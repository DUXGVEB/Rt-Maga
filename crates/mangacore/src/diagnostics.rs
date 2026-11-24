//! Centralized source failure taxonomy, secret-redacting diagnostic reports, and recovery guidance.

use serde::{Deserialize, Serialize};
use url::Url;

use crate::recognize::{UrlKind, now_unix_secs};

/// Exhaustive, machine-readable error codes for comic source identification, loading,
/// authentication, format decoding, and network/adapter failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum SourceErrorCode {
    /// Submitted URL is syntactically malformed or points to a private/disallowed address.
    InvalidUrl,
    /// Website or resource type is outside supported comic sources.
    UnsupportedWebsite,
    /// Target page loaded cleanly, but represents non-comic content (blog, news, store, etc.).
    NotAComicPage,
    /// Website loaded, but automatic source/page detection lacked sufficient structural signals.
    SourceDetectionFailed,
    /// Target chapter could not be found (HTTP 404 or removed chapter reference).
    ChapterNotFound,
    /// Page was identified as a chapter, yet readable page content failed to load into the DOM.
    ChapterContentNotLoaded,
    /// Initial HTML is an SPA/JavaScript shell that requires browser rendering to load images.
    JavascriptRenderRequired,
    /// JavaScript rendering is required, but no local browser engine is available on this device.
    BrowserEngineUnavailable,
    /// Content requires signing in with an ordinary user account.
    LoginRequired,
    /// Existing session cookies expired or were rejected by the website (HTTP 401/login redirect).
    SessionExpired,
    /// Website presented an interactive human verification or anti-bot gate.
    VerificationRequired,
    /// Server denied access to this resource (HTTP 403 without an interactive challenge).
    AccessDenied,
    /// Content is restricted in the user's current network region (HTTP 451 or explicit geo notice).
    GeoRestriction,
    /// Chapter requires an active subscription, VIP tier, coin purchase, or rental unlock.
    PaywallOrSubscriptionRequired,
    /// Website uses DRM or browser-only display protection (e.g. canvas taint / IMGX).
    DrmProtectedContent,
    /// Website policy or robots/extraction restriction forbids automated native extraction.
    ExtractionNotPermitted,
    /// Website API returned a non-JSON / binary / protobuf / unrecognized schema payload.
    UnsupportedApiFormat,
    /// Image endpoint returned an unsupported media type or non-image payload (e.g. HTML/SVG).
    UnsupportedImageFormat,
    /// Downloaded image payload is empty, truncated, corrupted, or fails dimension checks.
    ImageDecodeFailed,
    /// Downloading an individual chapter image failed due to transport or upstream errors.
    ImageRequestFailed,
    /// Image CDN link expired, moved, or rejected hotlinking (HTTP 403/410 on image request).
    ImageSourceChanged,
    /// Remote website rate-limited requests (HTTP 429).
    RateLimited,
    /// Upstream network request timed out before completion.
    NetworkTimeout,
    /// Upstream website returned a 5xx server error (HTTP 500..=599).
    ServerError,
    /// Site-specific adapter failed to match the current website layout and may need updating.
    AdapterOutdated,
    /// Site-specific adapter encountered an internal execution failure or panic (contained safely).
    AdapterExecutionFailed,
    /// Unclassified source failure where exact root cause could not be determined.
    UnknownSourceError,
}

impl SourceErrorCode {
    /// Returns every stable [`SourceErrorCode`] variant defined in the taxonomy.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::InvalidUrl,
            Self::UnsupportedWebsite,
            Self::NotAComicPage,
            Self::SourceDetectionFailed,
            Self::ChapterNotFound,
            Self::ChapterContentNotLoaded,
            Self::JavascriptRenderRequired,
            Self::BrowserEngineUnavailable,
            Self::LoginRequired,
            Self::SessionExpired,
            Self::VerificationRequired,
            Self::AccessDenied,
            Self::GeoRestriction,
            Self::PaywallOrSubscriptionRequired,
            Self::DrmProtectedContent,
            Self::ExtractionNotPermitted,
            Self::UnsupportedApiFormat,
            Self::UnsupportedImageFormat,
            Self::ImageDecodeFailed,
            Self::ImageRequestFailed,
            Self::ImageSourceChanged,
            Self::RateLimited,
            Self::NetworkTimeout,
            Self::ServerError,
            Self::AdapterOutdated,
            Self::AdapterExecutionFailed,
            Self::UnknownSourceError,
        ]
    }
}

/// High-level UI grouping distinguishing *where* and *why* resolution or loading failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FailureCategoryGroup {
    /// Failed to identify the website or recognize the page as a comic source.
    SourceDetection,
    /// Failed to locate or resolve the requested comic chapter.
    ChapterIdentification,
    /// Failed to load dynamic page content or browser runtime.
    ContentLoading,
    /// Failed while resolving, fetching, or validating chapter image payloads.
    ImageExtraction,
    /// Blocked by login, expired session, verification, paywall, geo-block, or DRM.
    AccessPermission,
    /// Server responded with an unsupported API schema or media format.
    UnsupportedFormat,
    /// Transport timeout, rate limit, DNS/TLS error, upstream 5xx, or adapter fault.
    Network,
}

