<script lang="ts">
  import {
    Bell,
    Bookmark,
    Clock,
    Download,
    ExternalLink,
    Home as HomeIcon,
    Settings as SettingsIcon,
    Sparkles,
    X,
  } from "lucide-svelte";
  import { onMount } from "svelte";
  import logoUrl from "./assets/logo.png";
  import { api } from "./lib/api";
  import FloatingControls from "./lib/components/FloatingControls.svelte";
  import NotificationDrawer from "./lib/components/NotificationDrawer.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import ToastContainer from "./lib/components/ToastContainer.svelte";
  import UpdatePromptModal from "./lib/components/UpdatePromptModal.svelte";
  import { appReleaseStore } from "./lib/stores/appRelease.svelte";
  import { deviceEnv } from "./lib/stores/deviceEnv.svelte";
  import { notificationsStore } from "./lib/stores/notifications.svelte";
  import { router } from "./lib/stores/router.svelte";
  import { settingsStore } from "./lib/stores/settings.svelte";
  import { updatesStore } from "./lib/stores/updates.svelte";
  import type { FloatingScreenKey } from "./lib/types";
  import Bookmarks from "./pages/Bookmarks.svelte";
  import History from "./pages/History.svelte";
  import Home from "./pages/Home.svelte";
  import Manga from "./pages/Manga.svelte";
  import Reader from "./pages/Reader.svelte";
  import Settings from "./pages/Settings.svelte";

  let mainEl = $state<HTMLElement | null>(null);
  let showMainScrollTop = $state(false);

  onMount(() => {
    router.installBackBridge();
    void deviceEnv.init();
    void settingsStore.init();
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

    window.addEventListener("error", onWindowError);
    window.addEventListener("unhandledrejection", onUnhandledRejection);
    return () => {
      window.removeEventListener("error", onWindowError);
      window.removeEventListener("unhandledrejection", onUnhandledRejection);
    };
  });

  const isReader = $derived(router.current.name === "reader");
  const t = $derived(settingsStore.t);
  const activeScreen = $derived<FloatingScreenKey>(router.current.name);

  $effect(() => {
    void router.current.name;
    showMainScrollTop = false;
    if (mainEl && !isReader) {
      mainEl.scrollTop = 0;
    }
    if (deviceEnv.isAndroid) {
      void deviceEnv.reassertImmersive();
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

<div class="flex h-full w-full flex-col overflow-hidden md:flex-row">
  {#if !isReader}
    <Sidebar />
  {/if}

  <main
    bind:this={mainEl}
    onscroll={handleMainScroll}
    class="relative flex-1 {isReader ? 'overflow-hidden' : 'overflow-y-auto pb-20 md:pb-0'}"
    style={isReader
      ? ""
      : "padding-top: var(--safe-top); padding-left: var(--safe-left); padding-right: var(--safe-right);"}
  >
    <!-- Mobile compact top bar with Bell Notification Center -->
    {#if !isReader}
      <div
        class="sticky top-0 z-20 flex items-center justify-between border-b px-4 py-2.5 backdrop-blur-md md:hidden"
        style="background-color: color-mix(in srgb, var(--surface) 92%, transparent); border-color: var(--border);"
      >
        <button
          type="button"
          onclick={() => router.goHome()}
          class="flex items-center gap-2 text-left"
        >
          <img src={logoUrl} alt="RtMaga" class="h-6 w-6 rounded-md shadow-sm" />
          <span class="text-sm font-semibold tracking-tight">RtMaga</span>
        </button>

        <button
          type="button"
          onclick={() => notificationsStore.toggleDrawer()}
          class="relative rounded-lg p-1.5"
          style="color: var(--text);"
          title={t.notifCenterTitle}
          aria-label={t.notifCenterTitle}
        >
          <Bell size={19} />
          {#if notificationsStore.unreadCount > 0}
            <span
              class="absolute -top-0.5 -right-0.5 flex h-4 min-w-4 items-center justify-center rounded-full px-1 text-[10px] font-bold text-white"
              style="background-color: var(--accent);"
            >
              {notificationsStore.unreadCount > 99 ? "99+" : notificationsStore.unreadCount}
            </span>
          {/if}
        </button>
      </div>
    {/if}

    {#if !isReader && !appReleaseStore.promptOpen && appReleaseStore.shouldShowBanner && appReleaseStore.release}
      <div
        class="sticky top-11 z-20 flex flex-wrap items-center justify-between gap-3 border-b px-4 py-2.5 text-xs backdrop-blur-md md:top-0"
        style="background-color: color-mix(in srgb, var(--surface) 92%, var(--accent-soft)); border-color: var(--border);"
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
            class="inline-flex items-center gap-1.5 rounded-lg px-3 py-1 font-semibold text-white transition-opacity hover:opacity-90"
            style="background-color: var(--accent);"
          >
            <Download size={13} />
            <span>{t.updateDownloadAndInstall}</span>
          </button>
          <button
            type="button"
            onclick={() => appReleaseStore.openReleasePage()}
            class="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1 font-medium transition-opacity hover:opacity-85"
            style="border-color: var(--border); background-color: var(--surface);"
          >
            <ExternalLink size={12} />
            <span>{t.viewRelease}</span>
          </button>
          <button
            type="button"
            onclick={() => appReleaseStore.dismissPrompt()}
            class="rounded-lg p-1 opacity-70 transition-opacity hover:opacity-100"
            aria-label={t.dismissUpdate}
            title={t.dismissUpdate}
          >
            <X size={14} />
          </button>
        </div>
      </div>
    {/if}

    {#if router.current.name === "home"}
      <Home />
    {:else if router.current.name === "bookmarks"}
      <Bookmarks />
    {:else if router.current.name === "manga"}
      <Manga id={router.current.id} />
    {:else if router.current.name === "reader"}
      <Reader
        mangaId={router.current.mangaId}
        chapterUrl={router.current.chapterUrl}
        initialScrollRatio={router.current.scrollRatio ?? 0}
        initialPageIndex={router.current.pageIndex ?? 0}
      />
    {:else if router.current.name === "history"}
      <History />
    {:else if router.current.name === "settings"}
      <Settings />
    {/if}
  </main>

  {#if !isReader}
    <!-- Floating Scroll-to-Top for non-reader screens -->
    <FloatingControls
      screen={activeScreen}
      showScrollTop={showMainScrollTop}
      onScrollTop={scrollMainToTop}
      hasBottomNav={deviceEnv.isMobile}
    />

    <!-- Mobile Bottom Navigation: Home | Bookmarks | History (double-tap resume) | Settings -->
    <nav
      class="fixed right-0 bottom-0 left-0 z-20 flex items-center justify-around border-t py-1.5 backdrop-blur-md md:hidden"
      style="padding-bottom: max(0.375rem, var(--safe-bottom)); background-color: color-mix(in srgb, var(--surface) 95%, transparent); border-color: var(--border);"
    >
      <button
        type="button"
        onclick={() => router.goHome()}
        class="flex flex-col items-center gap-0.5 px-3 py-1 text-[11px] font-medium"
        style="color: {router.current.name === 'home' || router.current.name === 'manga'
          ? 'var(--accent)'
          : 'var(--text-muted)'};"
      >
        <HomeIcon size={19} />
        <span>{t.navHome}</span>
      </button>

      <button
        type="button"
        onclick={() => router.goBookmarks()}
        class="flex flex-col items-center gap-0.5 px-3 py-1 text-[11px] font-medium"
        style="color: {router.current.name === 'bookmarks'
          ? 'var(--accent)'
          : 'var(--text-muted)'};"
      >
        <Bookmark size={19} />
        <span>{t.navBookmarks}</span>
      </button>

      <button
        type="button"
        onclick={() => void router.goHistoryOrResume()}
        class="flex flex-col items-center gap-0.5 px-3 py-1 text-[11px] font-medium"
        style="color: {router.current.name === 'history' ? 'var(--accent)' : 'var(--text-muted)'};"
        title={t.historyResumeHint}
      >
        <Clock size={19} />
        <span>{t.navHistory}</span>
      </button>

      <button
        type="button"
        onclick={() => router.goSettings()}
        class="flex flex-col items-center gap-0.5 px-3 py-1 text-[11px] font-medium"
        style="color: {router.current.name === 'settings' ? 'var(--accent)' : 'var(--text-muted)'};"
      >
        <SettingsIcon size={19} />
        <span>{t.navSettings}</span>
      </button>
    </nav>
  {/if}

  <NotificationDrawer />

  {#if !isReader && appReleaseStore.promptOpen && appReleaseStore.release}
    <UpdatePromptModal />
  {/if}

  <ToastContainer />
</div>
