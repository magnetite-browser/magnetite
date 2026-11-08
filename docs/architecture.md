# Architecture overview

```
┌──────────────────────────── Magnetite.app ────────────────────────────┐
│ frame window (CEF Views, Alloy style, frameless)                          │
│ ┌──────────────┐ ┌──────────────────────────────────────────────────────┐ │
│ │ sidebar view │ │ content panel (empty; only defines the rectangle)    │ │
│ │ Alloy browser│ │   ┌────────────── content window (child) ─────────┐  │ │
│ │ magnetite://ui/  │ │   │ Chrome-style browser = one loaded tab         │  │ │
│ │ React + TS   │ │   │ (extensions, PDF viewer, permissions)         │  │ │
│ └──────────────┘ │   └───────────────────────────────────────────────┘  │ │
│                  └──────────────────────────────────────────────────────┘ │
└───────────────────────────────────────────────────────────────────────────┘
```

- One **content window per loaded tab** (ADR-0004). Only the active tab (or
  the split-view panes, or the peek) is visible; the rest are hidden child
  windows. Tabs hidden for 30 minutes lose their window; Today tabs unused
  for 12 hours are archived.
- The **sidebar** is our own web app. It receives the whole UI state as JSON
  after each change (`window.__magnetite.setState`) and sends commands through
  CEF's message router (`window.magnetiteQuery`). Commands are handled
  asynchronously on the UI thread (creating browsers inside the IPC callback
  deadlocks).

## Crates

| Crate | Depends on | Role |
|---|---|---|
| `magnetite-core` | serde | Domain model (`SidebarSnapshot`, spaces, tabs, folders) and pure sidebar operations. No I/O, no CEF. |
| `magnetite-importer` | core | Importers for Arc (`StorableSidebar.json`), Chromium-based browsers, Firefox and Zen; exporters (magnetite JSON, bookmarks HTML); `magnetite-import` CLI. |
| `magnetite-engine` | cef | Everything CEF: bootstrap, frame and content windows, scheme handler, router, shortcuts. Emits `EngineEvent`s to a delegate, never calls it re-entrantly. |
| `magnetite` (app) | all | `AppState` (snapshot + runtime + archive + panes + peek), the `EngineDelegate` that maps events and UI commands onto state and engine calls, persistence (`state.json`). |
| `crates/ui/web` | React, Vite, Tailwind | The sidebar. Built to `dist/`, bundled as `Resources/ui`. |

The app crate is laid out by feature. `app.rs` holds the app itself and
`app/` what it does: `commands.rs` (what the sidebar can ask for), `events.rs`
(what the engine reports), then one file per feature (`tabs.rs`,
`split_view.rs`, `peek.rs`, `site.rs`, `screenshots.rs`, `smart_folders.rs`,
`syncing.rs`, `extensions.rs`, `shortcuts.rs`, `addresses.rs`). `state.rs`
holds the state's types and `state/` its logic the same way (`tabs.rs`,
`folders.rs`, `profiles.rs`, `splits.rs`, `settings.rs`, `persist.rs`,
`adopt.rs` for sync, `import.rs`), each file with its tests.

## Event flow

1. User clicks a tab row → UI sends `{cmd: "activate_tab", id}`.
2. App: set active tab, `present()` computes the visible panes, creates
   missing content windows (`Engine::create_content`), shows/hides windows,
   serialises `UiState` back to the sidebar.
3. CEF reports title/URL/favicon/loading for a content window → engine maps
   the browser to its `ContentId` → `EngineEvent::*Changed` → app updates the
   tab → pushes state.

Shortcuts: Chrome-style windows see keys first (`OnPreKeyEvent`) and Chrome
commands (`OnChromeCommand`, e.g. IDC_NEW_TAB) are intercepted; both become
`EngineEvent::Shortcut`. The sidebar handles its own keys in JS and sends
`{cmd: "shortcut"}`.

## Persistence

`<data-dir>/state.json`: sidebar snapshot, active space/tab, archive, panes,
favicon URLs. Written atomically after every change (small file). CEF's own
profile (cookies, cache, extensions, history) lives in `<data-dir>/cef`.

## Development loop

```
scripts/bundle.sh                       # builds UI + binaries + .app
target/bundle/Magnetite.app/Contents/MacOS/magnetite \
  --mock-keychain --data-dir /tmp/magnetite-dev [--ui-url http://localhost:5173]
```

`MAGNETITE_LOG=1` prints every engine event and UI command. For end-to-end
checks, start a development build with `MAGNETITE_DEVTOOLS_PORT=9333` (plus a
throw-away `--data-dir`, `--no-import` and `--mock-keychain`) and drive its
pages, the sidebar included, through the Chrome DevTools Protocol
(`Runtime.evaluate`, `Page.captureScreenshot`): no injected mouse or keyboard
input (`CONTRIBUTING.md`, `tools/README.md`).
