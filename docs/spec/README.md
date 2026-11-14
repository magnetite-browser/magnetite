# Feature specs

How Magnetite behaves, feature by feature. Each spec describes what is built
today, checked against the code, and ends with what is not built yet. A change
in behaviour changes its spec in the same pull request.

Shortcuts are the macOS ones. On Windows and Linux `Cmd` is `Ctrl` and
`Option` is `Alt`; where the keys differ more, the app's own labels say
so (Settings → About lists them).

| Spec | Covers |
|---|---|
| [first-start.md](first-start.md) | the welcome page: what a new installation asks before it does anything |
| [sidebar.md](sidebar.md) | layout, favorites, pinned and Today tabs, folders, hiding |
| [spaces.md](spaces.md) | spaces, profiles and how sites are kept apart |
| [command-bar.md](command-bar.md) | addresses, search, site search, open tabs, bookmarks, history, the archive and commands in one input |
| [archive.md](archive.md) | closing a tab, auto-archive, restoring |
| [history.md](history.md) | the pages visited on this device: what is kept, for how long, the history view |
| [bookmarks.md](bookmarks.md) | saved addresses per space, in folders, in one view for all spaces |
| [split-view.md](split-view.md) | several pages side by side |
| [peek.md](peek.md) | a link opened over the page, without leaving it |
| [mini-window.md](mini-window.md) | a page in a small window of its own, for links from other apps and the quick search from anywhere |
| [site-controls.md](site-controls.md) | address row, site panel, permissions, DevTools, capture |
| [ad-blocking.md](ad-blocking.md) | blocking ads and trackers with filter lists the user chooses |
| [passwords-and-autofill.md](passwords-and-autofill.md) | Chromium's password manager and autofill: switches, saved lists, what stays local |
| [media.md](media.md) | media controls in the sidebar, picture-in-picture |
| [extensions.md](extensions.md) | Chrome extensions: pinned icons, the Extensions menu, popups |
| [smart-folders.md](smart-folders.md) | folders that fill themselves from GitHub, GitLab, Jira or YouTrack |
| [import.md](import.md) | bringing tabs over from another browser |
| [arc-sidebar-format.md](arc-sidebar-format.md) | the sidebar file of Arc, as far as the importer reads it |
| [sync.md](sync.md) | optional, end-to-end encrypted sync between devices |

Why Magnetite is built the way it is: `docs/adr/`. The rules its own code
keeps on top of Chromium's protections: `docs/design/security.md`.
