mod csi;
mod models;
mod server;
mod signal;
mod sources;
mod spatial;

use clap::Parser;
use models::{CsiFrame, RoomConfig, RoomState};
use spatial::RoomProcessor;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser, Debug)]
#[command(author, version, about = "Deep Basics Monitor - Cognitum RuView Rust Native CSI Spatial Sensing Engine", long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "8000")]
    port: u16,

    #[arg(short, long, default_value = "5005")]
    udp_port: u16,

    #[arg(short, long)]
    serial: Vec<String>,

    #[arg(short, long, default_value = "config/room.json")]
    config: PathBuf,
}

fn get_local_ip() -> String {
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(local_addr) = socket.local_addr() {
                return local_addr.ip().to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let cli = Cli::parse();
    info!("Starting Deep Basics Monitor (Cognitum RuView Rust Engine v0.1.0)...");

    let config_content = match std::fs::read_to_string(&cli.config) {
        Ok(c) => c,
        Err(_) => {
            info!("Config file {:?} not found, using default 5x4m office configuration", cli.config);
            r##"{
                "name": "Deep Basics Office",
                "width_m": 5.0,
                "height_m": 4.0,
                "calibration_samples": 5,
                "motion_threshold": 0.20,
                "links": [
                    {"id": "receiver-1", "label": "ESP32 Node 1 (Corner B - North-East)", "x": 4.65, "y": 0.35, "color": "#2563eb"},
                    {"id": "receiver-2", "label": "ESP8266 Node 2 (Zone D - South-West)", "x": 0.35, "y": 3.65, "color": "#ea580c"}
                ]
            }"##.to_string()
        }
    };

    let room_config: RoomConfig = serde_json::from_str(&config_content)?;
    let local_ip = get_local_ip();
    info!("Local IP: {}, Web Port: {}, UDP Listener Port: {}", local_ip, cli.port, cli.udp_port);

    let mut processor = RoomProcessor::new(room_config.clone());
    let initial_state = processor.snapshot(0, false);

    let (tx_broadcast, _) = broadcast::channel::<RoomState>(256);
    let app_state = Arc::new(server::AppState {
        processor: RwLock::new(processor),
        current_state: RwLock::new(initial_state),
        config: room_config.clone(),
        server_ip: local_ip,
        udp_port: cli.udp_port,
        tx_broadcast: tx_broadcast.clone(),
    });

    // Ingestion Channel (Multi-source mpsc)
    let (tx_frame, mut rx_frame) = mpsc::channel::<CsiFrame>(1000);

    // Launch hardware ingestion streams (UDP + Serial)
    sources::run_hybrid_sources(cli.udp_port, cli.serial, tx_frame.clone()).await;

    // Real-Time Processing Event Loop (RuView Signal Pipeline)
    let state_clone = app_state.clone();
    tokio::spawn(async move {
        let mut last_activity = tokio::time::Instant::now();
        let timeout_duration = tokio::time::Duration::from_millis(500);

        loop {
            tokio::select! {
                Some(frame) = rx_frame.recv() => {
                    last_activity = tokio::time::Instant::now();
                    let updated_state = {
                        let mut proc = state_clone.processor.write().await;
                        proc.ingest(Some(&frame))
                    };

                    {
                        let mut cur = state_clone.current_state.write().await;
                        *cur = updated_state.clone();
                    }
                    let _ = state_clone.tx_broadcast.send(updated_state);
                }
                _ = tokio::time::sleep(timeout_duration) => {
                    if last_activity.elapsed() > tokio::time::Duration::from_secs(6) {
                        let offline_state = {
                            let mut proc = state_clone.processor.write().await;
                            proc.ingest(None)
                        };
                        {
                            let mut cur = state_clone.current_state.write().await;
                            *cur = offline_state.clone();
                        }
                        let _ = state_clone.tx_broadcast.send(offline_state);
                    }
                }
            }
        }
    });

    // Create Axum HTTP & WebSocket Router
    let static_dir = PathBuf::from("web");
    let router = server::create_router(app_state, static_dir);

    let addr = SocketAddr::from(([0, 0, 0, 0], cli.port));
    info!("RuView HTTP & WebSocket dashboard listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;

    Ok(())
}
