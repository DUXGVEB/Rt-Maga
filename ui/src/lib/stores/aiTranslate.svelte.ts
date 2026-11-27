import { api, formatAppError } from "../api";
import type {
  AiDownloadProgressEvent,
  AiModelEntryView,
  AiStatusView,
  PageAiResultView,
  TranslationModelTier,
} from "../types";
import { settingsStore } from "./settings.svelte";
import { toasts } from "./toast.svelte";

const MAX_RAM_PAGE_CACHE = 24;
const DEFAULT_TIER2_REQUIRED_BYTES = 1_455_437_792;

const FALLBACK_STATUS: AiStatusView = {
  activeTier: "lite",
  platform: "other",
  storageDir: "",
  tier1Ready: true,
  tier2Ready: false,
  downloading: false,
  vulkanSupported: false,
  loadedInMemory: false,
  activeJobs: 0,
  models: [
    {
      tier: "lite",
      id: "tier1-lite",
      displayName: "Lite (Built-in)",
      status: "ready",
      installedBytes: 6_298_800,
      requiredBytes: 6_298_800,
      ready: true,
      bundled: true,
      accelerator: "CPU",
    },
    {
      tier: "qwen3_vl_2b",
      id: "qwen3-vl-2b",
      displayName: "Qwen3-VL-2B (High Accuracy)",
      status: "not_downloaded",
      installedBytes: 0,
      requiredBytes: DEFAULT_TIER2_REQUIRED_BYTES,
      ready: false,
      bundled: false,
      accelerator: "Vulkan",
    },
  ],
};

class AiTranslateStore {
  status = $state<AiStatusView | null>(null);
  loadingStatus = $state(false);
  downloadProgress = $state<AiDownloadProgressEvent | null>(null);
  busyAction = $state<"tier" | "download" | "cancel" | "delete" | null>(null);
  lastError = $state<string | null>(null);

  firstUseModalOpen = $state(false);
  pendingEnableMode = $state<"translate" | "bubble" | null>(null);

  generation = $state(0);
  pageCache = $state<Record<string, PageAiResultView>>({});
  inFlightPages = $state<Record<string, boolean>>({});

  private initialized = false;
  private inFlightPromises = new Map<string, Promise<PageAiResultView | null>>();
  private cacheOrder: string[] = [];
  private purgePromise: Promise<void> | null = null;

  activeTier = $derived<TranslationModelTier>(
    this.status?.activeTier ?? settingsStore.value.accessibility.translationModelTier ?? "lite"
  );

  tier1Ready = $derived(this.status?.tier1Ready ?? true);
  tier2Ready = $derived(this.status?.tier2Ready ?? false);
  downloading = $derived(
    Boolean(this.status?.downloading) || this.busyAction === "download"
  );

  liteModel = $derived<AiModelEntryView | null>(
    this.status?.models.find((m) => m.tier === "lite") ?? FALLBACK_STATUS.models[0]
  );

  tier2Model = $derived<AiModelEntryView | null>(
    this.status?.models.find((m) => m.tier === "qwen3_vl_2b") ?? FALLBACK_STATUS.models[1]
  );

  isTranslating = $derived(Object.keys(this.inFlightPages).length > 0);

  async init() {
    if (this.initialized) return;
    this.initialized = true;
    await this.refreshStatus();
    try {
      await api.onAiDownloadProgress((ev) => {
        if (ev.stage === "completed" || ev.stage === "cancelled" || ev.stage === "failed") {
          this.downloadProgress = null;
          void this.refreshStatus();
        } else {
          this.downloadProgress = ev;
        }
      });
    } catch {
      // Outside Tauri runtime.
    }
  }

  async refreshStatus(): Promise<AiStatusView> {
    this.loadingStatus = true;
    try {
      const next = await api.aiGetStatus();
      this.status = next;
      if (!next.downloading) {
        this.downloadProgress = null;
      }
      return next;
    } catch {
      if (!this.status) {
        this.status = structuredClone(FALLBACK_STATUS);
      }
      return this.status;
    } finally {
      this.loadingStatus = false;
    }
  }

  private cacheKey(pageToken: string, detectOnly: boolean): string {
    const lang = settingsStore.value.language;
    const tier = this.activeTier;
    return `${pageToken}|${tier}|${lang}|${detectOnly ? "d" : "t"}`;
  }

  private putCache(key: string, result: PageAiResultView) {
    const nextCache = { ...this.pageCache, [key]: result };
    this.cacheOrder = [...this.cacheOrder.filter((k) => k !== key), key];
    while (this.cacheOrder.length > MAX_RAM_PAGE_CACHE) {
      const oldest = this.cacheOrder.shift();
      if (oldest) {
        delete nextCache[oldest];
      }
    }
    this.pageCache = nextCache;
  }

