<script lang="ts">
  import {
    Bookmark,
    BookOpen,
    Check,
    Database,
    FolderGit2,
    Info,
    Link2,
    Search,
    Tag,
    X,
  } from "lucide-svelte";
  import { api, formatAppError, imgProxyUrl } from "../api";
  import { getLocale } from "../i18n";
  import {
    adaptiveOverlay,
    backdropFade,
    badgePop,
    createOverlayGuard,
    viewReveal,
  } from "../motion";
  import { deviceEnv } from "../stores/deviceEnv.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
  import type { TachibkBackupPreview, TachibkImportReport, TachibkMangaPreview } from "../types";
  import LatticeLoader from "./LatticeLoader.svelte";

  let {
    preview,
    onClose,
    onImported,
  }: {
    preview: TachibkBackupPreview | null;
    onClose: () => void;
    onImported: (report: TachibkImportReport) => void;
  } = $props();

  let selectedIndices = $state<Set<number>>(new Set());
  let searchQuery = $state("");
  let activeCategory = $state<string>("__all__");
  let matchFilter = $state<"all" | "matched" | "unmatched">("all");
  let syncReadingProgress = $state(true);
  let importMetadataOnly = $state(true);
  let importCategories = $state(true);
  let importBookmarks = $state(true);
  let importing = $state(false);
  let failedCovers = $state<Record<number, boolean>>({});
  let lastPreviewKey = $state<string | null>(null);

  const t = $derived(settingsStore.t);
  const locale = $derived(getLocale(settingsStore.value.language));

  $effect(() => {
    if (!preview) {
      lastPreviewKey = null;
      return;
    }
    const key = `${preview.sourcePath ?? preview.uploadId ?? preview.fileName}:${preview.totalManga}`;
    if (key !== lastPreviewKey) {
      lastPreviewKey = key;
      selectedIndices = new Set(preview.entries.map((e) => e.index));
      searchQuery = "";
      activeCategory = "__all__";
      matchFilter = "all";
      syncReadingProgress = true;
      importMetadataOnly = true;
      importCategories = true;
      importBookmarks = true;
      failedCovers = {};
    }
  });

  function formatFileSize(bytes: number): string {
    if (bytes >= 1024 * 1024) {
      return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
    }
    if (bytes >= 1024) {
      return `${(bytes / 1024).toFixed(1)} KB`;
    }
    return `${bytes} B`;
  }

  function formatDate(unixSecs: number | null): string | null {
    if (!unixSecs || unixSecs <= 0) return null;
    try {
      return new Intl.DateTimeFormat(locale, {
        year: "numeric",
        month: "short",
        day: "numeric",
      }).format(new Date(unixSecs * 1000));
    } catch {
      return null;
    }
  }

  function isMatched(entry: TachibkMangaPreview): boolean {
    return entry.matchKind !== "none";
  }

  const filteredEntries = $derived(() => {
    if (!preview) return [] as TachibkMangaPreview[];
    const q = searchQuery.trim().toLowerCase();
    return preview.entries.filter((entry) => {
      if (matchFilter === "matched" && !isMatched(entry)) return false;
      if (matchFilter === "unmatched" && isMatched(entry)) return false;
      if (activeCategory !== "__all__") {
        const hasCat = entry.categories.some(
          (c) => c.toLowerCase() === activeCategory.toLowerCase()
        );
        if (!hasCat) return false;
      }
      if (!q) return true;
      return (
        entry.title.toLowerCase().includes(q) ||
        (entry.author?.toLowerCase().includes(q) ?? false) ||
        (entry.artist?.toLowerCase().includes(q) ?? false) ||
        (entry.sourceName?.toLowerCase().includes(q) ?? false) ||
        entry.categories.some((c) => c.toLowerCase().includes(q)) ||
        entry.genres.some((g) => g.toLowerCase().includes(q))
      );
    });
  });

  const effectiveSelection = $derived(() => {
    if (!preview) return { indices: [] as number[], matched: 0, catalog: 0 };
    const indices: number[] = [];
    let matched = 0;
    let catalog = 0;
    for (const entry of preview.entries) {
      if (!selectedIndices.has(entry.index)) continue;
      if (isMatched(entry)) {
        indices.push(entry.index);
        matched += 1;
      } else if (importMetadataOnly) {
        indices.push(entry.index);
        catalog += 1;
      }
    }
    return { indices, matched, catalog };
  });

  function toggleIndex(idx: number) {
    const next = new Set(selectedIndices);
    if (next.has(idx)) {
      next.delete(idx);
    } else {
      next.add(idx);
    }
    selectedIndices = next;
  }

  function selectAllVisible() {
    const next = new Set(selectedIndices);
    for (const entry of filteredEntries()) {
      next.add(entry.index);
    }
    selectedIndices = next;
  }

  function clearAllVisible() {
    const next = new Set(selectedIndices);
    for (const entry of filteredEntries()) {
      next.delete(entry.index);
    }
    selectedIndices = next;
  }

  const overlayGuard = createOverlayGuard(180);

  function handleCancel() {
    if (importing || !overlayGuard.canAct()) return;
    overlayGuard.lock();
    if (preview?.uploadId) {
      void api.discardTachibkUpload(preview.uploadId).catch(() => {});
    }
    onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (preview && e.key === "Escape" && !importing) {
      e.preventDefault();
      handleCancel();
    }
  }

  async function handleConfirmImport() {
    if (!preview || importing) return;
    const { indices } = effectiveSelection();
    if (indices.length === 0) return;

    importing = true;
    try {
      const report = await api.importTachibkBackup({
        sourcePath: preview.sourcePath,
        uploadId: preview.uploadId,
        selectedIndices: indices,
        syncReadingProgress,
        importMetadataOnly,
        importCategories,
        importBookmarks,
      });
      onImported(report);
    } catch (err) {
      const formatted = formatAppError(err, settingsStore.value.language);
      toasts.push(formatted.detail || formatted.title, "error", 5000);
    } finally {
      importing = false;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if preview}
  <div
    class="fixed inset-0 z-50 flex items-end justify-center p-0 sm:items-center sm:p-5"
    style="padding-top: max(0.5rem, var(--safe-top)); padding-left: var(--safe-left); padding-right: var(--safe-right);"
    role="dialog"
    aria-modal="true"
    aria-labelledby="tachibk-modal-title"
  >
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      transition:backdropFade|local
      onclick={handleCancel}
      class="modal-backdrop fixed inset-0"
    ></div>

    <div
      transition:adaptiveOverlay|local={{ isMobile: deviceEnv.layoutTier === "compact" }}
      class="modal-cq relative z-10 flex max-h-[calc(100dvh-1rem-var(--safe-top))] w-full max-w-3xl flex-col overflow-hidden rounded-t-3xl border-t sm:max-h-[calc(100dvh-2.5rem-var(--safe-top)-var(--safe-bottom))] sm:rounded-2xl sm:border"
      style="background-color: var(--surface); border-color: var(--border); color: var(--text); box-shadow: var(--shadow-lg); padding-bottom: var(--safe-bottom);"
    >
      <div class="sheet-handle sm:hidden" aria-hidden="true"></div>
      <!-- Header -->
      <div
        class="flex shrink-0 items-start justify-between gap-3 border-b px-5 py-4"
        style="border-color: var(--border);"
      >
        <div class="flex min-w-0 items-center gap-3">
          <div
            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl"
            style="background-color: var(--accent-soft); color: var(--accent);"
          >
            <Database size={19} />
          </div>
          <div class="min-w-0">
            <h2 id="tachibk-modal-title" class="truncate text-base font-bold leading-snug">
              {t.tachibkModalTitle}
            </h2>
            <div
              class="mt-0.5 flex flex-wrap items-center gap-1.5 text-xs"
              style="color: var(--text-muted);"
            >
              <span class="truncate font-medium">{preview.fileName}</span>
              <span>·</span>
              <span>{formatFileSize(preview.fileSize)}</span>
            </div>
          </div>
        </div>

        {#if !importing}
          <button
            type="button"
            onclick={handleCancel}
            class="gpu-press rounded-lg p-1.5 opacity-70 hover:opacity-100"
            aria-label={t.cancel}
          >
            <X size={17} />
          </button>
        {/if}
      </div>

      <!-- Body -->
      <div class="flex min-h-0 flex-1 flex-col gap-3.5 overflow-hidden px-5 py-4">
        <!-- Metadata-only clarification badge/note -->
        <div
          class="flex shrink-0 items-start gap-2.5 rounded-xl border px-3.5 py-2.5 text-xs"
          style="background-color: var(--surface-subtle); border-color: var(--border);"
        >
          <Info size={15} class="mt-0.5 shrink-0" style="color: var(--accent);" />
          <p class="leading-relaxed" style="color: var(--text-muted);">
            {t.tachibkMetadataNote}
          </p>
        </div>

        <!-- Backup Stats -->
        <div class="grid shrink-0 grid-cols-2 gap-2 sm:grid-cols-4">
          <div
            class="rounded-xl border px-3 py-2"
            style="background-color: var(--surface-subtle); border-color: var(--border);"
          >
            <div class="text-[11px]" style="color: var(--text-muted);">{t.tachibkStatTitles}</div>
            <div class="text-base font-bold">{preview.totalManga}</div>
          </div>
          <div
            class="rounded-xl border px-3 py-2"
            style="background-color: var(--surface-subtle); border-color: var(--border);"
          >
            <div class="text-[11px]" style="color: var(--text-muted);">{t.tachibkStatMatched}</div>
            <div class="flex items-baseline gap-1.5">
              <span class="text-base font-bold" style="color: #10b981;">
                {preview.matchedExistingCount}
              </span>
              {#if preview.matchedLocalCount > 0}
                <span class="text-[10px] font-semibold" style="color: var(--text-muted);">
                  ({preview.matchedLocalCount} local)
                </span>
              {/if}
            </div>
          </div>
          <div
            class="rounded-xl border px-3 py-2"
            style="background-color: var(--surface-subtle); border-color: var(--border);"
          >
            <div class="text-[11px]" style="color: var(--text-muted);">
              {t.tachibkStatCategories}
            </div>
            <div class="text-base font-bold">{preview.categories.length}</div>
          </div>
          <div
            class="rounded-xl border px-3 py-2"
            style="background-color: var(--surface-subtle); border-color: var(--border);"
          >
            <div class="text-[11px]" style="color: var(--text-muted);">
              {t.tachibkStatReadChapters}
            </div>
            <div class="text-base font-bold">
              {preview.totalReadChapters}
              <span class="text-xs font-normal" style="color: var(--text-muted);">
                / {preview.totalChapters}
              </span>
            </div>
          </div>
        </div>

        <!-- Import Option Toggles -->
        <div class="flex shrink-0 flex-wrap items-center gap-2">
          <label
            class="inline-flex cursor-pointer items-center gap-2 rounded-xl border px-3 py-1.5 text-xs font-medium transition-colors"
            style="background-color: {syncReadingProgress
              ? 'var(--accent-soft)'
              : 'var(--surface-subtle)'}; border-color: {syncReadingProgress
              ? 'var(--accent)'
              : 'var(--border)'}; color: {syncReadingProgress ? 'var(--accent)' : 'var(--text)'};"
          >
            <input
              type="checkbox"
              bind:checked={syncReadingProgress}
              disabled={importing}
              class="accent-[var(--accent)]"
            />
            <span>{t.tachibkOptSyncProgress}</span>
          </label>

          <label
            class="inline-flex cursor-pointer items-center gap-2 rounded-xl border px-3 py-1.5 text-xs font-medium transition-colors"
            style="background-color: {importMetadataOnly
              ? 'var(--accent-soft)'
              : 'var(--surface-subtle)'}; border-color: {importMetadataOnly
              ? 'var(--accent)'
              : 'var(--border)'}; color: {importMetadataOnly ? 'var(--accent)' : 'var(--text)'};"
          >
            <input
              type="checkbox"
              bind:checked={importMetadataOnly}
              disabled={importing}
              class="accent-[var(--accent)]"
            />
            <span>{t.tachibkOptImportCatalog}</span>
          </label>

          <label
            class="inline-flex cursor-pointer items-center gap-2 rounded-xl border px-3 py-1.5 text-xs font-medium transition-colors"
            style="background-color: {importCategories
              ? 'var(--accent-soft)'
              : 'var(--surface-subtle)'}; border-color: {importCategories
              ? 'var(--accent)'
              : 'var(--border)'}; color: {importCategories ? 'var(--accent)' : 'var(--text)'};"
          >
            <input
              type="checkbox"
              bind:checked={importCategories}
              disabled={importing}
              class="accent-[var(--accent)]"
            />
            <span>{t.tachibkOptImportCategories}</span>
          </label>

          <label
            class="inline-flex cursor-pointer items-center gap-2 rounded-xl border px-3 py-1.5 text-xs font-medium transition-colors"
            style="background-color: {importBookmarks
              ? 'var(--accent-soft)'
              : 'var(--surface-subtle)'}; border-color: {importBookmarks
              ? 'var(--accent)'
              : 'var(--border)'}; color: {importBookmarks ? 'var(--accent)' : 'var(--text)'};"
          >
            <input
              type="checkbox"
              bind:checked={importBookmarks}
              disabled={importing}
              class="accent-[var(--accent)]"
            />
            <span>{t.tachibkOptImportBookmarks}</span>
          </label>
        </div>

        <!-- Search & Match / Category Filters -->
        <div class="flex shrink-0 flex-col gap-2">
          <div class="flex flex-wrap items-center gap-2">
            <div
              class="flex min-w-44 flex-1 items-center gap-2 rounded-xl border px-3 py-1.5 text-xs"
              style="background-color: var(--surface-subtle); border-color: var(--border);"
            >
              <Search size={14} style="color: var(--text-muted);" />
              <input
                type="text"
                bind:value={searchQuery}
                placeholder={t.tachibkSearchPlaceholder}
                class="w-full bg-transparent text-xs outline-none"
              />
            </div>

            <div
              class="inline-flex rounded-xl border p-0.5 text-[11px] font-medium"
              style="background-color: var(--surface-subtle); border-color: var(--border);"
            >
              {#each [
                { key: "all", label: t.tachibkFilterAll },
                { key: "matched", label: `${t.tachibkFilterMatched} (${preview.matchedExistingCount})` },
                { key: "unmatched", label: `${t.tachibkFilterUnmatched} (${preview.unmatchedCount})` },
              ] as tab (tab.key)}
                <button
                  type="button"
                  onclick={() => {
                    matchFilter = tab.key as "all" | "matched" | "unmatched";
                  }}
                  class="gpu-press rounded-lg px-2.5 py-1 transition-colors"
                  style="background-color: {matchFilter === tab.key
                    ? 'var(--accent)'
                    : 'transparent'}; color: {matchFilter === tab.key
                    ? 'var(--accent-fg)'
                    : 'var(--text-muted)'};"
                >
                  {tab.label}
                </button>
              {/each}
            </div>

            <div class="flex items-center gap-1.5 text-xs">
              <button
                type="button"
                onclick={selectAllVisible}
                disabled={importing}
                class="gpu-press rounded-lg border px-2.5 py-1 text-[11px] font-medium hover:opacity-85"
                style="border-color: var(--border); background-color: var(--surface-subtle);"
              >
                {t.tachibkSelectVisible}
              </button>
              <button
                type="button"
                onclick={clearAllVisible}
                disabled={importing}
                class="gpu-press rounded-lg border px-2.5 py-1 text-[11px] font-medium hover:opacity-85"
                style="border-color: var(--border); background-color: var(--surface-subtle); color: var(--text-muted);"
              >
                {t.tachibkClearVisible}
              </button>
            </div>
          </div>

          {#if preview.categories.length > 0}
            <div class="flex flex-wrap items-center gap-1.5">
              <Tag size={12} style="color: var(--text-muted);" />
              <button
                type="button"
                onclick={() => {
                  activeCategory = "__all__";
                }}
                class="gpu-press rounded-lg border px-2 py-0.5 text-[11px] font-medium transition-colors"
                style="border-color: {activeCategory === '__all__'
                  ? 'var(--accent)'
                  : 'var(--border)'}; background-color: {activeCategory === '__all__'
                  ? 'var(--accent-soft)'
                  : 'var(--surface-subtle)'}; color: {activeCategory === '__all__'
                  ? 'var(--accent)'
                  : 'var(--text-muted)'};"
              >
                {t.tachibkFilterAll}
              </button>
              {#each preview.categories as cat (cat.name)}
                <button
                  type="button"
                  onclick={() => {
                    activeCategory = cat.name;
                  }}
                  class="gpu-press rounded-lg border px-2 py-0.5 text-[11px] font-medium transition-colors"
                  style="border-color: {activeCategory === cat.name
                    ? 'var(--accent)'
                    : 'var(--border)'}; background-color: {activeCategory === cat.name
                    ? 'var(--accent-soft)'
                    : 'var(--surface-subtle)'}; color: {activeCategory === cat.name
                    ? 'var(--accent)'
                    : 'var(--text-muted)'};"
                >
                  {cat.name} ({cat.mangaCount})
                </button>
              {/each}
            </div>
          {/if}
        </div>

        <!-- Scrollable Manga Selection List -->
        <div
          class="flex min-h-40 flex-1 flex-col gap-1.5 overflow-y-auto overscroll-contain rounded-xl border p-2"
          style="background-color: var(--surface-subtle); border-color: var(--border);"
        >
          {#if filteredEntries().length === 0}
            <div
              in:viewReveal|local={{ duration: 150, y: 4 }}
              class="flex flex-1 flex-col items-center justify-center gap-2 py-10 text-xs"
              style="color: var(--text-muted);"
            >
              <BookOpen size={24} strokeWidth={1.5} />
              <span>{t.tachibkEmptyEntries}</span>
            </div>
          {:else}
            {#each filteredEntries() as entry (entry.index)}
              {@const checked = selectedIndices.has(entry.index)}
              {@const skippedByOpt = !isMatched(entry) && !importMetadataOnly}
              {@const lastReadLabel = formatDate(entry.lastReadAt)}
              <button
                type="button"
                disabled={importing}
                onclick={() => toggleIndex(entry.index)}
                class="gpu-press flex w-full items-start gap-3 rounded-xl border px-3 py-2.5 text-left hover:opacity-95"
                style="background-color: {checked && !skippedByOpt
                  ? 'var(--surface)'
                  : 'transparent'}; border-color: {checked && !skippedByOpt
                  ? 'var(--accent)'
                  : 'var(--border)'}; opacity: {skippedByOpt ? 0.52 : 1};"
              >
                <!-- Selection checkbox visual -->
                <span
                  class="mt-1 flex h-4.5 w-4.5 shrink-0 items-center justify-center rounded-md border transition-colors"
                  style="border-color: {checked
                    ? 'var(--accent)'
                    : 'var(--border)'}; background-color: {checked
                    ? 'var(--accent)'
                    : 'var(--surface)'}; color: var(--accent-fg);"
                >
                  {#if checked}
                    <span in:badgePop|local={{ duration: 120, start: 0.55 }} class="inline-flex">
                      <Check size={12} strokeWidth={3} />
                    </span>
                  {/if}
                </span>

                <!-- Cover thumbnail -->
                <div
                  class="h-14 w-10 shrink-0 overflow-hidden rounded-lg border"
                  style="background-color: var(--surface-hover); border-color: var(--border);"
                >
                  {#if entry.coverToken && !failedCovers[entry.index]}
                    <img
                      src={imgProxyUrl(entry.coverToken)}
                      alt={entry.title}
                      loading="lazy"
                      decoding="async"
                      onerror={() => {
                        failedCovers = { ...failedCovers, [entry.index]: true };
                      }}
                      class="h-full w-full object-cover"
                    />
                  {:else}
                    <div
                      class="flex h-full w-full items-center justify-center"
                      style="color: var(--text-muted);"
                    >
                      <BookOpen size={15} />
                    </div>
                  {/if}
                </div>

                <!-- Details -->
                <div class="flex min-w-0 flex-1 flex-col gap-1">
                  <div class="flex flex-wrap items-center gap-1.5">
                    <span class="truncate text-xs font-semibold sm:text-sm">{entry.title}</span>

                    {#if entry.matchKind === "local_library"}
                      <span
                        class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold"
                        style="background-color: rgba(16, 185, 129, 0.14); color: #10b981;"
                      >
                        <Link2 size={10} />
                        {t.tachibkMatchLocalBadge}
                      </span>
                    {:else if entry.matchKind === "web_library"}
                      <span
                        class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold"
                        style="background-color: var(--accent-soft); color: var(--accent);"
                      >
                        <Link2 size={10} />
                        {t.tachibkMatchLibraryBadge}
                      </span>
                    {:else if entry.matchKind === "catalog_entry"}
                      <span
                        class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold"
                        style="background-color: var(--accent-soft); color: var(--accent);"
                      >
                        <FolderGit2 size={10} />
                        {t.tachibkMatchCatalogBadge}
                      </span>
                    {:else if skippedByOpt}
                      <span
                        class="rounded-full px-2 py-0.5 text-[10px] font-medium"
                        style="background-color: var(--surface-hover); color: var(--text-muted);"
                      >
                        {t.tachibkSkippedHint}
                      </span>
                    {:else}
                      <span
                        class="rounded-full px-2 py-0.5 text-[10px] font-medium"
                        style="background-color: var(--surface-hover); color: var(--text-muted);"
                      >
                        {t.tachibkMetadataOnlyBadge}
                      </span>
                    {/if}
                  </div>

                  <div
                    class="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11px]"
                    style="color: var(--text-muted);"
                  >
                    {#if entry.author}
                      <span class="truncate">{entry.author}</span>
                    {/if}
                    {#if entry.sourceName}
                      <span>· {entry.sourceName}</span>
                    {/if}
                    {#if entry.status}
                      <span class="capitalize">· {entry.status}</span>
                    {/if}
                    <span>· {t.tachibkChaptersReadProgress(entry.readChapters, entry.totalChapters)}</span>
                    {#if entry.bookmarkedChapters > 0}
                      <span class="inline-flex items-center gap-0.5">
                        · <Bookmark size={10} /> {entry.bookmarkedChapters}
                      </span>
                    {/if}
                    {#if lastReadLabel}
                      <span>· {lastReadLabel}</span>
                    {/if}
                  </div>

                  {#if entry.categories.length > 0}
                    <div class="flex flex-wrap items-center gap-1 pt-0.5">
                      {#each entry.categories as cat (cat)}
                        <span
                          class="rounded-md px-1.5 py-0.5 text-[10px] font-medium"
                          style="background-color: var(--surface-hover); color: var(--text-muted);"
                        >
                          {cat}
                        </span>
                      {/each}
                    </div>
                  {/if}
                </div>
              </button>
            {/each}
          {/if}
        </div>
      </div>

      <!-- Footer -->
      <div
        class="flex shrink-0 flex-wrap items-center justify-between gap-3 border-t px-5 py-3.5"
        style="border-color: var(--border);"
      >
        <div class="text-xs font-medium" style="color: var(--text-muted);">
          {t.tachibkImportSummary(effectiveSelection().matched, effectiveSelection().catalog)}
        </div>

        <div class="flex items-center gap-2">
          <button
            type="button"
            disabled={importing}
            onclick={handleCancel}
            class="gpu-press rounded-xl border px-4 py-2 text-xs font-medium hover:opacity-85 disabled:opacity-50"
            style="border-color: var(--border); background-color: var(--surface-subtle);"
          >
            {t.cancel}
          </button>

          <button
            type="button"
            disabled={importing || effectiveSelection().indices.length === 0}
            onclick={() => void handleConfirmImport()}
            class="gpu-press inline-flex items-center gap-1.5 rounded-xl px-4 py-2 text-xs font-semibold hover:opacity-90 disabled:opacity-45"
            style="background-color: var(--accent); color: var(--accent-fg);"
          >
            {#if importing}
              <LatticeLoader size="xs" status="loading" tone="inherit" inline />
              <span>{t.tachibkImportingBtn}</span>
            {:else}
              <Database size={14} />
              <span>{t.tachibkImportBtn(effectiveSelection().indices.length)}</span>
            {/if}
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}
