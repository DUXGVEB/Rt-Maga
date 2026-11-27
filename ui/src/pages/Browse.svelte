<script lang="ts">
  import {
    ArrowLeft,
    BookOpen,
    ChevronLeft,
    ChevronRight,
    Globe,
    RefreshCw,
    Search,
  } from "lucide-svelte";
  import { api, formatAppError, imgProxyUrl } from "../lib/api";
  import LatticeLoader from "../lib/components/LatticeLoader.svelte";
  import SourceDiagnosticCard from "../lib/components/SourceDiagnosticCard.svelte";
  import WebFilterBar from "../lib/components/WebFilterBar.svelte";
  import {
    backdropFade,
    pressable,
    shouldStaggerItem,
    staggerDelay,
    viewReveal,
  } from "../lib/motion";
  import { heroTransitionStore } from "../lib/stores/heroTransition.svelte";
  import { router } from "../lib/stores/router.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import {
    emptyFilterQuery,
    emptyFilterSchema,
    filterKeyFor,
    hasActiveFilters,
    webFilterStore,
  } from "../lib/stores/webFilter.svelte";
  import type {
    CatalogEntryView,
    CatalogFilterQuery,
    CatalogPageView,
    FilterTagOption,
    FormattedAppError,
    NavLinkView,
    SiteFilterSchema,
  } from "../lib/types";

  let {
    url,
    sourceId,
    initialCatalog,
    initialTag,
  }: {
    url: string;
    sourceId?: number;
    initialCatalog?: CatalogPageView;
    initialTag?: string;
  } = $props();

  let catalog = $state<CatalogPageView | null>(null);
  let loading = $state(false);
  let openingEntryUrl = $state<string | null>(null);
  let searchQuery = $state("");
  let activeQuery = $state<string | null>(null);
  let currentPage = $state(1);
  let jumpPageInput = $state("");
  let failedCovers = $state<Record<string, boolean>>({});
  let errorState = $state<FormattedAppError | null>(null);
  let verifying = $state(false);
  let loggingIn = $state(false);
  let loadSeq = 0;
  let lastAppliedTagKey = "";

  const t = $derived(settingsStore.t);

  const effectiveSourceId = $derived(sourceId ?? catalog?.sourceId ?? undefined);
  const effectiveInitialTag = $derived(
    (
      initialTag ??
      (router.current.name === "browse" ? router.current.initialTag : undefined) ??
      ""
    ).trim()
  );
  const activeFilterKey = $derived(filterKeyFor(effectiveSourceId, catalog?.url ?? url));
  const activeFilters = $derived(webFilterStore.getQuery(activeFilterKey));
  const activeSchema = $derived(
    catalog?.filterSchema ?? webFilterStore.getSchema(activeFilterKey) ?? emptyFilterSchema()
  );

  function normalizeTagSlug(raw: string): string {
    return raw
      .trim()
      .toLowerCase()
      .normalize("NFKD")
      .replace(/[\u0300-\u036f]/g, "")
      .replace(/[^a-z0-9\p{L}]+/gu, "-")
      .replace(/^-+|-+$/g, "");
  }

  function findMatchingTag(
    tags: readonly FilterTagOption[] | undefined,
    label: string
  ): FilterTagOption | null {
    if (!tags || tags.length === 0) return null;
    const rawLower = label.trim().toLowerCase();
    const targetSlug = normalizeTagSlug(label);
    if (!rawLower) return null;

    // 1) Exact case-insensitive match on label or id
    for (const tag of tags) {
      if (tag.label.trim().toLowerCase() === rawLower || tag.id.trim().toLowerCase() === rawLower) {
        return tag;
      }
    }
    // 2) Slug-normalized match on label, id, or trailing URL segment
    if (targetSlug) {
      for (const tag of tags) {
        if (
          normalizeTagSlug(tag.label) === targetSlug ||
          normalizeTagSlug(tag.id) === targetSlug
        ) {
          return tag;
        }
        if (tag.url) {
          const cleaned = tag.url.replace(/[?#].*$/, "").replace(/\/+$/, "");
          const tail = cleaned.split("/").pop() ?? "";
          if (tail && normalizeTagSlug(decodeURIComponent(tail)) === targetSlug) {
            return tag;
          }
        }
      }
    }
    return null;
  }

  function findMatchingCategory(
    categories: readonly NavLinkView[] | undefined,
    label: string
  ): NavLinkView | null {
    if (!categories || categories.length === 0) return null;
    const rawLower = label.trim().toLowerCase();
    const targetSlug = normalizeTagSlug(label);
    for (const cat of categories) {
      if (cat.label.trim().toLowerCase() === rawLower) return cat;
      if (targetSlug && normalizeTagSlug(cat.label) === targetSlug) return cat;
    }
    return null;
  }

  async function loadWithInitialTag(
    targetUrl: string,
    sid: number | undefined,
    tagLabel: string
  ) {
    const seq = ++loadSeq;
    loading = true;
    errorState = null;
    failedCovers = {};
    const key = filterKeyFor(sid, targetUrl);

    let schema: SiteFilterSchema | null = webFilterStore.getSchema(key);
    if (!schema || schema.tags.length === 0) {
      try {
        schema = await api.getWebFilterSchema(
          sid !== undefined ? { sourceId: sid } : { url: targetUrl }
        );
        if (seq !== loadSeq) return;
        webFilterStore.setSchema(key, schema);
      } catch {
        schema = schema ?? emptyFilterSchema();
      }
    }

    const matched = findMatchingTag(schema.tags, tagLabel);
    if (matched) {
      const nextFilters: CatalogFilterQuery = {
        ...emptyFilterQuery(),
        includeTags: [matched.id],
      };
      webFilterStore.setQuery(key, nextFilters);
      searchQuery = "";
      activeQuery = null;
      if (sid !== undefined) {
        await loadFromSavedSource(sid, null, 1, nextFilters, false);
        return;
      }
      if (matched.url && !schema.includeTagParam && !schema.filterEndpoint) {
        await loadCatalogUrl(matched.url, 1, emptyFilterQuery(), false);
        return;
      }
      await loadCatalogUrl(targetUrl, 1, nextFilters, false);
      return;
    }

    // Fallback when filter schema doesn't expose a matching tag ID yet:
    // Load base catalog or tag search, and if base catalog brings a fuller schema/category list, apply it cleanly.
    const fallbackFilters: CatalogFilterQuery = {
      ...emptyFilterQuery(),
      query: tagLabel,
    };
    webFilterStore.setQuery(key, fallbackFilters);
    searchQuery = tagLabel;
    activeQuery = tagLabel;

    let loadedCat: CatalogPageView | null = null;
    if (sid !== undefined) {
      loadedCat = await loadFromSavedSource(sid, tagLabel, 1, fallbackFilters, false);
    } else {
      loadedCat = await loadCatalogUrl(targetUrl, 1, fallbackFilters, false);
    }
    if (!loadedCat || seq !== loadSeq) return;

    const lateTag = findMatchingTag(loadedCat.filterSchema.tags, tagLabel);
    if (lateTag) {
      const refinedFilters: CatalogFilterQuery = {
        ...emptyFilterQuery(),
        includeTags: [lateTag.id],
      };
      webFilterStore.setQuery(filterKeyFor(sid ?? loadedCat.sourceId, loadedCat.url), refinedFilters);
      searchQuery = "";
      activeQuery = null;
      if (sid !== undefined) {
        await loadFromSavedSource(sid, null, 1, refinedFilters, false);
      } else if (lateTag.url && !loadedCat.filterSchema.includeTagParam && !loadedCat.filterSchema.filterEndpoint) {
        await loadCatalogUrl(lateTag.url, 1, emptyFilterQuery(), false);
      } else {
        await loadCatalogUrl(loadedCat.url, 1, refinedFilters, false);
      }
      return;
    }

    if (loadedCat.entries.length === 0) {
      const matchedCat = findMatchingCategory(loadedCat.categories, tagLabel);
      if (matchedCat) {
        webFilterStore.resetQuery(filterKeyFor(sid ?? loadedCat.sourceId, loadedCat.url));
        searchQuery = "";
        activeQuery = null;
        await loadCatalogUrl(matchedCat.url, 1, emptyFilterQuery(), false);
      }
    }
  }

  async function loadFromSavedSource(
    id: number,
    query: string | null,
    page: number,
    filters: CatalogFilterQuery,
    forceRefresh = false
  ): Promise<CatalogPageView | null> {
    const seq = ++loadSeq;
    loading = true;
    errorState = null;
    failedCovers = {};
    try {
      const mergedFilters: CatalogFilterQuery = {
        ...filters,
        query: query ?? filters.query ?? null,
      };
      const cat = await api.browseSavedSource(
        id,
        query,
        page,
        hasActiveFilters(mergedFilters) ? mergedFilters : null,
        forceRefresh
      );
      if (seq !== loadSeq) return null;
      catalog = cat;
      webFilterStore.setSchema(filterKeyFor(id, cat.url), cat.filterSchema);
      activeQuery = query;
      currentPage = cat.pagination.currentPage || page;
      searchQuery = cat.query ?? query ?? "";
      return cat;
    } catch (err) {
      if (seq !== loadSeq) return null;
      errorState = formatAppError(err, settingsStore.value.language);
      return null;
    } finally {
      if (seq === loadSeq) {
        loading = false;
      }
    }
  }

  async function loadCatalogUrl(
    targetUrl: string,
    page = 1,
    filters?: CatalogFilterQuery | null,
    forceRefresh = false
  ): Promise<CatalogPageView | null> {
    const activeF = filters ?? webFilterStore.getQuery(filterKeyFor(sourceId, targetUrl));
    if (sourceId !== undefined && !targetUrl.includes("/the-loai/") && !targetUrl.includes("/trang-")) {
      return loadFromSavedSource(sourceId, activeQuery, page, activeF, forceRefresh);
    }
    const seq = ++loadSeq;
    loading = true;
    errorState = null;
    failedCovers = {};
    try {
      const cat = await api.browseCatalogUrl(
        targetUrl,
        page,
        hasActiveFilters(activeF) ? activeF : null,
        forceRefresh
      );
      if (seq !== loadSeq) return null;
      catalog = cat;
      webFilterStore.setSchema(filterKeyFor(cat.sourceId, cat.url), cat.filterSchema);
      currentPage = cat.pagination.currentPage || page;
      searchQuery = cat.query ?? "";
      return cat;
    } catch (err) {
      if (seq !== loadSeq) return null;
      errorState = formatAppError(err, settingsStore.value.language);
      return null;
    } finally {
      if (seq === loadSeq) {
        loading = false;
      }
    }
  }

  $effect(() => {
    const target = url;
    const sid = sourceId;
    const preloaded = initialCatalog;
    const tag = effectiveInitialTag;
    if (tag) {
      const tagKey = `${sid ?? ""}|${target}|${tag.toLowerCase()}`;
      if (tagKey !== lastAppliedTagKey) {
        lastAppliedTagKey = tagKey;
        void loadWithInitialTag(target, sid, tag);
      }
      return;
    }
    lastAppliedTagKey = "";
    if (preloaded && (preloaded.url === target || sid !== undefined)) {
      catalog = preloaded;
      webFilterStore.setSchema(filterKeyFor(sid ?? preloaded.sourceId, preloaded.url), preloaded.filterSchema);
      searchQuery = preloaded.query ?? "";
      currentPage = preloaded.pagination.currentPage || 1;
      errorState = null;
      loading = false;
      failedCovers = {};
    } else if (sid !== undefined) {
      const savedFilters = webFilterStore.getQuery(filterKeyFor(sid, target));
      void loadFromSavedSource(sid, savedFilters.query ?? null, 1, savedFilters, false);
    } else if (target) {
      void loadCatalogUrl(target, 1, null, false);
    }
  });

  async function openEntry(entry: CatalogEntryView, triggerEl?: HTMLElement | null) {
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
      const f = formatAppError(err, settingsStore.value.language);
      toasts.push(f.detail || f.title, "error", 3600);
    } finally {
      openingEntryUrl = null;
    }
  }

  function submitSearch() {
    const q = searchQuery.trim();
    const nextFilters: CatalogFilterQuery = {
      ...activeFilters,
      query: q || null,
    };
    webFilterStore.setQuery(activeFilterKey, nextFilters);
    if (effectiveSourceId !== undefined) {
      void loadFromSavedSource(effectiveSourceId, q || null, 1, nextFilters, false);
      return;
    }
    if (q && catalog?.searchUrlTemplate && !hasActiveFilters({ ...nextFilters, query: null })) {
      const nextUrl = catalog.searchUrlTemplate.replace("{query}", encodeURIComponent(q));
      void loadCatalogUrl(nextUrl, 1, emptyFilterQuery(), false);
      return;
    }
    void loadCatalogUrl(catalog?.url ?? url, 1, nextFilters, false);
  }

  function handleApplyFilters(nextFilters: CatalogFilterQuery) {
    const withSearch: CatalogFilterQuery = {
      ...nextFilters,
      query: searchQuery.trim() || null,
    };
    webFilterStore.setQuery(activeFilterKey, withSearch);
    if (effectiveSourceId !== undefined) {
      void loadFromSavedSource(effectiveSourceId, withSearch.query ?? null, 1, withSearch, false);
    } else {
      void loadCatalogUrl(catalog?.url ?? url, 1, withSearch, false);
    }
  }

  function handleResetFilters() {
    webFilterStore.resetQuery(activeFilterKey);
    searchQuery = "";
    activeQuery = null;
    if (effectiveSourceId !== undefined) {
      void loadFromSavedSource(effectiveSourceId, null, 1, emptyFilterQuery(), false);
    } else {
      void loadCatalogUrl(url, 1, emptyFilterQuery(), false);
    }
  }

  function navigateCategory(nextUrl: string) {
    webFilterStore.resetQuery(activeFilterKey);
    void loadCatalogUrl(nextUrl, 1, emptyFilterQuery(), false);
  }

  function goToPage(targetPage: number) {
    const p = Math.max(1, Math.floor(targetPage));
    if (loading || p === currentPage) return;

    if (hasActiveFilters(activeFilters)) {
      if (effectiveSourceId !== undefined) {
        void loadFromSavedSource(effectiveSourceId, activeQuery, p, activeFilters, false);
      } else {
        void loadCatalogUrl(catalog?.url ?? url, p, activeFilters, false);
      }
      return;
    }

    const tmpl = catalog?.pagination.pageUrlTemplate;
    if (tmpl && tmpl.includes("{page}")) {
      const concreteUrl = tmpl.replace("{page}", String(p));
      void loadCatalogUrl(concreteUrl, p, emptyFilterQuery(), false);
      return;
    }

    if (p === currentPage - 1 && catalog?.pagination.prevUrl) {
      void loadCatalogUrl(catalog.pagination.prevUrl, p, emptyFilterQuery(), false);
      return;
    }
    if (p === currentPage + 1 && catalog?.pagination.nextUrl) {
      void loadCatalogUrl(catalog.pagination.nextUrl, p, emptyFilterQuery(), false);
      return;
    }

    if (effectiveSourceId !== undefined) {
      void loadFromSavedSource(effectiveSourceId, activeQuery, p, activeFilters, false);
    } else {
      void loadCatalogUrl(catalog?.url ?? url, p, activeFilters, false);
    }
  }

  function submitPageJump() {
    const parsed = Number.parseInt(jumpPageInput.trim(), 10);
    if (!Number.isFinite(parsed) || parsed < 1) return;
    jumpPageInput = "";
    goToPage(parsed);
  }

  const pageNumbers = $derived(() => {
    const cur = catalog?.pagination.currentPage ?? currentPage;
    const total = catalog?.pagination.totalPages ?? (catalog?.pagination.nextUrl ? cur + 1 : cur);
    const maxP = Math.max(cur, total);
    if (maxP <= 1) return [] as (number | "...")[];

    const out: (number | "...")[] = [];
    const addPage = (n: number) => {
      if (n >= 1 && n <= maxP && !out.includes(n)) out.push(n);
    };

    addPage(1);
    if (cur > 3) out.push("...");
    for (let i = Math.max(2, cur - 1); i <= Math.min(maxP - 1, cur + 1); i += 1) {
      addPage(i);
    }
    if (cur < maxP - 2) out.push("...");
    addPage(maxP);
    return out;
  });

  const canGoPrev = $derived(
    currentPage > 1 || Boolean(catalog?.pagination.prevUrl)
  );
  const canGoNext = $derived(
    Boolean(catalog?.pagination.nextUrl) ||
      (catalog?.pagination.totalPages !== null &&
        catalog?.pagination.totalPages !== undefined &&
        currentPage < catalog.pagination.totalPages) ||
      (effectiveSourceId !== undefined && (catalog?.entries.length ?? 0) > 0)
  );
  const canJumpArbitrary = $derived(
    Boolean(catalog?.pagination.pageUrlTemplate) ||
      effectiveSourceId !== undefined ||
      (catalog?.pagination.totalPages ?? 1) > 1
  );

  async function handleManualVerify(targetUrl?: string) {
    const target = targetUrl || errorState?.blockedUrl || url;
    if (!target || verifying) return;
    verifying = true;
    try {
      toasts.push(t.toastOpeningVerify, "info", 4500);
      await api.openManualVerification(target);
      toasts.push(t.toastVerifyDone, "success");
      await loadCatalogUrl(target, currentPage, activeFilters, true);
    } catch (err) {
      errorState = formatAppError(err, settingsStore.value.language);
    } finally {
      verifying = false;
    }
  }

  async function handleBrowserLogin(targetUrl: string) {
    const target = targetUrl || url;
    if (!target || loggingIn) return;
    loggingIn = true;
    try {
      toasts.push(t.errActionOpeningLogin, "info", 4500);
      const outcome = await api.openBrowserLoginSession(target);
      if (outcome.cookiesSynced) {
        toasts.push(t.toastLoginSynced, "success");
        await loadCatalogUrl(target, currentPage, activeFilters, true);
      } else {
        toasts.push(t.toastLoginExternalOpened, "info", 4500);
      }
    } catch (err) {
      errorState = formatAppError(err, settingsStore.value.language);
    } finally {
      loggingIn = false;
    }
  }

  async function handleOpenExternal(targetUrl: string) {
    const target = targetUrl || errorState?.externalUrl || url;
    if (!target) return;
    try {
      await api.openExternalUrl(target);
    } catch (err) {
      toasts.push(formatAppError(err, settingsStore.value.language).detail, "error");
    }
  }
</script>

<div class="mx-auto flex w-full max-w-6xl flex-col gap-5 px-4 py-5 sm:px-8 sm:py-7">
  <!-- Top Bar -->
  <div class="flex flex-wrap items-center justify-between gap-2">
    <button
      type="button"
      use:pressable
      onclick={() => {
        if (!router.goBack()) router.goHome();
      }}
      class="btn-3d btn-3d-subtle inline-flex items-center gap-1.5 rounded-xl px-3 py-1.5 text-sm font-medium"
      style="color: var(--text-muted);"
    >
      <ArrowLeft size={16} />
      <span>{t.back}</span>
    </button>

    <div class="flex flex-wrap items-center gap-2">
      {#if catalog}
        <span
          class="inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium"
          style="background-color: var(--surface); border-color: var(--border);"
        >
          <Globe size={13} style="color: var(--accent);" />
          <span>{catalog.siteName}</span>
          <span class="opacity-60">· {t.browseResultsCount(catalog.entries.length)}</span>
        </span>
      {/if}
      <button
        type="button"
        use:pressable
        disabled={loading}
        onclick={() => void loadCatalogUrl(catalog?.url ?? url, currentPage, activeFilters, true)}
        class="btn-3d btn-3d-surface inline-flex items-center gap-1.5 rounded-xl px-3 py-1.5 text-xs font-medium"
      >
        <RefreshCw size={13} class={loading ? "animate-spin" : ""} />
        <span>{t.retry}</span>
      </button>
    </div>
  </div>

  <!-- Header + Site Search + Categories -->
  <section
    class="flex flex-col gap-3 rounded-2xl border p-4 sm:p-5"
    style="background-color: var(--surface); border-color: var(--border);"
  >
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div class="min-w-0 flex-1">
        <h1 class="truncate text-lg font-bold tracking-tight sm:text-xl">
          {catalog?.title ?? t.browseTitle}
        </h1>
        <div class="mt-0.5 truncate text-xs" style="color: var(--text-muted);">
          {catalog?.url ?? url}
        </div>
      </div>

      {#if effectiveSourceId !== undefined || catalog?.searchUrlTemplate}
        <form
          onsubmit={(e) => {
            e.preventDefault();
            submitSearch();
          }}
          class="flex w-full items-center gap-1.5 rounded-xl border px-2.5 py-1.5 sm:w-80"
          style="background-color: var(--surface-subtle); border-color: var(--border);"
        >
          <Search size={14} class="shrink-0" style="color: var(--text-muted);" />
          <input
            type="text"
            bind:value={searchQuery}
            placeholder={t.browseSearchPlaceholder}
            class="min-w-0 flex-1 bg-transparent text-xs outline-none"
          />
          <button
            type="submit"
            use:pressable
            disabled={(effectiveSourceId === undefined && !searchQuery.trim()) || loading}
            class="btn-3d btn-3d-accent shrink-0 rounded-lg px-2.5 py-1 text-xs font-semibold disabled:opacity-45"
          >
            {t.browseSearchBtn}
          </button>
        </form>
      {/if}
    </div>

    {#if catalog && catalog.categories.length > 0}
      <div class="flex flex-wrap items-center gap-1.5 pt-1">
        <span class="mr-1 text-[11px] font-medium" style="color: var(--text-muted);">
          {t.browseCategoriesLabel}:
        </span>
        {#each catalog.categories.slice(0, 28) as cat (cat.url)}
          <button
            type="button"
            use:pressable
            onclick={() => navigateCategory(cat.url)}
            class="pill-3d rounded-lg border px-2.5 py-1 text-xs font-medium"
            style="background-color: {cat.url === catalog.url
              ? 'var(--accent-soft)'
              : 'var(--surface-subtle)'}; border-color: {cat.url === catalog.url
              ? 'var(--accent)'
              : 'var(--border)'}; color: {cat.url === catalog.url ? 'var(--accent)' : 'var(--text)'};"
          >
            {cat.label}
          </button>
        {/each}
      </div>
    {/if}
  </section>

  <!-- Synchronized Per-Website Multi-Tag, Min Chapters, Update Recency & Dynamic Filter Bar -->
  <WebFilterBar
    schema={activeSchema}
    value={activeFilters}
    applying={loading}
    defaultExpanded={hasActiveFilters(activeFilters)}
    siteLabel={catalog?.siteName ?? ""}
    onApply={handleApplyFilters}
    onReset={handleResetFilters}
  />

  <!-- Body -->
  {#if loading}
    <div class="fluid-manga-grid">
      {#each Array(10) as _, i (i)}
        <div
          class="aspect-[3/4] animate-pulse rounded-2xl"
          style="background-color: var(--surface-hover);"
        ></div>
      {/each}
    </div>
  {:else if errorState}
    <SourceDiagnosticCard
      error={errorState}
      fallbackUrl={url}
      {verifying}
      {loggingIn}
      onRetry={() => void loadCatalogUrl(catalog?.url ?? url, currentPage, activeFilters, true)}
      onVerify={(u) => void handleManualVerify(u)}
      onLogin={(u) => void handleBrowserLogin(u)}
      onOpenExternal={(u) => void handleOpenExternal(u)}
      onOpenNetworkSettings={() => router.goSettings("network")}
    />
  {:else if !catalog || catalog.entries.length === 0}
    <div
      in:viewReveal|local={{ duration: 165, y: 6 }}
      class="flex flex-col items-center justify-center gap-3 rounded-2xl border border-dashed py-16 text-center"
      style="border-color: var(--border-strong); color: var(--text-muted);"
    >
      <BookOpen size={30} strokeWidth={1.5} />
      <div class="text-sm font-medium" style="color: var(--text);">{t.browseEmptyTitle}</div>
    </div>
  {:else}
    {#key `${catalog.url}:${currentPage}`}
      <div class="fluid-manga-grid">
        {#each catalog.entries as entry, idx (entry.url)}
          {@const isOpening = openingEntryUrl === entry.url}
          {@const animateStagger = shouldStaggerItem(idx, catalog.entries.length)}
          <button
            type="button"
            use:pressable
            disabled={openingEntryUrl !== null}
            onclick={(e) => void openEntry(entry, e.currentTarget)}
            class="card-cq card-contain card-press surface-card group flex flex-col overflow-hidden rounded-2xl text-left disabled:opacity-70"
            class:stagger-item={animateStagger}
            style={animateStagger ? `--stagger-delay: ${staggerDelay(idx, 16, 180, catalog.entries.length)}ms;` : undefined}
          >
            <div
              data-hero-cover-src
              class="relative aspect-[3/4] w-full overflow-hidden"
              style="background-color: var(--surface-subtle);"
            >
              {#if entry.coverToken && !failedCovers[entry.url]}
                <img
                  src={imgProxyUrl(entry.coverToken)}
                  alt={entry.title}
                  loading="lazy"
                  decoding="async"
                  onerror={() => {
                    failedCovers = { ...failedCovers, [entry.url]: true };
                  }}
                  class="h-full w-full object-cover transition-transform duration-200 ease-out group-hover:scale-[1.03]"
                />
              {:else}
                <div
                  class="flex h-full w-full items-center justify-center"
                  style="color: var(--text-muted);"
                >
                  <BookOpen size={30} strokeWidth={1.5} />
                </div>
              {/if}

              {#if entry.updatedAt}
                <span
                  class="absolute top-2 left-2 max-w-[60%] truncate rounded-lg px-1.5 py-0.5 text-[10px] font-medium text-white backdrop-blur-md"
                  style="background-color: rgba(11, 11, 15, 0.74);"
                >
                  {entry.updatedAt}
                </span>
              {/if}

              {#if entry.latestChapter}
                <span
                  class="absolute bottom-2 left-2 max-w-[calc(100%-1rem)] truncate rounded-lg px-2 py-0.5 text-[10px] font-semibold text-white backdrop-blur-md"
                  style="background-color: rgba(11, 11, 15, 0.8);"
                >
                  {entry.latestChapter}
                </span>
              {/if}

              {#if entry.status}
                <span
                  class="absolute top-2 right-2 rounded-lg px-1.5 py-0.5 text-[10px] font-semibold uppercase text-white backdrop-blur-md"
                  style="background-color: rgba(11, 11, 15, 0.72);"
                >
                  {entry.status}
                </span>
              {/if}

              {#if isOpening}
                <div
                  transition:backdropFade|local={{ duration: 140 }}
                  class="absolute inset-0 flex flex-col items-center justify-center gap-1.5 bg-black/55 text-xs font-semibold text-white"
                >
                  <LatticeLoader size="md" tone="inherit" label={t.browseOpeningManga} showTimer />
                </div>
              {/if}
            </div>

            <div class="card-body flex flex-1 flex-col gap-1 p-2.5">
              <div class="card-title line-clamp-2 font-semibold">
                {entry.title}
              </div>
              {#if entry.genres.length > 0}
                <div class="mt-auto flex flex-wrap gap-1 pt-1">
                  {#each entry.genres.slice(0, 2) as genre (genre)}
                    <span
                      class="rounded-md px-1.5 py-0.5 text-[10px]"
                      style="background-color: var(--surface-subtle); color: var(--text-muted);"
                    >
                      {genre}
                    </span>
                  {/each}
                </div>
              {/if}
            </div>
          </button>
        {/each}
      </div>
    {/key}

    <!-- Full Multi-Page Pagination + Direct Page Jump -->
    {#if canGoPrev || canGoNext || pageNumbers().length > 0}
      <div class="flex flex-wrap items-center justify-center gap-2 pt-2">
        <button
          type="button"
          use:pressable
          disabled={!canGoPrev || loading}
          onclick={() => goToPage(currentPage - 1)}
          class="btn-3d btn-3d-surface inline-flex items-center gap-1 rounded-xl px-3 py-2 text-xs font-medium disabled:opacity-40"
        >
          <ChevronLeft size={14} />
          <span>{t.browsePrevPage}</span>
        </button>

        {#if pageNumbers().length > 0}
          <div class="flex flex-wrap items-center justify-center gap-1">
            {#each pageNumbers() as item, idx (String(item) + "-" + idx)}
              {#if item === "..."}
                <span class="px-1.5 text-xs opacity-60">…</span>
              {:else}
                {@const active = item === currentPage}
                <button
                  type="button"
                  use:pressable
                  disabled={loading}
                  onclick={() => goToPage(item)}
                  class="btn-3d min-w-8 rounded-lg px-2.5 py-1.5 text-xs font-semibold tabular-nums {active
                    ? 'btn-3d-accent'
                    : 'btn-3d-surface'}"
                >
                  {item}
                </button>
              {/if}
            {/each}
          </div>
        {:else}
          <span class="px-2 text-xs font-semibold tabular-nums" style="color: var(--text-muted);">
            {t.browsePageIndicator(catalog.pagination.currentPage, catalog.pagination.totalPages)}
          </span>
        {/if}

        <button
          type="button"
          use:pressable
          disabled={!canGoNext || loading}
          onclick={() => goToPage(currentPage + 1)}
          class="btn-3d btn-3d-surface inline-flex items-center gap-1 rounded-xl px-3 py-2 text-xs font-medium disabled:opacity-40"
        >
          <span>{t.browseNextPage}</span>
          <ChevronRight size={14} />
        </button>

        {#if canJumpArbitrary}
          <form
            onsubmit={(e) => {
              e.preventDefault();
              submitPageJump();
            }}
            class="ml-1 flex items-center gap-1 rounded-xl border px-2 py-1 text-xs"
            style="background-color: var(--surface); border-color: var(--border);"
          >
            <input
              type="number"
              min="1"
              max={catalog.pagination.totalPages ?? 9999}
              bind:value={jumpPageInput}
              placeholder={t.browseJumpPagePlaceholder}
              class="w-16 bg-transparent py-0.5 text-xs outline-none tabular-nums"
              style="color: var(--text);"
            />
            <button
              type="submit"
              use:pressable
              disabled={loading || !jumpPageInput.trim()}
              class="btn-3d btn-3d-accent rounded-lg px-2 py-1 text-[11px] font-semibold disabled:opacity-45"
            >
              {t.browseJumpGoBtn}
            </button>
          </form>
        {/if}
      </div>
    {/if}
  {/if}
</div>
