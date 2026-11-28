<script module lang="ts">
  const DECODED_COVERS_MAX = 400;
  const decodedCoverTokens = new Set<string>();

  function markCoverDecoded(token: string) {
    if (!token) return;
    if (decodedCoverTokens.size >= DECODED_COVERS_MAX) {
      const first = decodedCoverTokens.values().next().value;
      if (first) decodedCoverTokens.delete(first);
    }
    decodedCoverTokens.add(token);
  }
</script>

<script lang="ts">
  import { Bookmark, BookOpen, CheckCircle2, Sparkles } from "lucide-svelte";
  import { imgProxyUrl } from "../api";
  import { isMetadataOnlyChapterUrl, isTachibkCatalogSite } from "../comicFormats";
  import { badgePop, pressable } from "../motion";
  import { heroTransitionStore } from "../stores/heroTransition.svelte";
  import { libraryCache } from "../stores/libraryCache.svelte";
  import { router } from "../stores/router.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
  import type { HistoryItemView } from "../types";

  let { item, priority = false }: { item: HistoryItemView; priority?: boolean } = $props();
  let coverError = $state(false);
  let coverLoaded = $state(false);
  let coverBoxEl = $state<HTMLDivElement | null>(null);
  let coverImgEl = $state<HTMLImageElement | null>(null);
  const t = $derived(settingsStore.t);

  $effect(() => {
    const tok = item.coverToken;
    coverError = false;
    coverLoaded = Boolean(
      tok &&
        (decodedCoverTokens.has(tok) ||
          (coverImgEl && coverImgEl.complete && coverImgEl.naturalWidth > 0))
    );
  });

  const progressPct = $derived(() => {
    if (item.totalChapters <= 0 || item.currentChapterIdx === null) {
      return Math.round(item.scrollRatio * 100);
    }
    return Math.min(100, Math.round(((item.currentChapterIdx + 1) / item.totalChapters) * 100));
  });

  const isCompletedOnly = $derived(item.isCompleted && !item.hasNew);
  const isCatalogOnly = $derived(isTachibkCatalogSite(item.site));

  function seedPreview() {
    libraryCache.setMangaPreview({
      id: item.mangaId,
      title: item.title,
      coverToken: item.coverToken,
      site: item.site,
      isCompleted: item.isCompleted,
      unreadCount: item.unreadCount,
      bookmarked: item.bookmarked,
    });
  }

  function openContinue() {
    if (isCatalogOnly || isMetadataOnlyChapterUrl(item.chapterUrl)) {
      seedPreview();
      heroTransitionStore.captureFromElement(coverBoxEl, item.coverToken, item.mangaId);
      router.goManga(item.mangaId);
      return;
    }
    if (isCompletedOnly) {
      toasts.push(t.toastMangaCompletedNoResume, "info", 2800);
      return;
    }
    router.goReader(item.mangaId, item.chapterUrl, item.scrollRatio, item.pageIndex);
  }

  function openDetail(e?: MouseEvent) {
    e?.stopPropagation();
    seedPreview();
    heroTransitionStore.captureFromElement(coverBoxEl, item.coverToken, item.mangaId);
    router.goManga(item.mangaId);
  }
</script>

<article
  use:pressable
  class="card-cq card-contain card-press surface-card group relative flex flex-col overflow-hidden rounded-2xl"
>
  <div
    bind:this={coverBoxEl}
    class="relative aspect-[3/4] w-full overflow-hidden"
    style="background-color: var(--surface-subtle);"
  >
    <button
      type="button"
      onclick={openContinue}
      title={isCompletedOnly ? t.toastMangaCompletedNoResume : t.coverTapResumeHint}
      class="block h-full w-full cursor-pointer text-left"
    >
      {#if item.coverToken && !coverError}
        <img
          bind:this={coverImgEl}
          src={imgProxyUrl(item.coverToken)}
          alt={item.title}
          loading={priority ? "eager" : "lazy"}
          fetchpriority={priority ? "high" : undefined}
          decoding="async"
          onload={() => {
            if (item.coverToken) markCoverDecoded(item.coverToken);
            coverLoaded = true;
          }}
          onerror={() => {
            coverError = true;
          }}
          class="h-full w-full object-cover transition-[transform,opacity] duration-150 ease-out group-hover:scale-[1.03] {coverLoaded
            ? 'opacity-100'
            : 'opacity-0'}"
        />
      {:else}
        <div class="flex h-full w-full items-center justify-center" style="color: var(--text-muted);">
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
      {:else if isCompletedOnly}
        <span
          class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold text-white shadow-sm backdrop-blur-md"
          style="background-color: rgba(16, 185, 129, 0.88);"
        >
          <CheckCircle2 size={10} />
          {t.badgeCompleted}
        </span>
      {/if}

      {#if isCatalogOnly}
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

    {#if item.bookmarked}
      <span
        transition:badgePop|local
        class="pointer-events-none absolute top-2 right-2 flex h-6 w-6 items-center justify-center rounded-full text-white shadow-sm backdrop-blur-md"
        style="background-color: rgba(11, 11, 15, 0.65);"
        title={t.navBookmarks}
      >
        <Bookmark size={12} fill="currentColor" />
      </span>
    {/if}

    <button
      type="button"
      onclick={openDetail}
      class="btn-3d absolute right-2 bottom-2 rounded-xl px-2.5 py-1 text-[11px] font-semibold text-white opacity-95 backdrop-blur-md sm:opacity-0 sm:group-hover:opacity-100"
      style="background-color: rgba(11, 11, 15, 0.78);"
    >
      {t.chaptersBtn}
    </button>
  </div>

  <div class="card-body flex flex-1 flex-col gap-1.5 p-3">
    <button
      type="button"
      onclick={() => openDetail()}
      class="card-title line-clamp-2 cursor-pointer text-left font-semibold hover:underline"
      title={item.title}
    >
      {item.title}
    </button>
    <div class="card-meta truncate" style="color: var(--text-muted);">
      {item.chapterName}
    </div>

    <div class="mt-auto pt-1.5">
      <div class="h-1 w-full overflow-hidden rounded-full" style="background-color: var(--surface-hover);">
        <div
          class="h-full w-full origin-left rounded-full transition-transform duration-300 ease-out"
          style="transform: scaleX({Math.max(0, Math.min(1, progressPct() / 100))}); background-color: {isCompletedOnly ? '#10b981' : 'var(--accent)'};"
        ></div>
      </div>
    </div>
  </div>
</article>
