//! SQLite repository operations for `saved_source` and `source_rule_spec`.

use mangacore::CustomRuleSpec;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::AppError;
use crate::repo::{Database, now_unix_secs};

/// Controls which visibility bucket of saved sources to query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum VisibilityFilter {
    /// Standard sources displayed in the primary Sources screen (`is_supplemental = 0`).
    #[default]
    MainOnly,
    /// Discreet supplemental sources managed inside Settings (`is_supplemental = 1`).
    SupplementalOnly,
    /// All saved sources regardless of visibility bucket.
    All,
}

/// Controls automatic vs user-pinned visibility when onboarding or recording a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum SourcePlacement {
    /// Classify from detected signals (`is_supplemental = 1` when adult signals appear),
    /// while respecting an existing `manual_visibility = 1` pin.
    #[default]
    Auto,
    /// Pin the source to the primary Sources view (`manual_visibility = 1`, `is_supplemental = 0`).
    ForceMain,
    /// Pin the source to Settings -> Supplemental Sources (`manual_visibility = 1`, `is_supplemental = 1`).
    ForceSupplemental,
}

/// Visibility patch action when manually editing a saved source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum VisibilityPatch {
    /// Leave visibility and manual pin untouched.
    #[default]
    Unchanged,
    /// Move into the primary Sources view and lock manual visibility.
    MoveToMain,
    /// Move into Settings -> Supplemental Sources and lock manual visibility.
    MoveToSupplemental,
}

/// Persisted record in `saved_source`.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct SavedSourceRow {
    pub id: i64,
    pub source_key: String,
    pub display_name: String,
    pub base_url: String,
    pub source_kind: String,
    pub parser_ref: Option<String>,
    pub search_template: Option<String>,
    pub icon_url: Option<String>,
    pub is_enabled: i64,
    pub is_supplemental: i64,
    pub manual_visibility: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Persisted record in `source_rule_spec`.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct RuleSpecRow {
    pub rule_id: String,
    pub name: String,
    pub version: i64,
    pub raw_json: String,
    pub origin: String,
    pub is_enabled: i64,
    pub updated_at: i64,
}

/// Input payload for upserting a discovered or manually added source.
#[derive(Debug, Clone)]
pub(crate) struct UpsertSavedSource<'a> {
    pub source_key: &'a str,
    pub display_name: &'a str,
    pub base_url: &'a str,
    pub source_kind: &'a str,
    pub parser_ref: Option<&'a str>,
    pub search_template: Option<&'a str>,
    pub icon_url: Option<&'a str>,
    pub detected_adult: bool,
    pub placement: SourcePlacement,
    pub prefer_new_name: bool,
}

/// Input patch payload for updating an existing [`SavedSourceRow`].
#[derive(Debug, Clone, Default)]
pub(crate) struct UpdateSavedSource<'a> {
    pub display_name: Option<&'a str>,
    pub base_url: Option<&'a str>,
    pub is_enabled: Option<bool>,
    pub visibility: VisibilityPatch,
}

impl Database {
    /// Lists saved sources filtered by [`VisibilityFilter`], newest added first.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite query failure.
    pub(crate) async fn list_saved_sources(
        &self,
        filter: VisibilityFilter,
    ) -> Result<Vec<SavedSourceRow>, AppError> {
        let rows = match filter {
            VisibilityFilter::MainOnly => {
                sqlx::query_as::<_, SavedSourceRow>(
                    "SELECT * FROM saved_source WHERE is_supplemental = 0 ORDER BY created_at DESC, id DESC",
                )
                .fetch_all(self.pool())
                .await?
            }
            VisibilityFilter::SupplementalOnly => {
                sqlx::query_as::<_, SavedSourceRow>(
                    "SELECT * FROM saved_source WHERE is_supplemental = 1 ORDER BY created_at DESC, id DESC",
                )
                .fetch_all(self.pool())
                .await?
            }
            VisibilityFilter::All => {
                sqlx::query_as::<_, SavedSourceRow>(
                    "SELECT * FROM saved_source ORDER BY created_at DESC, id DESC",
                )
                .fetch_all(self.pool())
                .await?
            }
        };
        Ok(rows)
    }

