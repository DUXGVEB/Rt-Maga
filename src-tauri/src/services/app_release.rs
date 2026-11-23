//! Automatic detection, verified in-app download, pre-update data backup, and installation
//! of newer official `RtMaga` application releases.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info, warn};
use url::Url;

use crate::error::AppError;
use crate::state::AppState;

/// Official release metadata endpoint queried at startup and on manual check.
const LATEST_RELEASE_API_URL: &str = "https://api.github.com/repos/DUXGVEB/Rt-Maga/releases/latest";

/// Fallback releases page URL if a release payload omits or alters `html_url`.
const FALLBACK_RELEASES_URL: &str = "https://github.com/DUXGVEB/Rt-Maga/releases";

/// Current compiled application version from `Cargo.toml`.
pub(crate) const CURRENT_APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Timeout for the background release metadata check request.
const RELEASE_CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum total duration for downloading an installer binary.
const INSTALLER_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// Maximum allowed installer binary size (150 MiB).
const MAX_INSTALLER_BYTES: u64 = 150 * 1024 * 1024;

/// Startup delay before querying the release API so the UI renders immediately.
const STARTUP_CHECK_DELAY: Duration = Duration::from_millis(2500);

/// Minimum interval between progress event emissions during installer download.
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(120);

/// Result of checking for a newer `RtMaga` application release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppReleaseInfo {
    /// Currently installed application version (e.g. `"1.1.0"`).
    pub current_version: String,
    /// Normalized latest release version (e.g. `"1.2.0"`).
    pub latest_version: String,
    /// Raw release tag (e.g. `"release-1.2.0"`).
    pub tag_name: String,
    /// Verified release page URL.
    pub release_url: String,
    /// Verified direct installer download URL for the current platform, if present.
    pub download_url: Option<String>,
    /// Expected installer size in bytes, if reported by the release API.
    pub download_size: Option<u64>,
    /// Expected lowercase SHA-256 hex digest of the installer asset, if reported.
    pub download_sha256: Option<String>,
    /// ISO-8601 publish timestamp, if available.
    pub published_at: Option<String>,
    /// `true` when `latest_version` is strictly newer than `current_version`.
    pub update_available: bool,
    /// `true` when the app can download and launch the installer directly on this platform.
    pub can_auto_install: bool,
}

/// Phase reported to the UI during an in-app update download & install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum UpdateStage {
    Downloading,
    Verifying,
    BackingUp,
    Installing,
}

/// Live progress event emitted over `app://update-download-progress`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateDownloadProgress {
    pub stage: UpdateStage,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

/// Final outcome returned by `download_and_install_update`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum InstallStatus {
    /// Windows installer launched in passive update + auto-relaunch mode; app is exiting.
    #[cfg_attr(
        not(target_os = "windows"),
        expect(dead_code, reason = "constructed on Windows targets")
    )]
    InstallingAndRestarting,
    /// Android system package installer dialog launched over the app.
    #[cfg_attr(
        not(target_os = "android"),
        expect(dead_code, reason = "constructed on Android targets")
    )]
    InstallerLaunched,
    /// Android *Install unknown apps* settings opened; user should grant permission and retry.
    #[cfg_attr(
        not(target_os = "android"),
        expect(dead_code, reason = "constructed on Android targets")
    )]
    PermissionRequired,
    /// Fallback for platforms without an integrated installer (opened release page in browser).
    OpenedBrowser,
}

/// Payload returned to the frontend when `download_and_install_update` finishes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InstallUpdateOutcome {
    pub status: InstallStatus,
}

#[derive(Debug, Deserialize)]
struct RemoteRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<RemoteAsset>,
}

#[derive(Debug, Deserialize)]
struct RemoteAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedVersion {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: bool,
}

struct InstallRunGuard<'a>(&'a AtomicBool);

