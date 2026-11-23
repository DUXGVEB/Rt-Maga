export type Language = "en" | "vi" | "zh" | "ja" | "ru" | "ko" | "th";

export type Theme = "system" | "light" | "dark";

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

export type SettingsSection =
  | "appearance"
  | "device"
  | "reader"
  | "floating"
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
  device: DeviceSettings;
  reader: ReaderSettings;
  floating: FloatingButtonsSettings;
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
  | "UNSUPPORTED_URL"
  | "INVALID_URL"
  | "BLOCKED_OR_RESTRICTED";

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
  | "VERIFICATION_REQUIRED";

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
  detectedWebtoon: boolean;
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

export interface CatalogEntryView {
  url: string;
  title: string;
  altTitles: string[];
  coverToken: string | null;
  latestChapter: string | null;
  status: string | null;
  genres: string[];
  sourceId: string | null;
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
  latestChapterUrl: string | null;
  latestChapterName: string | null;
  hasNew: boolean;
  bookmarked: boolean;
  totalChapters: number;
  unreadCount: number;
  isCompleted: boolean;
  currentChapterIdx: number | null;
}

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
  | { kind: "not_recognized"; message: string }
  | { kind: "missing_metadata"; message: string }
  | { kind: "missing_images"; message: string }
  | { kind: "invalid_image_format"; message: string }
  | { kind: "corrupted_data"; message: string }
  | { kind: "not_found"; message: string }
  | { kind: "db"; message: string }
  | { kind: "internal"; message: string };
