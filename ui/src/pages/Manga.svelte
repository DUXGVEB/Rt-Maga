<script lang="ts">
  import {
    AlertTriangle,
    ArrowDownUp,
    ArrowLeft,
    Bookmark,
    BookOpen,
    Check,
    CheckCircle2,
    CornerDownLeft,
    EyeOff,
    Play,
    RefreshCw,
    Search,
    Trash2,
  } from "lucide-svelte";
  import { api, formatAppError, imgProxyUrl } from "../lib/api";
  import {
    isLocalComicSite,
    isMetadataOnlyChapterUrl,
    isTachibkCatalogSite,
  } from "../lib/comicFormats";
  import CbzDropZone from "../lib/components/CbzDropZone.svelte";
  import LatticeLoader from "../lib/components/LatticeLoader.svelte";
  import SourceDiagnosticCard from "../lib/components/SourceDiagnosticCard.svelte";
  import { backdropFade, createOverlayGuard, modalScale } from "../lib/motion";
  import { notificationsStore } from "../lib/stores/notifications.svelte";
  import { router } from "../lib/stores/router.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import type { FormattedAppError, MangaView } from "../lib/types";

  let { id }: { id: number } = $props();

  let manga = $state<MangaView | null>(null);
  let loading = $state(true);
  let refreshing = $state(false);
  let verifying = $state(false);
  let loggingIn = $state(false);
  let hiding = $state(false);
  let deleting = $state(false);
  let showDeleteModal = $state(false);
  let errorState = $state<FormattedAppError | null>(null);
  let refreshErrorState = $state<FormattedAppError | null>(null);
  let coverError = $state(false);
  let query = $state("");
  let jumpInput = $state("");
  let newestFirst = $state(true);
  let selectedChapterLang = $state<string>("auto");
  let descExpanded = $state(false);
  let tagsExpanded = $state(false);
  let loadSeq = 0;
  const deleteModalGuard = createOverlayGuard(200);

  function openDeleteModal() {
    if (!deleteModalGuard.canAct()) return;
    deleteModalGuard.lock();
    showDeleteModal = true;
  }

  function closeDeleteModal() {
    if (deleting || !deleteModalGuard.canAct()) return;
    deleteModalGuard.lock();
    showDeleteModal = false;
  }

  const t = $derived(settingsStore.t);

  const combinedTags = $derived(() => {
    if (!manga) return [] as { label: string; isGenre: boolean }[];
    const seen = new Set<string>();
    const out: { label: string; isGenre: boolean }[] = [];
    for (const g of manga.genres) {
      const clean = g.trim();
      const key = clean.toLowerCase();
      if (clean && !seen.has(key)) {
        seen.add(key);
        out.push({ label: clean, isGenre: true });
      }
    }
    for (const tag of manga.tags) {
      const clean = tag.trim();
      const key = clean.toLowerCase();
      if (clean && !seen.has(key)) {
        seen.add(key);
        out.push({ label: clean, isGenre: false });
      }
    }
    return out;
  });

  async function load(targetId: number) {
    const seq = ++loadSeq;
    loading = true;
    errorState = null;
    refreshErrorState = null;
    coverError = false;
    showDeleteModal = false;
    descExpanded = false;
    tagsExpanded = false;
    try {
      const loaded = await api.getManga(targetId);
      if (seq !== loadSeq) return;
      manga = loaded;
    } catch (err) {
      if (seq !== loadSeq) return;
      errorState = formatAppError(err, settingsStore.value.language);
    } finally {
      if (seq === loadSeq) {
        loading = false;
      }
    }
  }

  async function refresh() {
    if (refreshing) return;
    const targetId = id;
    const seq = ++loadSeq;
    refreshing = true;
    refreshErrorState = null;
    try {
      const updated = await api.refreshManga(targetId);
      if (seq !== loadSeq) return;
      manga = updated;
      coverError = false;
      toasts.push(t.toastChaptersUpdated, "success");
    } catch (err) {
      if (seq === loadSeq) {
        const f = formatAppError(err, settingsStore.value.language);
        refreshErrorState = f;
        toasts.push(f.detail || f.title, "error");
      }
    } finally {
      refreshing = false;
    }
  }

  async function handleManualVerify(targetUrl: string) {
    if (!targetUrl || verifying) return;
    verifying = true;
    try {
      toasts.push(t.toastOpeningVerify, "info", 4500);
      await api.openManualVerification(targetUrl);
      toasts.push(t.toastVerifyDone, "success");
      refreshErrorState = null;
      if (manga) {
        await refresh();
      } else {
        await load(id);
      }
    } catch (err) {
      const f = formatAppError(err, settingsStore.value.language);
      if (manga) refreshErrorState = f;
      else errorState = f;
      toasts.push(f.detail || f.title, "error");
    } finally {
      verifying = false;
    }
  }

  async function handleBrowserLogin(targetUrl: string) {
    if (!targetUrl || loggingIn) return;
    loggingIn = true;
    try {
      toasts.push(t.errActionOpeningLogin, "info", 4500);
      const outcome = await api.openBrowserLoginSession(targetUrl);
      if (outcome.cookiesSynced) {
        toasts.push(t.toastLoginSynced, "success");
        refreshErrorState = null;
        if (manga) {
          await refresh();
        } else {
          await load(id);
        }
      } else {
        toasts.push(t.toastLoginExternalOpened, "info", 4500);
      }
    } catch (err) {
      const f = formatAppError(err, settingsStore.value.language);
      if (manga) refreshErrorState = f;
      else errorState = f;
      toasts.push(f.detail || f.title, "error");
    } finally {
      loggingIn = false;
    }
  }

  async function handleOpenExternal(targetUrl: string) {
    if (!targetUrl) return;
    try {
      await api.openExternalUrl(targetUrl);
    } catch (err) {
      toasts.push(formatAppError(err, settingsStore.value.language).detail, "error");
    }
  }

  $effect(() => {
    void load(id);
  });

  const chapterLanguages = $derived(() => {
    if (!manga) return [];
    const set = new Set<string>();
    for (const c of manga.chapters) {
      if (c.language && c.language.trim()) {
        set.add(c.language.trim().toLowerCase());
      }
    }
    return Array.from(set).sort();
  });

  const activeChapterLang = $derived(() => {
    const langs = chapterLanguages();
    if (langs.length <= 1) return "all";
    if (selectedChapterLang !== "auto") return selectedChapterLang;
    const uiLang = settingsStore.value.language.toLowerCase();
    if (langs.includes(uiLang)) return uiLang;
    if (langs.includes("en")) return "en";
    return "all";
  });

  const filteredChapters = $derived(() => {
    if (!manga) return [];
    const lang = activeChapterLang();
    const langFiltered =
      lang === "all"
        ? manga.chapters
        : manga.chapters.filter((c) => (c.language ?? "").toLowerCase() === lang);
    const q = query.trim().toLowerCase();
    const list = q
      ? langFiltered.filter(
          (c) =>
            c.name.toLowerCase().includes(q) ||
            (c.number !== null && String(c.number).includes(q)) ||
            (c.volume !== null && c.volume.toLowerCase().includes(q))
        )
      : langFiltered;
    return newestFirst ? [...list].reverse() : list;
  });

  function normUrl(u: string): string {
    return u.endsWith("/") ? u.slice(0, -1) : u;
  }

  function startPrimary() {
    if (!manga || manga.chapters.length === 0) return;
    if (manga.progress) {
      if (isMetadataOnlyChapterUrl(manga.progress.chapterUrl)) {
        toasts.push(t.tachibkCatalogChapterNoFilesToast, "info", 3200);
        return;
      }
      router.goReader(
        manga.id,
        manga.progress.chapterUrl,
        manga.progress.scrollRatio,
        manga.progress.pageIndex
      );
    } else {
      const firstReadable = manga.chapters.find((c) => !isMetadataOnlyChapterUrl(c.url));
      if (!firstReadable) {
        toasts.push(t.tachibkCatalogChapterNoFilesToast, "info", 3200);
        return;
      }
      router.goReader(manga.id, firstReadable.url, 0, 0);
    }
  }

  function handleQuickJump() {
    if (!manga || manga.chapters.length === 0) return;
    const raw = jumpInput.trim();
    if (!raw) return;
    const parsedNum = Number(raw);
    const target =
      Number.isFinite(parsedNum)
        ? manga.chapters.find((c) => c.number === parsedNum) ??
          manga.chapters.find(
            (c) => c.number !== null && Math.floor(c.number) === Math.floor(parsedNum)
          ) ??
          manga.chapters.find((c) => c.name.toLowerCase().includes(raw.toLowerCase()))
        : manga.chapters.find((c) => c.name.toLowerCase().includes(raw.toLowerCase()));

    if (!target) {
      toasts.push(t.quickJumpNotFound(raw), "info", 2200);
      return;
    }
    jumpInput = "";
    if (isMetadataOnlyChapterUrl(target.url)) {
      toasts.push(t.tachibkCatalogChapterNoFilesToast, "info", 3200);
      return;
    }
    const isCurrent =
      manga.progress !== null && normUrl(manga.progress.chapterUrl) === normUrl(target.url);
    router.goReader(
      manga.id,
      target.url,
      isCurrent ? manga.progress?.scrollRatio ?? 0 : 0,
      isCurrent ? manga.progress?.pageIndex ?? 0 : 0
    );
  }

  async function toggleBookmark() {
    if (!manga) return;
    try {
      const next = await api.toggleBookmark(manga.id);
      manga = { ...manga, bookmarked: next };
      toasts.push(next ? t.toastBookmarkAdded : t.toastBookmarkRemoved, "success", 1800);
    } catch {
      toasts.push(t.toastHistoryUpdateFailed, "error");
    }
  }

  async function hideManga() {
    if (!manga || hiding) return;
    hiding = true;
    try {
      await api.setMangaHidden(manga.id, "hide");
      void notificationsStore.refresh();
      toasts.push(t.toastMangaHidden, "info", 2800);
      router.goHome();
    } catch (err) {
      const f = formatAppError(err, settingsStore.value.language);
      toasts.push(f.detail || f.title, "error");
    } finally {
      hiding = false;
    }
  }

  async function confirmDeleteManga() {
    if (!manga || deleting) return;
    deleting = true;
    try {
      await api.deleteMangaPermanently(manga.id);
      void notificationsStore.refresh();
      showDeleteModal = false;
      toasts.push(t.toastMangaDeleted, "info", 2800);
      router.goHome();
    } catch (err) {
      const f = formatAppError(err, settingsStore.value.language);
      toasts.push(f.detail || f.title, "error");
    } finally {
      deleting = false;
    }
  }
