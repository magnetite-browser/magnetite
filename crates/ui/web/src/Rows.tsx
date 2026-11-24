import { memo, useEffect, useRef, useState } from "react";
import { Favicon } from "./Favicon";
import { ChevronDown, Close, FolderIcon, HomeArrow, KeepActiveIcon, Minus, PhoneIcon, ScreenShareIcon, SmartFolderIcon, SnoozeIcon, Speaker, SpeakerOff, WakeIcon, WarningIcon } from "./Icons";
import { ago, findFolder, localInputValue, snoozeChoices, wakeLabel } from "./smart";
import { Glyph, glyphFor } from "./Glyphs";
import { dragging, dropIndex, endDrag, indexAfterTaking, movedIds, overRowMiddle, rowZone, sideOf, splittable, startDrag } from "./dnd";
import { errorCount } from "./DevBar";
import { findTab, hostOf, IS_MAC, send, splitGroup, splitIds, type DeviceTab, type FavoriteList, type Folder, type SidebarNode, type Side, type Snoozed, type SplitNode, type TabItem, type TabRuntime, type UiState } from "./bridge";
import { SEPARATOR, type MenuItem } from "./ContextMenu";
import { language, t } from "./i18n";

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

// The elements the current run of clicks began on (`detail` 1), for
// `movedUnder`: the whole path, as the element hit can be gone by the next
// click (a folder's icon is swapped when it opens).
let firstPath: EventTarget[] = [];
const noteFirstClick = (e: MouseEvent) => {
  if (e.detail === 1) firstPath = e.composedPath();
};
document.addEventListener("click", noteFirstClick, true);
document.addEventListener("auxclick", noteFirstClick, true);

/**
 * A later click of a double click that began on another element. The
 * system counts clicks by place and time, not by element, so after a click
 * that took a row away the next one lands on whatever moved up under the
 * pointer. Pinned rows (close, minus, middle click), "Clear", folder rows
 * and their buttons, snoozed items, split cells and device tabs ignore such
 * a click; a real double click on them works as before. Closing a Today
 * tab does not, so quick clicks in one place close tab after tab.
 */
export function movedUnder(e: React.MouseEvent): boolean {
  return e.detail > 1 && !firstPath.includes(e.currentTarget);
}

/** Is `tab` an item its smart folder `folderId` got from its source (not a tab put there by hand)? */
function isSmartItem(state: UiState, tab: TabItem, folderId?: string): boolean {
  const smart = folderId ? findFolder(state, folderId)?.folder.smart : undefined;
  return !!smart?.items.includes(tab.home_url ?? tab.url);
}

/**
 * The snooze menu of a smart folder's item: the times to put it away until.
 * `onCustom` asks for a time of the user's own.
 */
function snoozeMenu(tab: TabItem, onCustom: () => void): MenuItem[] {
  const snooze = (until?: number) => send({ cmd: "smart_snooze", id: tab.id, until });
  return [
    ...snoozeChoices(new Date()).map((c) => ({ label: c.label, hint: c.hint, onClick: () => snooze(c.until) })),
    SEPARATOR,
    { label: t("sidebar.snooze.custom"), title: t("sidebar.snooze.custom_title"), onClick: onCustom },
    { label: t("sidebar.snooze.forever"), title: t("sidebar.snooze.forever_title"), onClick: () => snooze() },
  ];
}

/** How long the pointer rests on the snooze button before its menu opens by itself. */
const SNOOZE_HOVER_MS = 400;

/**
 * Right-click menu of a tab; `at` is the click point on the screen (the share
 * menu opens there). `smart`: an item its smart folder got from its source.
 */
