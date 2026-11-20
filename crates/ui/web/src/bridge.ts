// Bridge between the sidebar page and the Rust application.
//
// Rust -> JS: window.__magnetite.setState(state) and a few UI calls.
// JS -> Rust: window.magnetiteQuery({request: JSON, onSuccess, onFailure}) (CEF message router).

import type { ShortcutAction, ShortcutKeys } from "./settings/shortcutKeys";
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
  /** A favorite for these spaces only; absent or empty: for every space. */
  only_in?: Id[];
  /** "Keep active": never unloaded for being out of sight, never auto-archived. */
  keep_active?: boolean;
  /** A favorite that opens as a web panel, over the page in front. */
  panel?: boolean;
  /** The width the web panel was dragged to, in points. */
  panel_width?: number;
  /** The web panel's page is shown as on a phone. */
  panel_mobile?: boolean;
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
  /** Items put away; absent when there are none. */
  snoozed?: Snoozed[];
}

/** A smart folder's item that is put away. */
export interface Snoozed {
  url: string;
  title: string;
  /** Unix seconds when it comes back; none: only when woken by hand. */
  until?: number;
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
  /** A picture of the user's own shown in place of `icon`: a small WebP file as base64 (`storeIcon`). It syncs with the space. */
  icon_image?: string;
  accent?: Color;
  profile_id: Id;
  /** The keys saved for the space; those that count are in `UiState.space_keys`. */
  shortcut?: string;
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
  /** A page shares this tab's picture right now (screen sharing). */
  shared: boolean;
  dev: DevState;
}

/** What a page is shown as instead of this computer's own settings (`dev.rs`). */
export interface Emulation {
  color_scheme: "light" | "dark" | null;
  reduced_motion: boolean;
  print: boolean;
  network: "online" | "slow" | "offline";
  slow_cpu: boolean;
  no_cache: boolean;
  /** The time zone the page sees (`Asia/Tokyo`); `null`: this computer's. */
  timezone: string | null;
}

/** A page for developers (`dev.rs`). */
export interface DevState {
  /** Served from this computer or a host named in Settings → Developer. */
  local: boolean;
  /** Console errors since the page began to load. */
  errors: number;
  /** `host:port` nothing listens at yet; the page loads once something does. */
  waiting: string | null;
  /** The folder of the program serving the page. */
  project: string | null;
  /** This computer serves the page: "reset" clears its site data too. */
  clears: boolean;
  emulation: Emulation;
  /** The page has a phone beside it (a responsive split): both show one address. */
  responsive: boolean;
}

/** A program listening on one of this computer's ports. */
export interface DevServer {
  port: number;
  command: string;
  project: string | null;
}

/** Connection and zoom of a page, from the engine. */
export interface SiteState {
  /** true: encrypted, false: plain http, null: no network origin (magnetite:, file:). */
  secure: boolean | null;
  cert_error: boolean;
  /** Percent, 100 = default. */
  zoom: number;
  /** The zoom, in percent, the site has without one of its own. */
  base_zoom?: number;
  /** The system's own web engine shows the page (WebKit on macOS), not Chromium. */
  system_engine?: boolean;
}

/** Chromium's zoom steps in percent (same list as the engine). */
export const ZOOM_STEPS = [25, 33, 50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300, 400, 500];

export type LinkVariant = "clean" | "original" | "markdown";

export type PermissionKind = "camera" | "microphone" | "location" | "notifications" | "popups" | "clipboard";
export type PermissionValue = "allow" | "ask" | "block";
/** A site's own choice for one permission, as Settings → Permissions lists them. */
export interface PermissionEntry { origin: string; kind: PermissionKind; value: PermissionValue }
export interface PermissionRow { kind: PermissionKind; origin: string; value: PermissionValue; requested: boolean }
export interface CaptureInfo { width: number; height: number; thumb: string; cut_at: number | null; /** The element's description went to the clipboard as text. */ context: boolean }

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
  /** A muted video: no sound to control, but it can pop out. */
  silent: boolean;
  can_next: boolean;
  can_prev: boolean;
}

export type MediaAction = "play" | "pause" | "next" | "previous" | "seekforward" | "seekbackward";

