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

The list starts with the overall settings, which stand under no name,
and has four named groups after them:

| Group | Sections |
|---|---|
| (none) | General, Appearance, Shortcuts |
| Browsing | Tabs and archive, Links, Search, Content blocking, Protected video, Permissions, Passwords and autofill, User scripts and styles |
| Organization | Spaces, Profiles, Favorites, Bookmarks, Smart folders |
| Services | Sync, Extensions |
| Advanced | Developer tools, Data and privacy, About |

A section's name has no "&": "Tabs and archive".

- **General** has what belongs to no feature of its own: windows (several
  windows, the private space's own window, web panels, playing videos),
  where downloads are saved, spell
  check and the languages websites are asked for, the quick search from
  anywhere and the welcome page.
- **Links** has what happens to a link from another app (the default
  browser, where such a link opens, the sites whose links open in a space
  of their own) and to a link that is copied (trackers removed).
- **Tabs and archive** has when tabs are archived and suspended, how long the
  archive keeps them, the order of Ctrl+Tab and what the archive button
  opens.
- **Spaces** lists the spaces and has a page for each space (below).
- **Profiles**, **Favorites** and **Bookmarks** are below too.
- **Permissions** lists every site with a choice of its own for the
  camera, microphone, location, notifications, pop-ups or the clipboard
  (`site-controls.md`).
- **Data and privacy** has the history (how long it is kept, clearing it),
  deleting the cookies, site data and cache of all websites
  (`site-controls.md`),
  the import from another browser (`import.md`; a bookmarks file is
  imported and exported in Bookmarks), the servers
  Magnetite talks to, each with what for, what websites are told
  (`global-privacy-control.md`), and where the data lives.

The address names the section: `magnetite://ui/settings#links`.

## Spaces, Profiles, Favorites and Bookmarks

Four sections of the group Organization:

- **Spaces**: every space in the order of the sidebar, each with its icon,
  its profile, "only on this device" and its keys where it has them. A
  click on a space opens its page. A space is moved by dragging it (its
  handle on the left, or the row): it follows the pointer, with the
  hand closed, and the other spaces make room where it lands; Esc gives
  up. With the
  handle focused, the up and down arrow keys move it. "A new space" adds
  one by name, after the others, without leaving Settings, in the profile
  chosen beside the name (the choices of "Choosing a profile" in
  `spaces.md`); the choice stays for the next space.
- **Profiles**: every lasting profile with what uses it, in two columns:
  "Used by spaces" and "Used by favorites" (a space and a favorite may
  have the same name). A space named there opens its page. A profile is
  renamed there, and deleted once nothing uses it (`spaces.md`). "A new
  profile" adds an empty one by name, to give to a space or a favorite
  afterwards. The section says that a profile may be for favorites only,
  and what that means for logins and for the history. In a narrow page
  the two columns are two lines, each with its name in front.
- **Favorites**: every favorite, to name, to sort inside the list of its
  profile (drag its handle, or the arrow keys on it), to show in every
  space or in the spaces ticked (below), to give a profile, and to remove (its page
  is in the archive afterwards). "A new favorite" adds one by its
  address (`sidebar.md`).
  With favorites in more than one profile, each profile's list is
  headed by the profile's name, in the order of the profiles. The handle
  of a profile's only favorite is greyed out and says why.
  Where a favorite shows is a button with its spaces as tags, each with
  the space's icon in the space's colour: "Every space", or as many tags
  as have room and then how many more ("+3"; all their names when the
  pointer rests on it). A click opens a list of the spaces to tick, as
  many as wanted, which stays open meanwhile; "Every space" on top
  unticks them all, and so does unticking the last one.
- **Bookmarks**: the bookmarks of one space, chosen from a list that
  starts with the space in front (`magnetite://ui/settings#bookmarks=<id>`
  starts with that space): how many, "Add pinned tabs to bookmarks" (copy
  or move) and "Delete bookmarks…"; and the bookmarks as a file, to import
  into that space and to export (`bookmarks.md`).

The page of a space (`magnetite://ui/settings#spaces=<id>`) is headed by
the space's own icon and name, with a line that says the page is about
this space alone. "All spaces", above the name, and the browser's Back
lead to the list. The page has all that is the space's own: its name and
icon (a glyph, or a picture of your own), its look (`spaces.md`), its
profile, how many bookmarks it has (with a button to Bookmarks, opened
at this space), whether it stays on this device (`sync.md`), where its
keys and its links from other apps are set, and "Delete space…".

What changes the spaces elsewhere (the sidebar, another window) shows on
an open Settings page at once.

Where a section is about one profile (Permissions, Passwords and autofill,
Extensions, deleting browsing data) and there is more than one lasting
profile, a list chooses which, starting with the profile of the space in
front. Each choice names the profile and, beside it, the spaces that use
it: "Main profile", "Personal, Reading". A profile named after its only
space does not name it again.

A profile is chosen from a list of the same kind (a favorite's, a
space's on its page, a new space's): the lasting profiles, each with the
spaces that use it, then "A new profile" and "Private". The space whose
bookmarks show is chosen so too, as a tag, and so is every other space
in Settings: the one a site's links open in (Links), the one a bookmarks
file goes into and the ones to export. The arrow keys go through such a
list, Esc closes it.

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
profiles and favorites),
`crates/ui/web/src/settings/BookmarksSection.tsx` (bookmarks),
`crates/ui/web/src/settings/sorting.ts` (sorting rows by dragging),
`crates/ui/web/src/settings/PickList.tsx` (the lists of spaces and profiles).

## Not built yet

- A shortcut that puts the cursor into the field.
