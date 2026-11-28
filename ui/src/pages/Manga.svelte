<script lang="ts">
  import {
    AlertTriangle,
    ArrowDownUp,
    ArrowLeft,
    Bookmark,
    BookOpen,
    Check,
    CheckCircle2,
    ChevronDown,
    Copy,
    CornerDownLeft,
    ExternalLink,
    Eye,
    EyeOff,
    Play,
    RefreshCw,
    Search,
    Trash2,
  } from "lucide-svelte";
  import { onMount } from "svelte";
  import { api, formatAppError, imgProxyUrl } from "../lib/api";
  import {
    isLocalComicSite,
    isMetadataOnlyChapterUrl,
    isTachibkCatalogSite,
  } from "../lib/comicFormats";
  import CbzDropZone from "../lib/components/CbzDropZone.svelte";
  import LatticeLoader from "../lib/components/LatticeLoader.svelte";
  import SourceDiagnosticCard from "../lib/components/SourceDiagnosticCard.svelte";
  import {
    adaptiveOverlay,
    backdropFade,
    badgePop,
    collapseReveal,
    createOverlayGuard,
    popoverPop,
    pressable,
    viewReveal,
  } from "../lib/motion";
  import { deviceEnv } from "../lib/stores/deviceEnv.svelte";
  import { heroTransitionStore } from "../lib/stores/heroTransition.svelte";
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
  let titleMenuOpen = $state(false);
  let navigatingTag = $state<string | null>(null);
  let loadSeq = 0;
  const deleteModalGuard = createOverlayGuard(200);
  const coverHiddenByHero = $derived(heroTransitionStore.isTargetHidden(id));

  function normalizeHost(rawUrl: string): string {
    try {
      const normalized = rawUrl.includes("://") ? rawUrl : `https://${rawUrl}`;
      return new URL(normalized).hostname.replace(/^(www\.|m\.)/i, "").toLowerCase();
    } catch {
      return "";
    }
  }

  async function handleTagClick(tagLabel: string) {
    const clean = tagLabel.trim();
    if (!manga || !clean || navigatingTag) return;

    const rawUrl = (manga.canonicalUrl ?? manga.url ?? "").trim();
    const isWeb =
      !isLocalComicSite(manga.site) &&
      !isTachibkCatalogSite(manga.site) &&
      /^https?:\/\//i.test(rawUrl);

    if (!isWeb) {
      router.goSources(clean);
      return;
    }

    navigatingTag = clean;
    try {
      const parsed = new URL(rawUrl);
      const origin = parsed.origin;
      const mangaHost = normalizeHost(rawUrl);
      const siteKey = manga.site.trim().toLowerCase();

      const saved = await api.listSavedSources("all").catch(() => []);
      const matched =
        saved.find((s) => normalizeHost(s.baseUrl) === mangaHost) ??
        saved.find((s) => s.sourceKey.trim().toLowerCase() === siteKey) ??
        null;

      if (matched) {
        router.goBrowse(matched.baseUrl, undefined, false, matched.id, clean);
      } else {
        router.goBrowse(origin, undefined, false, undefined, clean);
      }
    } catch {
      router.goSources(clean);
    } finally {
      navigatingTag = null;
    }
  }

  onMount(() => {
    const unregisterBack = router.registerBackInterceptor(() => {
      if (titleMenuOpen) {
        titleMenuOpen = false;
        return true;
      }
      if (showDeleteModal && !deleting) {
        showDeleteModal = false;
        return true;
      }
      return false;
    });
    const onDocPointerDown = (e: PointerEvent) => {
      if (!titleMenuOpen) return;
      if (e.target instanceof HTMLElement && e.target.closest("[data-title-menu-root]")) {
        return;
      }
      titleMenuOpen = false;
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && titleMenuOpen) {
        titleMenuOpen = false;
      }
    };
    document.addEventListener("pointerdown", onDocPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      unregisterBack();
      document.removeEventListener("pointerdown", onDocPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  });

  function searchMangaInSources() {
    if (!manga) return;
    const title = manga.title.trim();
    titleMenuOpen = false;
    if (!title) return;
    router.goSources(title);
  }

  async function copyMangaTitle() {
    if (!manga) return;
    const title = manga.title.trim();
    titleMenuOpen = false;
    if (!title) return;
    try {
      await navigator.clipboard.writeText(title);
      toasts.push(t.mangaTitleCopiedToast, "success", 2000);
    } catch {
      toasts.push(t.mangaTitleCopiedToast, "info", 2000);
    }
  }

  const externalSourceUrl = $derived(() => {
    if (!manga || isLocalComicSite(manga.site)) return null;
    const raw = (manga.canonicalUrl ?? manga.url ?? "").trim();
    return /^https?:\/\//i.test(raw) ? raw : null;
  });

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
      if (manga.hidden) {
        await api.setMangaHidden(manga.id, "unhide");
        manga = { ...manga, hidden: false };
        void notificationsStore.refresh();
        toasts.push(t.toastMangaUnhidden(manga.title), "success", 2400);
      } else {
        await api.setMangaHidden(manga.id, "hide");
        void notificationsStore.refresh();
        toasts.push(t.toastMangaHidden, "info", 2800);
        router.goHome();
      }
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

    {#if manga}
      <div in:viewReveal|local={{ duration: 150, y: 0 }} class="flex flex-wrap items-center gap-2">
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
            use:pressable
            onclick={refresh}
            disabled={refreshing}
            class="btn-3d btn-3d-surface inline-flex items-center gap-1.5 rounded-xl px-3 py-1.5 text-xs font-medium"
          >
            <RefreshCw size={14} class={refreshing ? "animate-spin" : ""} />
            <span>{refreshing ? t.refreshing : t.refreshChapters}</span>
          </button>
        {/if}
        <button
          type="button"
          use:pressable
          onclick={hideManga}
          disabled={hiding}
          class="btn-3d btn-3d-surface inline-flex items-center gap-1.5 rounded-xl px-3 py-1.5 text-xs font-medium disabled:opacity-50"
          style="color: {manga.hidden ? 'var(--accent)' : 'var(--text-muted)'};"
        >
          {#if manga.hidden}
            <Eye size={14} />
            <span>{t.unhideMangaBtn}</span>
          {:else}
            <EyeOff size={14} />
            <span>{t.hideMangaBtn}</span>
          {/if}
        </button>
        <button
          type="button"
          use:pressable
          onclick={openDeleteModal}
          class="btn-3d btn-3d-surface inline-flex items-center gap-1.5 rounded-xl px-3 py-1.5 text-xs font-medium"
          style="color: var(--danger);"
        >
          <Trash2 size={14} />
          <span>{t.deleteMangaBtn}</span>
        </button>
      </div>
    {/if}
  </div>

  {#if loading}
    <section
      class="surface-card relative flex flex-col gap-5 overflow-hidden rounded-2xl p-4 sm:flex-row sm:gap-6 sm:p-6"
    >
      <div class="relative z-10 flex items-start gap-4 sm:block sm:shrink-0">
        <div
          data-hero-cover-target="manga-{id}"
          class="aspect-[3/4] w-28 shrink-0 rounded-xl border shadow-md sm:h-60 sm:w-44 {coverHiddenByHero
            ? ''
            : 'animate-pulse'}"
          style="background-color: var(--surface-hover); border-color: var(--border); opacity: {coverHiddenByHero
            ? 0
            : 1};"
        ></div>
        <div class="flex flex-1 flex-col gap-2.5 pt-1 sm:hidden">
          <div class="h-5 w-4/5 animate-pulse rounded" style="background-color: var(--surface-hover);"></div>
          <div class="h-3.5 w-1/2 animate-pulse rounded" style="background-color: var(--surface-hover);"></div>
        </div>
      </div>
      <div class="hidden flex-1 flex-col gap-3 pt-2 sm:flex">
        <div class="h-7 w-2/3 animate-pulse rounded" style="background-color: var(--surface-hover);"></div>
        <div class="h-4 w-1/3 animate-pulse rounded" style="background-color: var(--surface-hover);"></div>
        <div class="mt-2 h-16 w-full animate-pulse rounded-xl" style="background-color: var(--surface-hover);"></div>
      </div>
    </section>
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
    <div class="flex flex-col gap-6">
      {#if refreshErrorState}
        <div transition:collapseReveal|local>
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
        </div>
      {/if}
    <!-- Editorial Mobile-First & Desktop Series Hero -->
    <section
      class="surface-card relative flex flex-col gap-5 overflow-hidden rounded-2xl p-4 sm:flex-row sm:gap-6 sm:p-6"
      style="contain: layout style;"
    >
      <!-- Zero-cost GPU ambient tint (replaces full-bleed blur-2xl image shader that caused open stutter) -->
      <div
        class="pointer-events-none absolute inset-0"
        aria-hidden="true"
        style="background: radial-gradient(56% 90% at 12% 20%, color-mix(in srgb, var(--accent) 8%, transparent), transparent 72%);"
      ></div>

      <!-- Mobile compact top strip (cover + primary headline side-by-side); stacks as left column on sm+ -->
      <div class="relative z-10 flex items-start gap-4 sm:block sm:shrink-0">
        <div
          data-hero-cover-target="manga-{id}"
          class="aspect-[3/4] w-28 shrink-0 overflow-hidden rounded-xl border shadow-md sm:h-60 sm:w-44"
          style="background-color: var(--surface-subtle); border-color: var(--border); opacity: {coverHiddenByHero
            ? 0
            : 1};"
        >
          {#if manga.coverToken && !coverError}
            <img
              src={imgProxyUrl(manga.coverToken)}
              alt={manga.title}
              decoding="async"
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
        <div
          class="hero-emerge flex min-w-0 flex-1 flex-col gap-1.5 sm:hidden"
        >
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

          <div class="relative" data-title-menu-root>
            <button
              type="button"
              onclick={() => {
                titleMenuOpen = !titleMenuOpen;
              }}
              aria-expanded={titleMenuOpen}
              class="group inline-flex max-w-full items-start gap-1 rounded-lg text-left transition-opacity hover:opacity-85"
            >
              <h1 class="text-lg font-bold leading-snug tracking-tight group-hover:underline">
                {manga.title}
              </h1>
              <ChevronDown
                size={15}
                class="mt-1 shrink-0 opacity-60 transition-transform duration-150 {titleMenuOpen ? 'rotate-180' : ''}"
              />
            </button>

            {#if titleMenuOpen}
              <div
                transition:popoverPop|local={{ duration: 140, y: -4 }}
                role="menu"
                class="absolute top-full left-0 z-30 mt-1.5 flex w-56 flex-col gap-1 rounded-xl border p-1.5 shadow-lg"
                style="background-color: var(--surface-elevated); border-color: var(--border); will-change: transform, opacity;"
              >
                <button
                  type="button"
                  role="menuitem"
                  use:pressable
                  onclick={searchMangaInSources}
                  class="btn-3d btn-3d-subtle flex min-h-[40px] w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-xs font-semibold"
                  style="background-color: var(--accent-soft); border-color: transparent; color: var(--accent);"
                >
                  <Search size={14} class="shrink-0" />
                  <span>{t.mangaTitleSearchInSources}</span>
                </button>
                <button
                  type="button"
                  role="menuitem"
                  use:pressable
                  onclick={() => void copyMangaTitle()}
                  class="btn-3d btn-3d-subtle flex min-h-[38px] w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-xs font-medium"
                  style="background-color: var(--surface); border-color: transparent; color: var(--text);"
                >
                  <Copy size={13} class="shrink-0" style="color: var(--text-muted);" />
                  <span>{t.mangaTitleCopyTitle}</span>
                </button>
              </div>
            {/if}
          </div>
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

      <div class="hero-emerge relative z-10 flex min-w-0 flex-1 flex-col justify-between gap-4">
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

            <div class="relative inline-block max-w-full" data-title-menu-root>
              <button
                type="button"
                onclick={() => {
                  titleMenuOpen = !titleMenuOpen;
                }}
                aria-expanded={titleMenuOpen}
                class="group inline-flex max-w-full items-center gap-1.5 rounded-lg text-left transition-opacity hover:opacity-85"
              >
                <h1 class="text-2xl font-bold leading-tight tracking-tight group-hover:underline">
                  {manga.title}
                </h1>
                <ChevronDown
                  size={17}
                  class="mt-0.5 shrink-0 opacity-60 transition-transform duration-150 {titleMenuOpen ? 'rotate-180' : ''}"
                />
              </button>

              {#if titleMenuOpen}
                <div
                  transition:popoverPop|local={{ duration: 140, y: -4 }}
                  role="menu"
                  class="absolute top-full left-0 z-30 mt-1.5 flex w-60 flex-col gap-1 rounded-xl border p-1.5 shadow-lg"
                  style="background-color: var(--surface-elevated); border-color: var(--border); will-change: transform, opacity;"
                >
                  <button
                    type="button"
                    role="menuitem"
                    use:pressable
                    onclick={searchMangaInSources}
                    class="btn-3d btn-3d-subtle flex min-h-[38px] w-full items-center gap-2 rounded-lg px-2.5 py-2 text-left text-xs font-semibold"
                    style="background-color: var(--accent-soft); border-color: transparent; color: var(--accent);"
                  >
                    <Search size={14} class="shrink-0" />
                    <span>{t.mangaTitleSearchInSources}</span>
                  </button>
                  <button
                    type="button"
                    role="menuitem"
                    use:pressable
                    onclick={() => void copyMangaTitle()}
                    class="btn-3d btn-3d-subtle flex min-h-[36px] w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-xs font-medium"
                    style="background-color: var(--surface); border-color: transparent; color: var(--text);"
                  >
                    <Copy size={13} class="shrink-0" style="color: var(--text-muted);" />
                    <span>{t.mangaTitleCopyTitle}</span>
                  </button>
                </div>
              {/if}
            </div>
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

          <div
            class="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs"
            style="color: var(--text-muted);"
          >
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
              <div
                class="w-full overflow-hidden transition-[max-height] duration-200 ease-out"
                style="max-height: {descExpanded ? '2400px' : '5.5rem'};"
              >
                <p
                  class="{descExpanded ? '' : 'line-clamp-4'} text-xs leading-relaxed whitespace-pre-line"
                  style="color: var(--text-muted);"
                >
                  {manga.description}
                </p>
              </div>
              {#if manga.description.length > 260}
                <button
                  type="button"
                  onclick={() => {
                    descExpanded = !descExpanded;
                  }}
                  class="gpu-press text-[11px] font-semibold hover:underline"
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
                {@const isBusyTag = navigatingTag === item.label}
                <button
                  type="button"
                  use:pressable
                  disabled={navigatingTag !== null}
                  onclick={() => void handleTagClick(item.label)}
                  class="pill-3d inline-flex items-center rounded-lg border px-2.5 py-0.5 text-[11px] disabled:opacity-60 {item.isGenre
                    ? 'font-semibold'
                    : 'font-medium'}"
                  style="background-color: {item.isGenre
                    ? 'var(--accent-soft)'
                    : 'var(--surface-subtle)'}; border-color: {item.isGenre
                    ? 'color-mix(in srgb, var(--accent) 32%, transparent)'
                    : 'var(--border)'}; color: {item.isGenre
                    ? 'var(--accent)'
                    : 'var(--text)'}; opacity: {isBusyTag ? 0.65 : 1};"
                >
                  {item.label}
                </button>
              {/each}
              {#if allTags.length > 24}
                <button
                  type="button"
                  use:pressable
                  onclick={() => {
                    tagsExpanded = !tagsExpanded;
                  }}
                  class="pill-3d rounded-lg border px-2.5 py-0.5 text-[11px] font-semibold"
                  style="border-color: var(--border); background-color: var(--surface); color: var(--accent);"
                >
                  {tagsExpanded ? t.showLessTags : t.showMoreTags(allTags.length - 24)}
                </button>
              {/if}
            </div>
          {/if}

          {#if externalSourceUrl()}
            {@const extUrl = externalSourceUrl()!}
            <button
              type="button"
              onclick={() => void handleOpenExternal(extUrl)}
              title={t.mangaOpenExternalSourceTooltip}
              class="hero-emerge gpu-press inline-flex max-w-full items-center gap-1.5 self-start rounded-lg px-1.5 py-1 text-left text-[11px] font-medium hover:underline hover:opacity-100"
              style="--stagger-delay: 130ms; color: var(--accent);"
            >
              <ExternalLink size={12} class="shrink-0" />
              <span class="truncate">{extUrl}</span>
            </button>
          {:else}
            <div
              class="hero-emerge truncate text-[11px] opacity-75"
              style="--stagger-delay: 130ms; color: var(--text-muted);"
            >
              {manga.canonicalUrl ?? manga.url}
            </div>
          {/if}
        </div>

        <!-- Primary Action Bar: Full-width thumb CTA on mobile, inline row on sm+ -->
        <div
          class="hero-emerge flex flex-col gap-2 sm:flex-row sm:flex-wrap sm:items-center sm:gap-3"
          style="--stagger-delay: 150ms;"
        >
          <button
            type="button"
            use:pressable
            onclick={startPrimary}
            disabled={manga.chapters.length === 0}
            class="btn-3d btn-3d-accent inline-flex min-h-[46px] w-full items-center justify-center gap-2 rounded-xl px-5 py-2.5 text-sm font-semibold disabled:opacity-45 sm:min-h-[42px] sm:w-auto"
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
              use:pressable
              onclick={toggleBookmark}
              class="btn-3d btn-3d-surface inline-flex min-h-[44px] flex-1 items-center justify-center gap-1.5 rounded-xl px-4 py-2.5 text-sm font-medium sm:min-h-[42px] sm:flex-initial"
              style="border-color: {manga.bookmarked
                ? 'var(--accent)'
                : 'var(--border-strong)'}; background-color: {manga.bookmarked
                ? 'var(--accent-soft)'
                : 'var(--surface)'}; color: {manga.bookmarked
                ? 'var(--accent)'
                : 'var(--text)'};"
            >
              {#key manga.bookmarked}
                <span in:badgePop|local={{ duration: 160, start: 0.65 }} class="inline-flex">
                  <Bookmark size={15} fill={manga.bookmarked ? "currentColor" : "none"} />
                </span>
              {/key}
              <span>{manga.bookmarked ? t.bookmarkedLabel : t.bookmarkAdd}</span>
            </button>

            {#if manga.progress && manga.chapters.length > 0 && !isTachibkCatalogSite(manga.site)}
              <button
                type="button"
                use:pressable
                onclick={() => {
                  if (!manga) return;
                  const firstReadable = manga.chapters.find((c) => !isMetadataOnlyChapterUrl(c.url));
                  if (!firstReadable) {
                    toasts.push(t.tachibkCatalogChapterNoFilesToast, "info", 3200);
                    return;
                  }
                  router.goReader(manga.id, firstReadable.url, 0, 0);
                }}
                class="btn-3d btn-3d-surface min-h-[44px] flex-1 rounded-xl px-4 py-2.5 text-sm font-medium sm:min-h-[42px] sm:flex-initial"
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
              use:pressable
              class="btn-3d btn-3d-accent inline-flex min-h-[30px] items-center gap-1 rounded-lg px-2.5 py-1 text-[11px] font-semibold"
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
            use:pressable
            onclick={() => {
              newestFirst = !newestFirst;
            }}
            class="btn-3d btn-3d-surface inline-flex min-h-[40px] items-center gap-1.5 rounded-xl px-3 py-1.5 text-xs font-medium"
          >
            <ArrowDownUp size={14} class="chevron-rot" data-open={newestFirst ? "false" : "true"} />
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
            use:pressable
            onclick={() => {
              selectedChapterLang = "all";
            }}
            class="pill-3d rounded-lg border px-2.5 py-1 text-xs font-medium"
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
              use:pressable
              onclick={() => {
                selectedChapterLang = lang;
              }}
              class="pill-3d rounded-lg border px-2.5 py-1 text-xs font-medium uppercase"
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
        class="max-h-[55dvh] overflow-y-auto rounded-2xl border p-3"
        style="background-color: var(--surface); border-color: var(--border);"
      >
        {#key `${newestFirst}:${activeChapterLang()}`}
          <div
            in:viewReveal|local={{ duration: 130, y: 3 }}
            class="grid grid-cols-1 gap-1.5 sm:grid-cols-2 lg:grid-cols-3"
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
        {/key}
      </div>
    </section>
    </div>
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
      class="modal-backdrop fixed inset-0"
    ></div>
    <div
      transition:adaptiveOverlay|local={{ isMobile: deviceEnv.layoutTier === "compact" }}
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
          use:pressable
          disabled={deleting}
          onclick={closeDeleteModal}
          class="btn-3d btn-3d-subtle min-h-[40px] rounded-xl px-4 py-2 text-xs font-medium"
        >
          {t.cancel}
        </button>
        <button
          type="button"
          use:pressable
          disabled={deleting}
          onclick={confirmDeleteManga}
          class="btn-3d btn-3d-danger inline-flex min-h-[40px] items-center justify-center gap-1.5 rounded-xl px-4 py-2 text-xs font-semibold disabled:opacity-50"
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
