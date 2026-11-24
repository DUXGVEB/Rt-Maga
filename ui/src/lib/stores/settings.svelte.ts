import { api } from "../api";
import { getDict, getLocale } from "../i18n";
import {
  computeAccentTokens,
  DEFAULT_DARK_ACCENT,
  DEFAULT_LIGHT_ACCENT,
  DEFAULT_NAV_ORDER,
  normalizeHexColor,
  normalizeNavOrder,
} from "../themeAccent";
import type { FloatingScreenKey, Settings, SettingsSection, Theme } from "../types";
import { toasts } from "./toast.svelte";

const ALL_FLOATING_SCREENS: FloatingScreenKey[] = [
  "home",
  "browse",
  "bookmarks",
  "history",
  "manga",
  "reader",
  "settings",
];

export const DEFAULT_SETTINGS: Settings = {
  language: "en",
  theme: "system",
  ui: {
    lightAccent: DEFAULT_LIGHT_ACCENT,
    darkAccent: DEFAULT_DARK_ACCENT,
    navOrder: [...DEFAULT_NAV_ORDER],
  },
  device: {
    customName: null,
  },
  reader: {
    orientation: "vertical",
    width: { mode: "px", value: 800 },
    gapPx: 8,
    preloadImages: 6,
    autoNextChapter: true,
    autoWebtoonDetect: true,
    tapZonesEnabled: true,
    orientationLock: "auto",
    readerBg: "dark",
    vertical: {
      layoutMode: "continuous_strip",
      pagesPerGroup: 2,
      pageGapPx: 8,
      groupGapPx: 32,
      snapEnabled: true,
      alignment: "center",
      dimUnfocused: false,
      dimLevel: 50,
      noGapMode: false,
    },
    horizontal: {
      pagesPerGroup: 1,
      pageGapPx: 8,
      groupGapPx: 36,
      direction: "ltr",
      snapMode: "group",
      centerGroups: true,
      dimUnfocused: true,
      dimLevel: 55,
      noGapMode: false,
    },
    behavior: {
      keepScreenOn: true,
      skipBrokenImages: false,
      autoMarkReadOnOpen: false,
      doubleTapAction: "zoom",
      toolbarAutoHideSecs: 3,
      verticalMarginPx: 0,
      navigationSpeed: 5,
    },
  },
  floating: {
    scrollToTopScreens: [...ALL_FLOATING_SCREENS],
    scrollToTop: {
      edge: "right",
      verticalPercent: 82,
      horizontalOffsetPx: 16,
    },
    showReaderSettings: true,
    readerSettings: {
      edge: "right",
      verticalPercent: 68,
      horizontalOffsetPx: 16,
    },
    showOrientationLock: true,
    orientationLock: {
      edge: "right",
      verticalPercent: 54,
      horizontalOffsetPx: 16,
    },
    autoDimInReader: true,
  },
  updates: {
    enabled: true,
    intervalHours: 6,
    notifyWindows: true,
    checkAppUpdates: true,
  },
  sources: {
    headlessEnabled: true,
    maxConnPerHost: 4,
    speedLimitKbps: 0,
  },
  doh: {
    enabled: false,
    providerId: "cloudflare",
    profileId: "default",
    customProfileId: null,
    allowSystemFallback: true,
  },
  proxy: {
    enabled: false,
    defaultProxy: {
      protocol: "socks5",
      host: "",
      port: 1080,
      username: null,
      password: null,
    },
    allowDirectFallback: false,
    domainRules: [],
  },
  cache: {
    maxMb: 128,
  },
};

