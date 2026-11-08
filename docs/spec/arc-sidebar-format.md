# Arc sidebar file format (import source)

Arc is one of the browsers Magnetite imports from (`import.md`). Arc keeps
its sidebar in a JSON file, `StorableSidebar.json`. This page describes that
file only as far as Magnetite's importer reads it, from the user's own copy
on their own computer; no Arc code or assets are involved. The importer
treats the format as unstable: fields it does not know are ignored, missing
ones get defaults, and the `version` field is not checked. All examples are
synthetic.

Where the importer looks:

- macOS: `~/Library/Application Support/Arc/StorableSidebar.json`
- Windows: `%LOCALAPPDATA%\Packages\TheBrowserCompany.Arc_ttt1ap7aakyb4\LocalCache\Local\Arc\StorableSidebar.json`

## Top level

```json
{ "sidebar": { "containers": [
    { "global": {} },
    { "items": ["I1", { "id": "I1" }, "I2", { "id": "I2" }],
      "spaces": ["S1", { "id": "S1" }],
      "topAppsContainerIDs": [{ "default": true }, "C3"] } ] } }
```

The first container that has `items` is used. `items` and `spaces` alternate
between an id and the object with that id; the importer keeps the objects
and drops the strings in between.

## Items

Every sidebar row is an item. What it is follows from `data`:

```json
{ "id": "I1", "childrenIds": [], "title": null,
  "createdAt": 800000000, "isUnread": false,
  "data": { "tab": { "savedURL": "https://example.org/docs",
                     "savedTitle": "Example docs",
                     "timeLastActiveAt": 800000500.25 } } }

{ "id": "I2", "childrenIds": ["I1"], "title": "Reading",
  "data": { "list": { "customInfo": { "iconType": { "icon": "flash" } } } } }
```

- `tab`: a page. A non-empty `title` is a name the user gave the tab.
- `list`: a folder; `childrenIds` are its rows in order. With
  `automaticLiveFolderData` whose `dataSource` has a `github` key, it is a
  folder that Arc fills from GitHub.
- A container (`C1` to `C3` in the examples) is an item that a space or
  `topAppsContainerIDs` points to; only its `childrenIds` are read.
- Any other item (a split view, for example) is not imported itself. If it
  has rows that are, they are kept as a folder with the item's title.

Timestamps are seconds since 2001-01-01 UTC (Unix time minus 978,307,200).

## Spaces

```json
{ "id": "S1", "title": "Work",
  "containerIDs": ["pinned", "C1", "unpinned", "C2"],
  "newContainerIDs": [{ "pinned": {} }, "C1", { "unpinned": {} }, "C2"],
  "customInfo": {
    "iconType": { "icon": "flash" },
    "windowTheme": { "anyKey": { "red": 0.2, "green": 0.6, "blue": 0.5 } } },
  "profile": { "default": true } }
```

- `containerIDs` and `newContainerIDs` both name the two containers of a
  space: a marker (the string or an object with the key `pinned` or
  `unpinned`) followed by the container's id. Both lists are scanned and
  the first id found per role counts.
- `iconType` is `{ "icon": "<name>" }` or `{ "emoji": "<emoji>" }` (also
  `emoji_v2`; the value can be a string or `{ "_0": "<value>" }`).
- `windowTheme` nests colour objects at any depth. The first object found
  with numeric `red`, `green` and `blue` becomes the space colour, clamped
  to 0..1.
- `profile` is `{ "default": true }` or `{ "custom": { "_0": <profile> } }`.
  The profile is read as its id, or as an object that names it with `id` or
  with `directoryBasename` (the profile's folder).

## Mapping

| In the file | In Magnetite |
|---|---|
| space | a space with its title, icon and colour |
| rows of the pinned container | the space's pinned tree; tabs return to `savedURL` |
| rows of the unpinned container | the space's Today tabs (folders there are flattened) |
| rows of the containers in `topAppsContainerIDs` | favorites of the default profile |
| tab `title`, else `savedTitle`, else `savedURL` | the tab's title; a `title` counts as a custom name |
| folder `title` | the folder's title, "Untitled" if empty |
| `createdAt`, `timeLastActiveAt`, `isUnread` | created, last used, unread |
| folder filled from GitHub | a GitHub smart folder whose items are its tabs |
| custom `profile` id | a profile named "Profile 1", "Profile 2", … |

## Untrusted input

- A file over 256 MiB, or one that is not a regular file, is not read.
- Each item becomes at most one row: cycles and ids listed twice are
  skipped. Favorites are read first, so an item listed there and in a space
  stays a favorite. A space listed twice is taken once.
- Folders nested deeper than 32 levels, or more than 100,000 spaces, tabs
  and folders together, make the import fail.
- A profile id of 1 to 64 letters, digits, `-` and `_` is kept, lower-cased.
  Any other id, or one that clashes with an id already given out, becomes
  `arc-profile-N`, so every profile gets its own data folder. An import
  through Settings then replaces these ids with fresh ones.
- The app drops tabs whose addresses Magnetite does not open from stored
  data (`javascript:`, `data:`, `file:`, internal pages) and clamps colours
  and timestamps (`docs/design/security.md`).

Code: `crates/importer/src/arc/raw.rs` (the fields read), `arc/convert.rs`
(the mapping), `crates/importer/tests/import_fixture.rs`, `malformed.rs`.
