import { useState } from "react";
import { SpaceGlyph } from "./Glyphs";
import { accentCss, send, type Space } from "./bridge";
import { indexAfterTaking, sideOf } from "./dnd";

// The space being dragged. Kept apart from the tabs' and folders' drag state
// (`dnd.ts`), so the lists above do not take a space for one of their rows.
let dragged: string | null = null;

/** The footer's space icons: click to switch, right-click for the menu, drag to reorder. */
export function SpaceSwitcher({ spaces, current, onMenu }: { spaces: Space[]; current: string; onMenu: (x: number, y: number, space: Space) => void }) {
  // The icon a dragged space hovers, and the side of it the space would go to.
  const [over, setOver] = useState<{ id: string; side: "left" | "right" } | null>(null);
  // Where the dragged space goes when dropped on that side of icon `i`, or
  // `null` where it is already.
  const target = (i: number, side: "left" | "right"): number | null => {
    if (!dragged) return null;
    const to = indexAfterTaking(spaces, dragged, side === "left" ? i : i + 1);
    return spaces[to]?.id === dragged ? null : to;
  };

  return (
    <div className="scroll flex min-w-0 flex-1 items-center justify-center gap-0.5 overflow-x-auto px-0.5" style={{ overflowY: "hidden" }}>
      {spaces.map((s, i) => (
        <button
          key={s.id}
          className="sp"
          data-active={s.id === current || undefined}
          data-before={(over?.id === s.id && over.side === "left") || undefined}
          data-after={(over?.id === s.id && over.side === "right") || undefined}
          style={{ ["--space" as string]: accentCss(s.accent) }}
          draggable={spaces.length > 1}
          onDragStart={(e) => {
            dragged = s.id;
            e.dataTransfer.effectAllowed = "move";
            e.dataTransfer.setData("text/plain", s.title);
          }}
          onDragEnd={() => {
            dragged = null;
            setOver(null);
          }}
          onDragOver={(e) => {
            const side = sideOf(e);
            if (target(i, side) === null) return setOver(null);
            e.preventDefault();
            if (over?.id !== s.id || over.side !== side) setOver({ id: s.id, side });
          }}
          onDragLeave={() => setOver(null)}
          onDrop={(e) => {
            e.preventDefault();
            setOver(null);
            const index = target(i, sideOf(e));
            if (dragged && index !== null) send({ cmd: "move_space", id: dragged, index });
            dragged = null;
          }}
          onClick={() => send({ cmd: "switch_space", id: s.id })}
          onContextMenu={(e) => { e.preventDefault(); onMenu(e.clientX, e.clientY, s); }}
          title={s.title}
        >
          <SpaceGlyph icon={s.icon} title={s.title} size={17} />
        </button>
      ))}
    </div>
  );
}
