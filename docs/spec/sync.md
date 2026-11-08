# Sync

Sync keeps the sidebar (profiles, favorites, spaces with their pinned and
Today tabs) and a few settings the same on several devices. It is optional
and off until you turn it on. There is no account: a 24-word sync code is
the only key, and the devices that hold it form a sync chain. The data is
encrypted on the device, so the server sees only an anonymous key and padded
ciphertext. The server is one you can host yourself; Magnetite also runs a
free one at `sync.magnetitebrowser.org`.

This page covers the browser's side. The server (one binary and a Postgres
database), its configuration, limits and API are documented in its own
repository, <https://github.com/magnetite-browser/magnetite-sync>
(`README.md`, `docs/design.md`). Background: `docs/adr/0005-sync.md`,
`docs/adr/0008-public-sync-server.md`, `docs/design/security.md`.

## Setup (Settings → Sync)

1. Server: "Magnetite Sync" (`https://sync.magnetitebrowser.org`,
   preselected, no sign-up) or "Your own server" with its address. It needs
   `https://`; plain `http://` is accepted only for `localhost`, `127.0.0.1`
   and `::1`. A path prefix is fine; a user name or a query string is not.
2. First device: "Start syncing" (with your own server: plus an invite code
   if it asks for one). Magnetite creates the sync code, shows its 24 words
   and asks you to keep them. Nobody can recover them for you.
3. Other devices: paste the 24 words and "Join"; case and spacing do not
   matter. A device that joins takes the chain's sidebar in place of its
   own (see "Taking another device's data"). While the chain holds nothing
   yet, it waits and says so: only the device that started the chain writes
   first.
   A device that is in a chain cannot start or join another before it
   leaves.
4. While syncing, the page shows the server, the time of the last sync and
   the last error, with "Sync now" and "Show sync code".
5. "Leave on this device" forgets the code here; the chain and the other
   devices stay as they are. "Delete sync chain…" (click twice) also erases
   the chain on the server for every device.

## When and how it syncs

- Shortly after start, then every 5 minutes, on "Sync now" and right after
  starting or joining a chain.
- Each round handles the two collections in turn. It asks the server first:
  a newer version there is taken. Otherwise the device writes its own copy
  as the next version, but only when it has something new: not when the
  server holds the same, and not when the sidebar is still as the device
  took it from the chain (apart from what a device keeps of its own, see
  below). So a device left running does not write over what another one
  changed while offline.
- What is sent is the saved sidebar without the tabs other devices would
  not take: Magnetite's own pages and local files.
- A collection is replaced as a whole; there is no merging. When two devices
  change the sidebar between syncs, the one that syncs second takes the
  other's sidebar and loses its own changes since its last sync.
- Versions only go up. A write counts once the server confirms exactly the
  version written, and a server copy older than what the device has seen is
  refused, so a server cannot roll a device back.

## What syncs

| Collection | Content |
|---|---|
| `sidebar` | profiles, favorites, spaces with their pinned trees (folders, smart folder definitions), Today tabs and bookmarks |
| `settings` | when Today tabs are archived, when inactive tabs are suspended, where links from other apps open (a tab, a peek or a mini window; a version that does not know a choice uses a new tab) |

The server never sees these names, only ids derived from the sync code.

Not synced: the archive, downloads, cookies and logins, which pages are
open, split views, the search engine (a device holding the words could
otherwise send every search to an address of its choice), appearance and
all other settings, pinned extensions, and smart folder accounts and tokens.
Today tabs of a private profile are not synced; its pinned tabs and
favorites sync with the address they return to, not the page they show.

### Only on this device

A space can be kept on one device: Settings → Sync → "Spaces" → "Only on
this device". The choice is that device's own and is not synced, and it can
be made before the device starts or joins a chain. For a work space that
should not reach a private device, turn it on at the work device.

- The device no longer sends the space. A device whose spaces are all kept
  this way sends no sidebar at all and only takes the chain's.
- A sidebar that arrives leaves the space as it is on the device, at its
  place in the list (see below).
