<script lang="ts">
  import { Download, Languages, Sparkles, X, Zap } from "lucide-svelte";
  import { onMount } from "svelte";
  import { adaptiveOverlay, backdropFade } from "../motion";
  import { aiTranslateStore } from "../stores/aiTranslate.svelte";
  import { deviceEnv } from "../stores/deviceEnv.svelte";
  import { router } from "../stores/router.svelte";
  import { settingsStore } from "../stores/settings.svelte";

  const t = $derived(settingsStore.t);
  const open = $derived(aiTranslateStore.firstUseModalOpen);
  const missingPrompt = $derived(aiTranslateStore.missingNmtPrompt);
  const tier2Supported = $derived(aiTranslateStore.tier2Supported);
  const tier2Bytes = $derived(aiTranslateStore.tier2Model?.requiredBytes ?? 1_455_437_792);

  const missingPacksNames = $derived(
    missingPrompt ? missingPrompt.packs.map((p) => p.displayName).join(" + ") : ""
  );
  const missingPacksBytes = $derived(
    missingPrompt ? missingPrompt.packs.reduce((sum, p) => sum + p.requiredBytes, 0) : 0
  );

  function formatGb(bytes: number): string {
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  function formatMb(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes <= 0) return "~30 MB";
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  onMount(() => {
    const unregister = router.registerBackInterceptor(() => {
      if (aiTranslateStore.firstUseModalOpen) {
        aiTranslateStore.dismissFirstUseModal();
        return true;
      }
      if (aiTranslateStore.missingNmtPrompt) {
        aiTranslateStore.dismissMissingNmtPrompt();
        return true;
      }
      return false;
    });
    return () => unregister();
  });
</script>

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-end justify-center p-0 sm:items-center sm:p-4"
    style="padding-top: max(1rem, var(--safe-top)); padding-left: var(--safe-left); padding-right: var(--safe-right);"
    role="dialog"
    aria-modal="true"
    aria-labelledby="ai-first-use-title"
  >
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      transition:backdropFade|local
      onclick={() => aiTranslateStore.dismissFirstUseModal()}
      class="modal-backdrop fixed inset-0"
    ></div>

    <div
      transition:adaptiveOverlay|local={{ isMobile: deviceEnv.layoutTier === "compact" }}
      class="modal-cq relative z-10 flex w-full max-w-md flex-col gap-4 rounded-t-3xl border-t p-5 sm:rounded-2xl sm:border"
      style="background-color: var(--surface); border-color: var(--border); box-shadow: var(--shadow-lg); padding-bottom: max(1.25rem, var(--safe-bottom));"
    >
      <div class="sheet-handle -mt-2 sm:hidden" aria-hidden="true"></div>

      <div class="flex items-start justify-between gap-3">
        <div class="flex items-start gap-3">
          <span
            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl"
            style="background-color: var(--accent-soft); color: var(--accent);"
          >
            <Sparkles size={20} />
          </span>
          <div>
            <h3 id="ai-first-use-title" class="text-base font-bold">{t.a11yFirstUseTitle}</h3>
            <p class="mt-1 text-xs leading-relaxed" style="color: var(--text-muted);">
              {t.a11yFirstUseBody}
            </p>
          </div>
        </div>
        <button
          type="button"
          onclick={() => aiTranslateStore.dismissFirstUseModal()}
          class="gpu-press flex h-8 w-8 shrink-0 items-center justify-center rounded-xl opacity-75 hover:opacity-100"
          aria-label={t.cancel}
        >
          <X size={16} />
        </button>
      </div>

      <div class="flex flex-col gap-2">
        <button
          type="button"
          onclick={() => void aiTranslateStore.confirmFirstUseWithLite()}
          class="btn-3d btn-3d-accent gpu-press flex min-h-[46px] w-full items-center justify-between gap-2 rounded-xl px-4 py-2.5 text-xs font-semibold"
          style="background-color: var(--accent); color: var(--accent-fg);"
        >
          <span class="inline-flex items-center gap-2">
            <Zap size={15} />
            <span>{t.a11yFirstUseUseLite}</span>
          </span>
          <span class="rounded-full bg-white/20 px-2 py-0.5 text-[10px] font-semibold">
            {t.a11yStatusReady}
          </span>
        </button>

        {#if tier2Supported}
          <button
            type="button"
            onclick={() => void aiTranslateStore.confirmFirstUseWithTier2()}
            class="btn-3d btn-3d-surface gpu-press flex min-h-[46px] w-full items-center justify-between gap-2 rounded-xl border px-4 py-2.5 text-xs font-semibold"
            style="border-color: var(--border); background-color: var(--surface-subtle); color: var(--text);"
          >
            <span class="inline-flex items-center gap-2">
              <Download size={15} style="color: var(--accent);" />
              <span>{t.a11yFirstUseDownloadTier2}</span>
            </span>
            <span class="text-[11px] tabular-nums" style="color: var(--text-muted);">
              {aiTranslateStore.tier2Ready ? t.a11yStatusReady : formatGb(tier2Bytes)}
            </span>
          </button>
        {/if}
      </div>
    </div>
  </div>
{:else if missingPrompt}
  <div
    class="fixed inset-0 z-50 flex items-end justify-center p-0 sm:items-center sm:p-4"
    style="padding-top: max(1rem, var(--safe-top)); padding-left: var(--safe-left); padding-right: var(--safe-right);"
    role="dialog"
    aria-modal="true"
    aria-labelledby="ai-missing-nmt-title"
  >
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      transition:backdropFade|local
      onclick={() => aiTranslateStore.dismissMissingNmtPrompt()}
      class="modal-backdrop fixed inset-0"
    ></div>

    <div
      transition:adaptiveOverlay|local={{ isMobile: deviceEnv.layoutTier === "compact" }}
      class="modal-cq relative z-10 flex w-full max-w-md flex-col gap-4 rounded-t-3xl border-t p-5 sm:rounded-2xl sm:border"
      style="background-color: var(--surface); border-color: var(--border); box-shadow: var(--shadow-lg); padding-bottom: max(1.25rem, var(--safe-bottom));"
    >
      <div class="sheet-handle -mt-2 sm:hidden" aria-hidden="true"></div>

      <div class="flex items-start justify-between gap-3">
        <div class="flex items-start gap-3">
          <span
            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl"
            style="background-color: var(--accent-soft); color: var(--accent);"
          >
            <Languages size={20} />
          </span>
          <div>
            <h3 id="ai-missing-nmt-title" class="text-base font-bold">{t.a11yNmtPromptTitle}</h3>
            <p class="mt-1 text-xs leading-relaxed" style="color: var(--text-muted);">
              {t.a11yNmtPromptBody(missingPacksNames, formatMb(missingPacksBytes))}
            </p>
          </div>
        </div>
        <button
          type="button"
          onclick={() => aiTranslateStore.dismissMissingNmtPrompt()}
          class="gpu-press flex h-8 w-8 shrink-0 items-center justify-center rounded-xl opacity-75 hover:opacity-100"
          aria-label={t.cancel}
        >
          <X size={16} />
        </button>
      </div>

      <div class="flex items-center justify-end gap-2">
        <button
          type="button"
          onclick={() => aiTranslateStore.dismissMissingNmtPrompt()}
          class="btn-3d btn-3d-surface gpu-press min-h-[42px] rounded-xl border px-4 py-2 text-xs font-semibold"
          style="border-color: var(--border); background-color: var(--surface-subtle); color: var(--text);"
        >
          {t.cancel}
        </button>
        <button
          type="button"
          onclick={() => void aiTranslateStore.confirmDownloadMissingNmtPacks()}
          class="btn-3d btn-3d-accent gpu-press inline-flex min-h-[42px] items-center gap-1.5 rounded-xl px-4 py-2 text-xs font-semibold"
          style="background-color: var(--accent); color: var(--accent-fg);"
        >
          <Download size={14} />
          <span>{t.a11yNmtDownloadNowBtn}</span>
        </button>
      </div>
    </div>
  </div>
{/if}
