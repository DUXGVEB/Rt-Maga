export type Language = "en" | "vi" | "zh" | "ja" | "ru" | "ko" | "th";

export type Theme = "system" | "light" | "dark";

export type NavDestination = "home" | "sources" | "bookmarks" | "history" | "settings";

export interface UiCustomizationSettings {
  lightAccent: string;
  darkAccent: string;
  navOrder: NavDestination[];
}

export type ImageWidth =
  | { mode: "fit_window" }
  | { mode: "percent"; value: number }
  | { mode: "px"; value: number };

export type ReaderBg = "dark" | "light" | "follow_theme";

export type ReadingOrientation = "vertical" | "horizontal";

export type VerticalLayoutMode = "continuous_strip" | "vertical_grouped";

export type HorizontalDirection = "ltr" | "rtl";

export type HorizontalSnapMode = "none" | "page" | "group";

export type GroupAlignment = "center" | "start";

export type OrientationLockMode = "auto" | "portrait" | "landscape";

export type FloatingEdge = "left" | "right";

export type FloatingScreenKey =
  | "home"
  | "sources"
  | "browse"
  | "bookmarks"
  | "history"
  | "manga"
  | "reader"
  | "settings";

export interface FloatingButtonPosition {
  edge: FloatingEdge;
  verticalPercent: number;
  horizontalOffsetPx: number;
}

export interface FloatingButtonsSettings {
  scrollToTopScreens: FloatingScreenKey[];
  scrollToTop: FloatingButtonPosition;
  showReaderSettings: boolean;
  readerSettings: FloatingButtonPosition;
  showOrientationLock: boolean;
  orientationLock: FloatingButtonPosition;
  autoDimInReader: boolean;
}

export interface DeviceSettings {
  customName: string | null;
}

export type DoubleTapAction = "zoom" | "toggle_toolbar" | "next_page" | "disabled";

export type OneHandedMode = "default" | "left" | "right";

export type TranslationModelTier = "lite" | "qwen3_vl_2b";

export interface AccessibilitySettings {
  oneHandedMode: OneHandedMode;
  bubbleAutoZoom: boolean;
  aiTranslationOverlay: boolean;
  translationModelTier: TranslationModelTier;
  translationTargetLang: Language | null;
  firstAiPromptSeen: boolean;
}

export type SettingsSection =
  | "appearance"
  | "device"
  | "reader"
  | "floating"
  | "accessibility"
  | "network"
  | "updates"
  | "sources"
  | "cache";

export type HideAction = "hide" | "unhide";

export interface VerticalModeSettings {
  layoutMode: VerticalLayoutMode;
  pagesPerGroup: number;
  pageGapPx: number;
  groupGapPx: number;
  snapEnabled: boolean;
  alignment: GroupAlignment;
  dimUnfocused: boolean;
  dimLevel: number;
  noGapMode: boolean;
}

export interface HorizontalModeSettings {
  pagesPerGroup: number;
  pageGapPx: number;
  groupGapPx: number;
  direction: HorizontalDirection;
  snapMode: HorizontalSnapMode;
  centerGroups: boolean;
  dimUnfocused: boolean;
  dimLevel: number;
  noGapMode: boolean;
}

export interface ReaderBehaviorSettings {
  keepScreenOn: boolean;
  skipBrokenImages: boolean;
  autoMarkReadOnOpen: boolean;
  doubleTapAction: DoubleTapAction;
  toolbarAutoHideSecs: number;
  verticalMarginPx: number;
  navigationSpeed: number;
}

export interface ReaderSettings {
  orientation: ReadingOrientation;
  width: ImageWidth;
  gapPx: number;
  preloadImages: number;
  autoNextChapter: boolean;
  autoWebtoonDetect: boolean;
  tapZonesEnabled: boolean;
  orientationLock: OrientationLockMode;
  readerBg: ReaderBg;
  vertical: VerticalModeSettings;
  horizontal: HorizontalModeSettings;
  behavior: ReaderBehaviorSettings;
}

export type NetworkServiceType = "encrypted_dns" | "dns_filtering" | "local_tunnel";

export interface DohProfileInfo {
  id: string;
  label: string;
  description: string;
  endpointUrl: string;
  serviceType: NetworkServiceType;
}

export interface NetworkProviderInfo {
  id: string;
  name: string;
  serviceType: NetworkServiceType;
  summary: string;
  limitations: string;
  requiresAccountId: boolean;
  accountIdLabel: string | null;
  profiles: DohProfileInfo[];
}

