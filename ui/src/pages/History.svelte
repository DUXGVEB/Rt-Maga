<script lang="ts">
  import { BookOpen, CheckCircle2, Clock, Sparkles, Trash2 } from "lucide-svelte";
  import { api, imgProxyUrl } from "../lib/api";
  import { libraryCache } from "../lib/stores/libraryCache.svelte";
  import { router } from "../lib/stores/router.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { updatesStore } from "../lib/stores/updates.svelte";
  import type { HistoryItemView } from "../lib/types";

  let loading = $state(!libraryCache.historyLoaded);
  let confirmingClear = $state(false);
  let failedCovers = $state<Record<number, boolean>>({});
  let loadSeq = 0;

  const items = $derived(libraryCache.history);
  const t = $derived(settingsStore.t);

  async function load(showSkeleton: boolean) {
    const seq = ++loadSeq;
    if (showSkeleton) loading = true;
    try {
      const loaded = await api.listHistory();
      if (seq !== loadSeq) return;
      libraryCache.setHistory(loaded);
      updatesStore.setNewCountFromHistory(loaded.filter((i) => i.hasNew).length);
    } catch {
      if (seq !== loadSeq) return;
      if (!libraryCache.historyLoaded) {
        libraryCache.setHistory([]);
      }
    } finally {
      if (seq === loadSeq) {
        loading = false;
      }
    }
  }

  $effect(() => {
    void updatesStore.lastFound;
    void load(!libraryCache.historyLoaded);
  });

  function resumeFromCover(item: HistoryItemView) {
    if (item.isCompleted && !item.hasNew) {
      toasts.push(t.toastMangaCompletedNoResume, "info", 2800);
      return;
    }
    router.goReader(item.mangaId, item.chapterUrl, item.scrollRatio, item.pageIndex);
  }

  async function removeOne(mangaId: number, e: MouseEvent) {
    e.stopPropagation();
    try {
      await api.removeHistory(mangaId);
      libraryCache.removeHistoryItem(mangaId);
      updatesStore.setNewCountFromHistory(libraryCache.history.filter((i) => i.hasNew).length);
      toasts.push(t.toastRemovedHistory, "info");
    } catch {
      toasts.push(t.toastHistoryUpdateFailed, "error");
    }
  }

  async function clearAll() {
    try {
      await api.clearHistory();
      libraryCache.clearHistory();
      confirmingClear = false;
      updatesStore.setNewCountFromHistory(0);
      toasts.push(t.toastClearedHistory, "info");
    } catch {
      toasts.push(t.toastHistoryUpdateFailed, "error");
    }
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

<div class="mx-auto flex w-full max-w-4xl flex-col gap-6 px-5 py-6 sm:px-8 sm:py-8">
  <div class="flex items-center justify-between gap-4">
    <div>
      <h1 class="text-2xl font-bold tracking-tight">{t.historyTitle}</h1>
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
    </div>
  {:else}
    {#each grouped() as group (group.key)}
      <section class="flex flex-col gap-2.5">
        <h2 class="text-xs font-semibold tracking-wider uppercase" style="color: var(--text-muted);">
          {group.label}
        </h2>
        <div class="flex flex-col gap-2">
          {#each group.list as item (item.mangaId)}
            {@const completedOnly = item.isCompleted && !item.hasNew}
            <div
              class="flex items-center gap-4 rounded-xl border p-3 transition-colors"
              style="background-color: var(--surface); border-color: var(--border);"
            >
              <button
                type="button"
                onclick={() => resumeFromCover(item)}
                title={completedOnly ? t.toastMangaCompletedNoResume : t.coverTapResumeHint}
                class="h-16 w-12 shrink-0 cursor-pointer overflow-hidden rounded-lg transition-transform hover:scale-[1.03]"
                style="background-color: var(--surface-subtle);"
              >
                {#if item.coverToken && !failedCovers[item.mangaId]}
                  <img
                    src={imgProxyUrl(item.coverToken)}
                    alt={item.title}
                    loading="lazy"
                    onerror={() => {
                      failedCovers = { ...failedCovers, [item.mangaId]: true };
                    }}
                    class="h-full w-full object-cover"
                  />
                {:else}
                  <div class="flex h-full w-full items-center justify-center" style="color: var(--text-muted);">
                    <BookOpen size={18} />
                  </div>
                {/if}
              </button>

              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-2">
                  <button
                    type="button"
                    onclick={() => router.goManga(item.mangaId)}
                    class="truncate text-left text-sm font-semibold hover:underline"
                  >
                    {item.title}
                  </button>
                  {#if item.hasNew}
                    <span
                      class="inline-flex shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold"
                      style="background-color: var(--accent); color: var(--accent-fg);"
                    >
                      <Sparkles size={10} />
                      {item.latestChapterName ?? t.badgeNew}
                    </span>
                  {:else if completedOnly}
                    <span
                      class="inline-flex shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold"
                      style="background-color: rgba(16, 185, 129, 0.14); color: #10b981;"
                    >
                      <CheckCircle2 size={10} />
                      {t.badgeCompleted}
                    </span>
                  {/if}
                  {#if item.unreadCount > 0}
                    <span
                      class="inline-flex shrink-0 items-center rounded-full px-2 py-0.5 text-[10px] font-semibold"
                      style="background-color: var(--surface-hover); color: var(--text-muted);"
                    >
                      {t.unreadBadge(item.unreadCount)}
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
                  onclick={() => router.goManga(item.mangaId)}
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
