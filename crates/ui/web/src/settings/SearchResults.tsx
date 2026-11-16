// Settings search: the sections that hold a match on one page, the settings
// that do not match greyed out. The sections are rendered as they are and
// marked by what they show, so a new setting is found without being listed
// anywhere.
import { useLayoutEffect, useRef } from "react";
import type { IconComponent } from "../Icons";

export type SearchSection = { id: string; title: string; Icon: IconComponent };

/**
 * Marks what `query` finds below `root` and returns the ids of the sections
 * with a match. A setting is a card (`.set-card`), or a part of a section
 * outside any card. It matches by its name and description, every typed
 * word somewhere in them; the others are greyed out (`data-dim`), and a
 * section without any match is left out (`data-miss`). A section whose
 * title matches comes first, with nothing greyed out. In a card of rows
 * (`data-search-row`: the shortcuts, the spaces of sync) the rows are
 * marked one by one, by their name (`data-search-name`, or all their text),
 * unless the card's own name matches.
 */
function filter(root: HTMLElement, query: string): string[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const has = (text: string | null | undefined) => {
    const lower = (text ?? "").toLowerCase();
    return words.every((w) => lower.includes(w));
  };
  const light = (el: Element, on: boolean) => el.toggleAttribute("data-dim", !on);
  const found: string[] = [];
  for (const section of root.querySelectorAll<HTMLElement>("[data-search-section]")) {
    const whole = has(section.dataset.searchSection);
    section.style.order = whole ? "-1" : "";
    let hits = 0;
    for (const child of section.querySelector("[data-search-body]")?.children ?? []) {
      const cards = child.matches(".set-card") ? [child] : [...child.querySelectorAll(".set-card")].filter((c) => !c.parentElement?.closest(".set-card"));
      // Next to the cards of a wrapper, what else it holds (a closing paragraph).
      const apart = cards.length > 0 && cards[0] !== child ? [...child.children].filter((el) => !cards.some((c) => el.contains(c))) : [];
      for (const unit of cards.length > 0 ? [...cards, ...apart] : [child]) {
        const named = [...unit.querySelectorAll(".set-label, .set-hint")];
        const own = whole || has(named.length > 0 ? named.map((n) => n.textContent).join(" ") : unit.textContent);
        let rows = 0;
        for (const row of unit.querySelectorAll("[data-search-row]")) {
          const hit = own || has((row.querySelector("[data-search-name]") ?? row).textContent);
          light(row, hit);
          if (hit) rows++;
        }
        // A card with a matching row stays lit; its other rows are grey.
        light(unit, own || rows > 0);
        if (own || rows > 0) hits++;
      }
    }
    section.toggleAttribute("data-miss", hits === 0);
    if (hits > 0 && section.dataset.searchId) found.push(section.dataset.searchId);
  }
  return found;
}

/**
 * The page while a search is typed: each section with a match under its
 * name, which leads to the section itself. `body` draws a section's
 * settings; `onFound` hears which sections hold a match, `none` says that
 * none does.
 */
export function SearchResults({ query, sections, body, open, onFound, none }: { query: string; sections: SearchSection[]; body: (id: string) => React.ReactNode; open: (id: string) => void; onFound: (ids: string[]) => void; none: boolean }) {
  const root = useRef<HTMLDivElement>(null);
  // Sections change after they are drawn (lists that load, a switch that
  // brings more settings): marked again whenever they do.
  useLayoutEffect(() => {
    const el = root.current;
    if (!el) return;
    const apply = () => onFound(filter(el, query));
    apply();
    // Other words, other settings: back to the top of the page.
    el.closest("main")?.scrollTo(0, 0);
    const changes = new MutationObserver(apply);
    changes.observe(el, { childList: true, subtree: true, characterData: true });
    return () => changes.disconnect();
  }, [query, onFound]);
  return (
    <>
      <div ref={root} className="flex flex-col gap-8">
        {sections.map((s) => (
          <section key={s.id} data-search-id={s.id} data-search-section={s.title}>
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
