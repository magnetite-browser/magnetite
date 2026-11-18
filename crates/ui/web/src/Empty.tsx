// The page behind the tabs (magnetite://ui/empty). Around the pages it is the
// window's frame, with the effect of the current space's look and a shadow
// under each page; while no tab is on screen it also shows the logo and how
// to open a tab, or the space's own picture from Settings → Appearance.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import icon from "./assets/icon.png";
import { userImageUrl, windowBackground } from "./backgrounds";
import { Close } from "./Icons";
import { IS_MAC, applyAppearance, findTab, lookOf, onPageCards, send, subscribe, type PageCards, type UiState } from "./bridge";
import { useSidebarShortcuts } from "./useSidebarShortcuts";

export function Empty() {
  const [state, setState] = useState<UiState | null>(null);
  useEffect(() => subscribe(setState), []);
  useEffect(() => {
    send({ cmd: "get_state" }).then((s) => window.__magnetite?.setState(s));
  }, []);
  const [cards, setCards] = useState<PageCards | null>(null);
  useEffect(() => onPageCards(setCards), []);
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
  // Outside macOS a web panel sits beside the pages: the gap after it is this page's, and the button to close it while no page is there.
  const panel = !IS_MAC && state.web_panel ? findTab(state, state.web_panel) : null;
  const zoom = appearance.text_scale / 100;
  const grip = panel && (
    <PanelResizer key={panel.id} id={panel.id} width={panel.panel_width} zoom={zoom} />
  );
  // Pages lie over this one: only the gaps around them show, and their shadows.
  if (state.panes.length > 0 || state.peek) {
    return (
      <div className="h-full" style={frame}>
        {cards && <PageShadows at={cards} zoom={zoom} />}
        {grip}
      </div>
    );
  }
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

// The pages' corners: only macOS rounds them (`round_window_corners`).
const PAGE_RADIUS = IS_MAC ? 10 : 0;
// How far the box casting a page's shadow lies inside the page's edge: only
// the blurred part comes out from under the page, so no line shows along the
// edge, nor a sliver where the box's corner is not quite the window's.
const SHADOW_INSET = 2;

/**
 * A shadow under each page, where the app laid the pages out (`at`). Each one
 * holds on to the edges of the page area rather than to its size: while the
 * window is resized it follows before the app's next word. Laid out in window
 * points, so the page's text size (`zoom`) is taken out again.
 */
function PageShadows({ at, zoom }: { at: PageCards; zoom: number }) {
  return (
    <div className="page-shadows" style={{ zoom: 1 / zoom }}>
      {at.cards.map(([x, y, width, height], i) => (
        <div
          key={i}
          style={{
            left: x + SHADOW_INSET,
            top: y + SHADOW_INSET,
            right: at.width - x - width + SHADOW_INSET,
            bottom: at.height - y - height + SHADOW_INSET,
            borderRadius: Math.max(PAGE_RADIUS - SHADOW_INSET, 0),
          }}
        />
      ))}
    </div>
  );
}

// The page area's inset, the gap beyond a docked web panel, the panel's
// widths and its close button, as the app lays them out (`CONTENT_INSET` in
// frame.rs, `SPLIT_GAP` and `web_panel_rect` in content.rs, `SIZE` and
// `MARGIN` in panel_close.rs).
const INSET = 8;
const GAP = 8;
const CLOSE = { size: 32, margin: 8 };
const PANEL = { usual: 420, min: 280, rest: 80 };

/**
 * The gap between a docked web panel, `width` points wide (the usual width
 * without one), and the pages: drag to change the panel's width (the pages
 * follow live), release to keep it, double-click for the usual width. The gap
 * is laid out in window points; lengths on this page grow by `zoom`, its text
 * size. Beyond the gap, at the panel's top corner, is the round button that
 * closes panel `id`: a page beside the panel lies over it and has the same
 * button in the same place (`panel_close.rs`).
 */
function PanelResizer({ id, width, zoom }: { id: string; width?: number; zoom: number }) {
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
  const within = (w: number) => Math.max(1, Math.min(Math.max(w, PANEL.min), area - 2 * INSET - PANEL.rest));
  // The pointer stays in the middle of the gap.
  const wanted = (x: number) => within(Math.round(x - INSET - GAP / 2));
  const shown = within(dragged ?? width ?? PANEL.usual);
  const stop = () => {
    cancelAnimationFrame(frame.current);
    frame.current = 0;
  };
  return (
    <>
    <button
      className="panel-close"
      style={{ left: (INSET + shown + GAP + CLOSE.margin) / zoom, top: (INSET + CLOSE.margin) / zoom, width: CLOSE.size / zoom, height: CLOSE.size / zoom }}
      title="Close the panel"
      aria-label="Close the panel"
      onClick={() => send({ cmd: "toggle_web_panel", id })}
    >
      <Close size={14} />
    </button>
    <div
      className="panel-resizer"
      style={{ left: (INSET + shown) / zoom, width: GAP / zoom, top: INSET / zoom, bottom: INSET / zoom }}
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
    </>
  );
}
