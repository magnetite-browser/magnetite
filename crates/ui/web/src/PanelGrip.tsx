import { useEffect, useRef, useState } from "react";
import { send, subscribe, type UiState } from "./bridge";

/**
 * The strip beyond a floating web panel's far edge on Linux
 * (magnetite://ui/panel-grip), part of the panel's own window and as high as
 * it: the gap beside the panel, as between split pages. Drag it to change
 * the panel's width, double-click for the usual one. Like the dividers on
 * macOS, a short line shows in its middle while hovered, a longer one in
 * brass while dragged. The pointer is measured on the screen: the panel's
 * near edge, and with it the window, stays put while it grows or shrinks.
 */
/** Where the strip's middle is on the screen with the pointer at `screenX`, taken `grab` points into the strip. */
const middle = (screenX: number, grab: number) => Math.round(screenX - grab + window.innerWidth / 2);

export function PanelGrip() {
  const [chrome, setChrome] = useState<string | null>(null);
  useEffect(() => subscribe((s: UiState) => setChrome(s.chrome)), []);
  useEffect(() => {
    send({ cmd: "get_state" }).then((s) => window.__magnetite?.setState(s));
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
  return (
    <div
      className="panel-grip"
      data-dragging={dragging || undefined}
      style={chrome ? { background: chrome } : undefined}
      role="separator"
      aria-orientation="vertical"
      aria-label="Web panel width"
      title="Drag to resize the panel · double-click to reset"
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
