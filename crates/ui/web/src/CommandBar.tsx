import { useEffect, useRef, useState } from "react";
import { TIMEZONES } from "./DevBar";
import { Favicon } from "./Favicon";
import { Arrow, Command as CommandIcon } from "./Icons";
import { IS_MAC, findTab, findTabInSpace, hostOf, send, type Command, type DevServer, type PaletteMode, type SearchResult, type Side, type SiteSearch, type UiState } from "./bridge";
import { useDismiss } from "./dismiss";
import { rich, t } from "./i18n";
import type { English } from "./i18n/english.gen";

/**
 * A row of the palette. `keepOpen`: the pick ends the palette itself, because
 * another opens in its place or it has more to send before the page goes.
 * `peek`: what ⇧Enter opens as a peek instead.
 */
type Item =
  | (SearchResult & { run: () => void; peek?: string })
  | { kind: "action"; id: string; title: string; run: () => void; keepOpen?: boolean }
  | { kind: "url"; title: string; url: string; run: () => void; peek: string };

type Action = { id: string; title: string; keywords: string[]; cmd: Command };

/** A command's keywords from its text ("space, switch, go"): typing the start of one finds the command. */
const keywordsOf = (text: string) => text.split(",").map((k) => k.trim().toLowerCase()).filter(Boolean);

/** A command of `ACTIONS`, its title and keywords as commandBar.json has them under `commands.<id>`. */
const command = (id: keyof English["commandBar"]["commands"], cmd: Command): Action => ({
  id,
  title: t(`commandBar.commands.${id}.title`),
  keywords: keywordsOf(t(`commandBar.commands.${id}.keywords`)),
  cmd,
});

/** A tab command's keywords, as commandBar.json has them under `tab_commands.<id>`. */
const tabKeywords = (id: keyof English["commandBar"]["tab_commands"]) => keywordsOf(t(`commandBar.tab_commands.${id}.keywords`));

