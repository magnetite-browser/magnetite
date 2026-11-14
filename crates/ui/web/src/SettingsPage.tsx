// Full-page settings, opened as its own tab (magnetite://ui/settings).
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { AdblockIcon, Archive, Close, CodeIcon, CookieIcon, DevicesIcon, InfoIcon, PaletteIcon, PrivacyIcon, PuzzleIcon, Search, SettingsIcon, SmartFolderIcon, type IconComponent, KeyIcon } from "./Icons";
import { IsolationSection } from "./IsolationSection";
import { SEARCH_ENGINES } from "./settings/searchEngines";
import { ARCHIVE_AFTER, UNLOAD_AFTER } from "./settings/tabTimes";
import { applyAppearance, IS_MAC, keyLabel, send, type DefaultBrowser, type Settings, type SiteSearch } from "./bridge";
import { About } from "./settings/AboutSection";
import { AdblockSection } from "./settings/AdblockSection";
import { AutofillSection } from "./settings/AutofillSection";
import { AppearanceSection } from "./settings/AppearanceSection";
import { Field, Switch, Toggle } from "./settings/fields";
import { ImportBookmarksFile, ImportBrowsers } from "./settings/ImportBrowsers";
import { SmartFoldersSection } from "./settings/SmartFoldersSection";
import { SyncSection, type SyncJob } from "./settings/SyncSection";
import type { Info, SettingsPatch } from "./settings/types";

/** The Chrome Web Store's extensions section. */
const EXTENSION_STORE = "https://chromewebstore.google.com/category/extensions";

type Section = { id: string; label: string; Icon: IconComponent; title: string; intro: string };

const GROUPS: { label: string; sections: Section[] }[] = [
  {
    label: "Browsing",
    sections: [
      { id: "general", label: "General", Icon: SettingsIcon, title: "General", intro: "Magnetite keeps everything on this computer. No account is needed." },
      { id: "appearance", label: "Appearance", Icon: PaletteIcon, title: "Appearance", intro: "How the sidebar and Magnetite's own pages look. Web pages keep their own design." },
      { id: "tabs", label: "Tabs & archive", Icon: Archive, title: "Tabs & archive", intro: "Closing a tab never deletes it. Today tabs you stop using move to the archive on their own. The history of visited pages stays on this computer." },
      { id: "search", label: "Search", Icon: Search, title: "Search", intro: "Used when what you type in the command bar is not a web address. A keyword and a space search one site instead." },
      { id: "adblock", label: "Ad blocking", Icon: AdblockIcon, title: "Ad blocking", intro: "Magnetite can stop ads and trackers before they load, with filter lists you choose. Off until you turn it on." },
      { id: "autofill", label: "Passwords & autofill", Icon: KeyIcon, title: "Passwords & autofill", intro: "Magnetite can keep your passwords, addresses and cards and fill them in for you. All of it stays on this computer." },
    ],
  },
  {
    label: "Spaces & services",
    sections: [
      { id: "spaces", label: "Isolation", Icon: CookieIcon, title: "Isolation", intro: "Websites remember you with cookies. Isolation decides which spaces and favorites share them, so you can be signed in to two accounts of the same site at once." },
      { id: "smart", label: "Smart folders", Icon: SmartFolderIcon, title: "Smart folders", intro: "Folders that fill themselves with your pull requests, issues or tickets and stay up to date. Magnetite asks the service directly from this computer; tokens stay on this device and are never synced." },
      { id: "extensions", label: "Extensions", Icon: PuzzleIcon, title: "Extensions", intro: "Chrome extensions install from the Chrome Web Store, right here in Magnetite. Choose which ones sit in the sidebar; all of them are in the menu bar under Extensions." },
      { id: "sync", label: "Sync", Icon: DevicesIcon, title: "Sync", intro: "Optional, zero-knowledge sync between your devices, through Magnetite's free server or one you host yourself. No account: a 24-word sync code is the only key." },
    ],
  },
  {
    label: "Magnetite",
    sections: [
      { id: "data", label: "Data & privacy", Icon: PrivacyIcon, title: "Data & privacy", intro: "No telemetry, no Magnetite account. Magnetite talks only to the sites you open, the services you connect (sync, smart folders), the publishers of the ad blocking lists you choose, GitHub for update checks, and Google's component server, from which Chromium fetches the Widevine module for protected video and its certificate lists." },
      { id: "developer", label: "Developer", Icon: CodeIcon, title: "Developer", intro: "Tools for building and debugging websites." },
      { id: "about", label: "About", Icon: InfoIcon, title: "About Magnetite", intro: "A free, open-source browser built around a sidebar, on a Chromium engine, made in Europe." },
    ],
  },
];
const SECTIONS = GROUPS.flatMap((g) => g.sections);


