# Import from other browsers

Magnetite imports spaces, open and pinned tabs, tab groups and bookmarks from
other browsers on the same computer: Arc, Chromium-based browsers (Chrome,
Brave, Edge, Vivaldi, Opera, Chromium), Firefox, LibreWolf, Waterfox and Zen.
LibreWolf and Waterfox are read as Firefox is. It only reads their
files, also while the other browser runs. Nothing is uploaded. Passwords,
cookies and history are never read; extensions and settings do not come
along.

## Using it

- **Settings → Data & privacy → Import from another browser** shows a
  button for every browser found on this computer. A click imports; the
  line below says how many spaces and tabs were added, or why nothing was.
  The command bar's "Import from another browser…" opens this section.
- On the welcome page of the first start the browsers are chosen first and
  imported when the page is left (`first-start.md`).
- An import **adds**. The new spaces follow the existing ones and Magnetite
  switches to the first of them; nothing already in the sidebar changes.
  Importing twice gives the spaces twice. A space whose name is taken gets a
  number ("Work 2").
- Imported favorites join the favorites of their profile; an address that is
  already a favorite there is skipped. Imported Today tabs count as used at
  the time of the import, so auto-archive does not remove them at once.
- **Settings → Data & privacy → Import a bookmarks file** takes the HTML
  file a browser writes with "Export Bookmarks". This is the way for
  Safari (File → Export → Bookmarks…), whose data macOS keeps from other
  apps. The file's bookmarks and folders go to the top of the current
  space's bookmarks, the Reading List as a folder. An address the space has
  already is skipped, so the same file adds nothing a second time; folders
  left empty and addresses that are no web pages are left out. A file of
  more than 16 MiB, one without bookmarks or one nested more than 32
  folders deep is refused.
- Nothing is imported by itself, also not on the first start.
  `--import-arc` takes Arc's sidebar on that start in place of the saved
  one, while the archive, settings and downloads stay. Without a sidebar
  file of Arc nothing changes.

## What each browser brings

| | Spaces | Pinned section | Today | Favorites | Bookmarks |
|---|---|---|---|---|---|
| Arc | its spaces, with icon and colour | pinned tabs and folders, nested as in Arc | its Today tabs | its favorites | none (Arc has none) |
| Chromium-based | one per profile, named after it (a single profile: after the browser) | pinned tabs, each tab group as a folder | the other open tabs of the last session | none | per profile: "Bookmarks bar", "Other bookmarks" and "Mobile bookmarks" as folders |
| Firefox | one, named after the browser | pinned tabs, open and saved tab groups as folders | the other open tabs | none | "Bookmarks toolbar", "Bookmarks menu", "Other bookmarks" and "Mobile bookmarks" as folders |
| Zen | one per workspace, with its emoji icon and primary colour | the workspace's pinned tabs and folders, nested as in Zen | the workspace's other tabs | its essentials | the same folders as Firefox, in the first space |

- Only web pages (`http`, `https`) come along (from Arc also `about:blank`).
  Internal pages, local files and `javascript:` or `data:` addresses do not.
- Pinned tabs and grouped tabs become pinned tabs that return to the
  imported address. A tab the user named in Arc or Zen keeps that name; a
  tab without a title shows its address.
- Bookmarks stay bookmarks, with their names and folders, in the bookmarks
  of their space (`bookmarks.md`).
- Tab groups and bookmark folders with nothing importable are left out. A
  Zen folder and a Firefox bookmark subfolder can stay although empty.
- Profiles: Chromium's first profile maps to Magnetite's default profile and
  each further one becomes a new profile with the same name, so separate
  logins stay separate. Arc's other profiles become "Profile 1", "Profile
  2", …. Importing again adds these profiles again. Firefox and Zen: only
  the default profile is read, into Magnetite's default profile.
- Arc's GitHub Live Folders become GitHub smart folders (my pull requests
  plus review requests on github.com) with their tabs as items. They need a
  connected GitHub account (`smart-folders.md`).

