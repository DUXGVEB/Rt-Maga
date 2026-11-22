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
    <h1 class="text-2xl font-bold tracking-tight">Dán link truyện để đọc ngay</h1>
    <p class="max-w-lg text-sm" style="color: var(--text-muted);">
      Hỗ trợ link trang truyện hoặc link chương từ TruyenQQ và hầu hết các web truyện tranh khác.
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
        <h2 class="text-base font-semibold">Có chap mới</h2>
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
      <h2 class="text-base font-semibold">Đọc tiếp</h2>
      {#if history.length > 0}
        <button
          type="button"
          onclick={() => router.goHistory()}
          class="text-xs font-medium transition-opacity hover:opacity-80"
          style="color: var(--accent);"
        >
          Xem tất cả lịch sử
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
        <div class="text-sm font-medium" style="color: var(--text);">Chưa có truyện nào gần đây</div>
        <p class="max-w-xs text-xs">
          Dán một link truyện vào ô phía trên để bắt đầu. Các truyện bạn đọc sẽ tự động lưu tại đây.
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
