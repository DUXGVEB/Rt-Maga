<script lang="ts">
  import {
    AlertCircle,
    Bug,
    Check,
    ChevronDown,
    ChevronRight,
    Copy,
    ExternalLink,
    Loader2,
    LogIn,
    RefreshCw,
    Settings,
    ShieldCheck,
  } from "lucide-svelte";
  import { feedbackDraft } from "../stores/feedbackDraft.svelte";
  import { router } from "../stores/router.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
  import type { FailureCategoryGroup, FormattedAppError } from "../types";

  let {
    error,
    fallbackUrl,
    verifying = false,
    loggingIn = false,
    compact = false,
    onRetry,
    onVerify,
    onLogin,
    onOpenExternal,
    onOpenNetworkSettings,
  }: {
    error: FormattedAppError;
    fallbackUrl?: string;
    verifying?: boolean;
    loggingIn?: boolean;
    compact?: boolean;
    onRetry?: () => void;
    onVerify?: (url: string) => void;
    onLogin?: (url: string) => void;
    onOpenExternal?: (url: string) => void;
    onOpenNetworkSettings?: () => void;
  } = $props();

  let detailsOpen = $state(false);
  let copied = $state(false);

  const t = $derived(settingsStore.t);
  const report = $derived(error.report);
  const targetUrl = $derived(
    error.externalUrl || error.blockedUrl || error.loginUrl || report?.finalUrl || fallbackUrl || ""
  );

  function groupLabel(group?: FailureCategoryGroup): string {
    switch (group) {
      case "source_detection":
        return t.errGroupSourceDetection;
      case "chapter_identification":
        return t.errGroupChapterId;
      case "content_loading":
        return t.errGroupContentLoading;
      case "image_extraction":
        return t.errGroupImageExtraction;
      case "access_permission":
        return t.errGroupAccessPermission;
      case "unsupported_format":
        return t.errGroupUnsupportedFormat;
      case "network":
        return t.errGroupNetwork;
      default:
        return t.errAppTitle;
    }
  }

  function showLoginAction(): boolean {
    if (!onLogin || !targetUrl) return false;
    return (
      Boolean(error.loginUrl) ||
      report?.recoveryActions.includes("sign_in_via_browser") === true ||
      error.code === "LOGIN_REQUIRED" ||
      error.code === "SESSION_EXPIRED"
    );
  }

  function showVerifyAction(): boolean {
    if (!onVerify || !targetUrl) return false;
    return (
      Boolean(error.blockedUrl) ||
      report?.recoveryActions.includes("complete_verification") === true ||
      error.code === "VERIFICATION_REQUIRED"
    );
  }

  function showBrowserReadingAction(): boolean {
    if (!onOpenExternal || !targetUrl) return false;
    return (
      Boolean(error.externalUrl) ||
      error.browserReadingAvailable === true ||
      report?.recoveryActions.includes("open_browser_reading_mode") === true ||
      report?.recoveryActions.includes("open_original_website") === true
    );
  }

  function showNetworkAction(): boolean {
    if (!onOpenNetworkSettings) return false;
    return (
      report?.recoveryActions.includes("configure_network_or_proxy") === true ||
      error.code === "GEO_RESTRICTION" ||
      error.code === "NETWORK_TIMEOUT"
    );
  }

  function browserBtnLabel(): string {
    if (report?.recoveryActions.includes("open_browser_reading_mode")) {
      return t.errActionOpenBrowserReading;
    }
    if (report?.recoveryActions.includes("open_original_website")) {
      return t.errActionOpenOriginalSite;
    }
    return t.openInBrowserBtn;
  }

  function formatBytes(bytes: number | null): string {
    if (bytes === null || bytes === undefined) return "—";
    if (bytes < 1024) return `${bytes} B`;
    return `${(bytes / 1024).toFixed(1)} KB`;
  }

  async function copyReportJson() {
    if (!report) return;
    try {
      await navigator.clipboard.writeText(JSON.stringify(report, null, 2));
      copied = true;
      toasts.push(t.errDiagnosticsCopied, "success", 2400);
      window.setTimeout(() => {
        copied = false;
      }, 2200);
    } catch {
      toasts.push(t.clipboardPermissionError, "info", 2400);
    }
  }
</script>

