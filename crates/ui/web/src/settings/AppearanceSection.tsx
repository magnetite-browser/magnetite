// Settings → Appearance: a space's name, icon, colour and its strength, backgrounds; layout, text size, density, sidebar width.
import { useRef, useState } from "react";
import { ICON_SIDE, storeImage, userImageUrl, type ImageSlot } from "../backgrounds";
import { GlyphGrid, glyphFor } from "../Glyphs";
import { DEFAULT_ACCENT, accentCss, applyAppearance, keyLabel, lookOf, send, type Appearance, type Color, type SpaceLook } from "../bridge";
import { Field, Group, Slider, Toggle } from "./fields";
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
const sameColour = (a: Color, b: Color) => channels(a).join() === channels(b).join();

/** The space colours to pick from and the colour picker; `value` is the space's colour (none: the default). */
export function ColourSwatches({ value, onPick }: { value: Color | null | undefined; onPick: (c: Color) => void }) {
  return (
    <div className="swatches">
      {SPACE_COLOURS.map(([name, c]) => (
        <button key={name} type="button" className="swatch" title={name} aria-label={name} data-on={sameColour(value ?? DEFAULT_ACCENT, c) || undefined} style={{ background: accentCss(c) }} onClick={() => onPick(c)} />
      ))}
      <label className="swatch custom" title="Any colour">
        <input type="color" value={toHex(value)} onChange={(e) => onPick(fromHex(e.target.value))} />
      </label>
    </div>
  );
}

/** Chromium's zoom steps, in percent (`ZOOM_STEPS` in the engine). */
const PAGE_ZOOM = [25, 33, 50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300, 400, 500];

/** The built-in sidebar effects; the user's own picture is offered once one is stored. */
export const EFFECTS = [["None", "none"], ["Grain", "grain"], ["Glow", "glow"], ["Aurora", "aurora"], ["Fade", "fade"]] as const;

/**
 * Choose or remove space `space`'s own picture for `slot`, with a small preview of the
 * stored one (`stamp`). The picture is scaled down to `maxSide` before it is
 * stored (an icon: into a square of that size); it stays on this device.
 */
export function OwnImage({ slot, space, stamp, maxSide, reload }: { slot: ImageSlot; space: string; stamp: number | null; maxSide: number; reload: () => void }) {
  const [error, setError] = useState<string | null>(null);
  // The file input stays out of sight; the button opens its picker, so the keyboard reaches it too.
  const picker = useRef<HTMLInputElement>(null);
  const store = (file: File | null) =>
    storeImage(slot, space, file, maxSide).then(() => setError(null), (e: Error) => setError(e.message)).then(reload);
  return (
    <div className="flex items-center gap-3">
      {stamp != null && <img className={slot === "icon" ? "h-8 w-8 object-contain" : "h-8 w-12 rounded-md object-cover"} src={userImageUrl(slot, space, stamp)} alt="" />}
      <button type="button" className="btn sm" onClick={() => picker.current?.click()}>{stamp == null ? "Choose picture…" : "Change…"}</button>
      <input
        ref={picker}
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
      {stamp != null && <button type="button" className="btn sm" onClick={() => void store(null)}>Remove</button>}
      {error && <span className="text-[12.5px] text-[#f3a0a0]">{error}</span>}
    </div>
  );
}

