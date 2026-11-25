import assert from "node:assert/strict";
import test from "node:test";
import {
  computeAccentTokens,
  DEFAULT_DARK_ACCENT,
  DEFAULT_LIGHT_ACCENT,
  DEFAULT_NAV_ORDER,
  moveNavDestination,
  normalizeHexColor,
  normalizeNavOrder,
} from "../src/lib/themeAccent.ts";

test("normalizeHexColor validates and expands 3-digit and 6-digit hex colors cleanly", () => {
  assert.equal(normalizeHexColor("#2563EB", DEFAULT_LIGHT_ACCENT), "#2563eb");
  assert.equal(normalizeHexColor(" 3Af ", DEFAULT_LIGHT_ACCENT), "#33aaff");
  assert.equal(normalizeHexColor("invalid", DEFAULT_DARK_ACCENT), DEFAULT_DARK_ACCENT);
  assert.equal(normalizeHexColor("", DEFAULT_LIGHT_ACCENT), DEFAULT_LIGHT_ACCENT);
});

test("computeAccentTokens preserves independent mode contrast and selects readable foreground", () => {
  const lightCrimson = computeAccentTokens("#e11d48", "light");
  assert.equal(lightCrimson.accent, "#e11d48");
  assert.equal(lightCrimson.accentFg, "#ffffff");
  assert.ok(lightCrimson.surfaceContrast > 4.0);

  // Bright gold accent in dark mode must select dark text (`#18181b`) instead of white text
  const darkGold = computeAccentTokens("#fbbf24", "dark");
  assert.equal(darkGold.accent, "#fbbf24");
  assert.equal(darkGold.accentFg, "#18181b");
  assert.ok(darkGold.surfaceContrast > 8.0);
});

test("normalizeNavOrder deduplicates and preserves all five primary destinations", () => {
  assert.deepEqual(normalizeNavOrder(undefined), [...DEFAULT_NAV_ORDER]);
  assert.deepEqual(
    normalizeNavOrder(["settings", "home", "home"]),
    ["settings", "home", "sources", "bookmarks", "history"]
  );
  // Cast with invalid values simulates corrupted persisted JSON
  assert.deepEqual(
    normalizeNavOrder(["bookmarks", "unknown" as never, "history"]),
    ["bookmarks", "history", "home", "sources", "settings"]
  );
});

test("moveNavDestination reorders within bounds without duplicates or drops", () => {
  const step1 = moveNavDestination(DEFAULT_NAV_ORDER, 4, -1);
  assert.deepEqual(step1, ["home", "sources", "bookmarks", "settings", "history"]);

  const boundaryUp = moveNavDestination(step1, 0, -1);
  assert.deepEqual(boundaryUp, step1);

  const boundaryDown = moveNavDestination(step1, 4, 1);
  assert.deepEqual(boundaryDown, step1);

  const step2 = moveNavDestination(step1, 0, 1);
  assert.deepEqual(step2, ["sources", "home", "bookmarks", "settings", "history"]);
});
