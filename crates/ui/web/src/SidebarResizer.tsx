// The docked sidebar's inner edge, towards the pages: drag to change its
// width (the pages follow live), release to keep it, double-click for the
// default width. On the left the sidebar view's left edge stays put, so the
// pointer's x is the new width. On the right its left edge moves with the
// width: there the pointer's travel on the screen since the press counts.
import { useRef } from "react";
import { send, type SidebarSide } from "./bridge";

const DEFAULT_WIDTH = 264;

export function SidebarResizer({ side }: { side: SidebarSide }) {
  const frame = useRef(0);
  const latest = useRef(0);
  // Where the drag began: the pointer on the screen and the width then.
  const start = useRef({ x: 0, width: 0 });
  // Never below 0: the pointer may leave the window, and the app refuses a
  // negative width (the release would not be saved).
  const apply = (done: boolean) => send({ cmd: "sidebar_width", width: Math.max(0, Math.round(latest.current)), done });
  const follow = ({ clientX, screenX }: { clientX: number; screenX: number }) => {
    latest.current = side === "left" ? clientX : start.current.width + start.current.x - screenX;
  };
  return (
    <div
      className="sidebar-resizer no-drag"
      data-side={side}
      role="separator"
      aria-orientation="vertical"
      aria-label="Sidebar width"
      title="Drag to resize the sidebar · double-click to reset"
      onPointerDown={(e) => {
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        start.current = { x: e.screenX, width: window.innerWidth };
        follow(e);
      }}
      onPointerMove={(e) => {
        if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
        follow(e);
        // One resize per frame: each one re-lays out the pages.
        if (!frame.current) frame.current = requestAnimationFrame(() => { frame.current = 0; apply(false); });
      }}
      onPointerUp={(e) => {
        if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
        e.currentTarget.releasePointerCapture(e.pointerId);
        cancelAnimationFrame(frame.current);
        frame.current = 0;
        follow(e);
        apply(true);
      }}
      onDoubleClick={() => {
        latest.current = DEFAULT_WIDTH;
        apply(true);
      }}
    />
  );
}
