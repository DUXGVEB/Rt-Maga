# 📚 RtMaga

> **100% Serverless, Local-First Manga Reader** — built with **Rust**, **Tauri 2**, **Svelte 5**, and **SQLite WAL** for **Windows x64** and **Android ARM64**.
> Zero backend server • Zero cloud database • Zero accounts • Zero telemetry.

---

## ✨ Features

- 🔍 **Universal Link & Multi-Source Discovery:** Paste any series, chapter, or catalog URL — or search by manga title directly inside the **Sources** tab across your saved sources. Supports built-in adapters, automatic CMS detection, single-work gallery extraction, mirror domain overrides, and custom JSON CSS-selector rule packs.
- 🗂️ **Saved Sources & Supplemental Isolation:** Auto-saves discovered sources on browse/read, caches site icons, supports custom names & mirror domains, and keeps 18+/mature sources discreetly isolated under **Settings → Network → Supplemental Sources**.
- 📖 **Adaptive Reader (Vertical & Horizontal):** Continuous/No-Gap Webtoon mode, multi-page horizontal spreads (`1–8` pages, LTR/RTL), snap physics, focus dimming, smooth double-tap zoom, Quick Chapter Jump, and per-series layout overrides.
- ⚡ **Persistent Cover Cache & Low Footprint:** HMAC-signed `img://` image proxy with persistent local cover cache (`covers/`), in-memory hit indexing, SSRF/redirect/magic-byte guards, and automatic cleanup of transient chapter pages on chapter/manga exit.
- 🛡️ **Built-In Privacy & Networking:** RFC 8484 DNS-over-HTTPS (`DoH`), HTTP/HTTPS/`socks5h://` proxy routing with per-domain rules and latency testers.
- 🔄 **Direct LAN Device Sync (PC ↔ Android):** Offline peer-to-peer library, bookmark, and progress transfer over Wi-Fi via one-time QR code or 6-digit pairing PIN.
- 🐞 **Feedback & Bug Report Composer:** Draft bug reports or suggestions inside Settings, selectively attach specific sanitized logs, and export Markdown or open a pre-filled GitHub Issue.

---

## 🚀 Why RtMaga?

- **🔒 Truly Private & Offline-First:** Your reading history, bookmarks, covers, and sources stay locally on your device inside SQLite WAL.
- **💨 Fast & Light:** Native Rust scraper + bounded background prefetching + low RAM usage even on mobile devices.
- **🎨 Clean UX:** Light/Dark/System themes, customizable accent colors, reorderable navigation bar, 7 UI languages, and adaptive mobile portrait/landscape controls.

---

## 📖 How to Use

1. **Paste a Link or Search:** On **Home**, paste any manga/chapter/website URL or enter a title keyword to jump into **Sources** multi-source search.
2. **Manage Sources:** Open **Sources** to browse saved sites, edit mirror domains, import JSON rule packs, or manage isolated sources in **Settings → Network → Supplemental Sources**.
3. **Read & Customize:** Open any chapter and use tap zones, double-tap zoom, or the gear menu for layout, gap, and per-series overrides.
4. **Sync Between Devices:** Open **Settings → Network → LAN Sync** on both PC and Android to sync bookmarks and reading progress over local Wi-Fi.

---

## ⚠️ Important Limitations

> **Public Pages Only — No Login / Paid / DRM Content:**
> - **RtMaga CANNOT extract manga or comics from websites that require account login, paid subscriptions, VIP coins/tickets, or DRM-encrypted image protection.**
> - Only publicly accessible web pages and public APIs are supported.

---

## 📦 Downloads

Pre-built **Windows x64** (`.exe` / `.msi`) and **Android ARM64** (`.apk`) installers are published on [Releases](https://github.com/DUXGVEB/Rt-Maga/releases).

> Only builds from `https://github.com/DUXGVEB/Rt-Maga/releases` verified against published SHA-256 checksums are official releases.

---

## 🛠️ Build from Source

Requires **Rust 1.85+** and **Node.js 20+** (plus Android SDK/NDK for `.apk`).

```powershell
npm install
npm --prefix ui install
npm run dev                                  # Run desktop dev app
npm run build                                # Build Windows .exe / .msi
npx tauri android build --apk --target aarch64  # Build Android ARM64 .apk
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

---

## 📜 License & Legal

`Copyright (c) 2025-2026 RtMaga (DUXGVEB). All rights reserved.`

**RtMaga is source-available — NOT OSI Open Source.** Subject to the **RtMaga Source-Available License v1.0** (personal/educational builds permitted; redistribution or commercial use requires written permission):

- [LICENSE](LICENSE) · [COPYRIGHT.md](COPYRIGHT.md) · [NOTICE](NOTICE) · [CONTRIBUTING.md](CONTRIBUTING.md) · [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md) · [CHANGELOG.md](CHANGELOG.md)
