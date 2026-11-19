# Settings

Settings are a page of Magnetite's own, `magnetite://ui/settings`, opened as
a tab. A list of sections on the left, the settings of the chosen section on
the right. What each setting does is in the spec of its feature.

In a narrow page (a small window, a page of a split view, a large text
size: less than 720 points of the page's own), the list of sections is a
bar on top instead: the search field and a menu of the sections. A setting
whose control has no room beside its name shows it below. Narrower than 320
points, the page scrolls sideways.

## Sections

The list has five groups:

| Group | Sections |
|---|---|
| Basics | General, Appearance, Shortcuts |
| Browsing | Tabs & archive, Links, Search, Ad blocking, Permissions, Passwords & autofill |
| Spaces | Spaces, Smart folders |
| Services | Sync, Extensions |
| Advanced | Custom CSS & JS, Developer, Data & privacy, About |

- **General** has what belongs to no feature of its own: windows (several
  windows, web panels, playing videos), where downloads are saved, spell
  check and the languages websites are asked for, the quick search from
  anywhere and the welcome page.
- **Links** has what happens to a link from another app (the default
  browser, where such a link opens, the sites whose links open in a space
  of their own) and to a link that is copied (trackers removed).
- **Tabs & archive** has when tabs are archived and suspended, how long the
  archive keeps them, the order of Ctrl+Tab and what the archive button
  opens.
- **Spaces** lists the spaces, the profiles and the favorites, and has a
  page for each space (below).
- **Permissions** lists every site with a choice of its own for the
  camera, microphone, location, notifications, pop-ups or the clipboard
  (`site-controls.md`).
- **Data & privacy** has the history (how long it is kept, clearing it),
  deleting the cookies, site data and cache of all websites
  (`site-controls.md`),
  the import from another browser or a bookmarks file, the export of the
  bookmarks (`bookmarks.md`), the servers
  Magnetite talks to, each with what for, what websites are told
  (`global-privacy-control.md`), and where the data lives.

The address names the section: `magnetite://ui/settings#links`.

## Spaces

The section starts with three groups:

- **Spaces**: every space in the order of the sidebar, each with its icon,
  its profile, "only on this device" and its keys where it has them. A
  click on a space opens its page. A space is moved by dragging it (its
  handle on the left, or the row): a line shows where it lands. With the
  handle focused, the up and down arrow keys move it. "A new space" adds
  one by name, after the others, without leaving Settings.
- **Profiles**: every lasting profile with what uses it (spaces and
  favorites), to rename, and to delete once nothing uses it
  (`spaces.md`). "A new profile" adds an empty one by name, to give to
  a space or a favorite afterwards.
- **Favorites**: every favorite, to name, to sort inside the list of its
  profile (drag its handle, or the arrow keys on it), to show in every
  space, in one or in several, to give a profile, and to remove (its page
  is in the archive afterwards). "A new favorite" adds one by its
  address (`sidebar.md`).

The page of a space (`magnetite://ui/settings#spaces=<id>`, headed by the
space's name, "All spaces" and the browser's Back lead to the list) has
all that is the space's own: its name and icon (a glyph, or a picture of
your own), its look (`spaces.md`),
its profile, its bookmarks (how many, deleting them all, and its pinned
tabs copied or moved to them, `bookmarks.md`), whether it stays on this
device (`sync.md`), where its keys and its links from other apps are set,
and "Delete space…".

What changes the spaces elsewhere (the sidebar, another window) shows on
an open Settings page at once.

Where a section is about one profile (Permissions, Passwords & autofill,
Extensions, deleting browsing data) and there is more than one lasting
profile, a list chooses which, starting with the profile of the space in
front. Each choice names the profile and the spaces that use it: "Main
profile (Personal, Reading)".

## How a section looks

Settings that belong together stand in one card, a group, under its name
where it has one, with a line about the group where that helps. Each
setting is a row of the card: its name and a description, and its control.
A short control (a switch, a choice from a list, one or two buttons) sits
to the right of the name; a wide one (a list to edit, a text to type)
below it. A choice of two or three short answers is a row of buttons, one
of more or longer answers a list to open.

The groups of Shortcuts fold: a click on a group's name shows or hides its
commands, and the name says how many there are. The first group is open
when the section opens, the others closed. A search shows the commands of
a folded group all the same.

## Search

The field "Search settings" above the list of sections finds a setting
without knowing its section.

- While the field holds text, the page shows every section that holds a
  match instead of one section, each under its name. The settings that
  match look as always; the other settings of these sections are greyed
  out, and so are the sections without a match in the list on the left. No
  section is marked there.
- A setting matches when every typed word is in its name or its
  description; the words are marked where they stand in a setting that
  was found. Upper and lower case make no difference. The name of a
  group ("Windows") matches all its settings.
- A section whose name matches comes first, with nothing greyed out.
- The shortcuts match one by one, by the name of the command; the name of
  a group ("Tabs") matches the whole group.
- All settings shown work where they are, greyed out or not: a switch can
  be flipped in the results.
- A click on a section's name in the results, or on a section in the list,
  opens that section alone and keeps the search: its settings that match
  look as always, the others are greyed out, and the list on the left
  still greys out the sections without a match. A section without any
  match shows all its settings greyed out. "Everything for …" above the
  section goes back to the results, and so does typing.
- Esc in the field empties it and shows the section shown last.
- Nothing found: "No setting matches …".

What is searched is what the sections show, so the names of your spaces,
sites and extensions are found where a section lists them. Nothing typed
leaves the page.

## Where the code is

`crates/ui/web/src/SettingsPage.tsx` (sections, the field),
`crates/ui/web/src/settings/SearchResults.tsx` (matching),
`crates/ui/web/src/settings/fields.tsx` (groups, rows and their controls),
`crates/ui/web/src/settings/SpacesSection.tsx` (the spaces, their pages,
profiles and favorites).

## Not built yet

- A shortcut that puts the cursor into the field.
