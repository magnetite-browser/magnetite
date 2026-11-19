// The space settings dialog (magnetite://ui/space?id=…), shown in the
// centred palette window: a space's name, icon (a glyph or a picture of the
// user's own), colour, background and own pictures in one place. Every change applies at once, so the sidebar
// beside it shows it. Esc, Done or a click elsewhere closes it.
import { useEffect, useRef, useState } from "react";
import { GlyphGrid, SpaceIcon, glyphFor } from "./Glyphs";
import { ICON_SIDE } from "./backgrounds";
import { accentCss, applyAppearance, lookOf, send, type Appearance, type Space, type SpaceLook, type UiState } from "./bridge";
import { ColourSwatches, EFFECTS, OwnImage } from "./settings/AppearanceSection";
import { Slider } from "./settings/fields";

export function SpaceDialog() {
  const [state, setState] = useState<UiState | null>(null);
  const id = new URLSearchParams(location.search).get("id");
  useEffect(() => {
    document.title = "Space settings";
    send({ cmd: "get_state" }).then((s) => {
      applyAppearance(s.settings.appearance);
      setState(s);
    });
  }, []);
  const space = state?.sidebar.spaces.find((s) => s.id === id);
  // No such space (deleted meanwhile), or the private one, which keeps its look.
  const editable = space && space.id !== state?.private_space;
  useEffect(() => {
    if (state && !editable) send({ cmd: "palette_close" });
  }, [state, editable]);
  return state && space && editable ? <Form space={space} appearance={state.settings.appearance} /> : null;
}