const ACTIONS: Action[] = [
  command("new_space", { cmd: "new_space", title: "" }),
  command("spaces", { cmd: "open_palette", mode: "spaces" }),
  command("private_space", { cmd: "open_private_space" }),
  command("new_folder", { cmd: "new_folder", title: "" }),
  command("archive_today", { cmd: "archive_today" }),
  command("reopen", { cmd: "reopen_closed" }),
  command("downloads", { cmd: "show_downloads" }),
  command("bookmarks", { cmd: "show_bookmarks" }),
  command("bookmark_page", { cmd: "add_bookmark" }),
  command("toggle_sidebar", { cmd: "toggle_sidebar" }),
  command("web_panel", { cmd: "shortcut", shortcut: "web_panel" }),
  command("toggle_top_bar", { cmd: "toggle_top_bar" }),
  command("import", { cmd: "new_tab", url: "magnetite://ui/settings#data" }),
  command("welcome", { cmd: "new_tab", url: "magnetite://ui/welcome" }),
  command("copy_link", { cmd: "copy_link", variant: "clean", notify: true }),
  command("copy_original", { cmd: "copy_link", variant: "original", notify: true }),
  command("copy_markdown", { cmd: "copy_link", variant: "markdown", notify: true }),
  command("zoom_reset", { cmd: "set_zoom", percent: 0 }),
  command("split_right", { cmd: "open_palette", mode: "split", side: "right" }),
  command("split_left", { cmd: "open_palette", mode: "split", side: "left" }),
  command("split_top", { cmd: "open_palette", mode: "split", side: "top" }),
  command("split_bottom", { cmd: "open_palette", mode: "split", side: "bottom" }),
  command("split_even", { cmd: "split_equalize", path: [] }),
  command("split_grid", { cmd: "split_grid" }),
  command("separate", { cmd: "separate_all" }),
  command("devtools", { cmd: "toggle_devtools" }),
  command("dev_console", { cmd: "dev_panel", panel: "console" }),
  command("dev_picker", { cmd: "dev_panel", panel: "picker" }),
  command("dev_network", { cmd: "dev_panel", panel: "network" }),
  command("dev_application", { cmd: "dev_panel", panel: "application" }),
  command("dev_reset", { cmd: "dev_reset" }),
  command("emu_dark", { cmd: "dev_emulate", patch: { color_scheme: "dark" } }),
  command("emu_light", { cmd: "dev_emulate", patch: { color_scheme: "light" } }),
  command("emu_motion", { cmd: "dev_emulate", patch: { reduced_motion: true } }),
  command("emu_print", { cmd: "dev_emulate", patch: { print: true } }),
  command("emu_offline", { cmd: "dev_emulate", patch: { network: "offline" } }),
  command("emu_slow", { cmd: "dev_emulate", patch: { network: "slow" } }),
  command("emu_cpu", { cmd: "dev_emulate", patch: { slow_cpu: true } }),
  command("emu_cache", { cmd: "dev_emulate", patch: { no_cache: true } }),
  command("dev_responsive", { cmd: "dev_responsive" }),
  command("dev_capture_set", { cmd: "dev_capture_set" }),
  command("dev_record", { cmd: "dev_record" }),
  ...TIMEZONES.map(([zone, place]): Action => ({ id: `emu_zone_${zone}`, title: t("commandBar.time_zone.title", { place }), keywords: [...keywordsOf(t("commandBar.time_zone.keywords")), zone.toLowerCase()], cmd: { cmd: "dev_emulate", patch: { timezone: zone } } })),
  command("emu_zone_off", { cmd: "dev_emulate", patch: { timezone: null } }),
  command("emu_off", { cmd: "dev_emulate", patch: { color_scheme: null, reduced_motion: false, print: false, network: "online", slow_cpu: false, no_cache: false, timezone: null } }),
  command("capture", { cmd: "capture_start" }),
  command("capture_copy", { cmd: "capture_copy", notify: true }),
  command("find", { cmd: "shortcut", shortcut: "find" }),
  command("print", { cmd: "shortcut", shortcut: "print" }),
  command("history", { cmd: "show_history" }),
];

/**
 * Commands for the active tab and the spaces, as the tab's menu in the
 * sidebar offers them (`tabMenu` in Rows.tsx): what fits the tab now.
 */
