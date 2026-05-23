use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use turso::{transaction::TransactionBehavior, Builder, Connection};

use crate::config::{AppConfig, LogLevel, LEGACY_CONFIG_FILE_NAME};

pub const DEFAULT_DB_FILE_NAME: &str = "netdia.db";

const DB_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const UI_PREFERENCES_MIGRATED_KEY: &str = "legacy_ui_preferences_migrated";
const CURRENT_SCHEMA_VERSION: i64 = 3;

const CREATE_APP_CONFIG_SQL: &str = r#"
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
    auto_update_check INTEGER NOT NULL CHECK (auto_update_check IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TRIGGER IF NOT EXISTS app_config_touch_updated_at
AFTER UPDATE ON app_config
FOR EACH ROW
BEGIN
    UPDATE app_config SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;
"#;

const CREATE_NOTIFICATIONS_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS notifications (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL,
    dedupe_key TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    data_json TEXT,
    is_read INTEGER NOT NULL DEFAULT 0 CHECK (is_read IN (0, 1)),
    dismissed_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TRIGGER IF NOT EXISTS notifications_touch_updated_at
AFTER UPDATE ON notifications
FOR EACH ROW
BEGIN
    UPDATE notifications SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;
"#;

const CREATE_UI_PREFERENCES_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS ui_preferences (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TRIGGER IF NOT EXISTS ui_preferences_touch_updated_at
AFTER UPDATE ON ui_preferences
FOR EACH ROW
BEGIN
    UPDATE ui_preferences SET updated_at = CURRENT_TIMESTAMP WHERE key = NEW.key;
END;
"#;

const CREATE_APP_METADATA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS app_metadata (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TRIGGER IF NOT EXISTS app_metadata_touch_updated_at
AFTER UPDATE ON app_metadata
FOR EACH ROW
BEGIN
    UPDATE app_metadata SET updated_at = CURRENT_TIMESTAMP WHERE key = NEW.key;
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

#[derive(Debug, Clone, Serialize)]
pub struct LegacyUiPreferencesMigrationResult {
    pub preferences: UiPreferences,
    pub migrated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppNotification {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub message: String,
    pub data: Option<serde_json::Value>,
    pub is_read: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNotificationPayload {
    pub version: Option<String>,
    pub current_version: Option<String>,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
    pub store_url: Option<String>,
}

impl UiPreferencesPatch {
    pub fn is_empty(&self) -> bool {
        self.sidebar_compact.is_none()
            && self.last_dns_query.is_none()
            && self.public_ip_visible.is_none()
            && self.hostname_visible.is_none()
    }

    fn entries(&self) -> Result<Vec<(&'static str, String)>> {
        let mut entries = Vec::new();

        if let Some(value) = self.sidebar_compact {
            entries.push(("sidebar_compact", serde_json::to_string(&value)?));
        }
        if let Some(value) = &self.last_dns_query {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                entries.push(("last_dns_query", serde_json::to_string(trimmed)?));
            }
        }
        if let Some(value) = self.public_ip_visible {
            entries.push(("public_ip_visible", serde_json::to_string(&value)?));
        }
        if let Some(value) = self.hostname_visible {
            entries.push(("hostname_visible", serde_json::to_string(&value)?));
        }

        Ok(entries)
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
        let mut conn = self.connect()?;
        conn.pragma_update("journal_mode", "'wal'")
            .await
            .context("Failed to enable WAL mode")?;
        conn.pragma_update("foreign_keys", "ON")
            .await
            .context("Failed to enable foreign keys")?;

        let version = read_user_version(&conn).await?;
        self.migrate_schema(&mut conn, version).await?;
        Ok(())
    }

    async fn migrate_schema(&self, conn: &mut Connection, version: i64) -> Result<()> {
        let mut current = version;

        while current < CURRENT_SCHEMA_VERSION {
            match current {
                0 => {
                    migrate_to_v1(conn).await?;
                    current = 1;
                }
                1 => {
                    migrate_to_v2(conn).await?;
                    current = 2;
                }
                2 => {
                    migrate_to_v3(conn).await?;
                    current = 3;
                }
                other => {
                    return Err(anyhow!("Unsupported database schema version: {other}"));
                }
            }
        }

        if current > CURRENT_SCHEMA_VERSION {
            return Err(anyhow!(
                "Database schema version {current} is newer than supported version {CURRENT_SCHEMA_VERSION}"
            ));
        }

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
            self.replace_ui_preferences(&defaults).await?;
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

    async fn is_legacy_ui_preferences_migrated(&self) -> Result<bool> {
        let conn = self.connect()?;
        read_metadata_bool(&conn, UI_PREFERENCES_MIGRATED_KEY).await
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
                    auto_internet_check_interval_s,
                    auto_update_check
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
                auto_update_check: row.get::<i64>(9)? != 0,
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
                auto_internet_check_interval_s,
                auto_update_check
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(id) DO UPDATE SET
                startup = excluded.startup,
                background = excluded.background,
                refresh_interval_ms = excluded.refresh_interval_ms,
                theme = excluded.theme,
                data_unit = excluded.data_unit,
                logging_level = excluded.logging_level,
                logging_file_path = excluded.logging_file_path,
                auto_internet_check = excluded.auto_internet_check,
                auto_internet_check_interval_s = excluded.auto_internet_check_interval_s,
                auto_update_check = excluded.auto_update_check
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
                cfg.auto_update_check,
            ),
        )
        .await
        .context("Failed to save app config")?;
        Ok(())
    }

    pub async fn load_ui_preferences(&self) -> Result<UiPreferences> {
        let conn = self.connect()?;
        self.load_ui_preferences_with_conn(&conn).await
    }

    async fn load_ui_preferences_with_conn(&self, conn: &Connection) -> Result<UiPreferences> {
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

    pub async fn replace_ui_preferences(&self, prefs: &UiPreferences) -> Result<()> {
        let patch = UiPreferencesPatch {
            sidebar_compact: Some(prefs.sidebar_compact),
            last_dns_query: Some(prefs.last_dns_query.clone()),
            public_ip_visible: Some(prefs.public_ip_visible),
            hostname_visible: Some(prefs.hostname_visible),
        };
        self.patch_ui_preferences(patch).await?;
        Ok(())
    }

    pub async fn patch_ui_preferences(&self, patch: UiPreferencesPatch) -> Result<UiPreferences> {
        let entries = patch.entries()?;
        if entries.is_empty() {
            return self.load_ui_preferences().await;
        }

        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .context("Failed to start UI preferences transaction")?;

        for (key, value_json) in entries {
            tx.execute(
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

        let prefs = self.load_ui_preferences_with_conn(&tx).await?;
        tx.commit()
            .await
            .context("Failed to commit UI preferences transaction")?;
        Ok(prefs)
    }

    pub async fn migrate_legacy_ui_preferences(
        &self,
        patch: UiPreferencesPatch,
    ) -> Result<LegacyUiPreferencesMigrationResult> {
        if patch.is_empty() && self.is_legacy_ui_preferences_migrated().await? {
            return Ok(LegacyUiPreferencesMigrationResult {
                preferences: self.load_ui_preferences().await?,
                migrated: false,
            });
        }

        let mut conn = self.connect()?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .context("Failed to start legacy UI preferences migration transaction")?;

        let already_migrated = read_metadata_bool(&tx, UI_PREFERENCES_MIGRATED_KEY).await?;
        if already_migrated {
            let preferences = self.load_ui_preferences_with_conn(&tx).await?;
            tx.commit()
                .await
                .context("Failed to commit no-op legacy UI preferences migration")?;
            return Ok(LegacyUiPreferencesMigrationResult {
                preferences,
                migrated: false,
            });
        }

        let entries = patch.entries()?;
        for (key, value_json) in entries {
            tx.execute(
                r#"
                INSERT INTO ui_preferences (key, value_json)
                VALUES (?1, ?2)
                ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json
                "#,
                (key, value_json),
            )
            .await
            .with_context(|| format!("Failed to migrate legacy UI preference: {key}"))?;
        }

        write_metadata_value(
            &tx,
            UI_PREFERENCES_MIGRATED_KEY,
            serde_json::to_string(&true)?,
        )
        .await?;

        let preferences = self.load_ui_preferences_with_conn(&tx).await?;
        tx.commit()
            .await
            .context("Failed to commit legacy UI preferences migration")?;

        Ok(LegacyUiPreferencesMigrationResult {
            preferences,
            migrated: true,
        })
    }

    pub async fn list_notifications(&self) -> Result<Vec<AppNotification>> {
        let conn = self.connect()?;
        let mut rows = conn
            .query(
                r#"
                SELECT id, kind, title, message, data_json, is_read, created_at, updated_at
                FROM notifications
                WHERE dismissed_at IS NULL
                ORDER BY is_read ASC, created_at DESC, id DESC
                "#,
                (),
            )
            .await
            .context("Failed to load notifications")?;

        let mut notifications = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .context("Failed to read notification row")?
        {
            notifications.push(decode_notification_row(&row)?);
        }

        Ok(notifications)
    }

    pub async fn mark_all_notifications_read(&self) -> Result<Vec<AppNotification>> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            UPDATE notifications
            SET is_read = 1
            WHERE dismissed_at IS NULL AND is_read = 0
            "#,
            (),
        )
        .await
        .context("Failed to mark notifications as read")?;
        self.list_notifications().await
    }

    pub async fn dismiss_notification(&self, id: i64) -> Result<Vec<AppNotification>> {
        let conn = self.connect()?;
        conn.execute(
            r#"
            UPDATE notifications
            SET dismissed_at = CURRENT_TIMESTAMP
            WHERE id = ?1
            "#,
            [id],
        )
        .await
        .with_context(|| format!("Failed to dismiss notification: {id}"))?;
        self.list_notifications().await
    }

    pub async fn upsert_update_notification(
        &self,
        payload: UpdateNotificationPayload,
    ) -> Result<Vec<AppNotification>> {
        let version = payload
            .version
            .clone()
            .unwrap_or_else(|| "latest".to_string());
        let dedupe_key = format!("update_available:{version}");
        let title = format!("NetDia {version} is available");
        let message = match payload.current_version.as_deref() {
            Some(current) if !current.is_empty() => {
                format!("An update is ready. Current version: {current}.")
            }
            _ => "An update is ready for NetDia.".to_string(),
        };
        let data_json =
            serde_json::to_string(&payload).context("Failed to encode notification payload")?;

        let conn = self.connect()?;
        conn.execute(
            r#"
            INSERT INTO notifications (
                kind,
                dedupe_key,
                title,
                message,
                data_json,
                is_read
            ) VALUES (?1, ?2, ?3, ?4, ?5, 0)
            ON CONFLICT(dedupe_key) DO UPDATE SET
                title = excluded.title,
                message = excluded.message,
                data_json = excluded.data_json
            "#,
            ("update_available", dedupe_key, title, message, data_json),
        )
        .await
        .context("Failed to save update notification")?;

        self.list_notifications().await
    }
}

async fn migrate_to_v1(conn: &mut Connection) -> Result<()> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .context("Failed to start schema migration to v1")?;
    tx.execute_batch(CREATE_APP_CONFIG_SQL)
        .await
        .context("Failed to create app_config table")?;
    tx.execute_batch(CREATE_UI_PREFERENCES_SQL)
        .await
        .context("Failed to create ui_preferences table")?;
    tx.pragma_update("user_version", 1)
        .await
        .context("Failed to update schema version to 1")?;
    tx.commit()
        .await
        .context("Failed to commit schema migration to v1")?;
    Ok(())
}

async fn migrate_to_v2(conn: &mut Connection) -> Result<()> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .context("Failed to start schema migration to v2")?;
    tx.execute_batch(CREATE_APP_METADATA_SQL)
        .await
        .context("Failed to create app_metadata table")?;
    tx.pragma_update("user_version", 2)
        .await
        .context("Failed to update schema version to 2")?;
    tx.commit()
        .await
        .context("Failed to commit schema migration to v2")?;
    Ok(())
}

async fn migrate_to_v3(conn: &mut Connection) -> Result<()> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .context("Failed to start schema migration to v3")?;
    if !table_has_column(&tx, "app_config", "auto_update_check").await? {
        tx.execute(
            r#"
            ALTER TABLE app_config
            ADD COLUMN auto_update_check INTEGER NOT NULL DEFAULT 1 CHECK (auto_update_check IN (0, 1))
            "#,
            (),
        )
        .await
        .context("Failed to add auto_update_check column")?;
    }
    tx.execute_batch(CREATE_NOTIFICATIONS_SQL)
        .await
        .context("Failed to create notifications table")?;
    tx.pragma_update("user_version", 3)
        .await
        .context("Failed to update schema version to 3")?;
    tx.commit()
        .await
        .context("Failed to commit schema migration to v3")?;
    Ok(())
}

async fn read_user_version(conn: &Connection) -> Result<i64> {
    let mut rows = conn
        .query("PRAGMA user_version", ())
        .await
        .context("Failed to read schema version")?;
    match rows
        .next()
        .await
        .context("Failed to fetch schema version row")?
    {
        Some(row) => row
            .get::<i64>(0)
            .context("Failed to decode schema version value"),
        None => Ok(0),
    }
}

async fn read_metadata_bool(conn: &Connection, key: &str) -> Result<bool> {
    let mut rows = conn
        .query(
            "SELECT value_json FROM app_metadata WHERE key = ?1 LIMIT 1",
            [key],
        )
        .await
        .with_context(|| format!("Failed to read app metadata: {key}"))?;

    match rows.next().await? {
        Some(row) => {
            let value_json: String = row.get(0)?;
            serde_json::from_str(&value_json)
                .with_context(|| format!("Failed to decode app metadata: {key}"))
        }
        None => Ok(false),
    }
}

async fn write_metadata_value(conn: &Connection, key: &str, value_json: String) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO app_metadata (key, value_json)
        VALUES (?1, ?2)
        ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json
        "#,
        (key, value_json),
    )
    .await
    .with_context(|| format!("Failed to write app metadata: {key}"))?;
    Ok(())
}

async fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let query = format!("PRAGMA table_info({table})");
    let mut rows = conn
        .query(&query, ())
        .await
        .with_context(|| format!("Failed to inspect schema for table: {table}"))?;

    while let Some(row) = rows.next().await.context("Failed to read table info row")? {
        let existing: String = row.get(1)?;
        if existing == column {
            return Ok(true);
        }
    }

    Ok(false)
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

fn decode_notification_row(row: &turso::Row) -> Result<AppNotification> {
    let data_json: Option<String> = row.get(4)?;
    let data = match data_json {
        Some(value) => {
            Some(serde_json::from_str(&value).context("Failed to decode notification payload")?)
        }
        None => None,
    };

    Ok(AppNotification {
        id: row.get(0)?,
        kind: row.get(1)?,
        title: row.get(2)?,
        message: row.get(3)?,
        data,
        is_read: row.get::<i64>(5)? != 0,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}
