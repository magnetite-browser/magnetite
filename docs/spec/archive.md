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
| Settings → Tabs and archive → "Archive Today tabs after" | 12 hours, 24 hours, 7 days, 30 days, Never | 12 hours |

With "Never" no tab is archived by itself; Today tabs stay until they are
closed. Their pages are still unloaded (`sidebar.md`). Auto-archive skips the active tab of each space,
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
- Settings → Tabs and archive shows the number of archived tabs.

A restored tab returns to the top of Today in the space it came from, or in
the current space if that space is gone. It becomes the active tab and its
record leaves the archive.

The footer button is shared with the bookmarks, the history and the
downloads. A click opens the view chosen in Settings → Tabs and archive (the
archive by default), or the downloads while a download runs; resting on the
button offers all four and, below a line, Settings.

The downloads list every file a page downloads, except the package of an
extension being installed from the Chrome Web Store: that is part of the
install, as in Chrome, and Chromium deletes it once the extension is in.

A download also shows as a card in the notification panel at the top right
of the page: while it runs, and for six seconds after it ended or failed. At
most three show, the newest first. The pointer on the panel holds the cards
that are there, and they stay two more seconds after it has left. A card
that is gone does not come back: when the panel opens for something else (a
new version, a permission a page asks for), the pointer on it brings no
earlier download with it. The download list keeps them all.

A click on a finished download opens the file, and the icon at the end of
the row shows it in the file manager. Dragging the row takes the file
along, as dragging it from the file manager would: onto a web page to
upload it, into another app or into a folder, as a copy. "All downloads…"
stays above the list and opens the same entries as a page
(`magnetite://ui/downloads`) with a search by file name; its rows do the
same.

A right click offers what fits the download as it is:

| Download | Menu |
|---|---|
| Running | Pause or Resume, Cancel, Copy download link |
| Finished | Open, Show in Finder (Show in folder), Copy file, Copy file path, Copy download link, Remove from list, Move to Trash (Move to Recycle Bin on Windows) |
| Failed or cancelled | Try again, Copy download link, Remove from list |

- Open is left out for a file that runs code when opened (`security.md`).
  A file moved or deleted since keeps Show in Finder (its folder), the link
  and Remove from list.
- Copy file is built for macOS; Copy file path and Copy download link for
  macOS and Windows.
- The link and Try again are there while Magnetite runs: the address a
  download came from is not saved.
- Remove from list takes the entry off and leaves the file. Move to Trash
  leaves the entry, which then says "Deleted"; the file can be put back
  from the trash.
- A paused download says "Paused" before its progress, a cancelled one
  "Canceled".

### Where downloads are saved

| Setting | Values | Default |
|---|---|---|
| Settings → General → "Save downloads to" | the Downloads folder of the system, or a folder chosen in the system's dialog | the system's |
| Settings → General → "Ask where to save each file" | off, on | off |

- "Choose…" opens the system's dialog for a folder. Only a folder chosen
  there is used: the page cannot name one, it can only take the choice
  away ("Reset", back to the system's folder).
- With "Ask where to save each file" every download opens a dialog for the
  file's name and folder.
- Both hold for every profile and take effect at once. "Save as" for a
  page keeps starting in the folder it was last used with: that is another
  preference of Chromium's, which Magnetite leaves alone.
- A chosen folder that is not there is said so in Settings.
- Neither is synced: a folder is this computer's.

Chromium saves the files; the settings are its preferences
(`download.default_directory`, `download.prompt_for_download`).

A folder chosen in the dialog that is gone, or no folder, by the time the
app looks is refused, and Settings says that it did not work.

## Data

The archive is the `archive` list in `<data-dir>/state.json`, oldest first.
Each record holds the tab (address, title, a custom name, times of creation
and last use, home page and profile if it had them), the id of its space and
the time it was archived. Icons are not part of it; the view uses the icon
cache (`sidebar.md`). The file is readable by its owner only.

## How long records stay

A record leaves the archive when its tab is restored or removed by hand, or
once it is older than the retention period: the minute tick deletes those.
With "Forever" there is no period, and records only leave by hand.

| Setting | Options | Default |
|---|---|---|
| Settings → Tabs and archive → "Keep archived tabs for" | 30, 60 or 90 days, a number of days from 1 to 3650, or Forever | 90 days |

A typed number is saved on Enter or when the field is left. A shorter period
deletes older records within a minute, and they cannot be brought back. The
period is this device's and does not sync.

Code: `crates/app/src/state/tabs.rs` (`archive_tab`, `delete_space`,
`prune_archive`), `crates/app/src/app/tabs.rs` (`close_tab`, `housekeeping`,
`reopen_closed`), `crates/ui/web/src/ArchiveView.tsx`,
`crates/ui/web/src/LibraryButton.tsx`.
