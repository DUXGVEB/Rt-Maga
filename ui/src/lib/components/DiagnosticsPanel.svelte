<script lang="ts">
  import {
    AlertCircle,
    Check,
    CheckCircle2,
    ChevronDown,
    ChevronRight,
    Copy,
    Download,
    RefreshCw,
    Terminal,
    Trash2,
  } from "lucide-svelte";
  import { onMount } from "svelte";
  import { api, formatAppError } from "../api";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
  import type { DiagnosticLogEntry, SavedDiagnosticFile } from "../types";

  let logs = $state<DiagnosticLogEntry[]>([]);
  let loading = $state(false);
  let exporting = $state(false);
  let clearing = $state(false);
  let expandedIds = $state<Record<number, boolean>>({});
  let savedFile = $state<SavedDiagnosticFile | null>(null);
  let exportError = $state<string | null>(null);
  let copiedPath = $state(false);

  const t = $derived(settingsStore.t);

  async function loadLogs() {
    loading = true;
    try {
      logs = await api.listDiagnosticLogs(100);
    } catch {
      logs = [];
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadLogs();
  });

  function toggleExpanded(id: number) {
    expandedIds = { ...expandedIds, [id]: !expandedIds[id] };
  }

  function formatTimestamp(unixSecs: number): string {
    return new Date(unixSecs * 1000).toLocaleString(settingsStore.locale, {
      month: "short",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    return `${(bytes / 1024).toFixed(1)} KB`;
  }

  function levelColor(level: string): string {
    switch (level) {
      case "panic":
      case "error":
        return "var(--danger)";
      case "warn":
        return "#f59e0b";
      default:
        return "var(--accent)";
    }
  }

  async function copyLogs() {
    exporting = true;
    try {
      const payload = await api.exportDiagnosticLogs();
      await navigator.clipboard.writeText(payload);
      toasts.push(t.diagnosticsCopied, "success", 2200);
    } catch {
      toasts.push(t.diagnosticsExportFailed, "error");
    } finally {
      exporting = false;
    }
  }

  async function saveLogsToFile() {
    exporting = true;
    exportError = null;
    copiedPath = false;
    try {
      const result = await api.saveDiagnosticLogsFile();
      savedFile = result;
      toasts.push(t.diagnosticsExported, "success", 2800);
    } catch (err) {
      savedFile = null;
      const formatted = formatAppError(err, settingsStore.value.language);
      const msg = t.diagnosticsSaveFailed(formatted.detail || formatted.title);
      exportError = msg;
      toasts.push(msg, "error", 4000);
    } finally {
      exporting = false;
    }
  }

  async function copySavedPath() {
    if (!savedFile) return;
    try {
      await navigator.clipboard.writeText(savedFile.path);
      copiedPath = true;
      toasts.push(t.diagnosticsPathCopied, "success", 2200);
      window.setTimeout(() => {
        copiedPath = false;
      }, 2200);
    } catch {
      toasts.push(t.clipboardPermissionError, "info", 2800);
    }
  }

  async function clearLogs() {
    clearing = true;
    try {
      await api.clearDiagnosticLogs();
      logs = [];
      expandedIds = {};
      toasts.push(t.diagnosticsCleared, "info", 2000);
    } catch {
      toasts.push(t.diagnosticsExportFailed, "error");
    } finally {
      clearing = false;
    }
  }
</script>

<div class="flex flex-col gap-3">
  <div class="flex flex-wrap items-center justify-between gap-2">
    <div class="flex items-center gap-2 text-xs" style="color: var(--text-muted);">
      <Terminal size={14} style="color: var(--accent);" />
      <span>{t.diagnosticsCount(logs.length)}</span>
    </div>

    <div class="flex flex-wrap items-center gap-1.5">
      <button
        type="button"
        onclick={loadLogs}
        disabled={loading}
        class="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1.5 text-xs font-medium"
        style="border-color: var(--border); background-color: var(--surface-subtle);"
      >
        <RefreshCw size={12} class={loading ? "animate-spin" : ""} />
        <span>{t.diagnosticsRefreshBtn}</span>
      </button>
      <button
        type="button"
        onclick={copyLogs}
        disabled={exporting}
        class="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1.5 text-xs font-medium"
        style="border-color: var(--border); background-color: var(--surface-subtle);"
      >
        <Copy size={12} />
        <span>{t.diagnosticsCopyBtn}</span>
      </button>
      <button
        type="button"
        onclick={saveLogsToFile}
        disabled={exporting}
        class="inline-flex items-center gap-1 rounded-lg px-2.5 py-1.5 text-xs font-semibold disabled:opacity-50"
        style="background-color: var(--accent); color: var(--accent-fg);"
      >
        <Download size={12} />
        <span>{t.diagnosticsExportBtn}</span>
      </button>
      {#if logs.length > 0}
        <button
          type="button"
          onclick={clearLogs}
          disabled={clearing}
          class="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1.5 text-xs font-medium"
          style="border-color: var(--border); color: var(--danger);"
        >
          <Trash2 size={12} />
          <span>{t.diagnosticsClearBtn}</span>
        </button>
      {/if}
    </div>
  </div>

  <!-- Explicit Exported File Path Feedback (Part 6) -->
  {#if savedFile}
    <div
      class="flex flex-col gap-2 rounded-xl border p-3 text-xs"
      style="background-color: rgba(16, 185, 129, 0.08); border-color: rgba(16, 185, 129, 0.4);"
    >
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex items-center gap-1.5 font-semibold" style="color: #10b981;">
          <CheckCircle2 size={14} />
          <span>{t.diagnosticsSavedTitle}</span>
          <span class="font-normal opacity-80">({formatSize(savedFile.bytes)})</span>
        </div>
        <button
          type="button"
          onclick={copySavedPath}
          class="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1 text-[11px] font-medium"
          style="border-color: rgba(16, 185, 129, 0.45); background-color: var(--surface); color: var(--text);"
        >
          {#if copiedPath}
            <Check size={12} style="color: #10b981;" />
          {:else}
            <Copy size={12} />
          {/if}
          <span>{t.diagnosticsCopyPathBtn}</span>
        </button>
      </div>
      <div
        class="rounded-lg border px-2.5 py-1.5 font-mono text-[11px] break-all select-all"
        style="background-color: var(--surface); border-color: var(--border); color: var(--text);"
      >
        {savedFile.path}
      </div>
    </div>
  {/if}

  {#if exportError}
    <div
      class="flex items-start gap-2 rounded-xl border p-3 text-xs"
      style="background-color: rgba(239, 68, 68, 0.08); border-color: var(--danger); color: var(--danger);"
    >
      <AlertCircle size={15} class="mt-0.5 shrink-0" />
      <span class="break-words">{exportError}</span>
    </div>
  {/if}

  {#if logs.length === 0}
    <div
      class="flex items-center gap-2 rounded-xl border border-dashed px-4 py-5 text-xs"
      style="border-color: var(--border); color: var(--text-muted);"
    >
      <AlertCircle size={15} />
      <span>{t.diagnosticsEmpty}</span>
    </div>
  {:else}
    <div
      class="flex max-h-72 flex-col gap-1.5 overflow-y-auto rounded-xl border p-2"
      style="border-color: var(--border); background-color: var(--surface-subtle);"
    >
      {#each logs as entry (entry.id)}
        {@const open = !!expandedIds[entry.id]}
        <div
          class="rounded-lg border text-xs"
          style="border-color: var(--border); background-color: var(--surface);"
        >
          <button
            type="button"
            onclick={() => toggleExpanded(entry.id)}
            class="flex w-full items-start gap-2 px-2.5 py-2 text-left"
          >
            <span class="mt-0.5 shrink-0" style="color: var(--text-muted);">
              {#if open}
                <ChevronDown size={13} />
              {:else}
                <ChevronRight size={13} />
              {/if}
            </span>
            <div class="min-w-0 flex-1">
              <div class="flex flex-wrap items-center gap-1.5">
                <span
                  class="rounded px-1.5 py-0.5 text-[10px] font-bold uppercase text-white"
                  style="background-color: {levelColor(entry.level)};"
                >
                  {entry.level}
                </span>
                <span
                  class="rounded px-1.5 py-0.5 text-[10px] font-medium"
                  style="background-color: var(--surface-hover); color: var(--text-muted);"
                >
                  {entry.category}
                </span>
                {#if entry.httpStatus !== null}
                  <span
                    class="rounded px-1.5 py-0.5 text-[10px] font-semibold"
                    style="background-color: var(--surface-hover); color: var(--danger);"
                  >
                    HTTP {entry.httpStatus}
                  </span>
                {/if}
                {#if entry.adapter}
                  <span class="truncate text-[10px]" style="color: var(--accent);">
                    {entry.adapter}
                  </span>
                {:else if entry.source}
                  <span class="truncate text-[10px]" style="color: var(--text-muted);">
                    {entry.source}
                  </span>
                {/if}
                <span class="ml-auto text-[10px]" style="color: var(--text-muted);">
                  {formatTimestamp(entry.createdAt)}
                </span>
              </div>
              <div class="mt-1 break-words font-medium" style="color: var(--text);">
                {entry.message}
              </div>
            </div>
          </button>

          {#if open}
            <div
              class="flex flex-col gap-1 border-t px-3 py-2 font-mono text-[11px] break-all whitespace-pre-wrap"
              style="border-color: var(--border); color: var(--text-muted); background-color: var(--surface-subtle);"
            >
              {#if entry.failedUrl}
                <div>Failed URL: {entry.failedUrl}</div>
              {/if}
              {#if entry.resolvedUrl && entry.resolvedUrl !== entry.failedUrl}
                <div>Resolved URL: {entry.resolvedUrl}</div>
              {/if}
              {#if entry.chapterUrl}
                <div>Chapter URL: {entry.chapterUrl}</div>
              {/if}
              {#if entry.parsingStage}
                <div>Stage: {entry.parsingStage}</div>
              {/if}
              {#if entry.apiError}
                <div>API Error: {entry.apiError}</div>
              {/if}
              {#if entry.detail}
                <div class="mt-1">{entry.detail}</div>
              {/if}
              {#if entry.stackTrace}
                <div class="mt-1 opacity-85">{entry.stackTrace}</div>
              {/if}
              {#if !entry.failedUrl && !entry.chapterUrl && !entry.parsingStage && !entry.apiError && !entry.detail && !entry.stackTrace}
                <div>(no additional detail)</div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
