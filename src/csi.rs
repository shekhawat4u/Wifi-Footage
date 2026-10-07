use crate::models::CsiFrame;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn parse_csi_line(raw: &str, default_receiver: &str) -> Option<CsiFrame> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    // 1. JSON CSI Frame Format
    if trimmed.starts_with('{') && trimmed.ends_with('}') {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let rssi = val.get("rssi").and_then(|v| v.as_i64()).unwrap_or(-60) as i32;
            let mac = val.get("mac").and_then(|v| v.as_str()).unwrap_or("00:00:00:00:00:00").to_string();
            let seq = val.get("sequence").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let receiver = val.get("receiver_id").and_then(|v| v.as_str()).unwrap_or(default_receiver).to_string();

            let iq: Vec<i8> = val.get("iq")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_i64().map(|n| n as i8))
                        .collect()
                })
                .unwrap_or_default();

            return Some(CsiFrame {
                receiver_id: receiver,
                timestamp_ms: now,
                rssi,
                mac,
                sequence: seq,
                iq,
            });
        }
    }

    // 2. CSV CSI Frame Format
    if trimmed.starts_with("CSI_DATA") || trimmed.contains("CSI_DATA") {
        let parts: Vec<&str> = trimmed.split(',').map(|s| s.trim()).collect();
        if parts.len() < 5 {
            return None;
        }

        // Check if format is: CSI_DATA,receiver,unix_ms,mac,rssi,noise,channel,len,i0,q0,...
        let is_custom_format = parts.len() >= 8 && parts[1].starts_with("receiver");

        if is_custom_format {
            let receiver = if parts[1].is_empty() { default_receiver } else { parts[1] };
            let ts = parts[2].parse::<i64>().unwrap_or(now);
            let mac = parts[3].to_string();
            let rssi = parts[4].parse::<i32>().unwrap_or(-60);
            let declared_len = parts[7].parse::<usize>().unwrap_or(0);

            let mut iq = Vec::new();
            for item in &parts[8..] {
                let clean = item.trim_matches(|c| c == '[' || c == ']' || c == '"' || c == ' ');
                for token in clean.split_whitespace() {
                    if let Ok(val) = token.parse::<i8>() {
                        iq.push(val);
                    }
                }
            }

            if declared_len > 0 && iq.len() > declared_len {
                iq.truncate(declared_len);
            }

            return Some(CsiFrame {
                receiver_id: receiver.to_string(),
                timestamp_ms: ts,
                rssi,
                mac,
                sequence: 0,
                iq,
            });
        } else {
            // Standard ESP-IDF: CSI_DATA,seq,mac,rssi,...,data
            let seq = parts[1].parse::<u32>().unwrap_or(0);
            let mac = parts[2].to_string();
            let rssi = parts[3].parse::<i32>().unwrap_or(-60);

            let mut iq = Vec::new();
            for item in &parts[4..] {
                let clean = item.trim_matches(|c| c == '[' || c == ']' || c == '"' || c == ' ');
                for token in clean.split_whitespace() {
                    if let Ok(val) = token.parse::<i8>() {
                        iq.push(val);
                    }
                }
            }

            return Some(CsiFrame {
                receiver_id: default_receiver.to_string(),
                timestamp_ms: now,
                rssi,
                mac,
                sequence: seq,
                iq,
            });
        }
    }

    None
}
