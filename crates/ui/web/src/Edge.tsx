import { useEffect, useLayoutEffect, useRef } from "react";
import { send } from "./bridge";

/** After the window shows, how long the pointer it reports counts as resting there (ms). */
const SETTLE = 300;
/** At most one slide-out per this many ms while the pointer moves over the strip. */
const AGAIN = 400;

/**
 * The window's edge on the sidebar's side while the sidebar is hidden, under
 * Wayland (magnetite://ui/edge): a slim window of its own over that edge,
 * since a program there hears of the pointer only over its own windows. The
 * pointer moving onto it, or a finger touching it, slides the sidebar out
 * (`edge.rs`), a finger's with the keyboard (`Engine::touch_float_sidebar`).
 * Nothing to see: the window is see-through where the CEF can do it, else in
 * the colour the frame had when it was made (`edge.rs`).
 */
export function Edge() {
  // The page's own background (index.css) would cover what lies under it.
  useLayoutEffect(() => {
    for (const el of [document.documentElement, document.body, document.getElementById("root")]) {
      if (el) el.style.background = "transparent";
    }
  }, []);
  // When the window last showed. It is shown again above a page just shown,
  // and the compositor then reports a pointer resting on it as arriving:
  // that pointer counts once it moves.
  const shownAt = useRef(performance.now());
  useEffect(() => {
    const visible = () => {
      if (document.visibilityState === "visible") shownAt.current = performance.now();
    };
    document.addEventListener("visibilitychange", visible);
    return () => document.removeEventListener("visibilitychange", visible);
  }, []);
  const sentAt = useRef(-AGAIN);
  // Moves count as well as arrivals: a window that appeared over the pointer
  // (the floating sidebar) may leave the page thinking the pointer is still
  // here, and then no arrival comes. A press always counts; a finger first
  // enters, then presses, and counts once, as a touch.
  const slideOut = (e: React.PointerEvent) => {
    const touch = e.pointerType === "touch";
    const now = performance.now();
    if (e.type !== "pointerdown" && (touch || now - shownAt.current < SETTLE || now - sentAt.current < AGAIN)) return;
    sentAt.current = now;
    send({ cmd: "float_sidebar", show: true, touch });
  };
  return <div className="edge-zone" onPointerEnter={slideOut} onPointerMove={slideOut} onPointerDown={slideOut} />;
}
