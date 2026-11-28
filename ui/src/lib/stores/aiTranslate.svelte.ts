import { api, formatAppError } from "../api";
import type {
  AiDownloadProgressEvent,
  AiModelEntryView,
  AiNmtPackView,
  AiStatusView,
  Language,
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
  tier2Supported: true,
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
  nmtPacks: [],
};

function isNmtAssetId(assetId: string | null | undefined): boolean {
  if (!assetId) return false;
  return assetId.startsWith("opus-mt-") || assetId.startsWith("mlkit-");
}

class AiTranslateStore {
  status = $state<AiStatusView | null>(null);
  loadingStatus = $state(false);
  downloadProgress = $state<AiDownloadProgressEvent | null>(null);
  busyAction = $state<"tier" | "download" | "cancel" | "delete" | null>(null);
  downloadingNmtPackId = $state<string | null>(null);
  lastError = $state<string | null>(null);

  firstUseModalOpen = $state(false);
  pendingEnableMode = $state<"translate" | "bubble" | null>(null);
  missingNmtPrompt = $state<{ packs: AiNmtPackView[]; retryToken: string } | null>(null);

  peekOriginal = $state(false);
  generation = $state(0);
  pageCache = $state<Record<string, PageAiResultView>>({});
  inFlightPages = $state<Record<string, boolean>>({});
  pageErrors = $state<Record<string, string>>({});
  pageMissingPacks = $state<Record<string, AiNmtPackView[]>>({});

  private initialized = false;
  private inFlightPromises = new Map<string, Promise<PageAiResultView | null>>();
  private completedKeys = new Set<string>();
  private cacheOrder: string[] = [];
  private purgePromise: Promise<void> | null = null;
  private dismissedNmtPromptSig = "";

  activeTier = $derived<TranslationModelTier>(
    this.status?.activeTier ?? settingsStore.value.accessibility.translationModelTier ?? "lite"
  );

  effectiveTargetLang = $derived<Language>(
    settingsStore.value.accessibility.translationTargetLang ?? settingsStore.value.language
  );

  tier1Ready = $derived(this.status?.tier1Ready ?? true);
  tier2Supported = $derived(this.status?.tier2Supported ?? true);
  tier2Ready = $derived(this.status?.tier2Ready ?? false);
  nmtPacks = $derived<AiNmtPackView[]>(this.status?.nmtPacks ?? []);

  downloading = $derived(
    Boolean(this.status?.downloading) || this.busyAction === "download"
  );

