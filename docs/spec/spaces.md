# Spaces

A space is a named set of pinned and Today tabs with its own icon, colour
and profile. The sidebar shows one space at a time; a favorite is shared
by all spaces unless it is for some only (`sidebar.md`). The profile decides which logins the pages of a space use.

## Properties

| Field | Meaning |
|---|---|
| `title` | The name shown above the tabs. |
| `icon` | One of 27 glyphs, drawn in the space colour. An imported icon without a matching glyph shows as its emoji or as the first letter of the title. |
| `icon_image` | A picture of the user's own shown in place of the glyph: a WebP file of at most 4 KB, as base64 ("Own icon" below). |
| `accent` | The space colour. It tints the sidebar and outlines the focused page in split view. |
| `profile_id` | The profile the space's pages run in. |
| `shortcut` | The keys that go to the space, if it has some of its own (`shortcuts.md`). |
| `pinned`, `today` | The pinned tree (tabs and folders) and the Today list (`sidebar.md`). |

## Create, change, delete

- **Create**: "+" → New Space or `Cmd+Shift+N` opens the command bar to name
  the space. It is added after the other spaces with the star icon, a violet
  colour and the default profile, and becomes the current space. The first
  start creates one space, "Personal", unless the import fills the sidebar
  (`import.md`). "A new space" in Settings → Spaces creates one too, in
  the profile chosen beside its name (Main profile, one by its name, a new
  one or Private, as in "Choosing a profile"), and stays in Settings.
- **Space menu**: right-click the space title, the empty part of the tab
  list or a space icon in the footer, or use the "…" button on the title.
  It has Space settings…, New folder, the profile choice and Delete
  space….
- **Space settings**: Space settings… in the space menu switches to the
  space and opens a dialog over the page area with its name, icon, colour,
  colour strength, background and own pictures ("Look" below). Each
  change applies at once; Esc, Done or a click elsewhere closes the
  dialog. The private space has no such dialog.
- **Own icon**: the dialog and the space's page in Settings take a
  picture of your own, a PNG or an SVG for one, in place of the glyph. It is drawn into a square of 64 px (it
  shows at 15 to 18 points) and kept as WebP in the space itself, never as
  the SVG itself and without the file's metadata. So it syncs with the
  space, and it is small for that reason: at most 4 KB, an icon and not a
  way to pass pictures around. A picture with more detail than that is
  stored coarser, then at 48 and 32 px; the app refuses anything larger or
  not WebP, from a page and from another device alike. A version that does
  not know the field drops the picture when it writes the sidebar.
  Choosing a glyph removes it, in the dialog, in Settings and from the
  icon beside the title.
- **Rename**: the dialog, or double-click the title. **Icon**: the dialog,
  or click the icon. **Colour**: the dialog, with eleven presets and a
  colour picker.
- Settings → Spaces does the same without the sidebar: add, name, sort and
  delete spaces, and all of one space on its page (`settings.md`), the
  dialog's settings among them. All settings… in the dialog opens that
  page.
- **Reorder**: drag a space icon in the footer onto another one. A line
  shows where it goes: before the icon under the pointer on its left half,
  after it on its right half; where it already is, no line shows. The
  current space stays the current one. The order is the one
  `Cmd+Option+Right`, `Cmd+Option+Left` and the swipe follow, and it syncs
  with the sidebar (`sync.md`).
- **Delete**: "Delete space…", then click again to confirm. The last space
  cannot be deleted. The space's pinned and Today tabs move to the archive
  (`archive.md`). If it was the current space, the first space takes over.

## Look

The page of a space in Settings → Spaces has its look:

- **Colour**: the space colour (`accent`). It syncs with the space.
- **Colour strength**: how much of the space colour the window takes, 0 to
  30 percent; 8 unless changed.
