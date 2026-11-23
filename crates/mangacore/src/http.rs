//! Polite HTTP client with cookie jar, per-host concurrency limits, retries, SSRF checks,
//! and optional application-internal HTTP/HTTPS/SOCKS5 (`socks5h://`) proxy routing.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use bytes::Bytes;
use dashmap::DashMap;
use reqwest::cookie::Jar;
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, CONTENT_TYPE, HeaderMap, HeaderValue, REFERER};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;
use tracing::{debug, warn};
use url::Url;

use crate::doh::{DohConfig, DohResolver};
use crate::error::ScrapeError;
use crate::throttle::BandwidthLimiter;

/// Realistic desktop browser User-Agent used for page and image requests.
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36";

/// Maximum request timeout for page and image downloads.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

/// Timeout used for explicit proxy connectivity & latency checks.
const PROXY_TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum number of retry attempts on transient 5xx, 429, or transport errors.
const MAX_RETRIES: u8 = 2;

/// Default maximum concurrent requests allowed per remote host.
const DEFAULT_MAX_CONN_PER_HOST: usize = 4;

/// Maximum HTML document payload buffered in memory (16 MiB).
const MAX_HTML_BYTES: usize = 16 * 1024 * 1024;

/// Maximum single image payload buffered in memory (32 MiB).
const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

/// Supported application-internal proxy protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProxyProtocol {
    /// Standard HTTP CONNECT / forward proxy.
    #[default]
    Http,
    /// TLS-encrypted HTTPS proxy.
    Https,
    /// SOCKS5 proxy using remote DNS resolution (`socks5h://`) to prevent local DNS leaks.
    Socks5,
}

impl ProxyProtocol {
    /// Returns the `reqwest` proxy URL scheme (uses `socks5h` for SOCKS5 remote DNS).
    pub fn as_reqwest_scheme(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
            Self::Socks5 => "socks5h",
        }
    }

    /// Returns the Chromium `--proxy-server` scheme string.
    pub fn as_chromium_scheme(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
            Self::Socks5 => "socks5",
        }
    }
}

/// Connection parameters for a single HTTP, HTTPS, or SOCKS5 proxy server.
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProxyEndpoint {
    /// Proxy protocol (`http`, `https`, or `socks5`).
    pub protocol: ProxyProtocol,
    /// Proxy hostname or IP address.
    pub host: String,
    /// Proxy TCP port (`1..=65535`).
    pub port: u16,
    /// Optional proxy authentication username.
    pub username: Option<String>,
    /// Optional proxy authentication password (never logged).
    pub password: Option<String>,
}

impl fmt::Debug for ProxyEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProxyEndpoint")
            .field("protocol", &self.protocol)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username.as_deref().map(|_| "<redacted>"))
            .field("password", &self.password.as_deref().map(|_| "<redacted>"))
            .finish()
    }
}

impl ProxyEndpoint {
    /// Returns `true` when both `host` and `port` are populated.
    pub fn is_configured(&self) -> bool {
        !self.host.trim().is_empty() && self.port > 0
    }

    /// Validates that the proxy host and port are syntactically safe and well-formed.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::Proxy`] if host or port is missing or contains disallowed characters.
    pub fn validate(&self) -> Result<(), ScrapeError> {
        let host = self.host.trim();
        if host.is_empty() {
            return Err(ScrapeError::Proxy("proxy host is empty".to_owned()));
        }
        if self.port == 0 {
            return Err(ScrapeError::Proxy(
                "proxy port must be 1..=65535".to_owned(),
            ));
        }
        if host.chars().any(|c| {
            c.is_whitespace() || c.is_control() || matches!(c, '@' | '/' | '\\' | '?' | '#')
        }) {
            return Err(ScrapeError::Proxy(
                "proxy host contains invalid characters".to_owned(),
            ));
        }
        Ok(())
    }

    /// Returns a credential-free `scheme://host:port` string safe for logs and UI display.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::Proxy`] if the endpoint is invalid.
    pub fn sanitized_url(&self) -> Result<String, ScrapeError> {
        self.validate()?;
        let host = self.host.trim();
        let host_fmt = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host.to_owned()
        };
        Ok(format!(
            "{}://{host_fmt}:{}",
            self.protocol.as_reqwest_scheme(),
            self.port
        ))
    }

    fn to_reqwest_proxy(&self) -> Result<reqwest::Proxy, ScrapeError> {
        let url = self.sanitized_url()?;
        let mut proxy = reqwest::Proxy::all(&url)
            .map_err(|e| ScrapeError::Proxy(redact_secrets(&e.to_string(), self)))?;
        if let Some(user) = self.username.as_deref().map(str::trim)
            && !user.is_empty()
        {
            let pass = self.password.as_deref().unwrap_or("");
            proxy = proxy.basic_auth(user, pass);
        }
        Ok(proxy)
    }

    fn cache_key(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.protocol.as_reqwest_scheme().as_bytes());
        hasher.update(b"|");
        hasher.update(self.host.trim().to_ascii_lowercase().as_bytes());
        hasher.update(b"|");
        hasher.update(self.port.to_be_bytes());
        hasher.update(b"|");
        hasher.update(self.username.as_deref().unwrap_or("").as_bytes());
        hasher.update(b"|");
        hasher.update(self.password.as_deref().unwrap_or("").as_bytes());
        let digest = hasher.finalize();
        let mut out = String::with_capacity(64);
        for b in digest {
            use std::fmt::Write;
            let _ = write!(&mut out, "{b:02x}");
        }
        out
    }

    /// Returns Chromium `--proxy-server` and optional DNS-leak prevention resolver flags.
    pub fn chromium_args(&self) -> Option<(String, Option<String>)> {
        self.validate().ok()?;
        let host = self.host.trim();
        let host_fmt = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host.to_owned()
        };
        let proxy_arg = format!(
            "--proxy-server={}://{host_fmt}:{}",
            self.protocol.as_chromium_scheme(),
            self.port
        );
        let resolver_arg = if self.protocol == ProxyProtocol::Socks5 {
            Some(format!(
                "--host-resolver-rules=MAP * ~NOTFOUND , EXCLUDE {host}"
            ))
        } else {
            None
        };
        Some((proxy_arg, resolver_arg))
    }
}

/// Routing policy for a specific manga source domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DomainRuleAction {
    /// Follow the global proxy enabled/disabled setting.
    #[default]
    UseDefault,
    /// Always route this domain through the configured global proxy (even if global mode is Direct).
    ForceProxy,
    /// Always connect directly to this domain (bypass global proxy).
    ForceDirect,
    /// Route this domain through its own custom [`ProxyEndpoint`].
    CustomProxy,
}

/// Per-source / per-domain proxy routing override.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DomainProxyRule {
    /// Domain name to match (matches exact domain and subdomains, e.g. `example.org`).
    pub domain: String,
    /// Routing action applied to matching requests.
    pub action: DomainRuleAction,
    /// Dedicated proxy endpoint used when `action` is [`DomainRuleAction::CustomProxy`].
    pub custom_proxy: Option<ProxyEndpoint>,
}

