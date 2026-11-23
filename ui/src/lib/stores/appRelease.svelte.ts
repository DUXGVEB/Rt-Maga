import { api, formatAppError } from "../api";
import type {
  AppError,
  AppReleaseInfo,
  InstallStatus,
  UpdateDownloadProgress,
} from "../types";
import { settingsStore } from "./settings.svelte";
import { toasts } from "./toast.svelte";

const DISMISSED_KEY = "rtmaga.dismissedReleaseTag";

export type ReleaseCheckStatus =
  | "idle"
  | "checking"
  | "up_to_date"
  | "update_available"
  | "server_unavailable"
  | "invalid_metadata"
  | "rate_limited"
  | "network_error";

function loadDismissedTag(): string | null {
  try {
    return window.localStorage.getItem(DISMISSED_KEY);
  } catch {
    return null;
  }
}

function saveDismissedTag(tag: string | null) {
  try {
    if (tag) {
      window.localStorage.setItem(DISMISSED_KEY, tag);
    } else {
      window.localStorage.removeItem(DISMISSED_KEY);
    }
  } catch {
    // Ignore storage unavailability.
  }
}

function classifyReleaseError(err: unknown): {
  status: ReleaseCheckStatus;
  message: string;
} {
  const t = settingsStore.t;
  const payload =
    err && typeof err === "object" && "kind" in err
      ? (err as AppError)
      : null;

  if (payload) {
    switch (payload.kind) {
      case "rate_limited":
        return { status: "rate_limited", message: t.releaseStatusRateLimited };
      case "http_error":
      case "timeout":
      case "access_restricted":
        return { status: "server_unavailable", message: t.releaseStatusServerUnavailable };
      case "api_failure":
      case "missing_metadata":
      case "corrupted_data":
        return { status: "invalid_metadata", message: t.releaseStatusInvalidMetadata };
      case "dns_failure":
      case "tls_failure":
      case "network":
        return { status: "network_error", message: t.releaseStatusNetworkError };
      default:
        break;
    }
  }

  const formatted = formatAppError(err, settingsStore.value.language);
  const raw = `${formatted.title} ${formatted.detail}`.toLowerCase();
  if (raw.includes("429") || raw.includes("rate limit")) {
    return { status: "rate_limited", message: t.releaseStatusRateLimited };
  }
  if (raw.includes("invalid") || raw.includes("metadata") || raw.includes("json")) {
    return { status: "invalid_metadata", message: t.releaseStatusInvalidMetadata };
  }
  if (raw.includes("503") || raw.includes("502") || raw.includes("500") || raw.includes("timeout")) {
    return { status: "server_unavailable", message: t.releaseStatusServerUnavailable };
  }
  return { status: "network_error", message: formatted.detail || t.releaseStatusNetworkError };
}

class AppReleaseStore {
  currentVersion = $state("");
  release = $state<AppReleaseInfo | null>(null);
  checking = $state(false);
  checkStatus = $state<ReleaseCheckStatus>("idle");
  statusMessage = $state<string | null>(null);
  lastCheckedAt = $state<number | null>(null);
  dismissedTag = $state<string | null>(loadDismissedTag());

  promptOpen = $state(false);
  installing = $state(false);
  progress = $state<UpdateDownloadProgress | null>(null);
  installStatus = $state<InstallStatus | null>(null);
  installError = $state<string | null>(null);

  private initialized = false;

  shouldShowBanner = $derived(
    Boolean(this.release?.updateAvailable && this.dismissedTag !== this.release.tagName)
  );

  private handleDetectedRelease(info: AppReleaseInfo) {
    this.release = info;
    this.currentVersion = info.currentVersion;
    if (info.updateAvailable) {
      this.checkStatus = "update_available";
      this.statusMessage = settingsStore.t.releaseAvailableBanner(info.latestVersion);
      if (this.dismissedTag !== info.tagName) {
        this.promptOpen = true;
      }
    }
  }

  async init() {
    if (this.initialized) return;
    this.initialized = true;

    try {
      this.currentVersion = await api.getAppVersion();
    } catch {
      // Standalone browser preview fallback.
    }

    try {
      await api.onAppReleaseAvailable((info) => {
        this.handleDetectedRelease(info);
      });
      await api.onUpdateDownloadProgress((p) => {
        this.progress = p;
      });
      const cached = await api.getCachedAppRelease();
      if (cached) {
        this.handleDetectedRelease(cached);
      }
    } catch {
      // Outside Tauri runtime.
    }
  }

  async checkNow(options?: { showToast?: boolean }): Promise<ReleaseCheckStatus> {
    if (this.checking) return this.checkStatus;
    const showToast = options?.showToast ?? true;
    this.checking = true;
    this.checkStatus = "checking";
    const t = settingsStore.t;
    this.statusMessage = t.releaseStatusChecking;

    try {
      const info = await api.checkAppRelease();
      this.release = info;
      this.currentVersion = info.currentVersion;
      this.lastCheckedAt = Date.now();

      if (info.updateAvailable) {
        this.dismissedTag = null;
        saveDismissedTag(null);
        this.installError = null;
        this.installStatus = null;
        this.progress = null;
        this.checkStatus = "update_available";
        this.statusMessage = t.releaseAvailableBanner(info.latestVersion);
        this.promptOpen = true;
        return "update_available";
      }

      const msg = t.releaseStatusUpToDate(info.currentVersion);
      this.checkStatus = "up_to_date";
      this.statusMessage = msg;
      if (showToast) {
        toasts.push(msg, "success", 3200);
      }
      return "up_to_date";
    } catch (err) {
      this.lastCheckedAt = Date.now();
      const classified = classifyReleaseError(err);
      this.checkStatus = classified.status;
      this.statusMessage = classified.message;
      if (showToast) {
        toasts.push(classified.message, "error", 4000);
      }
      return classified.status;
    } finally {
      this.checking = false;
    }
  }

  openPrompt() {
    if (!this.release?.updateAvailable) return;
    if (!this.installing) {
      this.installError = null;
      this.installStatus = null;
      this.progress = null;
    }
    this.promptOpen = true;
  }

  dismissPrompt() {
    if (this.installing) return;
    this.promptOpen = false;
    if (this.release) {
      this.dismissedTag = this.release.tagName;
      saveDismissedTag(this.release.tagName);
    }
  }

  async installNow() {
    if (!this.release || this.installing) return;
    this.installing = true;
    this.installError = null;
    this.installStatus = null;
    this.progress = {
      stage: "downloading",
      downloadedBytes: 0,
      totalBytes: this.release.downloadSize,
    };

    try {
      const outcome = await api.downloadAndInstallUpdate();
      this.installStatus = outcome.status;
      if (outcome.status !== "installing_and_restarting") {
        this.installing = false;
      }
    } catch (err) {
      const formatted = formatAppError(err, settingsStore.value.language);
      this.installError = formatted.detail || formatted.title;
      this.installing = false;
    }
  }

  async openReleasePage() {
    if (!this.release) return;
    try {
      await api.openReleaseUrl(this.release.releaseUrl);
    } catch {
      toasts.push(settingsStore.t.releaseOpenFailed, "error");
    }
  }
}

export const appReleaseStore = new AppReleaseStore();
