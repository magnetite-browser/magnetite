# First start

The first time Magnetite starts with an empty data folder it opens a welcome
page and asks what to bring along. Until the user chooses, it reads nothing
from another browser, contacts no site and changes nothing on the system:
the sidebar holds one space, "Personal", and no tab.

## The welcome page

`magnetite://ui/welcome` opens as a tab, once. The steps are listed by name
on the left, the current one marked; a step that was seen can be gone back
to from there or with "Back", except while the app is still joining or
starting a chain, setting the ad blocking or, after "Start browsing",
importing. A narrow window shows the list as a row of dots on top.

The first step asks which of two starts this is, and the rest follows from
the answer:

| Step | What it offers | To go on |
|---|---|---|
| Welcome | what Magnetite is, a drawing of its window, and the two ways in: "I am new here" and "I use Magnetite already" | one of the two, or "Skip the questions" to leave at once |
| Your devices (only after "I use Magnetite already") | joining a chain with its 24 words: the join half of Settings → Sync (`sync.md`) | "Continue" once the device syncs, or "Go on without" |
| Your tabs (not on a device that joined) | every browser found on this computer; a chosen one is read and its spaces shown as the sidebar will list them, with names to change (`import.md`) | "Continue", or "Start fresh" with none chosen |
| Search and ads | the search engines of Settings → Search, the current one (DuckDuckGo at first) marked; and ad blocking as one of three answers (below) | "Continue" |
| Look | where back, forward and the address sit (in the sidebar at first, or in a bar on top), the text size and the height of the sidebar's rows: three of the settings of Settings → Appearance (`settings.md`) | "Continue" |
| Tabs and links | three settings, each as a line with its value and "Change" for the answers: when a page off screen is unloaded (4 hours at first, or never), when an unused Today tab is archived (12 hours at first), where links from other apps open (a mini window at first, a new tab or a peek window) | "Continue" |
| Your devices (only after "I am new here") | starting a sync chain: the start half of Settings → Sync, with the chain's 24 words once it exists (`sync.md`) | "Not now", or "Continue" once the device syncs |
| Ready | what was chosen, line by line with "Change" back to its step; three keys for the first day; "Make Magnetite the default browser" (off) | "Start browsing" |

- A choice of search engine, look, tab times or the place for links takes
  effect at once, as in Settings (the page itself follows the text size); the one about ad blocking with "Continue".
- **The drawing** on the first step is made of plain shapes and four words:
  a sidebar with favorites, pinned tabs, today's tabs and spaces, next to a
  page. It names what the later steps talk about. A window too narrow for
  it leaves it out.
- **A device that joins** a chain takes the chain's sidebar in place of its
  own, and with it the tab times, the place for links and the shortcuts
  (`sync.md`). So it is not asked about them: "Your tabs" and "Tabs and
  links" are left out. How Magnetite looks is each device's own, so "Look"
  stays. A browser chosen before the device joined is not imported, also if "I am new
  here" is chosen after the join. "Go on
  without" continues as a new start, import and all.
- **Your tabs.** The list of browsers is made when the step shows, by
  looking for their data folders. Choosing a browser reads it, to show what
  it has: each space it would add with its number of tabs and bookmarks,
  and the number of favorites. Nothing is added yet. Choosing a browser
  again reads it again, and what was chosen for its spaces starts over. The spaces of all
  chosen browsers stand in one list, "Your sidebar, as it will start", with
  a line on what a space is. Each space has a field with its name, to
  change; a browser without spaces of its own (one
  Chromium profile, Firefox) comes in as one space named after the browser,
  and the page says so. Two spaces that would come in under one name stay
  two: the later one gets a number ("Personal 2"), and the page says which
  name it will have. A click on a space's icon shows the icons to choose
  from, below its line. A switch per space leaves it out: it stays behind
  with the profile and the favorites that only it used, and a browser with
  every space switched off is not imported. A browser that cannot be read
  says why, at once, and is not chosen; one with nothing to import neither.
  Below the list the step says that passwords and extensions are not read,
  and where they come from instead (`passwords-and-autofill.md`,
  `extensions.md`).
