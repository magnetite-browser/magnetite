// Settings → Appearance: layout, text size, page zoom, density and sidebar
// width, the same in every space. How one space looks (colour, strength,
// backgrounds) is drawn here too, for Settings → Spaces, which shows it with
// the rest of the space: both change the same stored appearance.
import { useRef, useState, useEffect } from "react";
import { storeImage, userImageUrl, type ImageSlot } from "../backgrounds";
import { DEFAULT_ACCENT, accentCss, keyLabel, lookOf, send, type Appearance, type Color, type SpaceLook } from "../bridge";
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
      {SPACE_COLOURS.map(([name, c]) => {
        const on = sameColour(value ?? DEFAULT_ACCENT, c);
        return <button key={name} type="button" className="swatch" title={name} aria-label={name} aria-pressed={on} data-on={on || undefined} style={{ background: accentCss(c) }} onClick={() => onPick(c)} />;
      })}
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
 * Choose or remove a picture of the user's own, with a small preview of the
 * one there is (`preview`, its address; `square` for an icon). `store` keeps
 * the chosen file, or removes the picture (`null`); what it rejects with is
 * shown beside the buttons.
 */
export function OwnImage({ preview, square, store, reload }: { preview: string | null; square?: boolean; store: (file: File | null) => Promise<void>; reload: () => void }) {
  const [error, setError] = useState<string | null>(null);
  // The file input stays out of sight; the button opens its picker, so the keyboard reaches it too.
  const picker = useRef<HTMLInputElement>(null);
  const keep = (file: File | null) => store(file).then(() => setError(null), (e: Error) => setError(e.message)).then(reload);
  return (
    <div className="flex items-center gap-3">
      {preview != null && <img className={square ? "h-8 w-8 object-contain" : "h-8 w-12 rounded-md object-cover"} src={preview} alt="" />}
      <button type="button" className="btn sm" onClick={() => picker.current?.click()}>{preview == null ? "Choose picture…" : "Change…"}</button>
      <input
        ref={picker}
        className="hidden"
        type="file"
        accept="image/*"
        onChange={(e) => {
          const file = e.target.files?.[0];
          // Emptied, so choosing the same file again counts as a change.
          e.target.value = "";
          if (file) void keep(file);
        }}
      />
      {preview != null && <button type="button" className="btn sm" onClick={() => void keep(null)}>Remove</button>}
      {error && <span className="text-[12.5px] text-[#f3a0a0]">{error}</span>}
    </div>
  );
}

/**
 * Choose or remove space `space`'s own background picture for `slot`
 * (`stamp`: when one was stored). It is scaled down to `maxSide` before it
 * is stored and stays on this device.
 */
export function BackgroundImage({ slot, space, stamp, maxSide, reload }: { slot: ImageSlot; space: string; stamp: number | null; maxSide: number; reload: () => void }) {
  return <OwnImage preview={stamp == null ? null : userImageUrl(slot, space, stamp)} store={(file) => storeImage(slot, space, file, maxSide)} reload={reload} />;
}

/**
 * The appearance as the controls have it: what is saved, with what they
 * changed and is not saved yet laid over it. `set` saves shortly after a
 * slider stops moving, or at once with `now`. `preview` only moves the
 * control: for what lays the page out again (text size, sidebar width),
 * which would pull the slider away from under the pointer, so that is
 * saved when the slider is let go.
 */
function useAppearance(info: Info, update: (patch: SettingsPatch) => void) {
  const saved = info.settings.appearance;
  // The rest follows the saved values: the sidebar's edge changes its width too.
  const [pending, setPending] = useState<Partial<Appearance>>({});
  const draft = { ...saved, ...pending };
  // The saved values and `update` as they are when the delayed save runs.
  const latest = useRef({ saved, update });
  latest.current = { saved, update };
  const timer = useRef<number | undefined>(undefined);
  // The save that still waits. It runs when the fields go away (another
  // space's page takes their place): later it would write this page's
  // copy of the looks over what the next page saved meanwhile.
  const waiting = useRef<(() => void) | undefined>(undefined);
  useEffect(() => () => {
    window.clearTimeout(timer.current);
    waiting.current?.();
  }, []);
  const set = (patch: Partial<Appearance>, now = false) => {
    const changed = { ...pending, ...patch };
    setPending(changed);
    const save = () => {
      waiting.current = undefined;
      latest.current.update({ appearance: { ...latest.current.saved, ...changed } });
      setPending({});
    };
    window.clearTimeout(timer.current);
    waiting.current = save;
    if (now) save();
    else timer.current = window.setTimeout(save, 250);
  };
  const preview = (patch: Partial<Appearance>) => setPending({ ...pending, ...patch });
  return { saved, draft, set, preview };
}

/**
 * How space `space` looks: its colour and how much of it the window takes,
 * the background of the sidebar and the picture of the empty window. Rows
 * of a `Group` (Settings → Spaces).
 */