  private clearRamCache() {
    this.pageCache = {};
    this.inFlightPages = {};
    this.inFlightPromises.clear();
    this.cacheOrder = [];
  }

  getPageResult(pageToken: string): PageAiResultView | null {
    const fullKey = this.cacheKey(pageToken, false);
    if (this.pageCache[fullKey]) return this.pageCache[fullKey];
    const detectKey = this.cacheKey(pageToken, true);
    if (this.pageCache[detectKey]) return this.pageCache[detectKey];
    return null;
  }

  isPageLoading(pageToken: string): boolean {
    return Boolean(this.inFlightPages[pageToken]);
  }

  requestToggleTranslation(next: boolean) {
    if (next) {
      if (!settingsStore.value.accessibility.firstAiPromptSeen) {
        this.pendingEnableMode = "translate";
        this.firstUseModalOpen = true;
        void this.refreshStatus();
        return;
      }
      settingsStore.update((d) => {
        d.accessibility.aiTranslationOverlay = true;
      });
      void this.refreshStatus();
      return;
    }

    settingsStore.update((d) => {
      d.accessibility.aiTranslationOverlay = false;
    });
    void this.disableAndPurge();
  }

  requestToggleBubbleZoom(next: boolean) {
    settingsStore.update((d) => {
      d.accessibility.bubbleAutoZoom = next;
    });
    if (!next && !settingsStore.value.accessibility.aiTranslationOverlay) {
      void this.disableAndPurge();
    }
  }

  dismissFirstUseModal() {
    this.firstUseModalOpen = false;
    this.pendingEnableMode = null;
  }

  async confirmFirstUseWithLite() {
    const mode = this.pendingEnableMode ?? "translate";
    this.firstUseModalOpen = false;
    this.pendingEnableMode = null;
    if (this.status?.activeTier !== "lite") {
      try {
        this.status = await api.aiSetTier("lite");
      } catch {
        // Non-fatal in preview or already active.
      }
    }
    this.generation += 1;
    this.clearRamCache();
    settingsStore.update((d) => {
      d.accessibility.firstAiPromptSeen = true;
      d.accessibility.translationModelTier = "lite";
      if (mode === "translate") {
        d.accessibility.aiTranslationOverlay = true;
      } else {
        d.accessibility.bubbleAutoZoom = true;
      }
    });
  }

  async confirmFirstUseWithTier2() {
    const mode = this.pendingEnableMode ?? "translate";
    this.firstUseModalOpen = false;
    this.pendingEnableMode = null;
    if (this.tier2Ready) {
      await this.setTier("qwen3_vl_2b");
    }
    settingsStore.update((d) => {
      d.accessibility.firstAiPromptSeen = true;
      if (mode === "translate") {
        d.accessibility.aiTranslationOverlay = true;
      } else {
        d.accessibility.bubbleAutoZoom = true;
      }
    });
    if (!this.tier2Ready) {
      void this.downloadTier2();
    }
  }

  async setTier(tier: TranslationModelTier): Promise<boolean> {
    if (tier === "qwen3_vl_2b" && !this.tier2Ready) {
      return false;
    }
    this.busyAction = "tier";
    this.lastError = null;
    try {
      const next = await api.aiSetTier(tier);
      this.status = next;
      settingsStore.update((d) => {
        d.accessibility.translationModelTier = next.activeTier;
      });
      this.generation += 1;
      this.clearRamCache();
      return true;
    } catch (err) {
      const f = formatAppError(err, settingsStore.value.language);
      this.lastError = f.detail || f.title;
      toasts.push(this.lastError, "error", 3200);
      return false;
    } finally {
      this.busyAction = null;
    }
  }

  async downloadTier2(): Promise<boolean> {
    if (this.downloading) return false;
    this.busyAction = "download";
    this.lastError = null;
    this.downloadProgress = {
      stage: "starting",
      assetId: "qwen3-vl-2b",
      fileIndex: 0,
      fileCount: 2,
      downloadedBytes: 0,
      totalBytes: this.tier2Model?.requiredBytes ?? DEFAULT_TIER2_REQUIRED_BYTES,
      percent: 0,
      speedBytesPerSec: 0,
      message: null,
    };
    try {
      const next = await api.aiDownloadTier2((ev) => {
        this.downloadProgress = ev;
      });
      this.status = next;
      this.downloadProgress = null;
      if (next.tier2Ready) {
        settingsStore.update((d) => {
          d.accessibility.translationModelTier = next.activeTier;
        });
        this.generation += 1;
        this.clearRamCache();
        toasts.push(settingsStore.t.a11yDownloadCompleteToast, "success", 2800);
        return true;
      }
      return false;
    } catch (err) {
      this.downloadProgress = null;
      const f = formatAppError(err, settingsStore.value.language);
      const msg = f.detail || f.title;
      if (!/cancel/i.test(msg)) {
        this.lastError = msg;
        toasts.push(msg, "error", 3600);
      }
      await this.refreshStatus();
      return false;
    } finally {
      this.busyAction = null;
    }
  }

