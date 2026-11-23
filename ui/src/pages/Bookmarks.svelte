<script lang="ts">
  import { Bookmark, BookOpen, CheckCircle2, Play, Sparkles, Trash2 } from "lucide-svelte";
  import { api, imgProxyUrl } from "../lib/api";
  import { router } from "../lib/stores/router.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { updatesStore } from "../lib/stores/updates.svelte";
  import type { BookmarkItemView, BookmarkSort } from "../lib/types";

  let items = $state<BookmarkItemView[]>([]);
  let sort = $state<BookmarkSort>("newest");
  let loading = $state(true);
  let failedCovers = $state<Record<number, boolean>>({});
  let loadSeq = 0;

  const t = $derived(settingsStore.t);

  async function load(selectedSort: BookmarkSort, showSkeleton: boolean) {
    const seq = ++loadSeq;
    if (showSkeleton) loading = true;
    try {
      const loaded = await api.listBookmarks(selectedSort);
      if (seq !== loadSeq) return;
      items = loaded;
    } catch {
      if (seq !== loadSeq) return;
      items = [];
    } finally {
      if (seq === loadSeq) {
        loading = false;
      }
    }
  }

  $effect(() => {
    void updatesStore.lastFound;
    void load(sort, items.length === 0);
  });

  async function removeBookmark(mangaId: number, e: MouseEvent) {
    e.stopPropagation();
    try {
      await api.toggleBookmark(mangaId);
      items = items.filter((i) => i.mangaId !== mangaId);
      toasts.push(t.toastBookmarkRemoved, "info", 1800);
    } catch {
      toasts.push(t.toastHistoryUpdateFailed, "error");
    }
  }

  function openBookmarkCover(item: BookmarkItemView) {
    if (item.isCompleted && !item.hasNew) {
      toasts.push(t.toastMangaCompletedNoResume, "info", 2800);
      return;
    }
    if (item.chapterUrl) {
      router.goReader(
        item.mangaId,
        item.chapterUrl,
        item.scrollRatio ?? 0,
        item.pageIndex ?? 0
      );
    } else {
      router.goManga(item.mangaId);
    }
  }

  function progressPct(item: BookmarkItemView): number {
    if (item.totalChapters <= 0 || item.currentChapterIdx === null) {
      return Math.round((item.scrollRatio ?? 0) * 100);
    }
    return Math.min(100, Math.round(((item.currentChapterIdx + 1) / item.totalChapters) * 100));
  }
</script>

