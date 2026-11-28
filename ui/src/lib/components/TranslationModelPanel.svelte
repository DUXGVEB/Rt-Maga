<script lang="ts">
  import {
    AlertCircle,
    CheckCircle2,
    Cpu,
    Download,
    Languages,
    Loader2,
    Sparkles,
    Trash2,
    X,
  } from "lucide-svelte";
  import { onMount } from "svelte";
  import { collapseReveal, popoverPop } from "../motion";
  import { aiTranslateStore } from "../stores/aiTranslate.svelte";
  import { settingsStore } from "../stores/settings.svelte";

  let confirmingDelete = $state(false);
  let confirmingDeletePackId = $state<string | null>(null);

  const t = $derived(settingsStore.t);
  const activeTier = $derived(aiTranslateStore.activeTier);
  const liteModel = $derived(aiTranslateStore.liteModel);
  const tier2Model = $derived(aiTranslateStore.tier2Model);
  const tier2Supported = $derived(aiTranslateStore.tier2Supported);
  const tier2Ready = $derived(aiTranslateStore.tier2Ready);
  const nmtPacks = $derived(aiTranslateStore.nmtPacks);
  const downloading = $derived(aiTranslateStore.downloading);
  const isDownloadingNmtPack = $derived(aiTranslateStore.isDownloadingNmtPack);
  const downloadingTier2 = $derived(downloading && !isDownloadingNmtPack);
  const activeNmtPackId = $derived(
    aiTranslateStore.downloadingNmtPackId ??
      (isDownloadingNmtPack ? aiTranslateStore.downloadProgress?.assetId ?? null : null)
  );
  const progress = $derived(aiTranslateStore.downloadProgress);
  const busy = $derived(aiTranslateStore.busyAction !== null);

  onMount(() => {
    void aiTranslateStore.init();
  });

  function formatBytes(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  function formatSpeed(bps: number): string {
    if (!Number.isFinite(bps) || bps <= 0) return "";
    if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(0)} KB/s`;
    return `${(bps / (1024 * 1024)).toFixed(1)} MB/s`;
  }

  async function selectLite() {
    confirmingDelete = false;
    confirmingDeletePackId = null;
    await aiTranslateStore.setTier("lite");
  }

  async function selectTier2() {
    if (!tier2Ready || !tier2Supported) return;
    confirmingDelete = false;
    confirmingDeletePackId = null;
    await aiTranslateStore.setTier("qwen3_vl_2b");
  }

  async function handleDeleteTier2() {
    confirmingDelete = false;
    await aiTranslateStore.deleteTier2();
  }

  async function handleDeletePack(packId: string) {
    confirmingDeletePackId = null;
    await aiTranslateStore.deleteNmtPack(packId);
  }
</script>

<section
  id="settings-translation-model"
  class="flex flex-col gap-3 rounded-2xl border p-4 shadow-sm"
  style="background-color: var(--surface); border-color: var(--border);"
>
  <div class="flex items-center justify-between gap-2">
    <div class="flex items-center gap-2.5">
      <span
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-xl"
        style="background-color: var(--accent-soft); color: var(--accent);"
      >
        <Sparkles size={15} />
      </span>
      <h2 class="text-sm font-semibold">{t.a11yModelSectionTitle}</h2>
    </div>
    {#if aiTranslateStore.status?.vulkanSupported}
      <span
        class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-semibold"
        style="background-color: var(--surface-subtle); color: var(--text-muted);"
      >
        <Cpu size={11} />
        Vulkan
      </span>
    {/if}
  </div>

  {#if aiTranslateStore.lastError}
    <div
      transition:collapseReveal|local
      class="flex items-start justify-between gap-2 rounded-xl border px-3 py-2 text-xs"
      style="border-color: color-mix(in srgb, var(--danger) 40%, var(--border)); background-color: color-mix(in srgb, var(--danger) 10%, var(--surface)); color: var(--danger);"
    >
      <div class="flex min-w-0 items-start gap-2">
        <AlertCircle size={14} class="mt-0.5 shrink-0" />
        <span class="break-words">{aiTranslateStore.lastError}</span>
      </div>
      <button
        type="button"
        onclick={() => aiTranslateStore.clearLastError()}
        class="shrink-0 opacity-75 hover:opacity-100"
        aria-label={t.cancel}
      >
        <X size={13} />
      </button>
    </div>
  {/if}

  <div class="flex flex-col gap-2" role="radiogroup" aria-label={t.a11yModelSectionTitle}>
    <!-- Tier 1: Lite (Built-in) -->
    <button
      type="button"
      role="radio"
      aria-checked={activeTier === "lite"}
      disabled={busy && !downloading}
      onclick={selectLite}
      class="btn-3d btn-3d-surface gpu-press flex w-full items-center justify-between gap-3 rounded-xl border px-3.5 py-3 text-left"
      style="border-color: {activeTier === 'lite'
        ? 'var(--accent)'
        : 'var(--border)'}; background-color: {activeTier === 'lite'
        ? 'color-mix(in srgb, var(--surface) 86%, var(--accent-soft))'
        : 'var(--surface-subtle)'};"
    >
      <div class="flex min-w-0 items-center gap-2.5">
        <span
          class="flex h-4 w-4 shrink-0 items-center justify-center rounded-full border-2"
          style="border-color: {activeTier === 'lite' ? 'var(--accent)' : 'var(--border-strong)'};"
        >
          {#if activeTier === "lite"}
            <span class="h-2 w-2 rounded-full" style="background-color: var(--accent);"></span>
          {/if}
        </span>
        <div class="min-w-0">
          <div class="truncate text-xs font-semibold sm:text-sm">{t.a11yModelLiteTitle}</div>
          {#if liteModel}
            <div class="text-[11px] tabular-nums" style="color: var(--text-muted);">
              {formatBytes(liteModel.installedBytes || liteModel.requiredBytes)}
            </div>
          {/if}
        </div>
      </div>

      <span
        class="inline-flex shrink-0 items-center gap-1 rounded-full px-2.5 py-0.5 text-[11px] font-semibold"
        style="background-color: rgba(16, 185, 129, 0.14); color: #10b981;"
      >
        <CheckCircle2 size={12} />
        <span>{t.a11yStatusReady}</span>
      </span>
    </button>

    <!-- Tier 2: Qwen3-VL-2B (High Accuracy) -->
    <div
      class="flex flex-col gap-2.5 rounded-xl border px-3.5 py-3"
      style="border-color: {activeTier === 'qwen3_vl_2b'
        ? 'var(--accent)'
        : 'var(--border)'}; background-color: {activeTier === 'qwen3_vl_2b'
        ? 'color-mix(in srgb, var(--surface) 86%, var(--accent-soft))'
        : 'var(--surface-subtle)'};"
    >
      <div class="flex flex-wrap items-center justify-between gap-2">
        <button
          type="button"
          role="radio"
          aria-checked={activeTier === "qwen3_vl_2b"}
          disabled={!tier2Ready || !tier2Supported || (busy && !downloading)}
          onclick={selectTier2}
          class="flex min-w-0 flex-1 items-center gap-2.5 text-left disabled:cursor-default"
        >
          <span
            class="flex h-4 w-4 shrink-0 items-center justify-center rounded-full border-2"
            style="border-color: {activeTier === 'qwen3_vl_2b'
              ? 'var(--accent)'
              : 'var(--border-strong)'};"
          >
            {#if activeTier === "qwen3_vl_2b"}
              <span class="h-2 w-2 rounded-full" style="background-color: var(--accent);"></span>
            {/if}
          </span>
          <div class="min-w-0">
            <div class="truncate text-xs font-semibold sm:text-sm">{t.a11yModelTier2Title}</div>
            {#if tier2Model}
              <div class="text-[11px] tabular-nums" style="color: var(--text-muted);">
                {formatBytes(
                  tier2Ready
                    ? tier2Model.installedBytes || tier2Model.requiredBytes
                    : tier2Model.requiredBytes
                )}
              </div>
            {/if}
          </div>
        </button>

        <div class="flex shrink-0 items-center gap-1.5">
          {#if !tier2Supported}
            <span
              class="rounded-full px-2.5 py-0.5 text-[11px] font-medium"
              style="background-color: var(--surface-hover); color: var(--text-muted);"
            >
              {t.a11yTier2Unsupported}
            </span>
          {:else if downloadingTier2}
            <span
              class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-semibold"
              style="background-color: var(--accent-soft); color: var(--accent);"
            >
              <Loader2 size={11} class="animate-spin" />
              <span>{t.a11yStatusDownloading}</span>
            </span>
            <button
              type="button"
              onclick={() => void aiTranslateStore.cancelDownload()}
              class="btn-3d btn-3d-surface gpu-press inline-flex min-h-[34px] items-center gap-1 rounded-xl border px-2.5 py-1 text-xs font-semibold"
              style="border-color: var(--border); background-color: var(--surface); color: var(--danger);"
            >
              <X size={13} />
              <span>{t.a11yCancelDownloadBtn}</span>
            </button>
          {:else if tier2Ready}
            <span
              class="inline-flex items-center gap-1 rounded-full px-2.5 py-0.5 text-[11px] font-semibold"
              style="background-color: rgba(16, 185, 129, 0.14); color: #10b981;"
            >
              <CheckCircle2 size={12} />
              <span>{t.a11yStatusReady}</span>
            </span>
            {#if confirmingDelete}
              <div in:popoverPop|local={{ duration: 140, y: 0 }} class="flex items-center gap-1">
                <button
                  type="button"
                  disabled={busy}
                  onclick={handleDeleteTier2}
                  class="btn-3d gpu-press rounded-lg px-2.5 py-1 text-[11px] font-semibold text-white"
                  style="background-color: var(--danger);"
                >
                  {t.a11yDeleteModelBtn}
                </button>
                <button
                  type="button"
                  onclick={() => (confirmingDelete = false)}
                  class="btn-3d btn-3d-surface gpu-press rounded-lg border px-2 py-1 text-[11px] font-medium"
                  style="border-color: var(--border); background-color: var(--surface);"
                >
                  {t.cancel}
                </button>
              </div>
            {:else}
              <button
                type="button"
                disabled={busy}
                onclick={() => (confirmingDelete = true)}
                class="btn-3d btn-3d-surface gpu-press inline-flex min-h-[34px] items-center gap-1 rounded-xl border px-2.5 py-1 text-xs font-medium"
                style="border-color: var(--border); background-color: var(--surface); color: var(--danger);"
                title={t.a11yDeleteModelBtn}
                aria-label={t.a11yDeleteModelBtn}
              >
                <Trash2 size={13} />
                <span>{t.a11yDeleteModelBtn}</span>
              </button>
            {/if}
          {:else}
            <span
              class="rounded-full px-2.5 py-0.5 text-[11px] font-medium"
              style="background-color: var(--surface-hover); color: var(--text-muted);"
            >
              {t.a11yStatusNotDownloaded}
            </span>
            <button
              type="button"
              disabled={busy || downloading}
              onclick={() => void aiTranslateStore.downloadTier2()}
              class="btn-3d btn-3d-accent gpu-press inline-flex min-h-[34px] items-center gap-1.5 rounded-xl px-3 py-1 text-xs font-semibold disabled:opacity-50"
              style="background-color: var(--accent); color: var(--accent-fg);"
            >
              <Download size={13} />
              <span>{t.a11yDownloadBtn}</span>
            </button>
          {/if}
        </div>
      </div>

      {#if downloadingTier2 && progress}
        <div
          transition:collapseReveal|local
          class="flex flex-col gap-1.5 border-t pt-2.5 text-[11px]"
          style="border-color: var(--border);"
        >
          <div class="flex items-center justify-between gap-2 tabular-nums">
            <span class="truncate font-medium" style="color: var(--text-muted);">
              {formatBytes(progress.downloadedBytes)} / {formatBytes(progress.totalBytes)}
              {#if progress.speedBytesPerSec > 0}
                · {formatSpeed(progress.speedBytesPerSec)}
              {/if}
            </span>
            <span class="font-mono font-semibold" style="color: var(--accent);">
              {Math.max(0, Math.min(100, Math.round(progress.percent)))}%
            </span>
          </div>
          <div class="h-1.5 w-full overflow-hidden rounded-full" style="background-color: var(--border);">
            <div
              class="h-full w-full origin-left rounded-full transition-transform duration-150 ease-out"
              style="transform: scaleX({Math.max(0.03, Math.min(1, progress.percent / 100))}); background-color: var(--accent);"
            ></div>
          </div>
        </div>
      {/if}
    </div>
  </div>

  <!-- Offline Language Packs (Lite NMT / ML Kit) -->
  {#if nmtPacks.length > 0}
    <div class="flex flex-col gap-2 border-t pt-3" style="border-color: var(--border);">
      <div class="flex items-center gap-1.5 text-xs font-semibold">
        <Languages size={14} style="color: var(--accent);" />
        <span>{t.a11yNmtPacksTitle}</span>
      </div>

      <div class="flex flex-col gap-1.5">
        {#each nmtPacks as pack (pack.id)}
          {@const packDownloading = isDownloadingNmtPack && activeNmtPackId === pack.id}
          <div
            class="flex flex-col gap-2 rounded-xl border px-3 py-2"
            style="border-color: var(--border); background-color: var(--surface-subtle);"
          >
            <div class="flex flex-wrap items-center justify-between gap-2">
              <div class="min-w-0 flex-1">
                <div class="truncate text-xs font-semibold">{pack.displayName}</div>
                <div class="text-[11px] tabular-nums" style="color: var(--text-muted);">
                  {formatBytes(pack.ready ? pack.installedBytes || pack.requiredBytes : pack.requiredBytes)}
                </div>
              </div>

              <div class="flex shrink-0 items-center gap-1.5">
                {#if packDownloading}
                  <span
                    class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-semibold"
                    style="background-color: var(--accent-soft); color: var(--accent);"
                  >
                    <Loader2 size={11} class="animate-spin" />
                    <span>{t.a11yStatusDownloading}</span>
                  </span>
                  <button
                    type="button"
                    onclick={() => void aiTranslateStore.cancelDownload()}
                    class="btn-3d btn-3d-surface gpu-press inline-flex min-h-[30px] items-center gap-1 rounded-lg border px-2 py-0.5 text-[11px] font-semibold"
                    style="border-color: var(--border); background-color: var(--surface); color: var(--danger);"
                  >
                    <X size={12} />
                    <span>{t.a11yCancelDownloadBtn}</span>
                  </button>
                {:else if pack.ready}
                  <span
                    class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-semibold"
                    style="background-color: rgba(16, 185, 129, 0.14); color: #10b981;"
                  >
                    <CheckCircle2 size={11} />
                    <span>{t.a11yStatusReady}</span>
                  </span>
                  {#if confirmingDeletePackId === pack.id}
                    <div in:popoverPop|local={{ duration: 140, y: 0 }} class="flex items-center gap-1">
                      <button
                        type="button"
                        disabled={busy}
                        onclick={() => void handleDeletePack(pack.id)}
                        class="btn-3d gpu-press rounded-lg px-2 py-0.5 text-[11px] font-semibold text-white"
                        style="background-color: var(--danger);"
                      >
                        {t.a11yDeleteModelBtn}
                      </button>
                      <button
                        type="button"
                        onclick={() => (confirmingDeletePackId = null)}
                        class="btn-3d btn-3d-surface gpu-press rounded-lg border px-2 py-0.5 text-[11px] font-medium"
                        style="border-color: var(--border); background-color: var(--surface);"
                      >
                        {t.cancel}
                      </button>
                    </div>
                  {:else}
                    <button
                      type="button"
                      disabled={busy || downloading}
                      onclick={() => (confirmingDeletePackId = pack.id)}
                      class="btn-3d btn-3d-surface gpu-press inline-flex min-h-[30px] items-center gap-1 rounded-lg border px-2 py-0.5 text-[11px] font-medium disabled:opacity-50"
                      style="border-color: var(--border); background-color: var(--surface); color: var(--danger);"
                      title={t.a11yDeleteModelBtn}
                      aria-label={t.a11yDeleteModelBtn}
                    >
                      <Trash2 size={12} />
                    </button>
                  {/if}
                {:else}
                  <button
                    type="button"
                    disabled={busy || downloading}
                    onclick={() => void aiTranslateStore.downloadNmtPacks([pack.id])}
                    class="btn-3d btn-3d-surface gpu-press inline-flex min-h-[30px] items-center gap-1 rounded-lg border px-2.5 py-0.5 text-[11px] font-semibold disabled:opacity-50"
                    style="border-color: var(--border); background-color: var(--surface); color: var(--accent);"
                  >
                    <Download size={12} />
                    <span>{t.a11yDownloadBtn}</span>
                  </button>
                {/if}
              </div>
            </div>

            {#if packDownloading && progress}
              <div
                transition:collapseReveal|local
                class="flex flex-col gap-1 border-t pt-1.5 text-[10px]"
                style="border-color: var(--border);"
              >
                <div class="flex items-center justify-between gap-2 tabular-nums">
                  <span class="truncate font-medium" style="color: var(--text-muted);">
                    {#if progress.totalBytes > 0}
                      {formatBytes(progress.downloadedBytes)} / {formatBytes(progress.totalBytes)}
                    {:else}
                      {t.a11yStatusDownloading}
                    {/if}
                    {#if progress.speedBytesPerSec > 0}
                      · {formatSpeed(progress.speedBytesPerSec)}
                    {/if}
                  </span>
                  <span class="font-mono font-semibold" style="color: var(--accent);">
                    {Math.max(0, Math.min(100, Math.round(progress.percent)))}%
                  </span>
                </div>
                <div class="h-1 w-full overflow-hidden rounded-full" style="background-color: var(--border);">
                  <div
                    class="h-full w-full origin-left rounded-full transition-transform duration-150 ease-out"
                    style="transform: scaleX({Math.max(0.05, Math.min(1, progress.percent / 100))}); background-color: var(--accent);"
                  ></div>
                </div>
              </div>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}
</section>
