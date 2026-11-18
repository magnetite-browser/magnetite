// The page behind the tabs (magnetite://ui/empty). Around the pages it is the
// window's frame, with the effect of the current space's look; while no tab is
// on screen it also shows the logo and how to open a tab, or the space's own
// picture from Settings → Appearance.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import icon from "./assets/icon.png";
import { userImageUrl, windowBackground } from "./backgrounds";
import { Collapse } from "./Icons";
import { IS_MAC, applyAppearance, findTab, lookOf, send, subscribe, type UiState } from "./bridge";
import { useSidebarShortcuts } from "./useSidebarShortcuts";

export function Empty() {
  const [state, setState] = useState<UiState | null>(null);
  useEffect(() => subscribe(setState), []);
  useEffect(() => {
    send({ cmd: "get_state" }).then((s) => window.__magnetite?.setState(s));
  }, []);
  useSidebarShortcuts(false, () => {});
  const appearance = state?.settings.appearance;
  useLayoutEffect(() => {
    if (appearance) applyAppearance(appearance);
  }, [appearance]);

  if (!state || !appearance) return <div className="h-full" />;
  // The space in front decides: each has its own look.
  const look = lookOf(appearance, state.active_space);
  const space = state.sidebar.spaces.find((s) => s.id === state.active_space);
  const frame = windowBackground("frame", state.chrome, state.active_space, look, space?.accent);
  // Outside macOS a web panel sits beside the pages: the bar before it and the gap after it are this page's.
  const panel = !IS_MAC && state.web_panel ? findTab(state, state.web_panel) : null;
  const zoom = appearance.text_scale / 100;
  const grip = panel && (
    <>
      <PanelBar id={panel.id} zoom={zoom} />
      <PanelResizer key={panel.id} width={panel.panel_width} zoom={zoom} />
    </>
  );
  // Pages lie over this one: only the gaps around them show.
  if (state.panes.length > 0 || state.peek) return <div className="h-full" style={frame}>{grip}</div>;
  const stamp = look.empty_image;
  if (stamp != null) {
    return <div className="h-full bg-cover bg-center" style={{ backgroundColor: state.chrome, backgroundImage: `url("${userImageUrl("empty", state.active_space, stamp)}")` }}>{grip}</div>;
  }
  return (
    <div className="flex h-full select-none flex-col items-center justify-center gap-3.5" style={frame}>
      <img src={icon} width={72} height={72} alt="" draggable={false} />
      <div className="text-[15px] text-[var(--muted)]">Press {IS_MAC ? "⌘T" : "Ctrl+T"} to open a tab</div>
      {grip}
    </div>
  );
}

// The page area's inset, the bar before a docked web panel, the gap beyond
// it and the panel's widths, as the app lays them out (`CONTENT_INSET` in
// frame.rs, `WEB_PANEL_BAR`, `SPLIT_GAP` and `web_panel_rect` in content.rs).
const INSET = 8;
const BAR = 20;
const GAP = 8;
const PANEL = { usual: 420, min: 280, rest: 80 };

/**
 * The bar between the sidebar and docked web panel `id`, as high as the
 * panel and set off from the frame around it: a click on it collapses the
 * panel, always in the same place. The page stays loaded, as after a click
 * on the panel's favorite. Laid out in window points; lengths on this page
 * grow by `zoom`, its text size.
 */
function PanelBar({ id, zoom }: { id: string; zoom: number }) {
  // The same space to the sidebar as to the panel.
  const edge = 4;
  return (
    <button
      className="panel-collapse"
      style={{ left: edge / zoom, width: (INSET + BAR - 2 * edge) / zoom, top: INSET / zoom, bottom: INSET / zoom }}
      title="Collapse the panel"
      aria-label="Collapse the panel"
      onClick={() => send({ cmd: "toggle_web_panel", id })}
    >
      <Collapse size={16} />
    </button>
  );
}

/**
 * The gap between a docked web panel, `width` points wide (the usual width
 * without one), and the pages: drag to change the panel's width (the pages
 * follow live), release to keep it, double-click for the usual width. The gap
 * is laid out in window points; lengths on this page grow by `zoom`, its text
 * size.
 */
function PanelResizer({ width, zoom }: { width?: number; zoom: number }) {
  const [area, setArea] = useState(window.innerWidth);
  useEffect(() => {
    const measure = () => setArea(window.innerWidth);
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, []);
  // While dragging, the width the pointer asks for; the app answers a release with the kept one.
  const [dragged, setDragged] = useState<number | null>(null);
  useEffect(() => setDragged(null), [width]);
  // The drag under way: its pointer (a second finger is none of it), whether
  // it moved (a click changes nothing) and whether the app heard of it yet.
  const drag = useRef<{ pointer: number; moved: boolean; sent: boolean } | null>(null);
  const frame = useRef(0);
  // A resize still waiting when the panel goes would change the next one.
  useEffect(() => () => cancelAnimationFrame(frame.current), []);
  const latest = useRef(0);
  const apply = (done: boolean) => send({ cmd: "web_panel_width", width: latest.current, done });
  // A width within the panel's bounds, as the app keeps it.
  const within = (w: number) => Math.max(1, Math.min(Math.max(w, PANEL.min), area - 2 * INSET - BAR - PANEL.rest));
  // The pointer stays in the middle of the gap.
  const wanted = (x: number) => within(Math.round(x - INSET - BAR - GAP / 2));
  const shown = within(dragged ?? width ?? PANEL.usual);
  const stop = () => {
    cancelAnimationFrame(frame.current);
    frame.current = 0;
  };
  return (
    <div
      className="panel-resizer"
      style={{ left: (INSET + BAR + shown) / zoom, width: GAP / zoom, top: INSET / zoom, bottom: INSET / zoom }}
      role="separator"
      aria-orientation="vertical"
      aria-label="Web panel width"
      title="Drag to resize the panel · double-click to reset"
      onPointerDown={(e) => {
        e.preventDefault();
        if (drag.current) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { pointer: e.pointerId, moved: false, sent: false };
      }}
      onPointerMove={(e) => {
        const now = drag.current;
        if (now?.pointer !== e.pointerId) return;
        now.moved = true;
        latest.current = wanted(e.clientX);
        setDragged(latest.current);
        // One resize per frame: each one re-lays out the pages.
        frame.current ||= requestAnimationFrame(() => {
          frame.current = 0;
          now.sent = true;
          apply(false);
        });
      }}
      onPointerUp={(e) => {
        const ended = drag.current;
        if (ended?.pointer !== e.pointerId) return;
        drag.current = null;
        e.currentTarget.releasePointerCapture(e.pointerId);
        stop();
        if (!ended.moved) return;
        latest.current = wanted(e.clientX);
        apply(true);
      }}
      onPointerCancel={(e) => {
        const ended = drag.current;
        if (ended?.pointer !== e.pointerId) return;
        drag.current = null;
        stop();
        setDragged(null);
        // The drag did not end: the width from before it, if the app heard of another, and nothing kept.
        if (ended.sent) send({ cmd: "web_panel_width", width: width ?? null, done: false });
      }}
      onDoubleClick={() => send({ cmd: "web_panel_width", width: null, done: true })}
    />
  );
}