export function SettingsPage() {
  const [info, setInfo] = useState<Info | null>(null);
  const [sectionId, setSectionId] = useState(() => SECTIONS.find((s) => s.id === location.hash.slice(1))?.id ?? "general");
  // Changes made on this page. What the app answered to a question from
  // before the last one is older than what the page shows: asked again.
  const edits = useRef(0);
  const reload = useCallback(function ask(): Promise<Info> {
    const asked = edits.current;
    return send({ cmd: "get_settings" }).then((fresh) => {
      if (asked !== edits.current) return ask();
      setInfo(fresh);
      return fresh;
    });
  }, []);
  // The app sends an open Settings tab to another section by its address.
  useEffect(() => {
    const follow = () => {
      const wanted = SECTIONS.find((s) => s.id === location.hash.slice(1));
      if (wanted) setSectionId(wanted.id);
    };
    window.addEventListener("hashchange", follow);
    return () => window.removeEventListener("hashchange", follow);
  }, []);
  // A start or join of sync the app has not answered yet (see `SyncSection`).
  const [syncJob, setSyncJob] = useState<SyncJob | null>(null);
  useEffect(() => {
    document.title = "Settings";
    void reload();
  }, [reload]);
  // The app does not tell this page what changes elsewhere (the sidebar's
  // edge its width, a closed tab the archive, the Web Store the extensions),
  // and a change here is saved on top of this page's copy: ask again
  // whenever the page comes back.
  useEffect(() => {
    const back = () => {
      if (document.visibilityState === "visible") void reload();
    };
    window.addEventListener("focus", back);
    document.addEventListener("visibilitychange", back);
    return () => {
      window.removeEventListener("focus", back);
      document.removeEventListener("visibilitychange", back);
    };
  }, [reload]);
  // Text size and density of this page, as saved (Appearance previews its draft itself).
  const textScale = info?.settings.appearance.text_scale;
  const density = info?.settings.appearance.density;
  useLayoutEffect(() => {
    if (textScale !== undefined && density !== undefined) applyAppearance({ text_scale: textScale, density });
  }, [textScale, density]);
  if (!info) return <div className="h-full" />;
  const settings = info.settings;
  const update = (patch: SettingsPatch) => {
    const next = { ...settings, ...patch };
    edits.current++;
    setInfo({ ...info, settings: next });
    // Only what changed: the app keeps the rest as it has it.
    send({ cmd: "set_settings", settings: patch });
  };
  const section = SECTIONS.find((s) => s.id === sectionId) ?? SECTIONS[0]!;

  return (
    <div className="flex h-full" style={{ background: "var(--bg)" }}>
      <nav className="set-nav scroll w-[232px] shrink-0 border-r border-[var(--line)] p-3">
        <div className="px-2 pb-2 pt-1 text-[16px] font-semibold">Settings</div>
        {GROUPS.map((g) => (
          <div key={g.label} className="mt-3">
            <div className="set-nav-label">{g.label}</div>
            {g.sections.map((s) => (
              <button key={s.id} className="row w-full" data-active={sectionId === s.id || undefined} onClick={() => { setSectionId(s.id); location.hash = s.id; }}>
                <span className="icon-slot" style={{ color: sectionId === s.id ? "var(--accent)" : "var(--muted)" }}><s.Icon width={16} height={16} /></span>
                <span className="title text-left">{s.label}</span>
              </button>
            ))}
          </div>
        ))}
      </nav>
      <main className="scroll flex-1 min-h-0 px-10 py-8">
        <div className="max-w-[680px]">
          <header className="mb-6 flex items-start gap-4">
            <span className="set-icon"><section.Icon width={22} height={22} /></span>
            <div>
              <h1 className="text-[22px] font-semibold tracking-[-0.01em]">{section.title}</h1>
              <p className="mt-1 max-w-[560px] text-[14px] leading-5 text-[var(--muted)]">{section.intro}</p>
            </div>
          </header>
          <div className="space-y-3">
          {sectionId === "general" && (
            <>
              {info.default_browser !== "unknown" && <DefaultBrowserCard key={info.default_browser} state={info.default_browser} />}
              <Field label="Open links from other apps in" hint="Links from Mail, Slack and other apps, when Magnetite is your default browser.">
                <select className="field" value={settings.external_links} onChange={(e) => update({ external_links: e.target.value as Settings["external_links"] })}>
                  <option value="tab">a new tab in the current space</option>
                  <option value="peek">a peek window</option>
                  <option value="mini">a mini window</option>
                </select>
              </Field>
              <Toggle label={`Quick search from anywhere (${keyLabel("⌥⌘N")})`} hint="Opens the command bar over any app; what you pick opens in a mini window. Only that key combination reaches Magnetite, no other typing." checked={settings.quick_search} onChange={(v) => update({ quick_search: v })} />
              <Toggle label="Pop out playing videos" hint="When you leave a tab with a playing video, it continues in a floating picture-in-picture window. Coming back puts it into the page again." checked={settings.auto_pip} onChange={(v) => update({ auto_pip: v })} />
              <Toggle label="Remove trackers from copied links" hint={`Copy Link (${keyLabel("⇧⌘C")}) drops tracking parameters such as utm_source and fbclid.`} checked={settings.strip_trackers} onChange={(v) => update({ strip_trackers: v })} />
            </>
          )}
          {sectionId === "tabs" && (
            <>
              <Field label="Archive Today tabs after" hint="Pinned tabs and favorites are never archived.">
                <select className="field" value={settings.auto_archive_hours} onChange={(e) => update({ auto_archive_hours: Number(e.target.value) })}>
                  {ARCHIVE_AFTER.map((t) => <option key={t.hours} value={t.hours}>{t.label}</option>)}
                </select>
              </Field>
              <ArchiveRetention key={settings.archive_days} days={settings.archive_days} save={(days) => update({ archive_days: days })} />
              <Field label="Suspend inactive tabs after" hint="Suspended tabs keep their place and reload when opened. This is what keeps hundreds of tabs cheap.">
                <select className="field" value={settings.unload_minutes} onChange={(e) => update({ unload_minutes: Number(e.target.value) })}>
                  {UNLOAD_AFTER.map((t) => <option key={t.minutes} value={t.minutes}>{t.label}</option>)}
                </select>
              </Field>
              <Field label="Clicking the archive button opens" hint="The button at the bottom left of the sidebar; resting on it shows bookmarks, history, downloads and the archive. While a download runs, a click opens downloads.">
                <select className="field" value={settings.library_opens} onChange={(e) => update({ library_opens: e.target.value as Settings["library_opens"] })}>
                  <option value="archive">the archive</option>
                  <option value="downloads">downloads</option>
                  <option value="bookmarks">bookmarks</option>
                  <option value="history">history</option>
                </select>
              </Field>
              <Field label="Archive" hint={`${info.archive_count} archived tabs. Search them from the command bar or the archive view in the sidebar.`}>
                <button className="btn" onClick={() => send({ cmd: "reopen_closed" })}>Reopen last archived tab</button>
              </Field>
              <Field label="Keep history for" hint={`The pages you visit, with title and time, to find them again in the command bar and the history view (${keyLabel("⌘Y", "Ctrl+H")}). Kept on this computer only, never synced; private profiles leave none. Older visits are deleted within a minute.`}>
                <select className="field" value={settings.history_days} onChange={(e) => update({ history_days: Number(e.target.value) })}>
                  {[7, 30, 90].includes(settings.history_days) || settings.history_days === 0 ? null : <option value={settings.history_days}>{settings.history_days} days</option>}
                  <option value={7}>7 days</option><option value={30}>30 days</option><option value={90}>90 days</option><option value={0}>Don't keep history</option>
                </select>
              </Field>
              <Field label="History" hint={`${info.history_count} visited pages. Single pages can be removed in the history view.`}>
                <ClearHistory key={info.history_count} disabled={info.history_count === 0} onClear={() => send({ cmd: "clear_history" }).then(() => void reload())} />
              </Field>
            </>
          )}
          {sectionId === "appearance" && <AppearanceSection info={info} update={update} reload={reload} />}
          {sectionId === "smart" && <SmartFoldersSection />}
          {sectionId === "autofill" && <AutofillSection info={info} update={update} />}
          {sectionId === "adblock" && <AdblockSection info={info} update={update} reload={reload} />}
          {sectionId === "search" && (
            <>
              <Field label="Search engine">
                <div className="grid grid-cols-3 gap-2">
                  {SEARCH_ENGINES.map((e) => (
                    <button key={e.template} className="choice" data-active={settings.search_template === e.template || undefined} onClick={() => update({ search_template: e.template })}>{e.label}</button>
                  ))}
                </div>
              </Field>
              <SearchAddress key={settings.search_template} saved={settings.search_template} save={(template) => update({ search_template: template })} />
              <SiteSearches saved={settings.site_searches} save={(sites) => update({ site_searches: sites })} />
            </>
          )}
          {sectionId === "spaces" && <IsolationSection info={info} reload={reload} />}
          {sectionId === "developer" && (
            <Field label="Developer tools open" hint={`F12, ${keyLabel("⌥⌘I", "Ctrl+Shift+I")}, or Developer tools in the site panel. Docked tools sit next to the page inside the window; the page gets narrower or shorter to make room.`}>
              <div className="seg">
                {([["right", "Right of the page"], ["bottom", "Below the page"], ["window", "In a separate window"]] as const).map(([v, label]) => (
                  <button key={v} data-on={settings.devtools_dock === v || undefined} onClick={() => update({ devtools_dock: v })}>{label}</button>
                ))}
              </div>
            </Field>
          )}
          {sectionId === "extensions" && (
            <>
              <Field label="In the sidebar" hint="Pinned extensions sit above your tabs. Chromium's own “Pin” on its extensions page works too.">
                {info.extensions.length === 0 ? (
                  <p className="text-[13px] text-[var(--muted)]">No extensions installed yet.</p>
                ) : (
                  <div className="list">
                    {info.extensions.map((e) => {
                      const pinned = settings.pinned_extensions.includes(e.id);
                      return (
                        <label key={e.id} className="list-row">
                          <span className="min-w-0 flex-1 truncate">{e.name}</span>
                          <Switch
                            label={`${e.name} in the sidebar`}
                            checked={pinned}
                            onChange={(v) => {
                              const list = settings.pinned_extensions.filter((x) => x !== e.id).concat(v ? [e.id] : []);
                              edits.current++;
                              setInfo({ ...info, settings: { ...settings, pinned_extensions: list } });
                              send({ cmd: "pin_extension", id: e.id, pinned: v });
                            }}
                          />
                        </label>
                      );
                    })}
                  </div>
                )}
              </Field>
              <Field label="Install and manage" hint="Removing, turning off and site access live on Chromium's extensions page.">
                <div className="flex gap-2">
                  <button className="btn" onClick={() => send({ cmd: "new_tab", url: EXTENSION_STORE })}>Chrome Web Store</button>
                  <button className="btn" onClick={() => send({ cmd: "new_tab", url: "chrome://extensions" })}>Manage extensions</button>
                </div>
              </Field>
              <Toggle label="Show the Chrome toolbar inside pages" hint="Chromium's own toolbar row above every page, with its extension buttons. Applies to tabs loaded from now on." checked={settings.chrome_toolbar} onChange={(v) => update({ chrome_toolbar: v })} />
            </>
          )}
          {sectionId === "sync" && <SyncSection info={info} reload={reload} job={syncJob} setJob={setSyncJob} />}
          {sectionId === "data" && (
            <>
              <ImportBrowsers />
              <ImportBookmarksFile />
              <Field label="Where your data lives" hint="The sidebar, archive and settings are in state.json, the history in history.json; Chromium's profile data (cookies, cache, extensions) in the cef folder next to it.">
                <code className="block rounded-lg bg-white/5 px-3 py-2 text-[12px] text-[var(--secondary)]">{info.data_dir}</code>
              </Field>
              {info.keychain === "test" && (
                <Field
                  label="Development mode: cookies use a fixed key"
                  hint="This data folder runs with --mock-keychain: Chromium encrypts cookies and saved passwords with its built-in test key instead of one in your macOS Keychain, so any program that can read the folder can read them. Fine while developing; a signed release build uses the Keychain."
                />
              )}
            </>
          )}
          {sectionId === "about" && <About info={info} update={update} />}
          </div>
        </div>
      </main>
    </div>
  );
}

