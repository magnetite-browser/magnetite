# Upstream issues

Bugs in CEF or Chromium that affect Magnetite, what they cost us and how we
work around them. Once the repository is public, each gets a tracking issue
in our own tracker that links the upstream one; tick it off here when it
exists.

| Upstream | Effect in Magnetite | Workaround | Our issue |
|---|---|---|---|
| [CEF #4350](https://github.com/chromiumembedded/cef/issues/4350): extension action popups wait 2 s for new browser info | Every extension popup (Bitwarden, …) opens 2 s or more late; a second click during the wait cancels it. Still in CEF 156.0.26, whose fix for the related [#4348](https://github.com/chromiumembedded/cef/issues/4348) (`e4bc26e`) does not cover popups (checked 2026-11-09, also in cefclient) | None in our code; needs the fix in CEF | - [ ] TODO: open and link |

When CEF ships a fix, update `cef` in `Cargo.toml`, check the effect is gone,
and close our issue.
