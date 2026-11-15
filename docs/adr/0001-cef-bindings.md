# ADR-0001: Use `cef-rs` (tauri-apps) as the CEF binding

- Status: accepted (2026-11-06), amended 2026-11-15: we build from our own
  fork (see "Our fork")

## Context

The browser embeds Chromium through CEF (project rule: no Chromium fork). We
need Rust bindings that track CEF releases closely, work on macOS arm64 today,
and expose the Views framework (required for Chrome style, see ADR-0002).

## Evaluation of `cef-rs`

| Aspect | Finding |
|---|---|
| Version | `cef = "156.4.0+156.0.26"` bound CEF 156.0.26 / Chromium 156.0.8078.88 when this was written |
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

Adopt `cef-rs`. Pin the exact `cef` version in `Cargo.toml`. Contribute fixes
upstream where we hit binding bugs. What a CEF update takes is described
under "Our fork".

## Our fork

The crate on crates.io binds the CEF version upstream last published, and
`cef-dll-sys` refuses a newer CEF than that. Magnetite builds its own CEF
([magnetite-cef](https://github.com/magnetite-browser/magnetite-cef)), so the
CEF version would be upstream's to choose, not ours. We therefore build from
[our fork](https://github.com/magnetite-browser/cef-rs), branch `magnetite`,
pinned by commit in `crates/engine/Cargo.toml`.

The fork is upstream plus the bindings for the CEF version we build. Moving
to a new CEF version:

1. Build and release it in magnetite-cef.
2. In the fork, merge upstream, set the version in the workspace's
   `Cargo.toml` (`<crate version>+<CEF version>`) and regenerate the bindings:
   `cargo run -p update-bindings -- -b -d -t <target>` for each target.
3. Here, pin the fork's commit in `crates/engine/Cargo.toml` and the release
   in `scripts/cef.sh`.

Within one CEF branch the C API rarely changes, so step 2 changes little
more than version numbers. A new CEF branch can change interfaces, which the
hand-written part of `cef-rs` has to follow; there we take upstream's work
once it has it. Fixes for binding bugs still go upstream.

## Consequences

- CI caches the CEF download (350 MB) per system and CEF version
  (`.github/actions/setup`).
- Developers need CMake and Ninja; `CEF_PATH` keeps one copy of CEF for all
  builds (`docs/development/building.md`).
