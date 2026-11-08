// The docked sidebar's right edge: drag to change its width (the pages follow
// live), release to keep it, double-click for the default width. The sidebar
// view's left edge stays put, so the pointer's x is the new width.
import { useRef } from "react";
import { send } from "./bridge";

const DEFAULT_WIDTH = 264;

export function SidebarResizer() {
  const frame = useRef(0);
  const latest = useRef(0);
  // Never below 0: the pointer may leave the window on the left, and the app
  // refuses a negative width (the release would not be saved).
  const apply = (done: boolean) => send({ cmd: "sidebar_width", width: Math.max(0, Math.round(latest.current)), done });
  return (
    <div
      className="sidebar-resizer no-drag"
      role="separator"
      aria-orientation="vertical"
      aria-label="Sidebar width"
      title="Drag to resize the sidebar · double-click to reset"
      onPointerDown={(e) => {
        e.preventDefault();
        e.currentTarget.setPointerCapture(e.pointerId);
        latest.current = e.clientX;
      }}
      onPointerMove={(e) => {
        if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
        latest.current = e.clientX;
        // One resize per frame: each one re-lays out the pages.
        if (!frame.current) frame.current = requestAnimationFrame(() => { frame.current = 0; apply(false); });
      }}
      onPointerUp={(e) => {
        if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
        e.currentTarget.releasePointerCapture(e.pointerId);
        cancelAnimationFrame(frame.current);
        frame.current = 0;
        latest.current = e.clientX;
        apply(true);
      }}
      onDoubleClick={() => {
        latest.current = DEFAULT_WIDTH;
        apply(true);
      }}
    />
  );
}
