import assert from "node:assert/strict";
import test from "node:test";
import {
  badgePop,
  collapseReveal,
  easeSnappy,
  flipDuration,
  itemExit,
  MOTION_DURATION,
  shouldStaggerItem,
  staggerDelay,
} from "../src/lib/motion.ts";

test("easeSnappy accurately solves cubic-bezier(0.2, 0, 0, 1) with exact endpoints", () => {
  assert.equal(easeSnappy(0), 0);
  assert.equal(easeSnappy(1), 1);
  // Rapid acceleration early, smooth settle late (above linear midpoint)
  assert.ok(easeSnappy(0.5) > 0.7);
  let prev = 0;
  for (let i = 1; i <= 20; i += 1) {
    const v = easeSnappy(i / 20);
    assert.ok(v >= prev);
    prev = v;
  }
});

test("long-list stagger guard caps item count and skips stagger on long lists", () => {
  assert.equal(shouldStaggerItem(0, 10), true);
  assert.equal(shouldStaggerItem(MOTION_DURATION.staggerMaxItems - 1, 20), true);
  assert.equal(shouldStaggerItem(MOTION_DURATION.staggerMaxItems, 20), false);
  // Long list skips stagger completely for 60fps Android lock
  assert.equal(shouldStaggerItem(0, MOTION_DURATION.longListSkipThreshold + 1), false);
  assert.equal(staggerDelay(4, 18, 180, 100), 0);
  assert.equal(staggerDelay(4, 18, 180, 12), 72);
  assert.equal(staggerDelay(11, 30, 180, 12), 180);
});

test("flipDuration respects upper cap and skips FLIP on very long lists", () => {
  assert.equal(flipDuration(180, 20), 180);
  assert.equal(flipDuration(500, 20), MOTION_DURATION.flipMax);
  assert.equal(flipDuration(180, MOTION_DURATION.flipSkipThreshold + 10), 0);
});

test("GPU transition helpers produce bounded durations and compositor-only CSS", () => {
  const dummyEl = {} as Element;

  const badge = badgePop(dummyEl, { duration: 400, start: 0.7 });
  assert.equal(badge.duration, 200);
  const badgeMid = badge.css?.(0.5, 0.5) ?? "";
  assert.match(badgeMid, /opacity:\s*0\.5/);
  assert.match(badgeMid, /transform:\s*scale\(/);

  const panel = collapseReveal(dummyEl, { duration: 50, y: 8 });
  assert.equal(panel.duration, 120);
  const panelStart = panel.css?.(0, 1) ?? "";
  assert.match(panelStart, /translate3d\(0,\s*-8\.00px,\s*0\)/);
  assert.ok(!panelStart.includes("height:"));

  const exit = itemExit(dummyEl, { duration: 300 });
  assert.equal(exit.duration, 180);
  const exitMid = exit.css?.(0.5, 0.5) ?? "";
  assert.match(exitMid, /opacity:\s*0\.5/);
  assert.match(exitMid, /transform:\s*translate3d\(/);
});
