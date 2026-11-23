# Developer features

What Magnetite does for people who build websites: tools for the pages
their own computer serves, commands that show any page under other
conditions, a split that shows a page as a phone beside itself, pictures and
videos of a page, and a capture that describes an element in words. DevTools
themselves (docking, device mode) are in `site-controls.md`.

## Local pages

A page is local when it is served over `http` or `https` from this computer:
`localhost`, a name under it (`app.localhost`), or a loopback address
(`127.0.0.1`, `::1`). Any other site becomes one with **Developer mode** in
its site controls (the icon left of the address, under "This site"): the
switch adds the site's host to Settings → Developer tools → "Also local", or takes
it out, and the page changes at once. That list also takes hosts typed in:
`staging.example.org`, `*.test` for every name under one, or an address
such as `192.168.1.4` for a server in the local network. For a page this
computer serves the row says "Always, on this computer".

A local page gets, as long as "Developer bar for local pages" is on (the
default):

- **The developer bar** across the top of the page area (below).
- **A caution border.** Its tab row in the sidebar has a border of amber
  stripes, so a tab in developer mode is told from any other at a glance.
- **An error count.** Errors in the page's console since it began to load
  are counted: uncaught exceptions, `console.error`, failed requests. The
  count shows on the console button and, while another tab is in
  front, as a badge on the page's tab row ("99+" beyond 99). Only the
  number is kept, not what the errors say; other pages' errors are not
  counted.
- **Waiting for its server.** If nothing listens at the page's address
  (connection refused), the bar says "Waiting for localhost:5173" and
  Magnetite tries the address twice a second, for at most ten minutes. Once
  something listens, the page loads. Only loopback addresses are tried.
- **Its project's name.** The row names the folder the serving program runs
  in (`shop` for a server started in `~/code/shop`), found with `lsof`; the
  address when that says nothing (the home folder) or cannot be found.
- **No ad blocking.** Nothing is blocked on pages this computer serves,
  whatever the lists say.

## The developer bar

A bar 36 points high across the top of the page area, beside the sidebar
and below the top bar if that is on. It shows while the page in front has
developer tools and is gone otherwise, so the pages of other tabs keep
their full height. Its left end names the page, the tools are at its right;
the two panels (emulation, time zone) open as windows below it. The sidebar
and the top bar hold none of this. With a split on screen the bar is one
for all its pages and acts on the page in front.

| Part | Does |
|---|---|
| Name | The project, or the address; "Waiting for …" while the server is down |
| Pick an element | DevTools on Elements with the element picker started |
| Console | DevTools on the Console; shows the error count |
| Network | DevTools on the Network panel |
| Show the page as… | The emulation panel (below); coloured while anything is on |
| Time zone | The time zone the page sees (below); coloured while one is set, with its place beside it |
| Responsive split | The page as a phone beside itself (below); coloured while on |
| Capture set | One picture of the page in light and dark at three sizes (below) |
| Record | A video of the page (below); red with the running time while recording |
| Clear data (an eraser) | On a page this computer serves: clears the origin's cookies, storage, caches and service workers, then loads the page past the cache. On any other page the button is "Reload past the cache" and clears nothing |

The bar also shows for a page that is not local while it is shown as
something else or has a phone beside it, and for every page while a
recording runs, so each can be turned off from where it shows. Under a peek
it is hidden.

## Commands for any page

These work on the page in front whether it is local or not, from the command
bar, the menu bar (View) and by key:

| Shortcut (macOS default) | Command | If the page uses the key too |
|---|---|---|
| `Cmd+Option+J` | Console | the page keeps it |
| `Cmd+Option+C` | Pick an element | the page keeps it |
| `Cmd+Option+E` | Network | the page keeps it |
| `Cmd+Option+R` | Reset page | the page keeps it |

The keys can be changed (Settings → Shortcuts → Developer). The command bar
also has "Open storage and service workers" (the Application panel).

**Reset** clears site data only for a page this computer serves. Any other
page, a host named under "Also local" included, is only loaded again past
the cache: its cookies and storage stay, so a key press cannot sign the user
out of a site.

**Emulation** shows one page as something else until it is turned off, the
tab is closed or unloaded:

| Setting | Effect |
|---|---|
| Colours: System, Light, Dark | `prefers-color-scheme` |
| Network: Online, Slow, Offline | Slow: about 1.4 Mbit/s down, 0.7 Mbit/s up, 560 ms latency |
| Reduced motion | `prefers-reduced-motion: reduce` |
| Print styles | the print style sheet |
| Slow processor | four times slower |
| No cache | every request goes to the server |
| Time zone | the zone dates and times are in: this computer's, or one of twelve from Los Angeles to Auckland, each listed with its time now |