/** A tab open on another device of the sync chain. */
export interface DeviceTab {
  /** The device's name: "iPhone". */
  device: string;
  id: Id;
  url: string;
  title: string;
  /** The space it is open in there, if this device has it. */
  space: Id | null;
}

export interface UiState {
  sidebar: Snapshot;
  active_space: Id;
  /** The private space, while one is open and not hidden: not saved, a profile in memory per tab. */
  private_space: Id | null;
  active_tab: Id | null;
  runtime: Record<Id, TabRuntime>;
  sidebar_visible: boolean;
  archive_count: number;
  /** Tabs open on other devices of the sync chain; a click takes one over. */
  device_tabs: DeviceTab[];
  /** Visits in the history; the history view asks again when it changes. */
  history_count: number;
  panes: Id[];
  /** The split on screen (two or more pages), if any. */
  split: SplitNode | null;
  /** Every split, on screen or not; each shows as one sidebar row. */
  splits: SplitNode[];
  /** The page shown over the tab; `runtime` is what is known about it, as for a tab. */
  peek: { url: string; title: string; runtime: TabRuntime } | null;
  /** The favorite whose web panel is open. */
  web_panel: Id | null;
  /** While Ctrl+Tab goes through the tabs by last use: the tabs of the round and the place it stands on. */
  switcher: { tabs: Id[]; at: number } | null;
  settings: Settings;
  /** The keys each shortcut has now (Settings → Shortcuts). */
  keys: ShortcutKeys;
  /** The keys that go to a space, by space (Settings → Shortcuts); one without keys of its own is missing. */
  space_keys: Record<Id, string>;
  chrome: string;
  /** Connected smart folder accounts. */
  accounts: Account[];
  /** Refresh state per smart folder id. */
  smart: Record<Id, SmartStatus>;
  /** Installed extensions of the profile the page in front uses. */
  extensions: ExtensionInfo[];
  /** Those of them pinned to the sidebar in that profile, in order. */
  pinned_extensions: string[];
  /** The page being recorded as a video: its tab, and since when (ms since 1970). */
  recording: { tab: Id | null; since: number } | null;
}

/** An installed Chrome extension with a toolbar action. */
export interface ExtensionInfo {
  id: string;
  name: string;
  /** `chrome-extension://…` options page, if it has one. */
  options_url: string | null;
  /** Pinned in Chromium's own UI ("Pin to toolbar" on its extensions page). */
  chromium_pinned: boolean;
  /** From the Chrome Web Store: it has a page there to install it from. */
  from_store: boolean;
}

/** An extension's icon, served by the engine on this page's own origin (as the favicon cache, see `Favicon`). */
export const extensionIcon = (id: string) => `/ext-icons/${id}`;

export type Axis = "row" | "column";
export type Side = "left" | "right" | "top" | "bottom";
/** What the centred command palette opens for; every command-bar prompt uses it. `quick`: the quick search from anywhere (⌥⌘N), whose pick opens in a mini window. `spaces`: the list of all spaces, to go to one (`SpacePicker`). */
export const PALETTE_MODES = ["new", "split", "navigate", "new_space", "new_profile", "quick", "spaces"] as const;
export type PaletteMode = (typeof PALETTE_MODES)[number];
export type SplitNode =
  | { kind: "pane"; id: Id }
  | { kind: "split"; axis: Axis; children: SplitNode[]; sizes: number[] };

/** Page ids of a split in reading order. */
export function splitIds(n: SplitNode): Id[] {
  return n.kind === "pane" ? [n.id] : n.children.flatMap(splitIds);
}

/** A link from another app to `site` (a host, or a host and the start of a path) opens in space `space`. */
export interface LinkRule {
  site: string;
  space: Id;
}

