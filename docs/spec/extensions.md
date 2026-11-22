# Extensions

Chrome extensions run through Chromium's own extension system
([ADR-0003](../adr/0003-extensions.md)) and install from the Chrome Web
Store inside Magnetite. Magnetite adds its own places to reach them:
buttons in the sidebar, an Extensions menu in the menu bar and a section in
Settings.

## Where extensions show

| Place | What it offers |
|---|---|
| Sidebar, top row | Pinned extensions, after the sidebar button. Click: open the extension. Right-click: "Options" (if it has some), "Remove from Sidebar" ("Remove from Top Bar" while the top bar holds the icons) |
| Sidebar, extension bar | With Settings → Appearance → "Extension bar" on: the same icons in a row of their own below the address, not in the top row. A button at its right edge lists the extensions without an icon |
| Top bar | The same icons, after the address. A button after them lists the extensions without an icon |
| Menu bar → Extensions (macOS) | Every extension (click opens it), "Show in Sidebar" to pin or unpin each, "Manage Extensions…", "Chrome Web Store…" |
| Settings → Extensions | With more than one lasting profile, the profile the page is about; its extensions with a switch for the sidebar; what only other profiles have, with "Add to <profile>"; "Chrome Web Store", "Manage extensions", "Show the Chrome toolbar inside pages" |

- The lists hold enabled extensions that have a toolbar action, sorted by
  name. Chromium's built-in extensions are left out. An extension that was
  just removed or turned off leaves the lists at once, though Chromium
  writes that to disk some seconds later; a new one shows once it is
  written. Settings → Extensions looks again every few seconds while it
  is open.
- Extensions are per profile. The lists show those of the profile the page
  in front uses: the active tab's profile, else the active space's.
- An extension is installed profile by profile; Chromium has no install
  that counts for several. With more than one lasting profile, Settings →
  Extensions asks which profile it is about (`settings.md`), starting
  with that of the space in front, so every profile's extensions are
  reached from any space. It lists the extensions of the profile chosen,
  each with its switch for the sidebar, and below them "In your other
  profiles": what only other profiles have, each with those profiles and,
  when it is from the Chrome Web Store, "Add to <profile>": the
  extension's store page opens as a tab that runs in the profile chosen,
  where the user confirms the install. The copies are separate: each has
  its own data and its own on and off.
- "Chrome Web Store" and "Manage extensions" there open as a tab that runs
  in the profile chosen, so what is installed, turned off or removed there
  counts for it. The switches are the pins of the profile chosen. With one profile and several
  spaces the page says that all spaces share its extensions.
- A private profile (a space or favorite set to "Private" and the tabs of the
  private space, `spaces.md`) has no extensions of its own. Those of the
  default profile with "Allow in Incognito" switched on (Manage extensions
  → Details) run in it and are listed, without "Options"; the others do not
  run there. Such an extension works on its own data as everywhere else: a
  password manager stays unlocked. It sees the pages of every private tab it
  runs in.
- The sidebar shows as many pinned icons as fit: on macOS, where the row
  keeps room for the window's buttons, two at the default width and five
  at the widest; elsewhere four and eight. The menu bar has them all.
- The top bar shows as many pinned icons as fit beside the address. A
  button after them (a puzzle piece) shows while an extension has no icon
  there, and takes one of the places. Its menu lists the pinned extensions
  that did not fit, then the unpinned ones; a click opens the extension as
  its icon would. "Manage extensions" opens Settings → Extensions.
  The address comes first: in a window too narrow for a single place the
  bar shows neither icons nor the button. On macOS the menu bar still has
  them all.
- The extension bar (Settings → Appearance → "Extension bar", off by
  default, offered while the layout is "Sidebar") moves the sidebar's
  icons into one row below the address: as many pinned icons as fit, seven
  at the default width and ten at the widest. The button at its right edge
  (a puzzle piece) always shows and opens the same menu as the top bar's.
  The row is hidden while no extension is installed.
- Installing happens on the Chrome Web Store page; removing, turning off
  and site access on Chromium's own page, `chrome://extensions`. Both open
  as tabs. `--load-extension <dir>` (repeatable) loads an unpacked one.
- "Show the Chrome toolbar inside pages" (off by default) adds Chromium's
  own toolbar row, with its extension buttons, above every page. It applies
  to tabs loaded from then on. `--chrome-toolbar` turns it on for one run.

## Opening an extension

- A click runs the extension's action for the page in front: the open peek,
  else the active tab (in a split, the focused page). Its popup opens at
  that page's top-left corner, next to the sidebar (with the top bar on, at
  its top-right corner, below the top bar's buttons), and acts on that page:
  a password manager fills it, a blocker's switch applies to its site.
- On a page the system's engine shows (`protected-video.md`) the popup
  opens the same way on macOS and is told the page's address, but the
  extension does not reach the page: nothing is filled in.
- That page is also what Chromium tells the extension when it asks for
  the tab in front (the active tab of the window focused last): to
  Chromium every page is a window of its own, and the page is made the one
  focused last as its action runs. Between two actions an extension that
  asks in the background is told the page of the last action; on macOS a
  click into a page or choosing a tab makes it the one focused last too, on
  Linux it does not.
- With no page open, a notice asks to open one first.
- "Options" opens the extension's options page as a tab, in the profile of
  the page in front, where the extension is installed.
