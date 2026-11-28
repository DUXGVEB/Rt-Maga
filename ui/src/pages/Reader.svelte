<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    ArrowLeft,
    Bookmark,
    Check,
    CheckCircle2,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    Download,
    ExternalLink,
    Hand,
    Image as ImageIcon,
    List,
    Loader2,
    Maximize,
    Minimize,
    PanelLeft,
    PanelRight,
    RotateCcw,
    Search,
    SlidersHorizontal,
    Smartphone,
    Sparkles,
    X,
    ZoomOut,
  } from "lucide-svelte";
  import { flushSync, tick, untrack } from "svelte";
  import { api, formatAppError, imgProxyUrl } from "../lib/api";
  import BubbleMagnifierCard from "../lib/components/BubbleMagnifierCard.svelte";
  import FloatingControls from "../lib/components/FloatingControls.svelte";
  import LatticeLoader from "../lib/components/LatticeLoader.svelte";
  import PageImage from "../lib/components/PageImage.svelte";
  import ReaderAiPill from "../lib/components/ReaderAiPill.svelte";
  import SourceDiagnosticCard from "../lib/components/SourceDiagnosticCard.svelte";
  import SquishSwitch from "../lib/components/SquishSwitch.svelte";
  import {
    backdropFade,
    badgePop,
    collapseReveal,
    createOverlayGuard,
    drawerSlide,
    popoverPop,
    viewReveal,
  } from "../lib/motion";
  import {
    clampZoomPan,
    computeAnchoredZoom,
    computeFocusedZoom,
    computeHorizontalTrackMetrics,
    computeNavStepCooldownMs,
    computePageLoadMode,
    computeSeamlessTrackTranslateX,
    computeVerticalSpeedMultiplier,
    detectWebtoonFromDimensions,
    focusBrightness,
    partitionPages,
    resolveHorizontalTapAction,
    type PageDimensionSample,
    type RectBox,
  } from "../lib/readerLayout";
  import { aiTranslateStore } from "../lib/stores/aiTranslate.svelte";
  import { deviceEnv } from "../lib/stores/deviceEnv.svelte";
  import { libraryCache } from "../lib/stores/libraryCache.svelte";
  import { router } from "../lib/stores/router.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import type {
    ChapterView,
    FloatingEdge,
    FloatingScreenKey,
    FormattedAppError,
    HorizontalDirection,
    MangaView,
    OneHandedMode,
    OrientationLockMode,
    ReaderBg,
    ReaderModeOverride,
    ReaderSettings,
    ReadingOrientation,
    SettingsSection,
  } from "../lib/types";

  let {
    mangaId,
    chapterUrl,
    initialScrollRatio = 0,
    initialPageIndex = 0,
  }: {
    mangaId: number;
    chapterUrl: string;
    initialScrollRatio?: number;
    initialPageIndex?: number;
  } = $props();

  let manga = $state<MangaView | null>(null);
  let strip = $state<ChapterView[]>([]);
  let activeUrl = $state("");
  let loadingInitial = $state(true);
  let appendingNext = $state(false);
  let verifyingBlocked = $state(false);
  let loggingIn = $state(false);
  let errorState = $state<FormattedAppError | null>(null);
  let toolbarVisible = $state(true);
  let quickSettingsOpen = $state(false);
  let openQuickCat = $state<"layout" | "display" | "controls" | null>(null);
  let quickFloatConfig = $state<"scrollToTop" | "orientationLock" | null>(null);
  let chapterJumpOpen = $state(false);
  let chapterJumpQuery = $state("");
  let imageMenuTarget = $state<{
    token: string;
    pageIndex: number;
    chapterUrl: string;
    clientX: number;
    clientY: number;
    isContextMenu: boolean;
  } | null>(null);
  let imageActionBusy = $state<"download" | "cover" | null>(null);
  const LANDSCAPE_AUTO_SPREAD_KEY = "rtmaga.reader.landscapeAutoSpread.v1";
  let landscapeAutoSpread = $state(
    typeof window !== "undefined" &&
      window.localStorage?.getItem(LANDSCAPE_AUTO_SPREAD_KEY) === "1"
  );
  const popoverGuard = createOverlayGuard(160);
  const isCompactMobile = $derived(deviceEnv.layoutTier === "compact");

  function closeReaderOverlays() {
    if (!popoverGuard.canAct()) return;
    popoverGuard.lock(140);
    chapterJumpOpen = false;
    quickSettingsOpen = false;
    openQuickCat = null;
    quickFloatConfig = null;
    aiPillOpen = false;
    activeBubbleCallout = null;
    activeBubbleKey = null;
    imageMenuTarget = null;
  }

  function setLandscapeAutoSpread(next: boolean) {
    landscapeAutoSpread = next;
    try {
      window.localStorage?.setItem(LANDSCAPE_AUTO_SPREAD_KEY, next ? "1" : "0");
    } catch {
      // Ignore restricted storage environments.
    }
  }
  let isFullscreen = $state(false);
  let showScrollTopBtn = $state(false);
  let controlsDimmed = $state(false);
  let chapterDetectedLayout = $state<"manga" | "webtoon" | null>(null);
  let aiPillOpen = $state(false);
  let activeBubbleKey = $state<string | null>(null);
  let activeBubbleCallout = $state<{
    token: string;
    regionIndex: number;
    imgEl: HTMLImageElement | null;
  } | null>(null);
  const activeBubbleCalloutRegions = $derived(
    activeBubbleCallout ? (aiTranslateStore.getPageResult(activeBubbleCallout.token)?.regions ?? []) : []
  );
  let aiPageDispatchTimer: number | null = null;
  let aiScrollSettling = $state(false);
  let aiScrollSettleTimer: number | null = null;
  let lastAiFocusKey = "";

  function notifyAiScrollActivity() {
    if (!aiBubbleZoom && !aiTranslateOn) return;
    aiScrollSettling = true;
    if (aiScrollSettleTimer !== null) {
      window.clearTimeout(aiScrollSettleTimer);
    }
    aiScrollSettleTimer = window.setTimeout(() => {
      aiScrollSettleTimer = null;
      aiScrollSettling = false;
    }, 360);
  }

  function measureActivePageFocusY(pageIdx: number): number {
    if (isHorizontal || !scrollEl || !activeUrl) return 450;
    const pageEl = scrollEl.querySelector<HTMLElement>(
      `[data-page-key="${CSS.escape(`${activeUrl}:${pageIdx}`)}"]`
    );
    if (!pageEl) return 450;
    const vpRect = scrollEl.getBoundingClientRect();
    const pRect = pageEl.getBoundingClientRect();
    if (pRect.height < 16) return 450;
    const vpMidY = vpRect.top + vpRect.height * 0.48;
    const ratio = (vpMidY - pRect.top) / pRect.height;
    return Math.max(0, Math.min(1000, Math.round(ratio * 1000)));
  }

  // Double-tap / double-click focused zoom & 2-finger pinch-to-zoom state (`1.0` .. `3.0`).
  const ZOOM_TARGET_SCALE = 2.0;
  const ZOOM_MAX_SCALE = 3.0;
  const ZOOM_RESET_THRESHOLD = 1.05;
  const ZOOM_ANIM_MS = 230;
  let zoomScale = $state(1);
  let zoomTx = $state(0);
  let zoomTy = $state(0);
  let zoomPanning = $state(false);
  let zoomPinching = $state(false);
  let zoomSuppressTransition = $state(false);
  let zoomContentRect: RectBox | null = null;
  let zoomOriginLeft = 0;
  let zoomOriginTop = 0;
  let zoomTransitionLockUntil = 0;
  let verticalZoomExtraBottomPx = $state(0);
  let pendingVerticalZoomCommit: {
    targetScale: number;
    targetScrollTop: number;
    extraBottomPx: number;
  } | null = null;
  let verticalZoomCommitTimer: number | null = null;

  // Isolated Horizontal RTL center-focus peek track & snap state.
  let activeGroupIdx = $state(0);
  let horizontalAtEnd = $state(false);
  let horizontalVpWidth = $state(typeof window !== "undefined" ? window.innerWidth : 1280);
  let horizontalVpHeight = $state(typeof window !== "undefined" ? window.innerHeight : 800);
  let horizontalDragging = $state(false);
  let horizontalDragOffsetPx = $state(0);
  let suppressTrackTransition = $state(false);
  let seamlessTrackEl = $state<HTMLDivElement | null>(null);
  let seamlessActiveLeft = $state<number | null>(null);
  let seamlessActiveWidth = $state(0);
  let horizontalDragDidMove = false;
  let pinchPointers = new Map<number, { x: number; y: number }>();
  let pinchSource: "none" | "touch" | "pointer" = "none";
  let pinchStartDist = 0;
  let pinchStartScale = 1;
  let pinchStartTx = 0;
  let pinchStartTy = 0;
  let pinchAnchorContentX = 0;
  let pinchAnchorDocY = 0;
  let pinchBaseDocHeight = 0;

  // Viewport focus tracking.
  let focusedKeys = $state<Set<string>>(new Set());
  let focusedPageIdx = $state(0);

  let scrollEl = $state<HTMLDivElement | null>(null);
  let verticalContentEl = $state<HTMLDivElement | null>(null);
  let horizontalStageEl = $state<HTMLDivElement | null>(null);
  let lastScrollOffset = 0;
  let prefetched = new Map<string, ChapterView>();
  let preloadedImageTokens = new Set<string>();
  let purgedChapterUrls = new Set<string>();
  let saveTimer: number | null = null;
  let mangaSettingsSaveTimer: number | null = null;
  let pendingMangaSettings: { id: number; settings: ReaderSettings } | null = null;
  let idleDimTimer: number | null = null;
  let toolbarAutoHideTimer: number | null = null;
  let singleTapTimer: number | null = null;
  let orientationRestoreTimer: number | null = null;
  let zoomFocusSettleTimer: number | null = null;
  let verticalAnimRaf: number | null = null;
  let verticalAnimTargetTop = 0;
  let lastTapAt = 0;
  let lastTapX = 0;
  let lastTapY = 0;
  let lastHorizontalStepAt = 0;
  let wheelDeltaAccum = 0;
  let wheelResetTimer: number | null = null;
  let pointerDownX = 0;
  let pointerDownY = 0;
  let pointerDownAt = 0;
  let activePointerId: number | null = null;
  let activeViewportPointers = new Set<number>();
  let longPressTimer: number | null = null;
  let longPressPointerId: number | null = null;
  let longPressStartX = 0;
  let longPressStartY = 0;
  let suppressNextClickUntil = 0;
  let panStartX = 0;
  let panStartY = 0;
  let panOriginTx = 0;
  let panOriginTy = 0;
  let panOriginScrollTop = 0;
  let didPanDrag = false;
  let lastViewportW = typeof window !== "undefined" ? window.innerWidth : 0;
  let lastViewportH = typeof window !== "undefined" ? window.innerHeight : 0;
  let wakeLockSentinel: { release: () => Promise<void> } | null = null;

  let pendingProgress: {
    mangaId: number;
    url: string;
    name: string;
    ratio: number;
    pageIndex: number;
  } | null = null;
  let restoreTimer: number | null = null;
  let focusRaf: number | null = null;
  let markedReadSet = new Set<string>();
  let dimensionSamples = new Map<number, PageDimensionSample>();
  let webtoonDecidedForChapter = false;
  let restoredInitialScroll = false;
  let isUserScrollingBeforeRestore = false;
  let suppressAutoHideUntil = 0;
  let scrubbing = false;
  let closingReader = false;
  let loadSeq = 0;

  function normUrl(u: string): string {
    return u.endsWith("/") ? u.slice(0, -1) : u;
  }

  function cloneReaderSettings(rs: ReaderSettings): ReaderSettings {
    return {
      ...rs,
      width: { ...rs.width },
      vertical: { ...rs.vertical },
      horizontal: { ...rs.horizontal },
      behavior: { ...rs.behavior },
    };
  }

  function flushPendingMangaSettings() {
    if (mangaSettingsSaveTimer !== null) {
      window.clearTimeout(mangaSettingsSaveTimer);
      mangaSettingsSaveTimer = null;
    }
    if (!pendingMangaSettings) return;
    const snap = pendingMangaSettings;
    pendingMangaSettings = null;
    void api.setMangaReaderSettings(snap.id, snap.settings).catch(() => {});
  }

  function updateMangaReaderSettings(
    mutator: (draft: ReaderSettings) => void,
    debounceMs = 0
  ) {
    const base = cloneReaderSettings(manga?.readerSettings ?? settingsStore.value.reader);
    if (!manga?.readerSettings) {
      if (isAutoWebtoonActive) {
        base.orientation = "vertical";
        base.vertical.noGapMode = true;
      } else if (isAutoMangaActive) {
        // Auto-detected manga also defaults to vertical scroll while preserving user page spacing.
        base.orientation = "vertical";
      }
    }
    mutator(base);
    if (manga) {
      manga = {
        ...manga,
        readerSettings: base,
        readerModeOverride: null,
      };
    }
    pendingMangaSettings = { id: mangaId, settings: base };
    if (mangaSettingsSaveTimer !== null) {
      window.clearTimeout(mangaSettingsSaveTimer);
      mangaSettingsSaveTimer = null;
    }
    if (debounceMs > 0) {
      mangaSettingsSaveTimer = window.setTimeout(() => {
        flushPendingMangaSettings();
      }, debounceMs);
    } else {
      flushPendingMangaSettings();
    }
  }

  async function resetMangaReaderSettingsToGlobal() {
    if (mangaSettingsSaveTimer !== null) {
      window.clearTimeout(mangaSettingsSaveTimer);
      mangaSettingsSaveTimer = null;
    }
    pendingMangaSettings = null;
    if (manga) {
      manga = { ...manga, readerSettings: null, readerModeOverride: null };
    }
    try {
      await api.setMangaReaderSettings(mangaId, null);
      toasts.push(t.toastMangaSettingsReset, "info", 2200);
    } catch {
      // Ignore error.
    }
  }

  function purgeFinishedChapterCache(url: string) {
    const norm = normUrl(url);
    if (!norm || purgedChapterUrls.has(norm)) return;
    purgedChapterUrls.add(norm);
    void api.purgeChapterPageCache(mangaId, url).catch(() => {});
  }

  const hasCustomMangaSettings = $derived(Boolean(manga?.readerSettings));
  const readerSettings = $derived<ReaderSettings>(
    manga?.readerSettings ?? settingsStore.value.reader
  );
  const vCfg = $derived(readerSettings.vertical);
  const hCfg = $derived(readerSettings.horizontal);
  const behavior = $derived(readerSettings.behavior);
  const a11y = $derived(settingsStore.value.accessibility);
  const oneHandedMode = $derived(deviceEnv.isMobile ? a11y.oneHandedMode : "default");
  const aiBubbleZoom = $derived(a11y.bubbleAutoZoom);
  const aiTranslateOn = $derived(a11y.aiTranslationOverlay);
  const aiPeekOriginal = $derived(aiTranslateStore.peekOriginal);
  const t = $derived(settingsStore.t);

  const effectiveOverride = $derived<ReaderModeOverride>(
    hasCustomMangaSettings ? null : (manga?.readerModeOverride ?? null)
  );

  const effectiveDetectedLayout = $derived.by<"manga" | "webtoon" | null>(() => {
    if (hasCustomMangaSettings || effectiveOverride !== null || !readerSettings.autoWebtoonDetect) {
      return null;
    }
    if (chapterDetectedLayout) return chapterDetectedLayout;
    if (manga?.detectedLayout === "webtoon" || manga?.detectedLayout === "manga") {
      return manga.detectedLayout;
    }
    if (manga?.detectedWebtoon) return "webtoon";
    return null;
  });

  const isAutoWebtoonActive = $derived(effectiveDetectedLayout === "webtoon");
  const isAutoMangaActive = $derived(effectiveDetectedLayout === "manga");

  const effectiveOrientation = $derived<ReadingOrientation>(
    effectiveOverride === "horizontal"
      ? "horizontal"
      : effectiveOverride === "vertical_strip" || effectiveOverride === "vertical_grouped"
        ? "vertical"
        : isAutoWebtoonActive || isAutoMangaActive
          ? "vertical"
          : readerSettings.orientation
  );

  const effectiveNoGap = $derived(isAutoWebtoonActive || vCfg.noGapMode);

  const effectiveVerticalGapPx = $derived(effectiveNoGap ? 0 : vCfg.pageGapPx);

  const isHorizontal = $derived(effectiveOrientation === "horizontal");
  const effectiveHorizontalDirection = $derived<HorizontalDirection>(hCfg.direction);
  const isRtl = $derived(isHorizontal && effectiveHorizontalDirection === "rtl");

  const activeChapter = $derived(
    strip.find((c) => normUrl(c.url) === normUrl(activeUrl)) ?? strip[strip.length - 1] ?? null
  );

  const lastChapter = $derived(strip[strip.length - 1] ?? null);
  const totalPages = $derived(activeChapter?.images.length ?? 0);
  const effectivePagesPerGroup = $derived(
    isHorizontal && landscapeAutoSpread && deviceEnv.isLandscape && hCfg.pagesPerGroup === 1
      ? 2
      : Math.max(1, hCfg.pagesPerGroup)
  );
  const horizontalGroups = $derived(
    activeChapter ? partitionPages(activeChapter.images, effectivePagesPerGroup) : []
  );
  const currentHorizontalGroup = $derived(
    !horizontalAtEnd && horizontalGroups.length > 0
      ? horizontalGroups[Math.max(0, Math.min(horizontalGroups.length - 1, activeGroupIdx))] ?? null
      : null
  );
  const horizontalEndSlideIdx = $derived(horizontalGroups.length);
  const activeHorizontalSlideIdx = $derived(
    horizontalAtEnd
      ? horizontalEndSlideIdx
      : Math.max(0, Math.min(Math.max(0, horizontalGroups.length - 1), activeGroupIdx))
  );
  const horizontalTrackMetrics = $derived(
    computeHorizontalTrackMetrics(
      horizontalVpWidth,
      horizontalVpHeight,
      effectivePagesPerGroup,
      hCfg.groupGapPx || 18
    )
  );
  const horizontalDirSign = $derived(isRtl ? 1 : -1);
  const isHorizontalSeamless = $derived(isHorizontal && hCfg.noGapMode);
  const horizontalTrackTranslateX = $derived(
    horizontalDirSign * activeHorizontalSlideIdx * horizontalTrackMetrics.stride +
      horizontalDragOffsetPx
  );
  const horizontalTrackStyle = $derived(
    `transform: translate3d(${horizontalTrackTranslateX}px, 0, 0); transition: ${
      horizontalDragging || suppressTrackTransition
        ? "none"
        : "transform 320ms cubic-bezier(0.22, 1, 0.36, 1)"
    }; will-change: transform;`
  );
  const seamlessTrackTranslateX = $derived(
    seamlessActiveLeft !== null
      ? computeSeamlessTrackTranslateX(
          horizontalVpWidth,
          seamlessActiveLeft,
          seamlessActiveWidth,
          horizontalDragOffsetPx
        )
      : horizontalDragOffsetPx
  );
  const seamlessTrackStyle = $derived(
    `transform: translate3d(${seamlessTrackTranslateX}px, 0, 0); transition: ${
      horizontalDragging || suppressTrackTransition
        ? "none"
        : "transform 320ms cubic-bezier(0.22, 1, 0.36, 1)"
    }; will-change: transform;`
  );
  const visibleHorizontalSlides = $derived.by(() => {
    if (!isHorizontal || isHorizontalSeamless || !activeChapter) return [];
    const endIdx = horizontalEndSlideIdx;
    const minIdx = Math.max(0, activeHorizontalSlideIdx - 2);
    const maxIdx = Math.min(endIdx, activeHorizontalSlideIdx + 2);
    const slides: {
      slideIdx: number;
      isActive: boolean;
      isEndCard: boolean;
      group: (typeof horizontalGroups)[number] | null;
      leftCss: string;
    }[] = [];
    const halfSlot = horizontalTrackMetrics.slotWidth / 2;
    const stride = horizontalTrackMetrics.stride;
    for (let idx = minIdx; idx <= maxIdx; idx += 1) {
      const offsetPx = idx * stride;
      slides.push({
        slideIdx: idx,
        isActive: idx === activeHorizontalSlideIdx,
        isEndCard: idx === endIdx,
        group: idx < horizontalGroups.length ? horizontalGroups[idx] : null,
        leftCss: isRtl
          ? `calc(50% - ${halfSlot}px - ${offsetPx}px)`
          : `calc(50% - ${halfSlot}px + ${offsetPx}px)`,
      });
    }
    return slides;
  });

  function recalcSeamlessCenter(instant = false) {
    if (!isHorizontalSeamless || !seamlessTrackEl) return;
    const activeEl = seamlessTrackEl.querySelector<HTMLElement>(
      `[data-hslide-idx="${activeHorizontalSlideIdx}"]`
    );
    if (!activeEl) return;
    if (instant) {
      suppressTrackTransition = true;
      seamlessActiveLeft = activeEl.offsetLeft;
      seamlessActiveWidth = activeEl.offsetWidth;
      window.requestAnimationFrame(() => {
        suppressTrackTransition = false;
      });
    } else {
      seamlessActiveLeft = activeEl.offsetLeft;
      seamlessActiveWidth = activeEl.offsetWidth;
    }
  }

  function syncHorizontalViewportSize(instant = true) {
    if (!scrollEl) return;
    const w = scrollEl.clientWidth || window.innerWidth;
    const h = scrollEl.clientHeight || window.innerHeight;
    if (w > 0) horizontalVpWidth = w;
    if (h > 0) horizontalVpHeight = h;
    if (isHorizontalSeamless) {
      recalcSeamlessCenter(instant);
      return;
    }
    if (instant) {
      suppressTrackTransition = true;
      window.requestAnimationFrame(() => {
        suppressTrackTransition = false;
      });
    }
  }

  function hideHorizontalHud() {
    toolbarVisible = false;
    quickSettingsOpen = false;
    openQuickCat = null;
    quickFloatConfig = null;
    chapterJumpOpen = false;
    aiPillOpen = false;
  }

  const filteredJumpChapters = $derived(() => {
    if (!manga) return [];
    const q = chapterJumpQuery.trim().toLowerCase();
    const list = q
      ? manga.chapters.filter(
          (c) =>
            c.name.toLowerCase().includes(q) ||
            (c.number !== null && String(c.number).includes(q))
        )
      : manga.chapters;
    return [...list].reverse();
  });

  const readerBackdrop = $derived(() => {
    const bg: ReaderBg = readerSettings.readerBg;
    if (bg === "dark") return "#0b0b0d";
    if (bg === "light") return "#f7f7f8";
    return "var(--bg)";
  });

  const zoomTransitionCss = $derived(
    zoomPanning || zoomPinching || zoomSuppressTransition
      ? "none"
      : `transform ${ZOOM_ANIM_MS}ms cubic-bezier(0.22, 1, 0.36, 1)`
  );

  const horizontalZoomStyle = $derived(
    zoomScale > 1.005 || Math.abs(zoomTx) > 0.5 || Math.abs(zoomTy) > 0.5
      ? `transform: translate3d(${zoomTx}px, ${zoomTy}px, 0) scale(${zoomScale}); backface-visibility: hidden; transform-origin: 0 0; will-change: transform; transition: ${zoomTransitionCss};`
      : `transform: translate3d(0px, 0px, 0px) scale(1); backface-visibility: hidden; transform-origin: 0 0; transition: ${zoomTransitionCss};`
  );

  const verticalZoomStyle = $derived(
    zoomScale > 1.005 || Math.abs(zoomTx) > 0.5 || Math.abs(zoomTy) > 0.5
      ? `transform: translate3d(${zoomTx}px, ${zoomTy}px, 0) scale(${zoomScale}); backface-visibility: hidden; transform-origin: 0 0; will-change: transform; transition: ${zoomTransitionCss};`
      : `transform: translate3d(0px, 0px, 0px) scale(1); backface-visibility: hidden; transform-origin: 0 0; transition: ${zoomTransitionCss};`
  );

  function clearPendingVerticalZoomCommit() {
    if (verticalZoomCommitTimer !== null) {
      window.clearTimeout(verticalZoomCommitTimer);
      verticalZoomCommitTimer = null;
    }
    pendingVerticalZoomCommit = null;
  }

  /**
   * Synchronously commits a finished (or interrupted) GPU vertical zoom animation into
   * real `scrollTop` + `verticalZoomExtraBottomPx` with `zoomTy = 0` so there is zero jump.
   */
  function commitPendingVerticalZoom() {
    const pending = pendingVerticalZoomCommit;
    clearPendingVerticalZoomCommit();
    if (!pending || !scrollEl || isHorizontal) return;
    flushSync(() => {
      zoomSuppressTransition = true;
      verticalZoomExtraBottomPx = pending.extraBottomPx;
      zoomScale = pending.targetScale;
      zoomTy = 0;
      if (pending.targetScale <= 1.01) {
        zoomTx = 0;
        zoomContentRect = null;
        zoomOriginLeft = 0;
        zoomOriginTop = 0;
      }
    });
    const maxTop = Math.max(0, scrollEl.scrollHeight - scrollEl.clientHeight);
    const clampedTop = Math.max(0, Math.min(maxTop, pending.targetScrollTop));
    scrollEl.scrollTop = clampedTop;
    lastScrollOffset = clampedTop;
    window.requestAnimationFrame(() => {
      zoomSuppressTransition = false;
    });
  }

  /**
   * Smoothly animates vertical zoom (`1x <-> Nx` or bubble-to-bubble) purely on the GPU via
   * `translate3d(tx, st0 - st1, 0) scale(targetScale)` while keeping `scrollTop` steady during
   * the 230ms transition, then seamlessly commits `scrollTop = st1` with `zoomTy = 0`.
   */
  function animateVerticalZoomTo(
    targetScale: number,
    targetTx: number,
    desiredScrollTop: number,
    baseDocHeight: number
  ) {
    if (!scrollEl) return;
    commitPendingVerticalZoom();

    const st0 = scrollEl.scrollTop;
    const nextExtraBottom =
      targetScale > 1.01 ? Math.max(0, Math.round(baseDocHeight * (targetScale - 1))) : 0;
    const estimatedScrollHeight = Math.max(
      scrollEl.clientHeight,
      baseDocHeight + 112 + nextExtraBottom
    );
    const targetMaxTop = Math.max(0, estimatedScrollHeight - scrollEl.clientHeight);
    const st1 = Math.max(0, Math.min(targetMaxTop, Math.round(desiredScrollTop)));

    // Visually, shifting by `ty = st0 - st1` while keeping `scrollTop = st0` places every
    // pixel at the exact same viewport coordinate as `scrollTop = st1` with `ty = 0`.
    const visualTy = st0 - st1;

    zoomTransitionLockUntil = Date.now() + ZOOM_ANIM_MS + 40;
    suppressAutoHideUntil = Date.now() + ZOOM_ANIM_MS + 120;
    if (zoomFocusSettleTimer !== null) {
      window.clearTimeout(zoomFocusSettleTimer);
      zoomFocusSettleTimer = null;
    }

    pendingVerticalZoomCommit = {
      targetScale,
      targetScrollTop: st1,
      extraBottomPx: nextExtraBottom,
    };

    zoomSuppressTransition = false;
    zoomScale = targetScale;
    zoomTx = targetScale > 1.01 ? targetTx : 0;
    zoomTy = visualTy;

    verticalZoomCommitTimer = window.setTimeout(() => {
      verticalZoomCommitTimer = null;
      commitPendingVerticalZoom();
      zoomTransitionLockUntil = 0;
      updateFocusNow();
    }, ZOOM_ANIM_MS + 20);
  }

  function cancelVerticalSmoothScroll() {
    if (verticalAnimRaf !== null) {
      window.cancelAnimationFrame(verticalAnimRaf);
      verticalAnimRaf = null;
    }
  }

  function animateVerticalScrollBy(deltaPx: number) {
    if (!scrollEl || isHorizontal) return;
    const maxTop = Math.max(0, scrollEl.scrollHeight - scrollEl.clientHeight);
    const startBase = verticalAnimRaf !== null ? verticalAnimTargetTop : scrollEl.scrollTop;
    verticalAnimTargetTop = Math.max(0, Math.min(maxTop, startBase + deltaPx));

    if (verticalAnimRaf !== null) return;

    const step = () => {
      if (!scrollEl || isHorizontal) {
        verticalAnimRaf = null;
        return;
      }
      const speedMult = computeVerticalSpeedMultiplier(behavior.navigationSpeed);
      const factor = Math.min(0.45, Math.max(0.14, 0.2 * speedMult));
      const diff = verticalAnimTargetTop - scrollEl.scrollTop;
      if (Math.abs(diff) <= 1.2) {
        scrollEl.scrollTop = verticalAnimTargetTop;
        verticalAnimRaf = null;
        return;
      }
      scrollEl.scrollTop += diff * factor;
      verticalAnimRaf = window.requestAnimationFrame(step);
    };
    verticalAnimRaf = window.requestAnimationFrame(step);
  }

  function resetZoom(anchorClientY?: number, immediate = false) {
    commitPendingVerticalZoom();
    const wasZoomed = zoomScale > 1.01;
    const prevScale = zoomScale;
    activeBubbleCallout = null;
    activeBubbleKey = null;
    zoomPanning = false;
    zoomPinching = false;
    pinchPointers.clear();
    pinchSource = "none";
    if (zoomFocusSettleTimer !== null) {
      window.clearTimeout(zoomFocusSettleTimer);
      zoomFocusSettleTimer = null;
    }

    if (wasZoomed && !isHorizontal && scrollEl) {
      const vpRect = scrollEl.getBoundingClientRect();
      const vy = anchorClientY !== undefined ? anchorClientY - vpRect.top : vpRect.height * 0.5;
      const unscaledDocY = (scrollEl.scrollTop + vy) / Math.max(1, prevScale);
      const targetTop = Math.max(0, Math.round(unscaledDocY - vy));
      const baseHeight = verticalContentEl?.offsetHeight ?? scrollEl.scrollHeight;

      if (!immediate) {
        animateVerticalZoomTo(1, 0, targetTop, baseHeight);
        return;
      }

      zoomTransitionLockUntil = Date.now() + 180;
      suppressAutoHideUntil = Date.now() + 260;
      flushSync(() => {
        zoomSuppressTransition = true;
        zoomScale = 1;
        zoomTx = 0;
        zoomTy = 0;
        zoomContentRect = null;
        zoomOriginLeft = 0;
        zoomOriginTop = 0;
        verticalZoomExtraBottomPx = 0;
      });
      if (scrollEl) {
        const maxTop = Math.max(0, scrollEl.scrollHeight - scrollEl.clientHeight);
        const clampedTop = Math.max(0, Math.min(maxTop, targetTop));
        scrollEl.scrollTop = clampedTop;
        lastScrollOffset = clampedTop;
      }
      window.requestAnimationFrame(() => {
        zoomSuppressTransition = false;
      });
      zoomFocusSettleTimer = window.setTimeout(() => {
        zoomFocusSettleTimer = null;
        zoomTransitionLockUntil = 0;
        updateFocusNow();
      }, 140);
    } else {
      zoomScale = 1;
      zoomTx = 0;
      zoomTy = 0;
      zoomContentRect = null;
      zoomOriginLeft = 0;
      zoomOriginTop = 0;
      verticalZoomExtraBottomPx = 0;
      zoomTransitionLockUntil = 0;
    }
  }

  function preloadHorizontalNearbyImages() {
    if (!isHorizontal || !activeChapter) return;
    const windowSize = Math.max(5, Math.min(8, readerSettings.preloadImages || 6));
    const startIdx = Math.max(0, focusedPageIdx - 1);
    const endIdx = Math.min(activeChapter.images.length - 1, focusedPageIdx + windowSize);
    for (let i = startIdx; i <= endIdx; i += 1) {
      const token = activeChapter.images[i];
      if (!token || preloadedImageTokens.has(token)) continue;
      preloadedImageTokens.add(token);
      if (preloadedImageTokens.size > 80) {
        const oldest = preloadedImageTokens.values().next().value;
        if (oldest) preloadedImageTokens.delete(oldest);
      }
      const img = new Image();
      img.decoding = "async";
      img.src = imgProxyUrl(token, false);
    }
  }

  $effect(() => {
    void isHorizontal;
    void activeChapter?.url;
    void focusedPageIdx;
    preloadHorizontalNearbyImages();
  });

  function scheduleToolbarAutoHide() {
    if (toolbarAutoHideTimer !== null) {
      window.clearTimeout(toolbarAutoHideTimer);
      toolbarAutoHideTimer = null;
    }
    const secs = behavior.toolbarAutoHideSecs;
    if (
      !toolbarVisible ||
      secs <= 0 ||
      quickSettingsOpen ||
      quickFloatConfig !== null ||
      chapterJumpOpen ||
      aiPillOpen ||
      scrubbing
    ) {
      return;
    }
    toolbarAutoHideTimer = window.setTimeout(() => {
      toolbarAutoHideTimer = null;
      if (
        !quickSettingsOpen &&
        quickFloatConfig === null &&
        !chapterJumpOpen &&
        !aiPillOpen &&
        !scrubbing &&
        Date.now() >= suppressAutoHideUntil
      ) {
        toolbarVisible = false;
      }
    }, secs * 1000);
  }

  function wakeFloatingControls() {
    controlsDimmed = false;
    if (idleDimTimer !== null) {
      window.clearTimeout(idleDimTimer);
    }
    if (settingsStore.value.floating.autoDimInReader) {
      idleDimTimer = window.setTimeout(() => {
        idleDimTimer = null;
        if (!quickSettingsOpen && quickFloatConfig === null && !chapterJumpOpen && !aiPillOpen) {
          controlsDimmed = true;
        }
      }, 2400);
    }
  }

  async function applyKeepScreenOn(enabled: boolean) {
    if (deviceEnv.isAndroid) {
      void api.setKeepScreenOn(enabled).catch(() => {});
      return;
    }
    // Desktop / WebView2 Screen Wake Lock API (best-effort, zero errors if unsupported).
    const nav = navigator as unknown as {
      wakeLock?: { request: (t: "screen") => Promise<{ release: () => Promise<void> }> };
    };
    try {
      if (enabled) {
        if (!wakeLockSentinel && nav.wakeLock) {
          wakeLockSentinel = await nav.wakeLock.request("screen");
        }
      } else if (wakeLockSentinel) {
        await wakeLockSentinel.release();
        wakeLockSentinel = null;
      }
    } catch {
      // Ignore unsupported WakeLock environments.
    }
  }

  $effect(() => {
    void applyKeepScreenOn(behavior.keepScreenOn);
  });

  $effect(() => {
    void toolbarVisible;
    void quickSettingsOpen;
    void quickFloatConfig;
    void chapterJumpOpen;
    void aiPillOpen;
    void behavior.toolbarAutoHideSecs;
    scheduleToolbarAutoHide();
  });

  function isKeyFocused(key: string): boolean {
    if (focusedKeys.size === 0) return true;
    return focusedKeys.has(key);
  }

  function pageDimBrightness(pageKey: string): number {
    if (isHorizontal) return 1;
    return focusBrightness(isKeyFocused(pageKey), vCfg.dimUnfocused, vCfg.dimLevel);
  }

  function handlePageImageLoaded(
    chapUrl: string,
    info: { index: number; width: number; height: number }
  ) {
    if (isHorizontalSeamless) {
      window.requestAnimationFrame(() => recalcSeamlessCenter(true));
    }
    if (normUrl(chapUrl) !== normUrl(activeUrl)) return;

    if (
      !webtoonDecidedForChapter &&
      !hasCustomMangaSettings &&
      readerSettings.autoWebtoonDetect &&
      effectiveOverride === null &&
      info.index < 6
    ) {
      dimensionSamples.set(info.index, info);
      if (dimensionSamples.size >= 2) {
        const result = detectWebtoonFromDimensions(Array.from(dimensionSamples.values()));
        if (result.decided && result.detectedLayout) {
          webtoonDecidedForChapter = true;
          if (result.detectedLayout === "webtoon" && effectiveDetectedLayout !== "webtoon") {
            chapterDetectedLayout = "webtoon";
            if (manga) {
              manga = { ...manga, detectedWebtoon: true, detectedLayout: "webtoon" };
            }
            void api.markMangaDetectedWebtoon(mangaId).catch(() => {});
            toasts.push(t.toastWebtoonDetected, "info", 2600);
          } else if (result.detectedLayout === "manga" && !manga?.detectedLayout && !manga?.detectedWebtoon) {
            chapterDetectedLayout = "manga";
            if (manga) {
              manga = { ...manga, detectedWebtoon: false, detectedLayout: "manga" };
            }
            toasts.push(t.a11yToastMangaDetected, "info", 2200);
          }
        }
      }
    }
  }

  function updateFocusNow() {
    if (strip.length === 0 || loadingInitial) return;
    if (Date.now() < zoomTransitionLockUntil) return;

    if (isHorizontal) {
      const grp = currentHorizontalGroup;
      if (grp && grp.items.length > 0 && activeChapter) {
        focusedPageIdx = grp.items[0].pageIdx;
        focusedKeys = new Set([`${activeChapter.url}:hg:${grp.groupIdx}`]);
      }
      return;
    }

    if (!scrollEl) return;
    const scrollRect = scrollEl.getBoundingClientRect();
    const nextFocused = new Set<string>();
    const centerY = scrollRect.top + scrollRect.height * 0.45;
    const pages = Array.from(scrollEl.querySelectorAll<HTMLElement>("[data-page-key]"));
    let best: HTMLElement | null = null;
    let bestDist = Number.POSITIVE_INFINITY;
    let bestInActive: HTMLElement | null = null;
    let bestInActiveDist = Number.POSITIVE_INFINITY;
    const normActive = normUrl(activeUrl);

    for (const el of pages) {
      const r = el.getBoundingClientRect();
      if (r.top <= centerY && r.bottom >= centerY && el.dataset.pageKey) {
        nextFocused.add(el.dataset.pageKey);
      }
      const elCenter = r.top + r.height / 2;
      const dist = Math.abs(elCenter - centerY);
      if (dist < bestDist) {
        bestDist = dist;
        best = el;
      }
      const secUrl = el.closest<HTMLElement>("[data-chapter-url]")?.dataset.chapterUrl;
      if (secUrl && normUrl(secUrl) === normActive && dist < bestInActiveDist) {
        bestInActiveDist = dist;
        bestInActive = el;
      }
    }
    if (best?.dataset.pageKey) {
      nextFocused.add(best.dataset.pageKey);
    }
    const pageForIdx = bestInActive ?? best;
    if (pageForIdx?.dataset.pageIdx) {
      const rawIdx = Number(pageForIdx.dataset.pageIdx) || 0;
      focusedPageIdx =
        totalPages > 0 ? Math.max(0, Math.min(totalPages - 1, rawIdx)) : Math.max(0, rawIdx);
    }

    if (
      nextFocused.size !== focusedKeys.size ||
      Array.from(nextFocused).some((k) => !focusedKeys.has(k))
    ) {
      focusedKeys = nextFocused;
    }
  }

  function scheduleFocusUpdate() {
    if (focusRaf !== null) return;
    focusRaf = window.requestAnimationFrame(() => {
      focusRaf = null;
      updateFocusNow();
    });
  }

  function flushPendingProgress(keepSessionActive = !closingReader): Promise<void> {
    if (saveTimer !== null) {
      window.clearTimeout(saveTimer);
      saveTimer = null;
    }
    if (pendingProgress) {
      const snap = pendingProgress;
      pendingProgress = null;
      if (keepSessionActive) {
        const mode = isHorizontal ? "horizontal" : "vertical";
        void api
          .upsertReaderSession(snap.mangaId, snap.url, snap.name, snap.ratio, snap.pageIndex, mode)
          .catch(() => {});
      }
      return api
        .saveProgress(snap.mangaId, snap.url, snap.name, snap.ratio, snap.pageIndex)
        .catch(() => {});
    }
    return Promise.resolve();
  }

  function captureCurrentProgressNow(keepSessionActive = !closingReader) {
    if (!activeChapter || loadingInitial) return;
    let ratio = 0;
    if (isHorizontal) {
      const totalGroups = Math.max(1, horizontalGroups.length);
      ratio = horizontalAtEnd
        ? 1
        : totalGroups <= 1
          ? 1
          : Math.min(1, Math.max(0, activeGroupIdx / (totalGroups - 1)));
    } else if (scrollEl) {
      const maxScroll = Math.max(1, scrollEl.scrollHeight - scrollEl.clientHeight);
      ratio = Math.min(1, Math.max(0, scrollEl.scrollTop / maxScroll));
    }
    pendingProgress = {
      mangaId,
      url: activeChapter.url,
      name: activeChapter.name,
      ratio,
      pageIndex: focusedPageIdx,
    };
    void flushPendingProgress(keepSessionActive);
  }

  function clearPendingTimers(flush = false, keepSessionActive = !closingReader) {
    cancelVerticalSmoothScroll();
    clearPendingVerticalZoomCommit();
    if (flush) {
      void flushPendingProgress(keepSessionActive);
      flushPendingMangaSettings();
    } else {
      if (saveTimer !== null) {
        window.clearTimeout(saveTimer);
        saveTimer = null;
      }
      pendingProgress = null;
      if (mangaSettingsSaveTimer !== null) {
        window.clearTimeout(mangaSettingsSaveTimer);
        mangaSettingsSaveTimer = null;
      }
    }
    for (const timer of [
      restoreTimer,
      idleDimTimer,
      toolbarAutoHideTimer,
      singleTapTimer,
      orientationRestoreTimer,
      wheelResetTimer,
      zoomFocusSettleTimer,
      longPressTimer,
      aiPageDispatchTimer,
      aiScrollSettleTimer,
    ]) {
      if (timer !== null) window.clearTimeout(timer);
    }
    restoreTimer = null;
    idleDimTimer = null;
    toolbarAutoHideTimer = null;
    singleTapTimer = null;
    orientationRestoreTimer = null;
    wheelResetTimer = null;
    zoomFocusSettleTimer = null;
    longPressTimer = null;
    longPressPointerId = null;
    aiPageDispatchTimer = null;
    aiScrollSettleTimer = null;
    aiScrollSettling = false;
    if (focusRaf !== null) {
      window.cancelAnimationFrame(focusRaf);
      focusRaf = null;
    }
  }

  function markChapterReadLocalAndRemote(url: string) {
    const norm = normUrl(url);
    if (markedReadSet.has(norm)) return;
    markedReadSet.add(norm);
    if (manga) {
      const match = manga.chapters.find((c) => normUrl(c.url) === norm);
      if (match) match.read = true;
    }
    void api.markRead(mangaId, url).catch(() => {});
  }

  function scrollToStartOfContainer(smooth = false) {
    cancelVerticalSmoothScroll();
    if (isHorizontal) {
      resetZoom();
      horizontalDragging = false;
      horizontalDragOffsetPx = 0;
      horizontalAtEnd = false;
      activeGroupIdx = 0;
      focusedPageIdx = 0;
      showScrollTopBtn = false;
      lastScrollOffset = 0;
      syncHorizontalViewportSize(true);
      captureCurrentProgressNow();
      return;
    }
    if (!scrollEl) return;
    const behaviorScroll: ScrollBehavior = smooth ? "smooth" : "instant";
    scrollEl.scrollTo({ top: 0, behavior: behaviorScroll });
    lastScrollOffset = 0;
    showScrollTopBtn = false;
  }

  function seekToPage(pageIndex: number) {
    if (!activeChapter) return;
    const clamped = Math.max(0, Math.min(Math.max(0, activeChapter.images.length - 1), pageIndex));
    focusedPageIdx = clamped;
    suppressAutoHideUntil = Date.now() + 450;
    resetZoom();

    if (isHorizontal) {
      horizontalAtEnd = false;
      const perGroup = effectivePagesPerGroup;
      activeGroupIdx = Math.max(
        0,
        Math.min(Math.max(0, horizontalGroups.length - 1), Math.floor(clamped / perGroup))
      );
      showScrollTopBtn = activeGroupIdx > 0;
      updateFocusNow();
      captureCurrentProgressNow();
      return;
    }

    if (!scrollEl) return;
    const pageEl = scrollEl.querySelector<HTMLElement>(
      `[data-page-key="${CSS.escape(`${activeChapter.url}:${clamped}`)}"]`
    );
    if (pageEl) {
      pageEl.scrollIntoView({
        behavior: "instant",
        block: "start",
        inline: "center",
      });
      lastScrollOffset = scrollEl.scrollTop;
      updateFocusNow();
      captureCurrentProgressNow();
    }
  }

  function applyScrollRestore(ratio: number, pageIdx: number, targetChapUrl: string) {
    if (isHorizontal) {
      horizontalAtEnd = false;
      const totalImgs = activeChapter?.images.length ?? 0;
      const perGroup = effectivePagesPerGroup;
      const totalGroups = Math.max(1, horizontalGroups.length);
      let targetGroup = 0;
      if (pageIdx > 0 && totalImgs > 0) {
        const clampedPage = Math.max(0, Math.min(totalImgs - 1, pageIdx));
        targetGroup = Math.floor(clampedPage / perGroup);
        focusedPageIdx = clampedPage;
      } else if (ratio > 0.01 && totalGroups > 1) {
        targetGroup = Math.round((totalGroups - 1) * Math.max(0, Math.min(1, ratio)));
        focusedPageIdx = horizontalGroups[targetGroup]?.items[0]?.pageIdx ?? 0;
      }
      activeGroupIdx = Math.max(0, Math.min(totalGroups - 1, targetGroup));
      showScrollTopBtn = activeGroupIdx > 0;
      return;
    }

    if (!scrollEl) return;
    if (pageIdx > 0) {
      const pageEl = scrollEl.querySelector<HTMLElement>(
        `[data-page-key="${CSS.escape(`${targetChapUrl}:${pageIdx}`)}"]`
      );
      if (pageEl) {
        pageEl.scrollIntoView({ behavior: "instant", block: "start", inline: "center" });
        lastScrollOffset = scrollEl.scrollTop;
        focusedPageIdx = pageIdx;
        return;
      }
    }
    const maxY = Math.max(0, scrollEl.scrollHeight - scrollEl.clientHeight);
    const target = Math.round(maxY * ratio);
    scrollEl.scrollTop = target;
    lastScrollOffset = target;
  }

  async function loadInitial(targetUrl: string, scrollRatio: number, pageIndex: number) {
    const seq = ++loadSeq;
    closingReader = false;
    clearPendingTimers(true);
    resetZoom();
    loadingInitial = true;
    appendingNext = false;
    errorState = null;
    strip = [];
    focusedKeys = new Set();
    focusedPageIdx = Math.max(0, pageIndex);
    activeGroupIdx = 0;
    horizontalAtEnd = false;
    preloadedImageTokens.clear();
    prefetched.clear();
    purgedChapterUrls.clear();
    markedReadSet.clear();
    dimensionSamples.clear();
    webtoonDecidedForChapter = false;
    chapterDetectedLayout = null;
    activeBubbleKey = null;
    lastAiFocusKey = "";
    restoredInitialScroll = false;
    isUserScrollingBeforeRestore = false;
    showScrollTopBtn = false;
    lastScrollOffset = 0;

    scrollToStartOfContainer();

    try {
      const [mangaData, chapterData] = await Promise.all([
        api.getManga(mangaId).catch(() => null),
        api.getChapter(targetUrl, mangaId),
      ]);
      if (seq !== loadSeq) return;

      manga = mangaData;
      strip = [chapterData];
      activeUrl = chapterData.url;
      loadingInitial = false;

      if (deviceEnv.isAndroid) {
        void deviceEnv.setOrientationLock(readerSettings.orientationLock);
      }

      await tick();
      if (seq !== loadSeq) return;

      if ((pageIndex > 0 || scrollRatio > 0.02) && scrollEl) {
        applyScrollRestore(scrollRatio, pageIndex, chapterData.url);
        restoreTimer = window.setTimeout(() => {
          restoreTimer = null;
          if (seq === loadSeq && scrollEl && !isUserScrollingBeforeRestore) {
            applyScrollRestore(scrollRatio, pageIndex, chapterData.url);
          }
          restoredInitialScroll = true;
          updateFocusNow();
        }, 260);
      } else {
        scrollToStartOfContainer();
        restoredInitialScroll = true;
        updateFocusNow();
        window.requestAnimationFrame(() => {
          if (seq === loadSeq && scrollEl && scrollRatio <= 0.02 && lastScrollOffset === 0) {
            scrollToStartOfContainer();
            updateFocusNow();
          }
        });
      }

      const mode = isHorizontal ? "horizontal" : "vertical";
      void api
        .saveProgress(mangaId, chapterData.url, chapterData.name, scrollRatio, Math.max(0, pageIndex))
        .catch(() => {});
      void api
        .upsertReaderSession(
          mangaId,
          chapterData.url,
          chapterData.name,
          scrollRatio,
          Math.max(0, pageIndex),
          mode
        )
        .catch(() => {});

      if (behavior.autoMarkReadOnOpen || chapterData.images.length <= 2) {
        markChapterReadLocalAndRemote(chapterData.url);
      }

      // Non-blocking Rust-assisted Webtoon vs. Manga classification (metadata + header-only imagesize).
      // Manual per-manga overrides always take precedence and skip auto-switching.
      if (
        !hasCustomMangaSettings &&
        effectiveOverride === null &&
        readerSettings.autoWebtoonDetect &&
        !mangaData?.detectedLayout &&
        !mangaData?.detectedWebtoon &&
        chapterData.images.length > 0
      ) {
        const probeTokens = chapterData.images.slice(0, 6);
        void api
          .detectChapterLayout(mangaId, probeTokens)
          .then((verdict) => {
            if (seq !== loadSeq || !verdict.decided) return;
            if (verdict.layout === "manual_override") {
              webtoonDecidedForChapter = true;
              return;
            }
            webtoonDecidedForChapter = true;
            if (
              (verdict.layout === "webtoon" || verdict.isWebtoon) &&
              !hasCustomMangaSettings &&
              effectiveOverride === null &&
              effectiveDetectedLayout !== "webtoon"
            ) {
              chapterDetectedLayout = "webtoon";
              if (manga) {
                manga = { ...manga, detectedWebtoon: true, detectedLayout: "webtoon" };
              }
              toasts.push(t.toastWebtoonDetected, "info", 2600);
            } else if (
              verdict.layout === "manga" &&
              !hasCustomMangaSettings &&
              effectiveOverride === null &&
              effectiveDetectedLayout !== "manga"
            ) {
              chapterDetectedLayout = "manga";
              if (manga) {
                manga = { ...manga, detectedWebtoon: false, detectedLayout: "manga" };
              }
              toasts.push(t.a11yToastMangaDetected, "info", 2200);
            }
          })
          .catch(() => {
            // Non-fatal; natural image onload fallback remains active.
          });
      }

      wakeFloatingControls();
      scheduleToolbarAutoHide();
    } catch (err) {
      if (seq !== loadSeq) return;
      errorState = formatAppError(err, settingsStore.value.language);
      loadingInitial = false;
    }
  }

  $effect(() => {
    if (!deviceEnv.isAndroid) {
      void getCurrentWindow()
        .isFullscreen()
        .then((fs) => {
          isFullscreen = fs;
        })
        .catch(() => {});
    }

    const unregisterBack = router.registerBackInterceptor(() => {
      if (activeBubbleCallout !== null) {
        activeBubbleCallout = null;
        activeBubbleKey = null;
        return true;
      }
      if (imageMenuTarget !== null) {
        if (!imageActionBusy) imageMenuTarget = null;
        return true;
      }
      if (aiPillOpen) {
        aiPillOpen = false;
        return true;
      }
      if (chapterJumpOpen) {
        chapterJumpOpen = false;
        return true;
      }
      if (quickFloatConfig !== null) {
        quickFloatConfig = null;
        return true;
      }
      if (quickSettingsOpen) {
        quickSettingsOpen = false;
        openQuickCat = null;
        return true;
      }
      if (zoomScale > 1.01) {
        resetZoom();
        return true;
      }
      return false;
    });

    const onVisibilityChange = () => {
      if (document.visibilityState === "hidden") {
        captureCurrentProgressNow();
        void applyKeepScreenOn(false);
      } else {
        void applyKeepScreenOn(behavior.keepScreenOn);
        if (deviceEnv.isAndroid) {
          void deviceEnv.reassertImmersive();
        }
      }
    };

    const reclampActiveZoomAfterResize = () => {
      if (!scrollEl || zoomScale <= 1.01) return;
      const vpRect = scrollEl.getBoundingClientRect();
      if (vpRect.width <= 0 || vpRect.height <= 0) return;
      const contentEl = isHorizontal ? horizontalStageEl : verticalContentEl;
      const rect: RectBox = contentEl
        ? {
            left: 0,
            top: 0,
            width: contentEl.clientWidth || vpRect.width,
            height: contentEl.clientHeight || vpRect.height,
          }
        : { left: 0, top: 0, width: vpRect.width, height: vpRect.height };
      zoomContentRect = rect;
      const clamped = clampZoomPan(
        zoomTx,
        isHorizontal ? zoomTy : 0,
        zoomScale,
        vpRect.width,
        vpRect.height,
        rect,
        zoomOriginLeft,
        zoomOriginTop
      );
      zoomTx = clamped.tx;
      zoomTy = isHorizontal ? clamped.ty : 0;
    };

    const scheduleViewportAnchorRestore = (delayMs = 180) => {
      if (!scrollEl || loadingInitial || strip.length === 0) return;
      suppressAutoHideUntil = Date.now() + 650;
      captureCurrentProgressNow();

      const anchorChap = activeUrl;
      const anchorPage = focusedPageIdx;
      const wasHorizontal = isHorizontal;
      let intraPageRatio = 0.25;
      if (!wasHorizontal && anchorChap) {
        const curEl = scrollEl.querySelector<HTMLElement>(
          `[data-page-key="${CSS.escape(`${anchorChap}:${anchorPage}`)}"]`
        );
        if (curEl) {
          const sRect = scrollEl.getBoundingClientRect();
          const pRect = curEl.getBoundingClientRect();
          const focusY = sRect.top + sRect.height * 0.45;
          intraPageRatio = Math.max(0, Math.min(1, (focusY - pRect.top) / Math.max(1, pRect.height)));
        }
      }

      if (orientationRestoreTimer !== null) {
        window.clearTimeout(orientationRestoreTimer);
        orientationRestoreTimer = null;
      }

      const restoreNow = () => {
        if (!scrollEl || !anchorChap) return;
        if (isHorizontal) {
          scrollEl.scrollLeft = 0;
          if (!horizontalAtEnd) {
            const perGroup = effectivePagesPerGroup;
            activeGroupIdx = Math.max(
              0,
              Math.min(Math.max(0, horizontalGroups.length - 1), Math.floor(anchorPage / perGroup))
            );
          }
          syncHorizontalViewportSize(true);
          reclampActiveZoomAfterResize();
          updateFocusNow();
          return;
        }
        const pageEl = scrollEl.querySelector<HTMLElement>(
          `[data-page-key="${CSS.escape(`${anchorChap}:${anchorPage}`)}"]`
        );
        if (pageEl) {
          const sRect = scrollEl.getBoundingClientRect();
          const pRect = pageEl.getBoundingClientRect();
          const pageTopInScroll = scrollEl.scrollTop + (pRect.top - sRect.top);
          const targetTop = Math.max(
            0,
            Math.round(pageTopInScroll + intraPageRatio * pRect.height - scrollEl.clientHeight * 0.45)
          );
          scrollEl.scrollTop = targetTop;
          lastScrollOffset = targetTop;
        }
        reclampActiveZoomAfterResize();
        updateFocusNow();
      };

      orientationRestoreTimer = window.setTimeout(() => {
        orientationRestoreTimer = null;
        restoreNow();
      }, delayMs);
    };

    const onOrientationChanged = () => {
      // Strictly preserve current HUD visibility, page index, chapter, and zoom anchor on rotate.
      lastViewportW = window.innerWidth;
      lastViewportH = window.innerHeight;
      scheduleViewportAnchorRestore(200);
    };

    const onWindowResize = () => {
      const w = window.innerWidth;
      const h = window.innerHeight;
      if (Math.abs(w - lastViewportW) < 20 && Math.abs(h - lastViewportH) < 20) return;
      lastViewportW = w;
      lastViewportH = h;
      scheduleViewportAnchorRestore(140);
    };

    document.addEventListener("visibilitychange", onVisibilityChange);
    window.addEventListener("rtmaga:orientation-changed", onOrientationChanged);
    window.addEventListener("resize", onWindowResize);

    return () => {
      closingReader = true;
      captureCurrentProgressNow(false);
      clearPendingTimers(true, false);
      void aiTranslateStore.disableAndPurge();
      void api.clearReaderSession().catch(() => {});
      void api.purgeMangaPageCache(mangaId).catch(() => {});
      unregisterBack();
      document.removeEventListener("visibilitychange", onVisibilityChange);
      window.removeEventListener("rtmaga:orientation-changed", onOrientationChanged);
      window.removeEventListener("resize", onWindowResize);
      void applyKeepScreenOn(false);
      if (deviceEnv.isAndroid) {
        void deviceEnv.setOrientationLock("auto");
      }
    };
  });

  // Non-passive TouchEvent listener for reliable 2-finger pinch-to-zoom on Android WebView
  // and touchscreens in BOTH Vertical and Horizontal reading modes.
  // Never calls `preventDefault()` for 1-finger touches so native vertical scroll stays 100% smooth.
  $effect(() => {
    const el = scrollEl;
    if (!el) return;

    const onTouchStart = (e: TouchEvent) => {
      wakeFloatingControls();
      if (e.touches.length < 2) return;
      if (
        loadingInitial ||
        errorState ||
        quickSettingsOpen ||
        chapterJumpOpen ||
        quickFloatConfig !== null ||
        imageMenuTarget !== null ||
        aiPillOpen
      ) {
        return;
      }
      e.preventDefault();
      const t0 = e.touches[0];
      const t1 = e.touches[1];
      beginPinchZoom("touch", t0.clientX, t0.clientY, t1.clientX, t1.clientY, e.target);
    };

    const onTouchMove = (e: TouchEvent) => {
      if (pinchSource !== "touch") return;
      if (e.touches.length < 2) return;
      e.preventDefault();
      const t0 = e.touches[0];
      const t1 = e.touches[1];
      updatePinchZoom(t0.clientX, t0.clientY, t1.clientX, t1.clientY);
    };

    const onTouchEnd = (e: TouchEvent) => {
      if (pinchSource !== "touch") return;
      if (e.touches.length >= 2) {
        // 3rd finger lifted while 2 fingers remain: seamlessly re-anchor to remaining pair.
        const t0 = e.touches[0];
        const t1 = e.touches[1];
        beginPinchZoom("touch", t0.clientX, t0.clientY, t1.clientX, t1.clientY, e.target);
        return;
      }
      const remaining = e.touches.length === 1 ? e.touches[0] : null;
      endPinchZoom(remaining ? { x: remaining.clientX, y: remaining.clientY } : null);
    };

    el.addEventListener("touchstart", onTouchStart, { passive: false });
    el.addEventListener("touchmove", onTouchMove, { passive: false });
    el.addEventListener("touchend", onTouchEnd, { passive: true });
    el.addEventListener("touchcancel", onTouchEnd, { passive: true });
    return () => {
      el.removeEventListener("touchstart", onTouchStart);
      el.removeEventListener("touchmove", onTouchMove);
      el.removeEventListener("touchend", onTouchEnd);
      el.removeEventListener("touchcancel", onTouchEnd);
    };
  });

  $effect(() => {
    const _id = mangaId;
    const url = chapterUrl;
    const ratio = untrack(() => initialScrollRatio);
    const page = untrack(() => initialPageIndex);
    untrack(() => {
      void _id;
      void loadInitial(url, ratio, page);
    });
    return () => {
      loadSeq += 1;
      clearPendingTimers(true, !closingReader);
    };
  });

  // Reactive progressive AI driver: pauses while actively scrolling so 60/120fps reading stays
  // completely fluid, translates ONLY the visible page first (prioritizing bubbles closest to the
  // viewport center), and gently warms page +1 only after the active page finishes while idle.
  $effect(() => {
    const enabled = aiBubbleZoom || aiTranslateOn;
    const translateMode = aiTranslateOn;
    const chap = activeChapter;
    const pageIdx = focusedPageIdx;
    const horiz = isHorizontal;
    const grp = currentHorizontalGroup;
    const tier = aiTranslateStore.activeTier;
    const lang = aiTranslateStore.effectiveTargetLang;
    const loading = loadingInitial;
    const scrolling = aiScrollSettling;
    const gen = aiTranslateStore.generation;
    void gen;

    if (aiPageDispatchTimer !== null) {
      window.clearTimeout(aiPageDispatchTimer);
      aiPageDispatchTimer = null;
    }

    if (!enabled) {
      activeBubbleCallout = null;
      activeBubbleKey = null;
      lastAiFocusKey = "";
      return;
    }
    if (loading || scrolling || !chap || chap.images.length === 0) return;

    const activeTokens =
      horiz && grp && grp.items.length > 0
        ? grp.items.map((it) => it.token)
        : chap.images[pageIdx]
          ? [chap.images[pageIdx]]
          : [];
    if (activeTokens.length === 0) return;

    const warmNextToken =
      !horiz && pageIdx + 1 < chap.images.length ? chap.images[pageIdx + 1] : null;

    const focusKey = `${chap.url}|${activeTokens.join(",")}|${translateMode ? "t" : "d"}|${tier}|${lang}|${gen}`;
    if (focusKey === lastAiFocusKey) return;

    const movedPage = lastAiFocusKey !== "";
    lastAiFocusKey = focusKey;

    if (movedPage) {
      aiTranslateStore.cancelJobs();
    }

    const focusY = measureActivePageFocusY(pageIdx);
    aiPageDispatchTimer = window.setTimeout(() => {
      aiPageDispatchTimer = null;
      if (aiScrollSettling || lastAiFocusKey !== focusKey) return;
      const runTasks = activeTokens.map((tok) =>
        aiTranslateStore.processPage(tok, { detectOnly: !translateMode, focusY })
      );
      if (warmNextToken && !activeTokens.includes(warmNextToken)) {
        void Promise.all(runTasks).then(() => {
          if (
            lastAiFocusKey !== focusKey ||
            aiScrollSettling ||
            aiTranslateStore.isPageDone(warmNextToken, !translateMode)
          ) {
            return;
          }
          void aiTranslateStore.processPage(warmNextToken, {
            detectOnly: !translateMode,
            focusY: 180,
          });
        });
      }
    }, movedPage ? 220 : 80);
  });

  let prevLayoutSig = "";
  let prevLayoutOrient = "";
  $effect(() => {
    const orient = effectiveOrientation;
    const perGroup = effectivePagesPerGroup;
    const sig = `${orient}|${perGroup}|${effectiveHorizontalDirection}|${isHorizontalSeamless}`;
    if (!prevLayoutSig) {
      prevLayoutSig = sig;
      prevLayoutOrient = orient;
      return;
    }
    if (sig === prevLayoutSig) return;
    const orientationSwitched = prevLayoutOrient !== orient;
    prevLayoutSig = sig;
    prevLayoutOrient = orient;

    const anchorChap = untrack(() => activeUrl);
    const anchorPage = untrack(() => focusedPageIdx);
    if (!anchorChap || untrack(() => loadingInitial || strip.length === 0)) return;

    void (async () => {
      if (orientationSwitched) {
        resetZoom();
      }
      if (untrack(() => isHorizontal && strip.length > 1)) {
        const current = untrack(() => strip.find((c) => normUrl(c.url) === normUrl(anchorChap)));
        if (current) {
          strip = [current];
        }
      }
      await tick();
      if (isHorizontal) {
        horizontalAtEnd = false;
        activeGroupIdx = Math.max(
          0,
          Math.min(Math.max(0, horizontalGroups.length - 1), Math.floor(anchorPage / perGroup))
        );
        syncHorizontalViewportSize(true);
        updateFocusNow();
        return;
      }
      if (!scrollEl) return;
      const pageEl = scrollEl.querySelector<HTMLElement>(
        `[data-page-key="${CSS.escape(`${anchorChap}:${anchorPage}`)}"]`
      );
      if (pageEl) {
        pageEl.scrollIntoView({ behavior: "instant", block: "center", inline: "center" });
        lastScrollOffset = scrollEl.scrollTop;
      }
      updateFocusNow();
    })();
  });

  $effect(() => {
    const el = scrollEl;
    if (!el || typeof ResizeObserver === "undefined") return;
    if (isHorizontal) {
      syncHorizontalViewportSize(true);
    }
    let lastW = el.clientWidth;
    let lastH = el.clientHeight;
    let roDebounceTimer: number | null = null;
    const ro = new ResizeObserver((entries) => {
      const box = entries[0]?.contentRect;
      const nextW = Math.round(box?.width ?? el.clientWidth);
      const nextH = Math.round(box?.height ?? el.clientHeight);
      if (isHorizontal) {
        syncHorizontalViewportSize(true);
      }
      scheduleFocusUpdate();
      if (Math.abs(nextW - lastW) < 12 && Math.abs(nextH - lastH) < 12) return;
      const anchorChap = untrack(() => activeUrl);
      const anchorPage = untrack(() => focusedPageIdx);
      const savedRatio = (() => {
        if (isHorizontal || !anchorChap) return 0.25;
        const curEl = el.querySelector<HTMLElement>(
          `[data-page-key="${CSS.escape(`${anchorChap}:${anchorPage}`)}"]`
        );
        if (!curEl) return 0.25;
        const sRect = el.getBoundingClientRect();
        const pRect = curEl.getBoundingClientRect();
        const focusY = sRect.top + sRect.height * 0.45;
        return Math.max(0, Math.min(1, (focusY - pRect.top) / Math.max(1, pRect.height)));
      })();
      lastW = nextW;
      lastH = nextH;
      if (roDebounceTimer !== null) window.clearTimeout(roDebounceTimer);
      roDebounceTimer = window.setTimeout(() => {
        roDebounceTimer = null;
        if (!scrollEl || loadingInitial || strip.length === 0 || !anchorChap) return;
        if (isHorizontal) {
          syncHorizontalViewportSize(true);
        } else {
          const pageEl = scrollEl.querySelector<HTMLElement>(
            `[data-page-key="${CSS.escape(`${anchorChap}:${anchorPage}`)}"]`
          );
          if (pageEl) {
            const sRect = scrollEl.getBoundingClientRect();
            const pRect = pageEl.getBoundingClientRect();
            const pageTopInScroll = scrollEl.scrollTop + (pRect.top - sRect.top);
            const targetTop = Math.max(
              0,
              Math.round(pageTopInScroll + savedRatio * pRect.height - scrollEl.clientHeight * 0.45)
            );
            scrollEl.scrollTop = targetTop;
            lastScrollOffset = targetTop;
          }
        }
        updateFocusNow();
      }, 120);
    });
    ro.observe(el);
    return () => {
      if (roDebounceTimer !== null) window.clearTimeout(roDebounceTimer);
      ro.disconnect();
    };
  });

  let prevSeamlessChapterUrl = "";
  $effect(() => {
    const seamless = isHorizontalSeamless;
    const chapUrl = activeChapter?.url ?? "";
    const slideIdx = activeHorizontalSlideIdx;
    const dir = hCfg.direction;
    const perGroup = effectivePagesPerGroup;
    void slideIdx;
    void dir;
    void perGroup;
    if (!seamless) {
      seamlessActiveLeft = null;
      prevSeamlessChapterUrl = "";
      return;
    }
    const chapterSwitched = prevSeamlessChapterUrl !== chapUrl;
    prevSeamlessChapterUrl = chapUrl;
    void tick().then(() => {
      recalcSeamlessCenter(chapterSwitched || seamlessActiveLeft === null);
    });
  });

  $effect(() => {
    const track = seamlessTrackEl;
    if (!isHorizontalSeamless || !track || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => {
      recalcSeamlessCenter(true);
    });
    ro.observe(track);
    return () => ro.disconnect();
  });

  async function prefetchNext(nextUrl: string) {
    const normNext = normUrl(nextUrl);
    if (prefetched.has(normNext) || strip.some((c) => normUrl(c.url) === normNext)) return;
    const seq = loadSeq;
    try {
      const data = await api.getChapter(nextUrl, mangaId, "prefetch");
      if (seq === loadSeq) {
        if (prefetched.size >= 3) {
          const oldestKey = prefetched.keys().next().value;
          if (oldestKey) prefetched.delete(oldestKey);
        }
        prefetched.set(normNext, data);
      }
    } catch {
      // Prefetch errors surface only if user transitions.
    }
  }

  async function appendNextChapter() {
    if (isHorizontal) return;
    const tail = lastChapter;
    if (!tail?.next || appendingNext || loadingInitial) return;
    const normNext = normUrl(tail.next);
    if (strip.some((c) => normUrl(c.url) === normNext)) return;

    const seq = loadSeq;
    appendingNext = true;
    try {
      const nextChap =
        prefetched.get(normNext) ?? (await api.getChapter(tail.next, mangaId, "prefetch"));
      if (seq !== loadSeq) return;
      prefetched.delete(normNext);

      if (strip.some((c) => normUrl(c.url) === normUrl(nextChap.url))) {
        return;
      }

      const anchorScrollTop = scrollEl ? scrollEl.scrollTop : 0;
      const shouldTrimOldest =
        strip.length >= 4 && normUrl(strip[0].url) !== normUrl(activeUrl);

      let removedHeight = 0;
      if (shouldTrimOldest && scrollEl) {
        const firstSection = scrollEl.querySelector<HTMLElement>("[data-chapter-url]");
        if (firstSection) {
          removedHeight = firstSection.getBoundingClientRect().height;
        }
      }

      const removedChapUrl = shouldTrimOldest ? strip[0]?.url : null;
      strip = shouldTrimOldest ? [...strip.slice(1), nextChap] : [...strip, nextChap];
      appendingNext = false;
      if (removedChapUrl) {
        purgeFinishedChapterCache(removedChapUrl);
      }

      await tick();
      if (seq === loadSeq && scrollEl) {
        const targetTop = shouldTrimOldest
          ? Math.max(0, Math.round(anchorScrollTop - removedHeight))
          : anchorScrollTop;
        scrollEl.scrollTop = targetTop;
        lastScrollOffset = targetTop;
        window.requestAnimationFrame(() => {
          if (seq === loadSeq && scrollEl && Math.abs(scrollEl.scrollTop - targetTop) > 120) {
            scrollEl.scrollTop = targetTop;
            lastScrollOffset = targetTop;
          }
          updateFocusNow();
        });
      }
    } catch (err) {
      if (seq === loadSeq) {
        toasts.push(formatAppError(err, settingsStore.value.language).title, "error");
      }
    } finally {
      if (seq === loadSeq) {
        appendingNext = false;
      }
    }
  }

  function jumpToChapter(url: string) {
    if (!url) return;
    if (activeChapter && normUrl(activeChapter.url) !== normUrl(url)) {
      purgeFinishedChapterCache(activeChapter.url);
    }
    void flushPendingProgress();
    resetZoom();
    quickSettingsOpen = false;
    chapterJumpOpen = false;
    toolbarVisible = true;
    scrollToStartOfContainer();

    const sameRouteProp = url === chapterUrl && initialScrollRatio === 0 && initialPageIndex === 0;
    router.goReader(mangaId, url, 0, 0, true);
    if (sameRouteProp) {
      void loadInitial(url, 0, 0);
    }
  }

  function stepHorizontalGroup(deltaStep: 1 | -1, enforceCooldown = false) {
    if (!isHorizontal || !activeChapter) return;
    if (enforceCooldown) {
      const now = Date.now();
      const cooldown = computeNavStepCooldownMs(behavior.navigationSpeed);
      if (now - lastHorizontalStepAt < cooldown) return;
      lastHorizontalStepAt = now;
    }

    resetZoom();
    horizontalDragging = false;
    horizontalDragOffsetPx = 0;
    // Automatically hide top & bottom HUD toolbars the instant a horizontal turn is triggered.
    hideHorizontalHud();

    const groups = horizontalGroups;
    if (groups.length === 0) return;

    if (horizontalAtEnd) {
      if (deltaStep < 0) {
        horizontalAtEnd = false;
        activeGroupIdx = groups.length - 1;
        focusedPageIdx = groups[activeGroupIdx]?.items[0]?.pageIdx ?? 0;
        showScrollTopBtn = activeGroupIdx > 0;
        updateFocusNow();
        captureCurrentProgressNow();
      } else if (activeChapter.next) {
        jumpToChapter(activeChapter.next);
      }
      return;
    }

    const nextIdx = activeGroupIdx + deltaStep;
    if (nextIdx >= 0 && nextIdx < groups.length) {
      activeGroupIdx = nextIdx;
      focusedPageIdx = groups[nextIdx]?.items[0]?.pageIdx ?? 0;
      showScrollTopBtn = activeGroupIdx > 0;
      const ratio = groups.length <= 1 ? 1 : activeGroupIdx / (groups.length - 1);
      scheduleSaveProgress(activeChapter.url, activeChapter.name, ratio, focusedPageIdx);
      if (ratio >= 0.85) {
        markChapterReadLocalAndRemote(activeChapter.url);
      }
      if (ratio >= 0.7 && activeChapter.next) {
        void prefetchNext(activeChapter.next);
      }
      updateFocusNow();
      return;
    }

    if (deltaStep > 0) {
      markChapterReadLocalAndRemote(activeChapter.url);
      horizontalAtEnd = true;
      showScrollTopBtn = true;
      captureCurrentProgressNow();
    } else if (deltaStep < 0 && activeChapter.prev) {
      jumpToChapter(activeChapter.prev);
    }
  }

  function measureZoomTargetBox(target: EventTarget | null): {
    rect: RectBox;
    originLeft: number;
    originTop: number;
  } {
    if (!scrollEl) {
      return {
        rect: { left: 0, top: 0, width: window.innerWidth, height: window.innerHeight },
        originLeft: 0,
        originTop: 0,
      };
    }
    const vpRect = scrollEl.getBoundingClientRect();
    if (isHorizontal) {
      const targetEl = target instanceof HTMLElement ? target.closest("img") : null;
      const imgs = targetEl
        ? [targetEl]
        : Array.from(
            horizontalStageEl?.querySelectorAll<HTMLImageElement>(
              '[data-hslide-active="true"] img'
            ) ?? []
          );
      if (imgs.length > 0) {
        let minL = Number.POSITIVE_INFINITY;
        let minT = Number.POSITIVE_INFINITY;
        let maxR = Number.NEGATIVE_INFINITY;
        let maxB = Number.NEGATIVE_INFINITY;
        for (const img of imgs) {
          const r = img.getBoundingClientRect();
          if (r.width > 10 && r.height > 10) {
            minL = Math.min(minL, r.left - vpRect.left);
            minT = Math.min(minT, r.top - vpRect.top);
            maxR = Math.max(maxR, r.right - vpRect.left);
            maxB = Math.max(maxB, r.bottom - vpRect.top);
          }
        }
        if (Number.isFinite(minL) && maxR > minL && maxB > minT) {
          return {
            rect: {
              left: (minL - zoomTx) / Math.max(1, zoomScale),
              top: (minT - zoomTy) / Math.max(1, zoomScale),
              width: (maxR - minL) / Math.max(1, zoomScale),
              height: (maxB - minT) / Math.max(1, zoomScale),
            },
            originLeft: 0,
            originTop: 0,
          };
        }
      }
      return {
        rect: { left: 0, top: 0, width: vpRect.width, height: vpRect.height },
        originLeft: 0,
        originTop: 0,
      };
    }

    if (verticalContentEl) {
      // offsetLeft / offsetWidth are exact unscaled layout coordinates unaffected by CSS transform.
      const originLeft = verticalContentEl.offsetLeft;
      const contentWidth = verticalContentEl.offsetWidth || vpRect.width;
      return {
        rect: {
          left: 0,
          top: 0,
          width: contentWidth,
          height: vpRect.height,
        },
        originLeft,
        originTop: 0,
      };
    }
    return {
      rect: { left: 0, top: 0, width: vpRect.width, height: vpRect.height },
      originLeft: 0,
      originTop: 0,
    };
  }

  function isPointInsideActiveHorizontalImage(clientX: number, clientY: number): boolean {
    if (!horizontalStageEl) return false;
    const imgs = horizontalStageEl.querySelectorAll<HTMLImageElement>(
      '[data-hslide-active="true"] img'
    );
    for (const img of imgs) {
      const r = img.getBoundingClientRect();
      if (
        r.width > 10 &&
        r.height > 10 &&
        clientX >= r.left - 8 &&
        clientX <= r.right + 8 &&
        clientY >= r.top - 8 &&
        clientY <= r.bottom + 8
      ) {
        return true;
      }
    }
    return false;
  }

  function findVerticalPageAtPoint(
    clientY: number,
    target: EventTarget | null
  ): { key: string; idx: number } | null {
    if (target instanceof HTMLElement) {
      const direct = target.closest<HTMLElement>("[data-page-key]");
      if (direct?.dataset.pageKey) {
        return {
          key: direct.dataset.pageKey,
          idx: Number(direct.dataset.pageIdx) || 0,
        };
      }
    }
    if (!scrollEl) return null;
    const pages = Array.from(scrollEl.querySelectorAll<HTMLElement>("[data-page-key]"));
    let best: HTMLElement | null = null;
    let bestDist = Number.POSITIVE_INFINITY;
    for (const el of pages) {
      const r = el.getBoundingClientRect();
      if (clientY >= r.top && clientY <= r.bottom && el.dataset.pageKey) {
        return { key: el.dataset.pageKey, idx: Number(el.dataset.pageIdx) || 0 };
      }
      const dist = Math.abs(r.top + r.height / 2 - clientY);
      if (dist < bestDist) {
        bestDist = dist;
        best = el;
      }
    }
    if (best?.dataset.pageKey) {
      return { key: best.dataset.pageKey, idx: Number(best.dataset.pageIdx) || 0 };
    }
    return null;
  }

  function cancelLongPress() {
    if (longPressTimer !== null) {
      window.clearTimeout(longPressTimer);
      longPressTimer = null;
    }
    longPressPointerId = null;
  }

  function findImageTargetAtPoint(
    clientX: number,
    clientY: number,
    rawTarget: EventTarget | null
  ): { token: string; pageIndex: number; chapterUrl: string } | null {
    const extractFromEl = (
      el: HTMLElement
    ): { token: string; pageIndex: number; chapterUrl: string } | null => {
      const token = el.dataset.pageToken;
      if (!token) return null;
      const pageIndex = Number(el.dataset.pageIdx) || 0;
      const chapEl = el.closest<HTMLElement>("[data-chapter-url]");
      const chapUrl = chapEl?.dataset.chapterUrl ?? activeChapter?.url ?? chapterUrl;
      return { token, pageIndex, chapterUrl: chapUrl };
    };

    if (rawTarget instanceof HTMLElement) {
      const direct = rawTarget.closest<HTMLElement>("[data-page-token]");
      if (direct) {
        const extracted = extractFromEl(direct);
        if (extracted) return extracted;
      }
    }

    if (!scrollEl) return null;
    const selector = isHorizontal
      ? '[data-hslide-active="true"] [data-page-token]'
      : "[data-page-token]";
    const candidates = Array.from(scrollEl.querySelectorAll<HTMLElement>(selector));
    for (const el of candidates) {
      const img = el.querySelector<HTMLImageElement>("img");
      const box = (img && img.offsetWidth > 0 ? img : el).getBoundingClientRect();
      if (
        box.width > 12 &&
        box.height > 12 &&
        clientX >= box.left - 8 &&
        clientX <= box.right + 8 &&
        clientY >= box.top - 8 &&
        clientY <= box.bottom + 8
      ) {
        const extracted = extractFromEl(el);
        if (extracted) return extracted;
      }
    }

    if (isHorizontal && candidates.length === 1) {
      return extractFromEl(candidates[0]);
    }
    return null;
  }

  function handleViewportContextMenu(e: MouseEvent) {
    if (loadingInitial || errorState) return;
    if (e.target instanceof HTMLElement && e.target.closest("button, a, select, input")) {
      return;
    }
    e.preventDefault();
    // On Android touch devices, unified 450ms PointerEvents long-press owns the action sheet;
    // suppressing native WebView contextmenu prevents browser popups and duplicate menus.
    if (deviceEnv.isAndroid) return;

    cancelLongPress();
    if (singleTapTimer !== null) {
      window.clearTimeout(singleTapTimer);
      singleTapTimer = null;
    }
    const hit = findImageTargetAtPoint(e.clientX, e.clientY, e.target);
    if (!hit) {
      imageMenuTarget = null;
      return;
    }
    quickSettingsOpen = false;
    chapterJumpOpen = false;
    quickFloatConfig = null;
    imageMenuTarget = {
      ...hit,
      clientX: e.clientX,
      clientY: e.clientY,
      isContextMenu: true,
    };
  }

  async function handleDownloadImageAction() {
    if (!imageMenuTarget || imageActionBusy) return;
    const target = imageMenuTarget;
    imageActionBusy = "download";
    try {
      const res = await api.downloadReaderImage(
        target.token,
        manga?.title ?? null,
        target.pageIndex
      );
      imageMenuTarget = null;
      toasts.push(t.readerImgSavedToast(res.savedPath), "success", 3200);
    } catch (err) {
      const f = formatAppError(err, settingsStore.value.language);
      toasts.push(f.detail || f.title, "error", 3600);
    } finally {
      imageActionBusy = null;
    }
  }

  async function handleSetCoverAction() {
    if (!imageMenuTarget || imageActionBusy) return;
    const target = imageMenuTarget;
    imageActionBusy = "cover";
    try {
      const res = await api.setMangaCoverFromPage(mangaId, target.token);
      if (manga) {
        manga = { ...manga, coverToken: res.coverToken };
      }
      libraryCache.updateMangaCover(mangaId, res.coverToken);
      libraryCache.invalidate();
      imageMenuTarget = null;
      toasts.push(t.readerImgCoverUpdatedToast, "success", 2600);
    } catch (err) {
      const f = formatAppError(err, settingsStore.value.language);
      toasts.push(f.detail || f.title, "error", 3600);
    } finally {
      imageActionBusy = null;
    }
  }

  function beginPinchZoom(
    source: "touch" | "pointer",
    x1: number,
    y1: number,
    x2: number,
    y2: number,
    target: EventTarget | null
  ) {
    if (!scrollEl) return;
    commitPendingVerticalZoom();
    cancelLongPress();
    if (singleTapTimer !== null) {
      window.clearTimeout(singleTapTimer);
      singleTapTimer = null;
    }
    horizontalDragging = false;
    horizontalDragOffsetPx = 0;
    zoomPanning = false;
    activeBubbleKey = null;

    pinchSource = source;
    zoomPinching = true;
    horizontalDragDidMove = true;
    didPanDrag = true;
    suppressNextClickUntil = Date.now() + 360;

    const vpRect = scrollEl.getBoundingClientRect();
    const midX = (x1 + x2) / 2 - vpRect.left;
    const midY = (y1 + y2) / 2 - vpRect.top;

    const box = measureZoomTargetBox(target);
    zoomContentRect = box.rect;
    zoomOriginLeft = box.originLeft;
    zoomOriginTop = box.originTop;

    pinchStartDist = Math.max(8, Math.hypot(x2 - x1, y2 - y1));
    pinchStartScale = Math.max(1, zoomScale);
    pinchStartTx = zoomTx;
    pinchStartTy = isHorizontal ? zoomTy : 0;
    pinchAnchorContentX = (midX - zoomOriginLeft - pinchStartTx) / pinchStartScale;
    pinchAnchorDocY = isHorizontal
      ? (midY - zoomOriginTop - pinchStartTy) / pinchStartScale
      : (scrollEl.scrollTop + midY) / pinchStartScale;
    pinchBaseDocHeight = verticalContentEl?.offsetHeight ?? scrollEl.scrollHeight;
  }

  function updatePinchZoom(x1: number, y1: number, x2: number, y2: number) {
    if (!scrollEl || !zoomPinching) return;
    const dist = Math.max(8, Math.hypot(x2 - x1, y2 - y1));
    const ratio = dist / Math.max(8, pinchStartDist);
    const nextScale = Math.max(1, Math.min(ZOOM_MAX_SCALE, pinchStartScale * ratio));

    const vpRect = scrollEl.getBoundingClientRect();
    const curMidX = (x1 + x2) / 2 - vpRect.left;
    const curMidY = (y1 + y2) / 2 - vpRect.top;
    const rect = zoomContentRect ?? {
      left: 0,
      top: 0,
      width: vpRect.width,
      height: vpRect.height,
    };

    if (isHorizontal) {
      if (nextScale <= 1.005) {
        zoomScale = 1;
        zoomTx = 0;
        zoomTy = 0;
      } else {
        const rawTx = curMidX - zoomOriginLeft - pinchAnchorContentX * nextScale;
        const rawTy = curMidY - zoomOriginTop - pinchAnchorDocY * nextScale;
        const clamped = clampZoomPan(
          rawTx,
          rawTy,
          nextScale,
          vpRect.width,
          vpRect.height,
          rect,
          zoomOriginLeft,
          zoomOriginTop
        );
        zoomScale = nextScale;
        zoomTx = clamped.tx;
        zoomTy = clamped.ty;
      }
      return;
    }

    // Vertical continuous pinch-to-zoom anchored directly to the 2-finger midpoint.
    const nextExtraBottom =
      nextScale > 1.005 ? Math.max(0, Math.round(pinchBaseDocHeight * (nextScale - 1))) : 0;
    const rawTx = curMidX - zoomOriginLeft - pinchAnchorContentX * nextScale;
    const clamped =
      nextScale <= 1.005
        ? { tx: 0, ty: 0 }
        : clampZoomPan(
            rawTx,
            0,
            nextScale,
            vpRect.width,
            vpRect.height,
            rect,
            zoomOriginLeft,
            zoomOriginTop
          );
    const desiredScrollTop = Math.max(0, Math.round(pinchAnchorDocY * nextScale - curMidY));

    flushSync(() => {
      verticalZoomExtraBottomPx = nextExtraBottom;
      zoomScale = nextScale;
      zoomTx = clamped.tx;
      zoomTy = 0;
    });
    const maxTop = Math.max(0, scrollEl.scrollHeight - scrollEl.clientHeight);
    const clampedTop = Math.max(0, Math.min(maxTop, desiredScrollTop));
    scrollEl.scrollTop = clampedTop;
    lastScrollOffset = clampedTop;
  }

  function endPinchZoom(remainingFinger: { x: number; y: number } | null) {
    if (!zoomPinching) return;
    zoomPinching = false;
    pinchSource = "none";
    suppressNextClickUntil = Date.now() + 280;
    horizontalDragDidMove = false;
    didPanDrag = false;

    if (zoomScale < ZOOM_RESET_THRESHOLD) {
      resetZoom(remainingFinger?.y);
      return;
    }

    if (remainingFinger && scrollEl) {
      // Smoothly re-anchor single-finger pan origin so lifting one finger never jumps.
      panStartX = remainingFinger.x;
      panStartY = remainingFinger.y;
      panOriginTx = zoomTx;
      panOriginTy = zoomTy;
      panOriginScrollTop = scrollEl.scrollTop;
    }
    updateFocusNow();
  }

  function handlePointerDown(e: PointerEvent) {
    if (e.pointerType === "mouse" && e.button !== 0) return;
    if (e.target instanceof HTMLElement && e.target.closest("button, a, select, input")) {
      return;
    }

    if (pendingVerticalZoomCommit) {
      commitPendingVerticalZoom();
    }

    activeViewportPointers.add(e.pointerId);
    pinchPointers.set(e.pointerId, { x: e.clientX, y: e.clientY });

    if (activeViewportPointers.size >= 2 || pinchPointers.size >= 2) {
      // Multi-touch pinch-zoom immediately cancels any pending long-press gesture.
      cancelLongPress();
      if (pinchSource !== "touch" && pinchPointers.size >= 2) {
        const pts = Array.from(pinchPointers.values());
        beginPinchZoom("pointer", pts[0].x, pts[0].y, pts[1].x, pts[1].y, e.target);
      }
      return;
    }

    if (
      e.pointerType !== "mouse" &&
      !loadingInitial &&
      !errorState &&
      !horizontalAtEnd &&
      !quickSettingsOpen &&
      !chapterJumpOpen &&
      quickFloatConfig === null &&
      imageMenuTarget === null
    ) {
      const hit = findImageTargetAtPoint(e.clientX, e.clientY, e.target);
      if (hit) {
        cancelLongPress();
        longPressPointerId = e.pointerId;
        longPressStartX = e.clientX;
        longPressStartY = e.clientY;
        longPressTimer = window.setTimeout(() => {
          longPressTimer = null;
          longPressPointerId = null;
          if (activeViewportPointers.size !== 1 || horizontalDragging || didPanDrag || zoomPinching) {
            return;
          }
          if (singleTapTimer !== null) {
            window.clearTimeout(singleTapTimer);
            singleTapTimer = null;
          }
          suppressNextClickUntil = Date.now() + 650;
          imageMenuTarget = {
            ...hit,
            clientX: longPressStartX,
            clientY: longPressStartY,
            isContextMenu: false,
          };
          if (typeof navigator !== "undefined" && typeof navigator.vibrate === "function") {
            navigator.vibrate(16);
          }
        }, 450);
      }
    }

    if (isHorizontal) {
      horizontalDragDidMove = false;
    }

    pointerDownX = e.clientX;
    pointerDownY = e.clientY;
    pointerDownAt = Date.now();
    didPanDrag = false;
    activePointerId = e.pointerId;
    wakeFloatingControls();

    if (zoomScale > 1.01) {
      zoomPanning = true;
      panStartX = e.clientX;
      panStartY = e.clientY;
      panOriginTx = zoomTx;
      panOriginTy = zoomTy;
      panOriginScrollTop = scrollEl?.scrollTop ?? 0;
      if (scrollEl) {
        try {
          scrollEl.setPointerCapture(e.pointerId);
        } catch {
          // Ignore capture errors on synthetic events.
        }
      }
    }
  }

  function handlePointerMove(e: PointerEvent) {
    if (longPressTimer !== null) {
      if (
        activeViewportPointers.size >= 2 ||
        zoomPinching ||
        (longPressPointerId === e.pointerId &&
          Math.hypot(e.clientX - longPressStartX, e.clientY - longPressStartY) > 8)
      ) {
        cancelLongPress();
      }
    }

    if (pinchPointers.has(e.pointerId)) {
      pinchPointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (pinchSource === "pointer" && pinchPointers.size >= 2) {
        const pts = Array.from(pinchPointers.values());
        updatePinchZoom(pts[0].x, pts[0].y, pts[1].x, pts[1].y);
        return;
      }
    }
    if (zoomPinching) return;

    if (isHorizontal) {
      // Live horizontal RTL drag-swipe with rubber-band edge resistance (only when unzoomed).
      if (!zoomPanning && zoomScale <= 1.01 && activePointerId === e.pointerId) {
        const dx = e.clientX - pointerDownX;
        const dy = e.clientY - pointerDownY;
        if (!horizontalDragging && Math.abs(dx) > 8 && Math.abs(dx) > Math.abs(dy) * 1.1) {
          horizontalDragging = true;
          horizontalDragDidMove = true;
          hideHorizontalHud();
          if (scrollEl) {
            try {
              scrollEl.setPointerCapture(e.pointerId);
            } catch {
              // Ignore capture errors on synthetic events.
            }
          }
        }
        if (horizontalDragging) {
          const atStartBoundary =
            activeHorizontalSlideIdx === 0 && (isRtl ? dx < 0 : dx > 0);
          const atEndBoundary =
            activeHorizontalSlideIdx === horizontalEndSlideIdx && (isRtl ? dx > 0 : dx < 0);
          horizontalDragOffsetPx =
            atStartBoundary || atEndBoundary ? Math.round(dx * 0.28) : Math.round(dx);
        }
        return;
      }
    }

    if (!zoomPanning || activePointerId !== e.pointerId || !scrollEl || zoomScale <= 1.01) return;
    const dx = e.clientX - panStartX;
    const dy = e.clientY - panStartY;
    if (Math.hypot(dx, dy) > 6) {
      didPanDrag = true;
    }
    const vpRect = scrollEl.getBoundingClientRect();
    const rect = zoomContentRect ?? {
      left: 0,
      top: 0,
      width: vpRect.width,
      height: vpRect.height,
    };
    const clamped = clampZoomPan(
      panOriginTx + dx,
      isHorizontal ? panOriginTy + dy : 0,
      zoomScale,
      vpRect.width,
      vpRect.height,
      rect,
      zoomOriginLeft,
      zoomOriginTop
    );
    zoomTx = clamped.tx;
    if (isHorizontal) {
      zoomTy = clamped.ty;
    } else {
      const maxTop = Math.max(0, scrollEl.scrollHeight - scrollEl.clientHeight);
      const nextTop = Math.max(0, Math.min(maxTop, Math.round(panOriginScrollTop - dy)));
      scrollEl.scrollTop = nextTop;
      lastScrollOffset = nextTop;
    }
  }

  function handlePointerUp(e: PointerEvent) {
    activeViewportPointers.delete(e.pointerId);
    pinchPointers.delete(e.pointerId);
    if (longPressPointerId === e.pointerId || activeViewportPointers.size === 0) {
      cancelLongPress();
    }
    if (pinchSource === "pointer" && pinchPointers.size < 2) {
      const remaining = Array.from(pinchPointers.values())[0] ?? null;
      endPinchZoom(remaining);
    }
    if (activePointerId !== e.pointerId) return;
    const dx = e.clientX - pointerDownX;
    const elapsed = Date.now() - pointerDownAt;

    if (zoomPanning) {
      zoomPanning = false;
      if (scrollEl) {
        try {
          scrollEl.releasePointerCapture(e.pointerId);
        } catch {
          // Ignore if capture was not set.
        }
      }
    }

    if (isHorizontal && horizontalDragging) {
      horizontalDragging = false;
      horizontalDragOffsetPx = 0;
      if (scrollEl) {
        try {
          scrollEl.releasePointerCapture(e.pointerId);
        } catch {
          // Ignore if capture was not set.
        }
      }
      activePointerId = null;

      const distThreshold = Math.min(84, horizontalTrackMetrics.stride * 0.18);
      const velocity = Math.abs(dx) / Math.max(1, elapsed);
      if (Math.abs(dx) >= distThreshold || (Math.abs(dx) >= 28 && velocity >= 0.35)) {
        // Dragging RIGHT (`dx > 0`) pulls the left peeking slide into center:
        // in RTL that is Next (+1); in LTR that is Previous (-1).
        const step: 1 | -1 = dx > 0 ? (isRtl ? 1 : -1) : isRtl ? -1 : 1;
        if (step > 0 || activeHorizontalSlideIdx > 0 || activeChapter?.prev) {
          stepHorizontalGroup(step, false);
        }
      }
      return;
    }

    activePointerId = null;
  }

  function dismissHorizontalOverlayIfOpen(): boolean {
    if (activeBubbleCallout !== null) {
      activeBubbleCallout = null;
      activeBubbleKey = null;
      return true;
    }
    if (imageMenuTarget !== null) {
      if (!imageActionBusy) imageMenuTarget = null;
      return true;
    }
    if (aiPillOpen) {
      aiPillOpen = false;
      return true;
    }
    if (quickFloatConfig !== null) {
      quickFloatConfig = null;
      return true;
    }
    if (quickSettingsOpen) {
      quickSettingsOpen = false;
      openQuickCat = null;
      return true;
    }
    if (chapterJumpOpen) {
      chapterJumpOpen = false;
      return true;
    }
    return false;
  }

  function handleBubbleTap(info: {
    token: string;
    regionIndex: number;
    bubbleKey: string;
    imgEl: HTMLImageElement | null;
  }) {
    if (!aiBubbleZoom || !info.token) return;
    wakeFloatingControls();

    // Tapping the same bubble toggles the floating magnifier card closed cleanly.
    if (activeBubbleCallout !== null && activeBubbleKey === info.bubbleKey) {
      activeBubbleCallout = null;
      activeBubbleKey = null;
      return;
    }

    // Open crisp Floating Text/Bubble Callout card WITHOUT zooming or shifting the underlying page.
    activeBubbleKey = info.bubbleKey;
    activeBubbleCallout = {
      token: info.token,
      regionIndex: info.regionIndex,
      imgEl: info.imgEl,
    };
  }

  function hitTestBubbleAtPoint(clientX: number, clientY: number): boolean {
    if (!aiBubbleZoom) return false;
    const scope = isHorizontal ? horizontalStageEl : verticalContentEl;
    if (!scope) return false;
    const selector = isHorizontal
      ? '[data-hslide-active="true"] [data-ai-bubble="true"]'
      : '[data-ai-bubble="true"]';
    const candidates = scope.querySelectorAll<HTMLElement>(selector);
    const padPx = 8;

    let bestEl: HTMLElement | null = null;
    let bestArea = Number.POSITIVE_INFINITY;
    for (const el of candidates) {
      const r = el.getBoundingClientRect();
      if (r.width <= 4 || r.height <= 4) continue;
      if (
        clientX >= r.left - padPx &&
        clientX <= r.right + padPx &&
        clientY >= r.top - padPx &&
        clientY <= r.bottom + padPx
      ) {
        const area = r.width * r.height;
        if (area < bestArea) {
          bestArea = area;
          bestEl = el;
        }
      }
    }
    if (!bestEl) return false;

    const bubbleKey = bestEl.dataset.bubbleKey ?? "";
    const regionIndex = Number(bestEl.dataset.bubbleIdx ?? 0);
    if (!bubbleKey || !Number.isFinite(regionIndex)) {
      return false;
    }
    const pageBox = bestEl.closest<HTMLElement>("[data-page-idx]");
    const imgEl = pageBox?.querySelector<HTMLImageElement>("img") ?? null;
    if (!pageBox) return false;

    handleBubbleTap({
      token: pageBox.dataset.pageToken ?? "",
      regionIndex: Math.max(0, Math.floor(regionIndex)),
      bubbleKey,
      imgEl,
    });
    return true;
  }

  function handleHorizontalZoneTap(zone: "left" | "center" | "right", e: MouseEvent) {
    e.stopPropagation();
    wakeFloatingControls();
    if (Date.now() < suppressNextClickUntil) {
      return;
    }
    if (
      horizontalDragDidMove ||
      Math.hypot(e.clientX - pointerDownX, e.clientY - pointerDownY) > 14
    ) {
      horizontalDragDidMove = false;
      return;
    }
    if (dismissHorizontalOverlayIfOpen()) return;

    const tapX = e.clientX;
    const tapY = e.clientY;

    const runSingleAction = () => {
      if (aiBubbleZoom && hitTestBubbleAtPoint(tapX, tapY)) {
        return;
      }
      if (!readerSettings.tapZonesEnabled) {
        toolbarVisible = !toolbarVisible;
        return;
      }
      if (scrollEl && oneHandedMode !== "default") {
        const rect = scrollEl.getBoundingClientRect();
        const relX = (tapX - rect.left) / Math.max(1, rect.width);
        const relY = (tapY - rect.top) / Math.max(1, rect.height);
        const action = resolveHorizontalTapAction(relX, relY, { isRtl, oneHandedMode });
        if (action === "next") {
          stepHorizontalGroup(1, false);
        } else if (action === "prev") {
          stepHorizontalGroup(-1, false);
        } else {
          toolbarVisible = !toolbarVisible;
        }
        return;
      }
      if (zone === "left") {
        stepHorizontalGroup(isRtl ? 1 : -1, false);
      } else if (zone === "right") {
        stepHorizontalGroup(isRtl ? -1 : 1, false);
      } else {
        toolbarVisible = !toolbarVisible;
      }
    };

    const doubleTapEnabled = behavior.doubleTapAction !== "disabled";
    if (!doubleTapEnabled) {
      runSingleAction();
      return;
    }

    // On side margins outside the actual manga image (PC), flip pages immediately with zero delay.
    // On the image itself (all 3 zones on Mobile & PC), detect double-tap cleanly so users can
    // double-tap any speech bubble or corner to focus-zoom into that exact point.
    const tappedOnImage = zone === "center" || isPointInsideActiveHorizontalImage(tapX, tapY);
    if (!tappedOnImage) {
      lastTapAt = 0;
      if (singleTapTimer !== null) {
        window.clearTimeout(singleTapTimer);
        singleTapTimer = null;
      }
      runSingleAction();
      return;
    }

    const now = Date.now();
    const isDoubleTap =
      now - lastTapAt <= 260 && Math.hypot(tapX - lastTapX, tapY - lastTapY) <= 48;
    if (isDoubleTap) {
      lastTapAt = 0;
      if (singleTapTimer !== null) {
        window.clearTimeout(singleTapTimer);
        singleTapTimer = null;
      }
      executeDoubleTap(tapX, tapY, e.target);
      return;
    }

    lastTapAt = now;
    lastTapX = tapX;
    lastTapY = tapY;
    if (singleTapTimer !== null) {
      window.clearTimeout(singleTapTimer);
    }
    singleTapTimer = window.setTimeout(() => {
      singleTapTimer = null;
      runSingleAction();
    }, 220);
  }

  function handleHorizontalLeftZoneClick(e: MouseEvent) {
    handleHorizontalZoneTap("left", e);
  }

  function handleHorizontalRightZoneClick(e: MouseEvent) {
    handleHorizontalZoneTap("right", e);
  }

  function handleHorizontalCenterZoneClick(e: MouseEvent) {
    handleHorizontalZoneTap("center", e);
  }

  function executeSingleTap(clientX: number, clientY: number) {
    if (
      isHorizontal &&
      !horizontalAtEnd &&
      readerSettings.tapZonesEnabled &&
      scrollEl &&
      zoomScale <= 1.01
    ) {
      const rect = scrollEl.getBoundingClientRect();
      const relX = (clientX - rect.left) / Math.max(1, rect.width);
      const relY = (clientY - rect.top) / Math.max(1, rect.height);
      const action = resolveHorizontalTapAction(relX, relY, { isRtl, oneHandedMode });
      if (action === "next") {
        stepHorizontalGroup(1, false);
        return;
      }
      if (action === "prev") {
        stepHorizontalGroup(-1, false);
        return;
      }
    }
    toolbarVisible = !toolbarVisible;
  }

  function executeDoubleTap(clientX: number, clientY: number, target: EventTarget | null) {
    const action = behavior.doubleTapAction;
    if (action === "disabled") return;
    if (action === "toggle_toolbar") {
      toolbarVisible = !toolbarVisible;
      return;
    }
    if (action === "next_page") {
      if (isHorizontal) {
        stepHorizontalGroup(1, false);
      } else if (scrollEl) {
        const mult = computeVerticalSpeedMultiplier(behavior.navigationSpeed);
        animateVerticalScrollBy(Math.round(scrollEl.clientHeight * 0.78 * mult));
      }
      return;
    }

    // Focused zoom toggle (`1x <-> 2.0x`): brings the exact tapped point cleanly to viewport center
    // while locking focus onto the zoomed page so `dimUnfocused` and progress tracking stay intact.
    if (zoomScale > 1.01) {
      resetZoom(clientY);
      return;
    }
    if (!scrollEl) return;

    const vpRect = scrollEl.getBoundingClientRect();
    const pointerX = clientX - vpRect.left;
    const pointerY = clientY - vpRect.top;
    const box = measureZoomTargetBox(target);
    zoomContentRect = box.rect;
    zoomOriginLeft = box.originLeft;
    zoomOriginTop = box.originTop;

    const next = computeFocusedZoom(
      pointerX,
      pointerY,
      { scale: 1, tx: 0, ty: 0 },
      ZOOM_TARGET_SCALE,
      vpRect.width,
      vpRect.height,
      box.rect,
      box.originLeft,
      box.originTop
    );

    if (zoomFocusSettleTimer !== null) {
      window.clearTimeout(zoomFocusSettleTimer);
      zoomFocusSettleTimer = null;
    }

    if (isHorizontal) {
      zoomTransitionLockUntil = Date.now() + ZOOM_ANIM_MS + 30;
      zoomScale = next.scale;
      zoomTx = next.tx;
      zoomTy = next.ty;
      zoomFocusSettleTimer = window.setTimeout(() => {
        zoomFocusSettleTimer = null;
        zoomTransitionLockUntil = 0;
        updateFocusNow();
      }, ZOOM_ANIM_MS + 20);
    } else {
      commitPendingVerticalZoom();
      // Lock focus immediately onto the exact page the user tapped before the transform animates.
      const tappedPage = findVerticalPageAtPoint(clientY, target);
      if (tappedPage) {
        focusedKeys = new Set([tappedPage.key]);
        focusedPageIdx =
          totalPages > 0
            ? Math.max(0, Math.min(totalPages - 1, tappedPage.idx))
            : Math.max(0, tappedPage.idx);
      }

      const docYBefore = scrollEl.scrollTop + pointerY;
      const baseHeight = verticalContentEl?.offsetHeight ?? scrollEl.scrollHeight;
      const desiredScrollTop = Math.max(
        0,
        Math.round(docYBefore * next.scale - vpRect.height * 0.5)
      );
      animateVerticalZoomTo(next.scale, next.tx, desiredScrollTop, baseHeight);
    }
  }

  function handleViewportClick(e: MouseEvent) {
    wakeFloatingControls();
    if (Date.now() < suppressNextClickUntil) {
      return;
    }
    if (didPanDrag) {
      didPanDrag = false;
      return;
    }
    if (activeBubbleCallout !== null) {
      activeBubbleCallout = null;
      activeBubbleKey = null;
      return;
    }
    if (imageMenuTarget !== null) {
      if (!imageActionBusy) imageMenuTarget = null;
      return;
    }
    if (aiPillOpen) {
      aiPillOpen = false;
      return;
    }
    if (quickFloatConfig !== null) {
      quickFloatConfig = null;
      return;
    }
    if (quickSettingsOpen) {
      quickSettingsOpen = false;
      openQuickCat = null;
      return;
    }
    if (chapterJumpOpen) {
      chapterJumpOpen = false;
      return;
    }
    if (e.target instanceof HTMLElement && e.target.closest("button, a, select, input")) {
      return;
    }
    // Ignore drags/scroll gestures (> 14px movement to allow slight mobile finger jitter).
    if (Math.hypot(e.clientX - pointerDownX, e.clientY - pointerDownY) > 14) {
      return;
    }

    const tapX = e.clientX;
    const tapY = e.clientY;
    const runSingleTap = () => {
      // Bubble Magnifier: single tap on any detected speech bubble opens the Floating Text Callout.
      if (aiBubbleZoom && hitTestBubbleAtPoint(tapX, tapY)) {
        return;
      }
      executeSingleTap(tapX, tapY);
    };

    const now = Date.now();
    const doubleTapEnabled = behavior.doubleTapAction !== "disabled";
    const isDoubleTap =
      doubleTapEnabled &&
      now - lastTapAt <= 260 &&
      Math.hypot(tapX - lastTapX, tapY - lastTapY) <= 48;

    if (isDoubleTap) {
      lastTapAt = 0;
      if (singleTapTimer !== null) {
        window.clearTimeout(singleTapTimer);
        singleTapTimer = null;
      }
      executeDoubleTap(tapX, tapY, e.target);
      return;
    }

    lastTapAt = now;
    lastTapX = tapX;
    lastTapY = tapY;
    if (!doubleTapEnabled) {
      runSingleTap();
    } else {
      if (singleTapTimer !== null) {
        window.clearTimeout(singleTapTimer);
      }
      singleTapTimer = window.setTimeout(() => {
        singleTapTimer = null;
        runSingleTap();
      }, 220);
    }
  }

  async function handleManualVerify(targetUrl?: string) {
    const target = targetUrl || errorState?.blockedUrl || chapterUrl;
    if (!target || verifyingBlocked) return;
    verifyingBlocked = true;
    try {
      toasts.push(t.toastOpeningVerify, "info", 4500);
      await api.openManualVerification(target);
      toasts.push(t.toastVerifyDone, "success");
      await loadInitial(chapterUrl, initialScrollRatio, initialPageIndex);
    } catch (err) {
      errorState = formatAppError(err, settingsStore.value.language);
    } finally {
      verifyingBlocked = false;
    }
  }

  async function handleBrowserLogin(targetUrl: string) {
    const target = targetUrl || chapterUrl;
    if (!target || loggingIn) return;
    loggingIn = true;
    try {
      toasts.push(t.errActionOpeningLogin, "info", 4500);
      const outcome = await api.openBrowserLoginSession(target);
      if (outcome.cookiesSynced) {
        toasts.push(t.toastLoginSynced, "success");
        await loadInitial(chapterUrl, initialScrollRatio, initialPageIndex);
      } else {
        toasts.push(t.toastLoginExternalOpened, "info", 4500);
      }
    } catch (err) {
      errorState = formatAppError(err, settingsStore.value.language);
    } finally {
      loggingIn = false;
    }
  }

  async function handleOpenExternalUrl(targetUrl?: string) {
    const target = targetUrl || errorState?.externalUrl || chapterUrl;
    if (!target) return;
    try {
      await api.openExternalUrl(target);
    } catch (err) {
      toasts.push(formatAppError(err, settingsStore.value.language).detail, "error");
    }
  }

  function handleWheel(e: WheelEvent) {
    if (!scrollEl || e.ctrlKey) return;

    if (isHorizontal) {
      e.preventDefault();
      // If zoomed in horizontally, pan vertically/horizontally with wheel instead of flipping page.
      if (zoomScale > 1.01) {
        const vpRect = scrollEl.getBoundingClientRect();
        const rect = zoomContentRect ?? {
          left: 0,
          top: 0,
          width: vpRect.width,
          height: vpRect.height,
        };
        const clamped = clampZoomPan(
          zoomTx - e.deltaX,
          zoomTy - e.deltaY,
          zoomScale,
          vpRect.width,
          vpRect.height,
          rect,
          zoomOriginLeft,
          zoomOriginTop
        );
        zoomTx = clamped.tx;
        zoomTy = clamped.ty;
        return;
      }

      const dominant = Math.abs(e.deltaY) >= Math.abs(e.deltaX) ? e.deltaY : e.deltaX;
      if (Math.abs(dominant) < 2) return;
      wheelDeltaAccum += dominant;
      if (wheelResetTimer !== null) window.clearTimeout(wheelResetTimer);
      wheelResetTimer = window.setTimeout(() => {
        wheelDeltaAccum = 0;
        wheelResetTimer = null;
      }, 180);

      if (Math.abs(wheelDeltaAccum) >= 36) {
        const step: 1 | -1 = wheelDeltaAccum > 0 ? 1 : -1;
        wheelDeltaAccum = 0;
        stepHorizontalGroup(step, true);
      }
      return;
    }

    // Vertical mode: smooth speed-adjustable wheel movement for discrete desktop wheel ticks.
    if (e.deltaMode === 0 && Math.abs(e.deltaY) >= 40) {
      e.preventDefault();
      const mult = computeVerticalSpeedMultiplier(behavior.navigationSpeed);
      animateVerticalScrollBy(e.deltaY * mult);
    } else if (e.deltaMode === 1) {
      e.preventDefault();
      const mult = computeVerticalSpeedMultiplier(behavior.navigationSpeed);
      animateVerticalScrollBy(e.deltaY * 42 * mult);
    }
  }

  function handleScroll() {
    cancelLongPress();
    if (isHorizontal || !scrollEl || strip.length === 0 || loadingInitial) return;

    wakeFloatingControls();
    notifyAiScrollActivity();
    if (Date.now() >= zoomTransitionLockUntil) {
      scheduleFocusUpdate();
    }

    const suppressHide = scrubbing || Date.now() < suppressAutoHideUntil;

    const top = scrollEl.scrollTop;
    const delta = top - lastScrollOffset;
    showScrollTopBtn = top > 320;

    if (!restoredInitialScroll && restoreTimer !== null && Math.abs(delta) > 15) {
      isUserScrollingBeforeRestore = true;
      window.clearTimeout(restoreTimer);
      restoreTimer = null;
      restoredInitialScroll = true;
    }

    // Part 2.4: Scrolling downward automatically fades out reader toolbars & popovers;
    // scrolling/swiping upward does NOT open the reader controls (single-tap toggles them).
    if (!suppressHide && delta > 14 && top > 64) {
      toolbarVisible = false;
      quickSettingsOpen = false;
      openQuickCat = null;
      quickFloatConfig = null;
      chapterJumpOpen = false;
    }
    lastScrollOffset = top;

    // Scale-correct scroll offset against unscaled `offsetTop`/`offsetHeight` when zoomed.
    const scaleFactor = zoomScale > 1.01 ? zoomScale : 1;
    const unscaledTop = top / scaleFactor;
    const unscaledVpH = scrollEl.clientHeight / scaleFactor;
    const viewportMid = unscaledTop + unscaledVpH * 0.35;

    const sections = Array.from(scrollEl.querySelectorAll<HTMLElement>("[data-chapter-url]"));
    let currentSection: HTMLElement | null = null;

    for (const sec of sections) {
      const secTop = sec.offsetTop;
      const secBottom = secTop + sec.offsetHeight;
      if (viewportMid >= secTop && viewportMid <= secBottom) {
        currentSection = sec;
        break;
      }
    }
    if (!currentSection && sections.length > 0) {
      currentSection =
        viewportMid < sections[0].offsetTop ? sections[0] : sections[sections.length - 1];
    }

    if (currentSection && restoredInitialScroll) {
      const chapUrl = currentSection.dataset.chapterUrl ?? activeUrl;
      const chapIdx = strip.findIndex((c) => normUrl(c.url) === normUrl(chapUrl));
      const chap = chapIdx >= 0 ? strip[chapIdx] : undefined;
      if (chap) {
        const chapterChanged = normUrl(activeUrl) !== normUrl(chap.url);
        activeUrl = chap.url;
        if (chapterChanged) {
          updateFocusNow();
          if (behavior.autoMarkReadOnOpen) {
            markChapterReadLocalAndRemote(chap.url);
          }
          // Reclaim disk storage for earlier finished chapters (2+ chapters behind active view).
          for (let i = 0; i <= chapIdx - 2; i += 1) {
            const oldChap = strip[i];
            if (oldChap) purgeFinishedChapterCache(oldChap.url);
          }
        }
        const secHeight = Math.max(1, currentSection.offsetHeight - unscaledVpH);
        const ratio = Math.min(1, Math.max(0, (unscaledTop - currentSection.offsetTop) / secHeight));

        scheduleSaveProgress(chap.url, chap.name, ratio, focusedPageIdx);

        if (ratio >= 0.85) {
          markChapterReadLocalAndRemote(chap.url);
        }
      }
    }

    if (!restoredInitialScroll || appendingNext) return;
    const remaining = scrollEl.scrollHeight - (top + scrollEl.clientHeight);
    const tail = lastChapter;
    if (tail?.next) {
      const progressAll = (top + scrollEl.clientHeight) / Math.max(1, scrollEl.scrollHeight);
      if (progressAll >= 0.7) {
        void prefetchNext(tail.next);
      }
      if (readerSettings.autoNextChapter && remaining < 1400) {
        void appendNextChapter();
      }
    }
  }

  function scheduleSaveProgress(url: string, name: string, ratio: number, pageIndex: number) {
    pendingProgress = { mangaId, url, name, ratio, pageIndex };
    if (saveTimer !== null) {
      window.clearTimeout(saveTimer);
    }
    saveTimer = window.setTimeout(() => {
      void flushPendingProgress();
    }, 900);
  }

  async function toggleFullscreen() {
    isFullscreen = await deviceEnv.toggleDesktopFullscreen();
  }

  async function toggleBookmark() {
    if (!manga) return;
    try {
      const next = await api.toggleBookmark(manga.id);
      manga = { ...manga, bookmarked: next };
      toasts.push(next ? t.toastBookmarkAdded : t.toastBookmarkRemoved, "success", 1800);
    } catch {
      // Ignore error.
    }
  }

  async function setOrientationLockDirect(next: OrientationLockMode) {
    updateMangaReaderSettings((r) => {
      r.orientationLock = next;
    });
    await deviceEnv.setOrientationLock(next);
    const label =
      next === "portrait"
        ? t.orientationPortrait
        : next === "landscape"
          ? t.orientationLandscape
          : t.orientationAuto;
    toasts.push(label, "info", 1600);
  }

  async function cycleOrientationLock() {
    const order: OrientationLockMode[] = ["auto", "portrait", "landscape"];
    const curr = readerSettings.orientationLock;
    const next = order[(order.indexOf(curr) + 1) % order.length];
    await setOrientationLockDirect(next);
  }

  function toggleQuickSettings() {
    if (!popoverGuard.canAct()) return;
    popoverGuard.lock();
    const opening = !quickSettingsOpen;
    quickSettingsOpen = opening;
    openQuickCat = null;
    if (opening) {
      chapterJumpOpen = false;
      quickFloatConfig = null;
      aiPillOpen = false;
    }
    wakeFloatingControls();
  }

  function toggleChapterJump() {
    if (!popoverGuard.canAct()) return;
    popoverGuard.lock();
    chapterJumpOpen = !chapterJumpOpen;
    quickSettingsOpen = false;
    openQuickCat = null;
    quickFloatConfig = null;
    aiPillOpen = false;
    wakeFloatingControls();
  }

  function openFullSettings(section: SettingsSection) {
    quickSettingsOpen = false;
    openQuickCat = null;
    quickFloatConfig = null;
    chapterJumpOpen = false;
    if (activeChapter && !loadingInitial) {
      let ratio = 0;
      if (isHorizontal) {
        const totalGroups = Math.max(1, horizontalGroups.length);
        ratio =
          totalGroups <= 1 ? 1 : Math.min(1, Math.max(0, activeGroupIdx / (totalGroups - 1)));
      } else if (scrollEl) {
        const maxScroll = Math.max(1, scrollEl.scrollHeight - scrollEl.clientHeight);
        ratio = Math.min(1, Math.max(0, scrollEl.scrollTop / maxScroll));
      }
      router.updateCurrentReaderPosition(activeChapter.url, ratio, focusedPageIdx);
    }
    captureCurrentProgressNow(false);
    router.goSettings(section);
  }

  const allFloatingScreens: FloatingScreenKey[] = [
    "home",
    "sources",
    "browse",
    "bookmarks",
    "history",
    "manga",
    "reader",
    "settings",
  ];

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

  function toggleScrollTopScreen(screen: FloatingScreenKey) {
    settingsStore.update((d) => {
      const list = d.floating.scrollToTopScreens;
      if (list.includes(screen)) {
        d.floating.scrollToTopScreens = list.filter((x) => x !== screen);
      } else {
        d.floating.scrollToTopScreens = [...list, screen];
      }
    });
  }

  async function exitReader() {
    closingReader = true;
    flushPendingMangaSettings();
    captureCurrentProgressNow(false);
    await flushPendingProgress(false);
    void aiTranslateStore.disableAndPurge();
    await api.clearReaderSession().catch(() => {});
    // Reclaim transient chapter page cache immediately on exit while keeping cover & settings.
    void api.purgeMangaPageCache(mangaId).catch(() => {});
    await applyKeepScreenOn(false);
    // Preserve desktop fullscreen state (Part 8) so exiting a chapter does not unexpectedly leave fullscreen.
    if (!router.goBack()) {
      router.goManga(mangaId);
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (
      e.target instanceof HTMLInputElement ||
      e.target instanceof HTMLSelectElement ||
      e.target instanceof HTMLTextAreaElement
    ) {
      return;
    }

    if (e.key === "F11" || e.key === "f" || e.key === "F") {
      e.preventDefault();
      void toggleFullscreen();
      return;
    }

    if (e.key === "Escape") {
      e.preventDefault();
      if (activeBubbleCallout !== null) {
        activeBubbleCallout = null;
        activeBubbleKey = null;
      } else if (imageMenuTarget !== null) {
        if (!imageActionBusy) imageMenuTarget = null;
      } else if (aiPillOpen) aiPillOpen = false;
      else if (chapterJumpOpen) chapterJumpOpen = false;
      else if (quickFloatConfig !== null) quickFloatConfig = null;
      else if (quickSettingsOpen) {
        quickSettingsOpen = false;
        openQuickCat = null;
      } else if (zoomScale > 1.01) resetZoom();
      else if (!toolbarVisible) toolbarVisible = true;
      else void exitReader();
      return;
    }

    if (isHorizontal) {
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        stepHorizontalGroup(isRtl ? 1 : -1, e.repeat);
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        stepHorizontalGroup(isRtl ? -1 : 1, e.repeat);
      } else if (e.key === "PageDown" || e.key === " ") {
        e.preventDefault();
        stepHorizontalGroup(e.shiftKey ? -1 : 1, e.repeat);
      } else if (e.key === "PageUp") {
        e.preventDefault();
        stepHorizontalGroup(-1, e.repeat);
      } else if (e.key === "Home") {
        e.preventDefault();
        seekToPage(0);
      } else if (e.key === "End" && totalPages > 0) {
        e.preventDefault();
        seekToPage(totalPages - 1);
      }
      return;
    }

    const mult = computeVerticalSpeedMultiplier(behavior.navigationSpeed);
    if (e.key === "ArrowDown" && scrollEl) {
      e.preventDefault();
      animateVerticalScrollBy(Math.round(180 * mult));
    } else if (e.key === "ArrowUp" && scrollEl) {
      e.preventDefault();
      animateVerticalScrollBy(-Math.round(180 * mult));
    } else if (e.key === "PageDown" && scrollEl) {
      e.preventDefault();
      animateVerticalScrollBy(Math.round(scrollEl.clientHeight * 0.85 * mult));
    } else if (e.key === "PageUp" && scrollEl) {
      e.preventDefault();
      animateVerticalScrollBy(-Math.round(scrollEl.clientHeight * 0.85 * mult));
    } else if (e.key === "ArrowLeft" && activeChapter?.prev) {
      e.preventDefault();
      jumpToChapter(activeChapter.prev);
    } else if (e.key === "ArrowRight" && activeChapter?.next) {
      e.preventDefault();
      jumpToChapter(activeChapter.next);
    } else if (e.key === " " && scrollEl) {
      e.preventDefault();
      const amount = Math.round(scrollEl.clientHeight * 0.8 * mult);
      animateVerticalScrollBy(e.shiftKey ? -amount : amount);
    }
  }

  function setOrientation(o: ReadingOrientation) {
    resetZoom(undefined, true);
    updateMangaReaderSettings((r) => {
      r.orientation = o;
      if (o === "horizontal") {
        // Switching explicitly to horizontal on a detected webtoon/manga disables auto-layout override for this manga.
        r.autoWebtoonDetect = false;
      }
    });
  }

  function resetToAutoMode() {
    void resetMangaReaderSettingsToGlobal();
  }

  function setPagesPerGroup(n: number) {
    const clamped = Math.max(1, Math.min(8, n));
    updateMangaReaderSettings((r) => {
      r.horizontal.pagesPerGroup = clamped;
    });
  }

  function setHorizontalDirection(dir: HorizontalDirection) {
    updateMangaReaderSettings((r) => {
      r.horizontal.direction = dir;
    });
  }

  function setNavigationSpeed(speed: number) {
    const clamped = Math.max(1, Math.min(10, Math.round(speed)));
    updateMangaReaderSettings((r) => {
      r.behavior.navigationSpeed = clamped;
    }, 120);
  }

  function toggleDim() {
    updateMangaReaderSettings((r) => {
      if (isHorizontal) {
        r.horizontal.dimUnfocused = !r.horizontal.dimUnfocused;
      } else {
        r.vertical.dimUnfocused = !r.vertical.dimUnfocused;
      }
    });
  }

  function setDimLevel(val: number) {
    updateMangaReaderSettings((r) => {
      if (isHorizontal) {
        r.horizontal.dimLevel = val;
      } else {
        r.vertical.dimLevel = val;
      }
    }, 120);
  }

  function toggleAutoNext() {
    updateMangaReaderSettings((r) => {
      r.autoNextChapter = !r.autoNextChapter;
    });
  }

  function toggleNoGap() {
    const turningOff = effectiveNoGap;
    if (turningOff) {
      updateMangaReaderSettings((r) => {
        r.vertical.noGapMode = false;
        if (isAutoWebtoonActive) {
          r.autoWebtoonDetect = false;
        }
        if (r.vertical.pageGapPx === 0) {
          r.vertical.pageGapPx = 12;
          r.gapPx = 12;
        }
      });
    } else {
      updateMangaReaderSettings((r) => {
        r.vertical.noGapMode = true;
      });
    }
  }

  function setVerticalGapPx(val: number) {
    const clamped = Math.max(0, Math.min(48, Math.round(val)));
    updateMangaReaderSettings((r) => {
      r.vertical.noGapMode = false;
      r.vertical.pageGapPx = clamped;
      r.gapPx = clamped;
      if (isAutoWebtoonActive && clamped > 0) {
        r.autoWebtoonDetect = false;
      }
    }, 120);
  }

  function setReaderBg(bg: ReaderBg) {
    updateMangaReaderSettings((r) => {
      r.readerBg = bg;
    });
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="relative flex h-full w-full flex-col overflow-hidden"
  style="background-color: {readerBackdrop()};"
>
  <!-- Top Auto-Hide Toolbar (frosted glass, GPU transform/opacity only; Back tracks active One-Handed thumb side) -->
  <header
    class="glass-chrome fixed top-0 right-0 left-0 z-30 flex items-center justify-between gap-2 border-b px-3 py-2 transition-[opacity,transform] duration-200 sm:px-4 {oneHandedMode === 'right'
      ? 'flex-row-reverse'
      : ''}"
    style="padding-top: calc(0.5rem + var(--safe-top)); padding-left: max(0.75rem, var(--safe-left)); padding-right: max(0.75rem, var(--safe-right)); border-color: var(--border); opacity: {toolbarVisible
      ? 1
      : 0}; pointer-events: {toolbarVisible ? 'auto' : 'none'}; transform: translate3d(0, {toolbarVisible
      ? '0px'
      : '-8px'}, 0);"
  >
    <div class="flex min-w-0 items-center gap-2 {oneHandedMode === 'right' ? 'flex-row-reverse' : ''}">
      <button
        type="button"
        onclick={exitReader}
        class="gpu-press touch-target inline-flex min-h-[38px] items-center gap-1 rounded-xl px-2.5 py-1.5 text-sm font-medium hover:opacity-80"
        title={t.backTooltip}
      >
        <ArrowLeft size={17} />
        <span class="hidden sm:inline">{t.back}</span>
      </button>

      <div
        class="min-w-0 {oneHandedMode === 'right' ? 'border-r pr-2.5 text-right' : 'border-l pl-2.5'}"
        style="border-color: var(--border);"
      >
        <div class="flex items-center gap-1.5 {oneHandedMode === 'right' ? 'justify-end' : ''}">
          <div class="truncate text-sm font-semibold">{manga?.title ?? t.readingFallback}</div>
          {#if isAutoWebtoonActive}
            <span
              transition:badgePop|local
              class="hidden shrink-0 items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px] font-medium sm:inline-flex"
              style="background-color: var(--accent-soft); color: var(--accent);"
              title={t.autoWebtoonBadge}
            >
              <Sparkles size={10} />
              Webtoon
            </span>
          {:else if isAutoMangaActive}
            <span
              transition:badgePop|local
              class="hidden shrink-0 items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px] font-medium sm:inline-flex"
              style="background-color: var(--accent-soft); color: var(--accent);"
            >
              <Sparkles size={10} />
              {t.a11yAutoMangaBadge}
            </span>
          {/if}
        </div>
        <div class="truncate text-xs" style="color: var(--text-muted);">
          {activeChapter?.name ?? ""}
          {#if totalPages > 0}
            · {focusedPageIdx + 1} / {totalPages}
          {/if}
        </div>
      </div>
    </div>

    <div class="flex shrink-0 items-center gap-1 {oneHandedMode === 'right' ? 'flex-row-reverse' : ''}">
      {#if zoomScale > 1.01}
        <span transition:badgePop|local class="inline-flex">
          <button
            type="button"
            onclick={() => resetZoom()}
            class="gpu-press touch-target inline-flex min-h-[38px] items-center gap-1 rounded-xl border px-2.5 py-1.5 text-xs font-semibold"
            style="border-color: var(--accent); background-color: var(--accent-soft); color: var(--accent);"
            title={t.readerResetZoom}
          >
            <ZoomOut size={14} />
            <span>1x</span>
          </button>
        </span>
      {/if}

      {#if manga && manga.chapters.length > 0}
        <button
          type="button"
          onclick={toggleChapterJump}
          class="gpu-press touch-target inline-flex min-h-[38px] items-center gap-1 rounded-xl border px-2.5 py-1.5 text-xs font-medium"
          style="border-color: {chapterJumpOpen
            ? 'var(--accent)'
            : 'var(--border)'}; background-color: {chapterJumpOpen
            ? 'var(--accent-soft)'
            : 'var(--surface)'}; color: {chapterJumpOpen ? 'var(--accent)' : 'var(--text)'};"
          title={t.quickChapterJump}
        >
          <List size={15} />
          <span class="hidden sm:inline">{t.chaptersBtn}</span>
        </button>
      {/if}

      {#if manga}
        <button
          type="button"
          onclick={toggleBookmark}
          class="gpu-press touch-target flex min-h-[38px] min-w-[38px] items-center justify-center rounded-xl border"
          style="border-color: var(--border); background-color: {manga.bookmarked
            ? 'var(--accent-soft)'
            : 'var(--surface)'}; color: {manga.bookmarked ? 'var(--accent)' : 'var(--text)'};"
          title={manga.bookmarked ? t.bookmarkRemove : t.bookmarkAdd}
          aria-label={manga.bookmarked ? t.bookmarkRemove : t.bookmarkAdd}
        >
          {#key manga.bookmarked}
            <span in:badgePop|local={{ duration: 150, start: 0.68 }} class="inline-flex">
              <Bookmark size={16} fill={manga.bookmarked ? "currentColor" : "none"} />
            </span>
          {/key}
        </button>
      {/if}

      <button
        type="button"
        onclick={toggleQuickSettings}
        class="gpu-press touch-target flex min-h-[38px] min-w-[38px] items-center justify-center rounded-xl border"
        style="border-color: var(--border); background-color: {quickSettingsOpen
          ? 'var(--accent-soft)'
          : 'var(--surface)'}; color: {quickSettingsOpen ? 'var(--accent)' : 'var(--text)'};"
        title={t.quickSettingsTooltip}
        aria-label={t.quickSettingsTooltip}
      >
        <SlidersHorizontal size={16} />
      </button>

      {#if !deviceEnv.isAndroid}
        <button
          type="button"
          onclick={toggleFullscreen}
          class="gpu-press flex min-h-[38px] min-w-[38px] items-center justify-center rounded-xl border"
          style="border-color: var(--border); background-color: var(--surface);"
          title={t.fullscreenTooltip}
          aria-label={t.fullscreenTooltip}
        >
          {#key isFullscreen}
            <span in:badgePop|local={{ duration: 140, start: 0.72 }} class="inline-flex">
              {#if isFullscreen}
                <Minimize size={16} />
              {:else}
                <Maximize size={16} />
              {/if}
            </span>
          {/key}
        </button>
      {/if}
    </div>
  </header>

  <!-- Mobile Bottom-Sheet Backdrop when any Reader overlay is open on Compact Mobile (<768px) -->
  {#if isCompactMobile && (chapterJumpOpen || quickSettingsOpen || quickFloatConfig !== null)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      transition:backdropFade|local
      onclick={closeReaderOverlays}
      class="fixed inset-0 z-30 bg-black/48 backdrop-blur-[2px]"
    ></div>
  {/if}

  <!-- Quick Chapter Jump (Thumb-Reachable Bottom Sheet on <768px, Frosted Popover on >=768px) -->
  {#if chapterJumpOpen && manga}
    <div
      transition:drawerSlide|local={{
        side: isCompactMobile ? "bottom" : "top",
        distance: isCompactMobile ? 52 : 8,
        duration: isCompactMobile ? 210 : 165,
      }}
      class={isCompactMobile
        ? "glass-chrome fixed right-0 bottom-0 left-0 z-40 flex max-h-[78dvh] w-full flex-col gap-2.5 overscroll-contain rounded-t-3xl border-t px-4 pt-1 pb-4 text-xs"
        : "glass-chrome fixed z-40 flex max-h-[calc(100dvh-4.75rem-var(--safe-top)-var(--safe-bottom))] w-80 max-w-[calc(100dvw-1.5rem-var(--safe-left)-var(--safe-right))] flex-col gap-2.5 overscroll-contain rounded-2xl border p-3.5 text-xs"}
      style={isCompactMobile
        ? "padding-bottom: max(1rem, var(--safe-bottom)); padding-left: max(1rem, var(--safe-left)); padding-right: max(1rem, var(--safe-right)); border-color: var(--border); box-shadow: var(--shadow-lg);"
        : `top: calc(3.75rem + var(--safe-top)); ${oneHandedMode === "left" ? "left: max(0.75rem, var(--safe-left));" : "right: max(0.75rem, var(--safe-right));"} border-color: var(--border); box-shadow: var(--shadow-lg);`}
    >
      {#if isCompactMobile}
        <div class="sheet-handle" aria-hidden="true"></div>
      {/if}
      <div class="flex items-center justify-between font-semibold">
        <span>{t.quickChapterJump}</span>
        <div class="flex items-center gap-2">
          <span style="color: var(--text-muted);">{t.chapterCount(manga.chapters.length)}</span>
          {#if isCompactMobile}
            <button
              type="button"
              onclick={closeReaderOverlays}
              class="gpu-press flex h-8 w-8 items-center justify-center rounded-xl opacity-75 hover:opacity-100"
              aria-label={t.cancel}
            >
              <X size={16} />
            </button>
          {/if}
        </div>
      </div>
      <div
        class="flex min-h-[40px] shrink-0 items-center gap-2 rounded-xl border px-3 py-1.5"
        style="background-color: var(--surface-subtle); border-color: var(--border);"
      >
        <Search size={14} style="color: var(--text-muted);" />
        <input
          type="text"
          bind:value={chapterJumpQuery}
          placeholder={t.filterChapters}
          class="w-full bg-transparent text-xs outline-none"
        />
      </div>
      <div class="flex min-h-0 flex-1 flex-col gap-1.5 overflow-y-auto pr-0.5">
        {#each filteredJumpChapters() as ch (ch.url)}
          {@const isCurrent = normUrl(ch.url) === normUrl(activeUrl)}
          <button
            type="button"
            onclick={() => jumpToChapter(ch.url)}
            class="gpu-press flex min-h-[44px] items-center justify-between gap-2 rounded-xl px-3 py-2 text-left hover:opacity-95 sm:min-h-[36px]"
            style="background-color: {isCurrent
              ? 'var(--accent-soft)'
              : 'var(--surface-subtle)'}; color: {isCurrent
              ? 'var(--accent)'
              : ch.read
                ? 'var(--text-muted)'
                : 'var(--text)'}; opacity: {!isCurrent && ch.read ? 0.58 : 1};"
          >
            <span class="truncate font-medium">{ch.name}</span>
            {#if ch.read}
              <Check size={14} class="shrink-0 opacity-75" />
            {/if}
          </button>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Quick Reader Settings (Thumb-Reachable Bottom Sheet on <768px, Frosted Popover on >=768px) -->
  {#if quickSettingsOpen}
    <div
      transition:drawerSlide|local={{
        side: isCompactMobile ? "bottom" : "top",
        distance: isCompactMobile ? 52 : 8,
        duration: isCompactMobile ? 210 : 165,
      }}
      role="dialog"
      tabindex="-1"
      onpointerdown={(e) => e.stopPropagation()}
      ontouchstart={(e) => {
        e.stopPropagation();
        wakeFloatingControls();
      }}
      onwheel={(e) => e.stopPropagation()}
      class={isCompactMobile
        ? "glass-chrome fixed right-0 bottom-0 left-0 z-40 flex max-h-[82dvh] w-full flex-col gap-2.5 overflow-y-auto overscroll-contain touch-pan-y rounded-t-3xl border-t px-4 pt-1 pb-4 text-xs"
        : "reader-quick-popover glass-chrome fixed z-40 flex max-h-[calc(100dvh-4.75rem-var(--safe-top)-var(--safe-bottom))] w-80 max-w-[calc(100dvw-1.5rem-var(--safe-left)-var(--safe-right))] flex-col gap-2.5 overflow-y-auto overscroll-contain touch-pan-y rounded-2xl border p-3.5 text-xs"}
      style={isCompactMobile
        ? "padding-bottom: max(1rem, var(--safe-bottom)); padding-left: max(1rem, var(--safe-left)); padding-right: max(1rem, var(--safe-right)); border-color: var(--border); box-shadow: var(--shadow-lg); -webkit-overflow-scrolling: touch;"
        : `top: calc(3.75rem + var(--safe-top)); ${oneHandedMode === "left" ? "left: max(0.75rem, var(--safe-left));" : "right: max(0.75rem, var(--safe-right));"} border-color: var(--border); box-shadow: var(--shadow-lg); -webkit-overflow-scrolling: touch;`}
    >
      {#if isCompactMobile}
        <div class="sheet-handle" aria-hidden="true"></div>
      {/if}
      <div class="flex shrink-0 items-center justify-between gap-2 pb-0.5">
        <span class="font-semibold">{t.readerSection}</span>
        <div class="flex items-center gap-1.5">
          <button
            type="button"
            onclick={() => openFullSettings("reader")}
            class="gpu-press inline-flex min-h-[32px] items-center gap-1 rounded-lg px-2.5 py-1 text-[11px] font-medium hover:opacity-85"
            style="background-color: var(--accent-soft); color: var(--accent);"
          >
            <ExternalLink size={11} />
            <span>{t.openFullSettingsBtn}</span>
          </button>
          {#if isCompactMobile}
            <button
              type="button"
              onclick={closeReaderOverlays}
              class="gpu-press flex h-8 w-8 items-center justify-center rounded-xl opacity-75 hover:opacity-100"
              aria-label={t.cancel}
            >
              <X size={16} />
            </button>
          {/if}
        </div>
      </div>

      {#if hasCustomMangaSettings || isAutoWebtoonActive || isAutoMangaActive || effectiveOverride !== null}
        <div
          transition:collapseReveal|local
          class="flex shrink-0 items-center justify-between gap-2 rounded-xl border px-3 py-2"
          style="background-color: var(--surface-subtle); border-color: var(--border);"
        >
          <span class="font-medium" style="color: var(--text);">
            {hasCustomMangaSettings || effectiveOverride !== null
              ? t.perMangaSettingsBadge
              : isAutoWebtoonActive
                ? t.autoWebtoonBadge
                : t.a11yAutoMangaBadge}
          </span>
          {#if hasCustomMangaSettings || effectiveOverride !== null}
            <button
              type="button"
              onclick={resetToAutoMode}
              class="gpu-press inline-flex shrink-0 items-center gap-1 rounded-md px-2 py-0.5 text-[11px] font-medium"
              style="background-color: var(--accent-soft); color: var(--accent);"
            >
              <RotateCcw size={11} />
              {t.resetMangaSettingsBtn}
            </button>
          {/if}
        </div>
      {/if}

      <!-- Category 1: Layout & Spacing -->
      <div class="shrink-0 overflow-hidden rounded-xl border" style="border-color: var(--border); background-color: var(--surface-subtle);">
        <button
          type="button"
          onclick={() => (openQuickCat = openQuickCat === "layout" ? null : "layout")}
          aria-expanded={openQuickCat === "layout"}
          class="gpu-press flex w-full items-center justify-between gap-2 px-3 py-2.5 text-left font-medium"
        >
          <div class="min-w-0">
            <div class="font-semibold">{t.readerCatLayout}</div>
            <div class="truncate text-[10px]" style="color: var(--text-muted);">
              {isHorizontal
                ? `${t.modeHorizontal} · ${effectiveHorizontalDirection.toUpperCase()} · ${effectivePagesPerGroup}x${isHorizontalSeamless ? ` · ${t.horizontalNoGapLabel}` : ""}`
                : `${t.modeVertical} · ${effectiveNoGap ? t.noGapModeLabel : `${effectiveVerticalGapPx}px`}`}
            </div>
          </div>
          <ChevronDown
            size={15}
            class="chevron-rot shrink-0 opacity-70"
            data-open={openQuickCat === "layout" ? "true" : "false"}
          />
        </button>

        {#if openQuickCat === "layout"}
          <div
            transition:collapseReveal|local
            class="flex flex-col gap-3 border-t p-3"
            style="border-color: var(--border); background-color: var(--surface);"
          >
            <div class="flex flex-col gap-1.5">
              <span class="font-medium">{t.readingModeLabel}</span>
              <div class="grid grid-cols-2 gap-1.5">
                {#each [["vertical", t.modeVertical], ["horizontal", t.modeHorizontal]] as [val, label] (val)}
                  <button
                    type="button"
                    onclick={() => setOrientation(val as ReadingOrientation)}
                    class="gpu-press rounded-lg border py-1.5 font-medium"
                    style="border-color: {effectiveOrientation === val
                      ? 'var(--accent)'
                      : 'var(--border)'}; background-color: {effectiveOrientation === val
                      ? 'var(--accent-soft)'
                      : 'var(--surface-subtle)'}; color: {effectiveOrientation === val
                      ? 'var(--accent)'
                      : 'var(--text)'};"
                  >
                    {label}
                  </button>
                {/each}
              </div>
            </div>

            {#if isHorizontal}
              <div class="flex flex-col gap-1.5">
                <span class="font-medium">{t.horizontalDirLabel}</span>
                <div class="grid grid-cols-2 gap-1.5">
                  {#each [["ltr", t.dirLtr], ["rtl", t.dirRtl]] as [val, label] (val)}
                    <button
                      type="button"
                      onclick={() => setHorizontalDirection(val as HorizontalDirection)}
                      class="gpu-press rounded-lg border py-1.5 font-medium"
                      style="border-color: {effectiveHorizontalDirection === val
                        ? 'var(--accent)'
                        : 'var(--border)'}; background-color: {effectiveHorizontalDirection === val
                        ? 'var(--accent-soft)'
                        : 'var(--surface-subtle)'}; color: {effectiveHorizontalDirection === val
                        ? 'var(--accent)'
                        : 'var(--text)'};"
                    >
                      {label}
                    </button>
                  {/each}
                </div>
              </div>

              <div class="flex flex-col gap-1.5">
                <div class="flex items-center justify-between font-medium">
                  <span>{t.pagesPerGroupLabel}</span>
                  <span style="color: var(--text-muted);">{effectivePagesPerGroup}</span>
                </div>
                <div class="grid grid-cols-4 gap-1.5">
                  {#each [1, 2, 3] as n (n)}
                    <button
                      type="button"
                      onclick={() => setPagesPerGroup(n)}
                      class="gpu-press rounded-lg border py-1 font-medium"
                      style="border-color: {hCfg.pagesPerGroup === n
                        ? 'var(--accent)'
                        : 'var(--border)'}; background-color: {hCfg.pagesPerGroup === n
                        ? 'var(--accent-soft)'
                        : 'var(--surface-subtle)'}; color: {hCfg.pagesPerGroup === n
                        ? 'var(--accent)'
                        : 'var(--text)'};"
                    >
                      {n}
                    </button>
                  {/each}
                  <input
                    type="number"
                    min="1"
                    max="8"
                    value={hCfg.pagesPerGroup}
                    onchange={(e) => setPagesPerGroup(Number(e.currentTarget.value))}
                    class="rounded-lg border px-2 py-1 text-center text-xs outline-none"
                    style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
                  />
                </div>
              </div>

              {#if hCfg.pagesPerGroup === 1}
                <div transition:collapseReveal|local class="flex items-center justify-between gap-2">
                  <span class="font-medium">{t.landscapeAutoSpreadLabel}</span>
                  <SquishSwitch
                    size="sm"
                    checked={landscapeAutoSpread}
                    ariaLabel={t.landscapeAutoSpreadLabel}
                    onchange={(checked) => setLandscapeAutoSpread(checked)}
                  />
                </div>
              {/if}

              <div class="flex items-center justify-between gap-2">
                <span class="font-medium">{t.horizontalNoGapLabel}</span>
                <SquishSwitch
                  size="sm"
                  checked={hCfg.noGapMode}
                  ariaLabel={t.horizontalNoGapLabel}
                  onchange={(checked) => {
                    updateMangaReaderSettings((r) => {
                      r.horizontal.noGapMode = checked;
                    });
                  }}
                />
              </div>
            {:else}
              <div class="flex items-center justify-between gap-2">
                <div class="flex items-center gap-1.5">
                  <span class="font-medium">{t.noGapModeLabel}</span>
                  {#if isAutoWebtoonActive && !vCfg.noGapMode}
                    <span
                      class="rounded px-1.5 py-0.5 text-[10px] font-medium"
                      style="background-color: var(--accent-soft); color: var(--accent);"
                    >
                      {t.noGapWebtoonHint}
                    </span>
                  {/if}
                </div>
                <SquishSwitch
                  size="sm"
                  checked={effectiveNoGap}
                  ariaLabel={t.noGapModeLabel}
                  onchange={toggleNoGap}
                />
              </div>

              {#if !effectiveNoGap}
                <div transition:collapseReveal|local class="flex flex-col gap-1">
                  <div class="flex items-center justify-between">
                    <span class="font-medium">{t.gapLabel(vCfg.pageGapPx)}</span>
                  </div>
                  <input
                    type="range"
                    min="0"
                    max="48"
                    step="2"
                    value={vCfg.pageGapPx}
                    oninput={(e) => setVerticalGapPx(Number(e.currentTarget.value))}
                    class="w-full"
                  />
                </div>
              {/if}

              <div class="flex items-center justify-between">
                <span class="font-medium">{t.autoAppendNext}</span>
                <SquishSwitch
                  size="sm"
                  checked={readerSettings.autoNextChapter}
                  ariaLabel={t.autoAppendNext}
                  onchange={toggleAutoNext}
                />
              </div>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Category 2: Display & Visuals -->
      <div class="shrink-0 overflow-hidden rounded-xl border" style="border-color: var(--border); background-color: var(--surface-subtle);">
        <button
          type="button"
          onclick={() => (openQuickCat = openQuickCat === "display" ? null : "display")}
          aria-expanded={openQuickCat === "display"}
          class="gpu-press flex w-full items-center justify-between gap-2 px-3 py-2.5 text-left font-medium"
        >
          <div class="min-w-0">
            <div class="font-semibold">{t.readerCatDisplay}</div>
            <div class="truncate text-[10px]" style="color: var(--text-muted);">
              {readerSettings.readerBg === "dark"
                ? t.bgDark
                : readerSettings.readerBg === "light"
                  ? t.bgLight
                  : t.bgAppTheme}
            </div>
          </div>
          <ChevronDown
            size={15}
            class="chevron-rot shrink-0 opacity-70"
            data-open={openQuickCat === "display" ? "true" : "false"}
          />
        </button>

        {#if openQuickCat === "display"}
          <div
            transition:collapseReveal|local
            class="flex flex-col gap-3 border-t p-3"
            style="border-color: var(--border); background-color: var(--surface);"
          >
            <div class="flex flex-col gap-1.5">
              <span class="font-medium">{t.readerBg}</span>
              <div class="grid grid-cols-3 gap-1.5">
                {#each [["dark", t.bgDark], ["light", t.bgLight], ["follow_theme", t.bgAppTheme]] as [val, label] (val)}
                  <button
                    type="button"
                    onclick={() => setReaderBg(val as ReaderBg)}
                    class="gpu-press rounded-lg border py-1.5 font-medium"
                    style="border-color: {readerSettings.readerBg === val
                      ? 'var(--accent)'
                      : 'var(--border)'}; background-color: {readerSettings.readerBg === val
                      ? 'var(--accent-soft)'
                      : 'var(--surface-subtle)'}; color: {readerSettings.readerBg === val
                      ? 'var(--accent)'
                      : 'var(--text)'};"
                  >
                    {label}
                  </button>
                {/each}
              </div>
            </div>

            <div class="flex flex-col gap-1.5">
              <div class="flex items-center justify-between">
                <span class="font-medium">{t.dimUnfocusedLabel}</span>
                <SquishSwitch
                  size="sm"
                  checked={isHorizontal ? hCfg.dimUnfocused : vCfg.dimUnfocused}
                  ariaLabel={t.dimUnfocusedLabel}
                  onchange={toggleDim}
                />
              </div>
              {#if isHorizontal ? hCfg.dimUnfocused : vCfg.dimUnfocused}
                {@const dimPct = isHorizontal ? hCfg.dimLevel : vCfg.dimLevel}
                <div transition:collapseReveal|local class="flex items-center gap-2">
                  <input
                    type="range"
                    min="0"
                    max="100"
                    step="5"
                    value={dimPct}
                    oninput={(e) => setDimLevel(Number(e.currentTarget.value))}
                    class="w-full"
                  />
                  <span class="w-9 text-right" style="color: var(--text-muted);">{dimPct}%</span>
                </div>
              {/if}
            </div>
          </div>
        {/if}
      </div>

      <!-- Category 3: Controls & Behavior -->
      <div class="shrink-0 overflow-hidden rounded-xl border" style="border-color: var(--border); background-color: var(--surface-subtle);">
        <button
          type="button"
          onclick={() => (openQuickCat = openQuickCat === "controls" ? null : "controls")}
          aria-expanded={openQuickCat === "controls"}
          class="gpu-press flex w-full items-center justify-between gap-2 px-3 py-2.5 text-left font-medium"
        >
          <div class="min-w-0">
            <div class="font-semibold">{t.readerCatControls}</div>
            <div class="truncate text-[10px]" style="color: var(--text-muted);">
              {behavior.keepScreenOn ? t.keepScreenOnLabel : t.skipBrokenImagesLabel}
            </div>
          </div>
          <ChevronDown
            size={15}
            class="chevron-rot shrink-0 opacity-70"
            data-open={openQuickCat === "controls" ? "true" : "false"}
          />
        </button>

        {#if openQuickCat === "controls"}
          <div
            transition:collapseReveal|local
            class="flex flex-col gap-3 border-t p-3"
            style="border-color: var(--border); background-color: var(--surface);"
          >
            {#if deviceEnv.isMobile}
              <div class="flex flex-col gap-1.5">
                <div class="flex items-center gap-1.5 font-medium">
                  <Hand size={13} style="color: var(--accent);" />
                  <span>{t.a11yOneHandedTitle}</span>
                </div>
                <div class="grid grid-cols-3 gap-1.5" role="radiogroup" aria-label={t.a11yOneHandedTitle}>
                  {#each [
                    ["left", t.a11yOneHandedLeft],
                    ["right", t.a11yOneHandedRight],
                    ["default", t.a11yOneHandedDefault],
                  ] as [mode, label] (mode)}
                    {@const active = settingsStore.value.accessibility.oneHandedMode === mode}
                    <button
                      type="button"
                      role="radio"
                      aria-checked={active}
                      onclick={() =>
                        settingsStore.update((d) => {
                          d.accessibility.oneHandedMode = mode as OneHandedMode;
                        })}
                      class="gpu-press flex min-h-[36px] items-center justify-center gap-1 rounded-lg border px-2 py-1 font-medium"
                      style="border-color: {active ? 'var(--accent)' : 'var(--border)'}; background-color: {active
                        ? 'var(--accent-soft)'
                        : 'var(--surface-subtle)'}; color: {active ? 'var(--accent)' : 'var(--text)'};"
                    >
                      {#if mode === "left"}
                        <PanelLeft size={12} />
                      {:else if mode === "right"}
                        <PanelRight size={12} />
                      {:else}
                        <Smartphone size={12} />
                      {/if}
                      <span class="truncate">{label}</span>
                    </button>
                  {/each}
                </div>
              </div>
            {/if}

            <div class="flex items-center justify-between">
              <span class="font-medium">{t.keepScreenOnLabel}</span>
              <SquishSwitch
                size="sm"
                checked={behavior.keepScreenOn}
                ariaLabel={t.keepScreenOnLabel}
                onchange={(checked) => {
                  updateMangaReaderSettings((r) => {
                    r.behavior.keepScreenOn = checked;
                  });
                }}
              />
            </div>

            <div class="flex items-center justify-between">
              <span class="font-medium">{t.skipBrokenImagesLabel}</span>
              <SquishSwitch
                size="sm"
                checked={behavior.skipBrokenImages}
                ariaLabel={t.skipBrokenImagesLabel}
                onchange={(checked) => {
                  updateMangaReaderSettings((r) => {
                    r.behavior.skipBrokenImages = checked;
                  });
                }}
              />
            </div>

            <div class="flex flex-col gap-1">
              <div class="flex items-center justify-between">
                <span class="font-medium">{t.navigationSpeedLabel(behavior.navigationSpeed)}</span>
              </div>
              <input
                type="range"
                min="1"
                max="10"
                step="1"
                value={behavior.navigationSpeed}
                oninput={(e) => setNavigationSpeed(Number(e.currentTarget.value))}
                class="w-full"
              />
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <!-- Long-Press Contextual Configuration Panel (Bottom Sheet on <768px, Frosted Popover on >=768px) -->
  {#if quickFloatConfig !== null}
    <div
      transition:drawerSlide|local={{
        side: isCompactMobile ? "bottom" : "top",
        distance: isCompactMobile ? 52 : 8,
        duration: isCompactMobile ? 210 : 165,
      }}
      role="dialog"
      tabindex="-1"
      onpointerdown={(e) => e.stopPropagation()}
      ontouchstart={(e) => {
        e.stopPropagation();
        wakeFloatingControls();
      }}
      onwheel={(e) => e.stopPropagation()}
      class={isCompactMobile
        ? "glass-chrome fixed right-0 bottom-0 left-0 z-40 flex max-h-[80dvh] w-full flex-col gap-3 overflow-y-auto overscroll-contain touch-pan-y rounded-t-3xl border-t px-4 pt-1 pb-4 text-xs"
        : "reader-quick-popover glass-chrome fixed z-40 flex max-h-[calc(100dvh-4.75rem-var(--safe-top)-var(--safe-bottom))] w-80 max-w-[calc(100dvw-1.5rem-var(--safe-left)-var(--safe-right))] flex-col gap-3 overflow-y-auto overscroll-contain touch-pan-y rounded-2xl border p-4 text-xs"}
      style={isCompactMobile
        ? "padding-bottom: max(1rem, var(--safe-bottom)); padding-left: max(1rem, var(--safe-left)); padding-right: max(1rem, var(--safe-right)); border-color: var(--border); box-shadow: var(--shadow-lg); -webkit-overflow-scrolling: touch;"
        : `top: calc(3.75rem + var(--safe-top)); ${oneHandedMode === "left" ? "left: max(0.75rem, var(--safe-left));" : "right: max(0.75rem, var(--safe-right));"} border-color: var(--border); box-shadow: var(--shadow-lg); -webkit-overflow-scrolling: touch;`}
    >
      {#if isCompactMobile}
        <div class="sheet-handle" aria-hidden="true"></div>
      {/if}
      <div class="flex shrink-0 items-center justify-between gap-2">
        <span class="font-semibold">
          {quickFloatConfig === "scrollToTop"
            ? t.scrollTopQuickConfigTitle
            : t.orientationQuickConfigTitle}
        </span>
        <button
          type="button"
          onclick={() => (quickFloatConfig = null)}
          class="gpu-press rounded-lg p-1 opacity-70 hover:opacity-100"
          aria-label={t.cancel}
        >
          <X size={14} />
        </button>
      </div>

      {#if quickFloatConfig === "orientationLock"}
        <div class="flex shrink-0 flex-col gap-1.5">
          <span class="font-medium">{t.orientationCurrentMode}</span>
          <div class="grid grid-cols-3 gap-1.5">
            {#each [["auto", t.orientationAuto], ["portrait", t.orientationPortrait], ["landscape", t.orientationLandscape]] as [val, label] (val)}
              <button
                type="button"
                onclick={() => void setOrientationLockDirect(val as OrientationLockMode)}
                class="gpu-press rounded-lg border py-1.5 font-medium"
                style="border-color: {readerSettings.orientationLock === val
                  ? 'var(--accent)'
                  : 'var(--border)'}; background-color: {readerSettings.orientationLock === val
                  ? 'var(--accent-soft)'
                  : 'var(--surface-subtle)'}; color: {readerSettings.orientationLock === val
                  ? 'var(--accent)'
                  : 'var(--text)'};"
              >
                {label}
              </button>
            {/each}
          </div>
        </div>

        <div class="flex shrink-0 items-center justify-between">
          <span class="font-medium">{t.buttonVisibilityLabel}</span>
          <SquishSwitch
            size="sm"
            checked={settingsStore.value.floating.showOrientationLock}
            ariaLabel={t.buttonVisibilityLabel}
            onchange={(checked) => {
              settingsStore.update((d) => (d.floating.showOrientationLock = checked));
            }}
          />
        </div>

        <div class="flex shrink-0 flex-col gap-1.5">
          <span class="font-medium">{t.floatingPositionLabel}</span>
          <div class="flex items-center gap-2">
            {#each [["left", t.floatingEdgeLeft], ["right", t.floatingEdgeRight]] as [edge, label] (edge)}
              <button
                type="button"
                onclick={() =>
                  settingsStore.update((d) => (d.floating.orientationLock.edge = edge as FloatingEdge))}
                class="gpu-press rounded-lg border px-2.5 py-1 font-medium"
                style="border-color: {settingsStore.value.floating.orientationLock.edge === edge
                  ? 'var(--accent)'
                  : 'var(--border)'}; background-color: {settingsStore.value.floating.orientationLock.edge === edge
                  ? 'var(--accent-soft)'
                  : 'var(--surface-subtle)'}; color: {settingsStore.value.floating.orientationLock.edge === edge
                  ? 'var(--accent)'
                  : 'var(--text)'};"
              >
                {label}
              </button>
            {/each}
            <input
              type="range"
              min="15"
              max="88"
              step="1"
              value={settingsStore.value.floating.orientationLock.verticalPercent}
              oninput={(e) => {
                const v = Number(e.currentTarget.value);
                settingsStore.update((d) => (d.floating.orientationLock.verticalPercent = v), 100);
              }}
              class="w-full"
            />
            <span class="w-8 text-right tabular-nums" style="color: var(--text-muted);">
              {settingsStore.value.floating.orientationLock.verticalPercent}%
            </span>
          </div>
        </div>
      {:else}
        <div class="flex shrink-0 flex-col gap-1.5">
          <span class="font-medium">{t.floatingScrollTopScreens}</span>
          <div class="flex flex-wrap gap-1.5">
            {#each allFloatingScreens as screen (screen)}
              {@const active = settingsStore.value.floating.scrollToTopScreens.includes(screen)}
              <button
                type="button"
                onclick={() => toggleScrollTopScreen(screen)}
                class="gpu-press rounded-lg border px-2.5 py-1 font-medium"
                style="border-color: {active
                  ? 'var(--accent)'
                  : 'var(--border)'}; background-color: {active
                  ? 'var(--accent-soft)'
                  : 'var(--surface-subtle)'}; color: {active ? 'var(--accent)' : 'var(--text)'};"
              >
                {floatingScreenLabel(screen)}
              </button>
            {/each}
          </div>
        </div>

        <div class="flex shrink-0 flex-col gap-1.5">
          <span class="font-medium">{t.floatingPositionLabel}</span>
          <div class="flex items-center gap-2">
            {#each [["left", t.floatingEdgeLeft], ["right", t.floatingEdgeRight]] as [edge, label] (edge)}
              <button
                type="button"
                onclick={() =>
                  settingsStore.update((d) => (d.floating.scrollToTop.edge = edge as FloatingEdge))}
                class="gpu-press rounded-lg border px-2.5 py-1 font-medium"
                style="border-color: {settingsStore.value.floating.scrollToTop.edge === edge
                  ? 'var(--accent)'
                  : 'var(--border)'}; background-color: {settingsStore.value.floating.scrollToTop.edge === edge
                  ? 'var(--accent-soft)'
                  : 'var(--surface-subtle)'}; color: {settingsStore.value.floating.scrollToTop.edge === edge
                  ? 'var(--accent)'
                  : 'var(--text)'};"
              >
                {label}
              </button>
            {/each}
            <input
              type="range"
              min="15"
              max="88"
              step="1"
              value={settingsStore.value.floating.scrollToTop.verticalPercent}
              oninput={(e) => {
                const v = Number(e.currentTarget.value);
                settingsStore.update((d) => (d.floating.scrollToTop.verticalPercent = v), 100);
              }}
              class="w-full"
            />
            <span class="w-8 text-right tabular-nums" style="color: var(--text-muted);">
              {settingsStore.value.floating.scrollToTop.verticalPercent}%
            </span>
          </div>
        </div>

        <div class="flex shrink-0 items-center justify-between">
          <span class="font-medium">{t.floatingAutoDimLabel}</span>
          <SquishSwitch
            size="sm"
            checked={settingsStore.value.floating.autoDimInReader}
            ariaLabel={t.floatingAutoDimLabel}
            onchange={(checked) => {
              settingsStore.update((d) => (d.floating.autoDimInReader = checked));
            }}
          />
        </div>
      {/if}

      <button
        type="button"
        onclick={() => openFullSettings("floating")}
        class="gpu-press inline-flex shrink-0 items-center justify-center gap-1.5 rounded-xl border px-3 py-2 font-medium hover:opacity-85"
        style="border-color: var(--border); background-color: var(--surface-subtle); color: var(--accent);"
      >
        <ExternalLink size={12} />
        <span>{t.openFullSettingsBtn}</span>
      </button>
    </div>
  {/if}

  <!-- Reading Viewport Container -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    bind:this={scrollEl}
    onscroll={handleScroll}
    onwheel={handleWheel}
    onpointerdown={handlePointerDown}
    onpointermove={handlePointerMove}
    onpointerup={handlePointerUp}
    onpointercancel={handlePointerUp}
    oncontextmenu={handleViewportContextMenu}
    onclick={handleViewportClick}
    ontouchstart={wakeFloatingControls}
    class={isHorizontal
      ? "reader-no-scrollbar relative flex h-full w-full select-none overflow-hidden overscroll-none"
      : "reader-no-scrollbar relative h-full w-full select-none overflow-y-auto overscroll-none"}
    style={zoomPinching
      ? "touch-action: none;"
      : isHorizontal
        ? zoomScale > 1.01
          ? zoomPanning
            ? "touch-action: none; cursor: grabbing;"
            : "touch-action: none; cursor: grab;"
          : horizontalDragging
            ? "touch-action: none; cursor: grabbing;"
            : "touch-action: pan-y;"
        : zoomScale > 1.01
          ? zoomPanning
            ? "touch-action: none; cursor: grabbing;"
            : "touch-action: none; cursor: grab;"
          : "touch-action: pan-y;"}
  >
    {#if loadingInitial}
      <div
        class="flex h-full w-full flex-col items-center justify-center gap-3 text-sm"
        style="color: var(--text-muted);"
      >
        <LatticeLoader size="lg" label={t.loadingPages} showTimer />
      </div>
    {:else if errorState}
      <div class="flex h-full w-full flex-col items-center justify-center gap-4 px-4 py-6">
        <div class="w-full max-w-xl">
          <SourceDiagnosticCard
            error={errorState}
            fallbackUrl={chapterUrl}
            verifying={verifyingBlocked}
            {loggingIn}
            onRetry={() => void loadInitial(chapterUrl, initialScrollRatio, initialPageIndex)}
            onVerify={(u) => void handleManualVerify(u)}
            onLogin={(u) => void handleBrowserLogin(u)}
            onOpenExternal={(u) => void handleOpenExternalUrl(u)}
            onOpenNetworkSettings={() => router.goSettings("network")}
          />
        </div>
        <button
          type="button"
          onclick={exitReader}
          class="gpu-press rounded-xl border px-4 py-2 text-xs font-medium"
          style="border-color: var(--border); background-color: var(--surface);"
        >
          {t.back}
        </button>
      </div>
    {:else if isHorizontal && activeChapter}
      {#snippet endOfChapterCard()}
        <!-- End-of-Chapter Interstitial Summary Card -->
        <div
          class="relative z-20 mx-auto flex max-h-[calc(100%-2rem-var(--safe-top)-var(--safe-bottom))] w-full max-w-md flex-col gap-4 overflow-y-auto overscroll-contain rounded-2xl border p-5 text-center shadow-xl sm:p-6"
          style="background-color: var(--surface); border-color: var(--border); color: var(--text);"
        >
          <div class="flex flex-col items-center gap-2">
            <div
              class="flex h-11 w-11 items-center justify-center rounded-2xl"
              style="background-color: var(--accent-soft); color: var(--accent);"
            >
              <CheckCircle2 size={24} />
            </div>
            <span
              class="rounded-full px-2.5 py-0.5 text-[11px] font-semibold uppercase tracking-wide"
              style="background-color: var(--accent-soft); color: var(--accent);"
            >
              {t.endCardFinishedLabel}
            </span>
            <h2 class="line-clamp-2 text-base font-bold sm:text-lg">
              {activeChapter.name}
            </h2>
            {#if manga}
              <div class="line-clamp-1 text-xs font-medium" style="color: var(--text-muted);">
                {manga.title}
              </div>
            {/if}
            <div class="text-[11px]" style="color: var(--text-muted);">
              {t.endCardPagesSummary(totalPages)}
            </div>
          </div>

          <!-- Generic Metadata & Credits Placeholders -->
          <div
            class="flex flex-col gap-1.5 rounded-xl border p-3 text-left text-xs"
            style="background-color: var(--surface-subtle); border-color: var(--border);"
          >
            <div
              class="text-[10px] font-semibold uppercase tracking-wider"
              style="color: var(--text-muted);"
            >
              {t.endCardMetadataHeading}
            </div>
            <div class="flex items-baseline justify-between gap-2">
              <span style="color: var(--text-muted);">{t.mangaAuthorsLabel}</span>
              <span class="truncate font-medium">
                {manga && manga.authors.length > 0 ? manga.authors.join(", ") : "—"}
              </span>
            </div>
            <div class="flex items-baseline justify-between gap-2">
              <span style="color: var(--text-muted);">{t.endCardSourceLabel}</span>
              <span class="truncate font-medium">{manga?.site ?? "—"}</span>
            </div>
            <div class="flex items-baseline justify-between gap-2">
              <span style="color: var(--text-muted);">{t.endCardCreditsLabel}</span>
              <span class="truncate text-right" style="color: var(--text-muted);">
                {t.endCardCreditsPlaceholder}
              </span>
            </div>
          </div>

          <!-- Prominent Chapter Navigation Actions -->
          <div class="flex flex-col gap-2">
            {#if activeChapter.next}
              <button
                type="button"
                onclick={(e) => {
                  e.stopPropagation();
                  if (activeChapter?.next) jumpToChapter(activeChapter.next);
                }}
                class="gpu-press inline-flex w-full items-center justify-center gap-2 rounded-xl px-5 py-3 text-sm font-semibold shadow-sm hover:opacity-90"
                style="background-color: var(--accent); color: var(--accent-fg);"
              >
                <span>{t.nextChapter}</span>
                <ChevronRight size={17} />
              </button>
            {:else}
              <div
                class="rounded-xl border px-4 py-2.5 text-xs font-medium"
                style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text-muted);"
              >
                {t.reachedLatest}
              </div>
            {/if}

            <div class="grid grid-cols-2 gap-2">
              <button
                type="button"
                disabled={!activeChapter.prev}
                onclick={(e) => {
                  e.stopPropagation();
                  if (activeChapter?.prev) jumpToChapter(activeChapter.prev);
                }}
                class="gpu-press inline-flex items-center justify-center gap-1.5 rounded-xl border px-3 py-2 text-xs font-semibold hover:opacity-85 disabled:opacity-40"
                style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
              >
                <ChevronLeft size={14} />
                <span class="truncate">{t.prevChapter}</span>
              </button>

              <button
                type="button"
                onclick={(e) => {
                  e.stopPropagation();
                  stepHorizontalGroup(-1, false);
                }}
                class="gpu-press inline-flex items-center justify-center gap-1.5 rounded-xl border px-3 py-2 text-xs font-medium hover:opacity-85"
                style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text-muted);"
              >
                <span class="truncate">{t.endCardReturnLastPage}</span>
                <ChevronRight size={14} />
              </button>
            </div>
          </div>
        </div>
      {/snippet}

      <!-- ISOLATED HORIZONTAL READING MODE: LTR/RTL Center-Focus Peek Track + Optional Seamless 0px Join -->
      <div
        bind:this={horizontalStageEl}
        class="relative h-full w-full overflow-hidden"
        style={horizontalZoomStyle}
      >
        {#if isHorizontalSeamless}
          <!-- Seamless Joined Pages Track (True 0px edge-to-edge touching between pages & spreads) -->
          <div
            bind:this={seamlessTrackEl}
            class="flex h-full w-max items-center"
            style="flex-direction: {isRtl ? 'row-reverse' : 'row'}; gap: 0px; {seamlessTrackStyle}"
          >
            {#each horizontalGroups as group (`${activeChapter.url}:hseam:${group.groupIdx}`)}
              {@const isActive = group.groupIdx === activeHorizontalSlideIdx}
              <div
                data-hslide-idx={group.groupIdx}
                data-hslide-active={isActive ? "true" : "false"}
                data-group-key={`${activeChapter.url}:hg:${group.groupIdx}`}
                class="relative flex h-full shrink-0 items-center"
                style="flex-direction: {isRtl ? 'row-reverse' : 'row'}; gap: 0px; opacity: {isActive
                  ? 1
                  : hCfg.dimUnfocused
                    ? 0.3
                    : 1}; transition: opacity 300ms cubic-bezier(0.22, 1, 0.36, 1);"
              >
                {#each group.items as entry (`${activeChapter.url}:${entry.pageIdx}`)}
                  <div class="relative flex h-full shrink-0 items-center justify-center">
                    <PageImage
                      token={entry.token}
                      index={entry.pageIdx}
                      pageKey={`${activeChapter.url}:${entry.pageIdx}`}
                      loadMode={computePageLoadMode(
                        entry.pageIdx,
                        focusedPageIdx,
                        readerSettings.preloadImages,
                        true
                      )}
                      horizontal={true}
                      continuousHorizontal={true}
                      pageGapPx={0}
                      snapPage={false}
                      snapStopAlways={false}
                      groupPageCount={1}
                      skipBroken={behavior.skipBrokenImages}
                      bubbleZoomEnabled={aiBubbleZoom}
                      translationEnabled={aiTranslateOn}
                      peekOriginal={aiPeekOriginal}
                      {activeBubbleKey}
                      onImageLoaded={(info) => handlePageImageLoaded(activeChapter.url, info)}
                    />
                    <span
                      class="pointer-events-none absolute left-2.5 z-10 rounded-md px-2 py-0.5 text-[11px] font-medium tabular-nums select-none backdrop-blur-sm"
                      style="bottom: max(0.75rem, var(--safe-bottom)); background-color: rgba(12, 12, 16, 0.62); color: rgba(255, 255, 255, 0.88);"
                    >
                      {t.pageAlt(entry.pageIdx + 1)}
                    </span>
                  </div>
                {/each}
              </div>
            {/each}

            <div
              data-hslide-idx={horizontalEndSlideIdx}
              data-hslide-active={horizontalAtEnd ? "true" : "false"}
              class="flex h-full shrink-0 items-center justify-center px-6"
              style="width: min(86dvw, 28rem); opacity: {horizontalAtEnd
                ? 1
                : hCfg.dimUnfocused
                  ? 0.3
                  : 1}; transition: opacity 300ms cubic-bezier(0.22, 1, 0.36, 1);"
            >
              {@render endOfChapterCard()}
            </div>
          </div>
        {:else}
          <!-- Separated Center-Focus Peek Track -->
          <div class="relative h-full w-full" style={horizontalTrackStyle}>
            {#each visibleHorizontalSlides as slide (`${activeChapter.url}:hslide:${slide.slideIdx}`)}
              <div
                data-hslide-idx={slide.slideIdx}
                data-hslide-active={slide.isActive ? "true" : "false"}
                class="absolute top-0 bottom-0 flex items-center justify-center py-2"
                style="left: {slide.leftCss}; width: {horizontalTrackMetrics.slotWidth}px; opacity: {slide.isActive
                  ? 1
                  : hCfg.dimUnfocused
                    ? 0.3
                    : 1}; transition: opacity 300ms cubic-bezier(0.22, 1, 0.36, 1);"
              >
                {#if slide.isEndCard}
                  {@render endOfChapterCard()}
                {:else if slide.group}
                  <!-- Page Container (Respects LTR/RTL Spread Order + Subtle Bottom-Left Page Badge) -->
                  <div
                    dir={isRtl ? "rtl" : "ltr"}
                    data-group-key={`${activeChapter.url}:hg:${slide.group.groupIdx}`}
                    class="relative flex h-full w-full items-center justify-center"
                    style="gap: {hCfg.pageGapPx}px;"
                  >
                    {#each slide.group.items as entry (`${activeChapter.url}:${entry.pageIdx}`)}
                      <div
                        class="relative flex h-full items-center justify-center"
                        style={slide.group.items.length > 1
                          ? `max-width: calc((100% - ${(slide.group.items.length - 1) * hCfg.pageGapPx}px) / ${slide.group.items.length});`
                          : "width: 100%; max-width: 100%;"}
                      >
                        <PageImage
                          token={entry.token}
                          index={entry.pageIdx}
                          pageKey={`${activeChapter.url}:${entry.pageIdx}`}
                          loadMode={slide.isActive ? "high" : "preload"}
                          horizontal={true}
                          continuousHorizontal={false}
                          pageGapPx={hCfg.pageGapPx}
                          snapPage={false}
                          snapStopAlways={false}
                          groupPageCount={1}
                          skipBroken={behavior.skipBrokenImages}
                          bubbleZoomEnabled={aiBubbleZoom}
                          translationEnabled={aiTranslateOn}
                          peekOriginal={aiPeekOriginal}
                          {activeBubbleKey}
                          onImageLoaded={(info) => handlePageImageLoaded(activeChapter.url, info)}
                        />
                        <!-- Subtle Bottom-Left Page Badge -->
                        <span
                          class="pointer-events-none absolute left-2.5 z-10 rounded-md px-2 py-0.5 text-[11px] font-medium tabular-nums select-none backdrop-blur-sm"
                          style="bottom: max(0.75rem, var(--safe-bottom)); background-color: rgba(12, 12, 16, 0.62); color: rgba(255, 255, 255, 0.88);"
                        >
                          {t.pageAlt(entry.pageIdx + 1)}
                        </span>
                      </div>
                    {/each}
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <!-- Scoped 3-Zone Invisible Interactive Hitboxes (Horizontal Mode Only; clears HUD top/bottom bars when visible) -->
      {#if zoomScale <= 1.01 && !horizontalAtEnd}
        <div
          class="pointer-events-none absolute right-0 left-0 z-10 flex"
          style="top: {toolbarVisible ? 'calc(3.75rem + var(--safe-top))' : '0px'}; bottom: {toolbarVisible
            ? 'calc(4rem + var(--safe-bottom))'
            : '0px'};"
        >
          <!-- Left Zone (~27%): RTL = Next Page, LTR = Previous Page -->
          <div
            role="button"
            tabindex="-1"
            aria-label={isRtl ? t.zoneNextPageLabel : t.zonePrevPageLabel}
            onclick={handleHorizontalLeftZoneClick}
            class="pointer-events-auto h-full w-[27%] cursor-pointer"
          ></div>
          <!-- Center Zone (~46%): Toggle Top & Bottom HUD Toolbars -->
          <div
            role="button"
            tabindex="-1"
            aria-label={t.zoneToggleHudLabel}
            onclick={handleHorizontalCenterZoneClick}
            class="pointer-events-auto h-full flex-1 cursor-default"
          ></div>
          <!-- Right Zone (~27%): RTL = Previous Page, LTR = Next Page -->
          <div
            role="button"
            tabindex="-1"
            aria-label={isRtl ? t.zonePrevPageLabel : t.zoneNextPageLabel}
            onclick={handleHorizontalRightZoneClick}
            class="pointer-events-auto h-full w-[27%] cursor-pointer"
          ></div>
        </div>
      {:else if zoomScale <= 1.01 && horizontalAtEnd}
        <!-- On End-of-Chapter card: keep card buttons clickable; peeking last-page edge returns to final page -->
        <div
          role="button"
          tabindex="-1"
          aria-label={t.zonePrevPageLabel}
          onclick={isRtl ? handleHorizontalRightZoneClick : handleHorizontalLeftZoneClick}
          class="absolute z-10 w-[20%] cursor-pointer {isRtl ? 'right-0' : 'left-0'}"
          style="top: {toolbarVisible ? 'calc(3.75rem + var(--safe-top))' : '0px'}; bottom: {toolbarVisible
            ? 'calc(4rem + var(--safe-bottom))'
            : '0px'};"
        ></div>
      {/if}
    {:else}
      <!-- VERTICAL READING MODE (Continuous Vertical Scroll + Chapter Dividers) -->
      <div
        bind:this={verticalContentEl}
        class="mx-auto flex w-full max-w-[900px] flex-col items-center pt-14"
        style="padding-left: max({behavior.verticalMarginPx}px, var(--safe-left)); padding-right: max({behavior.verticalMarginPx}px, var(--safe-right)); padding-bottom: calc(7rem + {verticalZoomExtraBottomPx}px); {verticalZoomStyle}"
      >
        {#each strip as chap, chapIdx (chap.url)}
          {@const isActiveChap = normUrl(chap.url) === normUrl(activeUrl)}
          <section data-chapter-url={chap.url} class="flex w-full flex-col">
            {#if chapIdx > 0}
              <!-- Continuous Chapter Divider -->
              <div
                class="my-7 flex flex-col items-center gap-1.5 rounded-xl border px-4 py-3 text-center"
                style="background-color: var(--surface); border-color: var(--border);"
              >
                <div class="text-[11px]" style="color: var(--text-muted);">
                  {t.chapterDividerFinished(strip[chapIdx - 1]?.name ?? "")}
                </div>
                <div class="flex w-full items-center gap-3 text-xs font-semibold">
                  <div class="h-px flex-1" style="background-color: var(--border);"></div>
                  <span style="color: var(--accent);">{chap.name}</span>
                  <div class="h-px flex-1" style="background-color: var(--border);"></div>
                </div>
              </div>
            {/if}

            <div class="flex w-full flex-col" style="gap: {effectiveVerticalGapPx}px;">
              {#each chap.images as token, imgIdx (`${chap.url}:${imgIdx}`)}
                {@const pKey = `${chap.url}:${imgIdx}`}
                <PageImage
                  {token}
                  index={imgIdx}
                  pageKey={pKey}
                  loadMode={computePageLoadMode(
                    imgIdx,
                    focusedPageIdx,
                    readerSettings.preloadImages,
                    isActiveChap
                  )}
                  dimBrightness={pageDimBrightness(pKey)}
                  skipBroken={behavior.skipBrokenImages}
                  bubbleZoomEnabled={aiBubbleZoom}
                  translationEnabled={aiTranslateOn}
                  peekOriginal={aiPeekOriginal}
                  {activeBubbleKey}
                  onImageLoaded={(info) => handlePageImageLoaded(chap.url, info)}
                />
              {/each}
            </div>
          </section>
        {/each}

        <div
          class="mt-10 flex min-h-14 w-full flex-col items-center justify-center gap-3 px-4"
          style="overflow-anchor: none;"
        >
          {#if appendingNext}
            <div
              in:viewReveal|local={{ duration: 150, y: 4 }}
              class="flex items-center gap-2 text-xs"
              style="color: var(--text-muted);"
            >
              <LatticeLoader size="sm" label={t.loadingNextChapter} inline />
            </div>
          {:else if lastChapter?.next}
            <div in:viewReveal|local={{ duration: 150, y: 4 }}>
              <button
                type="button"
                onclick={() => {
                  if (lastChapter?.next) {
                    jumpToChapter(lastChapter.next);
                  }
                }}
                class="gpu-press inline-flex min-h-[44px] items-center gap-2 rounded-2xl px-6 py-3 text-sm font-semibold shadow-sm hover:opacity-90"
                style="background-color: var(--accent); color: var(--accent-fg);"
              >
                <span>{t.nextChapter}</span>
                <ChevronRight size={17} />
              </button>
            </div>
          {:else}
            <div
              in:viewReveal|local={{ duration: 150, y: 4 }}
              class="rounded-xl border px-5 py-3 text-xs font-medium"
              style="background-color: var(--surface); border-color: var(--border); color: var(--text-muted);"
            >
              {t.reachedLatest}
            </div>
          {/if}
        </div>
      </div>
    {/if}
  </div>

  <!-- Reader Page Image Action Sheet (Touch Long-Press) / Context Menu (Desktop Right-Click) -->
  {#if imageMenuTarget}
    {@const menu = imageMenuTarget}
    {@const useBottomSheet = isCompactMobile || !menu.isContextMenu}
    {@const clampedLeft =
      typeof window !== "undefined"
        ? Math.max(12, Math.min(window.innerWidth - 268, menu.clientX))
        : menu.clientX}
    {@const clampedTop =
      typeof window !== "undefined"
        ? Math.max(12, Math.min(window.innerHeight - 168, menu.clientY))
        : menu.clientY}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      transition:backdropFade|local
      onclick={() => {
        if (!imageActionBusy) imageMenuTarget = null;
      }}
      oncontextmenu={(e) => {
        e.preventDefault();
        if (!imageActionBusy) imageMenuTarget = null;
      }}
      class="fixed inset-0 z-50 {useBottomSheet ? 'modal-backdrop' : 'bg-transparent'}"
    ></div>

    {#if useBottomSheet}
      <div
        transition:drawerSlide|local={{ side: "bottom", distance: 48, duration: 200 }}
        role="menu"
        tabindex="-1"
        onpointerdown={(e) => e.stopPropagation()}
        class="glass-chrome fixed right-0 bottom-0 left-0 z-50 flex w-full flex-col gap-2.5 rounded-t-3xl border-t px-4 pt-1 pb-4"
        style="padding-bottom: max(1.1rem, var(--safe-bottom)); padding-left: max(1rem, var(--safe-left)); padding-right: max(1rem, var(--safe-right)); border-color: var(--border); box-shadow: var(--shadow-lg);"
      >
        <div class="sheet-handle" aria-hidden="true"></div>
        <div class="flex items-center justify-between gap-2 pb-0.5">
          <span class="text-sm font-bold">{t.readerImgActionTitle(menu.pageIndex + 1)}</span>
          <button
            type="button"
            disabled={imageActionBusy !== null}
            onclick={() => (imageMenuTarget = null)}
            class="gpu-press flex h-8 w-8 items-center justify-center rounded-xl opacity-75 hover:opacity-100"
            aria-label={t.cancel}
          >
            <X size={16} />
          </button>
        </div>

        <button
          type="button"
          role="menuitem"
          disabled={imageActionBusy !== null}
          onclick={() => void handleDownloadImageAction()}
          class="gpu-press flex min-h-[52px] w-full items-center gap-3 rounded-2xl border px-3.5 py-2.5 text-left disabled:opacity-55"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          <span
            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl"
            style="background-color: var(--accent-soft); color: var(--accent);"
          >
            {#if imageActionBusy === "download"}
              <Loader2 size={17} class="animate-spin" />
            {:else}
              <Download size={17} />
            {/if}
          </span>
          <div class="min-w-0">
            <div class="text-sm font-semibold">{t.readerImgDownloadBtn}</div>
            <div class="truncate text-[11px]" style="color: var(--text-muted);">
              {t.readerImgDownloadDesc}
            </div>
          </div>
        </button>

        <button
          type="button"
          role="menuitem"
          disabled={imageActionBusy !== null}
          onclick={() => void handleSetCoverAction()}
          class="gpu-press flex min-h-[52px] w-full items-center gap-3 rounded-2xl border px-3.5 py-2.5 text-left disabled:opacity-55"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          <span
            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl"
            style="background-color: var(--accent-soft); color: var(--accent);"
          >
            {#if imageActionBusy === "cover"}
              <Loader2 size={17} class="animate-spin" />
            {:else}
              <ImageIcon size={17} />
            {/if}
          </span>
          <div class="min-w-0">
            <div class="text-sm font-semibold">{t.readerImgSetCoverBtn}</div>
            <div class="truncate text-[11px]" style="color: var(--text-muted);">
              {t.readerImgSetCoverDesc}
            </div>
          </div>
        </button>
      </div>
    {:else}
      <div
        transition:popoverPop|local={{ duration: 140, y: -4 }}
        role="menu"
        tabindex="-1"
        onpointerdown={(e) => e.stopPropagation()}
        oncontextmenu={(e) => e.preventDefault()}
        class="glass-chrome fixed z-50 flex w-64 flex-col gap-1 rounded-2xl border p-1.5 shadow-xl"
        style="left: {clampedLeft}px; top: {clampedTop}px; border-color: var(--border); box-shadow: var(--shadow-lg);"
      >
        <div
          class="px-2.5 py-1 text-[11px] font-semibold"
          style="color: var(--text-muted);"
        >
          {t.readerImgActionTitle(menu.pageIndex + 1)}
        </div>
        <button
          type="button"
          role="menuitem"
          disabled={imageActionBusy !== null}
          onclick={() => void handleDownloadImageAction()}
          class="gpu-press flex min-h-[38px] w-full items-center gap-2.5 rounded-xl px-2.5 py-2 text-left text-xs font-semibold hover:opacity-90 disabled:opacity-55"
          style="background-color: var(--surface-subtle); color: var(--text);"
        >
          {#if imageActionBusy === "download"}
            <Loader2 size={14} class="shrink-0 animate-spin" style="color: var(--accent);" />
          {:else}
            <Download size={14} class="shrink-0" style="color: var(--accent);" />
          {/if}
          <span>{t.readerImgDownloadBtn}</span>
        </button>
        <button
          type="button"
          role="menuitem"
          disabled={imageActionBusy !== null}
          onclick={() => void handleSetCoverAction()}
          class="gpu-press flex min-h-[38px] w-full items-center gap-2.5 rounded-xl px-2.5 py-2 text-left text-xs font-semibold hover:opacity-90 disabled:opacity-55"
          style="background-color: var(--surface-subtle); color: var(--text);"
        >
          {#if imageActionBusy === "cover"}
            <Loader2 size={14} class="shrink-0 animate-spin" style="color: var(--accent);" />
          {:else}
            <ImageIcon size={14} class="shrink-0" style="color: var(--accent);" />
          {/if}
          <span>{t.readerImgSetCoverBtn}</span>
        </button>
      </div>
    {/if}
  {/if}

  <!-- Bottom Reading Progress Bar with Seek Support & One-Handed Thumb Clustering -->
  {#if !loadingInitial && !errorState && activeChapter && totalPages > 0}
    <footer
      class="glass-chrome fixed right-0 bottom-0 left-0 z-30 flex items-center gap-2.5 border-t px-3 py-2.5 transition-[opacity,transform] duration-200 sm:gap-3 sm:px-5"
      style="padding-bottom: calc(0.625rem + var(--safe-bottom)); padding-left: max(0.75rem, var(--safe-left)); padding-right: max(0.75rem, var(--safe-right)); border-color: var(--border); opacity: {toolbarVisible
        ? 1
        : 0}; pointer-events: {toolbarVisible ? 'auto' : 'none'}; transform: translate3d(0, {toolbarVisible
        ? '0px'
        : '8px'}, 0);"
    >
      {#if oneHandedMode === "left"}
        <div class="flex shrink-0 items-center gap-1.5">
          <button
            type="button"
            disabled={!activeChapter.prev}
            onclick={() => activeChapter?.prev && jumpToChapter(activeChapter.prev)}
            class="btn-3d btn-3d-surface gpu-press touch-target flex min-h-[38px] min-w-[38px] items-center justify-center rounded-xl border disabled:opacity-35"
            style="border-color: var(--border); background-color: var(--surface);"
            title={t.prevChapterTooltip}
            aria-label={t.prevChapterTooltip}
          >
            <ChevronLeft size={16} />
          </button>
          <button
            type="button"
            disabled={!activeChapter.next}
            onclick={() => activeChapter?.next && jumpToChapter(activeChapter.next)}
            class="btn-3d btn-3d-surface gpu-press touch-target flex min-h-[38px] min-w-[38px] items-center justify-center rounded-xl border disabled:opacity-35"
            style="border-color: var(--border); background-color: var(--surface);"
            title={t.nextChapterTooltip}
            aria-label={t.nextChapterTooltip}
          >
            <ChevronRight size={16} />
          </button>
        </div>
      {:else if oneHandedMode === "default"}
        <button
          type="button"
          disabled={!activeChapter.prev}
          onclick={() => activeChapter?.prev && jumpToChapter(activeChapter.prev)}
          class="btn-3d btn-3d-surface gpu-press touch-target flex min-h-[38px] min-w-[38px] items-center justify-center rounded-xl border disabled:opacity-35"
          style="border-color: var(--border); background-color: var(--surface);"
          title={t.prevChapterTooltip}
          aria-label={t.prevChapterTooltip}
        >
          <ChevronLeft size={16} />
        </button>
      {/if}

      {#if isHorizontal && manga && manga.chapters.length > 0}
        {@const selectedChapUrl =
          manga.chapters.find((c) => normUrl(c.url) === normUrl(activeChapter.url))?.url ??
          activeChapter.url}
        <select
          value={selectedChapUrl}
          onchange={(e) => jumpToChapter(e.currentTarget.value)}
          aria-label={t.quickChapterJump}
          class="min-h-[38px] max-w-[7.5rem] shrink-0 truncate rounded-xl border px-2.5 py-1 text-xs font-medium outline-none sm:max-w-[12rem]"
          style="border-color: var(--border); background-color: var(--surface); color: var(--text);"
        >
          {#each [...manga.chapters].reverse() as ch (ch.url)}
            <option value={ch.url}>{ch.name}</option>
          {/each}
        </select>
      {/if}

      <div class="flex min-w-0 flex-1 items-center gap-3">
        <span class="shrink-0 text-xs font-semibold tabular-nums" style="color: var(--accent);">
          {focusedPageIdx + 1} / {totalPages}
        </span>
        <input
          type="range"
          min="1"
          max={totalPages}
          step="1"
          value={Math.min(totalPages, focusedPageIdx + 1)}
          onpointerdown={() => {
            scrubbing = true;
          }}
          onpointerup={() => {
            scrubbing = false;
            scheduleToolbarAutoHide();
          }}
          onpointercancel={() => {
            scrubbing = false;
            scheduleToolbarAutoHide();
          }}
          oninput={(e) => {
            seekToPage(Number(e.currentTarget.value) - 1);
          }}
          onchange={() => {
            scrubbing = false;
            scheduleToolbarAutoHide();
          }}
          class="h-1.5 w-full cursor-pointer"
          aria-label={t.readerProgressSeek}
        />
        <span class="hidden shrink-0 text-[11px] tabular-nums sm:inline" style="color: var(--text-muted);">
          {Math.round(((focusedPageIdx + 1) / Math.max(1, totalPages)) * 100)}%
        </span>
      </div>

      {#if oneHandedMode === "right"}
        <div class="flex shrink-0 items-center gap-1.5">
          <button
            type="button"
            disabled={!activeChapter.prev}
            onclick={() => activeChapter?.prev && jumpToChapter(activeChapter.prev)}
            class="btn-3d btn-3d-surface gpu-press touch-target flex min-h-[38px] min-w-[38px] items-center justify-center rounded-xl border disabled:opacity-35"
            style="border-color: var(--border); background-color: var(--surface);"
            title={t.prevChapterTooltip}
            aria-label={t.prevChapterTooltip}
          >
            <ChevronLeft size={16} />
          </button>
          <button
            type="button"
            disabled={!activeChapter.next}
            onclick={() => activeChapter?.next && jumpToChapter(activeChapter.next)}
            class="btn-3d btn-3d-surface gpu-press touch-target flex min-h-[38px] min-w-[38px] items-center justify-center rounded-xl border disabled:opacity-35"
            style="border-color: var(--border); background-color: var(--surface);"
            title={t.nextChapterTooltip}
            aria-label={t.nextChapterTooltip}
          >
            <ChevronRight size={16} />
          </button>
        </div>
      {:else if oneHandedMode === "default"}
        <button
          type="button"
          disabled={!activeChapter.next}
          onclick={() => activeChapter?.next && jumpToChapter(activeChapter.next)}
          class="btn-3d btn-3d-surface gpu-press touch-target flex min-h-[38px] min-w-[38px] items-center justify-center rounded-xl border disabled:opacity-35"
          style="border-color: var(--border); background-color: var(--surface);"
          title={t.nextChapterTooltip}
          aria-label={t.nextChapterTooltip}
        >
          <ChevronRight size={16} />
        </button>
      {/if}
    </footer>
  {/if}

  {#if activeBubbleCallout && activeBubbleCalloutRegions.length > 0}
    <BubbleMagnifierCard
      regions={activeBubbleCalloutRegions}
      regionIndex={activeBubbleCallout.regionIndex}
      imgEl={activeBubbleCallout.imgEl}
      showTranslation={aiTranslateOn && !aiPeekOriginal}
      onSelectIndex={(nextIdx) => {
        if (!activeBubbleCallout) return;
        const reg = activeBubbleCalloutRegions[nextIdx];
        activeBubbleCallout = { ...activeBubbleCallout, regionIndex: nextIdx };
        activeBubbleKey = reg
          ? `${activeBubbleCallout.token}:${nextIdx}:${reg.bbox[0]},${reg.bbox[1]},${reg.bbox[2]},${reg.bbox[3]}`
          : null;
      }}
      onClose={() => {
        activeBubbleCallout = null;
        activeBubbleKey = null;
      }}
    />
  {/if}

  {#if !loadingInitial && !errorState && activeChapter}
    <ReaderAiPill
      bind:open={aiPillOpen}
      {oneHandedMode}
      {toolbarVisible}
      activePageToken={activeChapter.images[focusedPageIdx] ?? ""}
      onOpenAccessibilitySettings={() => openFullSettings("accessibility")}
    />
  {/if}

  <FloatingControls
    screen="reader"
    inReader={true}
    showScrollTop={showScrollTopBtn}
    onScrollTop={() => scrollToStartOfContainer(true)}
    onLongPressScrollTop={() => {
      quickFloatConfig = "scrollToTop";
      quickSettingsOpen = false;
      openQuickCat = null;
      chapterJumpOpen = false;
      aiPillOpen = false;
      wakeFloatingControls();
    }}
    onToggleReaderSettings={toggleQuickSettings}
    onLongPressReaderSettings={() => openFullSettings("reader")}
    readerSettingsActive={quickSettingsOpen || quickFloatConfig !== null}
    onCycleOrientationLock={cycleOrientationLock}
    onLongPressOrientationLock={() => {
      quickFloatConfig = "orientationLock";
      quickSettingsOpen = false;
      openQuickCat = null;
      chapterJumpOpen = false;
      aiPillOpen = false;
      wakeFloatingControls();
    }}
    orientationLock={readerSettings.orientationLock}
    dimmed={controlsDimmed}
    fadedOut={!toolbarVisible &&
      !quickSettingsOpen &&
      quickFloatConfig === null &&
      !chapterJumpOpen &&
      !aiPillOpen}
  />
</div>

<style>
  @media (max-height: 520px) {
    .reader-quick-popover {
      top: calc(0.65rem + var(--safe-top)) !important;
      max-height: calc(100dvh - 1.3rem - var(--safe-top) - var(--safe-bottom)) !important;
      padding: 0.65rem;
      gap: 0.45rem;
    }
  }
</style>
