# ADR-0002: Shell architecture on CEF Views with a Chrome-style content view

- Status: accepted (2026-11-06). The sidebar is option B below. ADR-0004
  replaced the single content view with one window per loaded tab.

## Context

CEF offers two runtime styles. Chrome style is built on Chromium's `chrome/`
layer and brings extensions, PDF viewer, print preview, permission bubbles and
autofill. Alloy style is the bare content layer: fewer features, but it allows
off-screen rendering and native parent views.

Hard constraints verified in a spike on macOS (CEF 156):

1. Windowless (off-screen) rendering always uses Alloy style.
2. On macOS, providing a native `parent_view` also forces Alloy style
   (`cef_types_mac.h`). Chrome style is therefore only reachable through the
   CEF Views framework: a `CefWindow` that CEF owns.
3. A Chrome-style window hosts at most one Chrome-style `BrowserView`, but any
   number of Alloy-style ones (`browser_view_impl.cc`, "Cannot add multiple
   Chrome style BrowserViews").
4. The Chrome-style view must be added to the window before any Alloy-style
   view, otherwise it is rejected (the widget already has a theme profile).
5. When our window delegate controls the layout, Chrome's own tab strip and
   toolbar are not shown. The content area is just the page.
6. The Chrome toolbar can be relocated into our own layout
   (`chrome_toolbar_type` = NORMAL, then `browser_view.chrome_toolbar()` after
   `on_window_changed`, attached from a posted task, followed by
   `window.layout()`). This is how extension action buttons reach the user.
7. Frameless windows with inline traffic lights work (`is_frameless`,
   `with_standard_window_buttons`, `titlebar_height`).

Because of (2), every native Rust UI toolkit that wants to own the window
(`winit`+`wgpu`/`vello`, `iced`, `egui`, `slint`) is incompatible with Chrome
style on macOS. Choosing one of them means giving up extensions (ADR-0003).

## Decision

The shell is a CEF Views `CefWindow` owned by our window delegate, laid out
with `BoxLayout`:

```
CefWindow (Chrome style, frameless, inline traffic lights)
├── Sidebar view (see options below)
└── Content panel (flex 1, vertical)
    ├── Chrome toolbar row (relocated, buttons pruned via IsChromeToolbarButtonVisible)
    └── Chrome-style BrowserView (the active tab)
```

`magnetite-core` never sees any of this.

### Sidebar toolkit: two options

**A. CEF Views widgets** (`CefPanel`, `CefLabelButton`, `CefTextfield`,
`CefScrollView`, `CefMenuButton`). Verified working. Zero extra processes.
But: no custom drawing, no drag-and-drop API, no animation, fixed widget set.
A sidebar with favicons, folders, drag reorder and hover states would be a
constant fight.

**B. Web-rendered sidebar** in an Alloy-style `BrowserView` loading our own
bundled HTML/TS (custom `magnetite://` scheme, no network). Verified working side
by side with the Chrome-style content view. Full UI freedom, React and
Tailwind, hot reload during development. Cost: one extra renderer process
(measured +120 MB RSS in a debug build), and discipline about performance:
the sidebar renders only the space in view.

**Chosen: B.** The sidebar is a React app in `crates/ui/web`, served from
`magnetite://ui/`.

### Split view

Only one Chrome-style view per window exists, so a second page cannot sit
next to the first in the same window. ADR-0004 solves this with one window
per page.

## Measurements (debug build of the spike unless noted, M-series MacBook Pro, macOS 27.2)

| Scenario | Time to first window | Processes | RSS total |
|---|---|---|---|
| cefsimple release, warm, about:blank (5 runs) | 279 to 461 ms | 7 | ~835 MB |
| cefsimple release, cold (first run after build) | 2075 ms | 7 | ~850 MB |
| spike, Views sidebar + Chrome content | 280 to 330 ms | 6 | ~650 MB |
| spike, web sidebar + Chrome content | 294 to 348 ms | 7 | ~770 MB |
| spike, + uBlock Origin Lite + relocated toolbar | 640 to 810 ms | 8 | ~1040 MB |

The 500 ms cold-start target is realistic for warm starts; the first launch
after a build pays macOS code-signing and dyld cache costs we do not control.
RSS numbers include all helper processes and are dominated by Chromium itself.

## Lesson from the spike: the "blank window" was the keychain

For most of the spike the content view stayed blank. The cause was not CEF:
every rebuild changes the ad-hoc code signature, macOS then asks for the
"Chromium Safe Storage" keychain item, and the browser process blocks on that
modal dialog before the first navigation. With `--use-mock-keychain` the page
painted 5/5 times. Development builds must pass that switch (or use a
dedicated `root_cache_path` plus a stable signing identity). Release builds
need a real signing identity anyway.