export function tabMenu(ctx: RowContext, tab: TabItem, section: Section, at: { x: number; y: number }, smart = false): MenuItem[] {
  const items: MenuItem[] = [];
  const spaces = ctx.state.sidebar.spaces;
  const onScreen = ctx.state.panes.includes(tab.id);
  const inSplit = ctx.inSplit.has(tab.id);
  const loaded = !!ctx.state.runtime[tab.id]?.loaded;
  if (loaded) items.push({ label: t("sidebar.tab_menu.reload"), onClick: () => send({ cmd: "reload", id: tab.id }) });
  // The system's share menu: only macOS has one here.
  if (IS_MAC) items.push({ label: t("sidebar.tab_menu.share"), onClick: () => send({ cmd: "share", id: tab.id, x: at.x, y: at.y }) });
  items.push({ label: t("sidebar.tab_menu.copy_link"), onClick: () => navigator.clipboard?.writeText(tab.url) });
  items.push(SEPARATOR);
  // The private space has only Today tabs, and nothing of it is kept or moves out.
  const inPrivate = ctx.state.private_space === ctx.spaceId && section !== "favorites";
  if (section === "today" && !inPrivate) items.push({ label: t("sidebar.tab_menu.pin"), onClick: () => send({ cmd: "pin", id: tab.id }) });
  if (section === "pinned") {
    items.push({ label: t("sidebar.tab_menu.unpin"), onClick: () => send({ cmd: "unpin", id: tab.id }) });
    if (tab.home_url && tab.home_url !== tab.url) items.push({ label: t("sidebar.tab_menu.back_to_pinned"), title: tab.home_url, onClick: () => send({ cmd: "reset_tab", id: tab.id }) });
  }
  if (inPrivate) {
    // Nothing to keep.
  } else if (section !== "favorites") items.push({ label: t("sidebar.tab_menu.add_to_favorites"), onClick: () => send({ cmd: "move_tab", id: tab.id, space: ctx.spaceId, section: "favorites", index: 999 }) });
  else items.push({ label: t("sidebar.tab_menu.remove_from_favorites"), onClick: () => send({ cmd: "move_tab", id: tab.id, space: ctx.spaceId, section: "today", index: 0 }) });
  if (!inPrivate) items.push({ label: t("sidebar.tab_menu.add_to_bookmarks"), onClick: () => send({ cmd: "add_bookmark", id: tab.id }) });
  // Spared from unloading and auto-archive until it is let go again.
  // Favorites and pinned tabs stay loaded anyway.
  if (section === "today") items.push({ label: `${tab.keep_active ? "✓ " : ""}${t("sidebar.tab_menu.keep_active")}`, onClick: () => send({ cmd: "set_keep_active", id: tab.id, keep: !tab.keep_active }) });
  // Over the page in front instead of in its place (a translator, a password manager).
  if (section !== "today") items.push({ label: `${tab.panel ? "✓ " : ""}${t("sidebar.tab_menu.open_as_panel")}`, onClick: () => send({ cmd: "set_web_panel", id: tab.id, panel: !tab.panel }) });
  // Sites that have a page for phones serve the panel that one.
  if (section !== "today" && tab.panel) items.push({ label: `${tab.panel_mobile ? "✓ " : ""}${t("sidebar.tab_menu.mobile_view")}`, onClick: () => send({ cmd: "set_panel_mobile", id: tab.id, mobile: !tab.panel_mobile }) });
  if (ctx.state.active_tab && ctx.state.active_tab !== tab.id && !onScreen && !tab.panel && ctx.state.panes.length < MAX_PANES) {
    items.push({ label: t("sidebar.tab_menu.open_in_split"), onClick: () => send({ cmd: "add_split", id: tab.id }) });
  }
  if (inSplit) {
    items.push({ label: t("sidebar.tab_menu.remove_from_split"), onClick: () => send({ cmd: "remove_split", id: tab.id }) });
    items.push({ label: t("sidebar.tab_menu.separate_all"), onClick: () => send({ cmd: "separate_all", id: tab.id }) });
  }
  // The same address as a new Today tab (a private one in the private space),
  // as "Duplicate tab" in the command bar. A favorite has its own entry below.
  if (section !== "favorites") items.push({ label: t("sidebar.tab_menu.duplicate"), onClick: () => send({ cmd: "new_tab", url: tab.url }) });
  items.push({ label: t("common.rename"), onClick: () => ctx.setRenaming(tab.id) });
  if (tab.custom_title) items.push({ label: t("sidebar.tab_menu.use_page_title"), onClick: () => send({ cmd: "rename_tab", id: tab.id, title: "" }) });
  if (section === "favorites") {
    // Each profile has its own cookies and logins: two favorites of the same
    // site in two profiles are two accounts side by side.
    const { profiles, favorites } = ctx.state.sidebar;
    // A favorite runs in the profile of its list; choosing another moves it.
    const current = favorites.find((f) => f.items.some((t) => t.id === tab.id))?.profile_id;
    items.push(SEPARATOR);
    items.push({ label: t("sidebar.tab_menu.duplicate_favorite"), onClick: () => send({ cmd: "duplicate_favorite", id: tab.id }) });
    for (const p of profiles) {
      items.push({
        label: `${p.id === current ? "● " : "○ "}${t("sidebar.profile.item", { name: p.name })}`,
        onClick: () => p.id !== current && send({ cmd: "set_tab_profile", id: tab.id, profile: p.is_default ? null : p.id }),
      });
    }
    items.push({ label: t("sidebar.tab_menu.new_profile"), onClick: () => ctx.newProfileFor(tab.id) });
    // For every space, or for this one alone.
    const here = spaces.find((sp) => sp.id === ctx.spaceId);
    if (here && spaces.filter((sp) => sp.id !== ctx.state.private_space).length > 1) {
      const own = (tab.only_in ?? []).filter((id) => spaces.some((sp) => sp.id === id));
      items.push(SEPARATOR);
      items.push({ label: `${own.length === 0 ? "● " : "○ "}${t("sidebar.tab_menu.show_in_every_space")}`, onClick: () => own.length > 0 && send({ cmd: "set_favorite_spaces", id: tab.id, spaces: [] }) });
      items.push({ label: `${own.length > 0 ? "● " : "○ "}${t("sidebar.tab_menu.show_only_in", { space: here.title })}`, onClick: () => send({ cmd: "set_favorite_spaces", id: tab.id, spaces: [here.id] }) });
    }
  }
  const targets = inPrivate ? [] : spaces.filter((sp) => sp.id !== ctx.state.private_space);
  if (targets.length > 1) {
    items.push(SEPARATOR);
    for (const sp of targets) {
      if (sp.id === ctx.spaceId) continue;
      items.push({ label: t("sidebar.tab_menu.move_to", { space: sp.title }), onClick: () => send({ cmd: "move_tab", id: tab.id, space: sp.id, section: "today", index: 0 }) });
    }
  }
  const last: MenuItem[] = [];
  // Closing a favorite or a pinned tab unloads its page and takes it back to
  // its pinned page: without a page, only a favorite away from it has
  // something to close (a pinned tab has "Back to pinned page" above). A
  // smart folder's own item has snooze on its row, so Close is here for it.
  const awayFavorite = section === "favorites" && !!tab.home_url && tab.home_url !== tab.url;
  if (section === "today" || loaded || awayFavorite) last.push({ label: section === "today" && !inPrivate ? t("sidebar.tab_menu.archive") : t("common.close"), danger: section === "today", onClick: () => send({ cmd: "close_tab", id: tab.id }) });
  // Not a smart item: its source would bring it back at the next refresh.
  if (section === "pinned" && !smart) last.push({ label: t("sidebar.tab_menu.remove_from_pinned"), danger: true, onClick: () => send({ cmd: "remove_pinned", id: tab.id }) });
  if (last.length > 0) items.push(SEPARATOR, ...last);
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
    <Favorites key={list.profile_id} ctx={ctx} list={list} title={titled ? (profile(list.profile_id)?.name ?? t("sidebar.profile.fallback")) : undefined} />
  ));
}

/** What one tile of the favorites shows: a favorite, or the favorites of one split. */
type Tile = [TabItem, ...TabItem[]];

/** On tile `el` and not at its left or right edge: where a dropped tab joins it in a split. */
function overMiddle(e: React.DragEvent, el: HTMLElement): boolean {
  const r = el.getBoundingClientRect();
  const x = e.clientX - r.left;
  return x > 10 && x < r.width - 10;
}

/**
 * The page of tile `el` nearest to the pointer: one of a split's halves (also
 * from the gap between two of them), or the tile itself for a single favorite.
 */
function pageAt(e: React.DragEvent, el: HTMLElement): HTMLElement {
  const off = (page: HTMLElement) => {
    const r = page.getBoundingClientRect();
    return Math.max(r.left - e.clientX, e.clientX - r.right, 0);
  };
  return [...el.querySelectorAll<HTMLElement>("[data-page]")].reduce<HTMLElement | null>((best, page) => (best && off(best) <= off(page) ? best : page), null) ?? el;
}

/** `tabs` as the one tile they make; none without a tab. */
function asTile(tabs: TabItem[]): Tile[] {
  const [first, ...rest] = tabs;
  return first ? [[first, ...rest]] : [];
}