/// Application-internal proxy configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProxyConfig {
    /// `false` (default) = Direct Mode; `true` = Global Proxy Mode.
    pub enabled: bool,
    /// Default proxy endpoint used when global mode or `ForceProxy` rules apply.
    pub default_proxy: ProxyEndpoint,
    /// Fail-closed by default (`false`) so failed proxy requests never leak the real IP.
    pub allow_direct_fallback: bool,
    /// Optional per-domain routing overrides.
    pub domain_rules: Vec<DomainProxyRule>,
}

impl ProxyConfig {
    /// Cleans whitespace and bounds rule counts for safe storage and matching.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        self.default_proxy.host = self.default_proxy.host.trim().to_owned();
        if let Some(u) = self.default_proxy.username.as_mut() {
            *u = u.trim().to_owned();
            if u.is_empty() {
                self.default_proxy.username = None;
            }
        }
        self.domain_rules.truncate(128);
        self.domain_rules.retain_mut(|rule| {
            rule.domain = normalize_domain_pattern(&rule.domain);
            if let Some(custom) = rule.custom_proxy.as_mut() {
                custom.host = custom.host.trim().to_owned();
                if let Some(u) = custom.username.as_mut() {
                    *u = u.trim().to_owned();
                    if u.is_empty() {
                        custom.username = None;
                    }
                }
            }
            !rule.domain.is_empty()
        });
        self
    }

    fn match_explicit_rule(&self, url: &Url) -> Option<&DomainProxyRule> {
        let host = url.host_str().unwrap_or("").to_ascii_lowercase();
        self.domain_rules
            .iter()
            .filter(|r| {
                r.action != DomainRuleAction::UseDefault && domain_matches(&host, &r.domain)
            })
            .max_by_key(|r| r.domain.len())
    }

    fn apply_rule(&self, rule: &DomainProxyRule) -> Option<(ProxyEndpoint, bool)> {
        match rule.action {
            DomainRuleAction::ForceDirect => None,
            DomainRuleAction::ForceProxy => {
                Some((self.default_proxy.clone(), self.allow_direct_fallback))
            }
            DomainRuleAction::CustomProxy => {
                let ep = rule
                    .custom_proxy
                    .clone()
                    .filter(ProxyEndpoint::is_configured)
                    .unwrap_or_else(|| self.default_proxy.clone());
                Some((ep, self.allow_direct_fallback))
            }
            DomainRuleAction::UseDefault => {
                if self.enabled {
                    Some((self.default_proxy.clone(), self.allow_direct_fallback))
                } else {
                    None
                }
            }
        }
    }

    /// Resolves whether `url` should route directly or through a specific [`ProxyEndpoint`].
    pub fn resolve_for_url(&self, url: &Url) -> Option<(ProxyEndpoint, bool)> {
        if let Some(rule) = self.match_explicit_rule(url) {
            return self.apply_rule(rule);
        }
        if self.enabled {
            Some((self.default_proxy.clone(), self.allow_direct_fallback))
        } else {
            None
        }
    }

    /// Resolves proxy routing for an image request: checks explicit domain rules on `image_url`
    /// first, then explicit domain rules on `referer` (so CDN images inherit their manga site's
    /// `ForceProxy`, `CustomProxy`, or `ForceDirect` rule), before falling back to global mode.
    pub fn resolve_for_image(
        &self,
        image_url: &Url,
        referer: Option<&Url>,
    ) -> Option<(ProxyEndpoint, bool)> {
        if let Some(rule) = self.match_explicit_rule(image_url) {
            return self.apply_rule(rule);
        }
        if let Some(ref_url) = referer
            && let Some(rule) = self.match_explicit_rule(ref_url)
        {
            return self.apply_rule(rule);
        }
        if self.enabled {
            Some((self.default_proxy.clone(), self.allow_direct_fallback))
        } else {
            None
        }
    }
}

/// Result returned by [`HttpClient::test_proxy`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyTestResult {
    /// `true` when the request reached the target through the proxy.
    pub ok: bool,
    /// Round-trip latency in milliseconds.
    pub latency_ms: u64,
    /// HTTP status code received from the target, if any.
    pub status_code: Option<u16>,
    /// Target URL that was probed.
    pub target_url: String,
    /// Credential-free proxy descriptor (`scheme://host:port`).
    pub via_proxy: String,
    /// Sanitized error reason when `ok` is `false`.
    pub error: Option<String>,
}

/// Configuration for [`HttpClient`].
#[derive(Debug, Clone)]
pub struct HttpPolicy {
    /// User-Agent header sent with every request.
    pub user_agent: String,
    /// Per-request timeout.
    pub timeout: Duration,
    /// Maximum concurrent requests to any single host.
    pub max_conn_per_host: usize,
    /// Initial application-internal proxy configuration (defaults to Direct Mode).
    pub proxy: ProxyConfig,
    /// Initial in-app DNS-over-HTTPS (`DoH`) provider configuration.
    pub doh: DohConfig,
    /// Global aggregate download speed limit in KiB/s (`0` = unlimited).
    pub speed_limit_kbps: u32,
}

impl Default for HttpPolicy {
    fn default() -> Self {
        Self {
            user_agent: DEFAULT_USER_AGENT.to_owned(),
            timeout: DEFAULT_TIMEOUT,
            max_conn_per_host: DEFAULT_MAX_CONN_PER_HOST,
            proxy: ProxyConfig::default(),
            doh: DohConfig::default(),
            speed_limit_kbps: 0,
        }
    }
}

/// An HTML response downloaded by [`HttpClient`].
#[derive(Debug, Clone)]
pub struct FetchedPage {
    /// Final URL after following redirects.
    pub final_url: Url,
    /// Decoded response body text.
    pub html: String,
}

/// Raw binary response downloaded for an image resource.
#[derive(Debug, Clone)]
pub struct FetchedBytes {
    /// Raw image payload.
    pub bytes: Bytes,
    /// MIME content type (e.g. `"image/jpeg"`).
    pub content_type: String,
}

/// Shared HTTP client with cookie jar, per-host concurrency limiting, and optional proxy routing.
#[derive(Debug, Clone)]
pub struct HttpClient {
    inner: Arc<HttpClientInner>,
}

#[derive(Debug)]
struct HttpClientInner {
    direct_client: RwLock<Client>,
    jar: Arc<Jar>,
    user_agent: String,
    timeout: Duration,
    host_semaphores: DashMap<String, Arc<Semaphore>>,
    max_conn_per_host: AtomicUsize,
    proxy_config: RwLock<ProxyConfig>,
    doh_config: RwLock<DohConfig>,
    proxy_clients: DashMap<String, Client>,
    limiter: BandwidthLimiter,
}