- **Background**: None, or an effect in the space colour: Grain
  (fine noise), Glow (soft patches in two corners), Aurora (the space colour
  and two neighbouring hues) and Fade (the colour running out from the top);
  the last three carry the grain too. Or a picture of your own, which lies
  under a veil of the sidebar's colour so the rows stay readable. An
  effect is drawn once across the whole window: the sidebar, the frame
  around the pages, the gaps of a split and the top bar each show their
  part of it, with no edge between them. A picture stays in the sidebar.
  Each page casts a soft shadow on the frame around it; the brighter the
  frame, the more of it shows, so on a space without tint or effect it is
  hardly there.
- **Empty window**: a picture of your own behind the window while no tab of
  the space is on screen, in place of the logo.

A picture is scaled down (1600 points on its longer side for the sidebar,
2880 for the empty window) and stored as WebP, which leaves out its metadata
such as place and camera. Strength, background and pictures are kept on this
device and are not synced: the settings in `state.json`
(`settings.appearance.looks`), the pictures in
`<data-dir>/appearance/`, readable by the owner only. Only the Settings page
and the space dialog may store or remove a picture, a space's own icon
included. Deleting a space removes
its look and its pictures.

## Switching

- Click a space icon in the footer.
- `Cmd+Option+Right` and `Cmd+Option+Left` go to the next and previous
  space; after the last comes the first.
- A two-finger horizontal swipe on the sidebar does the same, one space per
  gesture. The list goes along a little with the fingers, and the new space
  slides in from the side swiped toward. A gesture that starts vertically
  scrolls the list and never switches.
- `Ctrl+1` to `Ctrl+9` go to the first to ninth space, in the order of the
  footer icons. A number without a space does nothing. The keys are taken
  before the page sees them. On Linux they are `Alt+1` to `Alt+9`, since
  `Ctrl` with a number selects a tab there.
- The command "Switch to <space>" in the command bar.
- The list of all spaces ("The list of spaces" below).
- Keys of its own for a space (Settings → Shortcuts, `shortcuts.md`).

## The list of spaces

`Ctrl+0` (`Alt+0` on Linux; the keys can be changed), "Show All Spaces…" in
the Spaces menu and the command "Show all spaces…" open a list of every
space in the place of the command bar: icon and name in the space colour,
"Current" beside the space the window shows, the profile where the spaces
do not all use the same one, and the keys that go to the space (its own, or
the digit keys of its place).

- The list starts on the current space. Up and Down move, Enter goes to the
  selected space, Esc closes the list.
- Typing shortens the list to the spaces whose name contains the text:
  first those whose name starts with it, then those where a word does, then
  the others, each in the footer's order. The first one is selected.
- A hidden private space is not listed.

- Showing a tab of another space, for example from the command bar or the
  archive, switches to its space.

Switching changes the sidebar lists, the colour and the page on screen. The
space shows the tab that was last active in it, if that tab is still in the
space or is a favorite; otherwise its first tab, pinned before Today. An
empty space shows no page. Pages of other spaces stay loaded in the
background.

## Profiles

A profile is an isolated browsing context: cookies, logins, site storage and
extensions. Every space uses exactly one profile, and several spaces may
share one. The default profile always exists.

- Changing a space's profile (space menu, or Settings → Spaces) closes
  the pages of that space; they load again in the new profile when shown.
- A favorite runs in the profile of the favorites list it is in
  (`sidebar.md`). Its context menu moves it to another profile's list
  (Profile, or New profile for this favorite…).
