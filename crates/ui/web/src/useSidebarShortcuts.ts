import { useEffect } from "react";
import { send } from "./bridge";

/**
 * Esc while the sidebar has the keyboard: it calls `onEscape`, and the
 * floating sidebar (`float`) also hides unless a menu or pop-up is open.
 * The shortcuts themselves are the app's: it takes their keys before the
 * page sees them, here as in a tab (`crates/engine/src/keys.rs`).
 */
export function useSidebarShortcuts(float: boolean, onEscape: () => void) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      onEscape();
      if (float && !document.querySelector(".menu, .cmdbar-backdrop, .new-menu, .site-panel")) send({ cmd: "float_sidebar", show: false });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [float, onEscape]);
}
