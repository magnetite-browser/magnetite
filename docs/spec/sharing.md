# Sharing a folder

A folder of pinned tabs can be sent to someone as a link. Opened in
Magnetite, the link shows the folder's tabs and offers to add the folder
to the sidebar.

Nothing is uploaded: the folder travels in the link itself.

## Sharing

- Right-click a folder in the sidebar → "Copy link to share". The link is
  on the clipboard; the sidebar says so.
- The link carries the folder's title and icon, and for every tab its title
  and address. Folders inside the folder go along, with their tabs.
- A tab is shared with the address it returns to (where it was pinned),
  not the page it happens to show, and without tracking parameters.
- Pages that are not on the web (settings, files on this device) are left
  out. A folder without any web page is not shared.
- A smart folder is shared as the tabs it holds at that moment; the
  receiver gets a plain folder.
- One link carries at most 500 tabs.
- Nothing else leaves the device: no profile, no login, no history, and no
  word about who shared it.

Titles are the ones the sidebar shows. A page that puts an account name or
a subject into its title shares that with the link.

## The link

`magnetite://ui/shared#v1.<data>`

- `<data>` is base64url, without padding, of the raw deflate of a JSON
  text.
- The JSON is a folder: `{"t": title, "i": icon, "n": [...]}`. `i` is
  optional. Each entry of `n` is a tab, `{"t": title, "u": address}`, or
  another folder.
- The link names a page of Magnetite itself, so no server is involved at
  any point. Other browsers do not open it.
- Whoever has the link can read the folder, and the link cannot be taken
  back. A folder changed later needs a new link.

## Opening a shared link

Paste the link into the command bar (`command-bar.md`). The page shows the
folder's title, how many tabs it holds and the tabs with their sites,
folders inside it indented. Nothing is loaded from the listed sites.

- "Add to sidebar" puts the folder at the top of the current space's
  pinned tabs, with its tabs and folders. The tabs are not loaded until
  they are opened.
- The private space keeps no folders: there the sidebar says so and adds
  nothing.
- Like every page of Magnetite's own, the link opens only from the command
  bar: a website cannot send the browser to it.

A link is read as foreign input. It is refused as a whole ("This link
could not be read") when it holds an address that is not `http` or
`https`, more than 500 tabs, folders nested deeper than the sidebar allows
(32), more than 512 KB of text once unpacked, or no tab at all. Titles are
cut at 200 characters.

## Not built yet

- Sharing a whole space, or a single tab with a note.
- A link that follows later changes of the folder.
- A page that shows a shared folder in other browsers.
- Choosing where the folder goes when adding it.
