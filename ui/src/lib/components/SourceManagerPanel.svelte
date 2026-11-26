<script lang="ts">
  import {
    AlertTriangle,
    ArrowDown,
    ArrowLeftRight,
    ArrowUp,
    BookOpen,
    Check,
    Compass,
    Edit3,
    ExternalLink,
    Filter,
    Globe,
    Plus,
    Search,
    Trash2,
    X,
  } from "lucide-svelte";
  import { api, formatAppError, imgProxyUrl } from "../api";
  import { router } from "../stores/router.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
  import {
    emptyFilterQuery,
    emptyFilterSchema,
    filterKeyFor,
    webFilterStore,
  } from "../stores/webFilter.svelte";
  import type {
    CatalogEntryView,
    CatalogFilterQuery,
    LibrarySortField,
    MultiSourceSearchResponse,
    SavedSourceView,
    SiteFilterSchema,
    SortDirection,
  } from "../types";
  import LatticeLoader from "./LatticeLoader.svelte";
  import SquishSwitch from "./SquishSwitch.svelte";
  import WebFilterBar from "./WebFilterBar.svelte";

  let {
    scope = "main",
    initialQuery = "",
  }: {
    scope?: "main" | "supplemental";
    initialQuery?: string;
  } = $props();

  let sources = $state<SavedSourceView[]>([]);
  let loadingSources = $state(true);
  let filterText = $state("");
  let sortField = $state<LibrarySortField>("added");
  let sortDir = $state<SortDirection>("desc");
  let failedIcons = $state<Record<number, boolean>>({});
  let failedEntryCovers = $state<Record<string, boolean>>({});

  let searchQuery = $state("");
  let searching = $state(false);
  let searchResult = $state<MultiSourceSearchResponse | null>(null);
  let openingEntryUrl = $state<string | null>(null);

  // Active per-website filter bar state (shown when a saved source is selected OR a direct link is typed)
  let selectedSourceForFilter = $state<SavedSourceView | null>(null);
  let activeFilterSchema = $state<SiteFilterSchema>(emptyFilterSchema());
  let loadingFilterSchema = $state(false);
  let applyingFilter = $state(false);
  let schemaReqSeq = 0;

  let addModalOpen = $state(false);
  let addUrl = $state("");
  let addName = $state("");
  let addingSource = $state(false);

  let editTarget = $state<SavedSourceView | null>(null);
  let editName = $state("");
  let editBaseUrl = $state("");
  let editEnabled = $state(true);
  let savingEdit = $state(false);

  let confirmDeleteId = $state<number | null>(null);

  let lastInitQuery = "";
  const t = $derived(settingsStore.t);
  const visibilityFilter = $derived(
    scope === "supplemental" ? ("supplemental_only" as const) : ("main_only" as const)
  );

  function isLikelyDirectLink(raw: string): boolean {
    const trimmed = raw.trim();
    if (!trimmed || /\s/.test(trimmed)) return false;
    const lower = trimmed.toLowerCase();
    if (lower.startsWith("http://") || lower.startsWith("https://") || lower.startsWith("rtmaga://")) {
      return true;
    }
    return /^[a-z0-9.-]+\.[a-z]{2,}(\/.*)?$/i.test(trimmed);
  }

  const typedLink = $derived(isLikelyDirectLink(searchQuery) ? searchQuery.trim() : null);

  const activeFilterKey = $derived(() => {
    if (typedLink) return filterKeyFor(null, typedLink);
    if (selectedSourceForFilter) return filterKeyFor(selectedSourceForFilter.id, selectedSourceForFilter.baseUrl);
    return null;
  });

  const activeFilterValue = $derived(() => {
    const key = activeFilterKey();
    return key ? webFilterStore.getQuery(key) : emptyFilterQuery();
  });

  const activeFilterLabel = $derived(() => {
    if (typedLink) return hostFromUrl(typedLink);
    if (selectedSourceForFilter) return selectedSourceForFilter.displayName;
    return "";
  });

  async function loadSources() {
    loadingSources = true;
    try {
      sources = await api.listSavedSources(visibilityFilter);
    } catch (err) {
      toasts.push(formatAppError(err, settingsStore.value.language).detail, "error");
    } finally {
      loadingSources = false;
    }
  }

  $effect(() => {
    void scope;
    void loadSources();
  });

  $effect(() => {
    const q = (initialQuery ?? "").trim();
    if (q && q !== lastInitQuery) {
      lastInitQuery = q;
      searchQuery = q;
      void runSearch(q);
    }
  });

  // Automatically fetch/load per-website filter schema when a link is entered in the Sources bar.
  $effect(() => {
    const link = typedLink;
    if (!link) return;
    const key = filterKeyFor(null, link);
    const cached = webFilterStore.getSchema(key);
    if (cached) {
      activeFilterSchema = cached;
    }
    const seq = ++schemaReqSeq;
    const timer = setTimeout(() => {
      loadingFilterSchema = true;
      void api
        .getWebFilterSchema({ url: link })
        .then((schema) => {
          if (seq !== schemaReqSeq) return;
          activeFilterSchema = schema;
          webFilterStore.setSchema(key, schema);
        })
        .catch(() => {
          // Non-fatal: baseline chapter & recency filters still remain ready immediately.
        })
        .finally(() => {
          if (seq === schemaReqSeq) loadingFilterSchema = false;
        });
    }, 350);
    return () => clearTimeout(timer);
  });

  async function openSourceFilters(src: SavedSourceView) {
    if (selectedSourceForFilter?.id === src.id && !typedLink) {
      selectedSourceForFilter = null;
      return;
    }
    selectedSourceForFilter = src;
    const key = filterKeyFor(src.id, src.baseUrl);
    activeFilterSchema = src.filterSchema ?? webFilterStore.getSchema(key) ?? emptyFilterSchema();
    webFilterStore.setSchema(key, activeFilterSchema);

    if (activeFilterSchema.tags.length === 0) {
      const seq = ++schemaReqSeq;
      loadingFilterSchema = true;
      try {
        const fetched = await api.getWebFilterSchema({ sourceId: src.id });
        if (seq !== schemaReqSeq) return;
        activeFilterSchema = fetched;
        webFilterStore.setSchema(key, fetched);
      } catch {
        // Keep baseline filters usable even if schema probe times out.
      } finally {
        if (seq === schemaReqSeq) loadingFilterSchema = false;
      }
    }
  }

  async function applySourceFilters(nextQuery: CatalogFilterQuery) {
    const key = activeFilterKey();
    if (!key || applyingFilter) return;
    webFilterStore.setQuery(key, nextQuery);
    applyingFilter = true;
    try {
      if (typedLink) {
        const cat = await api.browseCatalogUrl(typedLink, 1, nextQuery, false);
        router.goBrowse(cat.url, cat, false, cat.sourceId ?? undefined);
      } else if (selectedSourceForFilter) {
        const sid = selectedSourceForFilter.id;
        const cat = await api.browseSavedSource(sid, nextQuery.query ?? null, 1, nextQuery, false);
        router.goBrowse(cat.url, cat, false, sid);
      }
    } catch (err) {
      toasts.push(formatAppError(err, settingsStore.value.language).detail, "error", 4000);
    } finally {
      applyingFilter = false;
    }
  }

  function resetSourceFilters() {
    const key = activeFilterKey();
    if (key) webFilterStore.resetQuery(key);
  }

  const sortedFilteredSources = $derived(() => {
    const q = filterText.trim().toLowerCase();
    const list = q
      ? sources.filter(
          (s) =>
            s.displayName.toLowerCase().includes(q) ||
            s.baseUrl.toLowerCase().includes(q) ||
            s.sourceKind.toLowerCase().includes(q)
        )
      : [...sources];

    const mult = sortDir === "desc" ? -1 : 1;
    list.sort((a, b) => {
      if (sortField === "title") {
        const cmp = a.displayName.localeCompare(b.displayName, undefined, {
          sensitivity: "base",
        });
        if (cmp !== 0) return cmp * mult;
        return (a.createdAt - b.createdAt) * mult;
      }
      if (a.createdAt !== b.createdAt) {
        return (a.createdAt - b.createdAt) * mult;
      }
      return (a.id - b.id) * mult;
    });
    return list;
  });

  async function runSearch(overrideQuery?: string) {
    const q = (overrideQuery ?? searchQuery).trim();
    if (!q || searching) return;

    if (isLikelyDirectLink(q)) {
      searching = true;
      try {
        const key = filterKeyFor(null, q);
        const existingFilters = webFilterStore.getQuery(key);
        const hasFilters =
          existingFilters.includeTags.length > 0 ||
          existingFilters.excludeTags.length > 0 ||
          (existingFilters.minChapters ?? 0) > 0 ||
          (existingFilters.updatedWithin && existingFilters.updatedWithin !== "any") ||
          Object.keys(existingFilters.extra).length > 0;

        if (hasFilters) {
          const cat = await api.browseCatalogUrl(q, 1, existingFilters, false);
          await loadSources();
          router.goBrowse(cat.url, cat, false, cat.sourceId ?? undefined);
          return;
        }

        const res = await api.resolveUrl(q);
        await loadSources();
        if (res.kind === "catalog" && res.catalog) {
          router.goBrowse(res.catalog.url, res.catalog, false, res.catalog.sourceId ?? undefined);
        } else if (res.manga) {
          if (res.startAt) {
            router.goReader(res.manga.id, res.startAt.url);
          } else {
            router.goManga(res.manga.id);
          }
        }
      } catch (err) {
        toasts.push(formatAppError(err, settingsStore.value.language).detail, "error");
      } finally {
        searching = false;
      }
      return;
    }

    searching = true;
    failedEntryCovers = {};
    try {
      searchResult = await api.searchSavedSources(q, visibilityFilter);
    } catch (err) {
      toasts.push(formatAppError(err, settingsStore.value.language).detail, "error");
    } finally {
      searching = false;
    }
  }

  async function openCatalogEntry(entry: CatalogEntryView) {
    if (openingEntryUrl) return;
    openingEntryUrl = entry.url;
    try {
      const res = await api.resolveUrl(entry.url);
      if (res.kind === "catalog" && res.catalog) {
        router.goBrowse(res.catalog.url, res.catalog, false, res.catalog.sourceId ?? undefined);
      } else if (res.manga) {
        if (res.startAt) {
          router.goReader(res.manga.id, res.startAt.url);
        } else {
          router.goManga(res.manga.id);
        }
      }
    } catch (err) {
      toasts.push(formatAppError(err, settingsStore.value.language).detail, "error", 3600);
    } finally {
      openingEntryUrl = null;
    }
  }

  async function submitAddSource() {
    const raw = addUrl.trim();
    if (!raw || addingSource) return;
    addingSource = true;
    try {
      const placement = scope === "supplemental" ? "force_supplemental" : "auto";
      const saved = await api.addSavedSource(raw, addName.trim() || null, placement);
      addModalOpen = false;
      addUrl = "";
      addName = "";
      await loadSources();
      toasts.push(t.sourcesAddedToast(saved.displayName, saved.isSupplemental), "success", 3200);
    } catch (err) {
      toasts.push(formatAppError(err, settingsStore.value.language).detail, "error", 4000);
    } finally {
      addingSource = false;
    }
  }

  function startEditSource(src: SavedSourceView) {
    editTarget = src;
    editName = src.displayName;
    editBaseUrl = src.baseUrl;
    editEnabled = src.isEnabled;
  }

  async function submitEditSource() {
    if (!editTarget || savingEdit) return;
    savingEdit = true;
    try {
      await api.updateSavedSource(editTarget.id, {
        displayName: editName.trim() || editTarget.displayName,
        baseUrl: editBaseUrl.trim() || editTarget.baseUrl,
        isEnabled: editEnabled,
      });
      editTarget = null;
      await loadSources();
      toasts.push(t.sourcesUpdatedToast, "success", 2200);
    } catch (err) {
      toasts.push(formatAppError(err, settingsStore.value.language).detail, "error", 3600);
    } finally {
      savingEdit = false;
    }
  }

  async function moveVisibility(src: SavedSourceView) {
    try {
      const patch = scope === "main" ? "move_to_supplemental" : "move_to_main";
      await api.updateSavedSource(src.id, { visibility: patch });
      if (selectedSourceForFilter?.id === src.id) selectedSourceForFilter = null;
      await loadSources();
      toasts.push(t.sourcesMovedToast, "info", 2400);
    } catch (err) {
      toasts.push(formatAppError(err, settingsStore.value.language).detail, "error");
    }
  }

  async function removeSource(id: number) {
    try {
      await api.deleteSavedSource(id);
      if (selectedSourceForFilter?.id === id) selectedSourceForFilter = null;
      confirmDeleteId = null;
      await loadSources();
      toasts.push(t.sourcesDeletedToast, "info", 2200);
    } catch (err) {
      toasts.push(formatAppError(err, settingsStore.value.language).detail, "error");
    }
  }

  function hostFromUrl(raw: string): string {
    try {
      const normalized = raw.includes("://") ? raw : `https://${raw}`;
      return new URL(normalized).hostname.replace(/^www\./i, "");
    } catch {
      return raw;
    }
  }

  function friendlyKind(kind: string): string {
    switch (kind) {
      case "builtin_adapter":
        return "Built-in";
      case "madara":
        return "Madara";
      case "manga_themesia":
        return "Themesia";
      case "wp_comics":
        return "WPComics";
      case "flat_gallery":
        return "Gallery";
      case "mangadex_api":
        return "API v5";
      case "comick_api":
        return "Comick API";
      case "fool_slide":
        return "FoolSlide";
      case "browser_only":
        return "Browser";
      default:
        return "Auto";
    }
  }

  const totalSearchHits = $derived(
    searchResult
      ? searchResult.groups.reduce((sum, g) => sum + g.entries.length, 0)
      : 0
  );
