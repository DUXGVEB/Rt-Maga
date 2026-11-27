<script lang="ts">
  import {
    Bell,
    Bookmark,
    Clock,
    Compass,
    Dices,
    Home,
    Maximize,
    Minimize,
    Moon,
    PanelLeftClose,
    PanelLeftOpen,
    RefreshCw,
    Settings,
    Sun,
  } from "lucide-svelte";
  import logoUrl from "../../assets/logo.png";
  import { appReleaseStore } from "../stores/appRelease.svelte";
  import { deviceEnv } from "../stores/deviceEnv.svelte";
  import { notificationsStore } from "../stores/notifications.svelte";
  import { randomMangaStore } from "../stores/randomManga.svelte";
  import { router } from "../stores/router.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { updatesStore } from "../stores/updates.svelte";
  import { normalizeNavOrder } from "../themeAccent";

  const COLLAPSE_STORAGE_KEY = "rtmaga.sidebar.collapsed.v1";

  let userCollapsed = $state(
    typeof window !== "undefined" && window.localStorage?.getItem(COLLAPSE_STORAGE_KEY) === "1"
  );

  const isDark = $derived(settingsStore.resolvedTheme === "dark");
  const t = $derived(settingsStore.t);
  const checkingAny = $derived(updatesStore.checking || appReleaseStore.checking);
  const navOrder = $derived(normalizeNavOrder(settingsStore.value.ui.navOrder));

  // 768px–1024px: automatic compact vertical rail to maximize reading/browsing height.
  // > 1024px: collapsible full sidebar (user toggleable).
  const isRailMode = $derived(
    deviceEnv.layoutTier === "rail" || (deviceEnv.layoutTier === "desktop" && userCollapsed)
  );
  const canToggleCollapse = $derived(deviceEnv.layoutTier === "desktop");

  function toggleCollapse() {
    userCollapsed = !userCollapsed;
    try {
      window.localStorage?.setItem(COLLAPSE_STORAGE_KEY, userCollapsed ? "1" : "0");
    } catch {
      // Ignore storage failures in restricted WebViews.
    }
  }

  function toggleTheme() {
    settingsStore.update((d) => {
      d.theme = isDark ? "light" : "dark";
    });
  }
</script>

<aside
  class="sidebar-shell glass-chrome hidden h-full shrink-0 flex-col justify-between overflow-y-auto border-r select-none md:flex"
  class:sidebar-rail={isRailMode}
  class:sidebar-expanded={!isRailMode}
  style="padding-top: calc(1rem + var(--safe-top)); padding-bottom: calc(1rem + var(--safe-bottom)); padding-left: max({isRailMode
    ? '0.5rem'
    : '0.75rem'}, var(--safe-left)); padding-right: {isRailMode ? '0.5rem' : '0.75rem'}; border-color: var(--border);"
