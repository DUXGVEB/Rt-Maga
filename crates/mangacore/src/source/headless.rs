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

/// Shut down the idle background headless browser after 5 minutes of inactivity.
const IDLE_SHUTDOWN_AFTER: Duration = Duration::from_secs(300);

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
                tab_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_TABS)),
                reaper_running: AtomicBool::new(false),
            }),
        }
    }

    /// Renders `url` in a headless browser tab, scrolls down to trigger lazy-loaded images,
    /// copies cookies into `http`, and returns the rendered HTML.
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
        validate_public_http_url(url)?;
        let proxy_args = http.chromium_proxy_args_for_url(url)?;
        let exe = find_browser_executable().ok_or_else(|| {
            ScrapeError::Headless("no compatible local browser executable found".to_owned())
        })?;

        let mut builder = BrowserConfig::builder()
            .chrome_executable(exe)
            .with_head()
            .arg(format!("--user-agent={DEFAULT_USER_AGENT}"));
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
            .map_err(|e| ScrapeError::Headless(format!("failed to open verification tab: {e}")))?;

        let deadline = tokio::time::Instant::now() + MANUAL_VERIFY_TIMEOUT;
        let mut verified = false;

        while tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if let Ok(html) = page.content().await
                && !looks_like_rendered_challenge(&html)
            {
                copy_page_cookies(&page, url, http).await;
                verified = true;
                break;
            }
        }

        let _ = browser.close().await;
        handler_task.abort();

        if verified {
            info!(%url, "manual verification completed and cookies synced");
            Ok(())
        } else {
            Err(ScrapeError::Headless(
                "manual verification timed out before challenge cleared".to_owned(),
            ))
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

async fn render_and_extract(
    page: &chromiumoxide::Page,
    requested_url: &Url,
    http: &HttpClient,
) -> Result<FetchedPage, ScrapeError> {
    let _ = page.wait_for_navigation().await;
    tokio::time::sleep(RENDER_SETTLE_DELAY).await;

    // Incrementally scroll down to trigger IntersectionObserver / lazy-loaded images.
    let scroll_script = r"
        (async () => {
            const step = Math.max(window.innerHeight || 800, 600);
            const limit = Math.min(document.body ? document.body.scrollHeight : 0, 40000);
            for (let y = 0; y <= limit; y += step) {
                window.scrollTo(0, y);
                await new Promise(r => setTimeout(r, 80));
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

    copy_page_cookies(page, &final_url, http).await;

    if looks_like_rendered_challenge(&html) {
        let host = final_url.host_str().unwrap_or("unknown").to_owned();
        return Err(ScrapeError::Blocked {
            host,
            url: final_url,
        });
    }

    Ok(FetchedPage { final_url, html })
}

async fn copy_page_cookies(page: &chromiumoxide::Page, url: &Url, http: &HttpClient) {
    if let Ok(cookies) = page.get_cookies().await {
        for cookie in cookies {
            let header = format!("{}={}; Path={}", cookie.name, cookie.value, cookie.path);
            http.add_cookie(&header, url);
        }
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
