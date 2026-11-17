// The list of all spaces by name (magnetite://ui/command?mode=spaces),
// opened with keys of its own, from the Spaces menu or by the command
// "Show all spaces": type part of a name, Enter goes to the space.
import { useEffect, useRef, useState } from "react";
import { SpaceGlyph } from "./Glyphs";
import { MaskIcon } from "./Icons";
import { IS_MAC, accentCss, send, type Space, type UiState } from "./bridge";
import { useDismiss } from "./dismiss";
import { keysLabel } from "./settings/shortcutKeys";

/**
 * The spaces whose name contains `query`, those first whose name starts with
 * it, then those where a word does; otherwise in the footer's order.
 */
function matching(spaces: Space[], query: string): Space[] {
  const q = query.trim().toLowerCase();
  if (!q) return spaces;
  const rank = (s: Space) => {
    const title = s.title.toLowerCase();
    const at = title.indexOf(q);
    return at < 0 ? null : at === 0 ? 0 : /\s/.test(title.charAt(at - 1)) ? 1 : 2;
  };
  return spaces
    .map((space) => ({ space, rank: rank(space) }))
    .filter((m): m is { space: Space; rank: number } => m.rank !== null)
    .sort((a, b) => a.rank - b.rank)
    .map((m) => m.space);
}

export function SpacePicker({ state, onClose }: { state: UiState; onClose: () => void }) {
  const { spaces, profiles } = state.sidebar;
  const [query, setQuery] = useState("");
  // The place in the list; before anything is typed or moved, the current space.
  const [selected, setSelected] = useState<number | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  // The palette window is focused already; it closes on Esc or when it loses focus.
  useDismiss(onClose, { takeFocus: false });
  useEffect(() => input.current?.focus(), []);

  const found = matching(spaces, query);
  const start = query.trim() ? 0 : Math.max(0, found.findIndex((s) => s.id === state.active_space));
  const sel = Math.min(selected ?? start, Math.max(0, found.length - 1));
  useEffect(() => {
    list.current?.querySelector<HTMLElement>("[data-selected]")?.scrollIntoView({ block: "nearest" });
  }, [sel, query]);

  const go = (space: Space) => {
    if (space.id !== state.active_space) send({ cmd: "switch_space", id: space.id });
    onClose();
  };
  /** The keys that go to `space`: its own, or the digit of its place among the first nine. */
  const keysOf = (space: Space) => {
    const place = spaces.indexOf(space) + 1;
    return state.space_keys[space.id] ?? (place <= 9 ? `${IS_MAC ? "ctrl" : "alt"}+${place}` : null);
  };
  // The profile says something only where the spaces do not all share one.
  const showProfile = new Set(spaces.map((s) => s.profile_id)).size > 1;

  const onKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") { e.preventDefault(); setSelected(Math.min(sel + 1, found.length - 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setSelected(Math.max(sel - 1, 0)); }
    else if (e.key === "Enter") {
      e.preventDefault();
      const space = found[sel];
      if (space) go(space);
    }
  };

  return (
    <div className="cmdbar" role="combobox" aria-expanded={found.length > 0}>
      <div className="flex items-center">
        <input
          ref={input}
          aria-label="Go to space"
          value={query}
          placeholder="Go to space…"
          onChange={(e) => { setQuery(e.target.value); setSelected(null); }}
          onKeyDown={onKey}
          spellCheck={false}
        />
      </div>
      {found.length > 0 && (
        <div ref={list} className="results scroll" role="listbox">
          {found.map((space, i) => {
            const keys = keysOf(space);
            const current = space.id === state.active_space;
            return (
              <div
                key={space.id}
                className="result"
                role="option"
                aria-selected={i === sel}
                data-selected={i === sel || undefined}
                // Only a pointer that really moves takes the selection from the keyboard.
                onMouseMove={(e) => { if (e.movementX !== 0 || e.movementY !== 0) setSelected(i); }}
                onClick={() => go(space)}
              >
                <span className="icon-slot" style={{ color: accentCss(space.accent) }}>
                  {space.id === state.private_space ? <MaskIcon width={16} height={16} /> : <SpaceGlyph icon={space.icon} title={space.title} size={16} />}
                </span>
                <div className="min-w-0 flex-1 truncate">{space.title}</div>
                {(current || showProfile) && (
                  <span className="meta">
                    {[current && "Current", showProfile && profiles.find((p) => p.id === space.profile_id)?.name].filter(Boolean).join(" · ")}
                  </span>
                )}
                {keys && <kbd>{keysLabel(keys)}</kbd>}
              </div>
            );
          })}
        </div>
      )}
      <div className="hints">
        {found.length > 0 ? <span><kbd>{IS_MAC ? "↵" : "Enter"}</kbd> Go to the space</span> : <span>No space with that name</span>}
        <span>Keys for a space: Settings → Shortcuts</span>
      </div>
    </div>
  );
}
