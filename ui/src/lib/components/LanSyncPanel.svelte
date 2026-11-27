<script lang="ts">
  import jsQR from "jsqr";
  import {
    Camera,
    CheckCircle2,
    Clock,
    Copy,
    ImageUp,
    Laptop,
    Loader2,
    QrCode,
    Smartphone,
    Wifi,
    XCircle,
  } from "lucide-svelte";
  import { onMount } from "svelte";
  import { api, formatAppError } from "../api";
  import { collapseReveal, popoverPop, viewReveal } from "../motion";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
  import { updatesStore } from "../stores/updates.svelte";
  import type { SyncDecision, SyncStatusView } from "../types";
  import LatticeLoader from "./LatticeLoader.svelte";

  const s = $derived(settingsStore.value);
  const t = $derived(settingsStore.t);

  let platform = $state<"windows" | "android" | "other">("windows");
  let activeTab = $state<"primary" | "secondary">("primary");
  let status = $state<SyncStatusView | null>(null);
  let busy = $state(false);
  let errorMsg = $state<string | null>(null);
  let nowSec = $state(Math.floor(Date.now() / 1000));
  let copiedCode = $state(false);

  // PC receive (PIN) inputs
  let pinInput = $state("");
  let showManualAddr = $state(false);
  let manualAddr = $state("");

  // Phone receive (QR camera / image / paste) inputs
  let qrPasteInput = $state("");
  let cameraActive = $state(false);
  let cameraError = $state<string | null>(null);
  let videoEl = $state<HTMLVideoElement | null>(null);
  let canvasEl = $state<HTMLCanvasElement | null>(null);
  let fileInputEl = $state<HTMLInputElement | null>(null);
  let mediaStream: MediaStream | null = null;
  let scanRafId: number | null = null;
  let lastScanMs = 0;

  const isPhone = $derived(platform === "android");

  const qrDataUrl = $derived.by(() => {
    const svg = status?.qrSvg;
    if (!svg) return null;
    return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
  });

  const remainingSec = $derived.by(() => {
    const exp = status?.expiresAtUnix;
    if (!exp) return 0;
    return Math.max(0, exp - nowSec);
  });

  const formattedRemaining = $derived.by(() => {
    const mins = Math.floor(remainingSec / 60);
    const secs = remainingSec % 60;
    return `${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
  });

  const isActiveFlow = $derived.by(() => {
    const p = status?.phase;
    return (
      p === "hosting_waiting" ||
      p === "host_confirm_peer" ||
      p === "host_waiting_pull" ||
      p === "receiver_confirm_peer" ||
      p === "receiver_waiting_host" ||
      p === "transferring"
    );
  });

  onMount(() => {
    let unlisten: (() => void) | null = null;
    let alive = true;

    void api
      .getPlatform()
      .then((p) => {
        if (alive) platform = p;
      })
      .catch(() => {
        if (alive) platform = /android/i.test(navigator.userAgent) ? "android" : "windows";
      });

    void api
      .syncGetStatus()
      .then((st) => {
        if (alive) status = st;
      })
      .catch(() => {
        // Ignore in non-Tauri preview.
      });

    void api
      .onSyncStateChanged((st) => {
        if (!alive) return;
        status = st;
        if (st.phase === "completed") {
          stopCamera();
          toasts.push(t.syncCompletedTitle, "success");
          if (st.report) {
            void api
              .listHistory()
              .then((items) =>
                updatesStore.setNewCountFromHistory(items.filter((i) => i.hasNew).length)
              )
              .catch(() => {});
          }
        } else if (st.phase === "expired" || st.phase === "locked_out") {
          stopCamera();
        }
      })
      .then((fn) => {
        if (alive) unlisten = fn;
        else fn();
      })
      .catch(() => {});

    const tick = window.setInterval(() => {
      nowSec = Math.floor(Date.now() / 1000);
      if (status?.expiresAtUnix && nowSec >= status.expiresAtUnix && isActiveFlow) {
        void api
          .syncGetStatus()
          .then((st) => {
            if (alive) status = st;
          })
          .catch(() => {});
      }
    }, 1000);

    return () => {
      alive = false;
      window.clearInterval(tick);
      stopCamera();
      if (unlisten) unlisten();
    };
  });

  function stopCamera() {
    if (scanRafId !== null) {
      window.cancelAnimationFrame(scanRafId);
      scanRafId = null;
    }
    if (mediaStream) {
      for (const track of mediaStream.getTracks()) {
        track.stop();
      }
      mediaStream = null;
    }
    if (videoEl) {
      videoEl.srcObject = null;
    }
    cameraActive = false;
  }

  async function startCameraScanner() {
    errorMsg = null;
    cameraError = null;
    if (!navigator.mediaDevices?.getUserMedia) {
      cameraError = t.syncCameraError;
      return;
    }
    try {
      mediaStream = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: { ideal: "environment" } },
        audio: false,
      });
      cameraActive = true;
      // Wait a tick for <video> to bind in DOM.
      await new Promise((r) => window.setTimeout(r, 50));
      if (!videoEl || !mediaStream) {
        stopCamera();
        return;
      }
      videoEl.srcObject = mediaStream;
      await videoEl.play();
      lastScanMs = 0;
      scanRafId = window.requestAnimationFrame(scanCameraFrame);
    } catch {
      stopCamera();
      cameraError = t.syncCameraError;
    }
  }

  function scanCameraFrame(ts: number) {
    if (!cameraActive || !videoEl || !canvasEl) return;

    if (ts - lastScanMs >= 160 && videoEl.readyState >= 2) {
      lastScanMs = ts;
      const w = videoEl.videoWidth;
      const h = videoEl.videoHeight;
      if (w > 0 && h > 0) {
        canvasEl.width = w;
        canvasEl.height = h;
        const ctx = canvasEl.getContext("2d", { willReadFrequently: true });
        if (ctx) {
          ctx.drawImage(videoEl, 0, 0, w, h);
          const img = ctx.getImageData(0, 0, w, h);
          const decoded = jsQR(img.data, w, h, { inversionAttempts: "dontInvert" });
          if (decoded?.data) {
            stopCamera();
            void connectWithQr(decoded.data);
            return;
          }
        }
      }
    }

    scanRafId = window.requestAnimationFrame(scanCameraFrame);
  }

  async function handleQrImageUpload(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;

    errorMsg = null;
    cameraError = null;

    try {
      const bitmap = await createImageBitmap(file);
      const canvas = document.createElement("canvas");
      canvas.width = bitmap.width;
      canvas.height = bitmap.height;
      const ctx = canvas.getContext("2d");
      if (!ctx) throw new Error("Canvas unavailable");
      ctx.drawImage(bitmap, 0, 0);
      bitmap.close();

      const img = ctx.getImageData(0, 0, canvas.width, canvas.height);
      const decoded = jsQR(img.data, canvas.width, canvas.height);
      if (!decoded?.data) {
        errorMsg = t.syncQrNotFound;
        return;
      }
      await connectWithQr(decoded.data);
    } catch {
      errorMsg = t.syncQrNotFound;
    }
  }

  async function startSender() {
    busy = true;
    errorMsg = null;
    try {
      status = await api.syncStartSender(isPhone ? "phone_pin" : "pc_qr");
    } catch (err) {
      errorMsg = formatAppError(err, s.language).detail;
    } finally {
      busy = false;
    }
  }

  async function cancelOrReset() {
    stopCamera();
    busy = true;
    errorMsg = null;
    try {
      status = await api.syncCancel();
    } catch (err) {
      errorMsg = formatAppError(err, s.language).detail;
    } finally {
      busy = false;
    }
  }

  async function connectWithQr(payload: string) {
    const trimmed = payload.trim();
    if (!trimmed) return;
    busy = true;
    errorMsg = null;
    try {
      status = await api.syncConnectQr(trimmed);
      qrPasteInput = "";
    } catch (err) {
      errorMsg = formatAppError(err, s.language).detail;
    } finally {
      busy = false;
    }
  }

  async function connectWithPin() {
    const pin = pinInput.replace(/\D/g, "");
    if (pin.length !== 6) return;
    busy = true;
    errorMsg = null;
    try {
      status = await api.syncConnectPin(pin, showManualAddr ? manualAddr.trim() || null : null);
      pinInput = "";
    } catch (err) {
      errorMsg = formatAppError(err, s.language).detail;
    } finally {
      busy = false;
    }
  }

  async function respondHost(decision: SyncDecision) {
    busy = true;
    errorMsg = null;
    try {
      status = await api.syncHostRespond(decision);
    } catch (err) {
      errorMsg = formatAppError(err, s.language).detail;
    } finally {
      busy = false;
    }
  }

  async function respondReceiver(decision: SyncDecision) {
    busy = true;
    errorMsg = null;
    try {
      status = await api.syncReceiverConfirm(decision);
    } catch (err) {
      errorMsg = formatAppError(err, s.language).detail;
    } finally {
      busy = false;
    }
  }

  async function copyFallbackCode() {
    const code = status?.compactCode;
    if (!code) return;
    try {
      await navigator.clipboard.writeText(code);
      copiedCode = true;
      window.setTimeout(() => (copiedCode = false), 1800);
    } catch {
      // Ignore clipboard failures.
    }
  }

  function formatPinSpaced(pin: string): string {
    return pin.length === 6 ? `${pin.slice(0, 3)} ${pin.slice(3)}` : pin;
  }
</script>

<section class="settings-card">
  <div class="flex flex-wrap items-start justify-between gap-2">
    <div>
      <h2 class="section-title flex items-center gap-2">
        <Wifi size={15} />
        <span>{t.syncSection}</span>
      </h2>
    </div>

    <span
      class="inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1 text-[11px] font-medium"
      style="border-color: var(--border); background-color: var(--surface-subtle); color: var(--text-muted);"
    >
      {#if isPhone}
        <Smartphone size={12} />
      {:else}
        <Laptop size={12} />
      {/if}
      <span>{t.syncDeviceLabel(status?.localDeviceName ?? (isPhone ? "Android" : "PC"))}</span>
    </span>
  </div>

  <!-- Terminal / Completed / Active Session Cards -->
  {#if status?.phase === "host_confirm_peer"}
    <div
      in:viewReveal|local={{ duration: 175, y: 6 }}
      class="flex flex-col gap-3 rounded-xl border p-4"
      style="border-color: var(--accent); background-color: var(--accent-soft);"
    >
      <div class="flex items-center justify-between gap-2">
        <span class="text-sm font-semibold">{t.syncConfirmTitle}</span>
        <span class="inline-flex items-center gap-1 font-mono text-xs">
          <Clock size={12} />
          {t.syncExpiresIn(formattedRemaining)}
        </span>
      </div>
      <p class="text-xs leading-relaxed">
        {t.syncConfirmSendDesc(
          status.peerName ?? "Unknown",
          status.peerIp ?? "-",
          status.mangaCount
        )}
      </p>
      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          disabled={busy}
          onclick={() => respondHost("approve")}
          class="gpu-press inline-flex items-center gap-1.5 rounded-lg px-3.5 py-1.5 text-xs font-semibold disabled:opacity-50"
          style="background-color: var(--accent); color: var(--accent-fg);"
        >
          <CheckCircle2 size={14} />
          <span>{t.syncApproveBtn}</span>
        </button>
        <button
          type="button"
          disabled={busy}
          onclick={() => respondHost("reject")}
          class="gpu-press inline-flex items-center gap-1.5 rounded-lg border px-3.5 py-1.5 text-xs font-medium disabled:opacity-50"
          style="border-color: var(--border); background-color: var(--surface);"
        >
          <XCircle size={14} />
          <span>{t.syncDeclineBtn}</span>
        </button>
      </div>
    </div>
  {:else if status?.phase === "receiver_confirm_peer"}
    <div
      in:viewReveal|local={{ duration: 175, y: 6 }}
      class="flex flex-col gap-3 rounded-xl border p-4"
      style="border-color: var(--accent); background-color: var(--accent-soft);"
    >
      <div class="flex items-center justify-between gap-2">
        <span class="text-sm font-semibold">{t.syncConfirmTitle}</span>
        <span class="inline-flex items-center gap-1 font-mono text-xs">
          <Clock size={12} />
          {t.syncExpiresIn(formattedRemaining)}
        </span>
      </div>
      <p class="text-xs leading-relaxed">
        {t.syncConfirmReceiveDesc(
          status.peerName ?? "Unknown",
          status.peerIp ?? "-",
          status.mangaCount
        )}
      </p>
      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          disabled={busy}
          onclick={() => respondReceiver("approve")}
          class="gpu-press inline-flex items-center gap-1.5 rounded-lg px-3.5 py-1.5 text-xs font-semibold disabled:opacity-50"
          style="background-color: var(--accent); color: var(--accent-fg);"
        >
          <CheckCircle2 size={14} />
          <span>{t.syncApproveBtn}</span>
        </button>
        <button
          type="button"
          disabled={busy}
          onclick={() => respondReceiver("reject")}
          class="gpu-press inline-flex items-center gap-1.5 rounded-lg border px-3.5 py-1.5 text-xs font-medium disabled:opacity-50"
          style="border-color: var(--border); background-color: var(--surface);"
        >
          <XCircle size={14} />
          <span>{t.syncDeclineBtn}</span>
        </button>
      </div>
    </div>
  {:else if status?.phase === "host_waiting_pull" || status?.phase === "receiver_waiting_host" || status?.phase === "transferring"}
    <div
      in:viewReveal|local={{ duration: 175, y: 6 }}
      class="flex flex-wrap items-center justify-between gap-3 rounded-xl border p-4"
      style="border-color: var(--border); background-color: var(--surface-subtle);"
    >
      <div class="flex items-center gap-2.5 text-xs">
        <LatticeLoader
          size="sm"
          label={status.phase === "transferring" ? t.syncTransferring : t.syncWaitingOtherConfirm}
          showTimer
          inline
        />
      </div>
      <button
        type="button"
        disabled={busy}
        onclick={cancelOrReset}
        class="gpu-press rounded-lg border px-3 py-1.5 text-xs font-medium"
        style="border-color: var(--border); background-color: var(--surface);"
      >
        {t.syncCancelBtn}
      </button>
    </div>
  {:else if status?.phase === "completed"}
    <div
      in:viewReveal|local={{ duration: 175, y: 6 }}
      class="flex flex-col gap-2.5 rounded-xl border p-4"
      style="border-color: var(--success, #10b981); background-color: var(--surface-subtle);"
    >
      <div class="flex items-center justify-between gap-2">
        <div
          class="flex items-center gap-2 text-sm font-semibold"
          style="color: var(--success, #10b981);"
        >
          <CheckCircle2 size={16} />
          <span>{t.syncCompletedTitle}</span>
        </div>
        <button
          type="button"
          onclick={cancelOrReset}
          class="gpu-press rounded-lg border px-3 py-1 text-xs font-medium"
          style="border-color: var(--border); background-color: var(--surface);"
        >
          {t.syncResetBtn}
        </button>
      </div>
      {#if status.report}
        <div class="text-xs" style="color: var(--text-muted);">
          {t.syncReportSummary(
            status.report.addedManga,
            status.report.updatedNewerChapter,
            status.report.skippedSameChapter,
            status.report.keptLocalNewer
          )}
        </div>
      {:else if status.message}
        <div class="text-xs" style="color: var(--text-muted);">{status.message}</div>
      {/if}
    </div>
  {:else if status?.phase === "hosting_waiting"}
    <!-- Active Sender Waiting Card (PC shows QR; Phone shows 6-digit Pairing Code) -->
    <div
      in:viewReveal|local={{ duration: 175, y: 6 }}
      class="flex flex-col items-center gap-3.5 rounded-xl border p-4 text-center"
      style="border-color: var(--border); background-color: var(--surface-subtle);"
    >
      <div class="flex w-full flex-wrap items-center justify-between gap-2 text-xs">
        <span class="inline-flex items-center gap-1.5 font-medium">
          <LatticeLoader size="xs" label={t.syncWaitingPeer} inline />
        </span>
        <span class="inline-flex items-center gap-1 font-mono font-semibold" style="color: var(--accent);">
          <Clock size={12} />
          {t.syncExpiresIn(formattedRemaining)}
        </span>
      </div>

      {#if status.mode === "pc_qr" && qrDataUrl}
        <div in:popoverPop|local={{ duration: 180, y: 6 }} class="rounded-2xl bg-white p-3 shadow-sm">
          <img src={qrDataUrl} alt="RtMaga LAN Sync QR" class="h-52 w-52" />
        </div>
        {#if status.compactCode}
          <button
            type="button"
            onclick={copyFallbackCode}
            class="gpu-press inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium"
            style="border-color: var(--border); background-color: var(--surface);"
          >
            <Copy size={12} />
            <span>{copiedCode ? t.syncCopied : t.syncCopyCode}</span>
          </button>
        {/if}
      {:else if status.pairingCode}
        <div
          in:popoverPop|local={{ duration: 180, y: 6 }}
          class="rounded-2xl border px-7 py-4 font-mono text-3xl font-bold tracking-[0.25em]"
          style="border-color: var(--accent); background-color: var(--surface); color: var(--accent);"
        >
          {formatPinSpaced(status.pairingCode)}
        </div>
      {/if}

      {#if status.localIp && status.localPort}
        <div class="font-mono text-[11px]" style="color: var(--text-muted);">
          {t.syncFallbackAddr(`${status.localIp}:${status.localPort}`)}
        </div>
      {/if}

      <button
        type="button"
        disabled={busy}
        onclick={cancelOrReset}
        class="gpu-press rounded-lg border px-3.5 py-1.5 text-xs font-medium"
        style="border-color: var(--border); background-color: var(--surface); color: var(--danger);"
      >
        {t.syncCancelBtn}
      </button>
    </div>
  {:else}
    <!-- Terminal notice banner if expired / locked / cancelled / error -->
    {#if status && status.phase !== "idle" && status.message}
      <div
        transition:collapseReveal|local
        class="flex items-center justify-between gap-2 rounded-xl border px-3 py-2 text-xs"
        style="border-color: var(--danger); color: var(--danger);"
      >
        <span>{status.message}</span>
        <button
          type="button"
          onclick={cancelOrReset}
          class="gpu-press rounded border px-2 py-0.5 text-[11px]"
          style="border-color: var(--border); color: var(--text);"
        >
          {t.syncResetBtn}
        </button>
      </div>
    {/if}

    <!-- Platform-Specific Sync Tabs (PC UI differs from Android Phone UI) -->
    <div class="grid grid-cols-2 gap-1.5">
      <button
        type="button"
        onclick={() => {
          stopCamera();
          errorMsg = null;
          activeTab = "primary";
        }}
        class="pill gpu-press text-center"
        class:pill-active={activeTab === "primary"}
      >
        {isPhone ? t.syncPhoneReceiveTab : t.syncPcSendTab}
      </button>
      <button
        type="button"
        onclick={() => {
          stopCamera();
          errorMsg = null;
          activeTab = "secondary";
        }}
        class="pill gpu-press text-center"
        class:pill-active={activeTab === "secondary"}
      >
        {isPhone ? t.syncPhoneSendTab : t.syncPcReceiveTab}
      </button>
    </div>

    {#key `${isPhone}:${activeTab}`}
      <div in:viewReveal|local={{ duration: 150, y: 4 }}>
        {#if !isPhone}
          <!-- PC INTERFACE -->
          {#if activeTab === "primary"}
            <!-- PC -> Phone via QR -->
            <div
              class="flex flex-col items-start gap-3 rounded-xl border p-4"
              style="border-color: var(--border); background-color: var(--surface-subtle);"
            >
              <button
                type="button"
                disabled={busy || isActiveFlow}
                onclick={startSender}
                class="gpu-press inline-flex items-center gap-2 rounded-lg px-4 py-2 text-xs font-semibold disabled:opacity-45"
                style="background-color: var(--accent); color: var(--accent-fg);"
              >
                <QrCode size={15} />
                <span>{t.syncStartQrBtn}</span>
              </button>
            </div>
          {:else}
            <!-- Phone -> PC via 6-digit Pairing Code + LAN -->
            <div
              class="flex flex-col gap-3 rounded-xl border p-4"
              style="border-color: var(--border); background-color: var(--surface-subtle);"
            >
              <label class="flex flex-col gap-1">
                <span class="text-xs font-medium">{t.syncEnterPinLabel}</span>
                <div class="flex flex-wrap items-center gap-2">
                  <input
                    type="text"
                    inputmode="numeric"
                    maxlength="7"
                    bind:value={pinInput}
                    placeholder={t.syncEnterPinPlaceholder}
                    class="field w-44 font-mono text-sm tracking-widest"
                  />
                  <button
                    type="button"
                    disabled={busy || pinInput.replace(/\D/g, "").length !== 6}
                    onclick={connectWithPin}
                    class="gpu-press inline-flex items-center gap-1.5 rounded-lg px-4 py-1.5 text-xs font-semibold disabled:opacity-45"
                    style="background-color: var(--accent); color: var(--accent-fg);"
                  >
                    {#if busy}
                      <Loader2 size={13} class="animate-spin" />
                      <span>{t.syncConnecting}</span>
                    {:else}
                      <span>{t.syncConnectBtn}</span>
                    {/if}
                  </button>
                </div>
              </label>

              <button
                type="button"
                onclick={() => (showManualAddr = !showManualAddr)}
                class="gpu-press self-start text-[11px] underline opacity-80 hover:opacity-100"
              >
                {t.syncManualAddrToggle}
              </button>

              {#if showManualAddr}
                <div transition:collapseReveal|local>
                  <input
                    type="text"
                    bind:value={manualAddr}
                    placeholder={t.syncManualAddrPlaceholder}
                    class="field w-64 font-mono"
                  />
                </div>
              {/if}
            </div>
          {/if}
        {:else}
          <!-- ANDROID PHONE INTERFACE -->
          {#if activeTab === "primary"}
            <!-- PC -> Phone: Scan QR via Camera / Image / Paste -->
            <div
              class="flex flex-col gap-3 rounded-xl border p-4"
              style="border-color: var(--border); background-color: var(--surface-subtle);"
            >
              <div class="flex flex-wrap gap-2">
                {#if !cameraActive}
                  <button
                    type="button"
                    disabled={busy}
                    onclick={startCameraScanner}
                    class="gpu-press inline-flex items-center gap-1.5 rounded-lg px-3.5 py-2 text-xs font-semibold disabled:opacity-45"
                    style="background-color: var(--accent); color: var(--accent-fg);"
                  >
                    <Camera size={14} />
                    <span>{t.syncStartCamera}</span>
                  </button>
                {:else}
                  <button
                    type="button"
                    onclick={stopCamera}
                    class="gpu-press inline-flex items-center gap-1.5 rounded-lg border px-3.5 py-2 text-xs font-semibold"
                    style="border-color: var(--danger); color: var(--danger); background-color: var(--surface);"
                  >
                    <XCircle size={14} />
                    <span>{t.syncStopCamera}</span>
                  </button>
                {/if}

                <button
                  type="button"
                  disabled={busy}
                  onclick={() => fileInputEl?.click()}
                  class="gpu-press inline-flex items-center gap-1.5 rounded-lg border px-3.5 py-2 text-xs font-medium"
                  style="border-color: var(--border); background-color: var(--surface);"
                >
                  <ImageUp size={14} />
                  <span>{t.syncScanImageBtn}</span>
                </button>

                <input
                  bind:this={fileInputEl}
                  type="file"
                  accept="image/*"
                  onchange={handleQrImageUpload}
                  class="hidden"
                />
              </div>

              {#if cameraActive}
                <div
                  transition:collapseReveal|local
                  class="relative overflow-hidden rounded-xl border bg-black"
                  style="border-color: var(--accent);"
                >
                  <!-- svelte-ignore a11y_media_has_caption -->
                  <video
                    bind:this={videoEl}
                    playsinline
                    muted
                    class="aspect-square w-full max-w-xs mx-auto object-cover"
                  ></video>
                  <div class="pointer-events-none absolute inset-4 overflow-hidden" aria-hidden="true">
                    <div
                      class="qr-scan-beam h-0.5 w-full rounded-full"
                      style="background-color: var(--accent); box-shadow: 0 0 8px var(--accent);"
                    ></div>
                  </div>
                  <canvas bind:this={canvasEl} class="hidden"></canvas>
                </div>
              {/if}

              {#if cameraError}
                <div transition:collapseReveal|local class="text-xs" style="color: var(--danger);">
                  {cameraError}
                </div>
              {/if}

              <div class="flex flex-wrap items-center gap-2 border-t pt-3" style="border-color: var(--border);">
                <input
                  type="text"
                  bind:value={qrPasteInput}
                  placeholder={t.syncPasteCodePlaceholder}
                  class="field min-w-44 flex-1 font-mono"
                />
                <button
                  type="button"
                  disabled={busy || !qrPasteInput.trim()}
                  onclick={() => connectWithQr(qrPasteInput)}
                  class="gpu-press rounded-lg border px-3 py-1.5 text-xs font-medium disabled:opacity-45"
                  style="border-color: var(--border); background-color: var(--surface);"
                >
                  {busy ? t.syncConnecting : t.syncConnectBtn}
                </button>
              </div>
            </div>
          {:else}
            <!-- Phone -> PC: Display 6-digit Pairing Code -->
            <div
              class="flex flex-col items-start gap-3 rounded-xl border p-4"
              style="border-color: var(--border); background-color: var(--surface-subtle);"
            >
              <button
                type="button"
                disabled={busy || isActiveFlow}
                onclick={startSender}
                class="gpu-press inline-flex items-center gap-2 rounded-lg px-4 py-2 text-xs font-semibold disabled:opacity-45"
                style="background-color: var(--accent); color: var(--accent-fg);"
              >
                <Wifi size={15} />
                <span>{t.syncStartPinBtn}</span>
              </button>
            </div>
          {/if}
        {/if}
      </div>
    {/key}
  {/if}

  {#if errorMsg}
    <div
      transition:collapseReveal|local
      class="rounded-lg border px-3 py-2 text-xs"
      style="border-color: var(--danger); color: var(--danger);"
    >
      {errorMsg}
    </div>
  {/if}
</section>

<style>
  .settings-card {
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
    padding: 1rem 1.15rem;
    border-radius: 1rem;
    border: 1px solid var(--border);
    background-color: var(--surface);
  }

  .section-title {
    font-size: 0.9rem;
    font-weight: 600;
    margin: 0;
  }

  .pill {
    padding: 0.4rem 0.75rem;
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
    color: var(--text);
    font-size: 0.75rem;
    font-weight: 500;
    cursor: pointer;
    transition:
      transform var(--dur-fast, 140ms) var(--ease-out, cubic-bezier(0.2, 0, 0, 1)),
      background-color var(--dur-fast, 140ms) var(--ease-out, cubic-bezier(0.2, 0, 0, 1)),
      border-color var(--dur-fast, 140ms) var(--ease-out, cubic-bezier(0.2, 0, 0, 1)),
      color var(--dur-fast, 140ms) var(--ease-out, cubic-bezier(0.2, 0, 0, 1));
  }

  .pill-active {
    border-color: var(--accent);
    background-color: var(--accent-soft);
    color: var(--accent);
  }

  .field {
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background-color: var(--surface);
    color: var(--text);
    padding: 0.4rem 0.65rem;
    font-size: 0.75rem;
    outline: none;
  }

  .field:focus {
    border-color: var(--accent);
  }
</style>