/**
 * The favorites of one profile: an icon per favorite. A tab dragged over the
 * list joins the tile it is on in a split, or goes between two tiles, where
 * a line marks its place. Dropped on the title (also while the list is
 * collapsed) it is added at the end.
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
  /** The tab or split being carried over this list (`id` stands for it) and where it would go: before tile `index` of the others. */
  const [place, setPlace] = useState<{ id: string; index: number } | null>(null);
  /** The tab `id` carried over a tile: dropped, it joins page `cell` of tile `tile` in a split, on `side`. */
  const [join, setJoin] = useState<{ id: string; tile: string; cell: string; side: Side } | null>(null);
  /** A tab this list takes: nothing of the private space is kept. */
  const takes = (d: ReturnType<typeof dragging>): d is NonNullable<typeof d> =>
    d?.kind === "tab" && (state.private_space !== ctx.spaceId || state.sidebar.favorites.some((f) => f.items.some((t) => t.id === d.id)));
  // The favorites of one split are one tile, where the first of them stands,
  // with its pages in reading order.
  const tiles = list.items.flatMap((t) => {
    const split = state.splits.find((s) => splitIds(s).includes(t.id));
    if (!split) return asTile([t]);
    const pages = splitIds(split).flatMap((id) => list.items.find((x) => x.id === id) ?? []);
    return list.items.find((x) => pages.includes(x)) === t ? asTile(pages) : [];
  });
  const without = (moved: string[]) => tiles.filter((tile) => !moved.includes(tile[0].id));
  // What a drag over this list shows. The tiles stay where they are: a line
  // between two of them marks where the carried one would go, and a tile it
  // would join previews the pair. A place left over from a drag that ended
  // elsewhere counts for nothing.
  const d = dragging();
  const moved = d ? movedIds(d) : [];
  const joining = join && d?.id === join.id ? join : null;
  const others = without(moved);
  const before = place && d?.id === place.id && !joining ? place.index : null;
  // Before the tile it stands before anyway: nothing to mark.
  const own = tiles.findIndex((tile) => moved.includes(tile[0].id));
  const mark = before === null || before === own ? null : others[before] ? { tile: others[before][0].id, side: "left" } : { tile: others.at(-1)?.[0].id, side: "right" };
  // A tile has no room for its name: it is edited in a row under the tiles.
  const renamed = list.items.find((t) => t.id === ctx.renaming);
  return (
    <div
      className="shrink-0 px-2 pt-3"
      onDragOver={(e) => {
        const d = dragging();
        if (!takes(d)) return;
        e.preventDefault();
        const rest = without(movedIds(d));
        const target = e.target as HTMLElement;
        const overTile = target.closest<HTMLElement>("[data-fav]");
        const k = rest.findIndex((tile) => tile[0].id === overTile?.dataset.fav);
        const onto = rest[k];
        // On a tile, the carried tab joins it: the two form a split, the tab
        // beside the page under the pointer, on the half of that page.
        // Between two tiles (their facing edges and the gap) it goes there.
        const joins = onto && splittable(d) && onto.length < MAX_PANES && !onto.some((t) => t.panel) && !findTab(state, d.id)?.panel;
        if (overTile && onto && joins && overMiddle(e, overTile)) {
          // Over the place the carried tab would join, the preview stays.
          if (target.closest("[data-ghost]")) return;
          const page = pageAt(e, overTile);
          const next = { id: d.id, tile: onto[0].id, cell: page.dataset.page ?? onto[0].id, side: sideOf(e, page) };
          return setJoin((j) => (j && j.id === next.id && j.cell === next.cell && j.side === next.side ? j : next));
        }
        setJoin(null);
        // Over the title: the end. Over the carried tile itself: where it
        // is. Over a gap: where the edge passed on the way there said.
        const own = tiles.findIndex((tile) => tile[0].id === overTile?.dataset.fav);
        const next = overTile && onto ? (sideOf(e, overTile) === "left" ? k : k + 1) : target.closest(".favs-title") ? rest.length : overTile && own >= 0 ? own : null;
        if (next !== null) setPlace((p) => (p?.id === d.id && p.index === next ? p : { id: d.id, index: next }));
      }}
      onDragLeave={(e) => { if (!e.currentTarget.contains(e.relatedTarget as Node)) { setPlace(null); setJoin(null); } }}
      onDrop={(e) => {
        e.preventDefault();
        const d = dragging();
        if (takes(d) && join?.id === d.id) {
          // Beside the page it joins, in this list, then into a split with it.
          const items = list.items.filter((t) => t.id !== d.id);
          const at = items.findIndex((t) => t.id === join.cell) + (join.side === "right" ? 1 : 0);
          send({ cmd: "move_tab", id: d.id, space: ctx.spaceId, section: "favorites", index: at, profile: list.profile_id });
          send({ cmd: "split_insert", target: join.cell, tab: d.id, side: join.side });
        } else if (takes(d)) {
          // The index counts the list without what moves, as the app takes it
          // out first: the place of the tile it lands before (where the first
          // of its pages stands in the list), else the end.
          const moved = movedIds(d);
          const items = list.items.filter((t) => !moved.includes(t.id));
          const before = place?.id === d.id ? without(moved)[place.index] : undefined;
          send({ cmd: "move_tab", id: d.id, space: ctx.spaceId, section: "favorites", index: before ? items.findIndex((t) => before.includes(t)) : items.length, profile: list.profile_id, split: d.pair });
        }
        setPlace(null);
        setJoin(null);
        endDrag();
      }}
    >
      {title && (
        <button className="favs-title no-drag" aria-expanded={open} onClick={toggle} title={open ? t("sidebar.favorites.collapse") : t("sidebar.favorites.expand")}>
          <ChevronDown width={12} height={12} className="chevron" />
          <span className="truncate">{title}</span>
          {!open && <span className="count">{list.items.length}</span>}
        </button>
      )}
      {(open || !title) && (
        <div className="favs">
          {tiles.map((tile) => {
            const page = (fav: TabItem) => {
              const rt = state.runtime[fav.id];
              return {
                "data-active": state.active_tab === fav.id || state.web_panel === fav.id || undefined,
                "data-unloaded": !rt?.loaded || undefined,
                "data-shared": rt?.shared || undefined,
                onClick: () => send(fav.panel ? { cmd: "toggle_web_panel", id: fav.id } : { cmd: "activate_tab", id: fav.id }),
                onContextMenu: (e: React.MouseEvent) => { e.preventDefault(); ctx.openMenu(e.clientX, e.clientY, tabMenu(ctx, fav, "favorites", { x: e.screenX, y: e.screenY })); },
                title: [fav.title, title, fav.panel && t("sidebar.favorites.panel"), rt?.shared && t("sidebar.favorites.shared")].filter(Boolean).join(" · "),
                children: <Favicon url={fav.url} src={rt?.favicon} loading={rt?.loading} />,
              };
            };
            const [lead] = tile;
            const drag = {
              "data-fav": lead.id,
              "data-carried": moved.includes(lead.id) || undefined,
              draggable: true,
              onDragEnd: () => { setPlace(null); setJoin(null); endDrag(); },
            };
            const joined = joining?.tile === lead.id ? joining : null;
            const carried = joined && findTab(state, joined.id);
            // Where the carried tile would go: a line in the gap beside this one.
            const line = mark?.tile === lead.id && <span className="fav-insert" data-side={mark.side} />;
            if (tile.length === 1 && !joined) {
              const { children, ...props } = page(lead);
              return <button key={lead.id} className="fav no-drag" data-panel={lead.panel || undefined} {...drag} onDragStart={(e) => startDrag(e, { kind: "tab", id: lead.id })} {...props}>{children}{line}</button>;
            }
            // A split: one tile with a half per page. It drags as one item.
            const ids = tile.map((x) => x.id);
            return (
              <div
                key={lead.id}
                className="fav fav-split no-drag"
                data-on-screen={ids.includes(state.active_tab ?? "") || undefined}
                data-wide={tile.length > 2 || undefined}
                {...drag}
                onDragStart={(e) => startDrag(e, tile.length === 1 ? { kind: "tab", id: lead.id } : { kind: "tab", id: lead.id, pair: true, group: ids })}
              >
                {tile.map((x) => {
                  const own = <button key={x.id} className="fav-page" data-page={x.id} {...page(x)} />;
                  if (joined?.cell !== x.id || !carried) return own;
                  const ghost = <span key="ghost" className="fav-page" data-ghost><Favicon url={carried.url} src={state.runtime[carried.id]?.favicon} /></span>;
                  return joined.side === "left" ? [ghost, own] : [own, ghost];
                })}
                {line}
              </div>
            );
          })}
        </div>
      )}
      {renamed && (
        <div className="row fav-rename">
          <Favicon url={renamed.url} src={state.runtime[renamed.id]?.favicon} />
          <RenameInput
            value={renamed.title}
            label={t("sidebar.favorites.rename_label", { name: renamed.title || hostOf(renamed.url) })}
            onCommit={(title, from) => {
              // Left as it was: no rename, so the title keeps following the page.
              if (title !== from) send({ cmd: "rename_tab", id: renamed.id, title });
              ctx.setRenaming(null);
            }}
            onCancel={() => ctx.setRenaming(null)}
          />
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
      active={ctx.state.active_tab === tab.id || ctx.state.web_panel === tab.id}
      renaming={ctx.renaming === tab.id}
      section={section}
      index={index}
      folderId={folderId}
      snoozable={isSmartItem(ctx.state, tab, folderId)}
      shareTarget={ctx.state.share_targets.includes(tab.id)}
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
  /** An item its smart folder got from its source: the row offers snooze in place of close. */
  snoozable: boolean;
  /** A page shares another tab: the row offers to share this one instead. */
  shareTarget: boolean;
  section: Section;
  index: number;
  folderId?: string;
  ctx: RowContext;
  onDropAt: (index: number) => void;
}

// Re-render only when this row's own data changes, or what its menu reads
// (spaces, the active tab, the pages on screen).
const TabRowInner = memo(
  function TabRowInner({ tab, rt, active, renaming, snoozable, shareTarget, section, index, folderId, ctx, onDropAt }: InnerProps) {
    // `left`/`right`: a tab hovers the middle and would open beside this one.
    const [over, setOver] = useState<"before" | "after" | Side | null>(null);
    // The user picks a time of their own to snooze until, in the title's place.
    const [pickingTime, setPickingTime] = useState(false);
    // Closed with the row's button or a middle click: it stays "close" until
    // the pointer has left, so a double click neither removes nor reopens
    // the tab.
    const [justClosed, setJustClosed] = useState(false);
    const hover = useRef<number | undefined>(undefined);
    // A row that goes away while the pointer rests on its button opens no menu.
    useEffect(() => () => window.clearTimeout(hover.current), []);
    const openSnooze = (button: HTMLElement) => {
      window.clearTimeout(hover.current);
      const r = button.getBoundingClientRect();
      ctx.openMenu(r.left, r.bottom + 4, snoozeMenu(tab, () => setPickingTime(true)));
    };
    const awayFromHome = section !== "today" && !!tab.home_url && tab.home_url !== tab.url;
    // A pinned tab without a page, at its pinned page, has nothing to close:
    // closing removes it (not a smart item, which only snoozes). One away
    // from that page goes back to it first.
    const removable = section === "pinned" && !rt?.loaded && !awayFromHome && !justClosed && !snoozable;
    // The row's close (its button, a middle click): unloads a page, archives
    // a Today tab, or removes a pinned tab without a page.
    const close = (e: React.MouseEvent) => {
      // Only a Today row goes away when closed, so only there is a quick
      // second click meant for the next tab.
      if (section !== "today" && movedUnder(e)) return;
      if (removable) {
        send({ cmd: "remove_pinned", id: tab.id });
        return;
      }
      // A key press (`detail` 0) leaves no pointer on the row to wait for.
      if (section === "pinned" && e.detail > 0) setJustClosed(true);
      send({ cmd: "close_tab", id: tab.id });
    };
    const joining = over === "left" || over === "right" ? findTab(ctx.state, dragging()?.id) : null;
    const preview = joining && [<SplitCell key={tab.id} tab={tab} rt={rt} />, <SplitCell key="ghost" tab={joining} rt={ctx.state.runtime[joining.id]} ghost />];

    return (
      <div
        className={"row drop"}
        data-active={active || undefined}
        data-dev={(rt?.dev?.local && ctx.state.settings.developer.enabled) || undefined}
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
          // A folder goes among pinned items, but not into itself.
          if (d.kind === "folder" && (section !== "pinned" || (folderId && !takesFolder(ctx.state, folderId, d.id)))) return;
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
            if (side && splittable(d)) {
              // Onto a pinned tab: pinned beside it, so the split is kept as one item.
              if (section === "pinned" && !snoozable) onDropAt(side === "left" ? index : index + 1);
              send({ cmd: "split_insert", target: tab.id, tab: d.id, side });
            } else onDropAt(dropIndex(e, index));
          }
          endDrag();
        }}
        onClick={(e) => {
          if (renaming) return;
          // A pinned tab that opens as a web panel: the click opens and closes it.
          if (tab.panel && section === "pinned") send({ cmd: "toggle_web_panel", id: tab.id });
          else if (e.altKey) send({ cmd: "add_split", id: tab.id });
          else send({ cmd: "activate_tab", id: tab.id });
        }}
        onDoubleClick={(e) => {
          // Two quick clicks on mute, close or back are not a rename.
          if (!(e.target instanceof Element && e.target.closest("button"))) ctx.setRenaming(tab.id);
        }}
        // Not from the rename or time field (a middle click pastes there on Linux).
        onAuxClick={(e) => e.button === 1 && !renaming && !pickingTime && close(e)}
        onMouseLeave={() => setJustClosed(false)}
        onContextMenu={(e) => {
          e.preventDefault();
          ctx.openMenu(e.clientX, e.clientY, tabMenu(ctx, tab, section, { x: e.screenX, y: e.screenY }, snoozable));
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
              title={t("sidebar.tab_row.back_to_home", { host: hostOf(tab.home_url!) })}
              aria-label={t("sidebar.tab_row.back_to_home_label", { host: hostOf(tab.home_url!) })}
              onClick={(e) => { e.stopPropagation(); send({ cmd: "reset_tab", id: tab.id }); }}
            >
              <Favicon url={tab.url} src={rt?.favicon} loading={rt?.loading} />
              <span className="home-arrow"><HomeArrow width={13} height={13} /></span>
            </button>
          ) : (
            <Favicon url={tab.url} src={rt?.favicon} loading={rt?.loading} />
          )}
          {pickingTime ? (
            <SnoozeUntilInput
              onCommit={(until) => {
                setPickingTime(false);
                send({ cmd: "smart_snooze", id: tab.id, until });
              }}
              onCancel={() => setPickingTime(false)}
            />
          ) : renaming ? (
            <RenameInput
              value={tab.title}
              onCommit={(title, from) => {
                // Left as it was: no rename, so the title keeps following the page.
                if (title !== from) send({ cmd: "rename_tab", id: tab.id, title });
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
          {((rt?.media?.playing && !rt.media.silent) || rt?.muted) && (
            <button className="slot" title={rt.muted ? t("sidebar.tab_row.unmute") : t("sidebar.tab_row.mute")} onClick={(e) => { e.stopPropagation(); send({ cmd: "set_muted", id: tab.id, muted: !rt.muted }); }}>
              {rt.muted ? <SpeakerOff width={14} height={14} /> : <Speaker width={14} height={14} />}
            </button>
          )}
          {rt?.shared && (
            <span className="shared" title={t("sidebar.tab_row.shared")} role="img" aria-label={t("sidebar.tab_row.shared_label")}>
              <ScreenShareIcon width={13} height={13} />
            </span>
          )}
          {!!rt?.dev?.errors && !active && (
            <span className="dev-errors" title={t("sidebar.tab_row.console_errors", { count: rt.dev.errors })} aria-label={t("sidebar.tab_row.console_errors_label", { count: rt.dev.errors })}>{errorCount(rt.dev.errors)}</span>
          )}
          {tab.panel && section === "pinned" && (
            <span className="panel-mark" data-open={active || undefined} title={t("sidebar.tab_row.web_panel")} aria-label={t("sidebar.tab_row.web_panel")} />
          )}
          {tab.keep_active && section === "today" && (
            <span className="kept" title={t("sidebar.tab_row.kept_active")} aria-label={t("sidebar.tab_row.kept_active_label")}>
              <KeepActiveIcon width={12} height={12} />
            </span>
          )}
          {shareTarget && (
            // Beside close, as Chromium's "Share this tab instead".
            <button
              className="close share-instead"
              title={t("sidebar.tab_row.share_instead")}
              aria-label={t("sidebar.tab_row.share_instead")}
              onClick={(e) => { e.stopPropagation(); send({ cmd: "share_tab_instead", id: tab.id }); }}
            >
              <ScreenShareIcon width={14} height={14} />
            </button>
          )}
          {snoozable ? (
            // Opens on a click, or by itself when the pointer rests on it.
            !pickingTime && (
              <button
                className="close"
                title={t("sidebar.snooze.button")}
                aria-label={t("sidebar.snooze.button")}
                aria-haspopup="menu"
                onClick={(e) => { e.stopPropagation(); openSnooze(e.currentTarget); }}
                onDoubleClick={(e) => e.stopPropagation()}
                onMouseEnter={(e) => {
                  const button = e.currentTarget;
                  hover.current = window.setTimeout(() => openSnooze(button), SNOOZE_HOVER_MS);
                }}
                onMouseLeave={() => window.clearTimeout(hover.current)}
              >
                <SnoozeIcon width={14} height={14} />
              </button>
            )
          ) : removable ? (
            // Its own key: a focused × must not turn into this button in place.
            <button
              key="remove"
              className="close"
              title={t("sidebar.tab_menu.remove_from_pinned")}
              aria-label={t("sidebar.tab_menu.remove_from_pinned")}
              onClick={(e) => { e.stopPropagation(); close(e); }}
            >
              <Minus width={14} height={14} />
            </button>
          ) : (
            <button
              key="close"
              className="close"
              onClick={(e) => { e.stopPropagation(); close(e); }}
              title={section === "today" ? t("sidebar.tab_row.archive") : t("common.close")}
            >
              <Close width={14} height={14} />
            </button>
          )}
        </>)}
      </div>
    );
  },
  (a, b) =>
    a.tab === b.tab && a.rt === b.rt && a.active === b.active && a.renaming === b.renaming && a.snoozable === b.snoozable && a.shareTarget === b.shareTarget &&
    a.section === b.section && a.index === b.index && a.folderId === b.folderId &&
    a.ctx.spaceId === b.ctx.spaceId && a.ctx.state.sidebar.spaces === b.ctx.state.sidebar.spaces &&
    a.ctx.state.active_tab === b.ctx.state.active_tab && a.ctx.state.panes === b.ctx.state.panes,
);

/**
 * Title editor in place of a title: Return or leaving the field saves, Esc
 * cancels (and goes no further: it would also hide the floating sidebar).
 * `onCommit` also gets the title the edit started from (`value` may have
 * followed the page since). `label` names the field where no title stood.
 */
export function RenameInput({ value, label, onCommit, onCancel }: { value: string; label?: string; onCommit: (title: string, from: string) => void; onCancel: () => void }) {
  const [from] = useState(value);
  return (
    <input
      autoFocus
      className="rename"
      aria-label={label}
      defaultValue={from}
      onBlur={(e) => onCommit(e.currentTarget.value, from)}
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

/**
 * A date and time to snooze until, in place of a title: Return snoozes (a
 * time that is not ahead is refused), Esc or leaving the field cancels.
 * Starts at tomorrow, 9:00.
 */
function SnoozeUntilInput({ onCommit, onCancel }: { onCommit: (until: number) => void; onCancel: () => void }) {
  const now = new Date();
  const start = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 9);
  return (
    <input
      autoFocus
      type="datetime-local"
      className="rename"
      aria-label={t("sidebar.snooze.until_label")}
      defaultValue={localInputValue(start)}
      min={localInputValue(now)}
      onClick={(e) => e.stopPropagation()}
      onDoubleClick={(e) => e.stopPropagation()}
      onBlur={onCancel}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          const at = new Date(e.currentTarget.value).getTime();
          if (at > Date.now()) onCommit(Math.floor(at / 1000));
        }
        if (e.key === "Escape") {
          e.stopPropagation();
          onCancel();
        }
      }}
    />
  );
}

