import assert from "node:assert/strict";
import test from "node:test";
import {
  clampZoomPan,
  computeAnchoredZoom,
  computeHorizontalRtlSlideOffset,
  computeHorizontalSlideOffset,
  computeHorizontalTrackMetrics,
  computeNavStepCooldownMs,
  computeSeamlessTrackTranslateX,
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

test("computeHorizontalTrackMetrics preserves center fit and visible side peeks across viewports", () => {
  const viewports = [
    { name: "desktop 1080p", w: 1920, h: 1080, count: 1 },
    { name: "laptop 1280x800", w: 1280, h: 800, count: 1 },
    { name: "mobile portrait", w: 390, h: 844, count: 1 },
    { name: "mobile landscape", w: 844, h: 390, count: 1 },
    { name: "desktop 2-page spread", w: 1920, h: 1080, count: 2 },
  ];

  for (const vp of viewports) {
    const m = computeHorizontalTrackMetrics(vp.w, vp.h, vp.count, 16);
    assert.ok(m.slotWidth < vp.w, `${vp.name}: active slot must be narrower than viewport`);
    assert.equal(m.stride, m.slotWidth + m.gapPx);

    // Adjacent slide inner edge relative to viewport center is (stride - slotWidth / 2).
    // For a visible peek on both left and right edges, that inner edge must lie strictly inside vp.w / 2!
    const peekInnerEdgeFromCenter = m.stride - m.slotWidth / 2;
    assert.ok(
      peekInnerEdgeFromCenter < vp.w / 2,
      `${vp.name}: adjacent slides must visibly peek inside viewport (edge=${peekInnerEdgeFromCenter}, halfVp=${vp.w / 2})`
    );
  }
});

test("computeHorizontalRtlSlideOffset enforces strict RTL ordering and center snapping", () => {
  const stride = 820;
  // When Page 2 (idx=1) is active:
  // - Page 1 (idx=0, Previous) sits to the RIGHT (+stride)
  // - Page 2 (idx=1, Active) sits DEAD CENTER (0)
  // - Page 3 (idx=2, Next) sits to the LEFT (-stride)
  // - End-of-Chapter Card (idx=3) sits at FAR LEFT (-2 * stride)
  assert.equal(computeHorizontalRtlSlideOffset(0, 1, stride, 0), 820);
  assert.equal(computeHorizontalRtlSlideOffset(1, 1, stride, 0), 0);
  assert.equal(computeHorizontalRtlSlideOffset(2, 1, stride, 0), -820);
  assert.equal(computeHorizontalRtlSlideOffset(3, 1, stride, 0), -1640);

  // Dragging RIGHT (+100px) pulls the left peeking Next Page (idx=2) right toward center.
  assert.equal(computeHorizontalRtlSlideOffset(2, 1, stride, 100), -720);
  // Advancing to End-of-Chapter Card (idx=3) locks End Card dead center (0) and puts last page on right (+stride).
  assert.equal(computeHorizontalRtlSlideOffset(3, 3, stride, 0), 0);
  assert.equal(computeHorizontalRtlSlideOffset(2, 3, stride, 0), 820);
});

test("computeHorizontalSlideOffset enforces symmetric LTR ordering and center snapping", () => {
  const stride = 820;
  // In LTR when Page 2 (idx=1) is active:
  // - Page 1 (idx=0, Previous) sits to the LEFT (-stride)
  // - Page 2 (idx=1, Active) sits DEAD CENTER (0)
  // - Page 3 (idx=2, Next) sits to the RIGHT (+stride)
  // - End-of-Chapter Card (idx=3) sits at FAR RIGHT (+2 * stride)
  assert.equal(computeHorizontalSlideOffset(0, 1, stride, "ltr", 0), -820);
  assert.equal(computeHorizontalSlideOffset(1, 1, stride, "ltr", 0), 0);
  assert.equal(computeHorizontalSlideOffset(2, 1, stride, "ltr", 0), 820);
  assert.equal(computeHorizontalSlideOffset(3, 1, stride, "ltr", 0), 1640);

  // Dragging LEFT (-100px) pulls the right peeking Next Page (idx=2) left toward center.
  assert.equal(computeHorizontalSlideOffset(2, 1, stride, "ltr", -100), 720);
  // Advancing to End-of-Chapter Card (idx=3) locks End Card dead center (0) and puts last page on left (-stride).
  assert.equal(computeHorizontalSlideOffset(3, 3, stride, "ltr", 0), 0);
  assert.equal(computeHorizontalSlideOffset(2, 3, stride, "ltr", 0), -820);
});

test("computeSeamlessTrackTranslateX locks arbitrary-width joined slides dead center at 0px gap", () => {
  const vpW = 1920;
  // Seamless strip of 3 joined pages (0px gap): Page 1 [0..720], Page 2 [720..1480], Page 3 [1480..2880].
  // Centering Page 2 (left=720, width=760 -> center=1100) inside vpW=1920 (center=960) => translateX = -140.
  const tx = computeSeamlessTrackTranslateX(vpW, 720, 760, 0);
  assert.equal(tx, -140);
  // Verify Page 1 right edge (720 + tx = 580) touches Page 2 left edge (720 + tx = 580) with 0px gap!
  assert.equal(720 + tx, 580);
  // Verify Page 2 center (1100 + tx = 960) is dead center of 1920px viewport!
  assert.equal(720 + 760 / 2 + tx, vpW / 2);
  // Verify dragOffsetPx shifts track 1:1.
  assert.equal(computeSeamlessTrackTranslateX(vpW, 720, 760, 85), -55);
});