- Pages an extension opens itself (a welcome page, `chrome.tabs.create`, a
  normal window) open as Magnetite tabs in the profile they came from (on
  Linux, a normal window showing the extension's own page stays a window,
  see Limits). A
  tab from `chrome.tabs.create` is the tab the extension got back: its id
  stays good for `chrome.tabs.get`, `update` and `remove`, and
  `active: false` opens it in the background, marked unread. A
  pop-up window (Bitwarden's passkey window) opens as a small window of its
  own, as in other browsers.

## Pinning

- Nothing is pinned until you pin it.
- "Pin to toolbar" on Chromium's extensions page counts too, the moment
  it is toggled (Magnetite watches Chromium's pin preference). Magnetite
  remembers Chromium's pins per profile; an extension newly pinned there
  joins the sidebar, one unpinned there leaves it. A change made in
  Magnetite afterwards stays until Chromium's pins change again.
- Pins are per profile, as extensions are: each lasting profile has its
  own list of extension ids, in order, and the sidebar shows that of the
  profile the page in front uses. A pin made in one profile is none in
  another. A profile that never had a pin changed shows those of the
  default profile (which is also what a data folder from before per-profile
  pins goes on showing everywhere, until the first change). A private
  profile shows the default profile's.

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
  up once per page. `Target.activateTarget` on that tab goes first: it
  activates the page through Chromium's own window code
  (`BrowserView::Activate`), which moves it to the front of Chromium's
  order of activation at once. Chromium learns that from the system's
  window activation otherwise, which never reaches a page window on Linux.
- **Anchor:** every page window carries Chromium's toolbar in its own
  views, out of sight: a row one point tall above the page holds a 400 pt
  box that shifts the toolbar up past its top edge. Chromium anchors popups
  and its own bubbles to the extension buttons there.
- **Stray windows:** windows Chromium opens by itself get Magnetite's
  default client (`stray.rs`). The window is hidden at once, its first
  address opens as a tab, and the window closes. A new-tab page opens
  nothing. DevTools that Chromium opens itself (Inspect on a popup) keep
  their window, and so does an extension's pop-up window
  (`chrome.windows.create({type: "popup"})`, such as Bitwarden's passkey
  window): it is shown with the keyboard, as in other browsers, because
  the extension waits for it and gives up when it closes. On macOS a
  pop-up window is told apart by its window lacking a tab strip (no
  full-size content view). On Linux, where CEF does not say what kind of
  window Chromium made, every such window showing an extension's own page
  (`chrome-extension://`) stays a window; a pop-up window showing a web
  page still becomes a tab.

## Data

- Pins: `settings.pinned_extensions` (the default profile) and
  `settings.profile_pins` (the others) in `<data-dir>/state.json`, with
  Chromium's pins as last seen in `settings.chromium_pins_seen`. Neither is
  synced.
- Extensions and their data stay in Chromium's profile folders under
  `<data-dir>/cef`.
- Magnetite's own extension UI reads names and icons from disk and sends
  nothing. The Chrome Web Store is a Google site: installing from it
  contacts Google.

## Limits

- Popup placement follows Chromium: at a top corner of the page, not
  under the icon itself.
- Popups open two seconds or more late: CEF holds each popup's page until a
  timeout ([CEF #4350](https://github.com/chromiumembedded/cef/issues/4350),
  see `docs/development/upstream.md`). A second click during the wait would
  cancel the popup, so a sidebar button ignores clicks until the popup has
  the keyboard, for at most five seconds.
- A normal window an extension opens with `chrome.windows.create` becomes
  a new Magnetite tab that loads the address again. The id Chromium
  returned to the extension belongs to the closed window. On Linux a normal
  window showing the extension's own page (`chrome-extension://`) stays a
  window, and its id stays good.
- With no page open, `chrome.tabs.create` fails ("No current window") and
  opens nothing.
- `chrome.tabs.create` ignores `index` and `pinned`: the tab goes where
  Magnetite puts new tabs. With the official CEF (systems Magnetite's own
  CEF build is not made for) such a tab still loads again under another id.
- A tab an extension closes with `chrome.tabs.remove` stays in the sidebar
  without its page, as a tab does whose page closed itself; it loads again
  when chosen.
- An extension's pop-up window is a plain Chromium window outside
  Magnetite's layout. On Linux an extension's own page in a normal window
  stays a window too: an options page meant to open in a tab,
  `chrome.windows.create({type: "normal"})`.
- The page in front is the window focused last only with Magnetite's CEF
  (`extension-last-active.patch` in magnetite-cef). Without it Chromium
  counts every page as active whenever the main window is: on macOS all
  pages turn active one after the other as a popup opens, on Linux each
  page only once, and an extension is given another page than the one in
  front, or an internal page, which Dark Reader shows as `about:`.
- With the official CEF (macOS on Intel, Linux on arm64) Chromium calls
  every page's window a pop-up window, so an extension that looks for the
  tab in front among normal windows (Dark Reader) finds none and takes the
  page for `about:blank`.
- Without the DevTools pipe (on Linux and macOS, file descriptors 3 and 4
  taken at start) the buttons and menu entries open nothing.
- On a page WebView2 shows on Windows the buttons open nothing: it has no
  Chromium tab for the extension to act on (`protected-video.md`).

## Not built yet

- Badges, the count an extension shows on its icon.
- An extension bar below the top bar.

Code: `crates/engine/src/extensions.rs`, `devpipe.rs`, `stray.rs`,
`content.rs` (`attach_toolbar`) and `menubar.rs`;
`crates/app/src/app/extensions.rs` and `crates/app/src/state.rs`
(`follow_chromium_pins`); `crates/ui/web/src/PinnedExtensions.tsx` and
`SettingsPage.tsx`.
