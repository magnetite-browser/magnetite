# History

Magnetite keeps its own record of the pages visited on this device, to
find them again in the command bar and the history view. The record is
small on purpose: address, title and time of each visit, kept for a period
the user chooses, never synced, and nothing from private profiles.

## What is recorded

A visit is a page shown in the main frame of a tab, a peek or a mini
window (`mini-window.md`): a link
followed, an address typed, a page restored when its tab loads again.

- Only `http://` and `https://` pages. Magnetite's own pages, Chromium's
  (`chrome://`), local files, `about:` and other schemes are not recorded.
- Nothing from a private profile (`spaces.md`), whether in a tab, a peek or
  a mini window.
- The same page again within an hour (a reload, the same address with
  another `#place`, a trailing slash) updates the visit already there: its
  time and address are the new ones and it moves to the top. Later it is a
  visit of its own.
- The title is the page's own, filled in when the page names itself. A
  page that never does shows its host.
- At most 10,000 visits; the oldest go first.

## The view

The history view replaces the sidebar, like the archive. It opens from:

- the footer button: rest on it and choose "History", or make history what
  a click opens (Settings → Tabs & archive);
- History → Show History in the menu bar, or `Cmd+Y`;
- the command "Open history".

It lists the newest 500 visits by day, each with its time, and has a search
field that matches title and address. A click opens the page as a new Today
tab in the current space (`Cmd`+click: in the background). The × on a row
removes that visit; "Clear" at the top removes all of them after a second
click.

The command bar finds visits too, below tabs and bookmarks
(`command-bar.md`).

## How long visits stay

| Setting | Options | Default |
|---|---|---|
| Settings → Tabs & archive → "Keep history for" | 7, 30 or 90 days, or "Don't keep history" | 90 days |

The minute tick deletes older visits; a shorter period, or "Don't keep
history", takes effect at once and deletes what is older for good. With
"Don't keep history" nothing is recorded. Settings → Tabs & archive shows
the number of visits and has "Clear history" (after a second click). The
period is this device's and does not sync.

## Data

`<data-dir>/history.json`, a list of visits, oldest first: an id, the
address, the title and the time. No space, profile, tab or count of visits.
The file is readable by its owner only and written whole: once a minute
when something changed, at once when visits are removed or cleared, and
when Magnetite quits. With no visits there is no file. A file that cannot
be read is started over. It is never synced and not part of `state.json`.

## Not built yet

- Removing all visits of one site.
- Import of another browser's history.

Code: `crates/app/src/state/history.rs`, `crates/app/src/state/search.rs`,
`crates/app/src/app/events.rs` (`UrlChanged`, `TitleChanged`),
`crates/app/src/app/tabs.rs` (`housekeeping`),
`crates/ui/web/src/HistoryView.tsx`, `crates/ui/web/src/LibraryButton.tsx`.
