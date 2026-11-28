<script lang="ts">
  import { ChevronLeft, ChevronRight, Languages, ScanSearch, X } from "lucide-svelte";
  import { backdropFade, popoverPop } from "../motion";
  import { settingsStore } from "../stores/settings.svelte";
  import type { TranslatedRegionView } from "../types";

  let {
    regions,
    regionIndex,
    imgEl,
    showTranslation = false,
    onSelectIndex,
    onClose,
  }: {
    regions: TranslatedRegionView[];
    regionIndex: number;
    imgEl: HTMLImageElement | null;
    showTranslation?: boolean;
    onSelectIndex: (nextIndex: number) => void;
    onClose: () => void;
  } = $props();

  let canvasEl = $state<HTMLCanvasElement | null>(null);
  let cropRendered = $state(false);

  const t = $derived(settingsStore.t);
  const safeIndex = $derived(
    regions.length > 0 ? Math.max(0, Math.min(regions.length - 1, regionIndex)) : 0
  );
  const region = $derived<TranslatedRegionView | null>(regions[safeIndex] ?? null);

  const primaryText = $derived(() => {
    if (!region) return "";
    const translated = region.text.trim();
    const source = (region.sourceText ?? "").trim();
    if (showTranslation && translated.length > 0) return translated;
    if (translated.length > 0) return translated;
    return source;
  });

  const secondarySourceText = $derived(() => {
    if (!region) return "";
    const primary = primaryText();
    const source = (region.sourceText ?? "").trim();
    if (!source || source === primary) return "";
    return source;
  });

  // Render a crisp magnified crop of just the speech bubble onto an offscreen-scaled canvas
  // without touching or zooming the underlying manga page.
  $effect(() => {
    const canvas = canvasEl;
    const img = imgEl;
    const current = region;
    cropRendered = false;

    if (!canvas || !img || !current || !img.complete || img.naturalWidth < 8 || img.naturalHeight < 8) {
      return;
    }

    const [bx1, by1, bx2, by2] = current.bbox;
    // Slight normalized padding so bubble outlines and edge characters remain crisp.
    const padX = Math.max(10, Math.round((bx2 - bx1) * 0.08));
    const padY = Math.max(10, Math.round((by2 - by1) * 0.08));
    const nx1 = Math.max(0, bx1 - padX);
    const ny1 = Math.max(0, by1 - padY);
    const nx2 = Math.min(1000, bx2 + padX);
    const ny2 = Math.min(1000, by2 + padY);

    const sx = Math.floor((nx1 / 1000) * img.naturalWidth);
    const sy = Math.floor((ny1 / 1000) * img.naturalHeight);
    const sw = Math.max(8, Math.ceil(((nx2 - nx1) / 1000) * img.naturalWidth));
    const sh = Math.max(8, Math.ceil(((ny2 - ny1) / 1000) * img.naturalHeight));

    const maxDisplayW = Math.min(340, Math.max(220, window.innerWidth - 56));
    const maxDisplayH = Math.min(260, Math.max(150, Math.round(window.innerHeight * 0.34)));
    // Magnify small speech bubbles cleanly (up to 2.6x) while fitting comfortably inside the card.
    const desiredScale = Math.min(2.6, maxDisplayW / sw, maxDisplayH / sh);
    const cssW = Math.max(120, Math.round(sw * desiredScale));
    const cssH = Math.max(64, Math.round(sh * desiredScale));
    const dpr = Math.min(2.5, window.devicePixelRatio || 1);

    canvas.width = Math.max(1, Math.round(cssW * dpr));
    canvas.height = Math.max(1, Math.round(cssH * dpr));
    canvas.style.width = `${cssW}px`;
    canvas.style.height = `${cssH}px`;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(0, 0, cssW, cssH);
    try {
      ctx.drawImage(img, sx, sy, sw, sh, 0, 0, cssW, cssH);
      cropRendered = true;
    } catch {
      cropRendered = false;
    }
  });

  function stopEvt(e: Event) {
    e.stopPropagation();
  }

  function stepBubble(delta: number) {
    if (regions.length <= 1) return;
    const next = (safeIndex + delta + regions.length) % regions.length;
    onSelectIndex(next);
  }
</script>