/** A snoozed item of smart folder `folderId`, shown while the folder shows them: when it wakes, and a button to wake it now. */
function SnoozedRow({ folderId, item }: { folderId: string; item: Snoozed }) {
  const until = wakeLabel(item.until);
  return (
    <div className="row snoozed" title={`${item.url}\n${item.until === undefined ? t("sidebar.snooze.snoozed") : t("sidebar.snooze.snoozed_until", { time: new Date(item.until * 1000).toLocaleString(language) })}`}>
      <Favicon url={item.url} />
      <span className="title">{item.title || hostOf(item.url)}</span>
      <span className="meta">{until}</span>
      <button className="close" title={t("sidebar.snooze.wake_now")} aria-label={t("sidebar.snooze.wake_now")} onClick={(e) => !movedUnder(e) && send({ cmd: "smart_wake", id: folderId, url: item.url })}>
        <WakeIcon width={14} height={14} />
      </button>
    </div>
  );
}

/**
 * May folder `dragged` go into folder `target`? Not into itself or a folder
 * inside it, and not into a smart folder, which lists what its service has.
 */
function takesFolder(state: UiState, target: string, dragged: string): boolean {
  if (target === dragged) return false;
  const holds = (nodes: SidebarNode[]): boolean => nodes.some((n) => n.kind === "folder" && (n.id === target || holds(n.children)));
  const moved = findFolder(state, dragged)?.folder;
  return !findFolder(state, target)?.folder.smart && !(moved && holds(moved.children));
}

