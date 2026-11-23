# Downloads

Chromium downloads the files; Magnetite lists them in the sidebar and on a
page of their own, tells about each one in the notification panel, and
lets a finished one be opened, dragged out or handled from its menu.

## The list

The list takes the place of the sidebar. It opens from the footer button
for bookmarks, the history, downloads and the archive (its menu, or a click
while a download runs; `archive.md`) and with the command "Open downloads".
It is grouped by day, newest first. "All downloads…" stays
above it and opens the same entries as a page (`magnetite://ui/downloads`)
with a search by file name; its rows do all that the sidebar's rows do.
"Clear" takes finished and failed downloads off the list; the files stay.

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

Each row shows the file's name, cut in the middle so the extension stays,
and its progress or size: "Paused" before the progress of a paused one,
"Failed" or "Canceled" for one that ended without its file, "Deleted" for a
file moved or deleted since.

A click on a finished download opens the file, and the icon at the end of
the row shows it in the file manager. A failed one has a button to try
again while Magnetite still runs.

## Dragging a download out

A finished download drags out of the list as its file, as it would from the
file manager: onto a web page to upload it, into another app or into a
folder, where it lands as a copy. The file stays where it is. A download
that runs, failed, or whose file is gone does not drag.

On macOS Magnetite starts the drag itself, with the file's icon under the
pointer. On Windows and Linux the list's page drags the file with
Chromium's own drag, on X11 and Wayland alike; the page gets the file, never
its path (`security.md`).

Tried by hand:

| | macOS (Apple silicon) | Linux | Windows |
|---|---|---|---|
| A finished download dragged out of the sidebar list | yes | not checked | not checked |

## The menu

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

## Where downloads are saved

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

| What | Where | Until |
|---|---|---|
| The list: file name, place, size, status, start time, whether it was cancelled | `downloads` in `<data-dir>/state.json`, at most 100 entries | the entry leaves the list |
| The address a download came from (for Try again and Copy download link) | memory | Magnetite quits |
| Downloads of private profiles | memory only | Magnetite quits |

Nothing about downloads is synced.

## Not built yet

- Copy file on Windows and Linux; copying a path or link on Linux.

Code: `crates/engine/src/downloads.rs`, `mac/drag.rs`, `trash.rs`;
`crates/app/src/downloads.rs`; `crates/ui/web/src/DownloadsView.tsx`,
`DownloadsPage.tsx`, `downloadDrag.ts` and `Notifications.tsx`.