{#if region}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    transition:backdropFade|local={{ duration: 135 }}
    onclick={(e) => {
      e.stopPropagation();
      onClose();
    }}
    onpointerdown={stopEvt}
    ontouchstart={stopEvt}
    onwheel={stopEvt}
    class="fixed inset-0 z-45 flex items-center justify-center bg-black/48 p-4"
    style="padding-top: max(1rem, var(--safe-top)); padding-bottom: max(1rem, var(--safe-bottom));"
  >
    <div
      transition:popoverPop|local={{ duration: 165, y: 10, start: 0.94 }}
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label={t.a11yBubbleZoomTitle}
      onclick={stopEvt}
      class="flex max-h-[82dvh] w-full max-w-[min(92dvw,380px)] flex-col overflow-hidden rounded-2xl border shadow-2xl"
      style="background-color: var(--surface-elevated, var(--surface)); border-color: var(--border); color: var(--text);"
    >
      <!-- Header -->
      <div
        class="flex items-center justify-between gap-2 border-b px-3.5 py-2.5"
        style="border-color: var(--border);"
      >
        <div class="flex min-w-0 items-center gap-2 text-xs font-semibold">
          <ScanSearch size={14} class="shrink-0" style="color: var(--accent);" />
          <span class="truncate">{t.a11yBubbleZoomTitle}</span>
          {#if regions.length > 1}
            <span
              class="shrink-0 rounded-full px-1.5 py-0.5 text-[10px] font-semibold tabular-nums"
              style="background-color: var(--accent-soft); color: var(--accent);"
            >
              {safeIndex + 1} / {regions.length}
            </span>
          {/if}
        </div>
        <button
          type="button"
          onclick={onClose}
          class="gpu-press flex h-8 w-8 shrink-0 items-center justify-center rounded-xl opacity-80 hover:opacity-100"
          style="background-color: var(--surface-subtle); color: var(--text);"
          aria-label={t.cancel}
        >
          <X size={15} />
        </button>
      </div>

      <!-- Body: Magnified Bubble Crop + Crisp Enlarged Text -->
      <div class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto overscroll-contain p-3.5">
        <div
          class="flex items-center justify-center overflow-hidden rounded-xl border p-2"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          <canvas
            bind:this={canvasEl}
            class="block max-w-full rounded-lg shadow-sm"
            style="display: {cropRendered ? 'block' : 'none'};"
          ></canvas>
          {#if !cropRendered && !primaryText()}
            <div class="py-6 text-xs" style="color: var(--text-muted);">
              {t.a11yDetectingBubblesStatus}
            </div>
          {/if}
        </div>

        {#if primaryText()}
          <div
            class="flex flex-col gap-1.5 rounded-xl border px-3.5 py-3 select-text"
            style="border-color: color-mix(in srgb, var(--accent) 32%, var(--border)); background-color: var(--surface);"
          >
            {#if showTranslation && region.text.trim().length > 0}
              <div
                class="inline-flex items-center gap-1 text-[10px] font-semibold uppercase tracking-wide"
                style="color: var(--accent);"
              >
                <Languages size={11} />
                <span>{t.a11yAiTranslateTitle}</span>
              </div>
            {/if}
            <p class="text-[17px] leading-relaxed font-semibold break-words sm:text-[18px]">
              {primaryText()}
            </p>
            {#if secondarySourceText()}
              <p
                class="border-t pt-1.5 text-xs leading-relaxed break-words"
                style="border-color: var(--border); color: var(--text-muted);"
              >
                {secondarySourceText()}
              </p>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Optional Bubble Stepper Footer -->
      {#if regions.length > 1}
        <div
          class="flex items-center justify-between gap-2 border-t px-3.5 py-2"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          <button
            type="button"
            onclick={() => stepBubble(-1)}
            class="btn-3d btn-3d-surface gpu-press inline-flex min-h-[36px] items-center gap-1 rounded-xl px-3 py-1.5 text-xs font-semibold"
          >
            <ChevronLeft size={14} />
            <span>{t.browsePrevPage}</span>
          </button>
          <button
            type="button"
            onclick={() => stepBubble(1)}
            class="btn-3d btn-3d-surface gpu-press inline-flex min-h-[36px] items-center gap-1 rounded-xl px-3 py-1.5 text-xs font-semibold"
          >
            <span>{t.browseNextPage}</span>
            <ChevronRight size={14} />
          </button>
        </div>
      {/if}
    </div>
  </div>
{/if}
