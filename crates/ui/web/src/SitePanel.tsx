// Site panel: identity and connection, quick tools for the page (capture,
// share, developer tools), then this site's permissions, zoom, ad blocking
// and sound.
// Opens under the address row; Esc or a click outside closes it.
import { useEffect, useRef, useState } from "react";
import { useDismiss } from "./dismiss";
import { Favicon } from "./Favicon";
import { CaptureIcon, CodeIcon, Minus, Plus, ShareIcon, Speaker, SpeakerOff } from "./Icons";
import { Switch } from "./settings/fields";
import { findTabInSpace, hostOf, onSiteDataCleared, send, ZOOM_STEPS, type ClearReport, type PermissionKind, type PermissionRow, type PermissionValue, type TabItem, type TabRuntime, type UiState } from "./bridge";

const PERMISSION_LABEL: Record<PermissionKind, string> = {
  camera: "Camera",
  microphone: "Microphone",
  location: "Location",
  notifications: "Notifications",
  popups: "Pop-ups and redirects",
  clipboard: "Clipboard",
};

/** Ad blocking on this site, while the ad blocker is on (Settings → Ad blocking). */
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

/** `inWindow`: the panel fills a window of its own below the top bar
 * (`TopBarPopup.tsx`), which has the keyboard already. */
export function SitePanel({ state, item, rt, onClose, inWindow = false }: { state: UiState; item: TabItem; rt: TabRuntime | undefined; onClose: () => void; inWindow?: boolean }) {
  const box = useRef<HTMLDivElement>(null);
  useDismiss(onClose, { takeFocus: !inWindow });
  useEffect(() => box.current?.focus(), []);
  const internal = item.url.startsWith("magnetite://");
  const conn = connection(item, rt);
  const zoom = rt?.site?.zoom ?? 100;
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
              {conn.text}{profile && !profile.is_default ? ` · Profile: ${profile.name}` : ""}
            </div>
          </div>
        </div>

        <div className="panel-tools">
          {!internal && (
            <button title="Capture (⇧⌘2)" onClick={() => { send({ cmd: "capture_start" }); onClose(); }}>
              <CaptureIcon width={16} height={16} />
              <span>Capture</span>
              <kbd>⇧⌘2</kbd>
            </button>
          )}
          <button title="Share" onClick={(e) => { send({ cmd: "share", id: item.id, x: e.clientX, y: e.clientY }); onClose(); }}>
            <ShareIcon width={16} height={16} />
            <span>Share</span>
          </button>
          {!internal && (
            <button title="Developer tools (⌥⌘I)" onClick={() => { send({ cmd: "toggle_devtools" }); onClose(); }}>
              <CodeIcon width={16} height={16} />
              <span>Developer tools</span>
              <kbd>⌥⌘I</kbd>
            </button>
          )}
        </div>

        {/* The rows are one tab's on one site: asked again when another tab comes in front
            while the panel is open (Ctrl+Tab, ⌘W) or the tab goes to another site. */}
        {!internal && <Permissions key={`${item.id} ${host}`} pageHost={host} />}

        {!internal && (
          <div className="panel-group">
            <div className="panel-label">This site</div>
            <div className="panel-row">
              <span className="flex-1">Zoom</span>
              <button className="ib sm" aria-label="Zoom out" disabled={zoom <= ZOOM_STEPS[0]!} onClick={() => step(-1)}><Minus width={14} height={14} /></button>
              <button className="zoom-value" aria-label="Reset zoom to 100%" title="Reset to 100%" onClick={() => send({ cmd: "set_zoom", percent: 100 })}>{zoom}%</button>
              <button className="ib sm" aria-label="Zoom in" disabled={zoom >= ZOOM_STEPS[ZOOM_STEPS.length - 1]!} onClick={() => step(1)}><Plus width={14} height={14} /></button>
            </div>
            {/* Asked again when the tab goes to another site; the app takes the host from the address (without port). */}
            <BlockAds key={`${item.id} ${host}`} url={item.url} />
            <button className="panel-row w-full" disabled={!rt?.loaded} onClick={() => send({ cmd: "set_muted", id: item.id, muted: !rt?.muted })}>
              <span className="flex-1 text-left">{rt?.muted ? "Unmute tab" : "Mute tab"}</span>
              {rt?.muted ? <SpeakerOff width={15} height={15} /> : <Speaker width={15} height={15} />}
            </button>
            {/* Per tab: a confirmation or a report is not the next tab's. */}
            <ClearData key={item.id} profileName={profile && !profile.is_default ? profile.name : null} noSite={rt?.site?.secure === null} />
          </div>
        )}
      </div>
    </>
  );
}
