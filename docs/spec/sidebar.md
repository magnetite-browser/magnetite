# Sidebar

The sidebar is Magnetite's main control. It holds the address row, the
favorites, the pinned and Today tabs of the current space, and the space
switcher. It is a page of its own (`magnetite://ui/`) docked at the left of
the window; the web pages take the rest.

## Layout

From top to bottom:

1. **Title band**: window buttons, the sidebar button, pinned extensions
   (`extensions.md`), back, forward and reload.
2. **Address row**: site controls, the host of the current page and a
   copy-link button. A click on the host opens the command bar with the
   address (`command-bar.md`). An open peek shows its controls here instead.
3. **Media rows**: up to two, for media in tabs not on screen (`media.md`).
4. **Favorites**: grids of icons, the same in every space: one list per
   profile that has favorites (below). Hidden while there are none.
5. **Space title**, then the **pinned** tabs and folders of that space
   (`spaces.md`). Only the current space is rendered.
6. A divider with **Clear**, which archives all Today tabs of the space.
7. The **New Tab** row, then the **Today** tabs. New tabs appear at the top.
8. **Footer**: the button for bookmarks, the history, downloads and the
   archive (`bookmarks.md`, `history.md`, `archive.md`; its menu also opens
   Settings), one icon
   per space, and
   "+" (New Space, New Folder, New Smart Folder, New Split, New Tab).

If the sidebar's page crashes, Magnetite loads it again and it shows the
current state; the same holds for the command bar and the notification
panel. A page that crashes again within ten seconds is left alone.

## Tab kinds

| Kind | Shown in | Closing it | Auto-archive | Link to another site |
|---|---|---|---|---|
| Favorite | every space | unloads the page, the item stays | never | opens as a peek |
| Pinned | one space | unloads the page, the item stays | never | opens as a peek |
| Today | one space | moves the tab to the archive (`archive.md`) | yes | opens in the tab |

A favorite or pinned tab remembers a **home page**: its address when it was
pinned or added. Its page is locked to that site, and a link that leaves
the site opens as a peek (`peek.md`). When the page of a
pinned tab has left its home page, the title starts with `/`, and hovering
the row turns the icon into a back arrow that takes the page back.

## Tab rows

- A row shows the icon, the title, a speaker button while the tab plays or
  is muted, and a close button on hover. A bold title marks a tab that
  opened in the background and was not viewed yet.
- A click shows the tab and loads its page if needed. Tabs without a loaded
  page are dimmed. A page is unloaded once it has been off screen for more
  than 4 hours (Settings → Tabs & archive: 10 minutes, 30 minutes,
  2, 4 or 12 hours, or never), unless it has media, is the tab its space
  returns to, is a favorite, or is kept active. When the system runs short
  of memory, such pages are unloaded without waiting, also with "never":
  at the system's warning the half that was off screen the longest, at its
  critical level all of them.
- **Favorites stay loaded** once opened, until they are closed.
- Closing a favorite or a pinned tab unloads it and takes it back to its
  pinned page, which it opens at next time. Its title stays the last page's
  until then.
- **Keep active** in the menu of a pinned or Today tab spares that tab:
  its page is never unloaded for being off screen, and a Today tab is not
  auto-archived. A check mark shows it in the menu and a small flash on the
  row; the same entry lets the tab sleep again. Closing the tab ends it.
- Double-click renames a tab; "Use page title" in the context menu undoes
  it. Middle-click closes a tab.
- The context menu has the actions that fit the tab, among them Reload,
  Share…, Copy link, Pin tab, Unpin tab, Add to favorites, Remove from
  favorites, Open in split view, Rename, Move to another space, and Archive
  (Today) or Close.

## Moving tabs

- Drag a tab to reorder it, or to a favorites list (added at the end),
  Pinned, a folder or Today. Dropped on the middle of another tab's row or
  on the page area, it opens in split view (`split-view.md`).
- `Cmd+D` pins the active Today tab at the top of Pinned, or moves the active
  pinned tab to the top of Today. It does nothing for a favorite.
- A tab that comes to Today from Pinned or the favorites counts as used at
  that moment, so auto-archive does not take it for the time before.
