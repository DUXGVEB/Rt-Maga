import type { ActionReturn } from "svelte/action";
import type { TransitionConfig } from "svelte/transition";

export const MOTION_EASING = {
  snappyCss: "cubic-bezier(0.2, 0, 0, 1)",
} as const;

export const MOTION_DURATION = {
  press: 120,
  fast: 140,
  base: 175,
  overlay: 190,
  slow: 200,
  flipMax: 200,
  staggerStep: 18,
  staggerMaxTotal: 180,
  staggerMaxItems: 12,
  longListSkipThreshold: 36,
  flipSkipThreshold: 48,
} as const;

export function prefersReducedMotion(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return false;
  }
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * Exact solver for CSS `cubic-bezier(0.2, 0, 0, 1)` (snappy UI decelerate curve).
 * Uses 4 Newton-Raphson iterations — zero allocations, sub-1e-5 accuracy.
 */
export function easeSnappy(x: number): number {
  if (x <= 0) return 0;
  if (x >= 1) return 1;
  // P1 = (0.2, 0), P2 = (0, 1)
  // Bx(t) = 3*(1-t)^2*t*0.2 + 3*(1-t)*t^2*0 + t^3 = 0.6*t*(1-t)^2 + t^3
  // By(t) = 3*(1-t)^2*t*0   + 3*(1-t)*t^2*1 + t^3 = 3*t^2 - 2*t^3
  let t = x;
  for (let i = 0; i < 5; i += 1) {
    const inv = 1 - t;
    const bx = 0.6 * t * inv * inv + t * t * t - x;
    const dbx = 0.6 * (inv * inv - 2 * t * inv) + 3 * t * t;
    if (Math.abs(dbx) < 1e-6) break;
    t = Math.max(0, Math.min(1, t - bx / dbx));
  }
  return 3 * t * t - 2 * t * t * t;
}

/** Smooth decelerating curve (easeOutQuart) for UI entries and panels. */
export function easeOutQuart(t: number): number {
  const clamped = Math.max(0, Math.min(1, t));
  return 1 - Math.pow(1 - clamped, 4);
}

/** Gentle overshoot-free curve (easeOutQuint) for page/card reveals. */
export function easeOutQuint(t: number): number {
  const clamped = Math.max(0, Math.min(1, t));
  return 1 - Math.pow(1 - clamped, 5);
}

/**
 * Bounded FLIP duration (<= 200ms, 0ms under reduced motion or on long lists
 * so sorting large libraries never stalls mid-range Android WebViews).
 */
export function flipDuration(ms = 180, totalCount?: number): number {
  if (prefersReducedMotion()) return 0;
  if (totalCount !== undefined && totalCount > MOTION_DURATION.flipSkipThreshold) {
    return 0;
  }
  return Math.max(0, Math.min(ms, MOTION_DURATION.flipMax));
}

/**
 * Long-list guard: caps stagger count at first 12 visible items and skips stagger
 * completely when total list length exceeds the mid-range Android budget.
 */
export function shouldStaggerItem(index: number, totalCount: number): boolean {
  if (prefersReducedMotion()) return false;
  if (totalCount > MOTION_DURATION.longListSkipThreshold) return false;
  return index >= 0 && index < MOTION_DURATION.staggerMaxItems;
}

/** Capped stagger delay (<= 180ms total, 0ms under reduced motion or beyond cap). */
export function staggerDelay(
  index: number,
  stepMs: number = MOTION_DURATION.staggerStep,
  maxTotalMs: number = MOTION_DURATION.staggerMaxTotal,
  totalCount?: number
): number {
  if (prefersReducedMotion() || index <= 0) return 0;
  if (totalCount !== undefined && !shouldStaggerItem(index, totalCount)) return 0;
  if (index >= MOTION_DURATION.staggerMaxItems) return 0;
  return Math.min(index * stepMs, Math.min(maxTotalMs, MOTION_DURATION.staggerMaxTotal));
}