  isDownloadingNmtPack = $derived(
    Boolean(this.downloadingNmtPackId) || isNmtAssetId(this.downloadProgress?.assetId)
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
          this.downloadingNmtPackId = null;
          void this.refreshStatus();
        } else {
          this.downloadProgress = ev;
          if (isNmtAssetId(ev.assetId)) {
            this.downloadingNmtPackId = ev.assetId;
          }
        }
      });
    } catch {
      // Outside Tauri runtime.
    }
    try {
      await api.onSettingsChanged((nextSettings) => {
        const nextTier = nextSettings.accessibility.translationModelTier;
        if (this.status && nextTier !== this.status.activeTier) {
          void this.refreshStatus();
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
        this.downloadingNmtPackId = null;
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

  clearLastError() {
    this.lastError = null;
  }

  setPeekOriginal(next: boolean) {
    this.peekOriginal = next;
  }

  togglePeekOriginal() {
    this.peekOriginal = !this.peekOriginal;
  }

  setTargetLanguage(lang: Language | null) {
    this.dismissedNmtPromptSig = "";
    this.missingNmtPrompt = null;
    this.pageErrors = {};
    this.pageMissingPacks = {};
    this.generation += 1;
    settingsStore.update((d) => {
      d.accessibility.translationTargetLang = lang;
    });
  }

  private cacheKey(pageToken: string, detectOnly: boolean): string {
    const lang = this.effectiveTargetLang;
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
        this.completedKeys.delete(oldest);
      }
    }
    this.pageCache = nextCache;
  }

  private clearRamCache() {
    this.pageCache = {};
    this.inFlightPages = {};
    this.pageErrors = {};
    this.pageMissingPacks = {};
    this.inFlightPromises.clear();
    this.completedKeys.clear();
    this.cacheOrder = [];
  }

  private clearPageError(pageToken: string) {
    if (this.pageErrors[pageToken]) {
      const next = { ...this.pageErrors };
      delete next[pageToken];
      this.pageErrors = next;
    }
  }

  private clearPageMissingPacks(pageToken: string) {
    if (this.pageMissingPacks[pageToken]) {
      const next = { ...this.pageMissingPacks };
      delete next[pageToken];
      this.pageMissingPacks = next;
    }
  }

  getPageResult(pageToken: string): PageAiResultView | null {
    const fullKey = this.cacheKey(pageToken, false);
    if (this.pageCache[fullKey]) return this.pageCache[fullKey];
    const detectKey = this.cacheKey(pageToken, true);
    if (this.pageCache[detectKey]) return this.pageCache[detectKey];
    return null;
  }

  getPageError(pageToken: string): string | null {
    if (!pageToken) return null;
    return this.pageErrors[pageToken] ?? null;
  }

  getPageMissingPacks(pageToken: string): AiNmtPackView[] {
    if (!pageToken) return [];
    return this.pageMissingPacks[pageToken] ?? [];
  }

  isPageLoading(pageToken: string): boolean {
    return Boolean(this.inFlightPages[pageToken]);
  }

  requestToggleTranslation(next: boolean) {
    if (next) {
      this.dismissedNmtPromptSig = "";
      this.lastError = null;
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

    this.peekOriginal = false;
    this.missingNmtPrompt = null;
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

  openMissingNmtPromptForPage(pageToken: string) {
    const packs = this.pageMissingPacks[pageToken];
    if (!packs || packs.length === 0) return;
    this.missingNmtPrompt = { packs, retryToken: pageToken };
  }

  dismissMissingNmtPrompt() {
    if (this.missingNmtPrompt) {
      this.dismissedNmtPromptSig = this.missingNmtPrompt.packs.map((p) => p.id).sort().join(",");
    }
    this.missingNmtPrompt = null;
  }

  async confirmDownloadMissingNmtPacks(): Promise<boolean> {
    const prompt = this.missingNmtPrompt;
    if (!prompt) return false;
    this.missingNmtPrompt = null;
    this.dismissedNmtPromptSig = "";
    const packIds = prompt.packs.map((p) => p.id);
    return this.downloadNmtPacks(packIds, prompt.retryToken);
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
    if (!this.tier2Supported) {
      toasts.push(settingsStore.t.a11yTier2Unsupported, "info", 2800);
      await this.confirmFirstUseWithLite();
      return;
    }
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
    if (tier === "qwen3_vl_2b" && (!this.tier2Ready || !this.tier2Supported)) {
      return false;
    }
    this.busyAction = "tier";
    this.lastError = null;
    this.missingNmtPrompt = null;
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
    if (!this.tier2Supported) {
      toasts.push(settingsStore.t.a11yTier2Unsupported, "info", 2800);
      return false;
    }
    this.busyAction = "download";
    this.downloadingNmtPackId = null;
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

  async downloadNmtPacks(packIds: string[], retryToken?: string): Promise<boolean> {
    const cleanIds = Array.from(new Set(packIds.map((id) => id.trim()).filter(Boolean)));
    if (cleanIds.length === 0 || this.downloading) return false;
    this.busyAction = "download";
    this.downloadingNmtPackId = cleanIds[0];
    this.lastError = null;
    this.dismissedNmtPromptSig = "";

    const estimatedBytes = cleanIds.reduce((sum, id) => {
      const found = this.nmtPacks.find((p) => p.id === id);
      return sum + (found?.requiredBytes ?? 0);
    }, 0);

    this.downloadProgress = {
      stage: "starting",
      assetId: cleanIds[0],
      fileIndex: 0,
      fileCount: cleanIds.length,
      downloadedBytes: 0,
      totalBytes: estimatedBytes,
      percent: 0,
      speedBytesPerSec: 0,
      message: null,
    };

    try {
      const next = await api.aiDownloadNmtPacks(cleanIds, (ev) => {
        this.downloadProgress = ev;
        if (isNmtAssetId(ev.assetId)) {
          this.downloadingNmtPackId = ev.assetId;
        }
      });
      this.status = next;
      this.downloadProgress = null;
      this.downloadingNmtPackId = null;
      this.generation += 1;
      this.clearRamCache();
      toasts.push(settingsStore.t.a11yNmtPackReadyToast, "success", 2600);
      if (retryToken && settingsStore.value.accessibility.aiTranslationOverlay) {
        void this.processPage(retryToken, { detectOnly: false });
      }
      return true;
    } catch (err) {
      this.downloadProgress = null;
      this.downloadingNmtPackId = null;
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

  async deleteNmtPack(packId: string): Promise<boolean> {
    if (!packId || this.busyAction) return false;
    this.busyAction = "delete";
    this.lastError = null;
    try {
      const next = await api.aiDeleteNmtPack(packId);
      this.status = next;
      this.generation += 1;
      this.clearRamCache();
      toasts.push(settingsStore.t.a11yNmtPackDeletedToast, "info", 2400);
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

  async cancelDownload() {
    if (!this.downloading && this.busyAction !== "download") return;
    this.busyAction = "cancel";
    try {
      const next = await api.aiCancelDownload();
      this.status = next;
      this.downloadProgress = null;
      this.downloadingNmtPackId = null;
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

  async retryPage(pageToken: string, detectOnly: boolean): Promise<PageAiResultView | null> {
    if (!pageToken) return null;
    const fullKey = this.cacheKey(pageToken, false);
    const detectKey = this.cacheKey(pageToken, true);
    const nextCache = { ...this.pageCache };
    delete nextCache[fullKey];
    delete nextCache[detectKey];
    this.completedKeys.delete(fullKey);
    this.completedKeys.delete(detectKey);
    this.pageCache = nextCache;
    this.clearPageError(pageToken);
    this.clearPageMissingPacks(pageToken);
    this.dismissedNmtPromptSig = "";
    return this.processPage(pageToken, { detectOnly });
  }

  isPageDone(pageToken: string, detectOnly: boolean): boolean {
    if (!pageToken) return false;
    const fullKey = this.cacheKey(pageToken, false);
    if (this.completedKeys.has(fullKey)) return true;
    if (detectOnly) {
      return this.completedKeys.has(this.cacheKey(pageToken, true));
    }
    return false;
  }

  async processPage(
    pageToken: string,
    opts: { detectOnly: boolean; focusY?: number }
  ): Promise<PageAiResultView | null> {
    if (!pageToken) return null;
    const a11y = settingsStore.value.accessibility;
    if (!a11y.aiTranslationOverlay && !a11y.bubbleAutoZoom) {
      return null;
    }

    const effectiveDetectOnly = opts.detectOnly || !a11y.aiTranslationOverlay;
    const fullKey = this.cacheKey(pageToken, false);
    if (this.completedKeys.has(fullKey) && this.pageCache[fullKey]) {
      return this.pageCache[fullKey];
    }

    const key = this.cacheKey(pageToken, effectiveDetectOnly);
    if (this.completedKeys.has(key) && this.pageCache[key]) {
      return this.pageCache[key];
    }

    const existing = this.inFlightPromises.get(key);
    if (existing) return existing;

    const gen = this.generation;
    const targetLang = this.effectiveTargetLang;
    const focusY = Math.max(0, Math.min(1000, Math.round(opts.focusY ?? 500)));

    this.clearPageError(pageToken);
    this.clearPageMissingPacks(pageToken);
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
        const res = await api.aiTranslatePage(
          {
            pageToken,
            targetLang,
            detectOnly: effectiveDetectOnly,
            generation: gen,
            focusY,
          },
          (partial) => {
            const cur = settingsStore.value.accessibility;
            if (
              partial.cancelled ||
              gen !== this.generation ||
              (!cur.aiTranslationOverlay && !cur.bubbleAutoZoom)
            ) {
              return;
            }
            // Stream each translated bubble immediately onto the page as soon as it finishes.
            this.putCache(key, partial);
          }
        );
        const cur = settingsStore.value.accessibility;
        if (
          res.cancelled ||
          gen !== this.generation ||
          (!cur.aiTranslationOverlay && !cur.bubbleAutoZoom)
        ) {
          return null;
        }

        const missing = res.missingNmtPacks ?? [];
        if (missing.length > 0) {
          // Cache detected bubble bounding boxes under detectOnly so Bubble Magnifier works
          // immediately without claiming translation succeeded.
          const detKey = this.cacheKey(pageToken, true);
          this.putCache(detKey, { ...res, detectOnly: true });
          this.completedKeys.add(detKey);
          this.pageMissingPacks = { ...this.pageMissingPacks, [pageToken]: missing };
          const sig = missing.map((p) => p.id).sort().join(",");
          if (!effectiveDetectOnly && !this.downloading && sig !== this.dismissedNmtPromptSig) {
            this.missingNmtPrompt = { packs: missing, retryToken: pageToken };
          }
          return res;
        }

        if (res.error) {
          this.pageErrors = { ...this.pageErrors, [pageToken]: res.error };
          this.lastError = res.error;
          if (res.regions.length > 0) {
            this.putCache(key, res);
          }
          return res;
        }

        this.putCache(key, res);
        this.completedKeys.add(key);
        return res;
      } catch (err) {
        if (gen === this.generation) {
          const f = formatAppError(err, settingsStore.value.language);
          const msg = f.detail || f.title;
          this.pageErrors = { ...this.pageErrors, [pageToken]: msg };
          this.lastError = msg;
        }
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
    this.peekOriginal = false;
    this.missingNmtPrompt = null;
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
