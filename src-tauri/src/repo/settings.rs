//! Persisted settings load/save operations.

use rand::RngCore;

use crate::error::AppError;
use crate::repo::Database;
use crate::settings::Settings;

const SETTINGS_KEY: &str = "app_settings_v1";
const PROXY_SECRET_KEY: &str = "local_img_proxy_secret_v1";

impl Database {
    /// Loads or creates a persistent 32-byte local HMAC secret for `img://` cover URLs so
    /// cover tokens remain stable across app restarts and leverage `WebView` HTTP cache.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn load_or_init_proxy_secret(&self) -> Result<[u8; 32], AppError> {
        let row: Option<(String,)> = sqlx::query_as("SELECT value FROM settings WHERE key = ?")
            .bind(PROXY_SECRET_KEY)
            .fetch_optional(self.pool())
            .await?;

        if let Some((hex,)) = row
            && let Some(decoded) = decode_hex_32(hex.trim())
        {
            return Ok(decoded);
        }

        let mut key = [0_u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        let hex = encode_hex_32(&key);

        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(PROXY_SECRET_KEY)
        .bind(hex)
        .execute(self.pool())
        .await?;

        Ok(key)
    }

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

fn encode_hex_32(bytes: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

fn decode_hex_32(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut out = [0_u8; 32];
    let bytes = hex.as_bytes();
    for (idx, chunk) in bytes.chunks_exact(2).enumerate() {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out[idx] = (hi << 4) | lo;
    }
    Some(out)
}

const fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
