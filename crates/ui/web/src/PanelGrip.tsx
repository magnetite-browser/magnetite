import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { dockedSide, send, subscribe, type SidebarSide, type UiState } from "./bridge";
import { t } from "./i18n";

/** Where the strip's middle is on the screen with the pointer at `screenX`, taken `grab` points into the strip. */
const middle = (screenX: number, grab: number) => Math.round(screenX - grab + window.innerWidth / 2);

/** See-through (`?clear`): the CEF in use can show the page under the panel through the strip (`cef_app::clear_pages`). */
const CLEAR = new URLSearchParams(location.search).has("clear");

/**
 * The strip beyond a floating web panel's far edge on Linux
 * (magnetite://ui/panel-grip), part of the panel's own window and as high as
 * it. Drag it to change the panel's width, double-click for the usual one.
 * Like the dividers on macOS, a short line shows in its middle while hovered,
 * a longer one in brass while dragged. See-through where the CEF can do it,
 * with the panel's shadow falling on the page beside it, as on macOS; else
 * the gap between split pages. The pointer is measured on the screen.
 */
export function PanelGrip() {
  const [look, setLook] = useState<{ chrome: string; side: SidebarSide } | null>(null);
  useEffect(() => subscribe((s: UiState) => setLook({ chrome: s.chrome, side: dockedSide(s.settings.appearance) })), []);
  useEffect(() => {
    send({ cmd: "get_state" }).then((s) => window.__magnetite?.setState(s));
  }, []);
  // The page's own background (index.css) would cover what lies under it.
  useLayoutEffect(() => {
    if (!CLEAR) return;
    for (const el of [document.documentElement, document.body, document.getElementById("root")]) {
      if (el) el.style.background = "transparent";
    }
  }, []);
  // The drag under way: its pointer, where in the strip it was taken (the
  // strip keeps that spot under the pointer), and whether it moved.
  const drag = useRef<{ pointer: number; grab: number; moved: boolean } | null>(null);
  const [dragging, setDragging] = useState(false);
  const frame = useRef(0);
  const latest = useRef(0);
  useEffect(() => () => cancelAnimationFrame(frame.current), []);
  const stop = () => {
    cancelAnimationFrame(frame.current);
    frame.current = 0;
    drag.current = null;
    setDragging(false);
  };
  // See-through from the start, not the page's colour until the state comes.
  const background = CLEAR ? (look ? shadow(look.side) : "none") : look?.chrome;
  return (
    <div
      className="panel-grip"
      data-dragging={dragging || undefined}
      style={background ? { background } : undefined}
      role="separator"
      aria-orientation="vertical"
      aria-label={t("sidebar.panel_grip.label")}
      title={t("sidebar.panel_grip.title")}
      onPointerDown={(e) => {
        e.preventDefault();
        if (drag.current) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { pointer: e.pointerId, grab: e.clientX, moved: false };
        setDragging(true);
        // The click gave the panel's window the keyboard: it goes back on release.
        send({ cmd: "web_panel_grip", done: false });
      }}
      onPointerMove={(e) => {
        const now = drag.current;
        if (now?.pointer !== e.pointerId) return;
        now.moved = true;
        latest.current = middle(e.screenX, now.grab);
        // One resize per frame: each one lays out the panel.
        frame.current ||= requestAnimationFrame(() => {
          frame.current = 0;
          send({ cmd: "web_panel_grip", x: latest.current, done: false });
        });
      }}
      onPointerUp={(e) => {
        const ended = drag.current;
        if (ended?.pointer !== e.pointerId) return;
        e.currentTarget.releasePointerCapture(e.pointerId);
        stop();
        // Without a move: only the keyboard goes back to where it was.
        const x = ended.moved ? middle(e.screenX, ended.grab) : undefined;
        send({ cmd: "web_panel_grip", x, done: true });
      }}
      onPointerCancel={(e) => {
        const ended = drag.current;
        if (ended?.pointer !== e.pointerId) return;
        stop();
        send({ cmd: "web_panel_grip", x: ended.moved ? latest.current : undefined, done: true });
      }}
      onDoubleClick={() => send({ cmd: "web_panel_width", width: null, done: true })}
    />
  );
}

/** The panel's shadow on the page beside it, from the panel's edge: the panel lies on the sidebar's `side` of the strip. */
const shadow = (side: SidebarSide) => `linear-gradient(to ${side === "left" ? "right" : "left"}, rgb(0 0 0 / 0.22), transparent)`;
