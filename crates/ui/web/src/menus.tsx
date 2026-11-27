// What the sidebar's context menus hold and in what order. Every menu is
// built from the same groups, top to bottom (`GROUPS`), with a line between
// two that have rows: a new action gets a group, not a place in a list.
import { SEPARATOR, type MenuItem, type MenuLeaf } from "./ContextMenu";
import { SpaceIcon } from "./Glyphs";
import type { RowContext, Section } from "./Rows";
import { accentCss, IS_MAC, keyLabel, send, type Snapshot, type Space, type TabItem } from "./bridge";
import { t } from "./i18n";
import { keysLabel } from "./settings/shortcutKeys";

/** Most pages one split holds (as `magnetite_core::split::MAX_PANES`). */
export const MAX_PANES = 4;

/**
 * The groups of a menu, in the order they stand:
 * - `page`: what acts on the page or its source (reload, share, copy link);
 * - `place`: where the item is kept (pinned, favorites, bookmarks, its space);
 * - `view`: how it shows (split view, web panel, kept active, folded);
 * - `edit`: the item itself (name, icon, a copy, something new inside it);
 * - `belongs`: what it belongs to (its profile, the spaces it shows in);
 * - `remove`: what takes it away (archive, close, delete), always last.
 */
const GROUPS = ["page", "place", "view", "edit", "belongs", "remove"] as const;
type Group = (typeof GROUPS)[number];

/** A row, or nothing where a condition left it out. */
type Entry = MenuItem | false | null | undefined;

/** The menu of `groups`: each group's rows in the order of `GROUPS`, a line between two groups that have rows. */
export function menuOf(groups: Partial<Record<Group, Entry[]>>): MenuItem[] {
  return GROUPS.map((g) => (groups[g] ?? []).filter((it): it is MenuItem => !!it))
    .filter((rows) => rows.length > 0)
    .flatMap((rows, i) => (i === 0 ? rows : [SEPARATOR, ...rows]));
}

/**
 * The row that names a profile and opens the profiles to choose from, each
 * with its colour, and "New profile…" after them. `fresh`: what the new
 * profile is for, as that row's full name.
 */
export function profileRow(profiles: Snapshot["profiles"], current: string | undefined, choose: (profile: Snapshot["profiles"][number]) => void, fresh: { title: string; run: () => void }): MenuItem {
  return {
    label: t("sidebar.tab_menu.profile"),
    hint: profiles.find((p) => p.id === current)?.name,
    items: [
      ...profiles.map((p): MenuLeaf => ({
        label: p.name,
        dot: p.color ? accentCss(p.color) : "",
        selected: p.id === current,
        onClick: () => p.id !== current && choose(p),
      })),
      SEPARATOR,
      { label: t("sidebar.profile.new"), title: fresh.title, onClick: fresh.run },
    ],
  };
}

/**
 * Right-click menu of a tab; `at` is the click point on the screen (the share
 * menu opens there). `smart`: an item its smart folder got from its source.
 * A row that belongs to this kind of tab but has nothing to do right now is
 * greyed out and says why; one that does not fit the tab at all is left out.
 * The tab in front shows the keys that do the same.
 */
