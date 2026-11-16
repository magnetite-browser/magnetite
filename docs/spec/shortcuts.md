# Shortcuts

Magnetite's commands have keys, and the user can give most of them other
keys, or none.

## Settings → Shortcuts

The page lists every shortcut whose keys can be changed, in four groups,
each with the keys it has now.

| Control | What it does |
|---|---|
| "Change" | waits for the next key press and gives the shortcut those keys; Esc, or leaving the page, cancels |
| "Remove" | the shortcut gets no keys; its command stays in the menu bar and the command bar |
| "Reset" | the shortcut gets its default keys back |
| "Reset all" | every shortcut gets its default keys back |

- While the page waits for keys, no shortcut runs and the menu bar's items
  have no keys, so the keys pressed reach the page. This ends with the key
  press, with Esc, and by itself after 20 seconds.
- Keys another shortcut has: the page names that shortcut and asks. "Use
  here" gives the keys to the new one; the other has none from then on.
  "Reset" works the same way: a shortcut that took the default keys since
  loses them.
- A shortcut needs Cmd, Ctrl or Option, or is a function key: anything else
  would be typing.
- Keys with a fixed meaning are refused, and the page says what has them:
  the system's and the editing keys (copy, paste, undo, quit, hide), and
  the keys of the commands below that cannot be changed.
- New keys count at once, in every window, and show in the menu bar, in
  tooltips, in the "+" menu and in the footer's menu.

## Keys that cannot be changed

Reload, back, forward and zoom (the engine's own), `Cmd+1` to `Cmd+9` (a tab
by its place) and `Ctrl+1` to `Ctrl+9` (a space by its place), the quick
search from anywhere (`mini-window.md`), `F12` and the other two ways to
open DevTools, and the keys inside a peek (`peek.md`).

## Who gets a key first

The shortcuts for tabs, windows and spaces are taken before the page sees
the key, whatever keys they have. The others ("Show or hide the sidebar",
"Pin or unpin the tab", the group "The page" and the group "Magnetite") go
to the page first, which may use the key itself (`site-controls.md`). This
belongs to the shortcut, not to its keys.

With the sidebar or the top bar in focus, a shortcut runs as in a page.

## Sync and other systems

- Changed shortcuts are part of the synced settings (`sync.md`), so they
  are set up once. Only what differs from the defaults is saved and sent.
- Keys are saved with the system's main modifier, not the literal key: Cmd
  on macOS is Ctrl elsewhere. The Control key of a Mac is Ctrl elsewhere
  too; of two shortcuts that end up on the same keys that way, the one
  listed first gets them.
- Keys that are fixed on the system they arrive on are not taken there;
  that shortcut keeps its default keys on that device.
- A device with a version from before changeable shortcuts writes the
  settings without them: the others then go back to the default keys.

## Data

| What | Where |
|---|---|
| The shortcuts with other keys or none: `{"new_tab": "primary+J", "print": null}` | `settings.shortcuts` in `state.json`, and the synced settings |

## Not built yet

- Other keys for the commands under "Keys that cannot be changed".
- Several keys for one shortcut.

Code: `crates/engine/src/keymap.rs` (defaults, checks), `keys.rs` (matching
key presses), `menubar.rs` (`apply_keys`),
`crates/app/src/app/shortcuts.rs`,
`crates/ui/web/src/settings/{ShortcutsSection.tsx,shortcutKeys.ts}`.
