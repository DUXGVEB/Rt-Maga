import type { DeviceInsets, FloatingButtonPosition, FloatingEdge } from "./types";

export interface FloatingControlRequest {
  id: string;
  position: FloatingButtonPosition;
  sizePx?: number;
}

export interface ResolvedFloatingControl {
  id: string;
  edge: FloatingEdge;
  topPx: number;
  offsetPx: number;
  sizePx: number;
}

export interface FloatingViewportBounds {
  width: number;
  height: number;
  topInsetPx: number;
  bottomInsetPx: number;
  leftInsetPx: number;
  rightInsetPx: number;
}

const DEFAULT_BUTTON_SIZE_PX = 42;
const MIN_GAP_PX = 10;

/**
 * Computes safe viewport bounds accounting for display cutouts, Android edge gesture strips,
 * mobile bottom navigation bar, and reader top toolbar.
 */
export function computeFloatingBounds(
  width: number,
  height: number,
  insets: DeviceInsets,
  opts: { hasBottomNav?: boolean; topToolbarHeightPx?: number } = {}
): FloatingViewportBounds {
  const topToolbar = opts.topToolbarHeightPx ?? 56;
  const bottomNav = opts.hasBottomNav ? 72 : 16;

  return {
    width: Math.max(240, width),
    height: Math.max(240, height),
    topInsetPx: Math.max(insets.cutoutTop + 12, topToolbar + 8),
    bottomInsetPx: Math.max(insets.cutoutBottom + bottomNav, insets.imeBottom + 16, bottomNav),
    leftInsetPx: Math.max(insets.cutoutLeft, insets.gestureLeft, 8),
    rightInsetPx: Math.max(insets.cutoutRight, insets.gestureRight, 8),
  };
}

/**
 * Resolves exact screen coordinates for active floating controls so they:
 * 1. Respect user-selected edge (`left` / `right`), vertical %, and horizontal offset.
 * 2. Stay strictly inside safe viewport bounds (never overlapping cutouts, nav bars, or gesture strips).
 * 3. Automatically separate vertically if two controls on the same edge would otherwise overlap.
 */
export function resolveFloatingPositions(
  requests: FloatingControlRequest[],
  bounds: FloatingViewportBounds
): ResolvedFloatingControl[] {
  const byEdge: Record<FloatingEdge, ResolvedFloatingControl[]> = {
    left: [],
    right: [],
  };

  for (const req of requests) {
    const sizePx = req.sizePx ?? DEFAULT_BUTTON_SIZE_PX;
    const edge: FloatingEdge = req.position.edge === "left" ? "left" : "right";
    const pct = Math.max(10, Math.min(90, req.position.verticalPercent)) / 100;
    const minTop = bounds.topInsetPx;
    const maxTop = Math.max(minTop, bounds.height - bounds.bottomInsetPx - sizePx);
    const desiredTop = Math.round(bounds.height * pct - sizePx / 2);
    const clampedTop = Math.max(minTop, Math.min(maxTop, desiredTop));
    const edgeInset = edge === "left" ? bounds.leftInsetPx : bounds.rightInsetPx;
    const rawOffset = Math.max(8, Math.min(48, req.position.horizontalOffsetPx));
    const offsetPx = Math.max(edgeInset + 4, rawOffset);

    byEdge[edge].push({
      id: req.id,
      edge,
      topPx: clampedTop,
      offsetPx,
      sizePx,
    });
  }

  // Collision-avoidance pass independently per edge.
  for (const edge of ["left", "right"] as const) {
    const list = byEdge[edge];
    if (list.length <= 1) continue;

    // Sort top-to-bottom by requested position.
    list.sort((a, b) => a.topPx - b.topPx);

    const minTop = bounds.topInsetPx;
    const maxBottom = bounds.height - bounds.bottomInsetPx;

    // Push overlapping neighbors downward first.
    for (let i = 1; i < list.length; i += 1) {
      const prev = list[i - 1];
      const curr = list[i];
      const minAllowed = prev.topPx + prev.sizePx + MIN_GAP_PX;
      if (curr.topPx < minAllowed) {
        curr.topPx = minAllowed;
      }
    }

    // If the bottom-most item overflowed the bottom safe bound, pull the stack upward cleanly.
    const last = list[list.length - 1];
    const overflow = last.topPx + last.sizePx - maxBottom;
    if (overflow > 0) {
      last.topPx = Math.max(minTop, last.topPx - overflow);
      for (let i = list.length - 2; i >= 0; i -= 1) {
        const next = list[i + 1];
        const curr = list[i];
        const maxAllowed = next.topPx - curr.sizePx - MIN_GAP_PX;
        if (curr.topPx > maxAllowed) {
          curr.topPx = Math.max(minTop, maxAllowed);
        }
      }
    }
  }

  return [...byEdge.left, ...byEdge.right];
}
