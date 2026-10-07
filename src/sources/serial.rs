use crate::csi::parse_csi_line;
use crate::models::CsiFrame;
use tokio::io::AsyncBufReadExt;
use tokio::sync::mpsc;
use tokio_serial::SerialPortBuilderExt;
use tracing::{info, warn};

pub async fn run_serial_source(specs: Vec<String>, tx: mpsc::Sender<CsiFrame>) {
    let mut handles = Vec::new();

    for spec in specs {
        let parts: Vec<&str> = spec.split(':').collect();
        let port_name = parts[0].to_string();
        let receiver_id = parts.get(1).unwrap_or(&"receiver-1").to_string();
        let tx_clone = tx.clone();

        let handle = tokio::spawn(async move {
            loop {
                info!("Connecting to serial port {} for {}", port_name, receiver_id);
                match tokio_serial::new(&port_name, 921_600).open_native_async() {
                    Ok(port) => {
                        info!("Serial port {} opened successfully", port_name);
                        let reader = tokio::io::BufReader::new(port);
                        let mut lines = reader.lines();

                        while let Ok(Some(line)) = lines.next_line().await {
                            if let Some(frame) = parse_csi_line(&line, &receiver_id) {
                                if tx_clone.send(frame).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Serial open error on {}: {}. Retrying in 2s...", port_name, e);
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }
}
