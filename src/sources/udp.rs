use crate::csi::parse_csi_line;
use crate::models::CsiFrame;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tracing::{error, info};

pub async fn run_udp_source(port: u16, tx: mpsc::Sender<CsiFrame>) {
    let addr = format!("0.0.0.0:{}", port);
    let socket = match UdpSocket::bind(&addr).await {
        Ok(s) => {
            info!("RuView UDP CSI listener active on {}", addr);
            s
        }
        Err(e) => {
            error!("Failed to bind UDP socket on {}: {}", addr, e);
            return;
        }
    };

    let mut buf = vec![0u8; 4096];
    let mut packet_count: usize = 0;
    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, _peer)) => {
                if let Ok(text) = std::str::from_utf8(&buf[..len]) {
                    for line in text.lines() {
                        packet_count += 1;
                        if packet_count <= 5 || packet_count % 50 == 0 {
                            let preview = if line.len() > 100 { &line[..100] } else { line };
                            info!("UDP packet #{}: len={}, preview: {}", packet_count, len, preview);
                        }
                        if let Some(frame) = parse_csi_line(line, "receiver-1") {
                            if packet_count <= 5 || packet_count % 50 == 0 {
                                info!("Parsed frame #{}: receiver={}, rssi={}, iq_len={}", packet_count, frame.receiver_id, frame.rssi, frame.iq.len());
                            }
                            let _ = tx.send(frame).await;
                        } else if packet_count <= 5 {
                            info!("Failed to parse line #{}: {}", packet_count, line);
                        }
                    }
                }
            }
            Err(e) => {
                error!("UDP receive error: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }
    }
}
