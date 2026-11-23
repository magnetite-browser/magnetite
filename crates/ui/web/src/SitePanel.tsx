// Site panel: identity and connection, quick tools for the page (capture,
// share, developer tools), then this site's permissions, zoom, ad blocking
// and sound.
// Opens under the address row; Esc or a click outside closes it.
import { useEffect, useRef, useState } from "react";
import { useDismiss } from "./dismiss";
import { Favicon } from "./Favicon";
import { BracesIcon, CaptureIcon, CodeIcon, Minus, Plus, ShareIcon, Speaker, SpeakerOff } from "./Icons";
import { Switch } from "./settings/fields";
import { keysHint, keysLabel } from "./settings/shortcutKeys";
import { findTabInSpace, hostOf, IS_MAC, onSiteDataCleared, send, ZOOM_STEPS, type ClearReport, type PermissionKind, type PermissionRow, type PermissionValue, type Reply, type TabItem, type TabRuntime, type UiState } from "./bridge";

const PERMISSION_LABEL: Record<PermissionKind, string> = {
  camera: "Camera",
  microphone: "Microphone",
  location: "Location",
  notifications: "Notifications",
  popups: "Pop-ups and redirects",
  clipboard: "Clipboard",
};

/** Ad blocking on this site, while the ad blocker is on (Settings → Content blocking). */
function BlockAds({ url }: { url: string }) {
  const [ads, setAds] = useState<{ site: string | null; enabled: boolean; blocking: boolean } | null>(null);
  useEffect(() => {
    void send({ cmd: "site_ads", url }).then(setAds);
  }, [url]);
  if (!ads?.enabled) return null;
  const set = (blocking: boolean) => {
    setAds({ ...ads, blocking });
    send({ cmd: "set_site_ads", url, blocking });
  };
  return (
    <label className="panel-row" title={`For all of ${ads.site}. The page loads again.`}>
      <span className="flex-1">Block ads</span>
      <Switch label="Block ads on this site" checked={ads.blocking} onChange={set} />
    </label>
  );
}

/** Who gets the keys of Magnetite's shortcuts first on this site: a web app with shortcuts of its own can have them all (Settings → Shortcuts). */
function SiteShortcuts({ url, saved }: { url: string; saved: string[] }) {
  const [keys, setKeys] = useState<Reply<"site_shortcuts"> | null>(null);
  // Asked again when the saved sites change (Settings, sync) while the panel is open.
  const savedSites = saved.join(" ");
  useEffect(() => {
    void send({ cmd: "site_shortcuts", url }).then(setKeys);
  }, [url, savedSites]);
  if (!keys?.site) return null;
  const set = (page_first: boolean) => {
    setKeys({ ...keys, page_first });
    send({ cmd: "set_site_shortcuts", url, page_first });
  };
  return (
    <label className="panel-row" title={`For all of ${keys.site}: the site may use the keys of Magnetite's shortcuts itself, and a shortcut runs only if the site leaves its keys alone. The digits that choose a tab or a space stay Magnetite's.`}>
      <span className="flex-1">Site's shortcuts first</span>
      <Switch label="This site gets the keys of shortcuts first" checked={keys.page_first} onChange={set} />
    </label>
  );
}

/** What the app says of the site's engine (`site_engine`). */
type SiteEngine = Reply<"site_engine">;

/**
 * Which engine shows this site: Chromium, or the system's own (WebKit on
 * macOS, WebView2 on Windows), which plays the protected video Chromium is
 * refused. Not there
 * where the system has no engine to use. A service that always opens in the
 * system's engine (Settings → Protected video) has no switch of its own.
 */
function SystemEngine({ url }: { url: string }) {
  const [engine, setEngine] = useState<SiteEngine | null>(null);
  // Why the app did not take the choice (the list of sites is full).
  const [refused, setRefused] = useState<string | null>(null);
  useEffect(() => {
    void send({ cmd: "site_engine", url }).then(setEngine);
  }, [url]);
  if (!engine?.site) return null;
  const set = (system: boolean) => {
    setRefused(null);
    setEngine({ ...engine, system });
    send({ cmd: "set_site_engine", url, system }).catch((e: unknown) => {
      setEngine(engine);
      setRefused(e instanceof Error ? e.message : String(e));
    });
  };
  return (
    <>
      <label className="panel-row" title={`For all of ${engine.site}: its pages open in ${engine.name}, ${engine.about}, which plays protected video. Extensions cannot read or fill its pages, there is no ad blocking, and the site asks you to sign in again. The page loads again.`}>
        <span className="flex-1">Open in {engine.name}</span>
        {engine.known ? <span className="text-[12px] text-[var(--muted)]">Always, for its video</span> : <Switch label={`Open this site in ${engine.name}`} checked={engine.system} onChange={set} />}
      </label>
      {refused && <div className="panel-note" role="alert">{refused}</div>}
    </>
  );
}