/** CSS and JavaScript the user added to the pages of `site` (a host, standing for its subdomains too); `enabled` off keeps them unused. */
export interface SiteCode {
  site: string;
  css: string;
  js: string;
  enabled: boolean;
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
  /** Which space a link from another app opens in, by its site. */
  link_rules: LinkRule[];
  /** The user's own CSS and JavaScript per site (Settings → Custom CSS & JS). */
  site_code: SiteCode[];
  /** The system-wide quick search key (⌥⌘N) is on; this device's own choice. */
  quick_search: boolean;
  chrome_toolbar: boolean;
  appearance: Appearance;
  auto_pip: boolean;
  several_windows: boolean;
  strip_trackers: boolean;
  /** A web panel closes by itself when a page behind it is used. */
  panel_autoclose: boolean;
  devtools_dock: "right" | "bottom" | "window";
  sync: SyncSettings;
  /** Extensions with an icon in the sidebar or the top bar; all are in the menu bar's Extensions menu. */
  pinned_extensions: string[];
  /** Look for a new release at start and once a day. */
  check_updates: boolean;
  /** What a click on the sidebar's archive and downloads button opens. */
  library_opens: LibraryView;
  /** The order Ctrl+Tab goes through the tabs in: by last use, or as the sidebar lists them. */
  tab_order: "recent" | "sidebar";
  /** Chromium's password manager and autofill: which parts are on (Settings → Passwords & autofill). */
  autofill: { passwords: boolean; addresses: boolean; cards: boolean; breach_check: boolean };
  /** Tell websites not to sell or share the user's data: Global Privacy Control (Settings → Data & privacy). */
  global_privacy_control: boolean;
  /** Where downloads go (Settings → General): `folder` as a whole path, empty for the system's Downloads folder; `ask` to be asked for each file. A patch can empty `folder`; a new one comes from `choose_download_folder`. */
  downloads: DownloadPrefs;
  /** The zoom, in percent, a site has until it is zoomed on its own (Settings → Appearance): one of Chromium's steps. */
  page_zoom: number;
  /** Spell check in text fields, and the languages websites are asked for, the preferred one first, as language tags; empty for the system's (Settings → General). */
  languages: { spellcheck: boolean; languages: string[] };
  /** The ad blocker (Settings → Ad blocking); this device's own choice, never synced. */
  /** Developer features (Settings → Developer). */
  developer: { enabled: boolean; hosts: string[] };
  /** Which sites the system's own web engine shows (Settings → Protected video): the known streaming services, and sites of the user's own, by host. This device's own, never synced. */
  system_engine: { known: boolean; sites: string[] };
  adblock: {
    enabled: boolean;
    /** Ids of the filter lists subscribed to (`Info.adblock.lists`). */
    lists: string[];
    /** Sites (registrable domains) where nothing is blocked. */
    allowed_sites: string[];
  };
}

/** What a page in device mode is shown as (the device bar, `device.rs`). */
export interface DeviceConfig {
  /** "Responsive", or a device's name. */
  preset: string;
  /** CSS pixels, as the device is held. */
  width: number;
  height: number;
  dpr: number;
  /** Touch and a mobile viewport. */
  mobile: boolean;
  /** Whose browser identity the page sees; null: Magnetite's own. */
  ua: "iphone" | "ipad" | "android" | null;
  /** Percent; null: as large as fits. */
  zoom: number | null;
  throttle: "none" | "fast4g" | "slow4g" | "3g" | "offline";
  /** The Save-Data hint; null: the browser's own. */
  save_data: boolean | null;
}

/** A full-height view behind the sidebar's archive and downloads button. */
export type LibraryView = "archive" | "downloads" | "bookmarks" | "history";

/** A keyword, a space and a query in the command bar search `url` (`%s`: the query). */
export interface SiteSearch {
  keyword: string;
  name: string;
  url: string;
}

/** The kinds of file bookmarks are exported as. */
export type BookmarkFormat = "html" | "json" | "csv";

export interface DownloadPrefs {
  folder: string;
  ask: boolean;
}

/** How the sidebar and Magnetite's own pages look (Settings → Appearance). */
export interface Appearance {
  /** Text size in percent, 85 to 130. */
  text_scale: number;
  density: "compact" | "comfortable" | "spacious";
  /** Docked sidebar width in points, 220 to 360. */
  sidebar_width: number;
  /** Navigation, address and window buttons in a bar across the top. */
  top_bar: boolean;
  /** A row of extension icons below the sidebar's address, in place of the icons in its title band. */
  extension_bar: boolean;
  /** How each space looks beyond its colour, by space id; a space without an entry has `DEFAULT_LOOK`. Kept on this device. */
  looks: Record<Id, SpaceLook>;
}

