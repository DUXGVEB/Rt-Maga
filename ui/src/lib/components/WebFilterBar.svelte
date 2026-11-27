<script lang="ts">
  import { Check, ChevronDown, Filter, RotateCcw, Search, X } from "lucide-svelte";
  import { badgePop, collapseReveal } from "../motion";
  import { settingsStore } from "../stores/settings.svelte";
  import {
    cloneFilterQuery,
    countActiveFilters,
    emptyFilterQuery,
  } from "../stores/webFilter.svelte";
  import type { CatalogFilterQuery, SiteFilterSchema, UpdatedWithinFilter } from "../types";
  import LatticeLoader from "./LatticeLoader.svelte";

  let {
    schema,
    value,
    loadingSchema = false,
    applying = false,
    defaultExpanded = true,
    siteLabel = "",
    onApply,
    onReset,
  }: {
    schema: SiteFilterSchema;
    value: CatalogFilterQuery;
    loadingSchema?: boolean;
    applying?: boolean;
    defaultExpanded?: boolean;
    siteLabel?: string;
    onApply: (next: CatalogFilterQuery) => void;
    onReset?: () => void;
  } = $props();

  let expanded = $state(false);
  let draft = $state<CatalogFilterQuery>(emptyFilterQuery());
  let tagSearch = $state("");
  let showAllTags = $state(false);
  let customMinCh = $state("");

  const t = $derived(settingsStore.t);

  $effect(() => {
    expanded = defaultExpanded;
  });

  $effect(() => {
    draft = cloneFilterQuery(value);
    customMinCh = value.minChapters && value.minChapters > 0 ? String(value.minChapters) : "";
  });

  const activeCount = $derived(countActiveFilters(draft));

  const filteredTags = $derived(() => {
    const q = tagSearch.trim().toLowerCase();
    const list = schema.tags ?? [];
    if (!q) return list;
    return list.filter(
      (tag) => tag.label.toLowerCase().includes(q) || tag.id.toLowerCase().includes(q)
    );
  });

  const visibleTags = $derived(() => {
    const list = filteredTags();
    if (showAllTags || tagSearch.trim().length > 0) return list;
    return list.slice(0, 24);
  });

  function tagState(tagId: string): "off" | "include" | "exclude" {
    if (draft.includeTags.includes(tagId)) return "include";
    if (draft.excludeTags.includes(tagId)) return "exclude";
    return "off";
  }

  function cycleTag(tagId: string) {
    const cur = tagState(tagId);
    const inc = draft.includeTags.filter((id) => id !== tagId);
    const exc = draft.excludeTags.filter((id) => id !== tagId);
    if (cur === "off") {
      inc.push(tagId);
    } else if (cur === "include") {
      exc.push(tagId);
    }
    draft = { ...draft, includeTags: inc, excludeTags: exc };
  }

  function setMinChaptersPreset(val: number | null) {
    draft = { ...draft, minChapters: val };
    customMinCh = val ? String(val) : "";
  }

  function setUpdatedWithin(val: UpdatedWithinFilter) {
    draft = { ...draft, updatedWithin: val };
  }

  function setAxisValue(key: string, val: string) {
    const nextExtra = { ...draft.extra };
    if (!val || val === "-1" || val === "0" || val === "all" || val === "any") {
      delete nextExtra[key];
    } else {
      nextExtra[key] = val;
    }
    draft = { ...draft, extra: nextExtra };
  }

  function handleApply() {
    const parsedCustom = Number.parseInt(customMinCh.trim(), 10);
    const minCh = Number.isFinite(parsedCustom) && parsedCustom > 0 ? parsedCustom : null;
    const finalized: CatalogFilterQuery = {
      ...cloneFilterQuery(draft),
      minChapters: minCh,
    };
    onApply(finalized);
  }

  function handleReset() {
    draft = emptyFilterQuery();
    customMinCh = "";
    tagSearch = "";
    if (onReset) {
      onReset();
    } else {
      onApply(emptyFilterQuery());
    }
  }

  const minChapterPresets = [null, 10, 20, 50, 100, 200] as const;
</script>

<section
  class="flex flex-col gap-3 rounded-2xl border p-3.5 sm:p-4"
  style="background-color: var(--surface); border-color: var(--border);"