/// Adaptive extraction strategy selected or attempted during resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ExtractionStrategyKind {
    /// Strategy D: Domain-matched dedicated website adapter.
    SiteAdapter,
    /// Strategy C: Structured embedded JSON (`__NEXT_DATA__`, JSON-LD, `ts_reader`) or public API.
    StructuredDataAndApi,
    /// Strategy A: Native HTML DOM, lazy-loading attributes, `<picture>`, and CSS `background-image`.
    NativeHtml,
    /// Strategy B: Browser-based JavaScript rendering runtime.
    BrowserRendering,
    /// Strategy E: User-authenticated browser session with synchronized session cookies.
    AuthenticatedBrowser,
}

/// Concrete recovery action presented to the user in failure diagnostics UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RecoveryActionKind {
    /// Retry the failed operation.
    Retry,
    /// Open in authorized browser-based reading mode / external browser.
    OpenBrowserReadingMode,
    /// Launch browser sign-in flow so the user can authenticate with their account.
    SignInViaBrowser,
    /// Launch interactive verification flow so the user can clear a human challenge.
    CompleteVerification,
    /// Open the original webpage directly in an external browser.
    OpenOriginalWebsite,
    /// Prompt the user to inspect In-App DNS (`DoH`) or Proxy settings.
    ConfigureNetworkOrProxy,
    /// Copy/export the sanitized diagnostic bundle.
    ExportDiagnostics,
}

/// Per-strategy attempt trace recorded in [`SourceDiagnosticReport`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyAttemptTrace {
    /// Strategy tier executed.
    pub strategy: ExtractionStrategyKind,
    /// Dedicated adapter identifier when `strategy` is [`ExtractionStrategyKind::SiteAdapter`].
    pub adapter_id: Option<String>,
    /// Concise, secret-free outcome string (e.g. `"succeeded"`, `"0_images"`, `"js_shell"`).
    pub outcome: String,
    /// Number of raw candidates discovered during this attempt.
    pub candidates_found: usize,
    /// Number of validated/accepted pages during this attempt.
    pub validated_count: usize,
    /// Elapsed wall-clock time for this strategy attempt in milliseconds.
    pub duration_ms: u64,
}

/// Localized explanation and recovery policy metadata for a [`SourceErrorCode`].
#[derive(Debug, Clone)]
pub struct ErrorCodeDescriptor {
    /// High-level UI failure category group.
    pub group: FailureCategoryGroup,
    /// Concise Vietnamese title for users.
    pub title_vi: &'static str,
    /// Clear, factual Vietnamese explanation.
    pub explanation_vi: &'static str,
    /// Concise English title for users.
    pub title_en: &'static str,
    /// Clear, factual English explanation.
    pub explanation_en: &'static str,
    /// Suggested next step in Vietnamese.
    pub suggested_action_vi: &'static str,
    /// Suggested next step in English.
    pub suggested_action_en: &'static str,
    /// Whether retrying directly may succeed without user intervention.
    pub retryable: bool,
    /// Whether manual user action (login, verification, browser reading) is required.
    pub user_interaction_required: bool,
    /// Ordered recovery actions supported for this failure.
    pub actions: Vec<RecoveryActionKind>,
}

/// Complete, sanitized diagnostic report attached to source resolution and chapter loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDiagnosticReport {
    /// Redacted originally submitted URL.
    pub original_url: String,
    /// Redacted final URL after HTTP or client redirects, if reached.
    pub final_url: Option<String>,
    /// Normalized host domain (without `www.`).
    pub domain: Option<String>,
    /// Detected semantic page kind.
    pub detected_page_type: UrlKind,
    /// Confidence score (`0.0..=1.0`) for source/page-type identification.
    pub detection_confidence: f32,
    /// Confidence score (`0.0..=1.0`) for extracted content quality.
    pub extraction_confidence: f32,
    /// Final selected extraction strategy when successful or primary when failed.
    pub selected_strategy: Option<ExtractionStrategyKind>,
    /// Chronological trace of all attempted extraction strategies.
    pub strategies_attempted: Vec<StrategyAttemptTrace>,
    /// Pipeline processing stage where failure occurred (or `"completed"`).
    pub failed_stage: Option<String>,
    /// Last received upstream HTTP status code, if any.
    pub http_status: Option<u16>,
    /// Upstream HTTP `Content-Type` header (media type only), if available.
    pub content_type: Option<String>,
    /// Upstream response body size in bytes, if measured.
    pub response_bytes: Option<usize>,
    /// Number of raw image URL candidates inspected during extraction.
    pub image_candidates_count: usize,
    /// Number of candidates validated as real comic page URLs/images.
    pub validated_images_count: usize,
    /// Name or ID of the active adapter, if one matched.
    pub adapter_name: Option<String>,
    /// Semantic version string of the active adapter, if one matched.
    pub adapter_version: Option<String>,
    /// Sanitized browser runtime / execution error messages observed during headless rendering.
    pub browser_errors: Vec<String>,
    /// Final stable 27-code classification when an operation failed.
    pub error_code: Option<SourceErrorCode>,
    /// High-level category grouping for the failure UI badge.
    pub failure_group: Option<FailureCategoryGroup>,
    /// User-friendly Vietnamese failure title.
    pub title_vi: Option<String>,
    /// User-friendly Vietnamese failure explanation.
    pub explanation_vi: Option<String>,
    /// User-friendly English failure title.
    pub title_en: Option<String>,
    /// User-friendly English failure explanation.
    pub explanation_en: Option<String>,
    /// Sanitized diagnostic evidence lines explaining what was inspected.
    pub evidence: Vec<String>,
    /// Whether retrying the operation directly is likely useful.
    pub retryable: bool,
    /// Whether manual user interaction (login, verification, paywall unlock) is required.
    pub user_interaction_required: bool,
    /// Whether opening the URL in an authorized browser reading mode is possible.
    pub browser_reading_available: bool,
    /// Actionable Vietnamese recovery guidance.
    pub suggested_action_vi: Option<String>,
    /// Actionable English recovery guidance.
    pub suggested_action_en: Option<String>,
    /// Ordered recovery actions recommended to the UI.
    pub recovery_actions: Vec<RecoveryActionKind>,
    /// Unix timestamp in seconds when this report was finalized.
    pub timestamp_unix: i64,
}

