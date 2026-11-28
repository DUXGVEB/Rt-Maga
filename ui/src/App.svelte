<script lang="ts">
  import {
    Bell,
    Bookmark,
    Clock,
    Compass,
    Dices,
    Download,
    ExternalLink,
    Home as HomeIcon,
    Moon,
    RefreshCw,
    Settings as SettingsIcon,
    Sparkles,
    Sun,
    X,
  } from "lucide-svelte";
  import { onMount } from "svelte";
  import { api } from "./lib/api";
  import AiFirstUseModal from "./lib/components/AiFirstUseModal.svelte";
  import FloatingControls from "./lib/components/FloatingControls.svelte";
  import HeroTransitionLayer from "./lib/components/HeroTransitionLayer.svelte";
  import NotificationDrawer from "./lib/components/NotificationDrawer.svelte";
  import RandomMangaModal from "./lib/components/RandomMangaModal.svelte";
  import RtMagaLogo from "./lib/components/RtMagaLogo.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import ToastContainer from "./lib/components/ToastContainer.svelte";
  import UpdatePromptModal from "./lib/components/UpdatePromptModal.svelte";
  import { backdropFade, badgePop, popoverPop, tabSlide, viewReveal } from "./lib/motion";
  import { aiTranslateStore } from "./lib/stores/aiTranslate.svelte";
  import { appReleaseStore } from "./lib/stores/appRelease.svelte";
  import { deviceEnv } from "./lib/stores/deviceEnv.svelte";
  import { heroTransitionStore } from "./lib/stores/heroTransition.svelte";
  import { notificationsStore } from "./lib/stores/notifications.svelte";
  import { randomMangaStore } from "./lib/stores/randomManga.svelte";
  import { router } from "./lib/stores/router.svelte";
  import { settingsStore } from "./lib/stores/settings.svelte";
  import { updatesStore } from "./lib/stores/updates.svelte";
  import { normalizeNavOrder } from "./lib/themeAccent";
  import type { FloatingScreenKey } from "./lib/types";
  import Bookmarks from "./pages/Bookmarks.svelte";
  import Browse from "./pages/Browse.svelte";
  import History from "./pages/History.svelte";
  import Home from "./pages/Home.svelte";
  import Manga from "./pages/Manga.svelte";
  import Reader from "./pages/Reader.svelte";
  import Settings from "./pages/Settings.svelte";
  import Sources from "./pages/Sources.svelte";

  let mainEl = $state<HTMLElement | null>(null);
  let showMainScrollTop = $state(false);

  onMount(() => {
    router.installBackBridge();
    void deviceEnv.init();
    void settingsStore.init();
    void aiTranslateStore.init();
    void updatesStore.init();
    void notificationsStore.init();
    void appReleaseStore.init();

    let lastLogAt = 0;
    const forwardClientError = (category: string, message: string, detail?: string) => {
      const now = Date.now();
      if (now - lastLogAt < 1500) return;
      lastLogAt = now;
      void api
        .logClientError({
          level: "error",
          category,
          message: message.slice(0, 500),
          detail: detail ? detail.slice(0, 2000) : null,
        })
        .catch(() => {
          // Never throw from global error handler.
        });
    };

    const onWindowError = (event: ErrorEvent) => {
      const msg = event.message || "Unhandled frontend error";
      const stack =
        event.error && typeof event.error === "object" && "stack" in event.error
          ? String((event.error as { stack?: unknown }).stack ?? "")
          : `${event.filename || ""}:${event.lineno || 0}:${event.colno || 0}`;
      forwardClientError("ui_error", msg, stack);
    };

    const onUnhandledRejection = (event: PromiseRejectionEvent) => {
      const reason = event.reason;
      const msg =
        reason instanceof Error
          ? reason.message
          : typeof reason === "string"
            ? reason
            : "Unhandled promise rejection";
      const detail = reason instanceof Error ? reason.stack : undefined;
      forwardClientError("ui_rejection", msg, detail);
    };

    const onGlobalKeydown = (e: KeyboardEvent) => {
      if (e.defaultPrevented || deviceEnv.isAndroid) return;
      if (e.key === "F11") {
        e.preventDefault();
        void deviceEnv.toggleDesktopFullscreen();
      }
    };

    window.addEventListener("error", onWindowError);
    window.addEventListener("unhandledrejection", onUnhandledRejection);
    window.addEventListener("keydown", onGlobalKeydown);
    return () => {
      window.removeEventListener("error", onWindowError);
      window.removeEventListener("unhandledrejection", onUnhandledRejection);
      window.removeEventListener("keydown", onGlobalKeydown);
    };
  });

  const isReader = $derived(router.current.name === "reader");
  const isDark = $derived(settingsStore.resolvedTheme === "dark");
  const checkingAny = $derived(updatesStore.checking || appReleaseStore.checking);
  const t = $derived(settingsStore.t);
  const activeScreen = $derived<FloatingScreenKey>(router.current.name);
  const navOrder = $derived(normalizeNavOrder(settingsStore.value.ui.navOrder));
  const oneHandedMode = $derived(
    deviceEnv.isMobile ? settingsStore.value.accessibility.oneHandedMode : "default"
  );
  const routeViewKey = $derived(() => {
    const r = router.current;
    if (r.name === "manga") return `manga:${r.id}`;
    if (r.name === "browse") return `browse:${r.sourceId ?? ""}:${r.url}`;
    return r.name;
  });

  function toggleThemeMobile() {
    settingsStore.update((d) => {
      d.theme = isDark ? "light" : "dark";
    });
  }

  $effect(() => {
    void router.current.name;
    void router.navPulse;
    showMainScrollTop = false;
    if (mainEl && !isReader) {
      mainEl.scrollTop = 0;
    }
    if (deviceEnv.isAndroid) {
      void deviceEnv.reassertImmersive();
    }
  });

  // Re-tapping an already-active primary tab (e.g. Home) smoothly scrolls to top without reloading.
  $effect(() => {
    const pulse = router.scrollTopPulse;
    if (pulse <= 0) return;
    if (mainEl && !isReader) {
      mainEl.scrollTo({ top: 0, behavior: "smooth" });
    }
  });

  function handleMainScroll() {
    if (!mainEl || isReader) {
      showMainScrollTop = false;
      return;
    }
    showMainScrollTop = mainEl.scrollTop > 320;
  }

  function scrollMainToTop() {
    mainEl?.scrollTo({ top: 0, behavior: "smooth" });
  }
