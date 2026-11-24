// Settings → Sync: start or join a chain, show the sync code, leave or delete,
// and choose which spaces stay on this device.
import { useEffect, useState } from "react";
import { SpaceIcon } from "../Glyphs";
import { send } from "../bridge";
import { useArmed } from "../dismiss";
import { language, rich, t } from "../i18n";
import { Field, Group, Switch } from "./fields";
import type { Info } from "./types";

/** Magnetite's own sync server: free to use, run by StackForge (docs/adr/0008-public-sync-server.md). */
const PUBLIC_SYNC = "https://sync.magnetitebrowser.org";

/** A start ("create") or join of a chain whose outcome the app has not reported yet. */
export type SyncJob = "create" | "join";

/** How often to ask the app how a pending start or join went. */
const POLL_MS = 500;

/**
 * Which spaces stay on this device. Shown with sync off too, so a space can
 * be kept here before the device joins a chain.
 */
function SpacesOnThisDevice({ info, reload }: { info: Info; reload: () => Promise<Info> }) {
  return (
    <Field label={t("sync.spaces.label")} hint={t("sync.spaces.hint")}>
      <div className="iso-list">
        {info.spaces.map((sp) => (
          <label key={sp.id} className="iso-row" data-search-row>
            <span className="icon-slot" style={{ color: "var(--accent)" }}><SpaceIcon space={sp} size={15} /></span>
            <span className="min-w-0 flex-1 truncate" data-search-name>{sp.title}</span>
            <span className="shrink-0 text-[13px] text-[var(--secondary)]">{t("sync.spaces.only")}</span>
            <Switch
              label={t("sync.spaces.only_switch", { space: sp.title })}
              checked={!sp.synced}
              onChange={(only) => void send({ cmd: "set_space_synced", id: sp.id, synced: !only }).then(reload)}
            />
          </label>
        ))}
      </div>
    </Field>
  );
}

/** Light squares around a QR code, which a scanner needs to find it. */
const QR_MARGIN = 4;

/**
 * A QR code from its rows ("1" is a dark square). Black on white whatever the
 * theme: scanners expect that.
 */
function QrCode({ rows, label }: { rows: string[]; label: string }) {
  const side = rows.length + 2 * QR_MARGIN;
  const squares = rows
    .flatMap((row, y) => [...row].map((c, x) => (c === "1" ? `M${x + QR_MARGIN} ${y + QR_MARGIN}h1v1h-1z` : "")))
    .join("");
  return (
    <svg role="img" aria-label={label} viewBox={`0 0 ${side} ${side}`} shapeRendering="crispEdges" className="h-[216px] w-[216px] shrink-0 rounded-lg" style={{ background: "#fff" }}>
      <path d={squares} fill="#000" />
    </svg>
  );
}

/**
 * `job` lives in the page, so leaving this section and coming back does not
 * forget it. `spaces` false leaves out the list of spaces, and `only` offers
 * just starting a chain or just joining one, in the words of a first start
 * (both: the welcome page).
 */
