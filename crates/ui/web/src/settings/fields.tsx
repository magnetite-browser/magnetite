// The Settings page's building blocks: a card per group of settings, a row
// per setting with its name, a description and its control.
import { createContext, useContext, useEffect, useRef, useState } from "react";
import { ChevronDown } from "../Icons";

/** Whether a setting is drawn inside a `Group`: then it is a row of it, else a card of its own. */
const InGroup = createContext(false);

/** Whether a search marks the settings around: then every group shows its rows, folded or not. */
export const Unfolded = createContext(false);

/**
 * Settings that belong together, as one card: an optional name and a line
 * about them, then one row per setting. With `fold` the name is a button
 * that shows and hides the rows ("closed": hidden at first), and `count`
 * says what is behind it; `onFold` hears when the rows are hidden, by the
 * button or by a search that ends. While a search marks the settings
 * (`Unfolded`) the rows show whatever the fold, and the name is no button:
 * there is nothing to fold then, and the fold stays as it was.
 */
export function Group({ title, hint, fold, count, onFold, children }: { title?: string; hint?: string; fold?: "open" | "closed"; count?: string; onFold?: () => void; children: React.ReactNode }) {
  const [folded, setFolded] = useState(fold === "closed");
  const unfolded = useContext(Unfolded);
  const open = !folded || unfolded;
  // Told once the rows are gone, whatever hid them.
  const hears = useRef(onFold);
  hears.current = onFold;
  const was = useRef(open);
  useEffect(() => {
    if (was.current && !open) hears.current?.();
    was.current = open;
  }, [open]);
  return (
    <section className="set-card set-group" data-folded={!open || undefined}>
      {(title || hint) && (
        <div className="set-group-head">
          {title && (!fold || unfolded) && <h2 className="set-label set-group-title">{title}</h2>}
          {title && fold && !unfolded && (
            <h2 className="set-label set-group-title">
              <button className="set-fold" aria-expanded={open} onClick={() => setFolded(open)}>
                <ChevronDown width={14} height={14} />
                <span className="flex-1 text-left">{title}</span>
                {count && <span className="font-normal text-[var(--muted)]">{count}</span>}
              </button>
            </h2>
          )}
          {hint && <p className="set-hint">{hint}</p>}
        </div>
      )}
      <div className="set-group-body" hidden={!open}>
        <InGroup.Provider value={true}>{children}</InGroup.Provider>
      </div>
    </section>
  );
}

/**
 * One setting: its name and explanation, and its control, below them or
 * (`side`, for a short one) to their right. The search finds it by the
 * name and the explanation.
 */
export function Field({ label, hint, side, children }: { label: string; hint?: string; side?: boolean; children?: React.ReactNode }) {
  const grouped = useContext(InGroup);
  const name = (
    <div className="min-w-0 flex-1" data-search-name={grouped || undefined}>
      <div className="set-label">{label}</div>
      {hint && <p className="set-hint">{hint}</p>}
    </div>
  );
  return (
    <div className={grouped ? "set-row" : "set-card"} data-search-row={grouped || undefined}>
      {side && children ? (
        <div className="set-sided">{name}<div className="set-side" role="group" aria-label={label}>{children}</div></div>
      ) : (
        <>{name}{children && <div className="mt-3" role="group" aria-label={label}>{children}</div>}</>
      )}
    </div>
  );
}

/** An on/off setting: the switch on the right. */
export function Toggle({ label, hint, checked, onChange }: { label: string; hint?: string; checked: boolean; onChange: (v: boolean) => void }) {
  const grouped = useContext(InGroup);
  return (
    <label className={`${grouped ? "set-row" : "set-card"} flex cursor-default items-start gap-4`} data-search-row={grouped || undefined}>
      <span className="min-w-0 flex-1" data-search-name={grouped || undefined}>
        <span className="set-label block">{label}</span>
        {hint && <span className="set-hint block">{hint}</span>}
      </span>
      <Switch label={label} checked={checked} onChange={onChange} />
    </label>
  );
}

export function Switch({ label, checked, onChange }: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button type="button" role="switch" aria-label={label} aria-checked={checked} onClick={() => onChange(!checked)} className="relative mt-0.5 h-5 w-9 shrink-0 rounded-full transition-colors" style={{ background: checked ? "var(--accent)" : "rgba(255,255,255,0.15)" }}>
      <span className="absolute top-0.5 h-4 w-4 rounded-full bg-white transition-all" style={{ left: checked ? 18 : 2 }} />
    </button>
  );
}

/**
 * A labelled range with a reset button. `onChange` gets every value the
 * thumb passes with `now` false, and the value it is let go at (a key step
 * and the reset count as that too) with `now` true.
 */
export function Slider({ value, min, max, step, unit, reset, onChange }: { value: number; min: number; max: number; step: number; unit: string; reset: number; onChange: (v: number, now: boolean) => void }) {
  const input = useRef<HTMLInputElement>(null);
  const latest = useRef(onChange);
  latest.current = onChange;
  // The browser's own "change" is the letting go; React's `onChange` is its "input".
  useEffect(() => {
    const el = input.current;
    if (!el) return;
    const released = () => latest.current(Number(el.value), true);
    el.addEventListener("change", released);
    return () => el.removeEventListener("change", released);
  }, []);
  return (
    <div className="flex max-w-[460px] items-center gap-3">
      <input ref={input} className="range flex-1" type="range" min={min} max={max} step={step} value={value} style={{ ["--fill" as string]: `${((value - min) / (max - min)) * 100}%` }} onChange={(e) => onChange(Number(e.target.value), false)} />
      <span className="w-14 text-right tabular-nums text-[var(--secondary)]">{value}{unit}</span>
      <button type="button" className="btn sm" disabled={value === reset} onClick={() => onChange(reset, true)}>Reset</button>
    </div>
  );
}
