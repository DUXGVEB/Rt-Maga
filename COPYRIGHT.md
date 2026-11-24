# Copyright and Ownership Notice

## 1. Original Project Materials

Except where expressly attributed to third-party licensors, all original source
code, software design, documentation, user interface components, original icons,
and logos in the **RtMaga** repository and **Official Builds** (targeting
**Windows**, **Linux**, **macOS**, and **Android**) are proprietary original
works owned by:

- **Copyright Notice:** `Copyright (c) 2025-2026 RtMaga (DUXGVEB). All rights reserved.`
- **Official Repository:** `https://github.com/DUXGVEB/Rt-Maga`

Original RtMaga materials include, without limitation:
- Rust core source-parsing, detection, diagnostic, and resolver engine
  (`crates/mangacore/`), except third-party crate dependencies;
- Tauri application backend, database migrations, local cache, LAN sync, image
  proxy, updater, and platform integration (`src-tauri/src/`,
  `src-tauri/migrations/`, `src-tauri/gen/android/app/src/main/java/com/rtmaga/reader/`);
- Svelte/TypeScript/CSS frontend interface (`ui/src/`);
- Project icons and logo artwork (`ui/src/assets/logo.png`, `src-tauri/icons/`,
  and launcher assets under `src-tauri/gen/android/app/src/main/res/`);
- Project documentation (`README.md`, `CHANGELOG.md`, `LICENSE`,
  `COPYRIGHT.md`, `NOTICE`, `CONTRIBUTING.md`, `THIRD-PARTY-LICENSES.md`).

All original project materials are governed by the **RtMaga Source-Available
License (Version 1.0)** set forth in [LICENSE](LICENSE). They are **not**
released under MIT or any OSI Open Source license.

---

## 2. Third-Party Components and External Assets

Third-party libraries, compiler/runtime packages, fonts, and Android framework
libraries used alongside RtMaga remain the intellectual property of their
respective authors and copyright holders under their respective open-source or
vendor terms:

- Rust crates listed in `Cargo.lock` and Android Gradle / npm packages are owned
  by their respective upstream maintainers (detailed in
  [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md) and [NOTICE](NOTICE)).
- Platform WebView runtimes (Microsoft Edge WebView2 on Windows, WebKitGTK on
  Linux, WKWebView on macOS, Android System WebView on Android) and OS UI fonts
  (e.g., `Segoe UI Variable`, `Segoe UI`, system sans-serif) belong to their
  respective OS vendors and are used only via platform APIs at runtime.
- RtMaga claims zero ownership over user-supplied links or external website
  content accessed by users on their own devices.

---

## 3. Items Requiring Maintainer Confirmation

The following ownership and legal metadata could not be verified from repository
history and should be confirmed or supplemented by the maintainer:
1. Full legal name of the copyright holder (individual vs. registered organization)
   if `RtMaga / DUXGVEB` needs to be expanded for formal legal notices.
2. Designated legal/licensing contact email address (currently directed via
   GitHub repository issues/channels).
3. Governing law and exclusive venue/jurisdiction clause, if desired.
4. Attribution/origin proof for `ui/src/assets/logo.png` and icons under
   `src-tauri/icons/` (assumed original project branding based on repository
   tracking).
