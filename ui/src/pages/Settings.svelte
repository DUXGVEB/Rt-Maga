<script lang="ts">
  import {
    AlertCircle,
    AlertTriangle,
    ArrowDown,
    ArrowDownToLine,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    Ban,
    Bell,
    Book,
    Bookmark,
    BookOpen,
    Check,
    CheckCheck,
    CheckCircle2,
    ChevronLeft,
    ChevronRight,
    CircleDot,
    Clock,
    Columns2,
    Compass,
    Download,
    Eye,
    EyeOff,
    Gauge,
    Globe,
    HardDrive,
    Home,
    ImageOff,
    Laptop,
    Layers,
    LayoutGrid,
    Loader2,
    Lock,
    Monitor,
    Moon,
    Palette,
    PanelLeft,
    PanelRight,
    PanelTop,
    RefreshCw,
    RotateCcw,
    Rows3,
    Search,
    Settings as SettingsIcon,
    Shield,
    ShieldCheck,
    ShieldOff,
    SkipForward,
    SlidersHorizontal,
    Smartphone,
    Sparkles,
    Sun,
    SunMedium,
    Timer,
    Trash2,
    Wrench,
    X,
    ZoomIn,
  } from "lucide-svelte";
  import { onMount, tick } from "svelte";
  import { api, formatAppError, imgProxyUrl } from "../lib/api";
  import DiagnosticsPanel from "../lib/components/DiagnosticsPanel.svelte";
  import FeedbackComposer from "../lib/components/FeedbackComposer.svelte";
  import LanSyncPanel from "../lib/components/LanSyncPanel.svelte";
  import NetworkProtectionPanel from "../lib/components/NetworkProtectionPanel.svelte";
  import SettingsVisualPreview from "../lib/components/SettingsVisualPreview.svelte";
  import SourceManagerPanel from "../lib/components/SourceManagerPanel.svelte";
  import ToggleSwitch from "../lib/components/ToggleSwitch.svelte";
  import { LANGUAGE_OPTIONS } from "../lib/i18n";
  import { appReleaseStore } from "../lib/stores/appRelease.svelte";
  import { deviceEnv } from "../lib/stores/deviceEnv.svelte";
  import { notificationsStore } from "../lib/stores/notifications.svelte";
  import { router } from "../lib/stores/router.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { updatesStore } from "../lib/stores/updates.svelte";
  import {
    computeAccentTokens,
    DARK_ACCENT_PRESETS,
    DEFAULT_DARK_ACCENT,
    DEFAULT_LIGHT_ACCENT,
    DEFAULT_NAV_ORDER,
    LIGHT_ACCENT_PRESETS,
    moveNavDestination,
    normalizeHexColor,
    normalizeNavOrder,
  } from "../lib/themeAccent";
  import type {
    CacheInfo,
    CacheMangaStats,
    CacheSourceStats,
    DoubleTapAction,
    FloatingEdge,
    FloatingScreenKey,
    HiddenMangaView,
    HorizontalDirection,
    Language,
    NavDestination,
    NotificationPermissionInfo,
    ReaderBg,
    ReadingOrientation,
    Settings,
    SettingsSection,
    Theme,
  } from "../lib/types";

  let {
    focusSection,
    subTarget,
  }: {
    focusSection?: SettingsSection;
    subTarget?: "supplemental" | "feedback";
  } = $props();

  type SettingsTabKey = "general" | "reader" | "floating" | "network" | "storage" | "system";
  type StorageSubTab = "source" | "manga" | "hidden";

  let activeTab = $state<SettingsTabKey>("general");
  let mobileScreen = $state<"hub" | SettingsTabKey>("hub");
  let storageSubTab = $state<StorageSubTab>("source");
  let searchQuery = $state("");

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
  const isDarkResolved = $derived(settingsStore.resolvedTheme === "dark");
  let editingAccentMode = $state<"light" | "dark">(settingsStore.resolvedTheme);
  let lastSyncedTheme = $state(settingsStore.resolvedTheme);
  $effect(() => {
    const resolved = settingsStore.resolvedTheme;
    if (resolved !== lastSyncedTheme) {
      lastSyncedTheme = resolved;
      editingAccentMode = resolved;
    }
  });

  const navOrder = $derived(normalizeNavOrder(s.ui.navOrder));
  const editedAccentHex = $derived(
    editingAccentMode === "dark" ? s.ui.darkAccent : s.ui.lightAccent
  );
  const editedAccentPresets = $derived(
    editingAccentMode === "dark" ? DARK_ACCENT_PRESETS : LIGHT_ACCENT_PRESETS
  );
  const editedAccentContrast = $derived(
    computeAccentTokens(editedAccentHex, editingAccentMode).surfaceContrast
  );

  const normalizedQuery = $derived(searchQuery.trim().toLowerCase());
  const isSearching = $derived(normalizedQuery.length > 0);

  function navDestLabel(dest: NavDestination): string {
    switch (dest) {
      case "home":
        return t.navHome;
      case "sources":
        return t.navSources;
      case "bookmarks":
        return t.navBookmarks;
      case "history":
        return t.navHistory;
      case "settings":
        return t.navSettings;
    }
  }

  function updateAccentForMode(mode: "light" | "dark", rawHex: string, debounceMs = 0) {
    const fallback = mode === "dark" ? DEFAULT_DARK_ACCENT : DEFAULT_LIGHT_ACCENT;
    const hex = normalizeHexColor(rawHex, fallback);
    patch((d) => {
      if (mode === "dark") {
        d.ui.darkAccent = hex;
      } else {
        d.ui.lightAccent = hex;
      }
    }, debounceMs);
  }

  function resetAccentForMode(mode: "light" | "dark") {
    updateAccentForMode(mode, mode === "dark" ? DEFAULT_DARK_ACCENT : DEFAULT_LIGHT_ACCENT);
  }

  function reorderNav(index: number, delta: -1 | 1) {
    patch((d) => {
      d.ui.navOrder = moveNavDestination(d.ui.navOrder, index, delta);
    });
  }

  function resetNavOrder() {
    patch((d) => {
      d.ui.navOrder = [...DEFAULT_NAV_ORDER];
    });
  }

  const floatingScreens: FloatingScreenKey[] = [
    "home",
    "sources",
    "browse",
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

  function sectionToTab(section: SettingsSection): SettingsTabKey {
    switch (section) {
      case "appearance":
      case "device":
      case "updates":
        return "general";
      case "reader":
        return "reader";
      case "floating":
        return "floating";
      case "network":
      case "sources":
        return "network";
      case "cache":
        return "storage";
    }
  }

  function selectTab(tab: SettingsTabKey) {
    activeTab = tab;
    mobileScreen = tab;
    searchQuery = "";
  }

  function backToMobileHub() {
    mobileScreen = "hub";
  }

  let lastNavPulse = router.navPulse;
  $effect(() => {
    const pulse = router.navPulse;
    if (pulse !== lastNavPulse) {
      lastNavPulse = pulse;
      if (!focusSection) {
        mobileScreen = "hub";
        searchQuery = "";
      }
    }
  });

  $effect(() => {
    const target = focusSection;
    const sub = subTarget;
    if (!target && !sub) return;
    let mapped: SettingsTabKey = target ? sectionToTab(target) : "general";
    let anchorId = target ? `settings-${target}` : "";
    if (sub === "supplemental") {
      mapped = "network";
      anchorId = "settings-supplemental-sources";
    } else if (sub === "feedback") {
      mapped = "system";
      anchorId = "settings-feedback";
    }
    activeTab = mapped;
    mobileScreen = mapped;
    searchQuery = "";
    if (anchorId) {
      void (async () => {
        await tick();
        const el = document.getElementById(anchorId);
        el?.scrollIntoView({ behavior: "smooth", block: "start" });
      })();
    }
  });

  // Search index across all 6 categories (preserving all existing EN + VI search keywords)
  interface SearchEntry {
    id: string;
    tab: SettingsTabKey;
    title: string;
    subtitle: string;
    keywords: string;
  }

  const searchCatalog = $derived.by<SearchEntry[]>(() => [
    {
      id: "appearance",
      tab: "general",
      title: t.appearanceSection,
      subtitle: `${t.languageLabel} · ${t.colorThemeLabel} · ${t.accentColorTitle}`,
      keywords: `${t.appearanceSection} ${t.languageLabel} ${t.languageDesc} ${t.colorThemeLabel} ${t.colorThemeDesc} ${t.accentColorTitle} ${t.accentColorDesc} appearance language theme accent color palette light dark system giao dien ngon ngu chu de mau chu dao`,
    },
    {
      id: "navigation",
      tab: "general",
      title: t.navOrderSection,
      subtitle: navOrder.map((d) => navDestLabel(d)).join(" · "),
      keywords: `${t.navOrderSection} ${t.navOrderDesc} ${t.navHome} ${t.navBookmarks} ${t.navHistory} ${t.navSettings} navigation sidebar bottom tab order position rearrange dieu huong vi tri thu tu`,
    },
    {
      id: "device",
      tab: "general",
      title: t.deviceIdentitySection,
      subtitle: s.device.customName || deviceEnv.info.realDeviceName,
      keywords: `${t.deviceIdentitySection} ${t.deviceDetectedLabel} ${t.deviceCustomNameLabel} ${t.deviceCustomNameDesc} device identity name lan sync thiet bi ten`,
    },
    {
      id: "updates",
      tab: "general",
      title: t.updatesSection,
      subtitle: `${t.everyHours(s.updates.intervalHours)} · ${t.notificationsLabel}`,
      keywords: `${t.updatesSection} ${t.autoCheckLabel} ${t.autoCheckDesc} ${t.checkIntervalLabel} ${t.notificationsLabel} ${t.notificationsDesc} updates chapter check interval notifications cap nhat chuong thong bao`,
    },
    {
      id: "app-updates",
      tab: "general",
      title: t.appUpdatesSection,
      subtitle: t.appVersionLabel(appReleaseStore.currentVersion),
      keywords: `${t.appUpdatesSection} ${t.checkAppUpdatesLabel} ${t.checkAppUpdatesDesc} app update release version phien ban`,
    },
    {
      id: "reader-layout",
      tab: "reader",
      title: `${t.readerSection} · ${t.readerCatLayout}`,
      subtitle: readerSummaryText,
      keywords: `${t.readerSection} ${t.readerCatLayout} ${t.readingModeLabel} ${t.readingModeDesc} ${t.noGapModeLabel} ${t.noGapModeDesc} ${t.gapDesc} ${t.verticalMarginDesc} ${t.horizontalDirLabel} ${t.pagesPerGroupLabel} ${t.horizontalNoGapLabel} ${t.centerGroupsLabel} ${t.autoAppendNext} ${t.visualPreviewTitle} reader trinh doc layout mode vertical horizontal gap no-gap seamless joined dinh lien margin group preview cuon doc ngang khoang cach le`,
    },
    {
      id: "reader-display",
      tab: "reader",
      title: `${t.readerSection} · ${t.readerCatDisplay}`,
      subtitle: `${readerBgLabel(s.reader.readerBg)}${s.reader.autoWebtoonDetect ? " · Webtoon" : ""}`,
      keywords: `${t.readerCatDisplay} ${t.readerBg} ${t.readerBgDesc} ${t.dimUnfocusedLabel} ${t.dimUnfocusedDesc} ${t.autoWebtoonLabel} ${t.autoWebtoonDesc} display background theme dark light dim unfocused webtoon hien thi nen lam toi`,
    },
    {
      id: "reader-controls",
      tab: "reader",
      title: `${t.readerSection} · ${t.readerCatControls}`,
      subtitle: `${doubleTapLabel(s.reader.behavior.doubleTapAction)} · ${t.preloadLabel(s.reader.preloadImages)}`,
      keywords: `${t.readerCatControls} ${t.doubleTapActionLabel} ${t.doubleTapActionDesc} ${t.tapZonesLabel} ${t.tapZonesDesc} ${t.navigationSpeedDesc} ${t.toolbarAutoHideLabel} ${t.keepScreenOnLabel} ${t.readerCatBehavior} ${t.autoMarkReadLabel} ${t.skipBrokenImagesLabel} ${t.preloadDesc} controls gestures double tap zoom speed wheel scroll keyboard toolbar auto hide screen awake behavior performance mark read broken image skip preload dieu khien cham dup toc do thanh cong cu man hinh da doc anh hong`,
    },
    {
      id: "floating",
      tab: "floating",
      title: t.floatingSection,
      subtitle: floatingSummaryText,
      keywords: `${t.floatingSection} ${t.floatingSubtitle} ${t.floatingScrollTopScreens} ${t.floatingReaderSettingsLabel} ${t.floatingOrientationLockLabel} ${t.floatingAutoDimLabel} floating scroll top orientation lock button nut noi`,
    },
    {
      id: "network",
      tab: "network",
      title: t.netProtectionSection,
      subtitle: networkSummaryText,
      keywords: `${t.netProtectionSection} ${t.netProtectionSubtitle} ${t.proxySection} ${t.proxySubtitle} dns doh cloudflare quad9 adguard nextdns proxy socks5 http domain rules mang bao ve`,
    },
    {
      id: "sources",
      tab: "network",
      title: t.sourcesSection,
      subtitle: `${speedValueLabel(s.sources.speedLimitKbps)} · ${t.headlessLabel}`,
      keywords: `${t.sourcesSection} ${t.headlessLabel} ${t.headlessDesc} ${t.speedLimitLabel} ${t.speedLimitDesc} sources headless browser speed limit bandwidth throttle toc do tai bang thong`,
    },
    {
      id: "lan-sync",
      tab: "network",
      title: t.syncSection,
      subtitle: t.syncSubtitle,
      keywords: `${t.syncSection} ${t.syncSubtitle} lan sync qr pin transfer wifi dong bo`,
    },
    {
      id: "supplemental-sources",
      tab: "network",
      title: t.suppSourcesTitle,
      subtitle: t.suppSourcesDesc,
      keywords: `${t.suppSourcesTitle} ${t.suppSourcesDesc} ${t.navSources} supplemental nguon bo sung`,
    },
    {
      id: "cache",
      tab: "storage",
      title: t.cacheSection,
      subtitle: storageSummaryText,
      keywords: `${t.cacheSection} ${t.cacheUsageLabel} ${t.cacheBySourceTitle} ${t.cacheByMangaTitle} cache disk storage clear source manga bo nho dem xoa`,
    },
    {
      id: "hidden",
      tab: "storage",
      title: t.hiddenMangaSection,
      subtitle: `${hiddenManga.length}`,
      keywords: `${t.hiddenMangaSection} ${t.hiddenMangaDesc} hidden hide unhide restore manga truyen an`,
    },
    {
      id: "feedback",
      tab: "system",
      title: t.feedbackSectionTitle,
      subtitle: t.feedbackSectionDesc,
      keywords: `${t.feedbackSectionTitle} ${t.feedbackSectionDesc} feedback bug report issue github bao loi gop y`,
    },
    {
      id: "diagnostics",
      tab: "system",
      title: t.diagnosticsSection,
      subtitle: t.diagnosticsDesc,
      keywords: `${t.diagnosticsSection} ${t.diagnosticsDesc} diagnostics crash error logs export bug nhat ky loi`,
    },
    {
      id: "reset-all",
      tab: "system",
      title: t.resetAllSection,
      subtitle: t.resetAllDesc,
      keywords: `${t.resetAllSection} ${t.resetAllDesc} reset default settings khoi phuc mac dinh`,
    },
  ]);

  const searchHits = $derived.by(() => {
    if (!normalizedQuery) return [];
    const tokens = normalizedQuery.split(/\s+/).filter(Boolean);
    return searchCatalog.filter((entry) => {
      const hay = `${entry.title} ${entry.subtitle} ${entry.keywords}`.toLowerCase();
      return tokens.every((tok) => hay.includes(tok));
    });
  });

  // Live compact summaries for Mobile Hub rows & header chips
  const activeLanguageLabel = $derived(
    LANGUAGE_OPTIONS.find((opt) => opt.code === s.language)?.label ?? s.language.toUpperCase()
  );

  const activeThemeLabel = $derived(
    s.theme === "dark" ? t.themeDark : s.theme === "light" ? t.themeLight : t.themeSystem
  );

  const generalSummaryText = $derived(`${activeThemeLabel} · ${activeLanguageLabel}`);

  const readerSummaryText = $derived.by(() => {
    if (isHorizontal) {
      const dir = s.reader.horizontal.direction === "rtl" ? "RTL" : "LTR";
      const base = `${t.modeHorizontal} · ${dir} · ${s.reader.horizontal.pagesPerGroup}x`;
      return s.reader.horizontal.noGapMode ? `${base} · ${t.horizontalNoGapLabel}` : base;
    }
    const gapText = s.reader.vertical.noGapMode
      ? t.noGapModeLabel
      : `${s.reader.vertical.pageGapPx}px`;
    return `${t.modeVertical} · ${gapText}`;
  });

  const enabledFloatingCount = $derived(
    (s.floating.scrollToTopScreens.length > 0 ? 1 : 0) +
      (s.floating.showReaderSettings ? 1 : 0) +
      (s.floating.showOrientationLock ? 1 : 0)
  );

  const floatingSummaryText = $derived(
    `${t.settingsActiveButtonsCount(enabledFloatingCount)}${s.floating.autoDimInReader ? ` · ${t.floatingAutoDimLabel}` : ""}`
  );

  const networkSummaryText = $derived.by(() => {
    const dnsPart = s.doh.enabled ? `DNS ${s.doh.providerId}` : t.netSystemDns;
    return s.proxy.enabled ? `${dnsPart} · Proxy` : `${dnsPart} · LAN`;
  });

  const storageSummaryText = $derived.by(() => {
    const used = cache ? formatBytes(cache.bytes) : "…";
    const base = `${used} / ${s.cache.maxMb} MB`;
    return hiddenManga.length > 0 ? `${base} · ${t.settingsHiddenTabLabel(hiddenManga.length)}` : base;
  });

  const systemSummaryText = $derived(
    `v${appReleaseStore.currentVersion} · ${t.diagnosticsSection}`
  );

  const cacheUsagePercent = $derived.by(() => {
    if (!cache || s.cache.maxMb <= 0) return 0;
    const maxBytes = s.cache.maxMb * 1024 * 1024;
    return Math.min(100, Math.max(cache.bytes > 0 ? 3 : 0, Math.round((cache.bytes / maxBytes) * 100)));
  });

  function tabLabel(tab: SettingsTabKey): string {
    switch (tab) {
      case "general":
        return t.settingsTabGeneral;
      case "reader":
        return t.settingsTabReader;
      case "floating":
        return t.settingsTabFloating;
      case "network":
        return t.settingsTabNetwork;
      case "storage":
        return t.settingsTabStorage;
      case "system":
        return t.settingsTabSystem;
    }
  }

  function readerBgLabel(bg: ReaderBg): string {
    if (bg === "dark") return t.bgDark;
    if (bg === "light") return t.bgLight;
    return t.bgAppTheme;
  }

  function doubleTapLabel(action: DoubleTapAction): string {
    if (action === "zoom") return t.doubleTapZoom;
    if (action === "toggle_toolbar") return t.doubleTapToggleToolbar;
    if (action === "next_page") return t.doubleTapNextPage;
    return t.doubleTapDisabled;
  }

  function floatingScreenLabel(key: FloatingScreenKey): string {
    switch (key) {
      case "home":
        return t.navHome;
      case "sources":
        return t.navSources;
      case "browse":
        return t.floatingScreenBrowse;
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
        return "512K";
      case "1m":
        return "1M";
      case "2m":
        return "2M";
      case "5m":
        return "5M";
      case "10m":
        return "10M";
    }
  }

  function speedValueLabel(kbps: number): string {
    if (kbps <= 0) return t.speedLimitUnlimited;
    if (kbps >= 1024 && kbps % 1024 === 0) return `${kbps / 1024} MB/s`;
    return `${kbps} KB/s`;
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

  onMount(() => {
    void (async () => {
      try {
        notifPerm = await api.checkNotificationPermission();
      } catch {
        notifPerm = { granted: true, canRequest: false };
      }
      await loadHiddenManga();
    })();

    // Android hardware Back: dismiss modals first, then return from Mobile sub-screen to Hub
    const unregisterBack = router.registerBackInterceptor(() => {
      if (showResetAllModal) {
        showResetAllModal = false;
        return true;
      }
      if (pendingDeleteHidden) {
        pendingDeleteHidden = null;
        return true;
      }
      if (searchQuery.trim()) {
        searchQuery = "";
        return true;
      }
      if (typeof window !== "undefined" && window.innerWidth < 768 && mobileScreen !== "hub") {
        mobileScreen = "hub";
        return true;
      }
      return false;
    });

    return () => {
      unregisterBack();
    };
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

  function quickToggleTheme() {
    patch((d) => {
      d.theme = isDarkResolved ? "light" : "dark";
    });
  }

  function quickToggleOrientation() {
    patch((d) => {
      d.reader.orientation = d.reader.orientation === "horizontal" ? "vertical" : "horizontal";
    });
  }

  function quickToggleDns() {
    patch((d) => {
      d.doh.enabled = !d.doh.enabled;
    });
  }

  const tabs: SettingsTabKey[] = ["general", "reader", "floating", "network", "storage", "system"];
</script>

<div class="settings-shell mx-auto flex w-full min-w-0 max-w-5xl flex-col gap-4 px-4 py-4 sm:px-6 sm:py-5">
  <!-- ==================== COMPACT HEADER BAR ==================== -->
  <header class="flex flex-wrap items-center justify-between gap-3">
    <!-- Left: Desktop title OR Mobile Hub/Back title -->
    <div class="flex min-w-0 items-center gap-2.5">
      {#if mobileScreen !== "hub" && !isSearching}
        <button
          type="button"
          onclick={backToMobileHub}
          class="inline-flex min-h-10 items-center gap-1 rounded-xl border px-2.5 py-1.5 text-sm font-semibold md:hidden"
          style="border-color: var(--border); background-color: var(--surface);"
        >
          <ChevronLeft size={17} />
          <span>{tabLabel(mobileScreen)}</span>
        </button>
      {/if}

      <div class="{mobileScreen !== 'hub' && !isSearching ? 'hidden md:flex' : 'flex'} items-center gap-2.5">
        <h1 class="text-xl font-bold tracking-tight sm:text-2xl">{t.settingsTitle}</h1>
        <span
          class="hidden rounded-full border px-2.5 py-0.5 text-[11px] font-medium sm:inline-flex"
          style="border-color: var(--border); background-color: var(--surface); color: var(--text-muted);"
        >
          v{appReleaseStore.currentVersion} · {t.settingsAutoSavedChip}
        </span>
      </div>
    </div>

    <!-- Right: Compact Inline Instant Search + Icon-only System/Reset Shortcut -->
    <div class="flex w-full items-center gap-2 sm:w-auto sm:min-w-64 sm:max-w-sm sm:flex-1">
      <div
        class="flex flex-1 items-center gap-2 rounded-xl border px-3 py-2 text-xs"
        style="background-color: var(--surface); border-color: var(--border);"
      >
        <Search size={14} class="shrink-0" style="color: var(--text-muted);" />
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
            class="rounded-md p-0.5 opacity-75 hover:opacity-100"
            style="color: var(--text-muted);"
            aria-label={t.settingsSearchClear}
          >
            <X size={14} />
          </button>
        {/if}
      </div>

      <button
        type="button"
        onclick={() => {
          showResetAllModal = true;
        }}
        class="icon-btn hidden md:inline-flex"
        title={t.resetAllSection}
        aria-label={t.resetAllSection}
      >
        <RotateCcw size={15} />
      </button>
    </div>
  </header>

  <!-- ==================== DESKTOP: OPTION B HORIZONTAL ICON-PILL TABS ==================== -->
  {#if !isSearching}
    <nav
      aria-label={t.settingsTitle}
      class="hidden flex-wrap items-center gap-1.5 rounded-2xl border p-1.5 md:flex"
      style="background-color: var(--surface); border-color: var(--border);"
    >
      {#each tabs as tab (tab)}
        {@const active = activeTab === tab}
        <button
          type="button"
          onclick={() => selectTab(tab)}
          class="top-tab"
          class:top-tab-active={active}
        >
          {#if tab === "general"}
            <SlidersHorizontal size={15} />
          {:else if tab === "reader"}
            <BookOpen size={15} />
          {:else if tab === "floating"}
            <CircleDot size={15} />
          {:else if tab === "network"}
            <Shield size={15} />
          {:else if tab === "storage"}
            <HardDrive size={15} />
          {:else}
            <Wrench size={15} />
          {/if}
          <span>{tabLabel(tab)}</span>
        </button>
      {/each}
    </nav>
  {/if}

  <!-- ==================== GLOBAL INSTANT SEARCH RESULTS ==================== -->
  {#if isSearching}
    {#if searchHits.length === 0}
      <div
        class="flex flex-col items-center gap-3 rounded-2xl border border-dashed py-10 text-center"
        style="border-color: var(--border); color: var(--text-muted);"
      >
        <Search size={24} />
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
    {:else}
      <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
        {#each searchHits as hit (hit.id)}
          <button
            type="button"
            onclick={() => selectTab(hit.tab)}
            class="flex items-center justify-between gap-3 rounded-xl border px-3.5 py-3 text-left transition-colors"
            style="border-color: var(--border); background-color: var(--surface);"
          >
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <span class="truncate text-xs font-semibold sm:text-sm">{hit.title}</span>
                <span
                  class="shrink-0 rounded-md px-1.5 py-0.5 text-[10px] font-semibold"
                  style="background-color: var(--accent-soft); color: var(--accent);"
                >
                  {tabLabel(hit.tab)}
                </span>
              </div>
              <div class="mt-0.5 truncate text-[11px]" style="color: var(--text-muted);">
                {hit.subtitle}
              </div>
            </div>
            <span
              class="inline-flex shrink-0 items-center gap-1 text-xs font-semibold"
              style="color: var(--accent);"
            >
              <span>{t.settingsSearchOpenBtn}</span>
              <ChevronRight size={14} />
            </span>
          </button>
        {/each}
      </div>
    {/if}
  {:else}
    <!-- ==================== MOBILE (<768px): 1-SCREEN HUB ==================== -->
    {#if mobileScreen === "hub"}
      <div class="flex flex-col gap-3.5 md:hidden">
        <!-- 3 Quick Visual Toggle Tiles -->
        <div class="grid grid-cols-3 gap-2">
          <button
            type="button"
            onclick={quickToggleTheme}
            class="quick-tile"
          >
            <span class="quick-icon" style="background-color: var(--accent-soft); color: var(--accent);">
              {#if isDarkResolved}
                <Moon size={16} />
              {:else}
                <Sun size={16} />
              {/if}
            </span>
            <span class="truncate text-[11px] font-semibold">{activeThemeLabel}</span>
          </button>

          <button
            type="button"
            onclick={quickToggleOrientation}
            class="quick-tile"
          >
            <span class="quick-icon" style="background-color: var(--accent-soft); color: var(--accent);">
              {#if isHorizontal}
                <Columns2 size={16} />
              {:else}
                <Rows3 size={16} />
              {/if}
            </span>
            <span class="truncate text-[11px] font-semibold">
              {isHorizontal ? t.modeHorizontal : t.modeVertical}
            </span>
          </button>

          <button
            type="button"
            onclick={quickToggleDns}
            class="quick-tile"
            style={s.doh.enabled
              ? "border-color: rgba(16, 185, 129, 0.55); background-color: rgba(16, 185, 129, 0.08);"
              : ""}
          >
            <span
              class="quick-icon"
              style={s.doh.enabled
                ? "background-color: #10b981; color: #ffffff;"
                : "background-color: var(--surface-subtle); color: var(--text-muted);"}
            >
              {#if s.doh.enabled}
                <ShieldCheck size={16} />
              {:else}
                <ShieldOff size={16} />
              {/if}
            </span>
            <span
              class="truncate text-[11px] font-semibold"
              style={s.doh.enabled ? "color: #10b981;" : ""}
            >
              {s.doh.enabled ? t.settingsQuickDnsOn : t.settingsQuickDnsOff}
            </span>
          </button>
        </div>

        <!-- 6 Grouped Category List Rows with Live Summaries -->
        <div
          class="flex flex-col overflow-hidden rounded-2xl border"
          style="border-color: var(--border); background-color: var(--surface);"
        >
          <button type="button" onclick={() => selectTab("general")} class="hub-row">
            <span class="hub-icon" style="background-color: var(--accent-soft); color: var(--accent);">
              <SlidersHorizontal size={17} />
            </span>
            <div class="min-w-0 flex-1">
              <div class="text-sm font-semibold">{t.settingsTabGeneral}</div>
              <div class="truncate text-xs" style="color: var(--text-muted);">{generalSummaryText}</div>
            </div>
            <ChevronRight size={16} style="color: var(--text-muted);" />
          </button>

          <button type="button" onclick={() => selectTab("reader")} class="hub-row">
            <span class="hub-icon" style="background-color: rgba(59, 130, 246, 0.14); color: #3b82f6;">
              <BookOpen size={17} />
            </span>
            <div class="min-w-0 flex-1">
              <div class="text-sm font-semibold">{t.settingsTabReader}</div>
              <div class="truncate text-xs" style="color: var(--text-muted);">{readerSummaryText}</div>
            </div>
            <ChevronRight size={16} style="color: var(--text-muted);" />
          </button>

          <button type="button" onclick={() => selectTab("floating")} class="hub-row">
            <span class="hub-icon" style="background-color: rgba(139, 92, 246, 0.14); color: #8b5cf6;">
              <CircleDot size={17} />
            </span>
            <div class="min-w-0 flex-1">
              <div class="text-sm font-semibold">{t.settingsTabFloating}</div>
              <div class="truncate text-xs" style="color: var(--text-muted);">{floatingSummaryText}</div>
            </div>
            <ChevronRight size={16} style="color: var(--text-muted);" />
          </button>

          <button type="button" onclick={() => selectTab("network")} class="hub-row">
            <span class="hub-icon" style="background-color: rgba(16, 185, 129, 0.14); color: #10b981;">
              <Shield size={17} />
            </span>
            <div class="min-w-0 flex-1">
              <div class="text-sm font-semibold">{t.settingsTabNetwork}</div>
              <div class="truncate text-xs" style="color: var(--text-muted);">{networkSummaryText}</div>
            </div>
            <ChevronRight size={16} style="color: var(--text-muted);" />
          </button>

          <button type="button" onclick={() => selectTab("storage")} class="hub-row">
            <span class="hub-icon" style="background-color: rgba(245, 158, 11, 0.15); color: #d97706;">
              <HardDrive size={17} />
            </span>
            <div class="min-w-0 flex-1">
              <div class="text-sm font-semibold">{t.settingsTabStorage}</div>
              <div class="truncate text-xs" style="color: var(--text-muted);">{storageSummaryText}</div>
            </div>
            <ChevronRight size={16} style="color: var(--text-muted);" />
          </button>

          <button type="button" onclick={() => selectTab("system")} class="hub-row hub-row-last">
            <span
              class="hub-icon"
              style="background-color: var(--surface-hover); color: var(--text-muted);"
            >
              <Wrench size={17} />
            </span>
            <div class="min-w-0 flex-1">
              <div class="text-sm font-semibold">{t.settingsTabSystem}</div>
              <div class="truncate text-xs" style="color: var(--text-muted);">{systemSummaryText}</div>
            </div>
            <ChevronRight size={16} style="color: var(--text-muted);" />
          </button>
        </div>

        <!-- Compact Mobile Footer Version Pill -->
        <div
          class="flex items-center justify-between rounded-xl border px-3.5 py-2.5 text-xs"
          style="border-color: var(--border); background-color: var(--surface); color: var(--text-muted);"
        >
          <span>RtMaga · v{appReleaseStore.currentVersion}</span>
          {#if appReleaseStore.release?.updateAvailable}
            <button
              type="button"
              onclick={() => appReleaseStore.openPrompt()}
              class="inline-flex items-center gap-1 font-semibold"
              style="color: var(--accent);"
            >
              <Sparkles size={13} />
              <span>v{appReleaseStore.release.latestVersion}</span>
            </button>
          {:else}
            <span class="font-medium" style="color: #10b981;">{t.settingsUpToDateShort}</span>
          {/if}
        </div>
      </div>
    {/if}

    <!-- ==================== TAB CONTENT (Desktop Always Visible | Mobile Drill-Down) ==================== -->
    <div class={mobileScreen === "hub" ? "hidden md:block" : "block"}>
      <!-- ==================== TAB 1: GENERAL (2x2 Bento Grid) ==================== -->
      {#if activeTab === "general"}
        <div class="grid grid-cols-1 items-start gap-3.5 lg:grid-cols-2">
          <!-- Bento Card 1: Appearance (Visual Theme Mini-Windows + Language) -->
          <section id="settings-appearance" class="bento-card">
            <div class="card-head">
              <div class="flex items-center gap-2.5">
                <span class="card-icon" style="background-color: var(--accent-soft); color: var(--accent);">
                  <Palette size={15} />
                </span>
                <div>
                  <h2 class="card-title">{t.appearanceSection}</h2>
                  <p class="card-sub">{t.colorThemeDesc}</p>
                </div>
              </div>
              <button
                type="button"
                onclick={() => handleResetSection("appearance")}
                disabled={resettingSection === "appearance"}
                class="icon-btn"
                title={t.resetSectionBtn}
                aria-label={t.resetSectionBtn}
              >
                <RotateCcw size={13} />
              </button>
            </div>

            <!-- 3 Visual Theme Mini-Window Cards -->
            <div class="grid grid-cols-3 gap-2">
              <button
                type="button"
                onclick={() => patch((d) => (d.theme = "system"))}
                class="theme-card"
                class:theme-card-active={s.theme === "system"}
              >
                <div class="theme-swatch flex overflow-hidden" style="border-color: var(--border);">
                  <div class="flex w-1/2 flex-col gap-1 bg-white p-1.5">
                    <span class="h-1 w-3/4 rounded-xs bg-zinc-200"></span>
                    <span class="h-1 w-full rounded-xs" style="background-color: {s.ui.lightAccent};"></span>
                  </div>
                  <div class="flex w-1/2 flex-col gap-1 bg-zinc-900 p-1.5">
                    <span class="h-1 w-3/4 rounded-xs bg-zinc-700"></span>
                    <span class="h-1 w-full rounded-xs" style="background-color: {s.ui.darkAccent};"></span>
                  </div>
                </div>
                <span class="inline-flex items-center gap-1 text-[11px] font-semibold">
                  <Monitor size={12} />
                  <span>{t.themeSystem}</span>
                </span>
              </button>

              <button
                type="button"
                onclick={() => patch((d) => (d.theme = "light"))}
                class="theme-card"
                class:theme-card-active={s.theme === "light"}
              >
                <div class="theme-swatch flex flex-col gap-1 bg-white p-1.5" style="border-color: #d4d4d8;">
                  <span class="h-1 w-3/5 rounded-xs bg-zinc-200"></span>
                  <span class="h-1.5 w-4/5 rounded-xs" style="background-color: {s.ui.lightAccent};"></span>
                </div>
                <span class="inline-flex items-center gap-1 text-[11px] font-semibold">
                  <Sun size={12} />
                  <span>{t.themeLight}</span>
                </span>
              </button>

              <button
                type="button"
                onclick={() => patch((d) => (d.theme = "dark"))}
                class="theme-card"
                class:theme-card-active={s.theme === "dark"}
              >
                <div class="theme-swatch flex flex-col gap-1 bg-zinc-900 p-1.5" style="border-color: #27272f;">
                  <span class="h-1 w-3/5 rounded-xs bg-zinc-700"></span>
                  <span class="h-1.5 w-4/5 rounded-xs" style="background-color: {s.ui.darkAccent};"></span>
                </div>
                <span class="inline-flex items-center gap-1 text-[11px] font-semibold">
                  <Moon size={12} />
                  <span>{t.themeDark}</span>
                </span>
              </button>
            </div>

            <!-- Independent Light / Dark Accent Color Customization -->
            <div class="flex flex-col gap-2.5 border-t pt-3" style="border-color: var(--border);">
              <div class="compact-row">
                <div class="min-w-0">
                  <div class="text-xs font-semibold sm:text-sm">{t.accentColorTitle}</div>
                  <div class="text-[11px]" style="color: var(--text-muted);">{t.accentColorDesc}</div>
                </div>

                <div
                  class="inline-flex rounded-lg border p-0.5"
                  style="border-color: var(--border); background-color: var(--surface-subtle);"
                >
                  <button
                    type="button"
                    onclick={() => (editingAccentMode = "light")}
                    class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-semibold transition-colors"
                    style={editingAccentMode === "light"
                      ? "background-color: var(--surface); color: var(--text); box-shadow: var(--shadow-sm);"
                      : "color: var(--text-muted);"}
                  >
                    <Sun size={11} />
                    <span>{t.accentModeLight}</span>
                  </button>
                  <button
                    type="button"
                    onclick={() => (editingAccentMode = "dark")}
                    class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-semibold transition-colors"
                    style={editingAccentMode === "dark"
                      ? "background-color: var(--surface); color: var(--text); box-shadow: var(--shadow-sm);"
                      : "color: var(--text-muted);"}
                  >
                    <Moon size={11} />
                    <span>{t.accentModeDark}</span>
                  </button>
                </div>
              </div>

              <div class="flex flex-wrap items-center gap-2">
                {#each editedAccentPresets as preset (preset.hex)}
                  {@const selected = editedAccentHex === preset.hex}
                  {@const swatchFg = computeAccentTokens(preset.hex, editingAccentMode).accentFg}
                  <button
                    type="button"
                    onclick={() => updateAccentForMode(editingAccentMode, preset.hex)}
                    title={preset.label}
                    aria-label={preset.label}
                    aria-pressed={selected}
                    class="flex h-7 w-7 shrink-0 items-center justify-center rounded-full border transition-transform hover:scale-105"
                    style="background-color: {preset.hex}; color: {swatchFg}; border-color: {selected
                      ? 'var(--text)'
                      : 'transparent'}; box-shadow: {selected
                      ? '0 0 0 2px var(--surface), 0 0 0 3.5px var(--text)'
                      : 'inset 0 0 0 1px rgba(128,128,128,0.28)'};"
                  >
                    {#if selected}
                      <Check size={13} strokeWidth={2.7} />
                    {/if}
                  </button>
                {/each}

                <label
                  class="inline-flex cursor-pointer items-center gap-1.5 rounded-lg border px-2 py-1 text-[11px] font-medium"
                  style="border-color: var(--border); background-color: var(--surface-subtle);"
                  title={t.accentCustomLabel}
                >
                  <input
                    type="color"
                    value={editedAccentHex}
                    aria-label={t.accentCustomLabel}
                    oninput={(e) => updateAccentForMode(editingAccentMode, e.currentTarget.value, 80)}
                    onchange={(e) => updateAccentForMode(editingAccentMode, e.currentTarget.value)}
                    class="h-4 w-4 cursor-pointer rounded border-0 bg-transparent p-0"
                  />
                  <span class="font-mono uppercase">{editedAccentHex}</span>
                </label>

                <button
                  type="button"
                  onclick={() => resetAccentForMode(editingAccentMode)}
                  disabled={editedAccentHex ===
                    (editingAccentMode === "dark" ? DEFAULT_DARK_ACCENT : DEFAULT_LIGHT_ACCENT)}
                  class="compact-btn ml-auto"
                >
                  <RotateCcw size={11} />
                  <span>{t.accentResetModeBtn}</span>
                </button>
              </div>

              {#if editedAccentContrast < 3.0}
                <div
                  class="flex items-center gap-1.5 rounded-lg border px-2.5 py-1.5 text-[11px]"
                  style="border-color: rgba(245, 158, 11, 0.45); background-color: rgba(245, 158, 11, 0.1); color: #d97706;"
                >
                  <AlertTriangle size={13} class="shrink-0" />
                  <span>{t.accentLowContrastHint}</span>
                </div>
              {/if}
            </div>

            <!-- Language Compact Row -->
            <div class="compact-row border-t pt-3" style="border-color: var(--border);">
              <label for="settings-lang" class="flex items-center gap-2 text-xs font-medium sm:text-sm">
                <Globe size={15} style="color: var(--text-muted);" />
                <span>{t.languageLabel}</span>
              </label>
              <select
                id="settings-lang"
                value={s.language}
                onchange={(e) => {
                  const lang = e.currentTarget.value as Language;
                  patch((d) => (d.language = lang));
                }}
                class="field-select"
              >
                {#each LANGUAGE_OPTIONS as opt (opt.code)}
                  <option value={opt.code}>{opt.label}</option>
                {/each}
              </select>
            </div>
          </section>

          <!-- Bento Card 2: Chapter Updates & Notifications -->
          <section id="settings-updates" class="bento-card">
            <div class="card-head">
              <div class="flex items-center gap-2.5">
                <span class="card-icon" style="background-color: var(--accent-soft); color: var(--accent);">
                  <Bell size={15} />
                </span>
                <div>
                  <h2 class="card-title">{t.updatesSection}</h2>
                  <p class="card-sub">{t.autoCheckDesc}</p>
                </div>
              </div>
              <div class="flex items-center gap-1.5">
                <button
                  type="button"
                  onclick={() => updatesStore.checkNow()}
                  disabled={updatesStore.checking || appReleaseStore.checking}
                  class="compact-btn"
                >
                  <RefreshCw
                    size={12}
                    class={updatesStore.checking || appReleaseStore.checking ? "animate-spin" : ""}
                  />
                  <span>{t.checkNow}</span>
                </button>
                <button
                  type="button"
                  onclick={() => handleResetSection("updates")}
                  disabled={resettingSection === "updates"}
                  class="icon-btn"
                  title={t.resetSectionBtn}
                  aria-label={t.resetSectionBtn}
                >
                  <RotateCcw size={13} />
                </button>
              </div>
            </div>

            <div class="compact-row">
              <span class="text-xs font-medium sm:text-sm">{t.autoCheckLabel}</span>
              <ToggleSwitch
                checked={s.updates.enabled}
                ariaLabel={t.autoCheckLabel}
                onchange={(checked) => patch((d) => (d.updates.enabled = checked))}
              />
            </div>

            <!-- Interval Segmented Chips -->
            <div class="compact-row">
              <span class="text-xs" style="color: var(--text-muted);">{t.checkIntervalLabel}</span>
              <div class="flex flex-wrap gap-1">
                {#each [1, 3, 6, 12, 24] as hrs (hrs)}
                  <button
                    type="button"
                    onclick={() => patch((d) => (d.updates.intervalHours = hrs))}
                    class="chip"
                    class:chip-active={s.updates.intervalHours === hrs}
                    title={t.everyHours(hrs)}
                  >
                    {hrs}h
                  </button>
                {/each}
              </div>
            </div>

            <div class="compact-row border-t pt-3" style="border-color: var(--border);">
              <div class="min-w-0">
                <div class="text-xs font-medium sm:text-sm">{t.notificationsLabel}</div>
              </div>
              <div class="flex items-center gap-2">
                {#if deviceEnv.isAndroid && !notifPerm.granted}
                  <button
                    type="button"
                    onclick={requestAndroidNotifPermission}
                    class="inline-flex items-center gap-1 rounded-lg border px-2 py-1 text-[11px] font-semibold"
                    style="border-color: var(--accent); color: var(--accent); background-color: var(--accent-soft);"
                  >
                    <Bell size={12} />
                    <span>{t.notifGrantPermissionBtn}</span>
                  </button>
                {/if}
                <ToggleSwitch
                  checked={s.updates.notifyWindows}
                  ariaLabel={t.notificationsLabel}
                  onchange={(checked) => patch((d) => (d.updates.notifyWindows = checked))}
                />
              </div>
            </div>
          </section>

          <!-- Bento Card 3: Device Identity -->
          <section id="settings-device" class="bento-card">
            <div class="card-head">
              <div class="flex items-center gap-2.5">
                <span
                  class="card-icon"
                  style="background-color: var(--surface-subtle); color: var(--text); border: 1px solid var(--border);"
                >
                  {#if deviceEnv.isAndroid}
                    <Smartphone size={15} />
                  {:else}
                    <Laptop size={15} />
                  {/if}
                </span>
                <div class="min-w-0">
                  <h2 class="card-title">{t.deviceIdentitySection}</h2>
                  <p class="card-sub truncate">
                    {deviceEnv.info.realDeviceName}
                    {#if deviceEnv.info.skin && deviceEnv.info.skin !== "desktop"}
                      · {deviceEnv.info.skin.toUpperCase()}
                    {/if}
                  </p>
                </div>
              </div>
              <button
                type="button"
                onclick={() => handleResetSection("device")}
                disabled={resettingSection === "device"}
                class="icon-btn"
                title={t.resetSectionBtn}
                aria-label={t.resetSectionBtn}
              >
                <RotateCcw size={13} />
              </button>
            </div>

            <div class="flex items-center gap-2">
              <input
                type="text"
                maxlength="48"
                value={s.device.customName ?? ""}
                placeholder={deviceEnv.info.realDeviceName}
                aria-label={t.deviceCustomNameLabel}
                oninput={(e) => {
                  const val = e.currentTarget.value;
                  patch((d) => {
                    d.device.customName = val.trim() ? val : null;
                  }, 300);
                }}
                class="field-input flex-1"
              />
              {#if s.device.customName}
                <button
                  type="button"
                  onclick={() => patch((d) => (d.device.customName = null))}
                  class="icon-btn"
                  title={t.deviceResetName}
                  aria-label={t.deviceResetName}
                >
                  <X size={14} />
                </button>
              {/if}
            </div>
          </section>

          <!-- Bento Card 4: App Release Updates -->
          <section class="bento-card">
            <div class="card-head">
              <div class="flex items-center gap-2.5">
                <span
                  class="card-icon"
                  style={appReleaseStore.release?.updateAvailable
                    ? "background-color: var(--accent-soft); color: var(--accent);"
                    : "background-color: rgba(16, 185, 129, 0.12); color: #10b981;"}
                >
                  {#if appReleaseStore.release?.updateAvailable}
                    <Sparkles size={15} />
                  {:else}
                    <CheckCircle2 size={15} />
                  {/if}
                </span>
                <div>
                  <h2 class="card-title">RtMaga v{appReleaseStore.currentVersion}</h2>
                  <p class="card-sub">{t.appUpdatesSection}</p>
                </div>
              </div>
              <div class="flex items-center gap-1.5">
                {#if appReleaseStore.release?.updateAvailable}
                  <button
                    type="button"
                    onclick={() => appReleaseStore.openPrompt()}
                    class="inline-flex items-center gap-1 rounded-lg px-2.5 py-1.5 text-xs font-semibold"
                    style="background-color: var(--accent); color: var(--accent-fg);"
                  >
                    <Download size={12} />
                    <span>v{appReleaseStore.release.latestVersion}</span>
                  </button>
                {/if}
                <button
                  type="button"
                  onclick={() => appReleaseStore.checkNow()}
                  disabled={appReleaseStore.checking}
                  class="compact-btn"
                >
                  <RefreshCw size={12} class={appReleaseStore.checking ? "animate-spin" : ""} />
                  <span>{t.checkReleaseNow}</span>
                </button>
              </div>
            </div>

            {#if appReleaseStore.checkStatus === "checking"}
              <div
                class="flex items-center gap-2 rounded-xl border px-3 py-2 text-xs font-medium"
                style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text-muted);"
              >
                <Loader2 size={13} class="shrink-0 animate-spin" style="color: var(--accent);" />
                <span>{t.releaseStatusChecking}</span>
              </div>
            {:else if appReleaseStore.release?.updateAvailable}
              <div
                class="flex items-center gap-2 rounded-xl border px-3 py-2 text-xs font-medium"
                style="background-color: var(--accent-soft); border-color: var(--accent); color: var(--accent);"
              >
                <Sparkles size={13} class="shrink-0" />
                <span>{t.releaseAvailableBanner(appReleaseStore.release.latestVersion)}</span>
              </div>
            {:else if appReleaseStore.checkStatus === "up_to_date" && appReleaseStore.statusMessage}
              <div
                class="flex items-center gap-2 rounded-xl border px-3 py-2 text-xs font-medium"
                style="background-color: rgba(16, 185, 129, 0.1); border-color: rgba(16, 185, 129, 0.45); color: #10b981;"
              >
                <CheckCircle2 size={13} class="shrink-0" />
                <span>{appReleaseStore.statusMessage}</span>
              </div>
            {:else if appReleaseStore.statusMessage && (appReleaseStore.checkStatus === "server_unavailable" || appReleaseStore.checkStatus === "invalid_metadata" || appReleaseStore.checkStatus === "rate_limited" || appReleaseStore.checkStatus === "network_error")}
              <div
                class="flex items-start gap-2 rounded-xl border px-3 py-2 text-xs font-medium"
                style="background-color: rgba(239, 68, 68, 0.08); border-color: var(--danger); color: var(--danger);"
              >
                <AlertCircle size={13} class="mt-0.5 shrink-0" />
                <span class="break-words">{appReleaseStore.statusMessage}</span>
              </div>
            {/if}

            <div class="compact-row border-t pt-3" style="border-color: var(--border);">
              <span class="text-xs font-medium sm:text-sm">{t.checkAppUpdatesLabel}</span>
              <ToggleSwitch
                checked={s.updates.checkAppUpdates}
                ariaLabel={t.checkAppUpdatesLabel}
                onchange={(checked) => patch((d) => (d.updates.checkAppUpdates = checked))}
              />
            </div>
          </section>

          <!-- Bento Card 5: Main Navigation Order -->
          <section id="settings-navigation" class="bento-card lg:col-span-2">
            <div class="card-head">
              <div class="flex items-center gap-2.5">
                <span class="card-icon" style="background-color: var(--accent-soft); color: var(--accent);">
                  <LayoutGrid size={15} />
                </span>
                <div>
                  <h2 class="card-title">{t.navOrderSection}</h2>
                  <p class="card-sub">{t.navOrderDesc}</p>
                </div>
              </div>
              <button
                type="button"
                onclick={resetNavOrder}
                disabled={navOrder.every((d, i) => d === DEFAULT_NAV_ORDER[i])}
                class="compact-btn"
                title={t.navResetOrderBtn}
                aria-label={t.navResetOrderBtn}
              >
                <RotateCcw size={12} />
                <span>{t.navResetOrderBtn}</span>
              </button>
            </div>

            <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
              {#each navOrder as dest, idx (dest)}
                <div class="list-row">
                  <div class="flex min-w-0 items-center gap-2.5">
                    <span
                      class="flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-[11px] font-bold"
                      style="background-color: var(--accent-soft); color: var(--accent);"
                    >
                      {idx + 1}
                    </span>
                    <span class="shrink-0" style="color: var(--text-muted);">
                      {#if dest === "home"}
                        <Home size={15} />
                      {:else if dest === "sources"}
                        <Compass size={15} />
                      {:else if dest === "bookmarks"}
                        <Bookmark size={15} />
                      {:else if dest === "history"}
                        <Clock size={15} />
                      {:else}
                        <SettingsIcon size={15} />
                      {/if}
                    </span>
                    <span class="truncate text-xs font-semibold sm:text-sm">{navDestLabel(dest)}</span>
                  </div>

                  <div class="flex shrink-0 items-center gap-1">
                    <button
                      type="button"
                      onclick={() => reorderNav(idx, -1)}
                      disabled={idx === 0}
                      class="icon-btn"
                      title={t.navMoveUp}
                      aria-label="{t.navMoveUp}: {navDestLabel(dest)}"
                    >
                      <ArrowUp size={13} />
                    </button>
                    <button
                      type="button"
                      onclick={() => reorderNav(idx, 1)}
                      disabled={idx === navOrder.length - 1}
                      class="icon-btn"
                      title={t.navMoveDown}
                      aria-label="{t.navMoveDown}: {navDestLabel(dest)}"
                    >
                      <ArrowDown size={13} />
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          </section>
        </div>
      {/if}

      <!-- ==================== TAB 2: READER (2-Column Bento, No Deep Accordions!) ==================== -->
      {#if activeTab === "reader"}
        <div id="settings-reader" class="grid grid-cols-1 items-start gap-3.5 lg:grid-cols-2">
          <!-- Left Bento Card: Layout Mode + Live Preview + Display -->
          <section class="bento-card">
            <div class="card-head">
              <div class="flex items-center gap-2.5">
                <span class="card-icon" style="background-color: var(--accent-soft); color: var(--accent);">
                  <BookOpen size={15} />
                </span>
                <div>
                  <h2 class="card-title">{t.readerCatLayout}</h2>
                  <p class="card-sub">{readerSummaryText}</p>
                </div>
              </div>
              <button
                type="button"
                onclick={() => handleResetSection("reader")}
                disabled={resettingSection === "reader"}
                class="icon-btn"
                title={t.resetSectionBtn}
                aria-label={t.resetSectionBtn}
              >
                <RotateCcw size={13} />
              </button>
            </div>

            <!-- Visual Orientation Icon Tiles -->
            <div class="grid grid-cols-2 gap-2">
              <button
                type="button"
                onclick={() => patch((d) => (d.reader.orientation = "vertical" as ReadingOrientation))}
                class="mode-tile"
                class:mode-tile-active={!isHorizontal}
              >
                <Rows3 size={18} class="shrink-0" />
                <div class="min-w-0 text-left">
                  <div class="truncate text-xs font-semibold">{t.modeVertical}</div>
                </div>
              </button>

              <button
                type="button"
                onclick={() => patch((d) => (d.reader.orientation = "horizontal" as ReadingOrientation))}
                class="mode-tile"
                class:mode-tile-active={isHorizontal}
              >
                <Columns2 size={18} class="shrink-0" />
                <div class="min-w-0 text-left">
                  <div class="truncate text-xs font-semibold">{t.modeHorizontal}</div>
                </div>
              </button>
            </div>

            <!-- Live Miniature Preview -->
            <SettingsVisualPreview settings={s} {t} />

            <!-- Mode-specific Compact Controls -->
            <div
              class="flex flex-col gap-2.5 rounded-xl border p-3"
              style="background-color: var(--surface-subtle); border-color: var(--border);"
            >
              {#if !isHorizontal}
                <div class="compact-row">
                  <span class="text-xs font-medium">{t.noGapModeLabel}</span>
                  <ToggleSwitch
                    size="sm"
                    checked={s.reader.vertical.noGapMode}
                    ariaLabel={t.noGapModeLabel}
                    onchange={(checked) => {
                      patch((d) => {
                        d.reader.vertical.noGapMode = checked;
                        if (!checked && d.reader.vertical.pageGapPx === 0) {
                          d.reader.vertical.pageGapPx = 12;
                          d.reader.gapPx = 12;
                        }
                      });
                    }}
                  />
                </div>

                {#if !s.reader.vertical.noGapMode}
                  <div class="slider-row">
                    <span class="text-xs" style="color: var(--text-muted);">{t.previewGapChip}</span>
                    <input
                      type="range"
                      min="0"
                      max="48"
                      step="2"
                      value={s.reader.vertical.pageGapPx}
                      aria-label={t.gapLabel(s.reader.vertical.pageGapPx)}
                      oninput={(e) => {
                        const v = Number(e.currentTarget.value);
                        patch((d) => {
                          d.reader.vertical.pageGapPx = v;
                          d.reader.gapPx = v;
                        }, 150);
                      }}
                      class="flex-1"
                    />
                    <span class="slider-val">{s.reader.vertical.pageGapPx}px</span>
                  </div>
                {/if}

                <div class="slider-row">
                  <span class="text-xs" style="color: var(--text-muted);">{t.previewMarginChip}</span>
                  <input
                    type="range"
                    min="0"
                    max="64"
                    step="2"
                    value={s.reader.behavior.verticalMarginPx}
                    aria-label={t.verticalMarginLabel(s.reader.behavior.verticalMarginPx)}
                    oninput={(e) => {
                      const v = Number(e.currentTarget.value);
                      patch((d) => (d.reader.behavior.verticalMarginPx = v), 150);
                    }}
                    class="flex-1"
                  />
                  <span class="slider-val">{s.reader.behavior.verticalMarginPx}px</span>
                </div>
              {:else}
                <div class="compact-row">
                  <span class="text-xs font-medium">{t.horizontalDirLabel}</span>
                  <div class="flex gap-1.5">
                    <button
                      type="button"
                      onclick={() => patch((d) => (d.reader.horizontal.direction = "ltr" as HorizontalDirection))}
                      class="chip inline-flex items-center gap-1"
                      class:chip-active={s.reader.horizontal.direction === "ltr"}
                    >
                      <span>LTR</span>
                      <ArrowRight size={12} />
                    </button>
                    <button
                      type="button"
                      onclick={() => patch((d) => (d.reader.horizontal.direction = "rtl" as HorizontalDirection))}
                      class="chip inline-flex items-center gap-1"
                      class:chip-active={s.reader.horizontal.direction === "rtl"}
                    >
                      <ArrowLeft size={12} />
                      <span>RTL</span>
                    </button>
                  </div>
                </div>

                <div class="compact-row">
                  <span class="text-xs font-medium">{t.pagesPerGroupLabel}</span>
                  <div class="flex items-center gap-1">
                    {#each [1, 2, 3] as n (n)}
                      <button
                        type="button"
                        onclick={() => patch((d) => (d.reader.horizontal.pagesPerGroup = n))}
                        class="chip"
                        class:chip-active={s.reader.horizontal.pagesPerGroup === n}
                      >
                        {n}
                      </button>
                    {/each}
                    <input
                      type="number"
                      min="1"
                      max="8"
                      value={s.reader.horizontal.pagesPerGroup}
                      aria-label={t.pagesPerGroupLabel}
                      onchange={(e) => {
                        const v = Math.max(1, Math.min(8, Math.round(Number(e.currentTarget.value) || 1)));
                        patch((d) => (d.reader.horizontal.pagesPerGroup = v));
                      }}
                      class="w-12 rounded-lg border px-1.5 py-1 text-center text-xs"
                      style="background-color: var(--surface); border-color: var(--border); color: var(--text);"
                    />
                  </div>
                </div>

                <div class="compact-row">
                  <span class="text-xs font-medium">{t.horizontalNoGapLabel}</span>
                  <ToggleSwitch
                    size="sm"
                    checked={s.reader.horizontal.noGapMode}
                    ariaLabel={t.horizontalNoGapLabel}
                    onchange={(checked) => patch((d) => (d.reader.horizontal.noGapMode = checked))}
                  />
                </div>

                {#if !s.reader.horizontal.noGapMode}
                  <div class="slider-row">
                    <span class="text-xs" style="color: var(--text-muted);">{t.previewGapChip}</span>
                    <input
                      type="range"
                      min="0"
                      max="48"
                      step="2"
                      value={s.reader.horizontal.pageGapPx}
                      aria-label={t.pageGapLabel(s.reader.horizontal.pageGapPx)}
                      oninput={(e) => {
                        const v = Number(e.currentTarget.value);
                        patch((d) => (d.reader.horizontal.pageGapPx = v), 150);
                      }}
                      class="flex-1"
                    />
                    <span class="slider-val">{s.reader.horizontal.pageGapPx}px</span>
                  </div>

                  <div class="slider-row">
                    <span class="text-xs" style="color: var(--text-muted);">{t.groupGapLabel(s.reader.horizontal.groupGapPx)}</span>
                    <input
                      type="range"
                      min="8"
                      max="96"
                      step="4"
                      value={s.reader.horizontal.groupGapPx}
                      aria-label={t.groupGapLabel(s.reader.horizontal.groupGapPx)}
                      oninput={(e) => {
                        const v = Number(e.currentTarget.value);
                        patch((d) => (d.reader.horizontal.groupGapPx = v), 150);
                      }}
                      class="flex-1"
                    />
                    <span class="slider-val">{s.reader.horizontal.groupGapPx}px</span>
                  </div>
                {/if}

                <div class="compact-row">
                  <span class="text-xs font-medium">{t.centerGroupsLabel}</span>
                  <ToggleSwitch
                    size="sm"
                    checked={s.reader.horizontal.centerGroups}
                    ariaLabel={t.centerGroupsLabel}
                    onchange={(checked) => patch((d) => (d.reader.horizontal.centerGroups = checked))}
                  />
                </div>
              {/if}
            </div>

            <!-- Reader Background Swatches + Dim Unfocused -->
            <div class="flex flex-col gap-2.5 border-t pt-3" style="border-color: var(--border);">
              <div class="compact-row">
                <span class="text-xs font-medium">{t.readerBg}</span>
                <div class="flex flex-wrap gap-1.5">
                  <button
                    type="button"
                    onclick={() => patch((d) => (d.reader.readerBg = "dark" as ReaderBg))}
                    class="chip inline-flex items-center gap-1.5"
                    class:chip-active={s.reader.readerBg === "dark"}
                  >
                    <span class="h-2.5 w-2.5 rounded-full border border-zinc-600 bg-zinc-950"></span>
                    <span>{t.bgDark}</span>
                  </button>
                  <button
                    type="button"
                    onclick={() => patch((d) => (d.reader.readerBg = "light" as ReaderBg))}
                    class="chip inline-flex items-center gap-1.5"
                    class:chip-active={s.reader.readerBg === "light"}
                  >
                    <span class="h-2.5 w-2.5 rounded-full border border-zinc-400 bg-zinc-100"></span>
                    <span>{t.bgLight}</span>
                  </button>
                  <button
                    type="button"
                    onclick={() => patch((d) => (d.reader.readerBg = "follow_theme" as ReaderBg))}
                    class="chip inline-flex items-center gap-1.5"
                    class:chip-active={s.reader.readerBg === "follow_theme"}
                  >
                    <Palette size={11} />
                    <span>{t.bgAppTheme}</span>
                  </button>
                </div>
              </div>

              {#if !isHorizontal}
                <div class="compact-row">
                  <span class="text-xs font-medium">{t.dimUnfocusedLabel}</span>
                  <ToggleSwitch
                    size="sm"
                    checked={s.reader.vertical.dimUnfocused}
                    ariaLabel={t.dimUnfocusedLabel}
                    onchange={(checked) => patch((d) => (d.reader.vertical.dimUnfocused = checked))}
                  />
                </div>
                {#if s.reader.vertical.dimUnfocused}
                  <div class="slider-row">
                    <span class="text-xs" style="color: var(--text-muted);">{t.dimLevelLabel(s.reader.vertical.dimLevel)}</span>
                    <input
                      type="range"
                      min="0"
                      max="100"
                      step="5"
                      value={s.reader.vertical.dimLevel}
                      aria-label={t.dimLevelLabel(s.reader.vertical.dimLevel)}
                      oninput={(e) => {
                        const v = Number(e.currentTarget.value);
                        patch((d) => (d.reader.vertical.dimLevel = v), 120);
                      }}
                      class="flex-1"
                    />
                  </div>
                {/if}
              {:else}
                <div class="compact-row">
                  <span class="text-xs font-medium">{t.dimUnfocusedLabel}</span>
                  <ToggleSwitch
                    size="sm"
                    checked={s.reader.horizontal.dimUnfocused}
                    ariaLabel={t.dimUnfocusedLabel}
                    onchange={(checked) => patch((d) => (d.reader.horizontal.dimUnfocused = checked))}
                  />
                </div>
                {#if s.reader.horizontal.dimUnfocused}
                  <div class="slider-row">
                    <span class="text-xs" style="color: var(--text-muted);">{t.dimLevelLabel(s.reader.horizontal.dimLevel)}</span>
                    <input
                      type="range"
                      min="0"
                      max="100"
                      step="5"
                      value={s.reader.horizontal.dimLevel}
                      aria-label={t.dimLevelLabel(s.reader.horizontal.dimLevel)}
                      oninput={(e) => {
                        const v = Number(e.currentTarget.value);
                        patch((d) => (d.reader.horizontal.dimLevel = v), 120);
                      }}
                      class="flex-1"
                    />
                  </div>
                {/if}
              {/if}
            </div>
          </section>

          <!-- Right Bento Card: Controls, Icon Toggle Grid & Performance Tuning -->
          <section class="bento-card">
            <div class="card-head">
              <div class="flex items-center gap-2.5">
                <span class="card-icon" style="background-color: var(--accent-soft); color: var(--accent);">
                  <LayoutGrid size={15} />
                </span>
                <div>
                  <h2 class="card-title">{t.readerCatControls} &amp; {t.readerCatBehavior}</h2>
                  <p class="card-sub">{doubleTapLabel(s.reader.behavior.doubleTapAction)} · {t.preloadLabel(s.reader.preloadImages)}</p>
                </div>
              </div>
            </div>

            <!-- Double-Tap Action Visual Icon Segmented Bar -->
            <div class="flex flex-col gap-1.5">
              <span class="text-xs font-medium" style="color: var(--text-muted);">{t.doubleTapActionLabel}</span>
              <div class="grid grid-cols-2 gap-1.5 sm:grid-cols-4">
                <button
                  type="button"
                  onclick={() => patch((d) => (d.reader.behavior.doubleTapAction = "zoom"))}
                  class="icon-seg"
                  class:icon-seg-active={s.reader.behavior.doubleTapAction === "zoom"}
                >
                  <ZoomIn size={14} />
                  <span class="truncate">{t.doubleTapZoom}</span>
                </button>
                <button
                  type="button"
                  onclick={() => patch((d) => (d.reader.behavior.doubleTapAction = "toggle_toolbar"))}
                  class="icon-seg"
                  class:icon-seg-active={s.reader.behavior.doubleTapAction === "toggle_toolbar"}
                >
                  <PanelTop size={14} />
                  <span class="truncate">{t.doubleTapToggleToolbar}</span>
                </button>
                <button
                  type="button"
                  onclick={() => patch((d) => (d.reader.behavior.doubleTapAction = "next_page"))}
                  class="icon-seg"
                  class:icon-seg-active={s.reader.behavior.doubleTapAction === "next_page"}
                >
                  <SkipForward size={14} />
                  <span class="truncate">{t.doubleTapNextPage}</span>
                </button>
                <button
                  type="button"
                  onclick={() => patch((d) => (d.reader.behavior.doubleTapAction = "disabled"))}
                  class="icon-seg"
                  class:icon-seg-active={s.reader.behavior.doubleTapAction === "disabled"}
                >
                  <Ban size={14} />
                  <span class="truncate">{t.doubleTapDisabled}</span>
                </button>
              </div>
            </div>

            <!-- Compact 2-Column Icon Toggle Tiles (replaces 6 long verbose rows) -->
            <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
              {#if !isHorizontal}
                <div class="toggle-tile" class:toggle-tile-on={s.reader.autoNextChapter} title={t.autoAppendDesc}>
                  <div class="flex min-w-0 items-center gap-2">
                    <ArrowDownToLine size={14} class="shrink-0" style="color: {s.reader.autoNextChapter ? 'var(--accent)' : 'var(--text-muted)'};" />
                    <span class="truncate text-xs font-medium">{t.autoAppendNext}</span>
                  </div>
                  <ToggleSwitch
                    size="sm"
                    checked={s.reader.autoNextChapter}
                    ariaLabel={t.autoAppendNext}
                    onchange={(checked) => patch((d) => (d.reader.autoNextChapter = checked))}
                  />
                </div>
              {/if}

              <div class="toggle-tile" class:toggle-tile-on={s.reader.autoWebtoonDetect} title={t.autoWebtoonDesc}>
                <div class="flex min-w-0 items-center gap-2">
                  <Sparkles size={14} class="shrink-0" style="color: {s.reader.autoWebtoonDetect ? 'var(--accent)' : 'var(--text-muted)'};" />
                  <span class="truncate text-xs font-medium">{t.autoWebtoonLabel}</span>
                </div>
                <ToggleSwitch
                  size="sm"
                  checked={s.reader.autoWebtoonDetect}
                  ariaLabel={t.autoWebtoonLabel}
                  onchange={(checked) => patch((d) => (d.reader.autoWebtoonDetect = checked))}
                />
              </div>

              <div class="toggle-tile" class:toggle-tile-on={s.reader.tapZonesEnabled} title={t.tapZonesDesc}>
                <div class="flex min-w-0 items-center gap-2">
                  <LayoutGrid size={14} class="shrink-0" style="color: {s.reader.tapZonesEnabled ? 'var(--accent)' : 'var(--text-muted)'};" />
                  <span class="truncate text-xs font-medium">{t.tapZonesLabel}</span>
                </div>
                <ToggleSwitch
                  size="sm"
                  checked={s.reader.tapZonesEnabled}
                  ariaLabel={t.tapZonesLabel}
                  onchange={(checked) => patch((d) => (d.reader.tapZonesEnabled = checked))}
                />
              </div>

              <div class="toggle-tile" class:toggle-tile-on={s.reader.behavior.keepScreenOn} title={t.keepScreenOnDesc}>
                <div class="flex min-w-0 items-center gap-2">
                  <SunMedium size={14} class="shrink-0" style="color: {s.reader.behavior.keepScreenOn ? 'var(--accent)' : 'var(--text-muted)'};" />
                  <span class="truncate text-xs font-medium">{t.keepScreenOnLabel}</span>
                </div>
                <ToggleSwitch
                  size="sm"
                  checked={s.reader.behavior.keepScreenOn}
                  ariaLabel={t.keepScreenOnLabel}
                  onchange={(checked) => patch((d) => (d.reader.behavior.keepScreenOn = checked))}
                />
              </div>

              <div class="toggle-tile" class:toggle-tile-on={s.reader.behavior.autoMarkReadOnOpen} title={t.autoMarkReadDesc}>
                <div class="flex min-w-0 items-center gap-2">
                  <CheckCheck size={14} class="shrink-0" style="color: {s.reader.behavior.autoMarkReadOnOpen ? 'var(--accent)' : 'var(--text-muted)'};" />
                  <span class="truncate text-xs font-medium">{t.autoMarkReadLabel}</span>
                </div>
                <ToggleSwitch
                  size="sm"
                  checked={s.reader.behavior.autoMarkReadOnOpen}
                  ariaLabel={t.autoMarkReadLabel}
                  onchange={(checked) => patch((d) => (d.reader.behavior.autoMarkReadOnOpen = checked))}
                />
              </div>

              <div class="toggle-tile" class:toggle-tile-on={s.reader.behavior.skipBrokenImages} title={t.skipBrokenImagesDesc}>
                <div class="flex min-w-0 items-center gap-2">
                  <ImageOff size={14} class="shrink-0" style="color: {s.reader.behavior.skipBrokenImages ? 'var(--accent)' : 'var(--text-muted)'};" />
                  <span class="truncate text-xs font-medium">{t.skipBrokenImagesLabel}</span>
                </div>
                <ToggleSwitch
                  size="sm"
                  checked={s.reader.behavior.skipBrokenImages}
                  ariaLabel={t.skipBrokenImagesLabel}
                  onchange={(checked) => patch((d) => (d.reader.behavior.skipBrokenImages = checked))}
                />
              </div>
            </div>

            <!-- Compact Performance & Timing Tuning Strip -->
            <div
              class="flex flex-col gap-3 rounded-xl border p-3"
              style="background-color: var(--surface-subtle); border-color: var(--border);"
            >
              <div class="slider-row">
                <span class="inline-flex items-center gap-1.5 text-xs font-medium">
                  <Gauge size={13} style="color: var(--text-muted);" />
                  <span>{t.navigationSpeedLabel(s.reader.behavior.navigationSpeed)}</span>
                </span>
                <input
                  type="range"
                  min="1"
                  max="10"
                  step="1"
                  value={s.reader.behavior.navigationSpeed}
                  aria-label={t.navigationSpeedLabel(s.reader.behavior.navigationSpeed)}
                  oninput={(e) => {
                    const v = Math.max(1, Math.min(10, Math.round(Number(e.currentTarget.value))));
                    patch((d) => (d.reader.behavior.navigationSpeed = v), 100);
                  }}
                  class="w-32 sm:w-40"
                />
              </div>

              <div class="slider-row">
                <span class="inline-flex items-center gap-1.5 text-xs font-medium">
                  <Layers size={13} style="color: var(--text-muted);" />
                  <span>{t.preloadLabel(s.reader.preloadImages)}</span>
                </span>
                <input
                  type="range"
                  min="5"
                  max="8"
                  step="1"
                  value={s.reader.preloadImages}
                  aria-label={t.preloadLabel(s.reader.preloadImages)}
                  oninput={(e) => {
                    const v = Number(e.currentTarget.value);
                    patch((d) => (d.reader.preloadImages = v), 150);
                  }}
                  class="w-32 sm:w-40"
                />
              </div>

              <div class="compact-row">
                <label for="toolbar-autohide" class="inline-flex items-center gap-1.5 text-xs font-medium">
                  <Timer size={13} style="color: var(--text-muted);" />
                  <span>{t.toolbarAutoHideLabel}</span>
                </label>
                <select
                  id="toolbar-autohide"
                  value={s.reader.behavior.toolbarAutoHideSecs}
                  onchange={(e) => {
                    const secs = Number(e.currentTarget.value);
                    patch((d) => (d.reader.behavior.toolbarAutoHideSecs = secs));
                  }}
                  class="field-select"
                >
                  <option value={0}>{t.toolbarAutoHideOff}</option>
                  {#each [2, 3, 5, 8, 12] as sec (sec)}
                    <option value={sec}>{t.secondsOption(sec)}</option>
                  {/each}
                </select>
              </div>
            </div>
          </section>
        </div>
      {/if}

      <!-- ==================== TAB 3: FLOATING CONTROLS (Compact + Live Edge Mini-Map) ==================== -->
      {#if activeTab === "floating"}
        <section id="settings-floating" class="bento-card">
          <div class="card-head">
            <div class="flex items-center gap-2.5">
              <span class="card-icon" style="background-color: var(--accent-soft); color: var(--accent);">
                <CircleDot size={15} />
              </span>
              <div>
                <h2 class="card-title">{t.floatingSection}</h2>
                <p class="card-sub">{t.floatingSubtitle}</p>
              </div>
            </div>
            <div class="flex items-center gap-2">
              <div
                class="hidden items-center gap-2 rounded-lg border px-2.5 py-1 text-xs sm:flex"
                style="border-color: var(--border); background-color: var(--surface-subtle);"
              >
                <span style="color: var(--text-muted);">{t.floatingAutoDimLabel}</span>
                <ToggleSwitch
                  size="sm"
                  checked={s.floating.autoDimInReader}
                  ariaLabel={t.floatingAutoDimLabel}
                  onchange={(checked) => patch((d) => (d.floating.autoDimInReader = checked))}
                />
              </div>
              <button
                type="button"
                onclick={() => handleResetSection("floating")}
                disabled={resettingSection === "floating"}
                class="icon-btn"
                title={t.resetSectionBtn}
                aria-label={t.resetSectionBtn}
              >
                <RotateCcw size={13} />
              </button>
            </div>
          </div>

          <!-- Compact Icon Chips for Scroll-to-Top Screens -->
          <div class="flex flex-col gap-2">
            <div class="text-xs font-medium" style="color: var(--text-muted);">
              {t.floatingScrollTopScreens}
            </div>
            <div class="flex flex-wrap gap-1.5">
              {#each floatingScreens as screen (screen)}
                {@const active = s.floating.scrollToTopScreens.includes(screen)}
                <button
                  type="button"
                  onclick={() => toggleScrollTopScreen(screen)}
                  class="chip inline-flex items-center gap-1.5"
                  class:chip-active={active}
                >
                  {#if screen === "home"}
                    <Home size={12} />
                  {:else if screen === "browse"}
                    <Compass size={12} />
                  {:else if screen === "bookmarks"}
                    <Bookmark size={12} />
                  {:else if screen === "history"}
                    <Clock size={12} />
                  {:else if screen === "manga"}
                    <Book size={12} />
                  {:else if screen === "reader"}
                    <BookOpen size={12} />
                  {:else}
                    <SettingsIcon size={12} />
                  {/if}
                  <span>{floatingScreenLabel(screen)}</span>
                </button>
              {/each}
            </div>
          </div>

          <!-- 3 Compact Single-Line Position Rows + Live Edge Preview Mini-Map -->
          <div class="flex flex-col gap-3.5 lg:flex-row">
            <div class="flex flex-1 flex-col gap-2">
              <!-- Row 1: Scroll-to-Top Position -->
              <div class="float-row">
                <div class="flex min-w-0 items-center gap-2">
                  <span
                    class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full"
                    style="background-color: var(--accent); color: var(--accent-fg);"
                  >
                    <ArrowUp size={12} />
                  </span>
                  <span class="truncate text-xs font-semibold">{t.floatingScrollTopPos}</span>
                </div>
                <div class="flex flex-wrap items-center gap-1.5">
                  <button
                    type="button"
                    onclick={() => patch((d) => (d.floating.scrollToTop.edge = "left" as FloatingEdge))}
                    class="edge-btn"
                    class:edge-btn-active={s.floating.scrollToTop.edge === "left"}
                    title={t.floatingEdgeLeft}
                    aria-label={t.floatingEdgeLeft}
                  >
                    <PanelLeft size={13} />
                  </button>
                  <button
                    type="button"
                    onclick={() => patch((d) => (d.floating.scrollToTop.edge = "right" as FloatingEdge))}
                    class="edge-btn"
                    class:edge-btn-active={s.floating.scrollToTop.edge === "right"}
                    title={t.floatingEdgeRight}
                    aria-label={t.floatingEdgeRight}
                  >
                    <PanelRight size={13} />
                  </button>
                  <input
                    type="range"
                    min="15"
                    max="88"
                    step="1"
                    value={s.floating.scrollToTop.verticalPercent}
                    aria-label={t.floatingScrollTopPos}
                    oninput={(e) => {
                      const v = Number(e.currentTarget.value);
                      patch((d) => (d.floating.scrollToTop.verticalPercent = v), 120);
                    }}
                    class="w-24 sm:w-28"
                  />
                  <span class="slider-val">{s.floating.scrollToTop.verticalPercent}%</span>
                </div>
              </div>

              <!-- Row 2: Quick Reader Settings -->
              <div class="float-row">
                <div class="flex min-w-0 items-center gap-2">
                  <ToggleSwitch
                    size="sm"
                    checked={s.floating.showReaderSettings}
                    ariaLabel={t.floatingReaderSettingsLabel}
                    onchange={(checked) => patch((d) => (d.floating.showReaderSettings = checked))}
                  />
                  <span
                    class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-white"
                    style="background-color: #3b82f6; opacity: {s.floating.showReaderSettings ? 1 : 0.35};"
                  >
                    <SettingsIcon size={12} />
                  </span>
                  <span class="truncate text-xs font-semibold">{t.floatingReaderSettingsLabel}</span>
                </div>
                {#if s.floating.showReaderSettings}
                  <div class="flex flex-wrap items-center gap-1.5">
                    <button
                      type="button"
                      onclick={() => patch((d) => (d.floating.readerSettings.edge = "left" as FloatingEdge))}
                      class="edge-btn"
                      class:edge-btn-active={s.floating.readerSettings.edge === "left"}
                      title={t.floatingEdgeLeft}
                      aria-label={t.floatingEdgeLeft}
                    >
                      <PanelLeft size={13} />
                    </button>
                    <button
                      type="button"
                      onclick={() => patch((d) => (d.floating.readerSettings.edge = "right" as FloatingEdge))}
                      class="edge-btn"
                      class:edge-btn-active={s.floating.readerSettings.edge === "right"}
                      title={t.floatingEdgeRight}
                      aria-label={t.floatingEdgeRight}
                    >
                      <PanelRight size={13} />
                    </button>
                    <input
                      type="range"
                      min="15"
                      max="88"
                      step="1"
                      value={s.floating.readerSettings.verticalPercent}
                      aria-label={t.floatingReaderSettingsLabel}
                      oninput={(e) => {
                        const v = Number(e.currentTarget.value);
                        patch((d) => (d.floating.readerSettings.verticalPercent = v), 120);
                      }}
                      class="w-24 sm:w-28"
                    />
                    <span class="slider-val">{s.floating.readerSettings.verticalPercent}%</span>
                  </div>
                {/if}
              </div>

              <!-- Row 3: Orientation Lock -->
              <div class="float-row">
                <div class="flex min-w-0 items-center gap-2">
                  <ToggleSwitch
                    size="sm"
                    checked={s.floating.showOrientationLock}
                    ariaLabel={t.floatingOrientationLockLabel}
                    onchange={(checked) => patch((d) => (d.floating.showOrientationLock = checked))}
                  />
                  <span
                    class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-white"
                    style="background-color: #8b5cf6; opacity: {s.floating.showOrientationLock ? 1 : 0.35};"
                  >
                    <Lock size={12} />
                  </span>
                  <span class="truncate text-xs font-semibold">{t.floatingOrientationLockLabel}</span>
                </div>
                {#if s.floating.showOrientationLock}
                  <div class="flex flex-wrap items-center gap-1.5">
                    <button
                      type="button"
                      onclick={() => patch((d) => (d.floating.orientationLock.edge = "left" as FloatingEdge))}
                      class="edge-btn"
                      class:edge-btn-active={s.floating.orientationLock.edge === "left"}
                      title={t.floatingEdgeLeft}
                      aria-label={t.floatingEdgeLeft}
                    >
                      <PanelLeft size={13} />
                    </button>
                    <button
                      type="button"
                      onclick={() => patch((d) => (d.floating.orientationLock.edge = "right" as FloatingEdge))}
                      class="edge-btn"
                      class:edge-btn-active={s.floating.orientationLock.edge === "right"}
                      title={t.floatingEdgeRight}
                      aria-label={t.floatingEdgeRight}
                    >
                      <PanelRight size={13} />
                    </button>
                    <input
                      type="range"
                      min="15"
                      max="88"
                      step="1"
                      value={s.floating.orientationLock.verticalPercent}
                      aria-label={t.floatingOrientationLockLabel}
                      oninput={(e) => {
                        const v = Number(e.currentTarget.value);
                        patch((d) => (d.floating.orientationLock.verticalPercent = v), 120);
                      }}
                      class="w-24 sm:w-28"
                    />
                    <span class="slider-val">{s.floating.orientationLock.verticalPercent}%</span>
                  </div>
                {/if}
              </div>

              <!-- Auto-dim row on mobile screens -->
              <div class="compact-row rounded-xl border px-3 py-2.5 sm:hidden" style="border-color: var(--border); background-color: var(--surface-subtle);">
                <span class="text-xs font-medium">{t.floatingAutoDimLabel}</span>
                <ToggleSwitch
                  size="sm"
                  checked={s.floating.autoDimInReader}
                  ariaLabel={t.floatingAutoDimLabel}
                  onchange={(checked) => patch((d) => (d.floating.autoDimInReader = checked))}
                />
              </div>
            </div>

            <!-- Live Screen-Edge Position Preview -->
            <div
              class="relative flex h-44 w-full shrink-0 items-center justify-center rounded-xl border lg:h-auto lg:min-h-44 lg:w-36"
              style="border-color: var(--border); background-color: var(--bg);"
            >
              <span class="px-2 text-center text-[10px] font-medium" style="color: var(--text-muted);">
                {t.settingsFloatPreviewTitle}
              </span>

              {#if s.floating.scrollToTopScreens.length > 0}
                <span
                  title={t.floatingScrollTopPos}
                  class="absolute flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-full shadow-sm transition-all"
                  style="top: {s.floating.scrollToTop.verticalPercent}%; {s.floating.scrollToTop.edge === 'left'
                    ? 'left: 8px;'
                    : 'right: 8px;'} background-color: var(--accent); color: var(--accent-fg);"
                >
                  <ArrowUp size={12} />
                </span>
              {/if}

              {#if s.floating.showReaderSettings}
                <span
                  title={t.floatingReaderSettingsLabel}
                  class="absolute flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-full text-white shadow-sm transition-all"
                  style="top: {s.floating.readerSettings.verticalPercent}%; {s.floating.readerSettings.edge === 'left'
                    ? 'left: 8px;'
                    : 'right: 8px;'} background-color: #3b82f6;"
                >
                  <SettingsIcon size={12} />
                </span>
              {/if}

              {#if s.floating.showOrientationLock}
                <span
                  title={t.floatingOrientationLockLabel}
                  class="absolute flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-full text-white shadow-sm transition-all"
                  style="top: {s.floating.orientationLock.verticalPercent}%; {s.floating.orientationLock.edge === 'left'
                    ? 'left: 8px;'
                    : 'right: 8px;'} background-color: #8b5cf6;"
                >
                  <Lock size={12} />
                </span>
              {/if}
            </div>
          </div>
        </section>
      {/if}

      <!-- ==================== TAB 4: NETWORK & LAN SYNC (2-Col Bento) ==================== -->
      {#if activeTab === "network"}
        <div id="settings-network" class="grid grid-cols-1 items-start gap-3.5 lg:grid-cols-12">
          <div class="flex flex-col gap-3.5 lg:col-span-7">
            <NetworkProtectionPanel />
          </div>

          <div class="flex flex-col gap-3.5 lg:col-span-5">
            <!-- Sources & Speed Limit Compact Card -->
            <section id="settings-sources" class="bento-card">
              <div class="card-head">
                <div class="flex items-center gap-2.5">
                  <span class="card-icon" style="background-color: var(--accent-soft); color: var(--accent);">
                    <Gauge size={15} />
                  </span>
                  <div>
                    <h2 class="card-title">{t.sourcesSection}</h2>
                    <p class="card-sub">{speedValueLabel(s.sources.speedLimitKbps)}</p>
                  </div>
                </div>
                <button
                  type="button"
                  onclick={() => handleResetSection("sources")}
                  disabled={resettingSection === "sources"}
                  class="icon-btn"
                  title={t.resetSectionBtn}
                  aria-label={t.resetSectionBtn}
                >
                  <RotateCcw size={13} />
                </button>
              </div>

              <!-- Bandwidth Preset Chips + Custom Input -->
              <div class="flex flex-col gap-2">
                <div class="compact-row">
                  <span class="text-xs font-medium">{t.speedLimitLabel}</span>
                  <div class="flex items-center gap-1">
                    <input
                      type="number"
                      min="0"
                      max="102400"
                      step="128"
                      value={s.sources.speedLimitKbps}
                      aria-label={t.speedLimitLabel}
                      onchange={(e) => {
                        const raw = Math.max(0, Math.min(102400, Math.round(Number(e.currentTarget.value) || 0)));
                        patch((d) => (d.sources.speedLimitKbps = raw));
                      }}
                      class="w-20 rounded-lg border px-2 py-1 text-right text-xs"
                      style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
                    />
                    <span class="text-[11px]" style="color: var(--text-muted);">KB/s</span>
                  </div>
                </div>

                <div class="flex flex-wrap gap-1">
                  {#each speedPresets as preset (preset.kbps)}
                    <button
                      type="button"
                      onclick={() => patch((d) => (d.sources.speedLimitKbps = preset.kbps))}
                      class="chip"
                      class:chip-active={s.sources.speedLimitKbps === preset.kbps}
                    >
                      {speedPresetLabel(preset.labelKey)}
                    </button>
                  {/each}
                </div>
              </div>

              <div class="compact-row border-t pt-3" style="border-color: var(--border);" title={t.headlessDesc}>
                <span class="text-xs font-medium">{t.headlessLabel}</span>
                <ToggleSwitch
                  size="sm"
                  checked={s.sources.headlessEnabled}
                  ariaLabel={t.headlessLabel}
                  onchange={(checked) => patch((d) => (d.sources.headlessEnabled = checked))}
                />
              </div>
            </section>

            <!-- Serverless LAN Sync -->
            <LanSyncPanel />
          </div>

          <!-- Supplemental Sources Manager (18+ / Mature Isolation) -->
          <section id="settings-supplemental-sources" class="bento-card lg:col-span-12">
            <div class="card-head">
              <div class="flex items-center gap-2.5">
                <span class="card-icon" style="background-color: var(--accent-soft); color: var(--accent);">
                  <Compass size={15} />
                </span>
                <div>
                  <h2 class="card-title">{t.suppSourcesTitle}</h2>
                  <p class="card-sub">{t.suppSourcesDesc}</p>
                </div>
              </div>
            </div>
            <SourceManagerPanel scope="supplemental" />
          </section>
        </div>
      {/if}

      <!-- ==================== TAB 5: STORAGE & HIDDEN MANGA (Tabbed Scoped View) ==================== -->
      {#if activeTab === "storage"}
        <section id="settings-cache" class="bento-card">
          <div class="card-head">
            <div class="flex items-center gap-2.5">
              <span class="card-icon" style="background-color: var(--accent-soft); color: var(--accent);">
                <HardDrive size={15} />
              </span>
              <div>
                <h2 class="card-title">{t.cacheSection}</h2>
                <p class="card-sub">{t.cachePreservesUserDataNote}</p>
              </div>
            </div>

            <div class="flex items-center gap-1.5">
              {#if confirmingClearAllCache}
                <button
                  type="button"
                  onclick={clearAllCache}
                  disabled={clearingCache}
                  class="rounded-lg px-2.5 py-1.5 text-xs font-semibold text-white"
                  style="background-color: var(--danger);"
                >
                  {clearingCache ? t.clearing : t.confirmClear}
                </button>
                <button
                  type="button"
                  onclick={() => {
                    confirmingClearAllCache = false;
                  }}
                  class="compact-btn"
                >
                  {t.cancel}
                </button>
              {:else}
                <button
                  type="button"
                  onclick={() => {
                    confirmingClearAllCache = true;
                  }}
                  disabled={clearingCache}
                  class="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1.5 text-xs font-medium"
                  style="border-color: var(--border); background-color: var(--surface-subtle); color: var(--danger);"
                >
                  <Trash2 size={13} />
                  <span>{t.clearCache}</span>
                </button>
              {/if}

              <button
                type="button"
                onclick={() => handleResetSection("cache")}
                disabled={resettingSection === "cache"}
                class="icon-btn"
                title={t.resetSectionBtn}
                aria-label={t.resetSectionBtn}
              >
                <RotateCcw size={13} />
              </button>
            </div>
          </div>

          <!-- Visual Storage Gauge Bar + Max Cache Inline Slider -->
          <div
            class="flex flex-col gap-2.5 rounded-xl border p-3.5"
            style="background-color: var(--surface-subtle); border-color: var(--border);"
          >
            <div class="flex flex-wrap items-baseline justify-between gap-2 text-xs">
              <span class="font-semibold">
                {#if cache}
                  {t.cacheUsageDesc(formatBytes(cache.bytes), cache.files)}
                {:else}
                  {t.calculating}
                {/if}
              </span>
              <span style="color: var(--text-muted);">{t.maxCacheLabel(s.cache.maxMb)}</span>
            </div>

            <div class="h-2 w-full overflow-hidden rounded-full" style="background-color: var(--border);">
              <div
                class="h-full rounded-full transition-all duration-300"
                style="width: {cacheUsagePercent}%; background-color: var(--accent);"
              ></div>
            </div>

            <div class="flex items-center gap-3">
              <span class="text-[11px]" style="color: var(--text-muted);">64 MB</span>
              <input
                type="range"
                min="64"
                max="2048"
                step="64"
                value={s.cache.maxMb}
                aria-label={t.maxCacheLabel(s.cache.maxMb)}
                oninput={(e) => {
                  const v = Number(e.currentTarget.value);
                  patch((d) => (d.cache.maxMb = v), 150);
                }}
                class="flex-1"
              />
              <span class="text-[11px]" style="color: var(--text-muted);">2 GB</span>
            </div>
          </div>

          <!-- Segmented Sub-Tabs: By Source | By Manga | Hidden Manga -->
          <div class="flex flex-wrap items-center gap-1.5 border-b pb-2.5" style="border-color: var(--border);">
            <button
              type="button"
              onclick={() => (storageSubTab = "source")}
              class="chip"
              class:chip-active={storageSubTab === "source"}
            >
              {t.settingsBySourceTabLabel(cacheBySource.length)}
            </button>
            <button
              type="button"
              onclick={() => (storageSubTab = "manga")}
              class="chip"
              class:chip-active={storageSubTab === "manga"}
            >
              {t.settingsByMangaTabLabel(cacheByManga.length)}
            </button>
            <button
              type="button"
              onclick={() => (storageSubTab = "hidden")}
              class="chip inline-flex items-center gap-1"
              class:chip-active={storageSubTab === "hidden"}
            >
              <EyeOff size={12} />
              <span>{t.settingsHiddenTabLabel(hiddenManga.length)}</span>
            </button>
          </div>

          {#if storageSubTab === "source"}
            {#if cacheBySource.length === 0}
              <div class="empty-box">{t.cacheUsageDesc("0 B", 0)}</div>
            {:else}
              <div class="settings-list-box">
                {#each cacheBySource as item (item.site)}
                  {@const scopeKey = `site:${item.site}`}
                  <div class="list-row">
                    <div class="min-w-0">
                      <div class="truncate text-xs font-semibold">{item.site}</div>
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
                          onclick={() => (confirmingScopeKey = null)}
                          class="rounded-md border px-2 py-1 text-[11px]"
                          style="border-color: var(--border);"
                        >
                          {t.cancel}
                        </button>
                      </div>
                    {:else}
                      <button
                        type="button"
                        onclick={() => (confirmingScopeKey = scopeKey)}
                        class="rounded-md border px-2.5 py-1 text-[11px] font-medium"
                        style="border-color: var(--border); color: var(--danger);"
                      >
                        {t.clearCache}
                      </button>
                    {/if}
                  </div>
                {/each}
              </div>
            {/if}
          {:else if storageSubTab === "manga"}
            {#if cacheByManga.length === 0}
              <div class="empty-box">{t.cacheUsageDesc("0 B", 0)}</div>
            {:else}
              <div class="settings-list-box">
                {#each cacheByManga as item (item.mangaId)}
                  {@const scopeKey = `manga:${item.mangaId}`}
                  <div class="list-row">
                    <div class="min-w-0">
                      <div class="truncate text-xs font-semibold">{item.title}</div>
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
                          onclick={() => (confirmingScopeKey = null)}
                          class="rounded-md border px-2 py-1 text-[11px]"
                          style="border-color: var(--border);"
                        >
                          {t.cancel}
                        </button>
                      </div>
                    {:else}
                      <button
                        type="button"
                        onclick={() => (confirmingScopeKey = scopeKey)}
                        class="rounded-md border px-2.5 py-1 text-[11px] font-medium"
                        style="border-color: var(--border); color: var(--danger);"
                      >
                        {t.clearCache}
                      </button>
                    {/if}
                  </div>
                {/each}
              </div>
            {/if}
          {:else}
            {#if hiddenManga.length === 0}
              <div class="empty-box">{t.hiddenMangaEmpty}</div>
            {:else}
              <div class="settings-list-box">
                {#each hiddenManga as item (item.id)}
                  <div class="list-row">
                    <div class="flex min-w-0 items-center gap-2.5">
                      <div
                        class="h-11 w-8 shrink-0 overflow-hidden rounded-md"
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
                        <div class="truncate text-xs font-semibold">{item.title}</div>
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
                        onclick={() => (pendingDeleteHidden = item)}
                        class="icon-btn"
                        style="color: var(--danger);"
                        title={t.deleteMangaBtn}
                        aria-label={t.deleteMangaBtn}
                      >
                        <Trash2 size={13} />
                      </button>
                    </div>
                  </div>
                {/each}
              </div>
            {/if}
          {/if}
        </section>
      {/if}

      <!-- ==================== TAB 6: SYSTEM (Feedback + Diagnostics + Reset All) ==================== -->
      {#if activeTab === "system"}
        <div class="flex flex-col gap-3.5">
          <section id="settings-feedback" class="bento-card">
            <div class="card-head">
              <div class="flex items-center gap-2.5">
                <span class="card-icon" style="background-color: var(--accent-soft); color: var(--accent);">
                  <AlertCircle size={15} />
                </span>
                <div>
                  <h2 class="card-title">{t.feedbackSectionTitle}</h2>
                  <p class="card-sub">{t.feedbackSectionDesc}</p>
                </div>
              </div>
            </div>
            <FeedbackComposer />
          </section>

          <section class="bento-card">
            <div class="card-head">
              <div class="flex items-center gap-2.5">
                <span class="card-icon" style="background-color: var(--accent-soft); color: var(--accent);">
                  <Wrench size={15} />
                </span>
                <div>
                  <h2 class="card-title">{t.diagnosticsSection}</h2>
                  <p class="card-sub">{t.diagnosticsDesc}</p>
                </div>
              </div>
            </div>
            <DiagnosticsPanel />
          </section>

          <section class="bento-card">
            <div class="compact-row">
              <div class="min-w-0">
                <h2 class="card-title">{t.resetAllSection}</h2>
                <p class="card-sub">{t.resetAllDesc}</p>
              </div>
              <button
                type="button"
                onclick={() => (showResetAllModal = true)}
                class="inline-flex shrink-0 items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-semibold"
                style="border-color: var(--danger); color: var(--danger);"
              >
                <RotateCcw size={13} />
                <span>{t.resetAllBtn}</span>
              </button>
            </div>
          </section>
        </div>
      {/if}
    </div>
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
      class="flex max-h-[calc(var(--app-height,100dvh)-2rem-var(--safe-top)-var(--safe-bottom)-var(--ime-bottom))] w-full max-w-md flex-col gap-4 overflow-y-auto overscroll-contain rounded-2xl border p-5 shadow-2xl"
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
          onclick={() => (showResetAllModal = false)}
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
      class="flex max-h-[calc(var(--app-height,100dvh)-2rem-var(--safe-top)-var(--safe-bottom)-var(--ime-bottom))] w-full max-w-md flex-col gap-4 overflow-y-auto overscroll-contain rounded-2xl border p-5 shadow-2xl"
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
          onclick={() => (pendingDeleteHidden = null)}
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
  .top-tab {
    flex: 1 1 auto;
    min-width: max-content;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.45rem;
    padding: 0.5rem 0.7rem;
    border-radius: 0.7rem;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-muted);
    font-size: 0.78rem;
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
    transition: background-color 140ms ease, color 140ms ease, border-color 140ms ease;
  }

  .top-tab:hover {
    background-color: var(--surface-hover);
    color: var(--text);
  }

  .top-tab-active {
    border-color: var(--accent);
    background-color: var(--accent-soft);
    color: var(--accent);
  }

  .quick-tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.4rem;
    min-height: 4.35rem;
    padding: 0.6rem 0.4rem;
    border-radius: 0.9rem;
    border: 1px solid var(--border);
    background-color: var(--surface);
    color: var(--text);
    cursor: pointer;
  }

  .quick-icon {
    display: flex;
    width: 1.85rem;
    height: 1.85rem;
    align-items: center;
    justify-content: center;
    border-radius: 0.55rem;
  }

  .hub-row {
    display: flex;
    width: 100%;
    min-height: 3.75rem;
    align-items: center;
    gap: 0.75rem;
    padding: 0.65rem 0.9rem;
    border: none;
    border-bottom: 1px solid var(--border);
    background: transparent;
    color: var(--text);
    text-align: left;
    cursor: pointer;
    transition: background-color 120ms ease;
  }

  .hub-row:hover {
    background-color: var(--surface-hover);
  }

  .hub-row-last {
    border-bottom: none;
  }

  .hub-icon {
    display: flex;
    width: 2.25rem;
    height: 2.25rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-radius: 0.65rem;
  }

  .bento-card {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    padding: 1rem 1.15rem;
    border-radius: 1rem;
    border: 1px solid var(--border);
    background-color: var(--surface);
  }

  .card-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem 0.75rem;
  }

  .card-icon {
    display: flex;
    width: 1.9rem;
    height: 1.9rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-radius: 0.55rem;
  }

  .card-title {
    margin: 0;
    font-size: 0.875rem;
    font-weight: 600;
    line-height: 1.25;
  }

  .card-sub {
    margin: 0.1rem 0 0;
    font-size: 0.72rem;
    color: var(--text-muted);
    line-height: 1.3;
  }

  .compact-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem 0.75rem;
  }

  .slider-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.65rem;
  }

  .slider-val {
    min-width: 2.3rem;
    text-align: right;
    font-size: 0.72rem;
    font-weight: 600;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }

  .icon-btn {
    display: inline-flex;
    width: 1.95rem;
    height: 1.95rem;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    border-radius: 0.55rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
    color: var(--text-muted);
    cursor: pointer;
    transition: color 120ms ease, background-color 120ms ease;
  }

  .icon-btn:hover:not(:disabled) {
    color: var(--text);
    background-color: var(--surface-hover);
  }

  .icon-btn:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .compact-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.38rem 0.65rem;
    border-radius: 0.55rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
    color: var(--text);
    font-size: 0.72rem;
    font-weight: 600;
    cursor: pointer;
  }

  .compact-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .theme-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.45rem;
    padding: 0.5rem;
    border-radius: 0.75rem;
    border: 1.5px solid var(--border);
    background-color: var(--surface-subtle);
    color: var(--text);
    cursor: pointer;
    transition: border-color 140ms ease, background-color 140ms ease;
  }

  .theme-card-active {
    border-color: var(--accent);
    background-color: var(--accent-soft);
    color: var(--accent);
  }

  .theme-swatch {
    width: 100%;
    height: 2.2rem;
    border-radius: 0.45rem;
    border: 1px solid var(--border);
  }

  .mode-tile {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 0.6rem 0.75rem;
    border-radius: 0.75rem;
    border: 1.5px solid var(--border);
    background-color: var(--surface-subtle);
    color: var(--text);
    cursor: pointer;
    transition: border-color 140ms ease, background-color 140ms ease;
  }

  .mode-tile-active {
    border-color: var(--accent);
    background-color: var(--accent-soft);
    color: var(--accent);
  }

  .icon-seg {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.25rem;
    padding: 0.45rem 0.4rem;
    border-radius: 0.65rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
    color: var(--text-muted);
    font-size: 0.68rem;
    font-weight: 600;
    cursor: pointer;
  }

  .icon-seg-active {
    border-color: var(--accent);
    background-color: var(--accent-soft);
    color: var(--accent);
  }

  .toggle-tile {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.55rem 0.7rem;
    border-radius: 0.7rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
    transition: border-color 140ms ease, background-color 140ms ease;
  }

  .toggle-tile-on {
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
    background-color: color-mix(in srgb, var(--surface-subtle) 85%, var(--accent-soft));
  }

  .float-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem 0.75rem;
    padding: 0.6rem 0.75rem;
    border-radius: 0.75rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
  }

  .edge-btn {
    display: inline-flex;
    width: 1.75rem;
    height: 1.65rem;
    align-items: center;
    justify-content: center;
    border-radius: 0.45rem;
    border: 1px solid var(--border);
    background-color: var(--surface);
    color: var(--text-muted);
    cursor: pointer;
  }

  .edge-btn-active {
    border-color: var(--accent);
    background-color: var(--accent-soft);
    color: var(--accent);
  }

  .chip {
    padding: 0.3rem 0.6rem;
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
    color: var(--text);
    font-size: 0.72rem;
    font-weight: 600;
    cursor: pointer;
    transition: border-color 120ms ease, background-color 120ms ease, color 120ms ease;
  }

  .chip-active {
    border-color: var(--accent);
    background-color: var(--accent-soft);
    color: var(--accent);
  }

  .field-select {
    border-radius: 0.55rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
    color: var(--text);
    padding: 0.35rem 0.65rem;
    font-size: 0.75rem;
    font-weight: 500;
    outline: none;
  }

  .field-input {
    border-radius: 0.55rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
    color: var(--text);
    padding: 0.42rem 0.7rem;
    font-size: 0.75rem;
    outline: none;
  }

  .field-input:focus,
  .field-select:focus {
    border-color: var(--accent);
  }

  .list-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.55rem 0.75rem;
    border-radius: 0.7rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
  }

  .settings-list-box {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
  }

  @media (min-width: 768px) and (min-height: 620px) {
    .settings-list-box {
      max-height: 15rem;
      overflow-y: auto;
      overscroll-behavior-y: contain;
    }
  }

  .empty-box {
    border-radius: 0.75rem;
    border: 1px dashed var(--border);
    padding: 1.25rem 1rem;
    text-align: center;
    font-size: 0.75rem;
    color: var(--text-muted);
  }

  @media (max-height: 540px) {
    .settings-shell {
      padding-top: 0.55rem;
      padding-bottom: 1rem;
      gap: 0.65rem;
    }

    .top-tab {
      padding: 0.34rem 0.55rem;
      font-size: 0.72rem;
    }

    .bento-card {
      padding: 0.75rem 0.85rem;
      gap: 0.6rem;
    }

    .hub-row {
      padding: 0.55rem 0.85rem;
    }
  }
</style>
