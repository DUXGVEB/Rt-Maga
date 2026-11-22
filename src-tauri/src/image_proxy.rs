//! Signed `img://` custom protocol proxy that fetches images with proper `Referer` and caching.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use http::{Response, StatusCode};
use mangacore::PageImage;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::warn;
use url::Url;

use crate::cache::ImageCache;
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
}

/// Encodes a [`PageImage`] into an HMAC-signed token safe for the `img://` protocol.
pub(crate) fn encode_page_token(image: &PageImage, key: &[u8; 32]) -> String {
    let payload = TokenPayload {
        u: image.url.as_str().to_owned(),
        f: image
            .fallbacks
            .iter()
            .map(|u| u.as_str().to_owned())
            .collect(),
        r: image.referer.as_str().to_owned(),
    };
    sign_payload(&payload, key)
}

/// Encodes a cover image URL (with its manga page URL as referer) into a signed proxy token.
pub(crate) fn encode_cover_token(cover_url: &str, referer_url: &str, key: &[u8; 32]) -> String {
    let payload = TokenPayload {
        u: cover_url.to_owned(),
        f: Vec::new(),
        r: referer_url.to_owned(),
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

    let cache_key = ImageCache::key_for_url(&payload.u);

    if !bypass_cache && let Some(hit) = state.images.get(&cache_key).await {
        return image_ok_response(hit.bytes.to_vec(), &hit.content_type);
    }

    // Deduplicate concurrent downloads of the same image.
    let _flight_guard = if bypass_cache {
        None
    } else {
        let guard = state.images.begin_single_flight(&cache_key).await;
        if guard.is_none()
            && let Some(hit) = state.images.get(&cache_key).await
        {
            return image_ok_response(hit.bytes.to_vec(), &hit.content_type);
        }
        guard
    };

    let referer = Url::parse(&payload.r).ok();
    let max_mb = state.current_settings().await.cache.max_mb;

    let mut urls_to_try = Vec::with_capacity(1 + payload.f.len());
    urls_to_try.push(payload.u.as_str());
    for fb in &payload.f {
        urls_to_try.push(fb.as_str());
    }

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
                    .put(&cache_key, &fetched.bytes, &fetched.content_type, max_mb)
                    .await;
                return image_ok_response(fetched.bytes.to_vec(), &fetched.content_type);
            }
            Ok(_) => {
                warn!(%candidate_url, "image response was empty");
            }
            Err(err) => {
                warn!(%candidate_url, %err, "failed to fetch image candidate");
            }
        }
    }

    error_response(StatusCode::BAD_GATEWAY, "failed to fetch image")
}

fn image_ok_response(body: Vec<u8>, content_type: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type)
        .header("Cache-Control", "public, max-age=86400")
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .expect("valid static response headers")
}

fn error_response(status: StatusCode, msg: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
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

        let token = encode_page_token(&img, &key);
        let decoded = decode_and_verify(&token, &key).expect("valid token must decode");
        assert_eq!(decoded.u, img.url.as_str());
        assert_eq!(decoded.f.len(), 1);
        assert_eq!(decoded.r, img.referer.as_str());

        let tampered = format!("{token}x");
        assert!(decode_and_verify(&tampered, &key).is_none());
        assert!(decode_and_verify(&token, &[7_u8; 32]).is_none());
    }
}