function mergeSettings(raw: Partial<Settings>): Settings {
  const rawUi = raw.ui ?? ({} as Partial<Settings["ui"]>);
  const rawReader = raw.reader ?? ({} as Partial<Settings["reader"]>);
  const rawFloating = raw.floating ?? ({} as Partial<Settings["floating"]>);
  const rawProxy = raw.proxy ?? ({} as Partial<Settings["proxy"]>);
  const rawDoh = raw.doh ?? ({} as Partial<Settings["doh"]>);
  const mergedVertical = {
    ...DEFAULT_SETTINGS.reader.vertical,
    ...(rawReader.vertical ?? {}),
  };
  if (
    rawReader.vertical?.pageGapPx === undefined &&
    typeof rawReader.gapPx === "number" &&
    rawReader.gapPx > 0
  ) {
    mergedVertical.pageGapPx = rawReader.gapPx;
  }
  const preloadRaw = rawReader.preloadImages ?? DEFAULT_SETTINGS.reader.preloadImages;
  const clampedPreload = Math.max(5, Math.min(8, preloadRaw));

  return {
    language: raw.language ?? DEFAULT_SETTINGS.language,
    theme: raw.theme ?? DEFAULT_SETTINGS.theme,
    ui: {
      lightAccent: normalizeHexColor(rawUi.lightAccent, DEFAULT_LIGHT_ACCENT),
      darkAccent: normalizeHexColor(rawUi.darkAccent, DEFAULT_DARK_ACCENT),
      navOrder: normalizeNavOrder(rawUi.navOrder),
    },
    device: {
      ...DEFAULT_SETTINGS.device,
      ...(raw.device ?? {}),
    },
    reader: {
      ...DEFAULT_SETTINGS.reader,
      ...rawReader,
      preloadImages: clampedPreload,
      vertical: mergedVertical,
      horizontal: {
        ...DEFAULT_SETTINGS.reader.horizontal,
        ...(rawReader.horizontal ?? {}),
      },
      behavior: {
        ...DEFAULT_SETTINGS.reader.behavior,
        ...(rawReader.behavior ?? {}),
      },
    },
    floating: {
      ...DEFAULT_SETTINGS.floating,
      ...rawFloating,
      scrollToTopScreens: Array.isArray(rawFloating.scrollToTopScreens)
        ? rawFloating.scrollToTopScreens
        : [...ALL_FLOATING_SCREENS],
      scrollToTop: {
        ...DEFAULT_SETTINGS.floating.scrollToTop,
        ...(rawFloating.scrollToTop ?? {}),
      },
      readerSettings: {
        ...DEFAULT_SETTINGS.floating.readerSettings,
        ...(rawFloating.readerSettings ?? {}),
      },
      orientationLock: {
        ...DEFAULT_SETTINGS.floating.orientationLock,
        ...(rawFloating.orientationLock ?? {}),
      },
    },
    updates: { ...DEFAULT_SETTINGS.updates, ...(raw.updates ?? {}) },
    sources: { ...DEFAULT_SETTINGS.sources, ...(raw.sources ?? {}) },
    doh: { ...DEFAULT_SETTINGS.doh, ...rawDoh },
    proxy: {
      ...DEFAULT_SETTINGS.proxy,
      ...rawProxy,
      defaultProxy: {
        ...DEFAULT_SETTINGS.proxy.defaultProxy,
        ...(rawProxy.defaultProxy ?? {}),
      },
      domainRules: Array.isArray(rawProxy.domainRules) ? rawProxy.domainRules : [],
    },
    cache: { ...DEFAULT_SETTINGS.cache, ...(raw.cache ?? {}) },
  };
}

function resolveTheme(theme: Theme): "light" | "dark" {
  const prefersDark =
    typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: dark)").matches;
  return theme === "system" ? (prefersDark ? "dark" : "light") : theme;
}

class SettingsStore {
  value = $state<Settings>(structuredClone(DEFAULT_SETTINGS));
  resolvedTheme = $state<"light" | "dark">("light");
  loaded = $state(false);

  t = $derived(getDict(this.value.language));
  locale = $derived(getLocale(this.value.language));

  private initialized = false;
  private saveTimer: number | null = null;
  private saveVersion = 0;
  private inFlightSaves = 0;
  private lastAppearanceKey = "";