export interface DohConfig {
  enabled: boolean;
  providerId: string;
  profileId: string;
  customProfileId: string | null;
  allowSystemFallback: boolean;
}

export interface DohTestResult {
  ok: boolean;
  latencyMs: number;
  providerName: string;
  profileLabel: string;
  resolvedIps: number;
  error: string | null;
}

export type ProxyProtocol = "http" | "https" | "socks5";

export interface ProxyEndpoint {
  protocol: ProxyProtocol;
  host: string;
  port: number;
  username: string | null;
  password: string | null;
}

export type DomainRuleAction = "use_default" | "force_proxy" | "force_direct" | "custom_proxy";

export interface DomainProxyRule {
  domain: string;
  action: DomainRuleAction;
  customProxy: ProxyEndpoint | null;
}

export interface ProxyConfig {
  enabled: boolean;
  defaultProxy: ProxyEndpoint;
  allowDirectFallback: boolean;
  domainRules: DomainProxyRule[];
}

export interface ProxyTestResult {
  ok: boolean;
  latencyMs: number;
  statusCode: number | null;
  targetUrl: string;
  viaProxy: string;
  error: string | null;
}

export type SyncSenderMode = "pc_qr" | "phone_pin";

export type SyncDecision = "approve" | "reject";

export type SyncPhase =
  | "idle"
  | "hosting_waiting"
  | "host_confirm_peer"
  | "host_waiting_pull"
  | "receiver_confirm_peer"
  | "receiver_waiting_host"
  | "transferring"
  | "completed"
  | "expired"
  | "locked_out"
  | "cancelled"
  | "error";

export interface SyncMergeReport {
  addedManga: number;
  updatedNewerChapter: number;
  skippedSameChapter: number;
  keptLocalNewer: number;
  syncedBookmarks: number;
  totalReceived: number;
}

export interface SyncStatusView {
  phase: SyncPhase;
  mode: SyncSenderMode | null;
  localDeviceName: string;
  localIp: string | null;
  localPort: number | null;
  pairingCode: string | null;
  qrSvg: string | null;
  compactCode: string | null;
  expiresAtUnix: number | null;
  mangaCount: number;
  peerName: string | null;
  peerIp: string | null;
  failedAttempts: number;
  maxAttempts: number;
  report: SyncMergeReport | null;
  message: string | null;
}

export interface UpdateSettings {
  enabled: boolean;
  intervalHours: number;
  notifyWindows: boolean;
  checkAppUpdates: boolean;
}

export interface SourceSettings {
  headlessEnabled: boolean;
  maxConnPerHost: number;
  speedLimitKbps: number;
}

export interface CacheSettings {
  maxMb: number;
}

export interface Settings {
  language: Language;
  theme: Theme;
  ui: UiCustomizationSettings;
  device: DeviceSettings;
  reader: ReaderSettings;
  floating: FloatingButtonsSettings;
  accessibility: AccessibilitySettings;
  updates: UpdateSettings;
  sources: SourceSettings;
  doh: DohConfig;
  proxy: ProxyConfig;
  cache: CacheSettings;
}

export interface DeviceInsets {
  cutoutTop: number;
  cutoutBottom: number;
  cutoutLeft: number;
  cutoutRight: number;
  gestureLeft: number;
  gestureRight: number;
  imeBottom: number;
}

export interface DeviceEnvInfo {
  platform: "windows" | "android" | "other" | string;
  sdkInt: number;
  manufacturer: string;
  model: string;
  skin: string;
  realDeviceName: string;
  orientation: "portrait" | "landscape" | string;
  orientationLock: OrientationLockMode | string;
  viewportWidthDp: number;
  viewportHeightDp: number;
  immersive: boolean;
  insets: DeviceInsets;
}

export interface NotificationPermissionInfo {
  granted: boolean;
  canRequest: boolean;
}

export interface OrientationLockReply {
  ok: boolean;
  mode: string;
}

export type UrlKind =
  | "WEBSITE_HOME"
  | "CATALOG_PAGE"
  | "SEARCH_PAGE"
  | "MANGA_DETAIL"
  | "CHAPTER_PAGE"
  | "IMAGE_URL"
  | "API_ENDPOINT"
  | "REDIRECTED_URL"
  | "LOGIN_PAGE"
  | "VERIFICATION_PAGE"
  | "PAYWALL_PAGE"
  | "NOT_A_COMIC_PAGE"
  | "UNSUPPORTED_URL"
  | "INVALID_URL"
  | "BLOCKED_OR_RESTRICTED";

