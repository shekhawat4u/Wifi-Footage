use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsiFrame {
    pub receiver_id: String,
    pub timestamp_ms: i64,
    pub rssi: i32,
    pub mac: String,
    pub sequence: u32,
    pub iq: Vec<i8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkConfig {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    #[serde(default = "default_color")]
    pub color: String,
}

fn default_color() -> String {
    "#2563eb".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomConfig {
    #[serde(default = "default_room_name")]
    pub name: String,
    #[serde(default = "default_width")]
    pub width_m: f64,
    #[serde(default = "default_height")]
    pub height_m: f64,
    #[serde(default = "default_calibration_samples")]
    pub calibration_samples: usize,
    #[serde(default = "default_motion_threshold")]
    pub motion_threshold: f64,
    pub links: Vec<LinkConfig>,
}

fn default_room_name() -> String {
    "Deep Basics Office".to_string()
}
fn default_width() -> f64 {
    5.0
}
fn default_height() -> f64 {
    4.0
}
fn default_calibration_samples() -> usize {
    5
}
fn default_motion_threshold() -> f64 {
    0.20
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OccupantRecord {
    pub id: String,
    pub label: String,
    pub color: String,
    pub x: f64,
    pub y: f64,
    pub zone: String,
    pub activity: String,
    pub dist_node1_m: f64,
    pub dist_node2_m: f64,
    pub speed: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkState {
    pub id: String,
    pub score: f64,
    pub rssi: f64,
    pub samples: u64,
    pub calibrated: bool,
    pub history: Vec<f64>,
}

impl LinkState {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            score: 0.0,
            rssi: -100.0,
            samples: 0,
            calibrated: false,
            history: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomState {
    pub timestamp_ms: i64,
    pub mode: String,
    pub motion_energy: f64,
    pub position: Position,
    pub people: Vec<OccupantRecord>,
    pub people_count: usize,
    pub confidence: f64,
    pub bad_records: u64,
    pub links: Vec<LinkState>,
}
