<script lang="ts">
  import { ArrowDown, ArrowUp, BookOpen, CheckCircle2, Play, RotateCcw, Sparkles, X } from "lucide-svelte";
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import LinkInput from "../lib/components/LinkInput.svelte";
  import MangaCard from "../lib/components/MangaCard.svelte";
  import { libraryCache } from "../lib/stores/libraryCache.svelte";
  import { router } from "../lib/stores/router.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { updatesStore } from "../lib/stores/updates.svelte";
  import type { CrashRecoverySession, HistoryItemView, LibrarySortField, SortDirection } from "../lib/types";

  let loading = $state(!libraryCache.historyLoaded);
  let crashSession = $state<CrashRecoverySession | null>(null);
  let sortField = $state<LibrarySortField>("added");
  let sortDir = $state<SortDirection>("desc");
  let loadSeq = 0;

  const history = $derived(libraryCache.history);
  const t = $derived(settingsStore.t);

  function sortItems(items: HistoryItemView[]): HistoryItemView[] {
    const out = [...items];
    const mult = sortDir === "desc" ? -1 : 1;
    out.sort((a, b) => {
      if (sortField === "title") {
        const cmp = a.title.localeCompare(b.title, undefined, { sensitivity: "base" });
        if (cmp !== 0) return cmp * mult;
        return (a.addedAt - b.addedAt) * mult;
      }
      if (a.addedAt !== b.addedAt) {
        return (a.addedAt - b.addedAt) * mult;
      }
      return (a.readAt - b.readAt) * mult;
    });
    return out;
  }

  async function loadHistory(showSkeleton: boolean) {
    const seq = ++loadSeq;
    if (showSkeleton) loading = true;
    try {
      const items = await api.listHistory();
      if (seq !== loadSeq) return;
      libraryCache.setHistory(items);
      updatesStore.setNewCountFromHistory(items.filter((i) => i.hasNew).length);
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

  onMount(async () => {
    try {
      crashSession = await api.getCrashRecoverySession();
    } catch {
      crashSession = null;
    }
  });

  async function resumeCrashSession() {
    if (!crashSession) return;
    const s = crashSession;
    crashSession = null;
    await api.clearReaderSession().catch(() => {});
    router.goReader(s.mangaId, s.chapterUrl, s.scrollRatio, s.pageIndex);
  }

  async function dismissCrashSession() {
    crashSession = null;
    try {
      await api.clearReaderSession();
    } catch {
      // Ignore cleanup failures.
    }
  }

  $effect(() => {
    void updatesStore.lastFound;
    void router.navPulse;
    void loadHistory(!libraryCache.historyLoaded);
  });

  // Partition history strictly without duplicates and sort according to user preference:
  // 1) updatedItems: hasNew === true (automatically returns completed manga to active top area)
  // 2) activeItems: not completed and no new badge
  // 3) completedItems: completed and no new chapters -> bottom Completed section
  const updatedItems = $derived(sortItems(history.filter((h) => h.hasNew)));
  const activeItems = $derived(sortItems(history.filter((h) => !h.hasNew && !h.isCompleted)));
  const completedItems = $derived(sortItems(history.filter((h) => !h.hasNew && h.isCompleted)));
</script>

<div class="mx-auto flex w-full max-w-5xl flex-col gap-10 px-5 py-8 sm:px-8 sm:py-10">
  <!-- Non-blocking Crash Recovery Banner -->
  {#if crashSession}
    <div
      class="flex flex-wrap items-center justify-between gap-3 rounded-2xl border px-4 py-3 text-xs shadow-sm"
      style="background-color: color-mix(in srgb, var(--surface) 88%, var(--accent-soft)); border-color: var(--accent);"
    >
      <div class="flex min-w-0 items-center gap-2.5">
        <RotateCcw size={16} class="shrink-0" style="color: var(--accent);" />
        <div class="min-w-0">
          <div class="font-semibold" style="color: var(--text);">{t.crashRecoveryTitle}</div>
          <div class="truncate" style="color: var(--text-muted);">
            {t.crashRecoveryDesc(crashSession.mangaTitle, crashSession.chapterName)}
          </div>
        </div>
      </div>

      <div class="flex shrink-0 items-center gap-2">
        <button
          type="button"
          onclick={resumeCrashSession}
          class="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 font-semibold transition-opacity hover:opacity-90"
          style="background-color: var(--accent); color: var(--accent-fg);"
        >
          <Play size={12} fill="currentColor" />
          <span>{t.crashRecoveryResume}</span>
        </button>
        <button
          type="button"
          onclick={dismissCrashSession}
          class="rounded-lg p-1.5 opacity-75 transition-opacity hover:opacity-100"
          style="color: var(--text-muted);"
          title={t.dismissUpdate}
          aria-label={t.dismissUpdate}
        >
          <X size={15} />
        </button>
      </div>
    </div>
  {/if}

  <!-- Hero Paste Box -->
  <section class="flex flex-col items-center gap-4 pt-2 text-center">
    <h1 class="text-2xl font-bold tracking-tight">{t.homeTitle}</h1>
    <div class="mt-1 flex w-full justify-center">
      <LinkInput />
    </div>
  </section>

  <!-- Unified Sort Bar -->
  {#if !loading && history.length > 0}
    <div class="flex flex-wrap items-center justify-end gap-2">
      <span class="text-xs" style="color: var(--text-muted);">{t.sortByLabel}</span>
      <select
        bind:value={sortField}
        class="rounded-lg border px-2.5 py-1.5 text-xs font-medium outline-none"
        style="background-color: var(--surface); border-color: var(--border); color: var(--text);"
      >
        <option value="added">{t.sortDateAdded}</option>
        <option value="title">{t.sortTitleName}</option>
      </select>
      <button
        type="button"
        onclick={() => {
          sortDir = sortDir === "desc" ? "asc" : "desc";
        }}
        class="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1.5 text-xs font-medium"
        style="background-color: var(--surface); border-color: var(--border); color: var(--text);"
        title={sortDir === "desc" ? t.sortDesc : t.sortAsc}
      >
        {#if sortDir === "desc"}
          <ArrowDown size={13} />
        {:else}
          <ArrowUp size={13} />
        {/if}
        <span>{sortDir === "desc" ? t.sortDesc : t.sortAsc}</span>
      </button>
    </div>
  {/if}

  <!-- New chapters section -->
  {#if updatedItems.length > 0}
    <section class="flex flex-col gap-4">
      <div class="flex items-center gap-2">
        <Sparkles size={17} style="color: var(--accent);" />
        <h2 class="text-base font-semibold">{t.newChapters}</h2>
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

  <!-- Continue reading (Active Library) -->
  {#if loading || history.length === 0 || activeItems.length > 0}
    <section class="flex flex-col gap-4">
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <h2 class="text-base font-semibold">{t.continueReading}</h2>
          {#if activeItems.length > 0}
            <span
              class="rounded-full px-2 py-0.5 text-xs font-medium"
              style="background-color: var(--surface-hover); color: var(--text-muted);"
            >
              {activeItems.length}
            </span>
          {/if}
        </div>
        {#if history.length > 0}
          <button
            type="button"
            onclick={() => router.goHistory()}
            class="text-xs font-medium transition-opacity hover:opacity-80"
            style="color: var(--accent);"
          >
            {t.viewAllHistory}
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
      {:else if history.length === 0}
        <div
          class="flex flex-col items-center justify-center gap-3 rounded-2xl border border-dashed py-14 text-center"
          style="border-color: var(--border); color: var(--text-muted);"
        >
          <BookOpen size={30} strokeWidth={1.5} />
          <div class="text-sm font-medium" style="color: var(--text);">{t.noRecentTitle}</div>
          <p class="max-w-xs text-xs">{t.noRecentDesc}</p>
        </div>
      {:else}
        <div class="grid grid-cols-2 gap-4 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5">
          {#each activeItems as item (item.mangaId)}
            <MangaCard {item} />
          {/each}
        </div>
      {/if}
    </section>
  {/if}

  <!-- Completed Section at the bottom with long horizontal underscore/line separator -->
  {#if !loading && completedItems.length > 0}
    <section class="flex flex-col gap-4 pt-2">
      <div class="flex items-center gap-3">
        <div
          class="h-px flex-1 border-b-2 border-dashed opacity-70"
          style="border-color: var(--border);"
          aria-hidden="true"
        ></div>
        <div
          class="flex items-center gap-2 rounded-full border px-3 py-1 text-xs font-semibold"
          style="background-color: var(--surface); border-color: var(--border); color: var(--text-muted);"
        >
          <CheckCircle2 size={14} style="color: #10b981;" />
          <span>{t.completedSection}</span>
          <span
            class="rounded-full px-1.5 py-0.5 text-[11px]"
            style="background-color: var(--surface-hover); color: var(--text);"
          >
            {completedItems.length}
          </span>
        </div>
        <div
          class="h-px flex-1 border-b-2 border-dashed opacity-70"
          style="border-color: var(--border);"
          aria-hidden="true"
        ></div>
      </div>

      <p class="text-center text-xs" style="color: var(--text-muted);">
        {t.completedSectionDesc}
      </p>

      <div class="grid grid-cols-2 gap-4 opacity-90 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5">
        {#each completedItems as item (item.mangaId)}
          <MangaCard {item} />
        {/each}
      </div>
    </section>
  {/if}
</div>
