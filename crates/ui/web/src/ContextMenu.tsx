import { useEffect, useState } from "react";
import { pageZoom } from "./bridge";

/** A line between groups, or an action. */
export type MenuItem =
  | { separator: true }
  | {
      separator?: false;
      label: string;
      /** Shown on hover in place of the label (which may end in "…"). */
      title?: string;
      onClick: () => void;
      danger?: boolean;
      /** Ask first: the first click shows this label in place, the second runs `onClick`. */
      confirm?: string;
    };

/** The line between groups of a menu. */
export const SEPARATOR: MenuItem = { separator: true };

export function ContextMenu({ x, y, items, onClose }: { x: number; y: number; items: MenuItem[]; onClose: () => void }) {
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
            title={it.title ?? it.label}
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
            <span>{armed === i ? it.confirm : it.label}</span>
          </button>
        ),
      )}
    </div>
  );
}
