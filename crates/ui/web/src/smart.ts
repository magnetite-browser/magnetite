// Smart folder helpers shared by the sidebar, the dialog and Settings.
import { normaliseHost, sourceHost, type Account, type Folder, type Service, type SidebarNode, type SmartSource, type UiState } from "./bridge";

export const SERVICES: readonly Service[] = ["github", "gitlab", "jira", "youtrack"];
export const SERVICE_LABEL: Record<Service, string> = { github: "GitHub", gitlab: "GitLab", jira: "Jira", youtrack: "YouTrack" };

export const DEFAULT_YOUTRACK_QUERY = "for: me #Unresolved sort by: updated desc";
export const DEFAULT_JIRA_JQL = "assignee = currentUser() AND resolution = Unresolved ORDER BY updated DESC";

/** A source per service, each with its defaults. */
export type SourceOf<S extends Service> = Extract<SmartSource, { service: S }>;
export type Sources = { [S in Service]: SourceOf<S> };
export const DEFAULT_SOURCES: Sources = {
  github: { service: "github", host: "github.com", show: "pull_requests", filter: "" },
  gitlab: { service: "gitlab", host: "gitlab.com", show: "pull_requests", group: "" },
  jira: { service: "jira", url: "", jql: DEFAULT_JIRA_JQL },
  youtrack: { service: "youtrack", url: "", query: DEFAULT_YOUTRACK_QUERY },
};

/** The host of a URL or host string, without path: `https://a.b/c` → `a.b`. */
function hostOnly(u: string): string {
  return normaliseHost(u).split("/")[0] ?? "";
}

/** Jira Cloud (Atlassian's servers) signs in with email + API token; Data Center with a personal token. */
export function isJiraCloud(url: string): boolean {
  return hostOnly(url).endsWith(".atlassian.net");
}

/** A site whose cached favicon stands for the service. */
export function serviceSite(source: SmartSource): string {
  const host = normaliseHost(sourceHost(source));
  const fallback = { github: "github.com", gitlab: "gitlab.com", jira: "atlassian.net", youtrack: "youtrack.jetbrains.com" }[source.service];
  return `https://${host || fallback}`;
}

/** Folder `id` in any space, with the space it is in. */
export function findFolder(state: UiState, id: string): { folder: Folder; space: string } | null {
  const walk = (nodes: SidebarNode[]): Folder | null => {
    for (const n of nodes) {
      if (n.kind !== "folder") continue;
      if (n.id === id) return n;
      const inner = walk(n.children);
      if (inner) return inner;
    }
    return null;
  };
  for (const s of state.sidebar.spaces) {
    const folder = walk(s.pinned);
    if (folder) return { folder, space: s.title };
  }
  return null;
}

/** Every smart folder, with the space it is in. */
export function smartFolders(state: UiState): { folder: Folder; space: string }[] {
  const out: { folder: Folder; space: string }[] = [];
  const walk = (nodes: SidebarNode[], space: string) => {
    for (const n of nodes) {
      if (n.kind !== "folder") continue;
      if (n.smart) out.push({ folder: n, space });
      walk(n.children, space);
    }
  };
  for (const s of state.sidebar.spaces) walk(s.pinned, s.title);
  return out;
}

/** Accounts that can read `source` (same service and server). */
export function accountsFor(accounts: Account[], source: SmartSource): Account[] {
  const host = normaliseHost(sourceHost(source));
  return accounts.filter((a) => a.service === source.service && normaliseHost(a.host) === host);
}

/** Where to create a token for `source`'s server, read-only where the service allows it. */
export function tokenPage(source: SmartSource): string | null {
  const raw = sourceHost(source).trim().replace(/\/+$/, "");
  const host = normaliseHost(raw);
  if (!host) return null;
  const base = /^[a-z]+:\/\//i.test(raw) ? raw : `https://${host}`;
  // A token page is a web page. The address may come from another device
  // (a synced folder): it must not open a file or a built-in page.
  if (!/^https?:\/\//i.test(base)) return null;
  switch (source.service) {
    case "github":
      return host === "github.com" ? "https://github.com/settings/personal-access-tokens/new" : `${base}/settings/tokens/new?description=Magnetite&scopes=repo`;
    case "gitlab":
      return `${base}/-/user_settings/personal_access_tokens?name=Magnetite&scopes=read_api`;
    case "jira":
      return isJiraCloud(raw) ? "https://id.atlassian.com/manage-profile/security/api-tokens" : `${base}/secure/ViewProfile.jspa?selectedTab=com.atlassian.pats.pats-plugin:jira-user-personal-access-tokens`;
    case "youtrack":
      return `${base}/users/me?tab=account-security`;
  }
}

/** "just now", "5 min ago", "3 h ago", "2 days ago". */
export function ago(unixSeconds: number): string {
  const s = Math.max(0, Date.now() / 1000 - unixSeconds);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  const d = Math.floor(s / 86400);
  return `${d} day${d === 1 ? "" : "s"} ago`;
}

/** A plausible source for a plain folder being turned smart, from where its tabs live. */
export function guessSource(folder: Folder | null): SmartSource {
  const urls = (folder?.children ?? []).flatMap((n) => (n.kind === "tab" ? [n.home_url ?? n.url] : []));
  const origin = (u: string) => {
    try {
      return new URL(u).origin;
    } catch {
      return "";
    }
  };
  const on = (test: (u: string) => boolean) => urls.find(test);
  if (on((u) => normaliseHost(u).startsWith("github.com/"))) return DEFAULT_SOURCES.github;
  const gitlab = on((u) => /gitlab/i.test(hostOnly(u)));
  if (gitlab) return { ...DEFAULT_SOURCES.gitlab, host: hostOnly(gitlab) };
  const jira = on((u) => isJiraCloud(u) || /\/browse\/[A-Z][A-Z0-9]+-\d+/.test(u));
  if (jira) return { ...DEFAULT_SOURCES.jira, url: origin(jira) };
  const youtrack = on((u) => /youtrack/i.test(u));
  if (youtrack) return { ...DEFAULT_SOURCES.youtrack, url: origin(youtrack) };
  return DEFAULT_SOURCES.github;
}
