// The bookmarks in place of the sidebar. Every space has its section, whatever
// space is in front: its name, then its folders and bookmarks. A bookmark
// belongs to one space and opens there, as a new tab.
import { useState } from "react";
import { ContextMenu, SEPARATOR, type MenuItem } from "./ContextMenu";
import { Favicon } from "./Favicon";
import { Back, ChevronDown, FolderIcon, Plus } from "./Icons";
import { RenameInput } from "./Rows";
import { hostOf, send, type Folder, type SidebarNode, type Space, type TabItem, type UiState } from "./bridge";
import { dropIndex, indexAfterTaking } from "./dnd";

/** The bookmark or folder being dragged (this view only; tabs have `dnd.ts`). */
let dragged: string | null = null;

/** What the rows share: the name being edited, and how to open a menu. */
interface Ctx {
  renaming: string | null;
  setRenaming: (id: string | null) => void;
  openMenu: (e: React.MouseEvent, items: MenuItem[]) => void;
}

/** Where a row stands: the space, and the folder it is in (none: the top level). */
interface Place {
  space: Space;
  folder?: Folder;
  list: SidebarNode[];
}

const countBookmarks = (nodes: SidebarNode[]): number => nodes.reduce((n, x) => n + (x.kind === "tab" ? 1 : countBookmarks(x.children)), 0);

/** Every bookmark below `nodes` with the folders above it, for a search. */
function flatten(nodes: SidebarNode[], path: string[] = []): { bookmark: TabItem; path: string[] }[] {
  return nodes.flatMap((n) => (n.kind === "tab" ? [{ bookmark: n, path }] : flatten(n.children, [...path, n.title])));
}

/** An open/closed state that is remembered under `key`. */
function useOpen(key: string, initially: boolean): [boolean, () => void] {
  const [open, setOpen] = useState(() => {
    const saved = localStorage.getItem(key);
    return saved ? saved === "open" : initially;
  });
  const toggle = () =>
    setOpen((o) => {
      localStorage.setItem(key, o ? "closed" : "open");
      return !o;
    });
  return [open, toggle];
}

/** Put the dragged item at `index` of `place.list`. */
function dropAt(place: Place, index: number) {
  if (!dragged) return;
  send({ cmd: "move_bookmark", id: dragged, space: place.space.id, folder: place.folder?.id, index: indexAfterTaking(place.list, dragged, index) });
  dragged = null;
}

