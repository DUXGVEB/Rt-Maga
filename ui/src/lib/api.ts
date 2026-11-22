import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getDict } from "./i18n";
import type {
  AppError,
  AppReleaseInfo,
  CacheInfo,
  ChapterView,
  FoundUpdate,
  HistoryItemView,
  InstallUpdateOutcome,
  Language,
  MangaView,
  ProxyEndpoint,
  ProxyTestResult,
  ResolveResponse,
  Settings,
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
          detail: t.errInvalidUrlDetail,
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
          detail: t.errNotRecognizedDetail,
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
    scrollRatio: number
  ) =>
    invoke<void>("save_progress", {
      mangaId,
      chapterUrl,
      chapterName,
      scrollRatio,
    }),
  markRead: (mangaId: number, chapterUrl: string) =>
    invoke<void>("mark_read", { mangaId, chapterUrl }),
  listHistory: () => invoke<HistoryItemView[]>("list_history"),
  removeHistory: (mangaId: number) => invoke<void>("remove_history", { mangaId }),
  clearHistory: () => invoke<void>("clear_history"),
  checkUpdatesNow: () => invoke<UpdateSummary>("check_updates_now"),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
  getPlatform: () => invoke<"windows" | "android" | "other">("get_platform"),
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
  clearImageCache: () => invoke<CacheInfo>("clear_image_cache"),

  onUpdatesFound: (cb: (found: FoundUpdate[]) => void): Promise<UnlistenFn> =>
    listen<FoundUpdate[]>("updates://found", (e) => cb(e.payload)),
  onUpdatesProgress: (cb: (p: UpdateProgress) => void): Promise<UnlistenFn> =>
    listen<UpdateProgress>("updates://progress", (e) => cb(e.payload)),
  onSettingsChanged: (cb: (s: Settings) => void): Promise<UnlistenFn> =>
    listen<Settings>("settings://changed", (e) => cb(e.payload)),
  onAppReleaseAvailable: (cb: (info: AppReleaseInfo) => void): Promise<UnlistenFn> =>
    listen<AppReleaseInfo>("app://release-available", (e) => cb(e.payload)),
  onUpdateDownloadProgress: (cb: (p: UpdateDownloadProgress) => void): Promise<UnlistenFn> =>
    listen<UpdateDownloadProgress>("app://update-download-progress", (e) => cb(e.payload)),
  onSyncStateChanged: (cb: (s: SyncStatusView) => void): Promise<UnlistenFn> =>
    listen<SyncStatusView>("sync://state-changed", (e) => cb(e.payload)),
};