/** A folder's row with what it holds below; `parentId`: the folder it is in. */
export function FolderRow({ ctx, folder, index, depth, parentId, onDropAt }: { ctx: RowContext; folder: Folder; index: number; depth: number; parentId?: string; onDropAt: (index: number) => void }) {
  const [open, setOpen] = useState(() => localStorage.getItem("folder:" + folder.id) !== "closed");
  // The app skips the tabs of closed folders on Ctrl+Tab.
  useEffect(() => {
    send({ cmd: "set_folder_open", id: folder.id, open });
  }, [folder.id, open]);
  // Where a dragged tab or folder would go: into this folder, or beside it.
  const [over, setOver] = useState<"before" | "inside" | "after" | null>(null);
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
  const snoozed = smart?.snoozed ?? [];
  // Whether the folder lists its snoozed items below the others.
  const [showSnoozed, setShowSnoozed] = useState(() => localStorage.getItem("snoozed:" + folder.id) === "shown");
  const toggleSnoozed = () => {
    setShowSnoozed((shown) => {
      localStorage.setItem("snoozed:" + folder.id, shown ? "hidden" : "shown");
      return !shown;
    });
  };
  const status = smart ? ctx.state.smart[folder.id] : undefined;
  const unread = smart && !open && folder.children.some((n) => n.kind === "tab" && n.unread);
  const settings = () => send({ cmd: "smart_folder_dialog", id: folder.id });
  const glyph = glyphFor(folder.icon);
  const changeIcon = (x: number, y: number) =>
    ctx.pickIcon({ x, y }, glyph ?? undefined, (g) => send({ cmd: "set_folder_icon", id: folder.id, icon: g }), true);
  const share: MenuItem = { label: t("sidebar.folder_menu.copy_share_link"), onClick: () => send({ cmd: "share_folder", id: folder.id }) };
  const menu: MenuItem[] = smart
    ? [
        ...(status?.needs_approval
          ? [{ label: t("sidebar.folder_menu.turn_on"), onClick: () => send({ cmd: "smart_allow", id: folder.id }) }, SEPARATOR]
          : []),
        { label: t("sidebar.folder_menu.refresh"), onClick: () => send({ cmd: "smart_refresh", id: folder.id }) },
        { label: t("sidebar.folder_menu.smart_settings"), onClick: settings },
        SEPARATOR,
        { label: t("sidebar.folder_menu.rename"), onClick: () => ctx.setRenaming(folder.id) },
        { label: t("sidebar.folder_menu.change_icon"), onClick: () => changeIcon(menuAt.current.x, menuAt.current.y) },
        { label: open ? t("sidebar.folder_menu.collapse") : t("sidebar.folder_menu.expand"), onClick: toggle },
        share,
        SEPARATOR,
        { label: t("sidebar.folder_menu.stop_updating"), onClick: () => send({ cmd: "smart_unlink", id: folder.id }) },
        { label: t("sidebar.folder_menu.delete_with_items"), danger: true, onClick: () => send({ cmd: "delete_folder", id: folder.id }) },
      ]
    : [
        { label: t("sidebar.folder_menu.rename"), onClick: () => ctx.setRenaming(folder.id) },
        { label: t("sidebar.folder_menu.change_icon"), onClick: () => changeIcon(menuAt.current.x, menuAt.current.y) },
        { label: open ? t("sidebar.folder_menu.collapse") : t("sidebar.folder_menu.expand"), onClick: toggle },
        share,
        {
          label: t("sidebar.folder_menu.new_folder_inside"),
          onClick: () => {
            send({ cmd: "new_folder", title: "", parent: folder.id });
            if (!open) toggle();
          },
        },
        SEPARATOR,
        { label: t("sidebar.folder_menu.make_smart"), onClick: settings },
        SEPARATOR,
        { label: t("sidebar.folder_menu.delete_keep_tabs"), onClick: () => send({ cmd: "delete_folder", id: folder.id }) },
      ];
  const title = smart && status
    ? status.error ?? (status.updated_at ? t("sidebar.folder_row.smart_updated", { ago: ago(status.updated_at) }) : t("sidebar.folder_row.smart"))
    : undefined;
  return (
    <div>
      <div
        className="row drop"
        data-active={over === "inside" || undefined}
        data-before={over === "before" || undefined}
        data-after={over === "after" || undefined}
        title={title}
        draggable={!renaming}
        onDragStart={(e) => startDrag(e, { kind: "folder", id: folder.id })}
        onDragEnd={endDrag}
        onDragOver={(e) => {
          const d = dragging();
          if (!d || d.id === folder.id) return;
          if (d.kind === "tab") {
            e.preventDefault();
            return setOver("inside");
          }
          // A folder: the edges place it beside this one, the middle inside.
          // Below an open folder's row its first item follows, so the lower
          // edge means inside as well.
          const inside = takesFolder(ctx.state, folder.id, d.id);
          let zone = rowZone(e);
          if (zone === "after" && open && folder.children.length > 0) zone = "inside";
          if (zone === "inside" && !inside) return setOver(null);
          // Beside this folder is inside its parent, which may be the dragged one.
          if (zone !== "inside" && parentId && !takesFolder(ctx.state, parentId, d.id)) return setOver(null);
          e.preventDefault();
          setOver(zone);
        }}
        onDragLeave={() => setOver(null)}
        onDrop={(e) => {
          e.preventDefault();
          const d = dragging();
          const zone = over;
          setOver(null);
          if (!d || !zone) return;
          if (d.kind === "tab") send({ cmd: "move_tab", id: d.id, space: ctx.spaceId, section: "pinned", index: 0, folder: folder.id, split: d.pair });
          else if (zone === "inside") send({ cmd: "move_folder", id: d.id, index: 0, folder: folder.id });
          else onDropAt(zone === "before" ? index : index + 1);
          endDrag();
        }}
        onClick={(e) => {
          if (!renaming && !movedUnder(e)) toggle();
        }}
        onDoubleClick={(e) => !movedUnder(e) && ctx.setRenaming(folder.id)}
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
          <span className="title" style={{ fontWeight: 600 }}>{folder.title || t("common.untitled")}</span>
        )}
        {status?.fetching && <span className="spinner" title={t("sidebar.folder_row.refreshing")} />}
        {smart && status?.error && !status.fetching && (
          <button className="slot" title={t("sidebar.folder_row.error", { error: status.error })} aria-label={t("sidebar.folder_row.error_label")} onClick={(e) => { e.stopPropagation(); if (!movedUnder(e)) settings(); }}>
            <WarningIcon width={14} height={14} />
          </button>
        )}
        {unread && <span className="unread-dot" aria-label={t("sidebar.folder_row.new_items")} />}
        {snoozed.length > 0 && (
          <button
            className="slot"
            data-on={showSnoozed || undefined}
            aria-pressed={showSnoozed}
            title={showSnoozed ? t("sidebar.snooze.hide_snoozed", { count: snoozed.length }) : t("sidebar.snooze.show_snoozed", { count: snoozed.length })}
            onClick={(e) => {
              e.stopPropagation();
              if (movedUnder(e)) return;
              toggleSnoozed();
              // Showing them in a collapsed folder opens it.
              if (!showSnoozed && !open) toggle();
            }}
            onDoubleClick={(e) => e.stopPropagation()}
          >
            <SnoozeIcon width={14} height={14} />
          </button>
        )}
        {!open && count > 0 && <span className="meta">{count}</span>}
      </div>
      {open && (
        <div style={{ paddingLeft: depth < 2 ? 16 : 8 }}>
          <NodeList ctx={ctx} nodes={folder.children} depth={depth + 1} folderId={folder.id} />
          {showSnoozed && snoozed.map((s) => <SnoozedRow key={s.url} folderId={folder.id} item={s} />)}
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
    const at = indexAfterTaking(nodes, movedIds(d), index);
    if (d.kind === "tab") send({ cmd: "move_tab", id: d.id, space: ctx.spaceId, section: "pinned", index: at, folder: folderId, split: d.pair });
    else send({ cmd: "move_folder", id: d.id, index: at, folder: folderId });
  };
  return (
    <>
      {nodes.map((n, i) =>
        n.kind === "tab" ? (
          <TabRow key={n.id} ctx={ctx} tab={n} section="pinned" index={i} folderId={folderId} onDropAt={dropTab} />
        ) : (
          <FolderRow key={n.id} ctx={ctx} folder={n} index={i} depth={depth} parentId={folderId} onDropAt={dropTab} />
        ),
      )}
    </>
  );
}

