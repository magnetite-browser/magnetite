# Protected video

Some streaming services play protected video only in a browser build that
Google's Widevine programme has verified. Magnetite's Chromium is not such a
build yet, so those services refuse it (Netflix shows error M7701-1003).
Until it is, their pages are shown by the system's own web engine, which
plays protected video by its own means: WebKit on macOS, with Apple's
FairPlay; WebView2 on Windows, Microsoft Edge's engine, with PlayReady and
Edge's Widevine.

Such a page is called a system page below. Why it is built this way:
[ADR-0015](../adr/0015-system-engine-pages.md).

## Where it works

- macOS 14 or newer, and Windows with WebView2 120 or newer, which comes
  with Windows 11 and is kept up to date by Windows. On macOS 13, on a
  Windows without WebView2 or with an older one, and on Linux every page is
  Chromium's, and the services that ask for a verified browser do not play.
- Nothing is shipped for it: the engine is the one that comes with the
  system, in the version the system has.

## Which pages

- The services Magnetite knows to need it, with their subdomains:
  Netflix (`netflix.com`), Disney+ (`disneyplus.com`), Max (`max.com`,
  `hbomax.com`), Prime Video (`primevideo.com`) and Hulu (`hulu.com`).
  The list ships with the app and is never fetched.
- Sites the user names: in Settings → Protected video ("Other sites", one
  host per line, each with its subdomains), or with "Open in WebKit"
  ("Open in WebView2" on Windows) in the site panel, which names the site of the page in front and loads the
  page again. At most 100 sites; the panel says so when the list is full.
- A site whose protected video fails in Chromium: the site is added to
  "Other sites" and the page loads again in the system's engine, with a
  note in the sidebar. It counts as failed when the page itself asks
  Widevine for a licence request and none of its keys is usable ten
  seconds later, which is how a service that refuses an unverified
  browser looks from inside, whatever error it shows. A service that plays
  in Chromium has its key long before and stays there. A player in a frame
  from another site does not move the page around it. "Move sites whose
  protected video fails to WebKit" (to WebView2) in Settings turns this
  off. On by default.
- A site taken out again with the site panel's switch stays in Chromium
  whatever it plays: it is listed under "Kept in Chromium" in Settings,
  where sites can be added and removed (at most 100; the oldest makes
  room).
- In a private space both count until Magnetite is quit and are written
  nowhere: a site that moves by itself, and the site panel's switch. The
  switch there cannot take out a site that Settings names.
- "Open streaming services in WebKit" (in WebView2) in Settings turns the known services
  off as a whole. On by default. A known service shows "Always, for its
  video" in the site panel in place of a switch.
- The choice is this device's own and is not synced.

The address decides the engine:

- A tab opened at such an address is a system page from the start: typed
  in the command bar, restored, loaded again after it was unloaded, opened
  from a link in another tab.
- A Chromium page that is about to go to such an address (a link, a
  redirect, the address typed into the tab) becomes a system page in
  place: the same tab, where it was on screen. Only for a plain load
  (`GET`); a form sent there stays in Chromium.
- A system page stays one wherever its own links and redirects lead, so a
  sign-in that passes through another site stays in one place. It becomes
  a Chromium page again when the tab is sent to an address outside the
  list: from the command bar, by a bookmark, by a favorite's home address.
- Pages that are open keep their engine when the list changes, until they
  load another address.
- A mini window is always Chromium's.

## What a system page is