<div class="mx-auto flex w-full max-w-5xl flex-col gap-6 px-5 py-6 sm:px-8 sm:py-8">
  <div class="flex flex-wrap items-center justify-between gap-4">
    <div>
      <h1 class="text-2xl font-bold tracking-tight">{t.bookmarksTitle}</h1>
      <p class="mt-1 text-xs" style="color: var(--text-muted);">
        {t.bookmarksSubtitle}
      </p>
    </div>

    {#if items.length > 0}
      <div class="flex items-center gap-2">
        <span class="text-xs" style="color: var(--text-muted);">{t.bookmarksSortLabel}</span>
        <select
          bind:value={sort}
          class="rounded-lg border px-3 py-1.5 text-xs font-medium outline-none"
          style="background-color: var(--surface); border-color: var(--border); color: var(--text);"
        >
          <option value="newest">{t.bookmarksSortNewest}</option>
          <option value="has_new">{t.bookmarksSortHasNew}</option>
          <option value="recent_read">{t.bookmarksSortRecent}</option>
          <option value="title">{t.bookmarksSortTitle}</option>
        </select>
      </div>
    {/if}
  </div>

  {#if loading}
    <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5">
      {#each Array(5) as _, i (i)}
        <div
          class="aspect-[3/4] animate-pulse rounded-xl"
          style="background-color: var(--surface-hover);"
        ></div>
      {/each}
    </div>
  {:else if items.length === 0}
    <div
      class="flex flex-col items-center justify-center gap-3 rounded-2xl border border-dashed py-16 text-center"
      style="border-color: var(--border); color: var(--text-muted);"
    >
      <Bookmark size={30} strokeWidth={1.5} />
      <div class="text-sm font-medium" style="color: var(--text);">{t.bookmarksEmptyTitle}</div>
      <p class="max-w-sm text-xs">{t.bookmarksEmptyDesc}</p>
    </div>
  {:else}
    <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5">
      {#each items as item (item.mangaId)}
        {@const completedOnly = item.isCompleted && !item.hasNew}
        <article
          class="group relative flex flex-col overflow-hidden rounded-xl border transition-all duration-150 hover:-translate-y-0.5"
          style="background-color: var(--surface); border-color: var(--border); box-shadow: var(--shadow-sm);"
        >
          <div
            class="relative aspect-[3/4] w-full overflow-hidden"
            style="background-color: var(--surface-subtle);"
          >
            <button
              type="button"
              onclick={() => openBookmarkCover(item)}
              title={completedOnly ? t.toastMangaCompletedNoResume : t.coverTapResumeHint}
              class="block h-full w-full cursor-pointer text-left"
            >
              {#if item.coverToken && !failedCovers[item.mangaId]}
                <img
                  src={imgProxyUrl(item.coverToken)}
                  alt={item.title}
                  loading="lazy"
                  onerror={() => {
                    failedCovers = { ...failedCovers, [item.mangaId]: true };
                  }}
                  class="h-full w-full object-cover transition-transform duration-300 group-hover:scale-[1.03]"
                />
              {:else}
                <div
                  class="flex h-full w-full items-center justify-center"
                  style="color: var(--text-muted);"
                >
                  <BookOpen size={32} strokeWidth={1.5} />
                </div>
              {/if}
            </button>

            <div class="pointer-events-none absolute top-2 left-2 flex flex-col items-start gap-1">
              {#if item.hasNew}
                <span
                  class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-semibold text-white shadow-sm"
                  style="background-color: var(--accent);"
                >
                  <Sparkles size={11} />
                  {t.badgeNew}
                </span>
              {:else if completedOnly}
                <span
                  class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-semibold text-white shadow-sm backdrop-blur-md"
                  style="background-color: rgba(16, 185, 129, 0.85);"
                >
                  <CheckCircle2 size={11} />
                  {t.badgeCompleted}
                </span>
              {/if}

              {#if item.unreadCount > 0}
                <span
                  class="inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-semibold text-white shadow-sm backdrop-blur-md"
                  style="background-color: rgba(15, 15, 18, 0.78);"
                >
                  {t.unreadBadge(item.unreadCount)}
                </span>
              {/if}
            </div>

            <button
              type="button"
              onclick={(e) => removeBookmark(item.mangaId, e)}
              title={t.bookmarkRemove}
              class="absolute top-2 right-2 rounded-lg p-1.5 text-white opacity-85 backdrop-blur-md transition-opacity hover:opacity-100"
              style="background-color: rgba(15, 15, 18, 0.65);"
            >
              <Trash2 size={13} />
            </button>

            <button
              type="button"
              onclick={() => router.goManga(item.mangaId)}
              class="absolute right-2 bottom-2 rounded-lg px-2 py-1 text-[11px] font-medium text-white opacity-90 backdrop-blur-md transition-opacity sm:opacity-0 sm:group-hover:opacity-100"
              style="background-color: rgba(15, 15, 18, 0.72);"
            >
              {t.chaptersBtn}
            </button>
          </div>

          <div class="flex flex-1 flex-col gap-1.5 p-3">
            <button
              type="button"
              onclick={() => router.goManga(item.mangaId)}
              class="line-clamp-2 cursor-pointer text-left text-sm font-semibold leading-snug hover:underline"
              title={item.title}
            >
              {item.title}
            </button>
            <div class="flex items-center gap-1 truncate text-xs" style="color: var(--text-muted);">
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
                    class="h-full rounded-full transition-all"
                    style="width: {progressPct(item)}%; background-color: {completedOnly ? '#10b981' : 'var(--accent)'};"
                  ></div>
                </div>
              </div>
            {/if}
          </div>
        </article>
      {/each}
    </div>
  {/if}
</div>
