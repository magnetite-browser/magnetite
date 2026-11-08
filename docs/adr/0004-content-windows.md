# ADR-0004: One child window per live tab ("content windows")

- Status: accepted (2026-11-06)

## Context

ADR-0002 established that extensions require Chrome style and that Chrome
style on macOS only works inside CEF Views windows. The spike then showed a
harder limit: a `CefWindow` can host exactly **one** Chrome-style browser for
its whole lifetime. `ChromeBrowserWidget::Init` binds the window's widget to
the first Chrome-style `BrowserView`; detaching that view and attaching
another fails with "Cannot add multiple Chrome style BrowserViews", and the
second browser is never created. Re-attaching the original view works.

So "N tabs, one window, swap the view" is impossible with Chrome style.

## Options

1. **Content windows**: a frame window (sidebar, toolbar row, empty content
   area) plus one frameless child `CefWindow` per *loaded* tab, each with its
   own Chrome-style browser, positioned over the content area. Exactly one
   child is shown per pane; the rest are hidden.
2. Alloy style for content: many views per window, but no extensions.
3. One Chrome-style browser, navigate it to the active tab's URL: loses page
   state, background audio, forms. Rejected.

## Decision

Option 1. Verified in a spike:

- Child windows via `CefWindowDelegate::GetParentWindow` are real macOS child
  windows: they move with the frame automatically and never get their own
  Dock/Mission Control presence.
- `Window::Hide` / `Show` + `BringToTop` switches tabs; both browsers stay
  alive (`is_valid` 1/1 after several switches). Moving windows off-screen is
  clamped by macOS and does not work; hiding does.
- Frame resize/move: `OnWindowBoundsChanged` re-applies the content rect
  (`content_panel.bounds_in_screen()`) to every child. Verified after a
  move+resize.
- Split view falls out naturally: several visible children side by side.
  A peek is also just a window.

## Consequences

- Only loaded tabs own a window. Tab discarding (unloading inactive tabs)
  is therefore not an optimisation but part of the core design; the
  sidebar must render fine with 500 items while only a handful have windows.
- Focus and keyboard routing cross window boundaries: the frame must forward
  shortcuts (`Cmd+T`, `Cmd+W`, …) when a child has focus, via
  `CefWindowDelegate::OnKeyEvent` / accelerators registered on each child.
- macOS fullscreen and Spaces (the OS ones) need testing with child windows.
- Window-level animations (space switch, peek) are our responsibility.
- Windows and Linux can use the same model; on Windows a native parent HWND
  would also allow Chrome style directly, which may be simpler there.
