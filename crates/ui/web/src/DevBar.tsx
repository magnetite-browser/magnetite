// Developer tools for the page in front: shown for a local page (this
// computer, or a host named in Settings → Developer tools), for any page shown as
// something else, and while a page is recorded. Whose project the page is,
// its console errors, the DevTools panels a click away, emulation and the
// time zone, the responsive split, a capture set, a recording, and a reset.
// They sit in a bar of their own across the top of the page area (`DevBar`).
import { useEffect, useLayoutEffect, useState } from "react";
import { windowBackground } from "./backgrounds";
import { record } from "./recorder";
import { useDismiss } from "./dismiss";
import { CaptureSetIcon, Check, ClearDataIcon, ConsoleIcon, EmulateIcon, PickerIcon, RecordIcon, RequestsIcon, Reset, ResponsiveIcon, StopIcon, TimezoneIcon } from "./Icons";
import { Switch } from "./settings/fields";
import { keysHint } from "./settings/shortcutKeys";
import { IS_MAC, accentCss, applyAppearance, dockedSide, findTab, hostOf, lookOf, onRecord, send, subscribe, type DevState, type Emulation, type TabItem, type TopBarPopupKind, type UiState } from "./bridge";

/** The time zones offered, west to east; any other is set from the command bar. */
export const TIMEZONES: [zone: string, place: string][] = [
  ["America/Los_Angeles", "Los Angeles"],
  ["America/New_York", "New York"],
  ["America/Sao_Paulo", "São Paulo"],
  ["UTC", "UTC"],
  ["Europe/London", "London"],
  ["Europe/Berlin", "Berlin"],
  ["Asia/Dubai", "Dubai"],
  ["Asia/Kolkata", "Kolkata"],
  ["Asia/Shanghai", "Shanghai"],
  ["Asia/Tokyo", "Tokyo"],
  ["Australia/Sydney", "Sydney"],
  ["Pacific/Auckland", "Auckland"],
];

/** A time zone by its place: "Tokyo" for `Asia/Tokyo`. */
export function placeOf(zone: string): string {
  return TIMEZONES.find(([z]) => z === zone)?.[1] ?? zone.split("/").pop()?.replaceAll("_", " ") ?? zone;
}

/** "14:05 · GMT+9": the time in `zone` now, and how far it is from Greenwich. */
function timeIn(zone: string | undefined): string {
  const parts = new Intl.DateTimeFormat("en-GB", { timeZone: zone, hour: "2-digit", minute: "2-digit", timeZoneName: "shortOffset" }).formatToParts();
  const part = (type: Intl.DateTimeFormatPartTypes) => parts.find((p) => p.type === type)?.value ?? "";
  return `${part("hour")}:${part("minute")} · ${part("timeZoneName")}`;
}

/** What the page is shown as, in a few words each; empty when as it is. */
export function emulationLabels(e: Emulation): string[] {
  const out: string[] = [];
  if (e.color_scheme) out.push(e.color_scheme === "dark" ? "Dark" : "Light");
  if (e.reduced_motion) out.push("Reduced motion");
  if (e.print) out.push("Print");
  if (e.network !== "online") out.push(e.network === "offline" ? "Offline" : "Slow network");
  if (e.slow_cpu) out.push("Slow CPU");
  if (e.no_cache) out.push("No cache");
  if (e.timezone) out.push(`${placeOf(e.timezone)} time`);
  return out;
}

/** Whether the tools show for a page: a local one, one shown as something
 * else, half of a responsive split, or any while a recording runs (to end it). */
export function showsDevTools(state: UiState, dev: DevState | undefined): dev is DevState {
  if (!dev) return false;
  return (dev.local && state.settings.developer.enabled) || emulationLabels(dev.emulation).length > 0 || dev.responsive || state.recording !== null;
}

/** The error count as a badge shows it. */
export function errorCount(n: number): string {
  return n > 99 ? "99+" : String(n);
}

