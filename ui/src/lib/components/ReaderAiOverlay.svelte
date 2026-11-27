<script lang="ts">
  import type { NormalizedBbox } from "../readerLayout";
  import { aiTranslateStore } from "../stores/aiTranslate.svelte";
  import type { TranslatedRegionView } from "../types";

  let {
    pageToken,
    overlayWidth,
    overlayHeight,
    bubbleZoomEnabled = false,
    translationEnabled = false,
    activeBubbleKey = null,
    onBubbleTap,
  }: {
    pageToken: string;
    overlayWidth: number;
    overlayHeight: number;
    bubbleZoomEnabled?: boolean;
    translationEnabled?: boolean;
    activeBubbleKey?: string | null;
    onBubbleTap?: (bbox: NormalizedBbox, bubbleKey: string) => void;
  } = $props();

  const pageResult = $derived(aiTranslateStore.getPageResult(pageToken));
  const regions = $derived<TranslatedRegionView[]>(pageResult?.regions ?? []);

  function bboxKey(bbox: [number, number, number, number], idx: number): string {
    return `${pageToken}:${idx}:${bbox[0]},${bbox[1]},${bbox[2]},${bbox[3]}`;
  }

  function regionRectStyle(bbox: [number, number, number, number]): string {
    const x1 = Math.max(0, Math.min(1000, bbox[0]));
    const y1 = Math.max(0, Math.min(1000, bbox[1]));
    const x2 = Math.max(x1 + 4, Math.min(1000, bbox[2]));
    const y2 = Math.max(y1 + 4, Math.min(1000, bbox[3]));
    return `left: ${(x1 / 10).toFixed(2)}%; top: ${(y1 / 10).toFixed(2)}%; width: ${((x2 - x1) / 10).toFixed(2)}%; height: ${((y2 - y1) / 10).toFixed(2)}%;`;
  }

  function computeFittedFontPx(text: string, bbox: [number, number, number, number]): number {
    const wPx = Math.max(24, ((bbox[2] - bbox[0]) / 1000) * Math.max(120, overlayWidth) - 8);
    const hPx = Math.max(16, ((bbox[3] - bbox[1]) / 1000) * Math.max(120, overlayHeight) - 6);
    const len = Math.max(1, text.trim().length);
    const est = Math.sqrt((wPx * hPx) / (len * 0.88));
    const maxByHeight = Math.max(9, hPx * 0.58);
    const maxByWidth = Math.max(9, wPx * 0.42);
    return Math.max(9, Math.min(20, Math.floor(Math.min(est, maxByHeight, maxByWidth))));
  }

  function handleTap(e: MouseEvent, region: TranslatedRegionView, key: string) {
    if (!bubbleZoomEnabled || !onBubbleTap) return;
    e.stopPropagation();
    onBubbleTap(region.bbox, key);
  }
</script>

{#if (bubbleZoomEnabled || translationEnabled) && regions.length > 0 && overlayWidth > 20 && overlayHeight > 20}
  <div
    class="pointer-events-none absolute inset-0 z-15 overflow-hidden select-none"
    aria-hidden={!translationEnabled && !bubbleZoomEnabled}
  >
    {#each regions as region, idx (bboxKey(region.bbox, idx))}
      {@const key = bboxKey(region.bbox, idx)}
      {@const isFocusedBubble = activeBubbleKey === key}
      {@const showText = translationEnabled && region.text.trim().length > 0}
      {@const fontPx = showText ? computeFittedFontPx(region.text, region.bbox) : 12}

      {#if bubbleZoomEnabled}
        <button
          type="button"
          data-ai-bubble="true"
          onclick={(e) => handleTap(e, region, key)}
          onpointerdown={(e) => e.stopPropagation()}
          class="pointer-events-auto absolute flex items-center justify-center overflow-hidden rounded-lg text-center transition-[box-shadow,border-color] duration-150"
          style="{regionRectStyle(region.bbox)} {showText
            ? `background-color: rgba(252, 252, 253, 0.94); color: #111116; padding: 2px 4px; font-size: ${fontPx}px; line-height: 1.16; font-weight: 600; box-shadow: ${isFocusedBubble ? '0 0 0 2px var(--accent), 0 2px 8px rgba(0,0,0,0.22)' : '0 1px 4px rgba(0,0,0,0.16)'};`
            : `background-color: ${isFocusedBubble ? 'color-mix(in srgb, var(--accent) 12%, transparent)' : 'transparent'}; border: 1.5px solid ${isFocusedBubble ? 'var(--accent)' : 'color-mix(in srgb, var(--accent) 34%, transparent)'};`}"
          aria-label={showText ? region.text : `Bubble ${idx + 1}`}
        >
          {#if showText}
            <span class="ai-bubble-text">{region.text}</span>
          {/if}
        </button>
      {:else if showText}
        <div
          class="pointer-events-none absolute flex items-center justify-center overflow-hidden rounded-lg px-1 py-0.5 text-center"
          style="{regionRectStyle(region.bbox)} background-color: rgba(252, 252, 253, 0.94); color: #111116; font-size: {fontPx}px; line-height: 1.16; font-weight: 600; box-shadow: 0 1px 4px rgba(0,0,0,0.16);"
        >
          <span class="ai-bubble-text">{region.text}</span>
        </div>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .ai-bubble-text {
    display: block;
    max-width: 100%;
    max-height: 100%;
    overflow: hidden;
    overflow-wrap: anywhere;
    word-break: break-word;
    text-wrap: balance;
  }
</style>
