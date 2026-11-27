import type { BookmarkItemView, HistoryItemView, MangaView } from "./types";

function isCatalogOnlyEntry(site: string, chapterUrl: string | null | undefined): boolean {
  if (site === "local:tachibk") return true;
  return Boolean(chapterUrl && chapterUrl.startsWith("https://tachibk.local/"));
}

export type TiltDirection = "right" | "left";

export interface DepthCarouselConfig {
  depth: number;
  spread: number;
  tilt: number;
  tiltDirection: TiltDirection;
  perspective: number;
  visibleCards: number;
  falloff: number;
  loop: boolean;
  autoplay: boolean;
}

export const DEFAULT_DEPTH_CAROUSEL_CONFIG: Readonly<DepthCarouselConfig> = {
  depth: 220,
  spread: 90,
  tilt: 22,
  tiltDirection: "right",
  perspective: 1400,
  visibleCards: 4,
  falloff: 0.2,
  loop: true,
  autoplay: false,
};

export const RANDOM_POOL_MIN_BOUND = 24;
export const RANDOM_POOL_DEFAULT_BOUND = 28;
export const RANDOM_POOL_MAX_BOUND = 32;

export interface CachedMangaMeta {
  id: number;
  genres: string[];
  tags: string[];
  status: string | null;
  summary: string | null;
  totalChapters: number;
  canonicalUrl: string | null;
}

/**
 * Normalized manga card payload bound to RtMaga's existing schema
 * (`id, title, cover, genres, chapters, rating, status, summary, slug`).
 */
export interface RandomMangaItem {
  id: number;
  title: string;
  cover: string | null;
  genres: string[];
  chapters: number;
  rating: string | null;
  status: string | null;
  summary: string | null;
  slug: string;
  site: string;
  bookmarked: boolean;
  hasNew: boolean;
  isCompleted: boolean;
  unreadCount: number;
  latestChapterName: string | null;
  resumeChapterUrl: string | null;
  resumeScrollRatio: number;
  resumePageIndex: number;
  metadataLoaded: boolean;
}

export interface GenreStat {
  name: string;
  count: number;
}

export interface Card3DState {
  tx: number;
  tz: number;
  rotY: number;
  scale: number;
  opacity: number;
  shadeOpacity: number;
  zIndex: number;
  visible: boolean;
  isCenter: boolean;
  absRel: number;
}

export interface SpinPlan {
  targetIndex: number;
  startPos: number;
  endPos: number;
  durationSec: number;
  extraLoops: number;
}

export function wrapIndex(index: number, count: number): number {
  if (count <= 0) return 0;
  return ((index % count) + count) % count;
}

/**
 * Shortest signed circular offset from `virtualPos` to `cardIndex` in `[-count/2, count/2]`.
 */
export function shortestCircularDelta(
  cardIndex: number,
  virtualPos: number,
  count: number
): number {
  if (count <= 1) return 0;
  const wrappedPos = ((virtualPos % count) + count) % count;
  let delta = cardIndex - wrappedPos;
  const half = count / 2;
  if (delta > half) delta -= count;
  else if (delta < -half) delta += count;
  return delta;
}

/**
 * Smooth `expo.out` / `power4.out` blend for the multi-loop spin:
 * whips rapidly through revolutions at the beginning, then glides cleanly
 * to a decisive settle at `t = 1` with zero asymptotic snap.
 */
export function easeSpinOut(t: number): number {
  if (t <= 0) return 0;
  if (t >= 1) return 1;
  const expoNorm = (1 - Math.pow(2, -10 * t)) / (1 - Math.pow(2, -10));
  const power4 = 1 - Math.pow(1 - t, 4);
  return 0.65 * expoNorm + 0.35 * power4;
}

/**
 * Exact analytical solution of a critically damped harmonic oscillator (`zeta = 1`).
 * Unconditionally stable and refresh-rate invariant across 60Hz Windows and 120Hz Android.
 */
export function stepCriticallyDampedSpring(
  pos: number,
  target: number,
  vel: number,
  omega: number,
  dtSec: number
): { pos: number; vel: number } {
  const dt = Math.max(0, Math.min(0.05, dtSec));
  if (dt === 0) return { pos, vel };
  const diff = pos - target;
  const exp = Math.exp(-omega * dt);
  const coeff = vel + omega * diff;
  return {
    pos: target + (diff + coeff * dt) * exp,
    vel: (vel - omega * coeff * dt) * exp,
  };
}