- On the other devices the space goes with their next sync.
- A separate profile that only such spaces use stays on the device with
  them: the profile, its favorites, and tabs in other spaces that run in
  it are not sent either. Settings → Isolation marks it "only on this
  device". On the other devices that profile and its favorites stay, as
  for any profile a sidebar does not name, until they are removed there.
  As soon as a synced space uses the profile it is sent again. The default
  profile always is.
- Turned off again, the space is sent like the rest.

## Taking another device's data

The sidebar from the server replaces this device's, with limits so that
whoever holds the words cannot harm the device through it:

- This device's profiles stay, and whether a profile is private, and which
  one is the default, is decided here. A profile only this device has
  keeps its favorites. Cookies and logins of a profile are deleted only
  when the profile is deleted here.
- Deleting a profile counts for the device it is done on. The other
  devices keep the profile until it is deleted there, and the device that
  deleted it does not take it back from their list. It returns only with a
  sidebar that uses it (a space, a favorite or a tab runs in it), and then
  empty.
- A space that is only on this device stays untouched, and so do a
  profile that stays with it and that profile's favorites, whatever the
  other device says about them. A copy of such a space that arrives is old
  and is dropped, and this device writes its sidebar so that the other
  devices lose the copy. A tab or folder of such a space that arrives in
  another space was moved there on another device, and is there.
- Tabs the other device never had stay as Today tabs: the one last active
  in each space, and those that are never sent (private Today tabs,
  Magnetite's own pages, tabs in a synced space that run in a profile that
  stays on this device). A private one stays private.
- A page that has to open again in another profile closes and leaves its
  split.
- Pages of tabs that still exist stay open and keep their own address and
  title. Pages of tabs that are gone are closed. Nothing that arrived loads
  before you open it.
- Smart folders added or changed on another device do not fetch until they
  are turned on here (`smart-folders.md`).
- Tabs with addresses Magnetite does not open from stored data are dropped.
  Colours, timestamps, smart folder limits and duplicate ids are checked,
  and settings pass the same checks as on the Settings page.
- A sidebar without spaces is refused, and data this version cannot read is
  not taken. Neither counts as applied; Settings → Sync shows the reason
  after every round until usable data is there. No device writes a sidebar
  without spaces.

## Safety

- Every request is signed with a key derived from the sync code. The
  signature covers the server's host, so no other server can replay it,
  and the time, so the device's clock must be right to within 5 minutes.
- Redirects are not followed. A request gives up after 10 s without a
  connection or 60 s in total. An answer over 32 MiB is refused.
- Errors are shown in Magnetite's own words by status code; text from the
  server never reaches the page.
- Only the Settings page and the welcome page of the first start
  (`first-start.md`) may show the code or start, join, leave or delete a
  chain.
- Leaving and deleting take effect on the device at once, even while a
  request hangs. The deletion on the server follows. Until the server
  confirms it (204, or 404 for a chain already gone), the device keeps the
  code and tries again at its next start.

## Where the data lives

| Where | What |
|---|---|
| `sync.key` in the data folder | the sync code, readable by the owner only (0600); all keys and ids are derived from it |
| `sync.delete` in the data folder | code and server of a chain whose deletion the server has not confirmed yet |
| `settings.sync` in `state.json` | on or off, the server address, the last version per collection, the time of the last sync, the last error |
| the server | the chain's public key, the days it was created and last used, collection ids, versions and ciphertext; by default a chain unused for 400 days is deleted (its README, "Data it stores") |

The code is 32 random bytes, shown as 24 BIP39 words. From it the device
derives an Ed25519 signing key (its public half is the chain's id on the
server), an XChaCha20-Poly1305 key and the collection ids. Data is padded to
a power of two, at least 1 KiB, before it is encrypted. This protocol code
is the `magnetite-sync-proto` crate in the server's repository; the browser
builds it from a checkout next to this one.

## Not built yet

- Syncing the archive.

Code: `crates/app/src/sync.rs` (client), `crates/app/src/state/adopt.rs`
(`adopt_pull`, `lasting_sidebar`), `crates/app/src/app/syncing.rs` (`sync_now`,
`on_sync_outcome`), `crates/ui/web/src/settings/SyncSection.tsx`.
