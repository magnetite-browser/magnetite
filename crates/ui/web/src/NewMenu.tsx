// The footer's "+" menu: grouped actions with icons and their shortcuts,
// anchored above the button (which turns into × while open). Arrow keys move,
// Return runs, Esc closes; "New Tab" is the default, nearest the button.
import { useEffect, useRef, useState } from "react";
import { useDismiss } from "./dismiss";
import type { IconComponent } from "./Icons";

export interface NewEntry {
  id: string;
  label: string;
  Icon: IconComponent;
  shortcut?: string;
  disabled?: boolean;
  run: () => void;
}

export function NewMenu({ groups, onClose }: { groups: NewEntry[][]; onClose: () => void }) {
  const entries = groups.flat().filter((e) => !e.disabled);
  const [active, setActive] = useState(entries.length - 1);
  const box = useRef<HTMLDivElement>(null);
  useDismiss(onClose);
  useEffect(() => {
    box.current?.focus();
    const close = (e: MouseEvent) => { if (!box.current?.contains(e.target as Node)) onClose(); };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [onClose]);
  const run = (e: NewEntry) => { onClose(); e.run(); };
  return (
    <div
      ref={box}
      className="new-menu no-drag"
      role="menu"
      tabIndex={-1}
      aria-label="New"
      onKeyDown={(e) => {
        if (e.key === "ArrowDown") setActive((a) => (a + 1) % entries.length);
        else if (e.key === "ArrowUp") setActive((a) => (a - 1 + entries.length) % entries.length);
        else if (e.key === "Enter" && entries[active]) run(entries[active]);
        else return;
        e.preventDefault();
      }}
    >
      {groups.map((group, gi) => (
        <div key={gi} className="new-group">
          {group.map((e) => {
            const index = entries.indexOf(e);
            return (
              <button
                key={e.id}
                role="menuitem"
                className="new-item"
                disabled={e.disabled}
                data-active={index === active || undefined}
                onMouseEnter={() => index >= 0 && setActive(index)}
                onClick={() => run(e)}
              >
                <e.Icon width={18} height={18} />
                <span>{e.label}</span>
                {e.shortcut && <kbd>{e.shortcut}</kbd>}
              </button>
            );
          })}
        </div>
      ))}
    </div>
  );
}
