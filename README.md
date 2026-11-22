# RtMaga

A lightweight, minimalist cross-platform manga reader built with **Rust** (**Tauri 2** + **Svelte 5** + **Tailwind CSS 4**), targeting **Windows x64** and **Android ARM64**.

## Features

- **Paste Any Link to Read Instantly:** Paste either a series overview URL or a direct chapter URL. Powered by a multi-tier extraction pipeline:
  1. Pluggable site adapter interface (`Source` trait) for custom extensions.
  2. Generic HTML heuristics (auto-detects chapter lists including fractional chapters like `2.5`, scores page-image containers, filters non-content elements, and falls back to scanning embedded `<script>` image arrays).
  3. Optional headless browser fallback on Windows for JS-rendered pages, plus an interactive browser window when a site requires manual human verification.
- **Hotlink-Safe Image Proxy & Disk Cache:** Backend-signed (HMAC-SHA256) `img://` protocol automatically attaches the proper `Referer` header, validates redirects against SSRF, retries fallback image mirrors, deduplicates concurrent requests, and caches images locally on disk with SQLite LRU eviction.
- **Continuous Vertical Reader:** Seamless vertical strip scrolling with automatic next-chapter prefetch and auto-append (toggleable), lazy-loaded pages with per-image retry, auto-hiding toolbar, scroll-position persistence & restore, and keyboard shortcuts (`←/→`, `Space`, `F`, `Esc`).
- **Home, History & Settings:** Clean Light / Dark / System theme, 7 built-in UI languages (English, Vietnamese, Chinese, Japanese, Russian, Korean, Thai), chronological reading history grouped by day, automatic background new-chapter checks for series in history with badges & desktop notifications, automatic new app release detection on startup, and instant auto-saving preferences.
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
