import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { ArchiveView } from "./ArchiveView";
import { BookmarksView } from "./BookmarksView";
import { DownloadsView } from "./DownloadsView";
import { HistoryView } from "./HistoryView";
import { LibraryButton } from "./LibraryButton";
import { SidebarResizer } from "./SidebarResizer";
import { ContextMenu, SEPARATOR, type MenuItem } from "./ContextMenu";
import { Back, ChevronDown, Close, Dots, FolderIcon, Forward, Plus, MaskIcon, PrivateIcon, Reload, SidebarIcon, SmartFolderIcon, SpacePlus, SplitIcon, TabPlus } from "./Icons";
import { AddressRow } from "./AddressRow";
import { CaptureToast } from "./CaptureToast";
import { record } from "./recorder";
import { MediaBar, type MediaEntry } from "./MediaBar";
import { NewMenu } from "./NewMenu";
import { ExtensionBar, PinnedExtensions } from "./PinnedExtensions";
import { DeviceTabRow, FavoriteLists, NodeList, RenameInput, SplitRow, TabRow, countTabs, dropAmongPinned, movedUnder, profileColor, type RowContext } from "./Rows";
import { windowBackground } from "./backgrounds";
import { IS_LINUX, IS_MAC, accentCss, applyAppearance, dockedSide, favoriteShows, keyLabel, lookOf, findTab, findTabInSpace, hostOf, onNotice, onOpenArchive, onOpenBookmarks, onOpenDownloads, onOpenHistory, onRecord, send, splitIds, subscribe, type LibraryView, type SidebarNode, type Space, type SplitNode, type TabItem, type UiState } from "./bridge";
import { IconPicker, SpaceIcon } from "./Glyphs";
import { SpaceSwitcher } from "./SpaceSwitcher";
import { dragging, endDrag, indexAfterTaking, movedIds } from "./dnd";
import { useSpaceSwipe } from "./useSpaceSwipe";
import { keysHint, keysLabel } from "./settings/shortcutKeys";
import { useSidebarShortcuts } from "./useSidebarShortcuts";
import { t } from "./i18n";
import { reportMediaDevices } from "./mediaDevices";

/**
 * `magnetite://ui/?float`: this page is the sidebar floating over the pages while
 * the docked one is hidden. It leaves when the pointer does (or on Esc).
 */
const FLOAT = new URLSearchParams(location.search).has("float");

/** Pinned tab `id` among `nodes`, with its place: the list it is in (`among`, of folder `folderId`) and where. */
function pinnedTab(nodes: SidebarNode[], id: string, folderId?: string): { tab: TabItem; among: SidebarNode[]; index: number; folderId?: string } | null {
  for (const [index, n] of nodes.entries()) {
    const found = n.kind === "tab" ? (n.id === id ? { tab: n, among: nodes, index, folderId } : null) : pinnedTab(n.children, id, n.id);
    if (found) return found;
  }
  return null;
}

