// Drop zones over the page area (magnetite://ui/splitdrop), shown while a tab is
// dragged from the sidebar. It shows the visible pages as they are; hovering
// a page highlights the half where the dragged tab would open. Dropping opens
// it there in split view.
import { useEffect, useState } from "react";
import { send, type Side, type SplitDropFrame as Frame, type SplitDropPane as Pane } from "./bridge";

/** The half of a pane nearest to the pointer. */
function sideAt(p: Pane, x: number, y: number): Side {
  const [px, py, w, h] = p.rect;
  const dx = (x - (px + w / 2)) / w;
  const dy = (y - (py + h / 2)) / h;
  return Math.abs(dx) >= Math.abs(dy) ? (dx < 0 ? "left" : "right") : dy < 0 ? "top" : "bottom";
}

function half(p: Pane, side: Side): [number, number, number, number] {
  const [x, y, w, h] = p.rect;
  switch (side) {
    case "left": return [x, y, w / 2, h];
    case "right": return [x + w / 2, y, w / 2, h];
    case "top": return [x, y, w, h / 2];
    case "bottom": return [x, y + h / 2, w, h / 2];
  }
}

export function SplitDrop() {
  const [frame, setFrame] = useState<Frame | null>(null);
  const [target, setTarget] = useState<{ pane: Pane; side: Side } | null>(null);
  useEffect(() => {
    document.title = "Split";
    send({ cmd: "split_drop_frame" }).then((f) => f.tab && setFrame(f));
  }, []);
  if (!frame) return <div className="splitdrop" />;

  // A tab that is one of the pages shown stays where it is: the app does not
  // move a page inside its split, so no half lights up for it.
  const shown = frame.panes.some((p) => p.tab === frame.tab);
  const at = (e: React.DragEvent) => {
    if (shown) return null;
    const pane = frame.panes.find((p) => {
      const [x, y, w, h] = p.rect;
      return e.clientX >= x && e.clientX <= x + w && e.clientY >= y && e.clientY <= y + h;
    });
    return pane ? { pane, side: sideAt(pane, e.clientX, e.clientY) } : null;
  };

  return (
    <div
      className="splitdrop"
      onDragOver={(e) => {
        if (frame.full) return;
        const t = at(e);
        setTarget(t);
        if (t) { e.preventDefault(); e.dataTransfer.dropEffect = "move"; }
      }}
      onDragLeave={() => setTarget(null)}
      onDrop={(e) => {
        e.preventDefault();
        const t = at(e);
        if (t) send({ cmd: "split_drop", target: t.pane.tab, side: t.side });
      }}
    >
      {frame.panes.map((p) => (
        <div key={p.tab} className="splitdrop-pane" style={{ left: p.rect[0], top: p.rect[1], width: p.rect[2], height: p.rect[3] }}>
          {p.image && <img src={p.image} alt="" draggable={false} />}
        </div>
      ))}
      {target && (() => {
        const [x, y, w, h] = half(target.pane, target.side);
        return (
          <div className="splitdrop-target" style={{ left: x, top: y, width: w, height: h }}>
            <span>Open {frame.title || "tab"} here</span>
          </div>
        );
      })()}
      {frame.full && <div className="splitdrop-note">Split view holds up to four pages</div>}
    </div>
  );
}
