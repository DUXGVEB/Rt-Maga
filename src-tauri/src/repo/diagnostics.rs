//! Sanitized crash & error diagnostic logging and structured diagnostic package export.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::AppError;
use crate::repo::{Database, now_unix_secs};

/// Maximum number of diagnostic log entries retained in SQLite.
const MAX_DIAGNOSTIC_LOGS: i64 = 500;

/// Maximum UTF-8 byte length kept for individual detail or stack trace fields.
const MAX_FIELD_LEN: usize = 4096;

/// Structured diagnostic log record persisted in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticLogEntry {
    pub id: i64,
    pub created_at: i64,
    pub level: String,
    pub category: String,
    pub message: String,
    pub detail: Option<String>,
    pub source: Option<String>,
    pub chapter_url: Option<String>,
    pub stack_trace: Option<String>,
    pub failed_url: Option<String>,
    pub resolved_url: Option<String>,
    pub http_status: Option<i64>,
    pub adapter: Option<String>,
    pub parsing_stage: Option<String>,
    pub api_error: Option<String>,
}

/// Input parameters for recording a new diagnostic entry.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct NewDiagnosticLog {
    pub level: String,
    pub category: String,
    pub message: String,
    pub detail: Option<String>,
    pub source: Option<String>,
    pub chapter_url: Option<String>,
    pub stack_trace: Option<String>,
    pub failed_url: Option<String>,
    pub resolved_url: Option<String>,
    pub http_status: Option<i64>,
    pub adapter: Option<String>,
    pub parsing_stage: Option<String>,
    pub api_error: Option<String>,
}

/// Scrubs credentials, bearer tokens, cookies, and sensitive query parameters from log text.
pub(crate) fn redact_sensitive(input: &str) -> String {
    let mut out = String::with_capacity(input.len().min(MAX_FIELD_LEN));

    // Redact `scheme://user:pass@host` credentials in URLs.
    let mut rest = input;
    while let Some(scheme_pos) = rest.find("://") {
        let after_scheme = scheme_pos + 3;
        out.push_str(&rest[..after_scheme]);
        let tail = &rest[after_scheme..];
        let delim = tail
            .find(['/', ' ', '\n', '\r', '\t', '"', '\''])
            .unwrap_or(tail.len());
        let authority = &tail[..delim];
        if let Some(at_pos) = authority.rfind('@') {
            out.push_str("***@");
            out.push_str(&authority[at_pos + 1..]);
        } else {
            out.push_str(authority);
        }
        rest = &tail[delim..];
    }
    out.push_str(rest);

    let mut sanitized = out;
    for secret_key in [
        "password=",
        "passwd=",
        "token=",
        "access_token=",
        "refresh_token=",
        "authorization:",
        "bearer ",
        "cookie:",
        "set-cookie:",
        "api_key=",
        "apikey=",
        "secret=",
        "session=",
        "session_id=",
        "sig=",
        "signature=",
        "x-amz-signature=",
        "hdnts=",
        "jwt=",
    ] {
        sanitized = redact_after_marker(&sanitized, secret_key);
    }

    let cleaned = scrub_personal_paths(&sanitized);
    truncate_utf8(&cleaned, MAX_FIELD_LEN)
}

/// Replaces local OS home/user directory segments (`C:\Users\<name>`, `C:/Users/<name>`,
/// `/Users/<name>`, `/home/<name>`) with `<USER_HOME>` so usernames never leak into logs or issues.
pub(crate) fn scrub_personal_paths(input: &str) -> String {
    let mut out = input.to_owned();
    for needle in [":\\users\\", ":/users/", "/users/", "/home/"] {
        out = scrub_prefix_pattern(&out, needle);
    }
    out
}

fn scrub_prefix_pattern(text: &str, needle_lower: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;

    while let Some(rel) = lower[cursor..].find(needle_lower) {
        let match_start = cursor + rel;
        let is_drive_prefixed = needle_lower.starts_with(':');
        let replace_start = if is_drive_prefixed && match_start > 0 {
            let prev = text[..match_start].chars().next_back();
            if prev.is_some_and(|c| c.is_ascii_alphabetic()) {
                match_start - 1
            } else {
                match_start
            }
        } else {
            match_start
        };

        result.push_str(&text[cursor..replace_start]);
        result.push_str("<USER_HOME>");

        let after_prefix = match_start + needle_lower.len();
        let tail = &text[after_prefix..];
        let seg_end = tail
            .find(['\\', '/', ' ', '\n', '\r', '\t', '"', '\'', ')', ']', ':'])
            .unwrap_or(tail.len());
        cursor = after_prefix + seg_end;
    }
    result.push_str(&text[cursor..]);
    result
}

