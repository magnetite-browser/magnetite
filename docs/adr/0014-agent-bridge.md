# ADR-0014: Coding agents read local pages over a socket in the data folder

- Status: withdrawn (2026-11-17). The bridge was built, checked in a running
  build and released in 0.0.7, then removed: see "Withdrawn" at the end.
  The rest is kept as written, for the reasoning about reach.

## Context

People who build websites increasingly work with a coding agent that edits
their code but cannot see the result: the page in the browser, its console,
what an element ended up looking like. The usual answers are a second,
automated browser that the agent drives (another profile, not the page the
person is looking at), or the browser's remote debugging port.

Magnetite's rule so far is that nothing outside its process can reach a
page: DevTools commands travel inside the process (`devtools.rs`) or over
pipes made before CEF starts (ADR-0007), and no port is opened. A bridge for
agents is the first way in from outside, so its reach has to be decided
before its features.

## Options

1. **The remote debugging port.** Everything an agent could want, and far
   more: every page of every profile, script in any page, cookies. Any
   program on the computer can connect, and a web page can try to. Chromium
   itself warns against it.
2. **A local HTTP server with a token.** A port again; web pages can reach
   `localhost` ports, so the token is the only wall, and it has to be kept
   somewhere agents and nothing else can read.
3. **A Unix socket in the data folder, with a fixed set of reading tools.**
   The data folder is the user's alone (0700), the socket too (0600): what
   can open it is what can already read the user's files. Web pages cannot
   open Unix sockets. The tools are ours, so the reach is what we write.
4. **A browser extension.** Would work in any browser, but agents would talk
   to it through native messaging or a port, and it could not be limited
   below what extensions may do.

## Decision

Option 3.

- **Transport.** `agent.sock` in the data folder, made only while Settings →
  Developer → "Agent bridge" is on, removed when it is turned off and when
  Magnetite quits. Agents speak the Model Context Protocol, one JSON-RPC
  message per line. They start `magnetite mcp`, which passes its standard
  input and output to the socket and does nothing else, so an agent needs
  no code of ours beyond the browser's own program.
- **Reach.** Only pages served by this computer (`localhost`, names under
  it, loopback addresses), loaded in a tab, outside the private space. The
  hosts a user adds as "also local" for the developer bar are left out: a
  staging server holds other people's data.
- **Reading only.** List the pages, a screenshot, the console's errors, a
  page's or an element's markup and text, an element's styles. No tool
  clicks, types, navigates, reloads or runs script an agent supplies:
  selectors and limits go into fixed functions as JSON values.
- **Off by default, and visible.** The setting says what it allows; while an
  agent is connected the developer bar shows a mark for local pages.

## Consequences

- "No port is opened" still holds. What is new: with the bridge on, a
  program running as the user can read the local pages open in Magnetite.
  Such a program can already read the code those pages are made of, but not
  what a signed-in session shows; a local page showing real data shows it to
  the agent. The setting's text says so.
- Page content is untrusted input for the agent. A page can carry text
  written to steer an agent that reads it; the tools' descriptions call
  page content data, and nothing an agent does through the bridge can change
  a page, but what the agent does elsewhere with what it read is outside
  our reach.
- A page is chosen by its tab's address, and judged again where it is
  read: the script that reads runs in a script world of its own
  (`Page.createIsolatedWorld`), bound to one document, and first checks that
  this computer serves that document. A page that left for another site in
  between answers nothing. A screenshot is handed out only if the tab is
  still at the address it was chosen by once the picture is there.
- That world shares the document with the page but none of its script
  (`Runtime.evaluate` with its context, without enabling the `Runtime`
  domain): a page cannot see that it is read, nor change the functions the
  check and the read use.
- A socket's path has about 100 characters of room. The default data folders
  fit; a long `--data-dir` does not, and Settings then says why the bridge
  did not start.
- Not on Windows yet: there the setting reports that it is not available.
- Acting on a page (click, fill, navigate) is left out on purpose. If it is
  wanted later it needs its own decision: per-page consent and a visible
  session, not another tool in this list.

## Withdrawn

The bridge was removed the day it was released. What it could do was too
little to be used, and what would have made it useful is what this decision
ruled out.

- **It could not look at a website.** Only pages served by this computer
  were in reach, so a question about any other site could not be asked
  through it.
- **It could not check a change.** An agent that edits code has to reload
  the page, click through it and look again. Every tool only read, so the
  agent needed a browser it can drive anyway.
- **Agents bring that browser.** The tools people run coding agents in come
  with a browser of their own, or with one that speaks the DevTools
  protocol. It reads and acts, on any page.
- **What was left is covered by hand.** The one thing only the bridge could
  do is show an agent the very tab the person is looking at, with its
  session and state. "Copy as context" in the capture overlay
  (`docs/spec/developer.md`) does that for one element when the person
  chooses to, without a way in from outside.

Making the bridge useful would have meant reaching every site and acting on
pages: any program running as the user could then read and drive signed-in
pages. That is options 1 and 2 above under another name.

Against that stood its cost: the only way into a page from outside the
process, about 800 lines to keep correct, a setting, and the texts of
console errors kept in memory only for it.

So "nothing outside its process can reach a page" holds again without an
exception. With the bridge went `magnetite mcp`, the setting, the agent
mark in the developer bar and the kept error texts; the error count stays.
A `developer.agent_bridge` left in a `state.json` from 0.0.7 is ignored.

If a way in for agents is wanted again, the last point under
"Consequences" still stands: it needs its own decision, with consent per
page and a visible session.