export type SourceErrorCode =
  | "INVALID_URL"
  | "UNSUPPORTED_WEBSITE"
  | "NOT_A_COMIC_PAGE"
  | "SOURCE_DETECTION_FAILED"
  | "CHAPTER_NOT_FOUND"
  | "CHAPTER_CONTENT_NOT_LOADED"
  | "JAVASCRIPT_RENDER_REQUIRED"
  | "BROWSER_ENGINE_UNAVAILABLE"
  | "LOGIN_REQUIRED"
  | "SESSION_EXPIRED"
  | "VERIFICATION_REQUIRED"
  | "ACCESS_DENIED"
  | "GEO_RESTRICTION"
  | "PAYWALL_OR_SUBSCRIPTION_REQUIRED"
  | "DRM_PROTECTED_CONTENT"
  | "EXTRACTION_NOT_PERMITTED"
  | "UNSUPPORTED_API_FORMAT"
  | "UNSUPPORTED_IMAGE_FORMAT"
  | "IMAGE_DECODE_FAILED"
  | "IMAGE_REQUEST_FAILED"
  | "IMAGE_SOURCE_CHANGED"
  | "RATE_LIMITED"
  | "NETWORK_TIMEOUT"
  | "SERVER_ERROR"
  | "ADAPTER_OUTDATED"
  | "ADAPTER_EXECUTION_FAILED"
  | "UNKNOWN_SOURCE_ERROR";

export type FailureCategoryGroup =
  | "source_detection"
  | "chapter_identification"
  | "content_loading"
  | "image_extraction"
  | "access_permission"
  | "unsupported_format"
  | "network";

export type ExtractionStrategyKind =
  | "site_adapter"
  | "structured_data_and_api"
  | "native_html"
  | "browser_rendering"
  | "authenticated_browser";

export type RecoveryActionKind =
  | "retry"
  | "open_browser_reading_mode"
  | "sign_in_via_browser"
  | "complete_verification"
  | "open_original_website"
  | "configure_network_or_proxy"
  | "export_diagnostics";

export interface StrategyAttemptTrace {
  strategy: ExtractionStrategyKind;
  adapterId: string | null;
  outcome: string;
  candidatesFound: number;
  validatedCount: number;
  durationMs: number;
}

export interface SourceDiagnosticReport {
  originalUrl: string;
  finalUrl: string | null;
  domain: string | null;
  detectedPageType: UrlKind;
  detectionConfidence: number;
  extractionConfidence: number;
  selectedStrategy: ExtractionStrategyKind | null;
  strategiesAttempted: StrategyAttemptTrace[];
  failedStage: string | null;
  httpStatus: number | null;
  contentType: string | null;
  responseBytes: number | null;
  imageCandidatesCount: number;
  validatedImagesCount: number;
  adapterName: string | null;
  adapterVersion: string | null;
  browserErrors: string[];
  errorCode: SourceErrorCode | null;
  failureGroup: FailureCategoryGroup | null;
  titleVi: string | null;
  explanationVi: string | null;
  titleEn: string | null;
  explanationEn: string | null;
  evidence: string[];
  retryable: boolean;
  userInteractionRequired: boolean;
  browserReadingAvailable: boolean;
  suggestedActionVi: string | null;
  suggestedActionEn: string | null;
  recoveryActions: RecoveryActionKind[];
  timestampUnix: number;
}

export interface BrowserAuthOutcome {
  mode: "integrated_session" | "external_browser" | string;
  cookiesSynced: boolean;
}

export type ChapterLoadPriority = "primary" | "prefetch";

export interface FormattedAppError {
  title: string;
  detail: string;
  blockedUrl?: string;
  externalUrl?: string;
  loginUrl?: string;
  code?: SourceErrorCode;
  failureGroup?: FailureCategoryGroup;
  suggestedAction?: string;
  retryable?: boolean;
  userInteractionRequired?: boolean;
  browserReadingAvailable?: boolean;
  report?: SourceDiagnosticReport;
}

