<script lang="ts">
  import {
    AlertTriangle,
    Archive,
    BookOpen,
    Check,
    CheckCircle2,
    Compass,
    Download,
    Loader2,
    ShieldCheck,
    SlidersHorizontal,
    Upload,
    X,
  } from "lucide-svelte";
  import { onMount } from "svelte";
  import { api, formatAppError } from "../api";
  import { getLocale } from "../i18n";
  import { backdropFade, createOverlayGuard, modalScale } from "../motion";
  import { libraryCache } from "../stores/libraryCache.svelte";
  import { notificationsStore } from "../stores/notifications.svelte";
  import { router } from "../stores/router.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
  import type {
    BackupBlockSelection,
    BackupImportMode,
    BackupPreviewView,
    BackupProgressEvent,
  } from "../types";

  let {
    onImported,
  }: {
    onImported?: () => void;
  } = $props();

  let selection = $state<BackupBlockSelection>({
    manga: true,
    trackers: true,
    prefs: true,
  });

  let exporting = $state(false);
  let pickingImport = $state(false);
  let importing = $state(false);
  let progress = $state<BackupProgressEvent | null>(null);

  let preview = $state<BackupPreviewView | null>(null);
  let importMode = $state<BackupImportMode>("smart_merge");
  const modalGuard = createOverlayGuard(180);

  const t = $derived(settingsStore.t);
  const locale = $derived(getLocale(settingsStore.value.language));
  const anySelected = $derived(selection.manga || selection.trackers || selection.prefs);
  const busy = $derived(exporting || pickingImport || importing);

  onMount(() => {
    const unregisterBack = router.registerBackInterceptor(() => {
      if (preview && !importing) {
        void closePreview();
        return true;
      }
      return false;
    });
    return () => unregisterBack();
  });

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  }

  function formatTimestamp(ms: number): string {
    if (!Number.isFinite(ms) || ms <= 0) return "—";
    try {
      return new Intl.DateTimeFormat(locale, {
        dateStyle: "medium",
        timeStyle: "short",
      }).format(new Date(ms));
    } catch {
      return new Date(ms).toLocaleString();
    }
  }

  async function handleExport() {
    if (busy) return;
    if (!anySelected) {
      toasts.push(t.rtmagaBackupSelectBlockWarn, "info", 2200);
      return;
    }
    exporting = true;
    progress = { stage: "starting", percent: 5, current: 0, total: 100, detail: t.rtmagaBackupExportingBtn };
    try {
      const res = await api.rtmagaBackupExport({ ...selection }, (ev) => {
        progress = ev;
      });
      if (!res.cancelled && res.saved) {
        toasts.push(
          t.rtmagaBackupExportedToast(res.fileName, formatBytes(res.fileSizeBytes)),
          "success",
          3600
        );
      }
    } catch (err) {
      const f = formatAppError(err, settingsStore.value.language);
      toasts.push(f.detail || f.title, "error", 4200);
    } finally {
      exporting = false;
      progress = null;
    }
  }

  async function handlePickImport() {
    if (busy || !modalGuard.canAct()) return;
    pickingImport = true;
    try {
      const picked = await api.rtmagaBackupPickPreview();
      if (picked) {
        modalGuard.lock();
        importMode = "smart_merge";
        preview = picked;
      }
    } catch (err) {
      const f = formatAppError(err, settingsStore.value.language);
      toasts.push(f.detail || f.title, "error", 4500);
    } finally {
      pickingImport = false;
    }
  }

  async function closePreview() {
    if (importing || !preview) return;
    const snap = preview;
    preview = null;
    if (snap.isTempFile) {
      void api.rtmagaBackupDiscardTemp(snap.filePath).catch(() => {});
    }
  }

  async function confirmImport() {
    if (!preview || importing) return;
    const snap = preview;
    importing = true;
    progress = { stage: "validating", percent: 10, current: 0, total: 100, detail: t.rtmagaBackupImportingBtn };
    try {
      const report = await api.rtmagaBackupImport(
        snap.filePath,
        importMode,
        snap.isTempFile,
        (ev) => {
          progress = ev;
        }
      );
      preview = null;
      libraryCache.invalidate();
      void notificationsStore.refresh();
      if (report.restoredPrefs) {
        await settingsStore.init();
      }
      onImported?.();
      toasts.push(
        t.rtmagaBackupImportedToast(report.addedManga, report.mergedManga, report.mergedChapters),
        "success",
        4200
      );
    } catch (err) {
      const f = formatAppError(err, settingsStore.value.language);
      toasts.push(f.detail || f.title, "error", 4500);
    } finally {
      importing = false;
      progress = null;
    }
  }
