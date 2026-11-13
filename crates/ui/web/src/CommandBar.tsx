import { useEffect, useRef, useState } from "react";
import { Favicon } from "./Favicon";
import { Arrow, Command as CommandIcon } from "./Icons";
import { IS_MAC, findTab, findTabInSpace, hostOf, send, type Command, type PaletteMode, type SearchResult, type Side, type SiteSearch, type UiState } from "./bridge";
import { useDismiss } from "./dismiss";

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

const ACTIONS: Action[] = [
  { id: "new_space", title: "New space", keywords: ["space"], cmd: { cmd: "new_space", title: "" } },
  { id: "new_folder", title: "New folder", keywords: ["folder"], cmd: { cmd: "new_folder", title: "" } },
  { id: "archive_today", title: "Archive all Today tabs", keywords: ["clear", "archive"], cmd: { cmd: "archive_today" } },
  { id: "reopen", title: "Reopen last archived tab", keywords: ["restore", "closed"], cmd: { cmd: "reopen_closed" } },
  { id: "downloads", title: "Open downloads", keywords: ["downloads", "files"], cmd: { cmd: "show_downloads" } },
  { id: "bookmarks", title: "Open bookmarks", keywords: ["bookmarks", "saved"], cmd: { cmd: "show_bookmarks" } },
  { id: "bookmark_page", title: "Bookmark this page", keywords: ["bookmark", "save", "keep"], cmd: { cmd: "add_bookmark" } },
  { id: "toggle_sidebar", title: "Toggle sidebar", keywords: ["hide", "show"], cmd: { cmd: "toggle_sidebar" } },
  { id: "import", title: "Import from another browser…", keywords: ["import", "chrome", "firefox", "zen", "arc", "brave", "edge"], cmd: { cmd: "new_tab", url: "magnetite://ui/settings#data" } },
  { id: "copy_link", title: "Copy link without trackers", keywords: ["copy", "link", "url", "clean"], cmd: { cmd: "copy_link", variant: "clean", notify: true } },
  { id: "copy_original", title: "Copy original link", keywords: ["copy", "link", "url", "raw"], cmd: { cmd: "copy_link", variant: "original", notify: true } },
  { id: "copy_markdown", title: "Copy link as Markdown", keywords: ["copy", "markdown", "md"], cmd: { cmd: "copy_link", variant: "markdown", notify: true } },
  { id: "zoom_reset", title: "Reset zoom", keywords: ["zoom", "100"], cmd: { cmd: "set_zoom", percent: 100 } },
  { id: "split_right", title: "Add right split…", keywords: ["split", "right", "side"], cmd: { cmd: "open_palette", mode: "split", side: "right" } },
  { id: "split_left", title: "Add left split…", keywords: ["split", "left", "side"], cmd: { cmd: "open_palette", mode: "split", side: "left" } },
  { id: "split_top", title: "Add top split…", keywords: ["split", "top", "above"], cmd: { cmd: "open_palette", mode: "split", side: "top" } },
  { id: "split_bottom", title: "Add bottom split…", keywords: ["split", "bottom", "below"], cmd: { cmd: "open_palette", mode: "split", side: "bottom" } },
  { id: "split_even", title: "Even out split sizes", keywords: ["split", "equal", "even", "reset"], cmd: { cmd: "split_equalize", path: [] } },
  { id: "separate", title: "Separate split tabs", keywords: ["split", "separate", "unsplit"], cmd: { cmd: "separate_all" } },
  { id: "devtools", title: "Toggle developer tools", keywords: ["devtools", "developer", "console", "inspect", "f12"], cmd: { cmd: "toggle_devtools" } },
  { id: "capture", title: "Capture page…", keywords: ["capture", "screenshot", "snap"], cmd: { cmd: "capture_start" } },
  { id: "capture_copy", title: "Copy last capture", keywords: ["capture", "screenshot", "copy"], cmd: { cmd: "capture_copy", notify: true } },
  { id: "print", title: "Print page", keywords: ["print", "pdf"], cmd: { cmd: "shortcut", shortcut: "print" } },
  { id: "history", title: "Open history", keywords: ["history", "visited"], cmd: { cmd: "show_history" } },
];

/**
 * Commands for the active tab and the spaces, as the tab's menu in the
 * sidebar offers them (`tabMenu` in Rows.tsx): what fits the tab now.
 */
