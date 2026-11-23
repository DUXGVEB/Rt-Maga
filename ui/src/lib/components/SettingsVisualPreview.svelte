<script lang="ts">
  import { ArrowLeft, ArrowRight, Eye } from "lucide-svelte";
  import type { FullDictionary } from "../i18n";
  import type { Settings } from "../types";

  let { settings, t }: { settings: Settings; t: FullDictionary } = $props();

  const isHorizontal = $derived(settings.reader.orientation === "horizontal");
  const bgStyle = $derived(() => {
    if (settings.reader.readerBg === "dark") return "#0d0d10";
    if (settings.reader.readerBg === "light") return "#f4f4f6";
    return "var(--bg)";
  });
  const fgStyle = $derived(() => {
    if (settings.reader.readerBg === "dark") return "#f3f4f6";
    if (settings.reader.readerBg === "light") return "#18181b";
    return "var(--text)";
  });

  // Scale down real pixel values so the miniature frame cleanly illustrates proportions.
  const scaledVerticalMargin = $derived(
    Math.min(28, Math.round(settings.reader.behavior.verticalMarginPx * 0.42))
  );
  const scaledVerticalGap = $derived(
    settings.reader.vertical.noGapMode
      ? 0
      : Math.min(18, Math.round(settings.reader.vertical.pageGapPx * 0.45))
  );
  const verticalDimOpacity = $derived(
    settings.reader.vertical.dimUnfocused
      ? Math.max(0.28, 1 - settings.reader.vertical.dimLevel / 135)
      : 1
  );

  const previewPagesPerGroup = $derived(
    Math.max(1, Math.min(4, settings.reader.horizontal.pagesPerGroup))
  );
  const scaledHorizPageGap = $derived(
    settings.reader.horizontal.noGapMode
      ? 0
      : Math.min(14, Math.round(settings.reader.horizontal.pageGapPx * 0.35))
  );
  const scaledHorizGroupGap = $derived(
    settings.reader.horizontal.noGapMode
      ? 0
      : Math.min(26, Math.max(6, Math.round(settings.reader.horizontal.groupGapPx * 0.32)))
  );
  const horizDimOpacity = $derived(
    settings.reader.horizontal.dimUnfocused
      ? Math.max(0.28, 1 - settings.reader.horizontal.dimLevel / 135)
      : 1
  );
  const isRtl = $derived(settings.reader.horizontal.direction === "rtl");

  const activeGroupPages = $derived(() => {
    const nums = Array.from({ length: previewPagesPerGroup }, (_, i) => i + 1);
    return isRtl ? nums.reverse() : nums;
  });
  const sideGroupPages = $derived(() => {
    const start = previewPagesPerGroup + 1;
    const count = Math.min(2, previewPagesPerGroup);
    const nums = Array.from({ length: count }, (_, i) => start + i);
    return isRtl ? nums.reverse() : nums;
  });
</script>

<div
  class="flex flex-col gap-2.5 rounded-xl border p-3"
  style="background-color: var(--surface-subtle); border-color: var(--border);"
