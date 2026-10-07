pub mod serial;
pub mod udp;

use crate::models::CsiFrame;
use tokio::sync::mpsc;
use tracing::info;

pub fn discover_serial_ports() -> Vec<String> {
    let mut detected = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/dev") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("ttyUSB") || name.starts_with("ttyACM") {
                let path = format!("/dev/{}", name);
                detected.push(path);
            }
        }
    }
    detected.sort();
    detected
        .into_iter()
        .enumerate()
        .map(|(i, path)| format!("{}:receiver-{}", path, i + 1))
        .collect()
}

pub async fn run_hybrid_sources(
    udp_port: u16,
    serial_specs: Vec<String>,
    tx: mpsc::Sender<CsiFrame>,
) {
    let mut specs = serial_specs;
    if specs.is_empty() {
        specs = discover_serial_ports();
    }

    info!("Discovered hardware serial endpoints: {:?}", specs);

    let tx_udp = tx.clone();
    tokio::spawn(async move {
        udp::run_udp_source(udp_port, tx_udp).await;
    });

    if !specs.is_empty() {
        let tx_serial = tx.clone();
        tokio::spawn(async move {
            serial::run_serial_source(specs, tx_serial).await;
        });
    }
}
