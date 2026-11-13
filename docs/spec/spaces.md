# Spaces

A space is a named set of pinned and Today tabs with its own icon, colour
and profile. The sidebar shows one space at a time; the favorites are shared
by all spaces. The profile decides which logins the pages of a space use.

## Properties

| Field | Meaning |
|---|---|
| `title` | The name shown above the tabs. |
| `icon` | One of 27 glyphs, drawn in the space colour. An imported icon without a matching glyph shows as its emoji or as the first letter of the title. |
| `accent` | The space colour. It tints the sidebar and outlines the focused page in split view. |
| `profile_id` | The profile the space's pages run in. |
| `pinned`, `today` | The pinned tree (tabs and folders) and the Today list (`sidebar.md`). |

## Create, change, delete

- **Create**: "+" → New Space or `Cmd+Shift+N` opens the command bar to name
  the space. It is added after the other spaces with the star icon, a violet
  colour and the default profile, and becomes the current space. The first
  start creates one space, "Personal", unless the import fills the sidebar
  (`import.md`).
- **Space menu**: right-click the space title, the empty part of the tab
  list or a space icon in the footer, or use the "…" button on the title.
  It has Rename space, Change icon, New folder, the profile choice and
  Delete space….
- **Rename**: the space menu, or double-click the title. **Icon**: the space
  menu, or click the icon. **Colour**: Settings → Appearance → Space
  colours, with eleven presets and a colour picker.
- **Reorder**: drag a space icon in the footer onto another one. A line
  shows where it goes: before the icon under the pointer on its left half,
  after it on its right half; where it already is, no line shows. The
  current space stays the current one. The order is the one
  `Cmd+Option+Right`, `Cmd+Option+Left` and the swipe follow, and it syncs
  with the sidebar (`sync.md`).
- **Delete**: "Delete space…", then click again to confirm. The last space
  cannot be deleted. The space's pinned and Today tabs move to the archive
  (`archive.md`). If it was the current space, the first space takes over.

## Switching

- Click a space icon in the footer.
- `Cmd+Option+Right` and `Cmd+Option+Left` go to the next and previous
  space; after the last comes the first.
- A two-finger horizontal swipe on the sidebar does the same, one space per
  gesture.
- `Ctrl+1` to `Ctrl+9` go to the first to ninth space, in the order of the
  footer icons. A number without a space does nothing. The keys are taken
  before the page sees them. On Linux they are `Alt+1` to `Alt+9`, since
  `Ctrl` with a number selects a tab there.
- The command "Switch to <space>" in the command bar.
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

- Changing a space's profile (space menu, or Settings → Isolation) closes
  the pages of that space; they load again in the new profile when shown.
- A favorite runs in the profile of the favorites list it is in
  (`sidebar.md`). Its context menu moves it to another profile's list
  (Profile, or New profile for this favorite…).
- A tab or peek opened from a page keeps the profile of that page.
- Magnetite's own pages (`magnetite://`) always run in the default profile.

### Isolation levels

Settings → Isolation presents profiles as three levels, set per space and
per favorite:

| Level | Profile | Logins |
|---|---|---|
| Shared | the default profile | The main logins, shared by everything that is Shared. |
| Separate | a persistent profile | Its own logins, kept between restarts. Choosing it creates a profile named after the item; the picker can switch to another separate profile to share logins. |
| Private | a profile in memory | Its own logins, gone when Magnetite quits. Created per item and removed once nothing uses it. |

A private profile keeps its browsing off the disk: its Today tabs are not
saved, synced or archived, its pinned tabs and favorites are saved with
their home page only, its pages leave no history (`history.md`), and its
icons are not cached (`docs/design/security.md`).

Separate profiles can be renamed in Settings → Isolation, and deleted there
once nothing uses them: no space, no favorite, no mini window, and no tab
or peek that was opened from a page of that profile and still runs in it. The same holds
for the clean-up of private profiles. The data folder of a profile deleted
here is removed at the next start, before any page opens; no other folder
is ever removed, so a state file that could not be read costs no profile
its logins. Deleting counts for this device: with sync on, the other
devices keep the profile and its logins until it is deleted there, and it
does not come back here (`sync.md`).

A space can be kept on one device (Settings → Sync, "Only on this
device"). A separate profile that only such spaces use stays there with
them and is marked "only on this device" in the list (`sync.md`, "Only on
this device").

## Data

- `<data-dir>/state.json`: spaces (in display order) and profiles as part
  of the sidebar, the active space, and the active tab of each space. With
  sync on, spaces and profiles travel with the sidebar (`sync.md`); whether
  a profile is private stays as this device has it. Which spaces are
  kept on this device alone, and which profiles were deleted here, is saved
  next to the sidebar and never synced.
- Profile data: `<data-dir>/cef/default` for the default profile,
  `<data-dir>/cef/profile-<profile-id>` for each separate profile, nothing
  for a private one.

## Not built yet

- Choosing icon, colour and profile while creating a space.
- Gradient themes and a light appearance.

Code: `crates/core/src/model.rs`, `crates/app/src/app/commands.rs`,
`crates/app/src/state/profiles.rs`,
`crates/app/src/state/tabs.rs` (`move_space`),
`crates/ui/web/src/{App,SpaceSwitcher,IsolationSection,Glyphs}.tsx`,
`crates/ui/web/src/useSpaceSwipe.ts`,
`crates/ui/web/src/settings/AppearanceSection.tsx`.
