# Smart folders

A smart folder is a folder in the pinned section that fills itself from a
service and stays up to date: your open pull requests on GitHub, merge
requests on GitLab, the Jira or YouTrack issues assigned to you. Magnetite
asks the service directly from this computer, with an access token you
connect.

## Sources

| Service | Server | Shows | Item titles |
|---|---|---|---|
| GitHub | github.com or a GitHub Enterprise server | open items, one of: my pull requests plus review requests (default), review requests, my pull requests, issues assigned to me; optional extra search terms (`org:acme`, `-label:wip`) | "Title · repo#12" |
| GitLab | gitlab.com or your own GitLab | the same four choices with merge requests; optionally only one group and its subgroups (`acme/platform`) | "Title · project!12", issues "Title · project#12" |
| Jira | Jira Cloud (`….atlassian.net`) or Jira Data Center | any JQL; presets: assigned to me (default), reported by me, watching | "ABC-12 Summary" |
| YouTrack | any YouTrack address | any YouTrack search; presets: assigned to me (default), reported by me, starred | "ABC-12 Summary" |

GitHub and GitLab lists are ordered by last update, newest first; Jira and
YouTrack lists follow the query. The default queries are
`assignee = currentUser() AND resolution = Unresolved ORDER BY updated DESC`
(Jira) and `for: me #Unresolved sort by: updated desc` (YouTrack).

## Behaviour

- Items are pinned tabs inside the folder. A refresh adds what the source
  lists (bold until opened, except on the first fill) and drops what it no
  longer lists. A dropped item whose page is open stays until a refresh
  finds it closed. Titles follow the source unless you renamed the tab.
- Tabs and folders you put into the folder by hand stay, above the source's
  items.
- "Hide from this folder" (tab menu) removes an item and keeps it out while
  the source still lists it. Closing only unloads it, as for any pinned tab.
- Refresh: when Magnetite starts, then every 5, 15 (default), 30 or 60
  minutes, and on "Refresh now" (folder menu, Settings). A folder takes at
  most 10, 20 (default) or 50 items from its source.
- A failed refresh keeps the items shown. The folder row shows a warning
  with the reason; a click on it opens the folder's settings. The next try
  follows after the refresh interval. A request gives up after 20 seconds.
- A folder without a connected account for its service and server says so
  and fills as soon as one is connected.
- Only items with web addresses are taken from the service's answer.
- "Stop updating" turns it back into a normal folder; its tabs stay.

## UI

- **Create:** + menu → New Smart Folder (it appears at the top of the
  current space's pinned section), Settings → Smart folders → New smart
  folder, or right-click a folder → Turn into smart folder….
- **Turning a folder smart:** the dialog suggests a source from the folder's
  tabs. Tabs on the source's server count as the folder's items, so those
  the source does not list disappear on the first refresh. Other tabs stay.
- **Dialog** (a centred window, like the command bar): name, source, server
  or address, what to show or the query, account, a live preview of the
  first six items, refresh interval and size. Without a name the folder is
  called "Pull requests", "Merge requests" or "Issues". Esc and Cancel
  discard the draft; clicking elsewhere closes the dialog but keeps the
  draft, without the token, for 15 minutes, so you can create a token in a
  tab and come back.
- **Sidebar:** a smart folder icon in the space colour (or the folder's own
  icon), a spinner while refreshing, a warning sign after a failure, and a
  dot on a collapsed folder with new items.
- **Settings → Smart folders:** every smart folder with its space, source
  and status, and the connected accounts (connect, disconnect).

## Accounts

An account is an access token for one server. On connect, Magnetite asks
the server whose token it is and keeps the login and display name with it.
A server can have several accounts; a folder uses the one chosen in its
dialog, else the first for its service and server. Connecting the same login
on the same server again replaces the token.

Servers are reached over `https://`; plain `http://` is accepted only for
this computer (`localhost`, `127.0.0.1`, `::1`). An address with a user name
or password in it is refused. The dialog links to the server's token page.

- GitHub: a token with read access to pull requests and issues.
- GitLab: a personal access token with the `read_api` scope.
- Jira Cloud: the Atlassian account email plus an API token (sent as Basic
  auth). Jira Data Center: a personal access token.
- YouTrack: Profile → Account security → New token, scope YouTrack.

## Sync

The folder's definition and its items are part of the sidebar, so they sync
when sync is on (`sync.md`). Accounts and tokens never sync. A smart folder
that another device added, or whose source or account it changed, does not
fetch here until it is turned on ("Turn on on this device" in the folder
menu, or saving it in the dialog): the sync code alone must not make this
device's accounts run a query. A folder that an import on this device
brought is on from the start.

## Data and privacy

- **Requests** go directly from this computer to the service you connect,
  hosted by its provider or by you, with your token. No Magnetite server and
  no third party is involved.
- **Tokens** live in `smart-accounts.json` in the data folder, readable by
  the owner only (0600), with the account's service, server, login and
  display name. They are never written to `state.json`, never synced and
  never logged (debug tracing with `MAGNETITE_LOG` records the names of
  commands, not their contents). Disconnecting deletes the token here;
  revoke it on the server as well.
- **Stored data:** the folder's definition (service, server, query, account
  login, size, interval), the addresses of its items and of hidden items,
  and the items themselves as pinned tabs (title, address) are part of the
  sidebar in `state.json`. Titles can contain personal data from the service
  (names in issue titles); they are needed to show the item. The refresh
  status (last update, last error) is kept in memory only.
- **Retention:** an item leaves the folder when the source stops listing
  it; a hidden address is forgotten at the same point.
- **Who processes it:** your own device and the service you connect. The
  Magnetite project receives none of this data.

Code: `crates/core/src/smart.rs` (model, `apply_smart`),
`crates/app/src/smart.rs` (accounts, fetching),
`crates/app/src/app/smart_folders.rs`, `crates/app/src/state/folders.rs`
(which folders may fetch), `crates/ui/web/src/SmartFolderDialog.tsx`, `smart.ts`,
`settings/SmartFoldersSection.tsx`. A new service needs a `SmartSource`
variant, a fetcher with its `whoami` request, and its fields in the dialog.