  async cancelDownload() {
    if (!this.downloading && this.busyAction !== "download") return;
    this.busyAction = "cancel";
    try {
      const next = await api.aiCancelDownload();
      this.status = next;
      this.downloadProgress = null;
      toasts.push(settingsStore.t.a11yDownloadCancelledToast, "info", 2200);
    } catch (err) {
      const f = formatAppError(err, settingsStore.value.language);
      toasts.push(f.detail || f.title, "error", 3000);
    } finally {
      this.busyAction = null;
    }
  }

  async deleteTier2(): Promise<boolean> {
    if (this.busyAction) return false;
    this.busyAction = "delete";
    this.lastError = null;
    try {
      const next = await api.aiDeleteTier2();
      this.status = next;
      settingsStore.update((d) => {
        d.accessibility.translationModelTier = next.activeTier;
      });
      this.generation += 1;
      this.clearRamCache();
      toasts.push(settingsStore.t.a11yModelDeletedToast, "info", 2400);
      return true;
    } catch (err) {
      const f = formatAppError(err, settingsStore.value.language);
      this.lastError = f.detail || f.title;
      toasts.push(this.lastError, "error", 3200);
      return false;
    } finally {
      this.busyAction = null;
    }
  }

  async processPage(
    pageToken: string,
    opts: { detectOnly: boolean }
  ): Promise<PageAiResultView | null> {
    if (!pageToken) return null;
    const a11y = settingsStore.value.accessibility;
    if (!a11y.aiTranslationOverlay && !a11y.bubbleAutoZoom) {
      return null;
    }

    const effectiveDetectOnly = opts.detectOnly || !a11y.aiTranslationOverlay;
    const fullKey = this.cacheKey(pageToken, false);
    if (this.pageCache[fullKey]) return this.pageCache[fullKey];

    const key = this.cacheKey(pageToken, effectiveDetectOnly);
    if (this.pageCache[key]) return this.pageCache[key];

    const existing = this.inFlightPromises.get(key);
    if (existing) return existing;

    const gen = this.generation;
    const targetLang = settingsStore.value.language;

    this.inFlightPages = { ...this.inFlightPages, [pageToken]: true };
    const promise = (async (): Promise<PageAiResultView | null> => {
      try {
        if (this.purgePromise) {
          await this.purgePromise;
        }
        const beforeInvoke = settingsStore.value.accessibility;
        if (
          gen !== this.generation ||
          (!beforeInvoke.aiTranslationOverlay && !beforeInvoke.bubbleAutoZoom)
        ) {
          return null;
        }
        const res = await api.aiTranslatePage({
          pageToken,
          targetLang,
          detectOnly: effectiveDetectOnly,
        });
        const cur = settingsStore.value.accessibility;
        if (
          res.cancelled ||
          gen !== this.generation ||
          (!cur.aiTranslationOverlay && !cur.bubbleAutoZoom)
        ) {
          return null;
        }
        this.putCache(key, res);
        return res;
      } catch {
        return null;
      } finally {
        this.inFlightPromises.delete(key);
        if (this.inFlightPages[pageToken]) {
          const nextMap = { ...this.inFlightPages };
          delete nextMap[pageToken];
          this.inFlightPages = nextMap;
        }
      }
    })();

    this.inFlightPromises.set(key, promise);
    return promise;
  }

  cancelJobs() {
    this.generation += 1;
    this.inFlightPages = {};
    this.inFlightPromises.clear();
    void api.aiCancelJobs().catch(() => {});
  }

  async disableAndPurge() {
    this.generation += 1;
    this.clearRamCache();
    const run = (async () => {
      try {
        const next = await api.aiHardUnloadAndPurge();
        if (next) {
          this.status = next;
        }
      } catch {
        // Ignore outside Tauri runtime.
      }
    })();
    this.purgePromise = run;
    try {
      await run;
    } finally {
      if (this.purgePromise === run) {
        this.purgePromise = null;
      }
    }
  }
}

export const aiTranslateStore = new AiTranslateStore();
