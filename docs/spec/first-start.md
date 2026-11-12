# First start

The first time Magnetite starts with an empty data folder it opens a welcome
page and asks what to bring along. Until the user chooses, it reads nothing
from another browser, contacts no site and changes nothing on the system:
the sidebar holds one space, "Personal", and no tab.

## The welcome page

`magnetite://ui/welcome` opens as a tab, once. It has five steps; "Back"
returns to the one before, and the bar at the bottom left shows the place.

| Step | What it offers | To go on |
|---|---|---|
| Welcome | what Magnetite is, in two sentences | "Get started", or "Skip" to leave at once |
| Sync | the controls of Settings → Sync: start a chain, or join one with its 24 words (`sync.md`) | "Not now", or "Continue" once the device syncs |
| Import | every browser found on this computer, to choose from (`import.md`) | "Continue", or "Not now" with none chosen |
| Search | the search engines of Settings → Search, the current one (DuckDuckGo at first) marked; a choice takes effect at once | "Continue" |
| Done | which browsers will be imported, five keys for the first day, and "Make Magnetite the default browser" (off) | "Start browsing" |

- Sync comes before the import because a device that joins a chain takes
  the chain's sidebar in place of its own: what was imported before would
  be gone.
- The list of browsers is made when the import step shows, by looking for
  their data folders. Choosing a browser there reads nothing.
- "Start browsing" reads the chosen browsers and closes the page. The first
  import takes the place of the starter space while nobody has used it, so
  an imported "Personal" keeps its name; further ones add their spaces
  (`import.md`). Magnetite then shows the first imported space.
- "Make Magnetite the default browser" shows only on macOS, and only while
  another app opens web links. Turning it on changes nothing yet: after
  "Start browsing" Magnetite asks macOS, which asks the user to confirm
  (Settings → General has the same, `Default browser` below).
- If a chosen browser cannot be read, nothing is imported: the page stays,
  names the browser and the reason, and leaves it out of the next try.
- With nothing chosen, and after "Skip", the page closes and the command
  bar opens for the first address. The page is not archived.
- Everything on the page is in Settings as well, so nothing is lost by
  skipping.

## When it shows

Only when no `state.json` exists in the data folder at start. A second start
does not show it again, also if the page was left half way. A state file
that could not be read is no first start (`spaces.md`, "Data").

`--no-import` starts without the page (tests). `--import-arc` takes Arc's
sidebar in place of the saved one on that start and is independent of the
page (`import.md`).

## Default browser

Settings → General shows whether Magnetite opens links from other apps
(the system's handler for `https`). While it does not, "Make default" asks
macOS to make it the handler for `http` and `https`; macOS asks the user to
confirm, and the card shows the outcome. Magnetite never asks on its own.
Outside macOS, or when Magnetite does not run as an app bundle, the card is
not shown.

## Not built yet

- A way to open the page again later.
- The default browser question on Linux and Windows.

Code: `crates/ui/web/src/Welcome.tsx`, `crates/app/src/app/commands.rs`
(`Ready`, `FinishWelcome`, `MakeDefaultBrowser`),
`crates/engine/src/default_browser.rs`, `crates/engine/src/mac/default_browser.rs`, `crates/app/src/state/import.rs`
(`add_first_import`), `crates/app/src/state/persist.rs`
(`load_or_default`, `first_start`).