>
  <div class="flex flex-wrap items-center justify-between gap-2 text-xs">
    <div class="flex items-center gap-1.5 font-semibold" style="color: var(--text);">
      <Eye size={14} style="color: var(--accent);" />
      <span>{t.visualPreviewTitle}</span>
    </div>
    <div class="flex flex-wrap items-center gap-1.5 text-[11px]" style="color: var(--text-muted);">
      {#if !isHorizontal}
        <span class="rounded-md px-1.5 py-0.5" style="background-color: var(--surface);">
          {settings.reader.vertical.noGapMode
            ? t.noGapModeLabel
            : `${t.previewGapChip}: ${settings.reader.vertical.pageGapPx}px`}
        </span>
        <span class="rounded-md px-1.5 py-0.5" style="background-color: var(--surface);">
          {t.previewMarginChip}: {settings.reader.behavior.verticalMarginPx}px
        </span>
      {:else}
        <span class="inline-flex items-center gap-1 rounded-md px-1.5 py-0.5" style="background-color: var(--surface);">
          {#if isRtl}
            <ArrowLeft size={11} />
            RTL
          {:else}
            LTR
            <ArrowRight size={11} />
          {/if}
        </span>
        <span class="rounded-md px-1.5 py-0.5" style="background-color: var(--surface);">
          {settings.reader.horizontal.pagesPerGroup}x
        </span>
        {#if settings.reader.horizontal.noGapMode}
          <span
            class="rounded-md px-1.5 py-0.5 font-medium"
            style="background-color: var(--accent-soft); color: var(--accent);"
          >
            {t.horizontalNoGapLabel}
          </span>
        {/if}
        <span class="rounded-md px-1.5 py-0.5" style="background-color: var(--surface);">
          {settings.reader.horizontal.centerGroups ? t.alignCenter : t.alignStart}
        </span>
      {/if}
    </div>
  </div>

  <!-- Miniature Reader Frame -->
  <div
    class="relative overflow-hidden rounded-lg border transition-colors"
    style="background-color: {bgStyle()}; border-color: var(--border); color: {fgStyle()}; height: 152px;"
  >
    {#if !isHorizontal}
      <!-- Vertical Strip Preview -->
      <div
        class="mx-auto flex h-full w-44 flex-col justify-center overflow-hidden transition-all"
        style="padding-left: {scaledVerticalMargin}px; padding-right: {scaledVerticalMargin}px; gap: {scaledVerticalGap}px;"
      >
        <div
          class="flex h-10 w-full items-center justify-center rounded-sm border text-[10px] font-medium transition-all"
          style="background-color: color-mix(in srgb, var(--accent) 18%, var(--surface)); border-color: var(--border); opacity: {verticalDimOpacity};"
        >
          1
        </div>
        <div
          class="flex h-14 w-full items-center justify-center rounded-sm border-2 text-[11px] font-bold shadow-sm transition-all"
          style="background-color: color-mix(in srgb, var(--accent) 26%, var(--surface)); border-color: var(--accent);"
        >
          2
        </div>
        <div
          class="flex h-10 w-full items-center justify-center rounded-sm border text-[10px] font-medium transition-all"
          style="background-color: color-mix(in srgb, var(--accent) 18%, var(--surface)); border-color: var(--border); opacity: {verticalDimOpacity};"
        >
          3
        </div>
      </div>
    {:else}
      <!-- Horizontal Spread Preview -->
      <div
        class="flex h-full w-full items-center overflow-hidden px-3 transition-all"
        style="justify-content: {settings.reader.horizontal.centerGroups
          ? 'center'
          : isRtl
            ? 'flex-end'
            : 'flex-start'}; gap: {scaledHorizGroupGap}px; flex-direction: {isRtl
          ? 'row-reverse'
          : 'row'};"
      >
        <!-- Active group -->
        <div class="flex shrink-0 items-center" style="gap: {scaledHorizPageGap}px;">
          {#each activeGroupPages() as num (num)}
            <div
              class="flex h-24 w-12 items-center justify-center rounded-sm border-2 text-xs font-bold shadow-sm transition-all"
              style="background-color: color-mix(in srgb, var(--accent) 25%, var(--surface)); border-color: var(--accent);"
            >
              {num}
            </div>
          {/each}
        </div>

        <!-- Adjacent group -->
        <div
          class="flex shrink-0 items-center transition-opacity"
          style="gap: {scaledHorizPageGap}px; opacity: {horizDimOpacity};"
        >
          {#each sideGroupPages() as num (num)}
            <div
              class="flex h-24 w-12 items-center justify-center rounded-sm border text-[11px] font-medium"
              style="background-color: color-mix(in srgb, var(--accent) 14%, var(--surface)); border-color: var(--border);"
            >
              {num}
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>
</div>
