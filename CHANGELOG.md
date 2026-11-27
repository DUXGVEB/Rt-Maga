# Changelog

All notable changes to **RtMaga** are documented in this file.

## [3.3.0] - 2026-11-27

### Added & Improved
- **Hybrid 2-Tier On-Device AI Manga Translation & Speech-Bubble Detection:**
  - **Tier 1 (`Lite`, bundled):** On-device `PP-OCRv6_tiny` ONNX detection/recognition (with Android ML Kit translation & Windows ONNX DirectML/CPU fallback), kept strictly under the 35 MB bundle budget and verified with SHA-256 checksums.
  - **Tier 2 (`Qwen3-VL-2B`, optional download):** User-initiated download (`Qwen3-VL-2B-Instruct-IQ4_XS.gguf` + `mmproj-Qwen3VL-2B-Instruct-Q8_0.gguf`) with verified SHA-256 digests, Vulkan GPU / CPU offload, `n_ctx=2048`, Flash Attention, Q8_0 KV cache, stateless per-page KV reset, and immediate hard-unload + transient AI cache purge on disable, reader exit, or app exit.
  - Added Reader AI overlay, floating AI control pill, first-use model choice modal, Settings -> Translation Model management panel, and Android native `ai-runtime` plugin + NDK JNI bridge.
- **Persistent Cover Store:**
  - Manga covers are now stored in `<data_dir>/covers/<xx>/<sha256>` keyed by canonical URL, shared across Home, Sources, Bookmarks, and History, and immune to LRU page-cache eviction and cache clears.
- **Fail-Fast Backup & Layout Classification:**
  - Hardened `.rtmaga` and `.tachibk` backup/import flows with stricter validation and layout classification migration (`0009_layout_classification.sql`).
- **3D UI, Motion System & Accessibility:**
  - Refined 3D depth carousel math, GPU-friendly adaptive motion system, hero transitions, brand logo component, and reader accessibility controls.

## [2.5.2] - 2026-11-25

### Fixed
- Refined update and installation run guards to always release duplicate-run locks on every exit path.
- Refined image cache hit tracking and single-flight prefetch guards for more reliable cover caching and cleanup.

## [2.5.1] - 2026-11-24

### Fixed
- Android release APKs are now signed with the maintained local release keystore and verified before distribution. This fixes the invalid unsigned APK packaging in v2.5.0 that HyperOS rejected with package parsing/packageInfo errors.

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