impl HttpClient {
    /// Builds a new [`HttpClient`] using [`HttpPolicy::default`].
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::Network`] if the underlying TLS/HTTP client cannot be initialized.
    pub fn new() -> Result<Self, ScrapeError> {
        Self::with_policy(HttpPolicy::default())
    }

    /// Builds a new [`HttpClient`] with a custom [`HttpPolicy`].
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::Network`] if the underlying TLS/HTTP client cannot be initialized.
    pub fn with_policy(policy: HttpPolicy) -> Result<Self, ScrapeError> {
        let jar = Arc::new(Jar::default());
        let clean_doh = policy.doh.sanitized();
        let direct_client = build_direct_client(
            &policy.user_agent,
            policy.timeout,
            Arc::clone(&jar),
            &clean_doh,
        )?;

        Ok(Self {
            inner: Arc::new(HttpClientInner {
                direct_client: RwLock::new(direct_client),
                jar,
                user_agent: policy.user_agent,
                timeout: policy.timeout,
                host_semaphores: DashMap::new(),
                max_conn_per_host: AtomicUsize::new(policy.max_conn_per_host.max(1)),
                proxy_config: RwLock::new(policy.proxy.sanitized()),
                doh_config: RwLock::new(clean_doh),
                proxy_clients: DashMap::new(),
                limiter: BandwidthLimiter::new(policy.speed_limit_kbps),
            }),
        })
    }

    /// Updates the global aggregate network download speed ceiling live (`0` = unlimited).
    pub fn set_speed_limit_kbps(&self, limit_kbps: u32) {
        self.inner.limiter.set_limit_kbps(limit_kbps);
    }

    /// Returns the currently active global download speed limit in KiB/s (`0` = unlimited).
    pub fn speed_limit_kbps(&self) -> u32 {
        self.inner.limiter.limit_kbps()
    }

    /// Updates the live DNS-over-HTTPS ([`DohConfig`]) provider and rebuilds the direct HTTP client
    /// when the configuration changes.
    pub fn set_doh_config(&self, config: DohConfig) {
        let clean = config.sanitized();
        let changed = match self.inner.doh_config.write() {
            Ok(mut guard) => {
                let diff = *guard != clean;
                *guard = clean.clone();
                diff
            }
            Err(poisoned) => {
                *poisoned.into_inner() = clean.clone();
                true
            }
        };
        if !changed {
            return;
        }
        match build_direct_client(
            &self.inner.user_agent,
            self.inner.timeout,
            Arc::clone(&self.inner.jar),
            &clean,
        ) {
            Ok(next_client) => {
                match self.inner.direct_client.write() {
                    Ok(mut g) => *g = next_client,
                    Err(p) => *p.into_inner() = next_client,
                }
                debug!(
                    enabled = clean.enabled,
                    provider = %clean.provider_id,
                    profile = %clean.profile_id,
                    "updated in-app DoH resolver configuration"
                );
            }
            Err(err) => {
                warn!(%err, "failed to rebuild direct client with DoH resolver");
            }
        }
    }

    fn direct_client(&self) -> Client {
        match self.inner.direct_client.read() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    /// Updates the live per-host concurrency limit and resets cached host semaphores if changed.
    pub fn set_max_conn_per_host(&self, max_conn: usize) {
        let clamped = max_conn.clamp(1, 16);
        let prev = self.inner.max_conn_per_host.swap(clamped, Ordering::AcqRel);
        if prev != clamped {
            self.inner.host_semaphores.clear();
        }
    }

    /// Updates the live [`ProxyConfig`] and clears cached proxy clients if the config changed.
    pub fn set_proxy_config(&self, config: ProxyConfig) {
        let clean = config.sanitized();
        let changed = match self.inner.proxy_config.write() {
            Ok(mut guard) => {
                let diff = *guard != clean;
                *guard = clean;
                diff
            }
            Err(poisoned) => {
                *poisoned.into_inner() = clean;
                true
            }
        };
        if changed {
            self.inner.proxy_clients.clear();
            debug!("updated in-app proxy configuration and cleared cached proxy clients");
        }
    }

    /// Returns a snapshot of the active [`ProxyConfig`].
    pub fn proxy_config(&self) -> ProxyConfig {
        match self.inner.proxy_config.read() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Returns Chromium `--proxy-server` and optional DNS-resolver flags for `url` when proxied.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::Proxy`] if `url` requires a proxy under active settings, the proxy
    /// endpoint is unconfigured/invalid, and direct fallback is disabled (fail-closed).
    pub fn chromium_proxy_args_for_url(
        &self,
        url: &Url,
    ) -> Result<Option<(String, Option<String>)>, ScrapeError> {
        let Some((endpoint, allow_fallback)) = self.proxy_config().resolve_for_url(url) else {
            return Ok(None);
        };
        if let Some(args) = endpoint.chromium_args() {
            return Ok(Some(args));
        }
        if allow_fallback {
            warn!("proxy invalid for headless browser; falling back to direct mode (user-enabled)");
            return Ok(None);
        }
        endpoint.validate()?;
        Err(ScrapeError::Proxy(
            "proxy mode is active but proxy endpoint is not configured".to_owned(),
        ))
    }

    /// Injects a `Set-Cookie` style header value for `url` into the shared cookie jar.
    pub fn add_cookie(&self, cookie_header: &str, url: &Url) {
        self.inner.jar.add_cookie_str(cookie_header, url);
    }

    /// Tests connectivity and round-trip latency through `endpoint` using a minimal cookie-free
    /// `HEAD` (or fallback `GET`) request to `test_url_raw` (defaults to `https://example.com`).
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::InvalidUrl`] or [`ScrapeError::DisallowedHost`] if the target URL
    /// fails SSRF validation, or [`ScrapeError::Proxy`] if the proxy endpoint itself is invalid.
    pub async fn test_proxy(
        &self,
        endpoint: &ProxyEndpoint,
        test_url_raw: Option<&str>,
    ) -> Result<ProxyTestResult, ScrapeError> {
        let via_proxy = endpoint.sanitized_url()?;
        let target_url = match test_url_raw.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => normalize_url(raw)?,
            None => normalize_url("https://example.com")?,
        };
        validate_public_http_url(&target_url)?;

        let proxy = endpoint.to_reqwest_proxy()?;
        let test_client = build_reqwest_client(
            &self.inner.user_agent,
            PROXY_TEST_TIMEOUT,
            None,
            Some(proxy),
            None,
        )
        .map_err(|e| ScrapeError::Proxy(redact_secrets(&e.to_string(), endpoint)))?;

        let started = Instant::now();
        let outcome = match test_client.head(target_url.as_str()).send().await {
            Ok(resp) if resp.status() == StatusCode::METHOD_NOT_ALLOWED => {
                test_client.get(target_url.as_str()).send().await
            }
            other => other,
        };
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

        match outcome {
            Ok(resp) => {
                let code = resp.status().as_u16();
                if resp.status() == StatusCode::PROXY_AUTHENTICATION_REQUIRED {
                    Ok(ProxyTestResult {
                        ok: false,
                        latency_ms,
                        status_code: Some(code),
                        target_url: target_url.into(),
                        via_proxy,
                        error: Some("Proxy authentication failed (HTTP 407)".to_owned()),
                    })
                } else {
                    Ok(ProxyTestResult {
                        ok: true,
                        latency_ms,
                        status_code: Some(code),
                        target_url: target_url.into(),
                        via_proxy,
                        error: None,
                    })
                }
            }
            Err(err) => Ok(ProxyTestResult {
                ok: false,
                latency_ms,
                status_code: None,
                target_url: target_url.into(),
                via_proxy,
                error: Some(redact_secrets(&err.to_string(), endpoint)),
            }),
        }
    }

    /// Fetches an HTML page while enforcing per-host concurrency limits, proxy routing,
    /// SSRF checks, and anti-bot challenge detection.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::DisallowedHost`], [`ScrapeError::Blocked`], [`ScrapeError::Proxy`],
    /// [`ScrapeError::HttpStatus`], or [`ScrapeError::Network`].
    pub async fn fetch_html(&self, url: &Url) -> Result<FetchedPage, ScrapeError> {
        validate_public_http_url(url)?;
        let _permit = self.acquire_host_permit(url).await;

        let limiter = &self.inner.limiter;
        let timeout_secs = self.inner.timeout.as_secs();

        if let Some((endpoint, allow_fallback)) = self.proxy_config().resolve_for_url(url) {
            let proxy_client = match self.client_for_proxy(&endpoint) {
                Ok(c) => c,
                Err(err) if allow_fallback => {
                    warn!(%err, "proxy unavailable; falling back to direct mode (user-enabled)");
                    return fetch_html_with_client(
                        &self.direct_client(),
                        url,
                        None,
                        limiter,
                        timeout_secs,
                    )
                    .await;
                }
                Err(err) => return Err(err),
            };

            match fetch_html_with_client(&proxy_client, url, Some(&endpoint), limiter, timeout_secs)
                .await
            {
                Ok(page) => return Ok(page),
                Err(err) if allow_fallback && is_proxy_fallback_eligible(&err) => {
                    warn!(%err, "proxy request failed; falling back to direct mode (user-enabled)");
                    return fetch_html_with_client(
                        &self.direct_client(),
                        url,
                        None,
                        limiter,
                        timeout_secs,
                    )
                    .await;
                }
                Err(err) => return Err(err),
            }
        }

        fetch_html_with_client(&self.direct_client(), url, None, limiter, timeout_secs).await
    }

    /// Downloads raw image bytes with an explicit `Referer` header and active proxy routing.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::DisallowedHost`], [`ScrapeError::Proxy`], [`ScrapeError::HttpStatus`],
    /// [`ScrapeError::InvalidImageFormat`], [`ScrapeError::Timeout`], or [`ScrapeError::Network`].
    pub async fn fetch_image(
        &self,
        url: &Url,
        referer: Option<&Url>,
    ) -> Result<FetchedBytes, ScrapeError> {
        validate_public_http_url(url)?;
        let _permit = self.acquire_host_permit(url).await;
        let limiter = &self.inner.limiter;
        let timeout_secs = self.inner.timeout.as_secs();

        // Match explicit per-source rule on image URL first, then explicit rule on referer domain
        // (so CDN images inherit their manga source's ForceProxy / CustomProxy / ForceDirect rule),
        // before falling back to global mode.
        let route = self.proxy_config().resolve_for_image(url, referer);

        if let Some((endpoint, allow_fallback)) = route {
            let proxy_client = match self.client_for_proxy(&endpoint) {
                Ok(c) => c,
                Err(err) if allow_fallback => {
                    warn!(%err, "proxy unavailable for image; falling back to direct mode");
                    return fetch_image_with_client(
                        &self.direct_client(),
                        url,
                        referer,
                        None,
                        limiter,
                        timeout_secs,
                    )
                    .await;
                }
                Err(err) => return Err(err),
            };

            match fetch_image_with_client(
                &proxy_client,
                url,
                referer,
                Some(&endpoint),
                limiter,
                timeout_secs,
            )
            .await
            {
                Ok(bytes) => return Ok(bytes),
                Err(err) if allow_fallback && is_proxy_fallback_eligible(&err) => {
                    warn!(%err, "proxied image fetch failed; falling back to direct mode");
                    return fetch_image_with_client(
                        &self.direct_client(),
                        url,
                        referer,
                        None,
                        limiter,
                        timeout_secs,
                    )
                    .await;
                }
                Err(err) => return Err(err),
            }
        }

        fetch_image_with_client(
            &self.direct_client(),
            url,
            referer,
            None,
            limiter,
            timeout_secs,
        )
        .await
    }

    fn client_for_proxy(&self, endpoint: &ProxyEndpoint) -> Result<Client, ScrapeError> {
        endpoint.validate()?;
        let key = endpoint.cache_key();
        if let Some(existing) = self.inner.proxy_clients.get(&key) {
            return Ok(existing.value().clone());
        }
        let proxy = endpoint.to_reqwest_proxy()?;
        let client = build_reqwest_client(
            &self.inner.user_agent,
            self.inner.timeout,
            Some(Arc::clone(&self.inner.jar)),
            Some(proxy),
            None,
        )
        .map_err(|e| ScrapeError::Proxy(redact_secrets(&e.to_string(), endpoint)))?;
        self.inner.proxy_clients.insert(key, client.clone());
        Ok(client)
    }

    async fn acquire_host_permit(&self, url: &Url) -> tokio::sync::OwnedSemaphorePermit {
        let host = url.host_str().unwrap_or("default").to_ascii_lowercase();
        let limit = self.inner.max_conn_per_host.load(Ordering::Acquire).max(1);
        let sem = Arc::clone(
            self.inner
                .host_semaphores
                .entry(host)
                .or_insert_with(|| Arc::new(Semaphore::new(limit)))
                .value(),
        );
        sem.acquire_owned()
            .await
            .expect("host semaphore is never closed")
    }
}

