// The shortcuts whose keys can be changed (Settings → Shortcuts): their
// names, how keys are written, and how a key press becomes saved keys.
// The app's side is `crates/engine/src/keymap.rs`.
import { IS_MAC } from "../bridge";

export const SHORTCUT_GROUPS = [
  {
    label: "Tabs",
    actions: {
      new_tab: "New tab, search, commands",
      focus_address: "Change the address",
      close_tab: "Close the tab (it moves to the archive)",
      reopen_closed: "Reopen the last closed tab",
      next_tab: "Next tab",
      prev_tab: "Previous tab",
      toggle_pin: "Pin or unpin the tab",
      new_split: "Split view",
    },
  },
  {
    label: "Windows and spaces",
    actions: {
      new_window: "New window",
      close_window: "Close the window",
      new_space: "New space",
      private_space: "Private space",
      close_private_space: "Close the private space",
      hide_private_space: "Hide the private space",
      next_space: "Next space",
      prev_space: "Previous space",
      space_picker: "Show all spaces, to go to one",
      toggle_sidebar: "Show or hide the sidebar",
      web_panel: "Open or close the web panel",
    },
  },
  {
    label: "The page",
    actions: {
      site_panel: "Site controls",
      copy_link: "Copy the link",
      capture: "Capture the page",
      find: "Find in the page",
      find_next: "Next match",
      find_previous: "Previous match",
      print: "Print the page",
    },
  },
  {
    label: "Magnetite",
    actions: {
      history: "History",
      downloads: "Downloads",
      settings: "Settings",
    },
  },
  {
    label: "Developer",
    actions: {
      toggle_dev_tools: "Developer tools",
      dev_console: "Console",
      dev_picker: "Pick an element",
      dev_network: "Network",
      dev_reset: "Reset the page (reload past the cache)",
    },
  },
] as const;

type Group = (typeof SHORTCUT_GROUPS)[number];
/** A shortcut whose keys can be changed, as the app names it. */
export type ShortcutAction = { [G in Group as G["label"]]: keyof G["actions"] }[Group["label"]];

/** Who gets a shortcut's keys first when a page is in focus: Magnetite (the page never sees them), or the page (the shortcut runs if the page leaves them alone). */
export type ShortcutFirst = "magnetite" | "page";

/** The keys each shortcut has now, as the app saves them (`primary+shift+T`); one without keys is missing. */
export type ShortcutKeys = Partial<Record<ShortcutAction, string>>;

/** What a shortcut does, in a few words. */
export const SHORTCUT_LABEL = Object.assign({}, ...SHORTCUT_GROUPS.map((g) => g.actions)) as Record<ShortcutAction, string>;

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
const OTHER_KEYS: Record<string, string> = { primary: "Ctrl", ctrl: "Ctrl", alt: "Alt", shift: "Shift" };
/** The order modifiers are written in on a Mac. */
const MAC_ORDER = ["ctrl", "alt", "shift", "primary"];

/** Saved keys as this system writes them: `⇧⌘T` on a Mac, `Ctrl+Shift+T` elsewhere. Empty without keys. */
export function keysLabel(keys: string | null | undefined): string {
  if (!keys) return "";
  // The key comes last and may be any sign but "+".
  const cut = keys.lastIndexOf("+");
  const mods = cut < 0 ? [] : keys.slice(0, cut).split("+");
  const key = keys.slice(cut + 1);
  if (!IS_MAC) return [...mods.map((m) => OTHER_KEYS[m] ?? m), key].join("+");
  const sorted = MAC_ORDER.filter((m) => mods.includes(m));
  return sorted.map((m) => MAC_KEYS[m]).join("") + (MAC_KEYS[key] ?? key);
}

/** ` (⌘S)` for a tooltip, or nothing when the shortcut has no keys. */
export function keysHint(keys: ShortcutKeys | undefined, action: ShortcutAction): string {
  const label = keysLabel(keys?.[action]);
  return label ? ` (${label})` : "";
}
