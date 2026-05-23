use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use turso::{Builder, Connection};

use crate::config::{AppConfig, LogLevel, LEGACY_CONFIG_FILE_NAME};

pub const DEFAULT_DB_FILE_NAME: &str = "netdia.db";

const DB_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS app_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    startup INTEGER NOT NULL CHECK (startup IN (0, 1)),
    background INTEGER NOT NULL CHECK (background IN (0, 1)),
    refresh_interval_ms INTEGER NOT NULL CHECK (refresh_interval_ms >= 100),
    theme TEXT NOT NULL CHECK (theme IN ('dark', 'light', 'system')),
    data_unit TEXT NOT NULL CHECK (data_unit IN ('bits', 'bytes')),
    logging_level TEXT NOT NULL CHECK (logging_level IN ('DEBUG', 'INFO', 'WARN', 'ERROR')),
    logging_file_path TEXT,
    auto_internet_check INTEGER NOT NULL CHECK (auto_internet_check IN (0, 1)),
    auto_internet_check_interval_s INTEGER NOT NULL CHECK (auto_internet_check_interval_s >= 1),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS ui_preferences (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TRIGGER IF NOT EXISTS app_config_touch_updated_at
AFTER UPDATE ON app_config
FOR EACH ROW
BEGIN
    UPDATE app_config SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS ui_preferences_touch_updated_at
AFTER UPDATE ON ui_preferences
FOR EACH ROW
BEGIN
    UPDATE ui_preferences SET updated_at = CURRENT_TIMESTAMP WHERE key = NEW.key;
END;
"#;

#[derive(Clone)]
pub struct DatabaseState {
    db: turso::Database,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiPreferences {
    pub sidebar_compact: bool,
    pub last_dns_query: String,
    pub public_ip_visible: bool,
    pub hostname_visible: bool,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            sidebar_compact: true,
            last_dns_query: "example.com".to_string(),
            public_ip_visible: true,
            hostname_visible: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UiPreferencesPatch {
    pub sidebar_compact: Option<bool>,
    pub last_dns_query: Option<String>,
    pub public_ip_visible: Option<bool>,
    pub hostname_visible: Option<bool>,
}

impl UiPreferences {
    pub fn apply_patch(&mut self, patch: UiPreferencesPatch) {
        if let Some(sidebar_compact) = patch.sidebar_compact {
            self.sidebar_compact = sidebar_compact;
        }
        if let Some(last_dns_query) = patch.last_dns_query {
            let trimmed = last_dns_query.trim();
            if !trimmed.is_empty() {
                self.last_dns_query = trimmed.to_string();
            }
        }
        if let Some(public_ip_visible) = patch.public_ip_visible {
            self.public_ip_visible = public_ip_visible;
        }
        if let Some(hostname_visible) = patch.hostname_visible {
            self.hostname_visible = hostname_visible;
        }
    }
}

impl DatabaseState {
    pub async fn initialize() -> Result<Self> {
        let path = crate::fs::get_user_file_path(DEFAULT_DB_FILE_NAME)
            .ok_or_else(|| anyhow!("Failed to resolve application database path"))?;
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow!("Database path is not valid UTF-8"))?;

        let db = Builder::new_local(path_str)
            .build()
            .await
            .context("Failed to open local Turso database")?;
        let state = Self { db };
        state.initialize_schema().await?;
        state.migrate_legacy_config_if_needed().await?;
        state.ensure_default_ui_preferences().await?;
        Ok(state)
    }

    fn connect(&self) -> Result<Connection> {
        let conn = self.db.connect().context("Failed to connect to database")?;
        conn.busy_timeout(DB_BUSY_TIMEOUT)
            .context("Failed to configure database busy timeout")?;
        Ok(conn)
    }

    async fn initialize_schema(&self) -> Result<()> {
        let conn = self.connect()?;
        conn.pragma_update("journal_mode", "'wal'")
            .await
            .context("Failed to enable WAL mode")?;
        conn.pragma_update("foreign_keys", "ON")
            .await
            .context("Failed to enable foreign keys")?;
        conn.pragma_update("user_version", 1)
            .await
            .context("Failed to set schema version")?;
        conn.execute_batch(SCHEMA_SQL)
            .await
            .context("Failed to initialize database schema")?;
        Ok(())
    }

    async fn migrate_legacy_config_if_needed(&self) -> Result<()> {
        if self.app_config_exists().await? {
            return Ok(());
        }

        let legacy = crate::fs::get_user_file_path(LEGACY_CONFIG_FILE_NAME)
            .and_then(|path| AppConfig::load_legacy_from_path(&path));
        let config = legacy.unwrap_or_default();
        self.save_app_config(&config).await?;
        Ok(())
    }

    async fn ensure_default_ui_preferences(&self) -> Result<()> {
        let defaults = UiPreferences::default();
        let current = self.load_ui_preferences().await?;
        if current == defaults {
            self.save_ui_preferences(&defaults).await?;
        }
        Ok(())
    }

    async fn app_config_exists(&self) -> Result<bool> {
        let conn = self.connect()?;
        let mut rows = conn
            .query("SELECT 1 FROM app_config WHERE id = 1 LIMIT 1", ())
            .await
            .context("Failed to check app config presence")?;
        Ok(rows.next().await?.is_some())
    }

    pub async fn load_app_config(&self) -> Result<AppConfig> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                r#"
                SELECT
                    startup,
                    background,
                    refresh_interval_ms,
                    theme,
                    data_unit,
                    logging_level,
                    logging_file_path,
                    auto_internet_check,
                    auto_internet_check_interval_s
                FROM app_config
                WHERE id = 1
                "#,
                (),
            )
            .await
            .context("Failed to load app config")?;

        match rows.next().await.context("Failed to read app config row")? {
            Some(row) => Ok(AppConfig {
                startup: row.get::<i64>(0)? != 0,
                background: row.get::<i64>(1)? != 0,
                refresh_interval_ms: row.get::<i64>(2)? as u64,
                theme: row.get::<String>(3)?,
                data_unit: row.get::<String>(4)?,
                logging: crate::config::LoggingConfig {
                    level: parse_log_level(&row.get::<String>(5)?)?,
                    file_path: row.get::<Option<String>>(6)?,
                },
                auto_internet_check: row.get::<i64>(7)? != 0,
                auto_internet_check_interval_s: row.get::<i64>(8)? as u64,
            }),
            None => {
                let config = AppConfig::default();
                self.save_app_config(&config).await?;
                Ok(config)
            }
        }
    }

    pub async fn save_app_config(&self, cfg: &AppConfig) -> Result<()> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            INSERT INTO app_config (
                id,
                startup,
                background,
                refresh_interval_ms,
                theme,
                data_unit,
                logging_level,
                logging_file_path,
                auto_internet_check,
                auto_internet_check_interval_s
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(id) DO UPDATE SET
                startup = excluded.startup,
                background = excluded.background,
                refresh_interval_ms = excluded.refresh_interval_ms,
                theme = excluded.theme,
                data_unit = excluded.data_unit,
                logging_level = excluded.logging_level,
                logging_file_path = excluded.logging_file_path,
                auto_internet_check = excluded.auto_internet_check,
                auto_internet_check_interval_s = excluded.auto_internet_check_interval_s
            "#,
            (
                1_i64,
                cfg.startup,
                cfg.background,
                cfg.refresh_interval_ms as i64,
                cfg.theme.as_str(),
                cfg.data_unit.as_str(),
                cfg.logging.level.to_string(),
                cfg.logging.file_path.clone(),
                cfg.auto_internet_check,
                cfg.auto_internet_check_interval_s as i64,
            ),
        )
        .await
        .context("Failed to save app config")?;
        Ok(())
    }

    pub async fn load_ui_preferences(&self) -> Result<UiPreferences> {
        let conn = self.connect()?;
        let mut rows = conn
            .query("SELECT key, value_json FROM ui_preferences", ())
            .await
            .context("Failed to load UI preferences")?;

        let mut prefs = UiPreferences::default();
        while let Some(row) = rows
            .next()
            .await
            .context("Failed to read UI preference row")?
        {
            let key: String = row.get(0)?;
            let value_json: String = row.get(1)?;
            match key.as_str() {
                "sidebar_compact" => {
                    prefs.sidebar_compact = serde_json::from_str(&value_json)
                        .context("Failed to decode sidebar_compact preference")?;
                }
                "last_dns_query" => {
                    let value: String = serde_json::from_str(&value_json)
                        .context("Failed to decode last_dns_query preference")?;
                    if !value.trim().is_empty() {
                        prefs.last_dns_query = value;
                    }
                }
                "public_ip_visible" => {
                    prefs.public_ip_visible = serde_json::from_str(&value_json)
                        .context("Failed to decode public_ip_visible preference")?;
                }
                "hostname_visible" => {
                    prefs.hostname_visible = serde_json::from_str(&value_json)
                        .context("Failed to decode hostname_visible preference")?;
                }
                _ => {}
            }
        }

        Ok(prefs)
    }

    pub async fn save_ui_preferences(&self, prefs: &UiPreferences) -> Result<()> {
        let conn = self.connect()?;
        let values = [
            (
                "sidebar_compact",
                serde_json::to_string(&prefs.sidebar_compact)?,
            ),
            (
                "last_dns_query",
                serde_json::to_string(&prefs.last_dns_query)?,
            ),
            (
                "public_ip_visible",
                serde_json::to_string(&prefs.public_ip_visible)?,
            ),
            (
                "hostname_visible",
                serde_json::to_string(&prefs.hostname_visible)?,
            ),
        ];

        for (key, value_json) in values {
            conn.execute(
                r#"
                INSERT INTO ui_preferences (key, value_json)
                VALUES (?1, ?2)
                ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json
                "#,
                (key, value_json),
            )
            .await
            .with_context(|| format!("Failed to save UI preference: {key}"))?;
        }

        Ok(())
    }

    pub async fn patch_ui_preferences(&self, patch: UiPreferencesPatch) -> Result<UiPreferences> {
        let mut prefs = self.load_ui_preferences().await?;
        prefs.apply_patch(patch);
        self.save_ui_preferences(&prefs).await?;
        Ok(prefs)
    }
}

fn parse_log_level(value: &str) -> Result<LogLevel> {
    match value {
        "DEBUG" => Ok(LogLevel::DEBUG),
        "INFO" => Ok(LogLevel::INFO),
        "WARN" => Ok(LogLevel::WARN),
        "ERROR" => Ok(LogLevel::ERROR),
        other => Err(anyhow!("Unsupported log level in database: {other}")),
    }
}