export function SpaceLookFields({ info, space, update, reload }: { info: Info; space: Info["spaces"][number]; update: (patch: SettingsPatch) => void; reload: () => void }) {
  const { saved, draft, set } = useAppearance(info, update);
  const colour = (c: Color | null) => send({ cmd: "set_space_color", id: space.id, color: c }).then(reload);
  const look = lookOf(draft, space.id);
  const setLook = (patch: Partial<SpaceLook>, now = false) => set({ looks: { ...draft.looks, [space.id]: { ...look, ...patch } } }, now);
  const stored = lookOf(saved, space.id);
  return (
    <>
      <Field side label="Colour" hint="Tints the sidebar and outlines the focused page in split view. It syncs with the space.">
        <ColourSwatches value={space.accent} onPick={colour} />
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
          <BackgroundImage slot="sidebar" space={space.id} stamp={stored.sidebar_image} maxSide={1600} reload={reload} />
        </div>
      </Field>
      <Field side label="Empty window" hint="A picture behind the window while no tab is open in this space. Without one, the logo shows there.">
        <BackgroundImage slot="empty" space={space.id} stamp={stored.empty_image} maxSide={2880} reload={reload} />
      </Field>
    </>
  );
}

/**
 * The same words at the text size in use and at the one the slider is on,
 * side by side: the page itself only takes the new size once the slider is
 * let go. The page is drawn at `from`, so the new size is set relative to
 * it. As high as the largest step up, so the rows below stay where they are.
 */
function TextSample({ from, to }: { from: number; to: number }) {
  const sample = (title: string, factor: number, on: boolean) => (
    <div className="min-w-0 flex-1">
      <div className="set-hint">{title}</div>
      <div className="flex h-9 items-center overflow-hidden whitespace-nowrap" style={{ fontSize: 14 * factor, color: on ? "var(--fg)" : "var(--secondary)" }}>Text in this size</div>
    </div>
  );
  return (
    <div className="mt-2 flex max-w-[460px] gap-4" aria-hidden>
      {sample("Now", 1, from === to)}
      {sample("New", to / from, from !== to)}
    </div>
  );
}

/** Layout, text size, page zoom, density and sidebar width. */
export function AppearanceSection({ info, update, open }: { info: Info; update: (patch: SettingsPatch) => void; open: (section: string) => void }) {
  const { draft, set, preview } = useAppearance(info, update);
  return (
    <>
      <Group title="Window" hint="The same in every space.">
      <Field side label="Layout" hint="Where back, forward, the address and the extensions sit: at the top of the sidebar, or in a bar across the window.">
        <div className="seg">
          {([["Sidebar", false], ["Top bar", true]] as const).map(([label, on]) => (
            <button key={label} data-on={draft.top_bar === on || undefined} aria-pressed={draft.top_bar === on} onClick={() => set({ top_bar: on }, true)}>{label}</button>
          ))}
        </div>
      </Field>
      {!draft.top_bar && (
        <Toggle label="Extension bar" hint="A row of pinned extensions below the address, with a button that lists the rest. The icons leave the row above it." checked={draft.extension_bar} onChange={(on) => set({ extension_bar: on }, true)} />
      )}
      <Field side label="Text size" hint={`The sidebar, the command bar and these pages. Web pages zoom on their own with ${keyLabel("⌘+", "Ctrl +")} and ${keyLabel("⌘−", "Ctrl −")}.`}>
        <Slider value={draft.text_scale} min={85} max={130} step={5} unit="%" reset={100} onChange={(v, now) => (now ? set({ text_scale: v }, true) : preview({ text_scale: v }))} />
        <TextSample from={info.settings.appearance.text_scale} to={draft.text_scale} />
      </Field>
      <Field side label="Page zoom" hint="The zoom a website has until you zoom it yourself. A change counts from the next start of Magnetite.">
        <select className="field" value={info.settings.page_zoom} aria-label="Page zoom" onChange={(e) => update({ page_zoom: Number(e.target.value) })}>
          {PAGE_ZOOM.map((pct) => <option key={pct} value={pct}>{pct}%</option>)}
        </select>
      </Field>
      <Field side label="Density" hint="Height and spacing of the sidebar rows.">
        <div className="seg">
          {(["compact", "comfortable", "spacious"] as const).map((d) => (
            <button key={d} data-on={draft.density === d || undefined} aria-pressed={draft.density === d} onClick={() => set({ density: d }, true)}>{d[0]!.toUpperCase() + d.slice(1)}</button>
          ))}
        </div>
      </Field>
      <Field side label="Sidebar width" hint="The pages take the rest of the window.">
        <Slider value={draft.sidebar_width} min={220} max={360} step={4} unit=" pt" reset={264} onChange={(v, now) => (now ? set({ sidebar_width: v }, true) : preview({ sidebar_width: v }))} />
      </Field>
      </Group>
      <Group>
        <Field side label="Colours and backgrounds" hint="Each space has a colour and a background of its own. They are with the space, in Settings → Spaces.">
          <button className="btn" onClick={() => open("spaces")}>Open Spaces</button>
        </Field>
      </Group>
    </>
  );
}
