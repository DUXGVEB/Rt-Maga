//! Polite HTTP client with cookie jar, per-host concurrency limits, retries, and SSRF checks.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use dashmap::DashMap;
use reqwest::cookie::Jar;
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, CONTENT_TYPE, HeaderMap, HeaderValue, REFERER};
use reqwest::{Client, StatusCode};
use tokio::sync::Semaphore;
use tracing::{debug, warn};
use url::Url;

use crate::error::ScrapeError;

/// Realistic desktop browser User-Agent used for page and image requests.
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36";

/// Maximum request timeout for page and image downloads.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

/// Maximum number of retry attempts on transient 5xx, 429, or transport errors.
const MAX_RETRIES: u8 = 2;

/// Default maximum concurrent requests allowed per remote host.
const DEFAULT_MAX_CONN_PER_HOST: usize = 4;

/// Maximum HTML document payload buffered in memory (16 MiB).
const MAX_HTML_BYTES: usize = 16 * 1024 * 1024;

/// Maximum single image payload buffered in memory (32 MiB).
const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

/// Configuration for [`HttpClient`].
#[derive(Debug, Clone)]
pub struct HttpPolicy {
    /// User-Agent header sent with every request.
    pub user_agent: String,
    /// Per-request timeout.
    pub timeout: Duration,
    /// Maximum concurrent requests to any single host.
    pub max_conn_per_host: usize,
}

impl Default for HttpPolicy {
    fn default() -> Self {
        Self {
            user_agent: DEFAULT_USER_AGENT.to_owned(),
            timeout: DEFAULT_TIMEOUT,
            max_conn_per_host: DEFAULT_MAX_CONN_PER_HOST,
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

/// Shared HTTP client with cookie jar and per-host concurrency limiting.
#[derive(Debug, Clone)]
pub struct HttpClient {
    inner: Arc<HttpClientInner>,
}

#[derive(Debug)]
struct HttpClientInner {
    client: Client,
    jar: Arc<Jar>,
    host_semaphores: DashMap<String, Arc<Semaphore>>,
    max_conn_per_host: usize,
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
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT_LANGUAGE,
            HeaderValue::from_static("vi-VN,vi;q=0.9,en-US;q=0.8,en;q=0.7"),
        );

        let client = Client::builder()
            .user_agent(policy.user_agent)
            .default_headers(headers)
            .cookie_provider(Arc::clone(&jar))
            .timeout(policy.timeout)
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 10 {
                    return attempt.error("too many redirects");
                }
                if let Err(err) = validate_public_http_url(attempt.url()) {
                    return attempt.error(err);
                }
                attempt.follow()
            }))
            .build()?;

        Ok(Self {
            inner: Arc::new(HttpClientInner {
                client,
                jar,
                host_semaphores: DashMap::new(),
                max_conn_per_host: policy.max_conn_per_host.max(1),
            }),
        })
    }

    /// Injects a `Set-Cookie` style header value for `url` into the shared cookie jar.
    pub fn add_cookie(&self, cookie_header: &str, url: &Url) {
        self.inner.jar.add_cookie_str(cookie_header, url);
    }

    /// Fetches an HTML page while enforcing per-host concurrency limits and anti-bot detection.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::DisallowedHost`] if the URL targets a local/private address,
    /// [`ScrapeError::Blocked`] if the response appears to be a bot challenge,
    /// [`ScrapeError::HttpStatus`] on non-2xx responses, or [`ScrapeError::Network`] on I/O errors.
    pub async fn fetch_html(&self, url: &Url) -> Result<FetchedPage, ScrapeError> {
        validate_public_http_url(url)?;
        let _permit = self.acquire_host_permit(url).await;

        let mut last_err: Option<ScrapeError> = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let backoff = Duration::from_millis(400 * u64::from(attempt));
                tokio::time::sleep(backoff).await;
            }

            let resp = self
                .inner
                .client
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
                    let final_url = response.url().clone();
                    let raw_bytes =
                        read_bounded_bytes(response, MAX_HTML_BYTES, &final_url).await?;
                    let body = String::from_utf8_lossy(&raw_bytes).into_owned();

                    if looks_like_challenge(status, &body) {
                        let host = final_url.host_str().unwrap_or("unknown").to_owned();
                        return Err(ScrapeError::Blocked {
                            host,
                            url: final_url,
                        });
                    }

                    if status.is_success() {
                        debug!(%final_url, len = body.len(), "fetched html page");
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
                    if (err.is_timeout() || err.is_connect()) && attempt < MAX_RETRIES {
                        warn!(%err, attempt, "transient network error, retrying");
                        last_err = Some(ScrapeError::Network(err));
                        continue;
                    }
                    return Err(ScrapeError::Network(err));
                }
            }
        }

        Err(last_err.unwrap_or_else(|| ScrapeError::HttpStatus {
            status: 500,
            url: url.clone(),
        }))
    }

    /// Downloads raw image bytes with an explicit `Referer` header.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::DisallowedHost`], [`ScrapeError::HttpStatus`], or
    /// [`ScrapeError::Network`] if the image cannot be fetched.
    pub async fn fetch_image(
        &self,
        url: &Url,
        referer: Option<&Url>,
    ) -> Result<FetchedBytes, ScrapeError> {
        validate_public_http_url(url)?;
        let _permit = self.acquire_host_permit(url).await;

        let mut last_err: Option<ScrapeError> = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let backoff = Duration::from_millis(300 * u64::from(attempt));
                tokio::time::sleep(backoff).await;
            }

            let mut req = self
                .inner
                .client
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
                            return Err(ScrapeError::HttpStatus {
                                status: StatusCode::UNSUPPORTED_MEDIA_TYPE.as_u16(),
                                url: url.clone(),
                            });
                        }
                        let bytes = read_bounded_bytes(response, MAX_IMAGE_BYTES, url).await?;
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
                    if (err.is_timeout() || err.is_connect()) && attempt < MAX_RETRIES {
                        last_err = Some(ScrapeError::Network(err));
                        continue;
                    }
                    return Err(ScrapeError::Network(err));
                }
            }
        }

        Err(last_err.unwrap_or_else(|| ScrapeError::HttpStatus {
            status: 500,
            url: url.clone(),
        }))
    }

    async fn acquire_host_permit(&self, url: &Url) -> tokio::sync::OwnedSemaphorePermit {
        let host = url.host_str().unwrap_or("default").to_ascii_lowercase();
        let sem = Arc::clone(
            self.inner
                .host_semaphores
                .entry(host)
                .or_insert_with(|| Arc::new(Semaphore::new(self.inner.max_conn_per_host)))
                .value(),
        );
        sem.acquire_owned()
            .await
            .expect("host semaphore is never closed")
    }
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
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        let disallowed = match ip {
            IpAddr::V4(v4) => is_disallowed_ipv4(v4),
            IpAddr::V6(v6) => {
                let s0 = v6.segments()[0];
                v6.is_loopback()
                    || v6.is_unspecified()
                    || (s0 & 0xfe00) == 0xfc00
                    || (s0 & 0xffc0) == 0xfe80
                    || v6.to_ipv4_mapped().is_some_and(is_disallowed_ipv4)
            }
        };
        if disallowed {
            return Err(ScrapeError::DisallowedHost(host.to_owned()));
        }
    }
    Ok(())
}

fn is_disallowed_ipv4(v4: std::net::Ipv4Addr) -> bool {
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
        buf.extend_from_slice(&chunk);
    }

    Ok(Bytes::from(buf))
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
}
