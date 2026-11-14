# Split view

Split view shows two to four tabs at once in the page area: side by side,
stacked, or both. Every page stays a normal tab with its own history, and
extensions work in each. A split is saved, shows as one row in the sidebar,
and comes back on screen whenever one of its tabs is opened.

## Layout

A split is a small tiling tree. A leaf is a page; an inner node divides its
area into a row (pages side by side) or a column (pages stacked).

- A new page goes left of, right of, above or below an existing page and
  takes half of that page's space. Mixed layouts follow from this, such as
  one tall page beside two stacked ones.
- A split holds at most four pages. A fifth is refused with the notice
  "Split view holds up to four pages".
- All pages of the split on screen count as in use: none of them is
  unloaded or auto-archived.
- Pages are 8 px apart. Sizes are relative, so the layout follows the
  window when it is resized.
- A tab is in at most one split. Adding it to another split takes it out of
  the first, and a split left with one page ends.
- Several splits can exist. The one that holds the active tab is on screen.
- Today tabs, pinned tabs and favorites can share a split. Each keeps its
  place in the sidebar.

## Adding a page

| Way | Result |
|---|---|
| `Cmd+Shift+=`, File → New Split View…, or New Split in the sidebar's + menu | The command bar opens; the pick goes right of the current tab (with no tab open it opens as a tab) |
| Command bar: "Add right split…", "Add left split…", "Add top split…", "Add bottom split…" | The same, on that side |
| "Open in split view" in a tab's menu, or `Option`+click on a tab row | That tab goes right of the current tab |
| Drag a tab onto the page area | Drop zones, see below |
| Drag a tab onto the middle of another tab's row | The two tabs form a split |
| "Open in split view" on a peek | The peeked page becomes a tab right of the current tab |

In the command bar, picking an open tab adds that tab; an address or a
search opens as a new Today tab beside the current one. "Open in split
view" is offered while a tab is open, the chosen tab is not on screen and
fewer than four pages show.

## Drag and drop

- Dragging a tab or favorite out of the sidebar shows drop zones over the
  page area: the visible pages, dimmed. The half of a page nearest to the
  pointer lights up with "Open <title> here"; dropping opens the tab on that
  side of that page. With four pages on screen the zones say that the split
  is full and take no drop.
- In the sidebar, the middle of a tab row is a drop target too. While the
  pointer is over it the row previews the pair, the dragged tab on the half
  under the pointer. On a split row the tab lands left or right of the page
  under the pointer; a full split row takes no drop. The upper and lower
  edge of a row still reorder.

## In the sidebar

- A split is one row with its pages side by side, in reading order. The row
  stands where the first of its pages that belongs to the current space
  would be; the other pages have no row of their own there.
- A split on screen whose pages all live elsewhere (favorites, other
  spaces) shows above the Today tabs.
- While the split is on screen the row is lit and the focused page is
  highlighted.
- Click a page to show the split with that page focused. × or a middle
  click takes the page out of the split; the tab stays open. The page's
  menu has "Remove from split view", "Separate all tabs" and, while the
  split is on screen, "Even out sizes".
- The row drags like a tab to reorder.

## Focus and sizes

- Clicking into a page makes it the active tab, so the address row, the
  site panel and page shortcuts act on it. Developer tools docked to a page
  count as that page. On macOS and Windows the focused page has a 2 pt
  outline in the space colour.
- On macOS and Windows each gap holds a divider (on macOS with a grip that
  appears on hover).
  Dragging it resizes the two neighbours; each keeps at least 200 px. A
  double-click gives all pages of that row or column the same size. "Even
  out sizes" in the page's menu and "Even out split sizes" in the command
  bar do this for the outermost row or column.
- A click on a page or a divider counts at once: no extra click is needed
  to activate its window first.

## Closing

- `Cmd+W` closes the focused page like any tab. A Today tab moves to the
  archive and leaves the split. A pinned tab or favorite is unloaded and
  leaves the split too; its item stays in the sidebar.
- "Separate all tabs" in the page's menu or "Separate split tabs" in the
  command bar ends the split. Its pages stay open as separate tabs.

## Data

Splits are saved in `<data-dir>/state.json` under `splits`: one tree per
split, with tab ids as leaves and relative sizes per row or column. New
sizes are saved when a divider drag ends. Splits are not synced; a split
that loses a tab to a synced sidebar ends. Each page is its own content
window, tiled over the page area
([ADR-0004](../adr/0004-content-windows.md)).

## Not built yet

- Dragging a link from a page into a split.
- Rearranging the pages of a split by dragging.
- Pinning a split as one item.
- A shortcut that removes the focused page from its split.

Code: `crates/core/src/split.rs` (the tree), `crates/app/src/state/splits.rs`
and `crates/app/src/app/split_view.rs`, `crates/engine/src/content.rs`,
`splitui.rs` and `mac/chrome.rs` (tiling, dividers), and `Rows.tsx`,
`SplitDrop.tsx` and `dnd.ts` in `crates/ui/web/src` (sidebar row, drop
zones).