/** GPU-only opacity fade for modal/drawer backdrops. */
export function backdropFade(
  _node: Element,
  params: { duration?: number } = {}
): TransitionConfig {
  const duration = prefersReducedMotion() ? 0 : (params.duration ?? 150);
  return {
    duration,
    easing: easeSnappy,
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
  const duration = Math.min(200, Math.max(120, params.duration ?? 180));
  const y = params.y ?? 10;
  const start = params.start ?? 0.968;
  return {
    duration,
    easing: easeSnappy,
    css: (t, u) => {
      const scale = start + (1 - start) * t;
      const ty = u * y;
      return `opacity: ${t}; transform: translate3d(0, ${ty.toFixed(2)}px, 0) scale(${scale.toFixed(4)});`;
    },
  };
}

/**
 * Adaptive GPU-only modal/bottom-sheet transition:
 * slides smoothly up from the bottom edge on compact mobile, or scales in on desktop.
 */
export function adaptiveOverlay(
  _node: Element,
  params: { isMobile?: boolean; duration?: number } = {}
): TransitionConfig {
  if (prefersReducedMotion()) {
    return { duration: 0 };
  }
  const duration = Math.min(200, Math.max(120, params.duration ?? 185));
  if (params.isMobile) {
    const distance = 28;
    return {
      duration,
      easing: easeSnappy,
      css: (t, u) =>
        `opacity: ${Math.min(1, t * 1.25).toFixed(3)}; transform: translate3d(0, ${(u * distance).toFixed(2)}px, 0);`,
    };
  }
  return modalScale(_node, { duration, y: 10, start: 0.968 });
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
  const duration = Math.min(200, Math.max(120, params.duration ?? 190));
  const distance = params.distance ?? 28;
  return {
    duration,
    easing: easeSnappy,
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
  const duration = Math.min(200, Math.max(120, params.duration ?? 150));
  const y = params.y ?? 6;
  const start = params.start ?? 0.965;
  return {
    duration,
    easing: easeSnappy,
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
  const duration = Math.min(200, Math.max(120, params.duration ?? 160));
  const y = params.y ?? 6;
  return {
    duration,
    easing: easeSnappy,
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
  const duration = Math.min(200, Math.max(120, params.duration ?? 175));
  return {
    duration,
    easing: easeSnappy,
    css: (t, u) => {
      const scale = 0.965 + 0.035 * t;
      return `opacity: ${t}; transform: translate3d(0, ${(u * 12).toFixed(2)}px, 0) scale(${scale.toFixed(4)});`;
    },
  };
}

/** GPU-only crisp scale + opacity pop for badges, chips, icon swaps, and floating action controls. */
export function badgePop(
  _node: Element,
  params: { duration?: number; start?: number } = {}
): TransitionConfig {
  if (prefersReducedMotion()) {
    return { duration: 0 };
  }
  const duration = Math.min(200, Math.max(100, params.duration ?? 150));
  const start = Math.min(0.98, Math.max(0.5, params.start ?? 0.76));
  return {
    duration,
    easing: easeSnappy,
    css: (t) => {
      const scale = start + (1 - start) * t;
      return `opacity: ${t}; transform: scale(${scale.toFixed(4)});`;
    },
  };
}

/**
 * GPU-only reveal for accordions, inline filter drawers, diagnostics, and conditional form panels.
 * Uses pure transform + opacity (zero height layout thrashing) to keep Android WebView locked at 60/120fps.
 */
export function collapseReveal(
  _node: Element,
  params: { duration?: number; y?: number; startScale?: number } = {}
): TransitionConfig {
  if (prefersReducedMotion()) {
    return { duration: 0 };
  }
  const duration = Math.min(200, Math.max(120, params.duration ?? 165));
  const y = params.y ?? 6;
  const startScale = Math.min(1, Math.max(0.92, params.startScale ?? 0.985));
  return {
    duration,
    easing: easeSnappy,
    css: (t, u) => {
      const scale = startScale + (1 - startScale) * t;
      const ty = -u * y;
      return `opacity: ${t}; transform-origin: top center; transform: translate3d(0, ${ty.toFixed(2)}px, 0) scale(${scale.toFixed(4)});`;
    },
  };
}

/** GPU-only fade + slight scale-down for list/grid item removals paired with `animate:flip`. */
export function itemExit(
  _node: Element,
  params: { duration?: number; scale?: number; y?: number } = {}
): TransitionConfig {
  if (prefersReducedMotion()) {
    return { duration: 0 };
  }
  const duration = Math.min(180, Math.max(100, params.duration ?? 140));
  const scaleStart = Math.min(1, Math.max(0.88, params.scale ?? 0.95));
  const y = params.y ?? 4;
  return {
    duration,
    easing: easeSnappy,
    css: (t, u) => {
      const scale = scaleStart + (1 - scaleStart) * t;
      return `opacity: ${t}; transform: translate3d(0, ${(u * y).toFixed(2)}px, 0) scale(${scale.toFixed(4)});`;
    },
  };
}

/**
 * Svelte action for tactile pointerdown press feedback (`scale(0.97)`) with strict VRAM hygiene:
 * promotes `will-change: transform` ONLY during the active press/release and clears it immediately after.
 */
export function pressable(
  node: HTMLElement,
  enabled = true
): ActionReturn<boolean | undefined> {
  let active = enabled;
  let clearTimer: number | null = null;
  let pressed = false;

  const clearWillChange = () => {
    if (clearTimer !== null) {
      window.clearTimeout(clearTimer);
      clearTimer = null;
    }
    node.style.willChange = "";
  };

  const release = () => {
    if (!pressed) return;
    pressed = false;
    delete node.dataset.pressed;
    if (clearTimer !== null) window.clearTimeout(clearTimer);
    clearTimer = window.setTimeout(clearWillChange, MOTION_DURATION.fast + 40);
  };

  const onPointerDown = (e: PointerEvent) => {
    if (!active || prefersReducedMotion()) return;
    if (e.pointerType === "mouse" && e.button !== 0) return;
    if (node instanceof HTMLButtonElement && node.disabled) return;
    if (clearTimer !== null) {
      window.clearTimeout(clearTimer);
      clearTimer = null;
    }
    pressed = true;
    node.style.willChange = "transform";
    node.dataset.pressed = "true";
  };

  const onTransitionEnd = (e: TransitionEvent) => {
    if (e.target === node && e.propertyName === "transform" && !pressed) {
      clearWillChange();
    }
  };

  node.addEventListener("pointerdown", onPointerDown, { passive: true });
  node.addEventListener("pointerup", release, { passive: true });
  node.addEventListener("pointercancel", release, { passive: true });
  node.addEventListener("pointerleave", release, { passive: true });
  node.addEventListener("transitionend", onTransitionEnd);

  return {
    update(nextEnabled?: boolean) {
      active = nextEnabled ?? true;
      if (!active) {
        release();
        clearWillChange();
      }
    },
    destroy() {
      release();
      clearWillChange();
      node.removeEventListener("pointerdown", onPointerDown);
      node.removeEventListener("pointerup", release);
      node.removeEventListener("pointercancel", release);
      node.removeEventListener("pointerleave", release);
      node.removeEventListener("transitionend", onTransitionEnd);
    },
  };
}

/**
 * Crisp WAAPI highlight pulse (compositor-only `transform` + `opacity`) for settled cards.
 * Returns a cancel callback for strict `onDestroy` cleanup.
 */
export function pulseHighlight(node: HTMLElement | null): () => void {
  if (!node || prefersReducedMotion() || typeof node.animate !== "function") {
    return () => {};
  }
  const anim = node.animate(
    [
      { transform: "scale(1)", opacity: 1 },
      { transform: "scale(1.032)", opacity: 1, offset: 0.36 },
      { transform: "scale(1)", opacity: 1 },
    ],
    {
      duration: 280,
      easing: MOTION_EASING.snappyCss,
      fill: "none",
    }
  );
  return () => {
    try {
      anim.cancel();
    } catch {
      // Ignore already finished/cancelled animations.
    }
  };
}

/**
 * Rapid-click/tap guard for overlays (modals, drawers, popovers).
 * Prevents a second close/open action mid-transition from leaving orphaned backdrops.
 */
export function createOverlayGuard(cooldownMs = 170) {
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
