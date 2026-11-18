# Ad blocking

Magnetite can stop requests of web pages to ad and tracking servers before
they load, and run the small scripts (scriptlets) that filter lists use
against "turn off your ad blocker" walls. It is off until the user turns it
on and chooses filter lists; the lists are downloaded from their publishers
and checked on this computer. The filter engine is Brave's adblock-rust
(MPL-2.0); the scriptlets are uBlock Origin's (GPL-3.0), a file of their
own next to the app.

## Turning it on

Settings → Ad blocking shows these controls. The welcome page asks first
whether to block with a recommended set of lists, with lists of one's own
choice or not at all, and sets the same two things with that step's
"Continue", not when the step is left another way (`first-start.md`):

- **Block ads and trackers** (off at first). Off, nothing is blocked and
  nothing is downloaded.
- While on, the **filter lists**, none chosen at first. Each shows its name,
  what it blocks, and the host it comes from; once chosen, when it was last
  downloaded, "Downloading…", or why the download failed. With no list
  chosen, a hint says that nothing is blocked.

| List | Blocks | From |
|---|---|---|
| EasyList | ads on most international websites | easylist.to |
| EasyPrivacy | trackers and analytics | easylist.to |
| EasyList Germany | ads on German-language websites, with EasyList | easylist.to |
| Peter Lowe's list | servers of ad and tracking companies | pgl.yoyo.org |
| uBlock Origin's lists | ads, trackers and "turn off your ad blocker" walls, with fixes for sites the other lists break | ublockorigin.github.io |

A list is one file or several kept as one: uBlock Origin's lists are its
five files "filters", "badware", "privacy", "quick fixes" and "unbreak",
each as its publisher has put it together. A list that includes others
(`!#include`) is not offered.

## Lists

- Choosing a list downloads it at once over https and keeps it in the data
  folder (`filter-lists/`). Every hour Magnetite looks for chosen lists
  older than four days and downloads them again. A failed download keeps
  the copy from before; a download that is a web page (a Wi-Fi sign-in
  page, served as `text/html` or starting with `<`) or larger than 16 MB
  is refused. A list of several files is kept only when all of them came.
- A list no longer chosen is deleted.
- Parts of a list marked for other browsers (`!#if env_firefox` …
  `!#endif`) are left out; what is marked `env_chromium` counts.
- The lists are compiled on a thread of their own (a fraction of a second
  for all five): at start while the engine starts, and before the first page is
  made, so restored pages are blocked from their first request; after a
  change, from the lists already here, and again once missing or old ones
  are downloaded. A download only starts while no newer change came.
- The download is the only request: its publisher sees the IP address and
  "Magnetite" as the client, as any website would. Which pages are opened,
  and what was blocked, is never sent anywhere.
- The settings stay on this device and are not synced: each device
  downloads its own lists.

## What is blocked

- Every request a web page makes, also from its frames and its service
  worker: scripts, images, styles, fonts, frames, media, fetches and the
  rest. Not the page the user opened itself.
- Not the requests of Magnetite's own pages, of extensions or of DevTools.
- The rules that apply are the lists' network rules: blocking (`||host^`),
  exceptions (`@@`), `$third-party`, `$domain=` and the other request
  options, and `$removeparam`, which takes tracking parameters off what a
  page fetches (the request goes to the address without them).
- The page the request comes from is the page in the tab, also for its
  frames; for a service worker, the worker's own site.
- A blocked frame shows Chromium's error page in its place.

## Scriptlets

A rule such as `example.com##+js(set-constant, adsBlocked, false)` names a
scriptlet and what to call it with. The scriptlets a list has for a page
run in the page before its own scripts, in every document the tab loads at
that address: so the page finds, say, a value already set, or a function
that refuses to open the wall.

- The scriptlets' code is the file `scriptlets/scriptlets.json` in every
  package, with its licence and a note of its origin next to it: uBlock
  Origin's, of the release named in `scripts/scriptlets.sh`, unchanged. It
  comes with Magnetite's updates, not with the lists'. A rule that names a
  scriptlet the file does not have is skipped.
- Some scriptlets can do a page anything (uBlock Origin marks them as
  needing trust). Only uBlock Origin's lists may use those; in another list
  such a rule is skipped.
- The address the page ends up at counts, after redirects. A frame of the
  page's own site runs the page's scriptlets; frames of other sites run
  none.
- Before the request for a page's document goes out, Magnetite hands
  Chromium the scriptlets for that address and waits for its answer, 300 ms
  at most (ADR-0013). Pages without scriptlets do not wait.
- A run from the checkout has no scriptlets file and runs none.

## Per site

The site panel (`site-controls.md`) has **Block ads** while the blocker is
on. Turning it off lets the site (its registrable domain, as for clearing
site data) load everything and run no scriptlets, in every tab and profile,
and loads the page again. Settings → Ad blocking lists these sites, each with "Block again".

## Stored data

| What | Where | Removed when |
|---|---|---|
| On/off, chosen lists, sites without blocking | `state.json` | changed |
| Downloaded lists | `filter-lists/` in the data folder | the list is no longer chosen |
| Why a download failed | memory | the next download, or Magnetite quits |

## Not built yet

Each has an issue labelled `known`; a 👍 there counts as a request.

- Hiding what is left of an ad on the page (cosmetic filtering, #52).
- Scriptlets in frames of other sites, and rules that pass a scriptlet its
  arguments as one `{…}` object (#53).
- Harmless stand-ins for blocked scripts (`$redirect`, #54). Such requests
  are blocked outright.
- WebSocket connections (#55): they do not pass the hook CEF offers.
- `$csp` rules (#56): CEF cannot add headers to a response.
- Trackers under the site's own domain (CNAME cloaking, #57).
- Service workers of the default profile's context, which has no request
  context handler of Magnetite's.
- A count of what was blocked, and lists of one's own.

Code: `crates/engine/src/blocker.rs`, `crates/engine/src/scriptlets.rs`,
`scripts/scriptlets.sh`, `tools/scriptlets.mjs`, `crates/app/src/adblock.rs`,
`crates/app/src/app/adblocking.rs`, `crates/ui/web/src/settings/AdblockSection.tsx`,
`crates/ui/web/src/SitePanel.tsx`.
