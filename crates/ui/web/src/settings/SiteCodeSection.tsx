// Settings → User scripts and styles: the user's own CSS and JavaScript per site.
// Changes are saved when a field is left; a site that would not be kept
// stays here, marked, and is left out until it is fixed.
import { useEffect, useRef, useState } from "react";
import { Close } from "../Icons";
import type { SiteCode } from "../bridge";
import { t } from "../i18n";
import { Switch } from "./fields";

/** The most sites the app keeps (`MAX_CODE_SITES`). */
const MAX_SITES = 200;
/** The most bytes of CSS or JavaScript for one site, as UTF-8 (`MAX_SITE_CODE`). */
const MAX_CODE = 256 * 1024;
const bytes = (text: string) => new TextEncoder().encode(text).length;

/** A site as the app keeps it (`site_host`): the host of what was typed, without `www.`. Null without a host. */
function siteHost(typed: string): string | null {
  const rest = typed.trim().replace(/^[a-z][a-z0-9+.-]*:\/\//i, "");
  const host = (rest.split(/[/?#]/)[0] ?? "").split("@").pop()!.split(":")[0]!.replace(/^www\./i, "").toLowerCase();
  return /^[a-z0-9.-]+$/.test(host) && host.includes(".") ? host : null;
}

function problem(entry: SiteCode, all: SiteCode[]): string | null {
  const site = siteHost(entry.site);
  if (!site) return t("siteCode.problem.host");
  if (all.find((c) => siteHost(c.site) === site) !== entry) return t("siteCode.problem.listed");
  if (bytes(entry.css) > MAX_CODE || bytes(entry.js) > MAX_CODE) return t("siteCode.problem.too_much");
  return null;
}

/** `open`: the site whose code to show at once (from the site panel); it gets a row if it has none. */
export function SiteCodeSection({ saved, open, save }: { saved: SiteCode[]; open?: string; save: (code: SiteCode[]) => void }) {
  const [rows, setRows] = useState(saved);
  // What this editor knows to be saved. Saved code that differs was changed
  // elsewhere (another Settings tab): it replaces the rows, drafts included.
  const known = useRef(JSON.stringify(saved));
  useEffect(() => {
    const now = JSON.stringify(saved);
    if (now === known.current) return;
    known.current = now;
    setRows(saved);
  }, [saved]);
  useEffect(() => {
    const site = open && siteHost(open);
    // No new row once the list is full: the app would drop the last site for it.
    if (site) setRows((list) => (list.some((c) => siteHost(c.site) === site) || list.length >= MAX_SITES ? list : [{ site, css: "", js: "", enabled: true }, ...list]));
  }, [open]);
  const commit = (next: SiteCode[]) => {
    // A site without code says nothing yet.
    const valid = next.filter((c) => !problem(c, next) && (c.css.trim() || c.js.trim())).map((c) => ({ ...c, site: siteHost(c.site)! }));
    if (JSON.stringify(valid) === JSON.stringify(saved)) return;
    known.current = JSON.stringify(valid);
    save(valid);
  };
  const change = (i: number, patch: Partial<SiteCode>) => {
    const next = rows.map((c, j) => (j === i ? { ...c, ...patch } : c));
    setRows(next);
    return next;
  };
  const area = (i: number, key: "css" | "js", label: string, placeholder: string) => (
    <label className="block">
      <span className="set-hint block">{label}</span>
      <textarea className="field mono code mt-1" rows={5} value={rows[i]![key]} placeholder={placeholder} aria-label={rows[i]!.site ? t("siteCode.code_for", { kind: label, site: rows[i]!.site }) : t("siteCode.code_for_this", { kind: label })} spellCheck={false} autoComplete="off" autoCapitalize="off" onChange={(e) => change(i, { [key]: e.target.value })} onBlur={() => commit(rows)} />
    </label>
  );
  return (
    <>
      {rows.map((c, i) => {
        const why = c.site ? problem(c, rows) : null;
        return (
          <div key={i} className="set-card set-group"><div className="set-row space-y-3">
            <div className="flex items-center gap-3">
              <input className="field mono" style={{ flex: 1, minWidth: 0 }} value={c.site} placeholder="example.org" aria-label={t("siteCode.site")} aria-invalid={!!why || undefined} autoFocus={!c.site} onChange={(e) => change(i, { site: e.target.value })} onBlur={() => commit(rows)} />
              <Switch label={c.site ? t("siteCode.use_on", { site: c.site }) : t("siteCode.use_on_this")} checked={c.enabled} onChange={(on) => commit(change(i, { enabled: on }))} />
              <button className="ib" title={t("common.remove")} aria-label={c.site ? t("siteCode.remove", { site: c.site }) : t("siteCode.remove_this")} onClick={() => { const next = rows.filter((_, j) => j !== i); setRows(next); commit(next); }}><Close width={14} height={14} /></button>
            </div>
            {why && <p className="set-hint" style={{ color: "var(--danger)" }}>{t("siteCode.not_saved", { reason: why })}</p>}
            {area(i, "css", "CSS", "header { position: static; }")}
            {area(i, "js", "JavaScript", "document.title = document.title.toLowerCase();")}
          </div></div>
        );
      })}
      <div className="set-card set-group"><div className="set-row flex items-center gap-4" data-search-row>
        <p className="set-hint min-w-0 flex-1" style={{ marginTop: 0 }} data-search-name>
          {t("siteCode.intro")}
        </p>
        <button className="btn" data-search-name disabled={rows.length >= MAX_SITES} title={rows.length >= MAX_SITES ? t("siteCode.full", { max: MAX_SITES }) : undefined} onClick={() => setRows((list) => [...list, { site: "", css: "", js: "", enabled: true }])}>{t("siteCode.add")}</button>
      </div></div>
    </>
  );
}