/** What the page is shown as. Fills a window of its own below the developer bar. */
export function EmulationPanel({ emulation, onClose }: { emulation: Emulation; onClose: () => void }) {
  useDismiss(onClose, { takeFocus: false });
  const set = (patch: Partial<Emulation>) => send({ cmd: "dev_emulate", patch });
  // The time zone has a panel of its own.
  const on = emulationLabels({ ...emulation, timezone: null }).length > 0;
  return (
    <div className="site-panel dev-panel" role="dialog" aria-label="Show the page as" tabIndex={-1}>
      <div className="panel-group" style={{ borderTop: 0 }}>
        <div className="panel-label">Show the page as</div>
        <div className="panel-row">
          <span className="flex-1">Colours</span>
          <div className="seg sm">
            {([[null, "System"], ["light", "Light"], ["dark", "Dark"]] as const).map(([v, label]) => (
              <button key={label} data-on={emulation.color_scheme === v || undefined} onClick={() => set({ color_scheme: v })}>{label}</button>
            ))}
          </div>
        </div>
        <div className="panel-row">
          <span className="flex-1">Network</span>
          <div className="seg sm">
            {([["online", "Online"], ["slow", "Slow"], ["offline", "Offline"]] as const).map(([v, label]) => (
              <button key={v} data-on={emulation.network === v || undefined} onClick={() => set({ network: v })}>{label}</button>
            ))}
          </div>
        </div>
        {([
          ["reduced_motion", "Reduced motion", "prefers-reduced-motion: reduce"],
          ["print", "Print styles", "The page as it prints"],
          ["slow_cpu", "Slow processor", "Four times slower"],
          ["no_cache", "No cache", "Every request goes to the server"],
        ] as const).map(([key, label, hint]) => (
          <label key={key} className="panel-row" title={hint}>
            <span className="flex-1">{label}</span>
            <Switch label={label} checked={emulation[key]} onChange={(v) => set({ [key]: v })} />
          </label>
        ))}
      </div>
      {on && (
        <div className="panel-group">
          <button className="panel-row w-full" onClick={() => set({ color_scheme: null, reduced_motion: false, print: false, network: "online", slow_cpu: false, no_cache: false })}>
            <span className="flex-1 text-left">Show the page as it is</span>
          </button>
        </div>
      )}
    </div>
  );
}

/** The time zone the page sees: this computer's, or one of `TIMEZONES`. */
export function TimezonePanel({ emulation, onClose }: { emulation: Emulation; onClose: () => void }) {
  useDismiss(onClose, { takeFocus: false });
  const pick = (timezone: string | null) => {
    void send({ cmd: "dev_emulate", patch: { timezone } });
    onClose();
  };
  const row = (zone: string | null, place: string) => (
    <button key={place} className="panel-row w-full" role="menuitemradio" aria-checked={emulation.timezone === zone} onClick={() => pick(zone)}>
      <span className="w-4 shrink-0">{emulation.timezone === zone && <Check width={14} height={14} />}</span>
      <span className="flex-1 truncate text-left">{place}</span>
      <span className="shrink-0 text-[12px] tabular-nums text-[var(--muted)]">{timeIn(zone ?? undefined)}</span>
    </button>
  );
  return (
    <div className="site-panel dev-panel" role="menu" aria-label="Time zone of the page" tabIndex={-1}>
      <div className="panel-group" style={{ borderTop: 0 }}>
        <div className="panel-label">Time zone of the page</div>
        {row(null, "This computer's")}
        {TIMEZONES.map(([zone, place]) => row(zone, place))}
      </div>
    </div>
  );
}

