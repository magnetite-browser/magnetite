import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { ArchiveView } from "./ArchiveView";
import { BookmarksView } from "./BookmarksView";
import { DownloadsView } from "./DownloadsView";
import { HistoryView } from "./HistoryView";
import { LibraryButton } from "./LibraryButton";
import { SidebarResizer } from "./SidebarResizer";
import { ContextMenu, SEPARATOR, type MenuItem } from "./ContextMenu";
import { Back, Close, Dots, FolderIcon, Forward, Plus, Reload, SidebarIcon, SmartFolderIcon, SpacePlus, SplitIcon, TabPlus } from "./Icons";
import { AddressRow } from "./AddressRow";
import { CaptureToast } from "./CaptureToast";
import { MediaBar, type MediaEntry } from "./MediaBar";
import { NewMenu } from "./NewMenu";
import { PinnedExtensions } from "./PinnedExtensions";
import { FavoriteLists, NodeList, RenameInput, SplitRow, TabRow, type RowContext } from "./Rows";
import { accentCss, applyAppearance, findTab, findTabInSpace, hostOf, IS_MAC, keyLabel, onNotice, onOpenArchive, onOpenBookmarks, onOpenDownloads, onOpenHistory, send, splitIds, subscribe, type LibraryView, type Space, type SplitNode, type UiState } from "./bridge";
import { IconPicker, SpaceGlyph } from "./Glyphs";
import { SpaceSwitcher } from "./SpaceSwitcher";
import { dragging, endDrag, indexAfterTaking } from "./dnd";
import { useSpaceSwipe } from "./useSpaceSwipe";
import { useSidebarShortcuts } from "./useSidebarShortcuts";

/**
 * `magnetite://ui/?float`: this page is the sidebar floating over the pages while
 * the docked one is hidden. It leaves when the pointer does (or on Esc).
 */
const FLOAT = new URLSearchParams(location.search).has("float");