/** Pop-ups have no "ask" state in Chromium. */
const CHOICES = (kind: PermissionKind): PermissionValue[] => (kind === "popups" ? ["allow", "block"] : ["allow", "ask", "block"]);

function Permissions({ pageHost }: { pageHost: string }) {
  const [rows, setRows] = useState<PermissionRow[] | null>(null);
  useEffect(() => {
    send({ cmd: "site_permissions" }).then((r) => setRows(r.rows ?? []));
  }, []);
  const set = (row: PermissionRow, value: PermissionValue) => {
    send({ cmd: "set_permission", origin: row.origin, kind: row.kind, value });
    setRows((rs) => rs?.map((r) => (r === row ? { ...r, value } : r)) ?? null);
  };
  return (
    <div className="panel-group">
      <div className="panel-label">Permissions</div>
      {rows === null && <div className="panel-note">Loading…</div>}
      {rows?.length === 0 && <div className="panel-note">This site has not asked for any permissions.</div>}
      {rows?.map((row) => {
        const via = hostOf(row.origin) !== pageHost ? hostOf(row.origin) : null;
        return (
          <div key={row.kind + row.origin} className="panel-row perm">
            <span className="min-w-0 flex-1">
              <span className="block truncate">{PERMISSION_LABEL[row.kind]}</span>
              {via && <span className="block truncate text-[11.5px] text-[var(--muted)]">via {via}</span>}
            </span>
            <span className="seg sm" role="radiogroup" aria-label={PERMISSION_LABEL[row.kind]}>
              {CHOICES(row.kind).map((v) => (
                <button key={v} role="radio" aria-checked={row.value === v} data-on={row.value === v || undefined} onClick={() => set(row, v)}>
                  {v === "allow" ? "Allow" : v === "ask" ? "Ask" : "Block"}
                </button>
              ))}
            </span>
          </div>
        );
      })}
    </div>
  );
}

/**
 * "Clear cookies and site storage": confirm with the exact scope, then report
 * each part. `noSite`: the page has no network origin (a local file, a
 * built-in page), so there is nothing to clear.
 */
function ClearData({ profileName, noSite }: { profileName: string | null; noSite: boolean }) {
  const [step, setStep] = useState<"idle" | "confirm" | "working">("idle");
  const [scope, setScope] = useState<{ domain: string; origins: string[] } | null>(null);
  const [report, setReport] = useState<ClearReport | null>(null);
  // The app had no scope when last asked: nothing to clear on this page.
  const [empty, setEmpty] = useState(false);
  useEffect(() => onSiteDataCleared((r) => { setReport(r); setStep("idle"); }), []);
  // The app clears `domain` only: a page that went elsewhere meanwhile is left alone.
  const clear = (domain: string) => { setReport(null); setStep("working"); send({ cmd: "clear_site_data", domain }); };
  // Ask the app what would be cleared and confirm it. A retry skips the
  // confirmation while the page is still on the site (`retryOf`) it was given for.
  const ask = async (retryOf?: string) => {
    const r = await send({ cmd: "site_data_scope" });
    setEmpty(!r.domain);
    if (!r.domain) {
      setReport(null);
    } else if (r.domain === retryOf) {
      clear(r.domain);
    } else {
      setReport(null);
      setScope(r);
      setStep("confirm");
    }
  };
  if (step === "confirm" && scope?.domain) {
    return (
      <div className="confirm">
        <p>Remove the cookies of <b>{scope.domain}</b> and its subdomains{profileName ? <> in profile <b>{profileName}</b></> : null}, and the site storage of:</p>
        <ul>{scope.origins.map((o) => <li key={o}>{hostOf(o)}</li>)}</ul>
        <p className="text-[var(--muted)]">You will be signed out. Storage of parts of the site you did not open this session may remain.</p>
        <div className="flex justify-end gap-2">
          <button className="btn" onClick={() => setStep("idle")}>Cancel</button>
          <button className="btn" data-danger onClick={() => clear(scope.domain)}>Clear</button>
        </div>
      </div>
    );
  }
  const failed = report && !report.moved && (report.cookies === null || report.failed_origins.length > 0);
  return (
    <>
      <button className="panel-row w-full" disabled={noSite || step === "working"} onClick={() => ask()}>
        <span className="flex-1 text-left">{step === "working" ? "Clearing…" : "Clear cookies and site storage…"}</span>
      </button>
      {(noSite || empty) && <div className="panel-note">Nothing to clear here: this works for websites only.</div>}
      {report?.moved && (
        <div className="panel-note" role="status" style={{ color: "var(--warn)" }}>
          Nothing was removed: the page is no longer on {report.domain}.
        </div>
      )}
      {report && !report.moved && (
        <div className="panel-note" role="status" style={{ color: failed ? "var(--warn)" : undefined }}>
          {report.cookies === null ? "Cookies could not be removed" : `Cookies: ${report.cookies} removed`}
          {" · "}Storage: {report.origins.length - report.failed_origins.length} of {report.origins.length} origins
          {failed && <> · <button className="link" onClick={() => ask(report.domain)}>Retry</button></>}
        </div>
      )}
    </>
  );
}

