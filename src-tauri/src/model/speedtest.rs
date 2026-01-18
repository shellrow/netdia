use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpeedtestDirection {
    Download,
    Upload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpeedtestResult {
    Full,
    Timeout,
    Canceled,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedtestSetting {
    pub direction: SpeedtestDirection,
    pub target_bytes: u64,
    /// default 30_000
    pub max_duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedtestUpdatePayload {
    pub direction: SpeedtestDirection,
    pub phase: String, // "running"
    pub elapsed_ms: u64,
    pub transferred_bytes: u64,
    pub target_bytes: u64,
    pub instant_mbps: f64,
    pub avg_mbps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedtestDonePayload {
    pub direction: SpeedtestDirection,
    pub result: SpeedtestResult,
    pub elapsed_ms: u64,
    pub transferred_bytes: u64,
    pub target_bytes: u64,
    pub avg_mbps: f64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LatencyUpdatePayload {
    pub phase: String, // "running" | "done"
    pub sample: u32,
    pub total: u32,
    pub rtt_ms: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LatencyDonePayload {
    pub latency_ms: f64,
    pub jitter_ms: f64,
    pub samples: Vec<f64>,
    pub colo: Option<String>,
}