- A tab or peek opened from a page keeps the profile of that page.
- Magnetite's own pages (`magnetite://`) always run in the default profile.
- The default profile is the same at every start: Chromium's `Default`
  folder, whichever profile a window was last used in. (Chromium by
  itself starts with the profile used last, which would run every space
  of the default profile in another profile's cookies and extensions.)

### Choosing a profile

Settings names profiles as such. A space (on its page in Settings →
Spaces, or when it is added there) and a favorite (in Settings →
Favorites) choose one of:

| Choice | Profile | Logins |
|---|---|---|
| Main profile | the default profile | The main logins, shared by everything in the main profile. |
| A profile by its name | a persistent profile | Its own logins, kept between restarts, shared by all that choose it. "A new profile" makes one named after the item. |
| Private | a profile in memory | Its own logins, gone when Magnetite quits. Created per item and removed once nothing uses it. |

Settings → Profiles lists every lasting profile with the spaces that use
it and the favorites that do, each in a column of their own, so what
shares logins shows in one place. A profile needs no space: one that only
favorites use keeps their logins apart from every space's.

A private profile keeps its browsing off the disk: its Today tabs are not
saved, synced or archived, its pinned tabs and favorites are saved with
their home page only, its pages leave no history (`history.md`), and its
icons are not cached or saved, only shown while its pages are open
(`docs/design/security.md`).

Persistent profiles can be renamed in Settings → Profiles, and deleted there
once nothing uses them: no space, no favorite, no mini window, and no tab
or peek that was opened from a page of that profile and still runs in it. The same holds
for the clean-up of private profiles. The data folder of a profile deleted
here is removed at the next start, before any page opens; no other folder
is ever removed, so a state file that could not be read costs no profile
its logins. Deleting counts for this device: with sync on, the other
devices keep the profile and its logins until it is deleted there, and it
does not come back here (`sync.md`).

## Private space

A space for browsing that leaves nothing behind. There is at most one, and
it is the last of the spaces while it is open.

- **Open**: "+" → Private Space, `Cmd+Shift+P`, File → Private Space, or
  "Open private space" in the command bar. If it is open already, these go
  to it, also when it is hidden.
- **Look**: a mask as its icon, in the footer and beside its name
  "Private". In place of the "…" button its title has a hide button and a
  close button, and
  "New Tab" reads "New Private Tab". It has only Today tabs: no pinned tabs,
  folders or bookmarks, and the favorites do not show there.
- **A profile per tab**: every tab you open in it (New Private Tab, an
  address from the command bar) runs in an in-memory profile of its own, so
  it shares no cookies, logins or storage with another such tab, nor with
  any other space. What a private tab's page opens (a link in a new tab, a
  sign-in window, a peek, a mini window) stays in that tab's profile and
  shares its cookies and logins: a sign-in window has to. Closing the space
  closes all of them, popup windows included.
- **Nothing is kept**: the space, its tabs and its profiles are not saved
  and not synced. Its pages leave no history, its closed tabs do not go to
  the archive, and a tab's profile is dropped with its data once the tab
  and everything opened from it are closed.
- **Tabs stay put**: a tab cannot be moved into the private space or out of
  it, pinned, or added to the favorites or bookmarks. A peek or mini window
  kept as a tab stays on its side too: a private one becomes a tab of the
  private space, any other a tab of the space beside it. The same holds for
  a tab that a page opens while another space is in front.
- **Extensions**: only those with "Allow in Incognito" switched on
  (`extensions.md`).
- **Hide**: the hide button on its title, its menu in the footer,
  `Cmd+Shift+D`, File → Hide Private Space, or "Hide private space" in the
  command bar. The keys work from any space and before the page sees them,
  and one hand reaches them, like those that close the space.
  The space stays open with its tabs and their logins, but nothing shows
  that it is there: it is gone from the footer, its tabs from the command
  bar's results and the media controls, the keys for the next, previous
  and numbered space pass it over, and "Close private space" is not
  offered. A window that showed it goes on in the space before it. Its
  pages are paused, and a video of theirs in picture-in-picture goes back
  into its page. A tab that one of its pages opens meanwhile opens in the
  background and stays hidden with it. Opening the private space (see Open)
  shows it again as it was, and so does keeping one of its mini windows as
  a tab. Its mini windows and popup windows are windows of their own and
  stay on screen; closing the space closes them.