function connection(item: TabItem, rt: TabRuntime | undefined): { text: string; tone?: "warn" | "bad" } {
  if (item.url.startsWith("magnetite://")) return { text: "Magnetite page" };
  if (item.url.startsWith("file://")) return { text: "Local file" };
  if (!rt?.site) return { text: "Loading…" };
  if (rt.site.cert_error) return { text: "Certificate problem", tone: "bad" };
  if (rt.site.secure === false) return { text: "Not encrypted", tone: "warn" };
  if (rt.site.secure) return { text: "Encrypted connection" };
  return { text: "Unavailable" };
}

/**
 * "Developer mode": the developer bar, the caution border and the console error count for
 * this site, as for a page this computer serves (`developer.md`). Adds the
 * site's host to Settings → Developer tools → "Also local", or takes it out. A
 * page this computer serves (`own`) has them anyway.
 */
function DevSwitch({ state, url, own }: { state: UiState; url: string; own: boolean }) {
  const dev = state.settings.developer;
  // As the app compares it: lower case, without a dot at the end.
  const host = /^https?:/.test(url) && URL.canParse(url) ? new URL(url).hostname.toLowerCase().replace(/\.$/, "") : "";
  if (!dev.enabled || !host) return null;
  const named = dev.hosts.includes(host);
  // On whatever the switch says: this computer's page, or a name under a
  // pattern (`*.test`), which taking the host out would not turn off.
  const always = own ? "Always, on this computer" : dev.hosts.some((h) => h.startsWith("*.") && host.endsWith(h.slice(1))) ? "On for its domain" : null;
  const set = (checked: boolean) => send({ cmd: "set_settings", settings: { developer: { ...dev, hosts: checked ? [...dev.hosts, host] : dev.hosts.filter((h) => h !== host) } } });
  return (
    <label className="panel-row" title="The developer bar, a caution border on the tab and the console error count for this site">
      <span className="flex-1">Developer mode</span>
      {always ? <span className="text-[12px] text-[var(--muted)]">{always}</span> : <Switch label="Developer mode" checked={named} onChange={set} />}
    </label>
  );
}

/** `inWindow`: the panel fills a window of its own below the top bar
 * (`TopBarPopup.tsx`), which has the keyboard already. */
