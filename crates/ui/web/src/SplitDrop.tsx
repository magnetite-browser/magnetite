// Drop zones over the page area (magnetite://ui/splitdrop), shown while a tab is
// dragged from the sidebar. It shows the visible pages as they are; hovering
// a page highlights the half where the dragged tab would open. Dropping opens
// it there in split view. A page of the split itself moves there instead, or
// trades places with the page whose middle it is dropped on.
import { useEffect, useState } from "react";
import { send, type Side, type SplitDropFrame as Frame, type SplitDropPane as Pane } from "./bridge";
import { t } from "./i18n";

/** The half of a pane nearest to the pointer; with `swap`, `null` over its middle. */
function sideAt(p: Pane, x: number, y: number, swap: boolean): Side | null {
  const [px, py, w, h] = p.rect;
  const dx = (x - (px + w / 2)) / w;
  const dy = (y - (py + h / 2)) / h;
  if (swap && Math.abs(dx) < 0.2 && Math.abs(dy) < 0.2) return null;
  return Math.abs(dx) >= Math.abs(dy) ? (dx < 0 ? "left" : "right") : dy < 0 ? "top" : "bottom";
}

function half(p: Pane, side: Side | null): [number, number, number, number] {
  const [x, y, w, h] = p.rect;
  switch (side) {
    case null: return [x, y, w, h];
    case "left": return [x, y, w / 2, h];
    case "right": return [x + w / 2, y, w / 2, h];
    case "top": return [x, y, w, h / 2];
    case "bottom": return [x, y + h / 2, w, h / 2];
  }
}

export function SplitDrop() {
  const [frame, setFrame] = useState<Frame | null>(null);
  const [target, setTarget] = useState<{ pane: Pane; side: Side | null } | null>(null);
  useEffect(() => {
    document.title = t("sidebar.split_drop.window_title");
    send({ cmd: "split_drop_frame" }).then((f) => f.tab && setFrame(f));
  }, []);
  if (!frame) return <div className="splitdrop" />;

  // A tab that is one of the pages shown moves inside the split: beside
  // another page, or into its place. Its own page takes no drop.
  const shown = frame.panes.some((p) => p.tab === frame.tab);
  const at = (e: React.DragEvent) => {
    const pane = frame.panes.find((p) => {
      const [x, y, w, h] = p.rect;
      return e.clientX >= x && e.clientX <= x + w && e.clientY >= y && e.clientY <= y + h;
    });
    return pane && pane.tab !== frame.tab ? { pane, side: sideAt(pane, e.clientX, e.clientY, shown) } : null;
  };

  return (
    <div
      className="splitdrop"
      onDragOver={(e) => {
        if (frame.full) return;
        const over = at(e);
        setTarget(over);
        if (over) { e.preventDefault(); e.dataTransfer.dropEffect = "move"; }
      }}
      onDragLeave={() => setTarget(null)}
      onDrop={(e) => {
        e.preventDefault();
        const drop = at(e);
        if (drop) send({ cmd: "split_drop", target: drop.pane.tab, side: drop.side ?? undefined });
      }}
    >
      {frame.panes.map((p) => (
        <div key={p.tab} className="splitdrop-pane" style={{ left: p.rect[0], top: p.rect[1], width: p.rect[2], height: p.rect[3] }}>
          {p.image && <img src={p.image} alt="" draggable={false} />}
        </div>
      ))}
      {target && (() => {
        const [x, y, w, h] = half(target.pane, target.side);
        const title = frame.title || t("sidebar.split_drop.tab");
        const other = target.pane.title;
        const label = target.side
          ? t(shown ? "sidebar.split_drop.move_here" : "sidebar.split_drop.open_here", { title })
          : other ? t("sidebar.split_drop.swap_with", { title, other }) : t("sidebar.split_drop.swap_with_this_page", { title });
        return (
          <div className="splitdrop-target" style={{ left: x, top: y, width: w, height: h }}>
            <span>{label}</span>
          </div>
        );
      })()}
      {frame.full && <div className="splitdrop-note">{t("sidebar.split_drop.full")}</div>}
    </div>
  );
}
