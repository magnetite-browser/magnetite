# Protected video

Some streaming services play protected video only in a browser build that
Google's Widevine programme has verified. Magnetite's Chromium is not such a
build yet, so those services refuse it (Netflix shows error M7701-1003).
Until it is, their pages are shown by the system's own web engine, which
plays protected video by its own means: WebKit on macOS, with Apple's
FairPlay.

Such a page is called a system page below. Why it is built this way:
[ADR-0015](../adr/0015-system-engine-pages.md).

## Where it works

- macOS 14 or newer. On macOS 13, on Windows and on Linux every page is
  Chromium's, and the services that ask for a verified browser do not play.
- Nothing is shipped for it: the engine is the one that comes with the
  system, in the version the system has.

## Which pages

- The services Magnetite knows to need it, with their subdomains:
  Netflix (`netflix.com`), Disney+ (`disneyplus.com`), Max (`max.com`,
  `hbomax.com`), Prime Video (`primevideo.com`) and Hulu (`hulu.com`).
  The list ships with the app and is never fetched.
- Sites the user names: in Settings → Protected video ("Other sites", one
  host per line, each with its subdomains), or with "Open in WebKit" in
  the site panel, which names the site of the page in front and loads the
  page again.
- "Open streaming services in WebKit" in Settings turns the known services
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
  engine its address belongs in. `Cmd`+click opens a tab behind,
  `Shift+Cmd`+click one in front, `Shift`+click a peek, as in a Chromium
  page. A link out of a tab locked to its site (a pinned tab, a favorite)
  opens elsewhere.
- **Pointer.** Hover effects and the page's own cursors show while the
  page has the keyboard, which it gets when it is shown and with a click
  into it. After a click into the sidebar they rest until the next click
  into the page; that click already counts as one.
- **Shortcuts.** The reserved shortcuts (`shortcuts.md`) run before the
  page sees the key, the peek's keys in a peek. The others run from the
  menu bar after the page left the key unused.
- **Profiles.** Each profile has a data store of its own in the system's
  engine; a private space's is in memory only and gone with the space.
  They are apart from Chromium's: a site the user is signed in to in
  Chromium asks again here, and the other way round.
- **The site panel** says "WebKit" beside the connection, and shows
  zoom, the engine switch and "Clear cookies and site storage", which
  deletes the site's data in the system's engine.
- **Settings → Protected video → "WebKit's website data"** deletes the
  cookies, site storage and cache of every profile there. Chromium's own
  dialog (Data & privacy) does not reach them.
- **A page that cannot be loaded** shows a note with the reason and the
  address; reload tries the address again.

What a system page does not have, because it hooks into Chromium:

- Chrome extensions, also password managers.
- Ad blocking, Global Privacy Control, the user's own CSS and JavaScript.
- The media row in the sidebar, the floating video, muting a tab.
- DevTools and the developer features, capture, find in page, printing.
- Chromium's password manager and autofill.
- Magnetite's permission prompts and the permission rows of the site
  panel: the system's engine asks for camera and microphone itself.
- Downloads: a link that would download a file does nothing.
- Sharing the tab's picture with another page.

## Data

The engine keeps a profile's data in its own place,
`~/Library/WebKit/org.magnetitebrowser.magnetite/WebsiteDataStore/<id>`,
not in Magnetite's data folder. Which store belongs to which profile is in
`<data-dir>/system-engine-stores.json`. A profile that is deleted loses
its store. Icons of system pages are fetched without cookies and kept with
the others in `<data-dir>/favicons`; a private page keeps none.

The engine's check of addresses against a list of fraudulent sites is off,
as Chromium's Safe Browsing is (`docs/design/security.md`).

## Not built yet

- Windows (WebView2) and anything for Linux, which has no such engine.
- Everything in the list above of what a system page does not have.
- A window a page opens for a sign-in gets no way to answer its opener:
  it opens as a tab or a peek of its own.
- JavaScript dialogs (`alert`, `confirm`) and choosing a file to upload.
- A service on a path of a larger site (Prime Video under `amazon.com`):
  only whole hosts can be named.
- Noticing that a page failed to play in Chromium and offering the switch.

Code: `crates/engine/src/system.rs` (which engine, changing it, a system
page's events), `mac/webkit.rs` (the WebKit view, its data stores),
`mac/mod.rs` (`set_focus_ring`), `content.rs` (`PageView`, `make_page`),
`client.rs` (`on_before_browse`), `favicon.rs`, `sitedata.rs`,
`crates/app/src/state/settings.rs` (`SystemEngine`,
`PROTECTED_VIDEO_SITES`), `app/engines.rs` (the site panel's switch),
`crates/ui/web/src/settings/ProtectedVideoSection.tsx`, `SitePanel.tsx`
(`SystemEngine`).
