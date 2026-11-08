import { send } from "./bridge";

// Minimal drag-and-drop state shared between rows. HTML5 DnD with a custom
// payload so drops can be computed per list.

export interface DragPayload {
  kind: "tab" | "folder";
  id: string;
  /** A whole split row being moved (by its row's tab); it only reorders. */
  pair?: boolean;
}

let current: DragPayload | null = null;

export function startDrag(e: React.DragEvent, payload: DragPayload) {
  current = payload;
  e.dataTransfer.effectAllowed = "move";
  e.dataTransfer.setData("text/plain", payload.id);
  // Tabs can also be dropped onto the page area to open in split view.
  if (splittable(payload)) send({ cmd: "split_drag_start", tab: payload.id });
}

export function endDrag() {
  if (splittable(current)) send({ cmd: "split_drag_end" });
  current = null;
}

/** A single tab, which can join a split when dropped (a moved split row cannot). */
export function splittable(d: DragPayload | null): d is DragPayload {
  return d?.kind === "tab" && !d.pair;
}

/** Which half of `el` the pointer is over: the side a dropped tab opens on. */
export function sideOf(e: React.DragEvent, el = e.currentTarget as HTMLElement): "left" | "right" {
  const r = el.getBoundingClientRect();
  return e.clientX - r.left < r.width / 2 ? "left" : "right";
}

export function dragging(): DragPayload | null {
  return current;
}

/** Over the middle of a tab row: drop there to open the two tabs in split view. */
export function overRowMiddle(e: React.DragEvent): boolean {
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  const y = (e.clientY - rect.top) / rect.height;
  return y > 0.3 && y < 0.7;
}

/** Index before/after the hovered row depending on pointer position. */
export function dropIndex(e: React.DragEvent, index: number): number {
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  return e.clientY - rect.top < rect.height / 2 ? index : index + 1;
}

/**
 * The index to send for item `id` dropped at `index` of `list`. The app takes
 * the item out before it puts it there (`move_tab`, `move_folder`), so one
 * that stood in the same list above the drop counts one less.
 */
export function indexAfterTaking(list: readonly { id: string }[], id: string, index: number): number {
  const from = list.findIndex((item) => item.id === id);
  return from >= 0 && from < index ? index - 1 : index;
}
