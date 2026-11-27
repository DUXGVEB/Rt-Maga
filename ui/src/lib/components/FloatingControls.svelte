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

  function wrapperStyleFor(id: string): string {
    const pos = resolvedMap().get(id);
    if (!pos) return "display: none;";
    const xPx =
      pos.edge === "left"
        ? pos.offsetPx
        : Math.max(0, deviceEnv.viewportWidth - pos.offsetPx - pos.sizePx);
    const pointerCss = isHiddenByFade ? "pointer-events: none;" : "";
    return `transform: translate3d(${Math.round(xPx)}px, ${Math.round(pos.topPx)}px, 0); width: ${pos.sizePx}px; height: ${pos.sizePx}px; opacity: ${effectiveOpacity}; ${pointerCss}`;
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
  <div
    class="floating-slot fixed top-0 left-0 z-30"
    style={wrapperStyleFor("orientationLock")}
  >
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
      class="glass-chrome gpu-press flex h-full w-full touch-manipulation items-center justify-center rounded-2xl border select-none hover:scale-105"
      style="box-shadow: var(--shadow-md); border-color: {holdingId === 'orientationLock' || orientationLock !== 'auto'
        ? 'var(--accent)'
        : 'color-mix(in srgb, var(--border-strong) 85%, transparent)'}; color: {orientationLock !== 'auto' || holdingId === 'orientationLock'
        ? 'var(--accent)'
        : 'var(--text)'}; -webkit-touch-callout: none; -webkit-user-select: none;"
    >
      {#if orientationLock === "portrait"}
        <Smartphone size={17} strokeWidth={1.9} />
      {:else if orientationLock === "landscape"}
        <Monitor size={17} strokeWidth={1.9} />
      {:else}
        <RotateCw size={17} strokeWidth={1.9} />
      {/if}
    </button>
  </div>
{/if}

{#if allowReaderSettings}
  <div
    class="floating-slot fixed top-0 left-0 z-30"
    style={wrapperStyleFor("readerSettings")}
  >
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
      class="glass-chrome gpu-press flex h-full w-full touch-manipulation items-center justify-center rounded-2xl border select-none hover:scale-105"
      style="box-shadow: var(--shadow-md); border-color: {readerSettingsActive || holdingId === 'readerSettings'
        ? 'var(--accent)'
        : 'color-mix(in srgb, var(--border-strong) 85%, transparent)'}; color: {readerSettingsActive || holdingId === 'readerSettings'
        ? 'var(--accent)'
        : 'var(--text)'}; -webkit-touch-callout: none; -webkit-user-select: none;"
    >
      <SlidersHorizontal size={17} strokeWidth={1.9} />
    </button>
  </div>
{/if}

{#if allowScrollTop}
  <div
    class="floating-slot fixed top-0 left-0 z-30"
    style={wrapperStyleFor("scrollToTop")}
  >
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
      class="glass-chrome gpu-press flex h-full w-full touch-manipulation items-center justify-center rounded-2xl border select-none hover:scale-105"
      style="box-shadow: var(--shadow-md); border-color: {holdingId === 'scrollToTop'
        ? 'var(--accent)'
        : 'color-mix(in srgb, var(--border-strong) 85%, transparent)'}; color: {holdingId === 'scrollToTop'
        ? 'var(--accent)'
        : 'var(--text)'}; -webkit-touch-callout: none; -webkit-user-select: none;"
    >
      <ArrowUp size={18} strokeWidth={2} />
    </button>
  </div>
{/if}

<style>
  .floating-slot {
    transition:
      transform var(--dur-base, 175ms) var(--ease-out, cubic-bezier(0.2, 0, 0, 1)),
      opacity var(--dur-base, 175ms) var(--ease-out, cubic-bezier(0.2, 0, 0, 1));
  }
</style>