</script>

<div class="app-canvas flex h-full w-full flex-col overflow-hidden md:flex-row">
  {#if !isReader}
    <Sidebar />
  {/if}

  <div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
    <!-- Mobile Top App Bar: frosted Sumi Studio header with One-Handed thumb bias -->
    {#if !isReader}
      <header
        class="glass-chrome z-20 flex shrink-0 items-center justify-between border-b md:hidden {oneHandedMode === 'left'
          ? 'flex-row-reverse'
          : ''}"
        style="padding-top: calc(0.45rem + var(--safe-top)); padding-bottom: 0.45rem; padding-left: max(0.75rem, var(--safe-left)); padding-right: max(0.75rem, var(--safe-right)); border-color: var(--border);"
      >
        <button
          type="button"
          onclick={() => router.goHome()}
          class="gpu-press flex min-h-[44px] items-center gap-2.5 rounded-xl px-1.5 text-left"
        >
          <RtMagaLogo size={28} />
          <div class="leading-tight">
            <div class="text-sm font-bold tracking-tight">RtMaga</div>
          </div>
        </button>

        <div class="flex items-center gap-0.5 {oneHandedMode === 'left' ? 'flex-row-reverse' : ''}">
          <button
            type="button"
            onclick={() => randomMangaStore.open(router.current.name === "bookmarks" ? "bookmarks" : "all")}
            class="gpu-press flex min-h-[44px] min-w-[44px] items-center justify-center rounded-xl"
            style="color: {randomMangaStore.isOpen ? 'var(--accent)' : 'var(--text-muted)'}; background-color: {randomMangaStore.isOpen
              ? 'var(--accent-soft)'
              : 'transparent'};"
            title={t.randomMangaTrigger}
            aria-label={t.randomMangaTrigger}
          >
            <Dices size={19} />
          </button>

          <button
            type="button"
            onclick={() => updatesStore.checkNow()}
            disabled={checkingAny}
            class="gpu-press flex min-h-[44px] min-w-[44px] items-center justify-center rounded-xl disabled:opacity-55"
            style="color: var(--text-muted);"
            title={t.checkUpdatesTooltip}
            aria-label={t.checkUpdates}
          >
            <RefreshCw size={18} class={checkingAny ? "animate-spin" : ""} />
          </button>

          <button
            type="button"
            onclick={toggleThemeMobile}
            class="gpu-press flex min-h-[44px] min-w-[44px] items-center justify-center rounded-xl"
            style="color: var(--text-muted);"
            title={isDark ? t.lightMode : t.darkMode}
            aria-label={isDark ? t.lightMode : t.darkMode}
          >
            {#key isDark}
              <span in:badgePop|local={{ duration: 140, start: 0.72 }} class="inline-flex">
                {#if isDark}
                  <Sun size={18} />
                {:else}
                  <Moon size={18} />
                {/if}
              </span>
            {/key}
          </button>

          <button
            type="button"
            onclick={() => notificationsStore.toggleDrawer()}
            class="gpu-press relative flex min-h-[44px] min-w-[44px] items-center justify-center rounded-xl"
            style="color: {notificationsStore.drawerOpen ? 'var(--accent)' : 'var(--text)'}; background-color: {notificationsStore.drawerOpen
              ? 'var(--accent-soft)'
              : 'transparent'};"
            title={t.notifCenterTitle}
            aria-label={t.notifCenterTitle}
          >
            <Bell size={19} />
            {#if notificationsStore.unreadCount > 0}
              <span
                transition:badgePop|local
                class="absolute top-1.5 right-1.5 flex h-4 min-w-4 items-center justify-center rounded-full px-1 text-[10px] font-bold"
                style="background-color: var(--accent); color: var(--accent-fg);"
              >
                {notificationsStore.unreadCount > 99 ? "99+" : notificationsStore.unreadCount}
              </span>
            {/if}
          </button>
        </div>
      </header>
    {/if}

    <main
      bind:this={mainEl}
      onscroll={handleMainScroll}
      class="relative min-h-0 min-w-0 flex-1 {isReader
        ? 'overflow-hidden overscroll-none'
        : 'main-scroll-area overflow-y-auto overflow-x-hidden overscroll-none touch-pan-y'}"
    >
      {#if !isReader && !appReleaseStore.promptOpen && appReleaseStore.shouldShowBanner && appReleaseStore.release}
        <div
          transition:popoverPop|local={{ duration: 170, y: -6 }}
          class="glass-chrome sticky top-0 z-20 flex flex-wrap items-center justify-between gap-3 border-b px-4 py-2.5 text-xs"
          style="border-color: var(--border);"
        >
          <div class="flex min-w-0 items-center gap-2 font-medium">
            <Sparkles size={15} class="shrink-0" style="color: var(--accent);" />
            <span class="truncate">
              {t.releaseAvailableBanner(appReleaseStore.release.latestVersion)}
            </span>
          </div>

          <div class="flex shrink-0 items-center gap-2">
            <button
              type="button"
              onclick={() => appReleaseStore.openPrompt()}
              class="gpu-press inline-flex min-h-[34px] items-center gap-1.5 rounded-xl px-3 py-1 font-semibold hover:opacity-90"
              style="background-color: var(--accent); color: var(--accent-fg);"
            >
              <Download size={13} />
              <span>{t.updateDownloadAndInstall}</span>
            </button>
            <button
              type="button"
              onclick={() => appReleaseStore.openReleasePage()}
              class="gpu-press inline-flex min-h-[34px] items-center gap-1 rounded-xl border px-2.5 py-1 font-medium hover:opacity-85"
              style="border-color: var(--border); background-color: var(--surface);"
            >
              <ExternalLink size={12} />
              <span>{t.viewRelease}</span>
            </button>
            <button
              type="button"
              onclick={() => appReleaseStore.dismissPrompt()}
              class="gpu-press flex h-8 w-8 items-center justify-center rounded-xl opacity-70 hover:opacity-100"
              aria-label={t.dismissUpdate}
              title={t.dismissUpdate}
            >
              <X size={14} />
            </button>
          </div>
        </div>
      {/if}

      {#if router.current.name === "reader"}
        <div in:backdropFade|local={{ duration: 150 }} class="h-full w-full">
          <Reader
            mangaId={router.current.mangaId}
            chapterUrl={router.current.chapterUrl}
            initialScrollRatio={router.current.scrollRatio ?? 0}
            initialPageIndex={router.current.pageIndex ?? 0}
          />
        </div>
      {:else}
        {#key routeViewKey()}
          <div
            data-route-shell
            in:tabSlide|local={{
              dir: router.lastTabSlideDir,
              duration:
                router.current.name === "manga" && heroTransitionStore.active !== null
                  ? 120
                  : router.lastTabSlideDir !== 0
                    ? 190
                    : 155,
            }}
            class="min-h-full w-full"
          >
            {#if router.current.name === "home"}
              <Home />
            {:else if router.current.name === "sources"}
              <Sources initialQuery={router.current.initialQuery} />
            {:else if router.current.name === "browse"}
              <Browse
                url={router.current.url}
                sourceId={router.current.sourceId}
                initialCatalog={router.current.initialCatalog}
              />
            {:else if router.current.name === "bookmarks"}
              <Bookmarks />
            {:else if router.current.name === "manga"}
              <Manga id={router.current.id} />
            {:else if router.current.name === "history"}
              <History />
            {:else if router.current.name === "settings"}
              <Settings focusSection={router.current.section} subTarget={router.current.subTarget} />
            {/if}
          </div>
        {/key}
      {/if}
    </main>
  </div>

  {#if !isReader}
    <!-- Floating Scroll-to-Top for non-reader screens -->
    <FloatingControls
      screen={activeScreen}
      showScrollTop={showMainScrollTop}
      onScrollTop={scrollMainToTop}
      onLongPressScrollTop={() => router.goSettings("floating")}
      hasBottomNav={deviceEnv.layoutTier === "compact"}
    />

    <!-- Compact Portrait Bottom Navigation (< 768px): ergonomic >=44x44px thumb targets -->
    <nav
      aria-label={t.navOrderSection}
      class="glass-chrome fixed right-0 bottom-0 left-0 z-20 flex items-center border-t pt-1.5 md:hidden {oneHandedMode === 'left'
        ? 'justify-start'
        : oneHandedMode === 'right'
          ? 'justify-end'
          : 'justify-center'}"
      style="padding-bottom: max(0.45rem, var(--safe-bottom)); padding-left: max(0.45rem, var(--safe-left)); padding-right: max(0.45rem, var(--safe-right)); border-color: var(--border);"
    >
      <div
        class="flex items-center transition-[width] duration-200 {oneHandedMode === 'default'
          ? 'w-full justify-around'
          : 'w-[78%] max-w-[330px] justify-between gap-0.5'}"
      >
      {#each navOrder as dest (dest)}
        {#if dest === "home"}
          {@const active = router.current.name === "home" || router.current.name === "manga"}
          <button
            type="button"
            onclick={() => router.goHome()}
            class="gpu-press relative flex min-h-[46px] min-w-[46px] flex-1 flex-col items-center justify-center gap-0.5 rounded-2xl px-1.5 py-1 text-[11px] font-medium"
            style="color: {active ? 'var(--accent)' : 'var(--text-muted)'};"
            aria-current={active ? "page" : undefined}
          >
            <span
              class="nav-pill-bg relative flex h-7 w-12 items-center justify-center rounded-full"
              style="background-color: {active ? 'var(--accent-soft)' : 'transparent'}; transform: scale({active ? 1 : 0.94});"
            >
              <HomeIcon size={19} strokeWidth={active ? 2.25 : 1.85} />
              {#if updatesStore.newCount > 0}
                <span
                  transition:badgePop|local
                  class="absolute -top-0.5 right-1 flex h-3.5 min-w-3.5 items-center justify-center rounded-full px-1 text-[9px] font-bold"
                  style="background-color: var(--accent); color: var(--accent-fg);"
                >
                  {updatesStore.newCount > 9 ? "9+" : updatesStore.newCount}
                </span>
              {/if}
            </span>
            <span class="truncate leading-tight {active ? 'font-semibold' : ''}">{t.navHome}</span>
          </button>
        {:else if dest === "sources"}
          {@const active = router.current.name === "sources" || router.current.name === "browse"}
          <button
            type="button"
            onclick={() => router.goSources()}
            class="gpu-press flex min-h-[46px] min-w-[46px] flex-1 flex-col items-center justify-center gap-0.5 rounded-2xl px-1.5 py-1 text-[11px] font-medium"
            style="color: {active ? 'var(--accent)' : 'var(--text-muted)'};"
            aria-current={active ? "page" : undefined}
          >
            <span
              class="nav-pill-bg flex h-7 w-12 items-center justify-center rounded-full"
              style="background-color: {active ? 'var(--accent-soft)' : 'transparent'}; transform: scale({active ? 1 : 0.94});"
            >
              <Compass size={19} strokeWidth={active ? 2.25 : 1.85} />
            </span>
            <span class="truncate leading-tight {active ? 'font-semibold' : ''}">{t.navSources}</span>
          </button>
        {:else if dest === "bookmarks"}
          {@const active = router.current.name === "bookmarks"}
          <button
            type="button"
            onclick={() => router.goBookmarks()}
            class="gpu-press flex min-h-[46px] min-w-[46px] flex-1 flex-col items-center justify-center gap-0.5 rounded-2xl px-1.5 py-1 text-[11px] font-medium"
            style="color: {active ? 'var(--accent)' : 'var(--text-muted)'};"
            aria-current={active ? "page" : undefined}
          >
            <span
              class="nav-pill-bg flex h-7 w-12 items-center justify-center rounded-full"
              style="background-color: {active ? 'var(--accent-soft)' : 'transparent'}; transform: scale({active ? 1 : 0.94});"
            >
              <Bookmark size={19} strokeWidth={active ? 2.25 : 1.85} />
            </span>
            <span class="truncate leading-tight {active ? 'font-semibold' : ''}">{t.navBookmarks}</span>
          </button>
        {:else if dest === "history"}
          {@const active = router.current.name === "history"}
          <button
            type="button"
            onclick={() => void router.goHistoryOrResume()}
            class="gpu-press flex min-h-[46px] min-w-[46px] flex-1 flex-col items-center justify-center gap-0.5 rounded-2xl px-1.5 py-1 text-[11px] font-medium"
            style="color: {active ? 'var(--accent)' : 'var(--text-muted)'};"
            aria-current={active ? "page" : undefined}
            title={t.historyResumeHint}
          >
            <span
              class="nav-pill-bg flex h-7 w-12 items-center justify-center rounded-full"
              style="background-color: {active ? 'var(--accent-soft)' : 'transparent'}; transform: scale({active ? 1 : 0.94});"
            >
              <Clock size={19} strokeWidth={active ? 2.25 : 1.85} />
            </span>
            <span class="truncate leading-tight {active ? 'font-semibold' : ''}">{t.navHistory}</span>
          </button>
        {:else}
          {@const active = router.current.name === "settings"}
          <button
            type="button"
            onclick={() => router.goSettings()}
            class="gpu-press flex min-h-[46px] min-w-[46px] flex-1 flex-col items-center justify-center gap-0.5 rounded-2xl px-1.5 py-1 text-[11px] font-medium"
            style="color: {active ? 'var(--accent)' : 'var(--text-muted)'};"
            aria-current={active ? "page" : undefined}
          >
            <span
              class="nav-pill-bg flex h-7 w-12 items-center justify-center rounded-full"
              style="background-color: {active ? 'var(--accent-soft)' : 'transparent'}; transform: scale({active ? 1 : 0.94});"
            >
              <SettingsIcon size={19} strokeWidth={active ? 2.25 : 1.85} />
            </span>
            <span class="truncate leading-tight {active ? 'font-semibold' : ''}">{t.navSettings}</span>
          </button>
        {/if}
      {/each}
      </div>
    </nav>
  {/if}

  <NotificationDrawer />
  <HeroTransitionLayer />
  <AiFirstUseModal />

  {#if !isReader}
    <RandomMangaModal />
    <UpdatePromptModal />
  {/if}

  <ToastContainer />
</div>
