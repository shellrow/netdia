use std::{fmt, path::Path};

use serde::{Deserialize, Serialize};

use crate::log::DEFAULT_LOG_FILE_NAME;

pub const LEGACY_CONFIG_FILE_NAME: &str = "netdia-config.json";
const DEFAULT_THEME: &str = "dark";

pub mod bps_unit {
    pub const BITS: &str = "bits";
    #[allow(dead_code)]
    pub const BYTES: &str = "bytes";
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AppConfig {
    /// Whether the app should start automatically.
    pub startup: bool,
    /// Run in background (tray / menubar).
    pub background: bool,
    /// Refresh interval in milliseconds.
    pub refresh_interval_ms: u64,
    /// Theme: "dark", "light", or "system".
    pub theme: String,
    /// Data unit: "bits" or "bytes".
    pub data_unit: String,
    /// Logging configuration.
    pub logging: LoggingConfig,
    /// Auto internet check
    pub auto_internet_check: bool,
    /// Auto internet check interval in seconds
    pub auto_internet_check_interval_s: u64,
}

// Implement default
impl Default for AppConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl AppConfig {
    pub fn new() -> AppConfig {
        AppConfig {
            startup: false,
            background: false,
            refresh_interval_ms: 1000,
            theme: DEFAULT_THEME.to_string(),
            data_unit: bps_unit::BITS.to_string(),
            logging: LoggingConfig::new(),
            auto_internet_check: true,
            auto_internet_check_interval_s: 60,
        }
    }
    pub fn load_legacy_from_path(path: &Path) -> Option<AppConfig> {
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(config) => Some(config),
                Err(e) => {
                    tracing::error!("Failed to parse legacy config file: {:?}", e);
                    None
                }
            },
            Err(e) => {
                tracing::error!("Failed to read legacy config file: {:?}", e);
                None
            }
        }
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
#[allow(clippy::upper_case_acronyms)]
pub enum LogLevel {
    DEBUG,
    INFO,
    WARN,
    ERROR,
}

impl LogLevel {
    #[allow(dead_code)]
    pub fn allows(&self, level: &LogLevel) -> bool {
        match self {
            LogLevel::DEBUG => true,
            LogLevel::INFO => level != &LogLevel::DEBUG,
            LogLevel::WARN => level == &LogLevel::WARN || level == &LogLevel::ERROR,
            LogLevel::ERROR => level == &LogLevel::ERROR,
        }
    }
    pub fn to_level_filter(&self) -> tracing::Level {
        match self {
            LogLevel::DEBUG => tracing::Level::DEBUG,
            LogLevel::INFO => tracing::Level::INFO,
            LogLevel::WARN => tracing::Level::WARN,
            LogLevel::ERROR => tracing::Level::ERROR,
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self {
            LogLevel::DEBUG => "DEBUG",
            LogLevel::INFO => "INFO",
            LogLevel::WARN => "WARN",
            LogLevel::ERROR => "ERROR",
        };
        write!(f, "{level}")
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct LoggingConfig {
    /// Log level.
    pub level: LogLevel,
    /// Log file path.
    pub file_path: Option<String>,
}

impl LoggingConfig {
    pub fn new() -> LoggingConfig {
        LoggingConfig {
            level: LogLevel::INFO,
            file_path: crate::fs::get_user_file_path(DEFAULT_LOG_FILE_NAME)
                .map(|path| path.to_string_lossy().to_string()),
        }
    }
}
