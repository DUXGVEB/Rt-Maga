import { api } from "../api";
import type { AppReleaseInfo } from "../types";
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
  private initialized = false;

  shouldShowBanner = $derived(
    Boolean(this.release?.updateAvailable && this.dismissedTag !== this.release.tagName)
  );

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
        this.release = info;
      });
      const cached = await api.getCachedAppRelease();
      if (cached) {
        this.release = cached;
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
        toasts.push(t.releaseAvailableBanner(info.latestVersion), "success", 5000);
      } else {
        toasts.push(t.releaseUpToDate(info.currentVersion), "info");
      }
    } catch {
      toasts.push(t.releaseCheckFailed, "error");
    } finally {
      this.checking = false;
    }
  }

  dismiss() {
    if (!this.release) return;
    this.dismissedTag = this.release.tagName;
    saveDismissedTag(this.release.tagName);
  }

  async openDownload(preferInstaller = true) {
    if (!this.release) return;
    const target =
      preferInstaller && this.release.downloadUrl
        ? this.release.downloadUrl
        : this.release.releaseUrl;
    try {
      await api.openReleaseUrl(target);
    } catch {
      toasts.push(settingsStore.t.releaseOpenFailed, "error");
    }
  }
}

export const appReleaseStore = new AppReleaseStore();
