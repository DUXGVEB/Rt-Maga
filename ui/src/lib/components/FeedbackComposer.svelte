<script lang="ts">
  import {
    Bug,
    CheckSquare,
    Copy,
    Download,
    ExternalLink,
    Lightbulb,
    Loader2,
    RefreshCw,
    Square,
  } from "lucide-svelte";
  import { onMount } from "svelte";
  import { api, formatAppError } from "../api";
  import { feedbackDraft } from "../stores/feedbackDraft.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
  import type { DiagnosticLogEntry, FeedbackReportKind } from "../types";
  import ToggleSwitch from "./ToggleSwitch.svelte";

  let kind = $state<FeedbackReportKind>("bug");
  let title = $state("");
  let description = $state("");
  let attachLogs = $state(false);

  let logs = $state<DiagnosticLogEntry[]>([]);
  let loadingLogs = $state(false);
  let selectedLogIds = $state<Set<number>>(new Set());
  let busyAction = $state<"github" | "copy" | "save" | null>(null);

  const t = $derived(settingsStore.t);

  async function refreshLogs(selectNewest = false) {
    loadingLogs = true;
    try {
      const loaded = await api.listDiagnosticLogs(100);
      logs = loaded;
      if (selectNewest && loaded.length > 0 && selectedLogIds.size === 0) {
        selectedLogIds = new Set(loaded.slice(0, 3).map((l) => l.id));
      } else {
        const valid = new Set(loaded.map((l) => l.id));
        selectedLogIds = new Set([...selectedLogIds].filter((id) => valid.has(id)));
      }
    } catch {
      logs = [];
    } finally {
      loadingLogs = false;
    }
  }

  onMount(() => {
    const draft = feedbackDraft.consume();
    if (draft) {
      kind = draft.kind;
      title = draft.title;
      description = draft.description;
      attachLogs = draft.attachLogs;
      if (draft.attachLogs) {
        void refreshLogs(true);
      }
    }
  });

  $effect(() => {
    if (attachLogs && logs.length === 0 && !loadingLogs) {
      void refreshLogs(false);
    }
  });

  function toggleLogId(id: number) {
    const next = new Set(selectedLogIds);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    selectedLogIds = next;
  }

  function selectAllLogs() {
    selectedLogIds = new Set(logs.map((l) => l.id));
  }

  function clearSelectedLogs() {
    selectedLogIds = new Set();
  }

  function buildPayload() {
    return {
      kind,
      title: title.trim(),
      description: description.trim(),
      attachLogs,
      logIds: attachLogs ? [...selectedLogIds] : [],
    };
  }

  async function submitGithub() {
    if (!title.trim()) {
      toasts.push(t.feedbackTitleRequired, "error", 2400);
      return;
    }
    busyAction = "github";
    try {
      const report = await api.buildFeedbackReport(buildPayload());
      if (report.urlTruncated) {
        await navigator.clipboard.writeText(report.markdown).catch(() => {});
      }
      await api.openExternalUrl(report.githubIssueUrl);
      toasts.push(t.feedbackOpenedGithubToast(report.urlTruncated), "success", 3800);
    } catch (err) {
      toasts.push(formatAppError(err, settingsStore.value.language).detail, "error");
    } finally {
      busyAction = null;
    }
  }

  async function copyMarkdown() {
    if (!title.trim()) {
      toasts.push(t.feedbackTitleRequired, "error", 2400);
      return;
    }
    busyAction = "copy";
    try {
      const report = await api.buildFeedbackReport(buildPayload());
      await navigator.clipboard.writeText(report.markdown);
      toasts.push(t.feedbackCopiedToast, "success", 2600);
    } catch (err) {
      toasts.push(formatAppError(err, settingsStore.value.language).detail, "error");
    } finally {
      busyAction = null;
    }
  }

  async function saveMarkdownFile() {
    if (!title.trim()) {
      toasts.push(t.feedbackTitleRequired, "error", 2400);
      return;
    }
    busyAction = "save";
    try {
      const saved = await api.saveFeedbackReportFile(buildPayload());
      toasts.push(t.feedbackSavedFileToast(saved.path), "success", 3800);
    } catch (err) {
      toasts.push(formatAppError(err, settingsStore.value.language).detail, "error");
    } finally {
      busyAction = null;
    }
  }

  function formatTs(unix: number): string {
    try {
      return new Date(unix * 1000).toLocaleString();
    } catch {
      return String(unix);
    }
  }
