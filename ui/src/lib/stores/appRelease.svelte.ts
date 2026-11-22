import { api, formatAppError } from "../api";
import type { AppReleaseInfo, InstallStatus, UpdateDownloadProgress } from "../types";
import { settingsStore } from "./settings.svelte";
import { toasts } from "./toast.svelte";

const DISMISSED_KEY = "rtmaga.dismissedReleaseTag";

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

class AppReleaseStore {
  currentVersion = $state("1.1.0");
  release = $state<AppReleaseInfo | null>(null);
  checking = $state(false);
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
    if (info.updateAvailable && this.dismissedTag !== info.tagName) {
      this.promptOpen = true;
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

  async checkNow() {
    if (this.checking) return;
    this.checking = true;
    const t = settingsStore.t;
    try {
      const info = await api.checkAppRelease();
      this.release = info;
      this.currentVersion = info.currentVersion;
      if (info.updateAvailable) {
        this.dismissedTag = null;
        saveDismissedTag(null);
        this.installError = null;
        this.installStatus = null;
        this.progress = null;
        this.promptOpen = true;
      } else {
        toasts.push(t.releaseUpToDate(info.currentVersion), "info");
      }
    } catch {
      toasts.push(t.releaseCheckFailed, "error");
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
