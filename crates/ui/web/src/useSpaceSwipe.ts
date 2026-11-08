import { useCallback, useRef, useState, type WheelEvent } from "react";

/** Horizontal travel (px of wheel delta) that switches a space. */
const THRESHOLD = 60;

/**
 * Two-finger horizontal swipe on the sidebar switches spaces. A trackpad
 * gesture arrives as a burst of wheel events followed by momentum events, so
 * one gesture switches at most one space; the gesture ends after a short
 * pause in events. `offset` gives a little rubber-band feedback while
 * swiping.
 */
export function useSpaceSwipe(onSwipe: (direction: 1 | -1) => void) {
  const travel = useRef(0);
  const fired = useRef(false);
  const idle = useRef(0);
  const [offset, setOffset] = useState(0);

  const onWheel = useCallback(
    (e: WheelEvent) => {
      if (Math.abs(e.deltaX) <= Math.abs(e.deltaY)) return;
      window.clearTimeout(idle.current);
      idle.current = window.setTimeout(() => {
        travel.current = 0;
        fired.current = false;
        setOffset(0);
      }, 160);
      if (fired.current) return;
      travel.current += e.deltaX;
      if (Math.abs(travel.current) >= THRESHOLD) {
        fired.current = true;
        setOffset(0);
        onSwipe(travel.current > 0 ? 1 : -1);
      } else {
        setOffset(-travel.current / 3);
      }
    },
    [onSwipe],
  );

  return { onWheel, offset };
}
