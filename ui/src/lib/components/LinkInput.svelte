<script lang="ts">
  import {
    ArrowRight,
    ClipboardPaste,
    Link2,
    X,
  } from "lucide-svelte";
  import { api, formatAppError } from "../api";
  import { router } from "../stores/router.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
  import type { FormattedAppError } from "../types";
  import LatticeLoader from "./LatticeLoader.svelte";
  import SourceDiagnosticCard from "./SourceDiagnosticCard.svelte";

  let urlInput = $state("");
  let inputEl = $state<HTMLInputElement | null>(null);
  let loading = $state(false);
  let verifying = $state(false);
  let loggingIn = $state(false);
  let pasting = $state(false);
  let lastSubmittedUrl = $state("");
  let errorState = $state<FormattedAppError | null>(null);

  const t = $derived(settingsStore.t);

  function isLikelyDirectLink(raw: string): boolean {
    const clean = raw.trim();
    if (!clean || /\s/.test(clean)) return false;
    const low = clean.toLowerCase();
    if (low.startsWith("http://") || low.startsWith("https://") || low.startsWith("rtmaga://")) {
      return true;
    }
    return /^[a-z0-9.-]+\.[a-z]{2,}(\/.*)?$/i.test(clean);
  }

  async function submit(raw?: string) {
    const target = (raw ?? urlInput).trim();
    if (!target || loading) return;

    if (!isLikelyDirectLink(target)) {
      urlInput = "";
      errorState = null;
      router.goSources(target);
      return;
    }

    loading = true;
    lastSubmittedUrl = target;
    errorState = null;

    try {
      const res = await api.resolveUrl(target);
      urlInput = "";
      if (res.kind === "catalog" && res.catalog) {
        router.goBrowse(res.catalog.url, res.catalog, false, res.catalog.sourceId ?? undefined);
      } else if (res.manga) {
        if (res.startAt) {
          router.goReader(res.manga.id, res.startAt.url);
        } else {
          router.goManga(res.manga.id);
        }
      }
    } catch (err) {
      errorState = formatAppError(err, settingsStore.value.language);
    } finally {
      loading = false;
    }
  }

  function clearUrlInput() {
    if (loading) return;
    urlInput = "";
    errorState = null;
    inputEl?.focus();
  }

  async function pasteFromClipboard() {
    if (loading || pasting) return;
    pasting = true;
    errorState = null;
    try {
      // Prefer native OS clipboard bridge (works reliably on Android WebView & Windows WebView2
      // where `navigator.clipboard.readText()` is blocked on `http://tauri.localhost`).
      let nativeReadSucceeded = false;
      try {
        const nativeText = await api.readClipboardText();
        nativeReadSucceeded = true;
        if (nativeText && nativeText.trim().length > 0) {
          urlInput = nativeText.trim();
          inputEl?.focus();
          return;
        }
      } catch {
        // Fall back to Web Clipboard API below (e.g. browser dev preview).
      }

      if (
        typeof navigator !== "undefined" &&
        navigator.clipboard &&
        typeof navigator.clipboard.readText === "function"
      ) {
        try {
          const text = (await navigator.clipboard.readText()).trim();
          if (text) {
            urlInput = text;
            inputEl?.focus();
            return;
          }
          toasts.push(t.clipboardEmptyError, "info", 2400);
          inputEl?.focus();
          return;
        } catch (err) {
          if (nativeReadSucceeded) {
            toasts.push(t.clipboardEmptyError, "info", 2400);
            inputEl?.focus();
            return;
          }
          const name = err instanceof Error ? err.name : "";
          if (name === "NotAllowedError" || name === "SecurityError") {
            toasts.push(t.clipboardPermissionError, "info", 3200);
          } else {
            toasts.push(t.clipboardUnsupportedError, "info", 2800);
          }
          inputEl?.focus();
          return;
        }
      }

      toasts.push(
        nativeReadSucceeded ? t.clipboardEmptyError : t.clipboardUnsupportedError,
        "info",
        2400
      );
      inputEl?.focus();
    } finally {
      pasting = false;
    }
  }

  async function handleOpenExternal(targetUrl?: string) {
    const target = targetUrl || errorState?.externalUrl || lastSubmittedUrl;
    if (!target) return;
    try {
      await api.openExternalUrl(target);
    } catch (err) {
      toasts.push(formatAppError(err, settingsStore.value.language).detail, "error");
    }
  }

  function handlePaste(e: ClipboardEvent) {
    const pasted = e.clipboardData?.getData("text")?.trim();
    if (pasted && /^https?:\/\//i.test(pasted)) {
      e.preventDefault();
      urlInput = pasted;
      errorState = null;
    }
  }

  async function handleManualVerify(targetUrl?: string) {
    const target = targetUrl || errorState?.blockedUrl || lastSubmittedUrl;
    if (!target || verifying) return;
    verifying = true;
    try {
      toasts.push(t.toastOpeningVerify, "info", 4500);
      await api.openManualVerification(target);
      toasts.push(t.toastVerifyDone, "success");
      await submit(target);
    } catch (err) {
      errorState = formatAppError(err, settingsStore.value.language);
    } finally {
      verifying = false;
    }
  }

  async function handleBrowserLogin(targetUrl: string) {
    const target = targetUrl || lastSubmittedUrl || urlInput.trim();
    if (!target || loggingIn) return;
    loggingIn = true;
    try {
      toasts.push(t.errActionOpeningLogin, "info", 4500);
      const outcome = await api.openBrowserLoginSession(target);
      if (outcome.cookiesSynced) {
        toasts.push(t.toastLoginSynced, "success");
        await submit(target);
      } else {
        toasts.push(t.toastLoginExternalOpened, "info", 4500);
      }
    } catch (err) {
      errorState = formatAppError(err, settingsStore.value.language);
    } finally {
      loggingIn = false;
    }
  }