fn build_direct_client(
    user_agent: &str,
    timeout: Duration,
    jar: Arc<Jar>,
    doh: &DohConfig,
) -> Result<Client, ScrapeError> {
    let resolver: Option<Arc<DohResolver>> = if doh.enabled {
        let fallback = if doh.allow_system_fallback {
            crate::doh::DohFallbackPolicy::SystemDns
        } else {
            crate::doh::DohFallbackPolicy::FailClosed
        };
        match doh.resolve_endpoint() {
            Ok(endpoint) => match DohResolver::new(endpoint, fallback) {
                Ok(r) => Some(Arc::new(r)),
                Err(err) if doh.allow_system_fallback => {
                    warn!(%err, "DoH resolver init failed; falling back to system DNS");
                    None
                }
                Err(err) => return Err(err),
            },
            Err(err) if doh.allow_system_fallback => {
                warn!(%err, "DoH endpoint unresolved; falling back to system DNS");
                None
            }
            Err(err) => return Err(err),
        }
    } else {
        None
    };

    build_reqwest_client(user_agent, timeout, Some(jar), None, resolver)
}

fn build_reqwest_client(
    user_agent: &str,
    timeout: Duration,
    jar: Option<Arc<Jar>>,
    proxy: Option<reqwest::Proxy>,
    dns_resolver: Option<Arc<DohResolver>>,
) -> Result<Client, ScrapeError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT_LANGUAGE,
        HeaderValue::from_static("vi-VN,vi;q=0.9,en-US;q=0.8,en;q=0.7"),
    );

    let mut builder = Client::builder()
        .user_agent(user_agent)
        .default_headers(headers)
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 {
                return attempt.error("too many redirects");
            }
            if let Err(err) = validate_public_http_url(attempt.url()) {
                return attempt.error(err);
            }
            attempt.follow()
        }));

    if let Some(cookie_jar) = jar {
        builder = builder.cookie_provider(cookie_jar);
    }
    if let Some(resolver) = dns_resolver {
        builder = builder.dns_resolver(resolver);
    }
    if let Some(p) = proxy {
        builder = builder.proxy(p);
    } else {
        builder = builder.no_proxy();
    }

    Ok(builder.build()?)
}

