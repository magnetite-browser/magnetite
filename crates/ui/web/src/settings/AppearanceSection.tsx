// Settings → Appearance: space colours and their strength, backgrounds, layout, text size, density, sidebar width.
import { useRef, useState } from "react";
import { SpaceGlyph } from "../Glyphs";
import { storeImage, userImageUrl, type ImageSlot } from "../backgrounds";
import { DEFAULT_ACCENT, accentCss, applyAppearance, lookOf, send, type Appearance, type Color, type SpaceLook } from "../bridge";
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

/** The built-in sidebar effects; the user's own picture is offered once one is stored. */
const EFFECTS = [["None", "none"], ["Grain", "grain"], ["Glow", "glow"], ["Aurora", "aurora"], ["Fade", "fade"]] as const;

/**
 * Choose or remove space `space`'s own picture for `slot`, with a small preview of the
 * stored one (`stamp`). The picture is scaled down to `maxSide` before it is
 * stored; it stays on this device.
 */
function OwnImage({ slot, space, stamp, maxSide, reload }: { slot: ImageSlot; space: string; stamp: number | null; maxSide: number; reload: () => void }) {
  const [error, setError] = useState<string | null>(null);
  const store = (file: File | null) =>
    storeImage(slot, space, file, maxSide).then(() => setError(null), (e: Error) => setError(e.message)).then(reload);
  return (
    <div className="flex items-center gap-3">
      {stamp != null && <img className="h-8 w-12 rounded-md object-cover" src={userImageUrl(slot, space, stamp)} alt="" />}
      <label className="btn sm inline-flex items-center">
        {stamp == null ? "Choose picture…" : "Change…"}
        <input
          className="hidden"
          type="file"
          accept="image/*"
          onChange={(e) => {
            const file = e.target.files?.[0];
            // Emptied, so choosing the same file again counts as a change.
            e.target.value = "";
            if (file) void store(file);
          }}
        />
      </label>
      {stamp != null && <button className="btn sm" onClick={() => void store(null)}>Remove</button>}
      {error && <span className="text-[12.5px] text-[#f3a0a0]">{error}</span>}
    </div>
  );
}

/**
 * Space colours and their strength, backgrounds, text size, density and
 * sidebar width. Sliders apply shortly
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
  // The space whose look the controls below change: the current one at first.
  const [chosen, setChosen] = useState(info.active_space);
  const sp = info.spaces.find((s) => s.id === chosen) ?? info.spaces[0];
  if (!sp) return null;
  const look = lookOf(draft, sp.id);
  const setLook = (patch: Partial<SpaceLook>, now = false) => set({ looks: { ...draft.looks, [sp.id]: { ...look, ...patch } } }, now);
  const stored = lookOf(saved, sp.id);
  return (
    <>
      <Field label="Space" hint="Colour, strength and backgrounds belong to a space: each one can look different.">
        <div className="flex flex-wrap gap-1.5">
          {info.spaces.map((s) => (
            <button key={s.id} className="chip inline-flex items-center gap-1.5" data-on={s.id === sp.id || undefined} onClick={() => setChosen(s.id)}>
              <span style={{ color: toHex(s.accent) }}><SpaceGlyph icon={s.icon} title={s.title} size={14} /></span>
              {s.title}
              {s.id === info.active_space && <span className="text-[var(--muted)]">· current</span>}
            </button>
          ))}
        </div>
      </Field>
      <Field label="Colour" hint="Tints the sidebar and outlines the focused page in split view. It syncs with the space.">
        <div className="swatches">
          {SPACE_COLOURS.map(([name, c]) => (
            <button key={name} className="swatch" title={name} aria-label={name} data-on={sameColour(sp.accent, c) || undefined} style={{ background: accentCss(c) }} onClick={() => colour(sp.id, c)} />
          ))}
          <label className="swatch custom" title="Any colour">
            <input type="color" value={toHex(sp.accent)} onChange={(e) => colour(sp.id, fromHex(e.target.value))} />
          </label>
        </div>
      </Field>
      <Field label="Colour strength" hint="How much of the space colour the window takes.">
        <Slider value={look.tint} min={0} max={30} step={1} unit="%" reset={8} onChange={(v, now) => setLook({ tint: v }, now)} />
      </Field>
      <Field label="Sidebar background" hint="An effect in the space colour over the sidebar, or a picture of your own under its rows.">
        <div className="flex flex-wrap items-center gap-3">
          <div className="seg">
            {EFFECTS.map(([label, effect]) => (
              <button key={effect} data-on={look.sidebar_effect === effect || undefined} onClick={() => setLook({ sidebar_effect: effect }, true)}>{label}</button>
            ))}
            {stored.sidebar_image != null && (
              <button data-on={look.sidebar_effect === "image" || undefined} onClick={() => setLook({ sidebar_effect: "image" }, true)}>Picture</button>
            )}
          </div>
          <OwnImage slot="sidebar" space={sp.id} stamp={stored.sidebar_image} maxSide={1600} reload={reload} />
        </div>
      </Field>
      <Field label="Empty window" hint="A picture behind the window while no tab is open in this space. Without one, the logo shows there.">
        <OwnImage slot="empty" space={sp.id} stamp={stored.empty_image} maxSide={2880} reload={reload} />
      </Field>
      <Field label="Layout" hint="Where back, forward, the address and the extensions sit: at the top of the sidebar, or in a bar across the window.">
        <div className="seg">
          {([["Sidebar", false], ["Top bar", true]] as const).map(([label, on]) => (
            <button key={label} data-on={draft.top_bar === on || undefined} onClick={() => set({ top_bar: on }, true)}>{label}</button>
          ))}
        </div>
      </Field>
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
