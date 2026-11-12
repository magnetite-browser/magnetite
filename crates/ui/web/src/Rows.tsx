import { memo, useRef, useState } from "react";
import { Favicon } from "./Favicon";
import { ChevronDown, Close, FolderIcon, HomeArrow, KeepActiveIcon, SmartFolderIcon, Speaker, SpeakerOff, WarningIcon } from "./Icons";
import { ago, findFolder } from "./smart";
import { Glyph, glyphFor } from "./Glyphs";
import { dragging, dropIndex, endDrag, indexAfterTaking, overRowMiddle, sideOf, splittable, startDrag } from "./dnd";
import { findTab, hostOf, send, splitIds, type FavoriteList, type Folder, type SidebarNode, type Side, type SplitNode, type TabItem, type TabRuntime, type UiState } from "./bridge";
import { SEPARATOR, type MenuItem } from "./ContextMenu";

export type Section = "favorites" | "pinned" | "today";

export interface RowContext {
  state: UiState;
  spaceId: string;
  openMenu: (x: number, y: number, items: MenuItem[]) => void;
  renaming: string | null;
  setRenaming: (id: string | null) => void;
  /** Open the command bar to create a profile for this tab. */
  newProfileFor: (tabId: string) => void;
  /** Split rows by the tab whose row they take (each split's first page in this space). */
  splitAt: Map<string, SplitNode>;
  /** Tabs in any split: their own rows give way to the split row. */
  inSplit: Set<string>;
  /** Open the glyph picker at `at`; `onPick(null)` asks for the default icon. */
  pickIcon: (at: { x: number; y: number }, current: string | undefined, onPick: (glyph: string | null) => void, allowDefault?: boolean) => void;
}

/** Most pages one split holds (as `magnetite_core::split::MAX_PANES`). */
const MAX_PANES = 4;

