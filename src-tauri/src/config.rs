use serde::{Deserialize, Serialize};

use crate::log::DEFAULT_LOG_FILE_NAME;

pub const DEFAULT_CONFIG_FILE_NAME: &str = "netdia-config.json";

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
            theme: "system".to_string(),
            data_unit: bps_unit::BITS.to_string(),
            logging: LoggingConfig::new(),
        }
    }
    pub fn load() -> AppConfig {
        match crate::fs::get_user_file_path(DEFAULT_CONFIG_FILE_NAME) {
            Some(path) => {
                match std::fs::read_to_string(&path) {
                    Ok(content) => match serde_json::from_str(&content) {
                        Ok(config) => config,
                        Err(e) => {
                            tracing::error!("{:?}", e);
                            AppConfig::new()
                        }
                    },
                    Err(e) => {
                        tracing::error!("{:?}", e);
                        // Create default config
                        let config = AppConfig::new();
                        config.save();
                        config
                    }
                }
            }
            None => {
                // Create default config
                let config = AppConfig::new();
                config.save();
                config
            }
        }
    }
    pub fn save(&self) {
        if let Some(path) = crate::fs::get_user_file_path(DEFAULT_CONFIG_FILE_NAME) {
            match serde_json::to_string_pretty(&self) {
                Ok(content) => match std::fs::write(&path, content) {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("{:?}", e);
                    }
                },
                Err(e) => {
                    tracing::error!("{:?}", e);
                }
            }
        }
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Clone)]
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
    #[allow(dead_code)]
    pub fn to_string(&self) -> String {
        match self {
            LogLevel::DEBUG => "DEBUG",
            LogLevel::INFO => "INFO",
            LogLevel::WARN => "WARN",
            LogLevel::ERROR => "ERROR",
        }
        .to_owned()
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
            file_path: if let Some(path) = crate::fs::get_user_file_path(DEFAULT_LOG_FILE_NAME) {
                Some(path.to_string_lossy().to_string())
            } else {
                None
            },
        }
    }
}