export function App() {
  const [state, setState] = useState<UiState | null>(null);
  const [newMenu, setNewMenu] = useState(false);
  const closeNewMenu = useCallback(() => setNewMenu(false), []);
  const newProfileFor = useCallback((tabId: string) => send({ cmd: "open_palette", mode: "new_profile", tab: tabId }), []);
  // Direction the space list slides in from after a space change.
  const enter = useRef({ id: "", index: 0, x: 0 });
  const swipe = useSpaceSwipe(state?.active_space ?? "", useCallback((direction: 1 | -1) => send({ cmd: direction > 0 ? "next_space" : "prev_space" }), []));
  // A full-height view in place of the sidebar.
  const [panel, setPanel] = useState<LibraryView | null>(null);
  const closePanel = useCallback(() => setPanel(null), []);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  // The space whose title is being edited (it shows once that space is active).
  const [renamingSpace, setRenamingSpace] = useState<string | null>(null);
  // The glyph picker, for the space icon or a folder's (`onPick(null)`: default icon).
  const [iconPicker, setIconPicker] = useState<{ x: number; y: number; current?: string; onPick: (glyph: string | null) => void; allowDefault?: boolean } | null>(null);
  // The spaces whose pinned tabs are hidden are kept by the page (`pinned:<space>`), as closed folders are; this redraws after a change.
  const [, redrawPinned] = useState(0);
  const pickIcon = useCallback<RowContext["pickIcon"]>((at, current, onPick, allowDefault) => setIconPicker({ ...at, current, onPick, allowDefault }), []);

  useEffect(() => subscribe(setState), []);
  const shownAt = useFloatLifecycle();
  useEffect(() => onOpenArchive(() => setPanel("archive")), []);
  useEffect(() => onOpenDownloads(() => setPanel("downloads")), []);
  useEffect(() => onOpenBookmarks(() => setPanel("bookmarks")), []);
  useEffect(() => onOpenHistory(() => setPanel("history")), []);
  useEffect(() => onRecord(() => void record()), []);
  const [notice, setNotice] = useState<string | null>(null);
  useEffect(() => {
    // Three seconds from the last notice: the same text sent again stays its full time.
    let timer = 0;
    return onNotice((text) => {
      setNotice(text);
      window.clearTimeout(timer);
      timer = window.setTimeout(() => setNotice(null), 3000);
    });
  }, []);
  useEffect(() => {
    // The floating sidebar is a second view of the same state: no start-up.
    if (FLOAT) send({ cmd: "get_state" }).then((s) => window.__magnetite?.setState(s));
    else send({ cmd: "ready" });
  }, []);
  // For the order of cameras and microphones in Settings; the floating sidebar is the same window's.
  useEffect(() => (FLOAT ? undefined : reportMediaDevices()), []);
  useSidebarShortcuts(FLOAT, closePanel);
  // Text size and density of this page, set before it paints.
  const textScale = state?.settings.appearance.text_scale;
  const density = state?.settings.appearance.density;
  useLayoutEffect(() => {
    if (textScale !== undefined && density !== undefined) applyAppearance({ text_scale: textScale, density });
  }, [textScale, density]);

  const openMenu = useCallback((x: number, y: number, items: MenuItem[]) => setMenu({ x, y, items }), []);

  if (!state) return <div className="h-full" />;
  const space = state.sidebar.spaces.find((s) => s.id === state.active_space) ?? state.sidebar.spaces[0]!;
  // The private space: only Today tabs, each in a profile of its own, and nothing of it is kept.
  const isPrivate = state.private_space === space.id;
  // Other devices' tabs show in the space they are open in there; one of a
  // space this device does not have shows in whichever space is in front.
  const deviceTabs = isPrivate ? [] : state.device_tabs.filter((t) =>
    t.space === space.id || !state.sidebar.spaces.some((s) => s.id === t.space));
  // In every space but the private one (a favorite runs in its own profile,
  // which is kept): one list of favorites per profile that has some for this
  // space. A favorite is for every space unless it names its spaces.
  const favoriteLists = isPrivate ? [] : state.sidebar.favorites
    .map((f) => ({ ...f, items: f.items.filter((t) => favoriteShows(state.sidebar, t, space.id)) }))
    .filter((f) => f.items.length > 0);
  const spaceIndex = state.sidebar.spaces.indexOf(space);
  if (enter.current.id !== space.id) {
    // From the side a swipe went to (also from the last space to the first), else by position.
    const step = swipe.direction ?? Math.sign(spaceIndex - enter.current.index);
    enter.current = { id: space.id, index: spaceIndex, x: enter.current.id ? step * 24 : 0 };
  }
  // Each split shows as one row, in place of the first of its pages that has a row in this space.
  const splitAt = new Map<string, SplitNode>();
  const inSplit = new Set<string>();
  for (const split of state.splits) {
    const ids = splitIds(split);
    for (const id of ids) inSplit.add(id);
    const owner = ids.find((id) => findTabInSpace(space, id) !== null);
    if (owner) splitAt.set(owner, split);
  }
  // The split on screen with no row here (pages of other spaces) shows above
  // Today. One of favorites only is a tile in their list.
  const isFavorite = (id: string) => favoriteLists.some((f) => f.items.some((t) => t.id === id));
  const looseSplit = state.split && !splitIds(state.split).some((id) => splitAt.has(id)) && !splitIds(state.split).every(isFavorite) ? state.split : null;
  const ctx: RowContext = { state, spaceId: space.id, openMenu, renaming, setRenaming, newProfileFor, splitAt, inSplit, pickIcon };
  // The space's profile shows beside its name once there is more than one to tell apart.
  const spaceProfile = state.sidebar.profiles.filter((p) => !p.ephemeral).length > 1 ? state.sidebar.profiles.find((p) => p.id === space.profile_id) : undefined;
  // The pinned tabs can be hidden, space by space. The tab in front stays: its row (a split's: the row of the split) is the one left, at its own place for what is dropped on it.
  const pinnedCount = isPrivate ? 0 : countTabs(space.pinned);
  const pinnedKey = "pinned:" + space.id;
  const pinnedOpen = pinnedCount === 0 || localStorage.getItem(pinnedKey) !== "closed";
  const togglePinned = () => {
    localStorage.setItem(pinnedKey, pinnedOpen ? "closed" : "open");
    redrawPinned((n) => n + 1);
  };
  const frontPinned = pinnedOpen ? null : [state.active_tab, state.web_panel]
    .flatMap((id) => (id ? [[...splitAt].find(([, split]) => splitIds(split).includes(id))?.[0] ?? id] : []))
    .reduce<ReturnType<typeof pinnedTab>>((found, id) => found ?? pinnedTab(space.pinned, id), null);
  const pickSpaceIcon = (x: number, y: number) =>
    pickIcon({ x, y }, space.icon, (g) => g && send({ cmd: "rename_space", id: space.id, icon: g }));
  const activeRt = state.active_tab ? state.runtime[state.active_tab] : undefined;
  // Back, forward and reload act on the page in front: the peek while one is open.
  const frontRt = state.peek ? state.peek.runtime : activeRt;
  const activeItem = state.active_tab ? findTab(state, state.active_tab) : null;
  const topBar = state.settings.appearance.top_bar && !FLOAT;
  const extensionBar = state.settings.appearance.extension_bar;
  const side = dockedSide(state.settings.appearance);
  const spaceVars = { ["--space" as string]: accentCss(space.accent), ["--space-soft" as string]: accentCss(space.accent, 0.14), ["--bg" as string]: state.chrome, ...windowBackground("sidebar", state.chrome, space.id, lookOf(state.settings.appearance, space.id), side, space.accent) };
  // Media in tabs that are not on screen, playing ones first (at most two rows).
  const onScreen = new Set(state.panes.length > 1 ? state.panes : state.active_tab ? [state.active_tab] : []);
  const media: MediaEntry[] = Object.entries(state.runtime)
    .flatMap(([id, rt]) => {
      const tab = !onScreen.has(id) && rt.media ? findTab(state, id) : null;
      return tab && rt.media ? [{ tab, rt, media: rt.media }] : [];
    })
    .sort((a, b) => Number(b.media.playing) - Number(a.media.playing))
    .slice(0, 2);

  // Shown over the library views too: "That file was moved or deleted" answers a click in Downloads.
  const noticeBox = notice && <div className="notice" role="status">{notice}</div>;
  // The pages cover the window's own buttons while the sidebar is hidden; the top bar keeps them.
  const windowButtons = FLOAT && IS_MAC && !state.settings.appearance.top_bar && <WindowButtons />;
  // A library view leaves its top band empty: the window buttons lie over it, where the sidebar's own band has them.
  const library = (view: ReactNode) => (
    <div className="relative h-full" style={spaceVars}>
      {view}
      {windowButtons && <div className="absolute left-2 top-[15px]">{windowButtons}</div>}
      {noticeBox}
    </div>
  );
  if (panel === "archive") return library(<ArchiveView count={state.archive_count} onClose={closePanel} />);
  if (panel === "downloads") return library(<DownloadsView onClose={closePanel} />);
  if (panel === "history") return library(<HistoryView count={state.history_count} onClose={closePanel} />);
  if (panel === "bookmarks") return library(<BookmarksView state={state} onClose={closePanel} />);

  const dropToday = (index: number) => {
    const d = dragging();
    if (d?.kind === "tab") send({ cmd: "move_tab", id: d.id, space: space.id, section: "today", index: indexAfterTaking(space.today, movedIds(d), index), split: d.pair });
  };

  // Options for `target` (the active space unless right-clicked in the footer).
  // New folders and new profiles act on the active space, so those switch first.
  const spaceMenu = (x: number, y: number, target: Space = space) => {
    const there = (run: () => void) => () => {
      if (target.id !== space.id) send({ cmd: "switch_space", id: target.id });
      run();
    };
    if (target.id === state.private_space) {
      return openMenu(x, y, [
        { label: t("sidebar.space.hide_private"), onClick: () => send({ cmd: "hide_private_space" }) },
        { label: t("sidebar.space.close_private"), danger: true, onClick: () => send({ cmd: "close_private_space" }) },
      ]);
    }
    openMenu(x, y, [
      // Name, icon, colour and background; switches first, so the changes show.
      { label: t("sidebar.space_menu.settings"), onClick: there(() => send({ cmd: "space_dialog", id: target.id })) },
      { label: t("sidebar.space_menu.new_folder"), onClick: there(() => send({ cmd: "new_folder", title: "" })) },
      SEPARATOR,
      ...state.sidebar.profiles.map((p) => ({
        label: `${p.id === target.profile_id ? "● " : "○ "}${t("sidebar.profile.item", { name: p.name })}`,
        onClick: () => p.id !== target.profile_id && send({ cmd: "set_space_profile", id: target.id, profile: p.id }),
      })),
      { label: t("sidebar.space_menu.new_profile"), onClick: there(() => send({ cmd: "open_palette", mode: "new_profile" })) },
      ...(state.sidebar.spaces.length > 1
        ? [
            SEPARATOR,
            { label: t("sidebar.space_menu.delete"), confirm: t("sidebar.space_menu.delete_confirm"), danger: true, onClick: () => send({ cmd: "delete_space", id: target.id }) },
          ]
        : []),
    ]);
  };


  return (
    <div className="flex flex-col h-full" style={spaceVars} ref={swipe.ref} data-float={FLOAT || undefined} data-side={side} key={FLOAT ? shownAt : undefined}>
      {!FLOAT && <SidebarResizer side={side} />}
      {/* With the top bar on, it holds the band and the address (TopBar.tsx); the floating sidebar keeps its own. */}
      {!topBar && (<>
      {/* Traffic-light band (macOS keeps room for the window's buttons): sidebar toggle left, navigation right. Empty space drags the window. */}
      <div className={`titlebar-drag hdr flex h-[42px] shrink-0 items-center gap-0.5 ${FLOAT || !IS_MAC ? "pl-2" : "pl-[86px]"} pr-2`}>
        {windowButtons}
        <button className="ib no-drag" onClick={() => send({ cmd: "toggle_sidebar" })} title={`${FLOAT ? t("sidebar.header.keep_open") : t("sidebar.header.hide")}${keysHint(state.keys, "toggle_sidebar")}`}><SidebarIcon /></button>
        {!extensionBar && <PinnedExtensions state={state} openMenu={openMenu} />}
        <span className="flex-1" />
        <button className="ib nav no-drag" disabled={!frontRt?.can_go_back} onClick={() => send({ cmd: "go_back" })} onAuxClick={(e) => e.button === 1 && send({ cmd: "open_history_tab", step: -1 })} title={`${t("common.back")} (${keyLabel("⌘[", "Alt+←")})`}><Back /></button>
        <button className="ib nav no-drag" disabled={!frontRt?.can_go_forward} onClick={() => send({ cmd: "go_forward" })} onAuxClick={(e) => e.button === 1 && send({ cmd: "open_history_tab", step: 1 })} title={`${t("sidebar.header.forward")} (${keyLabel("⌘]", "Alt+→")})`}><Forward /></button>
        <button className="ib no-drag" disabled={!activeItem && !state.peek} onClick={() => send({ cmd: "reload" })} onAuxClick={(e) => e.button === 1 && send({ cmd: "open_history_tab", step: 0 })} title={`${t("sidebar.header.reload")} (${keyLabel("⌘R")})`}><Reload /></button>
      </div>

      {/* Address field, or the peek pill in the same slot so nothing moves */}
      <div className="shrink-0 px-2">
        {state.peek ? (
          <div className="urlbar no-drag" data-peek>
            <span className="min-w-0 flex-1 truncate"><span className="text-[var(--muted)]">{t("sidebar.peek.label")} · </span>{state.peek.title || hostOf(state.peek.url)}</span>
            <button className="h-6 shrink-0 rounded-md bg-white/10 px-2 text-[12px] font-medium hover:bg-white/15" title={`${t("sidebar.peek.keep_as_tab")} (${keyLabel("⌘O")})`} onClick={() => send({ cmd: "peek_to_tab" })}>{t("sidebar.peek.keep")}</button>
            <button className="ib sm shrink-0" title={t("sidebar.peek.open_in_split")} aria-label={t("sidebar.peek.open_in_split")} onClick={() => send({ cmd: "peek_to_split" })}><SplitIcon width={14} height={14} /></button>
            <button className="ib sm shrink-0" title={`${t("common.close")} (Esc)`} onClick={() => send({ cmd: "close_peek" })}><Close width={14} height={14} /></button>
          </div>
        ) : (
          <AddressRow state={state} item={activeItem} rt={activeRt} onEdit={() => send({ cmd: "open_palette", mode: activeItem ? "navigate" : "new" })} openMenu={openMenu} />
        )}
      </div>
      {extensionBar && <ExtensionBar state={state} openMenu={openMenu} />}
      </>)}

      {media.length > 0 && <MediaBar entries={media} />}

      <FavoriteLists ctx={ctx} lists={favoriteLists} />

      {/* Lists */}
      <div key={space.id} className="scroll space-list flex-1 min-h-0 px-2 pb-2" style={{ ["--enter-x" as string]: `${enter.current.x}px`, ...(swipe.offset ? { transform: `translateX(${swipe.offset}px)`, transition: "none" } : {}) }} onContextMenu={(e) => { if (e.target === e.currentTarget) { e.preventDefault(); spaceMenu(e.clientX, e.clientY); } }}>
        {isPrivate ? (
        <div className="space-title" onContextMenu={(e) => { e.preventDefault(); spaceMenu(e.clientX, e.clientY); }}>
          <span className="grid h-5 w-5 shrink-0 place-items-center" style={{ color: "var(--space)" }}><MaskIcon width={15} height={15} /></span>
          <span className="truncate">{space.title}</span>
          <button className="ib sm ml-auto" title={`${t("sidebar.space.hide_private")}${keysHint(state.keys, "hide_private_space")}`} aria-label={t("sidebar.space.hide_private")} onClick={() => send({ cmd: "hide_private_space" })}><PrivateIcon width={14} height={14} /></button>
          <button className="ib sm" title={`${t("sidebar.space.close_private")}${keysHint(state.keys, "close_private_space")}`} aria-label={t("sidebar.space.close_private")} onClick={() => send({ cmd: "close_private_space" })}><Close width={14} height={14} /></button>
        </div>
        ) : (
        <div className="space-title group" onContextMenu={(e) => { e.preventDefault(); spaceMenu(e.clientX, e.clientY); }} onDoubleClick={() => setRenamingSpace(space.id)}>
          <button className="grid h-5 w-5 shrink-0 place-items-center rounded-md" style={{ color: "var(--space)" }} title={t("sidebar.space.change_icon")} onClick={(e) => pickSpaceIcon(e.clientX, e.clientY)}><SpaceIcon space={space} size={15} /></button>
          {renamingSpace === space.id ? (
            <RenameInput value={space.title} onCommit={(title) => { send({ cmd: "rename_space", id: space.id, title }); setRenamingSpace(null); }} onCancel={() => setRenamingSpace(null)} />
          ) : (
            <span className="truncate">{space.title}</span>
          )}
          {spaceProfile && renamingSpace !== space.id && <span className="space-profile" data-profile={profileColor(state.sidebar.profiles, spaceProfile.id) ? true : undefined} style={{ ["--profile" as string]: profileColor(state.sidebar.profiles, spaceProfile.id) }} title={t("sidebar.space.profile", { name: spaceProfile.name })}>{spaceProfile.name}</span>}
          <button className="ib sm ml-auto opacity-0 group-hover:opacity-100" title={t("sidebar.space.options")} onClick={(e) => spaceMenu(e.clientX, e.clientY)}><Dots width={14} height={14} /></button>
        </div>
        )}

        {/* Pinned tabs and folders, then one divider, then Today. No labels: position says it. The private space has only Today. */}
        {!isPrivate && <div onDragOver={(e) => dragging() && e.preventDefault()} onDrop={(e) => { if ((e.target as HTMLElement).closest(".row")) return; e.preventDefault(); const d = dragging(); if (d?.kind === "tab") send({ cmd: "move_tab", id: d.id, space: space.id, section: "pinned", index: indexAfterTaking(space.pinned, movedIds(d), space.pinned.length), split: d.pair }); else if (d) send({ cmd: "move_folder", id: d.id, index: indexAfterTaking(space.pinned, movedIds(d), space.pinned.length) }); endDrag(); }}>
          {pinnedOpen ? <NodeList ctx={ctx} nodes={space.pinned} depth={0} /> : frontPinned && <TabRow ctx={ctx} tab={frontPinned.tab} section="pinned" index={frontPinned.index} folderId={frontPinned.folderId} onDropAt={(index) => dropAmongPinned(space.id, frontPinned.among, frontPinned.folderId, index)} />}
        </div>}

        <div className="divider">
          {pinnedCount > 0 && (
            <button className="pinned-toggle" aria-expanded={pinnedOpen} title={pinnedOpen ? t("sidebar.today.hide_pinned_title") : t("sidebar.today.show_pinned_title")} onClick={(e) => !movedUnder(e) && togglePinned()}>
              <ChevronDown width={12} height={12} className="chevron" />
              {pinnedOpen ? t("sidebar.today.hide_pinned") : t("sidebar.today.show_pinned", { count: pinnedCount })}
            </button>
          )}
          <hr />
          {space.today.length > 0 && <button title={isPrivate ? t("sidebar.today.clear_private_title") : t("sidebar.today.clear_title")} onClick={(e) => !movedUnder(e) && send({ cmd: "archive_today" })}>{t("common.clear")}</button>}
        </div>

        <div
          className="min-h-[72px]"
          onDragOver={(e) => dragging()?.kind === "tab" && e.preventDefault()}
          onDrop={(e) => { if ((e.target as HTMLElement).closest(".row")) return; e.preventDefault(); dropToday(space.today.length); endDrag(); }}
        >
          <button className="row newtab w-full" onClick={() => send({ cmd: "open_palette", mode: "new" })} title={`${isPrivate ? t("sidebar.today.new_private_tab_title") : t("sidebar.today.new_tab_title")}${keysHint(state.keys, "new_tab")}`}>
            <span className="icon-slot"><Plus width={15} height={15} /></span>
            <span className="title text-left">{isPrivate ? t("sidebar.today.new_private_tab") : t("common.new_tab")}</span>
          </button>
          {looseSplit && <SplitRow ctx={ctx} split={looseSplit} />}
          {space.today.map((t, i) => (
            <TabRow key={t.id} ctx={ctx} tab={t} section="today" index={i} onDropAt={dropToday} />
          ))}
          {deviceTabs.map((t) => <DeviceTabRow key={t.id} tab={t} />)}
        </div>
      </div>

      <CaptureToast />
      {noticeBox}

      {/* Footer: archive and downloads left (one button), spaces centred, new right */}
      <div className="flex h-12 shrink-0 items-center gap-1 px-2">
        <LibraryButton opens={state.settings.library_opens} keys={state.keys} onOpen={setPanel} />
        <SpaceSwitcher spaces={state.sidebar.spaces} current={space.id} privateSpace={state.private_space} onMenu={spaceMenu} />
        <button
          className="ib lg plus-btn"
          aria-label={newMenu ? t("sidebar.new_menu.close") : t("sidebar.new_menu.open")}
          aria-expanded={newMenu}
          data-open={newMenu || undefined}
          onMouseDown={(e) => newMenu && e.stopPropagation()}
          onClick={() => setNewMenu((o) => !o)}
        >
          <Plus width={18} height={18} />
        </button>
      </div>
      {newMenu && (
        <NewMenu
          onClose={closeNewMenu}
          groups={[
            [
              { id: "space", label: t("sidebar.new_menu.space"), Icon: SpacePlus, shortcut: keysLabel(state.keys.new_space), run: () => send({ cmd: "open_palette", mode: "new_space" }) },
              { id: "private", label: t("sidebar.new_menu.private_space"), Icon: MaskIcon, shortcut: keysLabel(state.keys.private_space), run: () => send({ cmd: "open_private_space" }) },
              { id: "folder", label: t("sidebar.new_menu.folder"), Icon: FolderIcon, disabled: isPrivate, run: () => send({ cmd: "new_folder", title: "" }) },
              { id: "smart", label: t("sidebar.new_menu.smart_folder"), Icon: SmartFolderIcon, disabled: isPrivate, run: () => send({ cmd: "smart_folder_dialog" }) },
            ],
            [
              { id: "split", label: t("sidebar.new_menu.split"), Icon: SplitIcon, disabled: !activeItem, run: () => send({ cmd: "open_palette", mode: "split" }) },
              { id: "tab", label: isPrivate ? t("sidebar.new_menu.private_tab") : t("common.new_tab"), Icon: TabPlus, shortcut: keysLabel(state.keys.new_tab), run: () => send({ cmd: "open_palette", mode: "new" }) },
            ],
          ]}
        />
      )}

      {menu && <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} takeFocus />}
      {iconPicker && (
        <IconPicker
          at={iconPicker}
          current={iconPicker.current}
          allowDefault={iconPicker.allowDefault}
          onPick={(g) => { iconPicker.onPick(g); setIconPicker(null); }}
          onClose={() => setIconPicker(null)}
        />
      )}
    </div>
  );
}