</script>

<div class="w-full max-w-2xl">
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void submit();
    }}
    class="surface-card flex flex-col gap-2 rounded-2xl p-2 focus-within:ring-2 sm:flex-row sm:items-center sm:gap-2"
    style="--tw-ring-color: var(--accent-soft);"
  >
    <div class="flex min-w-0 flex-1 items-center gap-2.5 px-2 sm:pl-2.5 sm:pr-0">
      <div class="shrink-0" style="color: var(--text-muted);">
        <Link2 size={18} />
      </div>

      <input
        bind:this={inputEl}
        type="text"
        inputmode="url"
        autocapitalize="off"
        autocorrect="off"
        spellcheck="false"
        bind:value={urlInput}
        oninput={() => {
          if (errorState) errorState = null;
        }}
        onpaste={handlePaste}
        disabled={loading}
        placeholder={t.linkPlaceholder}
        class="min-w-0 flex-1 bg-transparent py-2 text-sm outline-none placeholder:opacity-65"
        style="color: var(--text);"
      />
    </div>

    <div class="flex shrink-0 items-center justify-end gap-1.5">
      <!-- Paste URL Button -->
      <button
        type="button"
        onclick={pasteFromClipboard}
        disabled={loading || pasting}
        class="gpu-press inline-flex min-h-[42px] flex-1 items-center justify-center gap-1.5 rounded-xl border px-3 py-1.5 text-xs font-medium hover:opacity-85 disabled:opacity-40 sm:min-h-[38px] sm:flex-initial"
        style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
        title={t.pasteUrlBtn}
        aria-label={t.pasteUrlBtn}
      >
        <ClipboardPaste size={14} />
        <span>{t.pasteUrlBtn}</span>
      </button>

      <!-- Clear URL Button: positioned immediately left of Open Manga -->
      <button
        type="button"
        onclick={clearUrlInput}
        disabled={loading || (!urlInput && !errorState)}
        class="gpu-press inline-flex min-h-[42px] min-w-[42px] shrink-0 items-center justify-center gap-1 rounded-xl border px-2.5 py-1.5 text-xs font-medium hover:opacity-85 disabled:opacity-35 sm:min-h-[38px] sm:min-w-0"
        style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text-muted);"
        title={t.clearUrlBtn}
        aria-label={t.clearUrlBtn}
      >
        <X size={14} />
        <span class="hidden sm:inline">{t.clearUrlBtn}</span>
      </button>

      <!-- Primary Open Manga Button -->
      <button
        type="submit"
        disabled={loading || !urlInput.trim()}
        class="gpu-press flex min-h-[42px] flex-1 shrink-0 items-center justify-center gap-1.5 rounded-xl px-4 py-2 text-sm font-semibold hover:opacity-92 disabled:opacity-45 sm:min-h-[38px] sm:flex-initial"
        style="background-color: var(--accent); color: var(--accent-fg);"
      >
        {#if loading}
          <LatticeLoader size="xs" tone="inherit" label={t.fetching} showTimer inline />
        {:else}
          <span>{t.open}</span>
          <ArrowRight size={15} />
        {/if}
      </button>
    </div>
  </form>

  {#if errorState}
    <div class="mt-3">
      <SourceDiagnosticCard
        error={errorState}
        fallbackUrl={lastSubmittedUrl || urlInput.trim() || undefined}
        {verifying}
        {loggingIn}
        onRetry={() => void submit(lastSubmittedUrl || urlInput.trim())}
        onVerify={(u) => void handleManualVerify(u)}
        onLogin={(u) => void handleBrowserLogin(u)}
        onOpenExternal={(u) => void handleOpenExternal(u)}
        onOpenNetworkSettings={() => router.goSettings("network")}
      />
    </div>
  {/if}
</div>
