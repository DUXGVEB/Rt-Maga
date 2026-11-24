# RtMaga

A lightweight, minimalist, **100% serverless (no-backend)** cross-platform manga reader built with **Rust** (**Tauri 2** + **Svelte 5** + **Tailwind CSS 4**), targeting **Windows x64** and **Android ARM64**.

> **Serverless & Local-First:** RtMaga operates completely offline-first on your own devices with **no central backend server**, **no cloud database**, **no telemetry**, and **no user accounts**. All scraping, caching, SQLite storage, proxy routing, and peer-to-peer LAN synchronization run directly on your device.

## Features

- **Paste Any Link to Read Instantly:** Paste either a series overview URL or a direct chapter URL. Powered by a multi-tier extraction pipeline:
  1. Pluggable site adapter interface (`Source` trait) for custom extensions.
  2. Generic HTML heuristics (auto-detects chapter lists including fractional chapters like `2.5`, scores page-image containers, filters non-content elements, and falls back to scanning embedded `<script>` image arrays).
  3. Optional headless browser fallback on Windows for JS-rendered pages, plus an interactive browser window when a site requires manual human verification.
- **Hotlink-Safe Image Proxy, Bandwidth Throttling & Scoped Disk Cache:** Backend-signed (HMAC-SHA256) `img://` protocol attaches the proper `Referer` header, validates redirects against SSRF, enforces magic-byte image validation, retries fallback mirrors with exponential backoff, applies an optional global download speed ceiling (`KB/s`), and caches images locally on disk with SQLite LRU eviction and scoped clearing (All, By Source, or By Series).
- **Customizable Vertical & Horizontal Reader Modes:**
  - **Vertical Mode:** Clean continuous vertical scrolling with **No-Gap Mode**, customizable page spacing, side margins (`0–64px`), per-image focus dimming, and Smart Webtoon / Long-Strip detection.
  - **Horizontal Mode:** Scrollable horizontal page groups (`1`, `2`, `3`, or custom `1–8` pages per group) with **LTR / RTL** reading direction, edge tap zones, anti-skip swipe physics, page or group snap, viewport centering, and unfocused-group brightness dimming (`0–100%`).
  - **Reader Controls & Gestures:** Interactive reading progress bar with page seek (`Page X / Y` + `%`), Quick Chapter Jump, configurable double-tap action (`Zoom`, `Toggle Toolbar`, `Next Page`, or `Disabled`), configurable toolbar auto-hide timer, Keep-Screen-On wake lock, optional broken-image skipping, and live visual layout previews in Settings.
- **Built-In Encrypted DNS (DoH) & Anti-Geo-Block Proxy Mode:** Zero-config RFC 8484 DNS-over-HTTPS plus application-internal HTTP, HTTPS, and SOCKS5 (`socks5h://` remote DNS resolution to prevent DNS leaks) proxy routing with per-domain rules (`Default`, `Force Proxy`, `Force Direct`, `Custom Proxy`), fail-closed IP-leak protection, SSRF filtering, and built-in latency (`ms`) testers.
- **Serverless LAN Device Sync (PC ↔ Android):** Direct peer-to-peer synchronization over local Wi-Fi/LAN with a 5-minute single-use session TTL, brute-force cooldown & lockout, and mutual confirmation on both devices showing peer Device Name and LAN IP:
  - **PC → Phone:** Generate a one-time QR code on PC and scan it from the phone.
  - **Phone → PC:** Generate a 6-digit Pairing Code on the phone and enter it on PC (with automatic UDP LAN discovery + manual `IP:Port` fallback).
  - **Smart Conflict Merge:** Deduplicates series by URL, skips identical chapter progress, picks the entry with further chapter reading progress when chapters differ, and merges per-chapter read flags.
- **Smart Homepage, Library & Searchable Settings:** Clean Light / Dark / System theme, independent Light & Dark **Accent Color** customization with automatic contrast-safe foregrounds, customizable **Navigation Order** (Home, Bookmarks, History, Settings) synced across desktop sidebar and mobile bottom bar, 7 built-in UI languages (English, Vietnamese, Chinese, Japanese, Russian, Korean, Thai), cover tap to resume vs. title tap for chapter list, unread chapter badges, dimmed read chapters, dedicated **Completed** section at the bottom of Home (automatically returning to active when a new chapter is found), **Hidden Series** manager & confirmed permanent deletion, non-blocking crash recovery, instant searchable Settings with per-section & global reset, and sanitized diagnostic log export.
- **Desktop & Adaptive Mobile Ready:** Left sidebar on Windows, Linux, and macOS desktop and adaptive edge-to-edge portrait & landscape layout on Android devices with customizable floating controls and safe-area/IME-aware scrolling.

## Downloads

Visit the official [Releases](https://github.com/DUXGVEB/Rt-Maga/releases) page to download official pre-built installers for **Windows x64** (`.exe` / `.msi`), **Android ARM64** (`.apk`), and future desktop/mobile packages.

> **Official Distribution Notice:** Only builds downloaded from `https://github.com/DUXGVEB/Rt-Maga/releases` (and verified against the published SHA-256 checksums) are official **RtMaga** releases. Third-party mirrors, rebranded clones, and unauthorized redistributed builds are neither permitted under our license nor verified by the project maintainer.

## License & Copyright

`Copyright (c) 2025-2026 RtMaga (DUXGVEB). All rights reserved.`

**RtMaga is a source-available project — NOT Open Source under OSI definitions.**
You may inspect the source code, download and run unmodified Official Builds for personal/educational use, make private local non-distributed builds/modifications, and report bugs, subject strictly to the **RtMaga Source-Available License (Version 1.0)**. Commercial use, redistribution of original or modified source/binaries, distribution of substantially derived products, and rebranding require prior written permission.

- [LICENSE](LICENSE) — Full terms of the RtMaga Source-Available License (v1.0)
- [COPYRIGHT.md](COPYRIGHT.md) — Copyright ownership and original vs. third-party materials
- [NOTICE](NOTICE) — Attribution and required third-party open-source notices (including MPL-2.0 Section 3.3)
- [CONTRIBUTING.md](CONTRIBUTING.md) — Bug reports, pull requests, and contribution license grant
- [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md) — Verified dependency license inventory
- [CHANGELOG.md](CHANGELOG.md) — Release history

## Development & Building

Requirements: Rust 1.85+, Node.js 20+.

```powershell
# Install frontend dependencies
npm install
npm --prefix ui install

# Run in development mode (Windows)
npm run dev

# Build Windows x64 installers (.exe / .msi)
npm run build

# Run Rust formatting, strict clippy lints, and tests
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

## Architecture

- `crates/mangacore`: Core scraping and URL resolution library with pure HTML parsers tested offline against synthetic fixtures (zero dependency on Tauri or SQLite).
- `src-tauri`: Tauri 2 application backend (SQLite WAL via `sqlx`, HMAC-signed `img://` image proxy, LRU disk cache, background chapter update checker).
- `ui`: Svelte 5 + TypeScript + Tailwind CSS 4 frontend.
