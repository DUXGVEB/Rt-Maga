<script lang="ts">
  import {
    AlertTriangle,
    ArrowDown,
    ArrowLeftRight,
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
  import { flip } from "svelte/animate";
  import { api, formatAppError, imgProxyUrl } from "../api";
  import {
    adaptiveOverlay,
    backdropFade,
    badgePop,
    collapseReveal,
    createOverlayGuard,
    flipDuration,
    itemExit,
    popoverPop,
    pressable,
    shouldStaggerItem,
    staggerDelay,
    viewReveal,
  } from "../motion";
  import { deviceEnv } from "../stores/deviceEnv.svelte";
  import { heroTransitionStore } from "../stores/heroTransition.svelte";
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
  const modalGuard = createOverlayGuard(180);

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
    if (!q) {
      lastInitQuery = "";
      return;
    }
    if (q !== lastInitQuery) {
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

  async function openCatalogEntry(entry: CatalogEntryView, triggerEl?: HTMLElement | null) {
    if (openingEntryUrl) return;
    const coverEl = triggerEl?.querySelector<HTMLElement>("[data-hero-cover-src]") ?? null;
    openingEntryUrl = entry.url;
    try {
      const res = await api.resolveUrl(entry.url);
      if (res.kind === "catalog" && res.catalog) {
        router.goBrowse(res.catalog.url, res.catalog, false, res.catalog.sourceId ?? undefined);
      } else if (res.manga) {
        if (res.startAt) {
          router.goReader(res.manga.id, res.startAt.url);
        } else {
          heroTransitionStore.captureFromElement(
            coverEl,
            res.manga.coverToken ?? entry.coverToken,
            res.manga.id
          );
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
  {#if scope === "supplemental"}
    <!-- Minimal Supplemental Sources (Settings > Network & LAN > Additional Sources):
         NO tag-filtering, NO search/filter clutter, NO CMS/disabled badges — ONLY Name + URL + minimal actions -->
    <div class="flex items-center justify-end">
      <button
        type="button"
        use:pressable
        onclick={() => {
          if (!modalGuard.canAct()) return;
          modalGuard.lock();
          addModalOpen = true;
        }}
        class="btn-3d btn-3d-accent inline-flex min-h-[36px] items-center gap-1.5 rounded-xl px-3 py-1.5 text-xs font-semibold"
      >
        <Plus size={14} />
        <span>{t.sourcesAddBtn}</span>
      </button>
    </div>

    {#if loadingSources}
      <div class="flex flex-col gap-2">
        {#each Array(2) as _, i (i)}
          <div class="h-16 animate-pulse rounded-2xl" style="background-color: var(--surface-hover);"></div>
        {/each}
      </div>
    {:else if sources.length === 0}
      <div
        in:viewReveal|local={{ duration: 160, y: 6 }}
        class="flex flex-col items-center justify-center gap-2 rounded-2xl border border-dashed py-10 text-center"
        style="border-color: var(--border); color: var(--text-muted);"
      >
        <Compass size={24} strokeWidth={1.5} />
        <div class="text-xs font-medium" style="color: var(--text);">{t.sourcesEmptyTitle}</div>
      </div>
    {:else}
      <div class="flex flex-col gap-2">
        {#each sources as src, idx (src.id)}
          {@const animateStagger = shouldStaggerItem(idx, sources.length)}
          <div
            animate:flip={{ duration: flipDuration(175, sources.length) }}
            out:itemExit|local
            class:stagger-item={animateStagger}
            style={animateStagger ? `--stagger-delay: ${staggerDelay(idx, 16, 180, sources.length)}ms;` : undefined}
          >
            <article
              class="card-contain flex items-center justify-between gap-3 rounded-2xl border px-4 py-3"
              style="background-color: var(--surface); border-color: var(--border);"
            >
              <div class="min-w-0 flex-1">
                <h3 class="truncate text-sm font-semibold">{src.displayName}</h3>
                <div class="mt-0.5 truncate text-xs" style="color: var(--text-muted);">
                  {src.baseUrl}
                </div>
              </div>

              <div class="flex shrink-0 items-center gap-1.5">
                <button
                  type="button"
                  use:pressable
                  onclick={() => router.goBrowse(src.baseUrl, undefined, false, src.id)}
                  class="btn-3d btn-3d-subtle flex h-9 w-9 items-center justify-center rounded-xl"
                  style="color: var(--accent);"
                  title={t.sourcesBrowseBtn}
                  aria-label={t.sourcesBrowseBtn}
                >
                  <ExternalLink size={14} />
                </button>

                <button
                  type="button"
                  use:pressable
                  onclick={() => {
                    if (!modalGuard.canAct()) return;
                    modalGuard.lock();
                    startEditSource(src);
                  }}
                  class="btn-3d btn-3d-subtle flex h-9 w-9 items-center justify-center rounded-xl"
                  style="color: var(--text-muted);"
                  title={t.sourcesEditBtn}
                  aria-label={t.sourcesEditBtn}
                >
                  <Edit3 size={14} />
                </button>

                {#if confirmDeleteId === src.id}
                  <div in:popoverPop|local={{ duration: 140, y: 0 }} class="flex items-center gap-1">
                    <button
                      type="button"
                      use:pressable
                      onclick={() => void removeSource(src.id)}
                      class="btn-3d btn-3d-danger inline-flex min-h-[36px] items-center gap-1 rounded-xl px-2.5 py-1 text-[11px] font-semibold"
                    >
                      <Check size={12} />
                      <span>{t.sourcesDeleteBtn}</span>
                    </button>
                    <button
                      type="button"
                      use:pressable
                      onclick={() => {
                        confirmDeleteId = null;
                      }}
                      class="btn-3d btn-3d-subtle flex h-9 w-9 items-center justify-center rounded-xl"
                      style="color: var(--text-muted);"
                      aria-label={t.cancel}
                    >
                      <X size={13} />
                    </button>
                  </div>
                {:else}
                  <button
                    type="button"
                    use:pressable
                    onclick={() => {
                      confirmDeleteId = src.id;
                    }}
                    class="btn-3d btn-3d-subtle flex h-9 w-9 items-center justify-center rounded-xl"
                    style="color: var(--danger);"
                    title={t.sourcesDeleteBtn}
                    aria-label={t.sourcesDeleteBtn}
                  >
                    <Trash2 size={14} />
                  </button>
                {/if}
              </div>
            </article>
          </div>
        {/each}
      </div>
    {/if}
  {:else}
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
        <span transition:badgePop|local class="inline-flex">
          <button
            type="button"
            onclick={() => {
              searchQuery = "";
              searchResult = null;
            }}
            class="gpu-press rounded-md p-0.5 opacity-70 hover:opacity-100"
            style="color: var(--text-muted);"
            aria-label={t.clearUrlBtn}
          >
            <X size={14} />
          </button>
        </span>
      {/if}
      <button
        type="submit"
        use:pressable
        disabled={searching || !searchQuery.trim()}
        class="btn-3d btn-3d-accent inline-flex shrink-0 items-center gap-1.5 rounded-lg px-3 py-1 text-xs font-semibold disabled:opacity-45"
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
        use:pressable
        onclick={() => {
          if (!modalGuard.canAct()) return;
          modalGuard.lock();
          addModalOpen = true;
        }}
        class="btn-3d btn-3d-surface inline-flex min-h-[38px] items-center gap-1.5 rounded-xl px-3.5 py-1.5 text-xs font-semibold"
        style="color: var(--accent);"
      >
        <Plus size={14} />
        <span>{t.sourcesAddBtn}</span>
      </button>
    </div>
  </div>

  <!-- Per-Website Multi-Tag, Min Chapters & Update Recency Filter Bar (ready as soon as a link is entered or a source is selected) -->
  {#if typedLink || selectedSourceForFilter}
    <div transition:collapseReveal|local>
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
    </div>
  {/if}

  {#if searchResult}
    <!-- Multi-Source Search Results -->
    <div in:viewReveal|local={{ duration: 165, y: 6 }} class="flex flex-col gap-4">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="text-sm font-semibold">
          {t.sourcesResultsHeading(searchResult.query, totalSearchHits)}
        </div>
        <button
          type="button"
          use:pressable
          onclick={() => {
            searchResult = null;
          }}
          class="btn-3d btn-3d-surface rounded-xl px-3 py-1.5 text-xs font-medium"
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
                  use:pressable
                  onclick={() =>
                    router.goBrowse(
                      group.source.baseUrl,
                      undefined,
                      false,
                      group.source.id
                    )}
                  class="btn-3d btn-3d-subtle inline-flex items-center gap-1 rounded-lg px-2.5 py-1 text-xs font-medium"
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
                <div class="fluid-manga-grid">
                  {#each group.entries as entry, idx (entry.url)}
                    {@const busy = openingEntryUrl === entry.url}
                    {@const animateStagger = shouldStaggerItem(idx, group.entries.length)}
                    <button
                      type="button"
                      use:pressable
                      disabled={openingEntryUrl !== null}
                      onclick={(e) => void openCatalogEntry(entry, e.currentTarget)}
                      class="card-cq card-contain card-press surface-card group flex flex-col overflow-hidden rounded-2xl text-left disabled:opacity-70"
                      class:stagger-item={animateStagger}
                      style={animateStagger ? `--stagger-delay: ${staggerDelay(idx, 16, 180, group.entries.length)}ms;` : undefined}
                    >
                      <div
                        data-hero-cover-src
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
                            class="h-full w-full object-cover transition-transform duration-300 ease-out group-hover:scale-[1.04]"
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
                            class="absolute bottom-1.5 left-1.5 max-w-[calc(100%-0.75rem)] truncate rounded-lg px-1.5 py-0.5 text-[10px] font-semibold text-white backdrop-blur-md"
                            style="background-color: rgba(11, 11, 15, 0.8);"
                          >
                            {entry.latestChapter}
                          </span>
                        {/if}
                        {#if busy}
                          <div
                            transition:backdropFade|local={{ duration: 140 }}
                            class="absolute inset-0 flex items-center justify-center bg-black/55 text-white"
                          >
                            <LatticeLoader size="md" tone="inherit" />
                          </div>
                        {/if}
                      </div>
                      <div class="card-body p-2.5">
                        <div class="card-title line-clamp-2 font-semibold">{entry.title}</div>
                      </div>
                    </button>
                  {/each}
                </div>
              {/if}
            </section>
          {/each}
        </div>
      {/if}
    </div>
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
            use:pressable
            onclick={() => {
              sortDir = sortDir === "desc" ? "asc" : "desc";
            }}
            class="btn-3d btn-3d-surface inline-flex items-center gap-1 rounded-lg px-2.5 py-1.5 text-xs font-medium"
            title={sortDir === "desc" ? t.sortDesc : t.sortAsc}
          >
            <ArrowDown size={13} class="chevron-rot" data-open={sortDir === "asc" ? "true" : "false"} />
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
        in:viewReveal|local={{ duration: 160, y: 6 }}
        class="flex flex-col items-center justify-center gap-2.5 rounded-2xl border border-dashed py-12 text-center"
        style="border-color: var(--border); color: var(--text-muted);"
      >
        <Compass size={28} strokeWidth={1.5} />
        <div class="text-sm font-medium" style="color: var(--text);">{t.sourcesEmptyTitle}</div>
      </div>
    {:else}
      {@const visibleSources = sortedFilteredSources()}
      <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {#each visibleSources as src, idx (src.id)}
          {@const isSelectedFilter = selectedSourceForFilter?.id === src.id && !typedLink}
          {@const animateStagger = shouldStaggerItem(idx, visibleSources.length)}
          <div
            animate:flip={{ duration: flipDuration(175, visibleSources.length) }}
            out:itemExit|local
            class:stagger-item={animateStagger}
            style={animateStagger ? `--stagger-delay: ${staggerDelay(idx, 16, 180, visibleSources.length)}ms;` : undefined}
          >
            <article
              class="card-contain flex h-full flex-col justify-between gap-3 rounded-2xl border p-4 transition-colors"
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
                    use:pressable
                    onclick={() => router.goBrowse(src.baseUrl, undefined, false, src.id)}
                    class="btn-3d btn-3d-subtle inline-flex min-h-[38px] items-center gap-1.5 rounded-xl px-3 py-1.5 text-xs font-semibold"
                    style="background-color: var(--accent-soft); border-color: color-mix(in srgb, var(--accent) 32%, transparent); color: var(--accent);"
                  >
                    <ExternalLink size={12} />
                    <span>{t.sourcesBrowseBtn}</span>
                  </button>
                  <button
                    type="button"
                    use:pressable
                    onclick={() => void openSourceFilters(src)}
                    class="btn-3d btn-3d-subtle inline-flex min-h-[38px] items-center gap-1 rounded-xl px-2.5 py-1.5 text-xs font-medium"
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
                    use:pressable
                    onclick={() => {
                      if (!modalGuard.canAct()) return;
                      modalGuard.lock();
                      startEditSource(src);
                    }}
                    class="btn-3d btn-3d-subtle flex h-9 w-9 items-center justify-center rounded-xl"
                    style="color: var(--text-muted);"
                    title={t.sourcesEditBtn}
                    aria-label={t.sourcesEditBtn}
                  >
                    <Edit3 size={14} />
                  </button>

                  <button
                    type="button"
                    use:pressable
                    onclick={() => void moveVisibility(src)}
                    class="btn-3d btn-3d-subtle flex h-9 w-9 items-center justify-center rounded-xl"
                    style="color: var(--text-muted);"
                    title={t.sourcesMoveToSuppBtn}
                    aria-label={t.sourcesMoveToSuppBtn}
                  >
                    <ArrowLeftRight size={14} />
                  </button>

                  {#if confirmDeleteId === src.id}
                    <div in:popoverPop|local={{ duration: 140, y: 0 }} class="flex items-center gap-1">
                      <button
                        type="button"
                        use:pressable
                        onclick={() => void removeSource(src.id)}
                        class="btn-3d btn-3d-danger inline-flex min-h-[36px] items-center gap-1 rounded-xl px-2.5 py-1 text-[11px] font-semibold"
                      >
                        <Check size={12} />
                        <span>{t.sourcesDeleteBtn}</span>
                      </button>
                      <button
                        type="button"
                        use:pressable
                        onclick={() => {
                          confirmDeleteId = null;
                        }}
                        class="btn-3d btn-3d-subtle flex h-9 w-9 items-center justify-center rounded-xl"
                        style="color: var(--text-muted);"
                        aria-label={t.cancel}
                      >
                        <X size={13} />
                      </button>
                    </div>
                  {:else}
                    <button
                      type="button"
                      use:pressable
                      onclick={() => {
                        confirmDeleteId = src.id;
                      }}
                      class="btn-3d btn-3d-subtle flex h-9 w-9 items-center justify-center rounded-xl"
                      style="color: var(--danger);"
                      title={t.sourcesDeleteBtn}
                      aria-label={t.sourcesDeleteBtn}
                    >
                      <Trash2 size={14} />
                    </button>
                  {/if}
                </div>
              </div>
            </article>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
  {/if}
</div>

<!-- Add Source Modal (Bottom Sheet on Compact Mobile, Centered Dialog on sm+) -->
{#if addModalOpen}
  <div
    class="fixed inset-0 z-50 flex items-end justify-center p-0 sm:items-center sm:p-4"
    style="padding-top: max(1rem, var(--safe-top)); padding-left: var(--safe-left); padding-right: var(--safe-right);"
    role="dialog"
    aria-modal="true"
  >
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      transition:backdropFade|local
      onclick={() => {
        if (!addingSource && modalGuard.canAct()) {
          modalGuard.lock();
          addModalOpen = false;
        }
      }}
      class="modal-backdrop fixed inset-0"
    ></div>

    <form
      transition:adaptiveOverlay|local={{ isMobile: deviceEnv.layoutTier === "compact" }}
      onsubmit={(e) => {
        e.preventDefault();
        void submitAddSource();
      }}
      class="modal-cq relative z-10 flex max-h-[calc(100dvh-1.5rem-var(--safe-top))] w-full max-w-md flex-col gap-3.5 overflow-y-auto overscroll-contain rounded-t-3xl border-t p-5 sm:max-h-[calc(100dvh-2rem-var(--safe-top)-var(--safe-bottom))] sm:rounded-2xl sm:border"
      style="background-color: var(--surface); border-color: var(--border); box-shadow: var(--shadow-lg); padding-bottom: max(1.25rem, var(--safe-bottom));"
    >
      <div class="sheet-handle -mt-2 sm:hidden" aria-hidden="true"></div>
      <div class="flex items-center justify-between">
        <h3 class="text-base font-bold">{t.sourcesAddModalTitle}</h3>
        <button
          type="button"
          onclick={() => (addModalOpen = false)}
          class="gpu-press flex h-9 w-9 items-center justify-center rounded-xl opacity-70 hover:opacity-100"
          aria-label={t.cancel}
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
          class="min-h-[42px] rounded-xl border px-3 py-2 text-xs outline-none"
          style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
        />
      </label>

      <label class="flex flex-col gap-1 text-xs">
        <span class="font-medium">{t.sourcesAddNameLabel}</span>
        <input
          type="text"
          bind:value={addName}
          placeholder={t.sourcesAddNamePlaceholder}
          class="min-h-[42px] rounded-xl border px-3 py-2 text-xs outline-none"
          style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
        />
      </label>

      <div class="modal-footer-stack flex justify-end gap-2 pt-1">
        <button
          type="button"
          use:pressable
          onclick={() => (addModalOpen = false)}
          class="btn-3d btn-3d-subtle min-h-[42px] rounded-xl px-4 py-2 text-xs font-medium"
        >
          {t.cancel}
        </button>
        <button
          type="submit"
          use:pressable
          disabled={addingSource || !addUrl.trim()}
          class="btn-3d btn-3d-accent inline-flex min-h-[42px] items-center justify-center gap-1.5 rounded-xl px-4 py-2 text-xs font-semibold disabled:opacity-50"
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

<!-- Edit Source & Mirror Modal (Bottom Sheet on Compact Mobile, Centered Dialog on sm+) -->
{#if editTarget}
  <div
    class="fixed inset-0 z-50 flex items-end justify-center p-0 sm:items-center sm:p-4"
    style="padding-top: max(1rem, var(--safe-top)); padding-left: var(--safe-left); padding-right: var(--safe-right);"
    role="dialog"
    aria-modal="true"
  >
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      transition:backdropFade|local
      onclick={() => {
        if (!savingEdit && modalGuard.canAct()) {
          modalGuard.lock();
          editTarget = null;
        }
      }}
      class="modal-backdrop fixed inset-0"
    ></div>

    <form
      transition:adaptiveOverlay|local={{ isMobile: deviceEnv.layoutTier === "compact" }}
      onsubmit={(e) => {
        e.preventDefault();
        void submitEditSource();
      }}
      class="modal-cq relative z-10 flex max-h-[calc(100dvh-1.5rem-var(--safe-top))] w-full max-w-md flex-col gap-3.5 overflow-y-auto overscroll-contain rounded-t-3xl border-t p-5 sm:max-h-[calc(100dvh-2rem-var(--safe-top)-var(--safe-bottom))] sm:rounded-2xl sm:border"
      style="background-color: var(--surface); border-color: var(--border); box-shadow: var(--shadow-lg); padding-bottom: max(1.25rem, var(--safe-bottom));"
    >
      <div class="sheet-handle -mt-2 sm:hidden" aria-hidden="true"></div>
      <div class="flex items-center justify-between">
        <h3 class="text-base font-bold">{t.sourcesEditModalTitle}</h3>
        <button
          type="button"
          onclick={() => (editTarget = null)}
          class="gpu-press flex h-9 w-9 items-center justify-center rounded-xl opacity-70 hover:opacity-100"
          aria-label={t.cancel}
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
          class="min-h-[42px] rounded-xl border px-3 py-2 text-xs outline-none"
          style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
        />
      </label>

      <label class="flex flex-col gap-1 text-xs">
        <span class="font-medium">{t.sourcesMirrorLabel}</span>
        <input
          type="url"
          required
          bind:value={editBaseUrl}
          class="min-h-[42px] rounded-xl border px-3 py-2 text-xs outline-none"
          style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
        />
      </label>

      <div class="flex min-h-[44px] items-center justify-between rounded-xl border px-3 py-2 text-xs" style="border-color: var(--border); background-color: var(--surface-subtle);">
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

      {#if scope === "supplemental" && editTarget}
        <button
          type="button"
          use:pressable
          onclick={() => {
            const target = editTarget;
            if (!target) return;
            editTarget = null;
            void moveVisibility(target);
          }}
          class="btn-3d btn-3d-subtle inline-flex min-h-[40px] items-center justify-center gap-1.5 rounded-xl px-3 py-2 text-xs font-medium"
          style="color: var(--text-muted);"
        >
          <ArrowLeftRight size={13} />
          <span>{t.sourcesMoveToMainBtn}</span>
        </button>
      {/if}

      <div class="modal-footer-stack flex justify-end gap-2 pt-1">
        <button
          type="button"
          use:pressable
          onclick={() => (editTarget = null)}
          class="btn-3d btn-3d-subtle min-h-[42px] rounded-xl px-4 py-2 text-xs font-medium"
        >
          {t.cancel}
        </button>
        <button
          type="submit"
          use:pressable
          disabled={savingEdit}
          class="btn-3d btn-3d-accent inline-flex min-h-[42px] items-center justify-center gap-1.5 rounded-xl px-4 py-2 text-xs font-semibold disabled:opacity-50"
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
