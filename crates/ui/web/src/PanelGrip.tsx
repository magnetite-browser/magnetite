import { useEffect, useRef } from "react";
import { send } from "./bridge";

/**
 * The grip at a floating web panel's far edge on Linux (magnetite://ui/panel-grip),
 * a window of its own over the page. It stays put during a drag, which goes by the
 * pointer on the screen, and moves to the panel's new edge on release. Double-click
 * for the usual width.
 */
export function PanelGrip() {
  const drag = useRef<{ pointer: number; moved: boolean } | null>(null);
  const frame = useRef(0);
  const latest = useRef(0);
  useEffect(() => () => cancelAnimationFrame(frame.current), []);
  const stop = () => {
    cancelAnimationFrame(frame.current);
    frame.current = 0;
  };
  return (
    <div
      className="panel-grip"
      role="separator"
      aria-orientation="vertical"
      aria-label="Web panel width"
      title="Drag to resize the panel · double-click to reset"
      onPointerDown={(e) => {
        e.preventDefault();
        if (drag.current) return;
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { pointer: e.pointerId, moved: false };
      }}
      onPointerMove={(e) => {
        const now = drag.current;
        if (now?.pointer !== e.pointerId) return;
        now.moved = true;
        latest.current = e.screenX;
        // One resize per frame: each one lays out the panel.
        frame.current ||= requestAnimationFrame(() => {
          frame.current = 0;
          send({ cmd: "web_panel_grip", x: latest.current, done: false });
        });
      }}
      onPointerUp={(e) => {
        const ended = drag.current;
        if (ended?.pointer !== e.pointerId) return;
        drag.current = null;
        e.currentTarget.releasePointerCapture(e.pointerId);
        stop();
        if (ended.moved) send({ cmd: "web_panel_grip", x: e.screenX, done: true });
      }}
      onPointerCancel={(e) => {
        const ended = drag.current;
        if (ended?.pointer !== e.pointerId) return;
        drag.current = null;
        stop();
        // The app holds the grip's window still until it hears the drag is over.
        if (ended.moved) send({ cmd: "web_panel_grip", x: latest.current, done: true });
      }}
      onDoubleClick={() => send({ cmd: "web_panel_width", width: null, done: true })}
    />
  );
}