export function BookmarksView({ state, onClose }: { state: UiState; onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const ctx: Ctx = {
    renaming,
    setRenaming,
    openMenu: (e, items) => {
      e.preventDefault();
      e.stopPropagation();
      setMenu({ x: e.clientX, y: e.clientY, items });
    },
  };
  const spaces = state.sidebar.spaces;
  const q = query.trim().toLowerCase();
  const total = spaces.reduce((n, sp) => n + countBookmarks(sp.bookmarks ?? []), 0);
  return (
    <div className="flex flex-col h-full">
      <div className="titlebar-drag h-[38px] shrink-0" />
      <div className="flex h-9 shrink-0 items-center gap-1 px-2">
        <button className="ib no-drag" onClick={onClose} title="Back"><Back /></button>
        <div className="flex-1 font-semibold">Bookmarks</div>
        {state.active_tab && (
          <button className="ib no-drag" title="Bookmark the page in front" aria-label="Bookmark the page in front" onClick={() => send({ cmd: "add_bookmark" })}><Plus /></button>
        )}
      </div>
      <div className="shrink-0 px-2 pb-1 no-drag">
        <input className="field" placeholder="Search bookmarks" value={query} onChange={(e) => setQuery(e.target.value)} autoFocus aria-label="Search bookmarks" />
      </div>
      <div className="scroll flex-1 min-h-0 px-2 pb-3 no-drag">
        {spaces.map((space) => (q ? <Found key={space.id} space={space} query={q} /> : <SpaceSection key={space.id} space={space} ctx={ctx} />))}
        {total === 0 && (
          <div className="px-2 py-6 text-[var(--muted)]">
            No bookmarks yet. Right-click a tab and choose "Add to bookmarks", or import them from another browser in Settings.
          </div>
        )}
        {total > 0 && q && !spaces.some((sp) => flatten(sp.bookmarks ?? []).some(({ bookmark }) => matches(bookmark, q))) && (
          <div className="px-2 py-6 text-[var(--muted)]">No matching bookmarks.</div>
        )}
      </div>
      {menu && <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />}
    </div>
  );
}

const matches = (b: TabItem, q: string) => b.title.toLowerCase().includes(q) || b.url.toLowerCase().includes(q);

/** A space's name over what it holds. */
function SpaceHead({ space, open, count, toggle, onContextMenu, onDrop }: { space: Space; open: boolean; count: number; toggle?: () => void; onContextMenu?: (e: React.MouseEvent) => void; onDrop?: () => void }) {
  const [over, setOver] = useState(false);
  return (
    <div
      className="bm-space"
      role={toggle ? "button" : undefined}
      tabIndex={toggle ? 0 : undefined}
      aria-expanded={toggle ? open : undefined}
      data-over={over || undefined}
      onClick={toggle}
      onKeyDown={(e) => e.key === "Enter" && toggle?.()}
      onContextMenu={onContextMenu}
      onDragOver={(e) => {
        if (!dragged || !onDrop) return;
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        onDrop?.();
      }}
    >
      {toggle && <ChevronDown width={12} height={12} className="bm-chevron" data-closed={!open || undefined} />}
      <span className="bm-space-name">{space.title}</span>
      {count > 0 && <span className="meta">{count}</span>}
    </div>
  );
}

/** One space: its name, which folds the section, then its folders and bookmarks. */
function SpaceSection({ space, ctx }: { space: Space; ctx: Ctx }) {
  const [open, toggle] = useOpen("bookmarks:" + space.id, true);
  const list = space.bookmarks ?? [];
  const place: Place = { space, list };
  const newFolder = () => send({ cmd: "new_bookmark_folder", space: space.id }).then((r) => r.id && ctx.setRenaming(r.id));
  return (
    <section>
      <SpaceHead
        space={space}
        open={open}
        count={countBookmarks(list)}
        toggle={toggle}
        onContextMenu={(e) => ctx.openMenu(e, [{ label: "New folder", onClick: newFolder }])}
        onDrop={() => dropAt(place, list.length)}
      />
      {open && <Nodes place={place} depth={0} ctx={ctx} />}
    </section>
  );
}

/** What a search finds in one space: the bookmarks alone, each with its folders. */
function Found({ space, query }: { space: Space; query: string }) {
  const found = flatten(space.bookmarks ?? []).filter(({ bookmark }) => matches(bookmark, query));
  if (found.length === 0) return null;
  return (
    <section>
      <SpaceHead space={space} open count={found.length} />
      {found.map(({ bookmark, path }) => (
        <div key={bookmark.id} className="row w-full" role="button" tabIndex={0} title={bookmark.url} onClick={(e) => send({ cmd: "open_bookmark", id: bookmark.id, background: e.metaKey })} onKeyDown={(e) => e.key === "Enter" && send({ cmd: "open_bookmark", id: bookmark.id })}>
          <Favicon url={bookmark.url} />
          <span className="title text-left">{bookmark.title || hostOf(bookmark.url)}</span>
          {path.length > 0 && <span className="meta max-w-[96px] truncate">{path.join(" / ")}</span>}
        </div>
      ))}
    </section>
  );
}

function Nodes({ place, depth, ctx }: { place: Place; depth: number; ctx: Ctx }) {
  return (
    <>
      {place.list.map((node, index) =>
        node.kind === "folder" ? (
          <FolderRow key={node.id} folder={node} place={place} depth={depth} ctx={ctx} />
        ) : (
          <BookmarkRow key={node.id} bookmark={node} index={index} place={place} ctx={ctx} />
        ),
      )}
    </>
  );
}

function BookmarkRow({ bookmark, index, place, ctx }: { bookmark: TabItem; index: number; place: Place; ctx: Ctx }) {
  const renaming = ctx.renaming === bookmark.id;
  const open = (background: boolean) => send({ cmd: "open_bookmark", id: bookmark.id, background });
  return (
    <div
      className="row w-full"
      role="button"
      tabIndex={0}
      title={bookmark.url}
      draggable={!renaming}
      onDragStart={(e) => {
        dragged = bookmark.id;
        e.dataTransfer.effectAllowed = "move";
        e.dataTransfer.setData("text/plain", bookmark.url);
      }}
      onDragEnd={() => (dragged = null)}
      onDragOver={(e) => dragged && dragged !== bookmark.id && e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        dropAt(place, dropIndex(e, index));
      }}
      onClick={(e) => !renaming && open(e.metaKey)}
      onKeyDown={(e) => e.key === "Enter" && e.target === e.currentTarget && open(false)}
      onContextMenu={(e) =>
        ctx.openMenu(e, [
          { label: `Open in ${place.space.title}`, onClick: () => open(false) },
          { label: "Open in the background", onClick: () => open(true) },
          SEPARATOR,
          { label: "Rename", onClick: () => ctx.setRenaming(bookmark.id) },
          { label: "Remove bookmark", danger: true, onClick: () => send({ cmd: "remove_bookmark", id: bookmark.id }) },
        ])
      }
    >
      <Favicon url={bookmark.url} />
      {renaming ? (
        <RenameInput
          value={bookmark.title}
          onCommit={(title) => {
            if (title.trim() && title !== bookmark.title) send({ cmd: "rename_bookmark", id: bookmark.id, title });
            ctx.setRenaming(null);
          }}
          onCancel={() => ctx.setRenaming(null)}
        />
      ) : (
        <span className="title text-left">{bookmark.title || hostOf(bookmark.url)}</span>
      )}
    </div>
  );
}

