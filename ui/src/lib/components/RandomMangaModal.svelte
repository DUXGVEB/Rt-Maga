<script lang="ts">
  import {
    Bookmark,
    BookOpen,
    CheckCircle2,
    Dices,
    Play,
    RefreshCw,
    Shuffle,
    Sparkles,
    Tag,
    X,
  } from "lucide-svelte";
  import { onDestroy } from "svelte";
  import { api } from "../api";
  import {
    buildUnifiedLibraryItems,
    extractCachedMangaMeta,
    extractGenreStats,
    filterItemsByGenre,
    RANDOM_POOL_DEFAULT_BOUND,
    sampleBoundedPool,
    type RandomMangaItem,
  } from "../depthCarouselMath";
  import {
    adaptiveOverlay,
    backdropFade,
    badgePop,
    collapseReveal,
    createOverlayGuard,
    pressable,
    viewReveal,
  } from "../motion";
  import { deviceEnv } from "../stores/deviceEnv.svelte";
  import { libraryCache } from "../stores/libraryCache.svelte";
  import { randomMangaStore, type RandomMangaScope } from "../stores/randomManga.svelte";
  import { router } from "../stores/router.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import DepthCarousel from "./DepthCarousel.svelte";

  const MAX_HYDRATE_ITEMS = 180;
  const HYDRATE_BATCH_SIZE = 6;
  const MAX_VISIBLE_GENRE_CHIPS = 8;

  const t = $derived(settingsStore.t);
  const isCompactMobile = $derived(deviceEnv.layoutTier === "compact");
  const overlayGuard = createOverlayGuard(170);

  let carouselRef = $state<{
    spinToRandom: (excludeId?: number | null) => RandomMangaItem | null;
    focusIndex: (index: number) => void;
    step: (delta: number) => void;
  } | null>(null);

  let scope = $state<RandomMangaScope>("all");
  let selectedGenre = $state<string>("__all__");
  let poolIds = $state<number[]>([]);
  let activeItemId = $state<number | null>(null);
  let isSpinning = $state(false);
  let loadingLibrary = $state(false);

  let hydrateVersion = 0;

  const bookmarksList = $derived(libraryCache.bookmarksBySort["newest"] ?? []);
  const hasBookmarks = $derived(bookmarksList.length > 0);

  const allLibraryItems = $derived(
    buildUnifiedLibraryItems(
      libraryCache.history,
      bookmarksList,
      libraryCache.mangaMetaById,
      scope
    )
  );

  const itemById = $derived(new Map(allLibraryItems.map((it) => [it.id, it])));
  const genreStats = $derived(extractGenreStats(allLibraryItems));
  const filteredItems = $derived(filterItemsByGenre(allLibraryItems, selectedGenre));

  const poolItems = $derived(
    poolIds
      .map((id) => itemById.get(id))
      .filter((it): it is RandomMangaItem => it !== undefined)
  );

  const activeItem = $derived<RandomMangaItem | null>(
    (activeItemId !== null ? itemById.get(activeItemId) : undefined) ?? poolItems[0] ?? null
  );

  function rebuildPool(avoidId: number | null = randomMangaStore.lastPickedId) {
    const sampled = sampleBoundedPool(filteredItems, RANDOM_POOL_DEFAULT_BOUND, avoidId);
    poolIds = sampled.map((it) => it.id);
    activeItemId = sampled[0]?.id ?? null;
  }

  async function ensureLibraryDataLoaded(version: number) {
    const needsHistory = !libraryCache.historyLoaded;
    const needsBookmarks = !libraryCache.bookmarksLoaded["newest"];
    if (!needsHistory && !needsBookmarks) return;

    loadingLibrary = allLibraryItems.length === 0;
    try {
      const [hist, bms] = await Promise.all([
        needsHistory ? api.listHistory().catch(() => null) : Promise.resolve(null),
        needsBookmarks ? api.listBookmarks("newest").catch(() => null) : Promise.resolve(null),
      ]);
      if (version !== hydrateVersion) return;
      if (hist) libraryCache.setHistory(hist);
      if (bms) libraryCache.setBookmarks("newest", bms);
    } finally {
      if (version === hydrateVersion) {
        loadingLibrary = false;
      }
    }
  }

  /**
   * Progressively hydrates lightweight local SQLite metadata (genres, status, summary)
   * in tiny batches without blocking the UI thread or mutating the active carousel deck.
   */
  async function hydrateLibraryMetadata(version: number) {
    const priorityIds: number[] = [];
    if (activeItemId !== null) priorityIds.push(activeItemId);
    for (const id of poolIds) {
      if (!priorityIds.includes(id)) priorityIds.push(id);
    }
    for (const item of allLibraryItems) {
      if (priorityIds.length >= MAX_HYDRATE_ITEMS) break;
      if (!priorityIds.includes(item.id)) priorityIds.push(item.id);
    }

    const missing = priorityIds.filter((id) => !libraryCache.mangaMetaById[id]);
    if (missing.length === 0) return;

    for (let i = 0; i < missing.length; i += HYDRATE_BATCH_SIZE) {
      if (version !== hydrateVersion || !randomMangaStore.isOpen) return;
      const batch = missing.slice(i, i + HYDRATE_BATCH_SIZE);
      const settled = await Promise.allSettled(batch.map((id) => api.getManga(id)));
      if (version !== hydrateVersion || !randomMangaStore.isOpen) return;

      const parsed = [];
      for (const res of settled) {
        if (res.status === "fulfilled" && res.value) {
          parsed.push(extractCachedMangaMeta(res.value));
        }
      }
      if (parsed.length > 0) {
        libraryCache.setMangaMetaBatch(parsed);
      }

      // Yield between batches so 60/120fps animations stay uninterrupted.
      if (i + HYDRATE_BATCH_SIZE < missing.length) {
        await new Promise<void>((resolve) => {
          window.setTimeout(resolve, 16);
        });
      }
    }
  }

  async function hydrateSingle(id: number) {
    if (libraryCache.mangaMetaById[id]) return;
    try {
      const view = await api.getManga(id);
      libraryCache.setMangaMetaBatch([extractCachedMangaMeta(view)]);
    } catch {
      // Ignore missing or unavailable local records.
    }
  }

  $effect(() => {
    if (!randomMangaStore.isOpen) {
      hydrateVersion += 1;
      isSpinning = false;
      return;
    }

    const version = ++hydrateVersion;
    scope = randomMangaStore.scope;
    selectedGenre = "__all__";
    isSpinning = false;
    overlayGuard.lock(140);

    const unregisterBack = router.registerBackInterceptor(() => {
      if (!randomMangaStore.isOpen) return false;
      if (!isSpinning) {
        randomMangaStore.close();
      }
      return true;
    });

    void (async () => {
      await ensureLibraryDataLoaded(version);
      if (version !== hydrateVersion || !randomMangaStore.isOpen) return;
      rebuildPool(randomMangaStore.lastPickedId);
      await hydrateLibraryMetadata(version);
    })();

    return () => {
      unregisterBack();
    };
  });

  function closeModal() {
    if (isSpinning || !overlayGuard.canAct()) return;
    overlayGuard.lock();
    randomMangaStore.close();
  }

  function handleScopeChange(nextScope: RandomMangaScope) {
    if (isSpinning || nextScope === scope) return;
    scope = nextScope;
    selectedGenre = "__all__";
    rebuildPool(activeItemId ?? randomMangaStore.lastPickedId);
  }

  function handleGenreChange(genre: string) {
    if (isSpinning || genre === selectedGenre) return;
    selectedGenre = genre;
    rebuildPool(activeItemId ?? randomMangaStore.lastPickedId);
  }

  function handleReshuffleDeck() {
    if (isSpinning) return;
    rebuildPool(activeItemId ?? randomMangaStore.lastPickedId);
  }

  function handleSpinClick() {
    if (isSpinning || poolItems.length === 0) return;
    carouselRef?.spinToRandom(randomMangaStore.lastPickedId);
  }

  function handleSelectStory(manga: RandomMangaItem) {
    if (isSpinning) return;
    randomMangaStore.rememberPick(manga.id);
    randomMangaStore.close();
    router.goManga(manga.id);
  }

  function handleReadNow(manga: RandomMangaItem) {
    if (isSpinning) return;
    randomMangaStore.rememberPick(manga.id);
    randomMangaStore.close();
    if (manga.resumeChapterUrl && !(manga.isCompleted && !manga.hasNew)) {
      router.goReader(
        manga.id,
        manga.resumeChapterUrl,
        manga.resumeScrollRatio,
        manga.resumePageIndex
      );
    } else {
      router.goManga(manga.id);
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!randomMangaStore.isOpen) return;
    if (e.key === "Escape") {
      e.preventDefault();
      closeModal();
      return;
    }
    const target = e.target as HTMLElement | null;
    const isTyping =
      target &&
      (target.tagName === "INPUT" ||
        target.tagName === "SELECT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable);
    if (!isTyping && !isSpinning && (e.key === "r" || e.key === "R")) {
      e.preventDefault();
      handleSpinClick();
    }
  }

  onDestroy(() => {
    hydrateVersion += 1;
  });
</script>

<svelte:window onkeydown={handleKeydown} />

{#if randomMangaStore.isOpen}
  <div
    class="fixed inset-0 z-50 flex items-end justify-center p-0 sm:items-center sm:p-4 lg:p-6"
    style="padding-top: max(0.5rem, var(--safe-top)); padding-left: var(--safe-left); padding-right: var(--safe-right);"
    role="dialog"
    aria-modal="true"
    aria-labelledby="random-manga-modal-title"
  >
    <!-- Backdrop (static blur desktop-only, clean opaque scrim on Android WebView) -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      transition:backdropFade|local
      onclick={closeModal}
      class="modal-backdrop fixed inset-0"
    ></div>

    <!-- Modal / Bottom Sheet Container -->
    <div
      transition:adaptiveOverlay|local={{ isMobile: isCompactMobile }}
      class="modal-cq relative z-10 flex max-h-[calc(100dvh-0.75rem-var(--safe-top))] w-full max-w-5xl flex-col overflow-hidden rounded-t-3xl border-t sm:max-h-[calc(100dvh-2rem-var(--safe-top)-var(--safe-bottom))] sm:rounded-2xl sm:border"
      style="background-color: var(--surface); border-color: var(--border); color: var(--text); box-shadow: var(--shadow-lg); padding-bottom: var(--safe-bottom);"
    >
      <div class="sheet-handle sm:hidden" aria-hidden="true"></div>

      <!-- Header Bar -->
      <div
        class="flex shrink-0 flex-wrap items-center justify-between gap-2.5 border-b px-4 py-3 sm:px-5 sm:py-3.5"
        style="border-color: var(--border);"
      >
        <div class="flex min-w-0 items-center gap-2.5">
          <div
            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl"
            style="background-color: var(--accent-soft); color: var(--accent);"
          >
            <Dices size={18} />
          </div>
          <div class="min-w-0">
            <h2 id="random-manga-modal-title" class="truncate text-base font-bold leading-snug">
              {t.randomMangaTitle}
            </h2>
            <p class="hidden truncate text-xs sm:block" style="color: var(--text-muted);">
              {t.randomMangaSubtitle}
            </p>
          </div>
        </div>

        <div class="flex items-center gap-1.5 sm:gap-2">
          <!-- Scope switcher (All Library vs Bookmarks) -->
          {#if hasBookmarks}
            <div
              class="inline-flex rounded-xl border p-0.5 text-[11px] font-medium"
              style="background-color: var(--surface-subtle); border-color: var(--border);"
            >
              <button
                type="button"
                disabled={isSpinning}
                onclick={() => handleScopeChange("all")}
                class="gpu-press rounded-lg px-2.5 py-1 disabled:opacity-50"
                style="background-color: {scope === 'all'
                  ? 'var(--accent)'
                  : 'transparent'}; color: {scope === 'all'
                  ? 'var(--accent-fg)'
                  : 'var(--text-muted)'};"
              >
                {t.randomMangaScopeAll}
              </button>
              <button
                type="button"
                disabled={isSpinning}
                onclick={() => handleScopeChange("bookmarks")}
                class="gpu-press inline-flex items-center gap-1 rounded-lg px-2.5 py-1 disabled:opacity-50"
                style="background-color: {scope === 'bookmarks'
                  ? 'var(--accent)'
                  : 'transparent'}; color: {scope === 'bookmarks'
                  ? 'var(--accent-fg)'
                  : 'var(--text-muted)'};"
              >
                <Bookmark size={11} />
                <span>{t.randomMangaScopeBookmarks}</span>
              </button>
            </div>
          {/if}

          {#if filteredItems.length > poolItems.length}
            <span transition:badgePop|local class="inline-flex">
              <button
                type="button"
                use:pressable
                disabled={isSpinning}
                onclick={handleReshuffleDeck}
                class="gpu-press inline-flex min-h-[34px] items-center gap-1.5 rounded-xl border px-2.5 py-1 text-xs font-medium disabled:opacity-45"
                style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
                title={t.randomMangaReshufflePool}
              >
                <Shuffle size={13} />
                <span class="hidden sm:inline">{t.randomMangaReshufflePool}</span>
              </button>
            </span>
          {/if}

          <button
            type="button"
            use:pressable
            disabled={isSpinning}
            onclick={closeModal}
            class="gpu-press flex h-9 w-9 items-center justify-center rounded-xl opacity-75 hover:opacity-100 disabled:opacity-35"
            aria-label={t.cancel}
          >
            <X size={18} />
          </button>
        </div>
      </div>

      <!-- Quick Genre Filter Bar (rendered whenever local genre metadata exists) -->
      {#if genreStats.length > 0}
        <div
          transition:collapseReveal|local
          class="flex shrink-0 items-center gap-2 border-b px-4 py-2.5 sm:px-5"
          style="background-color: var(--surface-subtle); border-color: var(--border);"
        >
          <div class="flex shrink-0 items-center gap-1 text-[11px] font-semibold" style="color: var(--text-muted);">
            <Tag size={12} />
            <span class="hidden sm:inline">{t.randomMangaGenreFilterLabel}:</span>
          </div>

          <div class="reader-no-scrollbar flex min-w-0 flex-1 items-center gap-1.5 overflow-x-auto py-0.5">
            <button
              type="button"
              disabled={isSpinning}
              onclick={() => handleGenreChange("__all__")}
              class="gpu-press shrink-0 rounded-lg border px-2.5 py-1 text-[11px] font-medium disabled:opacity-45"
              style="border-color: {selectedGenre === '__all__'
                ? 'var(--accent)'
                : 'var(--border)'}; background-color: {selectedGenre === '__all__'
                ? 'var(--accent-soft)'
                : 'var(--surface)'}; color: {selectedGenre === '__all__'
                ? 'var(--accent)'
                : 'var(--text)'};"
            >
              {t.randomMangaGenreAll} ({allLibraryItems.length})
            </button>

            {#each genreStats.slice(0, MAX_VISIBLE_GENRE_CHIPS) as genre (genre.name)}
              {@const active = selectedGenre.toLowerCase() === genre.name.toLowerCase()}
              <button
                type="button"
                disabled={isSpinning}
                onclick={() => handleGenreChange(genre.name)}
                class="gpu-press shrink-0 rounded-lg border px-2.5 py-1 text-[11px] font-medium disabled:opacity-45"
                style="border-color: {active
                  ? 'var(--accent)'
                  : 'var(--border)'}; background-color: {active
                  ? 'var(--accent-soft)'
                  : 'var(--surface)'}; color: {active ? 'var(--accent)' : 'var(--text)'};"
              >
                {genre.name} ({genre.count})
              </button>
            {/each}
          </div>

          {#if genreStats.length > MAX_VISIBLE_GENRE_CHIPS}
            <select
              aria-label={t.randomMangaGenreFilterLabel}
              disabled={isSpinning}
              value={selectedGenre}
              onchange={(e) => handleGenreChange(e.currentTarget.value)}
              class="shrink-0 rounded-lg border px-2 py-1 text-[11px] font-medium outline-none disabled:opacity-45"
              style="background-color: var(--surface); border-color: var(--border); color: var(--text);"
            >
              <option value="__all__">{t.randomMangaGenreAll}</option>
              {#each genreStats as genre (genre.name)}
                <option value={genre.name}>{genre.name} ({genre.count})</option>
              {/each}
            </select>
          {/if}
        </div>
      {/if}

      <!-- Responsive Body: Side-by-side on Desktop (lg+), Stacked on Mobile/Android -->
      <div class="min-h-0 flex-1 overflow-y-auto overscroll-contain p-4 sm:p-5">
        {#if loadingLibrary && poolItems.length === 0}
          <div class="grid grid-cols-1 gap-5 lg:grid-cols-[minmax(0,1.25fr)_minmax(300px,0.85fr)]">
            <div
              class="h-80 animate-pulse rounded-2xl lg:h-[430px]"
              style="background-color: var(--surface-hover);"
            ></div>
            <div class="flex flex-col gap-3">
              <div class="h-7 w-3/4 animate-pulse rounded-lg" style="background-color: var(--surface-hover);"></div>
              <div class="h-4 w-1/2 animate-pulse rounded-lg" style="background-color: var(--surface-hover);"></div>
              <div class="h-20 w-full animate-pulse rounded-xl" style="background-color: var(--surface-hover);"></div>
            </div>
          </div>
        {:else if poolItems.length === 0}
          <div
            in:viewReveal|local={{ duration: 160, y: 6 }}
            class="flex flex-col items-center justify-center gap-3 rounded-2xl border border-dashed py-16 text-center"
            style="border-color: var(--border-strong); color: var(--text-muted);"
          >
            <BookOpen size={32} strokeWidth={1.5} />
            <div class="text-sm font-semibold" style="color: var(--text);">
              {t.randomMangaEmptyTitle}
            </div>
            <p class="max-w-sm text-xs">{t.randomMangaEmptyDesc}</p>
            {#if selectedGenre !== "__all__"}
              <button
                type="button"
                use:pressable
                onclick={() => handleGenreChange("__all__")}
                class="gpu-press mt-1 rounded-xl px-3.5 py-2 text-xs font-semibold"
                style="background-color: var(--accent); color: var(--accent-fg);"
              >
                {t.randomMangaGenreAll}
              </button>
            {/if}
          </div>
        {:else}
          <div class="grid grid-cols-1 items-center gap-5 lg:grid-cols-[minmax(0,1.25fr)_minmax(310px,0.85fr)] lg:gap-6">
            <!-- Left / Top: Pure Svelte 3D Depth Carousel -->
            <DepthCarousel
              bind:this={carouselRef}
              items={poolItems}
              depth={220}
              spread={90}
              tilt={22}
              tiltDirection="right"
              perspective={1400}
              visibleCards={4}
              falloff={0.2}
              loop={true}
              autoplay={false}
              keyboardEnabled={randomMangaStore.isOpen}
              onActiveChange={(item) => {
                if (!item) return;
                activeItemId = item.id;
                if (!item.metadataLoaded) {
                  void hydrateSingle(item.id);
                }
              }}
              onSelectStory={handleSelectStory}
              onSpinStart={() => {
                isSpinning = true;
              }}
              onSpinEnd={(winner) => {
                isSpinning = false;
                activeItemId = winner.id;
                randomMangaStore.rememberPick(winner.id);
                if (!winner.metadataLoaded) {
                  void hydrateSingle(winner.id);
                }
              }}
            />

            <!-- Right / Bottom: Synced Info Panel -->
            <section
              class="card-contain flex flex-col justify-between gap-4 rounded-2xl border p-4 sm:p-5 lg:min-h-[430px]"
              style="background-color: var(--surface-subtle); border-color: var(--border);"
            >
              {#if isSpinning}
                <!-- Compact Spinning Skeleton State -->
                <div
                  in:viewReveal|local={{ duration: 140, y: 4 }}
                  class="flex flex-1 flex-col justify-center gap-3.5 py-4"
                >
                  <div class="flex items-center gap-2.5">
                    <span
                      class="flex h-8 w-8 items-center justify-center rounded-xl"
                      style="background-color: var(--accent-soft); color: var(--accent);"
                    >
                      <RefreshCw size={16} class="animate-spin" />
                    </span>
                    <div>
                      <div class="text-sm font-bold">{t.randomMangaPickingLabel}</div>
                      <div class="text-xs" style="color: var(--text-muted);">
                        {t.randomMangaPickingSub}
                      </div>
                    </div>
                  </div>

                  <div class="mt-2 flex flex-col gap-2">
                    <div
                      class="h-6 w-4/5 animate-pulse rounded-lg"
                      style="background-color: var(--surface-hover);"
                    ></div>
                    <div class="flex gap-1.5">
                      <div
                        class="h-5 w-16 animate-pulse rounded-md"
                        style="background-color: var(--surface-hover);"
                      ></div>
                      <div
                        class="h-5 w-20 animate-pulse rounded-md"
                        style="background-color: var(--surface-hover);"
                      ></div>
                      <div
                        class="h-5 w-14 animate-pulse rounded-md"
                        style="background-color: var(--surface-hover);"
                      ></div>
                    </div>
                    <div
                      class="mt-1 h-14 w-full animate-pulse rounded-xl"
                      style="background-color: var(--surface-hover);"
                    ></div>
                  </div>
                </div>
              {:else if activeItem}
                <!-- Settled Active Manga Metadata (lightweight fade/slide sync) -->
                {#key activeItem.id}
                  <div
                    in:viewReveal|local={{ duration: 155, y: 5 }}
                    class="flex flex-1 flex-col gap-3"
                  >
                    <!-- Badges Row -->
                    <div class="flex flex-wrap items-center gap-1.5">
                      <span
                        class="rounded-md px-2 py-0.5 text-[11px] font-semibold"
                        style="background-color: var(--surface); color: var(--text-muted); border: 1px solid var(--border);"
                      >
                        {t.randomMangaPoolBadge(poolItems.length, filteredItems.length)}
                      </span>

                      {#if activeItem.hasNew}
                        <span
                          class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold"
                          style="background-color: var(--accent); color: var(--accent-fg);"
                        >
                          <Sparkles size={10} />
                          {t.badgeNew}
                        </span>
                      {:else if activeItem.isCompleted}
                        <span
                          class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold"
                          style="background-color: rgba(16, 185, 129, 0.14); color: #10b981;"
                        >
                          <CheckCircle2 size={10} />
                          {t.badgeCompleted}
                        </span>
                      {:else if activeItem.status}
                        <span
                          class="rounded-full px-2 py-0.5 text-[10px] font-semibold capitalize"
                          style="background-color: var(--accent-soft); color: var(--accent);"
                        >
                          {activeItem.status}
                        </span>
                      {/if}

                      {#if activeItem.bookmarked}
                        <span
                          class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold"
                          style="background-color: var(--surface-hover); color: var(--text-muted);"
                        >
                          <Bookmark size={10} fill="currentColor" />
                        </span>
                      {/if}
                    </div>

                    <!-- Title -->
                    <div>
                      <h3 class="line-clamp-2 text-lg font-bold leading-snug tracking-tight">
                        {activeItem.title}
                      </h3>
                      <div
                        class="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs"
                        style="color: var(--text-muted);"
                      >
                        <span class="font-semibold" style="color: var(--text);">
                          {t.chapterCount(activeItem.chapters)}
                        </span>
                        {#if activeItem.unreadCount > 0}
                          <span>·</span>
                          <span style="color: var(--accent);">
                            {t.unreadBadge(activeItem.unreadCount)}
                          </span>
                        {/if}
                        {#if activeItem.latestChapterName}
                          <span>·</span>
                          <span class="truncate">{activeItem.latestChapterName}</span>
                        {/if}
                      </div>
                    </div>

                    <!-- Genre Tags -->
                    {#if activeItem.genres.length > 0}
                      <div class="flex flex-wrap items-center gap-1.5">
                        {#each activeItem.genres.slice(0, 6) as genre (genre)}
                          <span
                            class="rounded-md px-2 py-0.5 text-[11px] font-medium"
                            style="background-color: var(--accent-soft); color: var(--accent);"
                          >
                            {genre}
                          </span>
                        {/each}
                        {#if activeItem.genres.length > 6}
                          <span
                            class="rounded-md px-2 py-0.5 text-[11px] font-medium"
                            style="background-color: var(--surface-hover); color: var(--text-muted);"
                          >
                            +{activeItem.genres.length - 6}
                          </span>
                        {/if}
                      </div>
                    {/if}

                    <!-- 3-Line Clamped Summary -->
                    <p
                      class="line-clamp-3 text-xs leading-relaxed"
                      style="color: var(--text-muted);"
                    >
                      {activeItem.summary || t.randomMangaNoSummary}
                    </p>

                    <div class="mt-auto text-[11px]" style="color: var(--text-muted);">
                      {t.randomMangaClickCenterHint}
                    </div>
                  </div>
                {/key}
              {/if}

              <!-- Action Buttons -->
              <div class="flex flex-col gap-2 pt-2">
                <button
                  type="button"
                  use:pressable
                  disabled={isSpinning || poolItems.length === 0}
                  onclick={handleSpinClick}
                  class="gpu-press inline-flex min-h-[44px] w-full items-center justify-center gap-2 rounded-xl px-4 py-2.5 text-sm font-semibold shadow-sm hover:opacity-95 disabled:opacity-55"
                  style="background-color: var(--accent); color: var(--accent-fg);"
                >
                  {#if isSpinning}
                    <RefreshCw size={16} class="animate-spin" />
                    <span>{t.randomMangaSpinningBtn}</span>
                  {:else}
                    <Dices size={17} />
                    <span>{t.randomMangaSpinBtn}</span>
                  {/if}
                </button>

                <div class="grid grid-cols-2 gap-2">
                  <button
                    type="button"
                    use:pressable
                    disabled={isSpinning || !activeItem}
                    onclick={() => activeItem && handleReadNow(activeItem)}
                    class="gpu-press inline-flex min-h-[40px] items-center justify-center gap-1.5 rounded-xl border px-3 py-2 text-xs font-semibold disabled:opacity-45"
                    style="background-color: var(--surface); border-color: var(--border); color: var(--text);"
                  >
                    <Play size={13} fill="currentColor" style="color: var(--accent);" />
                    <span class="truncate">{t.randomMangaReadNowBtn}</span>
                  </button>

                  <button
                    type="button"
                    use:pressable
                    disabled={isSpinning || !activeItem}
                    onclick={() => activeItem && handleSelectStory(activeItem)}
                    class="gpu-press inline-flex min-h-[40px] items-center justify-center gap-1.5 rounded-xl border px-3 py-2 text-xs font-semibold disabled:opacity-45"
                    style="background-color: var(--surface); border-color: var(--border); color: var(--text);"
                  >
                    <BookOpen size={13} style="color: var(--text-muted);" />
                    <span class="truncate">{t.randomMangaViewDetailsBtn}</span>
                  </button>
                </div>
              </div>
            </section>
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}
