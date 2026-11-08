# Extensions

Chrome extensions run through Chromium's own extension system
([ADR-0003](../adr/0003-extensions.md)) and install from the Chrome Web
Store inside Magnetite. Magnetite adds its own places to reach them:
buttons in the sidebar, an Extensions menu in the menu bar and a section in
Settings.

## Where extensions show

| Place | What it offers |
|---|---|
| Sidebar, top row | Pinned extensions, after the sidebar button. Click: open the extension. Right-click: "Options" (if it has some), "Remove from Sidebar" |
| Menu bar → Extensions (macOS) | Every extension (click opens it), "Show in Sidebar" to pin or unpin each, "Manage Extensions…", "Chrome Web Store…" |
| Settings → Extensions | An "In the sidebar" switch per extension, "Chrome Web Store", "Manage extensions", "Show the Chrome toolbar inside pages" |

- The lists hold enabled extensions that have a toolbar action, sorted by
  name. Chromium's built-in extensions are left out.
- Extensions are per profile. The lists show those of the profile the page
  in front uses: the active tab's profile, else the active space's. A
  private profile lists none.
- The sidebar shows as many pinned icons as fit: one at the default width,
  four at the widest. The menu bar has them all.
- Installing happens on the Chrome Web Store page; removing, turning off
  and site access on Chromium's own page, `chrome://extensions`. Both open
  as tabs. `--load-extension <dir>` (repeatable) loads an unpacked one.
- "Show the Chrome toolbar inside pages" (off by default) adds Chromium's
  own toolbar row, with its extension buttons, above every page. It applies
  to tabs loaded from then on. `--chrome-toolbar` turns it on for one run.

## Opening an extension

- A click runs the extension's action for the page in front: the open peek,
  else the active tab (in a split, the focused page). Its popup opens at
  that page's top-left corner, next to the sidebar, and acts on that page:
  a password manager fills it, a blocker's switch applies to its site.
- With no page open, a notice asks to open one first.
- "Options" opens the extension's options page as a tab, in the profile of
  the page in front, where the extension is installed.
- Pages an extension opens itself (a welcome page, `chrome.tabs.create`, a
  pop-out window) open as Magnetite tabs in the profile they came from.

## Pinning

- Nothing is pinned until you pin it.
- "Pin to toolbar" on Chromium's extensions page counts too. Magnetite
  remembers Chromium's pins per profile; an extension newly pinned there
  joins the sidebar, one unpinned there leaves it. A change made in
  Magnetite afterwards stays until Chromium's pins change again.
- Pins are one list of extension ids for all profiles. A pinned extension
  shows in every profile that has it installed.

## How it works

- **List:** `extensions.rs` reads `extensions.settings` from the profile's
  `Preferences` and `Secure Preferences` for each extension's location and
  state, then its `manifest.json` for the name (localised through
  `_locales`), icon and options page. It reads again when Chromium rewrites
  either file. The sidebar gets the list with every state update and checks
  once a minute; the menu reads it each time it opens. Icons are served as
  `magnetite://ui/ext-icons/<id>`.
- **Opening:** `Extensions.triggerAction` on the private DevTools pipe
  ([ADR-0007](../adr/0007-private-devtools-pipe.md)), for the tab target of
  the page. Every page window is a browser window of its own, so the tab is
  the one in the page's window (`Browser.getWindowForTarget`). It is looked
  up once per page.
- **Anchor:** every page window carries Chromium's toolbar in its own
  views, out of sight: a row one point tall above the page holds a 400 pt
  box that shifts the toolbar up past its top edge. Chromium anchors popups
  and its own bubbles to the extension buttons there.
- **Stray windows:** windows Chromium opens by itself get Magnetite's
  default client (`stray.rs`). The window is hidden at once, its first
  address opens as a tab, and the window closes. A new-tab page opens
  nothing. DevTools that Chromium opens itself (Inspect on a popup) keep
  their window.

## Data

- Pins: `settings.pinned_extensions` in `<data-dir>/state.json`, with
  Chromium's pins as last seen in `settings.chromium_pins_seen`. Neither is
  synced.
- Extensions and their data stay in Chromium's profile folders under
  `<data-dir>/cef`.
- Magnetite's own extension UI reads names and icons from disk and sends
  nothing. The Chrome Web Store is a Google site: installing from it
  contacts Google.

## Limits

- Popup placement follows Chromium: at the page's top-left corner, not
  under the sidebar icon.
- Popups open two seconds or more late: CEF holds each popup's page until a
  timeout ([CEF #4350](https://github.com/chromiumembedded/cef/issues/4350),
  see `docs/development/upstream.md`). A second click during the wait would
  cancel the popup, so a sidebar button ignores clicks until the popup has
  the keyboard, for at most five seconds.
- A tab or window an extension opens with `chrome.tabs.create` or
  `chrome.windows.create` becomes a new Magnetite tab. The id Chromium
  returned to the extension belongs to the closed window.
- Without the DevTools pipe (file descriptors 3 and 4 taken at start, or on
  Windows) the buttons and menu entries open nothing.

## Not built yet

- Badges, the count an extension shows on its icon.

Code: `crates/engine/src/extensions.rs`, `devpipe.rs`, `stray.rs`,
`content.rs` (`attach_toolbar`) and `menubar.rs`;
`crates/app/src/app/extensions.rs` and `crates/app/src/state.rs`
(`follow_chromium_pins`); `crates/ui/web/src/PinnedExtensions.tsx` and
`SettingsPage.tsx`.