>
  <div class="sidebar-top flex flex-col gap-5">
    <!-- Brand + Collapse / Bell Controls -->
    <div
      class="flex items-center gap-1.5"
      class:flex-col={isRailMode}
      class:justify-between={!isRailMode}
      class:px-1={!isRailMode}
    >
      <button
        type="button"
        onclick={() => router.goHome()}
        class="gpu-press flex min-w-0 items-center gap-2.5 rounded-xl p-1.5 text-left"
        title="RtMaga"
        aria-label="RtMaga"
      >
        <img src={logoUrl} alt="RtMaga" class="h-8 w-8 shrink-0 rounded-xl shadow-sm" />
        {#if !isRailMode}
          <div class="min-w-0 leading-tight">
            <div class="truncate text-sm font-bold tracking-tight">RtMaga</div>
            <div class="truncate text-[10px] font-medium" style="color: var(--text-muted);">
              Offline Reader
            </div>
          </div>
        {/if}
      </button>

      <div class="flex items-center gap-1" class:flex-col={isRailMode}>
        <button
          type="button"
          onclick={() => notificationsStore.toggleDrawer()}
          class="gpu-press relative flex h-10 w-10 shrink-0 items-center justify-center rounded-xl"
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
              class="absolute top-1 right-1 flex h-4 min-w-4 items-center justify-center rounded-full px-1 text-[10px] font-bold"
              style="background-color: var(--accent); color: var(--accent-fg);"
            >
              {notificationsStore.unreadCount > 99 ? "99+" : notificationsStore.unreadCount}
            </span>
          {/if}
        </button>

        {#if canToggleCollapse}
          <button
            type="button"
            onclick={toggleCollapse}
            class="gpu-press flex h-10 w-10 shrink-0 items-center justify-center rounded-xl"
            style="color: var(--text-muted);"
            title={userCollapsed ? "Expand sidebar" : "Collapse sidebar"}
            aria-label={userCollapsed ? "Expand sidebar" : "Collapse sidebar"}
          >
            {#if userCollapsed}
              <PanelLeftOpen size={17} />
            {:else}
              <PanelLeftClose size={17} />
            {/if}
          </button>
        {/if}
      </div>
    </div>

    <!-- Navigation: user-ordered destinations -->
    <nav class="flex flex-col gap-1.5" aria-label={t.navOrderSection}>
      {#each navOrder as dest (dest)}
        {#if dest === "home"}
          {@const active = router.current.name === "home" || router.current.name === "manga"}
          <button
            type="button"
            onclick={() => router.goHome()}
            class="nav-item gpu-press"
            class:nav-item-rail={isRailMode}
            class:active
            aria-current={active ? "page" : undefined}
            title={t.navHome}
            aria-label={t.navHome}
          >
            <span class="nav-indicator" aria-hidden="true"></span>
            <Home size={19} strokeWidth={active ? 2.2 : 1.85} />
            {#if !isRailMode}
              <span class="truncate">{t.navHome}</span>
            {/if}
            {#if updatesStore.newCount > 0}
              <span
                class={isRailMode
                  ? "absolute top-1.5 right-1.5 flex h-4 min-w-4 items-center justify-center rounded-full px-1 text-[9px] font-bold"
                  : "ml-auto rounded-full px-2 py-0.5 text-[11px] font-semibold"}
                style="background-color: var(--accent); color: var(--accent-fg);"
              >
                {updatesStore.newCount}
              </span>
            {/if}
          </button>
        {:else if dest === "sources"}
          {@const active = router.current.name === "sources" || router.current.name === "browse"}
          <button
            type="button"
            onclick={() => router.goSources()}
            class="nav-item gpu-press"
            class:nav-item-rail={isRailMode}
            class:active
            aria-current={active ? "page" : undefined}
            title={t.navSources}
            aria-label={t.navSources}
          >
            <span class="nav-indicator" aria-hidden="true"></span>
            <Compass size={19} strokeWidth={active ? 2.2 : 1.85} />
            {#if !isRailMode}
              <span class="truncate">{t.navSources}</span>
            {/if}
          </button>
        {:else if dest === "bookmarks"}
          {@const active = router.current.name === "bookmarks"}
          <button
            type="button"
            onclick={() => router.goBookmarks()}
            class="nav-item gpu-press"
            class:nav-item-rail={isRailMode}
            class:active
            aria-current={active ? "page" : undefined}
            title={t.navBookmarks}
            aria-label={t.navBookmarks}
          >
            <span class="nav-indicator" aria-hidden="true"></span>
            <Bookmark size={19} strokeWidth={active ? 2.2 : 1.85} />
            {#if !isRailMode}
              <span class="truncate">{t.navBookmarks}</span>
            {/if}
          </button>
        {:else if dest === "history"}
          {@const active = router.current.name === "history"}
          <button
            type="button"
            onclick={() => void router.goHistoryOrResume()}
            class="nav-item gpu-press"
            class:nav-item-rail={isRailMode}
            class:active
            aria-current={active ? "page" : undefined}
            title={isRailMode ? `${t.navHistory} — ${t.historyResumeHint}` : t.historyResumeHint}
            aria-label={t.navHistory}
          >
            <span class="nav-indicator" aria-hidden="true"></span>
            <Clock size={19} strokeWidth={active ? 2.2 : 1.85} />
            {#if !isRailMode}
              <span class="truncate">{t.navHistory}</span>
            {/if}
          </button>
        {:else}
          {@const active = router.current.name === "settings"}
          <button
            type="button"
            onclick={() => router.goSettings()}
            class="nav-item gpu-press"
            class:nav-item-rail={isRailMode}
            class:active
            aria-current={active ? "page" : undefined}
            title={t.navSettings}
            aria-label={t.navSettings}
          >
            <span class="nav-indicator" aria-hidden="true"></span>
            <Settings size={19} strokeWidth={active ? 2.2 : 1.85} />
            {#if !isRailMode}
              <span class="truncate">{t.navSettings}</span>
            {/if}
          </button>
        {/if}
      {/each}

      <!-- Random Manga 3D Depth Carousel quick launcher -->
      <div class="my-1 border-t" style="border-color: var(--border);"></div>
      <button
        type="button"
        onclick={() => randomMangaStore.open(router.current.name === "bookmarks" ? "bookmarks" : "all")}
        class="nav-item gpu-press"
        class:nav-item-rail={isRailMode}
        class:active={randomMangaStore.isOpen}
        title={t.randomMangaTrigger}
        aria-label={t.randomMangaTrigger}
      >
        <span class="nav-indicator" aria-hidden="true"></span>
        <Dices size={19} strokeWidth={randomMangaStore.isOpen ? 2.2 : 1.85} />
        {#if !isRailMode}
          <span class="truncate">{t.randomMangaTrigger}</span>
        {/if}
      </button>
    </nav>
  </div>

  <!-- Footer controls -->
  <div class="flex flex-col gap-1 border-t pt-3" style="border-color: var(--border);">
    <button
      type="button"
      onclick={() => updatesStore.checkNow()}
      disabled={checkingAny}
      class="nav-item gpu-press text-xs"
      class:nav-item-rail={isRailMode}
      title={t.checkUpdatesTooltip}
      aria-label={t.checkUpdates}
    >
      <RefreshCw size={16} class={checkingAny ? "animate-spin" : ""} />
      {#if !isRailMode}
        <span class="truncate">
          {#if updatesStore.checking && updatesStore.progress}
            {t.checkingProgress(updatesStore.progress.done, updatesStore.progress.total)}
          {:else if checkingAny}
            {t.checking}
          {:else}
            {t.checkUpdates}
          {/if}
        </span>
      {/if}
    </button>

    {#if !deviceEnv.isAndroid}
      <button
        type="button"
        onclick={() => void deviceEnv.toggleDesktopFullscreen()}
        class="nav-item gpu-press text-xs"
        class:nav-item-rail={isRailMode}
        title={deviceEnv.desktopFullscreen ? t.appFullscreenExit : t.appFullscreenEnter}
        aria-label={deviceEnv.desktopFullscreen ? t.appFullscreenExit : t.appFullscreenEnter}
      >
        {#if deviceEnv.desktopFullscreen}
          <Minimize size={16} />
          {#if !isRailMode}
            <span class="truncate">{t.appFullscreenExit}</span>
          {/if}
        {:else}
          <Maximize size={16} />
          {#if !isRailMode}
            <span class="truncate">{t.appFullscreenEnter}</span>
          {/if}
        {/if}
      </button>
    {/if}

    <button
      type="button"
      onclick={toggleTheme}
      class="nav-item gpu-press text-xs"
      class:nav-item-rail={isRailMode}
      title={isDark ? t.lightMode : t.darkMode}
      aria-label={isDark ? t.lightMode : t.darkMode}
    >
      {#if isDark}
        <Sun size={16} />
        {#if !isRailMode}
          <span class="truncate">{t.lightMode}</span>
        {/if}
      {:else}
        <Moon size={16} />
        {#if !isRailMode}
          <span class="truncate">{t.darkMode}</span>
        {/if}
      {/if}
    </button>
  </div>
</aside>

<style>
  .sidebar-expanded {
    width: 15rem;
  }

  .sidebar-rail {
    width: 4.5rem;
  }

  .nav-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 0.7rem;
    width: 100%;
    min-height: 2.75rem;
    padding: 0.55rem 0.75rem;
    border-radius: 0.75rem;
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--text-muted);
    background: transparent;
    border: none;
    cursor: pointer;
    text-align: left;
  }

  .nav-item-rail {
    justify-content: center;
    padding: 0.55rem;
    gap: 0;
  }

  .nav-indicator {
    position: absolute;
    left: 0;
    top: 50%;
    width: 3px;
    height: 1.25rem;
    border-radius: 999px;
    background-color: var(--accent);
    opacity: 0;
    transform: translate3d(0, -50%, 0) scaleY(0.4);
    transition:
      opacity var(--dur-fast, 150ms) var(--ease-out, ease),
      transform var(--dur-base, 210ms) var(--ease-spring, ease);
  }

  .nav-item:hover:not(:disabled) {
    background-color: var(--surface-hover);
    color: var(--text);
  }

  .nav-item.active {
    background-color: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }

  .nav-item.active .nav-indicator {
    opacity: 1;
    transform: translate3d(0, -50%, 0) scaleY(1);
  }

  .nav-item:disabled {
    opacity: 0.55;
    cursor: default;
  }

  @media (max-height: 500px) {
    .sidebar-shell {
      padding-top: calc(0.5rem + var(--safe-top)) !important;
      padding-bottom: calc(0.5rem + var(--safe-bottom)) !important;
    }

    .sidebar-top {
      gap: 0.6rem;
    }

    .nav-item {
      min-height: 2.35rem;
      padding: 0.35rem 0.55rem;
      font-size: 0.8rem;
    }
  }
</style>