impl Drop for InstallRunGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Removes leftover installer packages from `<cache_dir>/updates` on startup.
pub(crate) async fn cleanup_stale_installers(cache_dir: PathBuf) {
    let updates_dir = cache_dir.join("updates");
    if tokio::fs::try_exists(&updates_dir).await.unwrap_or(false) {
        if let Err(err) = tokio::fs::remove_dir_all(&updates_dir).await {
            debug!(%err, "failed to clean up stale update installers");
        } else {
            debug!("cleaned stale update installer cache");
        }
    }
}

/// Spawns a non-blocking startup task that checks for newer app releases when enabled in settings.
pub(crate) fn spawn_startup_check(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_CHECK_DELAY).await;
        if !state.current_settings().await.updates.check_app_updates {
            debug!("startup app release check disabled in settings");
            return;
        }

        match check_latest_release(&state).await {
            Ok(info) if info.update_available => {
                info!(
                    current = %info.current_version,
                    latest = %info.latest_version,
                    tag = %info.tag_name,
                    "new RtMaga release detected"
                );
                let _ = app.emit("app://release-available", &info);
            }
            Ok(info) => {
                debug!(
                    current = %info.current_version,
                    latest = %info.latest_version,
                    "RtMaga is up to date"
                );
            }
            Err(err) => {
                debug!(%err, "startup app release check skipped (offline or unavailable)");
            }
        }
    });
}

/// Queries the official latest release endpoint, compares against [`CURRENT_APP_VERSION`],
/// caches the outcome in [`AppState`], and returns [`AppReleaseInfo`].
///
/// # Errors
///
/// Returns [`AppError::Network`] if the endpoint cannot be reached or returns an invalid payload.
pub(crate) async fn check_latest_release(state: &AppState) -> Result<AppReleaseInfo, AppError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "X-GitHub-Api-Version",
        HeaderValue::from_static("2022-11-28"),
    );

    let client = reqwest::Client::builder()
        .no_proxy()
        .user_agent(format!("RtMaga/{CURRENT_APP_VERSION}"))
        .default_headers(headers)
        .timeout(RELEASE_CHECK_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| AppError::Network {
            message: format!("failed to build release check client: {e}"),
        })?;

    let resp = client
        .get(LATEST_RELEASE_API_URL)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                AppError::Timeout {
                    timeout_secs: RELEASE_CHECK_TIMEOUT.as_secs(),
                    message: format!("Update server timed out: {e}"),
                }
            } else {
                AppError::Network {
                    message: e.to_string(),
                }
            }
        })?;

    let status = resp.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || (status == reqwest::StatusCode::FORBIDDEN
            && resp
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|v| v.to_str().ok())
                == Some("0"))
    {
        return Err(AppError::RateLimited {
            retry_after_secs: None,
            message: "GitHub release API rate limit reached; try again later.".to_owned(),
        });
    }

    if !status.is_success() {
        return Err(AppError::HttpError {
            status: status.as_u16(),
            message: format!("Update server returned HTTP {}", status.as_u16()),
        });
    }

    let payload: RemoteRelease = resp.json().await.map_err(|e| AppError::CorruptedData {
        message: format!("Invalid release metadata payload: {e}"),
    })?;

    if parse_version(&payload.tag_name).is_none() {
        return Err(AppError::CorruptedData {
            message: format!(
                "Invalid release metadata: unrecognized version tag '{}'",
                payload.tag_name
            ),
        });
    }

    let info = build_release_info(CURRENT_APP_VERSION, &payload);
    {
        let mut guard = state.latest_release.write().await;
        *guard = Some(info.clone());
    }
    Ok(info)
}

