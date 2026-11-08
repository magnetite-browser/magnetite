# ADR-0007: A private DevTools pipe for browser-wide commands

- Status: accepted (2026-11-06)

## Context

Opening an extension's popup from our own sidebar button has no CEF API. CEF
removed its extension API in M128 (ADR-0003), and Chromium opens extension
popups only from its own toolbar. Chromium does have a DevTools command for
it, `Extensions.triggerAction(id, tab)`, but only in a trusted, browser-wide
session. Checked against Chromium 156.0.8078.80:

- CEF's in-process DevTools channel (`ExecuteDevToolsMethod`) is attached to
  one page. Page sessions cannot attach to the browser target
  (`Target.attachToBrowserTarget` needs browser access), and the Extensions
  domain only allows `triggerAction` in a browser session.
- `--remote-debugging-port` gives a browser session, but on a TCP port that
  every local program can reach and drive. Not acceptable outside tests.
- `--remote-debugging-pipe` gives a browser session on file descriptors 3
  and 4 of the browser process. Its client counts as trusted.

## Decision

Make both pipes inside our browser process before CEF starts
(`crates/engine/src/devpipe.rs`), put Chromium's ends on fds 3 and 4, and
pass `--remote-debugging-pipe`. Our side of the pipes is only reachable
from inside the process.

- Every descriptor is close-on-exec, so programs we start (`open`, …) do not
  inherit them. Chromium starts its helpers with an explicit descriptor list.
- If fds 3 or 4 are taken at start, the pipe stays off and features that need
  it do nothing.
- The pipe carries only commands we send: no port, no listening socket, no
  file on disk.

## Consequences

- A compromised renderer gains nothing: it cannot reach browser-process
  descriptors. Code that already runs in our browser process could drive
  Chromium directly anyway.
- The Extensions domain is marked experimental. A Chromium update may change
  it; extension buttons then stop opening popups until we adapt, and nothing
  else breaks.
- `Extensions.triggerAction` needs Chromium's toolbar in the page's window:
  without one Chromium crashes. Every page window therefore has the toolbar
  in its own views, out of sight above the page (`content.rs`), which also
  gives popups and Chromium's bubbles their anchor at the page's top-left
  corner.
