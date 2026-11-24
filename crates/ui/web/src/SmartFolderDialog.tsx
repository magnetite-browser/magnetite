// The smart folder dialog (magnetite://ui/smart-folder?id=…), shown in the
// centred palette window: what a folder shows, from which account, how often
// it refreshes, with a live preview. Esc cancels; clicking elsewhere closes
// it but keeps the draft, so you can create a token and come back.
import { useEffect, useRef, useState } from "react";
import { Favicon } from "./Favicon";
import { ExternalIcon, KeyIcon, SmartFolderIcon } from "./Icons";
import { t } from "./i18n";
import { send, sourceHost, type Account, type ReviewShow, type Service, type SmartItem, type SmartSource, type UiState } from "./bridge";
import {
  DEFAULT_JIRA_JQL,
  DEFAULT_SOURCES,
  DEFAULT_YOUTRACK_QUERY,
  SERVICES,
  SERVICE_LABEL,
  accountsFor,
  ago,
  findFolder,
  guessSource,
  isJiraCloud,
  serviceSite,
  tokenPage,
  type SourceOf,
  type Sources,
} from "./smart";

const SHOW: Record<"github" | "gitlab", [ReviewShow, string][]> = {
  github: [
    ["pull_requests", t("dialogs.smart_folder.show.github.pull_requests")],
    ["review_requests", t("dialogs.smart_folder.show.github.review_requests")],
    ["my_pull_requests", t("dialogs.smart_folder.show.github.my_pull_requests")],
    ["assigned_issues", t("dialogs.smart_folder.show.github.assigned_issues")],
  ],
  gitlab: [
    ["pull_requests", t("dialogs.smart_folder.show.gitlab.pull_requests")],
    ["review_requests", t("dialogs.smart_folder.show.gitlab.review_requests")],
    ["my_pull_requests", t("dialogs.smart_folder.show.gitlab.my_pull_requests")],
    ["assigned_issues", t("dialogs.smart_folder.show.gitlab.assigned_issues")],
  ],
};
const JIRA_PRESETS: [string, string][] = [
  [t("dialogs.smart_folder.query.assigned_to_me"), DEFAULT_JIRA_JQL],
  [t("dialogs.smart_folder.query.reported_by_me"), "reporter = currentUser() AND resolution = Unresolved ORDER BY updated DESC"],
  [t("dialogs.smart_folder.query.watching"), "watcher = currentUser() AND resolution = Unresolved ORDER BY updated DESC"],
];
const YOUTRACK_PRESETS: [string, string][] = [
  [t("dialogs.smart_folder.query.assigned_to_me"), DEFAULT_YOUTRACK_QUERY],
  [t("dialogs.smart_folder.query.reported_by_me"), "reporter: me #Unresolved sort by: updated desc"],
  [t("dialogs.smart_folder.query.starred"), "tag: Star #Unresolved sort by: updated desc"],
];
const DEFAULT_TITLE: Record<Service, string> = {
  github: t("dialogs.smart_folder.default_name.github"),
  gitlab: t("dialogs.smart_folder.default_name.gitlab"),
  jira: t("dialogs.smart_folder.default_name.jira"),
  youtrack: t("dialogs.smart_folder.default_name.youtrack"),
};
const REFRESH = [5, 15, 30, 60];
const LIMITS = [10, 20, 50];

interface Draft {
  title: string;
  service: Service;
  sources: Sources;
  account?: string;
  limit: number;
  refresh: number;
  at: number;
}

const DRAFT_PREFIX = "smart-draft:";
const draftKey = (id?: string) => `${DRAFT_PREFIX}${id ?? "new"}`;
/**
 * The draft stored under `key`, if it is from the last 15 minutes and has
 * the source of its service (the token is never part of it).
 */
function readDraft(key: string): Draft | null {
  try {
    const d = JSON.parse(localStorage.getItem(key) ?? "null") as Draft | null;
    return d?.sources?.[d.service] && Date.now() - d.at < 15 * 60_000 ? d : null;
  } catch {
    return null;
  }
}
/**
 * The draft for folder `id` (none: a new folder). Drafts past their 15
 * minutes are deleted here, whichever folder they were for: a dialog closed
 * by a click elsewhere leaves its draft, and nothing else removes it.
 */
