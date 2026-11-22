//! Automatic detection of newer official `RtMaga` application releases.

use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tracing::{debug, info};
use url::Url;

use crate::error::AppError;
use crate::state::AppState;

/// Official release metadata endpoint queried at startup and on manual check.
const LATEST_RELEASE_API_URL: &str = "https://api.github.com/repos/DUXGVEB/Rt-Maga/releases/latest";

/// Fallback releases page URL if a release payload omits or alters `html_url`.
const FALLBACK_RELEASES_URL: &str = "https://github.com/DUXGVEB/Rt-Maga/releases";

/// Current compiled application version from `Cargo.toml`.
pub(crate) const CURRENT_APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Timeout for the background release check request.
const RELEASE_CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// Startup delay before querying the release API so the UI renders immediately.
const STARTUP_CHECK_DELAY: Duration = Duration::from_millis(2500);

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
    /// ISO-8601 publish timestamp, if available.
    pub published_at: Option<String>,
    /// `true` when `latest_version` is strictly newer than `current_version`.
    pub update_available: bool,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedVersion {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: bool,
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
        .map_err(|e| AppError::Network {
            message: e.to_string(),
        })?;

    if !resp.status().is_success() {
        return Err(AppError::Network {
            message: format!("release check returned HTTP {}", resp.status().as_u16()),
        });
    }

    let payload: RemoteRelease = resp.json().await.map_err(|e| AppError::Network {
        message: format!("invalid release metadata payload: {e}"),
    })?;

    let info = build_release_info(CURRENT_APP_VERSION, &payload);
    {
        let mut guard = state.latest_release.write().await;
        *guard = Some(info.clone());
    }
    Ok(info)
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

fn build_release_info(current_version: &str, payload: &RemoteRelease) -> AppReleaseInfo {
    let latest_version = normalize_version_label(&payload.tag_name);
    let release_url = if is_allowed_release_url(&payload.html_url) {
        payload.html_url.clone()
    } else {
        FALLBACK_RELEASES_URL.to_owned()
    };
    let download_url = pick_platform_asset(&payload.assets);
    let update_available = !payload.draft
        && !payload.prerelease
        && is_newer_version(current_version, &payload.tag_name);

    AppReleaseInfo {
        current_version: current_version.to_owned(),
        latest_version,
        tag_name: payload.tag_name.clone(),
        release_url,
        download_url,
        published_at: payload.published_at.clone(),
        update_available,
    }
}

fn pick_platform_asset(assets: &[RemoteAsset]) -> Option<String> {
    let valid: Vec<&RemoteAsset> = assets
        .iter()
        .filter(|a| is_allowed_release_url(&a.browser_download_url))
        .collect();

    if cfg!(target_os = "windows") {
        valid
            .iter()
            .find(|a| a.name.to_ascii_lowercase().ends_with("-setup.exe"))
            .or_else(|| {
                valid
                    .iter()
                    .find(|a| a.name.to_ascii_lowercase().ends_with(".msi"))
            })
            .or_else(|| {
                valid
                    .iter()
                    .find(|a| a.name.to_ascii_lowercase().ends_with(".exe"))
            })
            .map(|a| a.browser_download_url.clone())
    } else if cfg!(target_os = "android") {
        valid
            .iter()
            .find(|a| a.name.to_ascii_lowercase().ends_with(".apk"))
            .map(|a| a.browser_download_url.clone())
    } else {
        None
    }
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
}
