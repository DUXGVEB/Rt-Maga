<script lang="ts">
  import { RotateCcw } from "lucide-svelte";
  import { imgProxyUrl } from "../api";
  import type { PageLoadMode } from "../readerLayout";
  import { settingsStore } from "../stores/settings.svelte";

  let {
    token,
    index,
    pageKey = "",
    loadMode = "idle",
    horizontal = false,
    dimBrightness = 1,
    snapPage = false,
    snapStopAlways = false,
    groupPageCount = 1,
    onImageLoaded,
  }: {
    token: string;
    index: number;
    pageKey?: string;
    loadMode?: PageLoadMode;
    horizontal?: boolean;
    dimBrightness?: number;
    snapPage?: boolean;
    snapStopAlways?: boolean;
    groupPageCount?: number;
    onImageLoaded?: (info: { index: number; width: number; height: number }) => void;
  } = $props();

  let containerEl = $state<HTMLDivElement | null>(null);
  let shouldLoad = $state(false);
  let loaded = $state(false);
  let failed = $state(false);
  let retryCount = $state(0);
  let knownAspect = $state<number | null>(null);
  let autoRetried = false;
  let retryTimer: number | null = null;
  let lastToken = "";

  $effect(() => {
    const currentToken = token;
    if (lastToken && lastToken !== currentToken) {
      if (retryTimer !== null) {
        window.clearTimeout(retryTimer);
        retryTimer = null;
      }
      loaded = false;
      failed = false;
      retryCount = 0;
      knownAspect = null;
      autoRetried = false;
    }
    lastToken = currentToken;
  });

  // Controlled preload window & memory release when far outside viewport.
  $effect(() => {
    const mode = loadMode;
    if (mode === "high" || mode === "preload") {
      shouldLoad = true;
    } else if (mode === "unload" && shouldLoad) {
      // Release decoded bitmap from DOM while retaining measured aspect ratio to prevent scroll jump.
      shouldLoad = false;
      loaded = false;
    }
  });

  // Fallback observer only for `idle` pages very close to viewport (tight margin).
  $effect(() => {
    if (shouldLoad || loadMode === "unload") return;
    if (!containerEl) return;

    const rootMargin = horizontal ? "0px 420px" : "420px 0px";
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          shouldLoad = true;
          observer.disconnect();
        }
      },
      { rootMargin }
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

  const filterStyle = $derived(
    dimBrightness < 0.999
      ? `filter: brightness(${dimBrightness}); transition: filter 160ms ease;`
      : "transition: filter 160ms ease;"
  );

  const horizontalBoxStyle = $derived(() => {
    if (!horizontal) return "";
    if (groupPageCount <= 1) {
      return "width: 100vw; max-width: 100vw;";
    }
    const pct = Math.max(12, Math.floor(96 / Math.max(1, groupPageCount)));
    return `max-width: ${pct}vw;`;
  });

  const snapStyle = $derived(() => {
    const parts: string[] = [];
    if (snapPage) {
      parts.push("scroll-snap-align: center;");
    }
    if (snapStopAlways) {
      parts.push("scroll-snap-stop: always;");
    }
    return parts.join(" ");
  });

  const placeholderStyle = $derived(() => {
    if (knownAspect && knownAspect > 0.1 && knownAspect < 10) {
      return `aspect-ratio: ${knownAspect}; background-color: rgba(128, 128, 140, 0.08);`;
    }
    return "background-color: rgba(128, 128, 140, 0.10);";
  });

  function handleLoad(e: Event) {
    const img = e.currentTarget as HTMLImageElement | null;
    if (img && img.naturalWidth === 0) {
      handleError();
      return;
    }
    if (img && img.naturalWidth > 0 && img.naturalHeight > 0) {
      knownAspect = Number((img.naturalWidth / img.naturalHeight).toFixed(4));
      onImageLoaded?.({
        index,
        width: img.naturalWidth,
        height: img.naturalHeight,
      });
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

<div
  bind:this={containerEl}
  data-page-key={pageKey}
  data-page-idx={index}
  class={horizontal
    ? "relative flex h-full shrink-0 items-center justify-center"
    : "relative flex w-full justify-center"}
  style="{filterStyle} {horizontalBoxStyle()} {snapStyle()}"
>
  {#if failed}
    <div
      class="flex flex-col items-center justify-center gap-3 rounded-lg border px-4 py-8 text-center {horizontal
        ? 'h-[72dvh] w-[62vw] max-w-[420px]'
        : 'min-h-[260px] w-full'}"
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
        class="animate-pulse {horizontal
          ? 'h-[78dvh] aspect-[5/7] max-w-full rounded'
          : 'aspect-[5/7] w-full'}"
        style={placeholderStyle()}
      ></div>
    {/if}
    <img
      {src}
      alt={settingsStore.t.pageAlt(index + 1)}
      decoding="async"
      fetchpriority={loadMode === "high" ? "high" : "auto"}
      onload={handleLoad}
      onerror={handleError}
      class={horizontal
        ? "block max-h-[calc(100dvh-2.5rem)] w-auto max-w-full object-contain select-none"
        : "block h-auto w-full select-none"}
      class:hidden={!loaded}
    />
  {:else}
    <div
      class={horizontal
        ? "h-[78dvh] aspect-[5/7] max-w-full rounded"
        : "aspect-[5/7] w-full"}
      style={placeholderStyle()}
    ></div>
  {/if}
</div>
