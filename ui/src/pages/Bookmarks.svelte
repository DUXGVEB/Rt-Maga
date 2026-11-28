<script lang="ts">
  import { ArrowDown, Bookmark, BookOpen, CheckCircle2, Dices, Play, Sparkles, Trash2 } from "lucide-svelte";
  import { flip } from "svelte/animate";
  import { api, imgProxyUrl } from "../lib/api";
  import { isMetadataOnlyChapterUrl, isTachibkCatalogSite } from "../lib/comicFormats";
  import {
    flipDuration,
    itemExit,
    pressable,
    shouldStaggerItem,
    staggerDelay,
    viewReveal,
  } from "../lib/motion";
  import { heroTransitionStore } from "../lib/stores/heroTransition.svelte";
  import { libraryCache } from "../lib/stores/libraryCache.svelte";
  import { randomMangaStore } from "../lib/stores/randomManga.svelte";
  import { router } from "../lib/stores/router.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { updatesStore } from "../lib/stores/updates.svelte";
  import type { BookmarkItemView, BookmarkSort, LibrarySortField, SortDirection } from "../lib/types";

  let sortField = $state<LibrarySortField>("added");
  let sortDir = $state<SortDirection>("desc");
  const backendSort = $derived<BookmarkSort>(sortField === "title" ? "title" : "newest");
  let loading = $state(!libraryCache.bookmarksLoaded["newest"]);
  let failedCovers = $state<Record<number, boolean>>({});
  let loadSeq = 0;

  const rawItems = $derived(libraryCache.bookmarksBySort[backendSort] ?? []);
  const items = $derived(() => {
    const out = [...rawItems];
    const mult = sortDir === "desc" ? -1 : 1;
    out.sort((a, b) => {
      if (sortField === "title") {
        const cmp = a.title.localeCompare(b.title, undefined, { sensitivity: "base" });
        if (cmp !== 0) return cmp * mult;
        return (a.bookmarkedAt - b.bookmarkedAt) * mult;
      }
      if (a.bookmarkedAt !== b.bookmarkedAt) {
        return (a.bookmarkedAt - b.bookmarkedAt) * mult;
      }
      return (a.mangaId - b.mangaId) * mult;
    });
    return out;
  });
  const t = $derived(settingsStore.t);

  async function load(selectedSort: BookmarkSort, showSkeleton: boolean) {
    const seq = ++loadSeq;
    if (showSkeleton) loading = true;
    try {
      const loaded = await api.listBookmarks(selectedSort);
      if (seq !== loadSeq) return;
      libraryCache.setBookmarks(selectedSort, loaded);
    } catch {
      if (seq !== loadSeq) return;
      if (!libraryCache.bookmarksLoaded[selectedSort]) {
        libraryCache.setBookmarks(selectedSort, []);
      }
    } finally {
      if (seq === loadSeq) {
        loading = false;
      }
    }
  }

  $effect(() => {
    void updatesStore.lastFound;
    void load(backendSort, !libraryCache.bookmarksLoaded[backendSort]);
  });

  async function removeBookmark(mangaId: number, e: MouseEvent) {
    e.stopPropagation();
    try {
      await api.toggleBookmark(mangaId);
      libraryCache.removeBookmarkItem(mangaId);
      toasts.push(t.toastBookmarkRemoved, "info", 1800);
    } catch {
      toasts.push(t.toastHistoryUpdateFailed, "error");
    }
  }

  function goMangaWithHero(item: BookmarkItemView, triggerEl?: HTMLElement | null) {
    libraryCache.setMangaPreview({
      id: item.mangaId,
      title: item.title,
      coverToken: item.coverToken,
      site: item.site,
      isCompleted: item.isCompleted,
      unreadCount: item.unreadCount,
      bookmarked: true,
    });
    const coverEl =
      triggerEl?.closest("[data-hero-card]")?.querySelector<HTMLElement>("[data-hero-cover-src]") ??
      null;
    heroTransitionStore.captureFromElement(coverEl, item.coverToken, item.mangaId);
    router.goManga(item.mangaId);
  }

  function openBookmarkCover(item: BookmarkItemView, triggerEl?: HTMLElement | null) {
    if (isTachibkCatalogSite(item.site)) {
      goMangaWithHero(item, triggerEl);
      return;
    }
    if (item.isCompleted && !item.hasNew) {
      toasts.push(t.toastMangaCompletedNoResume, "info", 2800);
      return;
    }
    if (item.chapterUrl && !isMetadataOnlyChapterUrl(item.chapterUrl)) {
      router.goReader(
        item.mangaId,
        item.chapterUrl,
        item.scrollRatio ?? 0,
        item.pageIndex ?? 0
      );
    } else {
      goMangaWithHero(item, triggerEl);
    }
  }

  function progressPct(item: BookmarkItemView): number {
    if (item.totalChapters <= 0 || item.currentChapterIdx === null) {
      return Math.round((item.scrollRatio ?? 0) * 100);
    }
    return Math.min(100, Math.round(((item.currentChapterIdx + 1) / item.totalChapters) * 100));
  }
