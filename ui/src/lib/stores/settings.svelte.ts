import { api } from "../api";
import { getDict, getLocale } from "../i18n";
import type { Language, Settings, Theme } from "../types";
import { toasts } from "./toast.svelte";

export const DEFAULT_SETTINGS: Settings = {
  language: "en",
  theme: "system",
  reader: {
    width: { mode: "px", value: 800 },
    gapPx: 0,
    preloadImages: 4,
    autoNextChapter: true,
    readerBg: "dark",
  },
  updates: {
    enabled: true,
    intervalHours: 6,
    notifyWindows: true,
  },
  sources: {
    headlessEnabled: true,
    maxConnPerHost: 4,
  },
  cache: {
    maxMb: 1024,
  },
};

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
      this.value = { ...DEFAULT_SETTINGS, ...s };
      this.applyAppearance(this.value.theme, this.value.language);
    } catch {
      // Fallback to defaults when running UI standalone in browser preview.
    } finally {
      this.loaded = true;
    }

    try {
      await api.onSettingsChanged((s) => {
        if (this.saveTimer !== null) return;
        this.value = { ...DEFAULT_SETTINGS, ...s };
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
      try {
        const saved = await api.setSettings(payload);
        if (version === this.saveVersion) {
          this.value = { ...DEFAULT_SETTINGS, ...saved };
          this.applyAppearance(this.value.theme, this.value.language);
        }
      } catch (err) {
        if ("__TAURI_INTERNALS__" in window) {
          console.error("Failed to save settings:", err);
          toasts.push(this.t.toastSettingsSaveFailed, "error");
        }
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