async fn fetch_html_with_client(
    client: &Client,
    url: &Url,
    proxy_ep: Option<&ProxyEndpoint>,
    limiter: &BandwidthLimiter,
    timeout_secs: u64,
) -> Result<FetchedPage, ScrapeError> {
    let mut last_err: Option<ScrapeError> = None;
    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(400 * u64::from(attempt))).await;
        }

        let resp = client
            .get(url.as_str())
            .header(
                ACCEPT,
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .send()
            .await;

        match resp {
            Ok(response) => {
                let status = response.status();
                if status == StatusCode::PROXY_AUTHENTICATION_REQUIRED {
                    return Err(ScrapeError::Proxy(
                        "proxy authentication failed (HTTP 407)".to_owned(),
                    ));
                }
                let final_url = response.url().clone();
                let raw_bytes =
                    read_bounded_bytes(response, MAX_HTML_BYTES, &final_url, limiter).await?;
                let body = String::from_utf8_lossy(&raw_bytes).into_owned();

                if looks_like_challenge(status, &body) {
                    let host = final_url.host_str().unwrap_or("unknown").to_owned();
                    return Err(ScrapeError::Blocked {
                        host,
                        url: final_url,
                    });
                }

                if status.is_success() {
                    debug!(%final_url, len = body.len(), proxied = proxy_ep.is_some(), "fetched html page");
                    return Ok(FetchedPage {
                        final_url,
                        html: body,
                    });
                }

                let err = ScrapeError::HttpStatus {
                    status: status.as_u16(),
                    url: final_url,
                };
                if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                    && attempt < MAX_RETRIES
                {
                    warn!(%err, attempt, "transient http status, retrying");
                    last_err = Some(err);
                    continue;
                }
                return Err(err);
            }
            Err(err) => {
                let retryable = err.is_timeout() || err.is_connect();
                let mapped = map_transport_err(err, proxy_ep, url, timeout_secs);
                if retryable && attempt < MAX_RETRIES {
                    warn!(%mapped, attempt, "transient transport error, retrying");
                    last_err = Some(mapped);
                    continue;
                }
                return Err(mapped);
            }
        }
    }

    Err(last_err.unwrap_or_else(|| ScrapeError::HttpStatus {
        status: 500,
        url: url.clone(),
    }))
}

async fn fetch_image_with_client(
    client: &Client,
    url: &Url,
    referer: Option<&Url>,
    proxy_ep: Option<&ProxyEndpoint>,
    limiter: &BandwidthLimiter,
    timeout_secs: u64,
) -> Result<FetchedBytes, ScrapeError> {
    let mut last_err: Option<ScrapeError> = None;
    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(300 * u64::from(attempt))).await;
        }

        let mut req = client
            .get(url.as_str())
            .header(ACCEPT, "image/avif,image/webp,image/apng,image/*,*/*;q=0.8");

        if let Some(ref_url) = referer
            && let Ok(val) = HeaderValue::from_str(ref_url.as_str())
        {
            req = req.header(REFERER, val);
        }

        match req.send().await {
            Ok(response) => {
                let status = response.status();
                if status == StatusCode::PROXY_AUTHENTICATION_REQUIRED {
                    return Err(ScrapeError::Proxy(
                        "proxy authentication failed (HTTP 407)".to_owned(),
                    ));
                }
                if status.is_success() {
                    let content_type = response
                        .headers()
                        .get(CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .map_or_else(
                            || guess_mime_from_url(url).to_owned(),
                            |s| {
                                s.split(';')
                                    .next()
                                    .unwrap_or("image/jpeg")
                                    .trim()
                                    .to_owned()
                            },
                        );
                    if !is_acceptable_image_mime(&content_type) {
                        return Err(ScrapeError::InvalidImageFormat {
                            url: url.clone(),
                            detail: format!("non-image MIME type '{content_type}'"),
                        });
                    }
                    let bytes = read_bounded_bytes(response, MAX_IMAGE_BYTES, url, limiter).await?;
                    if !has_valid_image_magic_bytes(&bytes) {
                        return Err(ScrapeError::InvalidImageFormat {
                            url: url.clone(),
                            detail: "payload header is not a supported image format".to_owned(),
                        });
                    }
                    return Ok(FetchedBytes {
                        bytes,
                        content_type,
                    });
                }
                let err = ScrapeError::HttpStatus {
                    status: status.as_u16(),
                    url: url.clone(),
                };
                if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                    && attempt < MAX_RETRIES
                {
                    last_err = Some(err);
                    continue;
                }
                return Err(err);
            }
            Err(err) => {
                let retryable = err.is_timeout() || err.is_connect();
                let mapped = map_transport_err(err, proxy_ep, url, timeout_secs);
                if retryable && attempt < MAX_RETRIES {
                    last_err = Some(mapped);
                    continue;
                }
                return Err(mapped);
            }
        }
    }

    Err(last_err.unwrap_or_else(|| ScrapeError::HttpStatus {
        status: 500,
        url: url.clone(),
    }))
}

fn map_transport_err(
    err: reqwest::Error,
    proxy_ep: Option<&ProxyEndpoint>,
    url: &Url,
    timeout_secs: u64,
) -> ScrapeError {
    if let Some(ep) = proxy_ep {
        ScrapeError::Proxy(redact_secrets(&err.to_string(), ep))
    } else if err.is_timeout() {
        ScrapeError::Timeout {
            url: url.clone(),
            timeout_secs,
        }
    } else {
        ScrapeError::Network(err)
    }
}

