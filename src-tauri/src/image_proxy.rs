//! Signed `img://` custom protocol proxy that fetches images with proper `Referer`,
//! scoped source/manga cache ownership, and structured diagnostic logging.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use http::{Response, StatusCode};
use mangacore::{PageImage, ScrapeError};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::warn;
use url::Url;

use crate::cache::ImageCache;
use crate::cache::image::CacheEntryMeta;
use crate::repo::diagnostics::NewDiagnosticLog;
use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Serialize, Deserialize)]
struct TokenPayload {
    /// Primary image URL.
    u: String,
    /// Fallback mirror URLs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    f: Vec<String>,
    /// Required `Referer` header URL.
    r: String,
    /// Optional owning manga ID for scoped cache management.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    m: Option<i64>,
    /// Optional owning source site hostname for scoped cache management.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    s: Option<String>,
    /// `true` for persistent cover thumbnails, `false` for transient chapter page images.
    #[serde(default)]
    c: bool,
    /// Optional owning chapter URL for post-chapter cache reclamation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ch: Option<String>,
}

/// Encodes a [`PageImage`] into an HMAC-signed token safe for the `img://` protocol.
pub(crate) fn encode_page_token(
    image: &PageImage,
    manga_id: Option<i64>,
    site: Option<&str>,
    chapter_url: Option<&str>,
    key: &[u8; 32],
) -> String {
    let derived_site = site.map(str::to_owned).or_else(|| {
        image
            .referer
            .host_str()
            .map(|h| h.trim_start_matches("www.").to_ascii_lowercase())
    });
    let payload = TokenPayload {
        u: image.url.as_str().to_owned(),
        f: image
            .fallbacks
            .iter()
            .map(|u| u.as_str().to_owned())
            .collect(),
        r: image.referer.as_str().to_owned(),
        m: manga_id,
        s: derived_site,
        c: false,
        ch: chapter_url.map(str::to_owned),
    };
    sign_payload(&payload, key)
}

/// Encodes a cover image URL (with its manga page URL as referer) into a signed proxy token.
pub(crate) fn encode_cover_token(
    cover_url: &str,
    referer_url: &str,
    manga_id: Option<i64>,
    site: Option<&str>,
    key: &[u8; 32],
) -> String {
    let derived_site = site.map(str::to_owned).or_else(|| {
        Url::parse(referer_url).ok().and_then(|u| {
            u.host_str()
                .map(|h| h.trim_start_matches("www.").to_ascii_lowercase())
        })
    });
    let payload = TokenPayload {
        u: cover_url.to_owned(),
        f: Vec::new(),
        r: referer_url.to_owned(),
        m: manga_id,
        s: derived_site,
        c: true,
        ch: None,
    };
    sign_payload(&payload, key)
}

