# Settings

Settings are a page of Magnetite's own, `magnetite://ui/settings`, opened as
a tab. A list of sections on the left, the settings of the chosen section on
the right, one card per setting with its name, a description and its control.
What each setting does is in the spec of its feature.

## Search

The field "Search settings" above the list of sections finds a setting
without knowing its section.

- While the field holds text, the page shows every section that holds a
  match instead of one section, each under its name. The settings that
  match look as always; the other settings of these sections are greyed
  out, and so are the sections without a match in the list on the left. No
  section is marked there.
- A setting matches when every typed word is in its name or its
  description. Upper and lower case make no difference.
- A section whose name matches comes first, with nothing greyed out.
- The shortcuts match one by one, by the name of the command; the name of
  a group ("Tabs") matches the whole group.
- All settings shown work where they are, greyed out or not: a switch can
  be flipped in the results.
- A click on a section's name in the results, or on a section in the list,
  opens that section and empties the field. So does Esc in the field, which
  goes back to the section shown before.
- Nothing found: "No setting matches …".

What is searched is what the sections show, so the names of your spaces,
sites and extensions are found where a section lists them. Nothing typed
leaves the page.

## Where the code is

`crates/ui/web/src/SettingsPage.tsx` (sections, the field),
`crates/ui/web/src/settings/SearchResults.tsx` (matching),
`crates/ui/web/src/settings/fields.tsx` (the cards).

## Not built yet

- Marking the matched words in the results.
- A shortcut that puts the cursor into the field.