function tabActions(state: UiState): Action[] {
  const actions: Action[] = [];
  const spaces = state.sidebar.spaces;
  for (const sp of spaces) {
    if (sp.id !== state.active_space) actions.push({ id: `switch_${sp.id}`, title: `Switch to ${sp.title}`, keywords: ["space", "switch", "go"], cmd: { cmd: "switch_space", id: sp.id } });
  }
  const id = state.active_tab;
  const tab = id ? findTab(state, id) : null;
  if (!id || !tab) return actions;
  const owner = spaces.find((sp) => findTabInSpace(sp, id));
  const favorite = !owner;
  const today = owner?.today.some((t) => t.id === id) ?? false;
  if (owner && today) actions.push({ id: "pin", title: "Pin tab", keywords: ["pin"], cmd: { cmd: "pin", id } });
  if (owner && !today) actions.push({ id: "unpin", title: "Unpin tab", keywords: ["unpin", "pin"], cmd: { cmd: "unpin", id } });
  if (!today && tab.home_url && tab.home_url !== tab.url) actions.push({ id: "reset", title: `Back to pinned page (${hostOf(tab.home_url)})`, keywords: ["reset", "home", "pinned"], cmd: { cmd: "reset_tab", id } });
  if (favorite) actions.push({ id: "unfavorite", title: "Remove tab from favorites", keywords: ["favorite", "unfavorite"], cmd: { cmd: "move_tab", id, space: state.active_space, section: "today", index: 0 } });
  else actions.push({ id: "favorite", title: "Add tab to favorites", keywords: ["favorite"], cmd: { cmd: "move_tab", id, space: state.active_space, section: "favorites", index: 999 } });
  if (owner) {
    actions.push({ id: "keep_active", title: tab.keep_active ? "Stop keeping tab active" : "Keep tab active", keywords: ["keep", "active", "awake"], cmd: { cmd: "set_keep_active", id, keep: !tab.keep_active } });
    for (const sp of spaces) {
      if (sp.id !== owner.id) actions.push({ id: `move_${sp.id}`, title: `Move tab to ${sp.title}`, keywords: ["move", "space"], cmd: { cmd: "move_tab", id, space: sp.id, section: "today", index: 0 } });
    }
  }
  // A favorite's copy keeps its profile; a tab's opens as a new Today tab of this space.
  actions.push({ id: "duplicate", title: "Duplicate tab", keywords: ["duplicate", "copy", "clone"], cmd: favorite ? { cmd: "duplicate_favorite", id } : { cmd: "new_tab", url: tab.url } });
  const space = spaces.find((sp) => sp.id === state.active_space);
  if (space?.today.some((t) => t.id !== id)) actions.push({ id: "close_others", title: "Close other Today tabs", keywords: ["close", "others", "archive"], cmd: { cmd: "archive_today", keep: id } });
  return actions;
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
    const t = setTimeout(() => {
      send({ cmd: "search", query: q }).then((r) => {
        if (!cancelled) setResults(r.results ?? []);
      });
    }, 120);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [query, naming, site]);

  const q = query.trim();
  const items: Item[] = [];
  const groups: { label: string; items: Item[] }[] = [];
  /** Open `url` (typed input, or an address) as this palette's mode asks. */
  const open = (url: string) =>
    quick ? send({ cmd: "open_mini", url }) : mode === "navigate" ? send({ cmd: "navigate", url }) : send({ cmd: "new_tab", url, split: mode === "split", side: splitSide });
  if (mode === "new_space") {
    items.push({ kind: "action", id: "create_space", title: q ? `Create space "${q}"` : "Create space", run: () => send({ cmd: "new_space", title: q }) });
    groups.push({ label: "", items });
  } else if (mode === "new_profile") {
    const target = profileFor ? "this favorite" : "this space";
    items.push({ kind: "action", id: "create_profile", title: q ? `Create profile "${q}" and use it for ${target}` : "Name the profile", keepOpen: true, run: async () => {
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
    if (q) groups.push({ label: "", items: [{ kind: "url", title: `Search ${site.name} for "${q}"`, url: typed, peek: typed, run: () => open(typed) }] });
    for (const g of groups) items.push(...g.items);
  } else {
    const top: Item[] = [];
    if (q) {
      top.push({ kind: "url", title: opensAsAddress(q) ? `Open ${q}` : `Search for "${q}"`, url: q, peek: q, run: () => open(q) });
    }
    const actions: Item[] = [];
    // The quick search opens pages only: commands are for the main window.
    if (q && !quick) {
      const lower = q.toLowerCase();
      for (const a of [...ACTIONS, ...tabActions(state)]) {
        // "Add … split…" opens the next palette in this one's place: closing after it would close that one.
        if (a.title.toLowerCase().includes(lower) || a.keywords.some((k) => k.startsWith(lower))) actions.push({ kind: "action", id: a.id, title: a.title, keepOpen: a.cmd.cmd === "open_palette", run: () => send(a.cmd) });
      }
      if ("archive".startsWith(lower)) actions.push({ kind: "action", id: "open_archive", title: "Open archive", run: onOpenArchive });
      if ("settings".startsWith(lower) || "preferences".startsWith(lower)) actions.push({ kind: "action", id: "open_settings", title: "Open settings", run: onOpenSettings });
    }
    const tabs: Item[] = results.filter((r) => r.kind === "tab" && !quick).map((r) => ({ ...r, run: () => send(mode === "split" ? { cmd: "add_split", id: r.id, side: splitSide } : { cmd: "activate_tab", id: r.id }) }));
    const saved: Item[] = results.filter((r) => r.kind === "bookmark").map((r) => ({ ...r, peek: r.url, run: () => (quick ? open(r.url) : send({ cmd: "open_bookmark", id: r.id })) }));
    const visited: Item[] = results.filter((r) => r.kind === "history").map((r) => ({ ...r, peek: r.url, run: () => open(r.url) }));
    const archived: Item[] = results.filter((r) => r.kind === "archived" && !quick).map((r) => ({ ...r, run: () => send({ cmd: "restore_archived", id: r.id }) }));
    if (top.length) groups.push({ label: "", items: top });
    if (tabs.length) groups.push({ label: "Tabs", items: tabs });
    if (actions.length) groups.push({ label: "Commands", items: actions });
    if (saved.length) groups.push({ label: "Bookmarks", items: saved });
    if (visited.length) groups.push({ label: "History", items: visited });
    if (archived.length) groups.push({ label: "Archive", items: archived });
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
    const opens = quick ? "Open in a mini window" : mode === "navigate" ? "Open in this tab" : mode === "split" ? "Open in the split" : "Open in a new tab";
    const enter =
      current.kind === "action" ? (naming ? "Create" : "Run")
      : current.kind === "tab" ? (mode === "split" ? "Add to the split" : "Go to the tab")
      : current.kind === "archived" ? "Restore"
      : current.kind === "bookmark" && !quick ? "Open in a new tab"
      : opens;
    hints.push([KEYS.enter, enter]);
    if (!quick && current.kind !== "action" && current.peek) hints.push([KEYS.peek, "Peek"]);
    // Enter opens a new tab already when the bar was opened for one.
    if (!quick && current.kind === "url" && mode !== "new") hints.push([KEYS.newTab, "New tab"]);
  }
  const keyword = site ? undefined : sites.find((s) => s.keyword === q.toLowerCase());
  if (keyword) hints.push([KEYS.tab, `Search ${keyword.name}`]);
  if (site && !query) hints.push([KEYS.back, "All results"]);
  const idle = current ? null : site ? `Type what to search on ${site.name}` : quick ? "Finds bookmarks and visited pages" : "Finds tabs, bookmarks, visited pages and commands";

  let flat = -1;
  return (
    <div className="cmdbar" role="combobox" aria-expanded={items.length > 0}>
      <div className="flex items-center">
        {site && <span className="site-chip" title={`Searches ${site.name}; Backspace goes back`}>{site.name}</span>}
        <input
          ref={input}
          aria-label="Command bar"
          value={query}
          placeholder={site ? `Search ${site.name}` : mode === "new_space" ? "Name the new space" : mode === "new_profile" ? "Name the new profile" : mode === "split" ? `Open ${splitSide === "left" ? "left of" : splitSide === "top" ? "above" : splitSide === "bottom" ? "below" : "beside"} this tab…` : quick ? "Search or enter URL to open in a mini window" : "Search or enter URL"}
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
                      {it.kind !== "action" && it.kind !== "url" && <div className="meta">{it.kind === "archived" ? "Archived · " : it.space ? `${it.space} · ` : ""}{hostOf(it.url)}</div>}
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
        {idle && !site && example && <span><kbd>{example.keyword}</kbd> and a space searches {example.name}</span>}
        {hints.map(([keys, does]) => (
          <span key={keys}><kbd>{keys}</kbd> {does}</span>
        ))}
      </div>
    </div>
  );
}