</script>

<div class="mx-auto flex w-full max-w-6xl flex-col gap-6 px-4 py-6 sm:px-8 sm:py-8">
  <div class="flex flex-wrap items-center justify-between gap-4">
    <div class="flex items-center gap-2.5">
      <h1 class="text-2xl font-bold tracking-tight">{t.bookmarksTitle}</h1>
      {#if items().length > 0}
        <span
          class="rounded-full px-2.5 py-0.5 text-xs font-semibold tabular-nums"
          style="background-color: var(--surface-hover); color: var(--text-muted);"
        >
          {items().length}
        </span>
      {/if}
    </div>

    {#if items().length > 0}
      <div class="flex flex-wrap items-center gap-2">
        <button
          type="button"
          use:pressable
          onclick={() => randomMangaStore.open("bookmarks")}
          class="btn-3d btn-3d-accent inline-flex min-h-[38px] items-center gap-1.5 rounded-xl px-3.5 py-1.5 text-xs font-semibold"
        >
          <Dices size={14} />
          <span>{t.randomMangaTrigger}</span>
        </button>
        <span class="text-xs font-medium" style="color: var(--text-muted);">{t.sortByLabel}</span>
        <select
          bind:value={sortField}
          class="surface-card min-h-[38px] rounded-xl px-3 py-1.5 text-xs font-medium outline-none"
          style="color: var(--text);"
        >
          <option value="added">{t.sortDateAdded}</option>
          <option value="title">{t.sortTitleName}</option>
        </select>
        <button
          type="button"
          use:pressable
          onclick={() => {
            sortDir = sortDir === "desc" ? "asc" : "desc";
          }}
          class="btn-3d btn-3d-surface inline-flex min-h-[38px] items-center gap-1.5 rounded-xl px-3 py-1.5 text-xs font-medium"
          title={sortDir === "desc" ? t.sortDesc : t.sortAsc}
        >
          <ArrowDown size={13} class="chevron-rot" data-open={sortDir === "asc" ? "true" : "false"} />
          <span>{sortDir === "desc" ? t.sortDesc : t.sortAsc}</span>
        </button>
      </div>
    {/if}
  </div>

  {#if loading}
    <div class="fluid-manga-grid">
      {#each Array(6) as _, i (i)}
        <div
          class="aspect-[3/4] animate-pulse rounded-2xl"
          style="background-color: var(--surface-hover);"
        ></div>
      {/each}
    </div>
  {:else if items().length === 0}
    <div
      in:viewReveal|local={{ duration: 165, y: 6 }}
      class="flex flex-col items-center justify-center gap-3 rounded-2xl border border-dashed py-16 text-center"
      style="border-color: var(--border-strong); color: var(--text-muted);"
    >
      <Bookmark size={30} strokeWidth={1.5} />
      <div class="text-sm font-medium" style="color: var(--text);">{t.bookmarksEmptyTitle}</div>
    </div>
  {:else}
    {@const list = items()}
    <div class="fluid-manga-grid">
      {#each list as item, idx (item.mangaId)}
        {@const completedOnly = item.isCompleted && !item.hasNew}
        {@const animateStagger = shouldStaggerItem(idx, list.length)}
        <div
          animate:flip={{ duration: flipDuration(180, list.length) }}
          out:itemExit|local
          class:stagger-item={animateStagger}
          style={animateStagger ? `--stagger-delay: ${staggerDelay(idx, 16, 180, list.length)}ms;` : undefined}
        >
          <article
            use:pressable
            data-hero-card
            class="card-cq card-contain card-press surface-card group relative flex h-full flex-col overflow-hidden rounded-2xl"
          >
            <div
              data-hero-cover-src
              class="relative aspect-[3/4] w-full overflow-hidden"
              style="background-color: var(--surface-subtle);"
            >
              <button
                type="button"
                onclick={(e) => openBookmarkCover(item, e.currentTarget)}
                title={completedOnly ? t.toastMangaCompletedNoResume : t.coverTapResumeHint}
                class="block h-full w-full cursor-pointer text-left"
              >
                {#if item.coverToken && !failedCovers[item.mangaId]}
                  <img
                    src={imgProxyUrl(item.coverToken)}
                    alt={item.title}
                    loading="lazy"
                    decoding="async"
                    onerror={() => {
                      failedCovers = { ...failedCovers, [item.mangaId]: true };
                    }}
                    class="h-full w-full object-cover transition-transform duration-200 ease-out group-hover:scale-[1.03]"
                  />
                {:else}
                  <div
                    class="flex h-full w-full items-center justify-center"
                    style="color: var(--text-muted);"
                  >
                    <BookOpen size={32} strokeWidth={1.5} />
                  </div>
                {/if}
                <span
                  class="pointer-events-none absolute inset-x-0 bottom-0 h-16 bg-gradient-to-t from-black/45 via-black/10 to-transparent"
                  aria-hidden="true"
                ></span>
              </button>

              <div class="pointer-events-none absolute top-2 left-2 flex flex-col items-start gap-1">
                {#if item.hasNew}
                  <span
                    class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold shadow-sm"
                    style="background-color: var(--accent); color: var(--accent-fg);"
                  >
                    <Sparkles size={10} />
                    {t.badgeNew}
                  </span>
                {:else if completedOnly}
                  <span
                    class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold text-white shadow-sm backdrop-blur-md"
                    style="background-color: rgba(16, 185, 129, 0.88);"
                  >
                    <CheckCircle2 size={10} />
                    {t.badgeCompleted}
                  </span>
                {/if}

                {#if isTachibkCatalogSite(item.site)}
                  <span
                    class="inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-semibold text-white shadow-sm backdrop-blur-md"
                    style="background-color: rgba(11, 11, 15, 0.78);"
                  >
                    {t.tachibkCatalogShortBadge}
                  </span>
                {/if}

                {#if item.unreadCount > 0}
                  <span
                    class="inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-semibold text-white shadow-sm backdrop-blur-md"
                    style="background-color: rgba(11, 11, 15, 0.78);"
                  >
                    {t.unreadBadge(item.unreadCount)}
                  </span>
                {/if}
              </div>

              <button
                type="button"
                onclick={(e) => removeBookmark(item.mangaId, e)}
                title={t.bookmarkRemove}
                aria-label={t.bookmarkRemove}
                class="btn-3d absolute top-2 right-2 flex h-7 w-7 items-center justify-center rounded-xl text-white opacity-85 backdrop-blur-md hover:opacity-100"
                style="background-color: rgba(11, 11, 15, 0.68);"
              >
                <Trash2 size={13} />
              </button>

              <button
                type="button"
                onclick={(e) => goMangaWithHero(item, e.currentTarget)}
                class="btn-3d absolute right-2 bottom-2 rounded-xl px-2.5 py-1 text-[11px] font-semibold text-white opacity-95 backdrop-blur-md sm:opacity-0 sm:group-hover:opacity-100"
                style="background-color: rgba(11, 11, 15, 0.78);"
              >
                {t.chaptersBtn}
              </button>
            </div>

            <div class="card-body flex flex-1 flex-col gap-1.5 p-3">
              <button
                type="button"
                onclick={(e) => goMangaWithHero(item, e.currentTarget)}
                class="card-title line-clamp-2 cursor-pointer text-left font-semibold hover:underline"
                title={item.title}
              >
                {item.title}
              </button>
              <div class="card-meta flex items-center gap-1 truncate" style="color: var(--text-muted);">
                {#if item.chapterName}
                  <Play size={10} class="shrink-0" />
                  <span class="truncate">{item.chapterName}</span>
                {:else if item.latestChapterName}
                  <span class="truncate">{item.latestChapterName}</span>
                {:else}
                  <span>{t.chapterCount(item.totalChapters)}</span>
                {/if}
              </div>

              {#if item.chapterUrl}
                <div class="mt-auto pt-1.5">
                  <div
                    class="h-1 w-full overflow-hidden rounded-full"
                    style="background-color: var(--surface-hover);"
                  >
                    <div
                      class="h-full w-full origin-left rounded-full transition-transform duration-300 ease-out"
                      style="transform: scaleX({Math.max(0, Math.min(1, progressPct(item) / 100))}); background-color: {completedOnly ? '#10b981' : 'var(--accent)'};"
                    ></div>
                  </div>
                </div>
              {/if}
            </div>
          </article>
        </div>
      {/each}
    </div>
  {/if}
</div>
