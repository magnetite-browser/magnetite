// The shortcuts whose keys can be changed (Settings → Shortcuts): their
// names, how keys are written, and how a key press becomes saved keys.
// The app's side is `crates/engine/src/keymap.rs`.
import { IS_MAC } from "../bridge";
import { t } from "../i18n";

/** The shortcuts by group, each named by its texts (`settings.shortcuts.groups.<id>`, `settings.shortcuts.actions.<action>`). */
export const SHORTCUT_GROUPS = [
  { id: "tabs", actions: ["new_tab", "focus_address", "close_tab", "reopen_closed", "next_tab", "prev_tab", "toggle_pin", "new_split"] },
  { id: "windows", actions: ["new_window", "close_window", "new_space", "private_space", "close_private_space", "hide_private_space", "next_space", "prev_space", "space_picker", "toggle_sidebar", "web_panel"] },
  { id: "page", actions: ["site_panel", "copy_link", "capture", "find", "find_next", "find_previous", "print"] },
  { id: "magnetite", actions: ["history", "downloads", "settings"] },
  { id: "developer", actions: ["toggle_dev_tools", "dev_console", "dev_picker", "dev_network", "dev_reset"] },
] as const;

/** A shortcut whose keys can be changed, as the app names it. */
export type ShortcutAction = (typeof SHORTCUT_GROUPS)[number]["actions"][number];

/** Who gets a shortcut's keys first when a page is in focus: Magnetite (the page never sees them), or the page (the shortcut runs if the page leaves them alone). */
export type ShortcutFirst = "magnetite" | "page";

/** The keys each shortcut has now, as the app saves them (`primary+shift+T`); one without keys is missing. */
export type ShortcutKeys = Partial<Record<ShortcutAction, string>>;

/** What a shortcut does, in a few words. */
export const SHORTCUT_LABEL = Object.fromEntries(SHORTCUT_GROUPS.flatMap((g) => g.actions).map((action) => [action, t(`settings.shortcuts.actions.${action}`)])) as Record<ShortcutAction, string>;

/** Keys by their `KeyboardEvent.keyCode`, named as the app names them; letters, digits and F1 to F12 are computed. */
const NAMED_KEYS: Record<number, string> = {
  8: "Backspace", 9: "Tab", 13: "Enter", 32: "Space", 33: "PageUp", 34: "PageDown", 35: "End", 36: "Home",
  37: "Left", 38: "Up", 39: "Right", 40: "Down", 46: "Delete",
  186: ";", 187: "=", 188: ",", 189: "-", 190: ".", 191: "/", 192: "`", 219: "[", 220: "\\", 221: "]", 222: "'",
};

function keyName(code: number): string | null {
  if ((code >= 65 && code <= 90) || (code >= 48 && code <= 57)) return String.fromCharCode(code);
  if (code >= 112 && code <= 123) return `F${code - 111}`;
  return NAMED_KEYS[code] ?? null;
}

/**
 * The keys of a key press as the app saves them, or null while no key that
 * a shortcut can use is down (a modifier alone). `keyCode` is what the app
 * matches too, whatever the keyboard layout.
 */
export function pressedKeys(e: KeyboardEvent): string | null {
  const key = keyName(e.keyCode);
  if (!key) return null;
  // Outside macOS Ctrl is the shortcut modifier, and the only Control key.
  const mods = [
    (IS_MAC ? e.metaKey : e.ctrlKey) && "primary",
    IS_MAC && e.ctrlKey && "ctrl",
    e.altKey && "alt",
    e.shiftKey && "shift",
  ].filter((m) => m !== false);
  return [...mods, key].join("+");
}

const MAC_KEYS: Record<string, string> = {
  primary: "⌘", ctrl: "⌃", alt: "⌥", shift: "⇧",
  Left: "←", Right: "→", Up: "↑", Down: "↓", Enter: "↵", Backspace: "⌫", Delete: "⌦",
};
const OTHER_KEYS: Record<string, string> = { primary: t("settings.keys.ctrl"), ctrl: t("settings.keys.ctrl"), alt: t("settings.keys.alt"), shift: t("settings.keys.shift") };
/** The keys written as words, by the name the app saves them under; on a Mac some are signs (`MAC_KEYS`). */
const KEY_WORDS: Record<string, string> = {
  Space: t("settings.keys.space"), Tab: t("settings.keys.tab"), Enter: t("settings.keys.enter"), Backspace: t("settings.keys.backspace"), Delete: t("settings.keys.delete"),
  Left: t("settings.keys.left"), Right: t("settings.keys.right"), Up: t("settings.keys.up"), Down: t("settings.keys.down"),
  PageUp: t("settings.keys.page_up"), PageDown: t("settings.keys.page_down"), Home: t("settings.keys.home"), End: t("settings.keys.end"),
};
/** The order modifiers are written in on a Mac. */
const MAC_ORDER = ["ctrl", "alt", "shift", "primary"];

/** Saved keys as this system writes them, its key names in the page's language: `⇧⌘T` on a Mac, `Ctrl+Shift+T` elsewhere. Empty without keys. */
export function keysLabel(keys: string | null | undefined): string {
  if (!keys) return "";
  // The key comes last and may be any sign but "+".
  const cut = keys.lastIndexOf("+");
  const mods = cut < 0 ? [] : keys.slice(0, cut).split("+");
  const key = keys.slice(cut + 1);
  if (!IS_MAC) return [...mods.map((m) => OTHER_KEYS[m] ?? m), KEY_WORDS[key] ?? key].join("+");
  const sorted = MAC_ORDER.filter((m) => mods.includes(m));
  return sorted.map((m) => MAC_KEYS[m]).join("") + (MAC_KEYS[key] ?? KEY_WORDS[key] ?? key);
}

/** ` (⌘S)` for a tooltip, or nothing when the shortcut has no keys. */
export function keysHint(keys: ShortcutKeys | undefined, action: ShortcutAction): string {
  const label = keysLabel(keys?.[action]);
  return label ? ` (${label})` : "";
}