fn is_proxy_fallback_eligible(err: &ScrapeError) -> bool {
    matches!(
        err,
        ScrapeError::Proxy(_) | ScrapeError::Network(_) | ScrapeError::Timeout { .. }
    )
}

fn redact_secrets(raw: &str, ep: &ProxyEndpoint) -> String {
    let mut out = raw.to_owned();
    if let Some(pass) = ep.password.as_deref()
        && !pass.is_empty()
    {
        out = out.replace(pass, "***");
    }
    if let Some(user) = ep.username.as_deref()
        && !user.is_empty()
    {
        out = out.replace(user, "***");
    }
    out
}

fn normalize_domain_pattern(raw: &str) -> String {
    let trimmed = raw.trim().to_ascii_lowercase();
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(&trimmed);
    let host_only = without_scheme.split('/').next().unwrap_or("");
    let without_port = host_only.split(':').next().unwrap_or("");
    without_port
        .trim_start_matches("*.")
        .trim_start_matches('.')
        .trim_end_matches('.')
        .to_owned()
}

fn domain_matches(host: &str, rule_domain: &str) -> bool {
    if rule_domain.is_empty() || host.is_empty() {
        return false;
    }
    host == rule_domain
        || (host.len() > rule_domain.len()
            && host.ends_with(rule_domain)
            && host.as_bytes()[host.len() - rule_domain.len() - 1] == b'.')
}

/// Normalizes a user-pasted link: trims whitespace, adds `https://` if missing, strips fragments
/// and common tracking parameters (`utm_*`, `fbclid`, `gclid`).
///
/// # Errors
///
/// Returns [`ScrapeError::InvalidUrl`] if the string cannot be parsed into an HTTP(S) URL, or
/// [`ScrapeError::DisallowedHost`] if the host is local or private.
pub fn normalize_url(raw: &str) -> Result<Url, ScrapeError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ScrapeError::InvalidUrl("empty url".to_owned()));
    }

    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_owned()
    } else if trimmed.starts_with("//") {
        format!("https:{trimmed}")
    } else {
        format!("https://{trimmed}")
    };

    let mut url =
        Url::parse(&with_scheme).map_err(|e| ScrapeError::InvalidUrl(format!("{trimmed}: {e}")))?;

    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(ScrapeError::InvalidUrl(format!(
            "unsupported scheme: {}",
            url.scheme()
        )));
    }

    url.set_fragment(None);

    let kept_pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| {
            let lower = k.to_ascii_lowercase();
            !lower.starts_with("utm_") && lower != "fbclid" && lower != "gclid"
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    if kept_pairs.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut().clear().extend_pairs(kept_pairs);
    }

    validate_public_http_url(&url)?;
    Ok(url)
}

/// Rejects non-HTTP(S) schemes and loopback/private/link-local IP addresses to prevent SSRF.
///
/// # Errors
///
/// Returns [`ScrapeError::DisallowedHost`] if the URL targets a disallowed scheme or local address.
pub fn validate_public_http_url(url: &Url) -> Result<(), ScrapeError> {
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(ScrapeError::DisallowedHost(url.scheme().to_owned()));
    }
    let Some(host) = url.host_str() else {
        return Err(ScrapeError::InvalidUrl("missing host".to_owned()));
    };
    let lower = host.to_ascii_lowercase();
    if lower == "localhost"
        || lower.ends_with(".localhost")
        || lower.strip_suffix(".local").is_some()
    {
        return Err(ScrapeError::DisallowedHost(host.to_owned()));
    }
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>()
        && is_disallowed_ip(ip)
    {
        return Err(ScrapeError::DisallowedHost(host.to_owned()));
    }
    Ok(())
}

/// Returns `true` if `ip` is loopback, RFC1918 private, link-local, CGNAT, or unspecified.
pub(crate) fn is_disallowed_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_disallowed_ipv4(v4),
        IpAddr::V6(v6) => {
            let s0 = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || (s0 & 0xfe00) == 0xfc00
                || (s0 & 0xffc0) == 0xfe80
                || v6.to_ipv4_mapped().is_some_and(is_disallowed_ipv4)
        }
    }
}

fn is_disallowed_ipv4(v4: Ipv4Addr) -> bool {
    let octets = v4.octets();
    let is_cgnat = octets[0] == 100 && (octets[1] & 0xc0) == 64;
    v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_broadcast()
        || v4.is_unspecified()
        || octets[0] == 0
        || is_cgnat
}

async fn read_bounded_bytes(
    mut response: reqwest::Response,
    max_bytes: usize,
    url: &Url,
    limiter: &BandwidthLimiter,
) -> Result<Bytes, ScrapeError> {
    if let Some(len) = response.content_length()
        && u64::try_from(max_bytes).is_ok_and(|cap| len > cap)
    {
        return Err(ScrapeError::HttpStatus {
            status: StatusCode::PAYLOAD_TOO_LARGE.as_u16(),
            url: url.clone(),
        });
    }

    let initial_cap = response
        .content_length()
        .and_then(|n| usize::try_from(n).ok())
        .map_or(0, |n| n.min(max_bytes).min(256 * 1024));
    let mut buf = Vec::with_capacity(initial_cap);

    while let Some(chunk) = response.chunk().await? {
        if buf.len().saturating_add(chunk.len()) > max_bytes {
            return Err(ScrapeError::HttpStatus {
                status: StatusCode::PAYLOAD_TOO_LARGE.as_u16(),
                url: url.clone(),
            });
        }
        limiter.acquire(chunk.len()).await;
        buf.extend_from_slice(&chunk);
    }

    Ok(Bytes::from(buf))
}

/// Validates binary image magic headers so HTML error pages or corrupted responses disguised
/// as `image/*` are rejected cleanly before caching or rendering.
fn has_valid_image_magic_bytes(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    // JPEG (FF D8 FF)
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return true;
    }
    // PNG (89 50 4E 47)
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return true;
    }
    // GIF (GIF8)
    if bytes.starts_with(b"GIF8") {
        return true;
    }
    // WebP (RIFF....WEBP)
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return true;
    }
    // AVIF / HEIF / ISO BMFF (....ftyp)
    if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
        return true;
    }
    // BMP (BM)
    if bytes.starts_with(b"BM") {
        return true;
    }
    // SVG / XML image
    let trimmed = bytes
        .iter()
        .copied()
        .skip_while(u8::is_ascii_whitespace)
        .take(5)
        .collect::<Vec<u8>>();
    trimmed.eq_ignore_ascii_case(b"<svg ") || trimmed.eq_ignore_ascii_case(b"<svg>")
}