/// Downloads the verified platform installer, checkpoints & backs up the SQLite database so user
/// reading history and settings stay 100% intact, and launches the update installer.
///
/// # Errors
///
/// Returns [`AppError`] if no newer release exists, download/SHA-256 verification fails, database
/// backup fails, or the OS installer cannot be started.
pub(crate) async fn download_and_install_update(
    app: &AppHandle,
    state: &AppState,
) -> Result<InstallUpdateOutcome, AppError> {
    if state
        .update_install_running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(AppError::Internal {
            message: "an update installation is already in progress".to_owned(),
        });
    }
    let _guard = InstallRunGuard(&state.update_install_running);

    let release = match state.latest_release.read().await.clone() {
        Some(info) => info,
        None => check_latest_release(state).await?,
    };

    if !release.update_available {
        return Err(AppError::NotFound {
            message: "already running the latest version".to_owned(),
        });
    }

    let Some(download_url) = release.download_url.as_deref() else {
        return open_fallback_browser(app, &release.release_url);
    };

    if !is_allowed_release_url(download_url) {
        return Err(AppError::InvalidUrl {
            message: "release download URL failed repository verification".to_owned(),
        });
    }

    if !release.can_auto_install {
        return open_fallback_browser(app, &release.release_url);
    }

    let cache_dir = app.path().app_cache_dir().map_err(|e| AppError::Internal {
        message: format!("failed to resolve app cache directory: {e}"),
    })?;
    let updates_dir = cache_dir.join("updates");
    tokio::fs::create_dir_all(&updates_dir)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to prepare updates directory: {e}"),
        })?;

    let filename = sanitize_installer_filename(download_url, &release.latest_version);
    let dest_path = updates_dir.join(filename);

    // Reuse an already-downloaded file if it exists and matches the expected SHA-256 digest
    // (or expected byte size when no SHA-256 digest was published by the release API).
    let already_verified = match (release.download_sha256.as_deref(), release.download_size) {
        (Some(expected), _) => file_sha256_matches(&dest_path, expected).await,
        (None, Some(sz)) if sz > 0 => tokio::fs::metadata(&dest_path)
            .await
            .is_ok_and(|m| m.is_file() && m.len() == sz),
        _ => false,
    };

    if !already_verified {
        stream_and_verify_installer(
            app,
            download_url,
            &dest_path,
            release.download_size,
            release.download_sha256.as_deref(),
        )
        .await?;
    }

    // Flush WAL and write a verified pre-update backup of rtmaga.db before launching installer.
    emit_progress(
        app,
        UpdateStage::BackingUp,
        release.download_size.unwrap_or(0),
        release.download_size,
    );
    let backup = state.db.checkpoint_and_backup().await?;
    if let Some(bak_path) = backup {
        info!(path = %bak_path.display(), "created pre-update SQLite backup");
    }

    emit_progress(
        app,
        UpdateStage::Installing,
        release.download_size.unwrap_or(0),
        release.download_size,
    );

    launch_platform_installer(app, state, &dest_path, &release.release_url).await
}

fn build_download_client() -> Result<reqwest::Client, AppError> {
    reqwest::Client::builder()
        .no_proxy()
        .user_agent(format!("RtMaga/{CURRENT_APP_VERSION}"))
        .connect_timeout(Duration::from_secs(15))
        .timeout(INSTALLER_DOWNLOAD_TIMEOUT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 8 {
                return attempt.error("too many redirects");
            }
            if !is_allowed_download_redirect_url(attempt.url()) {
                return attempt.error("disallowed redirect host during update download");
            }
            attempt.follow()
        }))
        .build()
        .map_err(|e| AppError::Network {
            message: format!("failed to build update download client: {e}"),
        })
}

