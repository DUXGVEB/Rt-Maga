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
  import { api, formatAppError } from "../lib/api";
  import PageImage from "../lib/components/PageImage.svelte";
  import { router } from "../lib/stores/router.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import type { ChapterView, ImageWidth, MangaView, ReaderBg } from "../lib/types";

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
  let errorState = $state<{ title: string; detail: string } | null>(null);
  let toolbarVisible = $state(true);
  let quickSettingsOpen = $state(false);
  let isFullscreen = $state(false);

  let scrollEl = $state<HTMLDivElement | null>(null);
  let lastScrollTop = 0;
  let prefetched = new Map<string, ChapterView>();
  let saveTimer: number | null = null;
  let pendingProgress: { mangaId: number; url: string; name: string; ratio: number } | null = null;
  let restoreTimer: number | null = null;
  let markedReadSet = new Set<string>();
  let restoredInitialScroll = false;
  let loadSeq = 0;

  const readerSettings = $derived(settingsStore.value.reader);
  const t = $derived(settingsStore.t);

  const activeChapter = $derived(
    strip.find((c) => c.url === activeUrl) ?? strip[strip.length - 1] ?? null
  );

  const lastChapter = $derived(strip[strip.length - 1] ?? null);

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

  function flushPendingProgress() {
    if (saveTimer !== null) {
      window.clearTimeout(saveTimer);
      saveTimer = null;
    }
    if (pendingProgress) {
      const snap = pendingProgress;
      pendingProgress = null;
      void api.saveProgress(snap.mangaId, snap.url, snap.name, snap.ratio).catch(() => {});
    }
  }

  function clearPendingTimers(flush = false) {
    if (flush) {
      flushPendingProgress();
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
  }

  async function loadInitial(targetUrl: string, scrollRatio: number) {
    const seq = ++loadSeq;
    clearPendingTimers(true);
    loadingInitial = true;
    errorState = null;
    strip = [];
    prefetched.clear();
    markedReadSet.clear();
    restoredInitialScroll = false;

    try {
      const [mangaData, chapterData] = await Promise.all([
        api.getManga(mangaId).catch(() => null),
        api.getChapter(targetUrl, mangaId),
      ]);
      if (seq !== loadSeq) return;

      manga = mangaData;
      strip = [chapterData];
      activeUrl = chapterData.url;

      // Save immediate history entry when opening a chapter.
      await api
        .saveProgress(mangaId, chapterData.url, chapterData.name, scrollRatio)
        .catch(() => {});
      if (seq !== loadSeq) return;

      if (scrollRatio > 0.02 && scrollEl) {
        restoreTimer = window.setTimeout(() => {
          restoreTimer = null;
          if (seq === loadSeq && scrollEl && !restoredInitialScroll) {
            const maxScroll = Math.max(0, scrollEl.scrollHeight - scrollEl.clientHeight);
            scrollEl.scrollTop = Math.round(maxScroll * scrollRatio);
            restoredInitialScroll = true;
          }
        }, 320);
      } else if (scrollEl) {
        scrollEl.scrollTop = 0;
        restoredInitialScroll = true;
      } else {
        restoredInitialScroll = true;
      }

      // Short chapters that fit without scrolling should still be marked read.
      if (chapterData.images.length <= 2 && !markedReadSet.has(chapterData.url)) {
        markedReadSet.add(chapterData.url);
        void api.markRead(mangaId, chapterData.url).catch(() => {});
      }
    } catch (err) {
      if (seq !== loadSeq) return;
      errorState = formatAppError(err, settingsStore.value.language);
    } finally {
      if (seq === loadSeq) {
        loadingInitial = false;
      }
    }
  }

  $effect(() => {
    void mangaId;
    void chapterUrl;
    loadInitial(chapterUrl, initialScrollRatio);
    return () => {
      loadSeq += 1;
      clearPendingTimers(true);
    };
  });

  async function prefetchNext(nextUrl: string) {
    if (prefetched.has(nextUrl) || strip.some((c) => c.url === nextUrl)) return;
    const seq = loadSeq;
    try {
      const data = await api.getChapter(nextUrl, mangaId);
      if (seq === loadSeq) {
        if (prefetched.size >= 3) {
          const oldestKey = prefetched.keys().next().value;
          if (oldestKey) prefetched.delete(oldestKey);
        }
        prefetched.set(nextUrl, data);
      }
    } catch {
      // Prefetch errors will surface only if the user actually transitions.
    }
  }

  async function appendNextChapter() {
    const tail = lastChapter;
    if (!tail?.next || appendingNext) return;
    if (strip.some((c) => c.url === tail.next)) return;

    const seq = loadSeq;
    appendingNext = true;
    try {
      const nextChap =
        prefetched.get(tail.next) ?? (await api.getChapter(tail.next, mangaId));
      if (seq !== loadSeq) return;
      prefetched.delete(tail.next);
      strip = [...strip, nextChap];

      // Trim far-above chapters (> 4) while preserving current scroll offset.
      if (strip.length > 4 && scrollEl) {
        const firstSection = scrollEl.querySelector<HTMLElement>("[data-chapter-url]");
        const removedHeight = firstSection ? firstSection.offsetHeight : 0;
        strip = strip.slice(1);
        if (removedHeight > 0) {
          queueMicrotask(() => {
            if (seq === loadSeq && scrollEl) {
              scrollEl.scrollTop = Math.max(0, scrollEl.scrollTop - removedHeight);
            }
          });
        }
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
    flushPendingProgress();
    quickSettingsOpen = false;
    router.goReader(mangaId, url, 0);
  }

  function handleScroll() {
    if (!scrollEl || strip.length === 0) return;

    const top = scrollEl.scrollTop;
    // If the user starts scrolling manually before the initial restore timer fires, honor user scroll.
    if (!restoredInitialScroll && restoreTimer !== null && top > 4) {
      window.clearTimeout(restoreTimer);
      restoreTimer = null;
      restoredInitialScroll = true;
    }

    const delta = top - lastScrollTop;
    if (delta > 16 && top > 80) {
      toolbarVisible = false;
      quickSettingsOpen = false;
    } else if (delta < -16 || top < 60) {
      toolbarVisible = true;
    }
    lastScrollTop = top;

    // Determine which chapter section currently owns the upper viewport.
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
      currentSection = sections[sections.length - 1];
    }

    if (currentSection && restoredInitialScroll) {
      const chapUrl = currentSection.dataset.chapterUrl ?? activeUrl;
      const chap = strip.find((c) => c.url === chapUrl);
      if (chap) {
        activeUrl = chap.url;
        const secHeight = Math.max(1, currentSection.offsetHeight - scrollEl.clientHeight);
        const ratio = Math.min(1, Math.max(0, (top - currentSection.offsetTop) / secHeight));

        scheduleSaveProgress(chap.url, chap.name, ratio);

        if (ratio >= 0.85 && !markedReadSet.has(chap.url)) {
          markedReadSet.add(chap.url);
          api.markRead(mangaId, chap.url).catch(() => {});
        }
      }
    }

    // Trigger prefetch at 70% of scroll container, and continuous append within ~1600px of bottom.
    const remaining = scrollEl.scrollHeight - (top + scrollEl.clientHeight);
    const tail = lastChapter;
    if (tail?.next) {
      const progressAll = (top + scrollEl.clientHeight) / Math.max(1, scrollEl.scrollHeight);
      if (progressAll >= 0.7) {
        prefetchNext(tail.next);
      }
      if (readerSettings.autoNextChapter && remaining < 1600) {
        appendNextChapter();
      }
    }
  }

  function scheduleSaveProgress(url: string, name: string, ratio: number) {
    pendingProgress = { mangaId, url, name, ratio };
    if (saveTimer !== null) {
      window.clearTimeout(saveTimer);
    }
    saveTimer = window.setTimeout(() => {
      flushPendingProgress();
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
    flushPendingProgress();
    try {
      const win = getCurrentWindow();
      if (await win.isFullscreen()) {
        await win.setFullscreen(false);
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
      toggleFullscreen();
    } else if (e.key === "Escape") {
      e.preventDefault();
      if (isFullscreen) {
        toggleFullscreen();
      } else {
        exitReader();
      }
    }
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
          value={activeUrl}
          onchange={(e) => jumpToChapter(e.currentTarget.value)}
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
      class="fixed top-14 right-4 z-40 flex w-72 flex-col gap-4 rounded-2xl border p-4 text-xs shadow-xl"
      style="background-color: var(--surface); border-color: var(--border);"
    >
      <div class="flex flex-col gap-2">
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

  <!-- Vertical Continuous Scroll Area -->
  <div
    bind:this={scrollEl}
    onscroll={handleScroll}
    class="h-full w-full overflow-y-auto scroll-smooth"
  >
    {#if loadingInitial}
      <div class="flex h-full flex-col items-center justify-center gap-3 text-sm" style="color: var(--text-muted);">
        <Loader2 size={28} class="animate-spin" style="color: var(--accent);" />
        <span>{t.loadingPages}</span>
      </div>
    {:else if errorState}
      <div class="flex h-full flex-col items-center justify-center gap-3 px-6 text-center">
        <div class="text-base font-semibold">{errorState.title}</div>
        <p class="max-w-md text-xs" style="color: var(--text-muted);">{errorState.detail}</p>
        <div class="mt-2 flex gap-2">
          <button
            type="button"
            onclick={() => loadInitial(chapterUrl, 0)}
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
    {:else}
      <div class="mx-auto flex flex-col items-center pt-14 pb-24" style={contentWidthStyle()}>
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

            <div class="flex w-full flex-col" style="gap: {readerSettings.gapPx}px;">
              {#each chap.images as token, imgIdx (`${chap.url}:${imgIdx}`)}
                <PageImage
                  {token}
                  index={imgIdx}
                  {preloadMarginPx}
                  eager={chapIdx === 0 && imgIdx < readerSettings.preloadImages}
                />
              {/each}
            </div>
          </section>
        {/each}

        <!-- Footer: Next Chapter Card / Loading / End of Manga -->
        <div class="mt-10 flex w-full flex-col items-center gap-3 px-4">
          {#if appendingNext}
            <div class="flex items-center gap-2 text-xs" style="color: var(--text-muted);">
              <Loader2 size={16} class="animate-spin" style="color: var(--accent);" />
              <span>{t.loadingNextChapter}</span>
            </div>
          {:else if lastChapter?.next}
            <button
              type="button"
              onclick={() => {
                if (readerSettings.autoNextChapter) {
                  appendNextChapter();
                } else if (lastChapter?.next) {
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
