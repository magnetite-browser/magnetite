# Bookmarks

A bookmark is an address with a name that is kept without being a tab. Each
space has its own bookmarks, in folders. The bookmarks view shows those of
all spaces at once.

## The view

The footer button at the bottom left of the sidebar opens it: rest on the
button and choose "Bookmarks", or make bookmarks what a click opens (Settings
→ Tabs & archive). The command "Open bookmarks" does the same. The view
takes the sidebar's place; the arrow at its top goes back.

- **One section per space**, in the order of the spaces, whatever space is
  in front. The space's name stands over it, in small capitals of a
  monospace type, with the number of its bookmarks. A click on the name
  folds the section.
- Below it the space's **folders and bookmarks**, nested as deep as they
  go. A click on a folder opens or closes it; a closed one shows how many
  bookmarks it holds. Folders start closed.
- What is folded and what is open is remembered on this device.
- **Search** at the top finds bookmarks by name or address in all spaces.
  The matches are listed under their space, each with the folders it is
  in.

## Opening

A click on a bookmark opens it as a new Today tab **in the space it belongs
to**, with that space's logins, and that space comes to the front.
`Cmd`+click, or "Open in the background" in its menu, opens it as a
background tab of that space. The bookmark stays.

The command bar finds bookmarks too (`command-bar.md`).

## Saving and filing

- **Add to bookmarks** in a tab's menu, the "+" in the view (the page in
  front) and the command "Bookmark this page" save a tab's page to the
  bookmarks of the tab's space, at the top. A favorite belongs to no space:
  the space in front takes it. The sidebar says where it went.
- An address is saved once per space. Magnetite's own pages and local files
  are not saved.
- **Folders**: "New folder" in a space name's menu, "New folder inside" in a
  folder's menu. The new folder asks for its name at once.
- **Rename** and **Remove** are in the menu of a bookmark or a folder.
  Removing a folder removes what it holds, after a second click.
- **Dragging** moves a bookmark or a folder: onto a folder to put it in,
  onto a bookmark to place it before or after, onto a space's name to move
  it to that space. A folder does not go into itself.

## What a bookmark is not

- Not a tab: it has no page, takes no memory and is never archived.
- Not a pinned tab or a favorite: those keep a page and return to it.

## Import and sync

- Bookmarks of other browsers come in as bookmarks of the imported space,
  in their folders ("Bookmarks bar", "Other bookmarks" and so on,
  `import.md`).
- A bookmarks file adds its bookmarks to the current space (`import.md`).
- Bookmarks are part of their space, so they sync with it and stay on the
  device with a space that is only on this device (`sync.md`). What arrives
  is checked like tabs: addresses Magnetite does not open from stored data
  are dropped.

## Data

`state.json`, in each space: `bookmarks`, a tree of folders and bookmarks. A
bookmark keeps its address, its name and when it was saved.

## Not built yet

- Editing a bookmark's address.
- Sorting.

Code: `crates/app/src/state/bookmarks.rs`, `crates/app/src/app/tabs.rs`
(`open_bookmark`), `crates/ui/web/src/BookmarksView.tsx`,
`crates/ui/web/src/LibraryButton.tsx`.