/**
 * Close, minimize and full screen for the floating sidebar on macOS, drawn
 * like the window's own buttons, which the pages cover while it is out.
 */
function WindowButtons() {
  // The floating sidebar must not stay out over a window that left or changed size.
  const act = (cmd: "window_minimize" | "window_fullscreen") => {
    send({ cmd: "float_sidebar", show: false });
    send({ cmd });
  };
  return (
    <div className="window-buttons no-drag">
      <button data-kind="close" onClick={() => send({ cmd: "window_close" })} title={t("common.close")} aria-label={t("common.close")}>
        <svg viewBox="0 0 12 12"><path d="M3.6 3.6l4.8 4.8M8.4 3.6l-4.8 4.8" /></svg>
      </button>
      <button data-kind="minimize" onClick={() => act("window_minimize")} title={t("sidebar.window.minimize")} aria-label={t("sidebar.window.minimize")}>
        <svg viewBox="0 0 12 12"><path d="M2.8 6h6.4" /></svg>
      </button>
      <button data-kind="fullscreen" onClick={() => act("window_fullscreen")} title={t("sidebar.window.full_screen")} aria-label={t("sidebar.window.full_screen")}>
        <svg viewBox="0 0 12 12"><path className="fill" d="M3.4 3.4h3.6L3.4 7zM8.6 8.6H5L8.6 5z" /></svg>
      </button>
    </div>
  );
}