</script>

<div class="mx-auto flex w-full max-w-5xl flex-col gap-6 px-5 py-6 sm:px-8 sm:py-8">
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

    {#if manga}
      <div class="flex flex-wrap items-center gap-2">
        {#if isLocalComicSite(manga.site)}
          <CbzDropZone
            compact
            targetMangaId={manga.id}
            onImported={(updated) => {
              manga = updated;
              coverError = false;
            }}
          />
        {:else}
          <button
            type="button"
            onclick={refresh}
            disabled={refreshing}
            class="inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium transition-colors"
            style="background-color: var(--surface); border-color: var(--border);"
          >
            <RefreshCw size={14} class={refreshing ? "animate-spin" : ""} />
            <span>{refreshing ? t.refreshing : t.refreshChapters}</span>
          </button>
        {/if}
        <button
          type="button"
          onclick={hideManga}
          disabled={hiding}
          class="inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium transition-colors hover:opacity-85 disabled:opacity-50"
          style="background-color: var(--surface); border-color: var(--border); color: var(--text-muted);"
          title={t.hideMangaDesc}
        >
          <EyeOff size={14} />
          <span>{t.hideMangaBtn}</span>
        </button>
        <button
          type="button"
          onclick={openDeleteModal}
          class="gpu-press inline-flex items-center gap-1.5 rounded-xl border px-3 py-1.5 text-xs font-medium hover:opacity-85"
          style="background-color: var(--surface); border-color: var(--border); color: var(--danger);"
        >
          <Trash2 size={14} />
          <span>{t.deleteMangaBtn}</span>
        </button>
      </div>
    {/if}
  </div>

  {#if loading}
    <div class="flex gap-6">
      <div
        class="h-60 w-44 shrink-0 animate-pulse rounded-xl"
        style="background-color: var(--surface-hover);"
      ></div>
      <div class="flex flex-1 flex-col gap-3 pt-2">
        <div class="h-7 w-2/3 animate-pulse rounded" style="background-color: var(--surface-hover);"></div>
        <div class="h-4 w-1/3 animate-pulse rounded" style="background-color: var(--surface-hover);"></div>
      </div>
    </div>
  {:else if errorState}
    <SourceDiagnosticCard
      error={errorState}
      fallbackUrl={manga?.url}
      {verifying}
      {loggingIn}
      onRetry={() => void load(id)}
      onVerify={(u) => void handleManualVerify(u)}
      onLogin={(u) => void handleBrowserLogin(u)}
      onOpenExternal={(u) => void handleOpenExternal(u)}
      onOpenNetworkSettings={() => router.goSettings("network")}
    />
  {:else if manga}
    {#if refreshErrorState}
      <SourceDiagnosticCard
        compact
        error={refreshErrorState}
        fallbackUrl={manga.url}
        {verifying}
        {loggingIn}
        onRetry={() => void refresh()}
        onVerify={(u) => void handleManualVerify(u)}
        onLogin={(u) => void handleBrowserLogin(u)}
        onOpenExternal={(u) => void handleOpenExternal(u)}
        onOpenNetworkSettings={() => router.goSettings("network")}
      />
    {/if}
    <!-- Editorial Mobile-First & Desktop Series Hero -->
    <section
      class="surface-card relative flex flex-col gap-5 overflow-hidden rounded-2xl p-4 sm:flex-row sm:gap-6 sm:p-6"
    >
      {#if manga.coverToken && !coverError}
        <img
          src={imgProxyUrl(manga.coverToken)}
          alt=""
          aria-hidden="true"
          class="pointer-events-none absolute inset-0 h-full w-full scale-110 object-cover opacity-[0.07] blur-2xl"
        />
      {/if}

      <!-- Mobile compact top strip (cover + primary headline side-by-side); stacks as left column on sm+ -->
      <div class="relative z-10 flex items-start gap-4 sm:block sm:shrink-0">
        <div
          class="aspect-[3/4] w-28 shrink-0 overflow-hidden rounded-xl border shadow-md sm:h-60 sm:w-44"
          style="background-color: var(--surface-subtle); border-color: var(--border);"
        >
          {#if manga.coverToken && !coverError}
            <img
              src={imgProxyUrl(manga.coverToken)}
              alt={manga.title}
              onerror={() => {
                coverError = true;
              }}
              class="h-full w-full object-cover"
            />
          {:else}
            <div class="flex h-full w-full items-center justify-center" style="color: var(--text-muted);">
              <BookOpen size={34} strokeWidth={1.5} />
            </div>
          {/if}
        </div>

        <!-- Mobile-only side header summary beside cover so zero wasted horizontal space on phones -->
        <div class="flex min-w-0 flex-1 flex-col gap-1.5 sm:hidden">
          <div class="flex flex-wrap items-center gap-1.5">
            <span
              class="rounded-md px-2 py-0.5 text-[11px] font-semibold"
              style="background-color: {isTachibkCatalogSite(manga.site)
                ? 'var(--accent-soft)'
                : 'var(--surface-hover)'}; color: {isTachibkCatalogSite(manga.site)
                ? 'var(--accent)'
                : 'var(--text-muted)'};"
            >
              {isTachibkCatalogSite(manga.site)
                ? t.tachibkCatalogSiteBadge
                : isLocalComicSite(manga.site)
                  ? t.cbzLocalBadge
                  : manga.site}
            </span>
            {#if manga.unreadCount > 0}
              <span
                class="rounded-full px-2 py-0.5 text-[10px] font-semibold"
                style="background-color: var(--accent-soft); color: var(--accent);"
              >
                {t.unreadBadge(manga.unreadCount)}
              </span>
            {:else if manga.isCompleted}
              <span
                class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold"
                style="background-color: rgba(16, 185, 129, 0.14); color: #10b981;"
              >
                <CheckCircle2 size={10} />
                {t.badgeCompleted}
              </span>
            {/if}
          </div>

          <h1 class="text-lg font-bold leading-snug tracking-tight">{manga.title}</h1>
          {#if manga.originalTitle && manga.originalTitle.toLowerCase() !== manga.title.toLowerCase()}
            <div class="line-clamp-1 text-[11px] font-medium" style="color: var(--text-muted);">
              {manga.originalTitle}
            </div>
          {/if}
          <div class="flex flex-wrap items-center gap-x-2.5 gap-y-1 text-[11px]" style="color: var(--text-muted);">
            <span>{t.chapterCount(manga.chapters.length)}</span>
            {#if manga.status}
              <span class="capitalize">· {manga.status}</span>
            {/if}
          </div>
        </div>
      </div>

      <div class="relative z-10 flex min-w-0 flex-1 flex-col justify-between gap-4">
        <div class="flex flex-col gap-2">
          <!-- Desktop/Tablet headline block (hidden on <640px since rendered beside cover above) -->
          <div class="hidden flex-col gap-2 sm:flex">
            <div class="flex flex-wrap items-center gap-2">
              <span
                class="rounded-md px-2 py-0.5 text-xs font-medium"
                style="background-color: {isTachibkCatalogSite(manga.site)
                  ? 'var(--accent-soft)'
                  : 'var(--surface-hover)'}; color: {isTachibkCatalogSite(manga.site)
                  ? 'var(--accent)'
                  : 'var(--text-muted)'};"
              >
                {isTachibkCatalogSite(manga.site)
                  ? t.tachibkCatalogSiteBadge
                  : isLocalComicSite(manga.site)
                    ? t.cbzLocalBadge
                    : manga.site}
              </span>
              <span class="text-xs" style="color: var(--text-muted);">
                {t.chapterCount(manga.chapters.length)}
              </span>
              {#if manga.unreadCount > 0}
                <span
                  class="rounded-full px-2 py-0.5 text-[11px] font-semibold"
                  style="background-color: var(--accent-soft); color: var(--accent);"
                >
                  {t.unreadBadge(manga.unreadCount)}
                </span>
              {:else if manga.isCompleted}
                <span
                  class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-semibold"
                  style="background-color: rgba(16, 185, 129, 0.14); color: #10b981;"
                >
                  <CheckCircle2 size={11} />
                  {t.badgeCompleted}
                </span>
              {/if}
            </div>

            <h1 class="text-2xl font-bold leading-tight tracking-tight">{manga.title}</h1>
            {#if manga.originalTitle && manga.originalTitle.toLowerCase() !== manga.title.toLowerCase()}
              <div class="text-xs font-medium" style="color: var(--text-muted);">
                {manga.originalTitle}
              </div>
            {/if}
          </div>
          {#if manga.altTitles.length > 0}
            <div class="text-xs leading-relaxed" style="color: var(--text-muted);">
              <span class="font-semibold">{t.mangaAltTitlesLabel}:</span>
              {manga.altTitles.slice(0, 4).join(" · ")}
            </div>
          {/if}

          <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs" style="color: var(--text-muted);">
            {#if manga.authors.length > 0}
              <span>
                <strong style="color: var(--text);">{t.mangaAuthorsLabel}:</strong>
                {manga.authors.join(", ")}
              </span>
            {/if}
            {#if manga.artists.length > 0 && manga.artists.join(",") !== manga.authors.join(",")}
              <span>
                <strong style="color: var(--text);">{t.mangaArtistsLabel}:</strong>
                {manga.artists.join(", ")}
              </span>
            {/if}
            {#if manga.status}
              <span
                class="hidden rounded-md px-2 py-0.5 text-[11px] font-semibold capitalize sm:inline-flex"
                style="background-color: var(--surface-subtle); color: var(--text);"
              >
                {t.mangaStatusLabel}: {manga.status}
              </span>
            {/if}
          </div>

          {#if manga.description}
            <div class="flex flex-col items-start gap-1">
              <p
                class="{descExpanded ? '' : 'line-clamp-4'} text-xs leading-relaxed whitespace-pre-line"
                style="color: var(--text-muted);"
              >
                {manga.description}
              </p>
              {#if manga.description.length > 260}
                <button
                  type="button"
                  onclick={() => {
                    descExpanded = !descExpanded;
                  }}
                  class="text-[11px] font-semibold hover:underline"
                  style="color: var(--accent);"
                >
                  {descExpanded ? t.showLessDesc : t.readMoreDesc}
                </button>
              {/if}
            </div>
          {/if}

          {#if combinedTags().length > 0}
            {@const allTags = combinedTags()}
            {@const visibleTags = tagsExpanded ? allTags : allTags.slice(0, 24)}
            <div class="flex flex-wrap items-center gap-1.5 pt-0.5">
              {#each visibleTags as item (item.label)}
                <span
                  class="rounded-md px-2 py-0.5 text-[11px] {item.isGenre ? 'font-medium' : ''}"
                  style="background-color: {item.isGenre
                    ? 'var(--accent-soft)'
                    : 'var(--surface-subtle)'}; color: {item.isGenre
                    ? 'var(--accent)'
                    : 'var(--text-muted)'};"
                >
                  {item.label}
                </span>
              {/each}
              {#if allTags.length > 24}
                <button
                  type="button"
                  onclick={() => {
                    tagsExpanded = !tagsExpanded;
                  }}
                  class="rounded-md border px-2 py-0.5 text-[11px] font-semibold"
                  style="border-color: var(--border); background-color: var(--surface); color: var(--accent);"
                >
                  {tagsExpanded ? t.showLessTags : t.showMoreTags(allTags.length - 24)}
                </button>
              {/if}
            </div>
          {/if}

          <div class="truncate text-[11px] opacity-75" style="color: var(--text-muted);">
            {manga.canonicalUrl ?? manga.url}
          </div>

          {#if isTachibkCatalogSite(manga.site)}
            <div
              class="rounded-xl border px-3 py-2 text-xs leading-relaxed"
              style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text-muted);"
            >
              {t.tachibkCatalogBanner}
            </div>
          {/if}
        </div>

        <!-- Primary Action Bar: Full-width thumb CTA on mobile, inline row on sm+ -->
        <div class="flex flex-col gap-2 sm:flex-row sm:flex-wrap sm:items-center sm:gap-3">
          <button
            type="button"
            onclick={startPrimary}
            disabled={manga.chapters.length === 0}
            class="gpu-press inline-flex min-h-[46px] w-full items-center justify-center gap-2 rounded-xl px-5 py-2.5 text-sm font-semibold hover:opacity-92 disabled:opacity-45 sm:min-h-[42px] sm:w-auto"
            style="background-color: var(--accent); color: var(--accent-fg);"
          >
            <Play size={16} fill="currentColor" />
            <span class="truncate">
              {#if manga.progress}
                {t.continueChapter(manga.progress.chapterName)}
              {:else}
                {t.startReading}
              {/if}
            </span>
          </button>

          <div class="flex w-full items-center gap-2 sm:w-auto">
            <button
              type="button"
              onclick={toggleBookmark}
              class="gpu-press inline-flex min-h-[44px] flex-1 items-center justify-center gap-1.5 rounded-xl border px-4 py-2.5 text-sm font-medium sm:min-h-[42px] sm:flex-initial"
              style="border-color: {manga.bookmarked
                ? 'var(--accent)'
                : 'var(--border)'}; background-color: {manga.bookmarked
                ? 'var(--accent-soft)'
                : 'var(--surface-subtle)'}; color: {manga.bookmarked
                ? 'var(--accent)'
                : 'var(--text)'};"
            >
              <Bookmark size={15} fill={manga.bookmarked ? "currentColor" : "none"} />
              <span>{manga.bookmarked ? t.bookmarkedLabel : t.bookmarkAdd}</span>
            </button>

            {#if manga.progress && manga.chapters.length > 0 && !isTachibkCatalogSite(manga.site)}
              <button
                type="button"
                onclick={() => {
                  if (!manga) return;
                  const firstReadable = manga.chapters.find((c) => !isMetadataOnlyChapterUrl(c.url));
                  if (!firstReadable) {
                    toasts.push(t.tachibkCatalogChapterNoFilesToast, "info", 3200);
                    return;
                  }
                  router.goReader(manga.id, firstReadable.url, 0, 0);
                }}
                class="gpu-press min-h-[44px] flex-1 rounded-xl border px-4 py-2.5 text-sm font-medium sm:min-h-[42px] sm:flex-initial"
                style="border-color: var(--border); background-color: var(--surface-subtle);"
              >
                {t.readFromFirst}
              </button>
            {/if}
          </div>
        </div>
      </div>
    </section>

    <!-- Chapter list controls + Quick Chapter Jump -->
    <section class="flex flex-col gap-3">
      <div class="flex flex-col gap-2.5 sm:flex-row sm:flex-wrap sm:items-center sm:justify-between sm:gap-3">
        <h2 class="text-base font-bold tracking-tight">{t.chaptersHeading}</h2>

        <div class="flex flex-wrap items-center gap-2">
          <!-- Quick Chapter Jump -->
          <form
            onsubmit={(e) => {
              e.preventDefault();
              handleQuickJump();
            }}
            class="flex min-h-[40px] flex-1 items-center gap-1.5 rounded-xl border px-2.5 py-1 text-xs sm:flex-initial"
            style="background-color: var(--surface); border-color: var(--border);"
          >
            <input
              type="text"
              bind:value={jumpInput}
              placeholder={t.quickJumpPlaceholder}
              class="min-w-0 flex-1 bg-transparent py-0.5 text-xs outline-none sm:w-28 sm:flex-initial"
            />
            <button
              type="submit"
              class="gpu-press inline-flex min-h-[30px] items-center gap-1 rounded-lg px-2.5 py-1 text-[11px] font-semibold hover:opacity-90"
              style="background-color: var(--accent); color: var(--accent-fg);"
              title={t.quickChapterJump}
            >
              <CornerDownLeft size={12} />
              <span>{t.quickJumpGo}</span>
            </button>
          </form>

          <!-- Filter chapters -->
          <div
            class="flex min-h-[40px] flex-1 items-center gap-2 rounded-xl border px-3 py-1.5 text-sm sm:flex-initial"
            style="background-color: var(--surface); border-color: var(--border);"
          >
            <Search size={14} class="shrink-0" style="color: var(--text-muted);" />
            <input
              type="text"
              bind:value={query}
              placeholder={t.filterChapters}
              class="min-w-0 flex-1 bg-transparent text-xs outline-none sm:w-36 sm:flex-initial"
            />
          </div>

          <button
            type="button"
            onclick={() => {
              newestFirst = !newestFirst;
            }}
            class="gpu-press inline-flex min-h-[40px] items-center gap-1.5 rounded-xl border px-3 py-1.5 text-xs font-medium"
            style="background-color: var(--surface); border-color: var(--border);"
          >
            <ArrowDownUp size={14} />
            <span>{newestFirst ? t.newestFirst : t.oldestFirst}</span>
          </button>
        </div>
      </div>

      {#if chapterLanguages().length > 1}
        <div class="flex flex-wrap items-center gap-1.5">
          <span class="text-xs font-medium" style="color: var(--text-muted);">
            {t.mangaLangFilterLabel}:
          </span>
          <button
            type="button"
            onclick={() => {
              selectedChapterLang = "all";
            }}
            class="rounded-lg border px-2.5 py-1 text-xs font-medium"
            style="border-color: {activeChapterLang() === 'all'
              ? 'var(--accent)'
              : 'var(--border)'}; background-color: {activeChapterLang() === 'all'
              ? 'var(--accent-soft)'
              : 'var(--surface)'}; color: {activeChapterLang() === 'all'
              ? 'var(--accent)'
              : 'var(--text)'};"
          >
            {t.mangaLangAll} ({manga.chapters.length})
          </button>
          {#each chapterLanguages() as lang (lang)}
            {@const count = manga.chapters.filter((c) => (c.language ?? "").toLowerCase() === lang).length}
            <button
              type="button"
              onclick={() => {
                selectedChapterLang = lang;
              }}
              class="rounded-lg border px-2.5 py-1 text-xs font-medium uppercase"
              style="border-color: {activeChapterLang() === lang
                ? 'var(--accent)'
                : 'var(--border)'}; background-color: {activeChapterLang() === lang
                ? 'var(--accent-soft)'
                : 'var(--surface)'}; color: {activeChapterLang() === lang
                ? 'var(--accent)'
                : 'var(--text)'};"
            >
              {lang} ({count})
            </button>
          {/each}
        </div>
      {/if}

      <div
        class="grid max-h-[55dvh] grid-cols-1 gap-1.5 overflow-y-auto rounded-2xl border p-3 sm:grid-cols-2 lg:grid-cols-3"
        style="background-color: var(--surface); border-color: var(--border);"
      >
        {#each filteredChapters() as chapter (chapter.url)}
          {@const isCurrent =
            manga.progress !== null &&
            normUrl(manga.progress.chapterUrl) === normUrl(chapter.url)}
          {@const isMetaStub = isMetadataOnlyChapterUrl(chapter.url)}
          <button
            type="button"
            onclick={() => {
              if (!manga) return;
              if (isMetaStub) {
                toasts.push(t.tachibkCatalogChapterNoFilesToast, "info", 3200);
                return;
              }
              router.goReader(
                manga.id,
                chapter.url,
                isCurrent ? manga.progress?.scrollRatio : 0,
                isCurrent ? manga.progress?.pageIndex : 0
              );
            }}
            class="gpu-press flex min-h-[42px] items-center justify-between gap-2 rounded-xl px-3 py-2 text-left text-sm hover:opacity-95"
            style="background-color: {isCurrent
              ? 'var(--accent-soft)'
              : 'var(--surface-subtle)'}; color: {isCurrent
              ? 'var(--accent)'
              : chapter.read
                ? 'var(--text-muted)'
                : 'var(--text)'}; opacity: {chapter.read && !isCurrent ? 0.56 : 1};"
          >
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-1.5">
                {#if chapter.volume}
                  <span
                    class="shrink-0 rounded px-1 py-0.5 text-[10px] font-semibold"
                    style="background-color: var(--surface-hover); color: var(--text-muted);"
                  >
                    V.{chapter.volume}
                  </span>
                {/if}
                <span class="truncate font-medium">{chapter.name}</span>
              </div>
              {#if chapter.publishedAt || (chapter.language && chapterLanguages().length > 1)}
                <div class="mt-0.5 flex items-center gap-1.5 text-[10px]" style="color: var(--text-muted);">
                  {#if chapter.language && chapterLanguages().length > 1}
                    <span class="uppercase font-semibold">[{chapter.language}]</span>
                  {/if}
                  {#if chapter.publishedAt}
                    <span>{chapter.publishedAt}</span>
                  {/if}
                </div>
              {/if}
            </div>
            <div class="flex shrink-0 items-center gap-1">
              {#if chapter.sourceId === "tachibk:bookmarked"}
                <Bookmark size={12} fill="currentColor" style="color: var(--accent);" />
              {/if}
              {#if chapter.read}
                <Check size={14} class="opacity-75" />
              {/if}
            </div>
          </button>
        {/each}
      </div>
    </section>
  {/if}
</div>

<!-- Permanent Delete Confirmation Modal (Bottom Sheet on Compact Mobile, Centered Dialog on sm+) -->
{#if showDeleteModal && manga}
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
      onclick={closeDeleteModal}
      class="fixed inset-0 bg-black/60 backdrop-blur-sm"
    ></div>
    <div
      transition:modalScale|local
      class="modal-cq relative z-10 flex max-h-[calc(100dvh-1.5rem-var(--safe-top))] w-full max-w-md flex-col gap-4 overflow-y-auto overscroll-contain rounded-t-3xl border-t p-5 shadow-2xl sm:max-h-[calc(100dvh-2rem-var(--safe-top)-var(--safe-bottom))] sm:rounded-2xl sm:border"
      style="background-color: var(--surface); border-color: var(--border); box-shadow: var(--shadow-lg); padding-bottom: max(1.25rem, var(--safe-bottom));"
    >
      <div class="sheet-handle -mt-2 sm:hidden" aria-hidden="true"></div>
      <div class="flex items-start gap-3">
        <div
          class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl"
          style="background-color: rgba(239, 68, 68, 0.14); color: var(--danger);"
        >
          <AlertTriangle size={20} />
        </div>
        <div class="min-w-0">
          <h3 class="text-base font-bold">{t.deleteMangaConfirmTitle}</h3>
          <p class="mt-1 text-xs leading-relaxed" style="color: var(--text-muted);">
            {t.deleteMangaConfirmScope(manga.title)}
          </p>
        </div>
      </div>

      <div class="modal-footer-stack flex items-center justify-end gap-2 pt-1">
        <button
          type="button"
          disabled={deleting}
          onclick={closeDeleteModal}
          class="gpu-press min-h-[40px] rounded-xl border px-4 py-2 text-xs font-medium"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          {t.cancel}
        </button>
        <button
          type="button"
          disabled={deleting}
          onclick={confirmDeleteManga}
          class="gpu-press inline-flex min-h-[40px] items-center justify-center gap-1.5 rounded-xl px-4 py-2 text-xs font-semibold text-white disabled:opacity-50"
          style="background-color: var(--danger);"
        >
          {#if deleting}
            <LatticeLoader size="xs" tone="inherit" inline />
          {:else}
            <Trash2 size={13} />
          {/if}
          <span>{t.deleteMangaConfirmBtn}</span>
        </button>
      </div>
    </div>
  </div>
{/if}
