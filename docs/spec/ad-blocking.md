# Ad blocking

Magnetite can stop requests of web pages to ad and tracking servers before
they load. It is off until the user turns it on and chooses filter lists;
the lists are downloaded from their publishers and checked on this computer.
The filter engine is Brave's adblock-rust (MPL-2.0).

## Turning it on

Settings → Ad blocking, and the "Block ads and trackers?" step of the
welcome page (`first-start.md`), show the same controls:

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

Only lists in one file are offered: lists that include others
(`!#include`, such as uBlock Origin's) are not.

## Lists

- Choosing a list downloads it at once over https and keeps it in the data
  folder (`filter-lists/`). Every hour Magnetite looks for chosen lists
  older than four days and downloads them again. A failed download keeps
  the copy from before; a download that is a web page (a Wi-Fi sign-in
  page, served as `text/html` or starting with `<`) or larger than 16 MB
  is refused.
- A list no longer chosen is deleted.
- The lists are compiled on a thread of their own (about 40 ms for all
  four): at start while the engine starts, and before the first page is
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

## Per site

The site panel (`site-controls.md`) has **Block ads** while the blocker is
on. Turning it off lets the site (its registrable domain, as for clearing
site data) load everything, in every tab and profile, and loads the page
again. Settings → Ad blocking lists these sites, each with "Block again".

## Stored data

| What | Where | Removed when |
|---|---|---|
| On/off, chosen lists, sites without blocking | `state.json` | changed |
| Downloaded lists | `filter-lists/` in the data folder | the list is no longer chosen |
| Why a download failed | memory | the next download, or Magnetite quits |

## Not built yet

Each has an issue labelled `known`; a 👍 there counts as a request.

- Hiding what is left of an ad on the page (cosmetic filtering, #52).
- Scriptlets: YouTube ads and anti-adblock walls (#53).
- Harmless stand-ins for blocked scripts (`$redirect`, #54). Such requests
  are blocked outright.
- WebSocket connections (#55): they do not pass the hook CEF offers.
- `$csp` rules (#56): CEF cannot add headers to a response.
- Trackers under the site's own domain (CNAME cloaking, #57).
- Service workers of the default profile's context, which has no request
  context handler of Magnetite's.
- A count of what was blocked, and lists of one's own.

Code: `crates/engine/src/blocker.rs`, `crates/app/src/adblock.rs`,
`crates/app/src/app/adblocking.rs`, `crates/ui/web/src/settings/AdblockSection.tsx`,
`crates/ui/web/src/SitePanel.tsx`.
