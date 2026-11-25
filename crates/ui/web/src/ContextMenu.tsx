import { useEffect, useState, type ReactNode } from "react";
import { pageZoom, send } from "./bridge";

/** A line between groups, or an action. */
export type MenuItem =
  | { separator: true }
  | {
      separator?: false;
      label: string;
      /** A small picture before the label, 16 px. */
      icon?: ReactNode;
      /** Shown on hover in place of the label (which may end in "…"). */
      title?: string;
      /** A short note at the right edge, such as a time. */
      hint?: string;
      /** A name in a box right after the label, such as the profile a space uses. */
      tag?: string;
      onClick: () => void;
      danger?: boolean;
      /** Ask first: the first click shows this label in place, the second runs `onClick`. */
      confirm?: string;
    };

/** The line between groups of a menu. */
export const SEPARATOR: MenuItem = { separator: true };

/**
 * Closes on a pick, a key, a click beside it or when its page loses the
 * keyboard (a click into a web page, another app). `takeFocus`: ask for the
 * keyboard on opening, as a right click gives none and without it there is
 * nothing to lose; only for menus in the sidebar, a page or a window of its
 * own has it.
 */
export function ContextMenu({ x, y, items, onClose, takeFocus = false }: { x: number; y: number; items: MenuItem[]; onClose: () => void; takeFocus?: boolean }) {
  useEffect(() => {
    if (takeFocus) send({ cmd: "focus_sidebar" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    const close = () => onClose();
    window.addEventListener("mousedown", close);
    window.addEventListener("blur", close);
    window.addEventListener("keydown", close);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("blur", close);
      window.removeEventListener("keydown", close);
    };
  }, [onClose]);
  const [armed, setArmed] = useState<number | null>(null);
  const height = items.reduce((h, it) => h + (it.separator ? 9 : 28), 8);
  // `x`, `y` and the window in the menu's own px (see `pageZoom`), so it
  // opens at the pointer and stays inside at every text size.
  const zoom = pageZoom();
  const left = Math.max(0, Math.min(x / zoom, window.innerWidth / zoom - 208));
  const top = Math.min(y / zoom, window.innerHeight / zoom - height - 8);
  return (
    <div className="menu no-drag" style={{ left, top }} onMouseDown={(e) => e.stopPropagation()}>
      {items.map((it, i) =>
        it.separator ? (
          <hr key={i} />
        ) : (
          <button
            key={i}
            title={it.title ?? (it.tag ? `${it.label} · ${it.tag}` : it.label)}
            data-danger={it.danger || undefined}
            onClick={() => {
              if (it.confirm && armed !== i) {
                setArmed(i);
                return;
              }
              it.onClick();
              onClose();
            }}
          >
            {it.icon}
            <span>{armed === i ? it.confirm : it.label}</span>
            {it.tag && <span className="tag">{it.tag}</span>}
            {it.hint && <span className="hint">{it.hint}</span>}
          </button>
        ),
      )}
    </div>
  );
}
