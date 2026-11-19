# ADR-0015: Protected video plays in the system's own web engine

- Status: accepted (2026-11-20). Built for macOS; what was checked in a
  running build is listed under "Decision".

## Context

Chromium plays protected video with Google's Widevine module, which it
downloads itself. The module tells a service whether the browser build
around it is one Google has verified ("Verified Media Path"). Several
streaming services play only in a verified build and refuse every other one
with an error page (Netflix: M7701-1003). Magnetite's Chromium is CEF, built
by the project, and carries no such verification.

Google does not verify browsers itself. It refers them to partner labs,
whose certification is a commercial service with a code audit. The project
is in talks about it. Until they lead somewhere, Chromium itself cannot
play for these services.

Each desktop system ships a web engine of its own that plays protected
video without Widevine's verification: WebKit on macOS, with Apple's
FairPlay, which any app may show pages in; WebView2 on Windows, with
PlayReady. Linux has none.

## Options

1. **Leave it.** Document the services as unsupported. Costs nothing and
   leaves a browser people cannot watch their films in.
2. **Hand the page to another browser**: open the service's links in the
   system's default browser. Little code, but the page leaves Magnetite:
   no tab, no space, no split view.
3. **Certification** through a partner lab. The straight way, and the only
   one that makes Chromium itself play. Not there for now, with talks
   under way; possible later without undoing anything here.
4. **The system's engine in a tab.** The pages of the services that need it
   are shown by the system's engine, inside Magnetite's own page window.

## Decision

Option 4 until option 3 is there, for macOS first.

- **A system page is a page like any other** to the app and to the rest of
  the engine: the same id, events and calls, and the same frameless child
  window in the frame (ADR-0004). So it is shown, hidden, split, peeked,
  unloaded and closed by the code that does it for a Chromium page. What
  differs is what fills the window (`PageView` in `content.rs`): a
  Chromium browser view, or a view of the system's engine.
- **The engine decides by address**, from a list of sites the app hands it
  (`Engine::set_system_engine_sites`): when a page is made, when the app
  sends it to another address, and when a Chromium page is about to go to a
  listed site (`on_before_browse`). A page changes its engine in place: its
  window is replaced under the same id, and it loads the address in the
  other engine. The app does not take part.
- **A system page keeps its engine wherever its own links lead.** A sign-in
  that passes through another site (a service signing in at its parent
  company) stays in one cookie store. It leaves the system's engine when
  the user sends the tab elsewhere.
- **Which sites**: a short list that ships with the app
  (`PROTECTED_VIDEO_SITES`), which one switch turns off, and sites the user
  names in Settings or with a switch in the site panel. The list is this
  device's own and is not synced: whether a system has such an engine
  differs per device.
- **On macOS** the view is a `WKWebView` (`mac/webkit.rs`):
  - It lives in a plain window of AppKit's, without a frame, that lies
    over the page's own window and is attached to it as a child. The
    engine goes on moving, showing and hiding the page's own window, which
    CEF made; the attached window follows by itself and is kept the same
    size.
  - Not inside CEF's window, where two ways were tried and measured. A
    view inside the window's content view never gets the mouse: Chromium's
    content view answers every hit test with itself. A view beside the
    content view gets presses but loses the pointer's moves: of 60 moves
    handed to WebKit the page saw none, and so showed no hover and no
    cursor of its own. In a plain window, in the same process, it saw all
    60.
  - The attached window can take the keyboard and is never the main
    window, as the windows of Chromium pages: the frame stays main, and
    the sidebar keeps its hover.
  - WebKit's own user agent leaves Safari's version out. The view adds the
    version of the Safari that is installed, since the services look for
    it before they offer FairPlay.
  - Each profile has a data store of its own, named by an identifier kept
    in the data folder; a private profile's store is in memory only. Stores
    by identifier exist since macOS 14, so older systems keep every page in
    Chromium.
  - WebKit's check of addresses against a list of fraudulent sites is off,
    as Chromium's Safe Browsing is: both ask a service.
- **Only the official engine of the system is used**, through its public
  interface. Nothing of it is shipped, and no protection is worked around:
  the service sees the system's engine, which is what it is.

Checked in a running build (macOS 27.2, CEF 156):

- A tab for a listed site is made in WebKit and reports its title, address,
  loading state and icon. A Chromium tab sent to a listed site changes its
  engine in place, and back again when sent elsewhere; no window is left
  over.
- The attached window has the bounds of the page's own window after the
  frame is resized, also for a page that was hidden meanwhile; it is on
  screen only while its page is; it is gone when the tab closes. Two system
  pages share a split, with the outline on the one that has the keyboard.
- A page in a private space gets no store on disk.
- Signing in to Disney+ and playing a title there, with the pointer's hover
  and cursors, by hand.
- WebKit offers FairPlay for H.264 and HEVC in a process that is not Safari.

## Consequences

- Magnetite shows pages in two engines. A system page has none of what
  hooks into Chromium: extensions, ad blocking, the media row, DevTools,
  capture, find in page, the user's own CSS and JavaScript, Chromium's
  password manager and its permission prompts. The site panel says which
  engine shows a page, and Settings say what differs. What of this is
  rebuilt on the system engine's own interfaces is a matter of later work,
  feature by feature (`docs/spec/protected-video.md`, "Not built yet").
- Sign-ins are per engine: a site the user is signed in to in Chromium asks
  again in the system's engine.
- WebKit keeps its data outside Magnetite's data folder, under
  `~/Library/WebKit/<bundle id>`. Deleting a profile removes its store
  there; deleting the data folder by hand does not.
- The engine's version is the system's, not the project's: it is updated
  with macOS, and a page may behave differently from one Mac to the next.
- Windows can follow the same shape with WebView2 behind the same interface
  (`system.rs`). On Linux the services stay unsupported until Chromium
  itself may play them.
- Certification stays possible. With it the list of sites becomes empty and
  this code is no longer on any page's path.
