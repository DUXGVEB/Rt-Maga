# Contributing to RtMaga

Thank you for your interest in reporting bugs, suggesting improvements, or
contributing patches to **RtMaga**.

Because RtMaga is a **source-available** project rather than OSI Open Source,
please review the expectations and contribution licensing terms below before
submitting code, documentation, or assets.

---

## 1. How You Can Contribute Safely

- **Bug Reports & Diagnostics:** When reporting source loading or platform
  bugs, export a sanitized diagnostic report from Settings or include clean
  reproduction steps. Never post passwords, session cookies, private keys, or
  copyrighted scraped HTML dumps/images in issues or pull requests.
- **Code & Documentation Pull Requests:** Keep changes scoped, preserve site-agnostic
  tests using synthetic fixtures only (verified by the repository's pre-commit
  audit checks), and ensure code passes formatting, linting, and tests:
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  - `cargo test --workspace`
  - `npm run ui:check` and `npm run ui:build`

---

## 2. Contributor Representations

By submitting a pull request, patch, issue snippet, asset, or documentation
change ("Contribution") to RtMaga, You represent and warrant that:

1. **Originality & Authority:** The Contribution is Your own original work (or
   You hold all necessary rights, permissions, and employer/third-party
   clearances to submit it), and does not infringe any third-party copyright,
   trademark, patent, trade secret, or confidentiality obligation.
2. **No Unapproved Third-Party Dependencies:** You have not introduced new
   third-party libraries, fonts, icons, or assets incompatible with RtMaga's
   licensing and distribution model without explicitly documenting them.
3. **No Proprietary/Copyrighted Media:** You have not embedded scraped comic
   images, copyrighted site HTML dumps, private credentials, or signing keys.

---

## 3. Licensing of Accepted Contributions

RtMaga is distributed under the **RtMaga Source-Available License (Version 1.0)**
in [LICENSE](LICENSE), not under an OSI Open Source license.

By submitting a Contribution for inclusion in RtMaga, You grant to the
Copyright Holder (`RtMaga / DUXGVEB`) a **perpetual, irrevocable, worldwide,
royalty-free, non-exclusive, transferable, and sublicensable license** to:

- Use, reproduce, modify, adapt, compile, distribute, and display Your
  Contribution as part of the RtMaga Covered Work (across source code and
  Windows, Linux, macOS, and Android builds) under `LICENSE` or future
  revisions of RtMaga's license; and
- Retain credit/attribution for accepted contributions in project commit history
  or release notes where appropriate.

You retain ownership of Your standalone pre-existing ideas and code created
independently outside RtMaga. Submitting a Contribution does not obligate the
Copyright Holder to accept, merge, or ship it.
