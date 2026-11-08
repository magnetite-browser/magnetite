# ADR-0001: Use `cef-rs` (tauri-apps) as the CEF binding

- Status: accepted (2026-11-06)

## Context

The browser embeds Chromium through CEF (project rule: no Chromium fork). We
need Rust bindings that track CEF releases closely, work on macOS arm64 today,
and expose the Views framework (required for Chrome style, see ADR-0002).

## Evaluation of `cef-rs`

| Aspect | Finding |
|---|---|
| Version | `cef = "156.4.0+156.0.26"` binds CEF 156.0.26 / Chromium 156.0.8078.88 |
| Cadence | Bindings bumped within days of each CEF release (156.0 on 10-23, 156.1 10-24, 156.2 10-25, 156.3 11-01); automated via Renovate + release-plz |
| Platforms | Linux, macOS, Windows, x86_64 and arm64; macOS arm64 verified here |
| API surface | Generated from the CEF C API, including Views, Chrome style hooks (`chrome_toolbar_type`, `chrome_toolbar`, `is_chrome_toolbar_button_visible`, `on_chrome_command`), OSR with accelerated paint |
| Build | Needs `cmake` + `ninja` (builds `libcef_dll_wrapper`); one-time 350 MB binary download via `export-cef-dir`; incremental build of an example ~30 s |
| Bundling | `bundle-cef-app` produces a correct macOS `.app` with the five helper apps |
| License | Apache-2.0 OR MIT |
| Maintainers | tauri-apps org (wusyong, wravery, csmoe); 489 stars, 11 open issues at time of writing |

Quirks found during the spike (documented so nobody re-discovers them):

- `wrap_*_delegate!` macros: the unit-struct form (`struct Foo;`) silently drops
  the `impl ViewDelegate` block. Always use `struct Foo {}`.
- `label_button_create(None, ..)` returns `None`; pass a `ButtonDelegate`.
- `bundle-cef-app` hard-links the cargo output into the bundle. Rebuilding in
  place invalidates the ad-hoc code signature and macOS kills the process at
  `dyld` start with no output. Delete the bundle before re-bundling (or
  `codesign --force --deep --sign -`).
- Chrome style stores cookies encrypted via the macOS keychain. An unsigned or
  freshly re-signed dev binary triggers a modal keychain prompt that blocks
  the browser process. Pass `--use-mock-keychain` in development.
- Delegate structs are cloned per CEF handle; shared mutable state must live
  in `Arc<Mutex<_>>`, not `RefCell`.

## Alternatives considered

- Hand-written FFI over the CEF C API: full control, but the surface is huge
  (bindings file is ~440k lines generated) and we would re-implement what
  cef-rs already automates.
- `bunlet-cef-native`: NAPI/Node oriented, not a Rust-native API.
- `welding` / `bevy_cef`: OSR-only integrations, and OSR forces Alloy style
  (no extensions, ADR-0003).

## Decision

Adopt `cef-rs`. Pin the exact `cef` version in `Cargo.toml`; a CEF security
update is a one-line bump plus CI run. Contribute fixes upstream where we hit
binding bugs rather than forking.

## Consequences

- CI caches the CEF download (350 MB) per system and CEF version
  (`.github/actions/setup`).
- Developers need CMake and Ninja; `CEF_PATH` keeps one copy of CEF for all
  builds (`docs/development/building.md`).