</script>

<div class="flex flex-col gap-3.5 text-xs">
  <!-- Kind Selector: Bug Report / Feature Suggestion -->
  <div class="flex flex-col gap-1.5">
    <span class="font-medium">{t.feedbackKindLabel}</span>
    <div class="grid grid-cols-2 gap-2">
      <button
        type="button"
        onclick={() => (kind = "bug")}
        class="inline-flex items-center justify-center gap-1.5 rounded-xl border py-2 font-semibold transition-colors"
        style="border-color: {kind === 'bug'
          ? 'var(--accent)'
          : 'var(--border)'}; background-color: {kind === 'bug'
          ? 'var(--accent-soft)'
          : 'var(--surface-subtle)'}; color: {kind === 'bug' ? 'var(--accent)' : 'var(--text)'};"
      >
        <Bug size={14} />
        <span>{t.feedbackKindBug}</span>
      </button>

      <button
        type="button"
        onclick={() => (kind = "suggestion")}
        class="inline-flex items-center justify-center gap-1.5 rounded-xl border py-2 font-semibold transition-colors"
        style="border-color: {kind === 'suggestion'
          ? 'var(--accent)'
          : 'var(--border)'}; background-color: {kind === 'suggestion'
          ? 'var(--accent-soft)'
          : 'var(--surface-subtle)'}; color: {kind === 'suggestion'
          ? 'var(--accent)'
          : 'var(--text)'};"
      >
        <Lightbulb size={14} />
        <span>{t.feedbackKindSuggestion}</span>
      </button>
    </div>
  </div>

  <!-- Title -->
  <label class="flex flex-col gap-1.5">
    <span class="font-medium">{t.feedbackTitleLabel}</span>
    <input
      type="text"
      bind:value={title}
      placeholder={t.feedbackTitlePlaceholder}
      class="rounded-xl border px-3 py-2 text-xs outline-none"
      style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
    />
  </label>

  <!-- Description -->
  <label class="flex flex-col gap-1.5">
    <span class="font-medium">{t.feedbackDescLabel}</span>
    <textarea
      rows="4"
      bind:value={description}
      placeholder={t.feedbackDescPlaceholder}
      class="w-full rounded-xl border p-3 text-xs outline-none"
      style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
    ></textarea>
  </label>

  <!-- Optional Diagnostic Log Attachment Toggle + Selective Checklist -->
  <div
    class="flex flex-col gap-2.5 rounded-xl border p-3"
    style="border-color: var(--border); background-color: var(--surface-subtle);"
  >
    <div class="flex items-center justify-between gap-2">
      <div class="min-w-0">
        <div class="font-semibold">{t.feedbackAttachLogsLabel}</div>
        <div class="text-[11px]" style="color: var(--text-muted);">{t.feedbackAttachLogsDesc}</div>
      </div>
      <ToggleSwitch
        size="sm"
        checked={attachLogs}
        ariaLabel={t.feedbackAttachLogsLabel}
        onchange={(checked) => {
          attachLogs = checked;
        }}
      />
    </div>

    {#if attachLogs}
      <div class="flex flex-wrap items-center justify-between gap-2 border-t pt-2" style="border-color: var(--border);">
        <span class="text-[11px] font-medium" style="color: var(--text-muted);">
          {t.feedbackSelectLogsHint} ({t.feedbackSelectedCount(selectedLogIds.size)})
        </span>
        <div class="flex items-center gap-1.5">
          <button
            type="button"
            onclick={() => void refreshLogs(false)}
            disabled={loadingLogs}
            class="inline-flex items-center gap-1 rounded-lg border px-2 py-1 text-[11px]"
            style="border-color: var(--border); background-color: var(--surface);"
            title={t.retry}
          >
            <RefreshCw size={11} class={loadingLogs ? "animate-spin" : ""} />
          </button>
          <button
            type="button"
            onclick={selectAllLogs}
            class="inline-flex items-center gap-1 rounded-lg border px-2 py-1 text-[11px] font-medium"
            style="border-color: var(--border); background-color: var(--surface);"
          >
            <CheckSquare size={11} />
            <span>{t.feedbackSelectAllLogs}</span>
          </button>
          <button
            type="button"
            onclick={clearSelectedLogs}
            class="inline-flex items-center gap-1 rounded-lg border px-2 py-1 text-[11px] font-medium"
            style="border-color: var(--border); background-color: var(--surface);"
          >
            <Square size={11} />
            <span>{t.feedbackClearSelectedLogs}</span>
          </button>
        </div>
      </div>

      {#if loadingLogs}
        <div class="py-4 text-center text-[11px]" style="color: var(--text-muted);">
          {t.calculating}
        </div>
      {:else if logs.length === 0}
        <div class="py-3 text-center text-[11px]" style="color: var(--text-muted);">
          {t.feedbackNoLogsAvailable}
        </div>
      {:else}
        <div class="flex max-h-60 flex-col gap-1.5 overflow-y-auto overscroll-contain touch-pan-y pr-1">
          {#each logs as log (log.id)}
            {@const checked = selectedLogIds.has(log.id)}
            <label
              class="flex cursor-pointer items-start gap-2.5 rounded-lg border px-2.5 py-2 transition-colors"
              style="border-color: {checked ? 'var(--accent)' : 'var(--border)'}; background-color: var(--surface);"
            >
              <input
                type="checkbox"
                {checked}
                onchange={() => toggleLogId(log.id)}
                class="mt-0.5 h-3.5 w-3.5 shrink-0"
              />
              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-center gap-1.5 text-[10px]">
                  <span
                    class="rounded px-1.5 py-0.5 font-bold uppercase"
                    style="background-color: {log.level === 'crash' || log.level === 'error'
                      ? 'rgba(239, 68, 68, 0.14)'
                      : 'var(--surface-hover)'}; color: {log.level === 'crash' ||
                    log.level === 'error'
                      ? 'var(--danger)'
                      : 'var(--text-muted)'};"
                  >
                    {log.level}
                  </span>
                  <span class="font-mono font-semibold">{log.category}</span>
                  {#if log.source}
                    <span class="truncate opacity-75">· {log.source}</span>
                  {/if}
                  <span class="ml-auto opacity-65">{formatTs(log.createdAt)}</span>
                </div>
                <div class="mt-0.5 line-clamp-2 text-[11px]" style="color: var(--text);">
                  {log.message}
                </div>
              </div>
            </label>
          {/each}
        </div>
      {/if}
    {/if}
  </div>

  <!-- Export & Submit Action Buttons -->
  <div class="flex flex-wrap items-center gap-2 pt-1">
    <button
      type="button"
      disabled={busyAction !== null}
      onclick={() => void submitGithub()}
      class="inline-flex items-center gap-1.5 rounded-xl px-4 py-2 text-xs font-semibold disabled:opacity-50"
      style="background-color: var(--accent); color: var(--accent-fg);"
    >
      {#if busyAction === "github"}
        <Loader2 size={13} class="animate-spin" />
      {:else}
        <ExternalLink size={13} />
      {/if}
      <span>{t.feedbackOpenGithubBtn}</span>
    </button>

    <button
      type="button"
      disabled={busyAction !== null}
      onclick={() => void copyMarkdown()}
      class="inline-flex items-center gap-1.5 rounded-xl border px-3.5 py-2 text-xs font-medium disabled:opacity-50"
      style="border-color: var(--border); background-color: var(--surface-subtle);"
    >
      <Copy size={13} />
      <span>{t.feedbackCopyMarkdownBtn}</span>
    </button>

    <button
      type="button"
      disabled={busyAction !== null}
      onclick={() => void saveMarkdownFile()}
      class="inline-flex items-center gap-1.5 rounded-xl border px-3.5 py-2 text-xs font-medium disabled:opacity-50"
      style="border-color: var(--border); background-color: var(--surface-subtle);"
    >
      <Download size={13} />
      <span>{t.feedbackSaveFileBtn}</span>
    </button>
  </div>
</div>
