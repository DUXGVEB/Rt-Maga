// Copyright (c) 2025-2026 RtMaga (DUXGVEB). Source-available, not Open Source.
// See LICENSE and COPYRIGHT.md in the repository root for permitted uses and restrictions.

//! Core manga scraping, parsing, and URL resolution library for `RtMaga`.
//!
//! `mangacore` has no dependency on Tauri or SQLite. Its HTML parsing heuristics live in [`parse`]
//! as pure functions that operate on HTML strings and URLs, making them fast to test offline
//! against saved fixtures.

#![warn(missing_docs)]

pub mod cms;
pub mod detect;
pub mod diagnostics;
pub mod doh;
pub mod error;
pub mod extract;
pub mod http;
pub mod link_matcher;
pub mod model;
pub mod parse;
pub mod recognize;
pub mod registry;
pub mod resolver;
pub mod source;
pub mod throttle;

pub use cms::{
    CmsKind, SiteDetectionOutcome, SiteReachability, browse_or_filter_cms, browse_or_search_cms,
    detect_cms_from_html, detect_site_structure, fetch_madara_chapters_ajax,
    match_builtin_adapter_id, normalize_source_root,
};
pub use diagnostics::{
    BrowserFallbackAvailability, ErrorCodeDescriptor, ExtractionStrategyKind, FailureCategoryGroup,
    RecoveryActionKind, SourceDiagnosticReport, SourceErrorCode, StrategyAttemptTrace,
    describe_error_code, redact_url_for_diagnostics,
};
pub use doh::{
    DohConfig, DohProfileInfo, DohTestResult, NetworkProviderInfo, NetworkServiceType,
    network_provider_catalog, test_doh_config,
};
pub use error::{ClassifiedFailureDetail, ScrapeError};
pub use http::{
    DomainProxyRule, DomainRuleAction, FetchedBytes, FetchedPage, HttpClient, HttpPolicy,
    ProxyConfig, ProxyEndpoint, ProxyProtocol, ProxyTestResult, normalize_url,
};
pub use link_matcher::{
    UserQueryKind, classify_user_input, resolve_exact_from_source, rewrite_domain_override,
    within_edit_ratio,
};
pub use model::{
    CatalogEntry, CatalogPage, Chapter, ChapterRef, Manga, NavLink, PageImage, PaginationInfo,
    Resolved, SiteId,
};
pub use parse::filters::{
    CatalogFilterQuery, FilterAxis, FilterAxisKind, FilterOptionItem, FilterTagOption,
    SiteFilterSchema, UnhandledFilterMask, UpdatedWithinFilter, build_filtered_catalog_url,
    estimate_age_hours_from_label, extract_generic_filter_schema, filter_entries_client_side,
    merge_filter_schemas,
};
pub use parse::pagination::{
    apply_page_to_template, extract_page_number_from_url, extract_universal_pagination,
    template_from_numbered_url,
};
pub use recognize::{
    PreFetchClassification, RecognitionDiagnostic, RecognitionStatus, UrlKind,
    classify_normalized_url, classify_url_pre_fetch, diagnostic_from_error,
};
pub use registry::{HeadlessPolicy, Registry};
pub use resolver::{
    CandidateOrigin, DetectedImageFormat, ImageHeaderInfo, ImageValidationError, RawImageCandidate,
    ResolvedImageCollection, inspect_image_bytes,
};
pub use source::Source;
pub use source::default_filter_schema_for_adapter;
pub use throttle::{BandwidthLimiter, MAX_SPEED_LIMIT_KBPS};