/** One space's look beyond its colour. */
export interface SpaceLook {
  /** How much of the space colour the window takes, in percent, 0 to 30. */
  tint: number;
  /** What lies over the sidebar's colour; "image" is the user's own. */
  sidebar_effect: "none" | "grain" | "glow" | "aurora" | "fade" | "image";
  /** When the own sidebar image was stored, `null` without one (`userImageUrl`). */
  sidebar_image: number | null;
  /** The same for the picture behind the empty window. */
  empty_image: number | null;
}

export const DEFAULT_LOOK: SpaceLook = { tint: 8, sidebar_effect: "none", sidebar_image: null, empty_image: null };

/** The look of space `id`: its own, or the default one. */
export const lookOf = (a: Appearance, id: Id): SpaceLook => a.looks[id] ?? DEFAULT_LOOK;

/** Apply text size and row density to this page. */
export function applyAppearance(a: Pick<Appearance, "text_scale" | "density">) {
  const root = document.documentElement;
  root.style.zoom = String(a.text_scale / 100);
  root.style.setProperty("--zoom", root.style.zoom);
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
  | { cmd: "pin_extension"; id: string; pinned: boolean; profile?: Id }
  | { cmd: "extension_options"; id: string }
  | { cmd: "go_forward" }
  | { cmd: "reload"; id?: Id }
  | { cmd: "share"; id: Id; x: number; y: number }
  /** `notify`: the sidebar shows the result too (for a sender that closes at once). */
  | { cmd: "copy_link"; variant: LinkVariant; notify?: boolean }
  | { cmd: "set_zoom"; percent: number }
  | { cmd: "capture_start" }
  | { cmd: "capture_frame" }
  | { cmd: "capture_select"; rect: [number, number, number, number]; context?: boolean }
  | { cmd: "capture_visible" }
  | { cmd: "capture_full" }
  | { cmd: "capture_cancel" }
  | { cmd: "capture_copy"; notify?: boolean }
  | { cmd: "capture_save" }
  | { cmd: "capture_share"; x: number; y: number }
  | { cmd: "capture_dismiss" }
  | { cmd: "site_permissions" }
  /** Every site with a choice of its own, in the logins of `profile` (none: the default). */
  | { cmd: "all_permissions"; profile?: Id }
  /** Set what site `origin` may do in the logins of `profile`; "ask" takes the choice back. */
  | { cmd: "set_profile_permission"; profile?: Id; origin: string; kind: PermissionKind; value: PermissionValue }
  | { cmd: "device_get"; page: number }
  | { cmd: "device_set"; page: number; config: DeviceConfig | null }
  | { cmd: "site_ads"; url: string }
  | { cmd: "set_site_ads"; url: string; blocking: boolean }
  | { cmd: "site_engine"; url: string }
  /** Show the site of the page at `url` in the system's engine from now on, or in Chromium again; the page in front loads again. */
  | { cmd: "set_site_engine"; url: string; system: boolean }
  /** Delete the cookies, site storage and cache the system's engine keeps, in every profile. */
  | { cmd: "clear_system_engine_data" }
  | { cmd: "set_adblock_enabled"; enabled: boolean }
  | { cmd: "set_adblock_list"; id: string; subscribed: boolean }
  | { cmd: "set_permission"; origin: string; kind: PermissionKind; value: PermissionValue }
  | { cmd: "site_data_scope" }
  | { cmd: "clear_site_data"; domain: string }
  | { cmd: "switch_space"; id: Id }
  | { cmd: "open_private_space" }
  | { cmd: "close_private_space" }
  | { cmd: "hide_private_space" }
  | { cmd: "next_space" }
  | { cmd: "prev_space" }
  | { cmd: "pin"; id: Id }
  | { cmd: "unpin"; id: Id }
  | { cmd: "reset_tab"; id: Id }
  | { cmd: "rename_tab"; id: Id; title: string }
  | { cmd: "set_keep_active"; id: Id; keep: boolean }
  | { cmd: "set_web_panel"; id: Id; panel: boolean }
  | { cmd: "set_panel_mobile"; id: Id; mobile: boolean }
  | { cmd: "toggle_web_panel"; id: Id }
  /** A new space; it becomes the current one unless `stay` is set. */
  | { cmd: "new_space"; title: string; icon?: string; stay?: boolean }
  | { cmd: "rename_space"; id: Id; title?: string; icon?: string }
  | { cmd: "delete_space"; id: Id }
  | { cmd: "move_space"; id: Id; index: number }
  | { cmd: "new_folder"; title: string; parent?: Id }
  | { cmd: "rename_folder"; id: Id; title: string }
  | { cmd: "share_folder"; id: Id }
  | { cmd: "read_shared"; url: string }
  | { cmd: "add_shared"; url: string }
  | { cmd: "delete_folder"; id: Id }
  | { cmd: "move_tab"; id: Id; space: Id; section: "favorites" | "pinned" | "today"; index: number; folder?: Id; profile?: Id; split?: boolean }
  | { cmd: "move_folder"; id: Id; index: number; folder?: Id }
  | { cmd: "add_split"; id: Id; side?: Side }
  | { cmd: "split_insert"; target?: Id; tab: Id; side: Side }
  | { cmd: "split_reorder"; target: Id; tab: Id; side: Side }
  | { cmd: "split_equalize"; path: number[] }
  | { cmd: "split_grid" }
  | { cmd: "split_drag_start"; tab: Id }
  | { cmd: "split_drop_frame" }
  | { cmd: "split_drop"; target: Id; side?: Side }
  | { cmd: "split_drag_end" }
  | { cmd: "remove_split"; id: Id }
  | { cmd: "separate_all"; id?: Id }
  /** Archive the current space's Today tabs, all but `keep`. */
  | { cmd: "archive_today"; keep?: Id }
  | { cmd: "take_device_tab"; id: Id }
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
  /** The system's dialog for the folder downloads are saved to; answers with the settings' `downloads` once it is closed. */
  | { cmd: "choose_download_folder" }
  /** What is named changes; `downloads` and `languages` may each name one of their two parts. */
  | { cmd: "set_settings"; settings: Partial<Omit<Settings, "downloads" | "languages">> & { downloads?: Partial<DownloadPrefs>; languages?: Partial<Settings["languages"]> } }
  /** Store an own image (WebP as base64) for `slot` of space `space`, or remove it (`null`). Settings page only. */
  | { cmd: "set_appearance_image"; slot: "empty" | "sidebar"; space: Id; data: string | null }
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
  | { cmd: "space_dialog"; id: Id }
  | { cmd: "smart_connect"; service: Service; host: string; token: string }
  | { cmd: "smart_disconnect"; id: Id }
  | { cmd: "smart_preview"; source: SmartSource; account?: string }
  | { cmd: "smart_save"; id?: Id; title: string; source: SmartSource; account?: string; limit: number; refresh_minutes: number }
  | { cmd: "smart_refresh"; id: Id }
  | { cmd: "smart_unlink"; id: Id }
  | { cmd: "smart_snooze"; id: Id; until?: number }
  | { cmd: "smart_wake"; id: Id; url: string }
  | { cmd: "smart_allow"; id: Id }
  | { cmd: "toggle_sidebar" }
  | { cmd: "toggle_top_bar" }
  | { cmd: "window_minimize" }
  | { cmd: "window_maximize" }
  | { cmd: "window_fullscreen" }
  | { cmd: "window_close" }
  /** `dev_bar`: asked by the developer bar, and placed below it. */
  | { cmd: "top_bar_popup"; popup: TopBarPopupKind; id?: string; x: number; y: number; width: number; height: number; dev_bar?: boolean }
  | { cmd: "focus_sidebar" }
  | { cmd: "float_sidebar"; show: boolean }
  | { cmd: "set_space_color"; id: Id; color: Color | null }
  | { cmd: "set_space_icon_image"; id: Id; data: string | null }
  | { cmd: "set_folder_icon"; id: Id; icon: string | null }
  | { cmd: "toggle_devtools" }
  | { cmd: "dev_panel"; panel: "elements" | "console" | "network" | "sources" | "application" | "picker" }
  | { cmd: "dev_reset" }
  | { cmd: "dev_emulate"; patch: Partial<Emulation> }
  | { cmd: "dev_servers" }
  /** The page in front again, beside itself, with the logins of `profile`. */
  | { cmd: "dev_split_as"; profile: Id }
  | { cmd: "dev_responsive" }
  | { cmd: "dev_capture_set" }
  /** Start recording the page in front, or end the recording that runs. */
  | { cmd: "dev_record" }
  /** `recorder.ts` only: this page makes the video, and names the recording by the id it got. */
  | { cmd: "dev_record_start" }
  | { cmd: "dev_record_frame"; id: number }
  | { cmd: "dev_record_data"; id: number; data: string; mp4: boolean }
  | { cmd: "dev_record_end"; id: number; failed?: boolean }
  | { cmd: "dev_record_file"; reveal: boolean }
  | { cmd: "open_palette"; mode: PaletteMode; side?: Side; tab?: Id }
  /** The welcome page is done: the app reads the chosen browsers (`import_sources` keys) again, applies what was chosen for their spaces (`spaces`, per browser) and closes the page. */
  | { cmd: "finish_welcome"; import: string[]; spaces?: Record<string, SpaceChoice[]>; default_browser?: boolean }
  | { cmd: "palette_resize"; height: number }
  | { cmd: "palette_close" }
  | { cmd: "get_state" }
  | { cmd: "show_archive" }
  | { cmd: "downloads" }
  | { cmd: "open_download"; id: Id; reveal?: boolean }
  /** Put a finished download's file, or its path as text, on the clipboard. */
  | { cmd: "copy_download"; id: Id; what: "file" | "path" }
  | { cmd: "clear_downloads" }
  | { cmd: "show_downloads" }
  /** One of Chromium's own pages (saved passwords, addresses, cards) as a peek, in the logins of `profile`. */
  | { cmd: "open_engine_page"; page: "passwords" | "addresses" | "cards" | "clear_data" | "extensions" | "web_store"; profile?: Id }
  | { cmd: "open_extension_store"; id: string; profile: Id }
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
  | { cmd: "web_panel_width"; width: number | null; done?: boolean }
  | { cmd: "update_status" }
  | { cmd: "check_updates" }
  | { cmd: "install_update" }
  | { cmd: "open_update_page" }
  | { cmd: "set_tab_profile"; id: Id; profile: Id | null }
  | { cmd: "set_favorite_spaces"; id: Id; spaces: Id[] }
  | { cmd: "move_favorite"; id: Id; index: number }
  | { cmd: "add_favorite"; url: string }
  | { cmd: "pinned_to_bookmarks"; space: Id; remove?: boolean }
  | { cmd: "clear_bookmarks"; space: Id }
  | { cmd: "duplicate_favorite"; id: Id }
  | { cmd: "media"; id: Id; action: MediaAction }
  | { cmd: "set_muted"; id: Id; muted: boolean }
  | { cmd: "picture_in_picture"; id: Id }
  | { cmd: "import_sources" }
  /** Read browser `key` and say what an import would add; nothing is added. */
  | { cmd: "import_preview"; key: string }
  | { cmd: "import_from"; key: string }
  /** The text of a bookmarks file (the HTML browsers export): its bookmarks join the current space's. */
  | { cmd: "import_bookmarks"; html: string; space?: Id }
  /** The bookmarks of space `space`, or of every space, as the text of a file. */
  | { cmd: "export_bookmarks"; format: BookmarkFormat; space?: Id }
  | { cmd: "shortcut"; shortcut: string | { select_tab: number } | { select_space: number } }
  | { cmd: "set_shortcut"; action: ShortcutAction; keys: string | null }
  | { cmd: "set_space_shortcut"; id: Id; keys: string | null }
  | { cmd: "reset_shortcut"; action?: ShortcutAction }
  | { cmd: "record_shortcut"; on: boolean };

/** A browser on this computer that Magnetite can import from. */
export interface ImportSource {
  key: string;
  name: string;
}

/** What an import would add: its spaces in order, and how many favorites. */
export interface ImportPreview {
  spaces: { title: string; icon?: string | null; tabs: number; bookmarks: number }[];
  favorites: number;
}

/** What was chosen for the imported space at `index` of the preview, while it is still called `from`: a new name, an icon (a glyph name), or leaving it out. */
export interface SpaceChoice {
  index: number;
  from: string;
  to?: string;
  icon?: string;
  skip?: boolean;
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
  /** `error`: why these keys cannot be a shortcut; nothing changed. */
  set_shortcut: { error?: string };
  set_space_shortcut: { error?: string };
  search: { results: SearchResult[] };
  archive: { items: ArchiveItem[] };
  history: { items: Visit[] };
  make_default_browser: { default_browser: DefaultBrowser };
  choose_download_folder: { downloads: DownloadPrefs };
  dev_emulate: Emulation;
  dev_servers: { servers: DevServer[] };
  dev_record_start: { id: number };
  /** The recorded page's next picture (a JPEG, base64), or the end. */
  dev_record_frame: { frame?: string; done?: boolean };
  new_profile: { id: Id };
  export_bookmarks: { text: string; name: string; bookmarks: number };
  smart_save: { id: Id };
  smart_connect: Account;
  smart_preview: SmartItem[];
  /** `qr`: server and words as a QR code for a phone, one string per row, "1" a dark square. */
  sync_code: { words: string | null; qr: string[] | null };
  copy_link: CopyResult;
  read_shared: { folder: Folder | null };
  add_shared: { added: boolean };
  downloads: { items: DownloadEntry[] };
  permission_prompts: { items: PermissionPrompt[] };
  update_status: UpdateInfo;
  check_updates: UpdateInfo;
  import_sources: { sources: ImportSource[] };
  import_preview: ImportPreview;
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
  pinned_to_bookmarks: { bookmarks: number; pinned: number };
  clear_bookmarks: { bookmarks: number };
  all_permissions: { rows: PermissionEntry[] };
  /** `null` once device mode is off. */
  device_get: DeviceConfig | null;
  /** `site`: the registrable domain of the page asked about; null for no web page. */
  site_ads: { site: string | null; enabled: boolean; blocking: boolean };
  /** `site`: as for `site_ads`, and null also where the system has no engine to use. `known`: one of the services that always open there (Settings → Protected video). */
  site_engine: { site: string | null; system: boolean; known: boolean };
  site_data_scope: { domain: string; origins: string[] } | { domain?: undefined };
}

/** What command `K` resolves with: its entry in `Replies`, else `unknown`. */
type Reply<K extends Command["cmd"]> = K extends keyof Replies ? Replies[K] : unknown;

type QueryFn = (q: { request: string; persistent?: boolean; onSuccess: (r: string) => void; onFailure: (code: number, msg: string) => void }) => void;

declare global {
  interface Window {
    magnetiteQuery?: QueryFn;
    /** The device bar's page: the engine shows a size dragged with the handles. */
    __magnetiteDevice?: (c: DeviceConfig) => void;
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
      /** A recording is asked for: a page that stays makes it (`recorder.ts`). */
      record: () => void;
      /** The recording is in the Downloads folder as `name`. */
      recordingReady: (r: { name: string }) => void;
      /** macOS: a two-finger swipe began somewhere in the app. */
      swipeBegan: () => void;
      /** The window this page is in has this size now, in window points. */
      windowSize: (width: number, height: number) => void;
      /** The empty page: where the pages over it are now. */
      pageCards: (at: PageCards) => void;
    };
  }
}

