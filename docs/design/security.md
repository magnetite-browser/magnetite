# Security rules

Magnetite embeds Chromium and keeps its protections (sandbox on by default,
site isolation, Chromium's permission decisions). These are the rules for
Magnetite's own code on top of it. Keep them when changing the code they
name.

## Our UI is a privileged origin

- `magnetite://ui/` is the only origin that may call the app
  (`magnetiteQuery`), and only as a top-level page (`router.rs`, renderer
  side in `cef_app.rs`). The app is told the address a page was loaded
  from, not the one it shows: a page can change its path without loading
  anything, and commands kept to one page (Settings, the notification
  panel) go by that address.
- The scheme is display-isolated (`scheme.rs`): web pages cannot frame, link
  to or load `magnetite://` URLs, so they can neither click-jack Settings nor
  probe cached favicons or extension icons. Every response carries a CSP
  (own scripts only, `frame-ancestors 'none'`) and `nosniff`.
- The sidebar, the floating sidebar and overlays navigate only to our UI
  (`client.rs` `on_before_browse`), and state and calls are executed only in
  frames that show our UI (`Engine::eval_in_ui`). A dropped link cannot turn
  the sidebar into a page that receives the user's tabs.
- UI pages get no bookkeeping they do not need (`for_ui` in `app.rs`: sync
  versions, Chromium pins seen).
- The quick search (`Cmd+Option+N`) is the command bar page in a window of
  its own, with the same rules as every overlay: our UI only, the same
  commands as the command bar. What it opens goes through `open_mini` as
  typed input (`url_policy::Source::User`), checked like any. A mini
  window's page is a web page like a tab's (`Kind::Content`), never one of
  our privileged pages unless the user opened `magnetite://ui/` there.
- The system-wide key is a Carbon hot key (`mac/hotkey.rs`): macOS reports
  that one combination, not other typing, and Magnetite asks for no
  accessibility or input-monitoring permission.

## Addresses by where they come from

`crates/app/src/url_policy.rs` decides what opens in a tab:

| Source | Opens |
|---|---|
| Typed, or Magnetite's UI | web, `about:blank`, `blob:`, `magnetite://ui/`, `chrome://`, `chrome-extension://`, `file://`, `view-source:` |
| Web pages (popups, new-tab links) | web, `about:blank`, `blob:` |
| Windows Chromium opens itself (extensions) | web, `about:blank`, `chrome://`, `chrome-extension://` |
| Other apps (default browser) | web, `about:blank`, `file://` |
| Sync, imports, smart-folder results | web, `about:blank` |

`javascript:` and `data:` never open. Only typed input is turned into a
search; search and site search addresses are `https://` with `%s` only
(`Settings::sanitised`). Tabs from sync and the import that fail the policy
are dropped (`state::drop_unsafe_tabs`).

Magnetite asks the system to become the default browser only on the user's
click (Settings → General, or the welcome page's switch once "Start
browsing" is pressed), and macOS confirms with the user
(`Engine::make_default_browser`).

A new tab or peek opened by a page, or by an extension, runs in the opener's
profile (`OpenUrlRequested.context`), so a page in a private or separate
profile never gets the space's cookies. Links from `DevTools` get the
inspected page's profile. Windows Chromium opens itself are matched to their
profile by identity (`Engine::context_id_of`, private ones included); one
from a context Magnetite did not make is not opened.

## Private profiles leave nothing behind

- Their Today tabs are not saved to `state.json`, not synced and not
  archived when closed (`AppState::lasting_sidebar`, `archive_tab`). Their
  pinned tabs and favorites are saved with their home address only, never
  where they went.
- Whether a profile is private is this device's: a synced sidebar cannot
  change it for a profile this device has. A page that stays open through a
  pull keeps its address, title and profile; if the other device moved its
  tab to another profile, or made it private or not, the page closes
  (`adopt_sidebar`).
- They leave no history: their pages are not recorded (`history.rs`,
  `record_visit`), in a tab or a peek.
- Their favicons are not cached on disk.
- The sidebar (default context) never loads a remote favicon of a page in
  another profile: it would send the default profile's cookies to that site.

## Files and logs

- The data folder is the owner's only (0700); `state.json`, `history.json`,
  `sync.key` and `smart-accounts.json` are 0600 from the moment they exist
  and are written whole (`files.rs`).
- The history (`history.json`) holds address, title and time of visits to
  `http(s)` pages only, is never synced, is capped (10,000 visits) and
  pruned to the period set in Settings ("Don't keep history" records
  nothing and deletes the file). Removing or clearing visits writes the
  file at once.
- `MAGNETITE_LOG` prints command and event names only (`EngineEvent::name`):
  never tokens, sync words, invites, addresses, titles or file names.
- `MAGNETITE_DEVTOOLS_PORT` works in development builds only. A data folder
  that ever ran with `--mock-keychain` keeps that mode (Chromium resets its
  protected settings when the key changes) and Settings says so.

## Pages and extensions are untrusted input

- Media reports from pages are capped (4 KiB, 200-character texts) and built
  so page prototypes cannot add fields. A page owns the picture-in-picture
  controls only if we asked it into picture-in-picture or it is on screen
  (`pip::may_own`); a floating window another page's video still holds
  changes hands only when we asked the new page.
- Extension files are read only below the extension's real folder (symbolic
  links included) and only up to 1 MiB (`extensions.rs`).
- Extension popups open through a private `DevTools` pipe (ADR-0007).
- Site boundaries (domain lock, clearing site data) use the Public Suffix
  List, so tenants of shared hosts such as `github.io` stay apart.
- Every profile id has its own data folder (`profile_dir_name`).

## Network

- Smart folders: `https://` only, plain `http://` only to this computer
  (parsed host: `localhost`, `127.0.0.1`, `::1`); no credentials in addresses.
- Sync: `https://` only (plain `http://` to this computer), no redirects,
  10 s to connect and 60 s per request. A stored version never goes down: a
  write counts only if the server confirms exactly the version written, and
  an older copy from the server is refused ("not going back"), so a
  malicious server cannot roll a device back. Answers are capped at 32 MiB
  after decompression. Requests sign the server's host. Leaving removes the
  local key at once; a deletion the server has not confirmed is retried.
  Pulled data cannot delete this device's profiles, load pages by itself,
  run smart folders with this device's accounts before they are turned on
  here, or change the search engine or the site searches; it is sanitised
  before it is kept.
  A space kept on one device ("Only on this device") is never sent, nor
  is a profile that only such spaces use, with its favorites and the tabs
  that run in it (`sidebar_for_sync`), and pulled data cannot change or
  remove any of them there (`keep_unsynced`). Which spaces those are is
  the device's own record, not part of what syncs.
  Server text is never shown. Only the Settings page and the welcome page
  of the first start, which shows the same controls, may read the code or
  change the chain. Details in `docs/spec/sync.md` and ADR-0005.
- Sync server: lives in the magnetite-sync repository, with its own
  security notes (README "Limits", `docs/design.md`,
  `docs/public-instance.md`).
- The engine itself. Chromium features that contact Google on their own
  and that Magnetite has no use for are switched off (`NO_CALLS_HOME` in
  `cef_app.rs`): the lookup of form fields at the autofill server, the
  "AI Mode" probe of the address field, the network time query, the
  standing connection to Chromium's own search engine and the download
  of the translation's ranking model. The check of passwords against
  known breaches is off unless turned on in Settings
  (`docs/spec/passwords-and-autofill.md`). Checked with the engine's
  network log (`--log-net-log`) of a fresh start and one sign-in.
- Two parts of Chromium that rely on Google's services are off for every
  profile (`prepare_context`), because Magnetite's builds carry no Google
  API key and Google serves neither without one:
  - Page translation (`translate.enabled`). It would send the text of a
    page to Google.
  - Safe Browsing (`safebrowsing.enabled`). Magnetite does not check
    sites or downloads against Google's lists of phishing and malware;
    Chromium's other protections (sandbox, site isolation, certificate
    checks) are not affected.
