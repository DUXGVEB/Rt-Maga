<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    ArrowLeft,
    Bookmark,
    Check,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    ChevronUp,
    ExternalLink,
    List,
    Loader2,
    Maximize,
    Minimize,
    RotateCcw,
    Search,
    SlidersHorizontal,
    Sparkles,
    X,
    ZoomOut,
  } from "lucide-svelte";
  import { tick, untrack } from "svelte";
  import { api, formatAppError, imgProxyUrl } from "../lib/api";
  import FloatingControls from "../lib/components/FloatingControls.svelte";
  import PageImage from "../lib/components/PageImage.svelte";
  import {
    clampZoomPan,
    computeAnchoredZoom,
    computeNavStepCooldownMs,
    computePageLoadMode,
    computeVerticalSpeedMultiplier,
    detectWebtoonFromDimensions,
    focusBrightness,
    partitionPages,
    type PageDimensionSample,
    type RectBox,
  } from "../lib/readerLayout";
  import { deviceEnv } from "../lib/stores/deviceEnv.svelte";
  import { router } from "../lib/stores/router.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import type {
    ChapterView,
    FloatingEdge,
    FloatingScreenKey,
    HorizontalDirection,
    MangaView,
    OrientationLockMode,
    ReaderBg,
    ReaderModeOverride,
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
  let errorState = $state<{ title: string; detail: string; blockedUrl?: string } | null>(null);
  let toolbarVisible = $state(true);
  let quickSettingsOpen = $state(false);
  let openQuickCat = $state<"layout" | "display" | "controls" | null>(null);
  let quickFloatConfig = $state<"scrollToTop" | "orientationLock" | null>(null);
  let chapterJumpOpen = $state(false);
  let chapterJumpQuery = $state("");
  let isFullscreen = $state(false);
  let showScrollTopBtn = $state(false);
  let controlsDimmed = $state(false);
  let chapterDetectedWebtoon = $state(false);

  // Double-tap / double-click anchored zoom state (`1` = normal, `2.0` = zoomed).
  const ZOOM_TARGET_SCALE = 2.0;
  let zoomScale = $state(1);
  let zoomTx = $state(0);
  let zoomTy = $state(0);
  let zoomPanning = $state(false);
  let zoomContentRect: RectBox | null = null;
  let verticalZoomExtraBottomPx = $state(0);

  // Instant horizontal page/group navigation state (ZERO carousel/sliding animation).
  let activeGroupIdx = $state(0);
  let horizontalAtEnd = $state(false);

  // Viewport focus tracking.
  let focusedKeys = $state<Set<string>>(new Set());
  let focusedPageIdx = $state(0);

  let scrollEl = $state<HTMLDivElement | null>(null);
  let verticalContentEl = $state<HTMLDivElement | null>(null);
  let horizontalStageEl = $state<HTMLDivElement | null>(null);
  let lastScrollOffset = 0;
  let prefetched = new Map<string, ChapterView>();
  let preloadedImageTokens = new Set<string>();
  let saveTimer: number | null = null;
  let idleDimTimer: number | null = null;
  let toolbarAutoHideTimer: number | null = null;
  let singleTapTimer: number | null = null;
  let orientationRestoreTimer: number | null = null;
  let verticalAnimRaf: number | null = null;
  let verticalAnimTargetTop = 0;
  let lastTapAt = 0;
  let lastHorizontalStepAt = 0;
  let wheelDeltaAccum = 0;
  let wheelResetTimer: number | null = null;
  let pointerDownX = 0;
  let pointerDownY = 0;
  let pointerDownAt = 0;
  let activePointerId: number | null = null;
  let panStartX = 0;
  let panStartY = 0;
  let panOriginTx = 0;
  let panOriginTy = 0;
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

  const readerSettings = $derived(settingsStore.value.reader);
  const vCfg = $derived(readerSettings.vertical);
  const hCfg = $derived(readerSettings.horizontal);
  const behavior = $derived(readerSettings.behavior);
  const t = $derived(settingsStore.t);

  const effectiveOverride = $derived<ReaderModeOverride>(manga?.readerModeOverride ?? null);

  const isAutoWebtoonActive = $derived(
    effectiveOverride === null &&
      readerSettings.autoWebtoonDetect &&
      Boolean(manga?.detectedWebtoon || chapterDetectedWebtoon)
  );

  const effectiveOrientation = $derived<ReadingOrientation>(
    effectiveOverride === "horizontal"
      ? "horizontal"
      : effectiveOverride === "vertical_strip" || effectiveOverride === "vertical_grouped"
        ? "vertical"
        : isAutoWebtoonActive
          ? "vertical"
          : readerSettings.orientation
  );

  const effectiveNoGap = $derived(isAutoWebtoonActive || vCfg.noGapMode);

  const effectiveVerticalGapPx = $derived(effectiveNoGap ? 0 : vCfg.pageGapPx);

  const isHorizontal = $derived(effectiveOrientation === "horizontal");
  const isRtl = $derived(isHorizontal && hCfg.direction === "rtl");

  const activeChapter = $derived(
    strip.find((c) => normUrl(c.url) === normUrl(activeUrl)) ?? strip[strip.length - 1] ?? null
  );

  const lastChapter = $derived(strip[strip.length - 1] ?? null);
  const totalPages = $derived(activeChapter?.images.length ?? 0);
  const horizontalGroups = $derived(
    activeChapter ? partitionPages(activeChapter.images, hCfg.pagesPerGroup) : []
  );
  const currentHorizontalGroup = $derived(
    !horizontalAtEnd && horizontalGroups.length > 0
      ? horizontalGroups[Math.max(0, Math.min(horizontalGroups.length - 1, activeGroupIdx))] ?? null
      : null
  );

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

  const horizontalZoomStyle = $derived(
    zoomScale > 1.01
      ? `transform: translate3d(${zoomTx}px, ${zoomTy}px, 0) scale(${zoomScale}); transform-origin: 0 0; transition: ${zoomPanning ? "none" : "transform 160ms cubic-bezier(0.2, 0.8, 0.2, 1)"};`
      : "transform: none; transform-origin: 0 0; transition: transform 140ms ease;"
  );

  const verticalZoomStyle = $derived(
    zoomScale > 1.01
      ? `transform: translate3d(${zoomTx}px, 0px, 0) scale(${zoomScale}); transform-origin: 0 0; transition: ${zoomPanning ? "none" : "transform 160ms cubic-bezier(0.2, 0.8, 0.2, 1)"};`
      : "transform: none; transform-origin: 0 0; transition: transform 140ms ease;"
  );

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

  function resetZoom(anchorClientY?: number) {
    const wasZoomed = zoomScale > 1.01;
    const prevScale = zoomScale;
    zoomScale = 1;
    zoomTx = 0;
    zoomTy = 0;
    zoomPanning = false;
    zoomContentRect = null;

    if (wasZoomed && !isHorizontal && scrollEl) {
      const vpRect = scrollEl.getBoundingClientRect();
      const vy = anchorClientY !== undefined ? anchorClientY - vpRect.top : vpRect.height * 0.5;
      const unscaledDocY = (scrollEl.scrollTop + vy) / Math.max(1, prevScale);
      verticalZoomExtraBottomPx = 0;
      const targetTop = Math.max(0, Math.round(unscaledDocY - vy));
      suppressAutoHideUntil = Date.now() + 260;
      window.requestAnimationFrame(() => {
        if (scrollEl) {
          scrollEl.scrollTop = targetTop;
          lastScrollOffset = targetTop;
        }
      });
    } else {
      verticalZoomExtraBottomPx = 0;
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
        if (!quickSettingsOpen && quickFloatConfig === null && !chapterJumpOpen) {
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
    if (normUrl(chapUrl) !== normUrl(activeUrl)) return;

    if (
      !webtoonDecidedForChapter &&
      readerSettings.autoWebtoonDetect &&
      effectiveOverride === null &&
      info.index < 8
    ) {
      dimensionSamples.set(info.index, info);
      if (dimensionSamples.size >= 3) {
        const result = detectWebtoonFromDimensions(Array.from(dimensionSamples.values()));
        if (result.decided) {
          webtoonDecidedForChapter = true;
          if (result.isWebtoon && !manga?.detectedWebtoon) {
            chapterDetectedWebtoon = true;
            if (manga) {
              manga = { ...manga, detectedWebtoon: true };
            }
            void api.markMangaDetectedWebtoon(mangaId).catch(() => {});
            toasts.push(t.toastWebtoonDetected, "info", 2600);
          }
        }
      }
    }
  }

  function updateFocusNow() {
    if (strip.length === 0 || loadingInitial) return;

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
    if (flush) {
      void flushPendingProgress(keepSessionActive);
    } else {
      if (saveTimer !== null) {
        window.clearTimeout(saveTimer);
        saveTimer = null;
      }
      pendingProgress = null;
    }
    for (const timer of [
      restoreTimer,
      idleDimTimer,
      toolbarAutoHideTimer,
      singleTapTimer,
      orientationRestoreTimer,
      wheelResetTimer,
    ]) {
      if (timer !== null) window.clearTimeout(timer);
    }
    restoreTimer = null;
    idleDimTimer = null;
    toolbarAutoHideTimer = null;
    singleTapTimer = null;
    orientationRestoreTimer = null;
    wheelResetTimer = null;
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
      horizontalAtEnd = false;
      activeGroupIdx = 0;
      focusedPageIdx = 0;
      showScrollTopBtn = false;
      lastScrollOffset = 0;
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
      const perGroup = Math.max(1, hCfg.pagesPerGroup);
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
      const perGroup = Math.max(1, hCfg.pagesPerGroup);
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
    markedReadSet.clear();
    dimensionSamples.clear();
    webtoonDecidedForChapter = false;
    chapterDetectedWebtoon = false;
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

    const scheduleViewportAnchorRestore = () => {
      if (!scrollEl || loadingInitial || strip.length === 0) return;
      resetZoom();
      suppressAutoHideUntil = Date.now() + 650;
      wakeFloatingControls();
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
        const pageEl = scrollEl.querySelector<HTMLElement>(
          `[data-page-key="${CSS.escape(`${anchorChap}:${anchorPage}`)}"]`
        );
        if (pageEl) {
          if (isHorizontal) {
            pageEl.scrollIntoView({ behavior: "instant", block: "nearest", inline: "center" });
            lastScrollOffset = Math.abs(scrollEl.scrollLeft);
          } else {
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
        }
        updateFocusNow();
      };

      orientationRestoreTimer = window.setTimeout(() => {
        orientationRestoreTimer = null;
        restoreNow();
      }, 220);
    };

    const onOrientationChanged = () => {
      toolbarVisible = true;
      lastViewportW = window.innerWidth;
      lastViewportH = window.innerHeight;
      scheduleViewportAnchorRestore();
    };

    const onWindowResize = () => {
      const w = window.innerWidth;
      const h = window.innerHeight;
      if (Math.abs(w - lastViewportW) < 24 && Math.abs(h - lastViewportH) < 24) return;
      lastViewportW = w;
      lastViewportH = h;
      scheduleViewportAnchorRestore();
    };

    document.addEventListener("visibilitychange", onVisibilityChange);
    window.addEventListener("rtmaga:orientation-changed", onOrientationChanged);
    window.addEventListener("resize", onWindowResize);

    return () => {
      closingReader = true;
      captureCurrentProgressNow(false);
      clearPendingTimers(true, false);
      void api.clearReaderSession().catch(() => {});
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

  let prevLayoutSig = "";
  $effect(() => {
    const sig = `${effectiveOrientation}|${hCfg.pagesPerGroup}|${hCfg.direction}`;
    if (!prevLayoutSig) {
      prevLayoutSig = sig;
      return;
    }
    if (sig === prevLayoutSig) return;
    prevLayoutSig = sig;

    const anchorChap = untrack(() => activeUrl);
    const anchorPage = untrack(() => focusedPageIdx);
    if (!anchorChap || untrack(() => loadingInitial || strip.length === 0)) return;

    void (async () => {
      resetZoom();
      if (untrack(() => isHorizontal && strip.length > 1)) {
        const current = untrack(() => strip.find((c) => normUrl(c.url) === normUrl(anchorChap)));
        if (current) {
          strip = [current];
        }
      }
      await tick();
      if (isHorizontal) {
        horizontalAtEnd = false;
        const perGroup = Math.max(1, hCfg.pagesPerGroup);
        activeGroupIdx = Math.max(
          0,
          Math.min(Math.max(0, horizontalGroups.length - 1), Math.floor(anchorPage / perGroup))
        );
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
    const ro = new ResizeObserver(() => scheduleFocusUpdate());
    ro.observe(el);
    return () => ro.disconnect();
  });

  async function prefetchNext(nextUrl: string) {
    const normNext = normUrl(nextUrl);
    if (prefetched.has(normNext) || strip.some((c) => normUrl(c.url) === normNext)) return;
    const seq = loadSeq;
    try {
      const data = await api.getChapter(nextUrl, mangaId);
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
        prefetched.get(normNext) ?? (await api.getChapter(tail.next, mangaId));
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

      strip = shouldTrimOldest ? [...strip.slice(1), nextChap] : [...strip, nextChap];
      appendingNext = false;

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
      if (deltaStep > 0) {
        // Advancing forward hides reader overlays quietly like scrolling downward.
        toolbarVisible = false;
        quickSettingsOpen = false;
        openQuickCat = null;
        quickFloatConfig = null;
        chapterJumpOpen = false;
      }
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
      if (activeChapter.next) {
        horizontalAtEnd = true;
        toolbarVisible = true;
      } else {
        horizontalAtEnd = true;
        toolbarVisible = true;
      }
    } else if (deltaStep < 0 && activeChapter.prev) {
      jumpToChapter(activeChapter.prev);
    }
  }

  function measureZoomTargetRect(target: EventTarget | null): RectBox {
    if (!scrollEl) {
      return { left: 0, top: 0, width: window.innerWidth, height: window.innerHeight };
    }
    const vpRect = scrollEl.getBoundingClientRect();
    if (isHorizontal) {
      const targetEl = target instanceof HTMLElement ? target.closest("img") : null;
      const imgs = targetEl
        ? [targetEl]
        : Array.from(horizontalStageEl?.querySelectorAll<HTMLImageElement>("img") ?? []);
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
            left: (minL - zoomTx) / Math.max(1, zoomScale),
            top: (minT - zoomTy) / Math.max(1, zoomScale),
            width: (maxR - minL) / Math.max(1, zoomScale),
            height: (maxB - minT) / Math.max(1, zoomScale),
          };
        }
      }
      return { left: 0, top: 0, width: vpRect.width, height: vpRect.height };
    }

    if (verticalContentEl) {
      const cRect = verticalContentEl.getBoundingClientRect();
      return {
        left: (cRect.left - vpRect.left - zoomTx) / Math.max(1, zoomScale),
        top: 0,
        width: cRect.width / Math.max(1, zoomScale),
        height: vpRect.height,
      };
    }
    return { left: 0, top: 0, width: vpRect.width, height: vpRect.height };
  }

  function handlePointerDown(e: PointerEvent) {
    if (e.pointerType === "mouse" && e.button !== 0) return;
    if (e.target instanceof HTMLElement && e.target.closest("button, a, select, input")) {
      return;
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
      if (isHorizontal && scrollEl) {
        try {
          scrollEl.setPointerCapture(e.pointerId);
        } catch {
          // Ignore capture errors on synthetic events.
        }
      }
    }
  }

  function handlePointerMove(e: PointerEvent) {
    if (!zoomPanning || activePointerId !== e.pointerId || !scrollEl || zoomScale <= 1.01) return;
    const dx = e.clientX - panStartX;
    const dy = e.clientY - panStartY;
    if (Math.hypot(dx, dy) > 4) {
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
      rect
    );
    zoomTx = clamped.tx;
    if (isHorizontal) {
      zoomTy = clamped.ty;
    }
  }

  function handlePointerUp(e: PointerEvent) {
    if (activePointerId !== e.pointerId) return;
    const dx = e.clientX - pointerDownX;
    const dy = e.clientY - pointerDownY;
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
    activePointerId = null;

    // Horizontal mode intentional horizontal swipe -> instant page replacement (NO slide animation).
    if (
      isHorizontal &&
      zoomScale <= 1.01 &&
      !didPanDrag &&
      elapsed < 650 &&
      Math.abs(dx) >= 56 &&
      Math.abs(dx) > Math.abs(dy) * 1.45
    ) {
      const swipedLeft = dx < 0;
      const step: 1 | -1 = swipedLeft ? (isRtl ? -1 : 1) : isRtl ? 1 : -1;
      stepHorizontalGroup(step, false);
    }
  }

  function executeSingleTap(clientX: number) {
    if (isHorizontal && readerSettings.tapZonesEnabled && scrollEl && zoomScale <= 1.01) {
      const rect = scrollEl.getBoundingClientRect();
      const relX = (clientX - rect.left) / Math.max(1, rect.width);
      if (relX <= 0.25) {
        stepHorizontalGroup(isRtl ? 1 : -1, false);
        return;
      }
      if (relX >= 0.75) {
        stepHorizontalGroup(isRtl ? -1 : 1, false);
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

    // Anchored zoom toggle (`1x <-> 2.0x`) centered precisely on tapped/clicked coordinates.
    if (zoomScale > 1.01) {
      resetZoom(clientY);
      return;
    }
    if (!scrollEl) return;

    const vpRect = scrollEl.getBoundingClientRect();
    const pointerX = clientX - vpRect.left;
    const pointerY = clientY - vpRect.top;
    const contentRect = measureZoomTargetRect(target);
    zoomContentRect = contentRect;

    const next = computeAnchoredZoom(
      pointerX,
      pointerY,
      { scale: zoomScale, tx: zoomTx, ty: zoomTy },
      ZOOM_TARGET_SCALE,
      vpRect.width,
      vpRect.height,
      contentRect
    );

    if (isHorizontal) {
      zoomScale = next.scale;
      zoomTx = next.tx;
      zoomTy = next.ty;
    } else {
      const docYBefore = scrollEl.scrollTop + pointerY;
      const baseHeight = verticalContentEl?.offsetHeight ?? scrollEl.scrollHeight;
      verticalZoomExtraBottomPx = Math.max(0, Math.round(baseHeight * (next.scale - 1)));
      zoomScale = next.scale;
      zoomTx = next.tx;
      zoomTy = 0;
      const targetScrollTop = Math.max(0, Math.round(docYBefore * next.scale - pointerY));
      suppressAutoHideUntil = Date.now() + 300;
      window.requestAnimationFrame(() => {
        if (scrollEl) {
          scrollEl.scrollTop = targetScrollTop;
          lastScrollOffset = targetScrollTop;
        }
      });
    }
  }

  function handleViewportClick(e: MouseEvent) {
    wakeFloatingControls();
    if (didPanDrag) {
      didPanDrag = false;
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
    // Ignore drags/scroll gestures (> 10px movement).
    if (Math.hypot(e.clientX - pointerDownX, e.clientY - pointerDownY) > 10) {
      return;
    }

    const now = Date.now();
    const doubleTapEnabled = behavior.doubleTapAction !== "disabled";

    if (doubleTapEnabled && now - lastTapAt <= 260) {
      lastTapAt = 0;
      if (singleTapTimer !== null) {
        window.clearTimeout(singleTapTimer);
        singleTapTimer = null;
      }
      executeDoubleTap(e.clientX, e.clientY, e.target);
      return;
    }

    lastTapAt = now;
    if (!doubleTapEnabled) {
      executeSingleTap(e.clientX);
    } else {
      const tapX = e.clientX;
      singleTapTimer = window.setTimeout(() => {
        singleTapTimer = null;
        executeSingleTap(tapX);
      }, 260);
    }
  }

  async function handleManualVerify() {
    if (!errorState?.blockedUrl || verifyingBlocked) return;
    verifyingBlocked = true;
    try {
      toasts.push(t.toastOpeningVerify, "info", 4500);
      await api.openManualVerification(errorState.blockedUrl);
      toasts.push(t.toastVerifyDone, "success");
      await loadInitial(chapterUrl, initialScrollRatio, initialPageIndex);
    } catch (err) {
      errorState = formatAppError(err, settingsStore.value.language);
    } finally {
      verifyingBlocked = false;
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
          rect
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
    if (isHorizontal || !scrollEl || strip.length === 0 || loadingInitial) return;

    wakeFloatingControls();
    scheduleFocusUpdate();

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

    const sections = Array.from(scrollEl.querySelectorAll<HTMLElement>("[data-chapter-url]"));
    const viewportMid = top + scrollEl.clientHeight * 0.35;
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
      const chap = strip.find((c) => normUrl(c.url) === normUrl(chapUrl));
      if (chap) {
        const chapterChanged = normUrl(activeUrl) !== normUrl(chap.url);
        activeUrl = chap.url;
        if (chapterChanged) {
          updateFocusNow();
          if (behavior.autoMarkReadOnOpen) {
            markChapterReadLocalAndRemote(chap.url);
          }
        }
        const secHeight = Math.max(1, currentSection.offsetHeight - scrollEl.clientHeight);
        const ratio = Math.min(1, Math.max(0, (top - currentSection.offsetTop) / secHeight));

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
    settingsStore.update((d) => {
      d.reader.orientationLock = next;
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
    const opening = !quickSettingsOpen;
    quickSettingsOpen = opening;
    openQuickCat = null;
    if (opening) {
      chapterJumpOpen = false;
      quickFloatConfig = null;
    }
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
    captureCurrentProgressNow(false);
    await flushPendingProgress(false);
    await api.clearReaderSession().catch(() => {});
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
      if (chapterJumpOpen) chapterJumpOpen = false;
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
      if (e.key === "ArrowRight") {
        e.preventDefault();
        stepHorizontalGroup(isRtl ? -1 : 1, e.repeat);
      } else if (e.key === "ArrowLeft") {
        e.preventDefault();
        stepHorizontalGroup(isRtl ? 1 : -1, e.repeat);
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

  async function applyPerMangaOverride(mode: ReaderModeOverride) {
    if (manga) {
      manga = { ...manga, readerModeOverride: mode };
    }
    try {
      await api.setMangaReaderOverride(mangaId, mode);
    } catch {
      // Ignore error.
    }
  }

  function setOrientation(o: ReadingOrientation) {
    settingsStore.update((d) => {
      d.reader.orientation = o;
    });
    const nextOverride: ReaderModeOverride = o === "horizontal" ? "horizontal" : "vertical_strip";
    void applyPerMangaOverride(nextOverride);
  }

  function resetToAutoMode() {
    void applyPerMangaOverride(null);
    toasts.push(t.readerOverrideResetToast, "info", 2000);
  }

  function setPagesPerGroup(n: number) {
    const clamped = Math.max(1, Math.min(8, n));
    settingsStore.update((d) => {
      d.reader.horizontal.pagesPerGroup = clamped;
    });
  }

  function setHorizontalDirection(dir: HorizontalDirection) {
    settingsStore.update((d) => {
      d.reader.horizontal.direction = dir;
    });
  }

  function setNavigationSpeed(speed: number) {
    const clamped = Math.max(1, Math.min(10, Math.round(speed)));
    settingsStore.update((d) => {
      d.reader.behavior.navigationSpeed = clamped;
    }, 80);
  }

  function toggleDim() {
    settingsStore.update((d) => {
      if (isHorizontal) {
        d.reader.horizontal.dimUnfocused = !d.reader.horizontal.dimUnfocused;
      } else {
        d.reader.vertical.dimUnfocused = !d.reader.vertical.dimUnfocused;
      }
    });
  }

  function setDimLevel(val: number) {
    settingsStore.update((d) => {
      if (isHorizontal) {
        d.reader.horizontal.dimLevel = val;
      } else {
        d.reader.vertical.dimLevel = val;
      }
    }, 80);
  }

  function toggleAutoNext() {
    settingsStore.update((d) => {
      d.reader.autoNextChapter = !d.reader.autoNextChapter;
    });
  }

  function toggleNoGap() {
    const turningOff = effectiveNoGap;
    if (turningOff) {
      settingsStore.update((d) => {
        d.reader.vertical.noGapMode = false;
        if (d.reader.vertical.pageGapPx === 0) {
          d.reader.vertical.pageGapPx = 12;
          d.reader.gapPx = 12;
        }
      });
      if (isAutoWebtoonActive) {
        void applyPerMangaOverride("vertical_strip");
      }
    } else {
      settingsStore.update((d) => {
        d.reader.vertical.noGapMode = true;
      });
    }
  }

  function setVerticalGapPx(val: number) {
    const clamped = Math.max(0, Math.min(48, Math.round(val)));
    settingsStore.update((d) => {
      d.reader.vertical.noGapMode = false;
      d.reader.vertical.pageGapPx = clamped;
      d.reader.gapPx = clamped;
    }, 100);
    if (isAutoWebtoonActive && clamped > 0) {
      void applyPerMangaOverride("vertical_strip");
    }
  }

  function setReaderBg(bg: ReaderBg) {
    settingsStore.update((d) => {
      d.reader.readerBg = bg;
    });
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="relative flex h-full w-full flex-col overflow-hidden"
  style="background-color: {readerBackdrop()};"
>
  <!-- Top Auto-Hide Toolbar (smooth fade, never auto-opened by upward scroll) -->
  <header
    class="fixed top-0 right-0 left-0 z-30 flex items-center justify-between gap-2 border-b px-3 py-2 backdrop-blur-md transition-[opacity,transform] duration-200 sm:px-4"
    style="padding-top: calc(0.5rem + var(--safe-top)); padding-left: max(0.75rem, var(--safe-left)); padding-right: max(0.75rem, var(--safe-right)); background-color: color-mix(in srgb, var(--surface) 88%, transparent); border-color: var(--border); opacity: {toolbarVisible
      ? 1
      : 0}; pointer-events: {toolbarVisible ? 'auto' : 'none'}; transform: translateY({toolbarVisible
      ? '0px'
      : '-6px'});"
  >
    <div class="flex min-w-0 items-center gap-2">
      <button
        type="button"
        onclick={exitReader}
        class="inline-flex items-center gap-1 rounded-lg px-2 py-1.5 text-sm hover:opacity-80"
        title={t.backTooltip}
      >
        <ArrowLeft size={17} />
        <span class="hidden sm:inline">{t.back}</span>
      </button>

      <div class="min-w-0 border-l pl-2.5" style="border-color: var(--border);">
        <div class="flex items-center gap-1.5">
          <div class="truncate text-sm font-semibold">{manga?.title ?? t.readingFallback}</div>
          {#if isAutoWebtoonActive}
            <span
              class="hidden shrink-0 items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px] font-medium sm:inline-flex"
              style="background-color: var(--accent-soft); color: var(--accent);"
              title={t.autoWebtoonBadge}
            >
              <Sparkles size={10} />
              Webtoon
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

    <div class="flex shrink-0 items-center gap-1">
      {#if zoomScale > 1.01}
        <button
          type="button"
          onclick={() => resetZoom()}
          class="inline-flex items-center gap-1 rounded-lg border px-2 py-1.5 text-xs font-medium"
          style="border-color: var(--accent); background-color: var(--accent-soft); color: var(--accent);"
          title={t.readerResetZoom}
        >
          <ZoomOut size={14} />
          <span>1x</span>
        </button>
      {/if}

      {#if manga && manga.chapters.length > 0}
        <button
          type="button"
          onclick={() => {
            chapterJumpOpen = !chapterJumpOpen;
            quickSettingsOpen = false;
            openQuickCat = null;
            quickFloatConfig = null;
            wakeFloatingControls();
          }}
          class="inline-flex items-center gap-1 rounded-lg border px-2 py-1.5 text-xs font-medium"
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
          class="rounded-lg border p-1.5"
          style="border-color: var(--border); background-color: {manga.bookmarked
            ? 'var(--accent-soft)'
            : 'var(--surface)'}; color: {manga.bookmarked ? 'var(--accent)' : 'var(--text)'};"
          title={manga.bookmarked ? t.bookmarkRemove : t.bookmarkAdd}
        >
          <Bookmark size={16} fill={manga.bookmarked ? "currentColor" : "none"} />
        </button>
      {/if}

      <button
        type="button"
        onclick={toggleQuickSettings}
        class="rounded-lg border p-1.5"
        style="border-color: var(--border); background-color: {quickSettingsOpen
          ? 'var(--accent-soft)'
          : 'var(--surface)'}; color: {quickSettingsOpen ? 'var(--accent)' : 'var(--text)'};"
        title={t.quickSettingsTooltip}
      >
        <SlidersHorizontal size={16} />
      </button>

      {#if !deviceEnv.isAndroid}
        <button
          type="button"
          onclick={toggleFullscreen}
          class="rounded-lg border p-1.5"
          style="border-color: var(--border); background-color: var(--surface);"
          title={t.fullscreenTooltip}
        >
          {#if isFullscreen}
            <Minimize size={16} />
          {:else}
            <Maximize size={16} />
          {/if}
        </button>
      {/if}
    </div>
  </header>

  <!-- Quick Chapter Jump Popover (with dimmed read chapters) -->
  {#if chapterJumpOpen && manga}
    <div
      class="fixed z-40 flex max-h-[calc(100dvh-4.5rem-var(--safe-top)-var(--safe-bottom))] w-80 max-w-[calc(100vw-1.5rem-var(--safe-left)-var(--safe-right))] flex-col gap-2.5 overscroll-contain rounded-2xl border p-3.5 text-xs shadow-xl"
      style="top: calc(3.5rem + var(--safe-top)); right: max(0.75rem, var(--safe-right)); background-color: var(--surface); border-color: var(--border);"
    >
      <div class="flex items-center justify-between font-semibold">
        <span>{t.quickChapterJump}</span>
        <span style="color: var(--text-muted);">{t.chapterCount(manga.chapters.length)}</span>
      </div>
      <div
        class="flex shrink-0 items-center gap-2 rounded-xl border px-2.5 py-1.5"
        style="background-color: var(--surface-subtle); border-color: var(--border);"
      >
        <Search size={13} style="color: var(--text-muted);" />
        <input
          type="text"
          bind:value={chapterJumpQuery}
          placeholder={t.filterChapters}
          class="w-full bg-transparent text-xs outline-none"
        />
      </div>
      <div class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto pr-0.5">
        {#each filteredJumpChapters() as ch (ch.url)}
          {@const isCurrent = normUrl(ch.url) === normUrl(activeUrl)}
          <button
            type="button"
            onclick={() => jumpToChapter(ch.url)}
            class="flex items-center justify-between gap-2 rounded-lg px-2.5 py-2 text-left transition-colors hover:brightness-95"
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
              <Check size={13} class="shrink-0 opacity-75" />
            {/if}
          </button>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Quick Reader Settings Popover (Collapsible Categories, Collapsed by Default) -->
  {#if quickSettingsOpen}
    <div
      class="fixed z-40 flex max-h-[calc(100dvh-4.5rem-var(--safe-top)-var(--safe-bottom))] w-80 max-w-[calc(100vw-1.5rem-var(--safe-left)-var(--safe-right))] flex-col gap-2.5 overflow-y-auto overscroll-contain rounded-2xl border p-3.5 text-xs shadow-xl"
      style="top: calc(3.5rem + var(--safe-top)); right: max(0.75rem, var(--safe-right)); background-color: var(--surface); border-color: var(--border);"
    >
      <div class="flex items-center justify-between gap-2 pb-0.5">
        <span class="font-semibold">{t.readerSection}</span>
        <button
          type="button"
          onclick={() => openFullSettings("reader")}
          class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium hover:opacity-85"
          style="background-color: var(--accent-soft); color: var(--accent);"
        >
          <ExternalLink size={11} />
          <span>{t.openFullSettingsBtn}</span>
        </button>
      </div>

      {#if isAutoWebtoonActive || effectiveOverride !== null}
        <div
          class="flex items-center justify-between gap-2 rounded-xl border px-3 py-2"
          style="background-color: var(--surface-subtle); border-color: var(--border);"
        >
          <span class="font-medium" style="color: var(--text);">
            {effectiveOverride !== null ? t.readerOverrideActive : t.autoWebtoonBadge}
          </span>
          {#if effectiveOverride !== null}
            <button
              type="button"
              onclick={resetToAutoMode}
              class="inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] font-medium"
              style="background-color: var(--accent-soft); color: var(--accent);"
            >
              <RotateCcw size={11} />
              {t.readerResetToAuto}
            </button>
          {/if}
        </div>
      {/if}

      <!-- Category 1: Layout & Spacing -->
      <div class="overflow-hidden rounded-xl border" style="border-color: var(--border); background-color: var(--surface-subtle);">
        <button
          type="button"
          onclick={() => (openQuickCat = openQuickCat === "layout" ? null : "layout")}
          aria-expanded={openQuickCat === "layout"}
          class="flex w-full items-center justify-between gap-2 px-3 py-2.5 text-left font-medium"
        >
          <div class="min-w-0">
            <div class="font-semibold">{t.readerCatLayout}</div>
            <div class="truncate text-[10px]" style="color: var(--text-muted);">
              {isHorizontal
                ? `${t.modeHorizontal} · ${hCfg.direction.toUpperCase()} · ${hCfg.pagesPerGroup}x`
                : `${t.modeVertical} · ${effectiveNoGap ? t.noGapModeLabel : `${effectiveVerticalGapPx}px`}`}
            </div>
          </div>
          {#if openQuickCat === "layout"}
            <ChevronUp size={15} class="shrink-0 opacity-70" />
          {:else}
            <ChevronDown size={15} class="shrink-0 opacity-70" />
          {/if}
        </button>

        {#if openQuickCat === "layout"}
          <div
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
                    class="rounded-lg border py-1.5 font-medium"
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
                      class="rounded-lg border py-1.5 font-medium"
                      style="border-color: {hCfg.direction === val
                        ? 'var(--accent)'
                        : 'var(--border)'}; background-color: {hCfg.direction === val
                        ? 'var(--accent-soft)'
                        : 'var(--surface-subtle)'}; color: {hCfg.direction === val
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
                  <span style="color: var(--text-muted);">{hCfg.pagesPerGroup}</span>
                </div>
                <div class="grid grid-cols-4 gap-1.5">
                  {#each [1, 2, 3] as n (n)}
                    <button
                      type="button"
                      onclick={() => setPagesPerGroup(n)}
                      class="rounded-lg border py-1 font-medium"
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
                <input
                  type="checkbox"
                  checked={effectiveNoGap}
                  onchange={toggleNoGap}
                  class="h-4 w-4 accent-rose-500"
                />
              </div>

              {#if !effectiveNoGap}
                <div class="flex flex-col gap-1">
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
                    class="w-full accent-rose-500"
                  />
                </div>
              {/if}

              <div class="flex items-center justify-between">
                <span class="font-medium">{t.autoAppendNext}</span>
                <input
                  type="checkbox"
                  checked={readerSettings.autoNextChapter}
                  onchange={toggleAutoNext}
                  class="h-4 w-4 accent-rose-500"
                />
              </div>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Category 2: Display & Visuals -->
      <div class="overflow-hidden rounded-xl border" style="border-color: var(--border); background-color: var(--surface-subtle);">
        <button
          type="button"
          onclick={() => (openQuickCat = openQuickCat === "display" ? null : "display")}
          aria-expanded={openQuickCat === "display"}
          class="flex w-full items-center justify-between gap-2 px-3 py-2.5 text-left font-medium"
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
          {#if openQuickCat === "display"}
            <ChevronUp size={15} class="shrink-0 opacity-70" />
          {:else}
            <ChevronDown size={15} class="shrink-0 opacity-70" />
          {/if}
        </button>

        {#if openQuickCat === "display"}
          <div
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
                    class="rounded-lg border py-1.5 font-medium"
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
                <input
                  type="checkbox"
                  checked={isHorizontal ? hCfg.dimUnfocused : vCfg.dimUnfocused}
                  onchange={toggleDim}
                  class="h-4 w-4 accent-rose-500"
                />
              </div>
              {#if isHorizontal ? hCfg.dimUnfocused : vCfg.dimUnfocused}
                {@const dimPct = isHorizontal ? hCfg.dimLevel : vCfg.dimLevel}
                <div class="flex items-center gap-2">
                  <input
                    type="range"
                    min="0"
                    max="100"
                    step="5"
                    value={dimPct}
                    oninput={(e) => setDimLevel(Number(e.currentTarget.value))}
                    class="w-full accent-rose-500"
                  />
                  <span class="w-9 text-right" style="color: var(--text-muted);">{dimPct}%</span>
                </div>
              {/if}
            </div>
          </div>
        {/if}
      </div>

      <!-- Category 3: Controls & Behavior -->
      <div class="overflow-hidden rounded-xl border" style="border-color: var(--border); background-color: var(--surface-subtle);">
        <button
          type="button"
          onclick={() => (openQuickCat = openQuickCat === "controls" ? null : "controls")}
          aria-expanded={openQuickCat === "controls"}
          class="flex w-full items-center justify-between gap-2 px-3 py-2.5 text-left font-medium"
        >
          <div class="min-w-0">
            <div class="font-semibold">{t.readerCatControls}</div>
            <div class="truncate text-[10px]" style="color: var(--text-muted);">
              {behavior.keepScreenOn ? t.keepScreenOnLabel : t.skipBrokenImagesLabel}
            </div>
          </div>
          {#if openQuickCat === "controls"}
            <ChevronUp size={15} class="shrink-0 opacity-70" />
          {:else}
            <ChevronDown size={15} class="shrink-0 opacity-70" />
          {/if}
        </button>

        {#if openQuickCat === "controls"}
          <div
            class="flex flex-col gap-3 border-t p-3"
            style="border-color: var(--border); background-color: var(--surface);"
          >
            <div class="flex items-center justify-between">
              <span class="font-medium">{t.keepScreenOnLabel}</span>
              <input
                type="checkbox"
                checked={behavior.keepScreenOn}
                onchange={(e) => {
                  const checked = e.currentTarget.checked;
                  settingsStore.update((d) => (d.reader.behavior.keepScreenOn = checked));
                }}
                class="h-4 w-4 accent-rose-500"
              />
            </div>

            <div class="flex items-center justify-between">
              <span class="font-medium">{t.skipBrokenImagesLabel}</span>
              <input
                type="checkbox"
                checked={behavior.skipBrokenImages}
                onchange={(e) => {
                  const checked = e.currentTarget.checked;
                  settingsStore.update((d) => (d.reader.behavior.skipBrokenImages = checked));
                }}
                class="h-4 w-4 accent-rose-500"
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
                class="w-full accent-rose-500"
              />
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <!-- Long-Press Contextual Configuration Panel (Scroll-to-Top / Screen Rotation) -->
  {#if quickFloatConfig !== null}
    <div
      class="fixed z-40 flex max-h-[calc(100dvh-4.5rem-var(--safe-top)-var(--safe-bottom))] w-80 max-w-[calc(100vw-1.5rem-var(--safe-left)-var(--safe-right))] flex-col gap-3 overflow-y-auto overscroll-contain rounded-2xl border p-4 text-xs shadow-xl"
      style="top: calc(3.5rem + var(--safe-top)); right: max(0.75rem, var(--safe-right)); background-color: var(--surface); border-color: var(--border);"
    >
      <div class="flex items-center justify-between gap-2">
        <span class="font-semibold">
          {quickFloatConfig === "scrollToTop"
            ? t.scrollTopQuickConfigTitle
            : t.orientationQuickConfigTitle}
        </span>
        <button
          type="button"
          onclick={() => (quickFloatConfig = null)}
          class="rounded-lg p-1 opacity-70 hover:opacity-100"
          aria-label={t.cancel}
        >
          <X size={14} />
        </button>
      </div>

      {#if quickFloatConfig === "orientationLock"}
        <div class="flex flex-col gap-1.5">
          <span class="font-medium">{t.orientationCurrentMode}</span>
          <div class="grid grid-cols-3 gap-1.5">
            {#each [["auto", t.orientationAuto], ["portrait", t.orientationPortrait], ["landscape", t.orientationLandscape]] as [val, label] (val)}
              <button
                type="button"
                onclick={() => void setOrientationLockDirect(val as OrientationLockMode)}
                class="rounded-lg border py-1.5 font-medium"
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

        <div class="flex items-center justify-between">
          <span class="font-medium">{t.buttonVisibilityLabel}</span>
          <input
            type="checkbox"
            checked={settingsStore.value.floating.showOrientationLock}
            onchange={(e) => {
              const checked = e.currentTarget.checked;
              settingsStore.update((d) => (d.floating.showOrientationLock = checked));
            }}
            class="h-4 w-4 accent-rose-500"
          />
        </div>

        <div class="flex flex-col gap-1.5">
          <span class="font-medium">{t.floatingPositionLabel}</span>
          <div class="flex items-center gap-2">
            {#each [["left", t.floatingEdgeLeft], ["right", t.floatingEdgeRight]] as [edge, label] (edge)}
              <button
                type="button"
                onclick={() =>
                  settingsStore.update((d) => (d.floating.orientationLock.edge = edge as FloatingEdge))}
                class="rounded-lg border px-2.5 py-1 font-medium"
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
              class="w-full accent-rose-500"
            />
            <span class="w-8 text-right tabular-nums" style="color: var(--text-muted);">
              {settingsStore.value.floating.orientationLock.verticalPercent}%
            </span>
          </div>
        </div>
      {:else}
        <div class="flex flex-col gap-1.5">
          <span class="font-medium">{t.floatingScrollTopScreens}</span>
          <div class="flex flex-wrap gap-1.5">
            {#each allFloatingScreens as screen (screen)}
              {@const active = settingsStore.value.floating.scrollToTopScreens.includes(screen)}
              <button
                type="button"
                onclick={() => toggleScrollTopScreen(screen)}
                class="rounded-lg border px-2.5 py-1 font-medium"
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

        <div class="flex flex-col gap-1.5">
          <span class="font-medium">{t.floatingPositionLabel}</span>
          <div class="flex items-center gap-2">
            {#each [["left", t.floatingEdgeLeft], ["right", t.floatingEdgeRight]] as [edge, label] (edge)}
              <button
                type="button"
                onclick={() =>
                  settingsStore.update((d) => (d.floating.scrollToTop.edge = edge as FloatingEdge))}
                class="rounded-lg border px-2.5 py-1 font-medium"
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
              class="w-full accent-rose-500"
            />
            <span class="w-8 text-right tabular-nums" style="color: var(--text-muted);">
              {settingsStore.value.floating.scrollToTop.verticalPercent}%
            </span>
          </div>
        </div>

        <div class="flex items-center justify-between">
          <span class="font-medium">{t.floatingAutoDimLabel}</span>
          <input
            type="checkbox"
            checked={settingsStore.value.floating.autoDimInReader}
            onchange={(e) => {
              const checked = e.currentTarget.checked;
              settingsStore.update((d) => (d.floating.autoDimInReader = checked));
            }}
            class="h-4 w-4 accent-rose-500"
          />
        </div>
      {/if}

      <button
        type="button"
        onclick={() => openFullSettings("floating")}
        class="inline-flex items-center justify-center gap-1.5 rounded-xl border px-3 py-2 font-medium transition-opacity hover:opacity-85"
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
    onclick={handleViewportClick}
    ontouchstart={wakeFloatingControls}
    class={isHorizontal
      ? "relative flex h-full w-full select-none overflow-hidden"
      : "relative h-full w-full select-none overflow-y-auto"}
    style={isHorizontal
      ? zoomScale > 1.01
        ? zoomPanning
          ? "touch-action: none; cursor: grabbing;"
          : "touch-action: none; cursor: grab;"
        : "touch-action: pan-y;"
      : zoomScale > 1.01
        ? zoomPanning
          ? "touch-action: pan-y; cursor: grabbing;"
          : "touch-action: pan-y; cursor: grab;"
        : "touch-action: pan-y;"}
  >
    {#if loadingInitial}
      <div
        class="flex h-full w-full flex-col items-center justify-center gap-3 text-sm"
        style="color: var(--text-muted);"
      >
        <Loader2 size={28} class="animate-spin" style="color: var(--accent);" />
        <span>{t.loadingPages}</span>
      </div>
    {:else if errorState}
      <div class="flex h-full w-full flex-col items-center justify-center gap-3 px-6 text-center">
        <div class="text-base font-semibold">{errorState.title}</div>
        <p class="max-w-md text-xs" style="color: var(--text-muted);">{errorState.detail}</p>
        <div class="mt-2 flex flex-wrap justify-center gap-2">
          {#if errorState.blockedUrl}
            <button
              type="button"
              disabled={verifyingBlocked}
              onclick={handleManualVerify}
              class="inline-flex items-center gap-1.5 rounded-xl px-4 py-2 text-xs font-medium text-white disabled:opacity-50"
              style="background-color: var(--accent);"
            >
              {#if verifyingBlocked}
                <Loader2 size={14} class="animate-spin" />
                <span>{t.waitingVerification}</span>
              {:else}
                <span>{t.openVerification}</span>
              {/if}
            </button>
          {/if}
          <button
            type="button"
            onclick={() => loadInitial(chapterUrl, initialScrollRatio, initialPageIndex)}
            class="rounded-xl px-4 py-2 text-xs font-medium text-white"
            style="background-color: var(--accent);"
          >
            {t.retry}
          </button>
          <button
            type="button"
            onclick={exitReader}
            class="rounded-xl border px-4 py-2 text-xs font-medium"
            style="border-color: var(--border); background-color: var(--surface);"
          >
            {t.back}
          </button>
        </div>
      </div>
    {:else if isHorizontal && activeChapter}
      <!-- HORIZONTAL READING MODE (Instant In-Place Replacement, ZERO Sliding / Carousel Animation) -->
      <div
        bind:this={horizontalStageEl}
        class="relative flex h-full w-full items-center justify-center overflow-hidden"
        style={horizontalZoomStyle}
      >
        {#if horizontalAtEnd}
          <div class="flex h-full w-full flex-col items-center justify-center gap-3 px-8 text-center">
            {#if activeChapter.next}
              <button
                type="button"
                onclick={() => activeChapter?.next && jumpToChapter(activeChapter.next)}
                class="inline-flex items-center gap-2 rounded-xl px-6 py-3 text-sm font-semibold text-white shadow-sm transition-opacity hover:opacity-90"
                style="background-color: var(--accent);"
              >
                <span>{t.nextChapter}</span>
                <ChevronRight size={17} />
              </button>
            {:else}
              <div
                class="rounded-xl border px-5 py-3 text-xs font-medium"
                style="background-color: var(--surface); border-color: var(--border); color: var(--text-muted);"
              >
                {t.reachedLatest}
              </div>
            {/if}
            {#if horizontalGroups.length > 0}
              <button
                type="button"
                onclick={() => {
                  horizontalAtEnd = false;
                  activeGroupIdx = horizontalGroups.length - 1;
                }}
                class="inline-flex items-center gap-1.5 rounded-xl border px-4 py-2 text-xs font-medium"
                style="background-color: var(--surface); border-color: var(--border); color: var(--text);"
              >
                <ChevronLeft size={14} />
                <span>{t.back}</span>
              </button>
            {/if}
          </div>
        {:else if currentHorizontalGroup}
          <div
            dir={isRtl ? "rtl" : "ltr"}
            data-group-key={`${activeChapter.url}:hg:${currentHorizontalGroup.groupIdx}`}
            class="box-border flex h-full w-full items-center {hCfg.centerGroups
              ? 'justify-center'
              : 'justify-start'}"
            style="padding-left: max(0.5rem, var(--safe-left)); padding-right: max(0.5rem, var(--safe-right)); gap: {hCfg.pageGapPx}px;"
          >
            {#each currentHorizontalGroup.items as entry (`${activeChapter.url}:${entry.pageIdx}`)}
              <PageImage
                token={entry.token}
                index={entry.pageIdx}
                pageKey={`${activeChapter.url}:${entry.pageIdx}`}
                loadMode="high"
                horizontal={true}
                continuousHorizontal={false}
                pageGapPx={hCfg.pageGapPx}
                snapPage={false}
                snapStopAlways={false}
                groupPageCount={currentHorizontalGroup.items.length}
                skipBroken={behavior.skipBrokenImages}
                onImageLoaded={(info) => handlePageImageLoaded(activeChapter.url, info)}
              />
            {/each}
          </div>
        {/if}
      </div>
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
            <div class="flex items-center gap-2 text-xs" style="color: var(--text-muted);">
              <Loader2 size={16} class="animate-spin" style="color: var(--accent);" />
              <span>{t.loadingNextChapter}</span>
            </div>
          {:else if lastChapter?.next}
            <button
              type="button"
              onclick={() => {
                if (lastChapter?.next) {
                  jumpToChapter(lastChapter.next);
                }
              }}
              class="inline-flex items-center gap-2 rounded-xl px-6 py-3 text-sm font-semibold text-white shadow-sm transition-opacity hover:opacity-90"
              style="background-color: var(--accent);"
            >
              <span>{t.nextChapter}</span>
              <ChevronRight size={17} />
            </button>
          {:else}
            <div
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

  <!-- Bottom Reading Progress Bar with Seek Support (smooth fade) -->
  {#if !loadingInitial && !errorState && activeChapter && totalPages > 0}
    <footer
      class="fixed right-0 bottom-0 left-0 z-30 flex items-center gap-3 border-t px-3 py-2.5 backdrop-blur-md transition-[opacity,transform] duration-200 sm:px-5"
      style="padding-bottom: calc(0.625rem + var(--safe-bottom)); padding-left: max(0.75rem, var(--safe-left)); padding-right: max(0.75rem, var(--safe-right)); background-color: color-mix(in srgb, var(--surface) 90%, transparent); border-color: var(--border); opacity: {toolbarVisible
        ? 1
        : 0}; pointer-events: {toolbarVisible ? 'auto' : 'none'}; transform: translateY({toolbarVisible
        ? '0px'
        : '6px'});"
    >
      <button
        type="button"
        disabled={!activeChapter.prev}
        onclick={() => activeChapter?.prev && jumpToChapter(activeChapter.prev)}
        class="rounded-lg border p-1.5 disabled:opacity-35"
        style="border-color: var(--border); background-color: var(--surface);"
        title={t.prevChapterTooltip}
      >
        <ChevronLeft size={16} />
      </button>

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
          class="h-1.5 w-full cursor-pointer accent-rose-500"
          aria-label={t.readerProgressSeek}
        />
        <span class="hidden shrink-0 text-[11px] tabular-nums sm:inline" style="color: var(--text-muted);">
          {Math.round(((focusedPageIdx + 1) / Math.max(1, totalPages)) * 100)}%
        </span>
      </div>

      <button
        type="button"
        disabled={!activeChapter.next}
        onclick={() => activeChapter?.next && jumpToChapter(activeChapter.next)}
        class="rounded-lg border p-1.5 disabled:opacity-35"
        style="border-color: var(--border); background-color: var(--surface);"
        title={t.nextChapterTooltip}
      >
        <ChevronRight size={16} />
      </button>
    </footer>
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
      wakeFloatingControls();
    }}
    orientationLock={readerSettings.orientationLock}
    dimmed={controlsDimmed}
    fadedOut={!toolbarVisible && !quickSettingsOpen && quickFloatConfig === null && !chapterJumpOpen}
  />
</div>
