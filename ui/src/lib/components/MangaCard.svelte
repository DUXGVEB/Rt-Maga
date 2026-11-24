<script lang="ts">
  import { Bookmark, BookOpen, CheckCircle2, Sparkles } from "lucide-svelte";
  import { imgProxyUrl } from "../api";
  import { router } from "../stores/router.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
  import type { HistoryItemView } from "../types";

  let { item }: { item: HistoryItemView } = $props();
  let coverError = $state(false);
  const t = $derived(settingsStore.t);

  $effect(() => {
    void item.coverToken;
    coverError = false;
  });

  const progressPct = $derived(() => {
    if (item.totalChapters <= 0 || item.currentChapterIdx === null) {
      return Math.round(item.scrollRatio * 100);
    }
    return Math.min(100, Math.round(((item.currentChapterIdx + 1) / item.totalChapters) * 100));
  });

  const isCompletedOnly = $derived(item.isCompleted && !item.hasNew);

  function openContinue() {
    if (isCompletedOnly) {
      toasts.push(t.toastMangaCompletedNoResume, "info", 2800);
      return;
    }
    router.goReader(item.mangaId, item.chapterUrl, item.scrollRatio, item.pageIndex);
  }

  function openDetail(e?: MouseEvent) {
    e?.stopPropagation();
    router.goManga(item.mangaId);
  }
</script>

<article
  class="group relative flex flex-col overflow-hidden rounded-xl border transition-all duration-150 hover:-translate-y-0.5"
  style="background-color: var(--surface); border-color: var(--border); box-shadow: var(--shadow-sm);"
>
  <div class="relative aspect-[3/4] w-full overflow-hidden" style="background-color: var(--surface-subtle);">
    <button
      type="button"
      onclick={openContinue}
      title={isCompletedOnly ? t.toastMangaCompletedNoResume : t.coverTapResumeHint}
      class="block h-full w-full cursor-pointer text-left"
    >
      {#if item.coverToken && !coverError}
        <img
          src={imgProxyUrl(item.coverToken)}
          alt={item.title}
          loading="lazy"
          onerror={() => {
            coverError = true;
          }}
          class="h-full w-full object-cover transition-transform duration-300 group-hover:scale-[1.03]"
        />
      {:else}
        <div class="flex h-full w-full items-center justify-center" style="color: var(--text-muted);">
          <BookOpen size={32} strokeWidth={1.5} />
        </div>
      {/if}
    </button>

    <div class="pointer-events-none absolute top-2 left-2 flex flex-col items-start gap-1">
      {#if item.hasNew}
        <span
          class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-semibold shadow-sm"
          style="background-color: var(--accent); color: var(--accent-fg);"
        >
          <Sparkles size={11} />
          {t.badgeNew}
        </span>
      {:else if isCompletedOnly}
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

    {#if item.bookmarked}
      <span
        class="pointer-events-none absolute top-2 right-2 flex h-6 w-6 items-center justify-center rounded-full text-white shadow-sm backdrop-blur-md"
        style="background-color: rgba(15, 15, 18, 0.62);"
        title={t.navBookmarks}
      >
        <Bookmark size={12} fill="currentColor" />
      </span>
    {/if}

    <button
      type="button"
      onclick={openDetail}
      class="absolute right-2 bottom-2 rounded-lg px-2 py-1 text-[11px] font-medium text-white opacity-90 backdrop-blur-md transition-opacity sm:opacity-0 sm:group-hover:opacity-100"
      style="background-color: rgba(15, 15, 18, 0.74);"
    >
      {t.chaptersBtn}
    </button>
  </div>

  <div class="flex flex-1 flex-col gap-1.5 p-3">
    <button
      type="button"
      onclick={() => openDetail()}
      class="line-clamp-2 cursor-pointer text-left text-sm font-semibold leading-snug transition-colors hover:underline"
      title={item.title}
    >
      {item.title}
    </button>
    <div class="truncate text-xs" style="color: var(--text-muted);">
      {item.chapterName}
    </div>

    <div class="mt-auto pt-1.5">
      <div class="h-1 w-full overflow-hidden rounded-full" style="background-color: var(--surface-hover);">
        <div
          class="h-full rounded-full transition-all"
          style="width: {progressPct()}%; background-color: {isCompletedOnly ? '#10b981' : 'var(--accent)'};"
        ></div>
      </div>
    </div>
  </div>
</article>
