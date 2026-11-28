import { api } from "../api";
import type { CatalogPageView, SettingsSection } from "../types";
import { notificationsStore } from "./notifications.svelte";
import { randomMangaStore } from "./randomManga.svelte";
import { settingsStore } from "./settings.svelte";
import { toasts } from "./toast.svelte";

export type SettingsRouteSection = SettingsSection | "accessibility";

export type SettingsSubTarget = "supplemental" | "feedback" | "hidden";

export type Route =
  | { name: "home" }
  | { name: "sources"; initialQuery?: string }
  | {
      name: "browse";
      url: string;
      sourceId?: number;
      initialCatalog?: CatalogPageView;
      initialTag?: string;
    }
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
  | { name: "settings"; section?: SettingsRouteSection; subTarget?: SettingsSubTarget };

type BackInterceptor = () => boolean;

const MAX_STACK_DEPTH = 32;
const DOUBLE_BACK_EXIT_WINDOW_MS = 2000;

function routesEqual(a: Route, b: Route): boolean {
  if (a.name !== b.name) return false;
  if (a.name === "sources" && b.name === "sources") return a.initialQuery === b.initialQuery;
  if (a.name === "browse" && b.name === "browse") {
    return a.url === b.url && a.sourceId === b.sourceId && a.initialTag === b.initialTag;
  }
  if (a.name === "manga" && b.name === "manga") return a.id === b.id;
  if (a.name === "reader" && b.name === "reader") {
    return a.mangaId === b.mangaId && a.chapterUrl === b.chapterUrl;
  }
  if (a.name === "settings" && b.name === "settings") {
    return a.section === b.section && a.subTarget === b.subTarget;
  }
  return true;
}

class RouterStore {
  current = $state<Route>({ name: "home" });
  /** Incremented whenever a primary nav button (especially Home) is clicked so active screens can scroll-to-top/reset cleanly. */
  navPulse = $state(0);
  private stack: Route[] = [];
  private backInterceptors: BackInterceptor[] = [];
  private lastRootBackAt = 0;
  private backBridgeInstalled = false;
  private historyResumeBusy = false;

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

  private dismissOverlays() {
    if (notificationsStore.drawerOpen) {
      notificationsStore.closeDrawer();
    }
    if (randomMangaStore.isOpen) {
      randomMangaStore.close();
    }
  }

  private pushAndSet(next: Route, replace = false) {
    this.dismissOverlays();
    if (routesEqual(this.current, next)) {
      this.current = next;
      this.navPulse += 1;
      return;
    }
    if (!replace) {
      this.stack.push(this.current);
      if (this.stack.length > MAX_STACK_DEPTH) {
        this.stack.shift();
      }
    }
    this.current = next;
    this.navPulse += 1;
  }

  goHome() {
    this.dismissOverlays();
    this.stack = [];
    this.current = { name: "home" };
    this.navPulse += 1;
  }

  goSources(initialQuery?: string) {
    this.pushAndSet({ name: "sources", initialQuery });
  }

  goBrowse(
    url: string,
    initialCatalog?: CatalogPageView,
    replace = false,
    sourceId?: number,
    initialTag?: string
  ) {
    this.pushAndSet({ name: "browse", url, sourceId, initialCatalog, initialTag }, replace);
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
    const shouldReplace =
      replace || (this.current.name === "reader" && this.current.mangaId === mangaId);
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
    this.dismissOverlays();
    if (this.current.name !== "history") {
      this.goHistory();
      return;
    }
    if (this.historyResumeBusy) return;
    this.historyResumeBusy = true;
    try {
      const dest = await api.getLastReadingDestination();
      if (dest) {
        this.goReader(dest.mangaId, dest.chapterUrl, dest.scrollRatio, dest.pageIndex);
        return;
      }
      toasts.push(settingsStore.t.historyNoResumeTarget, "info", 2200);
    } catch {
      toasts.push(settingsStore.t.historyNoResumeTarget, "info", 2200);
    } finally {
      this.historyResumeBusy = false;
    }
  }

  updateCurrentReaderPosition(chapterUrl: string, scrollRatio: number, pageIndex: number) {
    if (this.current.name === "reader") {
      this.current = {
        ...this.current,
        chapterUrl,
        scrollRatio,
        pageIndex,
      };
    }
  }

  goSettings(section?: SettingsRouteSection, subTarget?: SettingsSubTarget) {
    this.pushAndSet({ name: "settings", section, subTarget });
  }

  /**
   * Stamps the active Settings route with an exact section/subTarget before navigating deeper
   * (e.g. opening a hidden manga from Storage -> Hidden so Back returns directly to Hidden).
   */
  stampSettingsLocation(section?: SettingsRouteSection, subTarget?: SettingsSubTarget) {
    if (this.current.name === "settings") {
      this.current = { name: "settings", section, subTarget };
    }
  }

  /**
   * Navigates one step back in the in-app stack.
   * Returns `true` if navigation occurred, or `false` if already at root Home.
   */
  goBack(): boolean {
    if (this.stack.length > 0) {
      const prev = this.stack.pop()!;
      this.current = prev;
      this.navPulse += 1;
      return true;
    }
    if (this.current.name === "reader") {
      this.current = { name: "manga", id: this.current.mangaId };
      this.navPulse += 1;
      return true;
    }
    if (this.current.name !== "home") {
      this.current = { name: "home" };
      this.navPulse += 1;
      return true;
    }
    return false;
  }

  /**
   * Called by Android `MainActivity.kt` OnBackPressedCallback.
   * Returns `true` when consumed in-app; `false` allows Android to exit/background the app.
   */
  handleSystemBackPress(): boolean {
    for (let i = this.backInterceptors.length - 1; i >= 0; i -= 1) {
      try {
        if (this.backInterceptors[i]()) {
          return true;
        }
      } catch {
        // Continue checking remaining interceptors.
      }
    }

    if (this.goBack()) {
      this.lastRootBackAt = 0;
      return true;
    }

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
