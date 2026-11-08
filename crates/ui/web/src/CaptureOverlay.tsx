// Capture overlay (magnetite://ui/capture), shown in a short-lived window over a
// page. It displays the frozen frame: hover picks an element, drag picks a
// region, V takes the visible area, F the whole page, Esc cancels.
import { useCallback, useEffect, useMemo, useState } from "react";
import { send, type CaptureFrame } from "./bridge";

type Box = [number, number, number, number];

const PADDING = 16;

export function CaptureOverlay() {
  const [frame, setFrame] = useState<CaptureFrame | null>(null);
  const [hover, setHover] = useState<Box | null>(null);
  const [start, setStart] = useState<[number, number] | null>(null);
  const [drag, setDrag] = useState<Box | null>(null);
  const [padded, setPadded] = useState(false);

  // Clicking anywhere outside the overlay (sidebar, another app) cancels.
  useEffect(() => {
    const blur = () => send({ cmd: "capture_cancel" });
    window.addEventListener("blur", blur);
    return () => window.removeEventListener("blur", blur);
  }, []);

  useEffect(() => {
    document.title = "Capture";
    send({ cmd: "capture_frame" }).then((f) => (f.image ? setFrame(f) : send({ cmd: "capture_cancel" })));
  }, []);

  const scale = frame ? window.innerWidth / frame.css_width : 1;
  const pad = useCallback(
    (b: Box): Box => {
      if (!padded || !frame) return b;
      const x = Math.max(0, b[0] - PADDING), y = Math.max(0, b[1] - PADDING);
      return [x, y, Math.min(frame.css_width - x, b[2] + 2 * PADDING), Math.min(frame.css_height - y, b[3] + 2 * PADDING)];
    },
    [padded, frame],
  );
  const pick = useCallback((rect: Box) => send({ cmd: "capture_select", rect }), []);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      const k = e.key.toLowerCase();
      if (k === "escape") send({ cmd: "capture_cancel" });
      else if (k === "v") send({ cmd: "capture_visible" });
      else if (k === "f") send({ cmd: "capture_full" });
      else if (k === "p") setPadded((p) => !p);
      else if (k === "enter" && hover) pick(pad(hover));
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [hover, pad, pick]);

  // Smallest element box under the pointer (boxes come largest first).
  const boxAt = useMemo(() => (x: number, y: number): Box | null => {
    if (!frame?.element_ok) return null;
    for (let i = frame.boxes.length - 1; i >= 0; i--) {
      const b = frame.boxes[i]!;
      if (x >= b[0] && y >= b[1] && x <= b[0] + b[2] && y <= b[1] + b[3]) return b;
    }
    return null;
  }, [frame]);

  if (!frame) return <div className="capture-root" />;
  const point = (e: React.MouseEvent): [number, number] => [e.clientX / scale, e.clientY / scale];
  const selection = drag ?? (hover ? pad(hover) : null);

  return (
    <div
      className="capture-root"
      onMouseMove={(e) => {
        const [x, y] = point(e);
        if (start) {
          setDrag([Math.min(start[0], x), Math.min(start[1], y), Math.abs(x - start[0]), Math.abs(y - start[1])]);
        } else {
          setHover(boxAt(x, y));
        }
      }}
      onMouseDown={(e) => { setStart(point(e)); setDrag(null); }}
      onMouseUp={() => {
        if (drag && drag[2] > 4 && drag[3] > 4) pick(drag);
        else if (hover) pick(pad(hover));
        setStart(null);
        setDrag(null);
      }}
    >
      <img src={frame.image} alt="" draggable={false} className="capture-image" />
      {selection && (
        <div className="capture-selection" style={{ left: selection[0] * scale, top: selection[1] * scale, width: selection[2] * scale, height: selection[3] * scale }}>
          <span className="capture-size">{Math.round(selection[2])} × {Math.round(selection[3])}</span>
        </div>
      )}
      <div className="capture-hint" onMouseDown={(e) => e.stopPropagation()} onMouseUp={(e) => e.stopPropagation()}>
        {frame.element_ok ? <span>Click an element</span> : <span title="The page moved while freezing; element picking is off for this capture">Element picking unavailable</span>}
        <span>Drag a region</span>
        <button onClick={() => send({ cmd: "capture_visible" })}><kbd>V</kbd> Visible</button>
        <button onClick={() => send({ cmd: "capture_full" })}><kbd>F</kbd> Full page</button>
        {frame.element_ok && <button onClick={() => setPadded((p) => !p)} aria-pressed={padded}><kbd>P</kbd> Padding {padded ? "on" : "off"}</button>}
        <button onClick={() => send({ cmd: "capture_cancel" })}><kbd>Esc</kbd> Cancel</button>
      </div>
    </div>
  );
}