fn sign_payload(payload: &TokenPayload, key: &[u8; 32]) -> String {
    let json = serde_json::to_vec(payload).expect("TokenPayload serialization cannot fail");
    let encoded_body = URL_SAFE_NO_PAD.encode(&json);

    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts 32-byte keys");
    mac.update(encoded_body.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

    format!("{encoded_body}.{sig}")
}

fn decode_and_verify(token: &str, key: &[u8; 32]) -> Option<TokenPayload> {
    let (body_b64, sig_b64) = token.split_once('.')?;
    let sig_bytes = URL_SAFE_NO_PAD.decode(sig_b64).ok()?;

    let mut mac = HmacSha256::new_from_slice(key).ok()?;
    mac.update(body_b64.as_bytes());
    mac.verify_slice(&sig_bytes).ok()?;

    let json_bytes = URL_SAFE_NO_PAD.decode(body_b64).ok()?;
    serde_json::from_slice(&json_bytes).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponseCachePolicy {
    Cover,
    NoStorePage,
}

/// Handles an incoming `img://` (or `http://img.localhost/`) protocol request asynchronously.
pub(crate) async fn handle_protocol_request(state: &AppState, uri: &str) -> Response<Vec<u8>> {
    let Ok(parsed_uri) = Url::parse(uri) else {
        return error_response(StatusCode::BAD_REQUEST, "invalid proxy uri");
    };

    let token = parsed_uri.path().trim_start_matches('/');
    let bypass_cache = parsed_uri
        .query_pairs()
        .any(|(k, v)| k == "retry" && v == "1");

    let Some(payload) = decode_and_verify(token, &state.proxy_key) else {
        return error_response(StatusCode::FORBIDDEN, "invalid image token");
    };

    let policy = if payload.c {
        ResponseCachePolicy::Cover
    } else {
        ResponseCachePolicy::NoStorePage
    };
    let cache_key = ImageCache::key_for_url(&payload.u);
    let meta = CacheEntryMeta {
        site: payload.s.as_deref(),
        manga_id: payload.m,
        is_cover: payload.c,
        chapter_url: payload.ch.as_deref(),
    };

    if !bypass_cache && let Some(hit) = state.images.get(&cache_key, meta.clone()).await {
        return image_ok_response(hit.bytes.to_vec(), &hit.content_type, policy);
    }

    let _flight_guard = if bypass_cache {
        None
    } else {
        let guard = state.images.begin_single_flight(&cache_key).await;
        if guard.is_none()
            && let Some(hit) = state.images.get(&cache_key, meta.clone()).await
        {
            return image_ok_response(hit.bytes.to_vec(), &hit.content_type, policy);
        }
        guard
    };

    fetch_and_cache_image(state, &payload, &cache_key, meta, policy).await
}

async fn fetch_and_cache_image(
    state: &AppState,
    payload: &TokenPayload,
    cache_key: &str,
    meta: CacheEntryMeta<'_>,
    policy: ResponseCachePolicy,
) -> Response<Vec<u8>> {
    let referer = Url::parse(&payload.r).ok();
    let max_mb = state.current_settings().await.cache.max_mb;

    let mut urls_to_try = Vec::with_capacity(1 + payload.f.len());
    urls_to_try.push(payload.u.as_str());
    for fb in &payload.f {
        urls_to_try.push(fb.as_str());
    }

    let mut last_status = StatusCode::BAD_GATEWAY;
    let mut last_category = "network";
    let mut last_detail = "failed to fetch image".to_owned();

    for candidate_str in urls_to_try {
        let Ok(candidate_url) = Url::parse(candidate_str) else {
            continue;
        };
        match state
            .registry
            .http()
            .fetch_image(&candidate_url, referer.as_ref())
            .await
        {
            Ok(fetched) if !fetched.bytes.is_empty() => {
                state
                    .images
                    .put(
                        cache_key,
                        &fetched.bytes,
                        &fetched.content_type,
                        max_mb,
                        meta.clone(),
                    )
                    .await;
                return image_ok_response(fetched.bytes.to_vec(), &fetched.content_type, policy);
            }
            Ok(_) => {
                warn!(%candidate_url, "image response was empty");
                last_status = StatusCode::UNPROCESSABLE_ENTITY;
                last_category = "corrupted_data";
                "upstream returned an empty 0-byte image".clone_into(&mut last_detail);
            }
            Err(err) => {
                warn!(%candidate_url, %err, "failed to fetch image candidate");
                let (st, cat) = classify_image_fetch_err(&err);
                last_status = st;
                last_category = cat;
                last_detail = err.to_string();
            }
        }
    }

    let _ = state
        .db
        .record_diagnostic_log(&NewDiagnosticLog {
            level: "warn".to_owned(),
            category: last_category.to_owned(),
            message: "Image failed to load after fallback attempts".to_owned(),
            detail: Some(last_detail.clone()),
            source: payload.s.clone(),
            chapter_url: Some(payload.r.clone()),
            failed_url: Some(payload.u.clone()),
            http_status: Some(i64::from(last_status.as_u16())),
            parsing_stage: Some("image_proxy_fetch".to_owned()),
            ..NewDiagnosticLog::default()
        })
        .await;

    error_response(last_status, &last_detail)
}

fn classify_image_fetch_err(err: &ScrapeError) -> (StatusCode, &'static str) {
    match err {
        ScrapeError::Timeout { .. } => (StatusCode::REQUEST_TIMEOUT, "timeout"),
        ScrapeError::RateLimited { .. } => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
        ScrapeError::InvalidImageFormat { .. } => {
            (StatusCode::UNSUPPORTED_MEDIA_TYPE, "invalid_image_format")
        }
        ScrapeError::CorruptedData { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "corrupted_data"),
        ScrapeError::Classified(detail) => {
            let status = detail
                .status
                .and_then(|code| StatusCode::from_u16(code).ok())
                .unwrap_or(match detail.code {
                    mangacore::SourceErrorCode::ImageSourceChanged => StatusCode::GONE,
                    mangacore::SourceErrorCode::ImageDecodeFailed => {
                        StatusCode::UNPROCESSABLE_ENTITY
                    }
                    _ => StatusCode::BAD_GATEWAY,
                });
            let cat = match detail.code {
                mangacore::SourceErrorCode::ImageSourceChanged => "image_source_changed",
                mangacore::SourceErrorCode::ImageDecodeFailed => "image_decode_failed",
                mangacore::SourceErrorCode::ImageRequestFailed => "image_request_failed",
                _ => "network",
            };
            (status, cat)
        }
        ScrapeError::HttpStatus { status, .. } => (
            StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY),
            "http_error",
        ),
        _ => (StatusCode::BAD_GATEWAY, "network"),
    }
}

fn image_ok_response(
    body: Vec<u8>,
    content_type: &str,
    policy: ResponseCachePolicy,
) -> Response<Vec<u8>> {
    let safe_ct = http::HeaderValue::from_str(content_type)
        .unwrap_or_else(|_| http::HeaderValue::from_static("application/octet-stream"));
    // Chapter pages return `no-store` so Android WebView / WebView2 never duplicate large chapter
    // images into their own opaque HTTP disk cache; small covers remain cacheable for instant lists.
    let cache_control = match policy {
        ResponseCachePolicy::Cover => "private, max-age=86400",
        ResponseCachePolicy::NoStorePage => "no-store",
    };
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", safe_ct)
        .header("Cache-Control", cache_control)
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .expect("valid static response headers")
}

fn error_response(status: StatusCode, msg: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .header("Cache-Control", "no-store")
        .body(msg.as_bytes().to_vec())
        .expect("valid static error response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_token_round_trips_and_rejects_tampering() {
        let key = [42_u8; 32];
        let img = PageImage {
            url: Url::parse("https://cdn.example.com/16695/44/0.jpg").unwrap(),
            fallbacks: vec![Url::parse("https://cdn2.example.com/16695/44/0.jpg?cdn=1").unwrap()],
            referer: Url::parse("https://example.com/series/demo-chap-44").unwrap(),
        };

        let token = encode_page_token(
            &img,
            Some(7),
            Some("example.com"),
            Some("https://example.com/series/demo-chap-44"),
            &key,
        );
        let decoded = decode_and_verify(&token, &key).expect("valid token must decode");
        assert_eq!(decoded.u, img.url.as_str());
        assert_eq!(decoded.f.len(), 1);
        assert_eq!(decoded.r, img.referer.as_str());
        assert_eq!(decoded.m, Some(7));
        assert_eq!(decoded.s.as_deref(), Some("example.com"));
        assert!(!decoded.c, "page tokens must not be marked as cover");
        assert_eq!(
            decoded.ch.as_deref(),
            Some("https://example.com/series/demo-chap-44"),
            "page tokens must carry chapter url for post-chapter cache cleanup"
        );

        let cover_token = encode_cover_token(
            "https://cdn.example.com/cover.jpg",
            "https://example.com/series/demo",
            Some(7),
            Some("example.com"),
            &key,
        );
        let cover_decoded = decode_and_verify(&cover_token, &key).expect("cover token decodes");
        assert!(cover_decoded.c, "cover tokens must be marked as cover");

        let tampered = format!("{token}x");
        assert!(decode_and_verify(&tampered, &key).is_none());
        assert!(decode_and_verify(&token, &[7_u8; 32]).is_none());
    }
}
