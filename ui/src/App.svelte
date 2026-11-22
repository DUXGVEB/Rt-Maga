<script lang="ts">
  import { Clock, Home as HomeIcon, Settings as SettingsIcon } from "lucide-svelte";
  import { onMount } from "svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import ToastContainer from "./lib/components/ToastContainer.svelte";
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
  });

  const isReader = $derived(router.current.name === "reader");
</script>

<div class="flex h-full w-full flex-col overflow-hidden md:flex-row">
  {#if !isReader}
    <Sidebar />
  {/if}

  <main class="relative flex-1 overflow-y-auto {isReader ? '' : 'pb-16 md:pb-0'}">
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
        <span>Home</span>
      </button>
      <button
        type="button"
        onclick={() => router.goHistory()}
        class="flex flex-col items-center gap-0.5 px-4 py-1 text-[11px] font-medium"
        style="color: {router.current.name === 'history' ? 'var(--accent)' : 'var(--text-muted)'};"
      >
        <Clock size={19} />
        <span>History</span>
      </button>
      <button
        type="button"
        onclick={() => router.goSettings()}
        class="flex flex-col items-center gap-0.5 px-4 py-1 text-[11px] font-medium"
        style="color: {router.current.name === 'settings' ? 'var(--accent)' : 'var(--text-muted)'};"
      >
        <SettingsIcon size={19} />
        <span>Settings</span>
      </button>
    </nav>
  {/if}

  <ToastContainer />
</div>
