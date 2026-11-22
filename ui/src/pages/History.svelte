<script lang="ts">
  import { BookOpen, Clock, Sparkles, Trash2 } from "lucide-svelte";
  import { api, imgProxyUrl } from "../lib/api";
  import { router } from "../lib/stores/router.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { updatesStore } from "../lib/stores/updates.svelte";
  import type { HistoryItemView } from "../lib/types";

  let items = $state<HistoryItemView[]>([]);
  let loading = $state(true);
  let confirmingClear = $state(false);

  async function load() {
    loading = true;
    try {
      items = await api.listHistory();
      updatesStore.setNewCountFromHistory(items.filter((i) => i.hasNew).length);
    } catch {
      items = [];
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    load();
  });

  async function removeOne(mangaId: number, e: MouseEvent) {
    e.stopPropagation();
    await api.removeHistory(mangaId);
    items = items.filter((i) => i.mangaId !== mangaId);
    updatesStore.setNewCountFromHistory(items.filter((i) => i.hasNew).length);
    toasts.push("Đã xoá khỏi lịch sử và ngừng kiểm tra chap mới.", "info");
  }

  async function clearAll() {
    await api.clearHistory();
    items = [];
    confirmingClear = false;
    updatesStore.setNewCountFromHistory(0);
    toasts.push("Đã xoá toàn bộ lịch sử đọc.", "info");
  }

  function formatTime(unixSecs: number): string {
    const date = new Date(unixSecs * 1000);
    return date.toLocaleString("vi-VN", {
      hour: "2-digit",
      minute: "2-digit",
      day: "2-digit",
      month: "2-digit",
    });
  }

  function groupLabel(unixSecs: number): string {
    const now = new Date();
    const d = new Date(unixSecs * 1000);
    const startToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    const startYesterday = startToday - 86_400_000;
    const t = d.getTime();
    if (t >= startToday) return "Hôm nay";
    if (t >= startYesterday) return "Hôm qua";
    return "Cũ hơn";
  }

  const grouped = $derived(() => {
    const order = ["Hôm nay", "Hôm qua", "Cũ hơn"];
    const map = new Map<string, HistoryItemView[]>();
    for (const label of order) map.set(label, []);
    for (const item of items) {
      const label = groupLabel(item.readAt);
      map.get(label)?.push(item);
    }
    return order
      .map((label) => ({ label, list: map.get(label) ?? [] }))
      .filter((g) => g.list.length > 0);
  });
</script>

<div class="mx-auto flex w-full max-w-4xl flex-col gap-6 px-8 py-8">
  <div class="flex items-center justify-between gap-4">
    <div>
      <h1 class="text-2xl font-bold tracking-tight">Lịch sử đọc</h1>
      <p class="mt-1 text-xs" style="color: var(--text-muted);">
        App tự động kiểm tra chap mới cho các truyện có mặt ở đây. Xoá khỏi lịch sử sẽ ngừng kiểm tra.
      </p>
    </div>

    {#if items.length > 0}
      {#if confirmingClear}
        <div class="flex items-center gap-2">
          <button
            type="button"
            onclick={clearAll}
            class="rounded-lg px-3 py-1.5 text-xs font-semibold text-white"
            style="background-color: var(--danger);"
          >
            Xác nhận xoá hết
          </button>
          <button
            type="button"
            onclick={() => {
              confirmingClear = false;
            }}
            class="rounded-lg border px-3 py-1.5 text-xs"
            style="border-color: var(--border);"
          >
            Huỷ
          </button>
        </div>
      {:else}
        <button
          type="button"
          onclick={() => {
            confirmingClear = true;
          }}
          class="inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium transition-colors"
          style="background-color: var(--surface); border-color: var(--border); color: var(--danger);"
        >
          <Trash2 size={14} />
          <span>Xoá tất cả</span>
        </button>
      {/if}
    {/if}
  </div>

  {#if loading}
    <div class="flex flex-col gap-3">
      {#each Array(4) as _, i (i)}
        <div
          class="h-20 animate-pulse rounded-xl"
          style="background-color: var(--surface-hover);"
        ></div>
      {/each}
    </div>
  {:else if items.length === 0}
    <div
      class="flex flex-col items-center justify-center gap-3 rounded-2xl border border-dashed py-16 text-center"
      style="border-color: var(--border); color: var(--text-muted);"
    >
      <Clock size={30} strokeWidth={1.5} />
      <div class="text-sm font-medium" style="color: var(--text);">Lịch sử trống</div>
      <p class="text-xs">Truyện sẽ xuất hiện tại đây ngay khi bạn mở đọc chương đầu tiên.</p>
    </div>
  {:else}
    {#each grouped() as group (group.label)}
      <section class="flex flex-col gap-2.5">
        <h2 class="text-xs font-semibold tracking-wider uppercase" style="color: var(--text-muted);">
          {group.label}
        </h2>
        <div class="flex flex-col gap-2">
          {#each group.list as item (item.mangaId)}
            <div
              role="button"
              tabindex="0"
              onclick={() => router.goReader(item.mangaId, item.chapterUrl, item.scrollRatio)}
              onkeydown={(e) => {
                if (e.key === "Enter") {
                  router.goReader(item.mangaId, item.chapterUrl, item.scrollRatio);
                }
              }}
              class="flex cursor-pointer items-center gap-4 rounded-xl border p-3 transition-colors hover:brightness-95"
              style="background-color: var(--surface); border-color: var(--border);"
            >
              <div
                class="h-16 w-12 shrink-0 overflow-hidden rounded-lg"
                style="background-color: var(--surface-subtle);"
              >
                {#if item.coverToken}
                  <img
                    src={imgProxyUrl(item.coverToken)}
                    alt={item.title}
                    loading="lazy"
                    class="h-full w-full object-cover"
                  />
                {:else}
                  <div class="flex h-full w-full items-center justify-center" style="color: var(--text-muted);">
                    <BookOpen size={18} />
                  </div>
                {/if}
              </div>

              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2">
                  <span class="truncate text-sm font-semibold">{item.title}</span>
                  {#if item.hasNew}
                    <span
                      class="inline-flex shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold text-white"
                      style="background-color: var(--accent);"
                    >
                      <Sparkles size={10} />
                      {item.latestChapterName ?? "Chap mới"}
                    </span>
                  {/if}
                </div>
                <div class="mt-0.5 flex flex-wrap items-center gap-2 text-xs" style="color: var(--text-muted);">
                  <span>{item.chapterName}</span>
                  <span>·</span>
                  <span>{formatTime(item.readAt)}</span>
                </div>
              </div>

              <div class="flex shrink-0 items-center gap-1.5">
                <button
                  type="button"
                  onclick={(e) => {
                    e.stopPropagation();
                    router.goManga(item.mangaId);
                  }}
                  class="rounded-lg border px-2.5 py-1.5 text-xs font-medium"
                  style="border-color: var(--border); background-color: var(--surface-subtle);"
                >
                  Chương
                </button>
                <button
                  type="button"
                  onclick={(e) => removeOne(item.mangaId, e)}
                  class="rounded-lg p-1.5 opacity-60 transition-opacity hover:opacity-100"
                  style="color: var(--danger);"
                  title="Xoá khỏi lịch sử"
                >
                  <Trash2 size={15} />
                </button>
              </div>
            </div>
          {/each}
        </div>
      </section>
    {/each}
  {/if}
</div>
