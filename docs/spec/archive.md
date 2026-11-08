# Archive and auto-archive

Closing a Today tab does not delete it: the tab moves to the archive, a
local list of closed tabs that can be searched and restored. Today tabs that
are no longer used move there on their own. The archive stays on the device
and is not synced.

## What is archived

- Closing a Today tab: `Cmd+W`, the close button of the row, middle-click,
  or Archive in the context menu.
- **Clear** on the divider above Today, or the command "Archive all Today
  tabs": every Today tab of the current space.
- Auto-archive (below).
- Deleting a space: its pinned and Today tabs. A pinned one comes back as a
  Today tab.

Not archived:

- Favorites and pinned tabs. Closing one unloads its page and keeps the
  item.
- Tabs of a private profile. They close without a record (`spaces.md`).
  They leave no history either (`history.md`).
- Magnetite's own pages (Settings, the welcome page). They just close.
- A closed peek (`peek.md`).

An archived tab leaves its split view, and its page is closed. After
closing the active tab, the next tab of the space becomes active, or the
previous one if it was the last.

## Auto-archive

Once a minute Magnetite archives every Today tab that has been off screen
for longer than the auto-archive period. A tab that was never shown counts
from its creation, an imported tab from the import. The pages on screen (all
of a split's) and the tab each other space returns to are never archived.

| Setting | Options | Default |
|---|---|---|
| Settings → Tabs & archive → "Archive Today tabs after" | 12 hours, 24 hours, 7 days, 30 days | 12 hours |

Auto-archive cannot be turned off. It skips the active tab of each space,
tabs with media, playing or paused, and tabs kept active (`sidebar.md`).
The period is one of the settings that sync (`sync.md`).

## Finding and restoring

- `Cmd+Shift+T`, History → Reopen Closed Tab, or the command "Reopen last
  archived tab" restores the tab archived last.
- The archive view replaces the sidebar. It opens from the archive and
  downloads button in the footer or with the command "Open archive". It
  lists the newest 300 archived tabs by day, with their space, and has a
  search field that matches title and address. A click restores a tab;
  the × on its row removes it from the archive for good.
- The command bar finds archived tabs among its results (`command-bar.md`).
- Settings → Tabs & archive shows the number of archived tabs.

A restored tab returns to the top of Today in the space it came from, or in
the current space if that space is gone. It becomes the active tab and its
record leaves the archive.

The footer button is shared with the bookmarks, the history and the
downloads. A click opens the view chosen in Settings → Tabs & archive (the
archive by default), or the downloads while a download runs; resting on the
button offers all four and, below a line, Settings.

## Data

The archive is the `archive` list in `<data-dir>/state.json`, oldest first.
Each record holds the tab (address, title, a custom name, times of creation
and last use, home page and profile if it had them), the id of its space and
the time it was archived. Icons are not part of it; the view uses the icon
cache (`sidebar.md`). The file is readable by its owner only.

## How long records stay

A record leaves the archive when its tab is restored or removed by hand, or
once it is older than the retention period: the minute tick deletes those.

| Setting | Options | Default |
|---|---|---|
| Settings → Tabs & archive → "Keep archived tabs for" | 30, 60 or 90 days, or a number of days from 1 to 3650 | 90 days |

A typed number is saved on Enter or when the field is left. A shorter period
deletes older records within a minute, and they cannot be brought back. The
period is this device's and does not sync.

Code: `crates/app/src/state/tabs.rs` (`archive_tab`, `delete_space`,
`prune_archive`), `crates/app/src/app/tabs.rs` (`close_tab`, `housekeeping`,
`reopen_closed`), `crates/ui/web/src/ArchiveView.tsx`,
`crates/ui/web/src/LibraryButton.tsx`.
