// Developer tools for the page in front: shown for a local page (this
// computer, or a host named in Settings → Developer tools), for any page shown as
// something else, and while a page is recorded. Whose project the page is,
// its console errors, the DevTools panels a click away, emulation and the
// time zone, the responsive split, a capture set, a recording, and a reset.
// They sit in a bar of their own across the top of the page area (`DevBar`).
import { useEffect, useLayoutEffect, useState } from "react";
import { windowBackground } from "./backgrounds";
import { t } from "./i18n";
import { record } from "./recorder";
import { useDismiss } from "./dismiss";
import { CaptureSetIcon, Check, ClearDataIcon, ConsoleIcon, EmulateIcon, PickerIcon, RecordIcon, RequestsIcon, Reset, ResponsiveIcon, StopIcon, TimezoneIcon } from "./Icons";
import { Switch } from "./settings/fields";
import { keysHint } from "./settings/shortcutKeys";
import { IS_MAC, accentCss, applyAppearance, dockedSide, findTab, hostOf, lookOf, onRecord, send, subscribe, type DevState, type Emulation, type TabItem, type TopBarPopupKind, type UiState } from "./bridge";

/** The time zones offered, west to east; any other is set from the command bar. */
export const TIMEZONES: [zone: string, place: string][] = [
  ["America/Los_Angeles", t("developer.time_zone.places.los_angeles")],
  ["America/New_York", t("developer.time_zone.places.new_york")],
  ["America/Sao_Paulo", t("developer.time_zone.places.sao_paulo")],
  ["UTC", "UTC"],
  ["Europe/London", t("developer.time_zone.places.london")],
  ["Europe/Berlin", t("developer.time_zone.places.berlin")],
  ["Asia/Dubai", t("developer.time_zone.places.dubai")],
  ["Asia/Kolkata", t("developer.time_zone.places.kolkata")],
  ["Asia/Shanghai", t("developer.time_zone.places.shanghai")],
  ["Asia/Tokyo", t("developer.time_zone.places.tokyo")],
  ["Australia/Sydney", t("developer.time_zone.places.sydney")],
  ["Pacific/Auckland", t("developer.time_zone.places.auckland")],
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
  if (e.color_scheme) out.push(e.color_scheme === "dark" ? t("developer.emulation.labels.dark") : t("developer.emulation.labels.light"));
  if (e.reduced_motion) out.push(t("developer.emulation.labels.reduced_motion"));
  if (e.print) out.push(t("developer.emulation.labels.print"));
  if (e.network !== "online") out.push(e.network === "offline" ? t("developer.emulation.labels.offline") : t("developer.emulation.labels.slow_network"));
  if (e.slow_cpu) out.push(t("developer.emulation.labels.slow_cpu"));
  if (e.no_cache) out.push(t("developer.emulation.labels.no_cache"));
  if (e.timezone) out.push(t("developer.emulation.labels.time_zone", { place: placeOf(e.timezone) }));
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
    <div className="site-panel dev-panel" role="dialog" aria-label={t("developer.emulation.title")} tabIndex={-1}>
      <div className="panel-group" style={{ borderTop: 0 }}>
        <div className="panel-label">{t("developer.emulation.title")}</div>
        <div className="panel-row">
          <span className="flex-1">{t("developer.emulation.colours")}</span>
          <div className="seg sm">
            {([[null, t("developer.emulation.system")], ["light", t("developer.emulation.light")], ["dark", t("developer.emulation.dark")]] as const).map(([v, label]) => (
              <button key={v ?? "system"} data-on={emulation.color_scheme === v || undefined} onClick={() => set({ color_scheme: v })}>{label}</button>
            ))}
          </div>
        </div>
        <div className="panel-row">
          <span className="flex-1">{t("developer.emulation.network")}</span>
          <div className="seg sm">
            {([["online", t("developer.emulation.online")], ["slow", t("developer.emulation.slow")], ["offline", t("developer.emulation.offline")]] as const).map(([v, label]) => (
              <button key={v} data-on={emulation.network === v || undefined} onClick={() => set({ network: v })}>{label}</button>
            ))}
          </div>
        </div>
        {([
          ["reduced_motion", t("developer.emulation.reduced_motion"), "prefers-reduced-motion: reduce"],
          ["print", t("developer.emulation.print"), t("developer.emulation.print_hint")],
          ["slow_cpu", t("developer.emulation.slow_cpu"), t("developer.emulation.slow_cpu_hint")],
          ["no_cache", t("developer.emulation.no_cache"), t("developer.emulation.no_cache_hint")],
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
            <span className="flex-1 text-left">{t("developer.emulation.show_as_is")}</span>
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
    <button key={zone ?? "system"} className="panel-row w-full" role="menuitemradio" aria-checked={emulation.timezone === zone} onClick={() => pick(zone)}>
      <span className="w-4 shrink-0">{emulation.timezone === zone && <Check width={14} height={14} />}</span>
      <span className="flex-1 truncate text-left">{place}</span>
      <span className="shrink-0 text-[12px] tabular-nums text-[var(--muted)]">{timeIn(zone ?? undefined)}</span>
    </button>
  );
  return (
    <div className="site-panel dev-panel" role="menu" aria-label={t("developer.time_zone.title")} tabIndex={-1}>
      <div className="panel-group" style={{ borderTop: 0 }}>
        <div className="panel-label">{t("developer.time_zone.title")}</div>
        {row(null, t("developer.time_zone.this_computer"))}
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
      <button className="dev-btn no-drag" title={`${t("developer.bar.picker")}${keysHint(state.keys, "dev_picker")}`} aria-label={t("developer.bar.picker")} onClick={() => send({ cmd: "dev_panel", panel: "picker" })}>
        <PickerIcon width={14} height={14} />
      </button>
      <button
        className="dev-btn no-drag"
        data-errors={dev.errors > 0 || undefined}
        title={`${dev.errors > 0 ? t("developer.bar.console_errors", { count: dev.errors }) : t("developer.bar.console")}${keysHint(state.keys, "dev_console")}`}
        aria-label={dev.errors > 0 ? t("developer.bar.console_errors_label", { count: dev.errors }) : t("developer.bar.console")}
        onClick={() => send({ cmd: "dev_panel", panel: "console" })}
      >
        <ConsoleIcon width={14} height={14} />
        {dev.errors > 0 && <span className="dev-count">{errorCount(dev.errors)}</span>}
      </button>
      <button className="dev-btn no-drag" title={`${t("developer.bar.network")}${keysHint(state.keys, "dev_network")}`} aria-label={t("developer.bar.network")} onClick={() => send({ cmd: "dev_panel", panel: "network" })}>
        <RequestsIcon width={14} height={14} />
      </button>
      <span className="dev-sep" aria-hidden />
      <button
        className="dev-btn no-drag"
        data-on={shown.length > 0 || undefined}
        title={shown.length ? t("developer.bar.shown_as", { list: shown.join(", ") }) : t("developer.bar.show_as")}
        aria-label={shown.length ? t("developer.bar.shown_as", { list: shown.join(", ") }) : t("developer.bar.show_as_label")}
        onClick={(e) => onPanel("emulate", e.currentTarget)}
      >
        <EmulateIcon width={14} height={14} />
      </button>
      <button
        className="dev-btn no-drag"
        data-on={zone ? true : undefined}
        title={zone ? t("developer.bar.time_zone_on", { place: placeOf(zone), zone }) : t("developer.bar.time_zone")}
        aria-label={zone ? t("developer.bar.time_zone_on_label", { place: placeOf(zone) }) : t("developer.bar.time_zone_label")}
        onClick={(e) => onPanel("timezone", e.currentTarget)}
      >
        <TimezoneIcon width={14} height={14} />
        {zone && <span className="dev-count">{placeOf(zone)}</span>}
      </button>
      <span className="dev-sep" aria-hidden />
      <button
        className="dev-btn no-drag"
        data-on={dev.responsive || undefined}
        title={dev.responsive ? t("developer.bar.responsive_end") : t("developer.bar.responsive_start")}
        aria-label={t("developer.bar.responsive")}
        aria-pressed={dev.responsive}
        onClick={() => send({ cmd: "dev_responsive" })}
      >
        <ResponsiveIcon width={14} height={14} />
      </button>
      <button className="dev-btn no-drag" title={t("developer.bar.capture_set_hint")} aria-label={t("developer.bar.capture_set")} onClick={() => send({ cmd: "dev_capture_set" })}>
        <CaptureSetIcon width={14} height={14} />
      </button>
      <button
        className="dev-btn no-drag"
        data-recording={state.recording ? true : undefined}
        title={state.recording ? t("developer.bar.record_end_hint") : t("developer.bar.record_hint")}
        aria-label={state.recording ? t("developer.bar.record_end", { elapsed }) : t("developer.bar.record")}
        onClick={() => send({ cmd: "dev_record" })}
      >
        {state.recording ? <StopIcon width={11} height={11} fill="currentColor" /> : <RecordIcon width={14} height={14} />}
        {state.recording && <span className="dev-count">{elapsed}</span>}
      </button>
      <span className="dev-sep" aria-hidden />
      {/* A page this computer serves starts from nothing; any other is only loaded again. */}
      {clears ? (
        <button className="dev-btn no-drag" title={`${t("developer.bar.clear_hint")}${keysHint(state.keys, "dev_reset")}`} aria-label={t("developer.bar.clear")} onClick={() => send({ cmd: "dev_reset" })}>
          <ClearDataIcon width={14} height={14} />
        </button>
      ) : (
        <button className="dev-btn no-drag" title={`${t("developer.bar.reload_hint")}${keysHint(state.keys, "dev_reset")}`} aria-label={t("developer.bar.reload")} onClick={() => send({ cmd: "dev_reset" })}>
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
        <span className="dev-name" role="status" title={t("developer.bar.waiting_hint")}>
          <span className="dev-pulse" aria-hidden />
          <span className="truncate">{t("developer.bar.waiting", { address: dev.waiting })}</span>
        </span>
      ) : (
        <span className="dev-name" title={dev.project ? t("developer.bar.serves", { project: dev.project, host: hostOf(item.url) }) : item.url}>
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
