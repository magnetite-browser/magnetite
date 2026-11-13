# Site controls

The address row at the top of the sidebar shows which page is in front. From
it the user copies the page's link, opens the site panel (connection,
permissions, zoom, sound, site data), opens Chrome DevTools and starts the
capture tool for screenshots. Everything acts on the page in front: the
active tab, or the split pane that has the keyboard.

## Address row

| Control | Shows | Click |
|---|---|---|
| Site chip | a sliders icon, or the page's state | opens the site panel |
| Address | the host without `www.` | opens the command bar, URL selected |
| Copy button | a link icon; a tick after a copy | copies the link |

With no tab open the row only says "Search or enter address". While a peek is
open, the peek's own row takes its place.

The chip shows the first state that applies: camera or microphone in use (red
dot), certificate problem or unencrypted connection (warning shield), zoom
other than 100 % (dot), tab muted (crossed speaker). The first two also
outline the row. The chip's tooltip lists every state.

- "Copy link" (click, `Cmd+Shift+C`) copies the URL without tracking
  parameters. A right-click also offers "Copy original link" (the exact URL)
  and "Copy as Markdown" (`[title](url)`, cleaned URL); the command bar has
  all three. Pressed in a page, also a peek, the key copies that page's link.
- Tracking parameters are a fixed list in `crates/core/src/links.rs`
  (`fbclid`, `gclid`, `msclkid`, … and every name starting with `utm_`,
  `pk_` or `mtm_`). It ships with the app and is never fetched. Order,
  encoding, repeated parameters and the fragment stay as they were.
  Settings → General → "Remove trackers from copied links" (on by default)
  switches the removal off.
- A status line under the row confirms for 3 seconds, longer while the
  pointer rests on it: "Copied", "Copied · 2 trackers removed" with a "Copy
  original" button, or "Could not copy".

## Site panel

The panel opens under the address row: click the chip, press `Cmd+Shift+O`
or choose Tabs → Site Controls… in the menu bar (a hidden sidebar is shown
first). Esc, a click outside it, or
the sidebar losing focus (a click into the page, another app) closes it;
focus returns to the chip.

- **Header:** favicon, host and the connection: "Encrypted connection", "Not
  encrypted" (plain http), "Certificate problem" (expired, wrong host,
  untrusted), "Local file", "Magnetite page", "Loading…" until the first
  load has finished, or "Unavailable" for other pages without a network
  origin. A page in a profile other than the default adds "Profile: name".