Each has a command ("Show page in dark mode", "Take page offline",
"Show page in Tokyo time", …); "Show page as it is" ends all of them.
Emulation needs no open DevTools.

**Responsive split** (the bar, or "Responsive split" in the command bar)
shows the page in front a second time to its right, 390 points wide, as a
phone: touch instead of a mouse, and a mobile layout (the viewport tag
counts, scrollbars lie over the page). Both stay at one address: when
either goes to another page, by a link, a form, the address row or the
page's own script, the other loads that address. Details:

- The phone is a second view of the tab, not a tab: the sidebar keeps its
  one row, and the phone is in no archive, history or sync. It shows
  whenever its tab is on screen, also as a page of a split view, and
  closes with the tab or when the tab is unloaded.
- The divider between the two drags like a split's: the phone takes any
  width from 200 points up, and a double-click gives it 390 again.
- The phone runs in the tab's profile, with the same logins, and keeps this
  computer's browser identity; only its width, touch and layout are a
  phone's. Device mode in DevTools has whole devices.
- The other view loads the address anew. What a page keeps in memory alone
  (an open dialog, a form half filled in) is not carried over.
- The bar, the address row and the commands act on the tab's own page;
  keys pressed in the phone act on the phone.
- Calling it again ends it: the phone closes.
- Not offered in the private space, for a web panel, or when four pages
  are on screen already.
- Kept in memory only: after a restart the tab has no phone.

**Capture set** (the bar, or "Capture set" in the command bar) takes six
pictures of the page in front, from the top of the page: light and dark,
each at 390 × 844, 768 × 1024 and 1280 × 800 CSS pixels, one device pixel
per CSS pixel. They come as one picture, light in the upper row and dark in
the lower, on grey, and go where every capture goes: the toast with Copy,
Save and Share (`site-controls.md`). The page is shown at each size and
colour for about half a second, then as it was shown before.

**Record** (the bar, or "Record page as a video" in the command bar)
records the page in front until it is called again, the page closes, or
five minutes have passed. The video is written to the Downloads folder as
"Magnetite recording 2026-11-17 at 09.17.12.webm" while it is recorded; the
sidebar then offers "Copy file" (macOS) and "Show in Finder".

- The video shows the page alone, at up to 1920 pixels a side: no pointer,
  no sound, nothing of Magnetite's own windows or of other pages.
- It is WebM with VP9. MP4 is written instead where the browser build can
  encode H.264; the released builds cannot.
- A new picture is taken when the page changes, so a page that stands still
  costs nothing. A page that is not on screen is not drawn and adds nothing.
- The file has no length written into it, so some players cannot jump
  within it.
- One recording runs at a time.

**Open beside as <profile>** (command bar, one entry per profile other than
the page's own) opens the page in front a second time, in a split to its right,
with that profile's logins: two users of one site side by side. Not offered
in the private space.

**Local servers.** With the developer bar on, the command bar lists the
programs listening on this computer's ports that run from a folder below
the home folder (outside `~/Library` and app bundles), as
"shop · localhost:5173". Typing a project's name, a port, or "localhost"
finds them. The list is read with `lsof` each time the bar opens. They
are the last group of results.

## Copy an element as context

In the capture tool (`site-controls.md`), `C` turns "Context" on. Picking an
element then also puts its description on the clipboard as text:

- a selector that finds it again, the page's address and title;
- its size and position, and the viewport's size;
- its text (300 characters) and markup (4000 characters);
- the computed styles that shape it;
- the path of the element's picture.

The picture is `capture-context.png` in Magnetite's data folder, readable by
the user only, written over by the next one and removed when Magnetite quits.
The text is meant for an issue or a coding agent, which reads the picture
from that path. A dragged region has no element and copies nothing.

## What is stored

| Data | Where | Until |
|---|---|---|
| Developer bar on or off, "Also local" hosts (`settings.developer`) | `state.json`, this device only (not synced) | changed |
| Error count of a local page | memory | the page loads again, or its tab unloads or closes |
| Emulation of a page, its time zone included | memory | turned off, or its tab unloads or closes |
| The phone of a responsive split and its width | memory | ended, or its tab unloads or closes, or Magnetite quits |
| A recording | a video file in the Downloads folder | the user deletes it |
| A capture set | memory, as the last capture | the next capture, or dismissed |
| Picture of the last element copied as context | `capture-context.png` in the data folder | the next one, or Magnetite quits |