/** The top bar's pop-ups, each a window below the bar (`TopBarPopup.tsx`). */
export type TopBarPopupKind = "site" | "copy" | "extension" | "extensions" | "emulate" | "timezone";

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
  /** An rpm or deb install: the system's package manager updates it. */
  | { state: "system" }
  | { state: "checking" }
  | { state: "up_to_date"; checked_at: number }
  | { state: "available"; version: string; notes_url: string; installable: boolean }
  | { state: "installing"; version: string }
  | { state: "failed"; error: string };

export interface UpdateInfo {
  status: UpdateStatus;
  /** An rpm or deb install, also once `status` has moved on from "system". */
  system: boolean;
  /** The running version. */
  version: string;
}

export type CaptureEvent = { kind: "ready"; info: CaptureInfo } | { kind: "failed"; error: string } | { kind: "busy"; what: string } | { kind: "recorded"; name: string };

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
const swipes = signal();
const records = signal();
const pageCards = cached<PageCards>();

/**
 * Keep the window's size where the styles read it (`--win-w`, `--win-h`):
 * a background across the whole window is cut to this page with it
 * (`windowBackground`).
 */
function windowSize(width: number, height: number) {
  const root = document.documentElement;
  root.style.setProperty("--win-w", `${width}px`);
  root.style.setProperty("--win-h", `${height}px`);
}

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
  record: records.emit,
  recordingReady: ({ name }) => captures.emit({ kind: "recorded", name }),
  swipeBegan: swipes.emit,
  windowSize,
  pageCards: pageCards.emit,
};
// Until the app's first word: a page's outer size is its window's.
windowSize(window.outerWidth, window.outerHeight);