export function App() {
  const [state, setState] = useState<UiState | null>(null);
  const [newMenu, setNewMenu] = useState(false);
  const closeNewMenu = useCallback(() => setNewMenu(false), []);
  const newProfileFor = useCallback((tabId: string) => send({ cmd: "open_palette", mode: "new_profile", tab: tabId }), []);
  // Direction the space list slides in from after a space change.
  const enter = useRef({ id: "", index: 0, x: 0 });
  const swipe = useSpaceSwipe(useCallback((direction: 1 | -1) => send({ cmd: direction > 0 ? "next_space" : "prev_space" }), []));
  // A full-height view in place of the sidebar.
  const [panel, setPanel] = useState<LibraryView | null>(null);
  const closePanel = useCallback(() => setPanel(null), []);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  // The space whose title is being edited (it shows once that space is active).
  const [renamingSpace, setRenamingSpace] = useState<string | null>(null);
  // The glyph picker, for the space icon or a folder's (`onPick(null)`: default icon).
  const [iconPicker, setIconPicker] = useState<{ x: number; y: number; current?: string; onPick: (glyph: string | null) => void; allowDefault?: boolean } | null>(null);
  const pickIcon = useCallback<RowContext["pickIcon"]>((at, current, onPick, allowDefault) => setIconPicker({ ...at, current, onPick, allowDefault }), []);

  useEffect(() => subscribe(setState), []);
  const shownAt = useFloatLifecycle();
  useEffect(() => onOpenArchive(() => setPanel("archive")), []);
  useEffect(() => onOpenDownloads(() => setPanel("downloads")), []);
  useEffect(() => onOpenBookmarks(() => setPanel("bookmarks")), []);
  useEffect(() => onOpenHistory(() => setPanel("history")), []);
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
  // The same in every space: one list of favorites per profile that has some.
  const favoriteLists = state.sidebar.favorites.filter((f) => f.items.length > 0);
  const spaceIndex = state.sidebar.spaces.indexOf(space);
  if (enter.current.id !== space.id) {
    enter.current = { id: space.id, index: spaceIndex, x: enter.current.id ? Math.sign(spaceIndex - enter.current.index) * 24 : 0 };
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
  // The split on screen with no row here (favorites, other spaces) shows above Today.
  const looseSplit = state.split && !splitIds(state.split).some((id) => splitAt.has(id)) ? state.split : null;
  const ctx: RowContext = { state, spaceId: space.id, openMenu, renaming, setRenaming, newProfileFor, splitAt, inSplit, pickIcon };
  const pickSpaceIcon = (x: number, y: number, target: Space = space) =>
    pickIcon({ x, y }, target.icon, (g) => g && send({ cmd: "rename_space", id: target.id, title: target.title, icon: g }));
  const activeRt = state.active_tab ? state.runtime[state.active_tab] : undefined;
  // Back, forward and reload act on the page in front: the peek while one is open.
  const frontRt = state.peek ? state.peek.runtime : activeRt;
  const activeItem = state.active_tab ? findTab(state, state.active_tab) : null;
  const spaceVars = { ["--space" as string]: accentCss(space.accent), ["--space-soft" as string]: accentCss(space.accent, 0.14), ["--bg" as string]: state.chrome, background: state.chrome };
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
  if (panel === "archive") return <div className="h-full" style={spaceVars}><ArchiveView count={state.archive_count} onClose={closePanel} />{noticeBox}</div>;
  if (panel === "downloads") return <div className="h-full" style={spaceVars}><DownloadsView onClose={closePanel} />{noticeBox}</div>;
  if (panel === "history") return <div className="h-full" style={spaceVars}><HistoryView count={state.history_count} onClose={closePanel} />{noticeBox}</div>;
  if (panel === "bookmarks") return <div className="h-full" style={spaceVars}><BookmarksView state={state} onClose={closePanel} />{noticeBox}</div>;

  const dropToday = (index: number) => {
    const d = dragging();
    if (d?.kind === "tab") send({ cmd: "move_tab", id: d.id, space: space.id, section: "today", index: indexAfterTaking(space.today, d.id, index) });
  };

  // Options for `target` (the active space unless right-clicked in the footer).
  // Renaming, new folders and new profiles act on the active space, so those switch first.
  const spaceMenu = (x: number, y: number, target: Space = space) => {
    const there = (run: () => void) => () => {
      if (target.id !== space.id) send({ cmd: "switch_space", id: target.id });
      run();
    };
    openMenu(x, y, [
      { label: "Rename space", onClick: there(() => setRenamingSpace(target.id)) },
      { label: "Change icon", onClick: () => pickSpaceIcon(x, y, target) },
      { label: "New folder", onClick: there(() => send({ cmd: "new_folder", title: "" })) },
      SEPARATOR,
      ...state.sidebar.profiles.map((p) => ({
        label: `${p.id === target.profile_id ? "● " : "○ "}Profile: ${p.name}`,
        onClick: () => p.id !== target.profile_id && send({ cmd: "set_space_profile", id: target.id, profile: p.id }),
      })),
      { label: "New profile for this space…", onClick: there(() => send({ cmd: "open_palette", mode: "new_profile" })) },
      ...(state.sidebar.spaces.length > 1
        ? [
            SEPARATOR,
            { label: "Delete space…", confirm: "Click again to delete space", danger: true, onClick: () => send({ cmd: "delete_space", id: target.id }) },
          ]
        : []),
    ]);
  };


  return (
    <div className="flex flex-col h-full" style={spaceVars} onWheel={swipe.onWheel} data-float={FLOAT || undefined} key={FLOAT ? shownAt : undefined}>
      {!FLOAT && <SidebarResizer />}
      {/* Traffic-light band: sidebar toggle left, navigation right. Empty space drags the window.
          macOS draws its traffic lights into the room on the left; elsewhere the window's buttons
          are at the top right, above the pages (the engine's caption.rs). */}
      <div className={`titlebar-drag hdr flex h-[42px] shrink-0 items-center gap-0.5 ${FLOAT || !IS_MAC ? "pl-2" : "pl-[86px]"} pr-2`}>
        <button className="ib no-drag" onClick={() => send({ cmd: "toggle_sidebar" })} title={`${FLOAT ? "Keep the sidebar open" : "Hide sidebar"} (${keyLabel("⌘S")})`}><SidebarIcon /></button>
        <PinnedExtensions state={state} openMenu={openMenu} />
        <span className="flex-1" />
        <button className="ib no-drag" disabled={!frontRt?.can_go_back} onClick={() => send({ cmd: "go_back" })} title={`Back (${keyLabel("⌘[", "Alt+←")})`}><Back /></button>
        <button className="ib no-drag" disabled={!frontRt?.can_go_forward} onClick={() => send({ cmd: "go_forward" })} title={`Forward (${keyLabel("⌘]", "Alt+→")})`}><Forward /></button>
        <button className="ib no-drag" disabled={!activeItem && !state.peek} onClick={() => send({ cmd: "reload" })} title={`Reload (${keyLabel("⌘R")})`}><Reload /></button>
      </div>

      {/* Address field, or the peek pill in the same slot so nothing moves */}
      <div className="shrink-0 px-2">
        {state.peek ? (
          <div className="urlbar no-drag" data-peek>
            <span className="min-w-0 flex-1 truncate"><span className="text-[var(--muted)]">Peek · </span>{state.peek.title || hostOf(state.peek.url)}</span>
            <button className="h-6 shrink-0 rounded-md bg-white/10 px-2 text-[12px] font-medium hover:bg-white/15" title={`Keep as tab (${keyLabel("⌘O")})`} onClick={() => send({ cmd: "peek_to_tab" })}>Keep</button>
            <button className="ib sm shrink-0" title="Open in split view" aria-label="Open in split view" onClick={() => send({ cmd: "peek_to_split" })}><SplitIcon width={14} height={14} /></button>
            <button className="ib sm shrink-0" title="Close (Esc)" onClick={() => send({ cmd: "close_peek" })}><Close width={14} height={14} /></button>
          </div>
        ) : (
          <AddressRow state={state} item={activeItem} rt={activeRt} onEdit={() => send({ cmd: "open_palette", mode: activeItem ? "navigate" : "new" })} openMenu={openMenu} />
        )}
      </div>

      {media.length > 0 && <MediaBar entries={media} />}

      <FavoriteLists ctx={ctx} lists={favoriteLists} />

      {/* Lists */}
      <div key={space.id} className="scroll space-list flex-1 min-h-0 px-2 pb-2" style={{ ["--enter-x" as string]: `${enter.current.x}px`, ...(swipe.offset ? { transform: `translateX(${swipe.offset}px)`, transition: "none" } : {}) }} onContextMenu={(e) => { if (e.target === e.currentTarget) { e.preventDefault(); spaceMenu(e.clientX, e.clientY); } }}>
        <div className="space-title group" onContextMenu={(e) => { e.preventDefault(); spaceMenu(e.clientX, e.clientY); }} onDoubleClick={() => setRenamingSpace(space.id)}>
          <button className="grid h-5 w-5 shrink-0 place-items-center rounded-md" style={{ color: "var(--space)" }} title="Change icon" onClick={(e) => pickSpaceIcon(e.clientX, e.clientY)}><SpaceGlyph icon={space.icon} title={space.title} size={15} /></button>
          {renamingSpace === space.id ? (
            <RenameInput value={space.title} onCommit={(title) => { send({ cmd: "rename_space", id: space.id, title }); setRenamingSpace(null); }} onCancel={() => setRenamingSpace(null)} />
          ) : (
            <span className="truncate">{space.title}</span>
          )}
          <button className="ib sm ml-auto opacity-0 group-hover:opacity-100" title="Space options" onClick={(e) => spaceMenu(e.clientX, e.clientY)}><Dots width={14} height={14} /></button>
        </div>

        {/* Pinned tabs and folders, then one divider, then Today. No labels: position says it. */}
        <div onDragOver={(e) => dragging() && e.preventDefault()} onDrop={(e) => { if ((e.target as HTMLElement).closest(".row")) return; e.preventDefault(); const d = dragging(); if (d?.kind === "tab") send({ cmd: "move_tab", id: d.id, space: space.id, section: "pinned", index: space.pinned.length }); endDrag(); }}>
          <NodeList ctx={ctx} nodes={space.pinned} depth={0} />
        </div>

        <div className="divider">
          <hr />
          {space.today.length > 0 && <button title="Archive all Today tabs" onClick={() => send({ cmd: "archive_today" })}>Clear</button>}
        </div>

        <div
          className="min-h-[72px]"
          onDragOver={(e) => dragging()?.kind === "tab" && e.preventDefault()}
          onDrop={(e) => { if ((e.target as HTMLElement).closest(".row")) return; e.preventDefault(); dropToday(space.today.length); endDrag(); }}
        >
          <button className="row newtab w-full" onClick={() => send({ cmd: "open_palette", mode: "new" })} title={`New tab (${keyLabel("⌘T")})`}>
            <span className="icon-slot"><Plus width={15} height={15} /></span>
            <span className="title text-left">New Tab</span>
          </button>
          {looseSplit && <SplitRow ctx={ctx} split={looseSplit} />}
          {space.today.map((t, i) => (
            <TabRow key={t.id} ctx={ctx} tab={t} section="today" index={i} onDropAt={dropToday} />
          ))}
        </div>
      </div>

      <CaptureToast />
      {noticeBox}

      {/* Footer: archive and downloads left (one button), spaces centred, new right */}
      <div className="flex h-12 shrink-0 items-center gap-1 px-2">
        <LibraryButton opens={state.settings.library_opens} onOpen={setPanel} />
        <SpaceSwitcher spaces={state.sidebar.spaces} current={space.id} onMenu={spaceMenu} />
        <button
          className="ib lg plus-btn"
          aria-label={newMenu ? "Close menu" : "New…"}
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
              { id: "space", label: "New Space", Icon: SpacePlus, shortcut: keyLabel("⇧⌘N"), run: () => send({ cmd: "open_palette", mode: "new_space" }) },
              { id: "folder", label: "New Folder", Icon: FolderIcon, run: () => send({ cmd: "new_folder", title: "" }) },
              { id: "smart", label: "New Smart Folder", Icon: SmartFolderIcon, run: () => send({ cmd: "smart_folder_dialog" }) },
            ],
            [
              { id: "split", label: "New Split", Icon: SplitIcon, disabled: !activeItem, run: () => send({ cmd: "open_palette", mode: "split" }) },
              { id: "tab", label: "New Tab", Icon: TabPlus, shortcut: keyLabel("⌘T"), run: () => send({ cmd: "open_palette", mode: "new" }) },
            ],
          ]}
        />
      )}

      {menu && <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />}
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
 * Floating sidebar only: hide shortly after the pointer leaves (it may come
 * right back), and report each appearance so the slide-in plays again.
 */
function useFloatLifecycle(): number {
  const [shownAt, setShownAt] = useState(0);
  useEffect(() => {
    if (!FLOAT) return;
    let timer: number | undefined;
    const root = document.documentElement;
    const leave = () => (timer = window.setTimeout(() => send({ cmd: "float_sidebar", show: false }), 280));
    const enter = () => window.clearTimeout(timer);
    const visible = () => document.visibilityState === "visible" && setShownAt(Date.now());
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