/** "0:07": how long the recording runs. */
function useElapsed(since: number | undefined): string {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    if (since === undefined) return;
    setNow(Date.now());
    const tick = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(tick);
  }, [since]);
  const seconds = Math.max(0, Math.floor((now - (since ?? now)) / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

type PanelKind = Extract<TopBarPopupKind, "emulate" | "timezone">;

/** The tools as buttons. `onPanel` opens a panel below `anchor`. */
function DevTools({ state, dev, clears, onPanel }: { state: UiState; dev: DevState; clears: boolean; onPanel: (kind: PanelKind, anchor: HTMLElement) => void }) {
  const shown = emulationLabels({ ...dev.emulation, timezone: null });
  const zone = dev.emulation.timezone;
  const elapsed = useElapsed(state.recording?.since);
  return (
    <>
      <button className="dev-btn no-drag" title={`Pick an element${keysHint(state.keys, "dev_picker")}`} aria-label="Pick an element" onClick={() => send({ cmd: "dev_panel", panel: "picker" })}>
        <PickerIcon width={14} height={14} />
      </button>
      <button
        className="dev-btn no-drag"
        data-errors={dev.errors > 0 || undefined}
        title={`${dev.errors > 0 ? `${dev.errors} error${dev.errors === 1 ? "" : "s"} in the console` : "Console"}${keysHint(state.keys, "dev_console")}`}
        aria-label={dev.errors > 0 ? `Console, ${dev.errors} errors` : "Console"}
        onClick={() => send({ cmd: "dev_panel", panel: "console" })}
      >
        <ConsoleIcon width={14} height={14} />
        {dev.errors > 0 && <span className="dev-count">{errorCount(dev.errors)}</span>}
      </button>
      <button className="dev-btn no-drag" title={`Network${keysHint(state.keys, "dev_network")}`} aria-label="Network" onClick={() => send({ cmd: "dev_panel", panel: "network" })}>
        <RequestsIcon width={14} height={14} />
      </button>
      <span className="dev-sep" aria-hidden />
      <button
        className="dev-btn no-drag"
        data-on={shown.length > 0 || undefined}
        title={shown.length ? `Shown as: ${shown.join(", ")}` : "Show the page as…"}
        aria-label={shown.length ? `Shown as: ${shown.join(", ")}` : "Show the page as"}
        onClick={(e) => onPanel("emulate", e.currentTarget)}
      >
        <EmulateIcon width={14} height={14} />
      </button>
      <button
        className="dev-btn no-drag"
        data-on={zone ? true : undefined}
        title={zone ? `The page sees ${placeOf(zone)} time (${zone})` : "Time zone of the page…"}
        aria-label={zone ? `Time zone: ${placeOf(zone)}` : "Time zone of the page"}
        onClick={(e) => onPanel("timezone", e.currentTarget)}
      >
        <TimezoneIcon width={14} height={14} />
        {zone && <span className="dev-count">{placeOf(zone)}</span>}
      </button>
      <span className="dev-sep" aria-hidden />
      <button
        className="dev-btn no-drag"
        data-on={dev.responsive || undefined}
        title={dev.responsive ? "End the responsive split: the phone closes" : "Responsive split: this page as a phone beside itself, both at one address"}
        aria-label="Responsive split"
        aria-pressed={dev.responsive}
        onClick={() => send({ cmd: "dev_responsive" })}
      >
        <ResponsiveIcon width={14} height={14} />
      </button>
      <button className="dev-btn no-drag" title="Capture set: light and dark at a phone's, a tablet's and a computer's size, as one picture" aria-label="Capture set" onClick={() => send({ cmd: "dev_capture_set" })}>
        <CaptureSetIcon width={14} height={14} />
      </button>
      <button
        className="dev-btn no-drag"
        data-recording={state.recording ? true : undefined}
        title={state.recording ? "End the recording: the video goes to Downloads" : "Record the page as a video"}
        aria-label={state.recording ? `End the recording, ${elapsed}` : "Record the page"}
        onClick={() => send({ cmd: "dev_record" })}
      >
        {state.recording ? <StopIcon width={11} height={11} fill="currentColor" /> : <RecordIcon width={14} height={14} />}
        {state.recording && <span className="dev-count">{elapsed}</span>}
      </button>
      <span className="dev-sep" aria-hidden />
      {/* A page this computer serves starts from nothing; any other is only loaded again. */}
      {clears ? (
        <button className="dev-btn no-drag" title={`Clear this site's cookies, storage, caches and service workers, then load the page again${keysHint(state.keys, "dev_reset")}`} aria-label="Clear site data and reload" onClick={() => send({ cmd: "dev_reset" })}>
          <ClearDataIcon width={14} height={14} />
        </button>
      ) : (
        <button className="dev-btn no-drag" title={`Load the page again past the cache; its cookies and storage stay${keysHint(state.keys, "dev_reset")}`} aria-label="Reload past the cache" onClick={() => send({ cmd: "dev_reset" })}>
          <Reset width={14} height={14} />
        </button>
      )}
    </>
  );
}

/** Whose page it is: the project's name, else the address; "Waiting for …" while its server is down. */
function DevName({ item, dev }: { item: TabItem; dev: DevState }) {
  return (
    <>
      {dev.waiting ? (
        <span className="dev-name" role="status" title="The page loads by itself once something listens there">
          <span className="dev-pulse" aria-hidden />
          <span className="truncate">Waiting for {dev.waiting}</span>
        </span>
      ) : (
        <span className="dev-name" title={dev.project ? `${dev.project} serves ${hostOf(item.url)}` : item.url}>
          <span className="truncate">{dev.project ?? hostOf(item.url)}</span>
        </span>
      )}
    </>
  );
}

/** The sizes of the bar's pop-ups, in points. */
const POPUP = { emulate: { width: 300, height: 330 }, timezone: { width: 280, height: 470 } } as const;

/** The top bar's height in window points: the window begins that far above this bar while it is on. */
const TOP_BAR = 42;

/**
 * The developer bar (magnetite://ui/devbar): across the top of the page
 * area while the page in front has developer tools. The name at its left,
 * the tools at its right; their panels open as windows below it.
 */
export function DevBar() {
  const [state, setState] = useState<UiState | null>(null);
  useEffect(() => subscribe(setState), []);
  useEffect(() => {
    send({ cmd: "get_state" }).then((s) => window.__magnetite?.setState(s));
  }, []);
  useEffect(() => onRecord(() => void record()), []);
  const appearance = state?.settings.appearance;
  useLayoutEffect(() => {
    if (appearance) applyAppearance(appearance);
  }, [appearance]);
  if (!state) return <div className="h-full" />;
  const item = state.active_tab ? findTab(state, state.active_tab) : null;
  const dev = item && !state.peek ? state.runtime[item.id]?.dev : undefined;
  const space = state.sidebar.spaces.find((s) => s.id === state.active_space);
  const look = lookOf(state.settings.appearance, state.active_space);
  const onPanel = (kind: PanelKind, anchor: HTMLElement) => {
    const r = anchor.getBoundingClientRect();
    const { width, height } = POPUP[kind];
    send({ cmd: "top_bar_popup", popup: kind, dev_bar: true, x: Math.max(8, Math.round(r.right - width)), y: Math.round(r.bottom + 4), width, height });
  };
  // With the sidebar hidden and no top bar, macOS' window buttons lie over the bar's left end.
  const buttons = IS_MAC && !state.sidebar_visible && !state.settings.appearance.top_bar;
  return (
    <div
      className="devbar"
      data-buttons={buttons || undefined}
      style={{ ["--space" as string]: accentCss(space?.accent), ["--bg" as string]: state.chrome, ["--above" as string]: state.settings.appearance.top_bar ? `calc(-${TOP_BAR}px / var(--zoom, 1))` : "0px", ...windowBackground("devbar", state.chrome, state.active_space, look, dockedSide(state.settings.appearance), space?.accent) }}
    >
      {item && showsDevTools(state, dev) && (
        <div className="dev-row" data-waiting={dev.waiting ? true : undefined}>
          <DevName item={item} dev={dev} />
          <span className="flex-1" />
          <DevTools state={state} dev={dev} clears={dev.clears} onPanel={onPanel} />
        </div>
      )}
    </div>
  );
}