impl SourceDiagnosticReport {
    /// Initializes an empty report for `raw_url` with URL credentials/query secrets redacted.
    #[must_use]
    pub fn start(raw_url: &str, detected_page_type: UrlKind, domain: Option<String>) -> Self {
        Self {
            original_url: redact_url_for_diagnostics(raw_url.trim()),
            final_url: None,
            domain,
            detected_page_type,
            detection_confidence: 0.0,
            extraction_confidence: 0.0,
            selected_strategy: None,
            strategies_attempted: Vec::new(),
            failed_stage: None,
            http_status: None,
            content_type: None,
            response_bytes: None,
            image_candidates_count: 0,
            validated_images_count: 0,
            adapter_name: None,
            adapter_version: None,
            browser_errors: Vec::new(),
            error_code: None,
            failure_group: None,
            title_vi: None,
            explanation_vi: None,
            title_en: None,
            explanation_en: None,
            evidence: Vec::new(),
            retryable: false,
            user_interaction_required: false,
            browser_reading_available: false,
            suggested_action_vi: None,
            suggested_action_en: None,
            recovery_actions: Vec::new(),
            timestamp_unix: now_unix_secs(),
        }
    }

    /// Populates the localized failure descriptor and recovery capabilities for `code`.
    pub fn apply_failure(
        &mut self,
        code: SourceErrorCode,
        stage: &str,
        can_open_browser: BrowserFallbackAvailability,
    ) {
        let desc = describe_error_code(code);
        self.error_code = Some(code);
        self.failure_group = Some(desc.group);
        self.failed_stage = Some(stage.to_owned());
        self.title_vi = Some(desc.title_vi.to_owned());
        self.explanation_vi = Some(desc.explanation_vi.to_owned());
        self.title_en = Some(desc.title_en.to_owned());
        self.explanation_en = Some(desc.explanation_en.to_owned());
        self.suggested_action_vi = Some(desc.suggested_action_vi.to_owned());
        self.suggested_action_en = Some(desc.suggested_action_en.to_owned());
        self.retryable = desc.retryable;
        self.user_interaction_required = desc.user_interaction_required;
        self.browser_reading_available =
            matches!(can_open_browser, BrowserFallbackAvailability::Available)
                && code != SourceErrorCode::InvalidUrl;
        self.recovery_actions = desc.actions;
        if !self.browser_reading_available {
            self.recovery_actions.retain(|a| {
                !matches!(
                    a,
                    RecoveryActionKind::OpenBrowserReadingMode
                        | RecoveryActionKind::OpenOriginalWebsite
                        | RecoveryActionKind::SignInViaBrowser
                        | RecoveryActionKind::CompleteVerification
                )
            });
        }
        self.timestamp_unix = now_unix_secs();
    }
}

/// Explicitly indicates whether an authorized public browser fallback URL is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BrowserFallbackAvailability {
    /// URL is a valid public HTTP/HTTPS address that can open in the system browser.
    Available,
    /// URL is invalid, non-public, or unavailable for browser fallback.
    Unavailable,
}

const SENSITIVE_QUERY_KEYS: &[&str] = &[
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "auth",
    "authorization",
    "key",
    "api_key",
    "apikey",
    "secret",
    "sig",
    "signature",
    "x-amz-signature",
    "x-amz-credential",
    "x-amz-security-token",
    "x-goog-signature",
    "x-goog-credential",
    "hdnts",
    "policy",
    "session",
    "sessionid",
    "session_id",
    "phpsessid",
    "cookie",
    "password",
    "passwd",
    "pwd",
    "jwt",
    "code",
    "ticket",
    "hash",
];

/// Redacts URL credentials (`user:pass@`) and sensitive query parameters (`token`, `sig`, `session`,
/// `password`, etc.) before logging or displaying URLs in diagnostic reports.
#[must_use]
pub fn redact_url_for_diagnostics(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let Ok(mut parsed) = Url::parse(trimmed) else {
        return redact_raw_text_fallback(trimmed);
    };
    if !parsed.username().is_empty() || parsed.password().is_some() {
        let _ = parsed.set_username("redacted");
        let _ = parsed.set_password(Some("redacted"));
    }
    let has_sensitive = parsed
        .query_pairs()
        .any(|(k, _)| is_sensitive_param_key(&k));
    if has_sensitive {
        let pairs: Vec<(String, String)> = parsed
            .query_pairs()
            .map(|(k, v)| {
                let key = k.into_owned();
                if is_sensitive_param_key(&key) {
                    (key, "[REDACTED]".to_owned())
                } else {
                    (key, v.into_owned())
                }
            })
            .collect();
        parsed.query_pairs_mut().clear().extend_pairs(pairs);
    }
    parsed.into()
}