async fn stream_and_verify_installer(
    app: &AppHandle,
    download_url: &str,
    dest_path: &Path,
    expected_size: Option<u64>,
    expected_sha256: Option<&str>,
) -> Result<(), AppError> {
    let client = build_download_client()?;
    emit_progress(app, UpdateStage::Downloading, 0, expected_size);

    let mut resp = client
        .get(download_url)
        .send()
        .await
        .map_err(|e| AppError::Network {
            message: format!("failed to download update installer: {e}"),
        })?;

    if !resp.status().is_success() {
        return Err(AppError::Network {
            message: format!("update download returned HTTP {}", resp.status().as_u16()),
        });
    }

    let total_bytes = resp.content_length().or(expected_size);
    if total_bytes.is_some_and(|n| n > MAX_INSTALLER_BYTES) {
        return Err(AppError::Internal {
            message: "installer binary exceeds maximum permitted size".to_owned(),
        });
    }

    let nonce: u64 = rand::random();
    let tmp_path = dest_path.with_extension(format!("tmp.{nonce:016x}"));
    let (downloaded, actual_sha256) =
        write_response_to_tmp(app, &mut resp, &tmp_path, total_bytes).await?;

    emit_progress(app, UpdateStage::Verifying, downloaded, total_bytes);
    if let Some(expected) = expected_sha256
        && !actual_sha256.eq_ignore_ascii_case(expected)
    {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        warn!(%actual_sha256, %expected, "update installer SHA-256 verification failed");
        return Err(AppError::Internal {
            message: "installer integrity verification failed (SHA-256 mismatch)".to_owned(),
        });
    }

    if tokio::fs::try_exists(dest_path).await.unwrap_or(false) {
        let _ = tokio::fs::remove_file(dest_path).await;
    }
    if let Err(err) = tokio::fs::rename(&tmp_path, dest_path).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(AppError::Internal {
            message: format!("failed to finalize downloaded installer: {err}"),
        });
    }

    info!(
        path = %dest_path.display(),
        bytes = downloaded,
        sha256 = %actual_sha256,
        "verified update installer ready"
    );
    Ok(())
}

async fn write_response_to_tmp(
    app: &AppHandle,
    resp: &mut reqwest::Response,
    tmp_path: &Path,
    total_bytes: Option<u64>,
) -> Result<(u64, String), AppError> {
    let mut file = tokio::fs::File::create(tmp_path)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to create temporary installer file: {e}"),
        })?;

    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut last_emit = Instant::now();

    let res: Result<(), AppError> = async {
        while let Some(chunk) = resp.chunk().await.map_err(|e| AppError::Network {
            message: format!("update download interrupted: {e}"),
        })? {
            let chunk_len = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
            downloaded = downloaded.saturating_add(chunk_len);
            if downloaded > MAX_INSTALLER_BYTES {
                return Err(AppError::Internal {
                    message: "installer stream exceeded maximum permitted size".to_owned(),
                });
            }
            file.write_all(&chunk)
                .await
                .map_err(|e| AppError::Internal {
                    message: format!("failed to write update file to disk: {e}"),
                })?;
            hasher.update(&chunk);

            if last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
                emit_progress(app, UpdateStage::Downloading, downloaded, total_bytes);
                last_emit = Instant::now();
            }
        }
        file.flush().await.map_err(|e| AppError::Internal {
            message: format!("failed to flush update file: {e}"),
        })?;
        file.sync_all().await.map_err(|e| AppError::Internal {
            message: format!("failed to sync update file: {e}"),
        })?;
        Ok(())
    }
    .await;

    drop(file);
    if let Err(err) = res {
        let _ = tokio::fs::remove_file(tmp_path).await;
        return Err(err);
    }
    Ok((downloaded, hex_digest(hasher.finalize())))
}

#[cfg(target_os = "windows")]
async fn launch_platform_installer(
    app: &AppHandle,
    state: &AppState,
    installer_path: &Path,
    _release_url: &str,
) -> Result<InstallUpdateOutcome, AppError> {
    let ext = installer_path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("")
        .to_ascii_lowercase();

    let mut cmd = if ext == "msi" {
        let mut c = std::process::Command::new("msiexec");
        c.arg("/i").arg(installer_path).arg("/passive");
        c
    } else {
        // Tauri NSIS installer flags:
        // /P      = passive progress UI (auto-closes when done)
        // /UPDATE = in-place update (never deletes app data in %LOCALAPPDATA%\com.rtmaga.reader)
        // /R      = automatically relaunches RtMaga once installation succeeds
        let mut c = std::process::Command::new(installer_path);
        c.args(["/P", "/UPDATE", "/R"]);
        c
    };

    cmd.spawn().map_err(|e| AppError::Internal {
        message: format!("failed to start Windows installer: {e}"),
    })?;

    // Release SQLite pool locks cleanly now that the installer process is running, then exit.
    state.db.close().await;
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(350)).await;
        app_handle.exit(0);
    });

    Ok(InstallUpdateOutcome {
        status: InstallStatus::InstallingAndRestarting,
    })
}

