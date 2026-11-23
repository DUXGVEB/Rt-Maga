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

export interface VerticalModeSettings {
  layoutMode: VerticalLayoutMode;
  pagesPerGroup: number;
  pageGapPx: number;
  groupGapPx: number;
  snapEnabled: boolean;
  alignment: GroupAlignment;
  dimUnfocused: boolean;
  dimLevel: number;
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

export interface ChapterRefView {
  url: string;
  name: string;
  number: number | null;
  idx: number;
  read: boolean;
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
  readerModeOverride: ReaderModeOverride;
  detectedWebtoon: boolean;
  latestChapterName: string | null;
  chapters: ChapterRefView[];
  progress: ProgressView | null;
}

export interface ResolveResponse {
  manga: MangaView;
  startAt: ChapterRefView | null;
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
  | { kind: "network"; message: string }
  | { kind: "blocked"; host: string; url: string }
  | { kind: "not_recognized"; message: string }
  | { kind: "not_found"; message: string }
  | { kind: "db"; message: string }
  | { kind: "internal"; message: string };
