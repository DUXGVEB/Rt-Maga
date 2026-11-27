export interface PageGroupItem<T> {
  token: T;
  pageIdx: number;
}

export interface PageGroup<T> {
  groupIdx: number;
  items: PageGroupItem<T>[];
}

export interface PageDimensionSample {
  index: number;
  width: number;
  height: number;
}

export interface WebtoonDetectionResult {
  decided: boolean;
  isWebtoon: boolean;
  detectedLayout: "manga" | "webtoon" | null;
  qualifyingCount: number;
  tallCount: number;
  medianRatio: number;
}

export type PageLoadMode = "high" | "preload" | "idle" | "unload";

/**
 * Partitions an ordered chapter image list into contiguous groups of `pagesPerGroup` (clamped 1..8).
 * Any final group with fewer images contains only its real pages — never dummy slots or duplicates.
 */
export function partitionPages<T>(images: T[], pagesPerGroup: number): PageGroup<T>[] {
  const size = Math.max(1, Math.min(8, Math.floor(pagesPerGroup) || 1));
  const groups: PageGroup<T>[] = [];
  for (let i = 0; i < images.length; i += size) {
    const items: PageGroupItem<T>[] = [];
    const end = Math.min(i + size, images.length);
    for (let j = i; j < end; j += 1) {
      items.push({ token: images[j], pageIdx: j });
    }
    groups.push({ groupIdx: groups.length, items });
  }
  return groups;
}

/**
 * Computes CSS `brightness()` multiplier for focus dimming:
 * - Focused page/group or dimming disabled or level 0 -> `1.0` (normal brightness).
 * - Unfocused with level 1..100 -> smoothly scales down to `0.15` at 100% so content stays faintly visible.
 */
export function focusBrightness(
  isFocused: boolean,
  dimEnabled: boolean,
  dimLevel: number
): number {
  if (!dimEnabled || isFocused || dimLevel <= 0) return 1;
  const clamped = Math.max(0, Math.min(100, dimLevel));
  return Number((1 - (clamped / 100) * 0.85).toFixed(3));
}

/**
 * Passively classifies whether a chapter is a Webtoon / Long-Strip vs. standard Manga
 * using natural image dimensions (zero extra network requests), synchronized with the
 * strict multi-signal Rust classifier.
 *
 * Safeguards against Manga-as-Webtoon false positives:
 * - Ignores tiny icons/banners (`width < 200` or `height < 300`).
 * - Ignores landscape double-page spreads (`width > height * 1.15`).
 * - Requires at least 2 qualifying images before deciding.
 * - NEVER flips on a single tall credit/promo banner (`tallStripCount < 2` -> never Webtoon).
 * - Requires EITHER median aspect ratio >= 1.85 with >= 2 tall strips, OR majority tall strips
 *   with matching width continuity (±8%), OR >= 2 consecutive tall strips sharing width (±8%)
 *   when standard manga-ratio pages (`1.15..1.68`) are not the clear majority.
 */
export function detectWebtoonFromDimensions(
  samples: PageDimensionSample[]
): WebtoonDetectionResult {
  const ordered = [...samples].sort((a, b) => a.index - b.index);
  const qualifying: { index: number; width: number; ratio: number }[] = [];

  for (const s of ordered) {
    if (s.width < 200 || s.height < 300) continue;
    if (s.width > s.height * 1.15) continue;
    qualifying.push({ index: s.index, width: s.width, ratio: s.height / s.width });
  }

  if (qualifying.length < 2) {
    return {
      decided: false,
      isWebtoon: false,
      detectedLayout: null,
      qualifyingCount: qualifying.length,
      tallCount: 0,
      medianRatio: 0,
    };
  }

  const sorted = qualifying.map((q) => q.ratio).sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  const medianRatio =
    sorted.length % 2 === 1 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;

  const mangaRangeCount = qualifying.filter((q) => q.ratio >= 1.15 && q.ratio <= 1.68).length;
  const tallStrips = qualifying.filter((q) => q.ratio >= 1.85);
  const tallCount = tallStrips.length;

  const widthsMatch = (w1: number, w2: number) =>
    Math.abs(w1 - w2) <= Math.max(w1, w2) * 0.08;

  // Require at least 2 consecutive tall strips (`>= 1.85`) sharing width within ±8%.
  let hasConsecutiveTallStrips = false;
  for (let i = 1; i < qualifying.length; i += 1) {
    const prev = qualifying[i - 1];
    const curr = qualifying[i];
    if (prev.ratio >= 1.85 && curr.ratio >= 1.85 && widthsMatch(prev.width, curr.width)) {
      hasConsecutiveTallStrips = true;
      break;
    }
  }

  // Require at least 2 tall strips sharing width continuity for majority-tall classification.
  const hasTallPairWidthContinuity = tallStrips.some((a, i) =>
    tallStrips.some((b, j) => i !== j && widthsMatch(a.width, b.width))
  );

  const mangaFraction = mangaRangeCount / qualifying.length;
  const isMajorityTall = tallCount * 2 > qualifying.length;

  const isWebtoon =
    tallCount >= 2 &&
    ((medianRatio >= 1.85 && (hasTallPairWidthContinuity || isMajorityTall)) ||
      (hasConsecutiveTallStrips && mangaFraction < 0.6) ||
      (isMajorityTall && hasTallPairWidthContinuity));

  const isManga =
    !isWebtoon &&
    (mangaFraction >= 0.5 || medianRatio <= 1.68 || (qualifying.length >= 3 && tallCount < 2));

  const detectedLayout: "manga" | "webtoon" | null = isWebtoon
    ? "webtoon"
    : isManga
      ? "manga"
      : null;

  return {
    decided: detectedLayout !== null,
    isWebtoon,
    detectedLayout,
    qualifyingCount: qualifying.length,
    tallCount,
    medianRatio: Number(medianRatio.toFixed(2)),
  };
}