#[cfg(target_os = "android")]
async fn launch_platform_installer(
    app: &AppHandle,
    _state: &AppState,
    installer_path: &Path,
    _release_url: &str,
) -> Result<InstallUpdateOutcome, AppError> {
    let status = crate::android_installer::install_apk(app, installer_path)?;
    let mapped = if status == "permission_required" {
        InstallStatus::PermissionRequired
    } else {
        InstallStatus::InstallerLaunched
    };
    Ok(InstallUpdateOutcome { status: mapped })
}

#[cfg(not(any(target_os = "windows", target_os = "android")))]
async fn launch_platform_installer(
    app: &AppHandle,
    _state: &AppState,
    _installer_path: &Path,
    release_url: &str,
) -> Result<InstallUpdateOutcome, AppError> {
    open_fallback_browser(app, release_url)
}

fn open_fallback_browser(
    app: &AppHandle,
    release_url: &str,
) -> Result<InstallUpdateOutcome, AppError> {
    use tauri_plugin_opener::OpenerExt;
    if !is_allowed_release_url(release_url) {
        return Err(AppError::InvalidUrl {
            message: "only official release links may be opened".to_owned(),
        });
    }
    app.opener()
        .open_url(release_url, None::<&str>)
        .map_err(|e| AppError::Internal {
            message: format!("failed to open release link: {e}"),
        })?;
    Ok(InstallUpdateOutcome {
        status: InstallStatus::OpenedBrowser,
    })
}

fn emit_progress(
    app: &AppHandle,
    stage: UpdateStage,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
) {
    let _ = app.emit(
        "app://update-download-progress",
        UpdateDownloadProgress {
            stage,
            downloaded_bytes,
            total_bytes,
        },
    );
}

/// Validates that an external release URL belongs strictly to the official repository over HTTPS.
pub(crate) fn is_allowed_release_url(raw: &str) -> bool {
    let Ok(url) = Url::parse(raw.trim()) else {
        return false;
    };
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    if !host.eq_ignore_ascii_case("github.com") {
        return false;
    }
    url.path()
        .to_ascii_lowercase()
        .starts_with("/duxgveb/rt-maga/")
}

/// Validates redirect hops while downloading a release asset (allows HTTPS on `github.com`
/// and `*.githubusercontent.com` CDN hosts only).
fn is_allowed_download_redirect_url(url: &Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let lower = host.to_ascii_lowercase();
    lower == "github.com" || lower.ends_with(".githubusercontent.com")
}

fn build_release_info(current_version: &str, payload: &RemoteRelease) -> AppReleaseInfo {
    let latest_version = normalize_version_label(&payload.tag_name);
    let release_url = if is_allowed_release_url(&payload.html_url) {
        payload.html_url.clone()
    } else {
        FALLBACK_RELEASES_URL.to_owned()
    };
    let picked = pick_platform_asset(&payload.assets);
    let download_url = picked.map(|a| a.browser_download_url.clone());
    let download_size = picked.and_then(|a| a.size);
    let download_sha256 = picked
        .and_then(|a| a.digest.as_deref())
        .and_then(parse_sha256_digest);
    let update_available = !payload.draft
        && !payload.prerelease
        && is_newer_version(current_version, &payload.tag_name);
    let can_auto_install =
        download_url.is_some() && (cfg!(target_os = "windows") || cfg!(target_os = "android"));

    AppReleaseInfo {
        current_version: current_version.to_owned(),
        latest_version,
        tag_name: payload.tag_name.clone(),
        release_url,
        download_url,
        download_size,
        download_sha256,
        published_at: payload.published_at.clone(),
        update_available,
        can_auto_install,
    }
}

