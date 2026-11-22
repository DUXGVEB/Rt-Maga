import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppError,
  CacheInfo,
  ChapterView,
  FoundUpdate,
  HistoryItemView,
  MangaView,
  ResolveResponse,
  Settings,
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

export function formatAppError(err: unknown): {
  title: string;
  detail: string;
  blockedUrl?: string;
} {
  if (typeof err === "object" && err !== null && "kind" in err) {
    const e = err as AppError;
    switch (e.kind) {
      case "invalid_url":
        return {
          title: "Đường dẫn không hợp lệ",
          detail: "Hãy dán link truyện hoặc link chương hợp lệ (bắt đầu bằng https://...).",
        };
      case "blocked":
        return {
          title: `Trang ${e.host} yêu cầu xác minh`,
          detail:
            "Trang web đang bật kiểm tra chống bot (Cloudflare). Bạn có thể mở cửa sổ để tự xác minh rồi thử lại.",
          blockedUrl: e.url,
        };
      case "not_recognized":
        return {
          title: "Chưa nhận diện được truyện từ link này",
          detail:
            "Hãy thử dán trực tiếp link trang truyện hoặc trang chương, hoặc bật chế độ trình duyệt chạy ngầm (Headless) trong Cài đặt.",
        };
      case "network":
        return {
          title: "Không kết nối được tới trang truyện",
          detail: e.message,
        };
      case "not_found":
        return {
          title: "Không tìm thấy dữ liệu",
          detail: e.message,
        };
      case "db":
      case "internal":
        return {
          title: "Lỗi ứng dụng",
          detail: e.message,
        };
    }
  }
  return {
    title: "Đã xảy ra lỗi",
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
  cacheInfo: () => invoke<CacheInfo>("cache_info"),
  clearImageCache: () => invoke<CacheInfo>("clear_image_cache"),

  onUpdatesFound: (cb: (found: FoundUpdate[]) => void): Promise<UnlistenFn> =>
    listen<FoundUpdate[]>("updates://found", (e) => cb(e.payload)),
  onUpdatesProgress: (cb: (p: UpdateProgress) => void): Promise<UnlistenFn> =>
    listen<UpdateProgress>("updates://progress", (e) => cb(e.payload)),
  onSettingsChanged: (cb: (s: Settings) => void): Promise<UnlistenFn> =>
    listen<Settings>("settings://changed", (e) => cb(e.payload)),
};
