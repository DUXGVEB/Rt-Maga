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
 * Passively classifies whether a chapter is a Webtoon / Long-Strip format using only the natural
 * dimensions of already-loaded images (zero extra network requests).
 *
 * Safeguards against false positives:
 * - Ignores tiny icons/banners (`width < 200` or `height < 300`).
 * - Ignores landscape double-page spreads (`width > height * 1.15`).
 * - Requires at least 3 qualifying images before deciding so a single tall cover/strip never
 *   falsely triggers Webtoon mode on standard Japanese manga chapters.
 */
export function detectWebtoonFromDimensions(
  samples: PageDimensionSample[]
): WebtoonDetectionResult {
  const qualifyingRatios: number[] = [];

  for (const s of samples) {
    if (s.width < 200 || s.height < 300) continue;
    if (s.width > s.height * 1.15) continue;
    qualifyingRatios.push(s.height / s.width);
  }

  if (qualifyingRatios.length < 3) {
    return {
      decided: false,
      isWebtoon: false,
      qualifyingCount: qualifyingRatios.length,
      tallCount: 0,
      medianRatio: 0,
    };
  }

  const sorted = [...qualifyingRatios].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  const medianRatio =
    sorted.length % 2 === 1 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;

  const tallCount = qualifyingRatios.filter((r) => r >= 2.2).length;
  const veryTallCount = qualifyingRatios.filter((r) => r >= 2.8).length;
  const tallFraction = tallCount / qualifyingRatios.length;

  const isWebtoon =
    (medianRatio >= 2.0 && tallFraction >= 0.6) ||
    (veryTallCount >= 2 && medianRatio >= 1.9);

  return {
    decided: true,
    isWebtoon,
    qualifyingCount: qualifyingRatios.length,
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