- **Ads and trackers** has three answers: "Block with the recommended
  lists", "Choose the lists myself" and "Do not block", the last one marked
  at first. The recommended lists are EasyList, EasyPrivacy and uBlock
  Origin's lists, and EasyList Germany too where the system's language is
  German; the answer shows each of them with what it blocks and the host
  it comes from. "Choose the lists myself" shows all lists that way, each
  with a switch (`ad-blocking.md`). An answer here is only marked: the
  blocker is turned on or off, and the lists are downloaded, with
  "Continue". A line below the answers says so while something is still to
  be downloaded. With no answer given on this page, "Continue" leaves what
  is set. Until the app is done with it, the answers cannot be changed.
  If the app refuses, the step stays and says why.
- **Ready.** The summary names the browsers that will be imported with
  their spaces and tabs, the search engine, whether ads are blocked and
  with how many lists, the tab times, the layout and text size, and whether
  the device syncs. A space that was switched off does not count.
- **Your devices** uses the words of a first start ("Start a sync chain",
  "Your sync code"), with the choice of server that Settings → Sync has.
- **A chain started** on "Your devices" takes this device's sidebar, with
  what is imported after it. The step comes last of the questions, so that
  a new start is not asked about other devices before anything else.
- **The three keys** (new tab, change the address, show or hide the
  sidebar) show in one line with the keys they have; one without keys is
  left out.
- "Start browsing" reads the chosen browsers again, so what comes in is as
  new as it can be, applies what was chosen for their spaces and closes the
  page.
  A name goes to the space that is still at its place under its old name;
  a name of spaces only, or of control characters, changes nothing, and one
  longer than 80 characters is cut. The first import takes the place of the
  starter space while nobody has used it, so an imported "Personal" keeps
  its name; further ones add their spaces (`import.md`). Magnetite then
  shows the first imported space.
- "Make Magnetite the default browser" stands on the last step, above
  "Start browsing". It shows only on macOS, and only while another app
  opens web links. Turning it on changes nothing yet: after
  "Start browsing" Magnetite asks macOS, which asks the user to confirm
  (Settings → General has the same, `Default browser` below).
- If a chosen browser cannot be read at the end, nothing is imported: the
  page stays, names the browser and the reason, and leaves it out of the
  next try.
- With nothing chosen, and after "Skip the questions", the page closes and
  the command bar opens for the first address. The page is not archived.
- Everything on the page is in Settings as well, so nothing is lost by
  skipping.

## Opening it again

Settings → General → "Welcome page" and the command bar's "Show the welcome
page again" open the page in a new tab. It works as at the first start,
with what is set now marked: nothing changes until something is chosen.
An import from there adds its spaces to the sidebar (`import.md`). On a
device that syncs already the first step does not ask which start this is,
there is no "Your devices" step, and "Skip the questions" is not there:
closing the tab does the same.

## When it shows

Only when no `state.json` exists in the data folder at start. A second start
does not show it by itself again, also if the page was left half way. A state file
that could not be read is no first start (`spaces.md`, "Data").

`--no-import` starts without the page (tests). `--import-arc` takes Arc's
sidebar in place of the saved one on that start and is independent of the
page (`import.md`).

## Default browser

Settings → General shows whether Magnetite opens links from other apps
(the system's handler for `https`; any copy of Magnetite counts, since
macOS keeps the default by the app's identifier). While it does not, "Make default" asks
macOS to make it the handler for `http` and `https`; macOS asks the user to
confirm, and the card shows the outcome. Magnetite never asks on its own.
Outside macOS, or when Magnetite does not run as an app bundle, the card is
not shown.

## Not built yet

- A colour for an imported space (Settings → Appearance has it afterwards).
- The default browser question on Linux and Windows.

Code: `crates/ui/web/src/Welcome.tsx`, `crates/app/src/app/commands.rs`
(`Ready`, `ImportPreview`, `FinishWelcome`, `MakeDefaultBrowser`),
`crates/engine/src/default_browser.rs`, `crates/engine/src/mac/default_browser.rs`, `crates/app/src/state/import.rs`
(`add_first_import`, `preview`, `choose_spaces`), `crates/app/src/state/persist.rs`
(`load_or_default`, `first_start`).
