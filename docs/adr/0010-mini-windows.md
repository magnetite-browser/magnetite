# ADR-0010: Mini windows are content windows that start top-level

- Status: accepted (2026-11-11), not yet checked in a running build

## Context

A mini window (`docs/spec/mini-window.md`) shows one page outside the main
window, and "Open in <space>" must turn it into a tab without reloading the
page. ADR-0004 makes every page a frameless child window of the frame, with
exactly one Chrome-style browser per `CefWindow` for the window's whole
lifetime: a page's browser view cannot move into another window.

## Options

1. A mini window is a content window created without a parent, with a row
   of CEF Views widgets (label and menu buttons) above the page. Moving it
   into a space removes the row and attaches the same window to the frame
   through AppKit (`addChildWindow:`).
2. Move the page's browser view from the mini window into a new content
   window. ADR-0004's spike saw a second Chrome-style view fail in a
   window; moving a Chrome-style view between windows is not supported.
3. Close the mini window and load its address in a new tab. Simple, but the
   page reloads and loses its state (forms, media, scroll), the opposite of
   what a kept peek promises.
4. The row as our web UI (an Alloy view next to the page). One more
   renderer per mini window and a bridge round trip for each button; Views
   widgets need neither.

## Decision

Option 1. The content window's delegate takes an optional mini rectangle:
then it has no parent, opens there, shows itself with a shadow, and the row
is inserted above the page after the page's view (the Chrome-style view
must come first). `Engine::mini_to_tab` drops the row, hides the window and
shapes it like the others; `show_content` and `show_split` attach it to the
frame again each time they show it (`keep_attached`), because CEF does not
know of the new parent and lets go of it when the window is hidden.

## Consequences

- CEF and Chromium treat a kept mini window's page as a top-level window
  for its lifetime. Anything that relies on the Views parent (bubbles
  anchored to the frame, window activation order) may behave differently
  for such a tab than for others. To be checked in a running build:
  hiding and showing such a tab, moving and resizing the frame, full
  screen, minimising.
- The frame's layout code must skip mini windows (`reposition_contents`,
  the drop zones, the status pill, docking DevTools).
- On other systems the row works as is; attaching to the frame needs the
  platform's equivalent of `addChildWindow:` (no-op today).