- Closing the tab in front shows the next one of the space (a favorite:
  another page on screen, else the space's first tab). With none left, no
  tab is in front; a closed pinned tab does not load again by itself.
- "Remove from favorites" and "Move to …" put the tab at the top of Today,
  in the current space or the chosen one.
- Moving does not reload the page, unless the tab lands in another
  profile: then it loads again there. A tab that enters Pinned or the favorites
  takes its current address as home page if it has none; a tab that enters
  Today loses it.

## Favorites

- Every profile has its own list of favorites, and a favorite runs in the
  profile of its list: its cookies and logins are that profile's. The lists
  stand one below the other, in the order of the profiles.
- With favorites in the default profile only, the list has no title. With
  more than one list, or when the only one belongs to another profile, each
  list is titled with its profile's name. A click on a title collapses or
  expands its list; collapsed, the title shows the number of favorites. The
  sidebar page remembers this on the device.
- "Add to favorites" puts a tab into the list of the profile it runs in. A
  tab dropped on a list, also on a collapsed one's title, joins that list
  and its profile.
- The context menu's "Profile: …" entries move a favorite to another
  profile's list, and "New profile for this favorite…" makes a profile and
  a list for it. "Duplicate favorite" plus another profile gives two
  accounts of one site, each in its list.

## Folders

- Folders live in Pinned. "+" → New Folder, the space menu and the command
  bar create one named "Untitled" at the top of Pinned.
- A click collapses or expands a folder; collapsed, it shows its number of
  tabs. The sidebar page keeps this on the device, not in `state.json`.
- Double-click or the context menu renames a folder; "Change icon…" picks
  one of 27 glyphs or the default icon.
- A tab dropped on a folder goes to its top. A dragged folder is placed
  among the top-level items of Pinned.
- Folders inside folders are shown (an import can bring them), but the
  sidebar cannot nest a folder.
- "Delete folder (keep tabs)" in the context menu removes the folder; its
  tabs stay where it was. A smart folder is deleted with the items it
  listed.
- A folder can fill itself from a service (`smart-folders.md`).

## Hiding and resizing

`Cmd+S` or the sidebar button hides the sidebar: the pages take the whole
window and the keyboard. `Cmd+S` shows it again. In a window that is not
full screen, a strip above the pages stays free for the window's close,
minimize and zoom buttons. While it is hidden on macOS,
moving the pointer to the window's left edge slides the sidebar out over the
pages without taking the keyboard from the page. It leaves when the pointer
does, or on Esc; its sidebar button or `Cmd+S` docks it again. The sidebar
is docked at every start.

Dragging the sidebar's right edge sets its width (220 to 360 points, 264 by
default; double-click resets). Settings → Appearance has the width too, plus
text size (85 to 130 %) and row density (compact, comfortable, spacious).

## Keyboard

| Action | Shortcut |
|---|---|
| New tab (command bar) | Cmd+T |
| Change the address | Cmd+L |
| Close the tab | Cmd+W |
| Reopen the last archived tab | Cmd+Shift+T |
| Next / previous tab of the space (below) | Ctrl+Tab / Ctrl+Shift+Tab |
| Tab 1 to 9 of the space (pinned first, then Today) | Cmd+1 to Cmd+9 |
| Pin or unpin the active tab | Cmd+D |
| Show or hide the sidebar | Cmd+S |
| Back / forward / reload | Cmd+[ / Cmd+] / Cmd+R |
| History | Cmd+Y |
| Print the page | Cmd+P |

`Cmd+S`, `Cmd+D` and `Cmd+P` go to the page first, which may use them
itself. The six shortcuts above `Cmd+D` are taken before the page sees the
key.

`Ctrl+Tab` and `Ctrl+Shift+Tab` go through the rows of the current space
in the order of `Cmd+1` to `Cmd+9`: the pinned tabs, those in folders
included (also closed ones), then Today; after the last comes the first.
The tabs of a split count one by one, so the keys can leave a split or
move within it. Favorites are not part of the round; from a favorite the
keys go to the first or the last row.

## Data

`<data-dir>/state.json` holds the sidebar (profiles, favorites, spaces with
their pinned tree and Today list), the active space and the active tab of
each space. It is written whole after every change, readable by its owner
only.

Icons: for a page in the default profile the sidebar shows the icon the page
reported (its address is kept per tab in `state.json`). Otherwise it uses
the local cache, `<data-dir>/favicons/<host>.png`, written when a page loads
(never for a private profile). Without an icon the row shows the first
letter of the host. The sidebar never fetches the icon of a site that was
not opened.

## Not built yet

- Changing the home page of a pinned tab or favorite without pinning it
  again, and a "back to home page" control for favorites.

Code: `crates/core/src/{model,sidebar}.rs`, `crates/app/src/app/tabs.rs`,
`crates/app/src/state/{tabs,folders,profiles}.rs`,
`crates/engine/src/{keys,floatbar,favicon}.rs`,
`crates/ui/web/src/{App,Rows}.tsx`.
