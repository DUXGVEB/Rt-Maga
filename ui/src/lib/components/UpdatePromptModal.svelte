<script lang="ts">
  import {
    AlertCircle,
    CheckCircle2,
    Download,
    ExternalLink,
    ShieldCheck,
    Sparkles,
    X,
  } from "lucide-svelte";
  import { backdropFade, createOverlayGuard, modalScale } from "../motion";
  import { appReleaseStore } from "../stores/appRelease.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import LatticeLoader from "./LatticeLoader.svelte";

  const t = $derived(settingsStore.t);
  const release = $derived(appReleaseStore.release);
  const progress = $derived(appReleaseStore.progress);
  const modalGuard = createOverlayGuard(180);

  function dismissSafely() {
    if (appReleaseStore.installing || !modalGuard.canAct()) return;
    modalGuard.lock();
    appReleaseStore.dismissPrompt();
  }

  function formatMb(bytes: number): string {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  const percent = $derived(() => {
    if (!progress) return 0;
    if (progress.stage !== "downloading") return 100;
    if (!progress.totalBytes || progress.totalBytes <= 0) return 0;
    return Math.min(100, Math.round((progress.downloadedBytes / progress.totalBytes) * 100));
  });

  const stageLabel = $derived(() => {
    if (!progress) return "";
    switch (progress.stage) {
      case "downloading":
        return t.updateStageDownloading;
      case "verifying":
        return t.updateStageVerifying;
      case "backing_up":
        return t.updateStageBackingUp;
      case "installing":
        return t.updateStageInstalling;
    }
  });

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !appReleaseStore.installing) {
      e.preventDefault();
      appReleaseStore.dismissPrompt();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if release}
  <div
    class="fixed inset-0 z-50 flex items-end justify-center p-0 sm:items-center sm:p-4"
    style="padding-top: max(1rem, var(--safe-top)); padding-left: var(--safe-left); padding-right: var(--safe-right);"
    role="dialog"
    aria-modal="true"
    aria-labelledby="update-modal-title"
  >
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      transition:backdropFade|local
      onclick={dismissSafely}
      class="fixed inset-0 bg-black/55 backdrop-blur-sm"
    ></div>

    <div
      transition:modalScale|local
      class="modal-cq relative z-10 flex max-h-[calc(100dvh-1.5rem-var(--safe-top))] w-full max-w-md flex-col gap-4 overflow-y-auto overscroll-contain rounded-t-3xl border-t p-5 sm:max-h-[calc(100dvh-2rem-var(--safe-top)-var(--safe-bottom))] sm:rounded-2xl sm:border"
      style="background-color: var(--surface); border-color: var(--border); color: var(--text); box-shadow: var(--shadow-lg); padding-bottom: max(1.25rem, var(--safe-bottom));"
    >
      <div class="sheet-handle -mt-2 sm:hidden" aria-hidden="true"></div>
      <!-- Header -->
      <div class="flex items-start justify-between gap-3">
        <div class="flex items-center gap-2.5">
          <div
            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl"
            style="background-color: var(--accent-soft); color: var(--accent);"
          >
            <Sparkles size={18} />
          </div>
          <div>
            <h2 id="update-modal-title" class="text-base font-bold leading-snug">
              {t.releaseAvailableBanner(release.latestVersion)}
            </h2>
            <div class="mt-0.5 flex flex-wrap items-center gap-1.5 text-xs" style="color: var(--text-muted);">
              <span>v{release.currentVersion} → v{release.latestVersion}</span>
              {#if release.downloadSize}
                <span>·</span>
                <span>{formatMb(release.downloadSize)}</span>
              {/if}
            </div>
          </div>
        </div>

        {#if !appReleaseStore.installing}
          <button
            type="button"
            onclick={dismissSafely}
            class="gpu-press flex h-8 w-8 items-center justify-center rounded-xl opacity-65 hover:opacity-100"
            aria-label={t.dismissUpdate}
          >
            <X size={16} />
          </button>
        {/if}
      </div>

      <!-- Prompt description -->
      <p class="text-xs leading-relaxed" style="color: var(--text-muted);">
        {t.updatePromptQuestion}
      </p>

      <!-- Data preservation assurance -->
      <div
        class="flex items-start gap-2.5 rounded-xl border px-3 py-2.5 text-xs"
        style="background-color: var(--surface-subtle); border-color: var(--border);"
      >
        <ShieldCheck size={16} class="mt-0.5 shrink-0" style="color: #22c55e;" />
        <span class="leading-relaxed" style="color: var(--text);">
          {t.updateDataSafeNote}
        </span>
      </div>

      <!-- Live download / install progress -->
      {#if appReleaseStore.installing && progress}
        <div class="flex flex-col gap-2">
          <div class="flex items-center justify-between text-xs font-medium">
            <span class="flex items-center gap-1.5">
              <LatticeLoader size="xs" label={stageLabel()} showTimer inline />
            </span>
            {#if progress.stage === "downloading" && progress.totalBytes}
              <span style="color: var(--text-muted);">
                {formatMb(progress.downloadedBytes)} / {formatMb(progress.totalBytes)} ({percent()}%)
              </span>
            {:else if progress.stage === "downloading" && progress.downloadedBytes > 0}
              <span style="color: var(--text-muted);">
                {formatMb(progress.downloadedBytes)}
              </span>
            {/if}
          </div>

          <div
            class="h-2 w-full overflow-hidden rounded-full"
            style="background-color: var(--surface-hover);"
          >
            <div
              class="h-full w-full origin-left rounded-full transition-transform duration-150 ease-out"
              style="transform: scaleX({Math.max(0, Math.min(1, percent() / 100))}); background-color: var(--accent);"
            ></div>
          </div>
        </div>
      {/if}

      <!-- Outcome / Status messages -->
      {#if appReleaseStore.installStatus === "installing_and_restarting"}
        <div
          class="flex items-start gap-2 rounded-xl border px-3 py-2.5 text-xs"
          style="background-color: var(--accent-soft); border-color: var(--accent); color: var(--accent);"
        >
          <CheckCircle2 size={15} class="mt-0.5 shrink-0" />
          <span>{t.updateStatusRestarting}</span>
        </div>
      {:else if appReleaseStore.installStatus === "permission_required"}
        <div
          class="flex items-start gap-2 rounded-xl border px-3 py-2.5 text-xs"
          style="background-color: var(--surface-subtle); border-color: var(--accent); color: var(--text);"
        >
          <AlertCircle size={15} class="mt-0.5 shrink-0" style="color: var(--accent);" />
          <span>{t.updateAndroidPermissionHint}</span>
        </div>
      {:else if appReleaseStore.installStatus === "installer_launched"}
        <div
          class="flex items-start gap-2 rounded-xl border px-3 py-2.5 text-xs"
          style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
        >
          <CheckCircle2 size={15} class="mt-0.5 shrink-0" style="color: #22c55e;" />
          <span>{t.updateAndroidInstallerOpened}</span>
        </div>
      {/if}

      <!-- Error box -->
      {#if appReleaseStore.installError}
        <div
          class="flex items-start gap-2 rounded-xl border px-3 py-2.5 text-xs"
          style="background-color: var(--surface-subtle); border-color: var(--danger); color: var(--danger);"
        >
          <AlertCircle size={15} class="mt-0.5 shrink-0" />
          <span class="break-words">{appReleaseStore.installError}</span>
        </div>
      {/if}

      <!-- Action buttons -->
      <div class="mt-1 flex flex-wrap items-center justify-end gap-2">
        {#if !appReleaseStore.installing}
          <button
            type="button"
            onclick={() => appReleaseStore.dismissPrompt()}
            class="rounded-xl border px-3.5 py-2 text-xs font-medium transition-opacity hover:opacity-85"
            style="border-color: var(--border); background-color: var(--surface-subtle);"
          >
            {t.updateLater}
          </button>

          <button
            type="button"
            onclick={() => appReleaseStore.openReleasePage()}
            class="inline-flex items-center gap-1.5 rounded-xl border px-3.5 py-2 text-xs font-medium transition-opacity hover:opacity-85"
            style="border-color: var(--border); background-color: var(--surface);"
          >
            <ExternalLink size={13} />
            <span>{t.viewRelease}</span>
          </button>

          <button
            type="button"
            onclick={() => appReleaseStore.installNow()}
            class="inline-flex items-center gap-1.5 rounded-xl px-4 py-2 text-xs font-semibold transition-opacity hover:opacity-90"
            style="background-color: var(--accent); color: var(--accent-fg);"
          >
            <Download size={14} />
            <span>
              {appReleaseStore.installStatus === "permission_required"
                ? t.updateInstallNow
                : t.updateDownloadAndInstall}
            </span>
          </button>
        {/if}
      </div>
    </div>
  </div>
{/if}
