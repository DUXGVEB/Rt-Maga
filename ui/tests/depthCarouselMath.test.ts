import assert from "node:assert/strict";
import test from "node:test";
import {
  computeCard3DState,
  computeSpinPlan,
  computeStageScale,
  DEFAULT_DEPTH_CAROUSEL_CONFIG,
  deriveSlug,
  easeSpinOut,
  extractGenreStats,
  filterItemsByGenre,
  RANDOM_POOL_MAX_BOUND,
  sampleBoundedPool,
  shortestCircularDelta,
  stepCriticallyDampedSpring,
  wrapIndex,
  type RandomMangaItem,
} from "../src/lib/depthCarouselMath.ts";

function makeItem(id: number, genres: string[] = []): RandomMangaItem {
  return {
    id,
    title: `Manga ${id}`,
    cover: `tok_${id}`,
    genres,
    chapters: id * 10,
    rating: "50%",
    status: "ongoing",
    summary: `Summary for ${id}`,
    slug: `manga-${id}`,
    site: "example.com",
    bookmarked: id % 2 === 0,
    hasNew: false,
    isCompleted: false,
    unreadCount: 2,
    latestChapterName: `Ch. ${id * 10}`,
    resumeChapterUrl: `https://example.com/m/${id}/c1`,
    resumeScrollRatio: 0,
    resumePageIndex: 0,
    metadataLoaded: true,
  };
}

test("sampleBoundedPool enforces 24..32 RAM bound, deduplicates IDs, and avoids initial repeat", () => {
  const large = Array.from({ length: 120 }, (_, i) => makeItem(i + 1));
  large.push(makeItem(1), makeItem(2)); // duplicates

  const pool = sampleBoundedPool(large, 28, 1, () => 0.0);
  assert.equal(pool.length, 28);
  assert.ok(pool.length <= RANDOM_POOL_MAX_BOUND);
  assert.equal(new Set(pool.map((x) => x.id)).size, 28);
  assert.notEqual(pool[0].id, 1);

  // Oversized limit is clamped to max 32
  const clamped = sampleBoundedPool(large, 999, null, () => 0.3);
  assert.equal(clamped.length, RANDOM_POOL_MAX_BOUND);
});

test("computeSpinPlan always adds 2-3 full revolutions and excludes previous active item", () => {
  const count = 16;
  const itemIds = Array.from({ length: count }, (_, i) => 100 + i);

  for (let start = 0; start < count; start += 1) {
    let seq = 0;
    const rng = () => {
      seq += 1;
      return (seq * 0.37) % 1;
    };
    const plan = computeSpinPlan({
      currentPos: start,
      count,
      excludeId: itemIds[start],
      itemIds,
      rng,
    });
    assert.ok(plan);
    assert.notEqual(plan.targetIndex, start);
    assert.ok(plan.extraLoops === 2 || plan.extraLoops === 3);
    assert.ok(plan.endPos - plan.startPos >= 2 * count + 1);
    assert.equal(wrapIndex(plan.endPos, count), plan.targetIndex);
    assert.ok(plan.durationSec >= 2.2 && plan.durationSec <= 2.8);
  }

  // Single-item pool gracefully returns 0-duration snap
  const single = computeSpinPlan({ currentPos: 0, count: 1, excludeId: 100, itemIds: [100] });
  assert.ok(single);
  assert.equal(single.targetIndex, 0);
  assert.equal(single.durationSec, 0);
});

test("easeSpinOut hits exact boundaries and decelerates monotonically", () => {
  assert.equal(easeSpinOut(0), 0);
  assert.equal(easeSpinOut(1), 1);

  let prev = 0;
  for (let i = 1; i <= 20; i += 1) {
    const val = easeSpinOut(i / 20);
    assert.ok(val >= prev, `Expected monotonic increase at step ${i}`);
    prev = val;
  }
  // Fast initial whip (> 65% progress in first 25% of duration)
  assert.ok(easeSpinOut(0.25) > 0.65);
});

test("stepCriticallyDampedSpring is refresh-rate invariant between 60Hz and 120Hz", () => {
  const omega = 15;
  const totalTime = 0.25;

  let pos60 = 0;
  let vel60 = 6;
  const steps60 = Math.round(totalTime * 60);
  for (let i = 0; i < steps60; i += 1) {
    ({ pos: pos60, vel: vel60 } = stepCriticallyDampedSpring(pos60, 1, vel60, omega, 1 / 60));
  }

  let pos120 = 0;
  let vel120 = 6;
  const steps120 = Math.round(totalTime * 120);
  for (let i = 0; i < steps120; i += 1) {
    ({ pos: pos120, vel: vel120 } = stepCriticallyDampedSpring(pos120, 1, vel120, omega, 1 / 120));
  }

  assert.ok(Math.abs(pos60 - pos120) < 1e-9, `60Hz=${pos60}, 120Hz=${pos120}`);
  assert.ok(Math.abs(vel60 - vel120) < 1e-9, `60Hz=${vel60}, 120Hz=${vel120}`);
});

test("computeCard3DState respects baseline 3D depth/spread/tilt/falloff/visibleCards", () => {
  const center = computeCard3DState(0, 0, 12, DEFAULT_DEPTH_CAROUSEL_CONFIG, 1);
  assert.equal(center.tx, 0);
  assert.equal(center.tz, 0);
  assert.equal(center.rotY, 0);
  assert.equal(center.scale, 1);
  assert.equal(center.opacity, 1);
  assert.equal(center.shadeOpacity, 0);
  assert.equal(center.isCenter, true);
  assert.equal(center.visible, true);

  const right1 = computeCard3DState(1, 0, 12, DEFAULT_DEPTH_CAROUSEL_CONFIG, 1);
  assert.equal(right1.tx, 90);
  assert.equal(right1.tz, -220);
  assert.equal(right1.rotY, -22);
  assert.ok(Math.abs(right1.opacity - 0.8) < 1e-6);
  assert.equal(right1.isCenter, false);
  assert.ok(right1.shadeOpacity > 0);

  // Far circular wrap hides cards beyond visibleCards + margin
  const hidden = computeCard3DState(6, 0, 12, DEFAULT_DEPTH_CAROUSEL_CONFIG, 1);
  assert.equal(hidden.visible, false);
  assert.equal(hidden.opacity, 0);

  // Circular delta wraps across index 0 <-> 11
  assert.equal(shortestCircularDelta(11, 0, 12), -1);
  assert.equal(shortestCircularDelta(0, 11, 12), 1);
});

test("genre extraction, filtering, slug derivation, and responsive stage scaling work cleanly", () => {
  const items = [
    makeItem(1, ["Action", "Fantasy"]),
    makeItem(2, ["action", "Comedy"]),
    makeItem(3, ["Fantasy", "Romance"]),
  ];

  const stats = extractGenreStats(items);
  assert.equal(stats[0].count, 2);
  assert.equal(stats[1].count, 2);

  const filtered = filterItemsByGenre(items, "ACTION");
  assert.deepEqual(
    filtered.map((x) => x.id),
    [1, 2]
  );

  assert.equal(deriveSlug("https://example.com/manga/one-piece/", "One Piece"), "one-piece");
  assert.equal(deriveSlug("", "Solo Leveling!"), "solo-leveling");

  const phoneScale = computeStageScale(340, 320);
  const desktopScale = computeStageScale(700, 460);
  assert.ok(phoneScale < desktopScale);
  assert.ok(phoneScale >= 0.62 && desktopScale <= 1.06);
});
