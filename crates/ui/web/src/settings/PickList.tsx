// A choice from a list in Settings where the system's own list falls short:
// several choices at once, or choices with a colour and an icon of their
// own (the spaces). A button says what is chosen; the list opens under it.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Check } from "../Icons";
import { t } from "../i18n";

export type PickOption = {
  value: string;
  label: string;
  /** Said after the label in the list, quieter. */
  hint?: string;
  /** Shown before the label. */
  icon?: React.ReactNode;
  /** The option's own colour (a space's): its icon and its tag on the button have it. */
  color?: string;
  /** A line above it in the list. */
  apart?: boolean;
};

/**
 * `chosen` of `options`, to change. With `several` any number of them is
 * ticked and the list stays open meanwhile; none ticked is said as `none`,
 * which is the list's first entry and unticks them all. Without, one is
 * chosen and the list closes. Options with a colour show on the button as
 * tags, as many as have room, then how many more ("+3"); resting the
 * pointer on it names them all. The list closes when the keyboard leaves
 * it (Tab), on Esc and on a click elsewhere.
 */
export function PickList({ options, chosen, several, none, label, width = 170, align = "left", disabled, onChoose }: {
  options: PickOption[];
  chosen: string[];
  several?: boolean;
  none?: string;
  /** What is chosen here, for a screen reader: "Where GitHub shows". */
  label: string;
  width?: number;
  /** The side of the button the list lines up with. */
  align?: "left" | "right";
  disabled?: boolean;
  onChoose: (values: string[]) => void;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLSpanElement>(null);
  const list = useRef<HTMLSpanElement>(null);
  const close = (focus: boolean) => {
    setOpen(false);
    if (focus) root.current?.querySelector("button")?.focus();
  };
  // The list opens upwards where the page has no room left below the button.
  const [up, setUp] = useState(false);
  useLayoutEffect(() => {
    if (!open || !root.current || !list.current) return setUp(false);
    const button = root.current.getBoundingClientRect();
    const page = root.current.closest("main")?.getBoundingClientRect();
    const height = list.current.offsetHeight + 8;
    setUp(button.bottom + height > (page?.bottom ?? innerHeight) && button.top - height > (page?.top ?? 0));
  }, [open]);
  useEffect(() => {
    if (!open) return;
    // The keys start at what is chosen.
    (list.current?.querySelector<HTMLElement>('[aria-checked="true"]') ?? list.current?.querySelector("button"))?.focus();
    const away = (e: PointerEvent) => !root.current?.contains(e.target as Node) && setOpen(false);
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") return close(true);
      // The arrow keys are the list's only while the keyboard is in it.
      if ((e.key !== "ArrowDown" && e.key !== "ArrowUp") || !root.current?.contains(document.activeElement)) return;
      const all = [...(list.current?.querySelectorAll("button") ?? [])];
      const at = all.indexOf(document.activeElement as HTMLButtonElement);
      e.preventDefault();
      all[Math.max(0, Math.min(all.length - 1, at + (e.key === "ArrowDown" ? 1 : -1)))]?.focus();
    };
    const blur = () => setOpen(false);
    document.addEventListener("pointerdown", away, true);
    window.addEventListener("keydown", key);
    window.addEventListener("blur", blur);
    return () => {
      document.removeEventListener("pointerdown", away, true);
      window.removeEventListener("keydown", key);
      window.removeEventListener("blur", blur);
    };
  }, [open]);

  // In the order of the list; a choice that is no option (a space that is gone) is not named.
  const now = options.filter((o) => chosen.includes(o.value));
  // The tags that have room on the button: by the length of their names, one at least.
  const room = Math.floor((width - 44) / 7.5);
  let used = 0;
  const tags = now.filter((o, i) => (used += o.label.length + (o.icon ? 5 : 3)) <= room || i === 0);
  const more = now.length - tags.length;
  const names = now.map((o) => o.label).join(", ");
  const toggle = (value: string) => {
    if (!several) {
      close(true);
      if (!chosen.includes(value) || chosen.length !== 1) onChoose([value]);
      return;
    }
    onChoose(chosen.includes(value) ? chosen.filter((v) => v !== value) : [...chosen, value]);
  };
  return (
    <span ref={root} className="set-pick" style={{ width }} onBlur={(e) => e.relatedTarget && !e.currentTarget.contains(e.relatedTarget) && setOpen(false)}>
      <button className="field set-pick-button" disabled={disabled} aria-haspopup="menu" aria-expanded={open} aria-label={`${label}: ${names || none || t("settings.pick.nothing")}`} title={more > 0 ? names : undefined} onClick={() => setOpen(!open)}>
        {now.length === 0 && <span className="truncate">{none}</span>}
        {now.some((o) => o.color)
          ? tags.map((o) => (
              <span key={o.value} className="set-tag" style={{ ["--tag" as string]: o.color }}>
                {o.icon}
                <span>{o.label}</span>
              </span>
            ))
          : now.length > 0 && <span className="truncate">{names}</span>}
        {now.some((o) => o.color) && more > 0 && <span className="set-tag-more">+{more}</span>}
      </button>
      {open && (
        <span ref={list} className="set-pick-list scroll" data-align={align} data-up={up || undefined} role="menu" aria-label={label}>
          {several && none && (
            <>
              <button role="menuitemcheckbox" aria-checked={now.length === 0} onClick={() => onChoose([])}>
                <Check width={14} height={14} className="set-pick-check" />
                <span>{none}</span>
              </button>
              <hr />
            </>
          )}
          {options.map((o) => (
            <span key={o.value} className="contents">
              {o.apart && <hr />}
              <button role={several ? "menuitemcheckbox" : "menuitemradio"} aria-checked={chosen.includes(o.value)} onClick={() => toggle(o.value)}>
                <Check width={14} height={14} className="set-pick-check" />
                {o.icon && <span className="set-pick-icon" style={{ color: o.color }}>{o.icon}</span>}
                <span>{o.label}</span>
                {o.hint && <span className="set-pick-hint">{o.hint}</span>}
              </button>
            </span>
          ))}
        </span>
      )}
    </span>
  );
}