</script>

<section
  id="settings-rtmaga-backup"
  class="flex flex-col gap-3.5 rounded-2xl border p-4 shadow-sm sm:p-5"
  style="background-color: var(--surface); border-color: var(--border);"
>
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div class="flex min-w-0 items-start gap-2.5">
      <span
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-xl"
        style="background-color: var(--accent-soft); color: var(--accent);"
      >
        <Archive size={15} />
      </span>
      <div class="min-w-0">
        <div class="flex flex-wrap items-center gap-1.5">
          <h2 class="text-sm font-semibold">{t.rtmagaBackupTitle}</h2>
          <span
            class="rounded-md px-1.5 py-0.5 font-mono text-[10px] font-semibold"
            style="background-color: var(--surface-subtle); color: var(--text-muted);"
          >
            .rtmaga · LZ4 · CRC32
          </span>
        </div>
        <p class="mt-0.5 text-xs leading-relaxed" style="color: var(--text-muted);">
          {t.rtmagaBackupDesc}
        </p>
      </div>
    </div>
  </div>

  <!-- Block Selection Checkboxes -->
  <div class="grid grid-cols-1 gap-2 sm:grid-cols-3">
    <label
      class="flex cursor-pointer items-start gap-2.5 rounded-xl border p-3 transition-colors"
      style="border-color: {selection.manga
        ? 'var(--accent)'
        : 'var(--border)'}; background-color: {selection.manga
        ? 'var(--accent-soft)'
        : 'var(--surface-subtle)'};"
    >
      <input
        type="checkbox"
        bind:checked={selection.manga}
        disabled={busy}
        class="mt-0.5 h-4 w-4 shrink-0 accent-[var(--accent)]"
      />
      <div class="min-w-0">
        <div class="flex items-center gap-1.5 text-xs font-semibold">
          <BookOpen size={13} class="shrink-0" style="color: var(--accent);" />
          <span class="truncate">{t.rtmagaBackupBlockManga}</span>
        </div>
        <div class="mt-0.5 text-[11px] leading-snug" style="color: var(--text-muted);">
          {t.rtmagaBackupBlockMangaDesc}
        </div>
      </div>
    </label>

    <label
      class="flex cursor-pointer items-start gap-2.5 rounded-xl border p-3 transition-colors"
      style="border-color: {selection.trackers
        ? 'var(--accent)'
        : 'var(--border)'}; background-color: {selection.trackers
        ? 'var(--accent-soft)'
        : 'var(--surface-subtle)'};"
    >
      <input
        type="checkbox"
        bind:checked={selection.trackers}
        disabled={busy}
        class="mt-0.5 h-4 w-4 shrink-0 accent-[var(--accent)]"
      />
      <div class="min-w-0">
        <div class="flex items-center gap-1.5 text-xs font-semibold">
          <Compass size={13} class="shrink-0" style="color: var(--accent);" />
          <span class="truncate">{t.rtmagaBackupBlockTrackers}</span>
        </div>
        <div class="mt-0.5 text-[11px] leading-snug" style="color: var(--text-muted);">
          {t.rtmagaBackupBlockTrackersDesc}
        </div>
      </div>
    </label>

    <label
      class="flex cursor-pointer items-start gap-2.5 rounded-xl border p-3 transition-colors"
      style="border-color: {selection.prefs
        ? 'var(--accent)'
        : 'var(--border)'}; background-color: {selection.prefs
        ? 'var(--accent-soft)'
        : 'var(--surface-subtle)'};"
    >
      <input
        type="checkbox"
        bind:checked={selection.prefs}
        disabled={busy}
        class="mt-0.5 h-4 w-4 shrink-0 accent-[var(--accent)]"
      />
      <div class="min-w-0">
        <div class="flex items-center gap-1.5 text-xs font-semibold">
          <SlidersHorizontal size={13} class="shrink-0" style="color: var(--accent);" />
          <span class="truncate">{t.rtmagaBackupBlockPrefs}</span>
        </div>
        <div class="mt-0.5 text-[11px] leading-snug" style="color: var(--text-muted);">
          {t.rtmagaBackupBlockPrefsDesc}
        </div>
      </div>
    </label>
  </div>

  <!-- Non-blocking Tauri Channel Progress Bar -->
  {#if progress}
    <div
      class="flex flex-col gap-1.5 rounded-xl border px-3 py-2.5 text-xs"
      style="border-color: var(--border); background-color: var(--surface-subtle);"
    >
      <div class="flex items-center justify-between gap-2">
        <span class="inline-flex min-w-0 items-center gap-1.5 font-medium">
          <Loader2 size={13} class="shrink-0 animate-spin" style="color: var(--accent);" />
          <span class="truncate">{progress.detail}</span>
        </span>
        <span class="shrink-0 font-mono text-[11px] tabular-nums" style="color: var(--text-muted);">
          {progress.percent}%
        </span>
      </div>
      <div class="h-1.5 w-full overflow-hidden rounded-full" style="background-color: var(--border);">
        <div
          class="h-full w-full origin-left rounded-full transition-transform duration-150 ease-out"
          style="transform: scaleX({Math.max(0.04, Math.min(1, progress.percent / 100))}); background-color: var(--accent);"
        ></div>
      </div>
    </div>
  {/if}

  <!-- Primary Export & Import Actions -->
  <div class="flex flex-wrap items-center gap-2">
    <button
      type="button"
      onclick={handleExport}
      disabled={busy || !anySelected}
      class="gpu-press inline-flex min-h-[42px] flex-1 items-center justify-center gap-2 rounded-xl px-4 py-2 text-xs font-semibold hover:opacity-92 disabled:opacity-45 sm:flex-initial"
      style="background-color: var(--accent); color: var(--accent-fg);"
    >
      {#if exporting}
        <Loader2 size={14} class="animate-spin" />
        <span>{t.rtmagaBackupExportingBtn}</span>
      {:else}
        <Download size={14} />
        <span>{t.rtmagaBackupExportBtn}</span>
      {/if}
    </button>

    <button
      type="button"
      onclick={handlePickImport}
      disabled={busy}
      class="gpu-press inline-flex min-h-[42px] flex-1 items-center justify-center gap-2 rounded-xl border px-4 py-2 text-xs font-semibold hover:opacity-88 disabled:opacity-45 sm:flex-initial"
      style="border-color: var(--border); background-color: var(--surface-subtle); color: var(--text);"
    >
      {#if pickingImport}
        <Loader2 size={14} class="animate-spin" />
        <span>{t.rtmagaBackupImportingBtn}</span>
      {:else}
        <Upload size={14} />
        <span>{t.rtmagaBackupImportBtn}</span>
      {/if}
    </button>
  </div>
</section>

<!-- Import Preview & Smart Merge / Overwrite Modal -->
{#if preview}
  <div
    class="fixed inset-0 z-50 flex items-end justify-center p-0 sm:items-center sm:p-4"
    style="padding-top: max(1rem, var(--safe-top)); padding-left: var(--safe-left); padding-right: var(--safe-right);"
    role="dialog"
    aria-modal="true"
  >
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      transition:backdropFade|local
      onclick={() => void closePreview()}
      class="fixed inset-0 bg-black/60 backdrop-blur-sm"
    ></div>

    <div
      transition:modalScale|local
      class="modal-cq relative z-10 flex max-h-[calc(var(--app-height,100dvh)-1.5rem-var(--safe-top)-var(--ime-bottom))] w-full max-w-lg flex-col gap-4 overflow-y-auto overscroll-contain rounded-t-3xl border-t p-5 sm:max-h-[calc(var(--app-height,100dvh)-2rem-var(--safe-top)-var(--safe-bottom)-var(--ime-bottom))] sm:rounded-2xl sm:border"
      style="background-color: var(--surface); border-color: var(--border); box-shadow: var(--shadow-lg); padding-bottom: max(1.25rem, var(--safe-bottom));"
    >
      <div class="sheet-handle -mt-2 sm:hidden" aria-hidden="true"></div>

      <div class="flex items-start justify-between gap-3">
        <div class="flex min-w-0 items-start gap-2.5">
          <span
            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl"
            style="background-color: var(--accent-soft); color: var(--accent);"
          >
            <Archive size={19} />
          </span>
          <div class="min-w-0">
            <h3 class="text-base font-bold">{t.rtmagaBackupPreviewTitle}</h3>
            <div class="mt-0.5 flex flex-wrap items-center gap-1.5 text-[11px]" style="color: var(--text-muted);">
              <span class="truncate font-mono">{preview.fileName}</span>
              <span
                class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold"
                style="background-color: rgba(16, 185, 129, 0.14); color: #10b981;"
              >
                <ShieldCheck size={11} />
                {t.rtmagaBackupChecksumVerified} · {preview.checksumHex}
              </span>
            </div>
          </div>
        </div>
        <button
          type="button"
          disabled={importing}
          onclick={() => void closePreview()}
          class="flex h-8 w-8 shrink-0 items-center justify-center rounded-xl opacity-75 hover:opacity-100 disabled:opacity-40"
          aria-label={t.cancel}
        >
          <X size={16} />
        </button>
      </div>

      <!-- Verified Header & Payload Stats -->
      <div class="grid grid-cols-3 gap-2">
        <div
          class="rounded-xl border px-3 py-2 text-center"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          <div class="text-base font-bold tabular-nums">{preview.mangaCount}</div>
          <div class="text-[11px]" style="color: var(--text-muted);">{t.rtmagaBackupStatManga}</div>
        </div>
        <div
          class="rounded-xl border px-3 py-2 text-center"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          <div class="text-base font-bold tabular-nums">{preview.interactedChapterCount}</div>
          <div class="text-[11px]" style="color: var(--text-muted);">{t.rtmagaBackupStatChapters}</div>
        </div>
        <div
          class="rounded-xl border px-3 py-2 text-center"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          <div class="text-base font-bold tabular-nums">{preview.trackerCount}</div>
          <div class="text-[11px]" style="color: var(--text-muted);">{t.rtmagaBackupStatSources}</div>
        </div>
      </div>

      <div
        class="flex flex-col gap-1.5 rounded-xl border px-3.5 py-2.5 text-xs"
        style="border-color: var(--border); background-color: var(--surface-subtle);"
      >
        <div class="flex items-center justify-between gap-2">
          <span style="color: var(--text-muted);">{t.rtmagaBackupCreatedAtLabel}</span>
          <span class="font-medium">{formatTimestamp(preview.timestampMs)}</span>
        </div>
        <div class="flex items-center justify-between gap-2">
          <span style="color: var(--text-muted);">{t.rtmagaBackupDeviceLabel}</span>
          <span class="truncate font-medium">
            {preview.deviceName || "—"} · v{preview.appVersion} (fmt v{preview.formatVersion})
          </span>
        </div>
        <div class="flex items-center justify-between gap-2">
          <span style="color: var(--text-muted);">{t.rtmagaBackupArchiveSizeLabel}</span>
          <span class="font-mono text-[11px] font-medium">
            {formatBytes(preview.fileSizeBytes)} ({formatBytes(preview.uncompressedBytes)} raw)
          </span>
        </div>
        <div class="flex items-center justify-between gap-2 pt-0.5">
          <span style="color: var(--text-muted);">{t.rtmagaBackupBlockPrefs}</span>
          <span
            class="inline-flex items-center gap-1 font-medium"
            style="color: {preview.hasPrefs ? '#10b981' : 'var(--text-muted)'};"
          >
            {#if preview.hasPrefs}
              <CheckCircle2 size={12} />
              <span>{t.rtmagaBackupStatPrefsIncluded}</span>
            {:else}
              <span>{t.rtmagaBackupStatPrefsOmitted}</span>
            {/if}
          </span>
        </div>
      </div>

      <!-- Mode Selector: Smart Merge (default) vs Overwrite -->
      <div class="flex flex-col gap-2">
        <div class="text-xs font-semibold">{t.rtmagaBackupModeHeading}</div>
        <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
          <button
            type="button"
            disabled={importing}
            onclick={() => (importMode = "smart_merge")}
            class="flex flex-col items-start gap-1 rounded-xl border p-3 text-left transition-colors"
            style="border-color: {importMode === 'smart_merge'
              ? 'var(--accent)'
              : 'var(--border)'}; background-color: {importMode === 'smart_merge'
              ? 'var(--accent-soft)'
              : 'var(--surface-subtle)'};"
          >
            <div class="flex w-full items-center justify-between gap-1 text-xs font-semibold">
              <span style="color: {importMode === 'smart_merge' ? 'var(--accent)' : 'var(--text)'};">
                {t.rtmagaBackupModeSmartMerge}
              </span>
              {#if importMode === "smart_merge"}
                <Check size={13} style="color: var(--accent);" />
              {/if}
            </div>
            <p class="text-[11px] leading-snug" style="color: var(--text-muted);">
              {t.rtmagaBackupModeSmartMergeDesc}
            </p>
          </button>

          <button
            type="button"
            disabled={importing}
            onclick={() => (importMode = "overwrite")}
            class="flex flex-col items-start gap-1 rounded-xl border p-3 text-left transition-colors"
            style="border-color: {importMode === 'overwrite'
              ? 'var(--danger)'
              : 'var(--border)'}; background-color: {importMode === 'overwrite'
              ? 'rgba(239, 68, 68, 0.08)'
              : 'var(--surface-subtle)'};"
          >
            <div class="flex w-full items-center justify-between gap-1 text-xs font-semibold">
              <span style="color: {importMode === 'overwrite' ? 'var(--danger)' : 'var(--text)'};">
                {t.rtmagaBackupModeOverwrite}
              </span>
              {#if importMode === "overwrite"}
                <Check size={13} style="color: var(--danger);" />
              {/if}
            </div>
            <p class="text-[11px] leading-snug" style="color: var(--text-muted);">
              {t.rtmagaBackupModeOverwriteDesc}
            </p>
          </button>
        </div>

        {#if importMode === "overwrite"}
          <div
            class="flex items-start gap-2 rounded-xl border px-3 py-2 text-[11px]"
            style="border-color: rgba(239, 68, 68, 0.45); background-color: rgba(239, 68, 68, 0.08); color: var(--danger);"
          >
            <AlertTriangle size={14} class="mt-0.5 shrink-0" />
            <span>{t.rtmagaBackupOverwriteWarning}</span>
          </div>
        {/if}
      </div>

      {#if importing && progress}
        <div
          class="flex flex-col gap-1.5 rounded-xl border px-3 py-2 text-xs"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          <div class="flex items-center justify-between gap-2">
            <span class="truncate font-medium">{progress.detail}</span>
            <span class="font-mono text-[11px] tabular-nums">{progress.percent}%</span>
          </div>
          <div class="h-1.5 w-full overflow-hidden rounded-full" style="background-color: var(--border);">
            <div
              class="h-full w-full origin-left rounded-full transition-transform duration-150 ease-out"
              style="transform: scaleX({Math.max(0.05, Math.min(1, progress.percent / 100))}); background-color: var(--accent);"
            ></div>
          </div>
        </div>
      {/if}

      <div class="modal-footer-stack flex items-center justify-end gap-2 pt-1">
        <button
          type="button"
          disabled={importing}
          onclick={() => void closePreview()}
          class="gpu-press min-h-[42px] rounded-xl border px-4 py-2 text-xs font-medium disabled:opacity-45"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          {t.cancel}
        </button>
        <button
          type="button"
          disabled={importing}
          onclick={confirmImport}
          class="gpu-press inline-flex min-h-[42px] items-center justify-center gap-1.5 rounded-xl px-4 py-2 text-xs font-semibold text-white disabled:opacity-50"
          style="background-color: {importMode === 'overwrite' ? 'var(--danger)' : 'var(--accent)'}; color: {importMode === 'overwrite' ? '#ffffff' : 'var(--accent-fg)'};"
        >
          {#if importing}
            <Loader2 size={13} class="animate-spin" />
            <span>{t.rtmagaBackupImportingBtn}</span>
          {:else}
            <Upload size={13} />
            <span>{t.rtmagaBackupConfirmImportBtn}</span>
          {/if}
        </button>
      </div>
    </div>
  </div>
{/if}
