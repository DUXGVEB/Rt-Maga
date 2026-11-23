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

  const LONG_PRESS_MS = 500;
  const MOVE_CANCEL_PX = 10;

  type FloatingBtnId = "orientationLock" | "readerSettings" | "scrollToTop";

  let {
    screen,
    showScrollTop = false,
    onScrollTop,
    onLongPressScrollTop,
    onToggleReaderSettings,
    onLongPressReaderSettings,
    readerSettingsActive = false,
    onCycleOrientationLock,
    onLongPressOrientationLock,
    orientationLock = "auto",
    hasBottomNav = false,
    inReader = false,
    dimmed = false,
    fadedOut = false,
  }: {
    screen: FloatingScreenKey;
    showScrollTop?: boolean;
    onScrollTop?: () => void;
    onLongPressScrollTop?: () => void;
    onToggleReaderSettings?: () => void;
    onLongPressReaderSettings?: () => void;
    readerSettingsActive?: boolean;
    onCycleOrientationLock?: () => void;
    onLongPressOrientationLock?: () => void;
    orientationLock?: OrientationLockMode;
    hasBottomNav?: boolean;
    inReader?: boolean;
    dimmed?: boolean;
    fadedOut?: boolean;
  } = $props();

  let holdingId = $state<FloatingBtnId | null>(null);
  let holdTimer: number | null = null;
  let pressStartX = 0;
  let pressStartY = 0;
  let longPressFiredId: FloatingBtnId | null = null;
  let longPressFiredAt = 0;

  const floatingCfg = $derived(settingsStore.value.floating);
  const t = $derived(settingsStore.t);

  function cancelHold() {
    if (holdTimer !== null) {
      window.clearTimeout(holdTimer);
      holdTimer = null;
    }
    holdingId = null;
  }

  $effect(() => {
    return () => cancelHold();
  });

  function startHold(id: FloatingBtnId, e: PointerEvent, onLongPress?: () => void) {
    e.stopPropagation();
    if (!onLongPress) return;
    if (e.pointerType === "mouse" && e.button !== 0) return;
    cancelHold();
    holdingId = id;
    pressStartX = e.clientX;
    pressStartY = e.clientY;
    holdTimer = window.setTimeout(() => {
      holdTimer = null;
      holdingId = null;
      longPressFiredId = id;
      longPressFiredAt = Date.now();
      try {
        navigator.vibrate?.(18);
      } catch {
        // Ignore environments without vibration API.
      }
      onLongPress();
    }, LONG_PRESS_MS);
  }

  function moveHold(e: PointerEvent) {
    if (!holdingId) return;
    if (Math.hypot(e.clientX - pressStartX, e.clientY - pressStartY) > MOVE_CANCEL_PX) {
      cancelHold();
    }
  }

  function endHold(e: PointerEvent) {
    e.stopPropagation();
    cancelHold();
  }

  function handleClick(id: FloatingBtnId, e: MouseEvent, onShortPress?: () => void) {
    e.stopPropagation();
    cancelHold();
    if (longPressFiredId === id && Date.now() - longPressFiredAt < 650) {
      e.preventDefault();
      longPressFiredId = null;
      return;
    }
    longPressFiredId = null;
    onShortPress?.();
  }

  function handleKeydown(e: KeyboardEvent, onLongPress?: () => void) {
    if (!onLongPress) return;
    if (e.key === "ContextMenu" || (e.shiftKey && e.key === "Enter")) {
      e.preventDefault();
      e.stopPropagation();
      onLongPress();
    }
  }

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

  const isHiddenByFade = $derived(
    inReader && fadedOut && !readerSettingsActive && !holdingId
  );

  const effectiveOpacity = $derived(
    isHiddenByFade
      ? 0
      : inReader && floatingCfg.autoDimInReader && dimmed && !readerSettingsActive && !holdingId
        ? 0.38
        : 0.94
  );

  function styleFor(id: string): string {
    const pos = resolvedMap().get(id);
    if (!pos) return "display: none;";
    const edgeCss = pos.edge === "left" ? `left: ${pos.offsetPx}px;` : `right: ${pos.offsetPx}px;`;
    const pointerCss = isHiddenByFade ? "pointer-events: none;" : "";
    return `top: ${pos.topPx}px; ${edgeCss} width: ${pos.sizePx}px; height: ${pos.sizePx}px; opacity: ${effectiveOpacity}; ${pointerCss} -webkit-touch-callout: none; -webkit-user-select: none;`;
  }

  const orientationTitle = $derived(() => {
    const base =
      orientationLock === "portrait"
        ? t.orientationPortrait
        : orientationLock === "landscape"
          ? t.orientationLandscape
          : t.orientationAuto;
    return onLongPressOrientationLock ? `${base} (${t.longPressConfigHint})` : base;
  });

  const readerSettingsTitle = $derived(() =>
    onLongPressReaderSettings
      ? `${t.quickSettingsTooltip} (${t.longPressConfigHint})`
      : t.quickSettingsTooltip
  );

  const scrollTopTitle = $derived(() =>
    onLongPressScrollTop
      ? `${t.scrollToTopLabel} (${t.longPressConfigHint})`
      : t.scrollToTopLabel
  );