fn redact_after_marker(text: &str, marker_lower: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;

    while let Some(rel) = lower[cursor..].find(marker_lower) {
        let start = cursor + rel + marker_lower.len();
        result.push_str(&text[cursor..start]);

        let tail = &text[start..];
        let end_rel = tail
            .find(['&', ' ', '\n', '\r', '\t', ';', ',', '"', '\'', '}'])
            .unwrap_or(tail.len());
        result.push_str("[REDACTED]");
        cursor = start + end_rel;
    }
    result.push_str(&text[cursor..]);
    result
}

fn truncate_utf8(raw: &str, max_bytes: usize) -> String {
    if raw.len() <= max_bytes {
        return raw.to_owned();
    }
    let mut idx = max_bytes;
    while idx > 0 && !raw.is_char_boundary(idx) {
        idx -= 1;
    }
    let mut truncated = raw[..idx].to_owned();
    truncated.push_str("...[truncated]");
    truncated
}

fn sanitize_opt(opt: Option<&str>) -> Option<String> {
    opt.map(str::trim)
        .filter(|s| !s.is_empty())
        .map(redact_sensitive)
}

impl Database {
    /// Persists a sanitized diagnostic error or crash record and prunes oldest entries past 500.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn record_diagnostic_log(
        &self,
        entry: &NewDiagnosticLog,
    ) -> Result<i64, AppError> {
        let now = now_unix_secs();
        let level = match entry.level.trim().to_ascii_lowercase().as_str() {
            "crash" | "fatal" => "crash",
            "warn" | "warning" => "warn",
            "info" => "info",
            _ => "error",
        };
        let category = truncate_utf8(entry.category.trim(), 64);
        let category_clean = if category.is_empty() {
            "internal".to_owned()
        } else {
            category
        };
        let message = redact_sensitive(entry.message.trim());
        let detail = sanitize_opt(entry.detail.as_deref());
        let source = sanitize_opt(entry.source.as_deref());
        let chapter_url = sanitize_opt(entry.chapter_url.as_deref());
        let stack_trace = sanitize_opt(entry.stack_trace.as_deref());
        let failed_url = sanitize_opt(entry.failed_url.as_deref().or(entry.chapter_url.as_deref()));
        let resolved_url = sanitize_opt(entry.resolved_url.as_deref());
        let adapter = sanitize_opt(entry.adapter.as_deref());
        let parsing_stage = sanitize_opt(entry.parsing_stage.as_deref());
        let api_error = sanitize_opt(entry.api_error.as_deref());

        let res = sqlx::query(
            "INSERT INTO diagnostic_log
                (created_at, level, category, message, detail, source, chapter_url, stack_trace,
                 failed_url, resolved_url, http_status, adapter, parsing_stage, api_error)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(now)
        .bind(level)
        .bind(category_clean)
        .bind(message)
        .bind(detail)
        .bind(source)
        .bind(chapter_url)
        .bind(stack_trace)
        .bind(failed_url)
        .bind(resolved_url)
        .bind(entry.http_status)
        .bind(adapter)
        .bind(parsing_stage)
        .bind(api_error)
        .execute(self.pool())
        .await?;

        let _ = sqlx::query(
            "DELETE FROM diagnostic_log
             WHERE id NOT IN (
                 SELECT id FROM diagnostic_log ORDER BY created_at DESC, id DESC LIMIT ?
             )",
        )
        .bind(MAX_DIAGNOSTIC_LOGS)
        .execute(self.pool())
        .await;

        Ok(res.last_insert_rowid())
    }

    /// Lists stored diagnostic logs newest first.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn list_diagnostic_logs(
        &self,
        limit: i64,
    ) -> Result<Vec<DiagnosticLogEntry>, AppError> {
        let bounded = limit.clamp(1, MAX_DIAGNOSTIC_LOGS);
        let rows = sqlx::query_as::<_, DiagnosticLogEntry>(
            "SELECT * FROM diagnostic_log ORDER BY created_at DESC, id DESC LIMIT ?",
        )
        .bind(bounded)
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Loads specific diagnostic log entries matching `ids`, preserving newest-first order.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn list_diagnostic_logs_by_ids(
        &self,
        ids: &[i64],
    ) -> Result<Vec<DiagnosticLogEntry>, AppError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let id_set: std::collections::HashSet<i64> = ids.iter().copied().collect();
        let all = self.list_diagnostic_logs(MAX_DIAGNOSTIC_LOGS).await?;
        Ok(all
            .into_iter()
            .filter(|entry| id_set.contains(&entry.id))
            .collect())
    }

    /// Clears all stored diagnostic logs.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn clear_diagnostic_logs(&self) -> Result<(), AppError> {
        sqlx::query("DELETE FROM diagnostic_log")
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