/**
 * Computes responsive stage scale for the `260x390` (2:3) base card geometry
 * so the 3D depth arc stays crisp on narrow Android phones and wide Windows monitors.
 */
export function computeStageScale(stageWidth: number, stageHeight: number): number {
  const safeW = Math.max(240, stageWidth);
  const safeH = Math.max(260, stageHeight);
  const byWidth = safeW / 620;
  const byHeight = safeH / 445;
  return Math.max(0.62, Math.min(1.06, Math.min(byWidth, byHeight)));
}

/**
 * Computes GPU-composited 3D DepthCarousel transforms (`translate3d`, `rotateY`, `scale`, `opacity`)
 * plus a GPU-cheap dark tint opacity cue (zero CSS `filter: blur()` cost on Android WebView).
 */
export function computeCard3DState(
  cardIndex: number,
  virtualPos: number,
  count: number,
  config: DepthCarouselConfig,
  stageScale: number,
  out?: Card3DState
): Card3DState {
  const rel =
    config.loop && count > 1
      ? shortestCircularDelta(cardIndex, virtualPos, count)
      : cardIndex - virtualPos;
  const absRel = Math.abs(rel);
  const fadeMargin = 0.55;
  const visible = absRel <= config.visibleCards + fadeMargin;

  const tiltSign = config.tiltDirection === "right" ? -1 : 1;
  const tx = rel * config.spread * stageScale || 0;
  const tz = -absRel * config.depth * stageScale || 0;
  const clampedTiltRel = Math.max(-2.35, Math.min(2.35, rel));
  const rotY = tiltSign * clampedTiltRel * config.tilt || 0;
  const scale = Math.max(0.56, 1 - absRel * (config.falloff * 0.52));

  let opacity = Math.max(0, 1 - absRel * config.falloff);
  if (absRel > config.visibleCards) {
    const tail = Math.max(0, 1 - (absRel - config.visibleCards) / fadeMargin);
    opacity *= tail;
  }
  if (!visible) opacity = 0;

  const shadeOpacity = Math.min(0.56, absRel * 0.14);
  const zIndex = Math.max(1, Math.round(100 - absRel * 10));
  const isCenter = absRel < 0.35;

  if (out) {
    out.tx = tx;
    out.tz = tz;
    out.rotY = rotY;
    out.scale = scale;
    out.opacity = opacity;
    out.shadeOpacity = shadeOpacity;
    out.zIndex = zIndex;
    out.visible = visible;
    out.isCenter = isCenter;
    out.absRel = absRel;
    return out;
  }

  return {
    tx,
    tz,
    rotY,
    scale,
    opacity,
    shadeOpacity,
    zIndex,
    visible,
    isCenter,
    absRel,
  };
}

/**
 * Plans a multi-revolution forward spin (`2–3` full loops + forward offset),
 * deliberately avoiding shortest-path focus and excluding the previous active item
 * whenever `count > 1`.
 */
export function computeSpinPlan(params: {
  currentPos: number;
  count: number;
  excludeId?: number | null;
  itemIds?: readonly number[];
  rng?: () => number;
}): SpinPlan | null {
  const { currentPos, count, excludeId = null, itemIds, rng = Math.random } = params;
  if (count <= 0) return null;
  const snappedStart = Math.round(currentPos);
  if (count === 1) {
    return {
      targetIndex: 0,
      startPos: snappedStart,
      endPos: snappedStart,
      durationSec: 0,
      extraLoops: 0,
    };
  }

  const currentIdx = wrapIndex(snappedStart, count);
  const candidates: number[] = [];
  for (let i = 0; i < count; i += 1) {
    if (i === currentIdx) continue;
    if (excludeId !== null && itemIds && itemIds[i] === excludeId) continue;
    candidates.push(i);
  }
  // Fallback if excludeId filtered out the only remaining candidate.
  if (candidates.length === 0) {
    for (let i = 0; i < count; i += 1) {
      if (i !== currentIdx) candidates.push(i);
    }
  }

  const pickIdx = Math.min(candidates.length - 1, Math.floor(rng() * candidates.length));
  const targetIndex = candidates[pickIdx];
  const rawOffset = ((targetIndex - currentIdx) % count + count) % count;
  const forwardOffset = rawOffset === 0 ? count : rawOffset;
  const extraLoops = rng() < 0.5 ? 2 : 3;
  const endPos = snappedStart + extraLoops * count + forwardOffset;
  const durationSec = 2.2 + rng() * 0.6;

  return {
    targetIndex,
    startPos: snappedStart,
    endPos,
    durationSec,
    extraLoops,
  };
}

