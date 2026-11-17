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
  /** `keys`: the keys that go to the space (Settings → Shortcuts), null without. */
  spaces: { id: string; title: string; icon?: string; accent?: Color | null; profile_id: string; synced: boolean; keys: string | null }[];
  active_space: string;
  favorites: { id: string; title: string; url: string; profile_id?: string | null }[];
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
  /** Installed extensions of the page in front's profile. */
  extensions: ExtensionInfo[];
  /** The filter lists one can subscribe to, and whether downloads are under way. */
  adblock: {
    lists: { id: string; name: string; about: string; host: string; fetched_at: number | null; error: string | null }[];
    working: boolean;
  };
  /** Every shortcut whose keys can be changed, with the keys it has and those it began with (null: none). */
  shortcuts: { action: ShortcutAction; keys: string | null; default: string | null }[];
  /** Where Chromium keeps its encryption key: macOS Keychain, or its fixed test key (`--mock-keychain`). */
  keychain: "keychain" | "test";
}

/** Changed settings; the page saves them merged into the current ones. */
export type SettingsPatch = Partial<Settings>;
