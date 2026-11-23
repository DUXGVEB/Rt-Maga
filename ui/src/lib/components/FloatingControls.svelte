<script lang="ts">
  import { ArrowUp, Monitor, RotateCw, SlidersHorizontal, Smartphone } from "lucide-svelte";
  import {
    computeFloatingBounds,
    resolveFloatingPositions,
    type FloatingControlRequest,
  } from "../floatingLayout";
  import { deviceEnv } from "../stores/deviceEnv.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import type { FloatingScreenKey, OrientationLockMode } from "../types";

  let {
    screen,
    showScrollTop = false,
    onScrollTop,
    onToggleReaderSettings,
    readerSettingsActive = false,
    onCycleOrientationLock,
    orientationLock = "auto",
    hasBottomNav = false,
    inReader = false,
    dimmed = false,
  }: {
    screen: FloatingScreenKey;
    showScrollTop?: boolean;
    onScrollTop?: () => void;
    onToggleReaderSettings?: () => void;
    readerSettingsActive?: boolean;
    onCycleOrientationLock?: () => void;
    orientationLock?: OrientationLockMode;
    hasBottomNav?: boolean;
    inReader?: boolean;
    dimmed?: boolean;
  } = $props();

  const floatingCfg = $derived(settingsStore.value.floating);
  const t = $derived(settingsStore.t);

  const allowScrollTop = $derived(
    showScrollTop &&
      Boolean(onScrollTop) &&
      floatingCfg.scrollToTopScreens.includes(screen)
  );

  const allowReaderSettings = $derived(
    inReader && floatingCfg.showReaderSettings && Boolean(onToggleReaderSettings)
  );

  const allowOrientationLock = $derived(
    inReader &&
      deviceEnv.isMobile &&
      floatingCfg.showOrientationLock &&
      Boolean(onCycleOrientationLock)
  );

  const resolvedMap = $derived(() => {
    const requests: FloatingControlRequest[] = [];
    if (allowOrientationLock) {
      requests.push({
        id: "orientationLock",
        position: floatingCfg.orientationLock,
      });
    }
    if (allowReaderSettings) {
      requests.push({
        id: "readerSettings",
        position: floatingCfg.readerSettings,
      });
    }
    if (allowScrollTop) {
      requests.push({
        id: "scrollToTop",
        position: floatingCfg.scrollToTop,
      });
    }

    const bounds = computeFloatingBounds(
      deviceEnv.viewportWidth,
      deviceEnv.viewportHeight,
      deviceEnv.info.insets,
      {
        hasBottomNav,
        topToolbarHeightPx: inReader ? 58 : 52,
      }
    );
    const placed = resolveFloatingPositions(requests, bounds);
    return new Map(placed.map((p) => [p.id, p]));
  });

  const effectiveOpacity = $derived(
    inReader && floatingCfg.autoDimInReader && dimmed && !readerSettingsActive ? 0.38 : 0.94
  );

  function styleFor(id: string): string {
    const pos = resolvedMap().get(id);
    if (!pos) return "display: none;";
    const edgeCss = pos.edge === "left" ? `left: ${pos.offsetPx}px;` : `right: ${pos.offsetPx}px;`;
    return `top: ${pos.topPx}px; ${edgeCss} width: ${pos.sizePx}px; height: ${pos.sizePx}px; opacity: ${effectiveOpacity};`;
  }

  const orientationTitle = $derived(() => {
    if (orientationLock === "portrait") return t.orientationPortrait;
    if (orientationLock === "landscape") return t.orientationLandscape;
    return t.orientationAuto;
  });
</script>

{#if allowOrientationLock}
  <button
    type="button"
    onclick={(e) => {
      e.stopPropagation();
      onCycleOrientationLock?.();
    }}
    title={orientationTitle()}
    aria-label={orientationTitle()}
    class="fixed z-30 flex items-center justify-center rounded-xl border shadow-sm backdrop-blur-md transition-all duration-200 hover:scale-105 hover:opacity-100 active:scale-95"
    style="{styleFor('orientationLock')} background-color: color-mix(in srgb, var(--surface) 78%, transparent); border-color: color-mix(in srgb, var(--text) 18%, transparent); color: {orientationLock !== 'auto'
      ? 'var(--accent)'
      : 'var(--text)'};"
  >
    {#if orientationLock === "portrait"}
      <Smartphone size={17} strokeWidth={1.9} />
    {:else if orientationLock === "landscape"}
      <Monitor size={17} strokeWidth={1.9} />
    {:else}
      <RotateCw size={17} strokeWidth={1.9} />
    {/if}
  </button>
{/if}

{#if allowReaderSettings}
  <button
    type="button"
    onclick={(e) => {
      e.stopPropagation();
      onToggleReaderSettings?.();
    }}
    title={t.quickSettingsTooltip}
    aria-label={t.quickSettingsTooltip}
    class="fixed z-30 flex items-center justify-center rounded-xl border shadow-sm backdrop-blur-md transition-all duration-200 hover:scale-105 hover:opacity-100 active:scale-95"
    style="{styleFor('readerSettings')} background-color: color-mix(in srgb, var(--surface) 78%, transparent); border-color: {readerSettingsActive
      ? 'var(--accent)'
      : 'color-mix(in srgb, var(--text) 18%, transparent)'}; color: {readerSettingsActive
      ? 'var(--accent)'
      : 'var(--text)'};"
  >
    <SlidersHorizontal size={17} strokeWidth={1.9} />
  </button>
{/if}

{#if allowScrollTop}
  <button
    type="button"
    onclick={(e) => {
      e.stopPropagation();
      onScrollTop?.();
    }}
    title={t.scrollToTopLabel}
    aria-label={t.scrollToTopLabel}
    class="fixed z-30 flex items-center justify-center rounded-xl border shadow-sm backdrop-blur-md transition-all duration-200 hover:scale-105 hover:opacity-100 active:scale-95"
    style="{styleFor('scrollToTop')} background-color: color-mix(in srgb, var(--surface) 78%, transparent); border-color: color-mix(in srgb, var(--text) 18%, transparent); color: var(--text);"
  >
    <ArrowUp size={18} strokeWidth={2} />
  </button>
{/if}