export type RecognitionStatus =
  | "WEBSITE_IDENTIFIED"
  | "PAGE_TYPE_IDENTIFIED"
  | "MANGA_IDENTIFIED"
  | "CATALOG_IDENTIFIED"
  | "SEARCH_RESULTS_EXTRACTED"
  | "CHAPTER_IDENTIFIED"
  | "IMAGE_RESOURCES_EXTRACTED"
  | "WEBSITE_SUPPORTED_BUT_PAGE_UNRECOGNIZED"
  | "UNSUPPORTED_WEBSITE"
  | "INVALID_URL"
  | "NETWORK_FAILURE"
  | "DNS_FAILURE"
  | "TLS_FAILURE"
  | "API_FAILURE"
  | "RATE_LIMITED"
  | "HTTP_ERROR"
  | "PARSING_FAILURE"
  | "MISSING_METADATA"
  | "IMAGE_EXTRACTION_FAILURE"
  | "ACCESS_RESTRICTED"
  | "VERIFICATION_REQUIRED"
  | "CONTENT_PROTECTED";

export interface RecognitionDiagnostic {
  status: RecognitionStatus;
  urlKind: UrlKind;
  website: string | null;
  adapter: string | null;
  submittedUrl: string;
  resolvedUrl: string | null;
  stage: string;
  httpStatus: number | null;
  detail: string;
  suggestedAction: string | null;
  confidence?: number;
  report?: SourceDiagnosticReport | null;
  timestampUnix: number;
}

export interface ChapterRefView {
  url: string;
  name: string;
  number: number | null;
  idx: number;
  read: boolean;
  volume: string | null;
  publishedAt: string | null;
  language: string | null;
  sourceId: string | null;
}

export interface ProgressView {
  chapterUrl: string;
  chapterName: string;
  scrollRatio: number;
  pageIndex: number;
  readAt: number;
}

export type ReaderModeOverride = "vertical_strip" | "vertical_grouped" | "horizontal" | null;

export interface MangaView {
  id: number;
  url: string;
  site: string;
  title: string;
  coverToken: string | null;
  hasNew: boolean;
  bookmarked: boolean;
  hidden: boolean;
  unreadCount: number;
  isCompleted: boolean;
  readerModeOverride: ReaderModeOverride;
  readerSettings: ReaderSettings | null;
  detectedWebtoon: boolean;
  detectedLayout?: "manga" | "webtoon" | null;
  latestChapterName: string | null;
  sourceId: string | null;
  canonicalUrl: string | null;
  altTitles: string[];
  originalTitle: string | null;
  authors: string[];
  artists: string[];
  description: string | null;
  genres: string[];
  tags: string[];
  status: string | null;
  language: string | null;
  availableLanguages: string[];
  chapters: ChapterRefView[];
  progress: ProgressView | null;
}

export interface HiddenMangaView {
  id: number;
  url: string;
  site: string;
  title: string;
  coverToken: string | null;
  latestChapterName: string | null;
}

export interface FilterTagOption {
  id: string;
  label: string;
  url: string | null;
}

export interface FilterOptionItem {
  value: string;
  label: string;
}

export type FilterAxisKind = "single_select" | "multi_select";

export interface FilterAxis {
  key: string;
  label: string;
  kind: FilterAxisKind;
  options: FilterOptionItem[];
}

export interface SiteFilterSchema {
  tags: FilterTagOption[];
  extraAxes: FilterAxis[];
  filterEndpoint: string | null;
  includeTagParam: string | null;
  excludeTagParam: string | null;
  commaJoinedTags: boolean;
  minChaptersParam: string | null;
  keywordParam: string | null;
}

export type UpdatedWithinFilter = "any" | "last_24h" | "last_7d" | "last_30d" | "last_365d";

export interface CatalogFilterQuery {
  query?: string | null;
  includeTags: string[];
  excludeTags: string[];
  minChapters?: number | null;
  updatedWithin?: UpdatedWithinFilter;
  extra: Record<string, string>;
}

export interface CatalogEntryView {
  url: string;
  title: string;
  altTitles: string[];
  coverToken: string | null;
  latestChapter: string | null;
  status: string | null;
  genres: string[];
  sourceId: string | null;
  chapterCount: number | null;
  updatedAt: string | null;
}

export interface NavLinkView {
  label: string;
  url: string;
  kind: UrlKind;
}

export interface PaginationView {
  currentPage: number;
  totalPages: number | null;
  prevUrl: string | null;
  nextUrl: string | null;
  pageUrlTemplate: string | null;
}

