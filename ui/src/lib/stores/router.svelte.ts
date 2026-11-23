import { api } from "../api";
import { settingsStore } from "./settings.svelte";
import { toasts } from "./toast.svelte";

export type Route =
  | { name: "home" }
  | { name: "bookmarks" }
  | { name: "manga"; id: number }
  | {
      name: "reader";
      mangaId: number;
      chapterUrl: string;
      scrollRatio?: number;
      pageIndex?: number;
    }
  | { name: "history" }
  | { name: "settings" };

type BackInterceptor = () => boolean;

const MAX_STACK_DEPTH = 32;
const DOUBLE_BACK_EXIT_WINDOW_MS = 2000;

function routesEqual(a: Route, b: Route): boolean {
  if (a.name !== b.name) return false;
  if (a.name === "manga" && b.name === "manga") return a.id === b.id;
  if (a.name === "reader" && b.name === "reader") {
    return a.mangaId === b.mangaId && a.chapterUrl === b.chapterUrl;
  }
  return true;
}

class RouterStore {
  current = $state<Route>({ name: "home" });
  private stack: Route[] = [];
  private backInterceptors: BackInterceptor[] = [];
  private lastRootBackAt = 0;
  private backBridgeInstalled = false;

  installBackBridge() {
    if (this.backBridgeInstalled || typeof window === "undefined") return;
    this.backBridgeInstalled = true;
    (window as unknown as { __rtmagaBackPress?: () => boolean }).__rtmagaBackPress = () =>
      this.handleSystemBackPress();
  }

  /**
   * Registers a LIFO interceptor for modals, drawers, and popovers.
   * Return `true` if the interceptor consumed the Back event.
   */
  registerBackInterceptor(fn: BackInterceptor): () => void {
    this.backInterceptors.push(fn);
    return () => {
      const idx = this.backInterceptors.lastIndexOf(fn);
      if (idx >= 0) this.backInterceptors.splice(idx, 1);
    };
  }

  private pushAndSet(next: Route, replace = false) {
    if (routesEqual(this.current, next)) {
      this.current = next;
      return;
    }
    if (!replace) {
      this.stack.push(this.current);
      if (this.stack.length > MAX_STACK_DEPTH) {
        this.stack.shift();
      }
    }
    this.current = next;
  }

  goHome() {
    this.stack = [];
    this.current = { name: "home" };
  }

  goBookmarks() {
    this.pushAndSet({ name: "bookmarks" });
  }

  goManga(id: number) {
    this.pushAndSet({ name: "manga", id });
  }

  goReader(
    mangaId: number,
    chapterUrl: string,
    scrollRatio?: number,
    pageIndex?: number,
    replace = false
  ) {
    // Switching chapters while already inside Reader replaces the top reader entry so Back
    // always returns to the Manga / History / Bookmarks screen cleanly instead of looping chapters.
    const shouldReplace = replace || (this.current.name === "reader" && this.current.mangaId === mangaId);
    this.pushAndSet(
      { name: "reader", mangaId, chapterUrl, scrollRatio, pageIndex },
      shouldReplace
    );
  }

  goHistory() {
    this.pushAndSet({ name: "history" });
  }

  /**
   * History nav button behavior:
   * - If on another screen -> open History.
   * - If ALREADY on History -> resume the last persisted reading destination cleanly.
   */
  async goHistoryOrResume() {
    if (this.current.name !== "history") {
      this.goHistory();
      return;
    }

    try {
      const dest = await api.getLastReadingDestination();
      if (dest) {
        this.goReader(dest.mangaId, dest.chapterUrl, dest.scrollRatio, dest.pageIndex);
        return;
      }
    } catch {
      // Fallback toast below.
    }
    toasts.push(settingsStore.t.historyNoResumeTarget, "info", 2200);
  }

  goSettings() {
    this.pushAndSet({ name: "settings" });
  }

  /**
   * Navigates one step back in the in-app stack.
   * Returns `true` if navigation occurred, or `false` if already at root Home.
   */
  goBack(): boolean {
    if (this.stack.length > 0) {
      const prev = this.stack.pop()!;
      this.current = prev;
      return true;
    }
    if (this.current.name === "reader") {
      this.current = { name: "manga", id: this.current.mangaId };
      return true;
    }
    if (this.current.name !== "home") {
      this.current = { name: "home" };
      return true;
    }
    return false;
  }

  /**
   * Called by Android `MainActivity.kt` OnBackPressedCallback.
   * Returns `true` when consumed in-app; `false` allows Android to exit/background the app.
   */
  handleSystemBackPress(): boolean {
    // 1. Let top-most modal/drawer/quick-settings consume Back first.
    for (let i = this.backInterceptors.length - 1; i >= 0; i -= 1) {
      try {
        if (this.backInterceptors[i]()) {
          return true;
        }
      } catch {
        // Continue checking remaining interceptors.
      }
    }

    // 2. Navigate back inside the app if not on root Home.
    if (this.goBack()) {
      this.lastRootBackAt = 0;
      return true;
    }

    // 3. Root Home: require double-back within 2 seconds to prevent accidental exit.
    const now = Date.now();
    if (now - this.lastRootBackAt <= DOUBLE_BACK_EXIT_WINDOW_MS) {
      this.lastRootBackAt = 0;
      return false;
    }
    this.lastRootBackAt = now;
    toasts.push(settingsStore.t.pressBackAgainToExit, "info", DOUBLE_BACK_EXIT_WINDOW_MS);
    return true;
  }
}

export const router = new RouterStore();
