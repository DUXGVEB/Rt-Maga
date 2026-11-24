//! Headless browser fallback powered by an installed Chromium-based browser via `DevTools` Protocol.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use tokio::sync::{Mutex, Semaphore};
use tracing::{debug, info, warn};
use url::Url;

use crate::error::ScrapeError;
use crate::http::{DEFAULT_USER_AGENT, FetchedPage, HttpClient, validate_public_http_url};

/// Maximum number of concurrent headless tabs rendered at once.
const MAX_CONCURRENT_TABS: usize = 2;

/// Wait time after page load and scrolling for lazy-loaded DOM nodes to settle.
const RENDER_SETTLE_DELAY: Duration = Duration::from_millis(900);

/// Maximum wait time for a user to finish manual site verification in a visible window.
const MANUAL_VERIFY_TIMEOUT: Duration = Duration::from_secs(120);

/// Maximum wait time for a user to finish ordinary account sign-in in a visible browser window.
const USER_LOGIN_TIMEOUT: Duration = Duration::from_secs(180);

/// Shut down the idle background headless browser after 5 minutes of inactivity.
const IDLE_SHUTDOWN_AFTER: Duration = Duration::from_secs(300);

/// Controls the completion criteria of an interactive visible browser session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum InteractiveSessionMode {
    /// Wait for anti-bot / human verification challenge markers to clear.
    ManualVerification,
    /// Wait for ordinary user account sign-in (redirect off login/password form + session cookies).
    UserAuthentication,
}

/// Shared lazy-initialized headless browser controller.
#[derive(Debug, Clone)]
pub struct HeadlessBrowser {
    inner: Arc<HeadlessInner>,
}

struct BrowserSlot {
    browser: Option<Browser>,
    active_proxy_arg: Option<String>,
    last_used: tokio::time::Instant,
}

struct HeadlessInner {
    slot: Mutex<BrowserSlot>,
    interactive_guard: Mutex<()>,
    tab_permits: Arc<Semaphore>,
    reaper_running: AtomicBool,
}

impl std::fmt::Debug for HeadlessInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeadlessInner").finish_non_exhaustive()
    }
}

impl Default for HeadlessBrowser {
    fn default() -> Self {
        Self::new()
    }
}

