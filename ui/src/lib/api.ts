import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getDict } from "./i18n";
import type {
  AppError,
  AppReleaseInfo,
  BookmarkItemView,
  BookmarkSort,
  CacheInfo,
  CacheMangaStats,
  CacheSourceStats,
  ChapterView,
  CrashRecoverySession,
  DeviceEnvInfo,
  DiagnosticLogEntry,
  DohConfig,
  DohTestResult,
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
  ResolveResponse,
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
): {
  title: string;
  detail: string;
  blockedUrl?: string;
} {
  const t = getDict(lang);
  if (typeof err === "object" && err !== null && "kind" in err) {
    const e = err as AppError;
    switch (e.kind) {
      case "invalid_url":
        return {
          title: t.errInvalidUrlTitle,
          detail: `${t.errInvalidUrlDetail} (${e.message})`,
        };
      case "unsupported_source":
        return {
          title: t.errUnsupportedSourceTitle(e.host),
          detail: e.message,
        };
      case "timeout":
        return {
          title: t.errTimeoutTitle,
          detail: e.message,
        };
      case "http_error":
        return {
          title: t.errHttpStatusTitle(e.status),
          detail: e.message,
        };
      case "blocked":
        return {
          title: t.errBlockedTitle(e.host),
          detail: t.errBlockedDetail,
          blockedUrl: e.url,
        };
      case "not_recognized":
        return {
          title: t.errNotRecognizedTitle,
          detail: `${t.errNotRecognizedDetail} (${e.message})`,
        };
      case "missing_images":
        return {
          title: t.errMissingImagesTitle,
          detail: e.message,
        };
      case "invalid_image_format":
        return {
          title: t.errInvalidImageTitle,
          detail: e.message,
        };
      case "corrupted_data":
        return {
          title: t.errCorruptedDataTitle,
          detail: e.message,
        };
      case "network":
        return {
          title: t.errNetworkTitle,
          detail: e.message,
        };
      case "not_found":
        return {
          title: t.errNotFoundTitle,
          detail: e.message,
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
  getChapter: (url: string, mangaId?: number | null) =>
    invoke<ChapterView>("get_chapter", { url, mangaId: mangaId ?? null }),
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
  markMangaDetectedWebtoon: (mangaId: number) =>
    invoke<void>("mark_manga_detected_webtoon", { mangaId }),

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
