# Sidebar

The sidebar is Magnetite's main control. It holds the address row, the
favorites, the pinned and Today tabs of the current space, and the space
switcher. It is a page of its own (`magnetite://ui/`) docked at the left of
the window, or with the top bar at the right (below); the web pages take
the rest.

## Layout

From top to bottom:

1. **Title band**: window buttons, the sidebar button, pinned extensions
   (`extensions.md`), back, forward and reload. A middle click on back or
   forward opens the page it would go to in a new tab, and on reload the
   page itself; such a tab opens as a page's link opened in a new tab does
   ("Tab rows"), in the page's profile. The top bar's buttons do the same.
2. **Address row**: site controls, the host of the current page (one of
   Magnetite's own pages shows its name, as "Settings") and a
   copy-link button. A click on the host opens the command bar with the
   address (`command-bar.md`). An open peek shows its controls here instead.
   With Settings → Appearance → "Extension bar" on, a row of pinned
   extensions follows, and the title band has none (`extensions.md`).
3. **Media rows**: up to two, for media in tabs not on screen (`media.md`).
4. **Favorites**: one grid of icons, the same in every space but for the
   favorites that are for one space only (below). Hidden while there are
   none.
5. **Space title**, then the **pinned** tabs and folders of that space
   (`spaces.md`). Only the current space is rendered. With more than one
   profile, the profile the space runs in shows in a small box after its
   name, in the profile's colour ("Favorites"). The title stays in place:
   the lists under it (pinned, divider, Today) scroll on their own.
6. A divider. On hover it shows **Hide pinned** at its left, which hides
   the pinned tabs and folders of the space, and **Clear** at its right,
   which archives all Today tabs of the space. While the pinned tabs are
   hidden, the divider shows their number in that place ("5 pinned"), and a
   click there shows them again. A hidden pinned tab that is in front keeps
   its row. The sidebar page remembers this per space on the device.
7. The **New Tab** row, then the **Today** tabs. New tabs appear at the top
   (a page's new tab can go below its tab instead, "Tab rows" below).
8. **Footer**: the button for bookmarks, the history, downloads and the
   archive (`bookmarks.md`, `history.md`, `downloads.md`, `archive.md`; its menu also opens
   Settings), one icon
   per space, and
   "+" (New Space, Private Space, New Folder, New Smart Folder, New Split,
   New Tab).

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
  is muted, a red screen mark while the tab is being shared (below), and a
  button on hover: a × where the row goes away (a Today tab, which is
  archived), a minus where only the page is unloaded and the row stays (a
  pinned tab). A bold title marks a tab that
  opened in the background and was not viewed yet.
- Unread counts: chat and mail sites put the number of unread messages
  into their page title, in parentheses at its start ("(3) WhatsApp") or
  after its first word ("Inbox (12) - Gmail"), followed by a space or the
  end of the title ("invoice (1).pdf" is a file name). A plain number of
  four digits or more reads as a year and does not count ("(2024) Annual
  report"); larger counts need thousands separators ("(1,234)"). The
  sidebar shows the number in a red bubble, "99+" above 99: at the end of
  the tab's row (the hover button takes its place), in its cell of a
  split row, in the tab switcher and on the corner of a favorite's icon.
  The tab's title is shown, saved, synced and kept in the history without
  the number, also a count of 0 and the title of a peek kept as a tab;
  titles saved, synced or imported with a count lose it when read. The
  bubble goes when the site takes the number out of its title, for
  example once the messages are read on another device, and with the
  page: an unloaded tab has none. A renamed tab keeps its name and still
  gets the bubble. The count lives in memory only.
- A click shows the tab and loads its page if needed. Tabs without a loaded
  page are dimmed. A page is unloaded once it has been off screen for more
  than 4 hours (Settings → Tabs and archive: 10 minutes, 30 minutes,
  2, 4 or 12 hours, or never), unless it has media, is being shared, is the tab its space
  returns to, is a favorite or pinned, or is kept active. When the system runs short
  of memory, such pages are unloaded without waiting, also with "never":
  at the system's warning the half that was off screen the longest, at its
  critical level all of them.
- While no page is on screen (every tab closed, or a new space), the page
  area shows the logo and "Press ⌘T to open a tab", or the picture chosen
  for the space's empty window (`spaces.md`, "Look").
- **Favorites and pinned tabs stay loaded** once opened, until they are
  closed, also when the system runs short of memory.
- **Links opened in a new tab** (Open Link in New Tab, `Cmd`+click,
  middle click on a page's link) become Today tabs behind the current
  one, without a page until they are opened. Settings → Links → "Links
  opened in a new tab" can instead load them in the background, or switch
  to the new tab. The choice is this device's own, `settings.new_tab_links`
  (`"inactive"`, `"load"` or `"switch"`) in `state.json`; it is not
  synced. A tab of the hidden private space stays behind in every case.
- **Where such a tab goes.** At the top of Today, as every new tab. With
  Settings → Links → "Where the new tab goes" set to "below the tab it came
  from", a tab that a tab's page opens (a link in a new tab, `window.open`,
  a middle click on back, forward or reload) goes directly below that tab
  when it is in Today; the tabs the same page opened before, and the tabs
  those opened, stay between them, so several links read in the order they
  were opened. Below a split means below its row, which shows at its first
  page that has a row (its other pages show none); a split whose row is with
  the pinned tabs leaves the tab at the top. The tabs a split's pages opened
  count as its row's. The tab of a pinned tab, a favorite, a peek, a mini
  window, a web panel or an extension, and one that goes to another space,
  is at the top; so are a link from another app (no tab opened it, and a
  rule may send it to another space), the command bar's new tab, kept peeks
  and mini windows and restored tabs. Which tab opened which is not saved:
  after a restart the next tab of a page goes directly below it. This
  device's own, `settings.new_tab_place` (`"top"` or `"below"`); it is not
  synced.
- **Load pinned tabs at start** (Settings → Tabs and archive, off by
  default): the favorites and pinned tabs of every space load their pages
  in the background when Magnetite starts, three at a time, so none waits
  for its first click. Web panels and tabs that are no web address
  (files, Magnetite's own pages) are left out; they load when opened.
- Closing a favorite or a pinned tab unloads it and takes it back to its
  pinned page, which it opens at next time. Its title stays the last page's
  until then.
- A pinned tab without a loaded page, at its pinned page, has nothing to
  unload: on hover its row shows a × in place of the minus, which
  removes the tab from the space. (Away from its pinned page, the minus
  takes it back there first.) It goes to the archive at its pinned
  page, under its own title if its page is loaded there, else that page's
  title from the history, never the title of another page it was left on,
  and comes back from there as a Today tab (`archive.md`). One of
  Magnetite's own pages or of a private profile just closes, also from the
  menu, and is gone for good. A row whose page was just unloaded with its
  button or a middle click keeps the minus until the pointer leaves.
- After a click that takes a row away, the rows below move up under the
  pointer. The second click of a double click that lands on another
  element than the first does nothing on a pinned tab (minus, ×,
  middle click), on "Clear" and "Hide pinned", on a folder row and its buttons, on a snoozed
  item's "Wake now", on a split row's cell and on a tab of another device.
  So a double click removes one pinned tab and never unloads the next
  one, archives all of Today, opens a folder for renaming, breaks up a
  split or takes over another device's tab. A double click on one element
  works as before, and quick clicks in one place on Today tabs close one
  tab after another.
- **Keep active** in the menu of a Today tab spares that tab: its page is
  never unloaded for being off screen, and it is not auto-archived. A check mark shows it in the menu and a small flash on the
  row; the same entry lets the tab sleep again. Closing the tab ends it.
- Double-click renames a tab; "Use page title" in the context menu undoes
  it. A favorite is renamed from its context menu, in a row under the
  tiles of its list. Middle-click does what the row's close button or
  minus does; on an item a smart folder got from its source it closes,
  which does nothing without a loaded page (such an item snoozes instead).
  It does nothing on a row while it is being renamed or a snooze time is
  being picked.
- The context menu has the actions that fit the tab, in the groups and
  the order of every menu ("Context menus"): Reload, Share…, Copy link;
  Pin tab or Unpin tab, Back to pinned page, Add to or Remove from
  favorites, Add to bookmarks, Move to another space; Open in split view,
  Open as panel, Mobile view, Keep active; Rename, Duplicate tab; and
  Archive (Today) or Close. What has nothing to do right now is greyed out
  and says why on hover: Reload, and Close on a favorite or pinned tab,
  without a loaded page (Close stays for a favorite away from its pinned
  page, which it takes back there); Back to pinned page on a pinned tab
  that is there; Open in split view while the split is full; Mobile view
  on a tab that is no panel. A pinned tab also has "Remove from pinned
  tabs", loaded or not, except an item a smart folder got from its
  source, which the next refresh would bring back. "Duplicate tab" opens the tab's
  address as a new Today tab; a favorite has "Duplicate favorite" instead.
- With one other space, the row names it ("Move to Work"). With more,
  "Move to another space" shows them as a list in the menu's place, below a
  row back to the menu; past ten spaces the list scrolls. A menu taller
  than the window starts at the window's top and scrolls.

## Shared tabs

While a page shares a tab's picture (screen sharing in a meeting, "Tab" in
the share dialog), that tab is marked wherever it shows:

- Its row carries a red screen mark, also on hover; so does its cell in a
  split row, where the close button takes the mark's place on hover. A
  shared favorite has a red dot on its tile.
- The address row's chip has the red dot while the shared tab is the
  current one; the tooltip says "This tab is being shared".
- The mark follows "Share this tab instead" and goes when the sharing ends:
  "Stop sharing", the meeting page ending it, or the meeting tab closing or
  loading another page.
- A shared tab is not unloaded or archived in the background.

While a page shares a tab, the rows of the other loaded tabs of that
page's profile show a share button left of the close button on hover. It
shares that tab in place of the shared one, as Chromium's "Share this tab
instead" does: no dialog, and the tab in front stays in front. The mark
moves with the share. The sharing page's own tab and tabs without a loaded
page get no button, as Chromium has no tab to switch to there. A page that
asked for no switching (`surfaceSwitching: "exclude"`) keeps its tab; the
button does nothing then.

Sharing a window or the whole screen marks no tab. On macOS a shared
Magnetite window shows the sidebar and the pages in it, as on screen; that
takes Magnetite's CEF build with `window-share.patch`, and the Screen
Recording permission, as sharing any other app's window does.

How it works: CEF does not say which tab a capture shows, so the pages
tell. Every web page gets a capture handle, a random text set with
`setCaptureHandleConfig`. A page that calls `getDisplayMedia` has its
tracks watched (copies made with `clone` too), reads the handle of the tab
they show and reports it; this covers frames inside the page, also ones
without an address of their own (`srcdoc`). The hook reads a track through
the browser's own functions, so a page cannot make a live capture look
ended by redefining them. A tab is shared while such a report names its
handle. The handle is new with every page load, and only a page that
captures the tab can read it; it names nothing about the user or the tab.

Not marked, for that reason:

- pages without a secure context (plain `http`, except this computer's
  own addresses) and Magnetite's own pages;
- a page that sets a capture handle of its own, which replaces ours;
- captures started by an extension.

The share button needs Magnetite's CEF build with `tab-share-instead.patch`
(named in its `magnetite-patches.txt`). Chromium puts a sharing bar on
every tab of the profile while a page shares a tab, and its "Share this
tab instead" button on the tabs it can switch to. CEF has no call for it,
so the patch lets the app press that button of a given tab. With the
official CEF there is no button.

Code: `crates/engine/src/tabshare.rs`, `tabshare_hook.js`;
`crates/app/src/state/tabs.rs` (`in_use`, `share_targets`);
`crates/ui/web/src/Rows.tsx`.

## Moving tabs

- Drag a tab to reorder it, or to a favorites list, Pinned, a folder or
  Today. Dropped on the middle of another tab's row or on the page area, it
  opens in split view (`split-view.md`).
- Over the favorites the tiles stay where they are. On a tile, the
  dragged tab or favorite joins it in a split (`split-view.md`). Between two
  tiles, which is their facing edges (10 points each) and the gap, a line
  marks where it would go; it joins the profile of the tile it was last
  over. Off the tiles (the grid's edge, the room after the last tile) the
  line stays where it was, or is after the last tile when the tab was over
  none yet. A split's tile, and a tab on a tile that takes no further page, go
  before or after the tile by the half under the pointer.
- `Cmd+D` pins the active Today tab at the top of Pinned, or moves the active
  pinned tab to the top of Today. It does nothing for a favorite. With a
  split in front, the split is pinned or unpinned (`split-view.md`).
- A tab that comes to Today from Pinned or the favorites counts as used at
  that moment, so auto-archive does not take it for the time before.
- Closing the tab in front shows the next one of the space (a favorite:
  another page on screen, else the space's first tab). With none left, no
  tab is in front; a closed pinned tab does not load again by itself.
- "Remove from favorites" and "Move to …" put the tab at the top of Today,
  in the current space or the chosen one. Where the spaces use more than
  one profile, each space to move to shows its profile in a small box
  after its name, so two spaces of the same name can be told apart.
- Moving does not reload the page, unless the tab lands in another
  profile: then it loads again there. A tab that enters Pinned or the favorites
  takes its current address as home page if it has none; a tab that enters
  Today loses it.

## Favorites

- Every profile has its own list of favorites, and a favorite runs in the
  profile of its list: its cookies and logins are that profile's. The
  sidebar shows them as one grid, list after list in the order of the
  profiles (Settings → Profiles sorts them), with no titles and no rows of
  their own: profiles with one favorite each do not take a row each.
- A favorite's tile is tinted with the colour of its profile, so two
  logins of one site differ and the favorites of one profile are seen
  together. Every lasting profile but the default one has a colour: a new
  one gets the first of ten that no profile has yet, and Settings →
  Profiles changes it. The default profile is plain until it is given a
  colour there. The colour is part of the profile and syncs with it.
- The pointer on a tile shows a label below it at once: the favorite's
  name and, with favorites of more than one profile or of another than
  the default one, the name of its profile in a box of the profile's
  colour.
- "Add to favorites" puts a tab into the list of the profile it runs in. A
  tab dropped among the tiles joins the list of the tile it lands beside,
  and its profile.
- The context menu's "Profile" row names the favorite's profile and opens
  the list of profiles, each with its colour: choosing another moves the
  favorite to that profile's list, and "New profile…" at its end makes a
  profile and a list for it. "Duplicate favorite" plus another profile gives two
  accounts of one site, each in its list.
- The favorites of one split show as one tile with an icon per page
  (`split-view.md`).
- A favorite shows in every space, or only in the spaces it names. The
  context menu's "Shows in" row says which, and its list has "Show in
  every space" and "Show only in <the current space>"; Settings → Favorites lists every favorite with its name and,
  beside it, the address it goes to, deletes one from its row (a second
  click confirms, as for every deletion there; its page moves to the
  archive), and has a page per favorite (as
  Settings → Spaces has per space) with its name, its address to change (the
  favorite goes there from then on, and its page loads it at once; what is
  no address of a website is refused), its profile, its deletion and the
  choice for that favorite
  and any space, or several of them. So a space can have favorites of its own next to those
  all spaces share. A new favorite shows in every space. A favorite for
  one space only is left out of that space's Ctrl+Tab order and web panel
  key elsewhere, and a tile dropped among the favorites lands between the
  ones the space shows. Spaces that are gone (deleted, or only on another
  device) do not count: a favorite left with none shows in every space. A
  favorite that leaves the favorites forgets its spaces.
- "Open as panel" makes a favorite or a pinned tab a web panel: a click then opens it
  over the page in front instead of in its place (`web-panels.md`).

## Folders

- Folders live in Pinned. "+" → New Folder, the space menu and the command
  bar create one named "Untitled" at the top of Pinned.
- A click collapses or expands a folder; collapsed, it shows its number of
  tabs. The sidebar page keeps this on the device, not in `state.json`.
- Double-click or the context menu renames a folder; "Change icon…" picks
  one of 27 glyphs or the default icon.
- A tab dropped on a folder goes to its top.
- Folders nest: a folder dropped on the middle of another goes to its top,
  one dropped on the upper or lower edge of a row is placed there, at any
  level, and one dropped below the pinned items goes to the end of the top
  level. "New folder inside" in a folder's context menu makes one at its
  top. A folder does not go into itself, into a smart folder or more than
  32 folders deep.
- "Delete folder (keep tabs)" in the context menu removes the folder; its
  tabs stay where it was. A smart folder is deleted with the items it
  listed.
- A folder can fill itself from a service (`smart-folders.md`).
- "Copy link to share" in the context menu makes a link that carries the
  folder (`sharing.md`).

## Context menus

- A menu opens at the pointer and stays inside the sidebar. It closes on a
  pick, on a click beside it, on Esc and when the sidebar loses the
  keyboard.
- A row with a list of its own (Move to another space, Profile, Shows in)
  has an arrow at its right edge. A click on it opens the list in the
  menu's place, below a row that names it and leads back: a menu in the
  sidebar has no room beside it for one that flies out. A list never
  opens by itself. It opens where the menu stands, and a long one scrolls.
- Every menu is built from the same groups, in this order, with a line
  between two that have rows: the page (reload, share, copy link, a smart
  folder's refresh and settings); where the item is kept (pinned,
  favorites, bookmarks, its space); how it shows (split view, panel, kept
  active, a folder folded); the item itself (name, icon, a copy, a folder
  inside); what it belongs to (its profile, the spaces it shows in); and
  what takes it away (archive, close, delete), always last.
- A row that belongs to the item but has nothing to do right now stays in
  its place, greyed out, and a hover says why. It cannot be picked, and
  the keyboard skips it. A row that does not fit the item at all is left
  out.
- In the menu of the tab in front, Reload, Pin or Unpin and Archive or
  Close show the keys that do the same at the right edge; the keys act on
  the tab in front, so no other tab's menu shows them. The private
  space's menu shows the keys for hiding and closing it.
- A row that is on or off (Keep active, Open as panel) has a check mark
  while it is on, and of the rows to choose one from (the profiles) the one
  chosen has a dot. Both stand in a column of their own before the labels,
  so the labels are in line.
- Keyboard: the up and down arrows go through the rows, around the ends,
  and through an open list's rows; Home and End go to the first and the
  last. Enter or Space picks the row, once for a key held down: a row that
  asks first ("Delete space…") wants a second press. On a row with a list, Enter, Space
  or the right arrow opens it and goes to its first row; the left arrow,
  or the row that leads back, returns to the menu, onto that row. Any other key closes the menu.
  The pointer and the keyboard move the same mark, so one row is lit.

## Hiding and resizing

`Cmd+S` or the sidebar button hides the sidebar: the pages take the whole
window and the keyboard. `Cmd+S` shows it again. (On Windows the
window's buttons are in a row above the pages at the top right, with or
without the sidebar, unless the top bar is on, which has them.) While it is
hidden, moving the pointer to the window's edge on the sidebar's side slides
the sidebar out over the pages, along the window's whole height, without
taking the keyboard from the page. Where a page reaches that edge (no frame
around the page) the strip that reacts is 3 points wide instead of 8, so the
page's own edge, its scrollbar on the right, stays in reach. It leaves when
the pointer does, or on Esc; its sidebar button or `Cmd+S` docks it again.
On Linux under Wayland a finger on that edge slides it out too, with the
keyboard, and it leaves when a page takes the keyboard back: a tap on the
page, or a tab picked in it. The sidebar is docked at every start.

The pages cover the window's close, minimize and full screen buttons while
the sidebar is hidden, so the sidebar that slides out has them, in its title
band. With the top bar on they stay in the top bar. The gap above the pages
still drags the window.

Dragging the sidebar's edge towards the pages sets its width (220 to 360 points, 264 by
default; double-click resets). Settings → Appearance has the width too, plus
text size (85 to 130 %), row density (compact, comfortable, spacious), the
layout and the frame around the page (both below).

## Top bar

Settings → Appearance → Layout → **Top bar** (off by default), or "Toggle
top bar" in the command bar, moves the sidebar's title band and address row
into a bar across the whole window, above the sidebar and the pages. It
switches at once, without a restart.

- From left to right: the sidebar button, back, forward and reload, the
  address, the pinned extensions, and outside macOS minimize, maximize and
  close. On macOS the window's own buttons sit at its left end. The address
  keeps at least 240 points: pinned extensions that do not fit beside it
  wait behind the button after the icons (`extensions.md`).
- The address shows the whole address without `https://` and `www.`; a
  click opens the command bar with it, as in the sidebar. An open peek shows
  its controls in its place.
- The site controls, the copy menu (right-click on the copy button), an
  extension's menu and the list of extensions without an icon open in small
  windows just below the bar.
- The sidebar keeps the rest: favorites, tabs, the footer. Hidden (`Cmd+S`),
  it still slides out over the pages, below the bar.
- Layout → **Top bar, sidebar right** docks the sidebar at the right of the
  window, below the bar. The pages, the edge that slides out the hidden
  sidebar and web panels (`web-panels.md`) move with it. Only with the top
  bar, which has the window's buttons: without it they would lie over the
  pages. Turning the bar off ("Toggle top bar") docks the sidebar at the
  left; turning it on again puts it back at the right.
- Empty space in the bar drags the window; a double-click maximizes it.

## Frame around the page

Settings → Appearance → **Frame around the page** says when a page has the
gap to the window's edges and the sidebar, and its round corners:

- **Always** (the default).
- **Not in full screen**: the pages of a full screen window have none.
- **Never**.

Without the frame a page reaches the edges of the window and the sidebar,
with square corners. On macOS a corner of the page that lies in a corner of
the window follows the window's round corner. The pages of a split keep the
gap between them, where they are resized. A peek keeps its round corners,
and so does a web panel that lies over the pages. It switches at once,
without a restart, and is the same in every window.

## Keyboard

| Action | Shortcut |
|---|---|
| New tab (command bar) | Cmd+T |
| Change the address | Cmd+L |
| Close the tab | Cmd+W |
| Reopen the last archived tab | Cmd+Shift+T |
| The tab used before this one, and further back or forth (below) | Ctrl+Tab / Ctrl+Shift+Tab |
| Tab 1 to 9 of the space (pinned first, then Today) | Cmd+1 to Cmd+9 |
| Pin or unpin the active tab | Cmd+D |
| Show or hide the sidebar | Cmd+S |
| Back / forward / reload | Cmd+[ / Cmd+] / Cmd+R |
| Reload without the cache: the page and its files are fetched anew; cookies and storage stay | Cmd+Shift+R |
| History | Cmd+Y |
| Print the page | Cmd+P |
| Find in the page / next / previous match | Cmd+F / Cmd+G / Cmd+Shift+G |

These are the keys until the user changes them (`shortcuts.md`); the digits,
back, forward and both reloads are fixed. "Show or hide the sidebar", "Pin or
unpin the active tab", print and find go to the page first, which may use
the key itself. The six shortcuts above "Pin or unpin" are taken before the
page sees the key.

`Ctrl+Tab` goes to the tab used before this one, and pressed again, back:
the tabs are taken by last use, the favorites and the tabs of the current
space together. With Ctrl held, each further Tab goes one tab further back
(`Ctrl+Shift+Tab`: the other way), around the nine used last, and after
150 ms a list over the page shows them with the one it stands on marked.
Letting go of Ctrl chooses that tab; choosing a tab or a space some other
way ends the round. The list never takes the keyboard.

| Setting | Values | Default |
|---|---|---|
| Settings → Tabs and archive → "Ctrl+Tab goes through the tabs" | by last use, in sidebar order | by last use |

In sidebar order the two keys go through the rows of the current space in
the order of `Cmd+1` to `Cmd+9`: the pinned tabs, those in open folders
included (closed folders are passed over), then Today; after the last comes the first. The tabs of
a split count one by one, so the keys can leave a split or move within it.
Favorites are not part of the round; from a favorite the keys go to the
first or the last row.

The order of last use is kept while Magnetite runs; after a restart the
tabs count by the time they were last chosen. The setting stays on the
device.

## Data

`<data-dir>/state.json` holds the sidebar (profiles, favorites, spaces with
their pinned tree and Today list), the active space and the active tab of
each space. It is written whole after every change, readable by its owner
only.

Icons: for a page in the default profile the sidebar shows the icon the page
reported (its address is kept per tab in `state.json`). Otherwise it uses
the local cache, `<data-dir>/favicons/<host>.png`, written when a page loads
(never for a private profile). A page of a private profile fetches its icon
itself, in its own context, and the sidebar shows it from memory; it is
not saved. Without an icon the row shows the first
letter of the host. The sidebar never fetches the icon of a site that was
not opened.

## Not built yet

- Changing the home page of a pinned tab or favorite without pinning it
  again, and a "back to home page" control for favorites.

Code: `crates/core/src/{model,sidebar,unread}.rs`, `crates/app/src/app/tabs.rs`,
`crates/app/src/state/{tabs,folders,profiles}.rs`,
`crates/engine/src/{keys,frame,floatbar,favicon}.rs`,
`crates/ui/web/src/{App,Rows}.tsx`.