/** Right-click menu of a tab; `at` is the click point (the share menu opens there), `folderId` the folder it is in. */
export function tabMenu(ctx: RowContext, tab: TabItem, section: Section, at: { x: number; y: number }, folderId?: string): MenuItem[] {
  const items: MenuItem[] = [];
  const spaces = ctx.state.sidebar.spaces;
  const onScreen = ctx.state.panes.includes(tab.id);
  const inSplit = ctx.inSplit.has(tab.id);
  if (ctx.state.runtime[tab.id]?.loaded) items.push({ label: "Reload", onClick: () => send({ cmd: "reload", id: tab.id }) });
  items.push({ label: "Share…", onClick: () => send({ cmd: "share", id: tab.id, x: at.x, y: at.y }) });
  items.push({ label: "Copy link", onClick: () => navigator.clipboard?.writeText(tab.url) });
  items.push(SEPARATOR);
  if (section === "today") items.push({ label: "Pin tab", onClick: () => send({ cmd: "pin", id: tab.id }) });
  if (section === "pinned") {
    items.push({ label: "Unpin tab", onClick: () => send({ cmd: "unpin", id: tab.id }) });
    if (tab.home_url && tab.home_url !== tab.url) items.push({ label: "Back to pinned page", title: tab.home_url, onClick: () => send({ cmd: "reset_tab", id: tab.id }) });
  }
  if (section !== "favorites") items.push({ label: "Add to favorites", onClick: () => send({ cmd: "move_tab", id: tab.id, space: ctx.spaceId, section: "favorites", index: 999 }) });
  else items.push({ label: "Remove from favorites", onClick: () => send({ cmd: "move_tab", id: tab.id, space: ctx.spaceId, section: "today", index: 0 }) });
  items.push({ label: "Add to bookmarks", onClick: () => send({ cmd: "add_bookmark", id: tab.id }) });
  // Spared from unloading and auto-archive until it is let go again. A
  // favorite stays loaded anyway.
  if (section !== "favorites") items.push({ label: tab.keep_active ? "✓ Keep active" : "Keep active", onClick: () => send({ cmd: "set_keep_active", id: tab.id, keep: !tab.keep_active }) });
  if (ctx.state.active_tab && ctx.state.active_tab !== tab.id && !onScreen && ctx.state.panes.length < MAX_PANES) {
    items.push({ label: "Open in split view", onClick: () => send({ cmd: "add_split", id: tab.id }) });
  }
  if (inSplit) {
    items.push({ label: "Remove from split view", onClick: () => send({ cmd: "remove_split", id: tab.id }) });
    items.push({ label: "Separate all tabs", onClick: () => send({ cmd: "separate_all", id: tab.id }) });
  }
  items.push({ label: "Rename", onClick: () => ctx.setRenaming(tab.id) });
  if (tab.custom_title) items.push({ label: "Use page title", onClick: () => send({ cmd: "rename_tab", id: tab.id, title: "" }) });
  if (section === "favorites") {
    // Each profile has its own cookies and logins: two favorites of the same
    // site in two profiles are two accounts side by side.
    const { profiles, favorites } = ctx.state.sidebar;
    // A favorite runs in the profile of its list; choosing another moves it.
    const current = favorites.find((f) => f.items.some((t) => t.id === tab.id))?.profile_id;
    items.push(SEPARATOR);
    items.push({ label: "Duplicate favorite", onClick: () => send({ cmd: "duplicate_favorite", id: tab.id }) });
    for (const p of profiles) {
      items.push({
        label: `${p.id === current ? "● " : "○ "}Profile: ${p.name}`,
        onClick: () => p.id !== current && send({ cmd: "set_tab_profile", id: tab.id, profile: p.is_default ? null : p.id }),
      });
    }
    items.push({ label: "New profile for this favorite…", onClick: () => ctx.newProfileFor(tab.id) });
  }
  if (spaces.length > 1) {
    items.push(SEPARATOR);
    for (const sp of spaces) {
      if (sp.id === ctx.spaceId) continue;
      items.push({ label: `Move to ${sp.title}`, onClick: () => send({ cmd: "move_tab", id: tab.id, space: sp.id, section: "today", index: 0 }) });
    }
  }
  items.push(SEPARATOR);
  // A smart folder's own item: closing only unloads it, hiding keeps it out.
  const smart = folderId ? findFolder(ctx.state, folderId)?.folder.smart : undefined;
  if (smart?.items.includes(tab.home_url ?? tab.url)) items.push({ label: "Hide from this folder", onClick: () => send({ cmd: "smart_hide", id: tab.id }) });
  items.push({ label: section === "today" ? "Archive" : "Close", danger: section === "today", onClick: () => send({ cmd: "close_tab", id: tab.id }) });
  return items;
}

/**
 * The favorites above the space list: one list per profile that has some,
 * the same in every space. The lists get a title when there is more than one
 * or the only one is not the default profile's; a titled list collapses.
 */
export function FavoriteLists({ ctx, lists }: { ctx: RowContext; lists: FavoriteList[] }) {
  const { profiles } = ctx.state.sidebar;
  const profile = (id: string) => profiles.find((p) => p.id === id);
  const titled = lists.length > 1 || lists.some((l) => !profile(l.profile_id)?.is_default);
  return lists.map((list) => (
    <Favorites key={list.profile_id} ctx={ctx} list={list} title={titled ? (profile(list.profile_id)?.name ?? "Profile") : undefined} />
  ));
}

/**
 * The favorites of one profile: an icon per favorite. Drop a tab on the list
 * (also on its title while collapsed) to add it to this profile's favorites.
 */
