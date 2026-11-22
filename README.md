# RtMaga

A lightweight, minimalist, **100% serverless (no-backend)** cross-platform manga reader built with **Rust** (**Tauri 2** + **Svelte 5** + **Tailwind CSS 4**), targeting **Windows x64** and **Android ARM64**.

> **Serverless & Local-First:** RtMaga operates completely offline-first on your own devices with **no central backend server**, **no cloud database**, **no telemetry**, and **no user accounts**. All scraping, caching, SQLite storage, proxy routing, and peer-to-peer LAN synchronization run directly on your device.

## Features

- **Paste Any Link to Read Instantly:** Paste either a series overview URL or a direct chapter URL. Powered by a multi-tier extraction pipeline:
  1. Pluggable site adapter interface (`Source` trait) for custom extensions.
  2. Generic HTML heuristics (auto-detects chapter lists including fractional chapters like `2.5`, scores page-image containers, filters non-content elements, and falls back to scanning embedded `<script>` image arrays).
  3. Optional headless browser fallback on Windows for JS-rendered pages, plus an interactive browser window when a site requires manual human verification.
- **Hotlink-Safe Image Proxy & Disk Cache:** Backend-signed (HMAC-SHA256) `img://` protocol automatically attaches the proper `Referer` header, validates redirects against SSRF, retries fallback image mirrors, deduplicates concurrent requests, and caches images locally on disk with SQLite LRU eviction.
- **Customizable Vertical & Horizontal Reader Modes:**
  - **Vertical Mode:** Switch between **Continuous Strip** (seamless vertical scrolling with customizable page spacing and per-image focus dimming) and **Vertical Grouped Scroll** (groups of `1`, `2`, `3`, or custom `1–8` pages with separate intra-group and inter-group spacing, optional group snap, and unfocused-group dimming).
  - **Horizontal Mode:** Scrollable horizontal page groups (`1`, `2`, `3`, or custom `1–8` pages per group) with **LTR / RTL** reading direction, vertical-wheel-to-horizontal scrolling, page or group snap, viewport centering, and unfocused-group brightness dimming (`0–100%`).
  - Both modes persist their own independent settings and apply changes live without reloading the chapter or losing your current page position.
- **Built-In Anti-Geo-Block Proxy Mode:** Application-internal HTTP, HTTPS, and SOCKS5 (`socks5h://` remote DNS resolution to prevent DNS leaks) proxy routing with per-domain rules (`Default`, `Force Proxy`, `Force Direct`, `Custom Proxy`), fail-closed IP-leak protection, SSRF filtering, and a built-in connection & latency (`ms`) tester.
- **Serverless LAN Device Sync (PC ↔ Android):** Direct peer-to-peer synchronization over local Wi-Fi/LAN with a 5-minute single-use session TTL, brute-force cooldown & lockout, and mutual confirmation on both devices showing peer Device Name and LAN IP:
  - **PC → Phone:** Generate a one-time QR code on PC and scan it from the phone.
  - **Phone → PC:** Generate a 6-digit Pairing Code on the phone and enter it on PC (with automatic UDP LAN discovery + manual `IP:Port` fallback).
  - **Smart Conflict Merge:** Deduplicates series by URL, skips identical chapter progress, picks the entry with further chapter reading progress when chapters differ, and merges per-chapter read flags.
- **Home, History & Settings:** Clean Light / Dark / System theme, 7 built-in UI languages (English, Vietnamese, Chinese, Japanese, Russian, Korean, Thai), chronological reading history grouped by day, automatic background new-chapter checks for series in history with badges & desktop notifications, automatic in-app release detection & installer, and instant auto-saving preferences.
- **Desktop & Mobile Ready:** Left sidebar on Windows desktop and bottom navigation bar on Android phones.

## Downloads

Visit the [Releases](https://github.com/DUXGVEB/Rt-Maga/releases) page to download pre-built installers for **Windows x64** (`.exe` / `.msi`) and **Android ARM64** (`.apk`).

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