function tabActions(state: UiState): Action[] {
  const actions: Action[] = [];
  const spaces = state.sidebar.spaces;
  for (const sp of spaces) {
    if (sp.id !== state.active_space) actions.push({ id: `switch_${sp.id}`, title: t("commandBar.tab_commands.switch_space.title", { space: sp.title }), keywords: tabKeywords("switch_space"), cmd: { cmd: "switch_space", id: sp.id } });
  }
  if (state.private_space) {
    actions.push({ id: "close_private_space", title: t("commandBar.tab_commands.close_private_space.title"), keywords: tabKeywords("close_private_space"), cmd: { cmd: "close_private_space" } });
    actions.push({ id: "hide_private_space", title: t("commandBar.tab_commands.hide_private_space.title"), keywords: tabKeywords("hide_private_space"), cmd: { cmd: "hide_private_space" } });
  }
  const id = state.active_tab;
  const tab = id ? findTab(state, id) : null;
  if (!id || !tab) return actions;
  const owner = spaces.find((sp) => findTabInSpace(sp, id));
  // The same page beside this one with another profile's logins: two users
  // at once. Every profile but the page's own (`profile_for_tab` in the app).
  if (state.active_space !== state.private_space) {
    const { profiles, favorites } = state.sidebar;
    const home = owner ?? favorites.find((f) => f.items.some((item) => item.id === id));
    const own = (profiles.find((p) => p.id === tab.profile_id) ?? profiles.find((p) => p.id === home?.profile_id))?.id;
    for (const p of profiles) {
      if (p.id !== own && !p.ephemeral) actions.push({ id: `split_as_${p.id}`, title: t("commandBar.tab_commands.split_as.title", { profile: p.name }), keywords: tabKeywords("split_as"), cmd: { cmd: "dev_split_as", profile: p.id } });
    }
  }
  const duplicate = t("commandBar.tab_commands.duplicate.title");
  // A tab of the private space stays what and where it is.
  if (owner && owner.id === state.private_space) {
    actions.push({ id: "duplicate", title: duplicate, keywords: tabKeywords("duplicate"), cmd: { cmd: "new_tab", url: tab.url } });
    return actions;
  }
  const favorite = !owner;
  const today = owner?.today.some((item) => item.id === id) ?? false;
  if (owner && today) actions.push({ id: "pin", title: t("commandBar.tab_commands.pin.title"), keywords: tabKeywords("pin"), cmd: { cmd: "pin", id } });
  if (owner && !today) actions.push({ id: "unpin", title: t("commandBar.tab_commands.unpin.title"), keywords: tabKeywords("unpin"), cmd: { cmd: "unpin", id } });
  if (!today && tab.home_url && tab.home_url !== tab.url) actions.push({ id: "reset", title: t("commandBar.tab_commands.reset.title", { host: hostOf(tab.home_url) }), keywords: tabKeywords("reset"), cmd: { cmd: "reset_tab", id } });
  if (favorite) actions.push({ id: "unfavorite", title: t("commandBar.tab_commands.unfavorite.title"), keywords: tabKeywords("unfavorite"), cmd: { cmd: "move_tab", id, space: state.active_space, section: "today", index: 0 } });
  else actions.push({ id: "favorite", title: t("commandBar.tab_commands.favorite.title"), keywords: tabKeywords("favorite"), cmd: { cmd: "move_tab", id, space: state.active_space, section: "favorites", index: 999 } });
  if (today) actions.push({ id: "keep_active", title: tab.keep_active ? t("commandBar.tab_commands.keep_active.stop") : t("commandBar.tab_commands.keep_active.title"), keywords: tabKeywords("keep_active"), cmd: { cmd: "set_keep_active", id, keep: !tab.keep_active } });
  if (owner) {
    for (const sp of spaces) {
      if (sp.id !== owner.id && sp.id !== state.private_space) actions.push({ id: `move_${sp.id}`, title: t("commandBar.tab_commands.move.title", { space: sp.title }), keywords: tabKeywords("move"), cmd: { cmd: "move_tab", id, space: sp.id, section: "today", index: 0 } });
    }
  }
  // A favorite's copy keeps its profile; a tab's opens as a new Today tab of this space.
  actions.push({ id: "duplicate", title: duplicate, keywords: tabKeywords("duplicate"), cmd: favorite ? { cmd: "duplicate_favorite", id } : { cmd: "new_tab", url: tab.url } });
  const space = spaces.find((sp) => sp.id === state.active_space);
  if (space?.today.some((item) => item.id !== id)) actions.push({ id: "close_others", title: t("commandBar.tab_commands.close_others.title"), keywords: tabKeywords("close_others"), cmd: { cmd: "archive_today", keep: id } });
  return actions;
}

/** The split palette's prompt: where the pick opens, beside the current tab. */
function splitPlaceholder(side: Side | undefined): string {
  if (side === "left") return t("commandBar.input.split_left");
  if (side === "top") return t("commandBar.input.split_top");
  if (side === "bottom") return t("commandBar.input.split_bottom");
  return t("commandBar.input.split_beside");
}

/** The keys of the hint line, as this platform writes them. */
const KEYS = IS_MAC
  ? { enter: "↵", peek: "⇧↵", newTab: "⌘↵", tab: "⇥", back: "⌫" }
  : { enter: "Enter", peek: "Shift+Enter", newTab: "Ctrl+Enter", tab: "Tab", back: "Backspace" };

/**
 * The site search `input` starts with: its first word is a keyword (any
 * case) and a space follows (`site_search` in crates/app/src/app/addresses.rs).
 */
