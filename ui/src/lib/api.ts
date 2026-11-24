import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getDict } from "./i18n";
import type {
  AppError,
  AppReleaseInfo,
  BookmarkItemView,
  BookmarkSort,
  BrowserAuthOutcome,
  CacheInfo,
  CacheMangaStats,
  CacheSourceStats,
  ChapterLoadPriority,
  ChapterView,
  CrashRecoverySession,
  DeviceEnvInfo,
  DiagnosticLogEntry,
  DohConfig,
  DohTestResult,
  FormattedAppError,
  FoundUpdate,
  HiddenMangaView,
  HideAction,
  HistoryItemView,
  InstallUpdateOutcome,
  Language,
  LastReadingDestination,
  MangaView,
  NetworkProviderInfo,
  NewDiagnosticLog,
  NotificationItemView,
  NotificationPermissionInfo,
  OrientationLockMode,
  OrientationLockReply,
  ProxyEndpoint,
  ProxyTestResult,
  ReaderModeOverride,
  ReaderSettings,
  ResolveResponse,
  SavedDiagnosticFile,
  Settings,
  SettingsSection,
  SyncDecision,
  SyncSenderMode,
  SyncStatusView,
  UpdateDownloadProgress,
  UpdateProgress,
  UpdateSummary,
} from "./types";

/**
 * Converts a backend-signed image token into the Tauri custom protocol URL.
 * On Windows WebView2 and Android WebView, custom schemes map to `http://<scheme>.localhost/`.
 */
export function imgProxyUrl(token: string, retry = false): string {
  const usesHttpLocalhost = /windows|android/i.test(navigator.userAgent);
  const base = usesHttpLocalhost ? `http://img.localhost/${token}` : `img://localhost/${token}`;
  return retry ? `${base}?retry=1` : base;
}

