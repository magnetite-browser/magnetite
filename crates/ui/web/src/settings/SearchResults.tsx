// Settings search: every section on one page, cut down to the settings that
// match what was typed. The sections are rendered as they are and filtered
// by what they show, so a new setting is found without being listed anywhere.
import { useLayoutEffect, useRef, useState } from "react";
import type { IconComponent } from "../Icons";

export type SearchSection = { id: string; title: string; Icon: IconComponent };

/**
 * Hides what does not match `query` below `root` and returns how many
 * settings are left. A setting is a card (`.set-card`), or a part of a
 * section outside any card. It matches by its name and description, every
 * typed word somewhere in them; a section whose title matches shows whole,
 * ahead of the others.
 * In a card of rows (`data-search-row`, the shortcuts) only the matching
 * rows stay, unless the card's own name matches.
 */
function filter(root: HTMLElement, query: string): number {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const has = (text: string | null | undefined) => {
    const lower = (text ?? "").toLowerCase();
    return words.every((w) => lower.includes(w));
  };
  const show = (el: Element, on: boolean) => el.toggleAttribute("data-miss", !on);
  let total = 0;
  for (const section of root.querySelectorAll<HTMLElement>("[data-search-section]")) {
    const whole = has(section.dataset.searchSection);
    section.style.order = whole ? "-1" : "";
    let found = 0;
    for (const child of section.querySelector("[data-search-body]")?.children ?? []) {
      const cards = child.matches(".set-card") ? [child] : [...child.querySelectorAll(".set-card")].filter((c) => !c.parentElement?.closest(".set-card"));
      const units = cards.length > 0 ? cards : [child];
      let hits = 0;
      for (const unit of units) {
        const named = [...unit.querySelectorAll(".set-label, .set-hint")];
        const own = whole || has(named.length > 0 ? named.map((n) => n.textContent).join(" ") : unit.textContent);
        let rows = 0;
        for (const row of unit.querySelectorAll("[data-search-row]")) {
          const hit = own || has(row.textContent);
          show(row, hit);
          if (hit) rows++;
        }
        const hit = own || rows > 0;
        show(unit, hit);
        if (hit) hits++;
      }
      // A wrapper around cards goes when none of its cards is left.
      if (units[0] !== child) show(child, hits > 0);
      found += hits;
    }
    show(section, found > 0);
    total += found;
  }
  return total;
}

/**
 * The page while a search is typed: each section with a match under its
 * name, which leads to the section itself. `body` draws a section's settings.
 */
export function SearchResults({ query, sections, body, open }: { query: string; sections: SearchSection[]; body: (id: string) => React.ReactNode; open: (id: string) => void }) {
  const root = useRef<HTMLDivElement>(null);
  const [none, setNone] = useState(false);
  // Sections change after they are drawn (lists that load, a switch that
  // brings more settings): filtered again whenever they do.
  useLayoutEffect(() => {
    const el = root.current;
    if (!el) return;
    const apply = () => setNone(filter(el, query) === 0);
    apply();
    // Other words, other settings: back to the top of the page.
    el.closest("main")?.scrollTo(0, 0);
    const changes = new MutationObserver(apply);
    changes.observe(el, { childList: true, subtree: true, characterData: true });
    return () => changes.disconnect();
  }, [query]);
  return (
    <>
      <div ref={root} className="flex flex-col gap-8">
        {sections.map((s) => (
          <section key={s.id} data-search-section={s.title}>
            <button className="set-found" onClick={() => open(s.id)} title={`Open ${s.title}`}>
              <s.Icon width={15} height={15} />
              {s.title}
            </button>
            <div className="space-y-3" data-search-body>{body(s.id)}</div>
          </section>
        ))}
      </div>
      {none && <p className="text-[14px] text-[var(--muted)]">No setting matches “{query.trim()}”.</p>}
    </>
  );
}