export interface CatalogPageView {
  url: string;
  site: string;
  siteName: string;
  title: string;
  urlKind: UrlKind;
  query: string | null;
  searchUrlTemplate: string | null;
  entries: CatalogEntryView[];
  categories: NavLinkView[];
  pagination: PaginationView;
  filterSchema: SiteFilterSchema;
  sourceId: number | null;
}

export interface ResolveResponse {
  kind: "manga" | "chapter" | "catalog" | string;
  manga: MangaView | null;
  startAt: ChapterRefView | null;
  catalog: CatalogPageView | null;
  diagnostic: RecognitionDiagnostic;
}

export interface ChapterView {
  url: string;
  name: string;
  images: string[];
  prev: string | null;
  next: string | null;
  mangaId: number | null;
}

export interface HistoryItemView {
  mangaId: number;
  mangaUrl: string;
  site: string;
  title: string;
  coverToken: string | null;
  chapterUrl: string;
  chapterName: string;
  scrollRatio: number;
  pageIndex: number;
  readAt: number;
  addedAt: number;
  latestChapterUrl: string | null;
  latestChapterName: string | null;
  hasNew: boolean;
  bookmarked: boolean;
  totalChapters: number;
  unreadCount: number;
  isCompleted: boolean;
  currentChapterIdx: number | null;
}

export type VisibilityFilter = "main_only" | "supplemental_only" | "all";
export type SourcePlacement = "auto" | "force_main" | "force_supplemental";
export type VisibilityPatch = "unchanged" | "move_to_main" | "move_to_supplemental";

export interface SavedSourceView {
  id: number;
  sourceKey: string;
  displayName: string;
  baseUrl: string;
  sourceKind: string;
  parserRef: string | null;
  searchTemplate: string | null;
  iconToken: string | null;
  isEnabled: boolean;
  isSupplemental: boolean;
  manualVisibility: boolean;
  createdAt: number;
  updatedAt: number;
  filterSchema: SiteFilterSchema;
}

export interface SourceSearchGroupView {
  source: SavedSourceView;
  entries: CatalogEntryView[];
  error: string | null;
}

export interface MultiSourceSearchResponse {
  query: string;
  groups: SourceSearchGroupView[];
}

export type FeedbackReportKind = "bug" | "suggestion";

export interface FeedbackReportRequest {
  kind: FeedbackReportKind;
  title: string;
  description: string;
  attachLogs: boolean;
  logIds: number[];
}

export interface FormattedFeedbackReport {
  markdown: string;
  githubIssueUrl: string;
  urlTruncated: boolean;
  includedLogs: number;
}

export type LibrarySortField = "added" | "title";
export type SortDirection = "desc" | "asc";

export type BookmarkSort = "newest" | "title" | "recent_read" | "has_new";

export interface BookmarkItemView {
  mangaId: number;
  mangaUrl: string;
  site: string;
  title: string;
  coverToken: string | null;
  bookmarkedAt: number;
  latestChapterUrl: string | null;
  latestChapterName: string | null;
  hasNew: boolean;
  totalChapters: number;
  unreadCount: number;
  isCompleted: boolean;
  chapterUrl: string | null;
  chapterName: string | null;
  scrollRatio: number | null;
  pageIndex: number | null;
  readAt: number | null;
  currentChapterIdx: number | null;
  readerModeOverride: ReaderModeOverride;
  detectedWebtoon: boolean;
}

export interface NotificationItemView {
  id: number;
  mangaId: number;
  mangaTitle: string;
  coverToken: string | null;
  chapterRange: string;
  latestChapterUrl: string;
  chapterCount: number;
  read: boolean;
  createdAt: number;
}

export interface LastReadingDestination {
  mangaId: number;
  chapterUrl: string;
  chapterName: string;
  scrollRatio: number;
  pageIndex: number;
  readAt: number;
}

export interface CacheInfo {
  bytes: number;
  files: number;
}

export interface CacheSourceStats {
  site: string;
  bytes: number;
  files: number;
}

export interface CacheMangaStats {
  mangaId: number;
  title: string;
  site: string;
  bytes: number;
  files: number;
}

export interface CrashRecoverySession {
  mangaId: number;
  mangaTitle: string;
  chapterUrl: string;
  chapterName: string;
  scrollRatio: number;
  pageIndex: number;
  readingMode: ReadingOrientation;
  updatedAt: number;
}