>
  <div class="flex flex-wrap items-center justify-between gap-2">
    <button
      type="button"
      onclick={() => {
        expanded = !expanded;
      }}
      aria-expanded={expanded}
      class="gpu-press inline-flex items-center gap-2 text-left text-xs font-semibold sm:text-sm"
      style="color: var(--text);"
    >
      <Filter size={15} style="color: var(--accent);" />
      <span>
        {t.webFilterTitle}
        {#if siteLabel}
          <span class="font-normal opacity-75">· {siteLabel}</span>
        {/if}
      </span>
      {#if activeCount > 0}
        <span
          transition:badgePop|local
          class="rounded-full px-2 py-0.5 text-[11px] font-semibold"
          style="background-color: var(--accent-soft); color: var(--accent);"
        >
          {t.webFilterActiveCount(activeCount)}
        </span>
      {/if}
      {#if loadingSchema}
        <LatticeLoader size="xs" inline />
      {/if}
      <ChevronDown size={15} class="chevron-rot opacity-65" data-open={expanded ? "true" : "false"} />
    </button>

    <div class="flex items-center gap-1.5">
      {#if activeCount > 0}
        <span transition:badgePop|local class="inline-flex">
          <button
            type="button"
            onclick={handleReset}
            disabled={applying}
            class="gpu-press inline-flex items-center gap-1 rounded-lg border px-2.5 py-1 text-xs font-medium disabled:opacity-45"
            style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text-muted);"
          >
            <RotateCcw size={12} />
            <span>{t.webFilterResetBtn}</span>
          </button>
        </span>
      {/if}
      <button
        type="button"
        onclick={handleApply}
        disabled={applying}
        class="gpu-press inline-flex items-center gap-1.5 rounded-lg px-3 py-1 text-xs font-semibold hover:opacity-90 disabled:opacity-50"
        style="background-color: var(--accent); color: var(--accent-fg);"
      >
        {#if applying}
          <LatticeLoader size="xs" tone="inherit" inline />
        {/if}
        <span>{t.webFilterApplyBtn}</span>
      </button>
    </div>
  </div>

  {#if expanded}
    <div transition:collapseReveal|local class="flex flex-col gap-3.5 border-t pt-3" style="border-color: var(--border);">
      <!-- Baseline Filters: Min Chapters + Updated Within + Dynamic Site Axes -->
      <div class="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <!-- Minimum Chapters -->
        <div class="flex flex-col gap-1.5">
          <span class="text-[11px] font-semibold" style="color: var(--text-muted);">
            {t.webFilterMinChaptersLabel}
          </span>
          <div class="flex flex-wrap items-center gap-1">
            {#each minChapterPresets as preset (String(preset))}
              {@const selected = (draft.minChapters ?? null) === preset}
              <button
                type="button"
                onclick={() => setMinChaptersPreset(preset)}
                class="gpu-press rounded-md border px-2 py-1 text-[11px] font-medium"
                style="border-color: {selected ? 'var(--accent)' : 'var(--border)'}; background-color: {selected
                  ? 'var(--accent-soft)'
                  : 'var(--surface-subtle)'}; color: {selected ? 'var(--accent)' : 'var(--text)'};"
              >
                {preset === null ? t.webFilterAnyOption : `≥${preset}`}
              </button>
            {/each}
            <input
              type="number"
              min="1"
              max="9999"
              bind:value={customMinCh}
              onchange={() => {
                const n = Number.parseInt(customMinCh.trim(), 10);
                draft = { ...draft, minChapters: Number.isFinite(n) && n > 0 ? n : null };
              }}
              placeholder="≥..."
              class="w-16 rounded-md border px-2 py-1 text-[11px] outline-none"
              style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
            />
          </div>
        </div>

        <!-- Update Recency -->
        <label class="flex flex-col gap-1.5">
          <span class="text-[11px] font-semibold" style="color: var(--text-muted);">
            {t.webFilterUpdatedLabel}
          </span>
          <select
            value={draft.updatedWithin ?? "any"}
            onchange={(e) => setUpdatedWithin(e.currentTarget.value as UpdatedWithinFilter)}
            class="rounded-lg border px-2.5 py-1.5 text-xs font-medium outline-none"
            style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
          >
            <option value="any">{t.webFilterAnyOption}</option>
            <option value="last_24h">{t.webFilterUpdated24h}</option>
            <option value="last_7d">{t.webFilterUpdated7d}</option>
            <option value="last_30d">{t.webFilterUpdated30d}</option>
            <option value="last_365d">{t.webFilterUpdated365d}</option>
          </select>
        </label>

        <!-- Dynamic Website-Specific Axes (status, country, sort, demographic, etc.) -->
        {#each schema.extraAxes as axis (axis.key)}
          <label class="flex flex-col gap-1.5">
            <span class="text-[11px] font-semibold" style="color: var(--text-muted);">
              {axis.label}
            </span>
            <select
              value={draft.extra[axis.key] ?? axis.options[0]?.value ?? ""}
              onchange={(e) => setAxisValue(axis.key, e.currentTarget.value)}
              class="rounded-lg border px-2.5 py-1.5 text-xs font-medium outline-none"
              style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
            >
              {#each axis.options as opt (opt.value)}
                <option value={opt.value}>{opt.label}</option>
              {/each}
            </select>
          </label>
        {/each}
      </div>

      <!-- Multi-Tag 3-State Selector (Include / Exclude / Off) -->
      <div class="flex flex-col gap-2">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <div class="flex flex-wrap items-center gap-2">
            <span class="text-[11px] font-semibold" style="color: var(--text-muted);">
              {t.webFilterTagsLabel} ({schema.tags.length})
            </span>
          </div>

          {#if schema.tags.length > 10}
            <div
              class="flex items-center gap-1.5 rounded-lg border px-2 py-1"
              style="background-color: var(--surface-subtle); border-color: var(--border);"
            >
              <Search size={12} style="color: var(--text-muted);" />
              <input
                type="text"
                bind:value={tagSearch}
                placeholder={t.webFilterTagSearchPlaceholder}
                class="w-32 bg-transparent text-[11px] outline-none sm:w-40"
                style="color: var(--text);"
              />
              {#if tagSearch}
                <span transition:badgePop|local class="inline-flex">
                  <button
                    type="button"
                    onclick={() => {
                      tagSearch = "";
                    }}
                    class="gpu-press opacity-70 hover:opacity-100"
                  >
                    <X size={11} />
                  </button>
                </span>
              {/if}
            </div>
          {/if}
        </div>

        {#if loadingSchema && schema.tags.length === 0}
          <div class="flex items-center gap-2 py-1 text-xs" style="color: var(--text-muted);">
            <LatticeLoader size="xs" label={t.webFilterLoadingSchema} showTimer inline />
          </div>
        {:else if visibleTags().length > 0}
          <div class="flex flex-wrap gap-1.5">
            {#each visibleTags() as tag (tag.id)}
              {@const st = tagState(tag.id)}
              <button
                type="button"
                onclick={() => cycleTag(tag.id)}
                class="gpu-press inline-flex items-center gap-1 rounded-lg border px-2.5 py-1 text-xs font-medium"
                style="border-color: {st === 'include'
                  ? 'var(--accent)'
                  : st === 'exclude'
                    ? 'var(--danger)'
                    : 'var(--border)'}; background-color: {st === 'include'
                  ? 'var(--accent-soft)'
                  : st === 'exclude'
                    ? 'rgba(239, 68, 68, 0.14)'
                    : 'var(--surface-subtle)'}; color: {st === 'include'
                  ? 'var(--accent)'
                  : st === 'exclude'
                    ? 'var(--danger)'
                    : 'var(--text)'};"
              >
                {#if st === "include"}
                  <span in:badgePop|local={{ duration: 120, start: 0.6 }} class="inline-flex">
                    <Check size={12} />
                  </span>
                {:else if st === "exclude"}
                  <span in:badgePop|local={{ duration: 120, start: 0.6 }} class="inline-flex">
                    <X size={12} />
                  </span>
                {/if}
                <span>{tag.label}</span>
              </button>
            {/each}

            {#if !tagSearch.trim() && filteredTags().length > 24}
              <button
                type="button"
                onclick={() => {
                  showAllTags = !showAllTags;
                }}
                class="gpu-press rounded-lg border px-2.5 py-1 text-xs font-semibold"
                style="border-color: var(--border); background-color: var(--surface); color: var(--accent);"
              >
                {showAllTags
                  ? t.webFilterShowLessTags
                  : t.webFilterMoreTags(filteredTags().length - 24)}
              </button>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  {/if}
</section>
