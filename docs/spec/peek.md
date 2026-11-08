# Peek

A peek is a temporary page shown over the current tab. It lets you look at
a link without leaving or replacing what is open, and then close it, keep
it as a tab, or put it beside the current tab in split view.

## When a peek opens

| Trigger | Where |
|---|---|
| A click on a link that leaves the tab's site | Pinned tabs and favorites |
| A link that asks for a new tab or window | Pinned tabs and favorites |
| `Shift`+click on a link | Any tab |
| `Shift+Enter` on an address, search, bookmark or visit in the command bar (`command-bar.md`) | Over the page in front |
| A popup window (`window.open` with window features) | Any tab |
| A link from another app | Only if the setting below says so |

- Pinned tabs and favorites are locked to the site of their pinned address
  (the same registrable domain), so the tab itself stays where it is. Only
  clicks count: redirects, such as a sign-in flow, stay in the tab. The
  lock follows the pin state within a minute.
- Links opened in the background (`Cmd`+click, middle click) become
  background Today tabs, never a peek.
- Settings → General → "Open links from other apps in" offers "a new tab in
  the current space" (the default), "a peek window" and "a mini window"
  (`mini-window.md`).
- An address Magnetite does not open from that source, such as
  `javascript:` or `data:`, opens nothing.

## Behaviour

- The peek sits centred over the page area at 86 % of its width (less in a
  narrow window, to leave room for the buttons) and 90 % of its height. On
  macOS the pages behind it are dimmed.
- There is one peek at a time. A new one replaces the open one. Opening a
  tab in front or switching tabs closes it.
- The peek runs in the profile of the page that opened it. A link from
  another app, or one picked in the command bar, runs in the active
  space's profile.
- A link inside the peek navigates the peek. A link there that asks for a
  new tab or window opens as a Today tab, and the peek closes.
- While a peek is open, reload, back and forward (the sidebar's buttons and
  the menu bar's commands), zoom, Copy Link, the capture tool, developer
  tools and extension popups act on the peeked page. `Cmd+W` closes the
  peek; the tab behind it stays open. `Cmd+D` does nothing while a peek is
  in front. The sidebar's back and forward buttons are lit by the peek's
  own history.
- A new address for the tab (`Cmd+L`) and adding a page to a split close
  the peek.

## Actions

| Action | How | Result |
|---|---|---|
| Close | `Esc`, a click on the dimmed area, the close button | The page is dropped: not archived, nothing saved |
| Keep as tab | `Cmd+O`, "Open as tab", "Keep" in the sidebar | A Today tab at the top of the active space |
| Open in split view | The split button | A Today tab right of the current tab |

- On macOS the buttons are a column of round buttons right of the peek.
  The sidebar shows "Peek ·" and the page title in place of the address
  field, with the same three actions.
- `Esc` and `Cmd+O` count while the keyboard is in the peek.
- A kept page reuses the peek's window, so it does not reload, and the tab
  knows at once what the page was doing: its history, what it plays, its
  connection, its use of camera and microphone. It stays in the peek's
  profile, also when that is not the space's profile.
- "Open in split view" opens a plain tab instead when the current tab's
  split is full or no tab is open.

## Data

A peek lives in memory only: it is not written to `state.json`, not synced
and gone when Magnetite quits. Its page uses the profile's cookies and
cache like any tab. The choice for links from other apps is
`settings.external_links` (`"tab"`, `"peek"` or `"mini"`) in
`<data-dir>/state.json`; it syncs between devices.

## Limits

A page opened as a peek or as a new tab loads in a window of its own. It
has no script connection (`window.opener`) to the page that asked for it.

Code: `crates/app/src/app/peek.rs` (`open_peek`, `keep_peek`) and
`events.rs` (the `OpenUrlRequested` handler), `crates/engine/src/client.rs` (site lock,
popups), `content.rs` (`show_peek`, `peek_rect`), `keys.rs`
(`peek_shortcut`), `mac/chrome.rs` (backdrop and buttons),
`crates/ui/web/src/App.tsx` (the sidebar's peek row).