export function SitePanel({ state, item, rt, onClose, inWindow = false }: { state: UiState; item: TabItem; rt: TabRuntime | undefined; onClose: () => void; inWindow?: boolean }) {
  const box = useRef<HTMLDivElement>(null);
  useDismiss(onClose, { takeFocus: !inWindow });
  useEffect(() => box.current?.focus(), []);
  const internal = item.url.startsWith("magnetite://");
  // The system's engine shows the page: what hooks into Chromium (capture,
  // developer tools, own CSS, permissions, ad blocking, mute) is not there.
  const system = rt?.site?.system_engine ?? false;
  // Its name, for a page it shows.
  const [engine, setEngine] = useState<SiteEngine | null>(null);
  useEffect(() => {
    if (system) void send({ cmd: "site_engine", url: item.url }).then(setEngine);
  }, [system, item.url]);
  const conn = connection(item, rt);
  const zoom = rt?.site?.zoom ?? 100;
  // The zoom the site has without one of its own (Settings → Appearance).
  const baseZoom = rt?.site?.base_zoom || 100;
  const step = (dir: 1 | -1) => {
    const next = dir > 0 ? ZOOM_STEPS.find((z) => z > zoom) : [...ZOOM_STEPS].reverse().find((z) => z < zoom);
    if (next) send({ cmd: "set_zoom", percent: next });
  };
  // The profile the page runs in (`profile_for_tab` in the app): the tab's own
  // if that still exists, else its space's. A favorite belongs to no space and
  // runs in the profile of its list. Magnetite's own pages always run in the
  // default one.
  const { profiles, favorites, spaces } = state.sidebar;
  const home = spaces.find((s) => findTabInSpace(s, item.id)) ?? favorites.find((f) => f.items.some((t) => t.id === item.id));
  const profile = internal ? undefined : (profiles.find((p) => p.id === item.profile_id) ?? profiles.find((p) => p.id === home?.profile_id));
  const host = hostOf(item.url);

  return (
    <>
      {!inWindow && <div className="fixed inset-0 z-40" onMouseDown={onClose} />}
      <div ref={box} tabIndex={-1} className="site-panel no-drag" role="dialog" aria-label={`Site controls for ${host}`}>
        <div className="flex items-center gap-2.5 px-3 pt-3 pb-2">
          <Favicon url={item.url} src={rt?.favicon} />
          <div className="min-w-0">
            <div className="truncate font-semibold">{host}</div>
            <div className="truncate text-[12px]" style={{ color: conn.tone === "bad" ? "var(--danger)" : conn.tone === "warn" ? "var(--warn)" : "var(--muted)" }}>
              {conn.text}{system && engine ? ` · ${engine.name}` : ""}{profile && !profile.is_default ? ` · Profile: ${profile.name}` : ""}
            </div>
          </div>
        </div>

        <div className="panel-tools">
          {!internal && !system && (
            <button title={`Capture${keysHint(state.keys, "capture")}`} onClick={() => { send({ cmd: "capture_start" }); onClose(); }}>
              <CaptureIcon width={16} height={16} />
              <span>Capture</span>
              <kbd>{keysLabel(state.keys.capture)}</kbd>
            </button>
          )}
          {/* The system's share menu: only macOS has one here. */}
          {IS_MAC && (
            <button title="Share" onClick={(e) => { send({ cmd: "share", id: item.id, x: e.screenX, y: e.screenY }); onClose(); }}>
              <ShareIcon width={16} height={16} />
              <span>Share</span>
            </button>
          )}
          {!internal && !system && (
            <button title={`Your own CSS and JavaScript for ${host}`} onClick={() => { send({ cmd: "new_tab", url: `magnetite://ui/settings#code=${encodeURIComponent(host)}` }); onClose(); }}>
              <BracesIcon width={16} height={16} />
              <span>User scripts and styles</span>
            </button>
          )}
          {!internal && !system && (
            <button title={`Developer tools${keysHint(state.keys, "toggle_dev_tools")}`} onClick={() => { send({ cmd: "toggle_devtools" }); onClose(); }}>
              <CodeIcon width={16} height={16} />
              <span>Developer tools</span>
              <kbd>{keysLabel(state.keys.toggle_dev_tools)}</kbd>
            </button>
          )}
        </div>

        {/* The rows are one tab's on one site: asked again when another tab comes in front
            while the panel is open (Ctrl+Tab, ⌘W) or the tab goes to another site. */}
        {!internal && !system && <Permissions key={`${item.id} ${host}`} pageHost={host} />}
        {system && (
          <div className="panel-group">
            <div className="panel-note">{engine ? `${engine.name}, ${engine.about},` : "The system's own web engine"} shows this page. Extensions cannot read or fill it; ad blocking, Global Privacy Control and developer tools do not work here, and the site signs in apart from your other pages.</div>
          </div>
        )}

        {!internal && (
          <div className="panel-group">
            <div className="panel-label">This site</div>
            <div className="panel-row">
              <span className="flex-1">Zoom</span>
              <button className="ib sm" aria-label="Zoom out" disabled={zoom <= ZOOM_STEPS[0]!} onClick={() => step(-1)}><Minus width={14} height={14} /></button>
              <button className="zoom-value" aria-label={`Reset zoom to ${baseZoom}%`} title={`Reset to ${baseZoom}%`} onClick={() => send({ cmd: "set_zoom", percent: 0 })}>{zoom}%</button>
              <button className="ib sm" aria-label="Zoom in" disabled={zoom >= ZOOM_STEPS[ZOOM_STEPS.length - 1]!} onClick={() => step(1)}><Plus width={14} height={14} /></button>
            </div>
            {/* Asked again when the tab goes to another site; the app takes the host from the address (without port). */}
            {!system && <BlockAds key={`${item.id} ${host}`} url={item.url} />}
            {!system && (
              <button className="panel-row w-full" disabled={!rt?.loaded} onClick={() => send({ cmd: "set_muted", id: item.id, muted: !rt?.muted })}>
                <span className="flex-1 text-left">{rt?.muted ? "Unmute tab" : "Mute tab"}</span>
                {rt?.muted ? <SpeakerOff width={15} height={15} /> : <Speaker width={15} height={15} />}
              </button>
            )}
            {!system && <SiteShortcuts key={`${item.id} ${host} keys`} url={item.url} saved={state.settings.shortcut_sites} />}
            <SystemEngine key={`${item.id} ${host} engine`} url={item.url} />
            {!system && <DevSwitch state={state} url={item.url} own={rt?.dev.clears ?? false} />}
            {/* Per tab: a confirmation or a report is not the next tab's. */}
            <ClearData key={item.id} profileName={profile && !profile.is_default ? profile.name : null} noSite={rt?.site?.secure === null} />
          </div>
        )}
      </div>
    </>
  );
}
