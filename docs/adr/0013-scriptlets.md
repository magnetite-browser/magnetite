# ADR-0013: Scriptlets reach a page through its DevTools channel

- Status: accepted (2026-11-15). The way into the page is built and checked
  in a running build; the scriptlets themselves and lists that use them are
  not shipped yet (`docs/spec/ad-blocking.md`, "Not built yet").

## Context

Filter lists answer sites that fight ad blockers with scriptlets: rules such
as `example.com##+js(set-constant, adsBlocked, false)` name a small script
and its arguments. A scriptlet changes what the page's own scripts see, so
it must run in the page's script world before the first of them. One that
comes later does nothing: the page has read the value or kept the function
it was meant to replace.

Which scriptlets a page gets depends on its address, and the filter engine
that knows lives in the browser process, with the user's settings and
allowed sites. CEF has no call for "run this before the page's scripts",
and no way to answer a renderer's question before it goes on: its messages
between the processes do not wait for an answer.

## Options

1. **The DevTools protocol.** `Page.addScriptToEvaluateOnNewDocument` makes
   Chromium run a script in every new document of a page, before the
   page's scripts. The script belongs to one address, so it is exchanged
   when the page goes elsewhere: the request for the new document waits
   until Chromium has the script for it. All of it happens in the browser
   process, over the channel pages already have (`devtools.rs`).
2. **The renderer process.** `on_context_created` runs before the page's
   scripts, in every frame, and already installs the media hook. But the
   renderer would have to know the scriptlets for the address at that
   moment. The rules cannot be read there (renderers are sandboxed, without
   the file system), nor asked for in time. What is left is handing every
   renderer the rules of all sites when its browser is made: a second copy
   of the engine and of the settings in each of them, old after every
   change of lists, of the switch or of an allowed site.
3. **Rewriting the document**: a response filter that puts a `<script>` at
   the start of the page. A page's Content Security Policy forbids inline
   scripts on the very sites that matter, and CEF cannot change response
   headers to allow one.
4. **A patch to CEF** that asks the browser process for a script while a
   navigation commits. The straight way, at the price of one more patch to
   carry through every CEF update, on every platform, and a change to the
   bindings.

## Decision

Option 1.

- For the request of a page's document, the blocker returns a handler whose
  `on_before_resource_load` works out the script for the address and, if
  Chromium has another one for the page, holds the request
  (`RV_CONTINUE_ASYNC`). The UI thread removes the old script, adds the new
  one and lets the request go when Chromium has answered
  (`scriptlets.rs`).
- CEF asks for a handler again after every redirect, so the address the
  document ends up at decides.
- A request waits 300 ms at most. Chromium answers in a millisecond or two;
  only a page busy in a script of its own takes longer, and then the new
  document loads with the script the page had.
- A page with no scriptlets, before and after, costs one look-up and no
  wait.

Checked in a running build (CEF 156.0.30, macOS): the script ran before an
inline script in the document's `<head>` on the first load of a new tab,
on reload, on going to another site (another renderer process), across a
redirect to another site, on going back, and in a same-site frame; it was
gone on a page without rules, in the same renderer process and in another.

## Consequences

- The scripts run only while the `Page` domain of the channel is on, so it
  is turned on for a page with its first script. A page cannot tell: of
  the protocol's domains only `Runtime` can be noticed from inside.
- Frames of another site are separate targets in Chromium and are not
  reached: they run no scriptlets. A frame of the page's own site runs the
  page's.
- Removing a script answers "Script not found" although the script is gone
  afterwards; the answer is not taken as a failure.
- The rule "exchange before the document's request goes out" depends on
  two behaviours of CEF and Chromium that an update may change: the
  handler being asked again after a redirect, and scripts registered
  before a navigation commits applying to its document. Both are part of
  the check above and are to be run again with every new CEF version.
- Option 4 stays open, should frames of other sites turn out to matter.
