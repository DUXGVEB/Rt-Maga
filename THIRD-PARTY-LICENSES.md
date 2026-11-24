# Third-Party Software Licenses and Obligations

This document summarizes the third-party software dependencies used to build or
run **RtMaga** across **Windows**, **Linux**, **macOS**, and **Android**. Each
third-party library remains governed solely by its own license terms (never
overridden by RtMaga's `LICENSE`).

> **Verification Note for Maintainers:**
> - **Rust crate licenses** below were directly verified against `Cargo.lock`
>   via `cargo metadata` and `cargo-deny` (`.agents/config/deny.toml`) at
>   release time (`v2.5.1`).
> - **npm direct dependencies** were directly read from installed `package.json`
>   manifests under `node_modules/` and `ui/node_modules/`.
> - **Android Gradle dependencies** list their declared coordinates from
>   `src-tauri/gen/android/app/build.gradle.kts`; their typical upstream AOSP
>   license (`Apache-2.0`) is flagged below for online `.pom`/Gradle license report
>   confirmation because offline verification did not fetch `.pom` files.

---

## 1. Direct Rust Workspace Crates (`Cargo.toml` / `crates/mangacore` / `src-tauri`)

All Rust dependencies are fetched from `crates.io` (`https://github.com/rust-lang/crates.io-index`).

| Crate | Verified Version(s) | Verified SPDX License Expression |
|---|---|---|
| `async-trait` | `0.1.92` | `MIT OR Apache-2.0` |
| `base64` | `0.22.1` (workspace), `0.21.7`, `0.23.1` | `MIT OR Apache-2.0` |
| `bytes` | `1.12.2` | `MIT` |
| `chromiumoxide` | `0.9.1` (desktop optional `headless` feature) | `MIT OR Apache-2.0` |
| `dashmap` | `6.2.1` | `MIT` |
| `futures` | `0.3.34` (desktop `headless` feature) | `MIT OR Apache-2.0` |
| `hmac` | `0.12.1` | `MIT OR Apache-2.0` |
| `http` | `1.5.0` | `MIT OR Apache-2.0` |
| `moka` | `0.12.16` | `(MIT OR Apache-2.0) AND Apache-2.0` |
| `qrcode` | `0.14.1` | `MIT OR Apache-2.0` |
| `rand` | `0.8.8` (direct), `0.9.5`, `0.10.4` | `MIT OR Apache-2.0` |
| `regex` | `1.13.2` | `MIT OR Apache-2.0` |
| `reqwest` | `0.12.28` (direct), `0.13.5` | `MIT OR Apache-2.0` |
| `scraper` | `0.23.1` | `ISC` |
| `serde` | `1.0.229` | `MIT OR Apache-2.0` |
| `serde_json` | `1.0.154` | `MIT OR Apache-2.0` |
| `sha2` | `0.10.9` | `MIT OR Apache-2.0` |
| `sqlx` | `0.8.6` | `MIT OR Apache-2.0` |
| `tauri` | `2.11.6` | `Apache-2.0 OR MIT` |
| `tauri-build` | `2.6.3` (build) | `Apache-2.0 OR MIT` |
| `tauri-plugin-notification` | `2.4.0` | `Apache-2.0 OR MIT` |
| `tauri-plugin-opener` | `2.5.5` | `Apache-2.0 OR MIT` |
| `thiserror` | `2.0.21` (direct), `1.0.69` | `MIT OR Apache-2.0` |
| `tokio` | `1.54.1` | `MIT` |
| `tracing` | `0.1.44` | `MIT` |
| `tracing-subscriber` | `0.3.23` | `MIT` |
| `url` | `2.5.9` | `MIT OR Apache-2.0` |

---

## 2. Transitive Rust Crate License Breakdown & Key Obligations

Across all resolved third-party entries in `Cargo.lock` audited via `cargo metadata`:

- **Permissive Families:** `MIT`, `Apache-2.0` (including `Apache-2.0 WITH LLVM-exception`), `MIT OR Apache-2.0`, `ISC`, `BSD-2-Clause`, `BSD-3-Clause`, `Zlib`, `BSL-1.0`, `0BSD`, `CC0-1.0`, `Unlicense`.
  - **Obligations:** Preserve upstream copyright notices, license texts, and `NOTICE` disclosures (handled via [NOTICE](NOTICE) and this file).
- **Unicode-3.0** (`18x` pure + `3x` `AND Unicode-3.0`, e.g., `icu4x` crates, `unicode-ident`, `unicode-normalization`, `unicode-properties`):
  - **Obligations:** Retain copyright notice and permission notice from the Unicode License v3.
- **CDLA-Permissive-2.0** (`1x`: `webpki-roots` `1.0.12`):
  - **Obligations:** Preserve CDLA-Permissive-2.0 notice and disclaimer (no share-alike / copyleft source requirement).
- **MPL-2.0** (`7x` crate instances: `cssparser` `0.34.0`/`0.36.0`, `cssparser-macros` `0.6.1`, `selectors` `0.26.0`/`0.36.1`, `dtoa-short` `0.3.5`, `option-ext` `0.2.0`):
  - **Obligations:** Weak file-level copyleft. Unmodified MPL-2.0 crates may be combined into a "Larger Work" under RtMaga's own `LICENSE` (MPL-2.0 Section 3.3), provided users are informed where to obtain the MPL-2.0 source code (`NOTICE` lists the upstream repositories) and any future modifications to those MPL-2.0 files themselves remain under MPL-2.0.
- **Disjunctive Optional LGPL-2.1-or-later** (`2x`: `r-efi` `5.3.0`, `6.0.0`):
  - Licensed under `MIT OR Apache-2.0 OR LGPL-2.1-or-later`. RtMaga elects **MIT** or **Apache-2.0**.

*(Full transitive crate list can be exported at any time via `cargo metadata --format-version 1` or `cargo-about`.)*

---

## 3. Frontend & Tooling Dependencies (`package.json` & `ui/package.json`)

| Package | Installed Version | Verified License | Scope |
|---|---|---|---|
| `@tauri-apps/api` | `2.12.5` | `Apache-2.0 OR MIT` | Runtime UI |
| `jsqr` | `1.4.0` | `Apache-2.0` | Runtime UI (QR scanner) |
| `lucide-svelte` | `0.511.0` | `ISC` | Runtime UI (UI icons) |
| `@tauri-apps/cli` | `2.12.6` | `Apache-2.0 OR MIT` | Build tooling |
| `svelte` | `5.59.1` | `MIT` | Compile/Runtime UI |
| `@sveltejs/vite-plugin-svelte` | `5.1.1` | `MIT` | Build tooling |
| `tailwindcss` / `@tailwindcss/vite` | `4.3.5` | `MIT` | Build tooling |
| `vite` | `6.4.6` | `MIT` | Build tooling |
| `typescript` | `5.9.3` | `Apache-2.0` | Build/typecheck tooling |
| `svelte-check` | `4.7.8` | `MIT` | Typecheck tooling |

*(Maintainer note: When producing release archives, transitive npm tooling packages under `node_modules` exist only at build time unless bundled by Vite into `ui/dist/`.)*

---

## 4. Android Native / Gradle Dependencies (`src-tauri/gen/android/app/build.gradle.kts`)

| Coordinate | Declared Version | Upstream License (Maintainer Verify via Gradle) | Scope |
|---|---|---|---|
| `androidx.webkit:webkit` | `1.14.0` | `Apache-2.0` (standard AOSP/AndroidX) | Android runtime |
| `androidx.appcompat:appcompat` | `1.7.1` | `Apache-2.0` (standard AOSP/AndroidX) | Android runtime |
| `androidx.activity:activity-ktx` | `1.10.1` | `Apache-2.0` (standard AOSP/AndroidX) | Android runtime |
| `com.google.android.material:material` | `1.12.0` | `Apache-2.0` (Material Components for Android) | Android runtime |
| `androidx.lifecycle:lifecycle-process` | `2.10.0` | `Apache-2.0` (standard AOSP/AndroidX) | Android runtime |
| `junit:junit` / `androidx.test.*` | `4.13.2` / `1.1.4` / `3.5.0` | `EPL-1.0` / `Apache-2.0` | Test only (not shipped in release APK) |

---

## 5. System Runtimes & Fonts

- RtMaga bundles **no proprietary font files** and **no browser engine binaries**.
- UI stylesheets (`ui/src/app.css`) reference OS-provided system fonts (`"Segoe UI Variable"`, `"Segoe UI"`, system sans-serif fallback) and rely on the host operating system's native WebView component (WebView2 on Windows, WebKitGTK on Linux, WKWebView on macOS, Android System WebView on Android).
- Optional desktop headless fallback (`chromiumoxide`, Windows/Linux/macOS only) invokes a user-installed Chrome/Edge binary on the host machine when enabled.
