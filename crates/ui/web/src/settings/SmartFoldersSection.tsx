// Settings → Smart folders: the folders and the accounts they read with.
import { useCallback, useEffect, useState } from "react";
import { Favicon } from "../Favicon";
import { RefreshIcon, SmartFolderIcon } from "../Icons";
import { TokenForm } from "../SmartFolderDialog";
import { normaliseHost, send, type Service, type SmartSource, type UiState } from "../bridge";
import { useArmed } from "../dismiss";
import { DEFAULT_SOURCES, SERVICES, SERVICE_LABEL, ago, smartFolders } from "../smart";
import { Field } from "./fields";

const SHOW_LABEL = { pull_requests: "Mine + review requests", review_requests: "Review requests", my_pull_requests: "Mine", assigned_issues: "Issues assigned to me" } as const;

/** One line saying what a source shows. */
function describe(source: SmartSource): string {
  switch (source.service) {
    case "github":
    case "gitlab": {
      const host = normaliseHost(source.host);
      const scope = source.service === "github" ? source.filter : source.group;
      return [SERVICE_LABEL[source.service], SHOW_LABEL[source.show], host !== `${source.service}.com` && host, scope.trim()].filter(Boolean).join(" · ");
    }
    case "jira":
      return `${normaliseHost(source.url)} · ${source.jql}`;
    case "youtrack":
      return `${normaliseHost(source.url)} · ${source.query}`;
  }
}

/** Smart folders and the accounts they read with. Polls while open so refreshes show. */
export function SmartFoldersSection() {
  const [state, setState] = useState<UiState | null>(null);
  const [connect, setConnect] = useState<Service | null>(null);
  const [host, setHost] = useState("github.com");
  const { armed, arm } = useArmed(4000);
  const reload = useCallback(() => send({ cmd: "get_state" }).then(setState), []);
  useEffect(() => {
    void reload();
    const t = setInterval(reload, 4000);
    return () => clearInterval(t);
  }, [reload]);
  if (!state) return null;
  const folders = smartFolders(state);
  const startConnect = (s: Service) => {
    setConnect(s);
    const d = DEFAULT_SOURCES[s];
    setHost(d.service === "github" || d.service === "gitlab" ? d.host : "");
  };
  // Only service and server matter for connecting.
  const source: SmartSource =
    connect === "gitlab" ? { ...DEFAULT_SOURCES.gitlab, host }
    : connect === "jira" ? { ...DEFAULT_SOURCES.jira, url: host }
    : connect === "youtrack" ? { ...DEFAULT_SOURCES.youtrack, url: host }
    : { ...DEFAULT_SOURCES.github, host };
  return (
    <>
      <Field label="Folders" hint="Right-click any folder in the sidebar to turn it into a smart folder.">
        <div className="list">
          {folders.length === 0 && <div className="list-empty">No smart folders yet.</div>}
          {folders.map(({ folder, space }) => {
            const st = state.smart[folder.id];
            return (
              <div key={folder.id} className="list-row">
                <span className="icon-slot" style={{ color: "var(--accent)" }}><SmartFolderIcon /></span>
                <div className="min-w-0 flex-1">
                  <div className="truncate">{folder.title} <span className="text-[var(--muted)]">· {space}</span></div>
                  <div className="truncate text-[12px] text-[var(--muted)]">
                    {folder.smart && describe(folder.smart.source)}
                    {st?.error ? <span className="sf-error"> · {st.error}</span> : st?.updated_at ? ` · updated ${ago(st.updated_at)}` : ""}
                  </div>
                </div>
                {st?.fetching ? <span className="spinner" /> : (
                  <button className="ib sm" title="Refresh now" onClick={() => send({ cmd: "smart_refresh", id: folder.id }).then(reload)}><RefreshIcon width={14} height={14} /></button>
                )}
                <button className="btn sm" onClick={() => send({ cmd: "smart_folder_dialog", id: folder.id })}>Edit</button>
              </div>
            );
          })}
        </div>
        <button className="btn mt-3" onClick={() => send({ cmd: "smart_folder_dialog" })}>New smart folder</button>
      </Field>

      <Field label="Accounts" hint="An account is an access token for one server. Disconnecting deletes the token here; revoke it on the server as well if you no longer need it.">
        <div className="list">
          {state.accounts.length === 0 && <div className="list-empty">No accounts connected.</div>}
          {state.accounts.map((a) => (
            <div key={a.id} className="list-row">
              <Favicon url={`https://${normaliseHost(a.host)}`} />
              <div className="min-w-0 flex-1">
                <div className="truncate">@{a.login}{a.name && <span className="text-[var(--muted)]"> · {a.name}</span>}</div>
                <div className="truncate text-[12px] text-[var(--muted)]">{SERVICE_LABEL[a.service]} · {normaliseHost(a.host)}</div>
              </div>
              <button
                className="btn sm"
                data-danger={armed === a.id || undefined}
                onClick={() => arm(a.id, () => send({ cmd: "smart_disconnect", id: a.id }).then(reload))}
              >
                {armed === a.id ? "Click again to disconnect" : "Disconnect"}
              </button>
            </div>
          ))}
        </div>
        {connect ? (
          <div className="mt-3 space-y-2 max-w-[520px]">
            <div className="flex gap-2">
              <select className="field" style={{ width: 130 }} value={connect} onChange={(e) => startConnect(SERVICES.find((s) => s === e.target.value) ?? connect)}>
                {SERVICES.map((s) => <option key={s} value={s}>{SERVICE_LABEL[s]}</option>)}
              </select>
              <input className="field flex-1" value={host} placeholder={{ github: "github.com", gitlab: "gitlab.com", jira: "https://acme.atlassian.net", youtrack: "https://acme.youtrack.cloud" }[connect]} spellCheck={false} onChange={(e) => setHost(e.target.value)} />
            </div>
            <TokenForm key={connect} source={source} onConnected={() => { setConnect(null); reload(); }} onCancel={() => setConnect(null)} />
          </div>
        ) : (
          <button className="btn mt-3" onClick={() => startConnect("github")}>Connect an account</button>
        )}
      </Field>
    </>
  );
}