function Favorites({ ctx, list, title }: { ctx: RowContext; list: FavoriteList; title?: string }) {
  const { state } = ctx;
  const key = "favorites:" + list.profile_id;
  const [open, setOpen] = useState(() => localStorage.getItem(key) !== "closed");
  const toggle = () => {
    setOpen((o) => {
      localStorage.setItem(key, o ? "closed" : "open");
      return !o;
    });
  };
  return (
    <div
      className="shrink-0 px-2 pt-3"
      onDragOver={(e) => dragging()?.kind === "tab" && e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        const d = dragging();
        if (d?.kind === "tab") send({ cmd: "move_tab", id: d.id, space: ctx.spaceId, section: "favorites", index: list.items.length, profile: list.profile_id });
        endDrag();
      }}
    >
      {title && (
        <button className="favs-title no-drag" aria-expanded={open} onClick={toggle} title={open ? "Collapse" : "Expand"}>
          <ChevronDown width={12} height={12} className="chevron" />
          <span className="truncate">{title}</span>
          {!open && <span className="count">{list.items.length}</span>}
        </button>
      )}
      {(open || !title) && (
        <div className="favs">
          {list.items.map((t) => {
            const rt = state.runtime[t.id];
            return (
              <button
                key={t.id}
                className="fav no-drag"
                data-active={state.active_tab === t.id || undefined}
                data-unloaded={!rt?.loaded || undefined}
                draggable
                onDragStart={(e) => startDrag(e, { kind: "tab", id: t.id })}
                onDragEnd={endDrag}
                onClick={() => send({ cmd: "activate_tab", id: t.id })}
                onContextMenu={(e) => { e.preventDefault(); ctx.openMenu(e.clientX, e.clientY, tabMenu(ctx, t, "favorites", { x: e.clientX, y: e.clientY })); }}
                title={title ? `${t.title} · ${title}` : t.title}
              >
                <Favicon url={t.url} src={rt?.favicon} loading={rt?.loading} />
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function TabRow({ ctx, tab, section, index, folderId, onDropAt }: {
  ctx: RowContext;
  tab: TabItem;
  section: Section;
  index: number;
  folderId?: string;
  onDropAt: (index: number) => void;
}) {
  // A split shows as one row, in place of its first page's row.
  if (ctx.inSplit.has(tab.id)) {
    const split = ctx.splitAt.get(tab.id);
    return split ? <SplitRow ctx={ctx} split={split} owner={tab.id} index={index} onDropAt={onDropAt} /> : null;
  }
  return (
    <TabRowInner
      tab={tab}
      rt={ctx.state.runtime[tab.id]}
      active={ctx.state.active_tab === tab.id}
      renaming={ctx.renaming === tab.id}
      section={section}
      index={index}
      folderId={folderId}
      ctx={ctx}
      onDropAt={onDropAt}
    />
  );
}

interface InnerProps {
  tab: TabItem;
  rt: TabRuntime | undefined;
  active: boolean;
  renaming: boolean;
  section: Section;
  index: number;
  folderId?: string;
  ctx: RowContext;
  onDropAt: (index: number) => void;
}

// Re-render only when this row's own data changes, or what its menu reads
// (spaces, the active tab, the pages on screen).
const TabRowInner = memo(
  function TabRowInner({ tab, rt, active, renaming, section, index, folderId, ctx, onDropAt }: InnerProps) {
    // `left`/`right`: a tab hovers the middle and would open beside this one.
    const [over, setOver] = useState<"before" | "after" | Side | null>(null);
    const awayFromHome = section !== "today" && !!tab.home_url && tab.home_url !== tab.url;
    const joining = over === "left" || over === "right" ? findTab(ctx.state, dragging()?.id) : null;
    const preview = joining && [<SplitCell key={tab.id} tab={tab} rt={rt} />, <SplitCell key="ghost" tab={joining} rt={ctx.state.runtime[joining.id]} ghost />];

    return (
      <div
        className={"row drop"}
        data-active={active || undefined}
        data-unloaded={!rt?.loaded || undefined}
        data-before={over === "before" || undefined}
        data-after={over === "after" || undefined}
        data-split-preview={joining ? true : undefined}
        draggable={!renaming}
        onDragStart={(e) => startDrag(e, { kind: "tab", id: tab.id })}
        onDragEnd={endDrag}
        onDragOver={(e) => {
          const d = dragging();
          if (!d || d.id === tab.id) return;
          // A folder is placed among the top-level pinned items only.
          if (d.kind === "folder" && (section !== "pinned" || folderId)) return;
          e.preventDefault();
          // Middle of another tab: preview the pair, the new tab on the half
          // under the pointer. Edges: reorder.
          if (splittable(d) && overRowMiddle(e)) setOver(sideOf(e));
          else setOver(dropIndex(e, index) === index ? "before" : "after");
        }}
        onDragLeave={(e) => !e.currentTarget.contains(e.relatedTarget as Node) && setOver(null)}
        onDrop={(e) => {
          e.preventDefault();
          const d = dragging();
          const side = over === "left" || over === "right" ? over : null;
          setOver(null);
          if (d && d.id !== tab.id) {
            if (side && splittable(d)) send({ cmd: "split_insert", target: tab.id, tab: d.id, side });
            else onDropAt(dropIndex(e, index));
          }
          endDrag();
        }}
        onClick={(e) => {
          if (renaming) return;
          if (e.altKey) send({ cmd: "add_split", id: tab.id });
          else send({ cmd: "activate_tab", id: tab.id });
        }}
        onDoubleClick={(e) => {
          // Two quick clicks on mute, close or back are not a rename.
          if (!(e.target instanceof Element && e.target.closest("button"))) ctx.setRenaming(tab.id);
        }}
        onAuxClick={(e) => e.button === 1 && send({ cmd: "close_tab", id: tab.id })}
        onContextMenu={(e) => {
          e.preventDefault();
          ctx.openMenu(e.clientX, e.clientY, tabMenu(ctx, tab, section, { x: e.clientX, y: e.clientY }, folderId));
        }}
        title={tab.url}
        data-folder={folderId}
      >
        {preview ? (
          over === "left" ? preview.reverse() : preview
        ) : (<>
          {awayFromHome ? (
            // A pinned tab away from its pinned page: hovering turns the icon into "back".
            <button
              className="home-btn"
              title={`Back to ${hostOf(tab.home_url!)} (the pinned page)`}
              aria-label={`Back to the pinned page, ${hostOf(tab.home_url!)}`}
              onClick={(e) => { e.stopPropagation(); send({ cmd: "reset_tab", id: tab.id }); }}
            >
              <Favicon url={tab.url} src={rt?.favicon} loading={rt?.loading} />
              <span className="home-arrow"><HomeArrow width={13} height={13} /></span>
            </button>
          ) : (
            <Favicon url={tab.url} src={rt?.favicon} loading={rt?.loading} />
          )}
          {renaming ? (
            <RenameInput
              value={tab.title}
              onCommit={(title) => {
                // Left as it was: no rename, so the title keeps following the page.
                if (title !== tab.title) send({ cmd: "rename_tab", id: tab.id, title });
                ctx.setRenaming(null);
              }}
              onCancel={() => ctx.setRenaming(null)}
            />
          ) : (
            <span className="title" data-unread={tab.unread || undefined}>
              {awayFromHome && <span className="away-slash" aria-hidden>/</span>}
              {tab.title || hostOf(tab.url)}
            </span>
          )}
          {(rt?.media?.playing || rt?.muted) && (
            <button className="slot" title={rt.muted ? "Unmute tab" : "Mute tab"} onClick={(e) => { e.stopPropagation(); send({ cmd: "set_muted", id: tab.id, muted: !rt.muted }); }}>
              {rt.muted ? <SpeakerOff width={14} height={14} /> : <Speaker width={14} height={14} />}
            </button>
          )}
          {tab.keep_active && (
            <span className="kept" title="Kept active: this tab is not unloaded" aria-label="Kept active">
              <KeepActiveIcon width={12} height={12} />
            </span>
          )}
          <button
            className="close"
            onClick={(e) => {
              e.stopPropagation();
              send({ cmd: "close_tab", id: tab.id });
            }}
            title={section === "today" ? "Archive" : "Close"}
          >
            <Close width={14} height={14} />
          </button>
        </>)}
      </div>
    );
  },
  (a, b) =>
    a.tab === b.tab && a.rt === b.rt && a.active === b.active && a.renaming === b.renaming &&
    a.section === b.section && a.index === b.index && a.folderId === b.folderId &&
    a.ctx.spaceId === b.ctx.spaceId && a.ctx.state.sidebar.spaces === b.ctx.state.sidebar.spaces &&
    a.ctx.state.active_tab === b.ctx.state.active_tab && a.ctx.state.panes === b.ctx.state.panes,
);

/**
 * Title editor in place of a title: Return or leaving the field saves, Esc
 * cancels (and goes no further: it would also hide the floating sidebar).
 */
export function RenameInput({ value, onCommit, onCancel }: { value: string; onCommit: (title: string) => void; onCancel: () => void }) {
  return (
    <input
      autoFocus
      className="rename"
      defaultValue={value}
      onBlur={(e) => onCommit(e.currentTarget.value)}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") {
          e.stopPropagation();
          onCancel();
        }
      }}
    />
  );
}

export function FolderRow({ ctx, folder, index, depth, onDropAt }: { ctx: RowContext; folder: Folder; index: number; depth: number; onDropAt: (index: number) => void }) {
  const [open, setOpen] = useState(() => localStorage.getItem("folder:" + folder.id) !== "closed");
  const [overInside, setOverInside] = useState(false);
  // Where the menu opened: the icon picker opens there too.
  const menuAt = useRef({ x: 0, y: 0 });
  const renaming = ctx.renaming === folder.id;
  const toggle = () => {
    setOpen((o) => {
      localStorage.setItem("folder:" + folder.id, o ? "closed" : "open");
      return !o;
    });
  };
  const count = countTabs(folder.children);
  const smart = folder.smart;
  const status = smart ? ctx.state.smart[folder.id] : undefined;
  const unread = smart && !open && folder.children.some((n) => n.kind === "tab" && n.unread);
  const settings = () => send({ cmd: "smart_folder_dialog", id: folder.id });
  const glyph = glyphFor(folder.icon);
  const changeIcon = (x: number, y: number) =>
    ctx.pickIcon({ x, y }, glyph ?? undefined, (g) => send({ cmd: "set_folder_icon", id: folder.id, icon: g }), true);
  const menu: MenuItem[] = smart
    ? [
        ...(status?.needs_approval
          ? [{ label: "Turn on on this device", onClick: () => send({ cmd: "smart_allow", id: folder.id }) }, SEPARATOR]
          : []),
        { label: "Refresh now", onClick: () => send({ cmd: "smart_refresh", id: folder.id }) },
        { label: "Smart folder settings…", onClick: settings },
        SEPARATOR,
        { label: "Rename folder", onClick: () => ctx.setRenaming(folder.id) },
        { label: "Change icon…", onClick: () => changeIcon(menuAt.current.x, menuAt.current.y) },
        { label: open ? "Collapse" : "Expand", onClick: toggle },
        SEPARATOR,
        { label: "Stop updating (keep tabs)", onClick: () => send({ cmd: "smart_unlink", id: folder.id }) },
        { label: "Delete folder and its items", danger: true, onClick: () => send({ cmd: "delete_folder", id: folder.id }) },
      ]
    : [
        { label: "Rename folder", onClick: () => ctx.setRenaming(folder.id) },
        { label: "Change icon…", onClick: () => changeIcon(menuAt.current.x, menuAt.current.y) },
        { label: open ? "Collapse" : "Expand", onClick: toggle },
        SEPARATOR,
        { label: "Turn into smart folder…", onClick: settings },
        SEPARATOR,
        { label: "Delete folder (keep tabs)", onClick: () => send({ cmd: "delete_folder", id: folder.id }) },
      ];
  const title = smart && status
    ? status.error ?? (status.updated_at ? `Smart folder · updated ${ago(status.updated_at)}` : "Smart folder")
    : undefined;
  return (
    <div>
      <div
        className="row"
        data-active={overInside || undefined}
        title={title}
        draggable={!renaming}
        onDragStart={(e) => startDrag(e, { kind: "folder", id: folder.id })}
        onDragEnd={endDrag}
        onDragOver={(e) => {
          const d = dragging();
          if (!d || d.id === folder.id) return;
          e.preventDefault();
          setOverInside(d.kind === "tab");
        }}
        onDragLeave={() => setOverInside(false)}
        onDrop={(e) => {
          e.preventDefault();
          const d = dragging();
          setOverInside(false);
          if (!d) return;
          if (d.kind === "tab") send({ cmd: "move_tab", id: d.id, space: ctx.spaceId, section: "pinned", index: 0, folder: folder.id });
          else onDropAt(dropIndex(e, index));
          endDrag();
        }}
        onClick={() => {
          if (!renaming) toggle();
        }}
        onDoubleClick={() => ctx.setRenaming(folder.id)}
        onContextMenu={(e) => {
          e.preventDefault();
          menuAt.current = { x: e.clientX, y: e.clientY };
          ctx.openMenu(e.clientX, e.clientY, menu);
        }}
      >
        <span className="icon-slot" data-smart={smart ? true : undefined}>{glyph ? <Glyph name={glyph} size={16} /> : smart ? <SmartFolderIcon /> : <FolderIcon open={open} />}</span>
        {renaming ? (
          <RenameInput
            value={folder.title}
            onCommit={(title) => {
              send({ cmd: "rename_folder", id: folder.id, title });
              ctx.setRenaming(null);
            }}
            onCancel={() => ctx.setRenaming(null)}
          />
        ) : (
          <span className="title" style={{ fontWeight: 600 }}>{folder.title || "Untitled"}</span>
        )}
        {status?.fetching && <span className="spinner" title="Refreshing" />}
        {smart && status?.error && !status.fetching && (
          <button className="slot" title={`${status.error} Click for settings.`} aria-label="Smart folder problem" onClick={(e) => { e.stopPropagation(); settings(); }}>
            <WarningIcon width={14} height={14} />
          </button>
        )}
        {unread && <span className="unread-dot" aria-label="New items" />}
        {!open && count > 0 && <span className="meta">{count}</span>}
      </div>
      {open && (
        <div style={{ paddingLeft: depth < 2 ? 16 : 8 }}>
          <NodeList ctx={ctx} nodes={folder.children} depth={depth + 1} folderId={folder.id} />
        </div>
      )}
    </div>
  );
}

function countTabs(nodes: SidebarNode[]): number {
  return nodes.reduce((n, x) => n + (x.kind === "tab" ? 1 : countTabs(x.children)), 0);
}

export function NodeList({ ctx, nodes, depth, folderId }: { ctx: RowContext; nodes: SidebarNode[]; depth: number; folderId?: string }) {
  const dropTab = (index: number) => {
    const d = dragging();
    if (!d) return;
    const at = indexAfterTaking(nodes, d.id, index);
    if (d.kind === "tab") send({ cmd: "move_tab", id: d.id, space: ctx.spaceId, section: "pinned", index: at, folder: folderId });
    // A folder is placed among the top-level items only (`move_folder` counts there).
    else if (!folderId) send({ cmd: "move_folder", id: d.id, index: at });
  };
  return (
    <>
      {nodes.map((n, i) =>
        n.kind === "tab" ? (
          <TabRow key={n.id} ctx={ctx} tab={n} section="pinned" index={i} folderId={folderId} onDropAt={dropTab} />
        ) : (
          <FolderRow key={n.id} ctx={ctx} folder={n} index={i} depth={depth} onDropAt={dropTab} />
        ),
      )}
    </>
  );
}

/**
 * A split as one row: its pages side by side in reading order. While it is on
 * screen the row is lit and so is the page in focus. Drop a tab on a page to
 * add it on the half under the pointer; × or middle-click takes a page out.
 * `owner`, `index`, `onDropAt`: the list slot the row stands in, so the row
 * can be dragged and dropped around like a tab.
 */
export function SplitRow({ ctx, split, owner, index, onDropAt }: { ctx: RowContext; split: SplitNode; owner?: string; index?: number; onDropAt?: (index: number) => void }) {
  const ids = splitIds(split);
  const onScreen = ids.includes(ctx.state.active_tab ?? "") && ctx.state.panes.length > 1;
  const [over, setOver] = useState<"before" | "after" | { cell: string; side: Side } | null>(null);
  const join = over && typeof over === "object" ? over : null;
  const joining = join ? findTab(ctx.state, dragging()?.id) : null;
  const ghost = joining && <SplitCell key="ghost" tab={joining} rt={ctx.state.runtime[joining.id]} ghost />;
  const reorder = (e: React.DragEvent) => index !== undefined && onDropAt && setOver(dropIndex(e, index) === index ? "before" : "after");
  const menu = (e: React.MouseEvent, id: string) => {
    e.preventDefault();
    ctx.openMenu(e.clientX, e.clientY, [
      { label: "Remove from split view", onClick: () => send({ cmd: "remove_split", id }) },
      { label: "Separate all tabs", onClick: () => send({ cmd: "separate_all", id }) },
      ...(onScreen ? [{ label: "Even out sizes", onClick: () => send({ cmd: "split_equalize", path: [] }) }] : []),
    ]);
  };
  return (
    <div
      className="split-row drop"
      data-on-screen={onScreen || undefined}
      data-before={over === "before" || undefined}
      data-after={over === "after" || undefined}
      draggable={!!owner}
      onDragStart={(e) => owner && startDrag(e, { kind: "tab", id: owner, pair: true })}
      onDragEnd={endDrag}
      onDragOver={(e) => {
        const d = dragging();
        if (!d || (d.pair && d.id === owner)) return;
        if (splittable(d) && overRowMiddle(e) && !ids.includes(d.id)) {
          if (ids.length >= MAX_PANES) return setOver(null); // full: no drop here
          e.preventDefault();
          // Over the placeholder or a gap the preview stays as it is: the
          // placeholder shifts the pages, which must not flip the side.
          const cell = (e.target as HTMLElement).closest<HTMLElement>("[data-cell]");
          if (cell) setOver({ cell: cell.dataset.cell!, side: sideOf(e, cell) });
        } else if (index !== undefined && onDropAt) {
          e.preventDefault();
          reorder(e);
        }
      }}
      onDragLeave={(e) => !e.currentTarget.contains(e.relatedTarget as Node) && setOver(null)}
      onDrop={(e) => {
        e.preventDefault();
        const d = dragging();
        setOver(null);
        if (d && join && splittable(d)) send({ cmd: "split_insert", target: join.cell, tab: d.id, side: join.side });
        else if (d && index !== undefined && onDropAt) onDropAt(dropIndex(e, index));
        endDrag();
      }}
    >
      {ids.map((id) => {
        const tab = findTab(ctx.state, id);
        if (!tab) return null;
        const cell = (
          <SplitCell
            key={id}
            tab={tab}
            rt={ctx.state.runtime[id]}
            active={onScreen && id === ctx.state.active_tab}
            onClick={() => send({ cmd: "activate_tab", id })}
            onAuxClick={(e) => e.button === 1 && send({ cmd: "remove_split", id })}
            onContextMenu={(e) => menu(e, id)}
            onRemove={() => send({ cmd: "remove_split", id })}
          />
        );
        return join?.cell === id ? (join.side === "left" ? [ghost, cell] : [cell, ghost]) : cell;
      })}
    </div>
  );
}

/** One page of a split row; `ghost`: where a dragged tab would land. */
function SplitCell({ tab, rt, active, ghost, onRemove, ...events }: {
  tab: TabItem;
  rt: TabRuntime | undefined;
  active?: boolean;
  ghost?: boolean;
  onRemove?: () => void;
  onClick?: () => void;
  onAuxClick?: (e: React.MouseEvent) => void;
  onContextMenu?: (e: React.MouseEvent) => void;
}) {
  return (
    <div className="split-cell" data-cell={ghost ? undefined : tab.id} data-ghost={ghost || undefined} data-active={active || undefined} data-unloaded={!ghost && !rt?.loaded ? true : undefined} title={tab.url} {...events}>
      <Favicon url={tab.url} src={rt?.favicon} loading={rt?.loading} />
      <span className="title">{tab.title || hostOf(tab.url)}</span>
      {onRemove && (
        <button className="close" title="Remove from split view" aria-label="Remove from split view" onClick={(e) => { e.stopPropagation(); onRemove(); }}>
          <Close width={12} height={12} />
        </button>
      )}
    </div>
  );
}
