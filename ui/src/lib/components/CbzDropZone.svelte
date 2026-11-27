<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { open as openNativeDialog } from "@tauri-apps/plugin-dialog";
  import { ChevronDown, FileArchive, FolderOpen, Upload } from "lucide-svelte";
  import { onMount } from "svelte";
  import { api, formatAppError } from "../api";
  import {
    BROWSER_FILE_ACCEPT,
    COMIC_DIALOG_FILTERS,
    isBackupFilename,
    shouldUploadBrowserFile,
  } from "../comicFormats";
  import { backdropFade, badgePop, popoverPop, pressable } from "../motion";
  import { libraryCache } from "../stores/libraryCache.svelte";
  import { router } from "../stores/router.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
  import type { MangaView, TachibkBackupPreview, TachibkImportReport } from "../types";
  import LatticeLoader from "./LatticeLoader.svelte";
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
  let dragOver = $state(false);
  let menuOpen = $state(false);
  let platform = $state<"windows" | "android" | "other">("windows");
  let fileInputEl = $state<HTMLInputElement | null>(null);
  let folderInputEl = $state<HTMLInputElement | null>(null);
  let menuRootEl = $state<HTMLDivElement | null>(null);
  let backupPreview = $state<TachibkBackupPreview | null>(null);

  const t = $derived(settingsStore.t);

  const importMangaLabel = $derived(() => {
    const dict = t as unknown as Record<string, unknown>;
    if (typeof dict.importMangaBtn === "string" && dict.importMangaBtn) return dict.importMangaBtn;
    return settingsStore.value.language === "vi" ? "Nhập truyện" : "Import Manga";
  });

  const importFilesLabel = $derived(() => {
    const dict = t as unknown as Record<string, unknown>;
    if (typeof dict.importFilesOption === "string" && dict.importFilesOption) {
      return dict.importFilesOption;
    }
    return t.cbzPickBtn;
  });

  const importFolderLabel = $derived(() => {
    const dict = t as unknown as Record<string, unknown>;
    if (typeof dict.importFolderOption === "string" && dict.importFolderOption) {
      return dict.importFolderOption;
    }
    return t.cbzPickFolderBtn;
  });

  function showImportError(err: unknown) {
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

    const unregisterBack = router.registerBackInterceptor(() => {
      if (menuOpen) {
        menuOpen = false;
        return true;
      }
      return false;
    });

    const onDocPointerDown = (e: PointerEvent) => {
      if (!menuOpen) return;
      if (menuRootEl && e.target instanceof Node && menuRootEl.contains(e.target)) {
        return;
      }
      menuOpen = false;
    };

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && menuOpen) {
        menuOpen = false;
      }
    };

    document.addEventListener("pointerdown", onDocPointerDown);
    window.addEventListener("keydown", onKeyDown);

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
      unregisterBack();
      document.removeEventListener("pointerdown", onDocPointerDown);
      window.removeEventListener("keydown", onKeyDown);
      if (unlisten) unlisten();
    };
  });

  async function finishImport(manga: MangaView) {
    libraryCache.invalidate();
    toasts.push(t.cbzImportedToast(manga.title, manga.chapters.length), "success", 3000);
    if (onImported) {
      onImported(manga);
    } else {
      router.goManga(manga.id);
    }
  }

  async function importDroppedPaths(paths: string[]) {
    if (importing || paths.length === 0) return;
    menuOpen = false;
    importing = true;
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
    menuOpen = false;
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
          try {
            const manga = await api.pickAndImportCbz(targetMangaId);
            if (manga) {
              await finishImport(manga);
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
    menuOpen = false;
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
      use:pressable
      disabled={importing}
      onclick={() => void handlePickFilesClick()}
      class="btn-3d btn-3d-subtle inline-flex items-center gap-1.5 rounded-xl px-3 py-1.5 text-xs font-semibold disabled:opacity-55"
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
      <span transition:badgePop|local class="inline-flex">
        <button
          type="button"
          use:pressable
          onclick={(e) => void handlePickFolderClick(e)}
          class="btn-3d btn-3d-surface inline-flex items-center gap-1 rounded-xl px-2.5 py-1.5 text-xs font-medium"
          title={t.cbzPickFolderBtn}
        >
          <FolderOpen size={13} />
          <span>{t.cbzPickFolderBtn}</span>
        </button>
      </span>
    {/if}
  </div>
{:else}
  <!-- Compact 3D "Import Manga" button matching the exact size/elevation of Home's "Random" button -->
  <div bind:this={menuRootEl} class="relative inline-flex">
    <button
      type="button"
      use:pressable={!importing}
      disabled={importing}
      aria-expanded={menuOpen}
      aria-haspopup="menu"
      onclick={() => {
        if (importing) return;
        menuOpen = !menuOpen;
      }}
      class="btn-3d btn-3d-surface inline-flex min-h-[38px] items-center gap-2 rounded-xl px-3.5 py-1.5 text-xs font-semibold disabled:opacity-55"
    >
      {#if importing}
        <LatticeLoader size="xs" status="loading" tone="accent" inline />
        <span>{t.cbzImporting}</span>
      {:else}
        <Upload size={15} style="color: var(--accent);" />
        <span>{importMangaLabel()}</span>
        <ChevronDown
          size={13}
          class="chevron-rot opacity-65"
          data-open={menuOpen ? "true" : "false"}
        />
      {/if}
    </button>

    {#if menuOpen && !importing}
      <div
        transition:popoverPop|local={{ duration: 150, y: -4 }}
        role="menu"
        class="glass-chrome absolute top-full left-0 z-30 mt-2 flex min-w-44 flex-col gap-1 rounded-xl border p-1.5 shadow-lg"
        style="border-color: var(--border);"
      >
        <button
          type="button"
          role="menuitem"
          use:pressable
          onclick={() => void handlePickFilesClick()}
          class="btn-3d btn-3d-subtle flex min-h-[38px] w-full items-center gap-2 rounded-lg px-3 py-1.5 text-left text-xs font-semibold"
          style="background-color: var(--surface); border-color: transparent;"
        >
          <FileArchive size={14} style="color: var(--accent);" />
          <span>{importFilesLabel()}</span>
        </button>
        <button
          type="button"
          role="menuitem"
          use:pressable
          onclick={(e) => void handlePickFolderClick(e)}
          class="btn-3d btn-3d-subtle flex min-h-[38px] w-full items-center gap-2 rounded-lg px-3 py-1.5 text-left text-xs font-semibold"
          style="background-color: var(--surface); border-color: transparent;"
        >
          <FolderOpen size={14} style="color: var(--text-muted);" />
          <span>{importFolderLabel()}</span>
        </button>
      </div>
    {/if}
  </div>

  <!-- Full-viewport subtle drag-and-drop overlay rendered ONLY while actively dragging files -->
  {#if dragOver}
    <div
      transition:backdropFade|local={{ duration: 130 }}
      class="pointer-events-none fixed inset-0 z-50 flex items-center justify-center p-5"
      style="background-color: rgba(8, 8, 12, 0.44);"
      aria-hidden="true"
    >
      <div
        class="flex flex-col items-center gap-3 rounded-3xl border-2 border-dashed px-10 py-8 text-center shadow-2xl"
        style="background-color: var(--surface); border-color: var(--accent); color: var(--text);"
      >
        <div
          class="flex h-12 w-12 items-center justify-center rounded-2xl"
          style="background-color: var(--accent-soft); color: var(--accent);"
        >
          <FileArchive size={24} />
        </div>
        <div class="text-sm font-bold">{importMangaLabel()}</div>
      </div>
    </div>
  {/if}
{/if}

<TachibkImportModal
  preview={backupPreview}
  onClose={() => {
    backupPreview = null;
  }}
  onImported={handleBackupImportComplete}
/>