  private applyAppearance(settings: Settings) {
    const resolved = resolveTheme(settings.theme);
    const activeAccentHex =
      resolved === "dark" ? settings.ui.darkAccent : settings.ui.lightAccent;
    const tokens = computeAccentTokens(activeAccentHex, resolved);
    const key = `${resolved}|${settings.language}|${tokens.accent}`;
    this.resolvedTheme = resolved;
    if (key === this.lastAppearanceKey) return;
    this.lastAppearanceKey = key;

    const root = document.documentElement;
    root.setAttribute("data-theme", resolved);
    root.setAttribute("lang", settings.language);
    root.style.setProperty("--accent", tokens.accent);
    root.style.setProperty("--accent-hover", tokens.accentHover);
    root.style.setProperty("--accent-soft", tokens.accentSoft);
    root.style.setProperty("--accent-fg", tokens.accentFg);
  }

  async init() {
    if (this.initialized) return;
    this.initialized = true;

    this.applyAppearance(this.value);
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    mq.addEventListener("change", () => {
      if (this.value.theme === "system") {
        this.applyAppearance(this.value);
      }
    });

    try {
      const s = await api.getSettings();
      this.value = mergeSettings(s);
      this.applyAppearance(this.value);
    } catch {
      // Fallback to defaults when running UI standalone in browser preview.
    } finally {
      this.loaded = true;
    }

    try {
      await api.onSettingsChanged((s) => {
        if (this.saveTimer !== null || this.inFlightSaves > 0) return;
        this.value = mergeSettings(s);
        this.applyAppearance(this.value);
      });
    } catch {
      // Outside Tauri runtime.
    }
  }

  /**
   * Mutates a plain snapshot clone of the current settings, updates UI reactively right away,
   * and persists to the Rust backend.
   */
  update(mutator: (draft: Settings) => void, debounceMs = 0) {
    const draft = $state.snapshot(this.value) as Settings;
    mutator(draft);
    draft.ui = {
      lightAccent: normalizeHexColor(draft.ui?.lightAccent, DEFAULT_LIGHT_ACCENT),
      darkAccent: normalizeHexColor(draft.ui?.darkAccent, DEFAULT_DARK_ACCENT),
      navOrder: normalizeNavOrder(draft.ui?.navOrder),
    };
    this.value = draft;
    this.applyAppearance(draft);
    this.schedulePersist(draft, debounceMs);
  }

  async resetSection(section: SettingsSection) {
    if (this.saveTimer !== null) {
      window.clearTimeout(this.saveTimer);
      this.saveTimer = null;
    }
    const version = ++this.saveVersion;
    try {
      const saved = await api.resetSettingsSection(section);
      if (version === this.saveVersion && this.saveTimer === null) {
        this.value = mergeSettings(saved);
        this.applyAppearance(this.value);
      }
      toasts.push(this.t.toastSectionReset, "success", 2000);
    } catch {
      toasts.push(this.t.toastSettingsSaveFailed, "error");
    }
  }

  async resetAll() {
    if (this.saveTimer !== null) {
      window.clearTimeout(this.saveTimer);
      this.saveTimer = null;
    }
    const version = ++this.saveVersion;
    try {
      const saved = await api.resetAllSettings();
      if (version === this.saveVersion && this.saveTimer === null) {
        this.value = mergeSettings(saved);
        this.applyAppearance(this.value);
      }
      toasts.push(this.t.toastAllSettingsReset, "success", 2400);
    } catch {
      toasts.push(this.t.toastSettingsSaveFailed, "error");
    }
  }

  private schedulePersist(payload: Settings, debounceMs: number) {
    if (this.saveTimer !== null) {
      window.clearTimeout(this.saveTimer);
      this.saveTimer = null;
    }

    const version = ++this.saveVersion;
    const run = async () => {
      this.saveTimer = null;
      this.inFlightSaves += 1;
      try {
        const saved = await api.setSettings(payload);
        if (version === this.saveVersion && this.saveTimer === null) {
          this.value = mergeSettings(saved);
          this.applyAppearance(this.value);
        }
      } catch (err) {
        if ("__TAURI_INTERNALS__" in window) {
          console.error("Failed to save settings:", err);
          toasts.push(this.t.toastSettingsSaveFailed, "error");
        }
      } finally {
        this.inFlightSaves = Math.max(0, this.inFlightSaves - 1);
      }
    };

    if (debounceMs > 0) {
      this.saveTimer = window.setTimeout(run, debounceMs);
    } else {
      void run();
    }
  }
}

export const settingsStore = new SettingsStore();