A tab like any other: in the sidebar with its title and icon, in its
space, pinned or a favorite, in a split, a peek or a web panel, unloaded
and archived as the others. Back, forward, reload and stop work, as do
zoom (Settings' default zoom does not apply: it starts at 100 percent) and
the page's own full screen.

- **Links.** A link that asks for a new tab or window opens one, in the
  engine its address belongs in, if the address is an `http://` or
  `https://` one. `Cmd`+click opens a tab behind,
  `Shift+Cmd`+click one in front, `Shift`+click a peek, as in a Chromium
  page. A link out of a tab locked to its site (a pinned tab, a favorite)
  opens elsewhere.
- **Extensions.** On macOS an extension opens on a system page as on any
  other: its popup at the page's corner, with the page's address as the
  tab in front, so a password manager lists the logins of the site and a
  password can be copied from it. The extension does not reach the page
  itself: it fills nothing in and changes nothing there (see below). A tab
  it opens from its popup opens as a tab.
- **Pointer.** On macOS hover effects and the page's own cursors show
  while the page has the keyboard, which it gets when it is shown and with
  a click into it. After a click into the sidebar they rest until the next
  click into the page; that click already counts as one. On Windows they
  show as in any other page, also before the first click.
- **Shortcuts.** The reserved shortcuts (`shortcuts.md`) run before the
  page sees the key, the peek's keys in a peek. On macOS the others run
  from the menu bar after the page left the key unused. On Windows the
  others do not run in a system page: a key the page leaves unused goes to
  WebView2's own keys (its find bar, its zoom, printing).
- **Profiles.** Each profile has a data store of its own in the system's
  engine; a private space's is in memory only and gone with the space
  (`InPrivate` on Windows).
  They are apart from Chromium's: a site the user is signed in to in
  Chromium asks again here, and the other way round.
- **The site panel** names the engine (WebKit, WebView2) beside the connection, and shows
  zoom, the engine switch and "Clear cookies and site storage", which
  deletes the site's data in the system's engine. On Windows that is the
  cookies of the site and its subdomains and the storage at the site's own
  addresses and at those of its pages that are open: WebView2 lists no data
  by site, so storage of another subdomain stays until all is deleted.
- **Settings → Protected video → "WebKit's website data"** ("WebView2's") deletes the
  cookies, site storage and cache of every profile there. Chromium's own
  dialog (Data & privacy) does not reach them.
- **A page that cannot be loaded** shows a note with the reason and the
  address; reload tries the address again.

What a system page does not have, because it hooks into Chromium:

- What an extension does in a page: filling a form, content scripts,
  reading the page. On macOS the extension gets a Chromium tab of the
  page's own to act on, which holds an empty document at the page's
  address and is never seen; on Windows it gets none.
- Ad blocking, Global Privacy Control, the user's own CSS and JavaScript.
- The media row in the sidebar, the floating video, muting a tab.
- DevTools and the developer features, capture, find in page, printing.
- Chromium's password manager and autofill.
- Magnetite's permission prompts and the permission rows of the site
  panel: the system's engine asks for camera and microphone itself.
- Downloads: a link that would download a file does nothing.
- Sharing the tab's picture with another page.

## Data

On macOS the engine keeps a profile's data in its own place,
`~/Library/WebKit/org.magnetitebrowser.magnetite/WebsiteDataStore/<id>`,
not in Magnetite's data folder. On Windows it is in the data folder:
`<data-dir>/system-engine`, one WebView2 folder with a WebView2 profile per
Magnetite profile (`EBWebView/WV2Profile_<name>`). Which store belongs to
which profile is in `<data-dir>/system-engine-stores.json`. A profile that is deleted loses
its store at the next start, and stays in that file until the engine has
removed it. Where the file cannot be written, a profile's store is in
memory for that run. Icons of system pages are fetched without cookies,
over `https://` only, and kept with the others in `<data-dir>/favicons`;
a private page keeps none.

The engine's check of addresses against a list of fraudulent sites is off,
as Chromium's Safe Browsing is (`docs/design/security.md`); on Windows
WebView2's developer tools are off too.

## Not built yet

- Anything for Linux, which has no such engine.
- Everything in the list above of what a system page does not have.
- A window a page opens for a sign-in gets no way to answer its opener:
  it opens as a tab or a peek of its own.
- JavaScript dialogs (`alert`, `confirm`) and choosing a file to upload.
- A service on a path of a larger site (Prime Video under `amazon.com`):
  only whole hosts can be named.
- A service that refuses before it asks for a licence stays in Chromium
  unless it is on the list or named, as does one whose licence only
  arrives after more than ten seconds, which moves although it would have
  played.
- A site that moved from a player in one of its frames, and asking before
  a page with unsaved input loads again.

Code: `crates/engine/src/system.rs` (which engine, changing it, a system
page's events), `protected.rs` and `protected_hook.js` (a Chromium page
whose protected video fails), `shadow.rs` (the Chromium tab extensions act on), `mac/webkit.rs` (the WebKit view, its data stores),
`win/webview2.rs` (the WebView2 view, its profiles), `win/icon_fetch.rs`,
`mac/mod.rs` (`set_focus_ring`), `content.rs` (`PageView`, `make_page`),
`client.rs` (`on_before_browse`), `favicon.rs`, `sitedata.rs`,
`crates/app/src/state/settings.rs` (`SystemEngine`,
`PROTECTED_VIDEO_SITES`), `app/engines.rs` (the site panel's switch, a
site that moves by itself),
`crates/ui/web/src/settings/ProtectedVideoSection.tsx`, `SitePanel.tsx`
(`SystemEngine`).
