import { useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { pageZoom, send } from "./bridge";
import { byName } from "./byName";
import { Back, Check, Forward } from "./Icons";

/** What every row shows. */
interface Row {
  label: string;
  /** A small picture before the label, 16 px. */
  icon?: ReactNode;
  /** A colour as a dot before the label (a profile's), as CSS. */
  dot?: string;
  /** Shown on hover in place of the label (which may end in "…"). */
  title?: string;
  /** A short note at the right edge, such as a time, or what a list has chosen. */
  hint?: string;
}

/** A row that does something. */
type Action = Row & {
  separator?: false;
  /** A name in a box right after the label, such as the profile a space uses. */
  tag?: string;
  /** Something that is on or off: a check mark while it is on. */
  checked?: boolean;
  /** One of several to choose from: a dot on the one chosen. */
  selected?: boolean;
  onClick: () => void;
  /** Has nothing to do right now: greyed out, not picked, and `title` says why. */
  disabled?: boolean;
  danger?: boolean;
  /** Ask first: the first click shows this label in place, the second runs `onClick`. */
  confirm?: string;
};

/** A line between groups, or an action. */
type Leaf = { separator: true } | Action;
export type MenuLeaf = Leaf;

/**
 * A field above a list's rows that has the keyboard while the list is open:
 * typing there keeps the rows whose label has the text, best first (see
 * `byName`), and Enter picks the first of them.
 */
interface Filter {
  placeholder: string;
  /** Shown in place of the rows when none has the text. */
  none: string;
}

/**
 * A line between groups, an action, or a list of its own. A list opens in
 * the menu's place, below a row that leads back: a menu in the sidebar has
 * no room beside it for one that flies out. It opens on a click or by the
 * keyboard, never by itself. `filter`: a field to narrow it by typing.
 */
export type MenuItem = Leaf | (Row & { separator?: false; items: Leaf[]; filter?: Filter });

/** The line between groups of a menu. */
export const SEPARATOR: { separator: true } = { separator: true };

/**
 * An opened list shows ten rows and half of the next, which tells that it
 * scrolls (macOS shows no scrollbar until then); the row back stays above it.
 */
const LIST_HEIGHT = 10.5 * 28;

/**
 * The widest a menu gets: it is as wide as its widest row (`.menu` in
 * index.css), up to this and 8 px inside the window's sides.
 */
const MENU_MAX_WIDTH = 320;

/** The height of a list's field with the gap below it (`.menu-filter` in index.css). */
const FILTER_HEIGHT = 28 + 4;

/** The height of `items`: 28 px a row, 9 a line (see `.menu` in index.css). */
function rowsHeight(items: MenuItem[]): number {
  return items.reduce((h, it) => h + (it.separator ? 9 : 28), 0);
}

/** A row by its place: in the menu, or (`sub`) in the list of row `top`. While that list is open, `top` alone is the row that leads back. */
interface Place {
  top: number;
  sub?: number;
}
const same = (a: Place | null, b: Place) => !!a && a.top === b.top && a.sub === b.sub;
const keyOf = (p: Place) => (p.sub === undefined ? `${p.top}` : `${p.top}.${p.sub}`);

/**
 * Closes on a pick, Esc or a key it has no use for, a click beside it or
 * when its page loses the keyboard (a click into a web page, another app).
 * The arrow keys go through its rows (Home and End: the first and the
 * last), Enter or Space picks one, the right arrow opens a list and the
 * left one goes back from it. A list with a field keeps the keyboard in it:
 * there the arrows up and down, Enter, Esc and Tab are the menu's keys,
 * every other key is the field's, and the left arrow goes back only once
 * the field is empty.
 * `takeFocus`: ask for the keyboard on opening, as a right click gives none
 * and without it there is nothing to lose; only for menus in the sidebar, a
 * page or a window of its own has it.
 */
export function ContextMenu({ x, y, items, onClose, takeFocus = false }: { x: number; y: number; items: MenuItem[]; onClose: () => void; takeFocus?: boolean }) {
  useEffect(() => {
    if (takeFocus) send({ cmd: "focus_sidebar" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const [armed, setArmed] = useState<string | null>(null);
  // The row whose list is open, if one is.
  const [open, setOpen] = useState<number | null>(null);
  // The row the keyboard is on (the pointer moves it too).
  const [at, setAt] = useState<Place | null>(null);
  // What is typed into the open list's field.
  const [query, setQuery] = useState("");
  const buttons = useRef(new Map<string, HTMLButtonElement>());
  const field = useRef<HTMLInputElement>(null);
  // The rows' ids, so the field can name the one lit up while it keeps the keyboard.
  const uid = useId();
  // The menu's width as drawn, in its own px: its rows decide it. Read
  // before the paint, so it opens where it fits from the start. While a
  // list's field has text, the width read with all of its rows stays, so
  // the menu does not move while typing narrows them.
  const box = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    if (box.current && !query) setWidth(box.current.offsetWidth);
  });

  const list = open === null ? undefined : items[open];
  const opened = list && "items" in list ? list.items : null;
  const filter = list && "items" in list ? list.filter : undefined;
  /** A row to go to and to pick: no line, and none that is greyed out. */
  const live = (it: MenuItem) => !it.separator && !("disabled" in it && it.disabled);
  /** The open list's rows that show for `text`, with their place in it: all of them, or with a field those that have it, best first. */
  const shownFor = (text: string) => {
    const rows = (opened ?? []).map((it, sub) => ({ it, sub }));
    // A line has no name, so it goes as soon as something is typed.
    return filter ? byName(rows, text, ({ it }) => (it.separator ? "" : it.label)) : rows;
  };
  const shown = shownFor(query);
  /** The rows the keyboard goes through, top to bottom: the menu's, or the row back and the open list's. */
  const places: Place[] = opened && open !== null
    ? [{ top: open }, ...shown.flatMap(({ it, sub }) => (live(it) ? [{ top: open, sub }] : []))]
    : items.flatMap((it, top) => (live(it) ? [{ top }] : []));
  const itemAt = (p: Place): MenuItem | undefined => {
    const top = items[p.top];
    return p.sub === undefined || !top || !("items" in top) ? top : top.items[p.sub];
  };
  const go = (p: Place | undefined) => {
    if (!p) return;
    setAt(p);
    // After the draw: a list just opened has its rows only then. Where a
    // field has the keyboard, it keeps it, and the row only comes into view.
    requestAnimationFrame(() => {
      const button = buttons.current.get(keyOf(p));
      if (field.current) button?.scrollIntoView({ block: "nearest" });
      else button?.focus();
    });
  };
  const toggle = (top: number, on = open !== top) => {
    setArmed(null);
    setQuery("");
    setOpen(on ? top : null);
    // A list with a field starts on its first row, opened by a click as by a key: Enter picks it.
    const it = items[top];
    if (on && it && "items" in it && it.filter) setAt({ top, sub: firstRow(it) });
  };
  const pick = (p: Place) => {
    const it = itemAt(p);
    if (!it || it.separator) return;
    if ("items" in it) return toggle(p.top);
    if (it.disabled) return;
    if (it.confirm && armed !== keyOf(p)) return setArmed(keyOf(p));
    it.onClick();
    onClose();
  };

  // The newest of what the keys act on: the listener stays the same one.
  const filtering = !!filter;
  const keys = useRef({ places, at, open, filtering, pick, go, toggle, itemAt, onClose });
  keys.current = { places, at, open, filtering, pick, go, toggle, itemAt, onClose };
  useEffect(() => {
    const close = () => keys.current.onClose();
    const onKey = (e: KeyboardEvent) => {
      const { places, at, open, filtering, pick, go, toggle, itemAt, onClose } = keys.current;
      const i = at ? places.findIndex((p) => same(at, p)) : -1;
      const here = at && i >= 0 ? itemAt(at) : undefined;
      const isList = !!here && !here.separator && "items" in here;
      const handled = () => {
        e.preventDefault();
        e.stopPropagation();
      };
      // A key held for a shortcut alone is no key of the menu's, and none to close it for.
      if (["Shift", "Control", "Alt", "Meta"].includes(e.key)) return;
      // A key of the list's field, or a word still being composed there: into
      // the field, back in it first should a click beside the rows have taken
      // the keyboard.
      const fieldKey = !["ArrowDown", "ArrowUp", "Enter", "Escape", "Tab"].includes(e.key) && !(e.key === "ArrowLeft" && !field.current?.value);
      if (filtering && (fieldKey || e.isComposing)) {
        field.current?.focus();
        return;
      }
      if (e.key === "ArrowDown") return handled(), go(places[(i + 1) % places.length]);
      if (e.key === "ArrowUp") return handled(), go(places[i < 0 ? places.length - 1 : (i - 1 + places.length) % places.length]);
      if (e.key === "Home") return handled(), go(places[0]);
      if (e.key === "End") return handled(), go(places.at(-1));
      if (at && i >= 0 && (e.key === "Enter" || e.key === " ")) {
        handled();
        // A key held down picks once: its repeats would answer a row that asks first, and open and close a list.
        if (e.repeat) return;
        pick(at);
        // Into a list that was just opened; back from it, onto its row.
        if (isList) go(open === at.top ? { top: at.top } : { top: at.top, sub: firstRow(here) });
        return;
      }
      // No row has what was typed: nothing to pick.
      if (filtering && e.key === "Enter") return handled();
      if (at && isList && e.key === "ArrowRight") {
        handled();
        if (open !== at.top) toggle(at.top, true);
        return go({ top: at.top, sub: firstRow(here) });
      }
      if (e.key === "ArrowLeft" && open !== null) {
        handled();
        toggle(open, false);
        return go({ top: open });
      }
      // Esc, and every key that is not the menu's: a shortcut typed now is not meant for it.
      if (e.key === "Escape") handled();
      onClose();
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("blur", close);
    // Before the sidebar's own keys.
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("blur", close);
      window.removeEventListener("keydown", onKey, true);
    };
  }, []);

  // `x`, `y` and the window in the menu's own px (see `pageZoom`), so it
  // opens at the pointer and stays inside at every text size; taller than
  // the window, it starts at its top and scrolls.
  const zoom = pageZoom();
  const room = window.innerHeight / zoom - 16;
  const closed = 8 + rowsHeight(items);
  const wide = Math.min(MENU_MAX_WIDTH, window.innerWidth / zoom - 16);
  const left = Math.max(8, Math.min(x / zoom, window.innerWidth / zoom - width - 8));
  // Where the menu stands. A list opens at the same place, so the row back
  // is where the menu's first row was; only a list taller than the room
  // below moves it up.
  const rest = Math.max(8, Math.min(y / zoom, window.innerHeight / zoom - Math.min(room, closed) - 8));
  // The padding, then the rows; an opened list has its row back and a line
  // above, and its field. Counted with all of its rows, so the menu stays
  // where it is while typing narrows them.
  const height = Math.min(room, opened ? 8 + 28 + 9 + (filter ? FILTER_HEIGHT : 0) + Math.min(LIST_HEIGHT, rowsHeight(opened)) : closed);
  const top = Math.max(8, Math.min(rest, window.innerHeight / zoom - height - 8));

  /** Whether rows of `list` keep room for a mark: one of them has one. */
  const marked = (list: MenuItem[]) => list.some((it) => !it.separator && !("items" in it) && (it.checked !== undefined || it.selected !== undefined));
  const row = (it: MenuItem, p: Place, marks: boolean) => {
    const key = keyOf(p);
    if (it.separator) return <hr key={key} />;
    const common = {
      ref: (el: HTMLButtonElement | null) => {
        if (el) buttons.current.set(key, el);
        else buttons.current.delete(key);
      },
      id: `${uid}-${key}`,
      tabIndex: -1,
      "data-at": (same(at, p) && live(it)) || undefined,
      onMouseMove: () => live(it) && !same(at, p) && setAt(p),
    };
    const lead = (
      <>
        {marks && <span className="mark" aria-hidden />}
        {it.icon && <span className="icon">{it.icon}</span>}
        {it.dot !== undefined && <span className="dot" style={{ background: it.dot || undefined }} data-plain={!it.dot || undefined} />}
      </>
    );
    if ("items" in it) {
      return (
        <button key={key} {...common} role="menuitem" aria-haspopup="menu" title={it.title ?? (it.hint ? `${it.label} · ${it.hint}` : it.label)} onClick={() => pick(p)}>
          {lead}
          <span>{it.label}</span>
          {it.hint && <span className="hint">{it.hint}</span>}
          <Forward width={14} height={14} className="chevron" />
        </button>
      );
    }
    const on = it.checked ?? it.selected;
    return (
      <button
        key={key}
        {...common}
        role={it.checked !== undefined ? "menuitemcheckbox" : it.selected !== undefined ? "menuitemradio" : "menuitem"}
        aria-checked={on}
        title={it.title ?? (it.tag ? `${it.label} · ${it.tag}` : it.label)}
        data-danger={it.danger || undefined}
        aria-disabled={it.disabled || undefined}
        onClick={() => pick(p)}
      >
        {marks && <span className="mark" aria-hidden>{it.checked ? <Check width={13} height={13} /> : it.selected ? <span className="chosen" /> : null}</span>}
        {it.icon && <span className="icon">{it.icon}</span>}
        {it.dot !== undefined && <span className="dot" style={{ background: it.dot || undefined }} data-plain={!it.dot || undefined} />}
        <span>{armed === key ? it.confirm : it.label}</span>
        {it.tag && <span className="tag">{it.tag}</span>}
        {it.hint && <span className="hint">{it.hint}</span>}
      </button>
    );
  };
  const back = list && !list.separator && open !== null ? { label: list.label, place: { top: open } } : null;
  return (
    // Keyed by what it shows, so a menu scrolled in a short window starts at its top again.
    <div
      key={open ?? "menu"}
      ref={box}
      className="menu scroll no-drag"
      role="menu"
      aria-label={back?.label}
      style={{ left, top, width: query ? width : undefined, ["--menu-room" as string]: `${room}px`, ["--menu-wide" as string]: `${wide}px` }}
      onMouseDown={(e) => e.stopPropagation()}
    >
      {back && opened ? (
        <>
          <button
            ref={(el) => {
              if (el) buttons.current.set(keyOf(back.place), el);
              else buttons.current.delete(keyOf(back.place));
            }}
            id={`${uid}-${keyOf(back.place)}`}
            tabIndex={-1}
            role="menuitem"
            className="menu-back"
            title={back.label}
            data-at={same(at, back.place) || undefined}
            onMouseMove={() => !same(at, back.place) && setAt(back.place)}
            onClick={() => pick(back.place)}
          >
            <Back width={14} height={14} />
            <span>{back.label}</span>
          </button>
          <hr />
          {filter && (
            <input
              ref={field}
              className="menu-filter"
              // Typing narrows the list right away, opened by a click as by a key.
              autoFocus
              value={query}
              placeholder={filter.placeholder}
              aria-label={filter.placeholder}
              aria-activedescendant={at ? `${uid}-${keyOf(at)}` : undefined}
              spellCheck={false}
              onChange={(e) => {
                const text = e.target.value;
                setQuery(text);
                setArmed(null);
                // The best match is the one Enter picks.
                const best = shownFor(text).find(({ it }) => live(it));
                if (best) go({ top: back.place.top, sub: best.sub });
                else setAt(null);
              }}
            />
          )}
          <div className="scroll" style={{ maxHeight: LIST_HEIGHT }}>
            {shown.map(({ it, sub }) => row(it, { top: back.place.top, sub }, marked(opened)))}
            {shown.length === 0 && <div className="menu-none">{filter?.none}</div>}
          </div>
        </>
      ) : (
        items.map((it, i) => row(it, { top: i }, marked(items)))
      )}
    </div>
  );
}

/** The first row of list `it` that is no line. */
function firstRow(it: MenuItem | undefined): number {
  return it && !it.separator && "items" in it ? Math.max(0, it.items.findIndex((sub) => !sub.separator && !sub.disabled)) : 0;
}
