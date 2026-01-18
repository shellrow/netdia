use netdev::Interface;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Instant, SystemTime},
};
use tauri::async_runtime::JoinHandle;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct IfStats {
    // Total bytes received
    pub rx_bytes: u64,
    // Total bytes transmitted
    pub tx_bytes: u64,
    // Current receive bandwidth in bytes per second
    pub rx_bytes_per_sec: f64,
    // Current transmit bandwidth in bytes per second
    pub tx_bytes_per_sec: f64,
    // Timestamp of the stats
    pub ts: Instant,
}

#[derive(Debug)]
pub struct AppState {
    /// Cached network interfaces
    pub interfaces: Mutex<HashMap<u32, Interface>>,
    /// Last fetched stats
    pub stats: Mutex<HashMap<u32, IfStats>>,
    /// Last refresh time
    pub last_refresh: Mutex<SystemTime>,
    /// Update task handle
    pub task: Mutex<Option<JoinHandle<()>>>,
    /// Speedtest task handle
    pub speedtest_task: Mutex<Option<JoinHandle<()>>>,
    /// Last speedtest result: (direction, bytes)
    pub speedtest_last: Mutex<Option<(crate::model::speedtest::SpeedtestDirection, u64)>>, // direction, bytes
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            interfaces: Mutex::new(HashMap::new()),
            stats: Mutex::new(HashMap::new()),
            last_refresh: Mutex::new(SystemTime::now()),
            task: Mutex::new(None),
            speedtest_task: Mutex::new(None),
            speedtest_last: Mutex::new(None),
        }
    }
}

pub type SharedState = Arc<AppState>;