export function tabMenu(ctx: RowContext, tab: TabItem, section: Section, at: { x: number; y: number }, smart = false): MenuItem[] {
  const { state } = ctx;
  const { spaces, profiles, favorites } = state.sidebar;
  const onScreen = state.panes.includes(tab.id);
  const inSplit = ctx.inSplit.has(tab.id);
  const loaded = !!state.runtime[tab.id]?.loaded;
  // The private space has only Today tabs, and nothing of it is kept or moves out.
  const inPrivate = state.private_space === ctx.spaceId && section !== "favorites";
  const favorite = section === "favorites";
  // The shortcuts act on the tab in front: only its menu names them.
  const inFront = state.active_tab === tab.id;
  const keys = (label: string) => (inFront && label ? label : undefined);
  const away = !!tab.home_url && tab.home_url !== tab.url;

  // One other space is a row of its own; more are a list (see `ContextMenu`).
  // Where the spaces use more than one profile, each names its own: two
  // spaces may share a name, and a tab moved into another profile loads
  // again with that profile's logins.
  const movable = inPrivate ? [] : spaces.filter((sp) => sp.id !== state.private_space);
  const targets = movable.filter((sp) => sp.id !== ctx.spaceId);
  const showProfile = new Set(movable.map((sp) => sp.profile_id)).size > 1;
  const moveTo = (sp: Space) => ({
    tag: showProfile ? (profiles.find((p) => p.id === sp.profile_id)?.name ?? t("sidebar.profile.fallback")) : undefined,
    onClick: () => send({ cmd: "move_tab", id: tab.id, space: sp.id, section: "today", index: 0 }),
  });
  const [first, ...others] = targets;
  const move: MenuItem | undefined = !first
    ? undefined
    : others.length === 0
      ? { label: t("sidebar.tab_menu.move_to", { space: first.title }), ...moveTo(first) }
      : {
          label: t("sidebar.tab_menu.move_to_space"),
          items: targets.map((sp) => ({ label: sp.title, icon: <SpaceIcon space={sp} size={16} />, ...moveTo(sp) })),
          // Typing a part of the name finds a space among many.
          filter: { placeholder: t("sidebar.tab_menu.find_space"), none: t("sidebar.tab_menu.no_space") },
        };

  // A favorite runs in the profile of its list; choosing another moves it.
  // Each profile has its own cookies and logins: two favorites of the same
  // site in two profiles are two accounts side by side.
  const current = favorites.find((f) => f.items.some((x) => x.id === tab.id))?.profile_id;
  // For every space, or for this one alone.
  const here = spaces.find((sp) => sp.id === ctx.spaceId);
  const own = (tab.only_in ?? []).filter((id) => spaces.some((sp) => sp.id === id));
  const several = spaces.filter((sp) => sp.id !== state.private_space).length > 1;

  // Closing a favorite or a pinned tab unloads its page and takes it back to
  // its pinned page: without a page, only a favorite away from it has
  // something to close (a pinned tab has "Back to pinned page" above).
  const closes = section === "today" || loaded || (favorite && away);
  const full = state.panes.length >= MAX_PANES;

  return menuOf({
    page: [
      { label: t("sidebar.tab_menu.reload"), hint: keys(keyLabel("⌘R")), disabled: !loaded, title: loaded ? undefined : t("sidebar.tab_menu.why.no_page"), onClick: () => send({ cmd: "reload", id: tab.id }) },
      // The system's share menu: only macOS has one here.
      IS_MAC && { label: t("sidebar.tab_menu.share"), onClick: () => send({ cmd: "share", id: tab.id, x: at.x, y: at.y }) },
      { label: t("sidebar.tab_menu.copy_link"), onClick: () => navigator.clipboard?.writeText(tab.url) },
    ],
    place: [
      section === "today" && !inPrivate && { label: t("sidebar.tab_menu.pin"), hint: keys(keysLabel(state.keys.toggle_pin)), onClick: () => send({ cmd: "pin", id: tab.id }) },
      section === "pinned" && { label: t("sidebar.tab_menu.unpin"), hint: keys(keysLabel(state.keys.toggle_pin)), onClick: () => send({ cmd: "unpin", id: tab.id }) },
      section === "pinned" && { label: t("sidebar.tab_menu.back_to_pinned"), disabled: !away, title: away ? tab.home_url : t("sidebar.tab_menu.why.at_pinned_page"), onClick: () => send({ cmd: "reset_tab", id: tab.id }) },
      !inPrivate && !favorite && { label: t("sidebar.tab_menu.add_to_favorites"), onClick: () => send({ cmd: "move_tab", id: tab.id, space: ctx.spaceId, section: "favorites", index: 999 }) },
      favorite && { label: t("sidebar.tab_menu.remove_from_favorites"), onClick: () => send({ cmd: "move_tab", id: tab.id, space: ctx.spaceId, section: "today", index: 0 }) },
      !inPrivate && { label: t("sidebar.tab_menu.add_to_bookmarks"), onClick: () => send({ cmd: "add_bookmark", id: tab.id }) },
      move,
    ],
    view: [
      // Beside the page in front: not for that page itself, one on screen already, or a web panel.
      !!state.active_tab && !inFront && !onScreen && !tab.panel && { label: t("sidebar.tab_menu.open_in_split"), disabled: full, title: full ? t("sidebar.tab_menu.why.split_full") : undefined, onClick: () => send({ cmd: "add_split", id: tab.id }) },
      inSplit && { label: t("sidebar.tab_menu.remove_from_split"), onClick: () => send({ cmd: "remove_split", id: tab.id }) },
      inSplit && { label: t("sidebar.tab_menu.separate_all"), onClick: () => send({ cmd: "separate_all", id: tab.id }) },
      // Over the page in front instead of in its place (a translator, a password manager).
      section !== "today" && { label: t("sidebar.tab_menu.open_as_panel"), checked: !!tab.panel, onClick: () => send({ cmd: "set_web_panel", id: tab.id, panel: !tab.panel }) },
      // Sites that have a page for phones serve the panel that one.
      section !== "today" && { label: t("sidebar.tab_menu.mobile_view"), checked: !!tab.panel_mobile, disabled: !tab.panel, title: tab.panel ? undefined : t("sidebar.tab_menu.why.no_panel"), onClick: () => send({ cmd: "set_panel_mobile", id: tab.id, mobile: !tab.panel_mobile }) },
      // Spared from unloading and auto-archive until it is let go again.
      // Favorites and pinned tabs stay loaded anyway.
      section === "today" && { label: t("sidebar.tab_menu.keep_active"), checked: !!tab.keep_active, onClick: () => send({ cmd: "set_keep_active", id: tab.id, keep: !tab.keep_active }) },
    ],
    edit: [
      { label: t("common.rename"), onClick: () => ctx.setRenaming(tab.id) },
      !!tab.custom_title && { label: t("sidebar.tab_menu.use_page_title"), onClick: () => send({ cmd: "rename_tab", id: tab.id, title: "" }) },
      // The same address as a new Today tab (a private one in the private space),
      // as "Duplicate tab" in the command bar. A favorite's copy stays a favorite, in its profile.
      favorite
        ? { label: t("sidebar.tab_menu.duplicate_favorite"), onClick: () => send({ cmd: "duplicate_favorite", id: tab.id }) }
        : { label: t("sidebar.tab_menu.duplicate"), onClick: () => send({ cmd: "new_tab", url: tab.url }) },
    ],
    belongs: favorite
      ? [
          profileRow(profiles, current, (p) => send({ cmd: "set_tab_profile", id: tab.id, profile: p.is_default ? null : p.id }), { title: t("sidebar.tab_menu.new_profile"), run: () => ctx.newProfileFor(tab.id) }),
          here && several && {
            label: t("sidebar.tab_menu.shows_in"),
            hint: own.length === 0 ? t("sidebar.tab_menu.every_space") : spaces.filter((sp) => own.includes(sp.id)).map((sp) => sp.title).join(", "),
            items: [
              { label: t("sidebar.tab_menu.show_in_every_space"), selected: own.length === 0, onClick: () => own.length > 0 && send({ cmd: "set_favorite_spaces", id: tab.id, spaces: [] }) },
              { label: t("sidebar.tab_menu.show_only_in", { space: here.title }), selected: own.length === 1 && own[0] === here.id, onClick: () => send({ cmd: "set_favorite_spaces", id: tab.id, spaces: [here.id] }) },
            ],
          },
        ]
      : [],
    remove: [
      section === "today"
        ? { label: inPrivate ? t("common.close") : t("sidebar.tab_menu.archive"), hint: keys(keysLabel(state.keys.close_tab)), danger: true, onClick: () => send({ cmd: "close_tab", id: tab.id }) }
        : { label: t("common.close"), hint: keys(keysLabel(state.keys.close_tab)), disabled: !closes, title: closes ? undefined : t("sidebar.tab_menu.why.no_page"), onClick: () => send({ cmd: "close_tab", id: tab.id }) },
      // Not a smart item: its source would bring it back at the next refresh.
      section === "pinned" && !smart && { label: t("sidebar.tab_menu.remove_from_pinned"), danger: true, onClick: () => send({ cmd: "remove_pinned", id: tab.id }) },
    ],
  });
}