impl HeadlessBrowser {
    /// Creates a new lazily-started [`HeadlessBrowser`] controller.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(HeadlessInner {
                slot: Mutex::new(BrowserSlot {
                    browser: None,
                    active_proxy_arg: None,
                    last_used: tokio::time::Instant::now(),
                }),
                interactive_guard: Mutex::new(()),
                tab_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_TABS)),
                reaper_running: AtomicBool::new(false),
            }),
        }
    }

    /// Renders `url` in a headless browser tab, waits for SPA/dynamic content, scrolls down
    /// to trigger lazy-loaded images, copies cookies into `http`, and returns the rendered HTML.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::DisallowedHost`] for non-public URLs, [`ScrapeError::Blocked`]
    /// if a bot challenge is detected, [`ScrapeError::Proxy`] if fail-closed proxy validation fails,
    /// or [`ScrapeError::Headless`] if launching/rendering fails.
    pub async fn fetch_rendered(
        &self,
        url: &Url,
        http: &HttpClient,
    ) -> Result<FetchedPage, ScrapeError> {
        validate_public_http_url(url)?;
        let proxy_args = http.chromium_proxy_args_for_url(url)?;
        let Ok(_permit) = self.inner.tab_permits.acquire().await else {
            return Err(ScrapeError::Headless("tab semaphore closed".to_owned()));
        };

        let page = self.open_tab(url, proxy_args).await?;
        let render_result = render_and_extract(&page, url, http).await;
        let _ = page.close().await;
        render_result
    }

    /// Opens a visible browser window so the user can manually pass a site verification challenge,
    /// then copies resulting cookies into `http` once the challenge clears.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::Proxy`] if fail-closed proxy validation fails, or
    /// [`ScrapeError::Headless`] if no browser can be launched or the user closes/times out.
    pub async fn open_manual_verification(
        &self,
        url: &Url,
        http: &HttpClient,
    ) -> Result<(), ScrapeError> {
        self.open_interactive_session(url, InteractiveSessionMode::ManualVerification, http)
            .await
    }

    /// Opens a visible browser window for manual verification or ordinary user account login and
    /// synchronizes authorized domain cookies into `http`.
    ///
    /// Never inspects or logs passwords or raw cookie values.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::DisallowedHost`], [`ScrapeError::Proxy`], or [`ScrapeError::Headless`].
    pub async fn open_interactive_session(
        &self,
        url: &Url,
        mode: InteractiveSessionMode,
        http: &HttpClient,
    ) -> Result<(), ScrapeError> {
        validate_public_http_url(url)?;
        let proxy_args = http.chromium_proxy_args_for_url(url)?;
        let exe = find_browser_executable().ok_or_else(|| {
            ScrapeError::Headless("no compatible local browser executable found".to_owned())
        })?;
        let _auth_lock = self.inner.interactive_guard.lock().await;

        let mut builder = BrowserConfig::builder()
            .chrome_executable(exe)
            .with_head()
            .arg(format!("--user-agent={DEFAULT_USER_AGENT}"));
        if mode == InteractiveSessionMode::UserAuthentication {
            let profile_dir = std::env::temp_dir().join("rtmaga-browser-auth");
            builder = builder.user_data_dir(profile_dir);
        }
        if let Some((proxy_arg, resolver_arg)) = proxy_args {
            builder = builder.arg(proxy_arg);
            if let Some(res_arg) = resolver_arg {
                builder = builder.arg(res_arg);
            }
        }
        let config = builder
            .build()
            .map_err(|e| ScrapeError::Headless(format!("browser config error: {e}")))?;

        let (mut browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| ScrapeError::Headless(format!("failed to launch visible browser: {e}")))?;

        let handler_task = tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if event.is_err() {
                    break;
                }
            }
        });

        let page = browser
            .new_page(url.as_str())
            .await
            .map_err(|e| ScrapeError::Headless(format!("failed to open interactive tab: {e}")))?;

        let completed = poll_interactive_session(&page, url, mode, http).await;
        let _ = browser.close().await;
        handler_task.abort();

        if completed {
            info!(%url, ?mode, "interactive browser session completed and cookies synced");
            Ok(())
        } else {
            Err(ScrapeError::Headless(match mode {
                InteractiveSessionMode::ManualVerification => {
                    "manual verification timed out before challenge cleared".to_owned()
                }
                InteractiveSessionMode::UserAuthentication => {
                    "browser login window timed out before sign-in completed".to_owned()
                }
            }))
        }
    }

    async fn open_tab(
        &self,
        url: &Url,
        proxy_args: Option<(String, Option<String>)>,
    ) -> Result<chromiumoxide::Page, ScrapeError> {
        let mut guard = self.inner.slot.lock().await;
        guard.last_used = tokio::time::Instant::now();
        let desired_proxy = proxy_args.as_ref().map(|(p, _)| p.clone());

        if guard.active_proxy_arg != desired_proxy
            && let Some(mut stale) = guard.browser.take()
        {
            debug!("proxy configuration changed; relaunching headless browser");
            let _ = stale.close().await;
            guard.active_proxy_arg = None;
        }

        if let Some(browser) = guard.browser.as_ref() {
            match browser.new_page(url.as_str()).await {
                Ok(page) => return Ok(page),
                Err(err) => {
                    warn!(%err, "existing headless browser failed to open tab; relaunching");
                    if let Some(mut stale) = guard.browser.take() {
                        let _ = stale.close().await;
                    }
                    guard.active_proxy_arg = None;
                }
            }
        }

        let browser = Self::launch_headless_browser(proxy_args.as_ref()).await?;
        let page = browser
            .new_page(url.as_str())
            .await
            .map_err(|e| ScrapeError::Headless(format!("failed to open tab: {e}")))?;
        guard.browser = Some(browser);
        guard.active_proxy_arg = desired_proxy;
        guard.last_used = tokio::time::Instant::now();
        drop(guard);

        self.spawn_idle_reaper();
        Ok(page)
    }

    async fn launch_headless_browser(
        proxy_args: Option<&(String, Option<String>)>,
    ) -> Result<Browser, ScrapeError> {
        let exe = find_browser_executable().ok_or_else(|| {
            ScrapeError::Headless("no compatible local browser executable found".to_owned())
        })?;
        debug!(
            ?exe,
            proxied = proxy_args.is_some(),
            "launching headless browser"
        );

        let mut builder = BrowserConfig::builder()
            .chrome_executable(exe)
            .arg(format!("--user-agent={DEFAULT_USER_AGENT}"))
            .arg("--disable-gpu")
            .arg("--no-first-run");
        if let Some((proxy_arg, resolver_arg)) = proxy_args {
            builder = builder.arg(proxy_arg.as_str());
            if let Some(res_arg) = resolver_arg {
                builder = builder.arg(res_arg.as_str());
            }
        }
        let config = builder
            .build()
            .map_err(|e| ScrapeError::Headless(format!("browser config error: {e}")))?;

        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| ScrapeError::Headless(format!("failed to launch browser: {e}")))?;

        tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(err) = event {
                    warn!(%err, "headless browser event loop ended");
                    break;
                }
            }
        });

        Ok(browser)
    }

    fn spawn_idle_reaper(&self) {
        if self
            .inner
            .reaper_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let mut guard = inner.slot.lock().await;
                if guard.browser.is_none() {
                    break;
                }
                let idle = guard.last_used.elapsed() >= IDLE_SHUTDOWN_AFTER;
                let no_active_tabs = inner.tab_permits.available_permits() == MAX_CONCURRENT_TABS;
                if idle && no_active_tabs {
                    if let Some(mut browser) = guard.browser.take() {
                        debug!("shutting down idle headless browser");
                        let _ = browser.close().await;
                    }
                    guard.active_proxy_arg = None;
                    break;
                }
            }
            inner.reaper_running.store(false, Ordering::Release);
        });
    }
}