/**
 * Controlled image preload window (5..8 pages around the active reading position).
 * - Prioritizes current page and immediate next page (`high`).
 * - Preloads up to `windowSize` pages ahead and 2 pages behind (`preload`).
 * - Unloads far-away images (`unload`) to keep decoded bitmap RAM bounded on mobile devices.
 */
export function computePageLoadMode(
  pageIdx: number,
  focusedIdx: number,
  windowSize: number,
  isActiveChapter: boolean
): PageLoadMode {
  if (!isActiveChapter) {
    return pageIdx <= 3 ? "preload" : "idle";
  }

  const clampedWindow = Math.max(5, Math.min(8, Math.round(windowSize) || 6));
  const delta = pageIdx - focusedIdx;

  if (delta >= 0 && delta <= 1) {
    return "high";
  }
  if (delta >= -2 && delta <= clampedWindow) {
    return "preload";
  }
  if (delta < -12 || delta > clampedWindow + 12) {
    return "unload";
  }
  return "idle";
}

export interface RectBox {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface ZoomTransform {
  scale: number;
  tx: number;
  ty: number;
}

/**
 * Clamps horizontal and vertical pan offsets so a zoomed content box (`contentRect` in unscaled
 * local coordinates relative to the transformed element's `(originLeft, originTop)`) stays
 * anchored cleanly inside a viewport (`vpWidth x vpHeight`) without exposing unnecessary empty
 * margins or pushing the image off-screen.
 */
export function clampZoomPan(
  tx: number,
  ty: number,
  scale: number,
  vpWidth: number,
  vpHeight: number,
  contentRect: RectBox,
  originLeft = 0,
  originTop = 0
): { tx: number; ty: number } {
  if (scale <= 1.001) {
    return { tx: 0, ty: 0 };
  }

  const scaledW = contentRect.width * scale;
  const scaledH = contentRect.height * scale;
  const contentRight = contentRect.left + contentRect.width;
  const contentBottom = contentRect.top + contentRect.height;

  let clampedTx: number;
  if (scaledW <= vpWidth) {
    // Center the scaled content horizontally inside the viewport.
    clampedTx = (vpWidth - scaledW) / 2 - originLeft - contentRect.left * scale;
  } else {
    const minTx = vpWidth - originLeft - contentRight * scale;
    const maxTx = -originLeft - contentRect.left * scale;
    clampedTx = Math.max(minTx, Math.min(maxTx, tx));
  }

  let clampedTy: number;
  if (scaledH <= vpHeight) {
    clampedTy = (vpHeight - scaledH) / 2 - originTop - contentRect.top * scale;
  } else {
    const minTy = vpHeight - originTop - contentBottom * scale;
    const maxTy = -originTop - contentRect.top * scale;
    clampedTy = Math.max(minTy, Math.min(maxTy, ty));
  }

  return {
    tx: Number(clampedTx.toFixed(2)),
    ty: Number(clampedTy.toFixed(2)),
  };
}

/**
 * Computes the exact `(scale, tx, ty)` transform (with `transform-origin: 0 0`) that keeps the
 * pinched/anchored image pixel directly under `(pointerX, pointerY)` (viewport-relative) while
 * respecting the transformed container's unscaled `(originLeft, originTop)` and viewport bounds.
 */
export function computeAnchoredZoom(
  pointerX: number,
  pointerY: number,
  current: ZoomTransform,
  targetScale: number,
  vpWidth: number,
  vpHeight: number,
  contentRect: RectBox,
  originLeft = 0,
  originTop = 0
): ZoomTransform {
  if (targetScale <= 1.001) {
    return { scale: 1, tx: 0, ty: 0 };
  }

  const safeCurScale = Math.max(0.5, current.scale);
  // Recover local unscaled point under pointer before scaling.
  const lx = (pointerX - originLeft - current.tx) / safeCurScale;
  const ly = (pointerY - originTop - current.ty) / safeCurScale;

  const rawTx = pointerX - originLeft - lx * targetScale;
  const rawTy = pointerY - originTop - ly * targetScale;
  const clamped = clampZoomPan(
    rawTx,
    rawTy,
    targetScale,
    vpWidth,
    vpHeight,
    contentRect,
    originLeft,
    originTop
  );

  return {
    scale: targetScale,
    tx: clamped.tx,
    ty: clamped.ty,
  };
}

/**
 * Computes the exact `(scale, tx, ty)` transform for double-tap / double-click zoom that
 * focuses/centers the tapped content point `(pointerX, pointerY)` directly into the middle of the
 * viewport `(vpWidth / 2, vpHeight / 2)`, clamped cleanly to content boundaries.
 */
export function computeFocusedZoom(
  pointerX: number,
  pointerY: number,
  current: ZoomTransform,
  targetScale: number,
  vpWidth: number,
  vpHeight: number,
  contentRect: RectBox,
  originLeft = 0,
  originTop = 0
): ZoomTransform {
  if (targetScale <= 1.001) {
    return { scale: 1, tx: 0, ty: 0 };
  }

  const safeCurScale = Math.max(0.5, current.scale);
  const lx = (pointerX - originLeft - current.tx) / safeCurScale;
  const ly = (pointerY - originTop - current.ty) / safeCurScale;

  const rawTx = vpWidth / 2 - originLeft - lx * targetScale;
  const rawTy = vpHeight / 2 - originTop - ly * targetScale;
  const clamped = clampZoomPan(
    rawTx,
    rawTy,
    targetScale,
    vpWidth,
    vpHeight,
    contentRect,
    originLeft,
    originTop
  );

  return {
    scale: targetScale,
    tx: clamped.tx,
    ty: clamped.ty,
  };
}

/**
 * Maps user navigation speed (`1..=10`) to horizontal step cooldown in milliseconds so mouse
 * wheel notches and held arrow keys never double-skip images while staying responsive.
 */
export function computeNavStepCooldownMs(speed: number): number {
  const s = Math.max(1, Math.min(10, Math.round(speed) || 5));
  return Math.round(320 - (s - 1) * 26);
}

/**
 * Maps user navigation speed (`1..=10`) to vertical scroll step multiplier (`0.55x..1.85x`).
 */
export function computeVerticalSpeedMultiplier(speed: number): number {
  const s = Math.max(1, Math.min(10, Math.round(speed) || 5));
  return Number((0.55 + (s - 1) * 0.145).toFixed(3));
}

export interface HorizontalTrackMetrics {
  slotWidth: number;
  gapPx: number;
  stride: number;
}

/**
 * Computes slide slot width, inter-slide gap, and stride for the Horizontal RTL center-focus
 * "peek" layout so the active page fits cleanly in the center while adjacent Previous (right)
 * and Next (left) slides remain visibly peeking across desktop, mobile portrait, and landscape.
 */
export function computeHorizontalTrackMetrics(
  vpWidth: number,
  vpHeight: number,
  pagesPerGroup: number,
  configuredGapPx = 16
): HorizontalTrackMetrics {
  const safeW = Math.max(240, vpWidth);
  const safeH = Math.max(200, vpHeight);
  const count = Math.max(1, Math.min(8, Math.floor(pagesPerGroup) || 1));
  const maxFraction = count > 1 ? 0.84 : safeW < 640 ? 0.8 : 0.78;
  const idealByHeight = safeH * 0.75 * count;
  const minSlot = Math.min(safeW * 0.78, count > 1 ? 300 : 220);
  const slotWidth = Math.round(Math.max(minSlot, Math.min(safeW * maxFraction, idealByHeight)));
  const gapPx = Math.max(10, Math.min(48, Math.round(configuredGapPx || 16)));
  return {
    slotWidth,
    gapPx,
    stride: slotWidth + gapPx,
  };
}

/**
 * Returns the screen-space center X offset (in px relative to viewport center) of `slideIdx`
 * inside a Horizontal center-focus track for either `"rtl"` or `"ltr"` reading direction:
 * - `slideIdx === activeSlideIdx` -> `0` (dead center)
 * - In `"rtl"`: Previous pages sit RIGHT (`+`), Next pages & End Card sit LEFT (`-`).
 * - In `"ltr"`: Previous pages sit LEFT (`-`), Next pages & End Card sit RIGHT (`+`).
 */
export function computeHorizontalSlideOffset(
  slideIdx: number,
  activeSlideIdx: number,
  stride: number,
  direction: "ltr" | "rtl",
  dragOffsetPx = 0
): number {
  const dirSign = direction === "rtl" ? 1 : -1;
  return dirSign * (activeSlideIdx - slideIdx) * stride + dragOffsetPx;
}

/**
 * Shorthand for strict Right-to-Left (RTL) horizontal slide offset.
 */
export function computeHorizontalRtlSlideOffset(
  slideIdx: number,
  activeSlideIdx: number,
  stride: number,
  dragOffsetPx = 0
): number {
  return computeHorizontalSlideOffset(slideIdx, activeSlideIdx, stride, "rtl", dragOffsetPx);
}

/**
 * Computes the horizontal `translateX` (in px) for a seamless 0px-gap edge-to-edge track
 * so that the slide occupying `[activeSlideLeft, activeSlideLeft + activeSlideWidth]` locks
 * dead-center inside a viewport of width `vpWidth` while tracking live `dragOffsetPx`.
 */
export function computeSeamlessTrackTranslateX(
  vpWidth: number,
  activeSlideLeft: number,
  activeSlideWidth: number,
  dragOffsetPx = 0
): number {
  const activeCenter = activeSlideLeft + activeSlideWidth / 2;
  return Math.round(vpWidth / 2 - activeCenter + dragOffsetPx);
}

export type NormalizedBbox = [number, number, number, number];

export interface BubbleZoomResult extends ZoomTransform {
  bubbleCenterX: number;
  bubbleCenterY: number;
  bubbleRect: RectBox;
}

/**
 * Maps a `[0..1000]` normalized speech-bubble bbox `[x1, y1, x2, y2]` onto rendered
 * image pixel coordinates inside the viewport.
 */
export function mapNormalizedBboxToRect(bbox: NormalizedBbox, imageRect: RectBox): RectBox {
  const x1 = Math.max(0, Math.min(1000, bbox[0]));
  const y1 = Math.max(0, Math.min(1000, bbox[1]));
  const x2 = Math.max(x1, Math.min(1000, bbox[2]));
  const y2 = Math.max(y1, Math.min(1000, bbox[3]));
  return {
    left: Number((imageRect.left + (x1 * imageRect.width) / 1000).toFixed(2)),
    top: Number((imageRect.top + (y1 * imageRect.height) / 1000).toFixed(2)),
    width: Math.max(1, Number((((x2 - x1) * imageRect.width) / 1000).toFixed(2))),
    height: Math.max(1, Number((((y2 - y1) * imageRect.height) / 1000).toFixed(2))),
  };
}

/**
 * Computes the exact `(scale, tx, ty)` zoom transform that smoothly magnifies and centers
 * a tapped speech bubble (`bbox` in `[0..1000]`) inside the viewport with comfortable padding,
 * clamped strictly to the page content bounds.
 */
export function computeBubbleZoom(
  bbox: NormalizedBbox,
  unscaledImageRect: RectBox,
  vpWidth: number,
  vpHeight: number,
  contentRect: RectBox,
  originLeft = 0,
  originTop = 0,
  opts: { minScale?: number; maxScale?: number; fillRatioX?: number; fillRatioY?: number } = {}
): BubbleZoomResult {
  const minScale = opts.minScale ?? 1.6;
  const maxScale = opts.maxScale ?? 3.2;
  const fillX = opts.fillRatioX ?? 0.64;
  const fillY = opts.fillRatioY ?? 0.54;

  const bubbleRect = mapNormalizedBboxToRect(bbox, unscaledImageRect);
  const bubbleCenterX = bubbleRect.left + bubbleRect.width / 2;
  const bubbleCenterY = bubbleRect.top + bubbleRect.height / 2;

  const fitScaleX = (vpWidth * fillX) / Math.max(16, bubbleRect.width);
  const fitScaleY = (vpHeight * fillY) / Math.max(16, bubbleRect.height);
  const rawScale = Math.min(fitScaleX, fitScaleY);
  const targetScale = Number(Math.max(minScale, Math.min(maxScale, rawScale)).toFixed(2));

  const focused = computeFocusedZoom(
    bubbleCenterX,
    bubbleCenterY,
    { scale: 1, tx: 0, ty: 0 },
    targetScale,
    vpWidth,
    vpHeight,
    contentRect,
    originLeft,
    originTop
  );

  return {
    ...focused,
    bubbleCenterX: Number(bubbleCenterX.toFixed(2)),
    bubbleCenterY: Number(bubbleCenterY.toFixed(2)),
    bubbleRect,
  };
}
