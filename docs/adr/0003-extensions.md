# ADR-0003: Chrome extensions via Chrome style

- Status: accepted (2026-11-06)

## Context

People only switch browsers if their password managers and content blockers
keep working. CEF removed the Alloy extension API in M128; since then
extensions exist only in Chrome style browsers/windows.

## What was verified (CEF 156.0.15, macOS arm64)

- `chrome://extensions` renders, including the Chrome Web Store link and the
  developer-mode toggle.
- uBlock Origin Lite 2026.1103.1743 (Manifest V3) loaded with
  `--load-extension=<unpacked dir>`: listed as enabled, toolbar action icon
  present, declarativeNetRequest active. adblock-tester.com: 100/100 with the
  extension, 46/100 without.
- The extension action button survives relocating the Chrome toolbar into our
  layout (ADR-0002, point 6).

## Verified later (2026-11-06)

- Installing from the Chrome Web Store inside Magnetite works (Bitwarden).
- Extension popups open for the right page: a test extension's popup reads
  the page in front through `chrome.tabs.query({active, currentWindow})`.
  They open from our own sidebar buttons and Extensions menu
  (`docs/spec/extensions.md`, ADR-0007).
- `chrome.tabs.create` from an extension opened a plain Chrome window; such
  windows now become Magnetite tabs (`stray.rs`).

## Not yet verified

- Behaviour for Alloy-style views (our sidebar); not needed there.

## Decision

Build on Chrome style and treat extension support as a first-class feature.
Our own UI (sidebar buttons, Extensions menu) opens extension actions; the
Chrome toolbar row is an option. See `docs/spec/extensions.md`.
