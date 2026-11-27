<script lang="ts">
  import { BookOpen, CheckCircle2, Clock, Sparkles, Trash2 } from "lucide-svelte";
  import { flip } from "svelte/animate";
  import { api, imgProxyUrl } from "../lib/api";
  import { isMetadataOnlyChapterUrl, isTachibkCatalogSite } from "../lib/comicFormats";
  import {
    flipDuration,
    itemExit,
    popoverPop,
    pressable,
    shouldStaggerItem,
    staggerDelay,
    viewReveal,
  } from "../lib/motion";
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
    if (isTachibkCatalogSite(item.site) || isMetadataOnlyChapterUrl(item.chapterUrl)) {
      router.goManga(item.mangaId);
      return;
    }
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

<div class="mx-auto flex w-full max-w-4xl flex-col gap-6 px-4 py-6 sm:px-8 sm:py-8">
  <div class="flex flex-wrap items-center justify-between gap-4">
    <div class="flex items-center gap-2.5">
      <h1 class="text-2xl font-bold tracking-tight">{t.historyTitle}</h1>
      {#if items.length > 0}
        <span
          class="rounded-full px-2.5 py-0.5 text-xs font-semibold tabular-nums"
          style="background-color: var(--surface-hover); color: var(--text-muted);"
        >
          {items.length}
        </span>
      {/if}
    </div>

    {#if items.length > 0}
      {#if confirmingClear}
        <div in:popoverPop|local={{ duration: 150, y: 0 }} class="flex items-center gap-2">
          <button
            type="button"
            onclick={clearAll}
            class="gpu-press min-h-[38px] rounded-xl px-3.5 py-1.5 text-xs font-semibold text-white"
            style="background-color: var(--danger);"
          >
            {t.confirmClear}
          </button>
          <button
            type="button"
            onclick={() => {
              confirmingClear = false;
            }}
            class="gpu-press min-h-[38px] rounded-xl border px-3.5 py-1.5 text-xs font-medium"
            style="border-color: var(--border); background-color: var(--surface);"
          >
            {t.cancel}
          </button>
        </div>
      {:else}
        <span in:popoverPop|local={{ duration: 140, y: 0 }} class="inline-flex">
          <button
            type="button"
            onclick={() => {
              confirmingClear = true;
            }}
            class="surface-card gpu-press inline-flex min-h-[38px] items-center gap-1.5 rounded-xl px-3.5 py-1.5 text-xs font-medium"
            style="color: var(--danger);"
          >
            <Trash2 size={14} />
            <span>{t.clearAll}</span>
          </button>
        </span>
      {/if}
    {/if}
  </div>

  {#if loading}
    <div class="flex flex-col gap-3">
      {#each Array(4) as _, i (i)}
        <div
          class="h-20 animate-pulse rounded-2xl"
          style="background-color: var(--surface-hover);"
        ></div>
      {/each}
    </div>
  {:else if items.length === 0}
    <div
      in:viewReveal|local={{ duration: 165, y: 6 }}
      class="flex flex-col items-center justify-center gap-3 rounded-2xl border border-dashed py-16 text-center"
      style="border-color: var(--border-strong); color: var(--text-muted);"
    >
      <Clock size={30} strokeWidth={1.5} />
      <div class="text-sm font-medium" style="color: var(--text);">{t.historyEmptyTitle}</div>
    </div>
  {:else}
    {#each grouped() as group (group.key)}
      <section out:itemExit|local class="flex flex-col gap-2.5">
        <h2 class="text-xs font-semibold tracking-wider uppercase" style="color: var(--text-muted);">
          {group.label}
        </h2>
        <div class="list-contain flex flex-col gap-2">
          {#each group.list as item, idx (item.mangaId)}
            {@const completedOnly = item.isCompleted && !item.hasNew}
            {@const animateStagger = shouldStaggerItem(idx, group.list.length)}
            <div
              animate:flip={{ duration: flipDuration(175, group.list.length) }}
              out:itemExit|local
              class="card-contain surface-card flex items-center gap-3.5 rounded-2xl p-3 sm:gap-4"
              class:stagger-item={animateStagger}
              style={animateStagger ? `--stagger-delay: ${staggerDelay(idx, 18, 180, group.list.length)}ms;` : undefined}
            >
              <button
                type="button"
                use:pressable
                onclick={() => resumeFromCover(item)}
                title={completedOnly ? t.toastMangaCompletedNoResume : t.coverTapResumeHint}
                class="gpu-press h-16 w-12 shrink-0 cursor-pointer overflow-hidden rounded-xl hover:scale-[1.03]"
                style="background-color: var(--surface-subtle);"
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
                  {#if isTachibkCatalogSite(item.site)}
                    <span
                      class="inline-flex shrink-0 items-center rounded-full px-2 py-0.5 text-[10px] font-semibold"
                      style="background-color: var(--accent-soft); color: var(--accent);"
                    >
                      {t.tachibkCatalogShortBadge}
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
                  class="gpu-press min-h-[38px] rounded-xl border px-3 py-1.5 text-xs font-medium"
                  style="border-color: var(--border); background-color: var(--surface-subtle);"
                >
                  {t.chaptersBtn}
                </button>
                <button
                  type="button"
                  onclick={(e) => removeOne(item.mangaId, e)}
                  class="gpu-press flex min-h-[38px] min-w-[38px] items-center justify-center rounded-xl opacity-70 hover:opacity-100"
                  style="color: var(--danger);"
                  title={t.removeHistoryTooltip}
                  aria-label={t.removeHistoryTooltip}
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
