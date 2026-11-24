# Changelog

All notable changes to **RtMaga** are documented in this file.

## [2.5.0] - 2026-11-24

### Added & Improved
- **Universal Source Compatibility & Failure Recovery Engine:**
  - Multi-strategy extraction pipeline combining Strategy A (adaptive HTML with `<img>`, `<picture><source srcset>`, `<amp-img>`, inline CSS `background-image`, and 17+ lazy-loading attributes), Strategy B (optional desktop headless browser rendering), Strategy C (embedded `#__NEXT_DATA__`, JSON-LD, preloaded JSON arrays, and reader script state), Strategy D (versioned panic-isolated site adapters), and Strategy E (user-authenticated browser session cookies without storing credentials or bypassing DRM/CAPTCHA/paywalls).
  - Expanded URL/HTML classification distinguishing homepages, catalog pages, search pages, series pages, chapter pages, login/session-expired gates, verification gates, paywalls, geo/DRM restrictions, and unrelated non-comic pages with confidence scoring.
  - Pure-Rust binary image header inspection across JPEG, PNG, WebP (`VP8`/`VP8L`/`VP8X`), GIF, BMP, and AVIF payloads, rejecting empty, truncated, disguised HTML/SVG, and 1x1 tracking payloads.
  - 27-code diagnostic error taxonomy (`SourceDiagnosticReport` + expandable `SourceDiagnosticCard` with sanitized report copy and one-click verification/browser sign-in/network settings recovery), plus automatic cancellation of stale chapter requests during rapid navigation.
- **Lightweight UI Customization (Independent Light & Dark Accent Colors):**
  - Separate configurable primary/accent color palettes and custom hex color pickers for Light mode and Dark mode, persisted independently in SQLite settings.
  - WCAG contrast-aware foreground calculation (`--accent-fg`) applied across navigation highlights, switches, range sliders, action buttons, and badges with memoized CSS updates.
- **Customizable Navigation Destination Order:**
  - Reorder **Home**, **Bookmarks**, **History**, and **Settings** in **Settings → General → Navigation order** using accessible Move Up / Move Down and Reset controls.
  - Synced dynamically across desktop sidebar and mobile bottom navigation while preserving routing, badges, quick-resume history behavior, and back navigation.
- **Android Landscape Settings & Viewport Fix:**
  - Synchronized `--app-height` with `window.visualViewport` resize and `MainActivity` IME layout requests to prevent stale portrait viewport constraints after horizontal rotation.
  - Wrapped settings header tab pills cleanly at medium/landscape widths, removed height-trapping nested scroll containers on mobile/landscape, and tuned safe-area/IME bottom clearance so all settings remain scrollable via natural vertical touch swipe.
- **Source-Available Copyright & Third-Party License Documentation:**
  - Added custom `LICENSE` (RtMaga Source-Available License v1.0, explicitly non-OSI Open Source), `COPYRIGHT.md`, `NOTICE` (including MPL-2.0 Section 3.3 Larger Work disclosures), `CONTRIBUTING.md`, `THIRD-PARTY-LICENSES.md`, and `README.md` distribution/license links.