export interface DiagnosticLogEntry {
  id: number;
  createdAt: number;
  level: "crash" | "error" | "warn" | "info" | string;
  category: string;
  message: string;
  detail: string | null;
  source: string | null;
  chapterUrl: string | null;
  stackTrace: string | null;
  failedUrl: string | null;
  resolvedUrl: string | null;
  httpStatus: number | null;
  adapter: string | null;
  parsingStage: string | null;
  apiError: string | null;
}

export interface NewDiagnosticLog {
  level: string;
  category: string;
  message: string;
  detail?: string | null;
  source?: string | null;
  chapterUrl?: string | null;
  stackTrace?: string | null;
  failedUrl?: string | null;
  resolvedUrl?: string | null;
  httpStatus?: number | null;
  adapter?: string | null;
  parsingStage?: string | null;
  apiError?: string | null;
}

export interface SavedDiagnosticFile {
  path: string;
  bytes: number;
  logCount: number;
}

export interface FoundUpdate {
  mangaId: number;
  title: string;
  latest: string;
}

export interface UpdateProgress {
  done: number;
  total: number;
}

export interface UpdateSummary {
  checked: number;
  found: FoundUpdate[];
}

export interface AppReleaseInfo {
  currentVersion: string;
  latestVersion: string;
  tagName: string;
  releaseUrl: string;
  downloadUrl: string | null;
  downloadSize: number | null;
  downloadSha256: string | null;
  publishedAt: string | null;
  updateAvailable: boolean;
  canAutoInstall: boolean;
}

export type UpdateStage = "downloading" | "verifying" | "backing_up" | "installing";

export interface UpdateDownloadProgress {
  stage: UpdateStage;
  downloadedBytes: number;
  totalBytes: number | null;
}

export type InstallStatus =
  | "installing_and_restarting"
  | "installer_launched"
  | "permission_required"
  | "opened_browser";

export interface InstallUpdateOutcome {
  status: InstallStatus;
}

export type AppError =
  | { kind: "invalid_url"; message: string }
  | { kind: "unsupported_source"; host: string; message: string }
  | { kind: "timeout"; timeout_secs: number; message: string }
  | { kind: "dns_failure"; host: string; message: string }
  | { kind: "tls_failure"; host: string; message: string }
  | { kind: "rate_limited"; retry_after_secs: number | null; message: string }
  | { kind: "access_restricted"; host: string; status: number; message: string }
  | { kind: "api_failure"; service: string; status: number | null; message: string }
  | { kind: "http_error"; status: number; message: string }
  | { kind: "network"; message: string }
  | { kind: "blocked"; host: string; url: string }
  | { kind: "content_protected"; host: string; url: string; message: string }
  | { kind: "not_recognized"; message: string }
  | { kind: "missing_metadata"; message: string }
  | { kind: "missing_images"; message: string }
  | { kind: "invalid_image_format"; message: string }
  | { kind: "corrupted_data"; message: string }
  | { kind: "encrypted_archive"; message: string }
  | { kind: "unsupported_format"; message: string }
  | { kind: "not_found"; message: string }
  | { kind: "db"; message: string }
  | {
      kind: "source_failure";
      code: SourceErrorCode;
      message: string;
      report: SourceDiagnosticReport;
    }
  | { kind: "internal"; message: string };

export type TachibkMatchKind = "none" | "local_library" | "web_library" | "catalog_entry";

export interface TachibkCategoryPreview {
  name: string;
  order: number;
  mangaCount: number;
}

export interface TachibkMangaPreview {
  index: number;
  title: string;
  author: string | null;
  artist: string | null;
  description: string | null;
  genres: string[];
  categories: string[];
  status: string | null;
  coverToken: string | null;
  sourceName: string | null;
  favorite: boolean;
  totalChapters: number;
  readChapters: number;
  bookmarkedChapters: number;
  lastReadAt: number | null;
  dateAdded: number | null;
  trackingCount: number;
  matchedMangaId: number | null;
  matchedMangaTitle: string | null;
  matchKind: TachibkMatchKind;
}

export interface TachibkBackupPreview {
  sourcePath: string | null;
  uploadId: string | null;
  fileName: string;
  fileSize: number;
  totalManga: number;
  matchedLocalCount: number;
  matchedExistingCount: number;
  unmatchedCount: number;
  totalChapters: number;
  totalReadChapters: number;
  categories: TachibkCategoryPreview[];
  entries: TachibkMangaPreview[];
}

export interface TachibkImportRequest {
  sourcePath: string | null;
  uploadId: string | null;
  selectedIndices: number[];
  syncReadingProgress: boolean;
  importMetadataOnly: boolean;
  importCategories: boolean;
  importBookmarks: boolean;
}