- What the engine still contacts on its own:
  - Google's component server (`update.googleapis.com`,
    `edgedl.me.gvt1.com`): the Widevine module for protected video,
    certificate revocation lists and Chromium's other lists.
  - Chromium's own settings page (`chrome://settings`) can fetch more
    when it shows all its sections: Google's list of translation
    languages and the icons of Chromium's built-in search engines. The
    lists Magnetite opens from its Settings did not.

## Downloads

Chromium saves files as usual. Magnetite tells about each download's start,
end or failure in the notification panel at the top right of the page area
(`notifications.rs`) and lists them behind the sidebar's Downloads button,
except the package of an extension being installed from the Chrome Web
Store, which is part of the install and is deleted once it is in;
Chromium's own bubble, which lives in the hidden toolbar, is turned off
(`prepare_context`).

- File names are cut in the middle, never at the end: the extension shows.
- Files that run code or hand off to other programs when opened (`RISKY` in
  `downloads.rs`: `.command`, `.pkg`, `.webloc`, scripts, installers) are
  only ever shown in the file manager; Magnetite does not open them.
- Open on a card counts only for a deliberate click, as Allow does below.

## Permission prompts

Chromium decides when a site may ask and keeps the answers in the page's
profile; Magnetite only draws the question, as a card in the notification
panel instead of Chromium's bubble at the page's top left (`prompts.rs`).

- A prompt shows while its page is on screen (tab, split pane or peek) and
  waits while it is not, so a card never speaks for a page out of sight.
- The card names the whole site: host and port, international names in
  punycode (`site_name` in the app), wrapped, never cut short.
- Only the notification panel may answer a prompt (the app checks the
  origin of `answer_permission`). Allow, Block and close (ask later) map to
  Chromium's accept, deny and dismiss.
- Allow counts only for a deliberate click: the cards have stayed as they
  are for a second, the pointer has moved over the card since, and the
  press began after that (`useDeliberate`); the app refuses an Allow sooner
  after a change of the cards too (`PROMPT_SETTLE`). A page that asks just
  before a click it expects, or keeps the user clicking where the card
  appears, cannot have that click grant it a permission.
- Windows that are not pages (an extension's own) keep Chromium's bubble.

- The list (`crates/app/src/downloads.rs`) keeps at most 100 entries in
  `state.json`: file name, place, size, status and start time, no addresses.
  Private profiles' downloads are listed while Magnetite runs, never saved.
- The UI names a list entry, never a path: Magnetite opens or reveals only
  files Chromium reported as downloaded, through the system's own opener.

## Updates

- A release's `latest.json` counts only with a valid Ed25519 signature from
  the release key whose public half is built into Magnetite (`update.rs`); a
  build without that key does not update itself. Packages count only with
  the manifest's SHA-256 and size; release-note links must be `https://`.
- macOS replaces the app only when the new bundle has the same bundle id,
  is signed by the same Apple developer team as the running app (a build
  without a team does not update itself) and Gatekeeper accepts it
  (notarized); otherwise, and on Linux and Windows, the release page opens.
  A failed install leaves the running app untouched; one install runs at a
  time.
- Versions compare by semantic versioning (`1.0.0-beta.10` is newer than
  `beta.9`); an older signed release is never installed over a newer one.
- Checks run at start and once a day, `https://` only, and can be turned
  off (Settings → About). Development builds may read a signed feed from
  this computer (`MAGNETITE_UPDATE_FEED`, `MAGNETITE_UPDATE_KEY`); release
  builds ignore both.