    /// Loads a single saved source by numeric primary key.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn get_saved_source_by_id(
        &self,
        id: i64,
    ) -> Result<Option<SavedSourceRow>, AppError> {
        let row = sqlx::query_as::<_, SavedSourceRow>("SELECT * FROM saved_source WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row)
    }

    /// Finds an enabled saved source matching `source_key`, `parser_ref`, or normalized hostname
    /// so domain mirror overrides apply transparently during resolution.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn find_matching_saved_source(
        &self,
        source_key: Option<&str>,
        host: Option<&str>,
    ) -> Result<Option<SavedSourceRow>, AppError> {
        if let Some(sk) = source_key.map(str::trim).filter(|s| !s.is_empty())
            && let Some(row) = sqlx::query_as::<_, SavedSourceRow>(
                "SELECT * FROM saved_source WHERE source_key = ? OR parser_ref = ? LIMIT 1",
            )
            .bind(sk)
            .bind(sk)
            .fetch_optional(self.pool())
            .await?
        {
            return Ok(Some(row));
        }

        if let Some(h) = host.map(str::trim).filter(|s| !s.is_empty()) {
            let bare = h.trim_start_matches("www.").to_ascii_lowercase();
            let all = self.list_saved_sources(VisibilityFilter::All).await?;
            for row in all {
                if let Ok(parsed) = url::Url::parse(&row.base_url)
                    && let Some(rh) = parsed.host_str()
                {
                    let rbare = rh.trim_start_matches("www.").to_ascii_lowercase();
                    if rbare == bare {
                        return Ok(Some(row));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Inserts or updates a saved source record while respecting user-pinned visibility (`manual_visibility`)
    /// and user-customized mirror domains (`base_url`).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn upsert_discovered_source(
        &self,
        input: &UpsertSavedSource<'_>,
    ) -> Result<SavedSourceRow, AppError> {
        let now = now_unix_secs();
        let base_norm = input.base_url.trim().trim_end_matches('/');
        let mut tx = self.pool().begin().await?;

        let existing = sqlx::query_as::<_, SavedSourceRow>(
            "SELECT * FROM saved_source
             WHERE source_key = ? OR RTRIM(base_url, '/') = RTRIM(?, '/')
             LIMIT 1",
        )
        .bind(input.source_key)
        .bind(base_norm)
        .fetch_optional(&mut *tx)
        .await?;

        let row = if let Some(prev) = existing {
            Self::update_existing_saved_source(&mut tx, &prev, input, base_norm, now).await?
        } else {
            Self::insert_new_saved_source(&mut tx, input, base_norm, now).await?
        };

        tx.commit().await?;
        Ok(row)
    }

    async fn update_existing_saved_source(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        prev: &SavedSourceRow,
        input: &UpsertSavedSource<'_>,
        base_norm: &str,
        now: i64,
    ) -> Result<SavedSourceRow, AppError> {
        let (next_supp, next_manual) = compute_visibility_update(
            prev.is_supplemental,
            prev.manual_visibility,
            input.detected_adult,
            input.placement,
        );
        let next_name = if input.prefer_new_name || prev.display_name.trim().is_empty() {
            input.display_name.trim()
        } else {
            prev.display_name.as_str()
        };
        let next_base = if input.placement == SourcePlacement::Auto {
            prev.base_url.as_str()
        } else {
            base_norm
        };
        let next_parser = input.parser_ref.or(prev.parser_ref.as_deref());
        let next_search = input.search_template.or(prev.search_template.as_deref());
        let next_icon = input.icon_url.or(prev.icon_url.as_deref());

        sqlx::query(
            "UPDATE saved_source
             SET display_name = ?,
                 base_url = ?,
                 source_kind = ?,
                 parser_ref = ?,
                 search_template = ?,
                 icon_url = ?,
                 is_supplemental = ?,
                 manual_visibility = ?,
                 updated_at = ?
             WHERE id = ?",
        )
        .bind(next_name)
        .bind(next_base)
        .bind(input.source_kind)
        .bind(next_parser)
        .bind(next_search)
        .bind(next_icon)
        .bind(next_supp)
        .bind(next_manual)
        .bind(now)
        .bind(prev.id)
        .execute(&mut **tx)
        .await?;

        sqlx::query_as::<_, SavedSourceRow>("SELECT * FROM saved_source WHERE id = ?")
            .bind(prev.id)
            .fetch_one(&mut **tx)
            .await
            .map_err(AppError::from)
    }

    async fn insert_new_saved_source(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        input: &UpsertSavedSource<'_>,
        base_norm: &str,
        now: i64,
    ) -> Result<SavedSourceRow, AppError> {
        let (is_supp, manual_vis) = match input.placement {
            SourcePlacement::ForceMain => (0_i64, 1_i64),
            SourcePlacement::ForceSupplemental => (1_i64, 1_i64),
            SourcePlacement::Auto => (i64::from(input.detected_adult), 0_i64),
        };
        let res = sqlx::query(
            "INSERT INTO saved_source
                (source_key, display_name, base_url, source_kind, parser_ref,
                 search_template, icon_url, is_enabled, is_supplemental, manual_visibility,
                 created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?)",
        )
        .bind(input.source_key)
        .bind(input.display_name.trim())
        .bind(base_norm)
        .bind(input.source_kind)
        .bind(input.parser_ref)
        .bind(input.search_template)
        .bind(input.icon_url)
        .bind(is_supp)
        .bind(manual_vis)
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await?;

        sqlx::query_as::<_, SavedSourceRow>("SELECT * FROM saved_source WHERE id = ?")
            .bind(res.last_insert_rowid())
            .fetch_one(&mut **tx)
            .await
            .map_err(AppError::from)
    }

    /// Updates user-editable properties of a saved source (display name, mirror `base_url`,
    /// enabled toggle, or manual Main/Supplemental placement).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NotFound`] if the source ID does not exist, [`AppError::InvalidUrl`] if
    /// `base_url` is invalid, or [`AppError::Db`] on SQLite failure.
    pub(crate) async fn update_saved_source(
        &self,
        id: i64,
        patch: &UpdateSavedSource<'_>,
    ) -> Result<SavedSourceRow, AppError> {
        let prev = self
            .get_saved_source_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound {
                message: format!("saved source {id} not found"),
            })?;

        let next_name = patch
            .display_name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(prev.display_name.as_str());

        let next_base = match patch.base_url.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => {
                let norm = mangacore::normalize_source_root(raw).map_err(AppError::from)?;
                norm.as_str().trim_end_matches('/').to_owned()
            }
            None => prev.base_url.clone(),
        };

        let next_enabled = patch.is_enabled.map_or(prev.is_enabled, i64::from);

        let (next_supp, next_manual) = match patch.visibility {
            VisibilityPatch::Unchanged => (prev.is_supplemental, prev.manual_visibility),
            VisibilityPatch::MoveToMain => (0_i64, 1_i64),
            VisibilityPatch::MoveToSupplemental => (1_i64, 1_i64),
        };

        sqlx::query(
            "UPDATE saved_source
             SET display_name = ?, base_url = ?, is_enabled = ?, is_supplemental = ?,
                 manual_visibility = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(next_name)
        .bind(&next_base)
        .bind(next_enabled)
        .bind(next_supp)
        .bind(next_manual)
        .bind(now_unix_secs())
        .bind(id)
        .execute(self.pool())
        .await?;

        self.get_saved_source_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound {
                message: format!("saved source {id} disappeared after update"),
            })
    }

    /// Deletes a saved source record by ID.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn delete_saved_source(&self, id: i64) -> Result<(), AppError> {
        sqlx::query("DELETE FROM saved_source WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Lists all custom JSON rule specifications ordered by update time.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn list_rule_specs(&self) -> Result<Vec<RuleSpecRow>, AppError> {
        let rows = sqlx::query_as::<_, RuleSpecRow>(
            "SELECT * FROM source_rule_spec ORDER BY updated_at DESC, name ASC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Parses and returns all currently enabled [`CustomRuleSpec`] entries.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn list_enabled_custom_rules(&self) -> Result<Vec<CustomRuleSpec>, AppError> {
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT raw_json FROM source_rule_spec WHERE is_enabled = 1 ORDER BY updated_at DESC",
        )
        .fetch_all(self.pool())
        .await?;

        let mut out = Vec::with_capacity(rows.len());
        for (raw,) in rows {
            if let Ok(spec) = serde_json::from_str::<CustomRuleSpec>(&raw) {
                out.push(spec);
            }
        }
        Ok(out)
    }

    /// Inserts or updates a validated declarative JSON rule pack.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn upsert_rule_spec(
        &self,
        spec: &CustomRuleSpec,
        raw_json: &str,
        origin: &str,
    ) -> Result<RuleSpecRow, AppError> {
        let now = now_unix_secs();
        let ver = i64::from(spec.version);
        sqlx::query(
            "INSERT INTO source_rule_spec (rule_id, name, version, raw_json, origin, is_enabled, updated_at)
             VALUES (?, ?, ?, ?, ?, 1, ?)
             ON CONFLICT(rule_id) DO UPDATE SET
                name = excluded.name,
                version = excluded.version,
                raw_json = excluded.raw_json,
                origin = excluded.origin,
                updated_at = excluded.updated_at",
        )
        .bind(&spec.id)
        .bind(&spec.name)
        .bind(ver)
        .bind(raw_json)
        .bind(origin)
        .bind(now)
        .execute(self.pool())
        .await?;

        sqlx::query_as::<_, RuleSpecRow>("SELECT * FROM source_rule_spec WHERE rule_id = ?")
            .bind(&spec.id)
            .fetch_one(self.pool())
            .await
            .map_err(AppError::from)
    }

    /// Enables or disables a custom JSON rule pack.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    #[expect(
        clippy::fn_params_excessive_bools,
        reason = "boolean enable/disable toggle matching SQLite column"
    )]
    pub(crate) async fn set_rule_spec_enabled(
        &self,
        rule_id: &str,
        enabled: bool,
    ) -> Result<(), AppError> {
        sqlx::query("UPDATE source_rule_spec SET is_enabled = ?, updated_at = ? WHERE rule_id = ?")
            .bind(i64::from(enabled))
            .bind(now_unix_secs())
            .bind(rule_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Deletes a custom JSON rule pack by ID.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn delete_rule_spec(&self, rule_id: &str) -> Result<(), AppError> {
        sqlx::query("DELETE FROM source_rule_spec WHERE rule_id = ?")
            .bind(rule_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "internal helper mapping detected adult classification flag"
)]
const fn compute_visibility_update(
    prev_supp: i64,
    prev_manual: i64,
    detected_adult: bool,
    placement: SourcePlacement,
) -> (i64, i64) {
    match placement {
        SourcePlacement::ForceMain => (0, 1),
        SourcePlacement::ForceSupplemental => (1, 1),
        SourcePlacement::Auto => {
            if prev_manual != 0 {
                (prev_supp, 1)
            } else if detected_adult {
                (1, 0)
            } else {
                (prev_supp, 0)
            }
        }
    }
}
