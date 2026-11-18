# Web panels

A web panel is a favorite or a pinned tab that opens over the page in
front instead of taking its place: a translator, a password manager, a
bookmark manager, a chat. Below, "favorite" stands for both. It comes out at the sidebar's edge of the page area, the tab
behind it stays as it is, and a second click puts the panel away again.

## Making one

- "Open as panel" in the context menu of a favorite or a pinned tab
  makes it a web panel, and opens it. A check mark shows it in the menu;
  the same entry makes it a plain one again.
- Today tabs cannot be web panels. A web panel that is unpinned, moved
  to Today, or closed for good with its space, is a plain tab again; one
  that moves between the favorites and Pinned stays a panel.
- A small panel glyph (a frame with its side filled) marks a web panel:
  in the corner of a favorite's icon, at the end of a pinned tab's row.
  It takes the space's colour while the panel is open.
- A pinned tab that is a web panel has no number for `Cmd+1` … `Cmd+9`
  and is passed over when going from tab to tab.

## Behaviour

- A click on the favorite opens its panel; a click on the favorite of the
  open panel closes it. Choosing the favorite another way, in the command
  bar for one, opens the panel too.
- A round button with a cross beside the panel's top corner, on the
  side of the pages, closes it, whether or not panels close by
  themselves, and always in the same place. On Linux it is part of the
  page beside the panel (the first of a split), since a window of its
  own would end up behind that page.
- "Open or close the web panel" (`Cmd+Option+P`, `Ctrl+Alt+P` outside
  macOS; Settings → Shortcuts, the View menu and the command bar) closes
  the open panel, or opens the one used last: of the favorites and the
  pinned tabs of the space in front. The page may claim the key
  (`shortcuts.md`).
- The panel is 420 points wide, less in a narrow window, and as high as
  the page area. On macOS it lies over the pages with a shadow, and they
  keep their size under it, as does the close button. On Linux it sits
  beside them: the pages (a split too) take the room that is left, with
  a gap between. There every
  page is a window of its own to the window manager, which puts a page
  that is shown or clicked on top, so a panel over a page would end up
  behind it.
- Dragging the panel's far edge (on Linux the gap beside it) changes its
  width, between 280 points and all but 80 points of the page area. In a
  window too narrow for both, the room left
  for the pages counts and the panel is narrower than 280 points. Each
  panel keeps its own width; a double click there gives it the usual one
  again.
- There is one panel per window at a time. Opening another favorite's
  panel hides the open one.
- The panel stays open while you switch tabs and spaces, open a split or
  a peek. A peek shows above it.
- With Settings → General → "Web panels close by themselves" (off by
  default) the panel closes as soon as a page behind it gets the
  keyboard: a click into the page, or choosing another tab.
- A web panel is never the tab in front: it has no place in the page
  area, in a split or in the `Ctrl+Tab` round. A favorite that becomes a
  panel while it is in front leaves the page area, and its space shows
  the tab it has next.
- The panel runs in the profile of its favorites list or its space,
  like every favorite and pinned tab, and stays loaded once opened: closing the panel only hides
  it, so a half-written text is still there the next time. "Close" in
  the favorite's context menu unloads the page and takes it back to its
  home page.
- Like every favorite the panel is locked to its site. A link that
  leaves the site, or asks for a new tab or window, opens as a Today tab
  of the space in front.
- While the keyboard is in the panel, reload, back and forward keys,
  zoom, find, Copy Link, the capture tool, print and developer tools act
  on its page, and `Cmd+W` closes the panel, not the tab behind it.

## Mobile view

- "Mobile view" in a web panel's context menu shows its page as on a
  phone: sites see a mobile browser's identity (Chrome on Android) and
  serve the page they have for phones. The page loads again when it is
  turned on or off.
- It is kept per panel, and ends when the favorite is a plain one again.
- Only the identity changes. Touch input is not imitated, and sites that
  lay out by width alone show their narrow layout in a panel anyway.

## Data

Whether a favorite is a web panel is `panel` on the favorite in
`<data-dir>/state.json`, with its width (`panel_width`, when dragged) and
`panel_mobile`; they sync with the sidebar (`sync.md`). Which panel is
open is kept in memory only: after a restart none is.

## Not built yet

- A key for one certain panel.
- A panel for a page that is not a favorite.
- The sidebar's back, forward and reload buttons and the address row
  stay with the tab in front while a panel is open.

Code: `crates/app/src/app/panels.rs` (`open_web_panel`, `settle_web_panel`),
`crates/app/src/state/panels.rs` (`settle_panels`, `latest_web_panel`),
`tabs.rs` (`activate_tab`), `crates/engine/src/content.rs`
(`show_web_panel`, `web_panel_rect`, `WEB_PANEL_DOCKED`,
`resize_web_panel`), `mac/chrome.rs` (`set_panel_close`, the close button
on macOS), `panel_close.rs` (the close button elsewhere),
`device.rs` (`set_mobile_view`),
`crates/ui/web/src/Rows.tsx` (the favorite's click and menu entry),
`Empty.tsx` (`PanelResizer`, the grip on Linux, and the close button
while no page is beside the panel).