</script>

<div class="flex flex-col gap-4">
  <!-- Search & Quick Action Toolbar -->
  <div class="flex flex-col gap-2.5 sm:flex-row sm:items-center">
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void runSearch();
      }}
      class="flex min-w-0 flex-1 items-center gap-2 rounded-xl border px-3 py-2"
      style="background-color: var(--surface); border-color: var(--border);"
    >
      <Search size={15} class="shrink-0" style="color: var(--text-muted);" />
      <input
        type="text"
        bind:value={searchQuery}
        placeholder={t.sourcesSearchPlaceholder}
        class="min-w-0 flex-1 bg-transparent text-xs outline-none sm:text-sm"
        style="color: var(--text);"
      />
      {#if searchQuery}
        <button
          type="button"
          onclick={() => {
            searchQuery = "";
            searchResult = null;
          }}
          class="rounded-md p-0.5 opacity-70 hover:opacity-100"
          style="color: var(--text-muted);"
          aria-label={t.clearUrlBtn}
        >
          <X size={14} />
        </button>
      {/if}
      <button
        type="submit"
        disabled={searching || !searchQuery.trim()}
        class="inline-flex shrink-0 items-center gap-1.5 rounded-lg px-3 py-1 text-xs font-semibold disabled:opacity-45"
        style="background-color: var(--accent); color: var(--accent-fg);"
      >
        {#if searching}
          <LatticeLoader size="xs" tone="inherit" inline />
        {/if}
        <span>{t.sourcesSearchBtn}</span>
      </button>
    </form>

    <div class="flex shrink-0 items-center gap-2">
      <button
        type="button"
        onclick={() => {
          addModalOpen = true;
        }}
        class="inline-flex items-center gap-1.5 rounded-xl border px-3 py-2 text-xs font-semibold transition-colors hover:brightness-95"
        style="background-color: var(--accent-soft); border-color: var(--accent); color: var(--accent);"
      >
        <Plus size={14} />
        <span>{t.sourcesAddBtn}</span>
      </button>
    </div>
  </div>

  <!-- Per-Website Multi-Tag, Min Chapters & Update Recency Filter Bar (ready as soon as a link is entered or a source is selected) -->
  {#if typedLink || selectedSourceForFilter}
    <WebFilterBar
      schema={activeFilterSchema}
      value={activeFilterValue()}
      loadingSchema={loadingFilterSchema}
      applying={applyingFilter}
      defaultExpanded={true}
      siteLabel={activeFilterLabel()}
      onApply={(q) => void applySourceFilters(q)}
      onReset={resetSourceFilters}
    />
  {/if}

  {#if searchResult}
    <!-- Multi-Source Search Results -->
    <div class="flex flex-wrap items-center justify-between gap-2">
      <div class="text-sm font-semibold">
        {t.sourcesResultsHeading(searchResult.query, totalSearchHits)}
      </div>
      <button
        type="button"
        onclick={() => {
          searchResult = null;
        }}
        class="rounded-lg border px-2.5 py-1 text-xs font-medium"
        style="background-color: var(--surface); border-color: var(--border); color: var(--text-muted);"
      >
        {t.sourcesClearSearchBtn}
      </button>
    </div>

    {#if searchResult.groups.length === 0}
      <div
        class="rounded-2xl border border-dashed py-10 text-center text-xs"
        style="border-color: var(--border); color: var(--text-muted);"
      >
        {t.sourcesSearchEmpty}
      </div>
    {:else}
      <div class="flex flex-col gap-5">
        {#each searchResult.groups as group (group.source.id)}
          <section
            class="flex flex-col gap-3 rounded-2xl border p-4"
            style="background-color: var(--surface); border-color: var(--border);"
          >
            <div class="flex flex-wrap items-center justify-between gap-2">
              <div class="flex items-center gap-2">
                <Globe size={15} style="color: var(--accent);" />
                <span class="text-sm font-semibold">{group.source.displayName}</span>
                <span
                  class="rounded-full px-2 py-0.5 text-[11px] font-medium"
                  style="background-color: var(--surface-hover); color: var(--text-muted);"
                >
                  {group.entries.length}
                </span>
              </div>

              <button
                type="button"
                onclick={() =>
                  router.goBrowse(
                    group.source.baseUrl,
                    undefined,
                    false,
                    group.source.id
                  )}
                class="inline-flex items-center gap-1 rounded-lg px-2.5 py-1 text-xs font-medium hover:opacity-80"
                style="color: var(--accent);"
              >
                <ExternalLink size={12} />
                <span>{t.sourcesBrowseBtn}</span>
              </button>
            </div>

            {#if group.error}
              <div
                class="flex items-center gap-2 rounded-xl border px-3 py-2 text-xs"
                style="border-color: var(--border); background-color: var(--surface-subtle); color: var(--text-muted);"
              >
                <AlertTriangle size={13} style="color: var(--danger);" />
                <span class="truncate">{group.error}</span>
              </div>
            {:else if group.entries.length === 0}
              <div class="text-xs" style="color: var(--text-muted);">{t.sourcesSearchEmpty}</div>
            {:else}
              <div class="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5">
                {#each group.entries as entry (entry.url)}
                  {@const busy = openingEntryUrl === entry.url}
                  <button
                    type="button"
                    disabled={openingEntryUrl !== null}
                    onclick={() => void openCatalogEntry(entry)}
                    class="group flex flex-col overflow-hidden rounded-xl border text-left transition-transform duration-150 hover:-translate-y-0.5 disabled:opacity-70"
                    style="background-color: var(--surface-subtle); border-color: var(--border);"
                  >
                    <div
                      class="relative aspect-[3/4] w-full overflow-hidden"
                      style="background-color: var(--surface-hover);"
                    >
                      {#if entry.coverToken && !failedEntryCovers[entry.url]}
                        <img
                          src={imgProxyUrl(entry.coverToken)}
                          alt={entry.title}
                          loading="lazy"
                          onerror={() => {
                            failedEntryCovers = { ...failedEntryCovers, [entry.url]: true };
                          }}
                          class="h-full w-full object-cover transition-transform duration-200 group-hover:scale-[1.03]"
                        />
                      {:else}
                        <div
                          class="flex h-full w-full items-center justify-center"
                          style="color: var(--text-muted);"
                        >
                          <BookOpen size={26} strokeWidth={1.5} />
                        </div>
                      {/if}
                      {#if entry.latestChapter}
                        <span
                          class="absolute bottom-1.5 left-1.5 max-w-[calc(100%-0.75rem)] truncate rounded px-1.5 py-0.5 text-[10px] font-semibold text-white"
                          style="background-color: rgba(15, 15, 18, 0.8);"
                        >
                          {entry.latestChapter}
                        </span>
                      {/if}
                      {#if busy}
                        <div class="absolute inset-0 flex items-center justify-center bg-black/55 text-white">
                          <LatticeLoader size="md" tone="inherit" />
                        </div>
                      {/if}
                    </div>
                    <div class="p-2">
                      <div class="line-clamp-2 text-xs font-semibold leading-snug">{entry.title}</div>
                    </div>
                  </button>
                {/each}
              </div>
            {/if}
          </section>
        {/each}
      </div>
    {/if}
  {:else}
    <!-- Sort & Filter Controls -->
    {#if sources.length > 0}
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div
          class="flex min-w-44 flex-1 items-center gap-2 rounded-lg border px-2.5 py-1.5 sm:max-w-xs"
          style="background-color: var(--surface); border-color: var(--border);"
        >
          <Search size={13} class="shrink-0" style="color: var(--text-muted);" />
          <input
            type="text"
            bind:value={filterText}
            placeholder={t.sourcesFilterPlaceholder}
            class="min-w-0 flex-1 bg-transparent text-xs outline-none"
          />
        </div>

        <div class="flex items-center gap-1.5">
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
      </div>
    {/if}

    {#if loadingSources}
      <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {#each Array(3) as _, i (i)}
          <div class="h-28 animate-pulse rounded-2xl" style="background-color: var(--surface-hover);"></div>
        {/each}
      </div>
    {:else if sortedFilteredSources().length === 0}
      <div
        class="flex flex-col items-center justify-center gap-2.5 rounded-2xl border border-dashed py-12 text-center"
        style="border-color: var(--border); color: var(--text-muted);"
      >
        <Compass size={28} strokeWidth={1.5} />
        <div class="text-sm font-medium" style="color: var(--text);">{t.sourcesEmptyTitle}</div>
      </div>
    {:else}
      <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {#each sortedFilteredSources() as src (src.id)}
          {@const isSelectedFilter = selectedSourceForFilter?.id === src.id && !typedLink}
          <article
            class="flex flex-col justify-between gap-3 rounded-2xl border p-4 transition-colors"
            style="background-color: var(--surface); border-color: {isSelectedFilter
              ? 'var(--accent)'
              : 'var(--border)'}; opacity: {src.isEnabled ? 1 : 0.62};"
          >
            <div class="flex items-start gap-3">
              <div
                class="flex h-10 w-10 shrink-0 items-center justify-center overflow-hidden rounded-xl border text-sm font-bold"
                style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--accent);"
              >
                {#if src.iconToken && !failedIcons[src.id]}
                  <img
                    src={imgProxyUrl(src.iconToken)}
                    alt={src.displayName}
                    onerror={() => {
                      failedIcons = { ...failedIcons, [src.id]: true };
                    }}
                    class="h-6 w-6 object-contain"
                  />
                {:else}
                  <span>{src.displayName.slice(0, 1).toUpperCase()}</span>
                {/if}
              </div>

              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-1.5">
                  <h3 class="truncate text-sm font-semibold">{src.displayName}</h3>
                  <span
                    class="rounded px-1.5 py-0.5 text-[10px] font-medium"
                    style="background-color: var(--surface-hover); color: var(--text-muted);"
                  >
                    {friendlyKind(src.sourceKind)}
                  </span>
                  {#if !src.isEnabled}
                    <span
                      class="rounded px-1.5 py-0.5 text-[10px] font-semibold"
                      style="background-color: rgba(239, 68, 68, 0.14); color: var(--danger);"
                    >
                      {t.sourcesDisabledBadge}
                    </span>
                  {/if}
                </div>
                <div class="mt-0.5 truncate text-xs" style="color: var(--text-muted);">
                  {hostFromUrl(src.baseUrl)}
                </div>
              </div>
            </div>

            <div class="flex flex-wrap items-center justify-between gap-2 border-t pt-2.5" style="border-color: var(--border);">
              <div class="flex items-center gap-1.5">
                <button
                  type="button"
                  onclick={() => router.goBrowse(src.baseUrl, undefined, false, src.id)}
                  class="inline-flex items-center gap-1.5 rounded-lg px-2.5 py-1 text-xs font-semibold"
                  style="background-color: var(--accent-soft); color: var(--accent);"
                >
                  <ExternalLink size={12} />
                  <span>{t.sourcesBrowseBtn}</span>
                </button>
                <button
                  type="button"
                  onclick={() => void openSourceFilters(src)}
                  class="inline-flex items-center gap-1 rounded-lg border px-2 py-1 text-xs font-medium"
                  style="border-color: {isSelectedFilter
                    ? 'var(--accent)'
                    : 'var(--border)'}; background-color: {isSelectedFilter
                    ? 'var(--accent-soft)'
                    : 'var(--surface-subtle)'}; color: {isSelectedFilter ? 'var(--accent)' : 'var(--text)'};"
                >
                  <Filter size={11} />
                  <span>{t.sourcesFilterThisSiteBtn}</span>
                </button>
              </div>

              <div class="flex items-center gap-1">
                <button
                  type="button"
                  onclick={() => startEditSource(src)}
                  class="rounded-lg border p-1.5 hover:brightness-95"
                  style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text-muted);"
                  title={t.sourcesEditBtn}
                  aria-label={t.sourcesEditBtn}
                >
                  <Edit3 size={13} />
                </button>

                <button
                  type="button"
                  onclick={() => void moveVisibility(src)}
                  class="rounded-lg border p-1.5 hover:brightness-95"
                  style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text-muted);"
                  title={scope === "main" ? t.sourcesMoveToSuppBtn : t.sourcesMoveToMainBtn}
                  aria-label={scope === "main" ? t.sourcesMoveToSuppBtn : t.sourcesMoveToMainBtn}
                >
                  <ArrowLeftRight size={13} />
                </button>

                {#if confirmDeleteId === src.id}
                  <button
                    type="button"
                    onclick={() => void removeSource(src.id)}
                    class="inline-flex items-center gap-1 rounded-lg px-2 py-1 text-[11px] font-semibold text-white"
                    style="background-color: var(--danger);"
                  >
                    <Check size={12} />
                    <span>{t.sourcesDeleteBtn}</span>
                  </button>
                  <button
                    type="button"
                    onclick={() => {
                      confirmDeleteId = null;
                    }}
                    class="rounded-lg border p-1"
                    style="border-color: var(--border); color: var(--text-muted);"
                  >
                    <X size={12} />
                  </button>
                {:else}
                  <button
                    type="button"
                    onclick={() => {
                      confirmDeleteId = src.id;
                    }}
                    class="rounded-lg border p-1.5 hover:brightness-95"
                    style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--danger);"
                    title={t.sourcesDeleteBtn}
                    aria-label={t.sourcesDeleteBtn}
                  >
                    <Trash2 size={13} />
                  </button>
                {/if}
              </div>
            </div>
          </article>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<!-- Add Source Modal -->
{#if addModalOpen}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm" role="dialog" aria-modal="true">
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void submitAddSource();
      }}
      class="flex max-h-[calc(100dvh-2rem-var(--safe-top)-var(--safe-bottom))] w-full max-w-md flex-col gap-3.5 overflow-y-auto overscroll-contain rounded-2xl border p-5 shadow-2xl"
      style="background-color: var(--surface); border-color: var(--border);"
    >
      <div class="flex items-center justify-between">
        <h3 class="text-base font-bold">{t.sourcesAddModalTitle}</h3>
        <button
          type="button"
          onclick={() => (addModalOpen = false)}
          class="rounded-lg p-1 opacity-70 hover:opacity-100"
        >
          <X size={16} />
        </button>
      </div>

      <label class="flex flex-col gap-1 text-xs">
        <span class="font-medium">{t.sourcesAddUrlLabel}</span>
        <input
          type="url"
          required
          bind:value={addUrl}
          placeholder={t.sourcesAddUrlPlaceholder}
          class="rounded-xl border px-3 py-2 text-xs outline-none"
          style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
        />
      </label>

      <label class="flex flex-col gap-1 text-xs">
        <span class="font-medium">{t.sourcesAddNameLabel}</span>
        <input
          type="text"
          bind:value={addName}
          placeholder={t.sourcesAddNamePlaceholder}
          class="rounded-xl border px-3 py-2 text-xs outline-none"
          style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
        />
      </label>

      <div class="flex justify-end gap-2 pt-1">
        <button
          type="button"
          onclick={() => (addModalOpen = false)}
          class="rounded-xl border px-4 py-2 text-xs font-medium"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          {t.cancel}
        </button>
        <button
          type="submit"
          disabled={addingSource || !addUrl.trim()}
          class="inline-flex items-center gap-1.5 rounded-xl px-4 py-2 text-xs font-semibold disabled:opacity-50"
          style="background-color: var(--accent); color: var(--accent-fg);"
        >
          {#if addingSource}
            <LatticeLoader size="xs" tone="inherit" inline />
          {/if}
          <span>{t.sourcesSaveBtn}</span>
        </button>
      </div>
    </form>
  </div>
{/if}

<!-- Edit Source & Mirror Modal -->
{#if editTarget}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm" role="dialog" aria-modal="true">
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void submitEditSource();
      }}
      class="flex max-h-[calc(100dvh-2rem-var(--safe-top)-var(--safe-bottom))] w-full max-w-md flex-col gap-3.5 overflow-y-auto overscroll-contain rounded-2xl border p-5 shadow-2xl"
      style="background-color: var(--surface); border-color: var(--border);"
    >
      <div class="flex items-center justify-between">
        <h3 class="text-base font-bold">{t.sourcesEditModalTitle}</h3>
        <button
          type="button"
          onclick={() => (editTarget = null)}
          class="rounded-lg p-1 opacity-70 hover:opacity-100"
        >
          <X size={16} />
        </button>
      </div>

      <label class="flex flex-col gap-1 text-xs">
        <span class="font-medium">{t.sourcesAddNameLabel}</span>
        <input
          type="text"
          required
          bind:value={editName}
          class="rounded-xl border px-3 py-2 text-xs outline-none"
          style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
        />
      </label>

      <label class="flex flex-col gap-1 text-xs">
        <span class="font-medium">{t.sourcesMirrorLabel}</span>
        <input
          type="url"
          required
          bind:value={editBaseUrl}
          class="rounded-xl border px-3 py-2 text-xs outline-none"
          style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
        />
      </label>

      <div class="flex items-center justify-between rounded-xl border px-3 py-2 text-xs" style="border-color: var(--border); background-color: var(--surface-subtle);">
        <span class="font-medium">{t.sourcesEnabledLabel}</span>
        <SquishSwitch
          size="sm"
          checked={editEnabled}
          ariaLabel={t.sourcesEnabledLabel}
          onchange={(v) => {
            editEnabled = v;
          }}
        />
      </div>

      <div class="flex justify-end gap-2 pt-1">
        <button
          type="button"
          onclick={() => (editTarget = null)}
          class="rounded-xl border px-4 py-2 text-xs font-medium"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          {t.cancel}
        </button>
        <button
          type="submit"
          disabled={savingEdit}
          class="inline-flex items-center gap-1.5 rounded-xl px-4 py-2 text-xs font-semibold disabled:opacity-50"
          style="background-color: var(--accent); color: var(--accent-fg);"
        >
          {#if savingEdit}
            <LatticeLoader size="xs" tone="inherit" inline />
          {/if}
          <span>{t.sourcesSaveBtn}</span>
        </button>
      </div>
    </form>
  </div>
{/if}
