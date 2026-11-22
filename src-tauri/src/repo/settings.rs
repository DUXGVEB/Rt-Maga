//! Persisted settings load/save operations.

use crate::error::AppError;
use crate::repo::Database;
use crate::settings::Settings;

const SETTINGS_KEY: &str = "app_settings_v1";

impl Database {
    /// Loads [`Settings`] from SQLite, falling back to [`Settings::default`] if not yet saved.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn load_settings(&self) -> Result<Settings, AppError> {
        let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
            .bind(SETTINGS_KEY)
            .fetch_optional(self.pool())
            .await?;

        let Some((json,)) = row else {
            return Ok(Settings::default());
        };

        let parsed = serde_json::from_str::<Settings>(&json).unwrap_or_default();
        Ok(parsed.sanitized())
    }

    /// Saves sanitized [`Settings`] into SQLite.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] or [`AppError::Internal`] if serialization/writing fails.
    pub(crate) async fn save_settings(&self, settings: &Settings) -> Result<Settings, AppError> {
        let clean = settings.clone().sanitized();
        let json = serde_json::to_string(&clean).map_err(|e| AppError::Internal {
            message: format!("failed to serialize settings: {e}"),
        })?;

        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(SETTINGS_KEY)
        .bind(json)
        .execute(self.pool())
        .await?;

        Ok(clean)
    }
}
