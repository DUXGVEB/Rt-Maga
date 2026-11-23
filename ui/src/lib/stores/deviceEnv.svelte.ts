import { api } from "../api";
import type { DeviceEnvInfo, OrientationLockMode } from "../types";

const DEFAULT_ENV: DeviceEnvInfo = {
  platform: "windows",
  sdkInt: 0,
  manufacturer: "",
  model: "",
  skin: "desktop",
  realDeviceName: "RtMaga Device",
  orientation: "landscape",
  orientationLock: "auto",
  viewportWidthDp: 1280,
  viewportHeightDp: 800,
  immersive: false,
  insets: {
    cutoutTop: 0,
    cutoutBottom: 0,
    cutoutLeft: 0,
    cutoutRight: 0,
    gestureLeft: 0,
    gestureRight: 0,
    imeBottom: 0,
  },
};

class DeviceEnvStore {
  info = $state<DeviceEnvInfo>(structuredClone(DEFAULT_ENV));
  viewportWidth = $state(typeof window !== "undefined" ? window.innerWidth : 1280);
  viewportHeight = $state(typeof window !== "undefined" ? window.innerHeight : 800);
  lastOrientationChangeAt = $state(0);

  isAndroid = $derived(this.info.platform === "android");
  isMobile = $derived(this.info.platform === "android" || this.viewportWidth < 768);
  isLandscape = $derived(
    this.info.orientation === "landscape" || this.viewportWidth > this.viewportHeight
  );

  private initialized = false;

  async init() {
    if (this.initialized) return;
    this.initialized = true;

    const updateViewport = () => {
      this.viewportWidth = window.innerWidth;
      this.viewportHeight = window.innerHeight;
    };
    window.addEventListener("resize", updateViewport);
    window.addEventListener("orientationchange", () => {
      this.lastOrientationChangeAt = Date.now();
      updateViewport();
      window.setTimeout(updateViewport, 160);
      window.setTimeout(updateViewport, 360);
    });

    window.addEventListener("rtmaga:env-changed", (e) => {
      const detail = (e as CustomEvent<Partial<DeviceEnvInfo>>).detail;
      if (detail) {
        this.info = {
          ...this.info,
          ...detail,
          insets: {
            ...this.info.insets,
            ...(detail.insets ?? {}),
          },
        };
      }
      updateViewport();
    });

    window.addEventListener("rtmaga:orientation-changed", (e) => {
      const detail = (e as CustomEvent<{ orientation?: string; width?: number; height?: number }>).detail;
      this.lastOrientationChangeAt = Date.now();
      if (detail?.orientation) {
        this.info = { ...this.info, orientation: detail.orientation };
      }
      updateViewport();
      // Re-assert viewport metrics after HyperOS/MIUI layout settles.
      window.setTimeout(updateViewport, 150);
      window.setTimeout(updateViewport, 360);
    });

    try {
      const env = await api.getDeviceEnv();
      this.info = {
        ...DEFAULT_ENV,
        ...env,
        insets: {
          ...DEFAULT_ENV.insets,
          ...(env.insets ?? {}),
        },
      };
      if (env.platform === "android") {
        void api.setImmersive(true).catch(() => {});
      }
    } catch {
      // Browser preview fallback.
      const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
      if (/android/i.test(ua)) {
        this.info = { ...this.info, platform: "android", skin: "aosp" };
      }
    }
  }

  async setOrientationLock(mode: OrientationLockMode): Promise<boolean> {
    this.info = { ...this.info, orientationLock: mode };
    try {
      const res = await api.setOrientationLock(mode);
      return res.ok;
    } catch {
      return false;
    }
  }

  async reassertImmersive() {
    if (!this.isAndroid) return;
    try {
      await api.setImmersive(true);
    } catch {
      // Ignore outside Android runtime.
    }
  }
}

export const deviceEnv = new DeviceEnvStore();
