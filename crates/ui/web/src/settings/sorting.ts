// Sorting the rows of a list in Settings by dragging one. The pointer does
// it, not the browser's drag and drop: during one of those the cursor is the
// system's arrow, whatever the page asks for, and it stays that after the
// drop until the pointer moves.
import { useEffect, useRef, useState } from "react";

type Over = { id: string; below: boolean };

type Drag = {
  id: string;
  pointer: number;
  /** What the pointer went down on, which keeps the pointer while it drags. */
  handle: HTMLElement;
  row: HTMLElement;
  /** What scrolls the list (the page's `main`). */
  scroller: HTMLElement | null;
  startY: number;
  startScroll: number;
  y: number;
  /** Past the few points that tell a drag from a click. */
  active: boolean;
  /** The rows as they lay when the drag began, in the page's own measure: the drag moves them about. */
  slots: { id: string; top: number; height: number }[];
  frame: number;
};

/** How far the pointer moves before a press is a drag. */
const SLACK = 4;
/** How near the edge of what scrolls the list a drag scrolls it. */
const EDGE = 48;

/**
 * A list whose rows are sorted by dragging. `landing` says where row
 * `dragged` ends up when dropped on the upper or lower half of row
 * `target` (null: nowhere new, or not allowed there), `move` does it and
 * settles once the list has its new order: until then the rows stay where
 * the drag left them, or the old order would show for a moment.
 *
 * `row(id)` goes on every row, `handle(id, enabled)` on what a row is
 * dragged by: the row itself or a part of it. The dragged row follows the
 * pointer, the other rows make room where it would land, the page scrolls
 * when the pointer nears its edge, and Esc gives up.
 */