</script>

{#if allowOrientationLock}
  <button
    type="button"
    onpointerdown={(e) => startHold("orientationLock", e, onLongPressOrientationLock)}
    onpointermove={moveHold}
    onpointerup={endHold}
    onpointercancel={endHold}
    onpointerleave={cancelHold}
    oncontextmenu={(e) => {
      if (onLongPressOrientationLock) {
        e.preventDefault();
        e.stopPropagation();
      }
    }}
    onkeydown={(e) => handleKeydown(e, onLongPressOrientationLock)}
    onclick={(e) => handleClick("orientationLock", e, onCycleOrientationLock)}
    title={orientationTitle()}
    aria-label={orientationTitle()}
    class="fixed z-30 flex touch-manipulation items-center justify-center rounded-xl border shadow-sm backdrop-blur-md transition-all duration-200 select-none hover:scale-105 hover:opacity-100 active:scale-95"
    style="{styleFor('orientationLock')} background-color: color-mix(in srgb, var(--surface) 78%, transparent); border-color: {holdingId === 'orientationLock' || orientationLock !== 'auto'
      ? 'var(--accent)'
      : 'color-mix(in srgb, var(--text) 18%, transparent)'}; color: {orientationLock !== 'auto' || holdingId === 'orientationLock'
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
    onpointerdown={(e) => startHold("readerSettings", e, onLongPressReaderSettings)}
    onpointermove={moveHold}
    onpointerup={endHold}
    onpointercancel={endHold}
    onpointerleave={cancelHold}
    oncontextmenu={(e) => {
      if (onLongPressReaderSettings) {
        e.preventDefault();
        e.stopPropagation();
      }
    }}
    onkeydown={(e) => handleKeydown(e, onLongPressReaderSettings)}
    onclick={(e) => handleClick("readerSettings", e, onToggleReaderSettings)}
    title={readerSettingsTitle()}
    aria-label={readerSettingsTitle()}
    class="fixed z-30 flex touch-manipulation items-center justify-center rounded-xl border shadow-sm backdrop-blur-md transition-all duration-200 select-none hover:scale-105 hover:opacity-100 active:scale-95"
    style="{styleFor('readerSettings')} background-color: color-mix(in srgb, var(--surface) 78%, transparent); border-color: {readerSettingsActive || holdingId === 'readerSettings'
      ? 'var(--accent)'
      : 'color-mix(in srgb, var(--text) 18%, transparent)'}; color: {readerSettingsActive || holdingId === 'readerSettings'
      ? 'var(--accent)'
      : 'var(--text)'};"
  >
    <SlidersHorizontal size={17} strokeWidth={1.9} />
  </button>
{/if}

{#if allowScrollTop}
  <button
    type="button"
    onpointerdown={(e) => startHold("scrollToTop", e, onLongPressScrollTop)}
    onpointermove={moveHold}
    onpointerup={endHold}
    onpointercancel={endHold}
    onpointerleave={cancelHold}
    oncontextmenu={(e) => {
      if (onLongPressScrollTop) {
        e.preventDefault();
        e.stopPropagation();
      }
    }}
    onkeydown={(e) => handleKeydown(e, onLongPressScrollTop)}
    onclick={(e) => handleClick("scrollToTop", e, onScrollTop)}
    title={scrollTopTitle()}
    aria-label={scrollTopTitle()}
    class="fixed z-30 flex touch-manipulation items-center justify-center rounded-xl border shadow-sm backdrop-blur-md transition-all duration-200 select-none hover:scale-105 hover:opacity-100 active:scale-95"
    style="{styleFor('scrollToTop')} background-color: color-mix(in srgb, var(--surface) 78%, transparent); border-color: {holdingId === 'scrollToTop'
      ? 'var(--accent)'
      : 'color-mix(in srgb, var(--text) 18%, transparent)'}; color: {holdingId === 'scrollToTop'
      ? 'var(--accent)'
      : 'var(--text)'};"
  >
    <ArrowUp size={18} strokeWidth={2} />
  </button>
{/if}
