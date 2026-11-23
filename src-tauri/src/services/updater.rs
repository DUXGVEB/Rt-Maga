//! Background periodic checker for new chapters on manga in reading history.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use rand::Rng;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;
use tracing::{debug, info, warn};

use crate::error::AppError;
use crate::repo::now_unix_secs;
use crate::settings::Language;
use crate::state::AppState;

/// Minimum interval (seconds) between automatic re-checks of the same manga.
const MIN_RECHECK_COOLDOWN_SECS: i64 = 3600;

/// Controls whether a check skips recently checked manga or forces checking all history items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CheckScope {
    /// Background tick: skip manga checked within the last hour.
    Scheduled,
    /// User pressed "Check now": check every manga currently in reading history.
    ManualNow,
}

/// Newly discovered chapter info emitted to the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FoundUpdate {
    pub manga_id: i64,
    pub title: String,
    pub latest: String,
}

/// Progress update emitted while checking multiple manga.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateProgress {
    pub done: usize,
    pub total: usize,
}

/// Summary returned to the caller of `check_updates_now`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateSummary {
    pub checked: usize,
    pub found: Vec<FoundUpdate>,
}

/// Spawns or restarts the background update loop according to current [`crate::settings::Settings`].
pub(crate) async fn restart_background_loop(app: AppHandle, state: Arc<AppState>) {
    let mut handle_guard = state.update_task.lock().await;
    if let Some(prev) = handle_guard.take() {
        prev.abort();
    }

    let settings = state.current_settings().await;
    if !settings.updates.enabled {
        debug!("background update checker disabled");
        return;
    }

    let interval = Duration::from_secs(u64::from(settings.updates.interval_hours) * 3600);
    let task_state = Arc::clone(&state);
    let task = tokio::spawn(async move {
        // Brief startup delay so the window paints before any background network activity.
        tokio::time::sleep(Duration::from_secs(12)).await;

        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            if let Err(err) = run_update_check(&app, &task_state, CheckScope::Scheduled).await {
                warn!(%err, "scheduled chapter update check failed");
            }
        }
    });

    *handle_guard = Some(task);
}

struct RunGuard<'a>(&'a std::sync::atomic::AtomicBool);

impl Drop for RunGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Checks every manga in reading history sequentially with polite jitter between requests.
///
/// # Errors
///
/// Returns [`AppError::Db`] if reading history cannot be queried.
pub(crate) async fn run_update_check(
    app: &AppHandle,
    state: &AppState,
    scope: CheckScope,
) -> Result<UpdateSummary, AppError> {
    if state
        .update_check_running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        debug!("chapter update check already in progress, skipping overlapping run");
        return Ok(UpdateSummary {
            checked: 0,
            found: Vec::new(),
        });
    }
    let _guard = RunGuard(&state.update_check_running);

    let settings = state.current_settings().await;
    let policy = settings.headless_policy();
    let now = now_unix_secs();

    let candidates = state.db.list_targets_for_updates().await?;
    let targets: Vec<_> = candidates
        .into_iter()
        .filter(|m| {
            scope == CheckScope::ManualNow
                || m.checked_at
                    .is_none_or(|ts| now.saturating_sub(ts) >= MIN_RECHECK_COOLDOWN_SECS)
        })
        .collect();

    let total = targets.len();
    let mut found = Vec::new();
    let mut created_notifications = 0usize;

    for (idx, manga_row) in targets.into_iter().enumerate() {
        if idx > 0 {
            let jitter_ms = rand::thread_rng().gen_range(1500_u64..=3500_u64);
            tokio::time::sleep(Duration::from_millis(jitter_ms)).await;
        }

        let _ = app.emit("updates://progress", UpdateProgress { done: idx, total });

        match state.registry.resolve(&manga_row.url, policy).await {
            Ok(resolved) => {
                let Some(resolved_manga) = resolved.as_manga() else {
                    continue;
                };
                if let Ok(discovered) = state
                    .db
                    .record_update_check(manga_row.id, resolved_manga)
                    .await
                    && !discovered.is_empty()
                {
                    let notif = state
                        .db
                        .record_chapter_notification(&manga_row, &discovered)
                        .await
                        .ok()
                        .flatten();

                    let range_label = notif.as_ref().map_or_else(
                        || crate::repo::notification::format_chapter_range(&discovered),
                        |n| n.chapter_range.clone(),
                    );
                    if notif.is_some() {
                        created_notifications += 1;
                    }

                    info!(
                        title = %manga_row.title,
                        chapters = %range_label,
                        count = discovered.len(),
                        "discovered new chapter(s)"
                    );
                    found.push(FoundUpdate {
                        manga_id: manga_row.id,
                        title: manga_row.title,
                        latest: range_label,
                    });
                }
            }
            Err(err) => {
                warn!(url = %manga_row.url, %err, "failed to check manga for updates");
            }
        }
    }

    let _ = app.emit("updates://progress", UpdateProgress { done: total, total });

    if created_notifications > 0 {
        let _ = app.emit("notifications://updated", created_notifications);
    }

    if !found.is_empty() {
        let _ = app.emit("updates://found", &found);

        if settings.updates.notify_windows {
            send_os_notification(app, &found, settings.language);
        }
    }

    Ok(UpdateSummary {
        checked: total,
        found,
    })
}

fn send_os_notification(app: &AppHandle, found: &[FoundUpdate], language: Language) {
    let title = match language {
        Language::En => "RtMaga - New Chapters",
        Language::Vi => "RtMaga - Chương mới",
        Language::Zh => "RtMaga - 新章节",
        Language::Ja => "RtMaga - 新着チャプター",
        Language::Ru => "RtMaga - Новые главы",
        Language::Ko => "RtMaga - 새 챕터",
        Language::Th => "RtMaga - ตอนใหม่",
    };

    let count = found.len();
    let body = if count == 1 {
        format!("{} - {}", found[0].title, found[0].latest)
    } else {
        match language {
            Language::En => format!("{count} manga have new chapters available"),
            Language::Vi => format!("Có {count} truyện vừa ra chương mới"),
            Language::Zh => format!("有 {count} 部漫画更新了新章节"),
            Language::Ja => format!("{count}作品に新しいチャプターがあります"),
            Language::Ru => format!("Новые главы вышли у {count} манг"),
            Language::Ko => format!("{count}개의 만화에 새 챕터가 있습니다"),
            Language::Th => format!("มีตอนใหม่สำหรับมังงะ {count} เรื่อง"),
        }
    };

    #[cfg(target_os = "android")]
    {
        let notif_id = i32::try_from(found[0].manga_id.rem_euclid(100_000) + 1000).unwrap_or(1001);
        if crate::android_env::send_android_notification(app, title, &body, notif_id) {
            return;
        }
    }

    if let Err(err) = app.notification().builder().title(title).body(body).show() {
        warn!(%err, "failed to display OS notification");
    }
}
