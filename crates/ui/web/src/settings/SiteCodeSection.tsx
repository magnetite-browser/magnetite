// Settings → Custom CSS & JS: the user's own CSS and JavaScript per site.
// Changes are saved when a field is left; a site that would not be kept
// stays here, marked, and is left out until it is fixed.
import { useEffect, useState } from "react";
import { Close } from "../Icons";
import type { SiteCode } from "../bridge";
import { Switch } from "./fields";

/** The most characters of CSS or JavaScript for one site (`MAX_SITE_CODE` in the app counts bytes). */
const MAX_CODE = 200_000;

/** A site as the app keeps it (`site_host`): the host of what was typed, without `www.`. Null without a host. */
function siteHost(typed: string): string | null {
  const rest = typed.trim().replace(/^[a-z][a-z0-9+.-]*:\/\//i, "");
  const host = (rest.split(/[/?#]/)[0] ?? "").split("@").pop()!.split(":")[0]!.replace(/^www\./i, "").toLowerCase();
  return /^[a-z0-9.-]+$/.test(host) && host.includes(".") ? host : null;
}

function problem(entry: SiteCode, all: SiteCode[]): string | null {
  const site = siteHost(entry.site);
  if (!site) return "Give the site by its host, such as example.org.";
  if (all.find((c) => siteHost(c.site) === site) !== entry) return "This site is listed already.";
  if (entry.css.length > MAX_CODE || entry.js.length > MAX_CODE) return "That is too much code for one site.";
  return null;
}

/** `open`: the site whose code to show at once (from the site panel); it gets a row if it has none. */
export function SiteCodeSection({ saved, open, save }: { saved: SiteCode[]; open?: string; save: (code: SiteCode[]) => void }) {
  const [rows, setRows] = useState(saved);
  useEffect(() => {
    const site = open && siteHost(open);
    if (site) setRows((list) => (list.some((c) => siteHost(c.site) === site) ? list : [{ site, css: "", js: "", enabled: true }, ...list]));
  }, [open]);
  const commit = (next: SiteCode[]) => {
    // A site without code says nothing yet.
    const valid = next.filter((c) => !problem(c, next) && (c.css.trim() || c.js.trim())).map((c) => ({ ...c, site: siteHost(c.site)! }));
    if (JSON.stringify(valid) !== JSON.stringify(saved)) save(valid);
  };
  const change = (i: number, patch: Partial<SiteCode>) => {
    const next = rows.map((c, j) => (j === i ? { ...c, ...patch } : c));
    setRows(next);
    return next;
  };
  const area = (i: number, key: "css" | "js", label: string, placeholder: string) => (
    <label className="block">
      <span className="set-hint block">{label}</span>
      <textarea className="field mono code mt-1" rows={5} value={rows[i]![key]} placeholder={placeholder} aria-label={`${label} for ${rows[i]!.site || "this site"}`} spellCheck={false} autoComplete="off" autoCapitalize="off" onChange={(e) => change(i, { [key]: e.target.value })} onBlur={() => commit(rows)} />
    </label>
  );
  return (
    <>
      {rows.map((c, i) => {
        const why = c.site ? problem(c, rows) : null;
        return (
          <div key={i} className="set-card space-y-3">
            <div className="flex items-center gap-3">
              <input className="field mono" style={{ flex: 1, minWidth: 0 }} value={c.site} placeholder="example.org" aria-label="Site" aria-invalid={!!why || undefined} autoFocus={!c.site} onChange={(e) => change(i, { site: e.target.value })} onBlur={() => commit(rows)} />
              <Switch label={`Use on ${c.site || "this site"}`} checked={c.enabled} onChange={(on) => commit(change(i, { enabled: on }))} />
              <button className="ib" title="Remove" aria-label={`Remove the code for ${c.site || "this site"}`} onClick={() => { const next = rows.filter((_, j) => j !== i); setRows(next); commit(next); }}><Close width={14} height={14} /></button>
            </div>
            {why && <p className="set-hint" style={{ color: "var(--danger)" }}>Not saved: {why}</p>}
            {area(i, "css", "CSS", "header { position: static; }")}
            {area(i, "js", "JavaScript", "document.title = document.title.toLowerCase();")}
          </div>
        );
      })}
      <div className="set-card flex items-center gap-4">
        <p className="set-hint min-w-0 flex-1">
          CSS reaches open pages of the site as soon as it is saved; JavaScript runs each time one of its pages has loaded. A site stands for its subdomains. The code runs as part of the page and can read and change everything on it: only add code you understand. It stays on this computer.
        </p>
        <button className="btn" onClick={() => setRows((list) => [...list, { site: "", css: "", js: "", enabled: true }])}>Add site</button>
      </div>
    </>
  );
}
