import { useEffect, useState, type ReactNode } from "react";
import { pageZoom, send } from "./bridge";
import { Back, Forward } from "./Icons";

/** What every row shows. */
interface Row {
  label: string;
  /** A small picture before the label, 16 px. */
  icon?: ReactNode;
  /** Shown on hover in place of the label (which may end in "…"). */
  title?: string;
}

/** A line between groups, an action, or a list of its own. */
export type MenuItem =
  | { separator: true }
  | (Row & {
      separator?: false;
      /** A short note at the right edge, such as a time. */
      hint?: string;
      onClick: () => void;
      danger?: boolean;
      /** Ask first: the first click shows this label in place, the second runs `onClick`. */
      confirm?: string;
    })
  /**
   * Opens `items` in the menu's place, below a row that leads back: a long
   * list (one row per space) stays out of the menu, and a menu in the sidebar
   * has no room beside it for one that flies out.
   */
  | (Row & { separator?: false; items: MenuItem[] });

/** The line between groups of a menu. */
export const SEPARATOR: MenuItem = { separator: true };

/**
 * An opened list shows ten rows and half of the next, which tells that it
 * scrolls (macOS shows no scrollbar until then); the row back stays above it.
 */
const LIST_HEIGHT = 10.5 * 28;

/** The height of `items`: 28 px a row, 9 a line (see `.menu` in index.css). */
function rowsHeight(items: MenuItem[]): number {
  return items.reduce((h, it) => h + (it.separator ? 9 : 28), 0);
}

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
  // The list opened from a row of the menu, if one is.
  const [open, setOpen] = useState<{ label: string; items: MenuItem[] } | null>(null);
  // `x`, `y` and the window in the menu's own px (see `pageZoom`), so it
  // opens at the pointer and stays inside at every text size; taller than
  // the window, it starts at its top and scrolls.
  const zoom = pageZoom();
  const room = window.innerHeight / zoom - 16;
  // The padding, then the rows; an opened list has its row back and line above.
  const height = Math.min(room, 8 + (open ? 28 + 9 + Math.min(LIST_HEIGHT, rowsHeight(open.items)) : rowsHeight(items)));
  const left = Math.max(0, Math.min(x / zoom, window.innerWidth / zoom - 208));
  const top = Math.max(8, Math.min(y / zoom, window.innerHeight / zoom - height - 8));
  const showList = (list: { label: string; items: MenuItem[] } | null) => {
    setArmed(null);
    setOpen(list);
  };
  const row = (it: MenuItem, i: number) =>
    it.separator ? (
      <hr key={i} />
    ) : "items" in it ? (
      <button key={i} title={it.title ?? it.label} onClick={() => showList({ label: it.label, items: it.items })}>
        {it.icon}
        <span>{it.label}</span>
        <Forward className="hint" />
      </button>
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
        {it.icon}
        <span>{armed === i ? it.confirm : it.label}</span>
        {it.hint && <span className="hint">{it.hint}</span>}
      </button>
    );
  return (
    // Keyed by what it shows, so a menu scrolled in a short window starts at its top again.
    <div key={open ? "open" : "menu"} className="menu scroll no-drag" style={{ left, top, ["--menu-room" as string]: `${room}px` }} onMouseDown={(e) => e.stopPropagation()}>
      {open ? (
        <>
          <button className="menu-back" title={open.label} onClick={() => showList(null)}>
            <Back />
            <span>{open.label}</span>
          </button>
          <hr />
          <div className="scroll" style={{ maxHeight: LIST_HEIGHT }}>
            {open.items.map(row)}
          </div>
        </>
      ) : (
        items.map(row)
      )}
    </div>
  );
}