export function useSorting(landing: (dragged: string, target: string, below: boolean) => number | null, move: (id: string, index: number) => Promise<unknown>) {
  const [dragged, setDragged] = useState<string | null>(null);
  const [over, setOver] = useState<Over | null>(null);
  const [shift, setShift] = useState(0);
  // The rows in their order and the height of the dragged one, while one is dragged.
  const [lay, setLay] = useState<{ order: string[]; height: number } | null>(null);
  const drag = useRef<Drag | null>(null);
  const overNow = useRef<Over | null>(null);
  // The click that ends a drag is not one on the row.
  const dropped = useRef(false);
  const latest = useRef({ landing, move });
  latest.current = { landing, move };

  /** The rows back where the list puts them. */
  const settle = () => {
    setDragged(null);
    setOver(null);
    setShift(0);
    setLay(null);
  };
  /** Lets go of the pointer; `keep` leaves the rows where they are for now. */
  const finish = (keep = false) => {
    const d = drag.current;
    if (!d) return;
    drag.current = null;
    cancelAnimationFrame(d.frame);
    if (d.handle.hasPointerCapture(d.pointer)) d.handle.releasePointerCapture(d.pointer);
    delete document.documentElement.dataset.sorting;
    overNow.current = null;
    if (!keep) settle();
  };
  const finishNow = useRef(finish);
  finishNow.current = finish;

  /** Where the dragged row is now, and the row it would land at. */
  const track = () => {
    const d = drag.current;
    if (!d?.active) return;
    const scrolled = d.scroller?.scrollTop ?? 0;
    setShift(d.y - d.startY + (scrolled - d.startScroll));
    // Against where the rows lay at first: they have moved to make room.
    const y = d.y + scrolled;
    const rows = d.slots.filter((r) => r.id !== d.id);
    let next: Over | null = null;
    for (const [i, r] of rows.entries()) {
      // Above the first row counts as on it, below the last one as on that.
      if (y < r.top + r.height || i === rows.length - 1) {
        const below = y > r.top + r.height / 2;
        if (latest.current.landing(d.id, r.id, below) !== null) next = { id: r.id, below };
        break;
      }
    }
    overNow.current = next;
    setOver((now) => (now?.id === next?.id && now?.below === next?.below ? now : next));
  };

  /** Scrolls while the pointer is near the top or the bottom of what scrolls. */
  const scroll = () => {
    const d = drag.current;
    if (!d?.active) return;
    if (d.scroller) {
      const box = d.scroller.getBoundingClientRect();
      const by = d.y < box.top + EDGE ? -Math.ceil((box.top + EDGE - d.y) / 4) : d.y > box.bottom - EDGE ? Math.ceil((d.y - (box.bottom - EDGE)) / 4) : 0;
      if (by) {
        const before = d.scroller.scrollTop;
        d.scroller.scrollTop += by;
        if (d.scroller.scrollTop !== before) track();
      }
    }
    d.frame = requestAnimationFrame(scroll);
  };

  useEffect(() => {
    const key = (e: KeyboardEvent) => e.key === "Escape" && finishNow.current();
    window.addEventListener("keydown", key);
    return () => {
      window.removeEventListener("keydown", key);
      finishNow.current();
    };
  }, []);

  const handle = (id: string, enabled: boolean) => ({
    onPointerDown: (e: React.PointerEvent<HTMLElement>) => {
      // A press that never became a drag is over, wherever it was let go.
      if (!enabled || e.button !== 0 || drag.current?.active) return;
      // A field in the row is typed into and chosen from, not dragged by.
      if ((e.target as HTMLElement).closest("input, select, textarea")) return;
      const row = e.currentTarget.closest<HTMLElement>("[data-sort-row]");
      if (!row) return;
      const scroller = row.closest<HTMLElement>("main");
      drag.current = { id, pointer: e.pointerId, handle: e.currentTarget, row, scroller, startY: e.clientY, startScroll: scroller?.scrollTop ?? 0, y: e.clientY, active: false, frame: 0, slots: [] };
      // Until the press is a drag the handle does not keep the pointer, so
      // a release elsewhere (the pointer left it sideways) is heard here.
      const pressed = drag.current;
      const over = () => {
        window.removeEventListener("pointerup", over, true);
        window.removeEventListener("pointercancel", over, true);
        if (drag.current === pressed && !pressed.active) drag.current = null;
      };
      window.addEventListener("pointerup", over, true);
      window.addEventListener("pointercancel", over, true);
    },
    onPointerMove: (e: React.PointerEvent<HTMLElement>) => {
      const d = drag.current;
      if (!d || d.pointer !== e.pointerId) return;
      d.y = e.clientY;
      if (!d.active) {
        if (Math.abs(e.clientY - d.startY) < SLACK) return;
        d.active = true;
        const scrolled = d.scroller?.scrollTop ?? 0;
        d.slots = [...(d.row.parentElement?.querySelectorAll<HTMLElement>("[data-sort-row]") ?? [])].map((r) => {
          const box = r.getBoundingClientRect();
          return { id: r.dataset.sortRow!, top: box.top + scrolled, height: box.height };
        });
        setLay({ order: d.slots.map((r) => r.id), height: d.row.getBoundingClientRect().height });
        d.handle.setPointerCapture(d.pointer);
        document.documentElement.dataset.sorting = "";
        getSelection()?.removeAllRanges();
        setDragged(d.id);
        d.frame = requestAnimationFrame(scroll);
      }
      track();
    },
    onPointerUp: (e: React.PointerEvent<HTMLElement>) => {
      const d = drag.current;
      if (!d || d.pointer !== e.pointerId) return;
      if (d.active) {
        const to = overNow.current;
        const index = to ? latest.current.landing(d.id, to.id, to.below) : null;
        dropped.current = true;
        window.setTimeout(() => (dropped.current = false), 0);
        finish(index !== null);
        // The rows settle with the new order, unless another drag began meanwhile.
        if (index !== null) void latest.current.move(d.id, index).finally(() => drag.current?.active || settle());
      } else {
        drag.current = null;
      }
    },
    onPointerCancel: () => finish(),
    onLostPointerCapture: () => finish(),
    onClickCapture: (e: React.MouseEvent) => {
      if (!dropped.current) return;
      e.preventDefault();
      e.stopPropagation();
    },
  });

  /** How far row `id` steps aside for the dragged one: the rows between where it was and where it would land. */
  const aside = (id: string) => {
    if (!lay || !dragged || !over) return 0;
    const from = lay.order.indexOf(dragged);
    const at = lay.order.indexOf(id);
    const target = lay.order.indexOf(over.id);
    if (from < 0 || at < 0 || target < 0) return 0;
    // The last row that moves up, or the first that moves down.
    if (target > from) return at > from && at <= (over.below ? target : target - 1) ? -lay.height : 0;
    return at < from && at >= (over.below ? target + 1 : target) ? lay.height : 0;
  };

  const row = (id: string) => ({
    "data-sort-row": id,
    "data-dragged": dragged === id || undefined,
    style: dragged === id ? { transform: `translateY(${shift}px)` } : aside(id) ? { transform: `translateY(${aside(id)}px)` } : undefined,
  });

  return { row, handle };
}