export function deriveSlug(url: string | null | undefined, title: string): string {
  const rawUrl = (url ?? "").trim();
  if (rawUrl) {
    try {
      const parsed = new URL(rawUrl);
      const segments = parsed.pathname.split("/").filter(Boolean);
      const last = segments[segments.length - 1];
      if (last) {
        return decodeURIComponent(last).slice(0, 64);
      }
    } catch {
      const cleaned = rawUrl.replace(/[?#].*$/, "").replace(/\/+$/, "");
      const last = cleaned.split("/").filter(Boolean).pop();
      if (last) return last.slice(0, 64);
    }
  }
  return title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
}

export function extractCachedMangaMeta(view: MangaView): CachedMangaMeta {
  const cleanGenres = dedupeLabels(view.genres);
  const cleanTags = dedupeLabels(view.tags);
  return {
    id: view.id,
    genres: cleanGenres,
    tags: cleanTags,
    status: view.status?.trim() || null,
    summary: view.description?.trim() || null,
    totalChapters: view.chapters.length,
    canonicalUrl: view.canonicalUrl ?? view.url ?? null,
  };
}

function dedupeLabels(values: readonly string[] | undefined): string[] {
  if (!values || values.length === 0) return [];
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of values) {
    const clean = raw.trim();
    if (!clean) continue;
    const key = clean.toLowerCase();
    if (!seen.has(key)) {
      seen.add(key);
      out.push(clean);
    }
  }
  return out;
}

function deriveRatingCue(
  totalChapters: number,
  unreadCount: number,
  isCompleted: boolean
): string | null {
  if (isCompleted) return "100%";
  if (totalChapters > 0 && unreadCount >= 0 && unreadCount < totalChapters) {
    const read = Math.max(0, totalChapters - unreadCount);
    const pct = Math.round((read / totalChapters) * 100);
    if (pct > 0) return `${pct}%`;
  }
  return null;
}

export function mapHistoryToRandomItem(
  item: HistoryItemView,
  meta?: CachedMangaMeta
): RandomMangaItem {
  const genres = meta ? (meta.genres.length > 0 ? meta.genres : meta.tags) : [];
  const chapters = Math.max(item.totalChapters, meta?.totalChapters ?? 0);
  const canResume = Boolean(item.chapterUrl) && !isCatalogOnlyEntry(item.site, item.chapterUrl);
  return {
    id: item.mangaId,
    title: item.title,
    cover: item.coverToken,
    genres,
    chapters,
    rating: deriveRatingCue(chapters, item.unreadCount, item.isCompleted),
    status: meta?.status ?? null,
    summary: meta?.summary ?? null,
    slug: deriveSlug(meta?.canonicalUrl ?? item.mangaUrl, item.title),
    site: item.site,
    bookmarked: item.bookmarked,
    hasNew: item.hasNew,
    isCompleted: item.isCompleted,
    unreadCount: item.unreadCount,
    latestChapterName: item.latestChapterName ?? item.chapterName ?? null,
    resumeChapterUrl: canResume ? item.chapterUrl : null,
    resumeScrollRatio: item.scrollRatio,
    resumePageIndex: item.pageIndex,
    metadataLoaded: Boolean(meta),
  };
}

export function mapBookmarkToRandomItem(
  item: BookmarkItemView,
  meta?: CachedMangaMeta
): RandomMangaItem {
  const genres = meta ? (meta.genres.length > 0 ? meta.genres : meta.tags) : [];
  const chapters = Math.max(item.totalChapters, meta?.totalChapters ?? 0);
  const canResume = Boolean(item.chapterUrl) && !isCatalogOnlyEntry(item.site, item.chapterUrl);
  return {
    id: item.mangaId,
    title: item.title,
    cover: item.coverToken,
    genres,
    chapters,
    rating: deriveRatingCue(chapters, item.unreadCount, item.isCompleted),
    status: meta?.status ?? null,
    summary: meta?.summary ?? null,
    slug: deriveSlug(meta?.canonicalUrl ?? item.mangaUrl, item.title),
    site: item.site,
    bookmarked: true,
    hasNew: item.hasNew,
    isCompleted: item.isCompleted,
    unreadCount: item.unreadCount,
    latestChapterName: item.latestChapterName ?? item.chapterName ?? null,
    resumeChapterUrl: canResume ? item.chapterUrl : null,
    resumeScrollRatio: item.scrollRatio ?? 0,
    resumePageIndex: item.pageIndex ?? 0,
    metadataLoaded: Boolean(meta),
  };
}

/**
 * Merges History + Bookmarks without duplicates into a unified library catalog for sampling.
 */
export function buildUnifiedLibraryItems(
  history: readonly HistoryItemView[],
  bookmarks: readonly BookmarkItemView[],
  metaById: Readonly<Record<number, CachedMangaMeta>>,
  scope: "all" | "bookmarks" = "all"
): RandomMangaItem[] {
  const byId = new Map<number, RandomMangaItem>();
  if (scope === "all") {
    for (const h of history) {
      byId.set(h.mangaId, mapHistoryToRandomItem(h, metaById[h.mangaId]));
    }
  }
  for (const b of bookmarks) {
    const existing = byId.get(b.mangaId);
    if (existing) {
      existing.bookmarked = true;
      if (!existing.cover && b.coverToken) existing.cover = b.coverToken;
      if (b.totalChapters > existing.chapters) existing.chapters = b.totalChapters;
    } else {
      byId.set(b.mangaId, mapBookmarkToRandomItem(b, metaById[b.mangaId]));
    }
  }
  return Array.from(byId.values());
}

/**
 * Extracts normalized genres sorted by descending frequency for the Quick Genre Filter.
 */
export function extractGenreStats(items: readonly RandomMangaItem[]): GenreStat[] {
  const counts = new Map<string, { name: string; count: number }>();
  for (const item of items) {
    const seenInItem = new Set<string>();
    for (const raw of item.genres) {
      const clean = raw.trim();
      if (!clean) continue;
      const key = clean.toLowerCase();
      if (seenInItem.has(key)) continue;
      seenInItem.add(key);
      const entry = counts.get(key);
      if (entry) {
        entry.count += 1;
      } else {
        counts.set(key, { name: clean, count: 1 });
      }
    }
  }
  return Array.from(counts.values()).sort(
    (a, b) => b.count - a.count || a.name.localeCompare(b.name)
  );
}

/**
 * Filters library items by a selected genre label (case-insensitive).
 */
export function filterItemsByGenre(
  items: readonly RandomMangaItem[],
  genre: string | null
): RandomMangaItem[] {
  if (!genre || genre === "__all__") return [...items];
  const target = genre.trim().toLowerCase();
  if (!target) return [...items];
  return items.filter((item) =>
    item.genres.some((g) => g.trim().toLowerCase() === target)
  );
}

/**
 * Maintains a strictly bounded DOM pool (`<= maxSize`, clamped to 24..32)
 * using partial Fisher-Yates sampling, deduplicating IDs, and avoiding an immediate
 * duplicate at slot 0 when `avoidInitialId` is provided.
 */
export function sampleBoundedPool<T extends { id: number }>(
  items: readonly T[],
  maxSize: number = RANDOM_POOL_DEFAULT_BOUND,
  avoidInitialId: number | null = null,
  rng: () => number = Math.random
): T[] {
  const limit = Math.max(RANDOM_POOL_MIN_BOUND, Math.min(RANDOM_POOL_MAX_BOUND, maxSize));
  const seen = new Set<number>();
  const unique: T[] = [];
  for (const item of items) {
    if (!seen.has(item.id)) {
      seen.add(item.id);
      unique.push(item);
    }
  }
  const n = unique.length;
  if (n === 0) return [];

  const take = Math.min(n, limit);
  const pool = [...unique];
  for (let i = 0; i < take; i += 1) {
    const j = i + Math.floor(rng() * (n - i));
    const tmp = pool[i];
    pool[i] = pool[j];
    pool[j] = tmp;
  }
  const bounded = pool.slice(0, take);

  if (avoidInitialId !== null && bounded.length > 1 && bounded[0].id === avoidInitialId) {
    const swapIdx = 1 + Math.floor(rng() * (bounded.length - 1));
    const tmp = bounded[0];
    bounded[0] = bounded[swapIdx];
    bounded[swapIdx] = tmp;
  }

  return bounded;
}
