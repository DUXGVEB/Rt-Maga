<script lang="ts">
  import { AlertCircle, ArrowRight, ExternalLink, Link2, Loader2 } from "lucide-svelte";
  import { api, formatAppError } from "../api";
  import { router } from "../stores/router.svelte";
  import { toasts } from "../stores/toast.svelte";

  let urlInput = $state("");
  let loading = $state(false);
  let verifying = $state(false);
  let errorState = $state<{ title: string; detail: string; blockedUrl?: string } | null>(null);

  async function submit(raw?: string) {
    const target = (raw ?? urlInput).trim();
    if (!target || loading) return;

    loading = true;
    errorState = null;

    try {
      const res = await api.resolveUrl(target);
      urlInput = "";
      if (res.startAt) {
        router.goReader(res.manga.id, res.startAt.url);
      } else {
        router.goManga(res.manga.id);
      }
    } catch (err) {
      errorState = formatAppError(err);
    } finally {
      loading = false;
    }
  }

  function handlePaste(e: ClipboardEvent) {
    const pasted = e.clipboardData?.getData("text")?.trim();
    if (pasted && /^https?:\/\//i.test(pasted)) {
      urlInput = pasted;
      queueMicrotask(() => submit(pasted));
    }
  }

  async function handleManualVerify() {
    if (!errorState?.blockedUrl || verifying) return;
    verifying = true;
    try {
      toasts.push("Đang mở cửa sổ trình duyệt để bạn tự xác minh...", "info", 4500);
      await api.openManualVerification(errorState.blockedUrl);
      toasts.push("Đã xác minh xong, đang thử tải lại truyện...", "success");
      await submit(errorState.blockedUrl);
    } catch (err) {
      errorState = formatAppError(err);
    } finally {
      verifying = false;
    }
  }
</script>

<div class="w-full max-w-2xl">
  <form
    onsubmit={(e) => {
      e.preventDefault();
      submit();
    }}
    class="flex items-center gap-2 rounded-2xl border p-2 transition-shadow focus-within:ring-2"
    style="background-color: var(--surface); border-color: var(--border); box-shadow: var(--shadow-sm); --tw-ring-color: var(--accent-soft);"
  >
    <div class="pl-2.5" style="color: var(--text-muted);">
      <Link2 size={19} />
    </div>

    <input
      type="url"
      bind:value={urlInput}
      onpaste={handlePaste}
      disabled={loading}
      placeholder="Dán link truyện hoặc link chương bất kỳ (vd: https://truyenqqko.com/truyen-tranh/...)"
      class="w-full bg-transparent py-2 text-sm outline-none placeholder:opacity-65"
      style="color: var(--text);"
    />

    <button
      type="submit"
      disabled={loading || !urlInput.trim()}
      class="flex shrink-0 items-center gap-1.5 rounded-xl px-4 py-2 text-sm font-medium text-white transition-opacity disabled:opacity-45"
      style="background-color: var(--accent);"
    >
      {#if loading}
        <Loader2 size={16} class="animate-spin" />
        <span>Đang lấy</span>
      {:else}
        <span>Mở truyện</span>
        <ArrowRight size={15} />
      {/if}
    </button>
  </form>

  {#if errorState}
    <div
      class="mt-3 flex items-start gap-3 rounded-xl border p-3.5 text-sm"
      style="background-color: var(--surface); border-color: var(--danger);"
    >
      <AlertCircle size={18} class="mt-0.5 shrink-0" style="color: var(--danger);" />
      <div class="min-w-0 flex-1">
        <div class="font-semibold">{errorState.title}</div>
        <p class="mt-0.5 text-xs leading-relaxed" style="color: var(--text-muted);">
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
              <span>Đang chờ xác minh...</span>
            {:else}
              <ExternalLink size={14} />
              <span>Mở cửa sổ để tự xác minh</span>
            {/if}
          </button>
        {/if}
      </div>
    </div>
  {/if}
</div>
