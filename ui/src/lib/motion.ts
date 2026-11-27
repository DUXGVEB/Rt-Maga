import { cubicOut } from "svelte/easing";
import type { TransitionConfig } from "svelte/transition";

export const MOTION_DURATION = {
  fast: 150,
  base: 210,
  slow: 280,
  flipMax: 240,
  staggerMaxTotal: 260,
} as const;

export function prefersReducedMotion(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return false;
  }
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** Smooth decelerating curve (easeOutQuart) for UI entries and panels. */
export function easeOutQuart(t: number): number {
  return 1 - Math.pow(1 - t, 4);
}

/** Gentle overshoot-free spring-like settle curve for cards, popovers, and modals. */
export function easeOutQuint(t: number): number {
  return 1 - Math.pow(1 - t, 5);
}

/** Bounded FLIP duration (<= 250ms, 0ms under reduced motion). */
export function flipDuration(ms = 220): number {
  if (prefersReducedMotion()) return 0;
  return Math.max(0, Math.min(ms, MOTION_DURATION.flipMax));
}

/** Capped stagger delay (<= 300ms total, 0ms under reduced motion). */
export function staggerDelay(
  index: number,
  stepMs = 22,
  maxTotalMs: number = MOTION_DURATION.staggerMaxTotal
): number {
  if (prefersReducedMotion() || index <= 0) return 0;
  return Math.min(index * stepMs, Math.min(maxTotalMs, 300));
}

/** GPU-only opacity fade for modal/drawer backdrops. */
export function backdropFade(
  _node: Element,
  params: { duration?: number } = {}
): TransitionConfig {
  const duration = prefersReducedMotion() ? 0 : (params.duration ?? 170);
  return {
    duration,
    easing: cubicOut,
    css: (t) => `opacity: ${t};`,
  };
}

/** GPU-only scale + translate3d + opacity transition for centered dialogs/modals. */
export function modalScale(
  _node: Element,
  params: { duration?: number; y?: number; start?: number } = {}
): TransitionConfig {
  if (prefersReducedMotion()) {
    return { duration: 0 };
  }
  const duration = params.duration ?? 210;
  const y = params.y ?? 10;
  const start = params.start ?? 0.965;
  return {
    duration,
    easing: easeOutQuart,
    css: (t, u) => {
      const scale = start + (1 - start) * t;
      const ty = u * y;
      return `opacity: ${t}; transform: translate3d(0, ${ty.toFixed(2)}px, 0) scale(${scale.toFixed(4)});`;
    },
  };
}

/** GPU-only translate3d + opacity slide for side drawers and mobile sheets. */
export function drawerSlide(
  _node: Element,
  params: { side?: "right" | "left" | "bottom" | "top"; duration?: number; distance?: number } = {}
): TransitionConfig {
  if (prefersReducedMotion()) {
    return { duration: 0 };
  }
  const side = params.side ?? "right";
  const duration = params.duration ?? 230;
  const distance = params.distance ?? 28;
  return {
    duration,
    easing: easeOutQuart,
    css: (t, u) => {
      const offset = u * distance;
      let tx = 0;
      let ty = 0;
      if (side === "right") tx = offset;
      else if (side === "left") tx = -offset;
      else if (side === "bottom") ty = offset;
      else ty = -offset;
      return `opacity: ${Math.min(1, t * 1.25).toFixed(3)}; transform: translate3d(${tx.toFixed(2)}px, ${ty.toFixed(2)}px, 0);`;
    },
  };
}

/** GPU-only popover reveal (subtle vertical shift + scale + opacity). */
export function popoverPop(
  _node: Element,
  params: { duration?: number; y?: number; start?: number } = {}
): TransitionConfig {
  if (prefersReducedMotion()) {
    return { duration: 0 };
  }
  const duration = params.duration ?? 165;
  const y = params.y ?? 6;
  const start = params.start ?? 0.96;
  return {
    duration,
    easing: easeOutQuart,
    css: (t, u) => {
      const scale = start + (1 - start) * t;
      const ty = u * y;
      return `opacity: ${t}; transform: translate3d(0, ${ty.toFixed(2)}px, 0) scale(${scale.toFixed(4)});`;
    },
  };
}

/** Crisp GPU-only entrance for page/tab transitions (in-only to avoid rapid-nav outro locks). */
export function viewReveal(
  _node: Element,
  params: { duration?: number; y?: number } = {}
): TransitionConfig {
  if (prefersReducedMotion()) {
    return { duration: 0 };
  }
  const duration = params.duration ?? 175;
  const y = params.y ?? 6;
  return {
    duration,
    easing: easeOutQuint,
    css: (t, u) => `opacity: ${t}; transform: translate3d(0, ${(u * y).toFixed(2)}px, 0);`,
  };
}

/** GPU-only slide + scale entrance/exit for toast notifications. */
export function toastSlide(
  _node: Element,
  params: { duration?: number } = {}
): TransitionConfig {
  if (prefersReducedMotion()) {
    return { duration: 0 };
  }
  const duration = params.duration ?? 195;
  return {
    duration,
    easing: easeOutQuart,
    css: (t, u) => {
      const scale = 0.96 + 0.04 * t;
      return `opacity: ${t}; transform: translate3d(0, ${(u * 12).toFixed(2)}px, 0) scale(${scale.toFixed(4)});`;
    },
  };
}

/**
 * Rapid-click/tap guard for overlays (modals, drawers, popovers).
 * Prevents a second close/open action mid-transition from leaving orphaned backdrops.
 */
export function createOverlayGuard(cooldownMs = 180) {
  let lockedUntil = 0;
  return {
    canAct(): boolean {
      return performance.now() >= lockedUntil;
    },
    lock(ms = cooldownMs): void {
      lockedUntil = performance.now() + (prefersReducedMotion() ? 0 : ms);
    },
    reset(): void {
      lockedUntil = 0;
    },
  };
}
