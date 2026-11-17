# ADR-0014: Coding agents read local pages over a socket in the data folder

- Status: accepted (2026-11-17), built and checked in a running build
  (`docs/spec/developer.md`, "Agent bridge").

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
- A page is judged by its tab's address when a tool runs. A local page that
  is leaving for another site can be read once more in the moment before
  its new address is known.
- The tools evaluate script in the page to read it (`Runtime.evaluate`,
  without enabling the `Runtime` domain, as the capture tool does): a page
  cannot see that it is read, but one that replaced `document.querySelector`
  decides what is found.
- A socket's path has about 100 characters of room. The default data folders
  fit; a long `--data-dir` does not, and Settings then says why the bridge
  did not start.
- Not on Windows yet: there the setting reports that it is not available.
- Acting on a page (click, fill, navigate) is left out on purpose. If it is
  wanted later it needs its own decision: per-page consent and a visible
  session, not another tool in this list.
