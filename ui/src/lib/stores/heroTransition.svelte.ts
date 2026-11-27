import { prefersReducedMotion } from "../motion";

export interface HeroFlightRect {
  top: number;
  left: number;
  width: number;
  height: number;
  borderRadius: string;
}

export interface HeroFlightState {
  id: number;
  mangaId: number;
  coverToken: string;
  from: HeroFlightRect;
  phase: "pending" | "flying" | "done";
}

const WATCHDOG_MS = 700;

class HeroTransitionStore {
  active = $state<HeroFlightState | null>(null);
  private seq = 0;
  private watchdog: number | null = null;

  private clearWatchdog() {
    if (this.watchdog !== null) {
      window.clearTimeout(this.watchdog);
      this.watchdog = null;
    }
  }

  captureFromElement(
    el: HTMLElement | null,
    coverToken: string | null | undefined,
    mangaId: number
  ): void {
    if (
      typeof window === "undefined" ||
      !el ||
      !coverToken ||
      prefersReducedMotion()
    ) {
      this.clear();
      return;
    }

    const rect = el.getBoundingClientRect();
    if (rect.width < 8 || rect.height < 8) {
      this.clear();
      return;
    }

    const computed = window.getComputedStyle(el);
    let borderRadius = computed.borderRadius;
    if (!borderRadius || borderRadius === "0px") {
      const parentRadius = el.parentElement
        ? window.getComputedStyle(el.parentElement).borderRadius
        : "";
      borderRadius = parentRadius && parentRadius !== "0px" ? parentRadius : "12px";
    }

    const flightId = ++this.seq;
    this.clearWatchdog();
    this.active = {
      id: flightId,
      mangaId,
      coverToken,
      from: {
        top: rect.top,
        left: rect.left,
        width: rect.width,
        height: rect.height,
        borderRadius,
      },
      phase: "pending",
    };

    this.watchdog = window.setTimeout(() => {
      if (this.active?.id === flightId) {
        this.clear();
      }
    }, WATCHDOG_MS);
  }

  markFlying(flightId: number): void {
    if (this.active && this.active.id === flightId && this.active.phase === "pending") {
      this.active = { ...this.active, phase: "flying" };
    }
  }

  finish(flightId: number): void {
    if (this.active?.id === flightId) {
      this.clear();
    }
  }

  clear(): void {
    this.clearWatchdog();
    this.active = null;
  }

  isTargetHidden(mangaId: number): boolean {
    return (
      this.active !== null &&
      this.active.mangaId === mangaId &&
      this.active.phase !== "done"
    );
  }
}

export const heroTransitionStore = new HeroTransitionStore();