function loadDraft(id?: string): Draft | null {
  for (const key of Object.keys(localStorage)) {
    if (key.startsWith(DRAFT_PREFIX) && !readDraft(key)) localStorage.removeItem(key);
  }
  return readDraft(draftKey(id));
}

export function SmartFolderDialog() {
  const [state, setState] = useState<UiState | null>(null);
  useEffect(() => {
    document.title = t("dialogs.smart_folder.title.smart");
    send({ cmd: "get_state" }).then(setState);
  }, []);
  return state ? <Form state={state} id={new URLSearchParams(location.search).get("id") ?? undefined} /> : null;
}

function Form({ state, id }: { state: UiState; id?: string }) {
  const folder = id ? findFolder(state, id)?.folder ?? null : null;
  const smart = folder?.smart;
  const initial = smart?.source ?? guessSource(folder);
  const [draft] = useState(() => loadDraft(id));
  const [title, setTitle] = useState(draft?.title ?? folder?.title ?? "");
  const [service, setService] = useState<Service>(draft?.service ?? initial.service);
  const [sources, setSources] = useState<Sources>(draft?.sources ?? { ...DEFAULT_SOURCES, [initial.service]: initial });
  const [account, setAccount] = useState(draft?.account ?? smart?.account);
  const [limit, setLimit] = useState(draft?.limit ?? smart?.limit ?? 20);
  const [refresh, setRefresh] = useState(draft?.refresh ?? smart?.refresh_minutes ?? 15);
  const [accounts, setAccounts] = useState<Account[]>(state.accounts);
  const [adding, setAdding] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ items?: SmartItem[]; error?: string; loading?: boolean }>({});
  const root = useRef<HTMLFormElement>(null);

  /** Change fields of one service's source. */
  const edit = <S extends Service>(s: S, patch: Partial<SourceOf<S>>) =>
    setSources((all) => {
      const next = { ...all };
      next[s] = { ...all[s], ...patch };
      return next;
    });
  const source: SmartSource = sources[service];
  const usable = accountsFor(accounts, source);
  const chosen = usable.find((a) => a.login === account) ?? usable[0];
  const status = id ? state.smart[id] : undefined;
  const label = SERVICE_LABEL[service];
  const hostReady = sourceHost(source).trim().length > 3;
  const queryReady = source.service === "jira" ? source.jql.trim() !== "" : source.service === "youtrack" ? source.query.trim() !== "" : true;

  const close = () => send({ cmd: "palette_close" });
  const cancel = () => {
    localStorage.removeItem(draftKey(id));
    close();
  };

  // Keep the draft while the dialog is open; a click elsewhere closes it.
  useEffect(() => {
    const d: Draft = { title, service, sources, account, limit, refresh, at: Date.now() };
    localStorage.setItem(draftKey(id), JSON.stringify(d));
  }, [id, title, service, sources, account, limit, refresh]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => e.key === "Escape" && (e.preventDefault(), cancel());
    window.addEventListener("keydown", key);
    window.addEventListener("blur", close);
    return () => {
      window.removeEventListener("keydown", key);
      window.removeEventListener("blur", close);
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

  // Live preview, shortly after the settings stop changing.
  const previewKey = JSON.stringify([source, chosen?.login]);
  useEffect(() => {
    if (!chosen || !hostReady || !queryReady) return setPreview({});
    let live = true;
    setPreview((p) => ({ ...p, loading: true }));
    const timer = setTimeout(() => {
      send({ cmd: "smart_preview", source, account: chosen.login })
        .then((items) => live && setPreview({ items }))
        .catch((e: Error) => live && setPreview({ error: e.message }));
    }, 450);
    return () => {
      live = false;
      clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [previewKey]);

  const save = async () => {
    setProblem(null);
    try {
      await send({ cmd: "smart_save", id, title, source, account: chosen?.login, limit, refresh_minutes: refresh });
      localStorage.removeItem(draftKey(id));
      close();
    } catch (e) {
      setProblem((e as Error).message);
    }
  };

  return (
    <form ref={root} className="sf" onSubmit={(e) => (e.preventDefault(), save())}>
      <header className="sf-head">
        <span className="sf-icon"><SmartFolderIcon width={18} height={18} /></span>
        <div className="min-w-0 flex-1">
          <h1>{smart ? t("dialogs.smart_folder.title.smart") : folder ? t("dialogs.smart_folder.title.turn_into") : t("dialogs.smart_folder.title.new")}</h1>
          <p>{status?.error ? <span className="sf-error">{status.error}</span> : status?.updated_at ? t("dialogs.smart_folder.updated", { when: ago(status.updated_at) }) : t("dialogs.smart_folder.intro", { service: label })}</p>
        </div>
      </header>

      <Row label={t("dialogs.smart_folder.name")}>
        <input className="field" value={title} placeholder={DEFAULT_TITLE[service]} onChange={(e) => setTitle(e.target.value)} autoFocus={!folder} />
      </Row>

      <Row label={t("dialogs.smart_folder.source")}>
        <div className="sf-source">
          <Favicon url={serviceSite(source)} />
          <select className="field" value={service} onChange={(e) => setService(SERVICES.find((s) => s === e.target.value) ?? service)}>
            {SERVICES.map((s) => <option key={s} value={s}>{SERVICE_LABEL[s]}</option>)}
          </select>
        </div>
      </Row>

      {source.service === "github" && (
        <>
          <Row label={t("dialogs.smart_folder.server.label")} hint={t("dialogs.smart_folder.server.github_hint")}>
            <input className="field" value={source.host} spellCheck={false} onChange={(e) => edit("github", { host: e.target.value })} />
          </Row>
          <ShowRow options={SHOW.github} value={source.show} onChange={(show) => edit("github", { show })} />
          <Row label={t("dialogs.smart_folder.only.label")} hint={t("dialogs.smart_folder.only.hint")}>
            <input className="field mono" value={source.filter} placeholder="org:acme" spellCheck={false} onChange={(e) => edit("github", { filter: e.target.value })} />
          </Row>
        </>
      )}
      {source.service === "gitlab" && (
        <>
          <Row label={t("dialogs.smart_folder.server.label")} hint={t("dialogs.smart_folder.server.gitlab_hint")}>
            <input className="field" value={source.host} spellCheck={false} onChange={(e) => edit("gitlab", { host: e.target.value })} />
          </Row>
          <ShowRow options={SHOW.gitlab} value={source.show} onChange={(show) => edit("gitlab", { show })} />
          <Row label={t("dialogs.smart_folder.group.label")} hint={t("dialogs.smart_folder.group.hint")}>
            <input className="field mono" value={source.group} placeholder="acme/platform" spellCheck={false} onChange={(e) => edit("gitlab", { group: e.target.value })} />
          </Row>
        </>
      )}
      {source.service === "jira" && (
        <>
          <Row label={t("dialogs.smart_folder.address.label")} hint={t("dialogs.smart_folder.address.jira_hint")}>
            <input className="field" value={source.url} placeholder="https://acme.atlassian.net" spellCheck={false} autoFocus={!!folder && !source.url} onChange={(e) => edit("jira", { url: e.target.value })} />
          </Row>
          <QueryRow hint={t("dialogs.smart_folder.query.jira_hint")} value={source.jql} presets={JIRA_PRESETS} onChange={(jql) => edit("jira", { jql })} />
        </>
      )}
      {source.service === "youtrack" && (
        <>
          <Row label={t("dialogs.smart_folder.address.label")}>
            <input className="field" value={source.url} placeholder="https://acme.youtrack.cloud" spellCheck={false} autoFocus={!!folder && !source.url} onChange={(e) => edit("youtrack", { url: e.target.value })} />
          </Row>
          <QueryRow hint={t("dialogs.smart_folder.query.youtrack_hint")} value={source.query} presets={YOUTRACK_PRESETS} onChange={(query) => edit("youtrack", { query })} />
        </>
      )}

      <Row label={t("dialogs.smart_folder.account.label")}>
        {!adding && chosen ? (
          <div className="sf-account">
            {usable.length > 1 ? (
              <select className="field" value={chosen.login} onChange={(e) => setAccount(e.target.value)}>
                {usable.map((a) => <option key={a.id} value={a.login}>@{a.login}{a.name ? ` · ${a.name}` : ""}</option>)}
              </select>
            ) : (
              <span className="sf-signed"><span className="ok-dot" />@{chosen.login}{chosen.name && <span className="text-[var(--muted)]"> · {chosen.name}</span>}</span>
            )}
            <button type="button" className="link" onClick={() => setAdding(true)}>{t("dialogs.smart_folder.account.add_another")}</button>
          </div>
        ) : (
          <TokenForm
            source={source}
            autoFocus={!!folder}
            onConnected={(a) => {
              setAccounts((list) => [...list.filter((x) => !(x.service === a.service && x.login === a.login && x.host === a.host)), a]);
              setAccount(a.login);
              setAdding(false);
            }}
            onCancel={adding && chosen ? () => setAdding(false) : undefined}
          />
        )}
      </Row>

      <div className="sf-preview" aria-live="polite">
        <div className="sf-preview-head">{t("dialogs.smart_folder.preview.label")}{preview.loading && <span className="spinner" />}</div>
        {!chosen ? (
          <div className="sf-empty">{t("dialogs.smart_folder.preview.no_account")}</div>
        ) : preview.error ? (
          <div className="sf-error px-1 py-1">{preview.error}</div>
        ) : preview.items && preview.items.length === 0 ? (
          <div className="sf-empty">{t("dialogs.smart_folder.preview.empty")}</div>
        ) : (
          (preview.items ?? []).map((i) => (
            <div key={i.url} className="sf-item" title={i.url}><Favicon url={i.url} /><span className="truncate">{i.title}</span></div>
          ))
        )}
      </div>

      <Row label={t("dialogs.smart_folder.refresh.label")}>
        <div className="flex flex-wrap items-center gap-3">
          <div className="seg sm">
            {REFRESH.map((m) => <button key={m} type="button" data-on={refresh === m || undefined} onClick={() => setRefresh(m)}>{m < 60 ? t("dialogs.smart_folder.refresh.minutes", { minutes: m }) : t("dialogs.smart_folder.refresh.hour")}</button>)}
          </div>
          <label className="flex items-center gap-2 text-[12.5px] text-[var(--secondary)]">
            {t("dialogs.smart_folder.refresh.up_to")}
            <select className="field sm" value={limit} onChange={(e) => setLimit(Number(e.target.value))}>
              {LIMITS.map((n) => <option key={n} value={n}>{t("dialogs.smart_folder.refresh.items", { count: n })}</option>)}
            </select>
          </label>
        </div>
      </Row>

      <p className="sf-note"><KeyIcon width={12} height={12} />{t("dialogs.smart_folder.token_note", { service: label })}</p>
      {problem && <p className="sf-error">{problem}</p>}

      <footer className="sf-foot">
        {smart && id && (
          <button type="button" className="btn ghost" title={t("dialogs.smart_folder.stop.title")} onClick={() => send({ cmd: "smart_unlink", id }).then(cancel)}>{t("dialogs.smart_folder.stop.label")}</button>
        )}
        <span className="flex-1" />
        <button type="button" className="btn" onClick={cancel}>{t("common.cancel")}</button>
        <button type="submit" className="btn primary" disabled={!hostReady || !queryReady}>{smart ? t("common.save") : t("dialogs.smart_folder.create")}</button>
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

function ShowRow({ options, value, onChange }: { options: [ReviewShow, string][]; value: ReviewShow; onChange: (v: ReviewShow) => void }) {
  return (
    <Row label={t("dialogs.smart_folder.show.label")}>
      <div className="chips">
        {options.map(([v, text]) => (
          <button key={v} type="button" className="chip" data-on={value === v || undefined} onClick={() => onChange(v)}>{text}</button>
        ))}
      </div>
    </Row>
  );
}

function QueryRow({ hint, value, presets, onChange }: { hint: string; value: string; presets: [string, string][]; onChange: (v: string) => void }) {
  return (
    <Row label={t("dialogs.smart_folder.query.label")} hint={hint}>
      <input className="field mono" value={value} spellCheck={false} onChange={(e) => onChange(e.target.value)} />
      <div className="chips mt-2">
        {presets.map(([text, q]) => (
          <button key={q} type="button" className="chip" data-on={value === q || undefined} onClick={() => onChange(q)}>{text}</button>
        ))}
      </div>
    </Row>
  );
}

const TOKEN_HINT: Record<Service, string> = {
  github: t("dialogs.smart_folder.token.hint.github"),
  gitlab: t("dialogs.smart_folder.token.hint.gitlab"),
  jira: t("dialogs.smart_folder.token.hint.jira"),
  youtrack: t("dialogs.smart_folder.token.hint.youtrack"),
};

/**
 * Connect an account for `source`'s service and server: paste a token, Magnetite
 * asks the server whose it is and keeps it on this device. Jira Cloud also
 * needs the Atlassian account email. Links to the server's token page.
 */
export function TokenForm({ source, autoFocus, onConnected, onCancel }: { source: SmartSource; autoFocus?: boolean; onConnected: (a: Account) => void; onCancel?: () => void }) {
  const [email, setEmail] = useState("");
  const [token, setToken] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const label = SERVICE_LABEL[source.service];
  const host = sourceHost(source).trim();
  const cloud = source.service === "jira" && isJiraCloud(host);
  const tokenUrl = tokenPage(source);
  const ready = token.trim() !== "" && host.length > 3 && (!cloud || email.includes("@"));
  const connect = async () => {
    if (!ready) return;
    setBusy(true);
    setProblem(null);
    try {
      // Jira Cloud signs in with email + API token (Basic auth on the app side).
      const secret = cloud ? `${email.trim()}:${token.trim()}` : token;
      onConnected(await send({ cmd: "smart_connect", service: source.service, host, token: secret }));
      setToken("");
    } catch (e) {
      setProblem((e as Error).message);
    } finally {
      setBusy(false);
    }
  };
  const enter = (e: React.KeyboardEvent) => e.key === "Enter" && (e.preventDefault(), connect());
  return (
    <div>
      {cloud && <input className="field mb-1.5" type="email" value={email} placeholder={t("dialogs.smart_folder.token.email")} autoComplete="off" onChange={(e) => setEmail(e.target.value)} onKeyDown={enter} />}
      <div className="sf-token">
        <input className="field" type="password" value={token} placeholder={cloud ? t("dialogs.smart_folder.token.api_token") : t("dialogs.smart_folder.token.paste", { service: label })} autoComplete="off" autoFocus={autoFocus && !cloud} onChange={(e) => setToken(e.target.value)} onKeyDown={enter} />
        <button type="button" className="btn" disabled={!ready || busy} onClick={connect}>{busy ? t("dialogs.smart_folder.token.checking") : t("dialogs.smart_folder.token.connect")}</button>
      </div>
      <div className="hint">
        {cloud ? t("dialogs.smart_folder.token.cloud_hint") : TOKEN_HINT[source.service]}{" "}
        {tokenUrl && host.length > 3 && (
          <button type="button" className="link" onClick={() => send({ cmd: "new_tab", url: tokenUrl })}>{t("dialogs.smart_folder.token.create")} <ExternalIcon width={11} height={11} /></button>
        )}
        {onCancel && <button type="button" className="link ml-2" onClick={onCancel}>{t("common.cancel")}</button>}
      </div>
      {problem && <div className="sf-error mt-1">{problem}</div>}
    </div>
  );
}