export interface TachibkImportReport {
  linkedLocalCount: number;
  updatedExistingCount: number;
  createdCatalogCount: number;
  skippedCount: number;
  syncedChaptersRead: number;
  syncedBookmarks: number;
  syncedHistory: number;
  importedMangaIds: number[];
}

export interface BackupBlockSelection {
  manga: boolean;
  trackers: boolean;
  prefs: boolean;
}

export type BackupImportMode = "smart_merge" | "overwrite";

export interface BackupProgressEvent {
  stage: string;
  percent: number;
  current: number;
  total: number;
  detail: string;
}

export interface BackupExportResult {
  saved: boolean;
  cancelled: boolean;
  fileName: string;
  displayPath: string | null;
  fileSizeBytes: number;
  compressedBytes: number;
  uncompressedBytes: number;
  mangaCount: number;
  trackerCount: number;
  includedPrefs: boolean;
  timestampMs: number;
}

export interface BackupPreviewView {
  filePath: string;
  fileName: string;
  fileSizeBytes: number;
  compressedBytes: number;
  uncompressedBytes: number;
  formatVersion: number;
  timestampMs: number;
  appVersion: string;
  deviceName: string;
  checksumHex: string;
  mangaCount: number;
  interactedChapterCount: number;
  trackerCount: number;
  sourceCount: number;
  categoryCount: number;
  hasManga: boolean;
  hasTrackers: boolean;
  hasPrefs: boolean;
  isTempFile: boolean;
}

export interface BackupImportReport {
  mode: BackupImportMode;
  addedManga: number;
  mergedManga: number;
  mergedChapters: number;
  restoredBookmarks: number;
  restoredTrackers: number;
  restoredPrefs: boolean;
}

export interface SavedImageOutcome {
  savedPath: string;
  fileName: string;
  bytes: number;
}

export interface SetCoverOutcome {
  mangaId: number;
  coverToken: string;
}

export type DetectedLayout = "webtoon" | "manga" | "manual_override" | "undecided";
export type DetectionSignal = "override" | "metadata" | "dimensions" | "none";

export interface LayoutDetectionResult {
  layout: DetectedLayout;
  signal: DetectionSignal;
  decided: boolean;
  isWebtoon: boolean;
  medianRatio: number;
  sampledPages: number;
}

export type AiModelStatus = "ready" | "not_downloaded" | "downloading" | "verifying" | "error";

export interface AiModelEntryView {
  tier: TranslationModelTier;
  id: string;
  displayName: string;
  status: AiModelStatus;
  installedBytes: number;
  requiredBytes: number;
  ready: boolean;
  bundled: boolean;
  accelerator: string;
}

export interface AiNmtPackView {
  id: string;
  displayName: string;
  sourceLang: string;
  targetLang: string;
  status: AiModelStatus;
  installedBytes: number;
  requiredBytes: number;
  ready: boolean;
}

export interface AiStatusView {
  activeTier: TranslationModelTier;
  platform: string;
  storageDir: string;
  tier1Ready: boolean;
  tier2Supported: boolean;
  tier2Ready: boolean;
  downloading: boolean;
  vulkanSupported: boolean;
  loadedInMemory: boolean;
  activeJobs: number;
  models: AiModelEntryView[];
  nmtPacks: AiNmtPackView[];
}

export type AiDownloadStage =
  | "starting"
  | "downloading"
  | "verifying"
  | "extracting"
  | "completed"
  | "cancelled"
  | "failed";

export interface AiDownloadProgressEvent {
  stage: AiDownloadStage;
  assetId: string;
  fileIndex: number;
  fileCount: number;
  downloadedBytes: number;
  totalBytes: number;
  percent: number;
  speedBytesPerSec: number;
  message?: string | null;
}

export interface TranslatedRegionView {
  text: string;
  sourceText?: string | null;
  bbox: [number, number, number, number];
  confidence: number;
}

export interface PageAiResultView {
  pageKey: string;
  tierUsed: TranslationModelTier;
  targetLang: string;
  detectOnly: boolean;
  fromCache: boolean;
  cancelled: boolean;
  elapsedMs: number;
  imageWidth: number;
  imageHeight: number;
  regions: TranslatedRegionView[];
  missingNmtPacks?: AiNmtPackView[];
  error?: string | null;
}