async fn poll_interactive_session(
    page: &chromiumoxide::Page,
    initial_url: &Url,
    mode: InteractiveSessionMode,
    http: &HttpClient,
) -> bool {
    match mode {
        InteractiveSessionMode::ManualVerification => {
            let deadline = tokio::time::Instant::now() + MANUAL_VERIFY_TIMEOUT;
            while tokio::time::Instant::now() < deadline {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let Ok(html) = page.content().await else {
                    continue;
                };
                if !looks_like_rendered_challenge(&html) {
                    let sync_url = page
                        .url()
                        .await
                        .ok()
                        .flatten()
                        .and_then(|s| Url::parse(&s).ok())
                        .filter(|u| validate_public_http_url(u).is_ok())
                        .unwrap_or_else(|| initial_url.clone());
                    copy_page_cookies(page, &sync_url, http).await;
                    return true;
                }
            }
            false
        }
        InteractiveSessionMode::UserAuthentication => {
            poll_user_login(page, initial_url, http).await
        }
    }
}

async fn poll_user_login(page: &chromiumoxide::Page, initial_url: &Url, http: &HttpClient) -> bool {
    let deadline = tokio::time::Instant::now() + USER_LOGIN_TIMEOUT;
    let mut baseline_cookies: Option<usize> = None;
    let mut saw_login_stage = is_login_like_url(initial_url);
    let mut last_observed: Vec<(String, String, String, String)> = Vec::new();
    let mut sync_url = initial_url.clone();

    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let Ok(cookies) = page.get_cookies().await else {
            if let Some(base) = baseline_cookies
                && last_observed.len() > base
            {
                apply_captured_cookies(&last_observed, &sync_url, http);
                return true;
            }
            break;
        };
        let base = *baseline_cookies.get_or_insert(cookies.len());
        last_observed = cookies
            .iter()
            .map(|c| {
                (
                    c.name.clone(),
                    c.value.clone(),
                    c.domain.clone(),
                    c.path.clone(),
                )
            })
            .collect();
        if let Some(u) = page
            .url()
            .await
            .ok()
            .flatten()
            .and_then(|s| Url::parse(&s).ok())
            .filter(|u| validate_public_http_url(u).is_ok())
        {
            sync_url = u;
        }
        if is_login_like_url(&sync_url) {
            saw_login_stage = true;
        }
        let Ok(html) = page.content().await else {
            if last_observed.len() > base {
                apply_captured_cookies(&last_observed, &sync_url, http);
                return true;
            }
            break;
        };
        let lower = html.to_ascii_lowercase();
        let has_pw = lower.contains("type=\"password\"") || lower.contains("type='password'");
        if has_pw {
            saw_login_stage = true;
        }
        let off_login =
            !has_pw && !is_login_like_url(&sync_url) && !looks_like_rendered_challenge(&html);
        if off_login && (saw_login_stage || last_observed.len() > base) {
            tokio::time::sleep(Duration::from_millis(1_200)).await;
            copy_page_cookies(page, &sync_url, http).await;
            return true;
        }
    }
    false
}

