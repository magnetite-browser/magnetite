# Mini window

A mini window is one page in a small window of its own, outside the main
window's sidebar and spaces. It is for a link from another app and for a
quick look-up from anywhere: read it, close it, or move it into a space as
a tab.

## Opening

| Trigger | Result |
|---|---|
| A link from another app | A mini window, if Settings → General → "Open links from other apps in" says "a mini window" |
| `Cmd+Option+N`, in any app | The quick search (below); Enter opens the pick in a mini window |
| File → New Mini Window (`Cmd+Option+N`) | The same, from Magnetite's menu bar |
| "Open Link in New Window" in a page's menu | A mini window with that link, in the page's profile |

- The default for links from other apps stays "a new tab in the current
  space"; "a peek window" is the third choice (`peek.md`).
- An address Magnetite does not open from that source, such as
  `javascript:` or `data:`, opens nothing (`docs/design/security.md`).
- Several mini windows can be open at once. Each new one opens centred on
  the screen with the pointer, 880 × 680 points at most, a little lower and
  further right than the one before.
- A new mini window comes to the front with the keyboard in its page.

## Quick search

`Cmd+Option+N` opens the command bar (`command-bar.md`) in a window of its
own: 640 points wide, near the top of the screen with the pointer, above
other apps' windows. It finds addresses, web search, site search
(`gh rust`), bookmarks and visited pages; tabs, archived tabs and commands
are left out. Enter opens the pick in a new mini window. `Shift+Enter` and
`Cmd+Enter` do the same as Enter here. Esc, or a click into another window,
closes it; opened from another app and closed without a pick, that app gets
the keyboard back.

- The key works while another app is in front. The system hands Magnetite
  this one combination only; no accessibility permission is needed, and no
  other typing reaches Magnetite. On Windows the key is `Ctrl+Alt+N`.
- Settings → General → "Quick search from anywhere (⌥⌘N)" turns it on or
  off; it is on by default. When another app already holds the combination,
  Magnetite does without it; File → New Mini Window still works while
  Magnetite is in front.

## The window

A row above the page holds, from left to right:

| Control | Does |
|---|---|
| ← → | back and forward in the page's history (dimmed when there is none) |
| ↻ | reload |
| The site's host | the address for a page that is no web page; drags the window |
| "Open in <space>" | keeps the page as a tab of the space in front of the main window (`Cmd+O`) |
| ▾ | a menu of all spaces: "Open in <space>" for each |
| ✕ | close |

- The window has rounded corners and a shadow, no traffic lights, and is
  moved by the host in the row. It takes the page's title as its own.
- The page runs in the profile of the space in front of the main window
  when it opens, like a peek opened by a link from another app.
- A link in the page navigates it. A link that asks for a new tab or window
  follows the rules for pages (`peek.md`): it opens in the main window, as
  a peek or a tab, and the main window comes to the front.
- In the page, `Cmd+W` closes the mini window; `Esc` belongs to the page.
  Reload, back, forward, zoom, Copy Link, Print and developer tools, by key
  or from the menu bar, act on the mini window's page. Developer tools open
  in a window of their own. Every other Magnetite shortcut brings the main
  window to the front and acts there.
- A page that asks for a permission (location, notifications, camera…)
  asks with Chromium's own prompt in the mini window; the notification
  panel belongs to the main window.

## Moving into a space

"Open in <space>", the menu or `Cmd+O` makes the page a Today tab at the
top of that space. The page keeps its window, so it does not reload, and
the tab knows at once what the page was doing (history, media, connection),
as a kept peek does. It stays in its profile, also when that is not the
space's. The main window comes to the front with the tab active, switching
space if needed.

## Closing

✕ or `Cmd+W` closes the window and drops its page: it is not archived and
nothing is saved, like a closed peek. Quitting Magnetite closes every mini
window.

## Data

A mini window lives in memory only: it is not written to `state.json`, not
synced, and gone when Magnetite quits. Its page uses the profile's cookies
and cache like any tab, and its visits go into the history like a tab's
(`history.md`); a private profile's leave none. The choice for links from
other apps is `settings.external_links` (`"tab"`, `"peek"` or `"mini"`) in
`<data-dir>/state.json`; it syncs between devices (`sync.md`). The quick
search switch is `settings.quick_search`, this device's own.

## Limits

- The link preview at the bottom of the page (`site-controls.md`) and the
  address row's site panel are the main window's: a mini window has
  neither.
- No system-wide key on Linux.

Code: `crates/app/src/app/mini.rs` (`open_mini`, `mini_to_tab`,
`mini_shortcut`), `events.rs` (`Ask::target`), `crates/app/src/state/tabs.rs`
(`keep_mini`), `crates/engine/src/mini.rs` (the row, `mini_to_tab`),
`content.rs` (`create_page`), `overlay.rs` (`open_quick_search`),
`mac/hotkey.rs`, `menubar.rs`, `crates/ui/web/src/CommandBar.tsx` (`quick`
mode). Why the window is kept when it becomes a tab: ADR-0010.
