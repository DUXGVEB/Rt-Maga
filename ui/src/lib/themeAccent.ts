// Copyright (c) 2026 RtMaga (DUXGVEB). Source-available, not Open Source.
// See LICENSE and COPYRIGHT.md in the repository root for permitted uses and restrictions.

import type { NavDestination } from "./types";

export const DEFAULT_LIGHT_ACCENT = "#e11d48";
export const DEFAULT_DARK_ACCENT = "#f43f5e";

export const DEFAULT_NAV_ORDER: readonly NavDestination[] = [
  "home",
  "bookmarks",
  "history",
  "settings",
];

export interface AccentPreset {
  hex: string;
  label: string;
}

export const LIGHT_ACCENT_PRESETS: readonly AccentPreset[] = [
  { hex: "#e11d48", label: "Crimson" },
  { hex: "#2563eb", label: "Cobalt" },
  { hex: "#0d9488", label: "Teal" },
  { hex: "#059669", label: "Emerald" },
  { hex: "#7c3aed", label: "Violet" },
  { hex: "#d97706", label: "Amber" },
  { hex: "#ea580c", label: "Tangerine" },
  { hex: "#0284c7", label: "Azure" },
];

export const DARK_ACCENT_PRESETS: readonly AccentPreset[] = [
  { hex: "#f43f5e", label: "Rose" },
  { hex: "#60a5fa", label: "Sky" },
  { hex: "#2dd4bf", label: "Mint" },
  { hex: "#34d399", label: "Jade" },
  { hex: "#a78bfa", label: "Lavender" },
  { hex: "#fbbf24", label: "Gold" },
  { hex: "#fb923c", label: "Coral" },
  { hex: "#38bdf8", label: "Cyan" },
];

export interface AccentTokens {
  accent: string;
  accentHover: string;
  accentSoft: string;
  accentFg: string;
  surfaceContrast: number;
}

interface Rgb {
  r: number;
  g: number;
  b: number;
}

export function normalizeHexColor(raw: string | null | undefined, fallback: string): string {
  const clean = (raw ?? "").trim().replace(/^#/, "");
  if (/^[0-9a-fA-F]{6}$/.test(clean)) {
    return `#${clean.toLowerCase()}`;
  }
  if (/^[0-9a-fA-F]{3}$/.test(clean)) {
    const expanded = clean
      .split("")
      .map((ch) => ch + ch)
      .join("")
      .toLowerCase();
    return `#${expanded}`;
  }
  return fallback;
}

function parseHexRgb(hex: string): Rgb {
  const normalized = normalizeHexColor(hex, DEFAULT_LIGHT_ACCENT);
  const val = Number.parseInt(normalized.slice(1), 16);
  return {
    r: (val >> 16) & 255,
    g: (val >> 8) & 255,
    b: val & 255,
  };
}

function channelLuminance(c: number): number {
  const srgb = c / 255;
  return srgb <= 0.03928 ? srgb / 12.92 : ((srgb + 0.055) / 1.055) ** 2.4;
}

export function relativeLuminance(hex: string): number {
  const { r, g, b } = parseHexRgb(hex);
  return (
    0.2126 * channelLuminance(r) +
    0.7152 * channelLuminance(g) +
    0.0722 * channelLuminance(b)
  );
}

export function contrastRatio(hexA: string, hexB: string): number {
  const lumA = relativeLuminance(hexA);
  const lumB = relativeLuminance(hexB);
  const lighter = Math.max(lumA, lumB);
  const darker = Math.min(lumA, lumB);
  return (lighter + 0.05) / (darker + 0.05);
}

function mixRgb(a: Rgb, b: Rgb, weightB: number): string {
  const w = Math.max(0, Math.min(1, weightB));
  const r = Math.round(a.r * (1 - w) + b.r * w);
  const g = Math.round(a.g * (1 - w) + b.g * w);
  const bl = Math.round(a.b * (1 - w) + b.b * w);
  return `#${((1 << 24) | (r << 16) | (g << 8) | bl).toString(16).slice(1)}`;
}

/**
 * Derives `--accent`, `--accent-hover`, `--accent-soft`, `--accent-fg`, and surface contrast
 * ratio for the active theme mode (`light` or `dark`) from a user-selected hex color.
 */
export function computeAccentTokens(rawHex: string, mode: "light" | "dark"): AccentTokens {
  const fallback = mode === "dark" ? DEFAULT_DARK_ACCENT : DEFAULT_LIGHT_ACCENT;
  const accent = normalizeHexColor(rawHex, fallback);
  const rgb = parseHexRgb(accent);
  const surfaceHex = mode === "dark" ? "#18181d" : "#ffffff";

  const accentHover =
    mode === "dark"
      ? mixRgb(rgb, { r: 255, g: 255, b: 255 }, 0.16)
      : mixRgb(rgb, { r: 0, g: 0, b: 0 }, 0.16);
  const accentSoft = `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${mode === "dark" ? 0.16 : 0.1})`;

  const whiteContrast = contrastRatio(accent, "#ffffff");
  const darkContrast = contrastRatio(accent, "#18181b");
  const accentFg = whiteContrast >= darkContrast ? "#ffffff" : "#18181b";
  const surfaceContrast = contrastRatio(accent, surfaceHex);

  return {
    accent,
    accentHover,
    accentSoft,
    accentFg,
    surfaceContrast,
  };
}

/**
 * Guarantees a canonical, duplicate-free list containing all four main navigation destinations.
 */
export function normalizeNavOrder(raw?: readonly NavDestination[] | null): NavDestination[] {
  const out: NavDestination[] = [];
  if (Array.isArray(raw)) {
    for (const item of raw) {
      if (DEFAULT_NAV_ORDER.includes(item) && !out.includes(item)) {
        out.push(item);
      }
    }
  }
  for (const fallback of DEFAULT_NAV_ORDER) {
    if (!out.includes(fallback)) {
      out.push(fallback);
    }
  }
  return out;
}

/**
 * Moves a destination up (`delta = -1`) or down (`delta = 1`) safely within bounds.
 */
export function moveNavDestination(
  raw: readonly NavDestination[],
  index: number,
  delta: -1 | 1
): NavDestination[] {
  const current = normalizeNavOrder(raw);
  const target = index + delta;
  if (index < 0 || index >= current.length || target < 0 || target >= current.length) {
    return current;
  }
  const next = [...current];
  const [moved] = next.splice(index, 1);
  next.splice(target, 0, moved);
  return next;
}
