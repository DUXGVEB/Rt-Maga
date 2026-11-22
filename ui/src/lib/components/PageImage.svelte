<script lang="ts">
  import { RotateCcw } from "lucide-svelte";
  import { imgProxyUrl } from "../api";
  import { settingsStore } from "../stores/settings.svelte";

  let {
    token,
    index,
    preloadMarginPx = 1800,
    eager = false,
  }: {
    token: string;
    index: number;
    preloadMarginPx?: number;
    eager?: boolean;
  } = $props();

  let containerEl = $state<HTMLDivElement | null>(null);
  let shouldLoad = $state(false);
  let loaded = $state(false);
  let failed = $state(false);
  let retryCount = $state(0);
  let autoRetried = false;
  let retryTimer: number | null = null;

  $effect(() => {
    if (shouldLoad) return;
    if (eager) {
      shouldLoad = true;
      return;
    }
    if (!containerEl) return;

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          shouldLoad = true;
          observer.disconnect();
        }
      },
      { rootMargin: `${preloadMarginPx}px 0px` }
    );

    observer.observe(containerEl);
    return () => observer.disconnect();
  });

  $effect(() => {
    return () => {
      if (retryTimer !== null) {
        window.clearTimeout(retryTimer);
        retryTimer = null;
      }
    };
  });

  const src = $derived(
    retryCount > 0 ? `${imgProxyUrl(token, true)}&t=${retryCount}` : imgProxyUrl(token, false)
  );

  function handleLoad(e: Event) {
    const img = e.currentTarget as HTMLImageElement | null;
    if (img && img.naturalWidth === 0) {
      handleError();
      return;
    }
    loaded = true;
    failed = false;
  }

  function handleError() {
    if (!autoRetried) {
      autoRetried = true;
      loaded = false;
      retryTimer = window.setTimeout(() => {
        retryTimer = null;
        retryCount += 1;
      }, 600);
      return;
    }
    failed = true;
  }

  function retry() {
    if (retryTimer !== null) {
      window.clearTimeout(retryTimer);
      retryTimer = null;
    }
    failed = false;
    loaded = false;
    retryCount += 1;
  }
</script>

<div bind:this={containerEl} class="cv-auto relative flex w-full justify-center">
  {#if failed}
    <div
      class="flex min-h-[260px] w-full flex-col items-center justify-center gap-3 rounded-lg border px-4 py-8 text-center"
      style="background-color: var(--surface); border-color: var(--border);"
    >
      <span class="text-xs" style="color: var(--text-muted);">
        {settingsStore.t.failedPage(index + 1)}
      </span>
      <button
        type="button"
        onclick={retry}
        class="inline-flex items-center gap-1.5 rounded-lg px-3.5 py-1.5 text-xs font-medium text-white"
        style="background-color: var(--accent);"
      >
        <RotateCcw size={13} />
        {settingsStore.t.retry}
      </button>
    </div>
  {:else if shouldLoad}
    {#if !loaded}
      <div
        class="aspect-[5/7] w-full animate-pulse"
        style="background-color: rgba(128, 128, 140, 0.14);"
      ></div>
    {/if}
    <img
      {src}
      alt={settingsStore.t.pageAlt(index + 1)}
      decoding="async"
      onload={handleLoad}
      onerror={handleError}
      class="block h-auto w-full select-none"
      class:hidden={!loaded}
    />
  {:else}
    <div
      class="aspect-[5/7] w-full"
      style="background-color: rgba(128, 128, 140, 0.08);"
    ></div>
  {/if}
</div>