<div
  class="flex flex-col gap-3 rounded-2xl border text-left {compact ? 'p-3.5 text-xs' : 'p-5 text-sm'}"
  style="background-color: var(--surface); border-color: var(--danger);"
>
  <!-- Category Badge + Error Code -->
  <div class="flex flex-wrap items-center justify-between gap-2">
    <div class="flex flex-wrap items-center gap-2">
      <span
        class="inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-[11px] font-semibold"
        style="background-color: rgba(239, 68, 68, 0.14); color: var(--danger);"
      >
        <AlertCircle size={12} />
        <span>{groupLabel(error.failureGroup)}</span>
      </span>
      {#if error.code}
        <code
          class="rounded-md border px-1.5 py-0.5 font-mono text-[10px] font-semibold"
          style="border-color: var(--border); background-color: var(--surface-subtle); color: var(--text-muted);"
        >
          {error.code}
        </code>
      {/if}
    </div>
  </div>

  <!-- Human-Friendly Title, Explanation & Suggested Recovery -->
  <div class="flex flex-col gap-1">
    <div class="font-semibold" style="color: var(--text);">{error.title}</div>
    <p class="text-xs leading-relaxed break-words" style="color: var(--text-muted);">
      {error.detail}
    </p>
    {#if error.suggestedAction}
      <p class="mt-0.5 text-xs leading-relaxed font-medium" style="color: var(--accent);">
        {error.suggestedAction}
      </p>
    {/if}
  </div>

  <!-- Action Buttons -->
  <div class="flex flex-wrap items-center gap-2 pt-0.5">
    {#if showVerifyAction()}
      <button
        type="button"
        disabled={verifying}
        onclick={() => onVerify?.(error.blockedUrl || targetUrl)}
        class="inline-flex items-center gap-1.5 rounded-xl px-3.5 py-1.5 text-xs font-semibold disabled:opacity-50"
        style="background-color: var(--accent); color: var(--accent-fg);"
      >
        {#if verifying}
          <Loader2 size={13} class="animate-spin" />
          <span>{t.waitingVerification}</span>
        {:else}
          <ShieldCheck size={13} />
          <span>{t.openVerification}</span>
        {/if}
      </button>
    {/if}

    {#if showLoginAction()}
      <button
        type="button"
        disabled={loggingIn}
        onclick={() => onLogin?.(error.loginUrl || targetUrl)}
        class="inline-flex items-center gap-1.5 rounded-xl px-3.5 py-1.5 text-xs font-semibold disabled:opacity-50"
        style="background-color: var(--accent); color: var(--accent-fg);"
      >
        {#if loggingIn}
          <Loader2 size={13} class="animate-spin" />
          <span>{t.errActionOpeningLogin}</span>
        {:else}
          <LogIn size={13} />
          <span>{t.errActionSignIn}</span>
        {/if}
      </button>
    {/if}

    {#if showBrowserReadingAction()}
      <button
        type="button"
        onclick={() => onOpenExternal?.(error.externalUrl || targetUrl)}
        class="inline-flex items-center gap-1.5 rounded-xl border px-3.5 py-1.5 text-xs font-semibold"
        style="border-color: var(--accent); background-color: var(--accent-soft); color: var(--accent);"
      >
        <ExternalLink size={13} />
        <span>{browserBtnLabel()}</span>
      </button>
    {/if}

    {#if onRetry && error.retryable !== false}
      <button
        type="button"
        onclick={onRetry}
        class="inline-flex items-center gap-1.5 rounded-xl border px-3.5 py-1.5 text-xs font-medium"
        style="border-color: var(--border); background-color: var(--surface-subtle); color: var(--text);"
      >
        <RefreshCw size={13} />
        <span>{t.retry}</span>
      </button>
    {/if}

    {#if showNetworkAction()}
      <button
        type="button"
        onclick={() => onOpenNetworkSettings?.()}
        class="inline-flex items-center gap-1.5 rounded-xl border px-3 py-1.5 text-xs font-medium"
        style="border-color: var(--border); background-color: var(--surface-subtle); color: var(--text);"
      >
        <Settings size={13} />
        <span>{t.errActionNetworkSettings}</span>
      </button>
    {/if}

    {#if report}
      <button
        type="button"
        onclick={copyReportJson}
        class="inline-flex items-center gap-1.5 rounded-xl border px-3 py-1.5 text-xs font-medium"
        style="border-color: var(--border); background-color: var(--surface-subtle); color: var(--text-muted);"
      >
        {#if copied}
          <Check size={13} style="color: #10b981;" />
        {:else}
          <Copy size={13} />
        {/if}
        <span>{t.errActionCopyDiagnostics}</span>
      </button>
    {/if}

    <button
      type="button"
      onclick={() => {
        feedbackDraft.setPrefill({
          kind: "bug",
          title: `[Source] ${report?.domain || targetUrl || error.code || "Error"}: ${error.title}`,
          description: `URL: ${targetUrl || "-"}\nCode: ${error.code ?? "-"}\nDetail: ${error.detail}`,
          attachLogs: true,
        });
        router.goSettings("sources", "feedback");
      }}
      class="inline-flex items-center gap-1.5 rounded-xl border px-3 py-1.5 text-xs font-medium"
      style="border-color: var(--border); background-color: var(--surface-subtle); color: var(--text);"
    >
      <Bug size={13} />
      <span>{t.feedbackReportSourceShortcut}</span>
    </button>
  </div>

  <!-- Expandable Technical Diagnostics for Developers -->
  {#if report}
    <div class="border-t pt-2" style="border-color: var(--border);">
      <button
        type="button"
        onclick={() => {
          detailsOpen = !detailsOpen;
        }}
        class="inline-flex items-center gap-1.5 text-[11px] font-medium hover:opacity-80"
        style="color: var(--text-muted);"
      >
        {#if detailsOpen}
          <ChevronDown size={13} />
        {:else}
          <ChevronRight size={13} />
        {/if}
        <span>{t.errTechDetailsToggle}</span>
      </button>

      {#if detailsOpen}
        <div
          class="mt-2 flex flex-col gap-1.5 rounded-xl border p-3 font-mono text-[11px]"
          style="border-color: var(--border); background-color: var(--surface-subtle); color: var(--text-muted);"
        >
          <div><strong>{t.errTechDomain}:</strong> {report.domain ?? "—"}</div>
          <div class="break-all">
            <strong>{t.errTechFinalUrl}:</strong> {report.finalUrl ?? report.originalUrl}
          </div>
          <div>
            <strong>{t.errTechPageType}:</strong> {report.detectedPageType}
            ({t.errTechConfidence}: {Math.round(report.detectionConfidence * 100)}%)
          </div>
          <div><strong>{t.errTechStage}:</strong> {report.failedStage ?? "—"}</div>
          <div>
            <strong>{t.errTechHttpStatus}:</strong> {report.httpStatus ?? "—"} ·
            <strong>{t.errTechContentType}:</strong> {report.contentType ?? "—"} ·
            <strong>{t.errTechBytes}:</strong> {formatBytes(report.responseBytes)}
          </div>
          <div>
            <strong>{t.errTechCandidates}:</strong> {report.imageCandidatesCount} ·
            <strong>{t.errTechValidated}:</strong> {report.validatedImagesCount}
          </div>
          {#if report.adapterName}
            <div>
              <strong>{t.errTechAdapter}:</strong> {report.adapterName}
              {report.adapterVersion ? `v${report.adapterVersion}` : ""}
            </div>
          {/if}
          {#if report.strategiesAttempted.length > 0}
            <div>
              <strong>{t.errTechStrategies}:</strong>
              <ul class="mt-0.5 ml-3 list-disc space-y-0.5">
                {#each report.strategiesAttempted as st, i (`${st.strategy}-${i}`)}
                  <li>
                    {st.strategy}{st.adapterId ? ` (${st.adapterId})` : ""}: {st.outcome} ·
                    cand={st.candidatesFound}, ok={st.validatedCount} ({st.durationMs}ms)
                  </li>
                {/each}
              </ul>
            </div>
          {/if}
          {#if report.browserErrors.length > 0}
            <div class="break-words" style="color: var(--danger);">
              <strong>{t.errTechBrowserErrors}:</strong> {report.browserErrors.join(" | ")}
            </div>
          {/if}
          {#if report.evidence.length > 0}
            <div class="break-words">
              <strong>{t.errTechEvidence}:</strong> {report.evidence.join(" · ")}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</div>
