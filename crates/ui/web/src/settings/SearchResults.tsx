// Settings search: the sections that hold a match on one page, the settings
// that do not match greyed out. The sections are rendered as they are and
// marked by what they show, so a new setting is found without being listed
// anywhere.
import { useCallback, useLayoutEffect, useRef } from "react";
import type { IconComponent } from "../Icons";
import { t } from "../i18n";
import { Unfolded } from "./fields";

export type SearchSection = { id: string; title: string; Icon: IconComponent };

/**
 * Marks what `query` finds below `root` and returns the ids of the sections
 * with a match. A setting is a card (`.set-card`), or a part of a section
 * outside any card. It matches by its name and description, every typed
 * word somewhere in them; the others are greyed out (`data-dim`), and a
 * section without any match is left out (`data-miss`). A section whose
 * title matches comes first, with nothing greyed out. In a card of rows
 * (`data-search-row`: the settings of a group, the shortcuts, the spaces
 * of sync) the rows are marked one by one, by their name
 * (`data-search-name`, or all their text), unless the card's own name
 * matches.
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
        const all = [...unit.querySelectorAll("[data-search-row]")];
        // The card's own name: what names it outside its rows. A card of
        // rows without a name of its own is found by its rows only.
        const named = [...unit.querySelectorAll(".set-label, .set-hint")].filter((n) => !n.closest("[data-search-row]"));
        const own = whole || (named.length > 0 ? has(named.map((n) => n.textContent).join(" ")) : all.length === 0 && has(unit.textContent));
        // A row's name: what it marks as such (its label and description),
        // the words on its buttons and its notes, not those of a row
        // inside it. Not what a list to open or a field holds.
        const byName = (row: Element) => {
          // The buttons and notes of a setting (`.set-row`) only: a line of
          // a list has the same buttons as every other line of it.
          const what = row.matches(".set-row") ? "[data-search-name], button, .set-hint" : "[data-search-name]";
          const names = [...row.querySelectorAll(what)].filter((n) => n.closest("[data-search-row]") === row);
          return has(names.length > 0 ? names.map((n) => n.textContent).join(" ") : row.textContent);
        };
        let rows = 0;
        for (const row of all) {
          // A row inside a row (the spaces listed by a setting) is found with it.
          const around = row.parentElement?.closest("[data-search-row]");
          const hit = own || byName(row) || (!!around && byName(around));
          light(row, hit);
          if (hit) rows++;
        }
        // A setting with a matching row inside stays lit.
        for (const row of all) if (row.querySelector("[data-search-row]:not([data-dim])")) light(row, true);
        // A card with a matching row stays lit; its other rows are grey.
        light(unit, own || rows > 0);
        if (own || rows > 0) hits++;
      }
    }
    // One section opened from a search stays, also without a match in it.
    section.toggleAttribute("data-miss", hits === 0 && !section.hasAttribute("data-search-keep"));
    if (hits > 0 && section.dataset.searchId) found.push(section.dataset.searchId);
  }
  return found;
}

/** The name of the highlight that marks the typed words (`::highlight` in index.css). */
const MARKED = "settings-search";

/**
 * Marks the typed words where they stand in the names and descriptions of
 * the settings found below `root`. The page is not changed for it: the
 * words are ranges of a CSS highlight.
 */
function mark(root: HTMLElement, query: string) {
  if (!CSS.highlights) return;
  const words = [...new Set(query.toLowerCase().split(/\s+/).filter(Boolean))];
  const ranges: Range[] = [];
  const texts = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let node = texts.nextNode(); node; node = texts.nextNode()) {
    const within = node.parentElement;
    if (!within?.closest(".set-label, .set-hint, .set-found, [data-search-name], [data-search-row]") || within.closest("[data-dim], [data-miss], select")) continue;
    const original = node.textContent ?? "";
    const text = original.toLowerCase();
    // A letter whose lower case is longer (İ) would shift every place after
    // it: such a text is found, but not marked.
    if (text.length !== original.length) continue;
    for (const word of words) {
      for (let at = text.indexOf(word); at >= 0; at = text.indexOf(word, at + word.length)) {
        const range = new Range();
        range.setStart(node, at);
        range.setEnd(node, at + word.length);
        ranges.push(range);
      }
    }
  }
  // One by one: there may be more places than a call takes arguments.
  const marked = new Highlight();
  for (const range of ranges) marked.add(range);
  CSS.highlights.set(MARKED, marked);
}

/**
 * Marks what `query` finds below the element the returned ref is on, and
 * again whenever that changes: sections change after they are drawn (lists
 * that load, a switch that brings more settings). `onFound` hears which
 * sections hold a match.
 */
function useMarked(query: string, onFound?: (ids: string[]) => void) {
  const root = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = root.current;
    if (!el) return;
    const apply = () => {
      // Always: the filter greys out, whoever hears what it found.
      const found = filter(el, query);
      onFound?.(found);
      mark(el, query);
    };
    apply();
    // Other words, other settings: back to the top of the page.
    if (!query.trim()) {
      // Nothing to find: the one pass above took the marks away, and no
      // change of the settings brings any back.
      return () => CSS.highlights?.delete(MARKED);
    }
    el.closest("main")?.scrollTo(0, 0);
    const changes = new MutationObserver(apply);
    changes.observe(el, { childList: true, subtree: true, characterData: true });
    return () => {
      changes.disconnect();
      CSS.highlights?.delete(MARKED);
    };
  }, [query, onFound]);
  return root;
}

/**
 * One section on the page: all its settings, and while a search is typed
 * (`query`) those it did not find greyed out. The same with and without a
 * search, so that emptying the field leaves what was typed into a setting.
 * `onFound` hears whether the section still holds a match, as its
 * settings change.
 */
export function SearchedSection({ query, section, onFound, children }: { query: string; section: SearchSection; onFound?: (found: boolean, id: string) => void; children: React.ReactNode }) {
  // What is found is the section's own, also when its answer comes after
  // the page has gone on to another one.
  const id = section.id;
  const told = useCallback((ids: string[]) => onFound?.(ids.includes(id), id), [onFound, id]);
  const root = useMarked(query, onFound ? told : undefined);
  return (
    <div ref={root}>
      <section data-search-id={section.id} data-search-section={section.title} data-search-keep>
        <Unfolded.Provider value={query.trim() !== ""}>
          <div className="space-y-4" data-search-body>{children}</div>
        </Unfolded.Provider>
      </section>
    </div>
  );
}

/**
 * The page while a search is typed: each section with a match under its
 * name, which leads to the section itself. `body` draws a section's
 * settings; `onFound` hears which sections hold a match, `none` says that
 * none does.
 */
export function SearchResults({ query, sections, body, open, onFound, none }: { query: string; sections: SearchSection[]; body: (id: string) => React.ReactNode; open: (id: string) => void; onFound: (ids: string[]) => void; none: boolean }) {
  const root = useMarked(query, onFound);
  return (
    <>
      <div ref={root} className="flex flex-col gap-8">
        {sections.map((s) => (
          <section key={s.id} data-search-id={s.id} data-search-section={s.title}>
            <button className="set-found" onClick={() => open(s.id)} title={t("settings.page.open_section", { section: s.title })}>
              <s.Icon width={15} height={15} />
              {s.title}
            </button>
            <Unfolded.Provider value={true}><div className="space-y-4" data-search-body>{body(s.id)}</div></Unfolded.Provider>
          </section>
        ))}
      </div>
      {none && <p className="text-[14px] text-[var(--muted)]">{t("settings.page.no_match", { query: query.trim() })}</p>}
    </>
  );
}