export function formatAppError(
  err: unknown,
  lang: Language = "en"
): FormattedAppError {
  const t = getDict(lang);
  const isVi = lang === "vi";
  if (typeof err === "object" && err !== null && "kind" in err) {
    const e = err as AppError;
    switch (e.kind) {
      case "source_failure": {
        const rep = e.report;
        const targetUrl = rep.finalUrl || rep.originalUrl;
        const title =
          (isVi ? rep.titleVi : rep.titleEn) ?? rep.titleEn ?? rep.titleVi ?? t.errGenericTitle;
        const detail =
          (isVi ? rep.explanationVi : rep.explanationEn) ??
          rep.explanationEn ??
          rep.explanationVi ??
          e.message;
        const suggestedAction =
          (isVi ? rep.suggestedActionVi : rep.suggestedActionEn) ?? undefined;
        const blockedUrl =
          e.code === "VERIFICATION_REQUIRED" ||
          rep.recoveryActions.includes("complete_verification")
            ? targetUrl
            : undefined;
        const loginUrl =
          e.code === "LOGIN_REQUIRED" ||
          e.code === "SESSION_EXPIRED" ||
          rep.recoveryActions.includes("sign_in_via_browser")
            ? targetUrl
            : undefined;
        const externalUrl = rep.browserReadingAvailable ? targetUrl : undefined;
        return {
          title,
          detail,
          blockedUrl,
          loginUrl,
          externalUrl,
          code: e.code,
          failureGroup: rep.failureGroup ?? undefined,
          suggestedAction,
          retryable: rep.retryable,
          userInteractionRequired: rep.userInteractionRequired,
          browserReadingAvailable: rep.browserReadingAvailable,
          report: rep,
        };
      }
      case "invalid_url":
        return {
          title: t.errInvalidUrlTitle,
          detail: `${t.errInvalidUrlDetail} (${e.message})`,
          code: "INVALID_URL",
          failureGroup: "source_detection",
        };
      case "unsupported_source":
        return {
          title: t.errUnsupportedSourceTitle(e.host),
          detail: e.message,
          code: "UNSUPPORTED_WEBSITE",
          failureGroup: "source_detection",
        };
      case "timeout":
        return {
          title: t.errTimeoutTitle,
          detail: e.message,
          code: "NETWORK_TIMEOUT",
          failureGroup: "network",
          retryable: true,
        };
      case "dns_failure":
        return {
          title: t.errDnsFailureTitle(e.host),
          detail: e.message,
          failureGroup: "network",
          retryable: true,
        };
      case "tls_failure":
        return {
          title: t.errTlsFailureTitle(e.host),
          detail: e.message,
          failureGroup: "network",
          retryable: true,
        };
      case "rate_limited":
        return {
          title: t.errRateLimitedTitle,
          detail: e.message,
          code: "RATE_LIMITED",
          failureGroup: "network",
          retryable: true,
        };
      case "access_restricted":
        return {
          title: t.errAccessRestrictedTitle(e.host, e.status),
          detail: e.message,
          code: e.status === 401 ? "LOGIN_REQUIRED" : "ACCESS_DENIED",
          failureGroup: "access_permission",
        };
      case "api_failure":
        return {
          title: t.errApiFailureTitle(e.service),
          detail: e.message,
          code: "UNSUPPORTED_API_FORMAT",
          failureGroup: "unsupported_format",
        };
      case "http_error":
        return {
          title: t.errHttpStatusTitle(e.status),
          detail: e.message,
          code: e.status >= 500 ? "SERVER_ERROR" : "UNKNOWN_SOURCE_ERROR",
          failureGroup: "network",
        };
      case "blocked":
        return {
          title: t.errBlockedTitle(e.host),
          detail: t.errBlockedDetail,
          blockedUrl: e.url,
          externalUrl: e.url,
          code: "VERIFICATION_REQUIRED",
          failureGroup: "access_permission",
          userInteractionRequired: true,
          browserReadingAvailable: true,
        };
      case "content_protected":
        return {
          title: t.errContentProtectedTitle(e.host),
          detail: t.errContentProtectedDetail,
          externalUrl: e.url,
          code: "DRM_PROTECTED_CONTENT",
          failureGroup: "access_permission",
          userInteractionRequired: true,
          browserReadingAvailable: true,
        };
      case "missing_metadata":
        return {
          title: t.errMissingMetadataTitle,
          detail: e.message,
          code: "SOURCE_DETECTION_FAILED",
          failureGroup: "source_detection",
        };
      case "not_recognized":
        return {
          title: t.errNotRecognizedTitle,
          detail: `${t.errNotRecognizedDetail} (${e.message})`,
          code: "SOURCE_DETECTION_FAILED",
          failureGroup: "source_detection",
        };
      case "missing_images":
        return {
          title: t.errMissingImagesTitle,
          detail: e.message,
          code: "CHAPTER_CONTENT_NOT_LOADED",
          failureGroup: "image_extraction",
          retryable: true,
        };
      case "invalid_image_format":
        return {
          title: t.errInvalidImageTitle,
          detail: e.message,
          code: "UNSUPPORTED_IMAGE_FORMAT",
          failureGroup: "unsupported_format",
        };
      case "corrupted_data":
        return {
          title: t.errCorruptedDataTitle,
          detail: e.message,
          code: "IMAGE_DECODE_FAILED",
          failureGroup: "image_extraction",
          retryable: true,
        };
      case "network":
        return {
          title: t.errNetworkTitle,
          detail: e.message,
          failureGroup: "network",
          retryable: true,
        };
      case "not_found":
        return {
          title: t.errNotFoundTitle,
          detail: e.message,
          code: "CHAPTER_NOT_FOUND",
          failureGroup: "chapter_identification",
        };
      case "db":
      case "internal":
        return {
          title: t.errAppTitle,
          detail: e.message,
        };
    }
  }
  return {
    title: t.errGenericTitle,
    detail: String(err),
  };
}