## Files read

| Browser | Found by | Read |
|---|---|---|
| Arc | `StorableSidebar.json` (macOS, Windows) | that file (`arc-sidebar-format.md`) |
| Chromium-based | `Local State` in the data folder | `Local State` (profiles and their names); per profile `Bookmarks` (JSON) and the newest complete `Sessions/Session_*` file, or `Current Session` where there is no `Sessions` folder |
| Firefox | `profiles.ini` in the data folder | the newer of `sessionstore.jsonlz4` and `sessionstore-backups/recovery.jsonlz4`; the newest `bookmarkbackups/bookmarks-*` file |
| Zen | `profiles.ini` in the data folder | `zen-sessions.jsonlz4`, else `zen-sessions-backup/clean.jsonlz4`, else the session files as for Firefox; the newest bookmark backup |

- Data folders are looked up below `~/Library/Application Support` (macOS),
  the home folder (Linux) and `%LOCALAPPDATA%` or `%APPDATA%` (Windows). On
  Linux also where the browser's Flatpak keeps them, `~/.var/app/<app ID>`;
  data from a package wins over a Flatpak's. Snaps are not looked at.
- Chromium session files are command logs (SNSS, versions 1 and 3) that the
  importer replays as Chromium does: tabs of normal windows, each with the
  page it shows. Encrypted session files are not read; the profile's
  bookmarks still come along. Opera's one profile is the data folder itself.
- Firefox and Zen files are JSON, most of them LZ4-compressed (`mozlz4`); no
  SQLite database is read. The default profile is the one the first
  `[Install…]` section of `profiles.ini` names, else the one marked
  `Default=1`, else the first. Private windows, popups and web-app windows
  are skipped. A broken file falls back to the next candidate.
- The files are untrusted input. A file over 256 MiB (Firefox and Zen: 64
  MiB, also when unpacked) is not read. An import with more than 100,000
  spaces, tabs and folders, or with nesting deeper than 32 levels, is refused
  (Chromium bookmark folders below that depth are left out instead). A
  failed import names the file it could not read.
- Before the app adds the result (`AppState::add_import`), every item gets
  a fresh ID, addresses Magnetite would not open are dropped, and colours
  and timestamps are clamped.

## Limits

- Chromium's open tabs come from its last saved session and Firefox's
  bookmarks from its bookmark backup, written about daily. The newest tabs
  or bookmarks can be missing.
- Zen: the sidebar file exists since Zen 1.18; from older versions all tabs
  land in one space. Tabs of a split view import as single tabs, Glance
  previews are skipped, and all bookmarks land in the first space.
- All Arc favorites join the default profile's favorites.
- Safari is not read directly: its open tabs, tab groups and profiles do
  not come along, only the bookmarks of an exported file.

## Command line

`magnetite-import` runs the same importers without the app:

```sh
cargo run -p magnetite-importer -- --list    # browsers found here
cargo run -p magnetite-importer -- --from chrome --format html --output b.html
```

Without `--from` it reads an Arc sidebar file (`--input`, default: Arc's
location). `--format` is `json` (Magnetite's sidebar snapshot, the default)
or `html` (a bookmarks file any browser imports). `--output` names a file
instead of standard output; `--summary` prints the tab counts per space.

## Privacy

Everything happens on this computer. Magnetite reads another browser's files
only when you click its button, and keeps what ends up in the sidebar:
addresses and titles of tabs, when they were created and last used, and
names, icons and colours of spaces and folders. It is stored in `state.json` in Magnetite's data folder.

Code: `crates/importer/src` (`lib.rs`, `arc/`, `chromium/`, `firefox/`,
`export.rs`, `main.rs`), `crates/app/src/state/import.rs` (`add_import`,
`fresh_import`), `crates/importer/src/bookmarks_file.rs`, `crates/app/src/state/bookmarks.rs`
(`add_bookmarks`), `crates/ui/web/src/settings/ImportBrowsers.tsx`.
