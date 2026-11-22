<script lang="ts">
  import {
    Clock,
    Download,
    ExternalLink,
    Home as HomeIcon,
    Settings as SettingsIcon,
    Sparkles,
    X,
  } from "lucide-svelte";
  import { onMount } from "svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import ToastContainer from "./lib/components/ToastContainer.svelte";
  import UpdatePromptModal from "./lib/components/UpdatePromptModal.svelte";
  import { appReleaseStore } from "./lib/stores/appRelease.svelte";
  import { router } from "./lib/stores/router.svelte";
  import { settingsStore } from "./lib/stores/settings.svelte";
  import { updatesStore } from "./lib/stores/updates.svelte";
  import History from "./pages/History.svelte";
  import Home from "./pages/Home.svelte";
  import Manga from "./pages/Manga.svelte";
  import Reader from "./pages/Reader.svelte";
  import Settings from "./pages/Settings.svelte";

  onMount(() => {
    void settingsStore.init();
    void updatesStore.init();
    void appReleaseStore.init();
  });

  const isReader = $derived(router.current.name === "reader");
  const t = $derived(settingsStore.t);
</script>

<div class="flex h-full w-full flex-col overflow-hidden md:flex-row">
  {#if !isReader}
    <Sidebar />
  {/if}

  <main class="relative flex-1 overflow-y-auto {isReader ? '' : 'pb-16 md:pb-0'}">
    {#if !isReader && !appReleaseStore.promptOpen && appReleaseStore.shouldShowBanner && appReleaseStore.release}
      <div
        class="sticky top-0 z-20 flex flex-wrap items-center justify-between gap-3 border-b px-4 py-2.5 text-xs backdrop-blur-md"
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
    {:else if router.current.name === "manga"}
      <Manga id={router.current.id} />
    {:else if router.current.name === "reader"}
      <Reader
        mangaId={router.current.mangaId}
        chapterUrl={router.current.chapterUrl}
        initialScrollRatio={router.current.scrollRatio ?? 0}
      />
    {:else if router.current.name === "history"}
      <History />
    {:else if router.current.name === "settings"}
      <Settings />
    {/if}
  </main>

  {#if !isReader}
    <nav
      class="fixed right-0 bottom-0 left-0 z-20 flex items-center justify-around border-t py-2 md:hidden"
      style="background-color: var(--surface); border-color: var(--border);"
    >
      <button
        type="button"
        onclick={() => router.goHome()}
        class="flex flex-col items-center gap-0.5 px-4 py-1 text-[11px] font-medium"
        style="color: {router.current.name === 'home' || router.current.name === 'manga'
          ? 'var(--accent)'
          : 'var(--text-muted)'};"
      >
        <HomeIcon size={19} />
        <span>{settingsStore.t.navHome}</span>
      </button>
      <button
        type="button"
        onclick={() => router.goHistory()}
        class="flex flex-col items-center gap-0.5 px-4 py-1 text-[11px] font-medium"
        style="color: {router.current.name === 'history' ? 'var(--accent)' : 'var(--text-muted)'};"
      >
        <Clock size={19} />
        <span>{settingsStore.t.navHistory}</span>
      </button>
      <button
        type="button"
        onclick={() => router.goSettings()}
        class="flex flex-col items-center gap-0.5 px-4 py-1 text-[11px] font-medium"
        style="color: {router.current.name === 'settings' ? 'var(--accent)' : 'var(--text-muted)'};"
      >
        <SettingsIcon size={19} />
        <span>{settingsStore.t.navSettings}</span>
      </button>
    </nav>
  {/if}

  {#if !isReader && appReleaseStore.promptOpen && appReleaseStore.release}
    <UpdatePromptModal />
  {/if}

  <ToastContainer />
</div>
