// The top bar (magnetite://ui/topbar): with Settings → Appearance → Top bar
// on, it spans the window above the sidebar and the pages and takes over the
// sidebar's top rows: the sidebar button, back, forward and reload, the
// address, pinned extensions and, outside macOS, the window buttons.
// Empty space drags the window; a double-click there maximizes it.
import { useCallback, useEffect, useLayoutEffect, useState } from "react";
import { AddressRow } from "./AddressRow";
import { record } from "./recorder";
import { Back, Close, Forward, Maximize, Minus, Reload, SidebarIcon, SplitIcon } from "./Icons";
import { PinnedExtensions, SLOT } from "./PinnedExtensions";
import { windowBackground } from "./backgrounds";
import { IS_MAC, accentCss, applyAppearance, findTab, hostOf, lookOf, onRecord, send, subscribe, type UiState } from "./bridge";
import { keysHint } from "./settings/shortcutKeys";
import { useSidebarShortcuts } from "./useSidebarShortcuts";

/** The least the address keeps beside pinned extensions, in points. */
const ADDRESS_MIN = 240;
/** What the bar holds besides the address and the extensions: the room for
 * the traffic lights (or the left padding), four buttons, the two gaps, the
 * right padding and, outside macOS, the three window buttons. */
const RESERVED = (IS_MAC ? 86 : 8) + 4 * SLOT + 2 * 8 + 8 + (IS_MAC ? 0 : 8 + 3 * SLOT);

export function TopBar() {
  const [state, setState] = useState<UiState | null>(null);
  useEffect(() => subscribe(setState), []);
  useEffect(() => {
    send({ cmd: "get_state" }).then((s) => window.__magnetite?.setState(s));
  }, []);
  useSidebarShortcuts(false, () => {});
  useEffect(() => onRecord(() => void record()), []);
  // The window's width in points: how many pinned extensions fit depends on it.
  const [width, setWidth] = useState(window.innerWidth);
  useEffect(() => {
    const measure = () => setWidth(window.innerWidth);
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, []);
  const appearance = state?.settings.appearance;
  useLayoutEffect(() => {
    if (appearance) applyAppearance(appearance);
  }, [appearance]);
  // Its pop-ups are windows of their own (`TopBarPopup.tsx`): nothing opens in here.
  const openMenu = useCallback(() => {}, []);

  if (!state) return <div className="h-full" />;
  const activeRt = state.active_tab ? state.runtime[state.active_tab] : undefined;
  // Back, forward and reload act on the page in front: the peek while one is open.
  const frontRt = state.peek ? state.peek.runtime : activeRt;
  const activeItem = state.active_tab ? findTab(state, state.active_tab) : null;
  // Focus rings take the active space's colour, as in the sidebar.
  const space = state.sidebar.spaces.find((s) => s.id === state.active_space);
  // Extensions that would squeeze the address stay in the menu bar's
  // Extensions menu. Lengths on the page grow with the text size: the one
  // this render's state asks for, which `applyAppearance` sets after it.
  const zoom = state.settings.appearance.text_scale / 100;
  const room = Math.max(0, Math.floor((width / zoom - RESERVED - ADDRESS_MIN) / SLOT));
  return (
    <div
      className="topbar titlebar-drag"
      style={{ ["--space" as string]: accentCss(space?.accent), ["--bg" as string]: state.chrome, ...windowBackground("topbar", state.chrome, state.active_space, lookOf(state.settings.appearance, state.active_space), space?.accent) }}
      onDoubleClick={(e) => e.target === e.currentTarget && send({ cmd: "window_maximize" })}
    >
      <div className={`flex shrink-0 items-center gap-0.5 ${IS_MAC ? "pl-[86px]" : "pl-2"}`}>
        <button className="ib no-drag" onClick={() => send({ cmd: "toggle_sidebar" })} title={`${state.sidebar_visible ? "Hide sidebar" : "Show sidebar"}${keysHint(state.keys, "toggle_sidebar")}`}><SidebarIcon /></button>
        <button className="ib no-drag" disabled={!frontRt?.can_go_back} onClick={() => send({ cmd: "go_back" })} title="Back (⌘[)"><Back /></button>
        <button className="ib no-drag" disabled={!frontRt?.can_go_forward} onClick={() => send({ cmd: "go_forward" })} title="Forward (⌘])"><Forward /></button>
        <button className="ib no-drag" disabled={!activeItem && !state.peek} onClick={() => send({ cmd: "reload" })} title="Reload (⌘R)"><Reload /></button>
      </div>

      {/* The address, or the peek's controls in the same place so nothing moves. */}
      <div className="topbar-address">
        {state.peek ? (
          <div className="urlbar no-drag" data-peek>
            <span className="min-w-0 flex-1 truncate"><span className="text-[var(--muted)]">Peek · </span>{state.peek.title || hostOf(state.peek.url)}</span>
            <button className="h-6 shrink-0 rounded-md bg-white/10 px-2 text-[12px] font-medium hover:bg-white/15" title="Keep as tab (⌘O)" onClick={() => send({ cmd: "peek_to_tab" })}>Keep</button>
            <button className="ib sm shrink-0" title="Open in split view" aria-label="Open in split view" onClick={() => send({ cmd: "peek_to_split" })}><SplitIcon width={14} height={14} /></button>
            <button className="ib sm shrink-0" title="Close (Esc)" onClick={() => send({ cmd: "close_peek" })}><Close width={14} height={14} /></button>
          </div>
        ) : (
          <AddressRow state={state} item={activeItem} rt={activeRt} onEdit={() => send({ cmd: "open_palette", mode: activeItem ? "navigate" : "new" })} openMenu={openMenu} popups />
        )}
      </div>

      <div className="flex shrink-0 items-center gap-0.5 pr-2">
        <PinnedExtensions state={state} openMenu={openMenu} popups room={room} />
        {!IS_MAC && (
          <div className="ml-2 flex items-center gap-0.5">
            <button className="ib no-drag" onClick={() => send({ cmd: "window_minimize" })} title="Minimize" aria-label="Minimize"><Minus /></button>
            <button className="ib no-drag" onClick={() => send({ cmd: "window_maximize" })} title="Maximize" aria-label="Maximize"><Maximize /></button>
            <button className="ib no-drag window-close" onClick={() => send({ cmd: "window_close" })} title="Close" aria-label="Close"><Close /></button>
          </div>
        )}
      </div>
    </div>
  );
}
