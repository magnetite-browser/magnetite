# Command bar

The command bar is one input for addresses, web search, site search, tabs,
bookmarks, the history, archived tabs and commands. It also asks for the name of a new space or profile. It opens
as a small window centred over the page area and searches only local data:
nothing typed leaves the device before Enter opens an address or a search.

## Opening

| Opened by | Input | Enter |
|---|---|---|
| `Cmd+T`, the New Tab row, "+" → New Tab | empty | opens the address or search in a new Today tab |
| `Cmd+L`, a click on the address in the sidebar | the current address, selected | loads it in the current tab |
| `Cmd+Shift+=`, "+" → New Split, the "Add … split…" commands | empty | opens it in a new tab beside the current one (`split-view.md`) |
| `Cmd+Shift+N`, "+" → New Space | a name | creates the space (`spaces.md`) |
| "New profile for this space…" or "… for this favorite…" | a name | creates a separate profile and assigns it |
| `Cmd+Option+N` in any app (the quick search), File → New Mini Window | empty | opens the address or search in a mini window (`mini-window.md`) |

With no tab open, `Cmd+L` behaves like `Cmd+T`. A tab that goes from one
of Magnetite's own pages to a website, or back, gets a new page in the
profile it belongs to (Magnetite's own pages run in the default profile
only). `Cmd+Shift+=` opens to the
right; the split commands also open to the left, top or bottom. The menu
bar has the first four under File.

The window is 640 points wide (narrower in a narrow window), sits in the
upper part of the page area and grows with its results. Esc, a pick, or a
click anywhere else closes it. Only one is open at a time. The quick search
is the same bar in a window of its own, on the screen with the pointer and
above other apps' windows; it lists the address or search, bookmarks and
visits only, and opens every pick in a mini window (`mini-window.md`).

## Results

Results appear while typing, in this order:

1. **Address or search**: `Open <input>` or `Search for "<input>"`.
2. **Tabs**: the tabs of all spaces, pinned and Today, and the favorites.
   A pick shows that tab, switching space if needed, instead of opening a
   second copy. In split mode the tab joins the split.
3. **Commands**: see below.
4. **Bookmarks**: bookmarks of all spaces. A pick opens it as a new tab in
   its space (`bookmarks.md`).
5. **History**: visited pages (`history.md`), newest first, at most 8. A
   page that is among the tabs or bookmarks found is left out, and each
   page shows once. A pick opens it like the address result: a new tab, the
   current tab with `Cmd+L`, or beside it in split mode.
6. **Archive**: archived tabs. A pick restores the tab (`archive.md`).

Tabs, bookmarks, visits and archived tabs match when the input is part of
their title or address, ignoring case. The search looks at the newest 200
archived tabs and returns at most 30 results, in the order above. A result
shows the host and the space it is in, "Favorites" or "Archived".

| Key | Effect |
|---|---|
| Up / Down | move the selection |
| Enter | run the selected result |
| Cmd+Enter | on the address or search result: always open a new tab |
| Shift+Enter | on an address, search, site search, bookmark or visit: open it as a peek over the page in front (`peek.md`) |
| Space or Tab after a keyword | start a site search (below) |
| Backspace at the start | end the site search; the keyword is text again |
| Esc | close |

## Addresses and search

- Input with `://`, or starting with `about:`, is used as typed.
- Input without spaces that contains a dot and does not end with one gets
  `https://` in front.
- An address on this computer (`localhost`, a name under `.localhost`,
  `127.0.0.1` or `[::1]`, with or without a port) gets `http://`.
- Anything else is searched with the engine from Settings → Search:
  DuckDuckGo (default), Startpage, Ecosia, Brave Search, Kagi, Google, or a
  custom `https://` address with `%s` for the query.
- Web addresses, `about:blank`, `file://`, `view-source:`, `chrome://`,
  extension pages and Magnetite's own pages open. Other schemes, such as
  `javascript:` and `data:`, are refused (`docs/design/security.md`).
- Magnetite's own pages, such as Settings, open once per space: a second
  request shows the existing tab.

## Site search

A keyword, a space and a query search one site instead of the search
engine: `gh rust cef` searches GitHub for "rust cef". As soon as a known
keyword is followed by a space (or Tab), the bar shows the site's name in
a chip before the input and lists only "Search <site> for "<query>""; the
other results wait until the chip is gone. Keywords match regardless of
case. Typed whole (`Cmd+Enter`, or `Cmd+L` input), the same text is
turned into the site's address by the app as well.

Built in, and changeable in Settings → Search → "Site search":

| Keyword | Site |
|---|---|
| `gh` | GitHub |
| `w` | Wikipedia (English) |
| `yt` | YouTube |
| `mdn` | MDN |
| `crates` | crates.io |
| `npm` | npm |
| `so` | Stack Overflow |
| `ddg` | DuckDuckGo |

Each entry has a keyword (one word of letters, digits, `-` or `_`, at most
20, unique), a name and a search address starting with `https://` with `%s`
where the query goes, checked like the search engine's. An entry that is
not is marked and not saved; at most 50 are kept. Changes are saved when a
field is left. The list is this device's and does not sync, like the search
engine.

## Commands

A command is listed when the input is part of its title or starts one of
its keywords, such as "clear" for Archive all Today tabs. Open archive and
Open settings match by keyword only ("archive"; "settings", "preferences").

| Group | Commands |
|---|---|
| Spaces and tabs | New space, New folder, Archive all Today tabs, Reopen last archived tab, Switch to <space> (each other space) |
| This tab | Pin tab or Unpin tab, Back to pinned page (a pinned tab or favorite away from its page), Add tab to favorites or Remove tab from favorites, Keep tab active or Stop keeping tab active, Move tab to <space> (each other space), Duplicate tab, Close other Today tabs |
| Views | Open archive, Open history, Open downloads, Open bookmarks, Open settings, Toggle sidebar, Import from another browser… |
| Links | Copy link without trackers, Copy original link, Copy link as Markdown, Bookmark this page |
| Page | Print page, Reset zoom, Toggle developer tools, Capture page…, Copy last capture |
| Split view | Add right split…, Add left split…, Add top split…, Add bottom split…, Even out split sizes, Separate split tabs |

"This tab" is the active tab (with a split on screen, the pane last
clicked), and only what fits it is listed: a favorite cannot be pinned,
kept active or moved to a space. They do what the tab's menu in the
sidebar does (`sidebar.md`). "Move tab to" puts it at the top of that
space's Today tabs. "Duplicate tab" opens the tab's address as a new Today
tab of the current space (a favorite gets a copy next to it, in its
profile). "Close other Today tabs" archives every Today tab of the current
space but the active one. "Print page" prints the page in front, a peek
included (`site-controls.md`).

"New space" creates a space named "New space" at once; `Cmd+Shift+N` asks
for the name first. The copy commands are confirmed in the sidebar, since
the command bar closes at once: the link button of the address row for a
link, a short note for a capture.

## Not built yet

- The `>` prefix for commands.

Code: `crates/ui/web/src/CommandBar.tsx`,
`crates/ui/web/src/CommandPalette.tsx`, `crates/app/src/app/`
(`addresses.rs`: `normalise_url_with`, `site_search`;
`shortcuts.rs`: `open_palette`; `mini.rs`: `open_mini`),
`crates/app/src/state/search.rs`,
`crates/app/src/state/settings.rs` (`SiteSearch`), `crates/app/src/url_policy.rs`,
`crates/engine/src/overlay.rs`.