fn pick_platform_asset(assets: &[RemoteAsset]) -> Option<&RemoteAsset> {
    let valid: Vec<&RemoteAsset> = assets
        .iter()
        .filter(|a| is_allowed_release_url(&a.browser_download_url))
        .collect();

    if cfg!(target_os = "windows") {
        valid
            .iter()
            .copied()
            .find(|a| a.name.to_ascii_lowercase().ends_with("-setup.exe"))
            .or_else(|| {
                valid
                    .iter()
                    .copied()
                    .find(|a| a.name.to_ascii_lowercase().ends_with(".msi"))
            })
            .or_else(|| {
                valid
                    .iter()
                    .copied()
                    .find(|a| a.name.to_ascii_lowercase().ends_with(".exe"))
            })
    } else if cfg!(target_os = "android") {
        valid
            .iter()
            .copied()
            .find(|a| a.name.to_ascii_lowercase().ends_with(".apk"))
    } else {
        None
    }
}

fn parse_sha256_digest(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let hex = trimmed
        .strip_prefix("sha256:")
        .or_else(|| trimmed.strip_prefix("SHA256:"))?;
    if hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(hex.to_ascii_lowercase())
    } else {
        None
    }
}

fn sanitize_installer_filename(download_url: &str, version: &str) -> String {
    let raw_name = Url::parse(download_url)
        .ok()
        .and_then(|u| {
            u.path_segments()
                .and_then(std::iter::Iterator::last)
                .map(str::to_owned)
        })
        .unwrap_or_default();

    let clean: String = raw_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();

    if !clean.is_empty() && !clean.starts_with('.') && !clean.contains("..") {
        return clean;
    }

    let safe_ver: String = version
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        .collect();
    if cfg!(target_os = "android") {
        format!("RtMaga_{safe_ver}.apk")
    } else {
        format!("RtMaga_{safe_ver}-setup.exe")
    }
}