/**
 * The custom search address, saved when the field is left. The app uses only
 * an address on https with `%s` in it (`Settings::sanitised` in
 * crates/app/src/state/settings.rs) and falls back to its default engine otherwise, so
 * any other is kept out and marked.
 */
function SearchAddress({ saved, save }: { saved: string; save: (template: string) => void }) {
  const [ignored, setIgnored] = useState(false);
  return (
    <Field label="Custom search address" hint="Use %s where the query goes. Takes effect when it starts with https:// and contains %s.">
      <input
        className="field"
        defaultValue={saved}
        aria-invalid={ignored || undefined}
        onChange={() => setIgnored(false)}
        onBlur={(e) => {
          const template = e.target.value.trim();
          if (template === saved) return;
          if (template.startsWith("https://") && template.includes("%s")) save(template);
          else setIgnored(true);
        }}
        spellCheck={false}
      />
      {ignored && <p className="set-hint" style={{ color: "var(--danger)" }}>Not saved: the address must start with https:// and contain %s.</p>}
    </Field>
  );
}

const RETENTION_DAYS = [30, 60, 90];
const MAX_RETENTION_DAYS = 3650;

/**
 * How long the archive keeps a tab: 30, 60 or 90 days, or a number of days
 * typed in. A typed number is saved on Enter or when the field is left, not
 * while typing: a shorter period deletes archived tabs within a minute.
 */
function ArchiveRetention({ days, save }: { days: number; save: (days: number) => void }) {
  const [custom, setCustom] = useState(!RETENTION_DAYS.includes(days));
  const [text, setText] = useState(String(days));
  const typed = Number(text);
  const valid = Number.isInteger(typed) && typed >= 1 && typed <= MAX_RETENTION_DAYS;
  const commit = () => {
    if (valid && typed !== days) save(typed);
  };
  return (
    <Field label="Keep archived tabs for" hint="Older ones are deleted from the archive for good. A tab can also be removed by hand in the archive view.">
      <div className="flex items-center gap-2">
        <select
          className="field"
          value={custom ? "custom" : days}
          onChange={(e) => {
            if (e.target.value === "custom") return setCustom(true);
            // The period already saved changes nothing in the app: only "Custom…" goes.
            setCustom(false);
            if (Number(e.target.value) !== days) save(Number(e.target.value));
          }}
        >
          {RETENTION_DAYS.map((d) => <option key={d} value={d}>{d} days</option>)}
          <option value="custom">Custom…</option>
        </select>
        {custom && (
          <>
            <input
              className="field"
              style={{ width: 88 }}
              inputMode="numeric"
              value={text}
              aria-label="Days"
              aria-invalid={!valid || undefined}
              autoFocus={RETENTION_DAYS.includes(days)}
              onChange={(e) => setText(e.target.value.replace(/\D/g, "").slice(0, 4))}
              onBlur={commit}
              onKeyDown={(e) => e.key === "Enter" && commit()}
            />
            <span className="shrink-0 text-[var(--muted)]">days</span>
          </>
        )}
      </div>
      {custom && !valid && <p className="set-hint" style={{ color: "var(--danger)" }}>Not saved: enter 1 to {MAX_RETENTION_DAYS} days.</p>}
    </Field>
  );
}

/** "Clear history", after a second click. */
function ClearHistory({ disabled, onClear }: { disabled: boolean; onClear: () => void }) {
  const [confirming, setConfirming] = useState(false);
  return (
    <div className="flex gap-2">
      <button className="btn" disabled={disabled} data-danger={confirming || undefined} onClick={() => (confirming ? onClear() : setConfirming(true))} onBlur={() => setConfirming(false)}>
        {confirming ? "Clear all history" : "Clear history…"}
      </button>
      <button className="btn" onClick={() => send({ cmd: "show_history" })}>Show history</button>
    </div>
  );
}

/**
 * Whether Magnetite opens links from other apps, and a button to make it so.
 * macOS asks the user itself; the card shows what the system says afterwards.
 * Windows has the user choose in its Settings, which open.
 */
function DefaultBrowserCard({ state }: { state: DefaultBrowser }) {
  const [now, setNow] = useState(state);
  const [asking, setAsking] = useState(false);
  const ask = () => {
    setAsking(true);
    send({ cmd: "make_default_browser" })
      .then((r) => r.default_browser && setNow(r.default_browser))
      .finally(() => setAsking(false));
  };
  return (
    <Field label="Default browser" hint={now === "yes" ? "Magnetite opens links from other apps." : "Links from other apps open in another browser."}>
      {now === "no" && <button className="btn" disabled={asking} onClick={ask}>{asking ? (IS_MAC ? "Waiting for macOS…" : "Opening Settings…") : "Make default"}</button>}
    </Field>
  );
}

const KEYWORD = /^[\p{L}\p{N}_-]{1,20}$/u;

/** Why a site search would not be kept (`SiteSearch::sanitised` in the app), or null. */
function siteSearchProblem(site: SiteSearch, all: SiteSearch[]): string | null {
  const keyword = site.keyword.trim().toLowerCase();
  if (!KEYWORD.test(keyword)) return "The keyword is one word of letters, digits, - or _ (up to 20).";
  if (all.find((s) => s.keyword.trim().toLowerCase() === keyword) !== site) return "Another site has this keyword.";
  if (!site.name.trim()) return "Give it a name.";
  if (!(site.url.trim().startsWith("https://") && site.url.includes("%s"))) return "The address must start with https:// and contain %s.";
  return null;
}

/**
 * The site searches: keyword, name and search address of each. Changes are
 * saved when a field is left; a row that would not be kept stays here,
 * marked, and is left out until it is fixed.
 */
function SiteSearches({ saved, save }: { saved: SiteSearch[]; save: (sites: SiteSearch[]) => void }) {
  const [rows, setRows] = useState(saved);
  const commit = (next: SiteSearch[]) => {
    const valid = next.filter((s) => !siteSearchProblem(s, next)).map((s) => ({ keyword: s.keyword.trim().toLowerCase(), name: s.name.trim(), url: s.url.trim() }));
    if (JSON.stringify(valid) !== JSON.stringify(saved)) save(valid);
  };
  const change = (i: number, patch: Partial<SiteSearch>) => setRows((list) => list.map((s, j) => (j === i ? { ...s, ...patch } : s)));
  const remove = (i: number) => {
    const next = rows.filter((_, j) => j !== i);
    setRows(next);
    commit(next);
  };
  return (
    <Field label="Site search" hint="Type a keyword and a space (or Tab) in the command bar, then what to look for: “gh rust” searches GitHub. Use %s where the query goes.">
      <div className="space-y-2">
        {rows.map((s, i) => {
          // A row just added says nothing until something is typed.
          const problem = s.keyword || s.name || s.url ? siteSearchProblem(s, rows) : null;
          return (
            <div key={i}>
              <div className="flex items-center gap-2">
                <input className="field mono" style={{ width: 88 }} value={s.keyword} placeholder="keyword" aria-label="Keyword" aria-invalid={!!problem || undefined} onChange={(e) => change(i, { keyword: e.target.value })} onBlur={() => commit(rows)} spellCheck={false} />
                <input className="field" style={{ width: 140 }} value={s.name} placeholder="Name" aria-label="Name" onChange={(e) => change(i, { name: e.target.value })} onBlur={() => commit(rows)} />
                <input className="field" style={{ flex: 1, minWidth: 0 }} value={s.url} placeholder="https://example.org/search?q=%s" aria-label="Search address" onChange={(e) => change(i, { url: e.target.value })} onBlur={() => commit(rows)} spellCheck={false} />
                <button className="ib" title="Remove" aria-label={`Remove ${s.name || "site search"}`} onClick={() => remove(i)}><Close width={14} height={14} /></button>
              </div>
              {problem && <p className="set-hint" style={{ color: "var(--danger)" }}>Not saved: {problem}</p>}
            </div>
          );
        })}
        <button className="btn" onClick={() => setRows((list) => [...list, { keyword: "", name: "", url: "" }])}>Add site</button>
      </div>
    </Field>
  );
}
