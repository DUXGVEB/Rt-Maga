<script lang="ts">
  import {
    Download,
    ExternalLink,
    Languages,
    Loader2,
    ScanSearch,
    Sparkles,
    X,
  } from "lucide-svelte";
  import { popoverPop } from "../motion";
  import { aiTranslateStore } from "../stores/aiTranslate.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import type { OneHandedMode } from "../types";
  import SquishSwitch from "./SquishSwitch.svelte";

  let {
    oneHandedMode = "default",
    toolbarVisible = false,
    open = $bindable(false),
    onOpenAccessibilitySettings,
  }: {
    oneHandedMode?: OneHandedMode;
    toolbarVisible?: boolean;
    open?: boolean;
    onOpenAccessibilitySettings?: () => void;
  } = $props();

  let hovered = $state(false);

  const t = $derived(settingsStore.t);
  const a11y = $derived(settingsStore.value.accessibility);
  const bubbleOn = $derived(a11y.bubbleAutoZoom);
  const translateOn = $derived(a11y.aiTranslationOverlay);
  const anyActive = $derived(bubbleOn || translateOn);
  const activeTier = $derived(aiTranslateStore.activeTier);
  const tier2Ready = $derived(aiTranslateStore.tier2Ready);
  const downloading = $derived(aiTranslateStore.downloading);
  const progress = $derived(aiTranslateStore.downloadProgress);
  const busyWorking = $derived(aiTranslateStore.isTranslating);

  const dockEdge = $derived<"left" | "right">(oneHandedMode === "left" ? "left" : "right");
  const bottomOffsetCss = $derived(
    toolbarVisible
      ? "calc(4.65rem + var(--safe-bottom))"
      : "calc(1.05rem + var(--safe-bottom))"
  );
  const sideOffsetCss = $derived(
    dockEdge === "left"
      ? "max(0.85rem, var(--safe-left), var(--gesture-left))"
      : "max(0.85rem, var(--safe-right), var(--gesture-right))"
  );
  const pillOpacity = $derived(open || hovered || busyWorking || anyActive ? 1 : 0.65);

  function stopEvt(e: Event) {
    e.stopPropagation();
  }

  function togglePopover(e: MouseEvent) {
    e.stopPropagation();
    open = !open;
    if (open) {
      void aiTranslateStore.init();
    }
  }

  async function handleQuickTierSwitch() {
    if (activeTier === "lite") {
      if (tier2Ready) {
        await aiTranslateStore.setTier("qwen3_vl_2b");
      } else if (!downloading) {
        void aiTranslateStore.downloadTier2();
      }
    } else {
      await aiTranslateStore.setTier("lite");
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="ai-pill-anchor fixed z-35"
  style="bottom: {bottomOffsetCss}; {dockEdge}: {sideOffsetCss};"
  onpointerdown={stopEvt}
  ontouchstart={stopEvt}
  onclick={stopEvt}
  onwheel={stopEvt}
>
  {#if open}
    <div
      transition:popoverPop|local={{ duration: 155, y: 8 }}
      role="dialog"
      aria-label={t.a11yAiToolsTitle}
      class="glass-chrome absolute bottom-11 z-40 flex w-64 max-w-[calc(100dvw-1.5rem)] flex-col gap-2.5 rounded-2xl border p-3 text-xs {dockEdge === 'left'
        ? 'left-0'
        : 'right-0'}"
      style="border-color: var(--border); box-shadow: var(--shadow-lg);"
    >
      <div class="flex items-center justify-between gap-2">
        <div class="flex items-center gap-1.5 font-semibold">
          <Sparkles size={13} style="color: var(--accent);" />
          <span>{t.a11yAiToolsTitle}</span>
        </div>
        <div class="flex items-center gap-1">
          {#if onOpenAccessibilitySettings}
            <button
              type="button"
              onclick={() => {
                open = false;
                onOpenAccessibilitySettings();
              }}
              class="gpu-press flex h-7 w-7 items-center justify-center rounded-lg opacity-75 hover:opacity-100"
              style="color: var(--accent);"
              title={t.settingsTabAccessibility}
              aria-label={t.settingsTabAccessibility}
            >
              <ExternalLink size={13} />
            </button>
          {/if}
          <button
            type="button"
            onclick={() => (open = false)}
            class="gpu-press flex h-7 w-7 items-center justify-center rounded-lg opacity-70 hover:opacity-100"
            aria-label={t.cancel}
          >
            <X size={14} />
          </button>
        </div>
      </div>

      <!-- Synced Toggle: Bubble Auto-Zoom -->
      <div
        class="flex items-center justify-between gap-2 rounded-xl border px-2.5 py-2"
        style="border-color: {bubbleOn
          ? 'color-mix(in srgb, var(--accent) 45%, var(--border))'
          : 'var(--border)'}; background-color: var(--surface-subtle);"
      >
        <div class="flex min-w-0 items-center gap-1.5 font-medium">
          <ScanSearch size={14} style="color: {bubbleOn ? 'var(--accent)' : 'var(--text-muted)'};" />
          <span class="truncate">{t.a11yBubbleZoomTitle}</span>
        </div>
        <SquishSwitch
          size="sm"
          checked={bubbleOn}
          ariaLabel={t.a11yBubbleZoomTitle}
          onchange={(checked) => aiTranslateStore.requestToggleBubbleZoom(checked)}
        />
      </div>

      <!-- Synced Toggle: AI Translation Overlay -->
      <div
        class="flex items-center justify-between gap-2 rounded-xl border px-2.5 py-2"
        style="border-color: {translateOn
          ? 'color-mix(in srgb, var(--accent) 45%, var(--border))'
          : 'var(--border)'}; background-color: var(--surface-subtle);"
      >
        <div class="flex min-w-0 items-center gap-1.5 font-medium">
          <Languages size={14} style="color: {translateOn ? 'var(--accent)' : 'var(--text-muted)'};" />
          <span class="truncate">{t.a11yAiTranslateTitle}</span>
        </div>
        <SquishSwitch
          size="sm"
          checked={translateOn}
          ariaLabel={t.a11yAiTranslateTitle}
          onchange={(checked) => aiTranslateStore.requestToggleTranslation(checked)}
        />
      </div>

      <!-- Active Tier Badge + 1-Tap Switch -->
      <div
        class="flex items-center justify-between gap-2 border-t pt-2"
        style="border-color: var(--border);"
      >
        <span
          class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold"
          style="background-color: var(--accent-soft); color: var(--accent);"
        >
          {activeTier === "qwen3_vl_2b" ? t.a11yModelTier2Badge : t.a11yModelLiteBadge}
        </span>

        {#if downloading}
          <span
            class="inline-flex items-center gap-1 text-[11px] font-semibold tabular-nums"
            style="color: var(--accent);"
          >
            <Loader2 size={11} class="animate-spin" />
            <span>{Math.round(progress?.percent ?? 0)}%</span>
          </span>
        {:else}
          <button
            type="button"
            disabled={aiTranslateStore.busyAction !== null}
            onclick={() => void handleQuickTierSwitch()}
            class="btn-3d btn-3d-surface gpu-press inline-flex items-center gap-1 rounded-lg border px-2 py-1 text-[11px] font-semibold"
            style="border-color: var(--border); background-color: var(--surface); color: var(--text);"
          >
            {#if activeTier === "lite" && !tier2Ready}
              <Download size={11} style="color: var(--accent);" />
              <span>{t.a11yModelTier2Badge}</span>
            {:else}
              <span>
                {activeTier === "lite" ? t.a11yModelTier2Badge : t.a11yModelLiteBadge}
              </span>
            {/if}
          </button>
        {/if}
      </div>
    </div>
  {/if}

  <!-- Compact 34px (32–36dp) 3D Floating AI Pill -->
  <button
    type="button"
    onclick={togglePopover}
    onpointerenter={() => (hovered = true)}
    onpointerleave={() => (hovered = false)}
    title={t.a11yAiToolsTitle}
    aria-label={t.a11yAiToolsTitle}
    aria-expanded={open}
    class="pill-3d btn-3d gpu-press ai-pill-btn relative flex h-[34px] w-[34px] touch-manipulation items-center justify-center rounded-full border select-none"
    style="opacity: {pillOpacity}; border-color: {anyActive || open
      ? 'var(--accent)'
      : 'color-mix(in srgb, var(--border-strong) 85%, transparent)'}; background-color: {anyActive
      ? 'var(--accent)'
      : 'var(--surface-glass-solid, var(--surface))'}; color: {anyActive
      ? 'var(--accent-fg)'
      : 'var(--text)'};"
  >
    {#if busyWorking}
      <span class="ai-pill-ring pointer-events-none absolute -inset-1 rounded-full" aria-hidden="true"></span>
    {/if}
    <Sparkles size={15} strokeWidth={2.1} />
  </button>
</div>

<style>
  .ai-pill-anchor {
    transition:
      bottom var(--dur-base, 175ms) var(--ease-out, cubic-bezier(0.2, 0, 0, 1)),
      left var(--dur-base, 175ms) var(--ease-out, cubic-bezier(0.2, 0, 0, 1)),
      right var(--dur-base, 175ms) var(--ease-out, cubic-bezier(0.2, 0, 0, 1));
  }

  .ai-pill-btn {
    box-shadow:
      0 2px 0 rgba(0, 0, 0, 0.16),
      0 6px 16px -4px rgba(0, 0, 0, 0.24),
      inset 0 1px 0 rgba(255, 255, 255, 0.24);
    transition:
      opacity var(--dur-fast, 140ms) var(--ease-out, cubic-bezier(0.2, 0, 0, 1)),
      transform var(--dur-fast, 140ms) var(--ease-out, cubic-bezier(0.2, 0, 0, 1));
  }

  .ai-pill-ring {
    border: 2px solid var(--accent);
    animation: ai-pill-pulse 1.35s cubic-bezier(0.2, 0, 0, 1) infinite;
  }

  @keyframes ai-pill-pulse {
    0% {
      transform: scale(0.92);
      opacity: 0.85;
    }
    70% {
      transform: scale(1.24);
      opacity: 0;
    }
    100% {
      transform: scale(1.24);
      opacity: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .ai-pill-ring {
      animation: none;
      opacity: 0.55;
    }
  }
</style>
