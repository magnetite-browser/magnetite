// Bridge between the sidebar page and the Rust application.
//
// Rust -> JS: window.__magnetite.setState(state) and a few UI calls.
// JS -> Rust: window.magnetiteQuery({request: JSON, onSuccess, onFailure}) (CEF message router).

import type { Info } from "./settings/types";

export type Id = string;

export interface TabItem {
  id: Id;
  url: string;
  title: string;
  custom_title: boolean;
  home_url?: string;
  created_at?: number;
  last_active_at?: number;
  unread: boolean;
  /** Profile override; absent means the space's (or the default) profile. */
  profile_id?: Id;
  /** "Keep active": never unloaded for being out of sight, never auto-archived. */
  keep_active?: boolean;
}

export interface Folder {
  id: Id;
  title: string;
  icon?: string;
  children: SidebarNode[];
  /** Set when the folder fills itself from a source. */
  smart?: SmartFolder;
}

/** Services smart folders read from. */
export type Service = "github" | "gitlab" | "jira" | "youtrack";
/** What a GitHub or GitLab folder shows (pull requests are merge requests on GitLab). */
export type ReviewShow = "review_requests" | "my_pull_requests" | "pull_requests" | "assigned_issues";
export type SmartSource =
  | { service: "github"; host: string; show: ReviewShow; filter: string }
  | { service: "gitlab"; host: string; show: ReviewShow; group: string }
  | { service: "jira"; url: string; jql: string }
  | { service: "youtrack"; url: string; query: string };

export interface SmartFolder {
  source: SmartSource;
  /** Login of the account to use; none: the first for the service and server. */
  account?: string;
  limit: number;
  refresh_minutes: number;
  items: string[];
  hidden: string[];
}

/** A connected account (the token never leaves the app side). */
export interface Account {
  id: Id;
  service: Service;
  host: string;
  login: string;
  name: string;
}

export interface SmartStatus {
  fetching: boolean;
  checked_at: number | null;
  updated_at: number | null;
  error: string | null;
  needs_account: boolean;
  /** Synced from another device; waits until turned on here. */
  needs_approval: boolean;
}

export interface SmartItem {
  url: string;
  title: string;
}

/** Server a source lives on: its host or URL. */
export function sourceHost(s: SmartSource): string {
  return s.service === "github" || s.service === "gitlab" ? s.host : s.url;
}

