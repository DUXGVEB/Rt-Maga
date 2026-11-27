<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { open as openNativeDialog } from "@tauri-apps/plugin-dialog";
  import { FileArchive, FolderOpen, Upload } from "lucide-svelte";
  import { onMount } from "svelte";
  import { api, formatAppError } from "../api";
  import {
    BROWSER_FILE_ACCEPT,
    COMIC_DIALOG_FILTERS,
    isBackupFilename,
    shouldUploadBrowserFile,
  } from "../comicFormats";
  import { libraryCache } from "../stores/libraryCache.svelte";
  import { router } from "../stores/router.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
  import type { MangaView, TachibkBackupPreview, TachibkImportReport } from "../types";
  import LatticeLoader, { type LatticeStatus } from "./LatticeLoader.svelte";
  import TachibkImportModal from "./TachibkImportModal.svelte";

  let {
    targetMangaId = null,
    compact = false,
    onImported,
    onBackupImported,
  }: {
    targetMangaId?: number | null;
    compact?: boolean;
    onImported?: (manga: MangaView) => void;
    onBackupImported?: (report: TachibkImportReport) => void;
  } = $props();

  let importing = $state(false);
  let uploadStatus = $state<LatticeStatus | "idle">("idle");
  let dragOver = $state(false);
  let platform = $state<"windows" | "android" | "other">("windows");
  let fileInputEl = $state<HTMLInputElement | null>(null);
  let folderInputEl = $state<HTMLInputElement | null>(null);
  let backupPreview = $state<TachibkBackupPreview | null>(null);
  let statusResetTimer: number | null = null;

  const t = $derived(settingsStore.t);

  function scheduleStatusReset(ms = 1400) {
    if (statusResetTimer !== null) window.clearTimeout(statusResetTimer);
    statusResetTimer = window.setTimeout(() => {
      uploadStatus = "idle";
      statusResetTimer = null;
    }, ms);
  }

  function showImportError(err: unknown) {
    uploadStatus = "error";
    scheduleStatusReset(2400);
    const formatted = formatAppError(err, settingsStore.value.language);
    const message =
      formatted.title && !formatted.detail.toLowerCase().includes(formatted.title.toLowerCase())
        ? `${formatted.title}: ${formatted.detail}`
        : formatted.detail;
    toasts.push(message, "error", 5000);
  }

  onMount(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;

    void api
      .getPlatform()
      .then((p) => {
        if (!disposed) platform = p;
      })
      .catch(() => {});

    void (async () => {
      try {
        const fn = await getCurrentWindow().onDragDropEvent((event) => {
          if (disposed || importing) return;
          if (event.payload.type === "over" || event.payload.type === "enter") {
            dragOver = true;
          } else if (event.payload.type === "leave") {
            dragOver = false;
          } else if (event.payload.type === "drop") {
            dragOver = false;
            const droppedPaths = event.payload.paths.filter((p) => p.trim().length > 0);
            if (droppedPaths.length > 0) {
              // Hand all dropped files & folders to Rust magic-byte detection so mislabeled
              // archives (e.g. a RAR named .cbz) and chapter/series folders import seamlessly.
              void importDroppedPaths(droppedPaths);
            }
          }
        });
        if (disposed) {
          fn();
        } else {
          unlisten = fn;
        }
      } catch {
        // Non-Tauri preview or unsupported platform: file picker buttons still work.
      }
    })();

    return () => {
      disposed = true;
      if (statusResetTimer !== null) window.clearTimeout(statusResetTimer);
      if (unlisten) unlisten();
    };
  });

  async function finishImport(manga: MangaView) {
    libraryCache.invalidate();
    uploadStatus = "success";
    scheduleStatusReset(1200);
    toasts.push(t.cbzImportedToast(manga.title, manga.chapters.length), "success", 3000);
    if (onImported) {
      onImported(manga);
    } else {
      router.goManga(manga.id);
    }
  }

  async function importDroppedPaths(paths: string[]) {
    if (importing || paths.length === 0) return;
    importing = true;
    uploadStatus = "loading";
    try {
      let backupPaths: string[] = [];
      if (!compact) {
        backupPaths = await api.detectTachibkPaths(paths).catch(() => []);
      }

      const backupSet = new Set(backupPaths);
      const comicPaths = paths.filter((p) => !backupSet.has(p));

      if (comicPaths.length > 0) {
        const manga = await api.importCbzPaths(comicPaths, targetMangaId);
        if (backupPaths.length > 0) {
          libraryCache.invalidate();
          toasts.push(t.cbzImportedToast(manga.title, manga.chapters.length), "success", 3000);
        } else {
          await finishImport(manga);
        }
      }

      if (backupPaths.length > 0) {
        const preview = await api.previewTachibkBackup(backupPaths[0]);
        uploadStatus = "idle";
        backupPreview = preview;
      }
    } catch (err) {
      showImportError(err);
    } finally {
      importing = false;
    }
  }

  function handleBackupImportComplete(report: TachibkImportReport) {
    backupPreview = null;
    libraryCache.invalidate();
    uploadStatus = "success";
    scheduleStatusReset(1400);
    const matchedTotal = report.linkedLocalCount + report.updatedExistingCount;
    toasts.push(
      t.tachibkImportedToast(matchedTotal, report.createdCatalogCount, report.syncedChaptersRead),
      "success",
      4200
    );
    onBackupImported?.(report);
  }

  function normalizeDialogSelection(selected: string | string[] | null): string[] {
    if (!selected) return [];
    return Array.isArray(selected) ? selected.filter((p) => p.trim().length > 0) : [selected];
  }

  async function handlePickFilesClick() {
    if (importing) return;
    if (platform !== "android") {
      try {
        const picked = await openNativeDialog({
          multiple: true,
          directory: false,
          filters: COMIC_DIALOG_FILTERS,
        });
        const paths = normalizeDialogSelection(picked);
        if (paths.length > 0) {
          await importDroppedPaths(paths);
        }
        return;
      } catch {
        // Fallback to native Win32 picker command if dialog plugin is unavailable.
        if (platform === "windows") {
          importing = true;
          uploadStatus = "loading";
          try {
            const manga = await api.pickAndImportCbz(targetMangaId);
            if (manga) {
              await finishImport(manga);
            } else {
              uploadStatus = "idle";
            }
          } catch (err) {
            showImportError(err);
          } finally {
            importing = false;
          }
          return;
        }
      }
    }
    fileInputEl?.click();
  }

  async function handlePickFolderClick(e?: MouseEvent) {
    e?.stopPropagation();
    if (importing) return;
    if (platform !== "android") {
      try {
        const picked = await openNativeDialog({
          multiple: true,
          directory: true,
        });
        const paths = normalizeDialogSelection(picked);
        if (paths.length > 0) {
          await importDroppedPaths(paths);
        }
        return;
      } catch {
        // Fall through to WebView folder input if native dialog unavailable.
      }
    }
    folderInputEl?.click();
  }

  async function handleBrowserFileChange(e: Event, isFolderInput = false) {
    const input = e.currentTarget as HTMLInputElement;
    const rawFiles = Array.from(input.files ?? []);
    input.value = "";
    if (rawFiles.length === 0 || importing) return;

    const files = rawFiles.filter((f) => shouldUploadBrowserFile(f, isFolderInput));
    if (files.length === 0) {
      toasts.push(t.cbzNoSupportedFilesToast, "error", 3800);
      return;
    }

    importing = true;
    uploadStatus = "loading";
    try {
      const backupFile =
        !compact && !isFolderInput
          ? files.find((f) => isBackupFilename(f.name) || (files.length === 1 && f.name.toLowerCase().endsWith(".gz")))
          : undefined;
      const comicFiles = backupFile ? files.filter((f) => f !== backupFile) : files;

      if (comicFiles.length > 0) {
        const manga = await api.importCbzBrowserFiles(comicFiles, targetMangaId);
        if (backupFile) {
          libraryCache.invalidate();
          toasts.push(t.cbzImportedToast(manga.title, manga.chapters.length), "success", 3000);
        } else {
          await finishImport(manga);
        }
      }

      if (backupFile) {
        const preview = await api.uploadTachibkBrowserFile(backupFile);
        uploadStatus = "idle";
        backupPreview = preview;
      }
    } catch (err) {
      showImportError(err);
    } finally {
      importing = false;
    }
  }