/**
 * A space's name, icon, colour and its strength, backgrounds, text size, density and
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
      <Group title="Spaces" hint="Name, icon, colour and backgrounds belong to a space: each one can look different.">
      <Field side label="Space" hint="The space the settings below are for.">
        <select className="field" value={sp.id} onChange={(e) => setChosen(e.target.value)} aria-label="Space">
          {info.spaces.map((s) => <option key={s.id} value={s.id}>{s.title}{s.id === info.active_space ? " (current)" : ""}</option>)}
        </select>
      </Field>
      <Field side label="Name" hint="Shown above the space's tabs. It syncs with the space.">
        <input
          key={`${sp.id} ${sp.title}`}
          className="field"
          style={{ width: 240 }}
          defaultValue={sp.title}
          aria-label="Name"
          onBlur={(e) => {
            const title = e.target.value.trim();
            if (title && title !== sp.title) void send({ cmd: "rename_space", id: sp.id, title }).then(reload);
            else e.target.value = sp.title;
          }}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
        />
      </Field>
      <Field label="Icon" hint="Or a picture of your own, such as a PNG or an SVG. It stays on this device; your other devices show the icon chosen here.">
        <div className="max-w-[420px]">
          <GlyphGrid
            current={stored.icon_image == null ? glyphFor(sp.icon) : null}
            // A glyph takes the place of the own picture (`rename_space`).
            onPick={(g) => void send({ cmd: "rename_space", id: sp.id, icon: g }).then(reload)}
          />
        </div>
        <div className="mt-2">
          <OwnImage slot="icon" space={sp.id} stamp={stored.icon_image} maxSide={ICON_SIDE} reload={reload} />
        </div>
      </Field>
      <Field side label="Colour" hint="Tints the sidebar and outlines the focused page in split view. It syncs with the space.">
        <ColourSwatches value={sp.accent} onPick={(c) => colour(sp.id, c)} />
      </Field>
      <Field side label="Colour strength" hint="How much of the space colour the window takes.">
        <Slider value={look.tint} min={0} max={30} step={1} unit="%" reset={8} onChange={(v, now) => setLook({ tint: v }, now)} />
      </Field>
      <Field label="Background" hint="An effect in the space colour across the window, or a picture of your own under the sidebar's rows.">
        <div className="flex flex-wrap items-center gap-3">
          <select className="field" style={{ width: 200 }} value={look.sidebar_effect} aria-label="Background" onChange={(e) => setLook({ sidebar_effect: e.target.value as SpaceLook["sidebar_effect"] }, true)}>
            {EFFECTS.map(([label, effect]) => <option key={effect} value={effect}>{label}</option>)}
            {stored.sidebar_image != null && <option value="image">Your picture</option>}
          </select>
          <OwnImage slot="sidebar" space={sp.id} stamp={stored.sidebar_image} maxSide={1600} reload={reload} />
        </div>
      </Field>
      <Field side label="Empty window" hint="A picture behind the window while no tab is open in this space. Without one, the logo shows there.">
        <OwnImage slot="empty" space={sp.id} stamp={stored.empty_image} maxSide={2880} reload={reload} />
      </Field>
      </Group>
      <Group title="Window" hint="The same in every space.">
      <Field side label="Layout" hint="Where back, forward, the address and the extensions sit: at the top of the sidebar, or in a bar across the window.">
        <div className="seg">
          {([["Sidebar", false], ["Top bar", true]] as const).map(([label, on]) => (
            <button key={label} data-on={draft.top_bar === on || undefined} onClick={() => set({ top_bar: on }, true)}>{label}</button>
          ))}
        </div>
      </Field>
      {!draft.top_bar && (
        <Toggle label="Extension bar" hint="A row of pinned extensions below the address, with a button that lists the rest. The icons leave the row above it." checked={draft.extension_bar} onChange={(on) => set({ extension_bar: on }, true)} />
      )}
      <Field side label="Text size" hint={`The sidebar, the command bar and these pages. Web pages zoom on their own with ${keyLabel("⌘+", "Ctrl +")} and ${keyLabel("⌘−", "Ctrl −")}.`}>
        <Slider value={draft.text_scale} min={85} max={130} step={5} unit="%" reset={100} onChange={(v, now) => set({ text_scale: v }, now)} />
      </Field>
      <Field side label="Page zoom" hint="The zoom a website has until you zoom it yourself. A change counts from the next start of Magnetite.">
        <select className="field" value={info.settings.page_zoom} aria-label="Page zoom" onChange={(e) => update({ page_zoom: Number(e.target.value) })}>
          {PAGE_ZOOM.map((pct) => <option key={pct} value={pct}>{pct}%</option>)}
        </select>
      </Field>
      <Field side label="Density" hint="Height and spacing of the sidebar rows.">
        <div className="seg">
          {(["compact", "comfortable", "spacious"] as const).map((d) => (
            <button key={d} data-on={draft.density === d || undefined} onClick={() => set({ density: d }, true)}>{d[0]!.toUpperCase() + d.slice(1)}</button>
          ))}
        </div>
      </Field>
      <Field side label="Sidebar width" hint="The pages take the rest of the window.">
        <Slider value={draft.sidebar_width} min={220} max={360} step={4} unit=" pt" reset={264} onChange={(v, now) => set({ sidebar_width: v }, now)} />
      </Field>
      </Group>
    </>
  );
}
