import assert from "node:assert/strict";
import test from "node:test";
import {
  clampZoomPan,
  computeAnchoredZoom,
  computeNavStepCooldownMs,
  computeVerticalSpeedMultiplier,
  detectWebtoonFromDimensions,
  partitionPages,
} from "../src/lib/readerLayout.ts";

test("computeAnchoredZoom keeps exact clicked pixel anchored under pointer", () => {
  const vpW = 1000;
  const vpH = 800;
  const content = { left: 100, top: 0, width: 800, height: 800 };
  const px = 650;
  const py = 250;

  const res = computeAnchoredZoom(px, py, { scale: 1, tx: 0, ty: 0 }, 2, vpW, vpH, content);
  assert.equal(res.scale, 2);

  // Under transform-origin: 0 0, unscaled point (px, py) maps to (px * scale + tx, py * scale + ty).
  // When unclamped, it must remain pinned directly under (px, py)!
  const mappedX = px * res.scale + res.tx;
  const mappedY = py * res.scale + res.ty;
  assert.ok(Math.abs(mappedX - px) < 0.05, `expected mappedX=${mappedX} to equal px=${px}`);
  assert.ok(Math.abs(mappedY - py) < 0.05, `expected mappedY=${mappedY} to equal py=${py}`);
});

test("computeAnchoredZoom clamps edges so scaled image never leaves empty viewport gap", () => {
  const vpW = 1000;
  const vpH = 800;
  const content = { left: 0, top: 0, width: 1000, height: 800 };

  // Clicking near top-left corner (10, 10) at 2x zoom would want tx=-10, ty=-10, valid inside [ -1000..0 ].
  const corner = computeAnchoredZoom(10, 10, { scale: 1, tx: 0, ty: 0 }, 2, vpW, vpH, content);
  assert.ok(corner.tx <= 0 && corner.tx >= -1000);
  assert.ok(corner.ty <= 0 && corner.ty >= -800);

  // Panning far past top-left or bottom-right must clamp tightly to image bounds.
  const clampedTopLeft = clampZoomPan(500, 500, 2, vpW, vpH, content);
  assert.equal(clampedTopLeft.tx, 0);
  assert.equal(clampedTopLeft.ty, 0);

  const clampedBottomRight = clampZoomPan(-5000, -5000, 2, vpW, vpH, content);
  assert.equal(clampedBottomRight.tx, -1000);
  assert.equal(clampedBottomRight.ty, -800);
});

test("computeAnchoredZoom resets cleanly to identity at scale 1", () => {
  const res = computeAnchoredZoom(
    400,
    300,
    { scale: 2, tx: -300, ty: -200 },
    1,
    1000,
    800,
    { left: 0, top: 0, width: 1000, height: 800 }
  );
  assert.deepEqual(res, { scale: 1, tx: 0, ty: 0 });
});

test("navigation speed maps monotonically across 1..10", () => {
  const slowCd = computeNavStepCooldownMs(1);
  const midCd = computeNavStepCooldownMs(5);
  const fastCd = computeNavStepCooldownMs(10);
  assert.ok(slowCd > midCd && midCd > fastCd && fastCd >= 80);

  const slowMult = computeVerticalSpeedMultiplier(1);
  const midMult = computeVerticalSpeedMultiplier(5);
  const fastMult = computeVerticalSpeedMultiplier(10);
  assert.ok(slowMult < midMult && midMult < fastMult);
});

test("partitionPages groups spreads without dummy slots", () => {
  const groups = partitionPages(["a", "b", "c", "d", "e"], 2);
  assert.equal(groups.length, 3);
  assert.equal(groups[0].items.length, 2);
  assert.equal(groups[2].items.length, 1);
  assert.equal(groups[2].items[0].pageIdx, 4);
});

test("detectWebtoonFromDimensions distinguishes tall webtoons from standard manga", () => {
  const tall = detectWebtoonFromDimensions([
    { index: 0, width: 800, height: 3200 },
    { index: 1, width: 800, height: 2800 },
    { index: 2, width: 800, height: 3500 },
  ]);
  assert.equal(tall.decided, true);
  assert.equal(tall.isWebtoon, true);

  const standard = detectWebtoonFromDimensions([
    { index: 0, width: 1000, height: 1450 },
    { index: 1, width: 1000, height: 1420 },
    { index: 2, width: 1000, height: 1400 },
  ]);
  assert.equal(standard.decided, true);
  assert.equal(standard.isWebtoon, false);
});