/**
 * A split as one row: its pages side by side in reading order. While it is on
 * screen the row is lit and so is the page in focus. Drop a tab on a page to
 * add it on the half under the pointer; × or middle-click takes a page out.
 * A page dragged onto another page of its row takes that place in the order;
 * the layout on screen stays.
 * `owner`, `index`, `onDropAt`: the list slot the row stands in, so the row
 * can be dragged and dropped around like a tab.
 */
export function SplitRow({ ctx, split, owner, index, onDropAt }: { ctx: RowContext; split: SplitNode; owner?: string; index?: number; onDropAt?: (index: number) => void }) {
  const ids = splitIds(split);
  const onScreen = ids.includes(ctx.state.active_tab ?? "") && ctx.state.panes.length > 1;
  const [over, setOver] = useState<"before" | "after" | { cell: string; side: Side } | null>(null);
  const join = over && typeof over === "object" ? over : null;
  /** The row's own page being dragged: it leaves its place while it is carried. */
  const [lifted, setLifted] = useState<string | null>(null);
  /** The tab a drag would place in this row: an outside tab, or one of the row's own pages. */
  const placing = (d: ReturnType<typeof dragging>) => (d?.pair ? (d.id === owner ? d.cell : undefined) : d?.id);
  const joining = join ? findTab(ctx.state, placing(dragging())) : null;
  const ghost = joining && <SplitCell key="ghost" tab={joining} rt={ctx.state.runtime[joining.id]} ghost />;
  const reorder = (e: React.DragEvent) => index !== undefined && onDropAt && setOver(dropIndex(e, index) === index ? "before" : "after");
  // Pinning the split or adding it to the favorites takes the pages that
  // stand with the row; nothing of the private space is kept.
  const today = !!owner && ctx.state.sidebar.spaces.some((sp) => sp.id === ctx.spaceId && sp.today.some((t) => t.id === owner));
  const keep: MenuItem[] = !owner || ctx.state.private_space === ctx.spaceId ? [] : [
    today ? { label: t("sidebar.split_menu.pin"), onClick: () => send({ cmd: "pin", id: owner }) } : { label: t("sidebar.split_menu.unpin"), onClick: () => send({ cmd: "unpin", id: owner }) },
    { label: t("sidebar.split_menu.add_to_favorites"), onClick: () => send({ cmd: "move_tab", id: owner, space: ctx.spaceId, section: "favorites", index: 999, split: true }) },
    SEPARATOR,
  ];
  const menu = (e: React.MouseEvent, id: string) => {
    e.preventDefault();
    ctx.openMenu(e.clientX, e.clientY, [
      ...keep,
      { label: t("sidebar.tab_menu.remove_from_split"), onClick: () => send({ cmd: "remove_split", id }) },
      { label: t("sidebar.tab_menu.separate_all"), onClick: () => send({ cmd: "separate_all", id }) },
      ...(onScreen ? [
        { label: t("sidebar.split_menu.even_out"), onClick: () => send({ cmd: "split_equalize", path: [] }) },
        { label: t("sidebar.split_menu.grid"), onClick: () => send({ cmd: "split_grid" }) },
      ] : []),
    ]);
  };
  return (
    <div
      className="split-row drop"
      data-on-screen={onScreen || undefined}
      data-before={over === "before" || undefined}
      data-after={over === "after" || undefined}
      draggable={!!owner}
      onDragStart={(e) => {
        if (!owner) return;
        // The row is what drags, but the picture under the pointer is the
        // one page the drag started on: that is the page being placed.
        const cell = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-cell]");
        if (cell) {
          const r = cell.getBoundingClientRect();
          e.dataTransfer.setDragImage(cell, e.clientX - r.left, e.clientY - r.top);
        }
        const id = cell?.dataset.cell;
        startDrag(e, { kind: "tab", id: owner, pair: true, cell: id, group: splitGroup(ctx.state, split, owner) });
        // A frame later, so the picture under the pointer is taken first.
        if (id) requestAnimationFrame(() => setLifted(id));
      }}
      onDragEnd={() => { setLifted(null); setOver(null); endDrag(); }}
      onDragOver={(e) => {
        const d = dragging();
        if (!d) return;
        if (d.pair && d.id === owner) {
          // One of the row's own pages: over another page it takes that place.
          // The row takes the drop anywhere, also on the placeholder or the
          // page's old place: a refused drop makes the picture fly back first.
          if (!d.cell) return;
          e.preventDefault();
          const cell = (e.target as HTMLElement).closest<HTMLElement>("[data-cell]");
          if (cell && cell.dataset.cell !== d.cell) setOver({ cell: cell.dataset.cell!, side: sideOf(e, cell) });
          return;
        }
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
        setLifted(null);
        const tab = placing(d);
        if (join && tab) {
          // Onto a pinned split: pinned with it.
          if (!d?.pair && owner && !today && index !== undefined && onDropAt) onDropAt(index + 1);
          send({ cmd: d?.pair ? "split_reorder" : "split_insert", target: join.cell, tab, side: join.side });
        }
        else if (d && !(d.pair && d.id === owner) && index !== undefined && onDropAt) onDropAt(dropIndex(e, index));
        endDrag();
      }}
    >
      {ids.map((id) => {
        const tab = findTab(ctx.state, id);
        // The carried page shows at its new place only, once there is one.
        if (!tab || (id === lifted && join)) return null;
        const cell = (
          <SplitCell
            key={id}
            lifted={id === lifted}
            tab={tab}
            rt={ctx.state.runtime[id]}
            active={onScreen && id === ctx.state.active_tab}
            onClick={(e) => !movedUnder(e) && send({ cmd: "activate_tab", id })}
            onAuxClick={(e) => e.button === 1 && !movedUnder(e) && send({ cmd: "remove_split", id })}
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
function SplitCell({ tab, rt, active, ghost, lifted, onRemove, ...events }: {
  tab: TabItem;
  rt: TabRuntime | undefined;
  active?: boolean;
  ghost?: boolean;
  /** Being dragged: its place stays, empty. */
  lifted?: boolean;
  onRemove?: () => void;
  onClick?: (e: React.MouseEvent) => void;
  onAuxClick?: (e: React.MouseEvent) => void;
  onContextMenu?: (e: React.MouseEvent) => void;
}) {
  return (
    <div className="split-cell" data-cell={ghost ? undefined : tab.id} data-ghost={ghost || undefined} data-lifted={lifted || undefined} data-active={active || undefined} data-unloaded={!ghost && !rt?.loaded ? true : undefined} title={tab.url} {...events}>
      <Favicon url={tab.url} src={rt?.favicon} loading={rt?.loading} />
      <span className="title">{tab.title || hostOf(tab.url)}</span>
      {!ghost && rt?.shared && (
        <span className="shared" title={t("sidebar.tab_row.shared")} role="img" aria-label={t("sidebar.tab_row.shared_label")}>
          <ScreenShareIcon width={13} height={13} />
        </span>
      )}
      {onRemove && (
        <button className="close" title={t("sidebar.tab_menu.remove_from_split")} aria-label={t("sidebar.tab_menu.remove_from_split")} onClick={(e) => { e.stopPropagation(); if (!movedUnder(e)) onRemove(); }}>
          <Close width={12} height={12} />
        </button>
      )}
    </div>
  );
}

/** A tab open on another device of the sync chain, marked by the device's
 *  icon on the right. A click takes it over: it opens here, and its device
 *  closes it at its next sync. */
export function DeviceTabRow({ tab }: { tab: DeviceTab }) {
  const hint = t("sidebar.device_tab.hint", { device: tab.device });
  return (
    <button className="row device-tab w-full" title={hint} onClick={(e) => !movedUnder(e) && send({ cmd: "take_device_tab", id: tab.id })}>
      <Favicon url={tab.url} />
      <span className="title text-left">{tab.title || hostOf(tab.url)}</span>
      <span className="from-device" aria-label={t("sidebar.device_tab.label", { device: tab.device })}>
        <PhoneIcon width={13} height={13} />
      </span>
    </button>
  );
}