function Form({ space, appearance: opened }: { space: Space; appearance: Appearance }) {
  // The settings as the app last told them: a stored picture changes the look there.
  const [appearance, setAppearance] = useState(opened);
  const [title, setTitle] = useState(space.title);
  const [icon, setIcon] = useState(space.icon);
  const [accent, setAccent] = useState(space.accent);
  const [look, setLook] = useState(() => lookOf(appearance, space.id));
  const root = useRef<HTMLFormElement>(null);
  // The name as last saved, and as typed: the close handlers below are set up once.
  const saved = useRef(space.title);
  const typed = useRef(title);
  typed.current = title;

  /** Save the typed name, and `glyph` with it when one was picked. An empty name keeps the saved one. */
  const rename = (glyph?: string) => {
    const name = typed.current.trim() || saved.current;
    if (name === saved.current && !glyph) return;
    saved.current = name;
    return send({ cmd: "rename_space", id: space.id, title: name, icon: glyph });
  };
  // The look not saved yet (a slider that just moved), saved at the latest on closing.
  const unsaved = useRef<(() => void) | null>(null);
  const close = () => {
    void rename();
    unsaved.current?.();
    void send({ cmd: "palette_close" });
  };
  // The system's file dialog is open: it takes the keyboard, which is no click elsewhere.
  const choosing = useRef(false);
  useEffect(() => {
    const key = (e: KeyboardEvent) => e.key === "Escape" && (e.preventDefault(), close());
    const blur = () => !choosing.current && close();
    const focus = () => (choosing.current = false);
    window.addEventListener("keydown", key);
    window.addEventListener("blur", blur);
    window.addEventListener("focus", focus);
    return () => {
      window.removeEventListener("keydown", key);
      window.removeEventListener("blur", blur);
      window.removeEventListener("focus", focus);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // The window follows the form's height.
  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const ro = new ResizeObserver(() => send({ cmd: "palette_resize", height: Math.ceil(el.getBoundingClientRect().height) }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // The look is saved shortly after a slider stops moving: each change re-lays out the pages.
  const timer = useRef<number | undefined>(undefined);
  const changeLook = (patch: Partial<SpaceLook>, now: boolean) => {
    const next = { ...look, ...patch };
    setLook(next);
    const save = () => {
      window.clearTimeout(timer.current);
      unsaved.current = null;
      void send({ cmd: "set_settings", settings: { appearance: { ...appearance, looks: { ...appearance.looks, [space.id]: next } } } });
    };
    window.clearTimeout(timer.current);
    unsaved.current = save;
    if (now) save();
    else timer.current = window.setTimeout(save, 250);
  };

  /**
   * After a picture was stored or removed: its stamp, and the effect the app
   * chose with it. A look not saved yet is saved first, so what comes back
   * has it.
   */
  const reload = () => {
    unsaved.current?.();
    return send({ cmd: "get_state" }).then((s) => {
      setAppearance(s.settings.appearance);
      setLook(lookOf(s.settings.appearance, space.id));
    });
  };

  const glyph = glyphFor(icon);
  return (
    // The dialog's own controls take the space colour too.
    <form ref={root} className="sf space-dialog" style={{ ["--space" as string]: accentCss(accent), ["--accent" as string]: accentCss(accent) }}
      onClickCapture={(e) => {
        if (e.target instanceof HTMLInputElement && e.target.type === "file") choosing.current = true;
      }}
      onSubmit={(e) => (e.preventDefault(), close())}>
      <header className="sf-head">
        <span className="sf-icon" style={{ background: accentCss(accent, 0.16), color: "var(--space)" }}><SpaceIcon space={{ id: space.id, icon, title: title || space.title }} looks={{ [space.id]: look }} size={18} /></span>
        <div className="min-w-0 flex-1">
          <h1>Space settings</h1>
          <p>Changes apply as you make them.</p>
        </div>
      </header>

      <Row label="Name">
        <input className="field" value={title} aria-label="Name" autoFocus onFocus={(e) => e.target.select()} onChange={(e) => setTitle(e.target.value)} onBlur={() => rename()} />
      </Row>

      <Row label="Icon">
        <GlyphGrid
          current={look.icon_image == null ? glyph : null}
          onPick={(g) => {
            setIcon(g);
            // A glyph takes the place of the own picture (`rename_space`).
            void rename(g)?.then(reload);
          }}
        />
        <div className="mt-2">
          <OwnImage slot="icon" space={space.id} stamp={look.icon_image} maxSide={ICON_SIDE} reload={reload} />
        </div>
        <div className="hint">Or a picture of your own, such as a PNG or an SVG. It stays on this device; your other devices show the icon chosen above.</div>
      </Row>

      <Row label="Colour">
        <div className="flex min-h-8 items-center">
          <ColourSwatches
            value={accent}
            onPick={(c) => {
              setAccent(c);
              void send({ cmd: "set_space_color", id: space.id, color: c });
            }}
          />
        </div>
      </Row>

      <Row label="Strength" hint="How much of the colour the window takes.">
        <Slider value={look.tint} min={0} max={30} step={1} unit="%" reset={8} onChange={(v, now) => changeLook({ tint: v }, now)} />
      </Row>

      <Row label="Background" hint="An effect in the space colour across the window, or a picture of your own under the sidebar's rows.">
        <div className="seg sm mb-2">
          {EFFECTS.map(([label, effect]) => (
            <button key={effect} type="button" data-on={look.sidebar_effect === effect || undefined} onClick={() => changeLook({ sidebar_effect: effect }, true)}>{label}</button>
          ))}
          {look.sidebar_image != null && (
            <button type="button" data-on={look.sidebar_effect === "image" || undefined} onClick={() => changeLook({ sidebar_effect: "image" }, true)}>Picture</button>
          )}
        </div>
        <OwnImage slot="sidebar" space={space.id} stamp={look.sidebar_image} maxSide={1600} reload={reload} />
      </Row>

      <Row label="Empty window" hint="A picture behind the window while no tab is open in this space.">
        <OwnImage slot="empty" space={space.id} stamp={look.empty_image} maxSide={2880} reload={reload} />
      </Row>

      <footer className="sf-foot">
        <span className="flex-1" />
        <button type="submit" className="btn primary">Done</button>
      </footer>
    </form>
  );
}

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="sf-row">
      <label>{label}</label>
      <div className="min-w-0">
        {children}
        {hint && <div className="hint">{hint}</div>}
      </div>
    </div>
  );
}