/** `https://www.GitHub.com/` → `github.com`; how accounts match sources. */
export function normaliseHost(h: string): string {
  return h.trim().replace(/\/+$/, "").replace(/^[a-z]+:\/\//i, "").replace(/^www\./, "").toLowerCase();
}

export type SidebarNode = ({ kind: "tab" } & TabItem) | ({ kind: "folder" } & Folder);

export interface Color {
  red: number;
  green: number;
  blue: number;
}

export interface Space {
  id: Id;
  title: string;
  icon?: string;
  accent?: Color;
  profile_id: Id;
  pinned: SidebarNode[];
  today: TabItem[];
  /** Saved addresses in folders. No tabs: one opens as a new tab of this space. */
  bookmarks?: SidebarNode[];
}

export interface FavoriteList {
  profile_id: Id;
  items: TabItem[];
}

export interface Snapshot {
  profiles: { id: Id; name: string; is_default: boolean; ephemeral?: boolean }[];
  /** One list per profile that has favorites; a favorite runs in its list's profile. */
  favorites: FavoriteList[];
  spaces: Space[];
}

export interface TabRuntime {
  loaded: boolean;
  loading: boolean;
  can_go_back: boolean;
  can_go_forward: boolean;
  favicon: string | null;
  media: MediaState | null;
  muted: boolean;
  site: SiteState | null;
  camera: boolean;
  microphone: boolean;
}

/** Connection and zoom of a page, from the engine. */
export interface SiteState {
  /** true: encrypted, false: plain http, null: no network origin (magnetite:, file:). */
  secure: boolean | null;
  cert_error: boolean;
  /** Percent, 100 = default. */
  zoom: number;
}

/** Chromium's zoom steps in percent (same list as the engine). */
export const ZOOM_STEPS = [25, 33, 50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300, 400, 500];

export type LinkVariant = "clean" | "original" | "markdown";

export type PermissionKind = "camera" | "microphone" | "location" | "notifications" | "popups" | "clipboard";
export type PermissionValue = "allow" | "ask" | "block";
export interface PermissionRow { kind: PermissionKind; origin: string; value: PermissionValue; requested: boolean }
export interface CaptureInfo { width: number; height: number; thumb: string; cut_at: number | null }

/** `moved`: the page had left the confirmed site, so nothing was removed. */
export interface ClearReport { domain: string; cookies: number | null; origins: string[]; failed_origins: string[]; moved: boolean }
export interface CopyResult { copied: boolean; removed?: number; variant?: LinkVariant }

/** What a page plays; reported by the engine's page script. */
export interface MediaState {
  playing: boolean;
  title: string;
  artist: string;
  video: boolean;
  pip: boolean;
  /** A stream without an end: nothing to seek in. */
  live: boolean;
  can_next: boolean;
  can_prev: boolean;
}

export type MediaAction = "play" | "pause" | "next" | "previous" | "seekforward" | "seekbackward";

export interface UiState {
  sidebar: Snapshot;
  active_space: Id;
  active_tab: Id | null;
  runtime: Record<Id, TabRuntime>;
  sidebar_visible: boolean;
  archive_count: number;
  /** Visits in the history; the history view asks again when it changes. */
  history_count: number;
  panes: Id[];
  /** The split on screen (two or more pages), if any. */
  split: SplitNode | null;
  /** Every split, on screen or not; each shows as one sidebar row. */
  splits: SplitNode[];
  /** The page shown over the tab; `runtime` is what is known about it, as for a tab. */
  peek: { url: string; title: string; runtime: TabRuntime } | null;
  settings: Settings;
  chrome: string;
  /** Connected smart folder accounts. */
  accounts: Account[];
  /** Refresh state per smart folder id. */
  smart: Record<Id, SmartStatus>;
  /** Installed extensions of the profile the page in front uses. */
  extensions: ExtensionInfo[];
}

/** An installed Chrome extension with a toolbar action. */
export interface ExtensionInfo {
  id: string;
  name: string;
  /** `chrome-extension://…` options page, if it has one. */
  options_url: string | null;
  /** Pinned in Chromium's own UI ("Pin to toolbar" on its extensions page). */
  chromium_pinned: boolean;
}

/** An extension's icon, served by the engine on this page's own origin (as the favicon cache, see `Favicon`). */
export const extensionIcon = (id: string) => `/ext-icons/${id}`;

export type Axis = "row" | "column";
export type Side = "left" | "right" | "top" | "bottom";
/** What the centred command palette opens for; every command-bar prompt uses it. `quick`: the quick search from anywhere (⌥⌘N), whose pick opens in a mini window. */
export const PALETTE_MODES = ["new", "split", "navigate", "new_space", "new_profile", "quick"] as const;
export type PaletteMode = (typeof PALETTE_MODES)[number];
export type SplitNode =
  | { kind: "pane"; id: Id }
  | { kind: "split"; axis: Axis; children: SplitNode[]; sizes: number[] };

/** Page ids of a split in reading order. */
export function splitIds(n: SplitNode): Id[] {
  return n.kind === "pane" ? [n.id] : n.children.flatMap(splitIds);
}

export interface Settings {
  auto_archive_hours: number;
  /** Archived tabs are deleted after this many days. */
  archive_days: number;
  /** Visits stay in the history this many days; 0 keeps none. */
  history_days: number;
  unload_minutes: number;
  search_template: string;
  /** Keywords that search one site from the command bar (`gh rust`). */
  site_searches: SiteSearch[];
  external_links: "tab" | "peek" | "mini";
  /** The system-wide quick search key (⌥⌘N) is on; this device's own choice. */
  quick_search: boolean;
  chrome_toolbar: boolean;
  appearance: Appearance;
  auto_pip: boolean;
  strip_trackers: boolean;
  devtools_dock: "right" | "bottom" | "window";
  sync: SyncSettings;
  /** Extensions shown in the sidebar; all are in the menu bar's Extensions menu. */
  pinned_extensions: string[];
  /** Look for a new release at start and once a day. */
  check_updates: boolean;
  /** What a click on the sidebar's archive and downloads button opens. */
  library_opens: LibraryView;
  /** Chromium's password manager and autofill: which parts are on (Settings → Passwords & autofill). */
  autofill: { passwords: boolean; addresses: boolean; cards: boolean; breach_check: boolean };
  /** The ad blocker (Settings → Ad blocking); this device's own choice, never synced. */
  adblock: {
    enabled: boolean;
    /** Ids of the filter lists subscribed to (`Info.adblock.lists`). */
    lists: string[];
    /** Sites (registrable domains) where nothing is blocked. */
    allowed_sites: string[];
  };
}

/** A full-height view behind the sidebar's archive and downloads button. */
export type LibraryView = "archive" | "downloads" | "bookmarks" | "history";

/** A keyword, a space and a query in the command bar search `url` (`%s`: the query). */
export interface SiteSearch {
  keyword: string;
  name: string;
  url: string;
}

/** How the sidebar and Magnetite's own pages look (Settings → Appearance). */
export interface Appearance {
  /** Text size in percent, 85 to 130. */
  text_scale: number;
  density: "compact" | "comfortable" | "spacious";
  /** Docked sidebar width in points, 220 to 360. */
  sidebar_width: number;
}

/** Apply text size and row density to this page. */
export function applyAppearance(a: Pick<Appearance, "text_scale" | "density">) {
  const root = document.documentElement;
  root.style.zoom = String(a.text_scale / 100);
  root.dataset.density = a.density;
}

/**
 * This page's text size as a factor (the zoom `applyAppearance` sets). The
 * pointer and the window are measured in window points, lengths set on the
 * page grow by this factor: divide the former by it to place something.
 */
export function pageZoom(): number {
  return Number(document.documentElement.style.zoom) || 1;
}

export interface SyncSettings {
  enabled: boolean;
  server_url: string;
  last_sync_at: number | null;
  last_error: string | null;
}

export type Command =
  | { cmd: "ready" }
  | { cmd: "activate_tab"; id: Id }
  | { cmd: "close_tab"; id: Id }
  | { cmd: "new_tab"; url: string; background?: boolean; split?: boolean; side?: Side }
  | { cmd: "navigate"; url: string }
  | { cmd: "go_back" }
  | { cmd: "open_extension"; id: string }
  | { cmd: "pin_extension"; id: string; pinned: boolean }
  | { cmd: "extension_options"; id: string }
  | { cmd: "go_forward" }
  | { cmd: "reload"; id?: Id }
  | { cmd: "share"; id: Id; x: number; y: number }
  /** `notify`: the sidebar shows the result too (for a sender that closes at once). */
  | { cmd: "copy_link"; variant: LinkVariant; notify?: boolean }
  | { cmd: "set_zoom"; percent: number }
  | { cmd: "capture_start" }
  | { cmd: "capture_frame" }
  | { cmd: "capture_select"; rect: [number, number, number, number] }
  | { cmd: "capture_visible" }
  | { cmd: "capture_full" }
  | { cmd: "capture_cancel" }
  | { cmd: "capture_copy"; notify?: boolean }
  | { cmd: "capture_save" }
  | { cmd: "capture_share"; x: number; y: number }
  | { cmd: "capture_dismiss" }
  | { cmd: "site_permissions" }
  | { cmd: "site_ads"; host: string }
  | { cmd: "set_site_ads"; host: string; blocking: boolean }
  | { cmd: "set_permission"; origin: string; kind: PermissionKind; value: PermissionValue }
  | { cmd: "site_data_scope" }
  | { cmd: "clear_site_data"; domain: string }
  | { cmd: "switch_space"; id: Id }
  | { cmd: "next_space" }
  | { cmd: "prev_space" }
  | { cmd: "pin"; id: Id }
  | { cmd: "unpin"; id: Id }
  | { cmd: "reset_tab"; id: Id }
  | { cmd: "rename_tab"; id: Id; title: string }
  | { cmd: "set_keep_active"; id: Id; keep: boolean }
  | { cmd: "new_space"; title: string; icon?: string }
  | { cmd: "rename_space"; id: Id; title: string; icon?: string }
  | { cmd: "delete_space"; id: Id }
  | { cmd: "move_space"; id: Id; index: number }
  | { cmd: "new_folder"; title: string }
  | { cmd: "rename_folder"; id: Id; title: string }
  | { cmd: "delete_folder"; id: Id }
  | { cmd: "move_tab"; id: Id; space: Id; section: "favorites" | "pinned" | "today"; index: number; folder?: Id; profile?: Id }
  | { cmd: "move_folder"; id: Id; index: number }
  | { cmd: "add_split"; id: Id; side?: Side }
  | { cmd: "split_insert"; target?: Id; tab: Id; side: Side }
  | { cmd: "split_equalize"; path: number[] }
  | { cmd: "split_drag_start"; tab: Id }
  | { cmd: "split_drop_frame" }
  | { cmd: "split_drop"; target: Id; side: Side }
  | { cmd: "split_drag_end" }
  | { cmd: "remove_split"; id: Id }
  | { cmd: "separate_all"; id?: Id }
  /** Archive the current space's Today tabs, all but `keep`. */
  | { cmd: "archive_today"; keep?: Id }
  | { cmd: "reopen_closed" }
  | { cmd: "restore_archived"; id: Id }
  | { cmd: "remove_archived"; id: Id }
  | { cmd: "archive"; query?: string }
  | { cmd: "search"; query: string }
  | { cmd: "history"; query?: string }
  | { cmd: "remove_history"; id: Id }
  | { cmd: "clear_history" }
  | { cmd: "show_history" }
  /** Typed input or an address, as a peek over the page in front. */
  | { cmd: "open_peek"; url: string }
  /** Typed input or an address, in a new mini window. */
  | { cmd: "open_mini"; url: string }
  /** The system asks the user; answers once they did. */
  | { cmd: "make_default_browser" }
  | { cmd: "set_settings"; settings: Partial<Settings> }
  | { cmd: "get_settings" }
  | { cmd: "sync_create"; server: string; invite?: string }
  | { cmd: "sync_join"; server: string; words: string }
  | { cmd: "sync_now" }
  | { cmd: "sync_code" }
  | { cmd: "sync_leave" }
  | { cmd: "sync_delete_chain" }
  | { cmd: "new_profile"; name: string; ephemeral?: boolean }
  | { cmd: "rename_profile"; id: Id; name: string }
  | { cmd: "delete_profile"; id: Id }
  /** `synced` false keeps the space on this device alone. */
  | { cmd: "set_space_synced"; id: Id; synced: boolean }
  | { cmd: "set_space_profile"; id: Id; profile: Id }
  | { cmd: "close_peek" }
  | { cmd: "peek_to_tab" }
  | { cmd: "peek_to_split" }
  | { cmd: "smart_folder_dialog"; id?: Id }
  | { cmd: "smart_connect"; service: Service; host: string; token: string }
  | { cmd: "smart_disconnect"; id: Id }
  | { cmd: "smart_preview"; source: SmartSource; account?: string }
  | { cmd: "smart_save"; id?: Id; title: string; source: SmartSource; account?: string; limit: number; refresh_minutes: number }
  | { cmd: "smart_refresh"; id: Id }
  | { cmd: "smart_unlink"; id: Id }
  | { cmd: "smart_hide"; id: Id }
  | { cmd: "smart_allow"; id: Id }
  | { cmd: "toggle_sidebar" }
  | { cmd: "focus_sidebar" }
  | { cmd: "float_sidebar"; show: boolean }
  | { cmd: "set_space_color"; id: Id; color: Color | null }
  | { cmd: "set_folder_icon"; id: Id; icon: string | null }
  | { cmd: "toggle_devtools" }
  | { cmd: "open_palette"; mode: PaletteMode; side?: Side; tab?: Id }
  /** The welcome page is done: the app reads the chosen browsers (`import_sources` keys) and closes the page. */
  | { cmd: "finish_welcome"; import: string[]; default_browser?: boolean }
  | { cmd: "palette_resize"; height: number }
  | { cmd: "palette_close" }
  | { cmd: "get_state" }
  | { cmd: "show_archive" }
  | { cmd: "downloads" }
  | { cmd: "open_download"; id: Id; reveal?: boolean }
  | { cmd: "clear_downloads" }
  | { cmd: "show_downloads" }
  /** One of Chromium's own pages (saved passwords, addresses, cards) as a peek, in the logins of `profile`. */
  | { cmd: "open_engine_page"; page: "passwords" | "addresses" | "cards"; profile?: Id }
  | { cmd: "show_bookmarks" }
  /** Save a tab's page to the bookmarks of its space; without `id`, the active tab's. */
  | { cmd: "add_bookmark"; id?: Id }
  /** Open a bookmark as a new tab of its space, which comes to the front. */
  | { cmd: "open_bookmark"; id: Id; background?: boolean }
  /** A bookmark, or a bookmark folder with all it holds. */
  | { cmd: "remove_bookmark"; id: Id }
  | { cmd: "rename_bookmark"; id: Id; title: string }
  | { cmd: "new_bookmark_folder"; space: Id; parent?: Id; title?: string }
  /** `index` counts the list without the moved item (`indexAfterTaking`). */
  | { cmd: "move_bookmark"; id: Id; space: Id; folder?: Id; index: number }
  | { cmd: "notifications_resize"; height: number }
  | { cmd: "permission_prompts" }
  | { cmd: "answer_permission"; id: string; answer: PromptAnswer }
  | { cmd: "sidebar_width"; width: number; done?: boolean }
  | { cmd: "update_status" }
  | { cmd: "check_updates" }
  | { cmd: "install_update" }
  | { cmd: "open_update_page" }
  | { cmd: "set_tab_profile"; id: Id; profile: Id | null }
  | { cmd: "duplicate_favorite"; id: Id }
  | { cmd: "media"; id: Id; action: MediaAction }
  | { cmd: "set_muted"; id: Id; muted: boolean }
  | { cmd: "picture_in_picture"; id: Id }
  | { cmd: "import_sources" }
  | { cmd: "import_from"; key: string }
  /** The text of a bookmarks file (the HTML browsers export): its bookmarks join the current space's. */
  | { cmd: "import_bookmarks"; html: string }
  | { cmd: "shortcut"; shortcut: string | { select_tab: number } | { select_space: number } };

/** A browser on this computer that Magnetite can import from. */
export interface ImportSource {
  key: string;
  name: string;
}

export interface SearchResult {
  kind: "tab" | "bookmark" | "history" | "archived";
  id: Id;
  title: string;
  url: string;
  space?: string;
  favicon?: string | null;
}

/** One visit in the history (`history.rs` in the app). */
export interface Visit {
  id: Id;
  url: string;
  title: string;
  visited_at: number;
}

/** Whether links from other apps open in Magnetite; "unknown" where the system cannot say. */
export type DefaultBrowser = "yes" | "no" | "unknown";

export interface ArchiveItem {
  id: Id;
  title: string;
  url: string;
  archived_at: number;
  space?: string | null;
}

/** The frozen page the capture overlay draws: element boxes in CSS px, largest first. */
export interface CaptureFrame {
  /** PNG data URL. */
  image: string;
  css_width: number;
  css_height: number;
  boxes: [number, number, number, number][];
  /** The boxes match the frame (the page did not move while freezing): elements can be picked. */
  element_ok: boolean;
}

/** A visible page under the split drop zones: bounds in the page area and a screenshot. */
export interface SplitDropPane {
  tab: Id;
  rect: [number, number, number, number];
  image: string | null;
  title: string;
}

/** What the split drop zones draw for the tab being dragged; `full`: no room for another page. */
export interface SplitDropFrame {
  tab: Id;
  title: string;
  url: string;
  full: boolean;
  panes: SplitDropPane[];
}

/**
 * Replies of the commands that answer with data (`handle_command` in
 * crates/app/src/app/commands.rs). `{ image?: undefined }` and the like stand for the
 * app's `{}`: nothing to show. The other commands answer only `{ "ok": … }`.
 */
interface Replies {
  get_state: UiState;
  get_settings: Info;
  search: { results: SearchResult[] };
  archive: { items: ArchiveItem[] };
  history: { items: Visit[] };
  make_default_browser: { default_browser: DefaultBrowser };
  new_profile: { id: Id };
  smart_save: { id: Id };
  smart_connect: Account;
  smart_preview: SmartItem[];
  sync_code: { words: string | null };
  copy_link: CopyResult;
  downloads: { items: DownloadEntry[] };
  permission_prompts: { items: PermissionPrompt[] };
  update_status: UpdateInfo;
  check_updates: UpdateInfo;
  import_sources: { sources: ImportSource[] };
  import_from: { spaces: number; tabs: number; bookmarks: number };
  import_bookmarks: { bookmarks: number };
  add_bookmark: { saved: boolean };
  new_bookmark_folder: { id: Id | null };
  /** `failed`: browsers that could not be read; then nothing was imported and the page stays. */
  finish_welcome: { failed?: { key: string; name: string; error: string }[] };
  capture_frame: CaptureFrame | { image?: undefined };
  capture_copy: { copied: boolean };
  capture_save: { saved: string | null };
  split_drop_frame: SplitDropFrame | { tab?: undefined };
  site_permissions: { rows: PermissionRow[] };
  /** `site`: the registrable domain of the host asked about. */
  site_ads: { site: string; enabled: boolean; blocking: boolean };
  site_data_scope: { domain: string; origins: string[] } | { domain?: undefined };
}

/** What command `K` resolves with: its entry in `Replies`, else `unknown`. */
type Reply<K extends Command["cmd"]> = K extends keyof Replies ? Replies[K] : unknown;

type QueryFn = (q: { request: string; persistent?: boolean; onSuccess: (r: string) => void; onFailure: (code: number, msg: string) => void }) => void;

declare global {
  interface Window {
    magnetiteQuery?: QueryFn;
    __magnetite: {
      setState: (s: UiState) => void;
      copied: (r: CopyResult) => void;
      openSitePanel: () => void;
      siteDataCleared: (r: ClearReport) => void;
      openArchive: () => void;
      openDownloads: () => void;
      openBookmarks: () => void;
      openHistory: () => void;
      downloads: (d: { items: DownloadEntry[] }) => void;
      prompts: (p: { items: PermissionPrompt[] }) => void;
      update: (u: UpdateInfo) => void;
      notice: (n: { text: string }) => void;
      captureReady: (r: CaptureInfo) => void;
      captureFailed: (r: { error: string }) => void;
      captureBusy: (r: { what: string }) => void;
    };
  }
}

/** Run `command` in the app; resolves with its reply (see `Replies`). */
export function send<K extends Command["cmd"]>(command: Extract<Command, { cmd: K }>): Promise<Reply<K>> {
  return new Promise((resolve, reject) => {
    const fn = window.magnetiteQuery;
    if (!fn) {
      // Running outside the browser (plain Vite dev in Chrome): log only.
      console.debug("magnetite command (no host)", command);
      resolve({} as Reply<K>);
      return;
    }
    fn({
      request: JSON.stringify(command),
      onSuccess: (r) => resolve((r ? JSON.parse(r) : {}) as Reply<K>),
      onFailure: (_code, msg) => reject(new Error(msg)),
    });
  });
}

/** One entry of the download list (`downloads.rs` in the app). */
export interface DownloadEntry {
  id: Id;
  name: string;
  status: "in_progress" | "finished" | "failed";
  received: number;
  total: number | null;
  /** Start time, Unix seconds. */
  at: number;
  /** When its status last changed in this run (Unix seconds; 0: before). */
  changed_at: number;
  /** Finished, but the file was moved or deleted since. */
  missing: boolean;
  /** Runs code or hands off when opened: only ever shown in the file manager. */
  risky: boolean;
}

/** What a permission prompt asks for (`PromptKind` in the engine). */
export type PromptKind =
  | "camera"
  | "camera_control"
  | "microphone"
  | "location"
  | "notifications"
  | "clipboard"
  | "downloads"
  | "midi"
  | "fonts"
  | "idle"
  | "windows"
  | "local_network"
  | "sensors"
  | "storage_access"
  | "links"
  | "keyboard_lock"
  | "pointer_lock"
  | "other";

export type PromptAnswer = "allow" | "block" | "dismiss";

/** A permission prompt of a page on screen; `site` is the asking host. */
export interface PermissionPrompt {
  id: string;
  site: string;
  kinds: PromptKind[];
}

/** Where the update check stands (`update.rs` in the app). */
export type UpdateStatus =
  | { state: "idle" }
  | { state: "unavailable" }
  | { state: "checking" }
  | { state: "up_to_date"; checked_at: number }
  | { state: "available"; version: string; notes_url: string; installable: boolean }
  | { state: "installing"; version: string }
  | { state: "failed"; error: string };

export interface UpdateInfo {
  status: UpdateStatus;
  /** The running version. */
  version: string;
}

export type CaptureEvent = { kind: "ready"; info: CaptureInfo } | { kind: "failed"; error: string } | { kind: "busy"; what: string };

/** Listeners for one kind of message from the app; `on` returns the unsubscribe. */
function signal<T = void>() {
  const listeners = new Set<(value: T) => void>();
  return {
    emit: (value: T) => listeners.forEach((l) => l(value)),
    on: (l: (value: T) => void): (() => void) => {
      listeners.add(l);
      return () => {
        listeners.delete(l);
      };
    },
  };
}

/**
 * A signal that keeps its last value and hands it to each new listener.
 * Until a value arrives, the first listener asks the app for it with `fetch`
 * (once); a fetch resolving `undefined` has nothing to show.
 */
function cached<T extends object>(fetch?: () => Promise<T | undefined>) {
  const changes = signal<T>();
  let last: T | undefined;
  let asked = false;
  const emit = (value: T) => {
    last = value;
    changes.emit(value);
  };
  return {
    emit,
    on: (l: (value: T) => void): (() => void) => {
      const off = changes.on(l);
      if (last) l(last);
      else if (fetch && !asked) {
        asked = true;
        void fetch().then((value) => value && emit(value));
      }
      return off;
    },
  };
}

const states = cached<UiState>();
const copies = signal<CopyResult>();
const sitePanelOpens = signal();
const clearReports = signal<ClearReport>();
const archiveOpens = signal();
const downloadsOpens = signal();
const bookmarksOpens = signal();
const historyOpens = signal();
const downloads = cached(() => send({ cmd: "downloads" }).then((r) => r.items ?? []));
const prompts = cached(() => send({ cmd: "permission_prompts" }).then((r) => r.items ?? []));
const updates = cached(() => send({ cmd: "update_status" }).then((u) => (u.status ? u : undefined)));
const notices = signal<string>();
const captures = signal<CaptureEvent>();

window.__magnetite = {
  setState: states.emit,
  copied: copies.emit,
  openSitePanel: sitePanelOpens.emit,
  siteDataCleared: clearReports.emit,
  openArchive: archiveOpens.emit,
  openDownloads: downloadsOpens.emit,
  openBookmarks: bookmarksOpens.emit,
  openHistory: historyOpens.emit,
  update: updates.emit,
  downloads: ({ items }) => downloads.emit(items),
  prompts: ({ items }) => prompts.emit(items),
  notice: ({ text }) => notices.emit(text),
  captureReady: (info) => captures.emit({ kind: "ready", info }),
  captureFailed: ({ error }) => captures.emit({ kind: "failed", error }),
  captureBusy: ({ what }) => captures.emit({ kind: "busy", what }),
};

/** The UI state, now (once the app sent it) and on every push. */
export const subscribe = states.on;
/** Short messages from the app ("Split view holds up to four pages"). */
export const onNotice = notices.on;
/** The download list, now and on every change (fetched on first use). */
export const onDownloads = downloads.on;
/** The permission prompts to show, now and on every change (fetched on first use). */
export const onPrompts = prompts.on;
/** The update status, now (fetched on first use) and on every change. */
export const onUpdate = updates.on;
/** "Downloads" from the menu bar, the palette or Chromium's shortcut. */
export const onOpenDownloads = downloadsOpens.on;
/** "Open bookmarks" chosen in the palette window. */
export const onOpenBookmarks = bookmarksOpens.on;
/** "Show History" from the menu bar, ⌘Y, or "Open history" in the palette. */
export const onOpenHistory = historyOpens.on;
/** "Open archive" chosen in the palette window. */
export const onOpenArchive = archiveOpens.on;
/** Results of the capture tool, for the sidebar toast. */
export const onCapture = captures.on;
export const onSiteDataCleared = clearReports.on;
/** Copy results from keyboard shortcuts pressed in a page window. */
export const onCopied = copies.on;
export const onOpenSitePanel = sitePanelOpens.on;

/** A space's colour when it has none of its own. */
export const DEFAULT_ACCENT: Color = { red: 120 / 255, green: 130 / 255, blue: 1 };

export function accentCss(c?: Color, alpha = 1): string {
  const { red, green, blue } = c ?? DEFAULT_ACCENT;
  const f = (v: number) => Math.round(Math.min(1, Math.max(0, v)) * 255);
  return `rgba(${f(red)}, ${f(green)}, ${f(blue)}, ${alpha})`;
}

export function hostOf(url: string): string {
  try {
    return new URL(url).host.replace(/^www\./, "") || url;
  } catch {
    return url;
  }
}

/** Tab `id` in a space: Today, then the pinned tabs and their folders. */
export function findTabInSpace(space: Space, id: Id): TabItem | null {
  for (const t of space.today) if (t.id === id) return t;
  const stack = [...space.pinned];
  for (let n = stack.pop(); n; n = stack.pop()) {
    if (n.kind === "tab" && n.id === id) return n;
    if (n.kind === "folder") stack.push(...n.children);
  }
  return null;
}

/** Tab `id` anywhere: the favorites, then each space. */
export function findTab(state: UiState, id: Id | undefined): TabItem | null {
  if (!id) return null;
  for (const f of state.sidebar.favorites) for (const t of f.items) if (t.id === id) return t;
  for (const s of state.sidebar.spaces) {
    const t = findTabInSpace(s, id);
    if (t) return t;
  }
  return null;
}

/** The app runs on macOS (Finder, ⌘ shortcuts). */
export const IS_MAC = navigator.userAgent.includes("Mac");
