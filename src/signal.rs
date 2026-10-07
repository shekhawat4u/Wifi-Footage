use crate::models::CsiFrame;
use std::collections::VecDeque;

pub fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

#[derive(Debug, Clone)]
pub struct RobustBaseline {
    pub target_samples: usize,
    pub samples: VecDeque<Vec<f64>>,
    pub center: Vec<f64>,
    pub scale: Vec<f64>,
}

impl RobustBaseline {
    pub fn new(target_samples: usize) -> Self {
        Self {
            target_samples,
            samples: VecDeque::with_capacity(target_samples),
            center: Vec::new(),
            scale: Vec::new(),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.samples.len() >= self.target_samples && !self.center.is_empty()
    }

    pub fn reset(&mut self) {
        self.samples.clear();
        self.center.clear();
        self.scale.clear();
    }

    pub fn add(&mut self, feature: Vec<f64>) {
        if self.samples.len() >= self.target_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(feature);

        if self.samples.len() == self.target_samples {
            let num_features = self.samples[0].len();
            let mut center = Vec::with_capacity(num_features);
            let mut scale = Vec::with_capacity(num_features);

            for idx in 0..num_features {
                let col: Vec<f64> = self.samples.iter().map(|s| s[idx]).collect();
                let m = median(col.clone());
                center.push(m);

                let diffs: Vec<f64> = col.iter().map(|v| (v - m).abs()).collect();
                let mad = median(diffs) * 1.4826;
                let floor_val = if idx == 0 { 1.4 } else { 0.75 };
                scale.push(mad.max(floor_val));
            }

            self.center = center;
            self.scale = scale;
        }
    }

    pub fn update_adaptive(&mut self, feature: &[f64]) {
        if !self.is_ready() || feature.len() != self.center.len() {
            return;
        }

        // Fast re-centering when sensor is relocated to another room (RSSI step > 3.5 dB)
        let rssi_delta = (feature[0] - self.center[0]).abs();
        if rssi_delta > 3.5 {
            self.center[0] = 0.85 * self.center[0] + 0.15 * feature[0];
            self.scale[0] = (self.scale[0] * 0.9 + rssi_delta * 0.1).max(1.4);
        } else {
            // Smooth slow baseline tracking for normal environmental drift
            self.center[0] = 0.994 * self.center[0] + 0.006 * feature[0];
        }

        for i in 1..self.center.len() {
            self.center[i] = 0.998 * self.center[i] + 0.002 * feature[i];
        }
    }

    pub fn distance(&self, feature: &[f64]) -> f64 {
        if !self.is_ready() || feature.len() != self.center.len() {
            return 0.0;
        }

        let mut active_indices = Vec::new();
        for (i, (m, s)) in self.center.iter().zip(self.scale.iter()).enumerate() {
            if *s > 0.0 && (*m != 0.0 || i == 0 || feature[i] != 0.0) {
                active_indices.push(i);
            }
        }

        if active_indices.is_empty() {
            return 0.0;
        }

        let mut z: Vec<f64> = active_indices
            .iter()
            .map(|&i| (feature[i] - self.center[i]).abs() / self.scale[i])
            .collect();

        // Single dimension (RSSI-only receiver like ESP8266 in another room)
        if z.len() == 1 {
            return (z[0] * 0.45).min(5.0);
        }

        // Multi-subcarrier CSI receiver (ESP32 in main office)
        z.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let top_k = 2.max(z.len() / 2);
        let k_mean = mean(&z[..top_k]);
        k_mean.min(20.0)
    }
}

pub fn extract_frame_features(frame: &CsiFrame, bins: usize) -> Vec<f64> {
    if frame.iq.is_empty() {
        let mut f = vec![frame.rssi as f64];
        f.resize(1 + bins, 0.0);
        return f;
    }

    let mut magnitudes = Vec::with_capacity(frame.iq.len() / 2);
    for chunk in frame.iq.chunks_exact(2) {
        let i = chunk[0] as f64;
        let q = chunk[1] as f64;
        magnitudes.push((i * i + q * q).sqrt());
    }

    if magnitudes.is_empty() {
        let mut f = vec![frame.rssi as f64];
        f.resize(1 + bins, 0.0);
        return f;
    }

    // 1. Acoustic Speaker Vibration Rejection (3-tap spectral median filter)
    if magnitudes.len() >= 3 {
        let mut smoothed = Vec::with_capacity(magnitudes.len());
        for i in 0..magnitudes.len() {
            let start = if i == 0 { 0 } else { i - 1 };
            let end = (i + 2).min(magnitudes.len());
            let win = magnitudes[start..end].to_vec();
            smoothed.push(median(win));
        }
        magnitudes = smoothed;
    }

    // 2. Subcarrier Bin Pooling
    let step = 1.max(magnitudes.len() / bins);
    let mut pooled = Vec::with_capacity(bins);
    for chunk in magnitudes.chunks(step) {
        pooled.push(mean(chunk));
    }
    while pooled.len() < bins {
        let last = *pooled.last().unwrap_or(&0.0);
        pooled.push(last);
    }
    pooled.truncate(bins);

    // 3. Common-mode Acoustic Rejection (Subtract Median Subcarrier Amplitude)
    let med_amp = median(pooled.clone());
    let avg_norm = 1.0f64.max(mean(&pooled));
    let mut subcarrier_features: Vec<f64> = pooled
        .iter()
        .map(|v| (v - med_amp).abs() / avg_norm * 12.0)
        .collect();

    let mut features = vec![frame.rssi as f64];
    features.append(&mut subcarrier_features);
    features
}