/**
 * Floating sidebar only: hide shortly after the pointer leaves (it may come
 * right back), and report each appearance so the slide-in plays again.
 */
function useFloatLifecycle(): number {
  const [shownAt, setShownAt] = useState(0);
  useEffect(() => {
    if (!FLOAT) return;
    let timer: number | undefined;
    const root = document.documentElement;
    const enter = () => window.clearTimeout(timer);
    // One timer at most: a second leave without an enter must not leave one behind that hides the sidebar under the pointer.
    const leave = () => {
      enter();
      timer = window.setTimeout(() => send({ cmd: "float_sidebar", show: false }), 280);
    };
    // On Linux, shown again within half a second is no new appearance: under Wayland it is hidden and shown to stay above a page just shown.
    let hiddenAt = 0;
    const visible = () => {
      if (document.visibilityState === "hidden") hiddenAt = Date.now();
      else if (!IS_LINUX || Date.now() - hiddenAt > 500) setShownAt(Date.now());
    };
    root.addEventListener("mouseleave", leave);
    root.addEventListener("mouseenter", enter);
    document.addEventListener("visibilitychange", visible);
    return () => {
      root.removeEventListener("mouseleave", leave);
      root.removeEventListener("mouseenter", enter);
      document.removeEventListener("visibilitychange", visible);
    };
  }, []);
  return shownAt;
}
