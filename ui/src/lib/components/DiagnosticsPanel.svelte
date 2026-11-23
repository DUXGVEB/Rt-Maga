<script lang="ts">
  import {
    AlertCircle,
    ChevronDown,
    ChevronRight,
    Copy,
    Download,
    RefreshCw,
    Terminal,
    Trash2,
  } from "lucide-svelte";
  import { onMount } from "svelte";
  import { api } from "../api";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
  import type { DiagnosticLogEntry } from "../types";

  let logs = $state<DiagnosticLogEntry[]>([]);
  let loading = $state(false);
  let exporting = $state(false);
  let clearing = $state(false);
  let expandedIds = $state<Record<number, boolean>>({});

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

  async function downloadLogs() {
    exporting = true;
    try {
      const payload = await api.exportDiagnosticLogs();
      const stamp = new Date().toISOString().slice(0, 10);
      const blob = new Blob([payload], { type: "application/json;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `rtmaga-diagnostics-${stamp}.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      toasts.push(t.diagnosticsExported, "success", 2400);
    } catch {
      await copyLogs();
    } finally {
      exporting = false;
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
        onclick={downloadLogs}
        disabled={exporting}
        class="inline-flex items-center gap-1 rounded-lg px-2.5 py-1.5 text-xs font-semibold text-white"
        style="background-color: var(--accent);"
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
                {#if entry.source}
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
              class="border-t px-3 py-2 font-mono text-[11px] break-all whitespace-pre-wrap"
              style="border-color: var(--border); color: var(--text-muted); background-color: var(--surface-subtle);"
            >
              {#if entry.chapterUrl}
                <div>URL: {entry.chapterUrl}</div>
              {/if}
              {#if entry.detail}
                <div class="mt-1">{entry.detail}</div>
              {:else}
                <div>(no additional detail)</div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
