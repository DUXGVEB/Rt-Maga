<script lang="ts">
  import { BookOpen, Sparkles } from "lucide-svelte";
  import { imgProxyUrl } from "../api";
  import { router } from "../stores/router.svelte";
  import type { HistoryItemView } from "../types";

  let { item }: { item: HistoryItemView } = $props();

  const progressPct = $derived(() => {
    if (item.totalChapters <= 0 || item.currentChapterIdx === null) {
      return Math.round(item.scrollRatio * 100);
    }
    return Math.min(100, Math.round(((item.currentChapterIdx + 1) / item.totalChapters) * 100));
  });

  function openContinue() {
    router.goReader(item.mangaId, item.chapterUrl, item.scrollRatio);
  }

  function openDetail(e: MouseEvent) {
    e.stopPropagation();
    router.goManga(item.mangaId);
  }
</script>

<div
  role="button"
  tabindex="0"
  onclick={openContinue}
  onkeydown={(e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      openContinue();
    }
  }}
  class="group relative flex cursor-pointer flex-col overflow-hidden rounded-xl border transition-all duration-150 hover:-translate-y-0.5"
  style="background-color: var(--surface); border-color: var(--border); box-shadow: var(--shadow-sm);"
>
  <div class="relative aspect-[3/4] w-full overflow-hidden" style="background-color: var(--surface-subtle);">
    {#if item.coverToken}
      <img
        src={imgProxyUrl(item.coverToken)}
        alt={item.title}
        loading="lazy"
        class="h-full w-full object-cover transition-transform duration-300 group-hover:scale-[1.03]"
      />
    {:else}
      <div class="flex h-full w-full items-center justify-center" style="color: var(--text-muted);">
        <BookOpen size={32} strokeWidth={1.5} />
      </div>
    {/if}

    {#if item.hasNew}
      <span
        class="absolute top-2 left-2 inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-semibold text-white shadow-sm"
        style="background-color: var(--accent);"
      >
        <Sparkles size={11} />
        New
      </span>
    {/if}

    <button
      type="button"
      onclick={openDetail}
      class="absolute right-2 bottom-2 rounded-lg px-2 py-1 text-[11px] font-medium text-white opacity-0 backdrop-blur-md transition-opacity group-hover:opacity-100"
      style="background-color: rgba(15, 15, 18, 0.72);"
    >
      Chapters
    </button>
  </div>

  <div class="flex flex-1 flex-col gap-1.5 p-3">
    <div class="line-clamp-2 text-sm font-semibold leading-snug" title={item.title}>
      {item.title}
    </div>
    <div class="truncate text-xs" style="color: var(--text-muted);">
      {item.chapterName}
    </div>

    <div class="mt-auto pt-1.5">
      <div class="h-1 w-full overflow-hidden rounded-full" style="background-color: var(--surface-hover);">
        <div
          class="h-full rounded-full transition-all"
          style="width: {progressPct()}%; background-color: var(--accent);"
        ></div>
      </div>
    </div>
  </div>
</div>