fn is_login_like_url(url: &Url) -> bool {
    let lower = url.path().to_ascii_lowercase();
    lower.contains("/login")
        || lower.contains("/signin")
        || lower.contains("/sign-in")
        || lower.contains("/dang-nhap")
        || lower.contains("/auth")
}

async fn render_and_extract(
    page: &chromiumoxide::Page,
    requested_url: &Url,
    http: &HttpClient,
) -> Result<FetchedPage, ScrapeError> {
    let _ = page.wait_for_navigation().await;
    wait_for_dynamic_dom(page).await;

    // Incrementally scroll down to trigger IntersectionObserver / lazy-loaded / infinite-scroll images.
    let scroll_script = r"
        (async () => {
            const step = Math.max(window.innerHeight || 800, 600);
            const limit = Math.min(document.body ? document.body.scrollHeight : 0, 50000);
            for (let y = 0; y <= limit; y += step) {
                window.scrollTo(0, y);
                window.dispatchEvent(new Event('scroll'));
                await new Promise(r => setTimeout(r, 90));
            }
            window.scrollTo(0, 0);
        })()
    ";
    let _ = page.evaluate(scroll_script).await;
    tokio::time::sleep(RENDER_SETTLE_DELAY).await;

    let final_url = page
        .url()
        .await
        .ok()
        .flatten()
        .and_then(|s| Url::parse(&s).ok())
        .unwrap_or_else(|| requested_url.clone());

    // Prevent JS-driven navigation SSRF to loopback, private LAN, or non-HTTP(S) schemes.
    validate_public_http_url(&final_url)?;

    let html = page
        .content()
        .await
        .map_err(|e| ScrapeError::Headless(format!("failed to read rendered HTML: {e}")))?;
    let response_bytes = html.len();

    copy_page_cookies(page, &final_url, http).await;

    if looks_like_rendered_challenge(&html) {
        let host = final_url.host_str().unwrap_or("unknown").to_owned();
        return Err(ScrapeError::Blocked {
            host,
            url: final_url,
        });
    }

    Ok(FetchedPage {
        final_url,
        html,
        status: 200,
        content_type: Some("text/html".to_owned()),
        response_bytes,
    })
}

async fn wait_for_dynamic_dom(page: &chromiumoxide::Page) {
    let ready_expr = r"
        (() => {
            if (document.readyState !== 'complete') return false;
            if (document.getElementById('__NEXT_DATA__')) return true;
            if (document.querySelectorAll('img').length >= 3) return true;
            return false;
        })()
    ";
    for _ in 0..6 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if let Ok(eval) = page.evaluate(ready_expr).await
            && eval.into_value::<bool>().ok() == Some(true)
        {
            break;
        }
    }
}

async fn copy_page_cookies(page: &chromiumoxide::Page, url: &Url, http: &HttpClient) {
    if let Ok(cookies) = page.get_cookies().await {
        let captured: Vec<(String, String, String, String)> = cookies
            .into_iter()
            .map(|c| (c.name, c.value, c.domain, c.path))
            .collect();
        apply_captured_cookies(&captured, url, http);
    }
}

fn apply_captured_cookies(
    cookies: &[(String, String, String, String)],
    url: &Url,
    http: &HttpClient,
) {
    for (name, value, domain, path) in cookies {
        let clean_path = if path.is_empty() { "/" } else { path.as_str() };
        let header = if domain.trim().is_empty() {
            format!("{name}={value}; Path={clean_path}")
        } else {
            let clean_domain = domain.trim().trim_start_matches('.');
            format!("{name}={value}; Domain={clean_domain}; Path={clean_path}")
        };
        http.add_cookie(&header, url);
    }
}

fn looks_like_rendered_challenge(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    lower.contains("cf-browser-verification")
        || lower.contains("cf_chl_opt")
        || lower.contains("just a moment...")
        || lower.contains("verify you are human")
}

/// Locates a locally installed Chromium-based browser executable on the host system.
pub fn find_browser_executable() -> Option<PathBuf> {
    let candidates = [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
        "/usr/bin/microsoft-edge",
    ];
    candidates.iter().map(PathBuf::from).find(|p| p.is_file())
}