- **Tools:** Capture, Share (the macOS share menu for the page's URL) and
  Developer tools. Magnetite's own pages offer Share only, and nothing below.
- **Zoom:** − and + move through Chromium's zoom steps, 25 % to 500 %; a
  click on the value resets it to 100 %.
- **Block ads:** shown while the ad blocker is on (`ad-blocking.md`). Off
  lets the whole site load everything and loads the page again.
- **Mute tab:** mutes this tab's audio until unmuted or the tab is unloaded.

**Permissions.** Rows exist for camera, microphone, location, notifications,
pop-ups and redirects, and clipboard. A row shows when the setting for the
page's origin differs from the profile's default, or when the permission was
requested since the tab came to this site; a request from an embedded frame
gets its own row, marked "via host". With no rows the panel says "This site
has not asked for any permissions." Each row offers Allow, Ask and Block
(pop-ups: Allow and Block). "Ask" returns to Chromium's default. A choice
applies at once, and Chromium saves it as a site setting in the page's
profile. The request itself is a card at the top right of the page area.

**Clear cookies and site storage.** A site is the registrable domain from
the Public Suffix List, private suffixes included: `shop.example.org` belongs
to `example.org`, `alice.github.io` is a site of its own, and IP addresses
and `localhost` count as they are.

1. A confirmation names the domain, the profile (if not the default) and the
   hosts whose storage is cleared, and warns: "You will be signed out. Storage
   of parts of the site you did not open this session may remain."
2. Clear deletes the cookies of the domain and its subdomains in the page's
   profile, then every storage type (`Storage.clearDataForOrigin`) of the
   page's origin and of each origin of the domain that a frame loaded in this
   profile since Magnetite started. Other profiles are never touched.
   Only the confirmed domain is cleared: if the page went to another site
   while the confirmation was open, nothing is removed and the panel says so.
3. If the cookies were removed, the open tabs of the domain in that profile
   reload. The panel reports both parts ("Cookies: 12 removed · Storage: 3
   of 3 origins"); after a failure it offers "Retry".

## Developer tools

Each page has its own Chrome DevTools. `Cmd+Option+I`, View → Developer
Tools, the site panel and the command bar ("Toggle developer tools") open or
close them. "Inspect" in a page's context menu opens them at that element.

| Settings → Developer → "Developer tools open" | Layout |
|---|---|
| Right of the page (default) | 42 % of the page's width at first; DevTools at least 240 px wide, the page 320 px |
| Below the page | 40 % of the page's height at first; DevTools at least 120 px high, the page 160 px |
| In a separate window | a normal window, 960 × 680 at first |

Docked DevTools take their room from the page they inspect and hide and show
with it. Dragging the gap between page and DevTools resizes both (a line in
the gap shows on hover); a double-click gives DevTools their first size
again. The size counts for every page's docked DevTools, per side, and is
kept on this device after a restart. The DevTools main menu (⋮) has "Dock side" with the same three
choices and "Close DevTools"; a choice there changes the setting. Docked
DevTools also have the close button (×) at the end of their toolbar, as in
Chrome.

**Device mode.** The button after the element picker in the DevTools
toolbar, or `Cmd+Shift+M` in DevTools, shows the page as a phone, tablet or
computer. A bar above the page, in the page area, has the choices:

- **Dimensions:** Responsive (a size of one's own, 390 × 844 at first),
  phones (iPhone SE, iPhone 16, iPhone 16 Pro Max, Pixel 9, Galaxy S24),
  tablets (iPad Mini, iPad Pro 13) and computers (Laptop, Desktop).
- **Width × height**, in CSS pixels; typing a size makes it Responsive.
  The arrow beside them turns the device to landscape and back.
- **Zoom:** Fit (as large as the area allows, at most 100 %), 50 % to 150 %.
- **Network:** no throttling, Fast 4G, Slow 4G, 3G or offline (Chrome's
  presets: latency, download and upload speed).
- **Save-Data:** the browser's own, on or off.
- **×** ends device mode.

The page gets the device's size and pixel density, touch input (phones
and tablets) and the device's browser identity (an iPhone's or iPad's
Safari, an Android Chrome; computers keep Magnetite's). It sits below the
bar, centred, scaled to the zoom. In Responsive, handles on its right,
bottom and corner resize it (the right one on both sides, so it stays
centred); the bar shows the new size. Device mode ends with the button, the
× or the DevTools closing, also when they move between docked and their own
window; the next one starts with the device used last. Chromium's own
device toolbar shows only for DevTools docked in a Chrome tab window, which
Magnetite's are not.

`Cmd+W` in DevTools that have a window of their own, or in a window an
extension opened itself, closes that window and no tab.

## Capture

`Cmd+Shift+2`, File → Take Screenshot…, the site panel and the command bar
("Capture page…") start the tool; in a peek or a split pane the key captures
that page. Magnetite takes a screenshot of the visible area and a layout
snapshot, then covers the page with a window that shows the frozen picture.
It reads the page over the DevTools protocol inside the browser process: no
port is opened, and DevTools need not be open.

| In the overlay | Captures |
|---|---|
| Click an element, or Return while it is highlighted | that element |
| Drag | the region |
| V, F | the visible area, the full page |
| P | adds or removes 16 px of padding around a picked element |
| Esc, or a click outside the overlay | nothing; the tool closes |

- Elements come from the page's main document, not from embedded frames. If
  the page scrolled while it was frozen, the overlay says "Element picking
  unavailable"; the other modes still work.
- Element, region and visible captures are cut from the frozen picture, in
  device pixels; an element reaching beyond the visible area is cut off.
- A full-page capture reads the live page in tiles 4096 CSS px high and
  joins them on a worker thread. Above 100 megapixels the page is cut and
  the result says "Page cut at N px (size limit)". The pixels are counted
  on the captured tiles themselves, so a page cannot talk its way to a
  larger picture.

The result appears at the bottom of the sidebar and stays until it is
dismissed or the next capture replaces it: size, thumbnail, and Copy, Save…,
Share… and Retake. Copy puts a PNG on the clipboard, Save… opens the macOS
save dialog (suggested name `Magnetite capture YYYY-MM-DD at HH.MM.SS.png`)
and Share… the macOS share menu. While the result has focus, Return copies
and Esc dismisses. The command bar has "Copy last capture". Nothing is copied
or written without one of these actions.

## Shortcuts

| Shortcut (macOS default) | Action | If the page uses the key too |
|---|---|---|
| `Cmd+L` | Command bar with the current address | Magnetite gets it |
| `Cmd+Shift+C` | Copy link | the page keeps it |
| `Cmd+Shift+O` | Site panel | the page keeps it |
| `Cmd+Shift+2` | Capture | the page keeps it |
| `Cmd+Option+I` | Open or close DevTools | Chromium's own command |
| `Cmd+P` | Print the page (Chromium's print preview); also File → Print… and the command "Print page" | the page keeps it |
| `Cmd++`, `Cmd+-`, `Cmd+0` | Zoom in, out, 100 % | Chromium's own command |

## What is stored

| Data | Where | Until |
|---|---|---|
| Permission choices | Chromium's site settings, per profile | changed |
| Requested permissions | memory | the tab leaves the site or closes |
| Origins seen | memory | Magnetite quits |
| Last capture (PNG) | memory | replaced, dismissed, or Magnetite quits |
| Tracker removal, DevTools dock and size | `state.json` | changed |

## Not built yet

- Editing the address in place (the address opens the command bar instead).
- Developer mode per site: status, load times and console counters in a strip.
- Device mode without DevTools, and devices of one's own in the list.
- Dividers beside docked DevTools, and the device mode's resize handles,
  on other platforms than macOS.
- Environment labels for hosts (such as "PROD"), set up in Settings.
- Site panel: certificate details and a link to all site settings.
- Site panel: a hint when macOS itself blocks camera, microphone or location.
- Capture: a whole element that is larger than the visible area.
- Capture: a way to cancel a full-page capture while it runs.

Code: `crates/ui/web/src` (`AddressRow.tsx`, `SitePanel.tsx`,
`Capture*.tsx`), `crates/core/src/links.rs`, `crates/app/src` (`app/site.rs`,
`app/screenshots.rs`, `capture.rs`), `crates/engine/src` (`site.rs`, `sitedata.rs`, `capture.rs`,
`devdock.rs`, `device.rs`, `devtools_menu.js`, `devtools.rs`, `keys.rs`),
`crates/ui/web/src/DeviceBar.tsx`.
