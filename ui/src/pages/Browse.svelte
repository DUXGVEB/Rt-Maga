<script lang="ts">
  import {
    AlertCircle,
    ArrowLeft,
    BookOpen,
    ChevronLeft,
    ChevronRight,
    ExternalLink,
    Globe,
    Loader2,
    RefreshCw,
    Search,
  } from "lucide-svelte";
  import { api, formatAppError, imgProxyUrl } from "../lib/api";
  import { router } from "../lib/stores/router.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import type { CatalogEntryView, CatalogPageView } from "../lib/types";

  let {
    url,
    initialCatalog,
  }: {
    url: string;
    initialCatalog?: CatalogPageView;
  } = $props();

  let catalog = $state<CatalogPageView | null>(null);
  let loading = $state(false);
  let openingEntryUrl = $state<string | null>(null);
  let searchQuery = $state("");
  let failedCovers = $state<Record<string, boolean>>({});
  let errorState = $state<{ title: string; detail: string; blockedUrl?: string } | null>(null);
  let verifying = $state(false);
  let loadSeq = 0;

  const t = $derived(settingsStore.t);

  async function loadCatalogUrl(targetUrl: string) {
    const seq = ++loadSeq;
    loading = true;
    errorState = null;
    failedCovers = {};
    try {
      const res = await api.resolveUrl(targetUrl);
      if (seq !== loadSeq) return;
      if (res.kind === "catalog" && res.catalog) {
        catalog = res.catalog;
        searchQuery = res.catalog.query ?? "";
      } else if (res.manga) {
        if (res.startAt) {
          router.goReader(res.manga.id, res.startAt.url);
        } else {
          router.goManga(res.manga.id);
        }
      }
    } catch (err) {
      if (seq !== loadSeq) return;
      errorState = formatAppError(err, settingsStore.value.language);
    } finally {
      if (seq === loadSeq) {
        loading = false;
      }
    }
  }

  $effect(() => {
    const target = url;
    const preloaded = initialCatalog;
    if (preloaded && preloaded.url === target) {
      catalog = preloaded;
      searchQuery = preloaded.query ?? "";
      errorState = null;
      loading = false;
      failedCovers = {};
    } else if (target) {
      void loadCatalogUrl(target);
    }
  });

  async function openEntry(entry: CatalogEntryView) {
    if (openingEntryUrl) return;
    openingEntryUrl = entry.url;
    try {
      const res = await api.resolveUrl(entry.url);
      if (res.kind === "catalog" && res.catalog) {
        router.goBrowse(res.catalog.url, res.catalog);
      } else if (res.manga) {
        if (res.startAt) {
          router.goReader(res.manga.id, res.startAt.url);
        } else {
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
    if (!q || !catalog?.searchUrlTemplate) return;
    const nextUrl = catalog.searchUrlTemplate.replace("{query}", encodeURIComponent(q));
    router.goBrowse(nextUrl, undefined, false);
  }

  function navigateCatalog(nextUrl: string) {
    router.goBrowse(nextUrl, undefined, false);
  }

  async function handleManualVerify() {
    if (!errorState?.blockedUrl || verifying) return;
    verifying = true;
    try {
      toasts.push(t.toastOpeningVerify, "info", 4500);
      await api.openManualVerification(errorState.blockedUrl);
      toasts.push(t.toastVerifyDone, "success");
      await loadCatalogUrl(errorState.blockedUrl);
    } catch (err) {
      errorState = formatAppError(err, settingsStore.value.language);
    } finally {
      verifying = false;
    }
  }
</script>

<div class="mx-auto flex w-full max-w-6xl flex-col gap-5 px-4 py-5 sm:px-8 sm:py-7">
  <!-- Top Bar -->
  <div class="flex flex-wrap items-center justify-between gap-2">
    <button
      type="button"
      onclick={() => {
        if (!router.goBack()) router.goHome();
      }}
      class="inline-flex items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-sm transition-colors hover:opacity-80"
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
        disabled={loading}
        onclick={() => void loadCatalogUrl(catalog?.url ?? url)}
        class="inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1.5 text-xs font-medium"
        style="background-color: var(--surface); border-color: var(--border);"
      >
        <RefreshCw size={13} class={loading ? "animate-spin" : ""} />
        <span>{t.retry}</span>
      </button>
    </div>
  </div>

  <!-- Header + Site Search -->
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

      {#if catalog?.searchUrlTemplate}
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
            disabled={!searchQuery.trim() || loading}
            class="shrink-0 rounded-lg px-2.5 py-1 text-xs font-semibold text-white disabled:opacity-45"
            style="background-color: var(--accent);"
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
        {#each catalog.categories as cat (cat.url)}
          <button
            type="button"
            onclick={() => navigateCatalog(cat.url)}
            class="rounded-lg border px-2.5 py-1 text-xs font-medium transition-colors hover:brightness-95"
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

  <!-- Body -->
  {#if loading}
    <div class="grid grid-cols-2 gap-3.5 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5">
      {#each Array(10) as _, i (i)}
        <div
          class="aspect-[3/4] rounded-xl"
          style="background-color: var(--surface-hover);"
        ></div>
      {/each}
    </div>
  {:else if errorState}
    <div
      class="flex flex-col items-center gap-3 rounded-2xl border p-6 text-center"
      style="background-color: var(--surface); border-color: var(--danger);"
    >
      <AlertCircle size={26} style="color: var(--danger);" />
      <div class="text-sm font-semibold">{errorState.title}</div>
      <p class="max-w-lg text-xs leading-relaxed" style="color: var(--text-muted);">
        {errorState.detail}
      </p>
      <div class="flex flex-wrap justify-center gap-2 pt-1">
        {#if errorState.blockedUrl}
          <button
            type="button"
            disabled={verifying}
            onclick={handleManualVerify}
            class="inline-flex items-center gap-1.5 rounded-xl px-4 py-2 text-xs font-semibold text-white"
            style="background-color: var(--accent);"
          >
            <ExternalLink size={14} />
            <span>{verifying ? t.waitingVerification : t.openVerification}</span>
          </button>
        {/if}
        <button
          type="button"
          onclick={() => void loadCatalogUrl(url)}
          class="rounded-xl border px-4 py-2 text-xs font-medium"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          {t.retry}
        </button>
      </div>
    </div>
  {:else if !catalog || catalog.entries.length === 0}
    <div
      class="flex flex-col items-center justify-center gap-3 rounded-2xl border border-dashed py-16 text-center"
      style="border-color: var(--border); color: var(--text-muted);"
    >
      <BookOpen size={30} strokeWidth={1.5} />
      <div class="text-sm font-medium" style="color: var(--text);">{t.browseEmptyTitle}</div>
      <p class="max-w-sm text-xs">{t.browseEmptyDesc}</p>
    </div>
  {:else}
    <div class="grid grid-cols-2 gap-3.5 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5">
      {#each catalog.entries as entry (entry.url)}
        {@const isOpening = openingEntryUrl === entry.url}
        <button
          type="button"
          disabled={openingEntryUrl !== null}
          onclick={() => void openEntry(entry)}
          class="group flex flex-col overflow-hidden rounded-xl border text-left transition-transform duration-150 hover:-translate-y-0.5 disabled:opacity-70"
          style="background-color: var(--surface); border-color: var(--border); box-shadow: var(--shadow-sm);"
        >
          <div
            class="relative aspect-[3/4] w-full overflow-hidden"
            style="background-color: var(--surface-subtle);"
          >
            {#if entry.coverToken && !failedCovers[entry.url]}
              <img
                src={imgProxyUrl(entry.coverToken)}
                alt={entry.title}
                loading="lazy"
                onerror={() => {
                  failedCovers = { ...failedCovers, [entry.url]: true };
                }}
                class="h-full w-full object-cover transition-transform duration-200 group-hover:scale-[1.03]"
              />
            {:else}
              <div
                class="flex h-full w-full items-center justify-center"
                style="color: var(--text-muted);"
              >
                <BookOpen size={30} strokeWidth={1.5} />
              </div>
            {/if}

            {#if entry.latestChapter}
              <span
                class="absolute bottom-2 left-2 max-w-[calc(100%-1rem)] truncate rounded-md px-2 py-0.5 text-[10px] font-semibold text-white"
                style="background-color: rgba(15, 15, 18, 0.8);"
              >
                {entry.latestChapter}
              </span>
            {/if}

            {#if entry.status}
              <span
                class="absolute top-2 right-2 rounded-md px-1.5 py-0.5 text-[10px] font-semibold uppercase text-white"
                style="background-color: rgba(15, 15, 18, 0.72);"
              >
                {entry.status}
              </span>
            {/if}

            {#if isOpening}
              <div
                class="absolute inset-0 flex flex-col items-center justify-center gap-1.5 bg-black/55 text-xs font-semibold text-white"
              >
                <Loader2 size={22} class="animate-spin" />
                <span>{t.browseOpeningManga}</span>
              </div>
            {/if}
          </div>

          <div class="flex flex-1 flex-col gap-1 p-2.5">
            <div class="line-clamp-2 text-xs font-semibold leading-snug sm:text-sm">
              {entry.title}
            </div>
            {#if entry.genres.length > 0}
              <div class="mt-auto flex flex-wrap gap-1 pt-1">
                {#each entry.genres.slice(0, 2) as genre (genre)}
                  <span
                    class="rounded px-1.5 py-0.5 text-[10px]"
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

    <!-- Pagination -->
    {#if catalog.pagination.prevUrl || catalog.pagination.nextUrl || (catalog.pagination.totalPages ?? 1) > 1}
      <div class="flex items-center justify-center gap-3 pt-2">
        <button
          type="button"
          disabled={!catalog.pagination.prevUrl || loading}
          onclick={() => catalog?.pagination.prevUrl && navigateCatalog(catalog.pagination.prevUrl)}
          class="inline-flex items-center gap-1.5 rounded-xl border px-4 py-2 text-xs font-medium disabled:opacity-40"
          style="background-color: var(--surface); border-color: var(--border);"
        >
          <ChevronLeft size={14} />
          <span>{t.browsePrevPage}</span>
        </button>

        <span class="text-xs font-semibold tabular-nums" style="color: var(--text-muted);">
          {t.browsePageIndicator(catalog.pagination.currentPage, catalog.pagination.totalPages)}
        </span>

        <button
          type="button"
          disabled={!catalog.pagination.nextUrl || loading}
          onclick={() => catalog?.pagination.nextUrl && navigateCatalog(catalog.pagination.nextUrl)}
          class="inline-flex items-center gap-1.5 rounded-xl border px-4 py-2 text-xs font-medium disabled:opacity-40"
          style="background-color: var(--surface); border-color: var(--border);"
        >
          <span>{t.browseNextPage}</span>
          <ChevronRight size={14} />
        </button>
      </div>
    {/if}
  {/if}
</div>
