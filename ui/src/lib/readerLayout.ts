export interface PageGroupItem<T> {
  token: T;
  pageIdx: number;
}

export interface PageGroup<T> {
  groupIdx: number;
  items: PageGroupItem<T>[];
}

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
