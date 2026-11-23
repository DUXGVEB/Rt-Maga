<script lang="ts">
  import {
    AlertTriangle,
    Bell,
    ChevronDown,
    ChevronUp,
    Download,
    Eye,
    EyeOff,
    Gauge,
    HardDrive,
    Loader2,
    RefreshCw,
    RotateCcw,
    Search,
    Smartphone,
    Sparkles,
    Trash2,
    X,
  } from "lucide-svelte";
  import { onMount, tick } from "svelte";
  import { api, formatAppError, imgProxyUrl } from "../lib/api";
  import DiagnosticsPanel from "../lib/components/DiagnosticsPanel.svelte";
  import LanSyncPanel from "../lib/components/LanSyncPanel.svelte";
  import NetworkProtectionPanel from "../lib/components/NetworkProtectionPanel.svelte";
  import SettingsVisualPreview from "../lib/components/SettingsVisualPreview.svelte";
  import { LANGUAGE_OPTIONS } from "../lib/i18n";
  import { appReleaseStore } from "../lib/stores/appRelease.svelte";
  import { deviceEnv } from "../lib/stores/deviceEnv.svelte";
  import { notificationsStore } from "../lib/stores/notifications.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { updatesStore } from "../lib/stores/updates.svelte";
  import type {
    CacheInfo,
    CacheMangaStats,
    CacheSourceStats,
    DoubleTapAction,
    FloatingEdge,
    FloatingScreenKey,
    HiddenMangaView,
    HorizontalDirection,
    HorizontalSnapMode,
    Language,
    NotificationPermissionInfo,
    ReaderBg,
    ReadingOrientation,
    Settings,
    SettingsSection,
    Theme,
  } from "../lib/types";

  let { focusSection }: { focusSection?: SettingsSection } = $props();

  type ReaderCategoryKey = "layout" | "display" | "controls" | "behavior";

  let searchQuery = $state("");
  let expandedReaderCats = $state<Record<ReaderCategoryKey, boolean>>({
    layout: false,
    display: false,
    controls: false,
    behavior: false,
  });
  let cache = $state<CacheInfo | null>(null);
  let cacheBySource = $state<CacheSourceStats[]>([]);
  let cacheByManga = $state<CacheMangaStats[]>([]);
  let hiddenManga = $state<HiddenMangaView[]>([]);
  let clearingCache = $state(false);
  let clearingScopeKey = $state<string | null>(null);
  let confirmingClearAllCache = $state(false);
  let confirmingScopeKey = $state<string | null>(null);
  let showResetAllModal = $state(false);
  let resettingSection = $state<SettingsSection | null>(null);
  let resettingAll = $state(false);
  let pendingDeleteHidden = $state<HiddenMangaView | null>(null);
  let deletingHidden = $state(false);
  let notifPerm = $state<NotificationPermissionInfo>({ granted: true, canRequest: false });

  const s = $derived(settingsStore.value);
  const t = $derived(settingsStore.t);
  const isHorizontal = $derived(s.reader.orientation === "horizontal");
  const normalizedQuery = $derived(searchQuery.trim().toLowerCase());

  const floatingScreens: FloatingScreenKey[] = [
    "home",
    "bookmarks",
    "history",
    "manga",
    "reader",
    "settings",
  ];

  const speedPresets: { kbps: number; labelKey: "unlimited" | "512k" | "1m" | "2m" | "5m" | "10m" }[] = [
    { kbps: 0, labelKey: "unlimited" },
    { kbps: 512, labelKey: "512k" },
    { kbps: 1024, labelKey: "1m" },
    { kbps: 2048, labelKey: "2m" },
    { kbps: 5120, labelKey: "5m" },
    { kbps: 10240, labelKey: "10m" },
  ];

  function matchesSearch(...fields: string[]): boolean {
    if (!normalizedQuery) return true;
    const hay = fields.join(" ").toLowerCase();
    return normalizedQuery.split(/\s+/).every((token) => hay.includes(token));
  }

  // Per-section search visibility
  const showAppearance = $derived(
    matchesSearch(
      t.appearanceSection,
      t.languageLabel,
      t.languageDesc,
      t.colorThemeLabel,
      t.colorThemeDesc,
      "appearance language theme dark light system giao dien ngon ngu chu de"
    )
  );
  const showDevice = $derived(
    matchesSearch(
      t.deviceIdentitySection,
      t.deviceDetectedLabel,
      t.deviceCustomNameLabel,
      t.deviceCustomNameDesc,
      "device identity name lan sync thiet bi ten"
    )
  );
  function toggleReaderCat(cat: ReaderCategoryKey) {
    expandedReaderCats = {
      ...expandedReaderCats,
      [cat]: !expandedReaderCats[cat],
    };
  }

  const matchCatLayout = $derived(
    matchesSearch(
      t.readerCatLayout,
      t.readingModeLabel,
      t.readingModeDesc,
      t.noGapModeLabel,
      t.noGapModeDesc,
      t.gapDesc,
      t.verticalMarginLabel(s.reader.behavior.verticalMarginPx),
      t.verticalMarginDesc,
      t.horizontalDirLabel,
      t.pagesPerGroupLabel,
      t.snapModeLabel,
      t.centerGroupsLabel,
      t.autoAppendNext,
      t.visualPreviewTitle,
      "layout mode vertical horizontal gap no-gap margin snap group preview cuon doc ngang khoang cach le"
    )
  );
  const matchCatDisplay = $derived(
    matchesSearch(
      t.readerCatDisplay,
      t.readerBg,
      t.readerBgDesc,
      t.dimUnfocusedLabel,
      t.dimUnfocusedDesc,
      t.autoWebtoonLabel,
      t.autoWebtoonDesc,
      "display background theme dark light dim unfocused webtoon hien thi nen lam toi"
    )
  );
  const matchCatControls = $derived(
    matchesSearch(
      t.readerCatControls,
      t.doubleTapActionLabel,
      t.doubleTapActionDesc,
      t.tapZonesLabel,
      t.tapZonesDesc,
      t.toolbarAutoHideLabel,
      t.toolbarAutoHideDesc,
      t.keepScreenOnLabel,
      t.keepScreenOnDesc,
      "controls gestures double tap zoom toolbar auto hide screen awake dieu khien cham dup thanh cong cu man hinh"
    )
  );
  const matchCatBehavior = $derived(
    matchesSearch(
      t.readerCatBehavior,
      t.autoMarkReadLabel,
      t.autoMarkReadDesc,
      t.skipBrokenImagesLabel,
      t.skipBrokenImagesDesc,
      t.preloadLabel(s.reader.preloadImages),
      t.preloadDesc,
      "behavior performance mark read broken image skip preload doc truyen hieu nang da doc anh hong"
    )
  );

  function isReaderCatOpen(cat: ReaderCategoryKey): boolean {
    if (normalizedQuery) {
      if (cat === "layout") return matchCatLayout;
      if (cat === "display") return matchCatDisplay;
      if (cat === "controls") return matchCatControls;
      return matchCatBehavior;
    }
    return expandedReaderCats[cat];
  }

  const readerLayoutSummary = $derived(() => {
    if (isHorizontal) {
      const dir = s.reader.horizontal.direction === "rtl" ? "RTL" : "LTR";
      return `${t.modeHorizontal} · ${dir} · ${s.reader.horizontal.pagesPerGroup}x`;
    }
    const gapText = s.reader.vertical.noGapMode
      ? t.noGapModeLabel
      : `${t.previewGapChip}: ${s.reader.vertical.pageGapPx}px`;
    return `${t.modeVertical} · ${gapText}`;
  });

  const readerDisplaySummary = $derived(() => {
    const bgLabel =
      s.reader.readerBg === "dark"
        ? t.bgDark
        : s.reader.readerBg === "light"
          ? t.bgLight
          : t.bgAppTheme;
    return s.reader.autoWebtoonDetect ? `${bgLabel} · Webtoon` : bgLabel;
  });

  const readerControlsSummary = $derived(() => {
    const dt =
      s.reader.behavior.doubleTapAction === "zoom"
        ? t.doubleTapZoom
        : s.reader.behavior.doubleTapAction === "toggle_toolbar"
          ? t.doubleTapToggleToolbar
          : s.reader.behavior.doubleTapAction === "next_page"
            ? t.doubleTapNextPage
            : t.doubleTapDisabled;
    return dt;
  });

  const readerBehaviorSummary = $derived(
    () => `${t.preloadLabel(s.reader.preloadImages)}`
  );

  $effect(() => {
    const target = focusSection;
    if (!target) return;
    void (async () => {
      await tick();
      const el = document.getElementById(`settings-${target}`);
      el?.scrollIntoView({ behavior: "smooth", block: "start" });
    })();
  });

  const showReader = $derived(
    matchesSearch(t.readerSection, "reader trinh doc") ||
      matchCatLayout ||
      matchCatDisplay ||
      matchCatControls ||
      matchCatBehavior
  );
  const showFloating = $derived(
    matchesSearch(
      t.floatingSection,
      t.floatingSubtitle,
      t.floatingScrollTopScreens,
      t.floatingReaderSettingsLabel,
      t.floatingOrientationLockLabel,
      t.floatingAutoDimLabel,
      "floating scroll top orientation lock button nut noi"
    )
  );
  const showLanSync = $derived(
    matchesSearch(
      t.syncSection,
      t.syncSubtitle,
      "lan sync qr pin transfer wifi dong bo"
    )
  );
  const showNetwork = $derived(
    matchesSearch(
      t.netProtectionSection,
      t.netProtectionSubtitle,
      t.proxySection,
      t.proxySubtitle,
      "dns doh cloudflare quad9 adguard nextdns proxy socks5 http domain rules mang bao ve"
    )
  );
  const showUpdates = $derived(
    matchesSearch(
      t.updatesSection,
      t.autoCheckLabel,
      t.autoCheckDesc,
      t.checkIntervalLabel,
      t.notificationsLabel,
      t.notificationsDesc,
      "updates chapter check interval notifications cap nhat chuong thong bao"
    )
  );
  const showAppUpdates = $derived(
    matchesSearch(
      t.appUpdatesSection,
      t.checkAppUpdatesLabel,
      t.checkAppUpdatesDesc,
      "app update release version phien ban"
    )
  );
  const showSources = $derived(
    matchesSearch(
      t.sourcesSection,
      t.headlessLabel,
      t.headlessDesc,
      t.speedLimitLabel,
      t.speedLimitDesc,
      "sources headless browser speed limit bandwidth throttle toc do tai bang thong"
    )
  );
  const showCache = $derived(
    matchesSearch(
      t.cacheSection,
      t.cacheUsageLabel,
      t.cacheBySourceTitle,
      t.cacheByMangaTitle,
      "cache disk storage clear source manga bo nho dem xoa"
    )
  );
  const showHidden = $derived(
    matchesSearch(
      t.hiddenMangaSection,
      t.hiddenMangaDesc,
      "hidden hide unhide restore manga truyen an"
    )
  );
  const showDiagnostics = $derived(
    matchesSearch(
      t.diagnosticsSection,
      t.diagnosticsDesc,
      "diagnostics crash error logs export bug nhat ky loi"
    )
  );
  const showResetAll = $derived(
    matchesSearch(
      t.resetAllSection,
      t.resetAllDesc,
      "reset default settings khoi phuc mac dinh"
    )
  );
  const hasAnySectionMatch = $derived(
    showAppearance ||
      showDevice ||
      showReader ||
      showFloating ||
      showLanSync ||
      showNetwork ||
      showUpdates ||
      showAppUpdates ||
      showSources ||
      showCache ||
      showHidden ||
      showDiagnostics ||
      showResetAll
  );

  function floatingScreenLabel(key: FloatingScreenKey): string {
    switch (key) {
      case "home":
        return t.navHome;
      case "bookmarks":
        return t.navBookmarks;
      case "history":
        return t.navHistory;
      case "manga":
        return t.floatingScreenManga;
      case "reader":
        return t.floatingScreenReader;
      case "settings":
        return t.navSettings;
    }
  }

  function speedPresetLabel(key: "unlimited" | "512k" | "1m" | "2m" | "5m" | "10m"): string {
    switch (key) {
      case "unlimited":
        return t.speedLimitUnlimited;
      case "512k":
        return "512 KB/s";
      case "1m":
        return "1 MB/s";
      case "2m":
        return "2 MB/s";
      case "5m":
        return "5 MB/s";
      case "10m":
        return "10 MB/s";
    }
  }

  function toggleScrollTopScreen(screen: FloatingScreenKey) {
    patch((d) => {
      const list = d.floating.scrollToTopScreens;
      if (list.includes(screen)) {
        d.floating.scrollToTopScreens = list.filter((x) => x !== screen);
      } else {
        d.floating.scrollToTopScreens = [...list, screen];
      }
    });
  }

  async function loadCacheInfo() {
    try {
      const [info, sources, mangas] = await Promise.all([
        api.cacheInfo(),
        api.listCacheBySource(),
        api.listCacheByManga(),
      ]);
      cache = info;
      cacheBySource = sources;
      cacheByManga = mangas;
    } catch {
      cache = null;
      cacheBySource = [];
      cacheByManga = [];
    }
  }

  async function loadHiddenManga() {
    try {
      hiddenManga = await api.listHiddenManga();
    } catch {
      hiddenManga = [];
    }
  }

  onMount(async () => {
    try {
      notifPerm = await api.checkNotificationPermission();
    } catch {
      notifPerm = { granted: true, canRequest: false };
    }
    void loadHiddenManga();
  });

  async function requestAndroidNotifPermission() {
    try {
      notifPerm = await api.requestNotificationPermission();
    } catch {
      // Ignore outside Android.
    }
  }

  $effect(() => {
    void s.cache.maxMb;
    const timer = window.setTimeout(() => {
      void loadCacheInfo();
    }, 300);
    return () => window.clearTimeout(timer);
  });

  function patch(mutator: (draft: Settings) => void, debounceMs = 0) {
    settingsStore.update(mutator, debounceMs);
  }

  async function handleResetSection(section: SettingsSection) {
    if (resettingSection) return;
    resettingSection = section;
    try {
      await settingsStore.resetSection(section);
    } finally {
      resettingSection = null;
    }
  }

  async function handleResetAll() {
    if (resettingAll) return;
    resettingAll = true;
    try {
      await settingsStore.resetAll();
      showResetAllModal = false;
    } finally {
      resettingAll = false;
    }
  }

  async function clearAllCache() {
    clearingCache = true;
    try {
      cache = await api.clearImageCache();
      cacheBySource = [];
      cacheByManga = [];
      confirmingClearAllCache = false;
      toasts.push(t.toastCacheCleared, "success");
    } catch {
      toasts.push(t.toastCacheClearFailed, "error");
    } finally {
      clearingCache = false;
    }
  }

  async function clearSourceCache(site: string) {
    const key = `site:${site}`;
    clearingScopeKey = key;
    try {
      cache = await api.clearCacheBySource(site);
      confirmingScopeKey = null;
      await loadCacheInfo();
      toasts.push(t.toastScopedCacheCleared(site), "success", 2200);
    } catch {
      toasts.push(t.toastCacheClearFailed, "error");
    } finally {
      clearingScopeKey = null;
    }
  }

  async function clearMangaCache(entry: CacheMangaStats) {
    const key = `manga:${entry.mangaId}`;
    clearingScopeKey = key;
    try {
      cache = await api.clearCacheByManga(entry.mangaId);
      confirmingScopeKey = null;
      await loadCacheInfo();
      toasts.push(t.toastScopedCacheCleared(entry.title), "success", 2200);
    } catch {
      toasts.push(t.toastCacheClearFailed, "error");
    } finally {
      clearingScopeKey = null;
    }
  }

  async function unhideManga(item: HiddenMangaView) {
    try {
      await api.setMangaHidden(item.id, "unhide");
      hiddenManga = hiddenManga.filter((m) => m.id !== item.id);
      void notificationsStore.refresh();
      toasts.push(t.toastMangaUnhidden(item.title), "success", 2200);
    } catch (err) {
      const f = formatAppError(err, settingsStore.value.language);
      toasts.push(f.detail || f.title, "error");
    }
  }

  async function confirmDeleteHiddenManga() {
    if (!pendingDeleteHidden || deletingHidden) return;
    deletingHidden = true;
    try {
      const target = pendingDeleteHidden;
      await api.deleteMangaPermanently(target.id);
      hiddenManga = hiddenManga.filter((m) => m.id !== target.id);
      pendingDeleteHidden = null;
      void notificationsStore.refresh();
      void loadCacheInfo();
      toasts.push(t.toastMangaDeleted, "info", 2400);
    } catch (err) {
      const f = formatAppError(err, settingsStore.value.language);
      toasts.push(f.detail || f.title, "error");
    } finally {
      deletingHidden = false;
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
</script>

<div class="mx-auto flex w-full max-w-3xl flex-col gap-6 px-5 py-6 sm:px-8 sm:py-8">
  <div class="flex flex-col gap-3">
    <div>
      <h1 class="text-2xl font-bold tracking-tight">{t.settingsTitle}</h1>
      <p class="mt-1 text-xs" style="color: var(--text-muted);">
        {t.settingsSubtitle}
      </p>
    </div>

    <!-- Instant Searchable Settings Bar -->
    <div
      class="flex items-center gap-2 rounded-xl border px-3.5 py-2.5 text-sm shadow-xs"
      style="background-color: var(--surface); border-color: var(--border);"
    >
      <Search size={15} class="shrink-0" style="color: var(--text-muted);" />
      <input
        type="text"
        bind:value={searchQuery}
        placeholder={t.settingsSearchPlaceholder}
        class="w-full bg-transparent text-xs outline-none sm:text-sm"
      />
      {#if searchQuery}
        <button
          type="button"
          onclick={() => {
            searchQuery = "";
          }}
          class="rounded-md p-1 opacity-70 hover:opacity-100"
          style="color: var(--text-muted);"
          aria-label={t.settingsSearchClear}
        >
          <X size={14} />
        </button>
      {/if}
    </div>
  </div>

  {#if !hasAnySectionMatch}
    <div
      class="flex flex-col items-center gap-3 rounded-2xl border border-dashed py-12 text-center"
      style="border-color: var(--border); color: var(--text-muted);"
    >
      <Search size={26} />
      <div class="text-sm font-medium" style="color: var(--text);">
        {t.settingsSearchEmpty(searchQuery)}
      </div>
      <button
        type="button"
        onclick={() => {
          searchQuery = "";
        }}
        class="rounded-lg border px-3 py-1.5 text-xs font-medium"
        style="border-color: var(--border); background-color: var(--surface);"
      >
        {t.settingsSearchClear}
      </button>
    </div>
  {/if}

  <!-- Appearance -->
  {#if showAppearance}
    <section class="settings-card">
      <div class="section-header">
        <h2 class="section-title">{t.appearanceSection}</h2>
        <button
          type="button"
          onclick={() => handleResetSection("appearance")}
          disabled={resettingSection === "appearance"}
          class="reset-btn"
          title={t.resetSectionBtn}
        >
          <RotateCcw size={12} />
          <span>{t.resetSectionBtn}</span>
        </button>
      </div>

      <div class="row">
        <div>
          <div class="label">{t.languageLabel}</div>
          <div class="desc">{t.languageDesc}</div>
        </div>
        <select
          value={s.language}
          onchange={(e) => {
            const lang = e.currentTarget.value as Language;
            patch((d) => (d.language = lang));
          }}
          class="rounded-lg border px-3 py-1.5 text-xs font-medium"
          style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
        >
          {#each LANGUAGE_OPTIONS as opt (opt.code)}
            <option value={opt.code}>{opt.label}</option>
          {/each}
        </select>
      </div>

      <div class="row">
        <div>
          <div class="label">{t.colorThemeLabel}</div>
          <div class="desc">{t.colorThemeDesc}</div>
        </div>
        <div class="flex gap-1.5">
          {#each [["system", t.themeSystem], ["light", t.themeLight], ["dark", t.themeDark]] as [val, label] (val)}
            <button
              type="button"
              onclick={() => patch((d) => (d.theme = val as Theme))}
              class="pill"
              class:pill-active={s.theme === val}
            >
              {label}
            </button>
          {/each}
        </div>
      </div>
    </section>
  {/if}

  <!-- Device Identity -->
  {#if showDevice}
    <section class="settings-card">
      <div class="section-header">
        <div class="flex items-center gap-2">
          <Smartphone size={15} style="color: var(--accent);" />
          <h2 class="section-title">{t.deviceIdentitySection}</h2>
        </div>
        <button
          type="button"
          onclick={() => handleResetSection("device")}
          disabled={resettingSection === "device"}
          class="reset-btn"
          title={t.resetSectionBtn}
        >
          <RotateCcw size={12} />
          <span>{t.resetSectionBtn}</span>
        </button>
      </div>

      <div class="row">
        <div>
          <div class="label">{t.deviceDetectedLabel}</div>
          <div class="desc">
            {deviceEnv.info.realDeviceName}
            {#if deviceEnv.info.skin && deviceEnv.info.skin !== "desktop"}
              · {deviceEnv.info.skin.toUpperCase()}
            {/if}
          </div>
        </div>
      </div>

      <div class="row">
        <div>
          <div class="label">{t.deviceCustomNameLabel}</div>
          <div class="desc">{t.deviceCustomNameDesc}</div>
        </div>
        <div class="flex items-center gap-1.5">
          <input
            type="text"
            maxlength="48"
            value={s.device.customName ?? ""}
            placeholder={deviceEnv.info.realDeviceName}
            oninput={(e) => {
              const val = e.currentTarget.value;
              patch((d) => {
                d.device.customName = val.trim() ? val : null;
              }, 300);
            }}
            class="w-44 rounded-lg border px-2.5 py-1.5 text-xs outline-none sm:w-52"
            style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
          />
          {#if s.device.customName}
            <button
              type="button"
              onclick={() => patch((d) => (d.device.customName = null))}
              class="rounded-lg border p-1.5"
              style="border-color: var(--border); background-color: var(--surface-subtle);"
              title={t.deviceResetName}
            >
              <RotateCcw size={13} />
            </button>
          {/if}
        </div>
      </div>
    </section>
  {/if}

  <!-- Reader (Collapsible Categories, Collapsed by Default) -->
  {#if showReader}
    <section id="settings-reader" class="settings-card">
      <div class="section-header">
        <h2 class="section-title">{t.readerSection}</h2>
        <button
          type="button"
          onclick={() => handleResetSection("reader")}
          disabled={resettingSection === "reader"}
          class="reset-btn"
          title={t.resetSectionBtn}
        >
          <RotateCcw size={12} />
          <span>{t.resetSectionBtn}</span>
        </button>
      </div>

      <div class="flex flex-col gap-2">
        <!-- Category 1: Layout & Spacing -->
        {#if !normalizedQuery || matchCatLayout}
          {@const openLayout = isReaderCatOpen("layout")}
          <div class="cat-box">
            <button
              type="button"
              onclick={() => toggleReaderCat("layout")}
              aria-expanded={openLayout}
              class="cat-trigger"
            >
              <div class="min-w-0 text-left">
                <div class="text-xs font-semibold sm:text-sm">{t.readerCatLayout}</div>
                <div class="truncate text-[11px]" style="color: var(--text-muted);">
                  {readerLayoutSummary()}
                </div>
              </div>
              {#if openLayout}
                <ChevronUp size={16} class="shrink-0 opacity-70" />
              {:else}
                <ChevronDown size={16} class="shrink-0 opacity-70" />
              {/if}
            </button>

            {#if openLayout}
              <div class="cat-body">
                <!-- Live Visual Preview for Margins, Gaps, No-Gap & Alignment -->
                <SettingsVisualPreview settings={s} {t} />

                <!-- Orientation -->
                <div class="row">
                  <div>
                    <div class="label">{t.readingModeLabel}</div>
                    <div class="desc">{t.readingModeDesc}</div>
                  </div>
                  <div class="flex gap-1.5">
                    {#each [["vertical", t.modeVertical], ["horizontal", t.modeHorizontal]] as [val, label] (val)}
                      <button
                        type="button"
                        onclick={() => patch((d) => (d.reader.orientation = val as ReadingOrientation))}
                        class="pill"
                        class:pill-active={s.reader.orientation === val}
                      >
                        {label}
                      </button>
                    {/each}
                  </div>
                </div>

                {#if !isHorizontal}
                  <!-- No-Gap Mode -->
                  <div class="row">
                    <div>
                      <div class="label">{t.noGapModeLabel}</div>
                      <div class="desc">{t.noGapModeDesc}</div>
                    </div>
                    <input
                      type="checkbox"
                      checked={s.reader.vertical.noGapMode}
                      onchange={(e) => {
                        const checked = e.currentTarget.checked;
                        patch((d) => {
                          d.reader.vertical.noGapMode = checked;
                          if (!checked && d.reader.vertical.pageGapPx === 0) {
                            d.reader.vertical.pageGapPx = 12;
                            d.reader.gapPx = 12;
                          }
                        });
                      }}
                      class="h-4 w-4 accent-rose-500"
                    />
                  </div>

                  <!-- Continuous vertical strip page gap -->
                  {#if !s.reader.vertical.noGapMode}
                    <div class="row">
                      <div>
                        <div class="label">{t.gapLabel(s.reader.vertical.pageGapPx)}</div>
                        <div class="desc">{t.gapDesc}</div>
                      </div>
                      <input
                        type="range"
                        min="0"
                        max="48"
                        step="2"
                        value={s.reader.vertical.pageGapPx}
                        oninput={(e) => {
                          const v = Number(e.currentTarget.value);
                          patch((d) => {
                            d.reader.vertical.pageGapPx = v;
                            d.reader.gapPx = v;
                          }, 150);
                        }}
                        class="w-40 accent-rose-500 sm:w-48"
                      />
                    </div>
                  {/if}

                  <!-- Vertical side margin -->
                  <div class="row">
                    <div>
                      <div class="label">{t.verticalMarginLabel(s.reader.behavior.verticalMarginPx)}</div>
                      <div class="desc">{t.verticalMarginDesc}</div>
                    </div>
                    <input
                      type="range"
                      min="0"
                      max="64"
                      step="2"
                      value={s.reader.behavior.verticalMarginPx}
                      oninput={(e) => {
                        const v = Number(e.currentTarget.value);
                        patch((d) => (d.reader.behavior.verticalMarginPx = v), 150);
                      }}
                      class="w-40 accent-rose-500 sm:w-48"
                    />
                  </div>

                  <div class="row">
                    <div>
                      <div class="label">{t.autoAppendNext}</div>
                      <div class="desc">{t.autoAppendDesc}</div>
                    </div>
                    <input
                      type="checkbox"
                      checked={s.reader.autoNextChapter}
                      onchange={(e) => {
                        const checked = e.currentTarget.checked;
                        patch((d) => (d.reader.autoNextChapter = checked));
                      }}
                      class="h-4 w-4 accent-rose-500"
                    />
                  </div>
                {:else}
                  <!-- Horizontal Mode Controls -->
                  <div class="row">
                    <div>
                      <div class="label">{t.horizontalDirLabel}</div>
                      <div class="desc">{t.horizontalDirDesc}</div>
                    </div>
                    <div class="flex flex-wrap gap-1.5">
                      {#each [["ltr", t.dirLtr], ["rtl", t.dirRtl]] as [val, label] (val)}
                        <button
                          type="button"
                          onclick={() =>
                            patch((d) => (d.reader.horizontal.direction = val as HorizontalDirection))}
                          class="pill"
                          class:pill-active={s.reader.horizontal.direction === val}
                        >
                          {label}
                        </button>
                      {/each}
                    </div>
                  </div>

                  <div class="row">
                    <div>
                      <div class="label">{t.pagesPerGroupLabel}</div>
                      <div class="desc">{t.pagesPerGroupDesc}</div>
                    </div>
                    <div class="flex items-center gap-1.5">
                      {#each [1, 2, 3] as n (n)}
                        <button
                          type="button"
                          onclick={() => patch((d) => (d.reader.horizontal.pagesPerGroup = n))}
                          class="pill"
                          class:pill-active={s.reader.horizontal.pagesPerGroup === n}
                        >
                          {n}
                        </button>
                      {/each}
                      <input
                        type="number"
                        min="1"
                        max="8"
                        value={s.reader.horizontal.pagesPerGroup}
                        onchange={(e) => {
                          const v = Math.max(1, Math.min(8, Math.round(Number(e.currentTarget.value) || 1)));
                          patch((d) => (d.reader.horizontal.pagesPerGroup = v));
                        }}
                        class="w-14 rounded-lg border px-2 py-1 text-center text-xs"
                        style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
                      />
                    </div>
                  </div>

                  <div class="row">
                    <div>
                      <div class="label">{t.pageGapLabel(s.reader.horizontal.pageGapPx)}</div>
                      <div class="desc">{t.pageGapDesc}</div>
                    </div>
                    <input
                      type="range"
                      min="0"
                      max="48"
                      step="2"
                      value={s.reader.horizontal.pageGapPx}
                      oninput={(e) => {
                        const v = Number(e.currentTarget.value);
                        patch((d) => (d.reader.horizontal.pageGapPx = v), 150);
                      }}
                      class="w-40 accent-rose-500 sm:w-48"
                    />
                  </div>

                  <div class="row">
                    <div>
                      <div class="label">{t.groupGapLabel(s.reader.horizontal.groupGapPx)}</div>
                      <div class="desc">{t.groupGapDesc}</div>
                    </div>
                    <input
                      type="range"
                      min="8"
                      max="96"
                      step="4"
                      value={s.reader.horizontal.groupGapPx}
                      oninput={(e) => {
                        const v = Number(e.currentTarget.value);
                        patch((d) => (d.reader.horizontal.groupGapPx = v), 150);
                      }}
                      class="w-40 accent-rose-500 sm:w-48"
                    />
                  </div>

                  <div class="row">
                    <div>
                      <div class="label">{t.snapModeLabel}</div>
                      <div class="desc">{t.snapModeDesc}</div>
                    </div>
                    <div class="flex gap-1.5">
                      {#each [["none", t.snapNone], ["page", t.snapPage], ["group", t.snapGroup]] as [val, label] (val)}
                        <button
                          type="button"
                          onclick={() =>
                            patch((d) => (d.reader.horizontal.snapMode = val as HorizontalSnapMode))}
                          class="pill"
                          class:pill-active={s.reader.horizontal.snapMode === val}
                        >
                          {label}
                        </button>
                      {/each}
                    </div>
                  </div>

                  <div class="row">
                    <div>
                      <div class="label">{t.centerGroupsLabel}</div>
                      <div class="desc">{t.centerGroupsDesc}</div>
                    </div>
                    <input
                      type="checkbox"
                      checked={s.reader.horizontal.centerGroups}
                      onchange={(e) => {
                        const checked = e.currentTarget.checked;
                        patch((d) => (d.reader.horizontal.centerGroups = checked));
                      }}
                      class="h-4 w-4 accent-rose-500"
                    />
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        {/if}

        <!-- Category 2: Display & Visuals -->
        {#if !normalizedQuery || matchCatDisplay}
          {@const openDisplay = isReaderCatOpen("display")}
          <div class="cat-box">
            <button
              type="button"
              onclick={() => toggleReaderCat("display")}
              aria-expanded={openDisplay}
              class="cat-trigger"
            >
              <div class="min-w-0 text-left">
                <div class="text-xs font-semibold sm:text-sm">{t.readerCatDisplay}</div>
                <div class="truncate text-[11px]" style="color: var(--text-muted);">
                  {readerDisplaySummary()}
                </div>
              </div>
              {#if openDisplay}
                <ChevronUp size={16} class="shrink-0 opacity-70" />
              {:else}
                <ChevronDown size={16} class="shrink-0 opacity-70" />
              {/if}
            </button>

            {#if openDisplay}
              <div class="cat-body">
                <div class="row">
                  <div>
                    <div class="label">{t.readerBg}</div>
                    <div class="desc">{t.readerBgDesc}</div>
                  </div>
                  <div class="flex gap-1.5">
                    {#each [["dark", t.bgDark], ["light", t.bgLight], ["follow_theme", t.bgAppTheme]] as [val, label] (val)}
                      <button
                        type="button"
                        onclick={() => patch((d) => (d.reader.readerBg = val as ReaderBg))}
                        class="pill"
                        class:pill-active={s.reader.readerBg === val}
                      >
                        {label}
                      </button>
                    {/each}
                  </div>
                </div>

                {#if !isHorizontal}
                  <div class="row">
                    <div>
                      <div class="label">{t.dimUnfocusedLabel}</div>
                      <div class="desc">{t.dimUnfocusedDesc}</div>
                    </div>
                    <input
                      type="checkbox"
                      checked={s.reader.vertical.dimUnfocused}
                      onchange={(e) => {
                        const checked = e.currentTarget.checked;
                        patch((d) => (d.reader.vertical.dimUnfocused = checked));
                      }}
                      class="h-4 w-4 accent-rose-500"
                    />
                  </div>

                  {#if s.reader.vertical.dimUnfocused}
                    <div class="row">
                      <div>
                        <div class="label">{t.dimLevelLabel(s.reader.vertical.dimLevel)}</div>
                        <div class="desc">{t.dimLevelDesc}</div>
                      </div>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        step="5"
                        value={s.reader.vertical.dimLevel}
                        oninput={(e) => {
                          const v = Number(e.currentTarget.value);
                          patch((d) => (d.reader.vertical.dimLevel = v), 120);
                        }}
                        class="w-40 accent-rose-500 sm:w-48"
                      />
                    </div>
                  {/if}
                {:else}
                  <div class="row">
                    <div>
                      <div class="label">{t.dimUnfocusedLabel}</div>
                      <div class="desc">{t.dimUnfocusedDesc}</div>
                    </div>
                    <input
                      type="checkbox"
                      checked={s.reader.horizontal.dimUnfocused}
                      onchange={(e) => {
                        const checked = e.currentTarget.checked;
                        patch((d) => (d.reader.horizontal.dimUnfocused = checked));
                      }}
                      class="h-4 w-4 accent-rose-500"
                    />
                  </div>

                  {#if s.reader.horizontal.dimUnfocused}
                    <div class="row">
                      <div>
                        <div class="label">{t.dimLevelLabel(s.reader.horizontal.dimLevel)}</div>
                        <div class="desc">{t.dimLevelDesc}</div>
                      </div>
                      <input
                        type="range"
                        min="0"
                        max="100"
                        step="5"
                        value={s.reader.horizontal.dimLevel}
                        oninput={(e) => {
                          const v = Number(e.currentTarget.value);
                          patch((d) => (d.reader.horizontal.dimLevel = v), 120);
                        }}
                        class="w-40 accent-rose-500 sm:w-48"
                      />
                    </div>
                  {/if}
                {/if}

                <div class="row">
                  <div>
                    <div class="label">{t.autoWebtoonLabel}</div>
                    <div class="desc">{t.autoWebtoonDesc}</div>
                  </div>
                  <input
                    type="checkbox"
                    checked={s.reader.autoWebtoonDetect}
                    onchange={(e) => {
                      const checked = e.currentTarget.checked;
                      patch((d) => (d.reader.autoWebtoonDetect = checked));
                    }}
                    class="h-4 w-4 accent-rose-500"
                  />
                </div>
              </div>
            {/if}
          </div>
        {/if}

        <!-- Category 3: Controls & Gestures -->
        {#if !normalizedQuery || matchCatControls}
          {@const openControls = isReaderCatOpen("controls")}
          <div class="cat-box">
            <button
              type="button"
              onclick={() => toggleReaderCat("controls")}
              aria-expanded={openControls}
              class="cat-trigger"
            >
              <div class="min-w-0 text-left">
                <div class="text-xs font-semibold sm:text-sm">{t.readerCatControls}</div>
                <div class="truncate text-[11px]" style="color: var(--text-muted);">
                  {readerControlsSummary()}
                </div>
              </div>
              {#if openControls}
                <ChevronUp size={16} class="shrink-0 opacity-70" />
              {:else}
                <ChevronDown size={16} class="shrink-0 opacity-70" />
              {/if}
            </button>

            {#if openControls}
              <div class="cat-body">
                <div class="row">
                  <div>
                    <div class="label">{t.doubleTapActionLabel}</div>
                    <div class="desc">{t.doubleTapActionDesc}</div>
                  </div>
                  <select
                    value={s.reader.behavior.doubleTapAction}
                    onchange={(e) => {
                      const action = e.currentTarget.value as DoubleTapAction;
                      patch((d) => (d.reader.behavior.doubleTapAction = action));
                    }}
                    class="rounded-lg border px-3 py-1.5 text-xs font-medium"
                    style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
                  >
                    <option value="zoom">{t.doubleTapZoom}</option>
                    <option value="toggle_toolbar">{t.doubleTapToggleToolbar}</option>
                    <option value="next_page">{t.doubleTapNextPage}</option>
                    <option value="disabled">{t.doubleTapDisabled}</option>
                  </select>
                </div>

                <div class="row">
                  <div>
                    <div class="label">{t.tapZonesLabel}</div>
                    <div class="desc">{t.tapZonesDesc}</div>
                  </div>
                  <input
                    type="checkbox"
                    checked={s.reader.tapZonesEnabled}
                    onchange={(e) => {
                      const checked = e.currentTarget.checked;
                      patch((d) => (d.reader.tapZonesEnabled = checked));
                    }}
                    class="h-4 w-4 accent-rose-500"
                  />
                </div>

                <div class="row">
                  <div>
                    <div class="label">{t.toolbarAutoHideLabel}</div>
                    <div class="desc">{t.toolbarAutoHideDesc}</div>
                  </div>
                  <select
                    value={s.reader.behavior.toolbarAutoHideSecs}
                    onchange={(e) => {
                      const secs = Number(e.currentTarget.value);
                      patch((d) => (d.reader.behavior.toolbarAutoHideSecs = secs));
                    }}
                    class="rounded-lg border px-3 py-1.5 text-xs font-medium"
                    style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
                  >
                    <option value={0}>{t.toolbarAutoHideOff}</option>
                    {#each [2, 3, 5, 8, 12] as sec (sec)}
                      <option value={sec}>{t.secondsOption(sec)}</option>
                    {/each}
                  </select>
                </div>

                <div class="row">
                  <div>
                    <div class="label">{t.keepScreenOnLabel}</div>
                    <div class="desc">{t.keepScreenOnDesc}</div>
                  </div>
                  <input
                    type="checkbox"
                    checked={s.reader.behavior.keepScreenOn}
                    onchange={(e) => {
                      const checked = e.currentTarget.checked;
                      patch((d) => (d.reader.behavior.keepScreenOn = checked));
                    }}
                    class="h-4 w-4 accent-rose-500"
                  />
                </div>
              </div>
            {/if}
          </div>
        {/if}

        <!-- Category 4: Reading & Performance -->
        {#if !normalizedQuery || matchCatBehavior}
          {@const openBehavior = isReaderCatOpen("behavior")}
          <div class="cat-box">
            <button
              type="button"
              onclick={() => toggleReaderCat("behavior")}
              aria-expanded={openBehavior}
              class="cat-trigger"
            >
              <div class="min-w-0 text-left">
                <div class="text-xs font-semibold sm:text-sm">{t.readerCatBehavior}</div>
                <div class="truncate text-[11px]" style="color: var(--text-muted);">
                  {readerBehaviorSummary()}
                </div>
              </div>
              {#if openBehavior}
                <ChevronUp size={16} class="shrink-0 opacity-70" />
              {:else}
                <ChevronDown size={16} class="shrink-0 opacity-70" />
              {/if}
            </button>

            {#if openBehavior}
              <div class="cat-body">
                <div class="row">
                  <div>
                    <div class="label">{t.autoMarkReadLabel}</div>
                    <div class="desc">{t.autoMarkReadDesc}</div>
                  </div>
                  <input
                    type="checkbox"
                    checked={s.reader.behavior.autoMarkReadOnOpen}
                    onchange={(e) => {
                      const checked = e.currentTarget.checked;
                      patch((d) => (d.reader.behavior.autoMarkReadOnOpen = checked));
                    }}
                    class="h-4 w-4 accent-rose-500"
                  />
                </div>

                <div class="row">
                  <div>
                    <div class="label">{t.skipBrokenImagesLabel}</div>
                    <div class="desc">{t.skipBrokenImagesDesc}</div>
                  </div>
                  <input
                    type="checkbox"
                    checked={s.reader.behavior.skipBrokenImages}
                    onchange={(e) => {
                      const checked = e.currentTarget.checked;
                      patch((d) => (d.reader.behavior.skipBrokenImages = checked));
                    }}
                    class="h-4 w-4 accent-rose-500"
                  />
                </div>

                <div class="row">
                  <div>
                    <div class="label">{t.preloadLabel(s.reader.preloadImages)}</div>
                    <div class="desc">{t.preloadDesc}</div>
                  </div>
                  <input
                    type="range"
                    min="5"
                    max="8"
                    step="1"
                    value={s.reader.preloadImages}
                    oninput={(e) => {
                      const v = Number(e.currentTarget.value);
                      patch((d) => (d.reader.preloadImages = v), 150);
                    }}
                    class="w-40 accent-rose-500 sm:w-48"
                  />
                </div>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </section>
  {/if}

  <!-- Floating Controls Customization -->
  {#if showFloating}
    <section id="settings-floating" class="settings-card">
      <div class="section-header">
        <div>
          <h2 class="section-title">{t.floatingSection}</h2>
          <div class="desc">{t.floatingSubtitle}</div>
        </div>
        <button
          type="button"
          onclick={() => handleResetSection("floating")}
          disabled={resettingSection === "floating"}
          class="reset-btn"
          title={t.resetSectionBtn}
        >
          <RotateCcw size={12} />
          <span>{t.resetSectionBtn}</span>
        </button>
      </div>

      <div class="flex flex-col gap-2">
        <div class="label">{t.floatingScrollTopScreens}</div>
        <div class="flex flex-wrap gap-1.5">
          {#each floatingScreens as screen (screen)}
            {@const active = s.floating.scrollToTopScreens.includes(screen)}
            <button
              type="button"
              onclick={() => toggleScrollTopScreen(screen)}
              class="pill"
              class:pill-active={active}
            >
              {floatingScreenLabel(screen)}
            </button>
          {/each}
        </div>
      </div>

      <div class="row">
        <div>
          <div class="label">{t.floatingScrollTopPos}</div>
          <div class="desc">{t.floatingPositionLabel}</div>
        </div>
        <div class="flex flex-wrap items-center gap-2">
          {#each [["left", t.floatingEdgeLeft], ["right", t.floatingEdgeRight]] as [edge, label] (edge)}
            <button
              type="button"
              onclick={() => patch((d) => (d.floating.scrollToTop.edge = edge as FloatingEdge))}
              class="pill"
              class:pill-active={s.floating.scrollToTop.edge === edge}
            >
              {label}
            </button>
          {/each}
          <input
            type="range"
            min="15"
            max="88"
            step="1"
            value={s.floating.scrollToTop.verticalPercent}
            oninput={(e) => {
              const v = Number(e.currentTarget.value);
              patch((d) => (d.floating.scrollToTop.verticalPercent = v), 120);
            }}
            class="w-28 accent-rose-500"
            title="{s.floating.scrollToTop.verticalPercent}%"
          />
          <span class="w-9 text-right text-xs" style="color: var(--text-muted);">
            {s.floating.scrollToTop.verticalPercent}%
          </span>
        </div>
      </div>

      <div class="row">
        <div>
          <div class="label">{t.floatingReaderSettingsLabel}</div>
          <div class="desc">{t.floatingReaderSettingsDesc}</div>
        </div>
        <input
          type="checkbox"
          checked={s.floating.showReaderSettings}
          onchange={(e) => {
            const checked = e.currentTarget.checked;
            patch((d) => (d.floating.showReaderSettings = checked));
          }}
          class="h-4 w-4 accent-rose-500"
        />
      </div>

      {#if s.floating.showReaderSettings}
        <div class="row">
          <div class="desc">{t.floatingPositionLabel}</div>
          <div class="flex flex-wrap items-center gap-2">
            {#each [["left", t.floatingEdgeLeft], ["right", t.floatingEdgeRight]] as [edge, label] (edge)}
              <button
                type="button"
                onclick={() => patch((d) => (d.floating.readerSettings.edge = edge as FloatingEdge))}
                class="pill"
                class:pill-active={s.floating.readerSettings.edge === edge}
              >
                {label}
              </button>
            {/each}
            <input
              type="range"
              min="15"
              max="88"
              step="1"
              value={s.floating.readerSettings.verticalPercent}
              oninput={(e) => {
                const v = Number(e.currentTarget.value);
                patch((d) => (d.floating.readerSettings.verticalPercent = v), 120);
              }}
              class="w-28 accent-rose-500"
            />
            <span class="w-9 text-right text-xs" style="color: var(--text-muted);">
              {s.floating.readerSettings.verticalPercent}%
            </span>
          </div>
        </div>
      {/if}

      <div class="row">
        <div>
          <div class="label">{t.floatingOrientationLockLabel}</div>
          <div class="desc">{t.floatingOrientationLockDesc}</div>
        </div>
        <input
          type="checkbox"
          checked={s.floating.showOrientationLock}
          onchange={(e) => {
            const checked = e.currentTarget.checked;
            patch((d) => (d.floating.showOrientationLock = checked));
          }}
          class="h-4 w-4 accent-rose-500"
        />
      </div>

      {#if s.floating.showOrientationLock}
        <div class="row">
          <div class="desc">{t.floatingPositionLabel}</div>
          <div class="flex flex-wrap items-center gap-2">
            {#each [["left", t.floatingEdgeLeft], ["right", t.floatingEdgeRight]] as [edge, label] (edge)}
              <button
                type="button"
                onclick={() => patch((d) => (d.floating.orientationLock.edge = edge as FloatingEdge))}
                class="pill"
                class:pill-active={s.floating.orientationLock.edge === edge}
              >
                {label}
              </button>
            {/each}
            <input
              type="range"
              min="15"
              max="88"
              step="1"
              value={s.floating.orientationLock.verticalPercent}
              oninput={(e) => {
                const v = Number(e.currentTarget.value);
                patch((d) => (d.floating.orientationLock.verticalPercent = v), 120);
              }}
              class="w-28 accent-rose-500"
            />
            <span class="w-9 text-right text-xs" style="color: var(--text-muted);">
              {s.floating.orientationLock.verticalPercent}%
            </span>
          </div>
        </div>
      {/if}

      <div class="row">
        <div>
          <div class="label">{t.floatingAutoDimLabel}</div>
          <div class="desc">{t.floatingAutoDimDesc}</div>
        </div>
        <input
          type="checkbox"
          checked={s.floating.autoDimInReader}
          onchange={(e) => {
            const checked = e.currentTarget.checked;
            patch((d) => (d.floating.autoDimInReader = checked));
          }}
          class="h-4 w-4 accent-rose-500"
        />
      </div>
    </section>
  {/if}

  <!-- Serverless LAN Sync -->
  {#if showLanSync}
    <LanSyncPanel />
  {/if}

  <!-- Provider-Based Network Protection (Encrypted DNS + Advanced Proxy) -->
  {#if showNetwork}
    <div id="settings-network">
      <NetworkProtectionPanel />
    </div>
  {/if}

  <!-- Updates -->
  {#if showUpdates}
    <section class="settings-card">
      <div class="section-header">
        <h2 class="section-title">{t.updatesSection}</h2>
        <div class="flex items-center gap-2">
          <button
            type="button"
            onclick={() => updatesStore.checkNow()}
            disabled={updatesStore.checking}
            class="inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium"
            style="border-color: var(--border); background-color: var(--surface-subtle);"
          >
            <RefreshCw size={13} class={updatesStore.checking ? "animate-spin" : ""} />
            <span>{t.checkNow}</span>
          </button>
          <button
            type="button"
            onclick={() => handleResetSection("updates")}
            disabled={resettingSection === "updates"}
            class="reset-btn"
            title={t.resetSectionBtn}
          >
            <RotateCcw size={12} />
            <span>{t.resetSectionBtn}</span>
          </button>
        </div>
      </div>

      <div class="row">
        <div>
          <div class="label">{t.autoCheckLabel}</div>
          <div class="desc">{t.autoCheckDesc}</div>
        </div>
        <input
          type="checkbox"
          checked={s.updates.enabled}
          onchange={(e) => {
            const checked = e.currentTarget.checked;
            patch((d) => (d.updates.enabled = checked));
          }}
          class="h-4 w-4 accent-rose-500"
        />
      </div>

      <div class="row">
        <div>
          <div class="label">{t.checkIntervalLabel}</div>
          <div class="desc">{t.checkIntervalDesc}</div>
        </div>
        <select
          value={s.updates.intervalHours}
          onchange={(e) => {
            const v = Number(e.currentTarget.value);
            patch((d) => (d.updates.intervalHours = v));
          }}
          class="rounded-lg border px-3 py-1.5 text-xs"
          style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
        >
          {#each [1, 3, 6, 12, 24] as hrs (hrs)}
            <option value={hrs}>{t.everyHours(hrs)}</option>
          {/each}
        </select>
      </div>

      <div class="row">
        <div>
          <div class="label">{t.notificationsLabel}</div>
          <div class="desc">{t.notificationsDesc}</div>
        </div>
        <div class="flex items-center gap-2">
          {#if deviceEnv.isAndroid && !notifPerm.granted}
            <button
              type="button"
              onclick={requestAndroidNotifPermission}
              class="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1 text-xs font-medium"
              style="border-color: var(--accent); color: var(--accent); background-color: var(--accent-soft);"
            >
              <Bell size={12} />
              <span>{t.notifGrantPermissionBtn}</span>
            </button>
          {/if}
          <input
            type="checkbox"
            checked={s.updates.notifyWindows}
            onchange={(e) => {
              const checked = e.currentTarget.checked;
              patch((d) => (d.updates.notifyWindows = checked));
            }}
            class="h-4 w-4 accent-rose-500"
          />
        </div>
      </div>
    </section>
  {/if}

  <!-- App Release Updates -->
  {#if showAppUpdates}
    <section class="settings-card">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h2 class="section-title">{t.appUpdatesSection}</h2>
          <div class="desc">{t.appVersionLabel(appReleaseStore.currentVersion)}</div>
        </div>

        <div class="flex items-center gap-2">
          {#if appReleaseStore.release?.updateAvailable}
            <button
              type="button"
              onclick={() => appReleaseStore.openPrompt()}
              class="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-semibold text-white"
              style="background-color: var(--accent);"
            >
              <Download size={13} />
              <span>{t.updateDownloadAndInstall} (v{appReleaseStore.release.latestVersion})</span>
            </button>
          {/if}

          <button
            type="button"
            onclick={() => appReleaseStore.checkNow()}
            disabled={appReleaseStore.checking}
            class="inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium"
            style="border-color: var(--border); background-color: var(--surface-subtle);"
          >
            <RefreshCw size={13} class={appReleaseStore.checking ? "animate-spin" : ""} />
            <span>{t.checkReleaseNow}</span>
          </button>
        </div>
      </div>

      {#if appReleaseStore.release?.updateAvailable}
        <div
          class="flex items-center gap-2 rounded-xl border px-3 py-2 text-xs font-medium"
          style="background-color: var(--accent-soft); border-color: var(--accent); color: var(--accent);"
        >
          <Sparkles size={14} class="shrink-0" />
          <span>{t.releaseAvailableBanner(appReleaseStore.release.latestVersion)}</span>
        </div>
      {/if}

      <div class="row">
        <div>
          <div class="label">{t.checkAppUpdatesLabel}</div>
          <div class="desc">{t.checkAppUpdatesDesc}</div>
        </div>
        <input
          type="checkbox"
          checked={s.updates.checkAppUpdates}
          onchange={(e) => {
            const checked = e.currentTarget.checked;
            patch((d) => (d.updates.checkAppUpdates = checked));
          }}
          class="h-4 w-4 accent-rose-500"
        />
      </div>
    </section>
  {/if}

  <!-- Sources & Network Speed Limit -->
  {#if showSources}
    <section class="settings-card">
      <div class="section-header">
        <div class="flex items-center gap-2">
          <Gauge size={15} style="color: var(--accent);" />
          <h2 class="section-title">{t.sourcesSection}</h2>
        </div>
        <button
          type="button"
          onclick={() => handleResetSection("sources")}
          disabled={resettingSection === "sources"}
          class="reset-btn"
          title={t.resetSectionBtn}
        >
          <RotateCcw size={12} />
          <span>{t.resetSectionBtn}</span>
        </button>
      </div>

      <div class="row">
        <div>
          <div class="label">{t.headlessLabel}</div>
          <div class="desc">{t.headlessDesc}</div>
        </div>
        <input
          type="checkbox"
          checked={s.sources.headlessEnabled}
          onchange={(e) => {
            const checked = e.currentTarget.checked;
            patch((d) => (d.sources.headlessEnabled = checked));
          }}
          class="h-4 w-4 accent-rose-500"
        />
      </div>

      <div class="flex flex-col gap-2.5 border-t pt-3" style="border-color: var(--border);">
        <div class="row">
          <div>
            <div class="label">{t.speedLimitLabel}</div>
            <div class="desc">{t.speedLimitDesc}</div>
          </div>
          <div class="flex items-center gap-1.5">
            <input
              type="number"
              min="0"
              max="102400"
              step="128"
              value={s.sources.speedLimitKbps}
              onchange={(e) => {
                const raw = Math.max(0, Math.min(102400, Math.round(Number(e.currentTarget.value) || 0)));
                patch((d) => (d.sources.speedLimitKbps = raw));
              }}
              class="w-24 rounded-lg border px-2 py-1 text-right text-xs"
              style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
            />
            <span class="text-xs" style="color: var(--text-muted);">KB/s</span>
          </div>
        </div>

        <div class="flex flex-wrap gap-1.5">
          {#each speedPresets as preset (preset.kbps)}
            <button
              type="button"
              onclick={() => patch((d) => (d.sources.speedLimitKbps = preset.kbps))}
              class="pill"
              class:pill-active={s.sources.speedLimitKbps === preset.kbps}
            >
              {speedPresetLabel(preset.labelKey)}
            </button>
          {/each}
        </div>
      </div>
    </section>
  {/if}

  <!-- Cache & Scoped Storage Management -->
  {#if showCache}
    <section class="settings-card">
      <div class="section-header">
        <div class="flex items-center gap-2">
          <HardDrive size={15} style="color: var(--accent);" />
          <h2 class="section-title">{t.cacheSection}</h2>
        </div>
        <button
          type="button"
          onclick={() => handleResetSection("cache")}
          disabled={resettingSection === "cache"}
          class="reset-btn"
          title={t.resetSectionBtn}
        >
          <RotateCcw size={12} />
          <span>{t.resetSectionBtn}</span>
        </button>
      </div>

      <div class="row">
        <div>
          <div class="label">{t.cacheUsageLabel}</div>
          <div class="desc">
            {#if cache}
              {t.cacheUsageDesc(formatBytes(cache.bytes), cache.files)}
            {:else}
              {t.calculating}
            {/if}
          </div>
          <div class="desc">{t.cachePreservesUserDataNote}</div>
        </div>
        {#if confirmingClearAllCache}
          <div class="flex shrink-0 items-center gap-1.5">
            <button
              type="button"
              onclick={clearAllCache}
              disabled={clearingCache}
              class="rounded-lg px-3 py-1.5 text-xs font-semibold text-white"
              style="background-color: var(--danger);"
            >
              {clearingCache ? t.clearing : t.confirmClear}
            </button>
            <button
              type="button"
              onclick={() => {
                confirmingClearAllCache = false;
              }}
              class="rounded-lg border px-2.5 py-1.5 text-xs"
              style="border-color: var(--border);"
            >
              {t.cancel}
            </button>
          </div>
        {:else}
          <button
            type="button"
            onclick={() => {
              confirmingClearAllCache = true;
            }}
            disabled={clearingCache}
            class="inline-flex shrink-0 items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium"
            style="border-color: var(--border); color: var(--danger);"
          >
            <Trash2 size={14} />
            <span>{t.clearCache}</span>
          </button>
        {/if}
      </div>

      <div class="row">
        <div>
          <div class="label">{t.maxCacheLabel(s.cache.maxMb)}</div>
          <div class="desc">{t.maxCacheDesc}</div>
        </div>
        <input
          type="range"
          min="256"
          max="4096"
          step="256"
          value={s.cache.maxMb}
          oninput={(e) => {
            const v = Number(e.currentTarget.value);
            patch((d) => (d.cache.maxMb = v), 150);
          }}
          class="w-40 accent-rose-500 sm:w-48"
        />
      </div>

      <!-- Scoped Clear: By Source -->
      {#if cacheBySource.length > 0}
        <div class="flex flex-col gap-2 border-t pt-3" style="border-color: var(--border);">
          <div>
            <div class="label">{t.cacheBySourceTitle}</div>
            <div class="desc">{t.cacheBySourceDesc}</div>
          </div>
          <div class="flex max-h-48 flex-col gap-1.5 overflow-y-auto">
            {#each cacheBySource as item (item.site)}
              {@const scopeKey = `site:${item.site}`}
              <div
                class="flex items-center justify-between gap-2 rounded-lg border px-3 py-2 text-xs"
                style="border-color: var(--border); background-color: var(--surface-subtle);"
              >
                <div class="min-w-0">
                  <div class="truncate font-medium">{item.site}</div>
                  <div class="text-[11px]" style="color: var(--text-muted);">
                    {t.cacheUsageDesc(formatBytes(item.bytes), item.files)}
                  </div>
                </div>
                {#if confirmingScopeKey === scopeKey}
                  <div class="flex shrink-0 items-center gap-1">
                    <button
                      type="button"
                      disabled={clearingScopeKey === scopeKey}
                      onclick={() => clearSourceCache(item.site)}
                      class="rounded-md px-2 py-1 text-[11px] font-semibold text-white"
                      style="background-color: var(--danger);"
                    >
                      {t.confirmClear}
                    </button>
                    <button
                      type="button"
                      onclick={() => {
                        confirmingScopeKey = null;
                      }}
                      class="rounded-md border px-2 py-1 text-[11px]"
                      style="border-color: var(--border);"
                    >
                      {t.cancel}
                    </button>
                  </div>
                {:else}
                  <button
                    type="button"
                    onclick={() => {
                      confirmingScopeKey = scopeKey;
                    }}
                    class="shrink-0 rounded-md border px-2.5 py-1 text-[11px] font-medium"
                    style="border-color: var(--border); color: var(--danger);"
                  >
                    {t.clearCache}
                  </button>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Scoped Clear: By Manga -->
      {#if cacheByManga.length > 0}
        <div class="flex flex-col gap-2 border-t pt-3" style="border-color: var(--border);">
          <div>
            <div class="label">{t.cacheByMangaTitle}</div>
            <div class="desc">{t.cacheByMangaDesc}</div>
          </div>
          <div class="flex max-h-52 flex-col gap-1.5 overflow-y-auto">
            {#each cacheByManga as item (item.mangaId)}
              {@const scopeKey = `manga:${item.mangaId}`}
              <div
                class="flex items-center justify-between gap-2 rounded-lg border px-3 py-2 text-xs"
                style="border-color: var(--border); background-color: var(--surface-subtle);"
              >
                <div class="min-w-0">
                  <div class="truncate font-medium">{item.title}</div>
                  <div class="text-[11px]" style="color: var(--text-muted);">
                    {item.site} · {t.cacheUsageDesc(formatBytes(item.bytes), item.files)}
                  </div>
                </div>
                {#if confirmingScopeKey === scopeKey}
                  <div class="flex shrink-0 items-center gap-1">
                    <button
                      type="button"
                      disabled={clearingScopeKey === scopeKey}
                      onclick={() => clearMangaCache(item)}
                      class="rounded-md px-2 py-1 text-[11px] font-semibold text-white"
                      style="background-color: var(--danger);"
                    >
                      {t.confirmClear}
                    </button>
                    <button
                      type="button"
                      onclick={() => {
                        confirmingScopeKey = null;
                      }}
                      class="rounded-md border px-2 py-1 text-[11px]"
                      style="border-color: var(--border);"
                    >
                      {t.cancel}
                    </button>
                  </div>
                {:else}
                  <button
                    type="button"
                    onclick={() => {
                      confirmingScopeKey = scopeKey;
                    }}
                    class="shrink-0 rounded-md border px-2.5 py-1 text-[11px] font-medium"
                    style="border-color: var(--border); color: var(--danger);"
                  >
                    {t.clearCache}
                  </button>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </section>
  {/if}

  <!-- Hidden Manga Management -->
  {#if showHidden}
    <section class="settings-card">
      <div class="section-header">
        <div class="flex items-center gap-2">
          <EyeOff size={15} style="color: var(--accent);" />
          <div>
            <h2 class="section-title">{t.hiddenMangaSection}</h2>
            <div class="desc">{t.hiddenMangaDesc}</div>
          </div>
        </div>
      </div>

      {#if hiddenManga.length === 0}
        <div
          class="rounded-xl border border-dashed px-4 py-5 text-center text-xs"
          style="border-color: var(--border); color: var(--text-muted);"
        >
          {t.hiddenMangaEmpty}
        </div>
      {:else}
        <div class="flex flex-col gap-2">
          {#each hiddenManga as item (item.id)}
            <div
              class="flex items-center justify-between gap-3 rounded-xl border p-2.5 text-xs"
              style="border-color: var(--border); background-color: var(--surface-subtle);"
            >
              <div class="flex min-w-0 items-center gap-3">
                <div
                  class="h-12 w-9 shrink-0 overflow-hidden rounded-md"
                  style="background-color: var(--surface-hover);"
                >
                  {#if item.coverToken}
                    <img
                      src={imgProxyUrl(item.coverToken)}
                      alt={item.title}
                      loading="lazy"
                      class="h-full w-full object-cover"
                    />
                  {/if}
                </div>
                <div class="min-w-0">
                  <div class="truncate font-semibold">{item.title}</div>
                  <div class="truncate text-[11px]" style="color: var(--text-muted);">
                    {item.site}{item.latestChapterName ? ` · ${item.latestChapterName}` : ""}
                  </div>
                </div>
              </div>

              <div class="flex shrink-0 items-center gap-1.5">
                <button
                  type="button"
                  onclick={() => unhideManga(item)}
                  class="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1.5 text-xs font-medium"
                  style="border-color: var(--accent); background-color: var(--accent-soft); color: var(--accent);"
                >
                  <Eye size={13} />
                  <span>{t.unhideMangaBtn}</span>
                </button>
                <button
                  type="button"
                  onclick={() => {
                    pendingDeleteHidden = item;
                  }}
                  class="rounded-lg border p-1.5"
                  style="border-color: var(--border); color: var(--danger);"
                  title={t.deleteMangaBtn}
                >
                  <Trash2 size={13} />
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  {/if}

  <!-- Crash & Error Diagnostics -->
  {#if showDiagnostics}
    <section class="settings-card">
      <div>
        <h2 class="section-title">{t.diagnosticsSection}</h2>
        <div class="desc">{t.diagnosticsDesc}</div>
      </div>
      <DiagnosticsPanel />
    </section>
  {/if}

  <!-- Reset All Settings -->
  {#if showResetAll}
    <section class="settings-card">
      <div class="row">
        <div>
          <h2 class="section-title">{t.resetAllSection}</h2>
          <div class="desc">{t.resetAllDesc}</div>
        </div>
        <button
          type="button"
          onclick={() => {
            showResetAllModal = true;
          }}
          class="inline-flex shrink-0 items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-semibold"
          style="border-color: var(--danger); color: var(--danger);"
        >
          <RotateCcw size={13} />
          <span>{t.resetAllBtn}</span>
        </button>
      </div>
    </section>
  {/if}
</div>

<!-- Reset All Settings Confirmation Modal -->
{#if showResetAllModal}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm"
    role="dialog"
    aria-modal="true"
  >
    <div
      class="flex w-full max-w-md flex-col gap-4 rounded-2xl border p-5 shadow-2xl"
      style="background-color: var(--surface); border-color: var(--border);"
    >
      <div class="flex items-start gap-3">
        <div
          class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl"
          style="background-color: var(--accent-soft); color: var(--accent);"
        >
          <RotateCcw size={20} />
        </div>
        <div>
          <h3 class="text-base font-bold">{t.resetAllConfirmTitle}</h3>
          <p class="mt-1 text-xs leading-relaxed" style="color: var(--text-muted);">
            {t.resetAllConfirmDesc}
          </p>
        </div>
      </div>
      <div class="flex items-center justify-end gap-2 pt-1">
        <button
          type="button"
          disabled={resettingAll}
          onclick={() => {
            showResetAllModal = false;
          }}
          class="rounded-xl border px-4 py-2 text-xs font-medium"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          {t.cancel}
        </button>
        <button
          type="button"
          disabled={resettingAll}
          onclick={handleResetAll}
          class="inline-flex items-center gap-1.5 rounded-xl px-4 py-2 text-xs font-semibold text-white disabled:opacity-50"
          style="background-color: var(--danger);"
        >
          {#if resettingAll}
            <Loader2 size={13} class="animate-spin" />
          {:else}
            <RotateCcw size={13} />
          {/if}
          <span>{t.resetAllBtn}</span>
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Hidden Manga Permanent Delete Modal -->
{#if pendingDeleteHidden}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm"
    role="dialog"
    aria-modal="true"
  >
    <div
      class="flex w-full max-w-md flex-col gap-4 rounded-2xl border p-5 shadow-2xl"
      style="background-color: var(--surface); border-color: var(--border);"
    >
      <div class="flex items-start gap-3">
        <div
          class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl"
          style="background-color: rgba(239, 68, 68, 0.14); color: var(--danger);"
        >
          <AlertTriangle size={20} />
        </div>
        <div>
          <h3 class="text-base font-bold">{t.deleteMangaConfirmTitle}</h3>
          <p class="mt-1 text-xs leading-relaxed" style="color: var(--text-muted);">
            {t.deleteMangaConfirmScope(pendingDeleteHidden.title)}
          </p>
        </div>
      </div>
      <div class="flex items-center justify-end gap-2 pt-1">
        <button
          type="button"
          disabled={deletingHidden}
          onclick={() => {
            pendingDeleteHidden = null;
          }}
          class="rounded-xl border px-4 py-2 text-xs font-medium"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          {t.cancel}
        </button>
        <button
          type="button"
          disabled={deletingHidden}
          onclick={confirmDeleteHiddenManga}
          class="inline-flex items-center gap-1.5 rounded-xl px-4 py-2 text-xs font-semibold text-white disabled:opacity-50"
          style="background-color: var(--danger);"
        >
          {#if deletingHidden}
            <Loader2 size={13} class="animate-spin" />
          {:else}
            <Trash2 size={13} />
          {/if}
          <span>{t.deleteMangaConfirmBtn}</span>
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .settings-card {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1.25rem;
    border-radius: 1rem;
    border: 1px solid var(--border);
    background-color: var(--surface);
  }

  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .section-title {
    font-size: 0.9rem;
    font-weight: 600;
    margin: 0;
  }

  .reset-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.3rem 0.6rem;
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
    color: var(--text-muted);
    font-size: 0.7rem;
    font-weight: 500;
    cursor: pointer;
    flex-shrink: 0;
  }

  .reset-btn:hover {
    color: var(--text);
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }

  .label {
    font-size: 0.875rem;
    font-weight: 500;
  }

  .desc {
    font-size: 0.75rem;
    color: var(--text-muted);
    margin-top: 0.15rem;
  }

  .pill {
    padding: 0.35rem 0.75rem;
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
    color: var(--text);
    font-size: 0.75rem;
    font-weight: 500;
    cursor: pointer;
  }

  .pill-active {
    border-color: var(--accent);
    background-color: var(--accent-soft);
    color: var(--accent);
  }

  .cat-box {
    border: 1px solid var(--border);
    border-radius: 0.75rem;
    background-color: var(--surface-subtle);
    overflow: hidden;
  }

  .cat-trigger {
    display: flex;
    width: 100%;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.75rem 0.9rem;
    background: transparent;
    border: none;
    color: var(--text);
    cursor: pointer;
    transition: background-color 140ms ease;
  }

  .cat-trigger:hover {
    background-color: var(--surface-hover);
  }

  .cat-body {
    display: flex;
    flex-direction: column;
    gap: 0.95rem;
    padding: 0.9rem;
    border-top: 1px solid var(--border);
    background-color: var(--surface);
  }
</style>
