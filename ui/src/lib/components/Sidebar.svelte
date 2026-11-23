<script lang="ts">
  import {
    Bell,
    Bookmark,
    Clock,
    Home,
    Maximize,
    Minimize,
    Moon,
    RefreshCw,
    Settings,
    Sun,
  } from "lucide-svelte";
  import logoUrl from "../../assets/logo.png";
  import { appReleaseStore } from "../stores/appRelease.svelte";
  import { deviceEnv } from "../stores/deviceEnv.svelte";
  import { notificationsStore } from "../stores/notifications.svelte";
  import { router } from "../stores/router.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { updatesStore } from "../stores/updates.svelte";

  const isDark = $derived(settingsStore.resolvedTheme === "dark");
  const t = $derived(settingsStore.t);
  const checkingAny = $derived(updatesStore.checking || appReleaseStore.checking);

  function toggleTheme() {
    settingsStore.update((d) => {
      d.theme = isDark ? "light" : "dark";
    });
  }
</script>

<aside
  class="sidebar-shell hidden h-full w-56 shrink-0 flex-col justify-between overflow-y-auto border-r select-none md:flex"
  style="padding-top: calc(1.25rem + var(--safe-top)); padding-bottom: calc(1.25rem + var(--safe-bottom)); padding-left: max(0.75rem, var(--safe-left)); padding-right: 0.75rem; background-color: var(--surface); border-color: var(--border);"
>
  <div class="sidebar-top flex flex-col gap-6">
    <!-- Brand + Bell Notification Center -->
    <div class="flex items-center justify-between gap-1 px-1">
      <button
        type="button"
        onclick={() => router.goHome()}
        class="flex min-w-0 items-center gap-2.5 rounded-lg px-1.5 py-1.5 text-left transition-opacity hover:opacity-85"
      >
        <img src={logoUrl} alt="RtMaga" class="h-8 w-8 shrink-0 rounded-lg shadow-sm" />
        <div class="min-w-0 leading-tight">
          <div class="truncate text-base font-semibold tracking-tight">RtMaga</div>
          <div class="truncate text-[11px]" style="color: var(--text-muted);">
            {t.brandSubtitle}
          </div>
        </div>
      </button>

      <button
        type="button"
        onclick={() => notificationsStore.toggleDrawer()}
        class="relative shrink-0 rounded-lg p-2 transition-colors hover:brightness-95"
        style="background-color: {notificationsStore.drawerOpen
          ? 'var(--accent-soft)'
          : 'transparent'}; color: {notificationsStore.drawerOpen
          ? 'var(--accent)'
          : 'var(--text-muted)'};"
        title={t.notifCenterTitle}
        aria-label={t.notifCenterTitle}
      >
        <Bell size={18} />
        {#if notificationsStore.unreadCount > 0}
          <span
            class="absolute top-0.5 right-0.5 flex h-4 min-w-4 items-center justify-center rounded-full px-1 text-[10px] font-bold text-white"
            style="background-color: var(--accent);"
          >
            {notificationsStore.unreadCount > 99 ? "99+" : notificationsStore.unreadCount}
          </span>
        {/if}
      </button>
    </div>

    <!-- Navigation: Home | Bookmarks | History (double-tap resume) | Settings -->
    <nav class="flex flex-col gap-1">
      <button
        type="button"
        onclick={() => router.goHome()}
        class="nav-item"
        class:active={router.current.name === "home" ||
          router.current.name === "manga" ||
          router.current.name === "browse"}
      >
        <Home size={18} />
        <span>{t.navHome}</span>
        {#if updatesStore.newCount > 0}
          <span
            class="ml-auto rounded-full px-2 py-0.5 text-[11px] font-semibold text-white"
            style="background-color: var(--accent);"
          >
            {updatesStore.newCount}
          </span>
        {/if}
      </button>

      <button
        type="button"
        onclick={() => router.goBookmarks()}
        class="nav-item"
        class:active={router.current.name === "bookmarks"}
      >
        <Bookmark size={18} />
        <span>{t.navBookmarks}</span>
      </button>

      <button
        type="button"
        onclick={() => void router.goHistoryOrResume()}
        class="nav-item"
        class:active={router.current.name === "history"}
        title={t.historyResumeHint}
      >
        <Clock size={18} />
        <span>{t.navHistory}</span>
      </button>

      <button
        type="button"
        onclick={() => router.goSettings()}
        class="nav-item"
        class:active={router.current.name === "settings"}
      >
        <Settings size={18} />
        <span>{t.navSettings}</span>
      </button>
    </nav>
  </div>

  <!-- Footer controls -->
  <div class="flex flex-col gap-1.5 border-t pt-3" style="border-color: var(--border);">
    <button
      type="button"
      onclick={() => updatesStore.checkNow()}
      disabled={checkingAny}
      class="nav-item text-xs"
      title={t.checkUpdatesTooltip}
    >
      <RefreshCw size={15} class={checkingAny ? "animate-spin" : ""} />
      <span>
        {#if updatesStore.checking && updatesStore.progress}
          {t.checkingProgress(updatesStore.progress.done, updatesStore.progress.total)}
        {:else if checkingAny}
          {t.checking}
        {:else}
          {t.checkUpdates}
        {/if}
      </span>
    </button>

    {#if !deviceEnv.isAndroid}
      <button
        type="button"
        onclick={() => void deviceEnv.toggleDesktopFullscreen()}
        class="nav-item text-xs"
        title={deviceEnv.desktopFullscreen ? t.appFullscreenExit : t.appFullscreenEnter}
      >
        {#if deviceEnv.desktopFullscreen}
          <Minimize size={15} />
          <span>{t.appFullscreenExit}</span>
        {:else}
          <Maximize size={15} />
          <span>{t.appFullscreenEnter}</span>
        {/if}
      </button>
    {/if}

    <button type="button" onclick={toggleTheme} class="nav-item text-xs">
      {#if isDark}
        <Sun size={15} />
        <span>{t.lightMode}</span>
      {:else}
        <Moon size={15} />
        <span>{t.darkMode}</span>
      {/if}
    </button>
  </div>
</aside>

<style>
  .nav-item {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    width: 100%;
    padding: 0.55rem 0.75rem;
    border-radius: 0.6rem;
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--text-muted);
    background: transparent;
    border: none;
    cursor: pointer;
    transition: background-color 140ms ease, color 140ms ease;
    text-align: left;
  }

  .nav-item:hover:not(:disabled) {
    background-color: var(--surface-hover);
    color: var(--text);
  }

  .nav-item.active {
    background-color: var(--accent-soft);
    color: var(--accent);
  }

  .nav-item:disabled {
    opacity: 0.6;
    cursor: default;
  }

  @media (max-height: 500px) {
    .sidebar-shell {
      width: 12.5rem;
      padding-top: calc(0.6rem + var(--safe-top)) !important;
      padding-bottom: calc(0.6rem + var(--safe-bottom)) !important;
    }

    .sidebar-top {
      gap: 0.75rem;
    }

    .nav-item {
      padding: 0.4rem 0.6rem;
      font-size: 0.8rem;
    }
  }
</style>