export const api = {
  resolveUrl: (url: string) => invoke<ResolveResponse>("resolve_url", { url }),
  getManga: (id: number) => invoke<MangaView>("get_manga", { id }),
  refreshManga: (id: number) => invoke<MangaView>("refresh_manga", { id }),
  openManualVerification: (url: string) =>
    invoke<void>("open_manual_verification", { url }),
  openBrowserLoginSession: (url: string) =>
    invoke<BrowserAuthOutcome>("open_browser_login_session", { url }),
  getChapter: (url: string, mangaId?: number | null, priority?: ChapterLoadPriority) =>
    invoke<ChapterView>("get_chapter", { url, mangaId: mangaId ?? null, priority: priority ?? "primary" }),
  saveProgress: (
    mangaId: number,
    chapterUrl: string,
    chapterName: string,
    scrollRatio: number,
    pageIndex = 0
  ) =>
    invoke<void>("save_progress", {
      mangaId,
      chapterUrl,
      chapterName,
      scrollRatio,
      pageIndex,
    }),
  getLastReadingDestination: () =>
    invoke<LastReadingDestination | null>("get_last_reading_destination"),
  markRead: (mangaId: number, chapterUrl: string) =>
    invoke<void>("mark_read", { mangaId, chapterUrl }),
  listHistory: () => invoke<HistoryItemView[]>("list_history"),
  removeHistory: (mangaId: number) => invoke<void>("remove_history", { mangaId }),
  clearHistory: () => invoke<void>("clear_history"),

  toggleBookmark: (mangaId: number) => invoke<boolean>("toggle_bookmark", { mangaId }),
  listBookmarks: (sort: BookmarkSort = "newest") =>
    invoke<BookmarkItemView[]>("list_bookmarks", { sort }),
  setMangaHidden: (mangaId: number, action: HideAction) =>
    invoke<boolean>("set_manga_hidden", { mangaId, action }),
  listHiddenManga: () => invoke<HiddenMangaView[]>("list_hidden_manga"),
  deleteMangaPermanently: (mangaId: number) =>
    invoke<void>("delete_manga_permanently", { mangaId }),
  setMangaReaderOverride: (mangaId: number, overrideMode: ReaderModeOverride) =>
    invoke<void>("set_manga_reader_override", { mangaId, overrideMode }),
  setMangaReaderSettings: (mangaId: number, readerSettings: ReaderSettings | null) =>
    invoke<ReaderSettings | null>("set_manga_reader_settings", { mangaId, readerSettings }),
  markMangaDetectedWebtoon: (mangaId: number) =>
    invoke<void>("mark_manga_detected_webtoon", { mangaId }),
  readClipboardText: () => invoke<string | null>("read_clipboard_text"),
  openExternalUrl: (url: string) => invoke<void>("open_external_url", { url }),

  listNotifications: () => invoke<NotificationItemView[]>("list_notifications"),
  markNotificationRead: (id: number) => invoke<void>("mark_notification_read", { id }),
  markAllNotificationsRead: () => invoke<void>("mark_all_notifications_read"),
  deleteNotification: (id: number) => invoke<void>("delete_notification", { id }),
  clearNotifications: () => invoke<void>("clear_notifications"),

  checkUpdatesNow: () => invoke<UpdateSummary>("check_updates_now"),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
  resetSettingsSection: (section: SettingsSection) =>
    invoke<Settings>("reset_settings_section", { section }),
  resetAllSettings: () => invoke<Settings>("reset_all_settings"),
  getPlatform: () => invoke<"windows" | "android" | "other">("get_platform"),
  getDeviceEnv: () => invoke<DeviceEnvInfo>("get_device_env"),
  setOrientationLock: (mode: OrientationLockMode) =>
    invoke<OrientationLockReply>("set_orientation_lock", { mode }),
  setImmersive: (enabled: boolean) => invoke<void>("set_immersive", { enabled }),
  setKeepScreenOn: (enabled: boolean) => invoke<void>("set_keep_screen_on", { enabled }),
  checkNotificationPermission: () =>
    invoke<NotificationPermissionInfo>("check_notification_permission"),
  requestNotificationPermission: () =>
    invoke<NotificationPermissionInfo>("request_notification_permission"),

  upsertReaderSession: (
    mangaId: number,
    chapterUrl: string,
    chapterName: string,
    scrollRatio: number,
    pageIndex: number,
    readingMode: string
  ) =>
    invoke<void>("upsert_reader_session", {
      mangaId,
      chapterUrl,
      chapterName,
      scrollRatio,
      pageIndex,
      readingMode,
    }),
  clearReaderSession: () => invoke<void>("clear_reader_session"),
  getCrashRecoverySession: () =>
    invoke<CrashRecoverySession | null>("get_crash_recovery_session"),

  logClientError: (entry: NewDiagnosticLog) => invoke<number>("log_client_error", { entry }),
  listDiagnosticLogs: (limit = 100) =>
    invoke<DiagnosticLogEntry[]>("list_diagnostic_logs", { limit }),
  clearDiagnosticLogs: () => invoke<void>("clear_diagnostic_logs"),
  exportDiagnosticLogs: () => invoke<string>("export_diagnostic_logs"),
  saveDiagnosticLogsFile: () => invoke<SavedDiagnosticFile>("save_diagnostic_logs_file"),

  getNetworkProviders: () => invoke<NetworkProviderInfo[]>("get_network_providers"),
  testDohProvider: (config: DohConfig) => invoke<DohTestResult>("test_doh_provider", { config }),
  testProxy: (endpoint: ProxyEndpoint, testUrl?: string | null) =>
    invoke<ProxyTestResult>("test_proxy", { endpoint, testUrl: testUrl ?? null }),

  syncGetStatus: () => invoke<SyncStatusView>("sync_get_status"),
  syncStartSender: (mode: SyncSenderMode) => invoke<SyncStatusView>("sync_start_sender", { mode }),
  syncCancel: () => invoke<SyncStatusView>("sync_cancel"),
  syncHostRespond: (decision: SyncDecision) =>
    invoke<SyncStatusView>("sync_host_respond", { decision }),
  syncConnectQr: (qrPayload: string) =>
    invoke<SyncStatusView>("sync_connect_qr", { qrPayload }),
  syncConnectPin: (pin: string, manualAddr?: string | null) =>
    invoke<SyncStatusView>("sync_connect_pin", { pin, manualAddr: manualAddr ?? null }),
  syncReceiverConfirm: (decision: SyncDecision) =>
    invoke<SyncStatusView>("sync_receiver_confirm", { decision }),

  getAppVersion: () => invoke<string>("get_app_version"),
  getCachedAppRelease: () => invoke<AppReleaseInfo | null>("get_cached_app_release"),
  checkAppRelease: () => invoke<AppReleaseInfo>("check_app_release"),
  openReleaseUrl: (url: string) => invoke<void>("open_release_url", { url }),
  downloadAndInstallUpdate: () => invoke<InstallUpdateOutcome>("download_and_install_update"),
  cacheInfo: () => invoke<CacheInfo>("cache_info"),
  listCacheBySource: () => invoke<CacheSourceStats[]>("list_cache_by_source"),
  listCacheByManga: () => invoke<CacheMangaStats[]>("list_cache_by_manga"),
  clearCacheBySource: (site: string) => invoke<CacheInfo>("clear_cache_by_source", { site }),
  clearCacheByManga: (mangaId: number) => invoke<CacheInfo>("clear_cache_by_manga", { mangaId }),
  purgeChapterPageCache: (mangaId: number, chapterUrl: string) =>
    invoke<CacheInfo>("purge_chapter_page_cache", { mangaId, chapterUrl }),
  purgeMangaPageCache: (mangaId: number) =>
    invoke<CacheInfo>("purge_manga_page_cache", { mangaId }),
  purgeAllPageCache: () => invoke<CacheInfo>("purge_all_page_cache"),
  clearImageCache: () => invoke<CacheInfo>("clear_image_cache"),

  onUpdatesFound: (cb: (found: FoundUpdate[]) => void): Promise<UnlistenFn> =>
    listen<FoundUpdate[]>("updates://found", (e) => cb(e.payload)),
  onUpdatesProgress: (cb: (p: UpdateProgress) => void): Promise<UnlistenFn> =>
    listen<UpdateProgress>("updates://progress", (e) => cb(e.payload)),
  onNotificationsUpdated: (cb: (count: number) => void): Promise<UnlistenFn> =>
    listen<number>("notifications://updated", (e) => cb(e.payload)),
  onSettingsChanged: (cb: (s: Settings) => void): Promise<UnlistenFn> =>
    listen<Settings>("settings://changed", (e) => cb(e.payload)),
  onAppReleaseAvailable: (cb: (info: AppReleaseInfo) => void): Promise<UnlistenFn> =>
    listen<AppReleaseInfo>("app://release-available", (e) => cb(e.payload)),
  onUpdateDownloadProgress: (cb: (p: UpdateDownloadProgress) => void): Promise<UnlistenFn> =>
    listen<UpdateDownloadProgress>("app://update-download-progress", (e) => cb(e.payload)),
  onSyncStateChanged: (cb: (s: SyncStatusView) => void): Promise<UnlistenFn> =>
    listen<SyncStatusView>("sync://state-changed", (e) => cb(e.payload)),
};
