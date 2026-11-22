<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    ArrowLeft,
    ChevronLeft,
    ChevronRight,
    Loader2,
    Maximize,
    Minimize,
    SlidersHorizontal,
  } from "lucide-svelte";
  import { tick, untrack } from "svelte";
  import { api, formatAppError } from "../lib/api";
  import PageImage from "../lib/components/PageImage.svelte";
  import { focusBrightness, partitionPages } from "../lib/readerLayout";
  import { router } from "../lib/stores/router.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import type {
    ChapterView,
    HorizontalDirection,
    HorizontalSnapMode,
    ImageWidth,
    MangaView,
    ReaderBg,
    ReadingOrientation,
    VerticalLayoutMode,
  } from "../lib/types";

  let {
    mangaId,
    chapterUrl,
    initialScrollRatio = 0,
  }: {
    mangaId: number;
    chapterUrl: string;
    initialScrollRatio?: number;
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
  let isFullscreen = $state(false);

  // Viewport focus tracking (key of focused group or page, plus focused page index for live setting changes).
  let focusedKeys = $state<Set<string>>(new Set());
  let focusedPageIdx = $state(0);

  let scrollEl = $state<HTMLDivElement | null>(null);
  let lastScrollOffset = 0;
  let prefetched = new Map<string, ChapterView>();
  let saveTimer: number | null = null;
  let pendingProgress: { mangaId: number; url: string; name: string; ratio: number } | null = null;
  let restoreTimer: number | null = null;
  let focusRaf: number | null = null;
  let markedReadSet = new Set<string>();
  let restoredInitialScroll = false;
  let isUserScrollingBeforeRestore = false;
  let loadSeq = 0;

  function normUrl(u: string): string {
    return u.endsWith("/") ? u.slice(0, -1) : u;
  }

  const readerSettings = $derived(settingsStore.value.reader);
  const vCfg = $derived(readerSettings.vertical);
  const hCfg = $derived(readerSettings.horizontal);
  const isHorizontal = $derived(readerSettings.orientation === "horizontal");
  const isRtl = $derived(isHorizontal && hCfg.direction === "rtl");
  const isVerticalGrouped = $derived(!isHorizontal && vCfg.layoutMode === "vertical_grouped");
  const t = $derived(settingsStore.t);

  const activeChapter = $derived(
    strip.find((c) => normUrl(c.url) === normUrl(activeUrl)) ?? strip[strip.length - 1] ?? null
  );

  const lastChapter = $derived(strip[strip.length - 1] ?? null);

  const selectChapterValue = $derived(() => {
    if (!manga || !activeUrl) return activeUrl;
    const norm = normUrl(activeUrl);
    return manga.chapters.find((c) => normUrl(c.url) === norm)?.url ?? activeUrl;
  });

  const readerBackdrop = $derived(() => {
    const bg: ReaderBg = readerSettings.readerBg;
    if (bg === "dark") return "#0b0b0d";
    if (bg === "light") return "#f7f7f8";
    return "var(--bg)";
  });

  const contentWidthStyle = $derived(() => {
    const w: ImageWidth = readerSettings.width;
    if (w.mode === "fit_window") return "width: 100%; max-width: 100%;";
    if (w.mode === "percent") return `width: ${w.value}%; max-width: 100%;`;
    return `width: 100%; max-width: ${w.value}px;`;
  });

  const preloadMarginPx = $derived(Math.max(800, readerSettings.preloadImages * 520));

  const scrollContainerSnapStyle = $derived(() => {
    if (isHorizontal) {
      if (hCfg.snapMode === "none") return "scroll-snap-type: none;";
      return "scroll-snap-type: x proximity;";
    }
    if (isVerticalGrouped && vCfg.snapEnabled) {
      return "scroll-snap-type: y proximity;";
    }
    return "scroll-snap-type: none;";
  });

  function isKeyFocused(key: string): boolean {
    if (focusedKeys.size === 0) return true;
    return focusedKeys.has(key);
  }

  function groupDimBrightness(groupKey: string, dimEnabled: boolean, dimLevel: number): number {
    return focusBrightness(isKeyFocused(groupKey), dimEnabled, dimLevel);
  }

  function pageDimBrightness(pageKey: string): number {
    if (isHorizontal || isVerticalGrouped) return 1; // Dimming is applied per group in grouped modes.
    return focusBrightness(isKeyFocused(pageKey), vCfg.dimUnfocused, vCfg.dimLevel);
  }

  function updateFocusNow() {
    if (!scrollEl || strip.length === 0 || loadingInitial) return;
    const scrollRect = scrollEl.getBoundingClientRect();
    const nextFocused = new Set<string>();

    if (isHorizontal) {
      const groups = Array.from(scrollEl.querySelectorAll<HTMLElement>("[data-group-key]"));
      const absLeft = Math.abs(scrollEl.scrollLeft);
      const maxScroll = Math.max(0, scrollEl.scrollWidth - scrollEl.clientWidth);
      let best: HTMLElement | null = null;

      if (groups.length > 0 && absLeft <= 16) {
        best = groups[0];
      } else if (groups.length > 0 && maxScroll > 16 && absLeft >= maxScroll - 16) {
        best = groups[groups.length - 1];
      } else {
        const centerX = scrollRect.left + scrollRect.width / 2;
        let bestDist = Number.POSITIVE_INFINITY;
        for (const el of groups) {
          const r = el.getBoundingClientRect();
          const elCenter = r.left + r.width / 2;
          const dist = Math.abs(elCenter - centerX);
          if (dist < bestDist) {
            bestDist = dist;
            best = el;
          }
        }
      }
      if (best?.dataset.groupKey) {
        nextFocused.add(best.dataset.groupKey);
        const firstPage = best.querySelector<HTMLElement>("[data-page-idx]");
        if (firstPage?.dataset.pageIdx) {
          focusedPageIdx = Number(firstPage.dataset.pageIdx) || 0;
        }
      }
    } else if (isVerticalGrouped) {
      const centerY = scrollRect.top + scrollRect.height * 0.45;
      const groups = Array.from(scrollEl.querySelectorAll<HTMLElement>("[data-group-key]"));
      let best: HTMLElement | null = null;
      let bestDist = Number.POSITIVE_INFINITY;

      for (const el of groups) {
        const r = el.getBoundingClientRect();
        // Keep any group actively crossing the focal center band un-dimmed.
        if (r.top <= centerY && r.bottom >= centerY && el.dataset.groupKey) {
          nextFocused.add(el.dataset.groupKey);
        }
        const elCenter = r.top + r.height / 2;
        const dist = Math.abs(elCenter - centerY);
        if (dist < bestDist) {
          bestDist = dist;
          best = el;
        }
      }
      if (best?.dataset.groupKey) {
        nextFocused.add(best.dataset.groupKey);
        const firstPage = best.querySelector<HTMLElement>("[data-page-idx]");
        if (firstPage?.dataset.pageIdx) {
          focusedPageIdx = Number(firstPage.dataset.pageIdx) || 0;
        }
      }
    } else {
      // Continuous Strip mode: keep images overlapping the focal center band un-dimmed.
      const centerY = scrollRect.top + scrollRect.height * 0.45;
      const pages = Array.from(scrollEl.querySelectorAll<HTMLElement>("[data-page-key]"));
      let best: HTMLElement | null = null;
      let bestDist = Number.POSITIVE_INFINITY;

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
      }
      if (best?.dataset.pageKey) {
        nextFocused.add(best.dataset.pageKey);
        if (best.dataset.pageIdx) {
          focusedPageIdx = Number(best.dataset.pageIdx) || 0;
        }
      }
    }

    // Only replace reactive Set when contents actually changed to avoid unnecessary re-renders.
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

  function flushPendingProgress(): Promise<void> {
    if (saveTimer !== null) {
      window.clearTimeout(saveTimer);
      saveTimer = null;
    }
    if (pendingProgress) {
      const snap = pendingProgress;
      pendingProgress = null;
      return api.saveProgress(snap.mangaId, snap.url, snap.name, snap.ratio).catch(() => {});
    }
    return Promise.resolve();
  }

  function clearPendingTimers(flush = false) {
    if (flush) {
      void flushPendingProgress();
    } else {
      if (saveTimer !== null) {
        window.clearTimeout(saveTimer);
        saveTimer = null;
      }
      pendingProgress = null;
    }
    if (restoreTimer !== null) {
      window.clearTimeout(restoreTimer);
      restoreTimer = null;
    }
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

  function scrollToStartOfContainer() {
    if (!scrollEl) return;
    if (isHorizontal) {
      scrollEl.scrollLeft = 0;
      lastScrollOffset = 0;
    } else {
      scrollEl.scrollTop = 0;
      lastScrollOffset = 0;
    }
  }

  function applyScrollRatio(ratio: number) {
    if (!scrollEl) return;
    if (isHorizontal) {
      const maxX = Math.max(0, scrollEl.scrollWidth - scrollEl.clientWidth);
      const target = Math.round(maxX * ratio);
      scrollEl.scrollLeft = isRtl ? -target : target;
      lastScrollOffset = Math.abs(scrollEl.scrollLeft);
    } else {
      const maxY = Math.max(0, scrollEl.scrollHeight - scrollEl.clientHeight);
      const target = Math.round(maxY * ratio);
      scrollEl.scrollTop = target;
      lastScrollOffset = target;
    }
  }

  async function loadInitial(targetUrl: string, scrollRatio: number) {
    const seq = ++loadSeq;
    clearPendingTimers(true);
    loadingInitial = true;
    appendingNext = false;
    errorState = null;
    strip = [];
    focusedKeys = new Set();
    focusedPageIdx = 0;
    prefetched.clear();
    markedReadSet.clear();
    restoredInitialScroll = false;
    isUserScrollingBeforeRestore = false;
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

      // Wait for Svelte to mount the single chapter's DOM into scrollEl before setting scroll position.
      await tick();
      if (seq !== loadSeq) return;

      if (scrollRatio > 0.02 && scrollEl) {
        applyScrollRatio(scrollRatio);
        restoreTimer = window.setTimeout(() => {
          restoreTimer = null;
          if (seq === loadSeq && scrollEl && !isUserScrollingBeforeRestore) {
            applyScrollRatio(scrollRatio);
          }
          restoredInitialScroll = true;
          updateFocusNow();
        }, 220);
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

      // Save immediate history entry when opening a chapter.
      void api
        .saveProgress(mangaId, chapterData.url, chapterData.name, scrollRatio)
        .catch(() => {});

      // Short chapters that fit without scrolling should still be marked read.
      if (chapterData.images.length <= 2) {
        markChapterReadLocalAndRemote(chapterData.url);
      }
    } catch (err) {
      if (seq !== loadSeq) return;
      errorState = formatAppError(err, settingsStore.value.language);
      loadingInitial = false;
    }
  }

  $effect(() => {
    const _id = mangaId;
    const url = chapterUrl;
    const ratio = untrack(() => initialScrollRatio);
    untrack(() => {
      void _id;
      void loadInitial(url, ratio);
    });
    return () => {
      loadSeq += 1;
      clearPendingTimers(true);
    };
  });

  // Preserve reading page position seamlessly whenever orientation, layout mode, pagesPerGroup, or direction changes!
  let prevLayoutSig = "";
  $effect(() => {
    const sig = `${readerSettings.orientation}|${vCfg.layoutMode}|${vCfg.pagesPerGroup}|${hCfg.pagesPerGroup}|${hCfg.direction}`;
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
      // If switching to Horizontal mode while multiple chapters were auto-appended, keep only the active chapter.
      if (untrack(() => isHorizontal && strip.length > 1)) {
        const current = untrack(() => strip.find((c) => normUrl(c.url) === normUrl(anchorChap)));
        if (current) {
          strip = [current];
        }
      }
      await tick();
      if (!scrollEl) return;
      const pageEl = scrollEl.querySelector<HTMLElement>(
        `[data-page-key="${CSS.escape(`${anchorChap}:${anchorPage}`)}"]`
      );
      if (pageEl) {
        pageEl.scrollIntoView({ behavior: "instant", block: "center", inline: "center" });
        lastScrollOffset = isHorizontal ? Math.abs(scrollEl.scrollLeft) : scrollEl.scrollTop;
      }
      updateFocusNow();
    })();
  });

  // Re-evaluate focus when viewport resizes so unfocused dimming never sticks on wrong page/group.
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

      // Capture exact scroll position before appending so the viewport NEVER jumps to the top.
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

  /**
   * Navigates directly to a single target chapter: hides all previous chapters in `strip`,
   * loads ONLY the target chapter, and resets the scroll container to the start (0).
   */
  function jumpToChapter(url: string) {
    if (!url) return;
    void flushPendingProgress();
    quickSettingsOpen = false;
    toolbarVisible = true;
    scrollToStartOfContainer();

    const sameRouteProp = url === chapterUrl && initialScrollRatio === 0;
    router.goReader(mangaId, url, 0);
    if (sameRouteProp) {
      void loadInitial(url, 0);
    }
  }

  function stepHorizontalGroup(deltaStep: 1 | -1) {
    if (!scrollEl || !isHorizontal) return;
    const groups = Array.from(scrollEl.querySelectorAll<HTMLElement>("[data-group-key]"));
    if (groups.length === 0) return;

    const currentIdx = groups.findIndex(
      (g) => g.dataset.groupKey && focusedKeys.has(g.dataset.groupKey)
    );
    const baseIdx = currentIdx >= 0 ? currentIdx : 0;
    const nextIdx = baseIdx + deltaStep;

    if (nextIdx >= 0 && nextIdx < groups.length) {
      const targetGroup = groups[nextIdx];
      if (targetGroup.dataset.groupKey) {
        focusedKeys = new Set([targetGroup.dataset.groupKey]);
        const firstPage = targetGroup.querySelector<HTMLElement>("[data-page-idx]");
        if (firstPage?.dataset.pageIdx) {
          focusedPageIdx = Number(firstPage.dataset.pageIdx) || 0;
        }
      }
      targetGroup.scrollIntoView({ behavior: "smooth", inline: "center", block: "nearest" });
      return;
    }
    // Past ends of chapter -> transition chapters cleanly.
    if (deltaStep > 0 && activeChapter?.next) {
      jumpToChapter(activeChapter.next);
    } else if (deltaStep < 0 && activeChapter?.prev) {
      jumpToChapter(activeChapter.prev);
    }
  }

  async function handleManualVerify() {
    if (!errorState?.blockedUrl || verifyingBlocked) return;
    verifyingBlocked = true;
    try {
      toasts.push(t.toastOpeningVerify, "info", 4500);
      await api.openManualVerification(errorState.blockedUrl);
      toasts.push(t.toastVerifyDone, "success");
      await loadInitial(chapterUrl, initialScrollRatio);
    } catch (err) {
      errorState = formatAppError(err, settingsStore.value.language);
    } finally {
      verifyingBlocked = false;
    }
  }

  function handleWheel(e: WheelEvent) {
    if (!isHorizontal || !scrollEl || e.ctrlKey) return;
    if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) {
      e.preventDefault();
      scrollEl.scrollLeft += e.deltaY * (isRtl ? -1 : 1);
    }
  }

  function handleScroll() {
    if (!scrollEl || strip.length === 0 || loadingInitial) return;

    scheduleFocusUpdate();

    if (isHorizontal) {
      const absLeft = Math.abs(scrollEl.scrollLeft);
      const delta = absLeft - lastScrollOffset;

      if (!restoredInitialScroll && restoreTimer !== null && Math.abs(delta) > 15) {
        isUserScrollingBeforeRestore = true;
        window.clearTimeout(restoreTimer);
        restoreTimer = null;
        restoredInitialScroll = true;
      }

      if (delta > 20 && absLeft > 80) {
        toolbarVisible = false;
        quickSettingsOpen = false;
      } else if (delta < -20 || absLeft < 40) {
        toolbarVisible = true;
      }
      lastScrollOffset = absLeft;

      if (restoredInitialScroll && activeChapter) {
        const maxScroll = Math.max(1, scrollEl.scrollWidth - scrollEl.clientWidth);
        const ratio = Math.min(1, Math.max(0, absLeft / maxScroll));
        scheduleSaveProgress(activeChapter.url, activeChapter.name, ratio);
        if (ratio >= 0.85) {
          markChapterReadLocalAndRemote(activeChapter.url);
        }
        if (ratio >= 0.7 && activeChapter.next) {
          void prefetchNext(activeChapter.next);
        }
      }
      return;
    }

    // Vertical scroll handling (Continuous Strip & Vertical Grouped Scroll).
    const top = scrollEl.scrollTop;
    const delta = top - lastScrollOffset;

    if (!restoredInitialScroll && restoreTimer !== null && Math.abs(delta) > 15) {
      isUserScrollingBeforeRestore = true;
      window.clearTimeout(restoreTimer);
      restoreTimer = null;
      restoredInitialScroll = true;
    }

    if (delta > 16 && top > 80) {
      toolbarVisible = false;
      quickSettingsOpen = false;
    } else if (delta < -16 || top < 60) {
      toolbarVisible = true;
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
        activeUrl = chap.url;
        const secHeight = Math.max(1, currentSection.offsetHeight - scrollEl.clientHeight);
        const ratio = Math.min(1, Math.max(0, (top - currentSection.offsetTop) / secHeight));

        scheduleSaveProgress(chap.url, chap.name, ratio);

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

  function scheduleSaveProgress(url: string, name: string, ratio: number) {
    pendingProgress = { mangaId, url, name, ratio };
    if (saveTimer !== null) {
      window.clearTimeout(saveTimer);
    }
    saveTimer = window.setTimeout(() => {
      void flushPendingProgress();
    }, 900);
  }

  async function toggleFullscreen() {
    try {
      const win = getCurrentWindow();
      const next = !(await win.isFullscreen());
      await win.setFullscreen(next);
      isFullscreen = next;
    } catch {
      // Browser preview fallback.
    }
  }

  async function exitReader() {
    await flushPendingProgress();
    try {
      const win = getCurrentWindow();
      if (await win.isFullscreen()) {
        await win.setFullscreen(false);
        isFullscreen = false;
      }
    } catch {
      // Ignore outside Tauri.
    }
    router.goManga(mangaId);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (
      e.target instanceof HTMLInputElement ||
      e.target instanceof HTMLSelectElement ||
      e.target instanceof HTMLTextAreaElement
    ) {
      return;
    }

    if (isHorizontal) {
      if (e.key === "ArrowRight") {
        e.preventDefault();
        stepHorizontalGroup(isRtl ? -1 : 1);
      } else if (e.key === "ArrowLeft") {
        e.preventDefault();
        stepHorizontalGroup(isRtl ? 1 : -1);
      } else if (e.key === " ") {
        e.preventDefault();
        stepHorizontalGroup(e.shiftKey ? -1 : 1);
      } else if (e.key === "f" || e.key === "F") {
        e.preventDefault();
        void toggleFullscreen();
      } else if (e.key === "Escape") {
        e.preventDefault();
        if (quickSettingsOpen) quickSettingsOpen = false;
        else if (isFullscreen) void toggleFullscreen();
        else void exitReader();
      }
      return;
    }

    if (e.key === "ArrowLeft" && activeChapter?.prev) {
      e.preventDefault();
      jumpToChapter(activeChapter.prev);
    } else if (e.key === "ArrowRight" && activeChapter?.next) {
      e.preventDefault();
      jumpToChapter(activeChapter.next);
    } else if (e.key === " " && scrollEl) {
      e.preventDefault();
      const amount = scrollEl.clientHeight * 0.85;
      scrollEl.scrollBy({ top: e.shiftKey ? -amount : amount, behavior: "smooth" });
    } else if (e.key === "f" || e.key === "F") {
      e.preventDefault();
      void toggleFullscreen();
    } else if (e.key === "Escape") {
      e.preventDefault();
      if (quickSettingsOpen) {
        quickSettingsOpen = false;
      } else if (isFullscreen) {
        void toggleFullscreen();
      } else {
        void exitReader();
      }
    }
  }

  function setOrientation(o: ReadingOrientation) {
    settingsStore.update((d) => {
      d.reader.orientation = o;
    });
  }

  function setVerticalLayout(m: VerticalLayoutMode) {
    settingsStore.update((d) => {
      d.reader.vertical.layoutMode = m;
    });
  }

  function setPagesPerGroup(n: number) {
    const clamped = Math.max(1, Math.min(8, n));
    settingsStore.update((d) => {
      if (isHorizontal) {
        d.reader.horizontal.pagesPerGroup = clamped;
      } else {
        d.reader.vertical.pagesPerGroup = clamped;
      }
    });
  }

  function setHorizontalDirection(dir: HorizontalDirection) {
    settingsStore.update((d) => {
      d.reader.horizontal.direction = dir;
    });
  }

  function setHorizontalSnap(mode: HorizontalSnapMode) {
    settingsStore.update((d) => {
      d.reader.horizontal.snapMode = mode;
    });
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

  function updateReaderWidth(px: number) {
    settingsStore.update((d) => {
      d.reader.width = { mode: "px", value: px };
    }, 150);
  }

  function toggleAutoNext() {
    settingsStore.update((d) => {
      d.reader.autoNextChapter = !d.reader.autoNextChapter;
    });
  }

  function setReaderBg(bg: ReaderBg) {
    settingsStore.update((d) => {
      d.reader.readerBg = bg;
    });
  }
</script>

<svelte:window
  onkeydown={handleKeydown}
  onmousemove={(e) => {
    if (e.clientY < 52) toolbarVisible = true;
  }}
/>

<div
  class="relative flex h-full w-full flex-col overflow-hidden"
  style="background-color: {readerBackdrop()};"
>
  <!-- Floating Auto-Hide Top Toolbar -->
  <header
    class="fixed top-0 right-0 left-0 z-30 flex items-center justify-between gap-3 border-b px-4 py-2.5 backdrop-blur-md transition-transform duration-200"
    style="background-color: color-mix(in srgb, var(--surface) 88%, transparent); border-color: var(--border); transform: translateY({toolbarVisible
      ? '0%'
      : '-110%'});"
  >
    <div class="flex min-w-0 items-center gap-2.5">
      <button
        type="button"
        onclick={exitReader}
        class="inline-flex items-center gap-1 rounded-lg px-2 py-1.5 text-sm hover:opacity-80"
        title={t.backTooltip}
      >
        <ArrowLeft size={17} />
        <span class="hidden sm:inline">{t.back}</span>
      </button>

      <div class="min-w-0 border-l pl-3" style="border-color: var(--border);">
        <div class="truncate text-sm font-semibold">{manga?.title ?? t.readingFallback}</div>
        <div class="truncate text-xs" style="color: var(--text-muted);">
          {activeChapter?.name ?? ""}
        </div>
      </div>
    </div>

    <!-- Chapter selector + prev/next -->
    <div class="flex items-center gap-1.5">
      <button
        type="button"
        disabled={!activeChapter?.prev}
        onclick={() => activeChapter?.prev && jumpToChapter(activeChapter.prev)}
        class="rounded-lg border p-1.5 disabled:opacity-35"
        style="border-color: var(--border); background-color: var(--surface);"
        title={t.prevChapterTooltip}
      >
        <ChevronLeft size={17} />
      </button>

      {#if manga && manga.chapters.length > 0}
        <select
          value={selectChapterValue()}
          onchange={(e) => {
            const target = e.currentTarget.value;
            e.currentTarget.blur();
            jumpToChapter(target);
          }}
          class="max-w-44 rounded-lg border px-2.5 py-1.5 text-xs font-medium outline-none"
          style="background-color: var(--surface); border-color: var(--border); color: var(--text);"
        >
          {#each manga.chapters as ch (ch.url)}
            <option value={ch.url}>{ch.name}</option>
          {/each}
        </select>
      {/if}

      <button
        type="button"
        disabled={!activeChapter?.next}
        onclick={() => activeChapter?.next && jumpToChapter(activeChapter.next)}
        class="rounded-lg border p-1.5 disabled:opacity-35"
        style="border-color: var(--border); background-color: var(--surface);"
        title={t.nextChapterTooltip}
      >
        <ChevronRight size={17} />
      </button>

      <button
        type="button"
        onclick={() => {
          quickSettingsOpen = !quickSettingsOpen;
        }}
        class="ml-1 rounded-lg border p-1.5"
        style="border-color: var(--border); background-color: {quickSettingsOpen
          ? 'var(--accent-soft)'
          : 'var(--surface)'}; color: {quickSettingsOpen ? 'var(--accent)' : 'var(--text)'};"
        title={t.quickSettingsTooltip}
      >
        <SlidersHorizontal size={16} />
      </button>

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
    </div>
  </header>

  <!-- Quick Reader Settings Popover -->
  {#if quickSettingsOpen}
    <div
      class="fixed top-14 right-4 z-40 flex max-h-[80vh] w-80 flex-col gap-3.5 overflow-y-auto rounded-2xl border p-4 text-xs shadow-xl"
      style="background-color: var(--surface); border-color: var(--border);"
    >
      <!-- Orientation toggle -->
      <div class="flex flex-col gap-1.5">
        <span class="font-medium">{t.readingModeLabel}</span>
        <div class="grid grid-cols-2 gap-1.5">
          {#each [["vertical", t.modeVertical], ["horizontal", t.modeHorizontal]] as [val, label] (val)}
            <button
              type="button"
              onclick={() => setOrientation(val as ReadingOrientation)}
              class="rounded-lg border py-1.5 font-medium"
              style="border-color: {readerSettings.orientation === val
                ? 'var(--accent)'
                : 'var(--border)'}; background-color: {readerSettings.orientation === val
                ? 'var(--accent-soft)'
                : 'var(--surface-subtle)'}; color: {readerSettings.orientation === val
                ? 'var(--accent)'
                : 'var(--text)'};"
            >
              {label}
            </button>
          {/each}
        </div>
      </div>

      {#if !isHorizontal}
        <!-- Vertical sub-mode -->
        <div class="flex flex-col gap-1.5">
          <span class="font-medium">{t.verticalStyleLabel}</span>
          <div class="grid grid-cols-2 gap-1.5">
            {#each [["continuous_strip", t.verticalContinuous], ["vertical_grouped", t.verticalGrouped]] as [val, label] (val)}
              <button
                type="button"
                onclick={() => setVerticalLayout(val as VerticalLayoutMode)}
                class="rounded-lg border py-1.5 font-medium"
                style="border-color: {vCfg.layoutMode === val
                  ? 'var(--accent)'
                  : 'var(--border)'}; background-color: {vCfg.layoutMode === val
                  ? 'var(--accent-soft)'
                  : 'var(--surface-subtle)'}; color: {vCfg.layoutMode === val
                  ? 'var(--accent)'
                  : 'var(--text)'};"
              >
                {label}
              </button>
            {/each}
          </div>
        </div>
      {:else}
        <!-- Horizontal direction -->
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
      {/if}

      <!-- Pages per group (Horizontal or Vertical Grouped) -->
      {#if isHorizontal || isVerticalGrouped}
        {@const activeCount = isHorizontal ? hCfg.pagesPerGroup : vCfg.pagesPerGroup}
        <div class="flex flex-col gap-1.5">
          <div class="flex items-center justify-between font-medium">
            <span>{t.pagesPerGroupLabel}</span>
            <span style="color: var(--text-muted);">{activeCount}</span>
          </div>
          <div class="grid grid-cols-4 gap-1.5">
            {#each [1, 2, 3] as n (n)}
              <button
                type="button"
                onclick={() => setPagesPerGroup(n)}
                class="rounded-lg border py-1 font-medium"
                style="border-color: {activeCount === n
                  ? 'var(--accent)'
                  : 'var(--border)'}; background-color: {activeCount === n
                  ? 'var(--accent-soft)'
                  : 'var(--surface-subtle)'}; color: {activeCount === n
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
              value={activeCount}
              onchange={(e) => setPagesPerGroup(Number(e.currentTarget.value))}
              class="rounded-lg border px-2 py-1 text-center text-xs outline-none"
              style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
            />
          </div>
        </div>
      {/if}

      <!-- Snap controls -->
      {#if isHorizontal}
        <div class="flex flex-col gap-1.5">
          <span class="font-medium">{t.snapModeLabel}</span>
          <div class="grid grid-cols-3 gap-1.5">
            {#each [["none", t.snapNone], ["page", t.snapPage], ["group", t.snapGroup]] as [val, label] (val)}
              <button
                type="button"
                onclick={() => setHorizontalSnap(val as HorizontalSnapMode)}
                class="rounded-lg border py-1 font-medium"
                style="border-color: {hCfg.snapMode === val
                  ? 'var(--accent)'
                  : 'var(--border)'}; background-color: {hCfg.snapMode === val
                  ? 'var(--accent-soft)'
                  : 'var(--surface-subtle)'}; color: {hCfg.snapMode === val
                  ? 'var(--accent)'
                  : 'var(--text)'};"
              >
                {label}
              </button>
            {/each}
          </div>
        </div>
      {:else if isVerticalGrouped}
        <div class="flex items-center justify-between">
          <span class="font-medium">{t.groupSnapLabel}</span>
          <input
            type="checkbox"
            checked={vCfg.snapEnabled}
            onchange={(e) => {
              const checked = e.currentTarget.checked;
              settingsStore.update((d) => (d.reader.vertical.snapEnabled = checked));
            }}
            class="h-4 w-4 accent-rose-500"
          />
        </div>
      {/if}

      <!-- Focus dimming quick toggle + slider -->
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

      {#if !isHorizontal}
        <div class="flex flex-col gap-1.5">
          <div class="flex items-center justify-between font-medium">
            <span>{t.imageWidth}</span>
            <span style="color: var(--text-muted);">
              {readerSettings.width.mode === "px"
                ? `${readerSettings.width.value}px`
                : readerSettings.width.mode === "percent"
                  ? `${readerSettings.width.value}%`
                  : t.fitWindow}
            </span>
          </div>
          <input
            type="range"
            min="480"
            max="1400"
            step="20"
            value={readerSettings.width.mode === "px" ? readerSettings.width.value : 800}
            oninput={(e) => updateReaderWidth(Number(e.currentTarget.value))}
            class="w-full accent-rose-500"
          />
        </div>

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
    </div>
  {/if}

  <!-- Reading Viewport Container -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    bind:this={scrollEl}
    onscroll={handleScroll}
    onwheel={handleWheel}
    onclick={() => {
      if (quickSettingsOpen) quickSettingsOpen = false;
    }}
    dir={isRtl ? "rtl" : "ltr"}
    class={isHorizontal
      ? "relative flex h-full w-full overflow-x-auto overflow-y-hidden"
      : "relative h-full w-full overflow-y-auto"}
    style={scrollContainerSnapStyle()}
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
            onclick={() => loadInitial(chapterUrl, initialScrollRatio)}
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
      <!-- HORIZONTAL READING MODE -->
      {@const groups = partitionPages(activeChapter.images, hCfg.pagesPerGroup)}
      <div
        class="flex h-full w-max items-center pt-12 pb-4 {hCfg.centerGroups ? 'px-[8vw]' : 'px-4'}"
        style="gap: {hCfg.groupGapPx}px;"
      >
        {#each groups as group (`${activeChapter.url}:hg:${group.groupIdx}`)}
          {@const gKey = `${activeChapter.url}:hg:${group.groupIdx}`}
          {@const gBright = groupDimBrightness(gKey, hCfg.dimUnfocused, hCfg.dimLevel)}
          <div
            data-group-key={gKey}
            class="flex h-full shrink-0 items-center justify-center"
            style="gap: {hCfg.pageGapPx}px; {hCfg.snapMode === 'group'
              ? 'scroll-snap-align: center;'
              : ''} {gBright < 0.999
              ? `filter: brightness(${gBright}); transition: filter 160ms ease;`
              : 'transition: filter 160ms ease;'}"
          >
            {#each group.items as entry (`${activeChapter.url}:${entry.pageIdx}`)}
              <PageImage
                token={entry.token}
                index={entry.pageIdx}
                pageKey={`${activeChapter.url}:${entry.pageIdx}`}
                {preloadMarginPx}
                eager={entry.pageIdx < readerSettings.preloadImages ||
                  Math.abs(entry.pageIdx - focusedPageIdx) <= readerSettings.preloadImages}
                horizontal={true}
                snapPage={hCfg.snapMode === "page"}
                groupPageCount={group.items.length}
              />
            {/each}
          </div>
        {/each}

        <!-- End-of-chapter horizontal card -->
        <div
          dir="ltr"
          class="flex h-full shrink-0 flex-col items-center justify-center px-8"
          style={hCfg.snapMode !== "none" ? "scroll-snap-align: center;" : ""}
        >
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
        </div>
      </div>
    {:else}
      <!-- VERTICAL READING MODE (Continuous Strip OR Vertical Grouped Scroll) -->
      <div
        class="mx-auto flex flex-col pt-14 pb-24 {isVerticalGrouped && vCfg.alignment === 'start'
          ? 'items-start'
          : 'items-center'}"
        style={contentWidthStyle()}
      >
        {#each strip as chap, chapIdx (chap.url)}
          <section data-chapter-url={chap.url} class="flex w-full flex-col">
            {#if chapIdx > 0}
              <div
                class="my-8 flex items-center gap-3 px-4 text-xs font-medium"
                style="color: var(--text-muted);"
              >
                <div class="h-px flex-1" style="background-color: var(--border);"></div>
                <span>{chap.name}</span>
                <div class="h-px flex-1" style="background-color: var(--border);"></div>
              </div>
            {/if}

            {#if isVerticalGrouped}
              {@const vGroups = partitionPages(chap.images, vCfg.pagesPerGroup)}
              <div class="flex w-full flex-col" style="gap: {vCfg.groupGapPx}px;">
                {#each vGroups as group (`${chap.url}:vg:${group.groupIdx}`)}
                  {@const gKey = `${chap.url}:vg:${group.groupIdx}`}
                  {@const gBright = groupDimBrightness(gKey, vCfg.dimUnfocused, vCfg.dimLevel)}
                  <div
                    data-group-key={gKey}
                    class="flex w-full flex-col"
                    style="gap: {vCfg.pageGapPx}px; {vCfg.snapEnabled
                      ? `scroll-snap-align: ${vCfg.alignment === 'start' ? 'start' : 'center'};`
                      : ''} {gBright < 0.999
                      ? `filter: brightness(${gBright}); transition: filter 160ms ease;`
                      : 'transition: filter 160ms ease;'}"
                  >
                    {#each group.items as entry (`${chap.url}:${entry.pageIdx}`)}
                      <PageImage
                        token={entry.token}
                        index={entry.pageIdx}
                        pageKey={`${chap.url}:${entry.pageIdx}`}
                        {preloadMarginPx}
                        eager={(chapIdx === 0 && entry.pageIdx < readerSettings.preloadImages) ||
                          (normUrl(chap.url) === normUrl(activeUrl) &&
                            Math.abs(entry.pageIdx - focusedPageIdx) <=
                              readerSettings.preloadImages)}
                      />
                    {/each}
                  </div>
                {/each}
              </div>
            {:else}
              <!-- Continuous Strip: never snaps, customizable page gap, optional per-image focus dimming -->
              <div class="flex w-full flex-col" style="gap: {vCfg.pageGapPx}px;">
                {#each chap.images as token, imgIdx (`${chap.url}:${imgIdx}`)}
                  {@const pKey = `${chap.url}:${imgIdx}`}
                  <PageImage
                    {token}
                    index={imgIdx}
                    pageKey={pKey}
                    {preloadMarginPx}
                    eager={(chapIdx === 0 && imgIdx < readerSettings.preloadImages) ||
                      (normUrl(chap.url) === normUrl(activeUrl) &&
                        Math.abs(imgIdx - focusedPageIdx) <= readerSettings.preloadImages)}
                    dimBrightness={pageDimBrightness(pKey)}
                  />
                {/each}
              </div>
            {/if}
          </section>
        {/each}

        <!-- Footer: Next Chapter Card / Loading / End of Manga -->
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
</div>