fn is_sensitive_param_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_QUERY_KEYS.contains(&lower.as_str())
        || lower.ends_with("_token")
        || lower.ends_with("_secret")
        || lower.ends_with("_key")
        || lower.ends_with("_sig")
}

fn redact_raw_text_fallback(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(scheme_pos) = rest.find("://") {
        let after_scheme = scheme_pos + 3;
        out.push_str(&rest[..after_scheme]);
        let tail = &rest[after_scheme..];
        let delim = tail.find('/').unwrap_or(tail.len());
        let auth = &tail[..delim];
        if let Some(at_pos) = auth.rfind('@') {
            out.push_str("***@");
            out.push_str(&auth[at_pos + 1..]);
        } else {
            out.push_str(auth);
        }
        rest = &tail[delim..];
    }
    out.push_str(rest);
    out
}

/// Returns complete Vietnamese & English descriptions, failure category grouping, and recovery
/// actions for every [`SourceErrorCode`].
#[must_use]
pub fn describe_error_code(code: SourceErrorCode) -> ErrorCodeDescriptor {
    match code {
        SourceErrorCode::InvalidUrl
        | SourceErrorCode::UnsupportedWebsite
        | SourceErrorCode::NotAComicPage
        | SourceErrorCode::SourceDetectionFailed => describe_detection_code(code),
        SourceErrorCode::ChapterNotFound
        | SourceErrorCode::ChapterContentNotLoaded
        | SourceErrorCode::JavascriptRenderRequired
        | SourceErrorCode::BrowserEngineUnavailable => describe_loading_code(code),
        SourceErrorCode::LoginRequired
        | SourceErrorCode::SessionExpired
        | SourceErrorCode::VerificationRequired
        | SourceErrorCode::AccessDenied
        | SourceErrorCode::GeoRestriction
        | SourceErrorCode::PaywallOrSubscriptionRequired
        | SourceErrorCode::DrmProtectedContent
        | SourceErrorCode::ExtractionNotPermitted => describe_access_code(code),
        SourceErrorCode::UnsupportedApiFormat
        | SourceErrorCode::UnsupportedImageFormat
        | SourceErrorCode::ImageDecodeFailed
        | SourceErrorCode::ImageRequestFailed
        | SourceErrorCode::ImageSourceChanged => describe_media_and_format_code(code),
        SourceErrorCode::RateLimited
        | SourceErrorCode::NetworkTimeout
        | SourceErrorCode::ServerError
        | SourceErrorCode::AdapterOutdated
        | SourceErrorCode::AdapterExecutionFailed
        | SourceErrorCode::UnknownSourceError => describe_network_and_adapter_code(code),
    }
}

