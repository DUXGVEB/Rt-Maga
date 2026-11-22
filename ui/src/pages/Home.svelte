<script lang="ts">
  import { BookOpen, Sparkles } from "lucide-svelte";
  import { api } from "../lib/api";
  import LinkInput from "../lib/components/LinkInput.svelte";
  import MangaCard from "../lib/components/MangaCard.svelte";
  import { router } from "../lib/stores/router.svelte";
  import { updatesStore } from "../lib/stores/updates.svelte";
  import type { HistoryItemView } from "../lib/types";

  let history = $state<HistoryItemView[]>([]);
  let loading = $state(true);

  async function loadHistory() {
    loading = true;
    try {
      const items = await api.listHistory();
      history = items;
      updatesStore.setNewCountFromHistory(items.filter((i) => i.hasNew).length);
    } catch {
      history = [];
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void updatesStore.lastFound;
    loadHistory();
  });

  const updatedItems = $derived(history.filter((h) => h.hasNew));
  const recentItems = $derived(history.slice(0, 12));
</script>

<div class="mx-auto flex w-full max-w-5xl flex-col gap-10 px-8 py-10">
  <!-- Hero Paste Box -->
  <section class="flex flex-col items-center gap-4 pt-4 text-center">
    <h1 class="text-2xl font-bold tracking-tight">Paste a manga link to start reading</h1>
    <p class="max-w-lg text-sm" style="color: var(--text-muted);">
      Supports manga overview links and direct chapter URLs from TruyenQQ and most manga websites.
    </p>
    <div class="mt-1 flex w-full justify-center">
      <LinkInput />
    </div>
  </section>

  <!-- New chapters section -->
  {#if updatedItems.length > 0}
    <section class="flex flex-col gap-4">
      <div class="flex items-center gap-2">
        <Sparkles size={17} style="color: var(--accent);" />
        <h2 class="text-base font-semibold">New Chapters</h2>
        <span
          class="rounded-full px-2 py-0.5 text-xs font-medium"
          style="background-color: var(--accent-soft); color: var(--accent);"
        >
          {updatedItems.length}
        </span>
      </div>
      <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5">
        {#each updatedItems as item (item.mangaId)}
          <MangaCard {item} />
        {/each}
      </div>
    </section>
  {/if}

  <!-- Continue reading -->
  <section class="flex flex-col gap-4">
    <div class="flex items-center justify-between">
      <h2 class="text-base font-semibold">Continue Reading</h2>
      {#if history.length > 0}
        <button
          type="button"
          onclick={() => router.goHistory()}
          class="text-xs font-medium transition-opacity hover:opacity-80"
          style="color: var(--accent);"
        >
          View all history
        </button>
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
    {:else if recentItems.length === 0}
      <div
        class="flex flex-col items-center justify-center gap-3 rounded-2xl border border-dashed py-14 text-center"
        style="border-color: var(--border); color: var(--text-muted);"
      >
        <BookOpen size={30} strokeWidth={1.5} />
        <div class="text-sm font-medium" style="color: var(--text);">No recent manga yet</div>
        <p class="max-w-xs text-xs">
          Paste any manga link above to start reading. Your reading progress will appear here automatically.
        </p>
      </div>
    {:else}
      <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5">
        {#each recentItems as item (item.mangaId)}
          <MangaCard {item} />
        {/each}
      </div>
    {/if}
  </section>
</div>