- **Close**: the close button on its title, its menu in the footer,
  `Cmd+Shift+X`, File → Close Private Space, or "Close private space" in
  the command bar. The keys work from any space, also while the space is
  hidden, and before the page sees them, and nothing asks first: the space
  is gone at once. Its
  tabs close, with the peeks and mini windows opened from them, and the
  space before it takes over.
  Quitting Magnetite closes it too: the next start goes on in the space
  before it.
- **Several windows** (Settings → General): one window lists the private
  space, the one it was opened in. The other windows do not show it, its
  tabs, or its entries in the command bar. Opening it from another window
  moves it into that window's sidebar, and the window it was in goes on in
  the space before it. Closing the window it is in closes it, unless that
  is the last window on macOS, which only goes out of sight. A new window
  opened from it starts in the space before it.
- **Its own window**: with Settings → General → "Private space in its own
  window" (shown while several windows are on), opening the private space
  opens a new window that lists it alone. Opening it again brings that
  window to the front. A space chosen in it (a new space, its keys) shows
  in another window, a new one if there is none, and so does a link from
  another app. Hiding the space takes the window out of sight, and brings
  another window to the front, a new one if there is none. Closing the
  window closes the space, also as the last window, and closing the space
  closes the window, unless it is the last one. The window does not open
  again after a restart.

A space can be kept on one device (Settings → Sync, "Only on this
device"). A separate profile that only such spaces use stays there with
them and is marked "only on this device" in the list (`sync.md`, "Only on
this device").

## Links from other apps

A link from another app opens in the space in front, unless a rule names
another space for its site. It never opens in the private space: with that
in front, the space beside it comes to the front for the link.

| Setting | Values |
|---|---|
| Settings → Links → "Links to these sites open in a space of their own" | a list of rules: a site and a space |

- A site is a host (`github.com`) or a host and the start of a path
  (`github.com/acme`). Typed as a whole address, its host and path are
  kept, in lower case and without `www.`.
- A rule fits a link on its host or a subdomain of it, under its path:
  `github.com/acme` fits `github.com/acme/app`, not `github.com/acmecorp`.
  Of several rules that fit, the one with the longest host counts, then
  the longest path.
- A link with a rule opens as a new tab in the rule's space, which comes
  to the front, whatever Settings → Links → "Open links from other apps
  in" says (`peek.md`, `mini-window.md`): that setting is for links
  without a rule. Of several links handed over at once as peeks, only the
  last is one; the others become background tabs, each in the space its
  rule names. Links without a rule open in the space that was in front
  when they arrived, also after one with a rule brought its space forward.
- A rule whose space is gone does nothing, and a less exact rule that fits
  counts in its place; Settings marks it until another space is chosen or
  the rule removed.
- At most 100 rules, one per site. They stay on the device.

## Data

- `<data-dir>/state.json`: spaces (in display order) and profiles as part
  of the sidebar, the active space, and the active tab of each space. With
  sync on, spaces and profiles travel with the sidebar (`sync.md`); whether
  a profile is private stays as this device has it. Which spaces are
  kept on this device alone, and which profiles were deleted here, is saved
  next to the sidebar and never synced.
- Profile data: `<data-dir>/cef/Default` for the default profile,
  `<data-dir>/cef/profile-<profile-id>` for each separate profile, nothing
  for a private one.

## Not built yet

- Choosing icon and colour while creating a space, and a profile when
  it is created from the sidebar.
- A light appearance.
- Syncing a space's look.

Code: `crates/core/src/model.rs`, `crates/app/src/app/commands.rs`,
`crates/app/src/state/profiles.rs`, `crates/app/src/state/private.rs`,
`crates/app/src/state/tabs.rs` (`move_space`),
`crates/ui/web/src/{App,SpaceSwitcher,SpaceDialog,Glyphs}.tsx`,
`crates/ui/web/src/settings/SpacesSection.tsx`,
`crates/ui/web/src/useSpaceSwipe.ts`,
`crates/ui/web/src/settings/AppearanceSection.tsx`,
`crates/ui/web/src/{backgrounds.ts,Empty.tsx}`, `crates/engine/src/empty.rs`.