/// Detects whether an HTTP response looks like an anti-bot verification challenge page.
pub fn looks_like_challenge(status: StatusCode, html: &str) -> bool {
    if status != StatusCode::FORBIDDEN && status != StatusCode::SERVICE_UNAVAILABLE {
        return false;
    }
    let lower = html.to_ascii_lowercase();
    lower.contains("cf-browser-verification")
        || lower.contains("cf_chl_opt")
        || lower.contains("just a moment...")
        || lower.contains("ddos-guard")
        || lower.contains("verify you are human")
}

fn is_acceptable_image_mime(content_type: &str) -> bool {
    let lower = content_type.to_ascii_lowercase();
    if lower.starts_with("text/")
        || lower.contains("html")
        || lower.contains("json")
        || lower.contains("javascript")
    {
        return false;
    }
    lower.starts_with("image/")
        || lower == "application/octet-stream"
        || lower == "binary/octet-stream"
}

fn guess_mime_from_url(url: &Url) -> &'static str {
    let ext = std::path::Path::new(url.path())
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("");
    if ext.eq_ignore_ascii_case("png") {
        "image/png"
    } else if ext.eq_ignore_ascii_case("webp") {
        "image/webp"
    } else if ext.eq_ignore_ascii_case("avif") {
        "image/avif"
    } else if ext.eq_ignore_ascii_case("gif") {
        "image/gif"
    } else {
        "image/jpeg"
    }
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<HttpClient>();
};

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    #[test]
    fn normalize_adds_https_and_strips_tracking() {
        let got = normalize_url("  example.com/series/demo?utm_source=share&page=2#top ")
            .expect("valid url");
        assert_eq!(
            got.as_str(),
            "https://example.com/series/demo?page=2",
            "should normalize scheme, fragment, and tracking params"
        );
    }

    #[test]
    fn rejects_loopback_and_private_ips() {
        assert!(normalize_url("http://127.0.0.1:8080/a").is_err());
        assert!(normalize_url("http://localhost/a").is_err());
        assert!(normalize_url("http://192.168.1.5/a").is_err());
        assert!(normalize_url("http://10.0.0.1/a").is_err());
        assert!(normalize_url("http://100.64.0.1/a").is_err());
        assert!(normalize_url("http://0.1.2.3/a").is_err());
        assert!(normalize_url("http://[::1]/a").is_err());
        assert!(normalize_url("http://[fe80::1]/a").is_err());
        assert!(normalize_url("http://[::ffff:127.0.0.1]/a").is_err());
        assert!(normalize_url("http://[::ffff:169.254.169.254]/latest").is_err());
    }

    #[test]
    fn rejects_non_image_mime_types() {
        assert!(is_acceptable_image_mime("image/jpeg"));
        assert!(is_acceptable_image_mime("image/webp"));
        assert!(is_acceptable_image_mime("application/octet-stream"));
        assert!(!is_acceptable_image_mime("text/html"));
        assert!(!is_acceptable_image_mime("application/xhtml+xml"));
        assert!(!is_acceptable_image_mime("application/json"));
    }

    #[test]
    fn resolves_per_domain_proxy_rules_and_socks5h_remote_dns() {
        let default_ep = ProxyEndpoint {
            protocol: ProxyProtocol::Socks5,
            host: "proxy.example.net".to_owned(),
            port: 1080,
            username: Some("alice".to_owned()),
            password: Some("s3cr3t!".to_owned()),
        };
        assert_eq!(
            default_ep.sanitized_url().unwrap(),
            "socks5h://proxy.example.net:1080"
        );
        let dbg = format!("{default_ep:?}");
        assert!(!dbg.contains("s3cr3t!"), "Debug must never expose password");
        assert!(!dbg.contains("alice"), "Debug must never expose username");

        let custom_ep = ProxyEndpoint {
            protocol: ProxyProtocol::Http,
            host: "eu.proxy.example.net".to_owned(),
            port: 8080,
            username: None,
            password: None,
        };

        let cfg = ProxyConfig {
            enabled: false,
            default_proxy: default_ep,
            allow_direct_fallback: false,
            domain_rules: vec![
                DomainProxyRule {
                    domain: "blocked.example.org".to_owned(),
                    action: DomainRuleAction::ForceProxy,
                    custom_proxy: None,
                },
                DomainProxyRule {
                    domain: "cdn.blocked.example.org".to_owned(),
                    action: DomainRuleAction::CustomProxy,
                    custom_proxy: Some(custom_ep),
                },
                DomainProxyRule {
                    domain: "direct.example.org".to_owned(),
                    action: DomainRuleAction::ForceDirect,
                    custom_proxy: None,
                },
            ],
        }
        .sanitized();

        assert!(
            cfg.resolve_for_url(&Url::parse("https://other.example.com/a").unwrap())
                .is_none(),
            "Direct Mode must stay direct for unlisted domains"
        );
        assert_eq!(
            cfg.resolve_for_url(&Url::parse("https://sub.blocked.example.org/ch-1").unwrap())
                .map(|(ep, fb)| (ep.port, fb)),
            Some((1080, false))
        );
        assert_eq!(
            cfg.resolve_for_url(&Url::parse("https://cdn.blocked.example.org/1.jpg").unwrap())
                .map(|(ep, _)| ep.port),
            Some(8080),
            "More specific subdomain rule must win over parent domain rule"
        );
        assert!(
            cfg.resolve_for_url(&Url::parse("https://direct.example.org/ch-1").unwrap())
                .is_none()
        );
    }

    #[tokio::test]
    async fn proxy_mode_fails_closed_and_redacts_secrets_on_auth_failure() {
        // Local mock HTTP proxy that always returns 407 Proxy Authentication Required.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0_u8; 1024];
                let _ = sock.read(&mut buf).await;
                let _ = sock
                    .write_all(
                        b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
            }
        });

        let ep = ProxyEndpoint {
            protocol: ProxyProtocol::Http,
            host: "127.0.0.1".to_owned(),
            port: addr.port(),
            username: Some("myuser".to_owned()),
            password: Some("supersecretpass".to_owned()),
        };

        let client = HttpClient::with_policy(HttpPolicy {
            proxy: ProxyConfig {
                enabled: true,
                default_proxy: ep.clone(),
                allow_direct_fallback: false,
                domain_rules: Vec::new(),
            },
            ..HttpPolicy::default()
        })
        .unwrap();

        let test_res = client
            .test_proxy(&ep, Some("http://example.com"))
            .await
            .unwrap();
        assert!(!test_res.ok, "407 proxy auth error must report ok=false");
        assert_eq!(test_res.status_code, Some(407));
        let err_text = test_res.error.unwrap_or_default();
        assert!(!err_text.contains("supersecretpass"));
        assert!(!err_text.contains("myuser"));

        let _ = server.await;
    }

    #[tokio::test]
    async fn unconfigured_proxy_in_proxy_mode_fails_closed_without_leaking_direct() {
        let client = HttpClient::with_policy(HttpPolicy {
            proxy: ProxyConfig {
                enabled: true,
                default_proxy: ProxyEndpoint::default(),
                allow_direct_fallback: false,
                domain_rules: Vec::new(),
            },
            ..HttpPolicy::default()
        })
        .unwrap();

        let err = client
            .fetch_html(&Url::parse("https://example.com/series").unwrap())
            .await
            .expect_err("proxy mode with empty host must fail closed");
        assert!(matches!(err, ScrapeError::Proxy(_)));

        let headless_err = client
            .chromium_proxy_args_for_url(&Url::parse("https://example.com/series").unwrap())
            .expect_err("headless proxy args must also fail closed when proxy is unconfigured");
        assert!(matches!(headless_err, ScrapeError::Proxy(_)));
    }

    #[test]
    fn resolve_for_image_inherits_referer_rules_without_overriding_explicit_cdn_rules() {
        let default_ep = ProxyEndpoint {
            protocol: ProxyProtocol::Socks5,
            host: "global.proxy.net".to_owned(),
            port: 1080,
            username: None,
            password: None,
        };
        let cfg = ProxyConfig {
            enabled: true,
            default_proxy: default_ep,
            allow_direct_fallback: false,
            domain_rules: vec![
                DomainProxyRule {
                    domain: "fast-manga.org".to_owned(),
                    action: DomainRuleAction::ForceDirect,
                    custom_proxy: None,
                },
                DomainProxyRule {
                    domain: "proxied-cdn.net".to_owned(),
                    action: DomainRuleAction::ForceProxy,
                    custom_proxy: None,
                },
            ],
        }
        .sanitized();

        // CDN image with no explicit rule inherits `ForceDirect` from its referer `fast-manga.org` even when global proxy is ON!
        let cdn_img = Url::parse("https://img.unlisted-cdn.net/1.jpg").unwrap();
        let fast_ref = Url::parse("https://fast-manga.org/ch-1").unwrap();
        assert!(
            cfg.resolve_for_image(&cdn_img, Some(&fast_ref)).is_none(),
            "unlisted CDN image must inherit ForceDirect from referer"
        );

        // Image on `fast-manga.org` (which has ForceDirect) must NOT be overridden by a proxied referer!
        let direct_img = Url::parse("https://cdn.fast-manga.org/1.jpg").unwrap();
        let proxied_ref = Url::parse("https://other-site.org/ch-1").unwrap();
        assert!(
            cfg.resolve_for_image(&direct_img, Some(&proxied_ref))
                .is_none(),
            "explicit ForceDirect on image domain must not be overridden by referer"
        );

        // Explicit ForceProxy on image domain wins even when referer is ForceDirect.
        let forced_img = Url::parse("https://proxied-cdn.net/1.jpg").unwrap();
        assert!(
            cfg.resolve_for_image(&forced_img, Some(&fast_ref))
                .is_some(),
            "explicit ForceProxy on image domain must win over referer"
        );
    }

    #[tokio::test]
    async fn valid_http_proxy_handles_html_images_and_concurrency() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            // Serve 3 requests: 1 HTML page + 2 concurrent image fetches.
            for _ in 0..3 {
                if let Ok((mut sock, _)) = listener.accept().await {
                    tokio::spawn(async move {
                        let mut buf = [0_u8; 2048];
                        let n = sock.read(&mut buf).await.unwrap_or(0);
                        let req = String::from_utf8_lossy(&buf[..n]);
                        let resp: &[u8] = if req.contains("/img/") {
                            b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: 4\r\nConnection: close\r\n\r\n\x89PNG"
                        } else {
                            b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 19\r\nConnection: close\r\n\r\n<h1>Proxied OK</h1>"
                        };
                        let _ = sock.write_all(resp).await;
                    });
                }
            }
        });

        let ep = ProxyEndpoint {
            protocol: ProxyProtocol::Http,
            host: "127.0.0.1".to_owned(),
            port: addr.port(),
            username: None,
            password: None,
        };
        let client = HttpClient::with_policy(HttpPolicy {
            proxy: ProxyConfig {
                enabled: true,
                default_proxy: ep,
                allow_direct_fallback: false,
                domain_rules: Vec::new(),
            },
            ..HttpPolicy::default()
        })
        .unwrap();

        let page_url = Url::parse("http://proxy-test.example.com/chapter-1").unwrap();
        let img1_url = Url::parse("http://proxy-test.example.com/img/1.png").unwrap();
        let img2_url = Url::parse("http://proxy-test.example.com/img/2.png").unwrap();

        let (page, img1, img2) = tokio::join!(
            client.fetch_html(&page_url),
            client.fetch_image(&img1_url, Some(&page_url)),
            client.fetch_image(&img2_url, Some(&page_url)),
        );

        assert_eq!(page.unwrap().html, "<h1>Proxied OK</h1>");
        assert_eq!(img1.unwrap().content_type, "image/png");
        assert_eq!(img2.unwrap().bytes.len(), 4);

        server.abort();
    }

    #[tokio::test]
    async fn proxy_blocks_ssrf_redirects_to_loopback() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0_u8; 1024];
                let _ = sock.read(&mut buf).await;
                let _ = sock
                    .write_all(
                        b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9999/internal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                    .await;
            }
        });

        let ep = ProxyEndpoint {
            protocol: ProxyProtocol::Http,
            host: "127.0.0.1".to_owned(),
            port: addr.port(),
            username: None,
            password: None,
        };
        let client = HttpClient::with_policy(HttpPolicy {
            proxy: ProxyConfig {
                enabled: true,
                default_proxy: ep,
                allow_direct_fallback: false,
                domain_rules: Vec::new(),
            },
            ..HttpPolicy::default()
        })
        .unwrap();

        let err = client
            .fetch_html(&Url::parse("http://redirect-test.example.com/start").unwrap())
            .await
            .expect_err("redirect to 127.0.0.1 must be blocked as SSRF");
        assert!(matches!(
            err,
            ScrapeError::Proxy(_) | ScrapeError::Network(_)
        ));

        server.abort();
    }

    #[tokio::test]
    async fn proxy_midstream_drop_recovers_and_fails_closed() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            for _ in 0..=MAX_RETRIES {
                if let Ok((sock, _)) = listener.accept().await {
                    drop(sock);
                }
            }
        });

        let ep = ProxyEndpoint {
            protocol: ProxyProtocol::Http,
            host: "127.0.0.1".to_owned(),
            port: addr.port(),
            username: None,
            password: None,
        };
        let client = HttpClient::with_policy(HttpPolicy {
            proxy: ProxyConfig {
                enabled: true,
                default_proxy: ep,
                allow_direct_fallback: false,
                domain_rules: Vec::new(),
            },
            ..HttpPolicy::default()
        })
        .unwrap();

        let err = client
            .fetch_html(&Url::parse("http://drop-test.example.com/ch").unwrap())
            .await
            .expect_err("proxy drop with fallback disabled must fail closed");
        assert!(matches!(err, ScrapeError::Proxy(_)));

        server.abort();
    }
}