function FolderRow({ folder, place, depth, ctx }: { folder: Folder; place: Place; depth: number; ctx: Ctx }) {
  // Folders start closed: an imported tree can be large.
  const [open, toggle] = useOpen("bookmark-folder:" + folder.id, false);
  const [over, setOver] = useState(false);
  const renaming = ctx.renaming === folder.id;
  const inside: Place = { space: place.space, folder, list: folder.children };
  const count = countBookmarks(folder.children);
  const newFolder = () => send({ cmd: "new_bookmark_folder", space: place.space.id, parent: folder.id }).then((r) => r.id && ctx.setRenaming(r.id));
  return (
    <div>
      <div
        className="row w-full"
        role="button"
        tabIndex={0}
        aria-expanded={open}
        data-active={over || undefined}
        draggable={!renaming}
        onDragStart={(e) => {
          dragged = folder.id;
          e.dataTransfer.effectAllowed = "move";
          e.dataTransfer.setData("text/plain", folder.title);
        }}
        onDragEnd={() => (dragged = null)}
        onDragOver={(e) => {
          if (!dragged || dragged === folder.id) return;
          e.preventDefault();
          setOver(true);
        }}
        onDragLeave={() => setOver(false)}
        onDrop={(e) => {
          e.preventDefault();
          setOver(false);
          // Onto a folder: into it, at the end.
          dropAt(inside, folder.children.length);
        }}
        onClick={() => !renaming && toggle()}
        onKeyDown={(e) => e.key === "Enter" && e.target === e.currentTarget && toggle()}
        onContextMenu={(e) =>
          ctx.openMenu(e, [
            { label: "New folder inside", onClick: newFolder },
            { label: "Rename folder", onClick: () => ctx.setRenaming(folder.id) },
            SEPARATOR,
            { label: count ? `Remove folder and its ${count} bookmark${count === 1 ? "" : "s"}` : "Remove folder", danger: true, confirm: count ? "Click again to remove them" : undefined, onClick: () => send({ cmd: "remove_bookmark", id: folder.id }) },
          ])
        }
      >
        <span className="icon-slot"><FolderIcon open={open} /></span>
        {renaming ? (
          <RenameInput
            value={folder.title}
            onCommit={(title) => {
              if (title.trim() && title !== folder.title) send({ cmd: "rename_bookmark", id: folder.id, title });
              ctx.setRenaming(null);
            }}
            onCancel={() => ctx.setRenaming(null)}
          />
        ) : (
          <span className="title text-left" style={{ fontWeight: 600 }}>{folder.title || "Untitled"}</span>
        )}
        {!open && count > 0 && <span className="meta">{count}</span>}
      </div>
      {open && (
        <div style={{ paddingLeft: depth < 2 ? 16 : 8 }}>
          <Nodes place={inside} depth={depth + 1} ctx={ctx} />
        </div>
      )}
    </div>
  );
}
