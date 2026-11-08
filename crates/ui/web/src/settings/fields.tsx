// The Settings page's building blocks: one card per setting and its controls.

/** One setting: label and explanation, then its control. */
export function Field({ label, hint, children }: { label: string; hint?: string; children?: React.ReactNode }) {
  return (
    <div className="set-card">
      <div className="set-label">{label}</div>
      {hint && <p className="set-hint">{hint}</p>}
      {children && <div className="mt-3">{children}</div>}
    </div>
  );
}

/** An on/off setting: the switch on the right. */
export function Toggle({ label, hint, checked, onChange }: { label: string; hint?: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="set-card flex cursor-default items-start gap-4">
      <span className="min-w-0 flex-1">
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

/** A labelled range with a reset button; `now` is true for the reset. */
export function Slider({ value, min, max, step, unit, reset, onChange }: { value: number; min: number; max: number; step: number; unit: string; reset: number; onChange: (v: number, now: boolean) => void }) {
  return (
    <div className="flex max-w-[460px] items-center gap-3">
      <input className="range flex-1" type="range" min={min} max={max} step={step} value={value} onChange={(e) => onChange(Number(e.target.value), false)} />
      <span className="w-14 text-right tabular-nums text-[var(--secondary)]">{value}{unit}</span>
      <button className="btn sm" disabled={value === reset} onClick={() => onChange(reset, true)}>Reset</button>
    </div>
  );
}
