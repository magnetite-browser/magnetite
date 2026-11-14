// Settings → Appearance: space colours, layout, text size, density, sidebar width.
import { useRef, useState } from "react";
import { SpaceGlyph } from "../Glyphs";
import { DEFAULT_ACCENT, IS_MAC, accentCss, applyAppearance, send, type Appearance, type Color } from "../bridge";
import { Field, Slider } from "./fields";
import type { Info, SettingsPatch } from "./types";

const toColor = (r: number, g: number, b: number): Color => ({ red: r / 255, green: g / 255, blue: b / 255 });
/** Red, green and blue of `c`, 0 to 255. */
const channels = (c: Color) => [c.red, c.green, c.blue].map((v) => Math.round(v * 255));

/** Space colours to pick from (the first is the default); any other comes from the colour picker. */
const SPACE_COLOURS: [string, Color][] = [
  ["Indigo", DEFAULT_ACCENT], ["Blue", toColor(77, 148, 255)], ["Teal", toColor(45, 190, 170)], ["Green", toColor(95, 196, 120)],
  ["Lime", toColor(170, 205, 80)], ["Yellow", toColor(235, 195, 70)], ["Orange", toColor(245, 150, 70)], ["Red", toColor(240, 95, 95)],
  ["Pink", toColor(235, 110, 170)], ["Purple", toColor(170, 120, 240)], ["Graphite", toColor(150, 150, 165)],
];
const toHex = (c?: Color | null) => "#" + channels(c ?? DEFAULT_ACCENT).map((v) => v.toString(16).padStart(2, "0")).join("");
const fromHex = (h: string) => toColor(parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16));
const sameColour = (a: Color | null | undefined, b: Color) => !!a && channels(a).join() === channels(b).join();

/**
 * Space colours, text size, density and sidebar width. Sliders apply shortly
 * after they stop moving: each change re-lays out the pages.
 */
export function AppearanceSection({ info, update, reload }: { info: Info; update: (patch: SettingsPatch) => void; reload: () => void }) {
  const saved = info.settings.appearance;
  // What the controls changed and is not saved yet. The rest follows the
  // saved values: the sidebar's edge changes its width too.
  const [pending, setPending] = useState<Partial<Appearance>>({});
  const draft = { ...saved, ...pending };
  // The saved values and `update` as they are when the delayed save runs.
  const latest = useRef({ saved, update });
  latest.current = { saved, update };
  const timer = useRef<number | undefined>(undefined);
  const set = (patch: Partial<Appearance>, now = false) => {
    const changed = { ...pending, ...patch };
    setPending(changed);
    applyAppearance({ ...saved, ...changed });
    const save = () => {
      latest.current.update({ appearance: { ...latest.current.saved, ...changed } });
      setPending({});
    };
    window.clearTimeout(timer.current);
    if (now) save();
    else timer.current = window.setTimeout(save, 250);
  };
  const colour = (id: string, c: Color | null) => send({ cmd: "set_space_color", id, color: c }).then(reload);
  return (
    <>
      <Field label="Space colours" hint="Tints the sidebar and outlines the focused page in split view. Each space keeps its own; they sync with your spaces.">
        <div className="list">
          {info.spaces.map((sp) => (
            <div key={sp.id} className="list-row">
              <span className="icon-slot" style={{ color: toHex(sp.accent) }}><SpaceGlyph icon={sp.icon} title={sp.title} size={15} /></span>
              <div className="min-w-0 flex-1 truncate">
                {sp.title}
                {sp.id === info.active_space && <span className="text-[var(--muted)]"> · current</span>}
              </div>
              <div className="swatches">
                {SPACE_COLOURS.map(([name, c]) => (
                  <button key={name} className="swatch" title={name} aria-label={name} data-on={sameColour(sp.accent, c) || undefined} style={{ background: accentCss(c) }} onClick={() => colour(sp.id, c)} />
                ))}
                <label className="swatch custom" title="Any colour">
                  <input type="color" value={toHex(sp.accent)} onChange={(e) => colour(sp.id, fromHex(e.target.value))} />
                </label>
              </div>
            </div>
          ))}
        </div>
      </Field>
      <Field label="Layout" hint="Where back, forward, the address and the extensions sit: at the top of the sidebar, or in a bar across the window.">
        <div className="seg">
          {([["Sidebar", false], ["Top bar", true]] as const).map(([label, on]) => (
            <button key={label} data-on={draft.top_bar === on || undefined} onClick={() => set({ top_bar: on }, true)}>{label}</button>
          ))}
        </div>
      </Field>
      {!IS_MAC && (
        <Field label="Title bar" hint="Magnetite's own window buttons, or the system's title bar and frame. From the next start.">
          <div className="seg">
            {([["Magnetite", false], ["System", true]] as const).map(([label, on]) => (
              <button key={label} data-on={draft.system_title_bar === on || undefined} onClick={() => set({ system_title_bar: on }, true)}>{label}</button>
            ))}
          </div>
        </Field>
      )}
      <Field label="Text size" hint="The sidebar, the command bar and these pages. Web pages zoom on their own with ⌘+ and ⌘−.">
        <Slider value={draft.text_scale} min={85} max={130} step={5} unit="%" reset={100} onChange={(v, now) => set({ text_scale: v }, now)} />
      </Field>
      <Field label="Density" hint="Height and spacing of the sidebar rows.">
        <div className="seg">
          {(["compact", "comfortable", "spacious"] as const).map((d) => (
            <button key={d} data-on={draft.density === d || undefined} onClick={() => set({ density: d }, true)}>{d[0]!.toUpperCase() + d.slice(1)}</button>
          ))}
        </div>
      </Field>
      <Field label="Sidebar width" hint="The pages take the rest of the window.">
        <Slider value={draft.sidebar_width} min={220} max={360} step={4} unit=" pt" reset={264} onChange={(v, now) => set({ sidebar_width: v }, now)} />
      </Field>
    </>
  );
}
