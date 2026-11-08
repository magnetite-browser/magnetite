import { useEffect, useRef, useState } from "react";
import { send } from "./bridge";

/**
 * For transient sidebar UI (command bar, site panel, menus, pickers): take
 * keyboard focus when it opens (a page window may have had it), and close on
 * Esc anywhere in the sidebar or as soon as the sidebar loses focus, e.g. by
 * a click into the page or a switch to another app. Clicks elsewhere inside
 * the sidebar are each component's own backdrop.
 */
export function useDismiss(onClose: () => void, { takeFocus = true }: { takeFocus?: boolean } = {}) {
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    // The palette window has focus of its own; sidebar pop-ups ask for it.
    if (takeFocus) send({ cmd: "focus_sidebar" });
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close.current();
      }
    };
    const blur = () => close.current();
    window.addEventListener("keydown", key);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("keydown", key);
      window.removeEventListener("blur", blur);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}

/**
 * "Click again to confirm" for buttons told apart by `key`: the first
 * `arm(key, run)` arms that button (show its confirm label while
 * `armed === key`), the second disarms it and calls `run`. An armed button
 * disarms on its own after `resetMs`; without it, it stays armed.
 */
export function useArmed(resetMs?: number) {
  const [armed, setArmed] = useState<string | null>(null);
  const arm = (key: string, run: () => void) => {
    if (armed === key) {
      setArmed(null);
      run();
      return;
    }
    setArmed(key);
    if (resetMs !== undefined) window.setTimeout(() => setArmed((a) => (a === key ? null : a)), resetMs);
  };
  return { armed, arm };
}
