# RtMaga

A lightweight, minimalist cross-platform manga reader built with **Rust** (**Tauri 2** + **Svelte 5** + **Tailwind CSS 4**), targeting **Windows x64** and **Android ARM64 (Snapdragon)**.

## Features

- **Paste Any Link to Read Instantly:** Paste either a manga overview URL or a direct chapter URL. Powered by a 3-tier extraction pipeline:
  1. Dedicated site adapters (starting with TruyenQQ across `truyenqq*` mirrors).
  2. Generic HTML heuristics (auto-detects chapter lists including fractional chapters like `2.5`, scores page-image containers, filters ads/icons, and falls back to scanning `<script>` image arrays).
  3. Headless Microsoft Edge / Chrome fallback on Windows for JS-rendered pages, plus a visible verification window when a site requires manual bot verification.
- **Hotlink-Safe Image Proxy & Disk Cache:** Backend-signed (HMAC-SHA256) `img://` protocol attaches the proper `Referer` header automatically, retries fallback image mirrors, deduplicates concurrent requests, and caches images locally on disk with SQLite LRU eviction.
- **Continuous Vertical Reader:** Webtoon-style scrolling with automatic next-chapter prefetch and seamless chapter appending (toggleable), lazy-loaded pages with per-image retry, auto-hiding toolbar, scroll-position persistence & restore, and keyboard shortcuts (`←/→`, `Space`, `F`, `Esc`).
- **Home, History & Settings:** Clean Light / Dark / System theme, chronological reading history grouped by day, automatic background new-chapter checks for manga in history with badges & desktop notifications, and instant auto-saving preferences.
- **Desktop & Mobile Ready:** Left sidebar on Windows desktop and bottom navigation bar on Android phones.

## Downloads

Visit the [Releases](https://github.com/DUXGVEB/Rt-Maga/releases) page to download pre-built installers for **Windows x64** (`.exe` / `.msi`) and **Android ARM64 / Snapdragon** (`.apk`).

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

- `crates/mangacore`: Core scraping and URL resolution library with pure HTML parsers tested offline against fixtures (zero dependency on Tauri or SQLite).
- `src-tauri`: Tauri 2 application backend (SQLite WAL via `sqlx`, HMAC-signed `img://` image proxy, LRU disk cache, background chapter update checker).
- `ui`: Svelte 5 + TypeScript + Tailwind CSS 4 frontend.