</script>

<input
  bind:this={fileInputEl}
  type="file"
  accept={BROWSER_FILE_ACCEPT}
  multiple
  onchange={(e) => void handleBrowserFileChange(e, false)}
  class="hidden"
/>

<input
  bind:this={folderInputEl}
  type="file"
  webkitdirectory
  multiple
  onchange={(e) => void handleBrowserFileChange(e, true)}
  class="hidden"
/>

{#if compact}
  <div class="inline-flex flex-wrap items-center gap-1.5">
    <button
      type="button"
      disabled={importing}
      onclick={() => void handlePickFilesClick()}
      class="inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-semibold transition-colors hover:brightness-95 disabled:opacity-55"
      style="background-color: var(--accent-soft); border-color: var(--accent); color: var(--accent);"
    >
      {#if importing}
        <LatticeLoader size="xs" status="loading" tone="inherit" inline />
        <span>{t.cbzImporting}</span>
      {:else}
        <FileArchive size={14} />
        <span>{t.cbzAddChaptersBtn}</span>
      {/if}
    </button>
    {#if !importing}
      <button
        type="button"
        onclick={(e) => void handlePickFolderClick(e)}
        class="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1.5 text-xs font-medium transition-colors hover:opacity-90"
        style="background-color: var(--surface); border-color: var(--border); color: var(--text-muted);"
        title={t.cbzPickFolderBtn}
      >
        <FolderOpen size={13} />
        <span>{t.cbzPickFolderBtn}</span>
      </button>
    {/if}
  </div>
{:else}
  <!-- Drag-and-Drop Upload Zone (#032) -->
  <div
    role="button"
    tabindex={importing ? -1 : 0}
    aria-disabled={importing}
    aria-label={t.cbzDropTitle}
    onclick={() => void handlePickFilesClick()}
    onkeydown={(e) => {
      if ((e.key === "Enter" || e.key === " ") && !importing) {
        e.preventDefault();
        void handlePickFilesClick();
      }
    }}
    class="dropzone-032 group relative w-full max-w-2xl cursor-pointer overflow-hidden rounded-2xl border-2 border-dashed px-5 py-4 text-left focus-visible:outline-2 focus-visible:outline-offset-2"
    class:dropzone-032-active={dragOver}
    class:dropzone-032-busy={importing}
  >
    <!-- Subtle architectural corner brackets (#032 signature framing) -->
    <span class="dz-corner dz-tl" aria-hidden="true"></span>
    <span class="dz-corner dz-tr" aria-hidden="true"></span>
    <span class="dz-corner dz-bl" aria-hidden="true"></span>
    <span class="dz-corner dz-br" aria-hidden="true"></span>

    <div class="relative z-10 flex flex-col gap-3.5 sm:flex-row sm:flex-wrap sm:items-center sm:justify-between sm:gap-4">
      <div class="flex min-w-0 flex-1 items-center gap-3.5">
        <!-- Layered tactile archive badge -->
        <div class="dz-icon-stage" aria-hidden="true">
          <span class="dz-sheet dz-sheet-back"></span>
          <span class="dz-sheet dz-sheet-mid"></span>
          <span class="dz-sheet dz-sheet-front">
            {#if uploadStatus !== "idle"}
              <LatticeLoader size="sm" status={uploadStatus} tone="accent" />
            {:else}
              <FileArchive size={18} strokeWidth={1.9} />
            {/if}
          </span>
        </div>

        <div class="min-w-0 flex-1">
          <div class="flex flex-wrap items-center gap-1.5">
            <span class="text-xs font-semibold sm:text-sm" style="color: var(--text);">
              {importing ? t.cbzImporting : t.cbzDropTitle}
            </span>
            {#if importing}
              <LatticeLoader size="xs" status="loading" showTimer inline />
            {/if}
          </div>
          <div class="mt-1.5 flex flex-wrap items-center gap-1">
            <span class="dz-ext-pill">.CBZ</span>
            <span class="dz-ext-pill">.CBR</span>
            <span class="dz-ext-pill">.CB7</span>
            <span class="dz-ext-pill">.CBT</span>
            <span class="dz-ext-pill">.EPUB</span>
            <span class="dz-ext-pill">.PDF</span>
            <span class="dz-ext-pill">.TACHIBK</span>
            <span class="dz-ext-pill dz-ext-pill-subtle">IMG / DIR</span>
          </div>
        </div>
      </div>

      <div class="flex w-full shrink-0 items-center gap-2 sm:w-auto">
        {#if !importing}
          <button
            type="button"
            onclick={(e) => void handlePickFolderClick(e)}
            class="dz-cta-secondary gpu-press inline-flex min-h-[42px] flex-1 items-center justify-center gap-1.5 rounded-xl border px-3 py-2 text-xs font-semibold hover:opacity-90 sm:min-h-[38px] sm:flex-initial"
            title={t.cbzPickFolderBtn}
          >
            <FolderOpen size={14} />
            <span>{t.cbzPickFolderBtn}</span>
          </button>
        {/if}

        <span
          class="dz-cta gpu-press inline-flex min-h-[42px] flex-1 shrink-0 items-center justify-center gap-1.5 rounded-xl px-3.5 py-2 text-xs font-semibold group-hover:scale-[1.02] sm:min-h-[38px] sm:flex-initial"
        >
          {#if importing}
            <LatticeLoader size="xs" status="loading" tone="inherit" inline />
            <span>{t.cbzImporting}</span>
          {:else}
            <Upload size={14} />
            <span>{t.cbzPickBtn}</span>
          {/if}
        </span>
      </div>
    </div>
  </div>
{/if}

<TachibkImportModal
  preview={backupPreview}
  onClose={() => {
    backupPreview = null;
    uploadStatus = "idle";
  }}
  onImported={handleBackupImportComplete}
/>

<style>
  .dropzone-032 {
    background-color: var(--surface);
    border-color: var(--border-strong);
    box-shadow: var(--shadow-sm);
    outline-color: var(--accent);
    transition:
      transform var(--dur-base, 210ms) var(--ease-out, cubic-bezier(0.22, 1, 0.36, 1)),
      opacity var(--dur-fast, 150ms) ease;
  }

  .dropzone-032:hover:not(.dropzone-032-busy),
  .dropzone-032-active {
    border-color: var(--accent);
    background-color: color-mix(in srgb, var(--surface) 88%, var(--accent-soft));
  }

  .dropzone-032-active {
    transform: translate3d(0, -1px, 0) scale(1.006);
  }

  .dropzone-032-busy {
    cursor: progress;
    border-color: var(--accent);
  }

  .dz-corner {
    position: absolute;
    width: 10px;
    height: 10px;
    border-color: var(--border-strong);
    pointer-events: none;
    transition: transform 220ms var(--ease-spring, cubic-bezier(0.34, 1.4, 0.64, 1));
  }

  .dz-tl {
    top: 6px;
    left: 6px;
    border-top: 2px solid;
    border-left: 2px solid;
    border-top-left-radius: 4px;
  }

  .dz-tr {
    top: 6px;
    right: 6px;
    border-top: 2px solid;
    border-right: 2px solid;
    border-top-right-radius: 4px;
  }

  .dz-bl {
    bottom: 6px;
    left: 6px;
    border-bottom: 2px solid;
    border-left: 2px solid;
    border-bottom-left-radius: 4px;
  }

  .dz-br {
    bottom: 6px;
    right: 6px;
    border-bottom: 2px solid;
    border-right: 2px solid;
    border-bottom-right-radius: 4px;
  }

  .dropzone-032:hover .dz-corner,
  .dropzone-032-active .dz-corner {
    border-color: var(--accent);
  }

  .dropzone-032-active .dz-tl {
    transform: translate(-2px, -2px);
  }
  .dropzone-032-active .dz-tr {
    transform: translate(2px, -2px);
  }
  .dropzone-032-active .dz-bl {
    transform: translate(-2px, 2px);
  }
  .dropzone-032-active .dz-br {
    transform: translate(2px, 2px);
  }

  .dz-icon-stage {
    position: relative;
    width: 44px;
    height: 44px;
    flex-shrink: 0;
  }

  .dz-sheet {
    position: absolute;
    inset: 3px;
    border-radius: 11px;
    border: 1px solid var(--border);
    transition: transform 240ms cubic-bezier(0.34, 1.56, 0.64, 1), border-color 180ms ease;
  }

  .dz-sheet-back {
    background-color: var(--surface-hover);
    transform: rotate(-7deg) translate(-2px, 1px);
  }

  .dz-sheet-mid {
    background-color: var(--surface-subtle);
    transform: rotate(5deg) translate(2px, 0);
  }

  .dz-sheet-front {
    display: flex;
    align-items: center;
    justify-content: center;
    background-color: var(--surface);
    color: var(--accent);
    transform: rotate(0deg);
  }

  .dropzone-032:hover .dz-sheet-back,
  .dropzone-032-active .dz-sheet-back {
    transform: rotate(-12deg) translate(-4px, -2px);
  }

  .dropzone-032:hover .dz-sheet-mid,
  .dropzone-032-active .dz-sheet-mid {
    transform: rotate(10deg) translate(4px, -2px);
  }

  .dropzone-032:hover .dz-sheet-front,
  .dropzone-032-active .dz-sheet-front {
    border-color: var(--accent);
    transform: translateY(-2px);
  }

  .dz-ext-pill {
    display: inline-flex;
    align-items: center;
    border-radius: 6px;
    padding: 1px 6px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
    background-color: var(--accent-soft);
    color: var(--accent);
  }

  .dz-ext-pill-subtle {
    background-color: var(--surface-hover);
    color: var(--text-muted);
  }

  .dz-cta {
    background-color: var(--accent);
    color: var(--accent-fg);
  }

  .dz-cta-secondary {
    background-color: var(--surface-subtle);
    border-color: var(--border);
    color: var(--text);
  }

  @media (prefers-reduced-motion: reduce) {
    .dropzone-032,
    .dz-corner,
    .dz-sheet {
      transition: none;
    }
  }
</style>
