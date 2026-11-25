import { useCallback, useEffect, useRef, useState } from "react";

/** Horizontal travel (px of wheel delta) that switches a space. */
const THRESHOLD = 60;
/** Travel (px) after which a gesture counts as horizontal or vertical. */
const AXIS_LOCK = 8;
/**
 * A pause in wheel events this long ends a gesture too. Without gesture
 * phases from the system (Windows, Linux, a plain mouse wheel) Chromium
 * starts the next gesture only 500 ms after the last event.
 */
const IDLE_MS = 160;
/** How long a swipe that switched waits for the new space before the list springs back. */
const SWITCH_WAIT_MS = 1000;

/** One scroll gesture on the sidebar, from where the fingers start. */
type Gesture = { x: number; y: number; axis?: "x" | "y"; done?: boolean };

/**
 * Two-finger horizontal swipe on the sidebar (`ref` goes on its root)
 * switches spaces, one per gesture. Where a gesture begins comes from
 * Chromium: only the first wheel event of a scroll gesture is cancelable,
 * the rest of it and its momentum are not. That needs a listener that is not
 * passive (React's `onWheel` is), and it arrives in order with the other
 * events, so a swipe during the last one's momentum counts at once. The
 * first few pixels decide the axis: a vertical gesture scrolls the list and
 * never switches.
 *
 * `offset`: the list of space `spaceId` goes along a little while swiping.
 * After a switch it stays there until the new space's list replaces it (or
 * springs back if none comes, with a single space); after a swipe that did
 * not switch it springs back. `direction`: where the swipe that just switched
 * went, so the new space slides in from that side.
 */
export function useSpaceSwipe(spaceId: string, onSwipe: (direction: 1 | -1) => void) {
  const gesture = useRef<Gesture>({ x: 0, y: 0 });
  const idle = useRef(0);
  const [nudge, setNudge] = useState<{ space: string; x: number; direction?: 1 | -1 } | null>(null);

  const onWheel = useCallback(
    (e: WheelEvent) => {
      if (e.cancelable) gesture.current = { x: 0, y: 0 };
      const g = gesture.current;
      window.clearTimeout(idle.current);
      idle.current = window.setTimeout(() => {
        gesture.current = { x: 0, y: 0 };
        setNudge((n) => (n?.direction ? n : null));
      }, IDLE_MS);
      g.x += e.deltaX;
      g.y += e.deltaY;
      if (!g.axis) {
        if (Math.max(Math.abs(g.x), Math.abs(g.y)) < AXIS_LOCK) return;
        g.axis = Math.abs(g.x) > Math.abs(g.y) ? "x" : "y";
      }
      if (g.axis === "y" || g.done) return;
      if (Math.abs(g.x) < THRESHOLD) {
        setNudge({ space: spaceId, x: -g.x / 3 });
        return;
      }
      g.done = true;
      const direction = g.x > 0 ? 1 : -1;
      setNudge((n) => ({ space: spaceId, x: n?.x ?? 0, direction }));
      onSwipe(direction);
    },
    [spaceId, onSwipe],
  );

  // A swipe that switched: the new space takes over from its list, else it springs back after a while.
  useEffect(() => setNudge(null), [spaceId]);
  useEffect(() => {
    if (!nudge?.direction) return;
    const timer = window.setTimeout(() => setNudge(null), SWITCH_WAIT_MS);
    return () => window.clearTimeout(timer);
  }, [nudge]);

  const ref = useCallback(
    (el: HTMLElement | null) => {
      if (!el) return;
      el.addEventListener("wheel", onWheel, { passive: false });
      return () => el.removeEventListener("wheel", onWheel);
    },
    [onWheel],
  );

  return { ref, offset: nudge?.space === spaceId ? nudge.x : 0, direction: nudge?.direction };
}