async fn file_sha256_matches(path: &Path, expected_hex: &str) -> bool {
    let Ok(mut file) = tokio::fs::File::open(path).await else {
        return false;
    };
    let mut hasher = Sha256::new();
    let mut buf = vec![0_u8; 64 * 1024];
    loop {
        match file.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(_) => return false,
        }
    }
    hex_digest(hasher.finalize()).eq_ignore_ascii_case(expected_hex)
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    let mut out = String::with_capacity(64);
    for b in bytes.as_ref() {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

fn normalize_version_label(tag: &str) -> String {
    let trimmed = tag.trim();
    let stripped = trimmed
        .strip_prefix("release-")
        .or_else(|| trimmed.strip_prefix("RELEASE-"))
        .or_else(|| trimmed.strip_prefix('v'))
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    stripped.to_owned()
}

fn parse_version(raw: &str) -> Option<ParsedVersion> {
    let stripped = normalize_version_label(raw);
    let without_build = stripped.split('+').next().unwrap_or(&stripped);
    let (core, prerelease) = match without_build.split_once('-') {
        Some((c, _pre)) => (c, true),
        None => (without_build, false),
    };

    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = match parts.next() {
        Some(p) => p.parse::<u64>().ok()?,
        None => 0,
    };

    Some(ParsedVersion {
        major,
        minor,
        patch,
        prerelease,
    })
}

/// Returns `true` when `latest` parses to a strictly higher stable release than `current`.
pub(crate) fn is_newer_version(current: &str, latest: &str) -> bool {
    let (Some(cur), Some(lat)) = (parse_version(current), parse_version(latest)) else {
        return false;
    };
    if lat.prerelease {
        return false;
    }
    let cur_tuple = (cur.major, cur.minor, cur.patch);
    let lat_tuple = (lat.major, lat.minor, lat.patch);
    lat_tuple > cur_tuple || (lat_tuple == cur_tuple && cur.prerelease)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_release_tags_and_semver_accurately() {
        assert!(is_newer_version("1.0.1", "release-1.1.0"));
        assert!(is_newer_version("1.1.0", "release-1.10.0"));
        assert!(is_newer_version("1.1.0", "v2.0.0"));
        assert!(is_newer_version("1.1.0-beta.1", "release-1.1.0"));

        assert!(!is_newer_version("1.1.0", "release-1.1.0"));
        assert!(!is_newer_version("1.1.0", "release-1.0.1"));
        assert!(!is_newer_version("1.1.0", "release-1.2.0-beta.1"));
        assert!(!is_newer_version("1.1.0", "beta-1.3"));
    }

    #[test]
    fn allows_only_official_https_repository_urls() {
        assert!(is_allowed_release_url(
            "https://github.com/DUXGVEB/Rt-Maga/releases/tag/release-1.1.0"
        ));
        assert!(is_allowed_release_url(
            "https://github.com/DUXGVEB/Rt-Maga/releases/download/release-1.1.0/RtMaga_1.1.0_windows_x64-setup.exe"
        ));
        assert!(!is_allowed_release_url(
            "http://github.com/DUXGVEB/Rt-Maga/releases"
        ));
        assert!(!is_allowed_release_url("https://evil.example/payload.exe"));
        assert!(!is_allowed_release_url("javascript:alert(1)"));
    }

    #[test]
    fn restricts_download_redirects_to_official_cdn_hosts() {
        assert!(is_allowed_download_redirect_url(
            &Url::parse("https://objects.githubusercontent.com/github-production-release-asset/a")
                .unwrap()
        ));
        assert!(is_allowed_download_redirect_url(
            &Url::parse("https://release-assets.githubusercontent.com/file.exe").unwrap()
        ));
        assert!(!is_allowed_download_redirect_url(
            &Url::parse("http://objects.githubusercontent.com/file.exe").unwrap()
        ));
        assert!(!is_allowed_download_redirect_url(
            &Url::parse("https://evilgithubusercontent.com/file.exe").unwrap()
        ));
    }

    #[test]
    fn parses_sha256_digests_and_sanitizes_filenames() {
        let valid = "sha256:52f0e6cf7c9f70b2ab90d2637ff4186b0b01e7d275047e4af909a0feb3257657";
        assert_eq!(
            parse_sha256_digest(valid).as_deref(),
            Some("52f0e6cf7c9f70b2ab90d2637ff4186b0b01e7d275047e4af909a0feb3257657")
        );
        assert!(parse_sha256_digest("md5:1234").is_none());
        assert!(parse_sha256_digest("sha256:nothex").is_none());

        assert_eq!(
            sanitize_installer_filename(
                "https://github.com/DUXGVEB/Rt-Maga/releases/download/release-1.2.0/RtMaga_1.2.0_windows_x64-setup.exe",
                "1.2.0"
            ),
            "RtMaga_1.2.0_windows_x64-setup.exe"
        );
        assert!(
            !sanitize_installer_filename("https://github.com/a/..%2Fevil", "1.2.0").contains("..")
        );
    }

    #[tokio::test]
    async fn verifies_downloaded_file_sha256() {
        let path =
            std::env::temp_dir().join(format!("rtmaga_sha_test_{}.bin", rand::random::<u64>()));
        tokio::fs::write(&path, b"rtmaga-installer-bytes")
            .await
            .unwrap();
        let expected = hex_digest(Sha256::digest(b"rtmaga-installer-bytes"));
        assert!(file_sha256_matches(&path, &expected).await);
        assert!(!file_sha256_matches(&path, &"0".repeat(64)).await);
        let _ = tokio::fs::remove_file(&path).await;
    }
}
