import { api } from "../api";
import { getDict, getLocale } from "../i18n";
import type { FloatingScreenKey, Language, Settings, Theme } from "../types";
import { toasts } from "./toast.svelte";

const ALL_FLOATING_SCREENS: FloatingScreenKey[] = [
  "home",
  "bookmarks",
  "history",
  "manga",
  "reader",
  "settings",
];

export const DEFAULT_SETTINGS: Settings = {
  language: "en",
  theme: "system",
  device: {
    customName: null,
  },
  reader: {
    orientation: "vertical",
    width: { mode: "px", value: 800 },
    gapPx: 0,
    preloadImages: 6,
    autoNextChapter: true,
    autoWebtoonDetect: true,
    tapZonesEnabled: true,
    orientationLock: "auto",
    readerBg: "dark",
    vertical: {
      layoutMode: "continuous_strip",
      pagesPerGroup: 2,
      pageGapPx: 0,
      groupGapPx: 32,
      snapEnabled: true,
      alignment: "center",
      dimUnfocused: false,
      dimLevel: 50,
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
    maxMb: 1024,
  },
};

function mergeSettings(raw: Partial<Settings>): Settings {
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

  private applyAppearance(theme: Theme, language: Language) {
    const resolved = resolveTheme(theme);
    this.resolvedTheme = resolved;
    document.documentElement.setAttribute("data-theme", resolved);
    document.documentElement.setAttribute("lang", language);
  }

  async init() {
    if (this.initialized) return;
    this.initialized = true;

    this.applyAppearance(this.value.theme, this.value.language);
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    mq.addEventListener("change", () => {
      if (this.value.theme === "system") {
        this.applyAppearance("system", this.value.language);
      }
    });

    try {
      const s = await api.getSettings();
      this.value = mergeSettings(s);
      this.applyAppearance(this.value.theme, this.value.language);
    } catch {
      // Fallback to defaults when running UI standalone in browser preview.
    } finally {
      this.loaded = true;
    }

    try {
      await api.onSettingsChanged((s) => {
        if (this.saveTimer !== null || this.inFlightSaves > 0) return;
        this.value = mergeSettings(s);
        this.applyAppearance(this.value.theme, this.value.language);
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
    this.value = draft;
    this.applyAppearance(draft.theme, draft.language);
    this.schedulePersist(draft, debounceMs);
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
          this.applyAppearance(this.value.theme, this.value.language);
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
