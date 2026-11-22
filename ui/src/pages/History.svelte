<script lang="ts">
  import { BookOpen, Clock, Sparkles, Trash2 } from "lucide-svelte";
  import { api, imgProxyUrl } from "../lib/api";
  import { router } from "../lib/stores/router.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { updatesStore } from "../lib/stores/updates.svelte";
  import type { HistoryItemView } from "../lib/types";

  let items = $state<HistoryItemView[]>([]);
  let loading = $state(true);
  let confirmingClear = $state(false);

  const t = $derived(settingsStore.t);

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
    toasts.push(t.toastRemovedHistory, "info");
  }

  async function clearAll() {
    await api.clearHistory();
    items = [];
    confirmingClear = false;
    updatesStore.setNewCountFromHistory(0);
    toasts.push(t.toastClearedHistory, "info");
  }

  function formatTime(unixSecs: number): string {
    const date = new Date(unixSecs * 1000);
    return date.toLocaleString(settingsStore.locale, {
      hour: "2-digit",
      minute: "2-digit",
      month: "short",
      day: "2-digit",
    });
  }

  type GroupKey = "today" | "yesterday" | "earlier";

  function groupKey(unixSecs: number): GroupKey {
    const now = new Date();
    const d = new Date(unixSecs * 1000);
    const startToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    const startYesterday = startToday - 86_400_000;
    const ts = d.getTime();
    if (ts >= startToday) return "today";
    if (ts >= startYesterday) return "yesterday";
    return "earlier";
  }

  const grouped = $derived(() => {
    const order: GroupKey[] = ["today", "yesterday", "earlier"];
    const labels: Record<GroupKey, string> = {
      today: t.today,
      yesterday: t.yesterday,
      earlier: t.earlier,
    };
    const map = new Map<GroupKey, HistoryItemView[]>();
    for (const k of order) map.set(k, []);
    for (const item of items) {
      map.get(groupKey(item.readAt))?.push(item);
    }
    return order
      .map((key) => ({ key, label: labels[key], list: map.get(key) ?? [] }))
      .filter((g) => g.list.length > 0);
  });
</script>

<div class="mx-auto flex w-full max-w-4xl flex-col gap-6 px-8 py-8">
  <div class="flex items-center justify-between gap-4">
    <div>
      <h1 class="text-2xl font-bold tracking-tight">{t.historyTitle}</h1>
      <p class="mt-1 text-xs" style="color: var(--text-muted);">
        {t.historySubtitle}
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
            {t.confirmClear}
          </button>
          <button
            type="button"
            onclick={() => {
              confirmingClear = false;
            }}
            class="rounded-lg border px-3 py-1.5 text-xs"
            style="border-color: var(--border);"
          >
            {t.cancel}
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
          <span>{t.clearAll}</span>
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
      <div class="text-sm font-medium" style="color: var(--text);">{t.historyEmptyTitle}</div>
      <p class="text-xs">{t.historyEmptyDesc}</p>
    </div>
  {:else}
    {#each grouped() as group (group.key)}
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
                      {item.latestChapterName ?? t.badgeNew}
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
                  {t.chaptersBtn}
                </button>
                <button
                  type="button"
                  onclick={(e) => removeOne(item.mangaId, e)}
                  class="rounded-lg p-1.5 opacity-60 transition-opacity hover:opacity-100"
                  style="color: var(--danger);"
                  title={t.removeHistoryTooltip}
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
