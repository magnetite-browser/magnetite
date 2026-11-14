import { useEffect } from "react";
import { IS_MAC, send } from "./bridge";

/**
 * Keyboard shortcuts while the sidebar has the keyboard; most go to the app
 * as `shortcut` commands. Esc calls `onEscape`, and the floating sidebar
 * (`float`) also hides unless a menu or pop-up is open.
 */
export function useSidebarShortcuts(float: boolean, onEscape: () => void) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const k = e.key.toLowerCase();
      // Exact modifier match: Cmd+D must not fire on Cmd+Option+D (or Cmd+Ctrl+D).
      type Mods = { shift?: boolean; alt?: boolean; ctrl?: boolean; meta?: boolean };
      const is = (key: string, m: Mods) =>
        k === key.toLowerCase() && e.metaKey === !!m.meta && e.shiftKey === !!m.shift && e.altKey === !!m.alt && e.ctrlKey === !!m.ctrl;
      const fire = (shortcut: string | { select_tab: number } | { select_space: number }) => {
        e.preventDefault();
        send({ cmd: "shortcut", shortcut });
      };
      // The main modifier: Cmd on macOS, Ctrl elsewhere, as the app's own
      // key handling has it (keys.rs) and as the labels say.
      const cmd: Mods = IS_MAC ? { meta: true } : { ctrl: true };
      const main = IS_MAC ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
      if (is("t", cmd)) return fire("new_tab");
      if (is("l", cmd)) return fire("focus_address");
      if (is("w", cmd)) return fire("close_tab");
      if (is("s", cmd)) return fire("toggle_sidebar");
      if (is("d", cmd)) return fire("toggle_pin");
      if (is("p", cmd)) return fire("print");
      if (is("y", cmd)) return fire("history");
      if (is("t", { ...cmd, shift: true })) return fire("reopen_closed");
      if ((e.key === "+" || e.key === "=") && main && e.shiftKey && !e.altKey) {
        e.preventDefault();
        send({ cmd: "open_palette", mode: "split", side: "right" });
        return;
      }
      if (is("n", { ...cmd, shift: true })) return fire("new_space");
      if (is("ArrowRight", { ...cmd, alt: true })) return fire("next_space");
      if (is("ArrowLeft", { ...cmd, alt: true })) return fire("prev_space");
      if (is("Tab", { ctrl: true })) return fire("next_tab");
      if (is("Tab", { ctrl: true, shift: true })) return fire("prev_tab");
      if (/^[1-9]$/.test(e.key) && is(e.key, cmd)) return fire({ select_tab: Number(e.key) });
      if (/^[1-9]$/.test(e.key) && is(e.key, IS_MAC ? { ctrl: true } : { alt: true })) return fire({ select_space: Number(e.key) });
      if (is(",", cmd)) {
        e.preventDefault();
        send({ cmd: "new_tab", url: "magnetite://ui/settings" });
        return;
      }
      if (e.key === "Escape") {
        onEscape();
        if (float && !document.querySelector(".menu, .cmdbar-backdrop, .new-menu, .site-panel")) send({ cmd: "float_sidebar", show: false });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [float, onEscape]);
}
