// What the Settings page works with.
import type { Color, DefaultBrowser, ExtensionInfo, Settings } from "../bridge";
import type { ShortcutAction } from "./shortcutKeys";

/** Everything the Settings page shows (`get_settings`). */
export interface Info {
  settings: Settings;
  sync_signed_in: boolean;
  /** `synced` false: stays on this device, because only spaces kept here use it. */
  profiles: { id: string; name: string; is_default: boolean; ephemeral?: boolean; synced: boolean }[];
  /** `synced` false: kept on this device alone ("Only on this device"). */
  /** `keys`: the keys that go to the space (Settings → Shortcuts), null without. `shortcut`: the keys saved for it, which may not count here (fixed on this system, or something else has them). */
  spaces: { id: string; title: string; icon?: string; icon_image?: string | null; accent?: Color | null; profile_id: string; bookmarks: number; bookmark_tree: boolean; pinned: number; synced: boolean; keys: string | null; shortcut: string | null }[];
  active_space: string;
  favorites: { id: string; title: string; url: string; profile_id?: string | null; only_in: string[] }[];
  archive_count: number;
  history_count: number;
  /** Whether Magnetite opens links from other apps ("unknown": not on this system). */
  default_browser: DefaultBrowser;
  version: string;
  /** Git commit of this build, when the build named it. */
  commit: string | null;
  /** Where the source lives (workspace `repository`). */
  repository: string;
  engine: { chromium: string; cef: string };
  data_dir: string;
  /** The chosen download folder is not there (any more). */
  download_folder_missing: boolean;
  /** Installed extensions of the page in front's profile. */
  extensions: ExtensionInfo[];
  /** The extensions of every lasting profile. */
  profile_extensions: { profile: string; extensions: ExtensionInfo[]; pinned: string[] }[];
  /** The filter lists one can subscribe to, and whether downloads are under way. */
  adblock: {
    lists: { id: string; name: string; about: string; host: string; fetched_at: number | null; error: string | null }[];
    working: boolean;
    /** A first start before its first website: chosen lists are downloaded once one is opened. */
    held: boolean;
    /** The ids of the recommended lists, which a new installation starts with. */
    recommended: string[];
  };
  /** Whether this system has a web engine of its own that Magnetite can show pages in, and the services that open in it. */
  system_engine: { available: boolean; known_sites: string[]; name: string; about: string };
  /** Every shortcut whose keys can be changed, with the keys it has and those it began with (null: none). */
  shortcuts: { action: ShortcutAction; keys: string | null; default: string | null }[];
  /** Where Chromium keeps its encryption key: macOS Keychain, or its fixed test key (`--mock-keychain`). */
  keychain: "keychain" | "test";
}

/** Changed settings; the page saves them merged into the current ones. */
export type SettingsPatch = Partial<Settings>;