fn describe_detection_code(code: SourceErrorCode) -> ErrorCodeDescriptor {
    match code {
        SourceErrorCode::InvalidUrl => ErrorCodeDescriptor {
            group: FailureCategoryGroup::SourceDetection,
            title_vi: "Liên kết không hợp lệ",
            explanation_vi: "Đường dẫn bạn nhập không đúng định dạng URL công khai (http:// hoặc https://) hoặc trỏ tới địa chỉ nội bộ bị chặn.",
            title_en: "Invalid URL",
            explanation_en: "The submitted link is not a valid public http:// or https:// URL.",
            suggested_action_vi: "Kiểm tra lại đường dẫn đầy đủ bắt đầu bằng https:// rồi thử lại.",
            suggested_action_en: "Check the full URL starting with https:// and try again.",
            retryable: false,
            user_interaction_required: false,
            actions: vec![RecoveryActionKind::ExportDiagnostics],
        },
        SourceErrorCode::UnsupportedWebsite => ErrorCodeDescriptor {
            group: FailureCategoryGroup::SourceDetection,
            title_vi: "Trang web hoặc loại tài nguyên chưa được hỗ trợ",
            explanation_vi: "Gt-Maga không hỗ trợ mở trực tiếp loại liên kết này (ví dụ URL ảnh đơn lẻ hoặc trang web ngoài cấu trúc truyện tranh).",
            title_en: "Unsupported website or resource",
            explanation_en: "Gt-Maga cannot open this resource directly as a comic series, chapter, or catalog page.",
            suggested_action_vi: "Dán link trang thông tin truyện, trang đọc chương hoặc mở trên trình duyệt.",
            suggested_action_en: "Paste the series detail, chapter, or catalog URL instead, or open it in a browser.",
            retryable: false,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::OpenOriginalWebsite,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
        SourceErrorCode::NotAComicPage => ErrorCodeDescriptor {
            group: FailureCategoryGroup::SourceDetection,
            title_vi: "Trang này không phải trang truyện tranh",
            explanation_vi: "Trang web phản hồi bình thường nhưng nội dung là bài viết, tin tức, tài liệu hoặc trang web thông thường chứ không có chương hay danh sách truyện.",
            title_en: "Not a comic page",
            explanation_en: "The webpage loaded cleanly, but it contains general web content rather than comic chapters or series data.",
            suggested_action_vi: "Mở đúng trang chi tiết truyện hoặc trang đọc chương từ trình duyệt rồi dán lại link.",
            suggested_action_en: "Open the actual series or chapter page in your browser and paste that link.",
            retryable: false,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::OpenOriginalWebsite,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
        _ => ErrorCodeDescriptor {
            group: FailureCategoryGroup::SourceDetection,
            title_vi: "Không nhận diện được cấu trúc nguồn truyện",
            explanation_vi: "Đã tải được phản hồi từ trang web nhưng không tìm thấy đủ dấu hiệu của danh mục truyện, trang giới thiệu hoặc trình đọc chương.",
            title_en: "Source detection failed",
            explanation_en: "The webpage was inspected, but insufficient structural signals were found to identify a comic series, chapter, or catalog.",
            suggested_action_vi: "Thử mở trên trình duyệt hoặc kiểm tra chi tiết chẩn đoán để xem những gì đã được quét.",
            suggested_action_en: "Try opening the page in a browser or inspect the diagnostic details.",
            retryable: true,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::Retry,
                RecoveryActionKind::OpenBrowserReadingMode,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
    }
}

fn describe_loading_code(code: SourceErrorCode) -> ErrorCodeDescriptor {
    match code {
        SourceErrorCode::ChapterNotFound => ErrorCodeDescriptor {
            group: FailureCategoryGroup::ChapterIdentification,
            title_vi: "Không tìm thấy chương truyện",
            explanation_vi: "Máy chủ báo chương này không tồn tại hoặc đã bị gỡ bỏ (HTTP 404/410).",
            title_en: "Chapter not found",
            explanation_en: "The upstream website reported that this chapter does not exist or has been removed.",
            suggested_action_vi: "Làm mới danh sách chương ở trang chi tiết truyện hoặc kiểm tra trên trang gốc.",
            suggested_action_en: "Refresh the series chapter list or verify the link on the original website.",
            retryable: false,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::OpenOriginalWebsite,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
        SourceErrorCode::JavascriptRenderRequired => ErrorCodeDescriptor {
            group: FailureCategoryGroup::ContentLoading,
            title_vi: "Cần trình duyệt hỗ trợ JavaScript",
            explanation_vi: "Trang web này tải ảnh chương bằng JavaScript sau khi mở trang. Mã HTML ban đầu chưa chứa các trang truyện thật.",
            title_en: "JavaScript rendering required",
            explanation_en: "This website loads chapter images dynamically using JavaScript. The initial HTML does not contain the actual comic pages.",
            suggested_action_vi: "Bật Headless Browser trong Cài đặt -> Nguồn truyện (trên Windows) hoặc dùng chế độ đọc qua trình duyệt.",
            suggested_action_en: "Open in browser-based reading mode or enable Headless Browser fallback in Settings.",
            retryable: true,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::OpenBrowserReadingMode,
                RecoveryActionKind::Retry,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
        SourceErrorCode::BrowserEngineUnavailable => ErrorCodeDescriptor {
            group: FailureCategoryGroup::ContentLoading,
            title_vi: "Không khả dụng trình duyệt nền trên thiết bị này",
            explanation_vi: "Trang web yêu cầu chạy JavaScript để hiển thị nội dung, nhưng không tìm thấy trình duyệt nền (Chromium/Edge) khả dụng để kết xuất tự động.",
            title_en: "Browser rendering engine unavailable",
            explanation_en: "JavaScript rendering is required for this source, but no local Chromium/headless browser runtime is available on this device.",
            suggested_action_vi: "Mở trực tiếp chương truyện bằng chế độ đọc qua trình duyệt hệ thống.",
            suggested_action_en: "Open this page in browser-based reading mode.",
            retryable: false,
            user_interaction_required: true,
            actions: vec![
                RecoveryActionKind::OpenBrowserReadingMode,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
        _ => ErrorCodeDescriptor {
            group: FailureCategoryGroup::ContentLoading,
            title_vi: "Nội dung chương chưa được tải đầy đủ",
            explanation_vi: "Đã nhận diện đây là trang đọc chương nhưng trình đọc trên trang chưa nạp danh sách ảnh sau khi chờ và cuộn trang.",
            title_en: "Chapter content not loaded",
            explanation_en: "The page was identified as a chapter reader, yet no readable page images finished loading.",
            suggested_action_vi: "Thử tải lại hoặc mở trực tiếp ở chế độ đọc qua trình duyệt.",
            suggested_action_en: "Retry loading or switch to browser-based reading mode.",
            retryable: true,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::Retry,
                RecoveryActionKind::OpenBrowserReadingMode,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
    }
}

fn describe_access_code(code: SourceErrorCode) -> ErrorCodeDescriptor {
    match code {
        SourceErrorCode::LoginRequired => ErrorCodeDescriptor {
            group: FailureCategoryGroup::AccessPermission,
            title_vi: "Yêu cầu đăng nhập tài khoản",
            explanation_vi: "Chương hoặc trang truyện này yêu cầu tài khoản thành viên. Gt-Maga hiện chưa có phiên đăng nhập hợp lệ cho trang này.",
            title_en: "Login required",
            explanation_en: "This chapter requires an authenticated account. Gt-Maga cannot access its content using the current session.",
            suggested_action_vi: "Đăng nhập bằng tài khoản của bạn thông qua trang web gốc.",
            suggested_action_en: "Sign in through the website and then retry loading.",
            retryable: true,
            user_interaction_required: true,
            actions: vec![
                RecoveryActionKind::SignInViaBrowser,
                RecoveryActionKind::Retry,
                RecoveryActionKind::OpenOriginalWebsite,
            ],
        },
        SourceErrorCode::SessionExpired => ErrorCodeDescriptor {
            group: FailureCategoryGroup::AccessPermission,
            title_vi: "Phiên đăng nhập đã hết hạn",
            explanation_vi: "Cookie hoặc phiên đăng nhập trước đó không còn hiệu lực hoặc đã bị trang web chuyển hướng về trang đăng nhập.",
            title_en: "Session expired",
            explanation_en: "Your previously authorized session has expired or was redirected to a login prompt.",
            suggested_action_vi: "Đăng nhập lại qua trình duyệt để làm mới phiên truy cập.",
            suggested_action_en: "Sign in again via browser to refresh your session.",
            retryable: true,
            user_interaction_required: true,
            actions: vec![
                RecoveryActionKind::SignInViaBrowser,
                RecoveryActionKind::Retry,
            ],
        },
        SourceErrorCode::VerificationRequired => ErrorCodeDescriptor {
            group: FailureCategoryGroup::AccessPermission,
            title_vi: "Cần xác minh bảo mật thủ công",
            explanation_vi: "Trang web yêu cầu bước xác minh người dùng (CAPTCHA / Turnstile / kiểm tra trình duyệt) trước khi cho phép đọc tiếp.",
            title_en: "Verification required",
            explanation_en: "The website requires an additional human verification step before continuing.",
            suggested_action_vi: "Mở cửa sổ xác minh để tự hoàn tất kiểm tra, sau đó Gt-Maga sẽ tự thử tải lại.",
            suggested_action_en: "Complete verification manually in the browser window, then retry.",
            retryable: true,
            user_interaction_required: true,
            actions: vec![
                RecoveryActionKind::CompleteVerification,
                RecoveryActionKind::OpenOriginalWebsite,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
        _ => describe_restriction_and_drm_code(code),
    }
}

fn describe_restriction_and_drm_code(code: SourceErrorCode) -> ErrorCodeDescriptor {
    match code {
        SourceErrorCode::GeoRestriction => ErrorCodeDescriptor {
            group: FailureCategoryGroup::AccessPermission,
            title_vi: "Bị giới hạn theo khu vực địa lý",
            explanation_vi: "Trang web hoặc nhà phát hành giới hạn quyền truy cập nội dung tại khu vực mạng hiện tại (HTTP 451 / thông báo giới hạn vùng).",
            title_en: "Geographic restriction",
            explanation_en: "This website restricts access to this comic or chapter in your current network region.",
            suggested_action_vi: "Kiểm tra cài đặt Proxy / DNS mã hóa (DoH) hợp lệ trong ứng dụng hoặc mở trang gốc.",
            suggested_action_en: "Review your in-app Proxy/DNS settings or open the original website.",
            retryable: false,
            user_interaction_required: true,
            actions: vec![
                RecoveryActionKind::ConfigureNetworkOrProxy,
                RecoveryActionKind::OpenOriginalWebsite,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
        SourceErrorCode::PaywallOrSubscriptionRequired => ErrorCodeDescriptor {
            group: FailureCategoryGroup::AccessPermission,
            title_vi: "Nội dung trả phí hoặc yêu cầu gói hội viên",
            explanation_vi: "Chương này yêu cầu mua bằng xu/vé hoặc tài khoản đang có gói VIP/đăng ký hợp lệ. Tài khoản hiện tại chưa có quyền truy cập.",
            title_en: "Subscription or purchase required",
            explanation_en: "This chapter requires an active subscription or purchase. The current session does not have access.",
            suggested_action_vi: "Mở trang web gốc để đăng nhập, mua chương hoặc đăng ký gói hội viên.",
            suggested_action_en: "Open the original website to sign in or unlock access.",
            retryable: false,
            user_interaction_required: true,
            actions: vec![
                RecoveryActionKind::OpenOriginalWebsite,
                RecoveryActionKind::SignInViaBrowser,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
        SourceErrorCode::DrmProtectedContent => ErrorCodeDescriptor {
            group: FailureCategoryGroup::AccessPermission,
            title_vi: "Nội dung được bảo vệ chống trích xuất (DRM)",
            explanation_vi: "Trang web sử dụng cơ chế bảo vệ hiển thị (ví dụ IMGX / canvas taint / mã hóa trình đọc) không cho phép trích xuất ảnh gốc trực tiếp.",
            title_en: "DRM-protected content",
            explanation_en: "This website uses content protection that prevents native image extraction. Gt-Maga cannot extract these protected images.",
            suggested_action_vi: "Đọc chương này trực tiếp trên trang web gốc bằng trình duyệt.",
            suggested_action_en: "Read through the original website in your browser.",
            retryable: false,
            user_interaction_required: true,
            actions: vec![
                RecoveryActionKind::OpenOriginalWebsite,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
        SourceErrorCode::ExtractionNotPermitted => ErrorCodeDescriptor {
            group: FailureCategoryGroup::AccessPermission,
            title_vi: "Nguồn không cho phép trích xuất tự động",
            explanation_vi: "Chính sách truy cập hoặc phản hồi từ trang web không cho phép trích xuất ảnh trực tiếp bên ngoài trình duyệt.",
            title_en: "Extraction not permitted",
            explanation_en: "The source does not permit automated image extraction outside an authorized browser view.",
            suggested_action_vi: "Sử dụng chế độ đọc qua trình duyệt hoặc mở trang gốc.",
            suggested_action_en: "Read directly through the original website.",
            retryable: false,
            user_interaction_required: true,
            actions: vec![
                RecoveryActionKind::OpenBrowserReadingMode,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
        _ => ErrorCodeDescriptor {
            group: FailureCategoryGroup::AccessPermission,
            title_vi: "Trang web từ chối quyền truy cập",
            explanation_vi: "Máy chủ trả về lỗi từ chối truy cập (HTTP 403) cho yêu cầu hiện tại.",
            title_en: "Access denied",
            explanation_en: "The upstream website denied access to this resource (HTTP 403).",
            suggested_action_vi: "Thử hoàn tất xác minh trình duyệt, kiểm tra mạng/Proxy hoặc mở trực tiếp trang web.",
            suggested_action_en: "Try browser verification, review network settings, or open the original website.",
            retryable: true,
            user_interaction_required: true,
            actions: vec![
                RecoveryActionKind::CompleteVerification,
                RecoveryActionKind::OpenOriginalWebsite,
                RecoveryActionKind::ConfigureNetworkOrProxy,
            ],
        },
    }
}

fn describe_media_and_format_code(code: SourceErrorCode) -> ErrorCodeDescriptor {
    match code {
        SourceErrorCode::UnsupportedApiFormat => ErrorCodeDescriptor {
            group: FailureCategoryGroup::UnsupportedFormat,
            title_vi: "Định dạng dữ liệu API chưa được hỗ trợ",
            explanation_vi: "Trang web trả về dữ liệu ở định dạng nhị phân (như protobuf) hoặc cấu trúc API mà bộ đọc hiện tại chưa giải mã được.",
            title_en: "Unsupported API data format",
            explanation_en: "The website returned data in a format that the current adapter cannot interpret.",
            suggested_action_vi: "Sử dụng chế độ đọc qua trình duyệt hoặc xuất báo cáo chẩn đoán.",
            suggested_action_en: "Use browser-based reading or export diagnostics for this source.",
            retryable: false,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::OpenBrowserReadingMode,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
        SourceErrorCode::UnsupportedImageFormat => ErrorCodeDescriptor {
            group: FailureCategoryGroup::UnsupportedFormat,
            title_vi: "Định dạng ảnh không được hỗ trợ",
            explanation_vi: "Máy chủ ảnh trả về tệp không phải ảnh truyện hợp lệ (ví dụ trang HTML báo lỗi, SVG hoặc kiểu MIME không hỗ trợ).",
            title_en: "Unsupported image format",
            explanation_en: "The image endpoint returned an unsupported MIME type or non-image payload.",
            suggested_action_vi: "Thử tải lại chương hoặc mở trang gốc trên trình duyệt.",
            suggested_action_en: "Retry loading the chapter or open the original website.",
            retryable: true,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::Retry,
                RecoveryActionKind::OpenBrowserReadingMode,
            ],
        },
        SourceErrorCode::ImageDecodeFailed => ErrorCodeDescriptor {
            group: FailureCategoryGroup::ImageExtraction,
            title_vi: "Dữ liệu ảnh bị hỏng hoặc không đầy đủ",
            explanation_vi: "Tệp ảnh tải về bị rỗng, cắt cụt phần đầu/đuôi hoặc có kích thước bất thường (như ảnh 1x1).",
            title_en: "Image decode/validation failed",
            explanation_en: "The downloaded image payload was empty, truncated, corrupted, or smaller than a comic page.",
            suggested_action_vi: "Nhấn Thử lại để tải lại từ máy chủ dự phòng hoặc mở qua trình duyệt.",
            suggested_action_en: "Retry to fall back to backup mirrors or open in a browser.",
            retryable: true,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::Retry,
                RecoveryActionKind::OpenBrowserReadingMode,
            ],
        },
        SourceErrorCode::ImageSourceChanged => ErrorCodeDescriptor {
            group: FailureCategoryGroup::ImageExtraction,
            title_vi: "Đường dẫn ảnh đã hết hạn hoặc bị đổi (CDN)",
            explanation_vi: "Liên kết ảnh trên CDN có chữ ký tạm thời đã hết hạn hoặc bị chặn hotlink (HTTP 403/410).",
            title_en: "Image source expired or changed",
            explanation_en: "The CDN image URL expired, moved, or rejected direct fetching (HTTP 403/410).",
            suggested_action_vi: "Tải lại chương để làm mới danh sách liên kết ảnh từ trang nguồn.",
            suggested_action_en: "Reload the chapter to refresh image URLs from the source page.",
            retryable: true,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::Retry,
                RecoveryActionKind::OpenOriginalWebsite,
            ],
        },
        _ => ErrorCodeDescriptor {
            group: FailureCategoryGroup::ImageExtraction,
            title_vi: "Tải ảnh chương thất bại",
            explanation_vi: "Không thể tải xuống một hoặc nhiều trang ảnh từ máy chủ CDN của nguồn truyện.",
            title_en: "Image request failed",
            explanation_en: "Failed to download chapter image bytes from the upstream image server.",
            suggested_action_vi: "Kiểm tra kết nối mạng/Proxy rồi thử tải lại trang.",
            suggested_action_en: "Check network/proxy connectivity and retry.",
            retryable: true,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::Retry,
                RecoveryActionKind::ConfigureNetworkOrProxy,
            ],
        },
    }
}

fn describe_network_and_adapter_code(code: SourceErrorCode) -> ErrorCodeDescriptor {
    match code {
        SourceErrorCode::RateLimited => ErrorCodeDescriptor {
            group: FailureCategoryGroup::Network,
            title_vi: "Trang web đang giới hạn tần suất (HTTP 429)",
            explanation_vi: "Máy chủ tạm thời chặn bớt yêu cầu vì gửi quá nhanh liên tiếp. Ứng dụng đã áp dụng thời gian chờ làm mát cho tên miền này.",
            title_en: "Rate limited by website (HTTP 429)",
            explanation_en: "The website temporarily throttled requests (HTTP 429 Too Many Requests).",
            suggested_action_vi: "Đợi 10–30 giây rồi nhấn Thử lại hoặc giảm số kết nối song song trong Cài đặt.",
            suggested_action_en: "Wait briefly and retry, or lower max concurrent connections per host.",
            retryable: true,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::Retry,
                RecoveryActionKind::OpenOriginalWebsite,
            ],
        },
        SourceErrorCode::NetworkTimeout => ErrorCodeDescriptor {
            group: FailureCategoryGroup::Network,
            title_vi: "Hết thời gian kết nối tới trang web",
            explanation_vi: "Trang nguồn hoặc CDN phản hồi quá chậm và vượt quá giới hạn chờ mạng.",
            title_en: "Network request timed out",
            explanation_en: "The remote website or image CDN took too long to respond.",
            suggested_action_vi: "Thử lại hoặc kiểm tra DNS/Proxy trong Cài đặt -> Mạng.",
            suggested_action_en: "Retry or inspect your DNS/Proxy settings in Settings -> Network.",
            retryable: true,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::Retry,
                RecoveryActionKind::ConfigureNetworkOrProxy,
            ],
        },
        SourceErrorCode::ServerError => ErrorCodeDescriptor {
            group: FailureCategoryGroup::Network,
            title_vi: "Máy chủ trang truyện đang gặp sự cố (5xx)",
            explanation_vi: "Trang web nguồn trả về lỗi máy chủ nội bộ hoặc đang quá tải/bảo trì.",
            title_en: "Upstream server error",
            explanation_en: "The comic website returned an upstream 5xx server error.",
            suggested_action_vi: "Chờ vài phút cho máy chủ ổn định rồi thử lại.",
            suggested_action_en: "Wait a moment for the upstream server to recover and retry.",
            retryable: true,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::Retry,
                RecoveryActionKind::OpenOriginalWebsite,
            ],
        },
        SourceErrorCode::AdapterOutdated => ErrorCodeDescriptor {
            group: FailureCategoryGroup::SourceDetection,
            title_vi: "Bộ đọc chuyên biệt có thể đã lỗi thời",
            explanation_vi: "Trang web dường như đã đổi bố cục HTML hoặc cấu trúc API so với phiên bản bộ chuyển đổi hiện tại.",
            title_en: "Site adapter appears outdated",
            explanation_en: "The website layout or API response changed beyond what the current adapter supports.",
            suggested_action_vi: "Thử mở trên trình duyệt hoặc xuất báo cáo chẩn đoán để cập nhật bộ nguồn.",
            suggested_action_en: "Use browser-based reading or export diagnostics so the adapter can be updated.",
            retryable: false,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::OpenBrowserReadingMode,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
        SourceErrorCode::AdapterExecutionFailed => ErrorCodeDescriptor {
            group: FailureCategoryGroup::Network,
            title_vi: "Bộ xử lý nguồn gặp lỗi thực thi nội bộ",
            explanation_vi: "Bộ chuyển đổi của trang này gặp lỗi xử lý bất ngờ nhưng đã được cô lập an toàn để không ảnh hưởng toàn bộ ứng dụng.",
            title_en: "Site adapter execution failed",
            explanation_en: "A site-specific adapter encountered an unexpected execution failure that was safely isolated.",
            suggested_action_vi: "Thử lại hoặc xuất báo cáo chẩn đoán.",
            suggested_action_en: "Retry or export the sanitized diagnostic report.",
            retryable: true,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::Retry,
                RecoveryActionKind::OpenBrowserReadingMode,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
        _ => ErrorCodeDescriptor {
            group: FailureCategoryGroup::Network,
            title_vi: "Lỗi nguồn chưa xác định được nguyên nhân cụ thể",
            explanation_vi: "Quá trình đọc nguồn thất bại nhưng phản hồi hiện có chưa đủ dữ kiện để kết luận nguyên nhân chắc chắn.",
            title_en: "Unknown source error",
            explanation_en: "The operation failed, and available evidence is insufficient to pinpoint a more specific cause.",
            suggested_action_vi: "Kiểm tra kết nối, thử mở qua trình duyệt hoặc sao chép chi tiết chẩn đoán.",
            suggested_action_en: "Retry, try browser-based reading, or inspect/export diagnostic details.",
            retryable: true,
            user_interaction_required: false,
            actions: vec![
                RecoveryActionKind::Retry,
                RecoveryActionKind::OpenBrowserReadingMode,
                RecoveryActionKind::ExportDiagnostics,
            ],
        },
    }
}