export function SyncSection({ info, reload, job, setJob, spaces = true, only }: { info: Info; reload: () => Promise<Info>; job: SyncJob | null; setJob: (job: SyncJob | null) => void; spaces?: boolean; only?: SyncJob }) {
  const sy = info.settings.sync;
  const ownAtStart = !!sy.server_url && sy.server_url !== PUBLIC_SYNC;
  const [own, setOwn] = useState(ownAtStart);
  const [custom, setCustom] = useState(ownAtStart ? sy.server_url : "");
  const server = own ? custom.trim() : PUBLIC_SYNC;
  const [invite, setInvite] = useState("");
  const [words, setWords] = useState("");
  const { armed, arm } = useArmed(4000);
  // The code is shown on demand, and right away after starting a new chain.
  const [code, setCode] = useState<{ words: string[]; qr: string[] | null } | null>(null);
  const [justCreated, setJustCreated] = useState(false);
  const [copied, setCopied] = useState(false);
  const showCode = async () => {
    const r = await send({ cmd: "sync_code" });
    setCode(r.words ? { words: r.words.split(" "), qr: r.qr } : null);
  };
  // Why the app refused a start or join outright (still connecting, in a chain already).
  const [refused, setRefused] = useState<string | null>(null);
  const begin = (what: SyncJob, request: Promise<unknown>) => {
    setJob(what);
    setRefused(null);
    request.catch((e: unknown) => {
      setJob(null);
      setRefused(e instanceof Error ? e.message : String(e));
    });
  };
  // The app answers a start or join at once and reports how it went in the
  // settings later: ask until this device is in a chain or an error shows.
  useEffect(() => {
    if (!job) return;
    let timer = 0;
    let stopped = false;
    const check = async () => {
      // A read that failed is asked again: the job ends with an answer only.
      const fresh = await reload().catch(() => null);
      if (stopped) return;
      if (!fresh) {
        timer = window.setTimeout(check, POLL_MS);
        return;
      }
      const { settings, sync_signed_in } = fresh;
      const joined = settings.sync.enabled && sync_signed_in;
      if (!joined && !settings.sync.last_error) {
        timer = window.setTimeout(check, POLL_MS);
        return;
      }
      setJob(null);
      if (joined && job === "create") {
        setJustCreated(true);
        void showCode();
      }
    };
    timer = window.setTimeout(check, POLL_MS);
    return () => {
      stopped = true;
      window.clearTimeout(timer);
    };
  }, [job, reload, setJob]);
  // Leaving takes effect in the app at once; the page follows shortly after.
  const afterLeave = () => setTimeout(reload, 1500);
  const when = sy.last_sync_at ? new Date(sy.last_sync_at * 1000).toLocaleString(language) : t("sync.status.not_yet");
  const wordCount = words.trim().split(/\s+/).filter(Boolean).length;

  if (sy.enabled && info.sync_signed_in) {
    return (
      <div className="space-y-4">
        <div className="set-card">
          <div className="flex items-center gap-3">
            <span className="h-2.5 w-2.5 rounded-full" style={{ background: sy.last_error ? "var(--danger)" : "#7fc98f" }} />
            <div className="min-w-0 flex-1">
              <div className="truncate">{rich("sync.status.syncing", { b: (s) => <b>{s}</b>, server: sy.server_url })}</div>
              <div className="hint" style={{ marginTop: 2 }}>{t("sync.status.last", { when })}</div>
              {sy.last_error && <div className="hint" style={{ color: "var(--danger)" }}>{sy.last_error}</div>}
            </div>
            <button className="btn" onClick={() => { send({ cmd: "sync_now" }); setTimeout(reload, 2000); }}>{t("sync.status.sync_now")}</button>
          </div>
        </div>

        <Group>
        {spaces && <SpacesOnThisDevice info={info} reload={reload} />}

        <Field label={t("sync.code.label")} hint={t("sync.code.hint")}>
          {/* Found by the buttons that lead here, also once the code shows and they are gone. */}
          <span hidden data-search-name>{t("sync.code.show")} {t("sync.create.start")}</span>
          {code ? (
            <div className="rounded-xl border p-4" style={{ borderColor: justCreated ? "var(--accent)" : "var(--line)" }}>
              {justCreated && <p className="mb-3 text-[14px]" style={{ color: "var(--accent)" }}>{t("sync.code.write_down")}</p>}
              <div className="flex flex-wrap items-center gap-5">
                <ol className="grid min-w-[300px] flex-1 grid-cols-3 gap-x-4 gap-y-1.5 font-mono text-[13px]">
                  {code.words.map((w, i) => <li key={i}><span className="inline-block w-6 text-right text-[var(--muted)]">{i + 1}</span> {w}</li>)}
                </ol>
                {code.qr && (
                  <figure className="w-[216px]">
                    <QrCode rows={code.qr} label={t("sync.code.qr_label")} />
                    <figcaption className="hint text-center">{t("sync.code.qr_caption")}</figcaption>
                  </figure>
                )}
              </div>
              <div className="mt-4 flex gap-2">
                <button className="btn" onClick={() => { navigator.clipboard?.writeText(code.words.join(" ")); setCopied(true); setTimeout(() => setCopied(false), 1500); }}>{copied ? t("common.copied") : t("sync.code.copy")}</button>
                <button className="btn" onClick={() => { setCode(null); setJustCreated(false); }}>{t("common.hide")}</button>
              </div>
            </div>
          ) : (
            <button className="btn" onClick={showCode}>{t("sync.code.show")}</button>
          )}
        </Field>

        <Field side label={t("sync.stop.label")} hint={t("sync.stop.hint")}>
          <div className="flex gap-2">
            <button className="btn" onClick={() => { send({ cmd: "sync_leave" }); setCode(null); afterLeave(); }}>{t("sync.stop.leave")}</button>
            <button className="btn" data-danger={armed === "del" || undefined} onClick={() => arm("del", () => { send({ cmd: "sync_delete_chain" }); setCode(null); afterLeave(); })}>{armed === "del" ? t("sync.stop.delete_confirm") : t("sync.stop.delete")}</button>
          </div>
        </Field>
        </Group>
      </div>
    );
  }
  return (
    <div className="space-y-4">
      <Group>
        <Field
          side
          label={t("sync.server.label")}
          hint={own ? t("sync.server.own_hint") : t("sync.server.public_hint")}
        >
          <div className="seg">
            <button data-on={!own || undefined} aria-pressed={!own} onClick={() => setOwn(false)}>Magnetite Sync</button>
            <button data-on={own || undefined} aria-pressed={own} onClick={() => setOwn(true)}>{t("sync.server.own")}</button>
          </div>
        </Field>
        {own && (
          <Field label={t("sync.server.address")}>
            <input className="field" value={custom} onChange={(e) => setCustom(e.target.value)} placeholder="https://sync.example.org" aria-label={t("sync.server.address")} spellCheck={false} />
          </Field>
        )}
        {only !== "join" && (
          <Field label={only ? t("sync.create.label_only") : t("sync.create.label")} hint={only ? t("sync.create.hint_only") : t("sync.create.hint")}>
            {own && <input className="field mb-3" value={invite} onChange={(e) => setInvite(e.target.value)} placeholder={t("sync.create.invite")} spellCheck={false} />}
            <button data-search-name className="btn" disabled={job !== null || !server} onClick={() => begin("create", send({ cmd: "sync_create", server, invite: (own && invite) || undefined }))}>{job === "create" ? t("sync.create.starting") : t("sync.create.start")}</button>
            {/* Found as "Start syncing" also while it starts. */}
            <span hidden data-search-name>{t("sync.create.start")}</span>
          </Field>
        )}
        {only !== "create" && (
          <Field label={only ? t("sync.join.label_only") : t("sync.join.label")} hint={only ? t("sync.join.hint_only") : t("sync.join.hint")}>
            <textarea className="field mono resize-none" style={{ height: 72, padding: 8 }} value={words} onChange={(e) => setWords(e.target.value)} placeholder={t("sync.join.placeholder")} spellCheck={false} autoComplete="off" />
            <button data-search-name className="btn mt-3" disabled={job !== null || !server || wordCount !== 24} onClick={() => { begin("join", send({ cmd: "sync_join", server, words })); setWords(""); }}>{job === "join" ? t("sync.join.joining") : wordCount > 0 && wordCount !== 24 ? t("sync.join.typed", { typed: wordCount }) : t("sync.join.join")}</button>
            {/* Found as "Join" also while the button counts the words. */}
            <span hidden data-search-name>{t("sync.join.join")}</span>
          </Field>
        )}
      </Group>
      {(refused ?? sy.last_error) && <div className="hint" style={{ color: "var(--danger)" }}>{refused ?? sy.last_error}</div>}
      {spaces && <Group><SpacesOnThisDevice info={info} reload={reload} /></Group>}
      <p className="hint">{t("sync.zero_knowledge")} {own ? t("sync.kept_own") : t("sync.kept_public")}</p>
    </div>
  );
}
