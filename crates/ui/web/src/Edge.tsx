import { useLayoutEffect } from "react";
import { send } from "./bridge";

/**
 * The window's edge on the sidebar's side while the sidebar is hidden, under
 * Wayland (magnetite://ui/edge): a slim window of its own over that edge,
 * since a program there hears of the pointer only over its own windows. The
 * pointer arriving, or a finger touching it, slides the sidebar out
 * (`edge.rs`), a finger's with the keyboard (`Engine::touch_float_sidebar`).
 * Nothing to see: the window is see-through where the CEF can do it, else in
 * the colour of the gap beside the pages.
 */
export function Edge() {
  // The page's own background (index.css) would cover what lies under it.
  useLayoutEffect(() => {
    for (const el of [document.documentElement, document.body, document.getElementById("root")]) {
      if (el) el.style.background = "transparent";
    }
  }, []);
  // A finger first enters, then presses: it counts once, as a touch.
  const slideOut = (e: React.PointerEvent) => {
    const touch = e.pointerType === "touch";
    if (e.type === "pointerenter" && touch) return;
    send({ cmd: "float_sidebar", show: true, touch });
  };
  return <div className="edge-zone" onPointerEnter={slideOut} onPointerDown={slideOut} />;
}
