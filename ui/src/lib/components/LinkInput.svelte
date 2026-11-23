<script lang="ts">
  import {
    AlertCircle,
    ArrowRight,
    ClipboardPaste,
    ExternalLink,
    Link2,
    Loader2,
    X,
  } from "lucide-svelte";
  import { api, formatAppError } from "../api";
  import { router } from "../stores/router.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";

  let urlInput = $state("");
  let inputEl = $state<HTMLInputElement | null>(null);
  let loading = $state(false);
  let verifying = $state(false);
  let pasting = $state(false);
  let errorState = $state<{ title: string; detail: string; blockedUrl?: string } | null>(null);

  const t = $derived(settingsStore.t);

  async function submit(raw?: string) {
    const target = (raw ?? urlInput).trim();
    if (!target || loading) return;

    loading = true;
    errorState = null;

    try {
      const res = await api.resolveUrl(target);
      urlInput = "";
      if (res.kind === "catalog" && res.catalog) {
        router.goBrowse(res.catalog.url, res.catalog);
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
      if (
        typeof navigator === "undefined" ||
        !navigator.clipboard ||
        typeof navigator.clipboard.readText !== "function"
      ) {
        toasts.push(t.clipboardUnsupportedError, "info", 2800);
        inputEl?.focus();
        return;
      }
      const text = (await navigator.clipboard.readText()).trim();
      if (!text) {
        toasts.push(t.clipboardEmptyError, "info", 2400);
        inputEl?.focus();
        return;
      }
      urlInput = text;
      inputEl?.focus();
    } catch (err) {
      const name = err instanceof Error ? err.name : "";
      if (name === "NotAllowedError" || name === "SecurityError") {
        toasts.push(t.clipboardPermissionError, "info", 3200);
      } else {
        toasts.push(t.clipboardUnsupportedError, "info", 2800);
      }
      inputEl?.focus();
    } finally {
      pasting = false;
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

  async function handleManualVerify() {
    if (!errorState?.blockedUrl || verifying) return;
    verifying = true;
    try {
      toasts.push(t.toastOpeningVerify, "info", 4500);
      await api.openManualVerification(errorState.blockedUrl);
      toasts.push(t.toastVerifyDone, "success");
      await submit(errorState.blockedUrl);
    } catch (err) {
      errorState = formatAppError(err, settingsStore.value.language);
    } finally {
      verifying = false;
    }
  }
</script>

<div class="w-full max-w-2xl">
  <form
    onsubmit={(e) => {
      e.preventDefault();
      void submit();
    }}
    class="flex flex-wrap items-center gap-1.5 rounded-2xl border p-2 transition-shadow focus-within:ring-2 sm:flex-nowrap sm:gap-2"
    style="background-color: var(--surface); border-color: var(--border); box-shadow: var(--shadow-sm); --tw-ring-color: var(--accent-soft);"
  >
    <div class="flex min-w-0 flex-1 items-center gap-2 pl-2">
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
        class="inline-flex shrink-0 items-center gap-1 rounded-lg border px-2 py-1.5 text-xs font-medium transition-opacity hover:opacity-85 disabled:opacity-40"
        style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
        title={t.pasteUrlBtn}
        aria-label={t.pasteUrlBtn}
      >
        <ClipboardPaste size={13} />
        <span class="hidden sm:inline">{t.pasteUrlBtn}</span>
      </button>

      <!-- Clear URL Button: positioned immediately left of Open Manga, smaller visual size -->
      <button
        type="button"
        onclick={clearUrlInput}
        disabled={loading || (!urlInput && !errorState)}
        class="inline-flex shrink-0 items-center gap-1 rounded-lg border px-2 py-1.5 text-xs font-medium transition-opacity hover:opacity-85 disabled:opacity-35"
        style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text-muted);"
        title={t.clearUrlBtn}
        aria-label={t.clearUrlBtn}
      >
        <X size={13} />
        <span class="hidden sm:inline">{t.clearUrlBtn}</span>
      </button>

      <!-- Primary Open Manga Button -->
      <button
        type="submit"
        disabled={loading || !urlInput.trim()}
        class="flex shrink-0 items-center gap-1.5 rounded-xl px-4 py-2 text-sm font-medium text-white transition-opacity disabled:opacity-45"
        style="background-color: var(--accent);"
      >
        {#if loading}
          <Loader2 size={16} class="animate-spin" />
          <span>{t.fetching}</span>
        {:else}
          <span>{t.open}</span>
          <ArrowRight size={15} />
        {/if}
      </button>
    </div>
  </form>

  {#if errorState}
    <div
      class="mt-3 flex items-start gap-3 rounded-xl border p-3.5 text-left text-sm"
      style="background-color: var(--surface); border-color: var(--danger);"
    >
      <AlertCircle size={18} class="mt-0.5 shrink-0" style="color: var(--danger);" />
      <div class="min-w-0 flex-1">
        <div class="font-semibold">{errorState.title}</div>
        <p class="mt-0.5 text-xs leading-relaxed break-words" style="color: var(--text-muted);">
          {errorState.detail}
        </p>
        {#if errorState.blockedUrl}
          <button
            type="button"
            onclick={handleManualVerify}
            disabled={verifying}
            class="mt-2.5 inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium text-white"
            style="background-color: var(--accent);"
          >
            {#if verifying}
              <Loader2 size={14} class="animate-spin" />
              <span>{t.waitingVerification}</span>
            {:else}
              <ExternalLink size={14} />
              <span>{t.openVerification}</span>
            {/if}
          </button>
        {/if}
      </div>
    </div>
  {/if}
</div>