function siteSearchOf(input: string, sites: SiteSearch[]): { site: SiteSearch; query: string } | null {
  const m = /^(\S+)\s(.*)$/s.exec(input.trimStart());
  const site = m && sites.find((s) => s.keyword === m[1]!.toLowerCase());
  return site ? { site, query: m[2]! } : null;
}

/**
 * Whether the app opens `input` (trimmed) as an address instead of searching
 * for it: the cases of `normalise_url_with` in crates/app/src/app/addresses.rs.
 */
function opensAsAddress(input: string): boolean {
  if (input.includes("://") || input.startsWith("about:")) return true;
  if (input.includes(" ")) return false;
  return isLoopback(input) || (input.includes(".") && !input.endsWith("."));
}

/** Whether `host[:port][/path]` names this computer (`is_loopback` in the app). */
function isLoopback(input: string): boolean {
  const authority = input.split(/[/?#]/)[0] ?? "";
  const colon = authority.lastIndexOf(":");
  const host = colon >= 0 && /^\d+$/.test(authority.slice(colon + 1)) ? authority.slice(0, colon) : authority;
  const octets = host.split(".");
  const loopbackAddress = octets.length === 4 && octets[0] === "127" && octets.every((o) => /^(0|[1-9]\d?\d?)$/.test(o) && Number(o) <= 255);
  return host === "localhost" || host.endsWith(".localhost") || host === "[::1]" || loopbackAddress;
}

/**
 * Search, open and name things: the whole page of the centred palette window
 * (`CommandPalette`). `split` opens the pick on `splitSide` of the current
 * tab; `new_profile` assigns the new profile to `profileFor` (a favorite) or
 * else the active space; `quick` (the quick search from anywhere) opens an
 * address, search, bookmark or visit in a mini window and lists nothing
 * else. A site search keyword followed by a space or Tab turns into a chip:
 * what is typed then searches that site.
 */
export function CommandBar({ mode, state, profileFor, splitSide, onClose, onOpenArchive, onOpenSettings }: { mode: PaletteMode; state: UiState; profileFor?: string | null; splitSide?: Side; onClose: () => void; onOpenArchive: () => void; onOpenSettings: () => void }) {
  const activeTab = state.active_tab ? findTab(state, state.active_tab) : null;
  // `navigate` starts with the current address, selected so typing replaces it.
  const [query, setQuery] = useState(() => (mode === "navigate" && activeTab ? activeTab.url : ""));
  // The site search the chip shows; `query` is then what to search there.
  const [site, setSite] = useState<SiteSearch | null>(null);
  const [results, setResults] = useState<SearchResult[]>([]);
  const [selected, setSelected] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const naming = mode === "new_space" || mode === "new_profile";
  // The site search the hint line names while nothing is typed: another one each time the bar opens.
  const [example] = useState(() => state.settings.site_searches[Math.floor(Math.random() * state.settings.site_searches.length)]);
  const quick = mode === "quick";
  // The programs listening on this computer's ports (Settings → Developer tools), asked once per opening.
  const [servers, setServers] = useState<DevServer[]>([]);
  const wantsServers = state.settings.developer.enabled && !quick && !naming;
  useEffect(() => {
    if (wantsServers) void send({ cmd: "dev_servers" }).then((r) => setServers(r.servers));
  }, [wantsServers]);
  const sites = naming ? [] : state.settings.site_searches;
  // The palette window is focused already; it closes on Esc or when it loses focus.
  useDismiss(onClose, { takeFocus: false });

  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);

  // Debounced backend search; a site search looks only at that site.
  useEffect(() => {
    const q = query.trim();
    if (naming || site || !q) {
      setResults([]);
      return;
    }
    let cancelled = false;
    const timer = setTimeout(() => {
      send({ cmd: "search", query: q }).then((r) => {
        if (!cancelled) setResults(r.results ?? []);
      });
    }, 120);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [query, naming, site]);

  const q = query.trim();
  const items: Item[] = [];
  const groups: { label: string; items: Item[] }[] = [];
  /** Open `url` (typed input, or an address) as this palette's mode asks. */
  const open = (url: string) =>
    quick ? send({ cmd: "open_mini", url }) : mode === "navigate" ? send({ cmd: "navigate", url }) : send({ cmd: "new_tab", url, split: mode === "split", side: splitSide });
  if (mode === "new_space") {
    items.push({ kind: "action", id: "create_space", title: q ? t("commandBar.results.create_space", { name: q }) : t("commandBar.results.create_space_unnamed"), run: () => send({ cmd: "new_space", title: q }) });
    groups.push({ label: "", items });
  } else if (mode === "new_profile") {
    const title = !q ? t("commandBar.results.name_profile") : profileFor ? t("commandBar.results.create_profile_favorite", { name: q }) : t("commandBar.results.create_profile_space", { name: q });
    items.push({ kind: "action", id: "create_profile", title, keepOpen: true, run: async () => {
      // Closing ends this page, so it comes last: once the new profile's use is on its way.
      try {
        if (!q) return;
        const r = await send({ cmd: "new_profile", name: q });
        if (!r.id) return;
        if (profileFor) send({ cmd: "set_tab_profile", id: profileFor, profile: r.id });
        else send({ cmd: "set_space_profile", id: state.active_space, profile: r.id });
      } finally {
        onClose();
      }
    } });
    groups.push({ label: "", items });
  } else if (site) {
    // The app turns "keyword query" into the site's search address.
    const typed = `${site.keyword} ${q}`;
    if (q) groups.push({ label: "", items: [{ kind: "url", title: t("commandBar.site_search.result", { site: site.name, query: q }), url: typed, peek: typed, run: () => open(typed) }] });
    for (const g of groups) items.push(...g.items);
  } else {
    const top: Item[] = [];
    if (q) {
      top.push({ kind: "url", title: opensAsAddress(q) ? t("commandBar.results.open_address", { address: q }) : t("commandBar.results.search_for", { query: q }), url: q, peek: q, run: () => open(q) });
    }
    const actions: Item[] = [];
    // The quick search opens pages only: commands are for the main window.
    if (q && !quick) {
      const lower = q.toLowerCase();
      for (const a of [...ACTIONS, ...tabActions(state)]) {
        // "Add … split…" opens the next palette in this one's place: closing after it would close that one.
        if (a.title.toLowerCase().includes(lower) || a.keywords.some((k) => k.startsWith(lower))) actions.push({ kind: "action", id: a.id, title: a.title, keepOpen: a.cmd.cmd === "open_palette", run: () => send(a.cmd) });
      }
      const typed = (k: string) => k.startsWith(lower);
      if (keywordsOf(t("commandBar.commands.open_archive.keywords")).some(typed)) actions.push({ kind: "action", id: "open_archive", title: t("commandBar.commands.open_archive.title"), run: onOpenArchive });
      if (keywordsOf(t("commandBar.commands.open_settings.keywords")).some(typed)) actions.push({ kind: "action", id: "open_settings", title: t("commandBar.commands.open_settings.title"), run: onOpenSettings });
    }
    const tabs: Item[] = results.filter((r) => r.kind === "tab" && !quick).map((r) => ({ ...r, run: () => send(mode === "split" ? { cmd: "add_split", id: r.id, side: splitSide } : { cmd: "activate_tab", id: r.id }) }));
    const saved: Item[] = results.filter((r) => r.kind === "bookmark").map((r) => ({ ...r, peek: r.url, run: () => (quick ? open(r.url) : send({ cmd: "open_bookmark", id: r.id })) }));
    const visited: Item[] = results.filter((r) => r.kind === "history").map((r) => ({ ...r, peek: r.url, run: () => open(r.url) }));
    const archived: Item[] = results.filter((r) => r.kind === "archived" && !quick).map((r) => ({ ...r, run: () => send({ cmd: "restore_archived", id: r.id }) }));
    // What listens on this computer's ports: by project, port, or "local".
    const lower = q.toLowerCase();
    const local: Item[] = !q ? [] : servers
      .filter((s) => s.project?.toLowerCase().includes(lower) || String(s.port).startsWith(lower) || keywordsOf(t("commandBar.local_servers_keywords")).some((w) => w.startsWith(lower)))
      .map((s) => {
        const url = `http://localhost:${s.port}/`;
        return { kind: "url", title: `${s.project ?? s.command} · localhost:${s.port}`, url, peek: url, run: () => open(url) };
      });
    if (top.length) groups.push({ label: "", items: top });
    if (tabs.length) groups.push({ label: t("commandBar.groups.tabs"), items: tabs });
    if (actions.length) groups.push({ label: t("commandBar.groups.commands"), items: actions });
    if (saved.length) groups.push({ label: t("commandBar.groups.bookmarks"), items: saved });
    if (visited.length) groups.push({ label: t("commandBar.groups.history"), items: visited });
    if (archived.length) groups.push({ label: t("commandBar.groups.archive"), items: archived });
    // Last: the list arrives a moment after the bar opens, and rows added
    // here move none of the rows above, so the selected one stays selected.
    if (local.length) groups.push({ label: t("commandBar.groups.local_servers"), items: local });
    for (const g of groups) items.push(...g.items);
  }
  const sel = Math.min(selected, Math.max(0, items.length - 1));

  useEffect(() => {
    list.current?.querySelector<HTMLElement>("[data-selected]")?.scrollIntoView({ block: "nearest" });
  }, [sel, results]);

  /**
   * Select row `i` when the pointer really moves over it. A row that appears
   * or scrolls under a resting pointer must not take the selection from the
   * keyboard (Return would run it; arrow keys would jump back).
   */
  const hover = (i: number) => (e: React.MouseEvent) => {
    if (e.movementX !== 0 || e.movementY !== 0) setSelected(i);
  };

  /** Run `item` and close the palette, unless the pick ends it itself (`keepOpen`). */
  const pick = (item: Item) => {
    item.run();
    if (!(item.kind === "action" && item.keepOpen)) onClose();
  };

  /** Typing: "keyword " (a known keyword and a space) becomes the chip. */
  const onChange = (value: string) => {
    const found = site ? null : siteSearchOf(value, sites);
    if (found) {
      setSite(found.site);
      setQuery(found.query);
    } else setQuery(value);
    setSelected(0);
  };

  const onKey = (e: React.KeyboardEvent<HTMLInputElement>) => {
    const caretAtStart = e.currentTarget.selectionStart === 0 && e.currentTarget.selectionEnd === 0;
    if (e.key === "Escape") onClose();
    else if (e.key === "Tab" && !site && !e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey) {
      // Tab after a keyword starts its site search, like a space does.
      const keyword = sites.find((s) => s.keyword === q.toLowerCase());
      e.preventDefault();
      if (keyword) {
        setSite(keyword);
        setQuery("");
      }
    } else if (e.key === "Backspace" && site && caretAtStart) {
      // Back to plain input: the keyword returns as text.
      e.preventDefault();
      setSite(null);
      setQuery(site.keyword + (query ? ` ${query}` : ""));
    } else if (e.key === "ArrowDown") { e.preventDefault(); setSelected((s) => Math.min(s + 1, items.length - 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setSelected((s) => Math.max(s - 1, 0)); }
    else if (e.key === "Enter") {
      e.preventDefault();
      const item = items[sel];
      if (!item) return;
      if (quick) pick(item);
      else if (e.shiftKey && item.kind !== "action" && item.peek) {
        send({ cmd: "open_peek", url: item.peek });
        onClose();
      } else if ((IS_MAC ? e.metaKey : e.ctrlKey) && item.kind === "url") {
        send({ cmd: "new_tab", url: item.url });
        onClose();
      } else pick(item);
    }
  };

  // The hint line: what the keys do with the selected row, or what to type.
  const current = items[sel];
  const hints: [keys: string, does: string][] = [];
  if (current) {
    const opens = quick ? t("commandBar.hints.open_mini") : mode === "navigate" ? t("commandBar.hints.open_here") : mode === "split" ? t("commandBar.hints.open_split") : t("commandBar.hints.open_new_tab");
    const enter =
      current.kind === "action" ? (naming ? t("commandBar.hints.create") : t("commandBar.hints.run"))
      : current.kind === "tab" ? (mode === "split" ? t("commandBar.hints.add_to_split") : t("commandBar.hints.go_to_tab"))
      : current.kind === "archived" ? t("common.restore")
      : current.kind === "bookmark" && !quick ? t("commandBar.hints.open_new_tab")
      : opens;
    hints.push([KEYS.enter, enter]);
    if (!quick && current.kind !== "action" && current.peek) hints.push([KEYS.peek, t("commandBar.hints.peek")]);
    // Enter opens a new tab already when the bar was opened for one.
    if (!quick && current.kind === "url" && mode !== "new") hints.push([KEYS.newTab, t("commandBar.hints.new_tab")]);
  }
  const keyword = site ? undefined : sites.find((s) => s.keyword === q.toLowerCase());
  if (keyword) hints.push([KEYS.tab, t("commandBar.site_search.search", { site: keyword.name })]);
  if (site && !query) hints.push([KEYS.back, t("commandBar.hints.all_results")]);
  const idle = current ? null : site ? t("commandBar.site_search.idle", { site: site.name }) : quick ? t("commandBar.hints.idle_quick") : t("commandBar.hints.idle");

  let flat = -1;
  return (
    <div className="cmdbar" role="combobox" aria-expanded={items.length > 0}>
      <div className="flex items-center">
        {site && <span className="site-chip" title={t("commandBar.site_search.chip_title", { site: site.name })}>{site.name}</span>}
        <input
          ref={input}
          aria-label={t("commandBar.input.label")}
          value={query}
          placeholder={site ? t("commandBar.site_search.search", { site: site.name }) : mode === "new_space" ? t("commandBar.input.name_space") : mode === "new_profile" ? t("commandBar.input.name_profile") : mode === "split" ? splitPlaceholder(splitSide) : quick ? t("commandBar.input.quick") : t("commandBar.input.default")}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={onKey}
          spellCheck={false}
        />
      </div>
      {items.length > 0 && (
        <div ref={list} className="results scroll" role="listbox">
          {groups.map((g, gi) => (
            <div key={gi}>
              {g.label && <div className="group">{g.label}</div>}
              {g.items.map((it) => {
                const i = ++flat;
                return (
                  <div key={it.kind + ("id" in it ? it.id : it.title)} className="result" role="option" aria-selected={i === sel} data-selected={i === sel || undefined} onMouseMove={hover(i)} onClick={() => pick(it)}>
                    {it.kind === "action" ? <span className="icon-slot"><CommandIcon width={14} height={14} /></span> : it.kind === "url" ? <span className="icon-slot"><Arrow width={14} height={14} /></span> : <Favicon url={it.url} src={it.favicon} />}
                    <div className="min-w-0 flex-1">
                      <div className="truncate">{it.kind === "action" ? it.title : it.title || hostOf(it.url)}</div>
                      {it.kind !== "action" && it.kind !== "url" && <div className="meta">{it.kind === "archived" ? t("commandBar.results.archived", { host: hostOf(it.url) }) : it.space ? `${it.space} · ${hostOf(it.url)}` : hostOf(it.url)}</div>}
                    </div>
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      )}
      <div className="hints">
        {idle && <span>{idle}</span>}
        {idle && !site && example && <span>{rich("commandBar.site_search.example", { keyword: <kbd>{example.keyword}</kbd>, site: example.name })}</span>}
        {hints.map(([keys, does]) => (
          <span key={keys}><kbd>{keys}</kbd> {does}</span>
        ))}
      </div>
    </div>
  );
}
