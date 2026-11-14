# Dev tools

## Driving pages without input injection

The way to test the running app (`CONTRIBUTING.md`). `MAGNETITE_DEVTOOLS_PORT=9333`
makes a development build of Magnetite expose the Chrome DevTools Protocol on
localhost (release builds ignore it). Pages, including the sidebar at
`magnetite://ui/`, can then be driven and screenshotted (`Runtime.evaluate`,
`Page.captureScreenshot`) without touching the mouse or keyboard.

Start test instances with a throw-away `--data-dir`, `--mock-keychain` and
`--no-import`: otherwise a fresh data directory starts with the welcome
page, which the test would have to click through first.

`MAGNETITE_CLIPBOARD_FILE=<path>` sends Magnetite's clipboard writes (copy link, …) to
that file instead of the macOS clipboard, so tests never overwrite the user's.

`tools/smoke.mjs` is a ready-made run of this kind: it opens tabs, a split
and a peek, moves and removes things through the sidebar's own commands and
waits for one minute tick, then reports what broke (its header says how to
start it). Run it after changes to how tabs, windows or the state behave.

## A Wayland session without a desktop (Linux)

`scripts/wayland-test.sh <stage folder>` starts a container build of
Magnetite in a headless Wayland session (sway with XWayland, `tools/wayland/`)
and runs `tools/smoke.mjs` there. With `--steps <file>` it runs your own
script in place of the smoke test, with a pointer (`at`, `click`), keys
(`wtype`) and screenshots (`grim`): input that reaches no other window than
the session's own. Its `--help` has the details.

## Swift utilities (macOS)

Small Swift utilities for measuring the app and, only where the DevTools
protocol cannot reach, scripting input.
Build once: `swiftc -O tools/drive.swift -o /usr/local/bin/magnetite-drive` (or any
directory on your PATH). The app under test must run with `--mock-keychain`
and a throw-away `--data-dir`.

- `drive <binary> "<script>" [app args]`: launches the app, runs a script of
  `wait`, `click x,y`, `rclick`, `dblclick`, `key cmd+t`, `type text`,
  `shot file.png`, `list` steps (window-relative DIP coordinates), then quits
  it. Example:
  `magnetite-drive target/bundle/Magnetite.app/Contents/MacOS/magnetite "wait 5; key cmd+t; type example.com; key enter; wait 3; shot /tmp/a.png" --mock-keychain --data-dir /tmp/magnetite-test`
- `launchmeasure <binary> <shot.png> <settle_s> [args]`: time to first window,
  process count and RSS, one screenshot. `LM_REGION=1` captures the screen
  region of the largest window (includes child windows), `LM_LIST=1` prints
  the window list.
- `pixel <png> <x> <y>`: colour probe, handy to assert "page painted".

Posting synthetic input requires Accessibility permission for the terminal
that runs the tool. **Only run input scripts when nobody is using the
machine**: the events go to whatever window is in front, including other
apps.

## Disk image layout (macOS)

`tools/dmg-layout.py` writes `packaging/macos/dmg.DS_Store`: the window size,
icon size and icon positions Finder uses for the release's disk image. The
file is kept in the repository, so run the script only to change the layout
(its header says how).