/**
 * Where the pages lie over the empty page, in window points: the size of the
 * page area they were laid out in, and each page's (and docked DevTools')
 * `[x, y, width, height]` in it.
 */
export interface PageCards {
  width: number;
  height: number;
  cards: [x: number, y: number, width: number, height: number][];
}
/** The empty page: the pages over it, now and whenever they move. */
export const onPageCards = pageCards.on;
/** The UI state, now (once the app sent it) and on every push. */
export const subscribe = states.on;
/** A recording is asked for (the sidebar, the top bar and the developer bar listen). */
export const onRecord = records.on;
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
/** macOS: each two-finger swipe as it begins, anywhere in the app. */
export const onSwipeBegan = swipes.on;
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

/**
 * Whether favorite `tab` shows in space `space`: in every space, unless it
 * names the spaces it is for. Spaces that are gone do not count, so one left
 * with none shows everywhere (as `favorite_shows` in the app).
 */
export function favoriteShows(sidebar: Snapshot, tab: TabItem, space: Id): boolean {
  const named = (tab.only_in ?? []).filter((id) => sidebar.spaces.some((s) => s.id === id));
  return named.length === 0 || named.includes(space);
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

/** The list tab `id` stands in: the favorites of a profile, or the pinned or Today tabs of a space. */
function homeOf(state: UiState, id: Id): string | null {
  const list = state.sidebar.favorites.find((f) => f.items.some((t) => t.id === id));
  if (list) return "favorites:" + list.profile_id;
  for (const s of state.sidebar.spaces) {
    if (s.today.some((t) => t.id === id)) return "today:" + s.id;
    if (findTabInSpace(s, id)) return "pinned:" + s.id;
  }
  return null;
}

/**
 * The pages of `split` that stand in the same list as its page `id`, in
 * reading order: they are pinned, moved and added to the favorites as one
 * item (as `split_group` in the app).
 */
export function splitGroup(state: UiState, split: SplitNode, id: Id): Id[] {
  const home = homeOf(state, id);
  return splitIds(split).filter((other) => homeOf(state, other) === home);
}

/** The app runs on macOS (Finder, ⌘ shortcuts). */
export const IS_MAC = navigator.userAgent.includes("Mac");

/**
 * A shortcut as this system writes it. On macOS `mac` as it is ("⇧⌘T");
 * elsewhere `other` when the keys differ there, or else `mac` with its
 * modifier signs spelled out, ⌘ being Ctrl ("Ctrl+Shift+T").
 */
export function keyLabel(mac: string, other?: string): string {
  if (IS_MAC) return mac;
  if (other) return other;
  const [, mods = "", key = ""] = /^([⌘⌥⇧⌃]*)(.*)$/u.exec(mac) ?? [];
  const names: string[] = [];
  if (mods.includes("⌘") || mods.includes("⌃")) names.push("Ctrl");
  if (mods.includes("⌥")) names.push("Alt");
  if (mods.includes("⇧")) names.push("Shift");
  return [...names, key.trim()].filter(Boolean).join("+");
}
