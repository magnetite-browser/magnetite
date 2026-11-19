import { send } from "./bridge";

// Minimal drag-and-drop state shared between rows. HTML5 DnD with a custom
// payload so drops can be computed per list.

export interface DragPayload {
  kind: "tab" | "folder";
  id: string;
  /** A whole split being moved, by its row or its favorites tile (`id`: the tab that stands for it). */
  pair?: boolean;
  /** The pages of that split that move with it (`splitGroup`). */
  group?: string[];
  /** The page of that split row the drag started on: it can move inside the split. */
  cell?: string;
}

/** The tab a drag can put into, or move inside, a split. */
function splitTab(d: DragPayload | null): string | null {
  return d?.kind === "tab" ? (d.pair ? d.cell ?? null : d.id) : null;
}

let current: DragPayload | null = null;

export function startDrag(e: React.DragEvent, payload: DragPayload) {
  current = payload;
  e.dataTransfer.effectAllowed = "move";
  e.dataTransfer.setData("text/plain", payload.id);
  // Tabs can also be dropped onto the page area to open in split view.
  const tab = splitTab(payload);
  if (tab) send({ cmd: "split_drag_start", tab });
}

export function endDrag() {
  if (splitTab(current)) send({ cmd: "split_drag_end" });
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

/**
 * Where over a folder's row the pointer is: its edges place a dragged folder
 * before or after it, the middle puts it inside.
 */
export function rowZone(e: React.DragEvent): "before" | "inside" | "after" {
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  const y = (e.clientY - rect.top) / rect.height;
  return y < 0.3 ? "before" : y > 0.7 ? "after" : "inside";
}

/** Index before/after the hovered row depending on pointer position. */
export function dropIndex(e: React.DragEvent, index: number): number {
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  return e.clientY - rect.top < rect.height / 2 ? index : index + 1;
}

/** The items a drag moves: the item itself, or the pages of a split that go with it. */
export function movedIds(d: DragPayload): string[] {
  return d.group ?? [d.id];
}

/**
 * The index to send for the items `moved`, dropped at `index` of `list`.
 * The app takes what moves out before it puts it there (`move_tab`,
 * `move_folder`, `move_space`), so each moved item that stood in the same
 * list above the drop counts one less.
 */
export function indexAfterTaking(list: readonly { id: string }[], moved: readonly string[], index: number): number {
  return index - list.slice(0, index).filter((item) => moved.includes(item.id)).length;
}
