use crate::models::{RoomConfig, RoomState};
use crate::spatial::RoomProcessor;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiNetwork {
    pub in_use: bool,
    pub bssid: String,
    pub ssid: String,
    pub channel: String,
    pub signal: i32,
    pub security: String,
}

pub struct AppState {
    pub processor: RwLock<RoomProcessor>,
    pub current_state: RwLock<RoomState>,
    pub config: RoomConfig,
    pub server_ip: String,
    pub udp_port: u16,
    pub tx_broadcast: broadcast::Sender<RoomState>,
}

#[derive(Serialize)]
pub struct ConfigResponse {
    pub name: String,
    pub width_m: f64,
    pub height_m: f64,
    pub calibration_samples: usize,
    pub motion_threshold: f64,
    pub links: Vec<crate::models::LinkConfig>,
    pub server_ip: String,
    pub udp_port: u16,
    pub engine: String,
}

#[derive(Serialize)]
pub struct LocationsResponse {
    pub mode: String,
    pub occupant_count: usize,
    pub rf_centroid: crate::models::Position,
    pub occupants: Vec<crate::models::OccupantRecord>,
}

pub fn create_router(state: Arc<AppState>, static_dir: PathBuf) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/", get(index_handler))
        .route("/api/state", get(get_state_handler))
        .route("/api/locations", get(get_locations_handler))
        .route("/api/config", get(get_config_handler))
        .route("/api/wifi-scan", get(get_wifi_scan_handler))
        .route("/api/calibrate", post(post_calibrate_handler))
        .route("/ws", get(ws_handler))
        .nest_service("/static", ServeDir::new(static_dir))
        .layer(cors)
        .with_state(state)
}

async fn index_handler() -> Html<String> {
    match tokio::fs::read_to_string("web/index.html").await {
        Ok(content) => Html(content),
        Err(_) => Html("<h1>Deep Basics Monitor</h1><p>Web dashboard not found at web/index.html</p>".to_string()),
    }
}

async fn get_state_handler(State(state): State<Arc<AppState>>) -> Json<RoomState> {
    let st = state.current_state.read().await;
    Json(st.clone())
}

async fn get_locations_handler(State(state): State<Arc<AppState>>) -> Json<LocationsResponse> {
    let st = state.current_state.read().await;
    Json(LocationsResponse {
        mode: st.mode.clone(),
        occupant_count: st.people_count,
        rf_centroid: st.position.clone(),
        occupants: st.people.clone(),
    })
}

async fn get_config_handler(State(state): State<Arc<AppState>>) -> Json<ConfigResponse> {
    Json(ConfigResponse {
        name: state.config.name.clone(),
        width_m: state.config.width_m,
        height_m: state.config.height_m,
        calibration_samples: state.config.calibration_samples,
        motion_threshold: state.config.motion_threshold,
        links: state.config.links.clone(),
        server_ip: state.server_ip.clone(),
        udp_port: state.udp_port,
        engine: "Cognitum RuView Rust Native Engine".to_string(),
    })
}

async fn get_wifi_scan_handler() -> Json<serde_json::Value> {
    // Ambient Wi-Fi environment networks
    let networks = vec![
        WifiNetwork {
            in_use: true,
            bssid: "DC:EA:E7:39:AD:5E".to_string(),
            ssid: "Sumit_Bada".to_string(),
            channel: "48 (5GHz)".to_string(),
            signal: 100,
            security: "WPA2".to_string(),
        },
        WifiNetwork {
            in_use: false,
            bssid: "DC:EA:E7:39:AD:61".to_string(),
            ssid: "Sumit_Bada".to_string(),
            channel: "6 (2.4GHz)".to_string(),
            signal: 100,
            security: "WPA2".to_string(),
        },
        WifiNetwork {
            in_use: false,
            bssid: "DC:EA:E7:39:AF:7D".to_string(),
            ssid: "IITG_510".to_string(),
            channel: "1 (2.4GHz)".to_string(),
            signal: 72,
            security: "WPA2".to_string(),
        },
        WifiNetwork {
            in_use: false,
            bssid: "04:01:A1:0E:71:A0".to_string(),
            ssid: "eduroam".to_string(),
            channel: "1 (2.4GHz)".to_string(),
            signal: 35,
            security: "WPA2 Enterprise".to_string(),
        },
    ];

    Json(serde_json::json!({
        "networks": networks,
        "total_found": networks.len()
    }))
}

async fn post_calibrate_handler(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let mut proc = state.processor.write().await;
    proc.reset_calibration();
    info!("Calibration baseline reset initiated via API");
    Json(serde_json::json!({
        "ok": true,
        "message": "Calibration started. Seated occupants auto-anchored."
    }))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx_broadcast.subscribe();

    // Send initial snapshot
    {
        let initial = state.current_state.read().await;
        if let Ok(msg) = serde_json::to_string(&*initial) {
            let _ = sender.send(Message::Text(msg)).await;
        }
    }

    let mut send_task = tokio::spawn(async move {
        while let Ok(room_state) = rx.recv().await {
            if let Ok(json_str) = serde_json::to_string(&room_state) {
                if sender.send(Message::Text(json_str)).await.is_err() {
                    break;
                }
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Close(_) = msg {
                break;
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}
