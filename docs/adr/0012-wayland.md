# ADR-0012: Under Wayland, child windows are subsurfaces of the frame

- Status: accepted (2026-11-13). Steps 1 and 3 are done and step 2 in part:
  with a CEF that has the patch, `--ozone-platform=wayland` passes the smoke
  test. Such a CEF is not released yet, so XWayland is still the default.

## Context

ADR-0004 gives every loaded page a frameless child window over the frame's
content area. The command bar and other overlays, the float bar and docked
DevTools are child windows of the frame too. The app places and stacks them
itself.

Wayland has no such windows. An app cannot place a toplevel (`xdg_toplevel`)
or say where it is; the compositor decides. Chromium's Wayland backend
knows three kinds of window:

| Chromium widget type | Wayland role | Placed by |
| --- | --- | --- |
| window | `xdg_toplevel` | the compositor |
| menu, tooltip | `xdg_popup` | the app, relative to its parent; no keyboard of its own |
| popup, bubble | `wl_subsurface` of the parent (`WaylandBubble`) | the app, relative to its parent; moves with it; takes the keyboard when activated |

CEF makes a window with a parent a "window" and then sets it up with X11
calls (`WM_TRANSIENT_FOR`; `window_view.cc`, in 156.0.26 and on CEF's
`master`). Under Wayland the last of them
(`LinuxUiDelegate::SetTransientWindowForParent`) is not implemented and stops
the program. Magnetite therefore died on the first page in every Wayland
session, which is the default session of Fedora, Ubuntu, GNOME, KDE and
COSMIC. CI did not see it: its tests ran under X11 (Xvfb) only.

## Options

1. **Subsurfaces, with a patch to CEF.** Under Wayland, CEF makes a window
   with a parent a bubble, so Chromium makes it a `wl_subsurface` of the
   frame, and skips the X11 calls. The windows stay ours: our delegates, the
   Chrome toolbar that extension popups need, DevTools docking. Needs
   Magnetite's own CEF build for Linux
   ([magnetite-cef](https://github.com/magnetite-browser/magnetite-cef)
   builds for Apple silicon only so far).
2. **Subsurfaces without a patch**, through CEF's "native parent" browsers
   (`CefWindowInfo::parent_window`), which Chromium also makes subsurfaces
   under Wayland. CEF then owns the window and the browser view: no Chrome
   toolbar of ours (extension popups have nothing to anchor to), no hook to
   dock DevTools, and a second way of hosting pages for one platform.
3. **`xdg_popup`** (CEF's `is_menu`). No patch, but a popup gets no keyboard
   focus: nobody could type in a page.
4. **XWayland only.** Works today, with what X11 clients lack under Wayland:
   one scale for all screens, and on several compositors blurry text when
   that scale is fractional.

## Decision

Option 1, with option 4 as the fallback until it is there and wherever the
CEF in use lacks the patch.

The spike behind it: option 2's code path (the same `WaylandBubble`, no
patch needed) in a headless sway session. A page drew over the content area
as a subsurface, took clicks and typing, gave the keyboard back when the
sidebar was clicked and took it again, and its context menu opened at the
pointer.

### Steps

1. **No more crash (done).** Without an `--ozone-platform` option, Magnetite
   runs as an X11 client whenever an X server is there (`cef_app.rs`,
   `through_xwayland`). `scripts/wayland-test.sh` runs a build in a headless
   Wayland session, with pointer and keyboard.
2. **CEF for Linux (in part).** magnetite-cef builds x86_64 and aarch64 with
   the codecs (H.264, AAC and HEVC, which Linux lacks today) and a patch to
   `CefWindowView::CreateWidget`: under Wayland a window with a parent is a
   bubble, and the X11 calls run under X11 only. The patch
   (`wayland-child-windows.patch`) is written and an x86_64 build with it
   was tested; a release, aarch64 and pinning it in `scripts/cef.sh` are
   open. The crash fix goes to CEF upstream as well.
3. **The engine under Wayland (done)**, tested with `scripts/wayland-test.sh
   -- --ozone-platform=wayland`. One change was needed: CEF gives a Linux
   window that cannot be resized its first size as minimum and maximum, and
   Chromium's Wayland backend holds every later size to them, so a page
   kept its first size. Under Wayland, pages and docked DevTools therefore
   say they can be resized (`cef_app.rs`, `wayland`); a subsurface has no
   resize handles anyway. Placement relative to the frame, stacking and
   focus between the sidebar and pages needed nothing. Checked in headless
   sway: a page, a split, typing in a page and in the sidebar, the command
   bar, a peek, docked DevTools, the context menu and moving the frame.
   Open: the frame cannot be resized by its edges (under X11 neither), and
   no desktop compositor (GNOME, KDE, COSMIC) was tried.
4. **Wayland by default** where the CEF in use has the patch; XWayland
   stays the fallback for other CEF builds and behind `--ozone-platform=x11`.
   CI runs the smoke test in a Wayland session.

## Consequences

- Linux needs Magnetite's CEF build, as Apple silicon does. A build from
  source with the official CEF keeps working, through XWayland.
- The patch is ours to carry from CEF version to CEF version until upstream
  takes it.
- What Wayland does not offer stays out under Wayland: mini windows and
  picture-in-picture open where the compositor puts them and cannot stay on
  top by themselves, and the shortcut that opens a mini window from any app
  needs the desktop portal for global shortcuts.
- Until step 4, a session without XWayland cannot run Magnetite: it starts
  as a Wayland client and stops at the first page.
