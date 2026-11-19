// Full-page settings, opened as its own tab (magnetite://ui/settings).
import { DeveloperSection } from "./settings/DeveloperSection";
import { ProtectedVideoSection } from "./settings/ProtectedVideoSection";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { AdblockIcon, Archive, Back, BracesIcon, CameraIcon, Close, CodeIcon, DevicesIcon, InfoIcon, PaletteIcon, PrivacyIcon, PuzzleIcon, Search, SettingsIcon, SharedIcon, SmartFolderIcon, type IconComponent, KeyIcon, KeyboardIcon, LinkIcon, VideoIcon } from "./Icons";
import { SEARCH_ENGINES } from "./settings/searchEngines";
import { EXTERNAL_LINKS } from "./settings/externalLinks";
import { ARCHIVE_AFTER, UNLOAD_AFTER } from "./settings/tabTimes";
import { applyAppearance, IS_MAC, keyLabel, send, type DefaultBrowser, type LinkRule, type Settings, type SiteSearch } from "./bridge";
import { About } from "./settings/AboutSection";
import { AdblockSection } from "./settings/AdblockSection";
import { AutofillSection } from "./settings/AutofillSection";
import { AppearanceSection } from "./settings/AppearanceSection";
import { ExtensionsSection } from "./settings/ExtensionsSection";
import { Field, Group, Toggle } from "./settings/fields";
import { ExportBookmarks, ImportBookmarksFile, ImportBrowsers } from "./settings/ImportBrowsers";
import { PermissionsSection } from "./settings/PermissionsSection";
import { SearchedSection, SearchResults } from "./settings/SearchResults";
import { ShortcutsSection } from "./settings/ShortcutsSection";
import { SiteCodeSection } from "./settings/SiteCodeSection";
import { profileLabel, SpacesSection } from "./settings/SpacesSection";
import { SmartFoldersSection } from "./settings/SmartFoldersSection";
import { SyncSection, type SyncJob } from "./settings/SyncSection";
import type { Info, SettingsPatch } from "./settings/types";

/** The Chrome Web Store's extensions section. */

type Section = { id: string; label: string; Icon: IconComponent; title: string; intro: string };

const GROUPS: { label: string; sections: Section[] }[] = [
  {
    label: "Basics",
    sections: [
      { id: "general", label: "General", Icon: SettingsIcon, title: "General", intro: "Windows, the command bar and the first start. No account is needed." },
      { id: "appearance", label: "Appearance", Icon: PaletteIcon, title: "Appearance", intro: "How the sidebar and Magnetite's own pages look. Web pages keep their own design." },
      { id: "shortcuts", label: "Shortcuts", Icon: KeyboardIcon, title: "Shortcuts", intro: "The keys that run Magnetite's commands. Give any of them other keys, or none." },
    ],
  },
  {
    label: "Browsing",
    sections: [
      { id: "tabs", label: "Tabs & archive", Icon: Archive, title: "Tabs & archive", intro: "A closed tab goes to the archive, and Today tabs you stop using move there on their own. It can be opened again for as long as the archive keeps it." },
      { id: "links", label: "Links", Icon: LinkIcon, title: "Links", intro: "What happens to a link you click in another app, and to a link you copy." },
      { id: "search", label: "Search", Icon: Search, title: "Search", intro: "Used when what you type in the command bar is not a web address. A keyword and a space search one site instead." },
      { id: "adblock", label: "Ad blocking", Icon: AdblockIcon, title: "Ad blocking", intro: "Magnetite can stop ads and trackers before they load, with filter lists you choose. Off until you turn it on." },
      { id: "video", label: "Protected video", Icon: VideoIcon, title: "Protected video", intro: "Some streaming services do not play in Magnetite's Chromium engine yet. Their pages open in your system's own web engine instead." },
      { id: "permissions", label: "Permissions", Icon: CameraIcon, title: "Permissions", intro: "The websites that may, or may not, use your camera, microphone or location, send notifications, open pop-ups or read the clipboard. A website asks first; this is what you answered." },
      { id: "autofill", label: "Passwords & autofill", Icon: KeyIcon, title: "Passwords & autofill", intro: "Magnetite can keep your passwords, addresses and cards and fill them in for you. What it saves stays on this computer." },
    ],
  },
  {
    label: "Spaces",
    sections: [
      { id: "spaces", label: "Spaces", Icon: SharedIcon, title: "Spaces", intro: "Your spaces: their names, order and look, and the profile each one signs in with." },
      { id: "smart", label: "Smart folders", Icon: SmartFolderIcon, title: "Smart folders", intro: "Folders that fill themselves with your pull requests, issues or tickets. Magnetite asks the service from this computer; tokens stay here and are never synced." },
    ],
  },
  {
    label: "Services",
    sections: [
      { id: "sync", label: "Sync", Icon: DevicesIcon, title: "Sync", intro: "Optional, zero-knowledge sync between your devices, through Magnetite's free server or one you host yourself. No account: a 24-word sync code is the only key." },
      { id: "extensions", label: "Extensions", Icon: PuzzleIcon, title: "Extensions", intro: "Chrome extensions install from the Chrome Web Store, right here in Magnetite. Choose which ones sit in the sidebar; all of them are in the menu bar under Extensions." },
    ],
  },
  {
    label: "Advanced",
    sections: [
      { id: "code", label: "Custom CSS & JS", Icon: BracesIcon, title: "Custom CSS & JS", intro: "Change how a site looks or behaves with CSS and JavaScript of your own: hide a part of a page, change its fonts or colours, or run a small script on it." },
      { id: "developer", label: "Developer", Icon: CodeIcon, title: "Developer", intro: "Tools for building and debugging websites." },
      { id: "data", label: "Data & privacy", Icon: PrivacyIcon, title: "Data & privacy", intro: "No telemetry, no Magnetite account. What Magnetite keeps on this computer, and the few servers it talks to." },
      { id: "about", label: "About", Icon: InfoIcon, title: "About Magnetite", intro: "A free, open-source browser built around a sidebar, on a Chromium engine, made in Europe." },
    ],
  },
];
const SECTIONS = GROUPS.flatMap((g) => g.sections);


/** The section the page's address names (General without one), and what in it to show. */
function fromHash(): { id: string; item?: string } {
  const [id, item] = location.hash.slice(1).split("=");
  return { id: SECTIONS.find((s) => s.id === id)?.id ?? "general", item: item ? decoded(item) : undefined };
}

/** `text` with its percent signs read; undefined if they do not make sense (`%` alone). */
function decoded(text: string): string | undefined {
  try {
    return decodeURIComponent(text);
  } catch {
    return undefined;
  }
}

/** The event the app sends an open Settings page when a setting changed
 * outside it (`events.rs`, `AutofillChanged`). */
const SETTINGS_CHANGED = "magnetite-settings-changed";

export function SettingsPage() {
  const [info, setInfo] = useState<Info | null>(null);
  // The address names the section, and after "=" what in it to show: `#code=example.org`.
  const [place, setPlace] = useState(fromHash);
  const sectionId = place.id;
  // What is typed into the search field; while there is any, the page shows
  // the matching settings of all sections instead of one section.
  const [query, setQuery] = useState("");
  const searching = query.trim() !== "";
  // The sections that hold a match; the others are greyed out in the list.
  const [found, setFound] = useState<string[]>([]);
  const onFound = useCallback((ids: string[]) => setFound((now) => (now.join() === ids.join() ? now : ids)), []);
  // The one section on the page while a search is typed: it alone is
  // looked at, so only its place among the sections found changes.
  const onSectionFound = useCallback(
    (hit: boolean, id: string) => setFound((now) => (hit === now.includes(id) ? now : hit ? [...now, id] : now.filter((x) => x !== id))),
    [],
  );
  // A section opened while a search is typed: the page shows it alone, with
  // what the search found still marked, until other words are typed.
  const [picked, setPicked] = useState(false);
  const type = (text: string) => {
    setQuery(text);
    setPicked(false);
  };
  // The section's name takes the keyboard when a result opens it: the
  // button that was pressed is gone with the results.
  const heading = useRef<HTMLHeadingElement>(null);
  // Back at the results, the keyboard is in the search field again: the
  // button that led back is gone with the section.
  const searchField = useRef<HTMLInputElement>(null);
  const toHeading = useRef(false);
  useEffect(() => {
    if (toHeading.current && picked) heading.current?.focus();
    toHeading.current = false;
  }, [picked, sectionId]);
  // `item`: what in the section to show (a space's own page).
  const openSection = (id: string, item?: string) => {
    toHeading.current = searching && !picked;
    setPicked(true);
    setPlace({ id, item });
    // The address follows without a `hashchange`: that event is the app,
    // or Back, sending the page elsewhere.
    const hash = item ? `#${id}=${encodeURIComponent(item)}` : `#${id}`;
    if (location.hash !== hash) history.pushState(null, "", hash);
    // A page of its own starts at its top.
    document.querySelector("main")?.scrollTo(0, 0);
  };
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
      // No section named (Back to the address Settings opened with) is General.
      if (location.hash !== "" && !SECTIONS.some((s) => `#${s.id}` === location.hash.split("=")[0])) return;
      // The section asked for, not what a search was showing.
      setQuery("");
      setPicked(false);
      setPlace(fromHash());
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
  // whenever the page comes back. The one change the app does tell about is
  // a setting changed on one of Chromium's own pages (`SETTINGS_CHANGED`).
  useEffect(() => {
    const back = () => {
      if (document.visibilityState === "visible") void reload();
    };
    window.addEventListener("focus", back);
    window.addEventListener(SETTINGS_CHANGED, back);
    document.addEventListener("visibilitychange", back);
    return () => {
      window.removeEventListener("focus", back);
      window.removeEventListener(SETTINGS_CHANGED, back);
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
  // A space's own page is headed by the space.
  const ownSpace = sectionId === "spaces" && place.item ? info.spaces.find((sp) => sp.id === place.item) : undefined;
  // One section is on the page: always, but for the list of what a search found.
  const shown = !searching || picked;
  /** The settings of one section. */
  const body = (sectionId: string) => (
    <>
      {sectionId === "general" && (
        <>
          <Group title="Windows">
          <Toggle label="Several windows" hint="New Window opens another window on the same sidebar; choosing a tab in one window moves it there. Windows open again after a restart. New, and untested on macOS and Windows." checked={settings.several_windows} onChange={(v) => update({ several_windows: v })} />
          <Toggle label="Web panels close by themselves" hint="A web panel (a favorite with &quot;Open as panel&quot;) closes when you click into the page behind it. Off: it stays until you close it." checked={settings.panel_autoclose} onChange={(v) => update({ panel_autoclose: v })} />
          <Toggle label="Pop out playing videos" hint="When you leave a tab with a playing video, it continues in a floating picture-in-picture window. Coming back puts it into the page again." checked={settings.auto_pip} onChange={(v) => update({ auto_pip: v })} />
          </Group>
          <Group title="Downloads">
            {/* Each sends its own part only: the other one may be newer than this page knows. */}
            <DownloadFolder folder={settings.downloads.folder} missing={info.download_folder_missing} reset={() => send({ cmd: "set_settings", settings: { downloads: { folder: "" } } }).then(reload)} chosen={reload} />
            <Toggle label="Ask where to save each file" hint="A dialog for every download, to name the file and its folder." checked={settings.downloads.ask} onChange={(ask) => void send({ cmd: "set_settings", settings: { downloads: { ask } } }).then(reload, reload).catch(() => {})} />
          </Group>
          <Group title="Text and languages">
            <Toggle label="Check spelling as you type" hint="Marks words the spell checker does not know, in the text fields of web pages." checked={settings.languages.spellcheck} onChange={(spellcheck) => void send({ cmd: "set_settings", settings: { languages: { spellcheck } } }).then(reload, reload).catch(() => {})} />
            <WebLanguages key={settings.languages.languages.join()} saved={settings.languages.languages} save={(languages) => send({ cmd: "set_settings", settings: { languages: { languages } } }).then(() => void reload().catch(() => undefined))} />
          </Group>
          <Group title="Command bar">
          <Toggle label={`Quick search from anywhere (${keyLabel("⌥⌘N")})`} hint="Opens the command bar over any app; what you pick opens in a mini window. Only that key combination reaches Magnetite, no other typing." checked={settings.quick_search} onChange={(v) => update({ quick_search: v })} />
          </Group>
          <Group title="First start">
          <Field side label="Welcome page" hint="The questions of the first start, once more. Nothing you have set changes until you choose something there.">
            <button className="btn" onClick={() => void send({ cmd: "new_tab", url: "magnetite://ui/welcome" })}>Show it again</button>
          </Field>
          </Group>
        </>
      )}
      {sectionId === "links" && (
        <>
          <Group title="From other apps">
          {info.default_browser !== "unknown" && <DefaultBrowserCard key={info.default_browser} state={info.default_browser} />}
          <Field side label="Open links from other apps in" hint="Links from Mail, Slack and other apps, when Magnetite is your default browser.">
            <select className="field" value={settings.external_links} onChange={(e) => update({ external_links: e.target.value as Settings["external_links"] })}>
              {EXTERNAL_LINKS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
            </select>
          </Field>
          <LinkRules saved={settings.link_rules} spaces={info.spaces} save={(rules) => update({ link_rules: rules })} />
          </Group>
          <Group title="Copied links">
          <Toggle label="Remove trackers from copied links" hint="Copy Link drops tracking parameters such as utm_source and fbclid." checked={settings.strip_trackers} onChange={(v) => update({ strip_trackers: v })} />
          </Group>
        </>
      )}
      {sectionId === "tabs" && (
        <>
          <Group title="Tidying up" hint="What Magnetite does with tabs you leave alone. An archived tab can be opened again for as long as the archive keeps it.">
          <Field side label="Archive Today tabs after" hint="Pinned tabs and favorites are left where they are.">
            <select className="field" value={settings.auto_archive_hours} onChange={(e) => update({ auto_archive_hours: Number(e.target.value) })}>
              {ARCHIVE_AFTER.map((t) => <option key={t.hours} value={t.hours}>{t.label}</option>)}
            </select>
          </Field>
          <ArchiveRetention key={settings.archive_days} days={settings.archive_days} save={(days) => update({ archive_days: days })} />
          <Field side label="Suspend inactive tabs after" hint="Suspended tabs keep their place and reload when opened. This is what keeps hundreds of tabs cheap.">
            <select className="field" value={settings.unload_minutes} onChange={(e) => update({ unload_minutes: Number(e.target.value) })}>
              {UNLOAD_AFTER.map((t) => <option key={t.minutes} value={t.minutes}>{t.label}</option>)}
            </select>
          </Field>
          <Field side label="Archived tabs" hint={`${info.archive_count} so far. Search them from the command bar or the archive view in the sidebar.`}>
            <button className="btn" onClick={() => send({ cmd: "reopen_closed" })}>Reopen last archived tab</button>
          </Field>
          </Group>
          <Group title="Getting around">
          <Field side label="Ctrl+Tab goes through the tabs" hint="By last use: one press goes to the tab used before this one; hold Ctrl and press Tab again to go further back. In sidebar order: down the sidebar, row by row.">
            <div className="seg">
              {([["recent", "By last use"], ["sidebar", "In sidebar order"]] as const).map(([order, label]) => (
                <button key={order} data-on={settings.tab_order === order || undefined} aria-pressed={settings.tab_order === order} onClick={() => update({ tab_order: order })}>{label}</button>
              ))}
            </div>
          </Field>
          <Field side label="Clicking the archive button opens" hint="The button at the bottom left of the sidebar; resting on it shows bookmarks, history, downloads and the archive. While a download runs, a click opens downloads.">
            <select className="field" value={settings.library_opens} onChange={(e) => update({ library_opens: e.target.value as Settings["library_opens"] })}>
              <option value="archive">the archive</option>
              <option value="downloads">downloads</option>
              <option value="bookmarks">bookmarks</option>
              <option value="history">history</option>
            </select>
          </Field>
          </Group>
        </>
      )}
      {sectionId === "appearance" && <AppearanceSection info={info} update={update} open={openSection} />}
      {sectionId === "smart" && <SmartFoldersSection />}
      {sectionId === "autofill" && <AutofillSection info={info} update={update} />}
      {sectionId === "adblock" && <AdblockSection info={info} update={update} reload={reload} />}
      {sectionId === "video" && <ProtectedVideoSection key={settings.system_engine.sites.join()} info={info} update={update} />}
      {sectionId === "search" && (
        <>
          <Group>
          <Field side label="Search engine" hint="Nothing goes to it until you search.">
            {/* Found by the names of the engines too, which only the list holds. */}
            <span hidden data-search-name>{SEARCH_ENGINES.map((e) => e.label).join(" ")}</span>
            <select className="field" aria-label="Search engine" value={SEARCH_ENGINES.some((e) => e.template === settings.search_template) ? settings.search_template : ""} onChange={(e) => e.target.value && update({ search_template: e.target.value })}>
              {SEARCH_ENGINES.map((e) => <option key={e.template} value={e.template}>{e.label}</option>)}
              {!SEARCH_ENGINES.some((e) => e.template === settings.search_template) && <option value="">Your own address</option>}
            </select>
          </Field>
          <SearchAddress key={settings.search_template} saved={settings.search_template} save={(template) => update({ search_template: template })} />
          </Group>
          <Group>
          <SiteSearches saved={settings.site_searches} save={(sites) => update({ site_searches: sites })} />
          </Group>
        </>
      )}
      {sectionId === "permissions" && <PermissionsSection info={info} />}
      {sectionId === "spaces" && <SpacesSection info={info} update={update} reload={reload} item={place.item} open={openSection} />}
      {sectionId === "developer" && (
        <>
          <Group>
        <Field label="Developer tools open" hint="With F12 or from the site panel. Docked tools sit next to the page inside the window.">
          <div className="seg">
            {([["right", "Right of the page"], ["bottom", "Below the page"], ["window", "In a separate window"]] as const).map(([v, label]) => (
              <button key={v} data-on={settings.devtools_dock === v || undefined} onClick={() => update({ devtools_dock: v })}>{label}</button>
            ))}
          </div>
        </Field>
        <DeveloperSection key={settings.developer.hosts.join()} settings={settings} update={update} />
          </Group>
        </>
      )}
      {sectionId === "extensions" && (
        <>
          <ExtensionsSection info={info} reload={reload} />
          <Group title="Inside pages">
          <Toggle label="Show the Chrome toolbar inside pages" hint="Chromium's own toolbar row above every page, with its extension buttons. Applies to tabs loaded from now on." checked={settings.chrome_toolbar} onChange={(v) => update({ chrome_toolbar: v })} />
          </Group>
        </>
      )}
      {sectionId === "sync" && <SyncSection info={info} reload={reload} job={syncJob} setJob={setSyncJob} />}
      {sectionId === "data" && (
        <>
          <Group title="Browsing data">
          <Field side label="Keep history for" hint="The pages you visit, with title and time, to find them again. Kept on this computer only, never synced; private profiles leave none.">
            <select className="field" value={settings.history_days} onChange={(e) => update({ history_days: Number(e.target.value) })}>
              {[7, 30, 90].includes(settings.history_days) || settings.history_days === 0 ? null : <option value={settings.history_days}>{settings.history_days} days</option>}
              <option value={7}>7 days</option><option value={30}>30 days</option><option value={90}>90 days</option><option value={0}>Don't keep history</option>
            </select>
          </Field>
          <Field side label="Visited pages" hint={`${info.history_count} so far. Single pages can be removed in the history view.`}>
            <ClearHistory key={info.history_count} disabled={info.history_count === 0} onClear={() => send({ cmd: "clear_history" }).then(() => void reload())} />
          </Field>
            <ClearBrowsingData info={info} reload={reload} />
          </Group>
          <Group title="Import">
          <ImportBrowsers />
          <ImportBookmarksFile spaces={info.spaces} current={info.active_space} />
          <ExportBookmarks spaces={info.spaces} />
          </Group>
          <Group title="What websites are told">
            <Toggle label="Tell websites not to sell or share my data" hint="Global Privacy Control: every request to a website carries the signal, and a page can read it in its scripts. Some laws oblige a website to honour it; whether one does is up to the website." checked={settings.global_privacy_control} onChange={(v) => update({ global_privacy_control: v })} />
          </Group>
          <Group title="What Magnetite talks to" hint="Nothing else leaves this computer: no telemetry, no usage data, no account.">
            <ul className="set-list set-talks">
              <li data-search-row><b>The sites you open</b><span>and what their pages load, less what ad blocking stops.</span></li>
              <li data-search-row><b>Services you connect</b><span>your sync server, and the servers of your smart folders.</span></li>
              <li data-search-row><b>Publishers of filter lists</b><span>for the ad blocking lists you choose, every four days.</span></li>
              <li data-search-row><b>GitHub</b><span>for update checks, at start and once a day, unless turned off in About. Not where your system's packages bring the updates.</span></li>
              <li data-search-row><b>Google's component server</b><span>from which Chromium fetches the Widevine module for protected video and its certificate lists.</span></li>
              <li data-search-row><b>Google's password check</b><span>only while "Warn about passwords from data breaches" is on: an encrypted fragment of a password you sign in with.</span></li>
            </ul>
          </Group>
          <Group title="On this computer">
          <Field label="Where your data lives" hint="The sidebar, archive and settings are in state.json, the history in history.json; Chromium's profile data (cookies, cache, extensions) in the cef folder next to it.">
            <code className="block rounded-lg bg-white/5 px-3 py-2 text-[12px] text-[var(--secondary)]">{info.data_dir}</code>
          </Field>
          {info.keychain === "test" && (
            <Field
              label="Development mode: cookies use a fixed key"
              hint="This data folder runs with --mock-keychain: Chromium encrypts cookies and saved passwords with its built-in test key instead of one in your macOS Keychain, so any program that can read the folder can read them. Fine while developing; a signed release build uses the Keychain."
            />
          )}
          </Group>
        </>
      )}
      {sectionId === "shortcuts" && <ShortcutsSection info={info} reload={() => void reload()} />}
      {sectionId === "code" && <SiteCodeSection key={place.item} saved={settings.site_code} open={place.item} save={(code) => update({ site_code: code })} />}
      {sectionId === "about" && <About info={info} update={update} />}
    </>
  );

  return (
    <div className="set-page" style={{ background: "var(--bg)" }}>
      <div className="set-layout">
      <nav className="set-nav scroll w-[232px] shrink-0 border-r border-[var(--line)] p-3">
        <div className="set-nav-title px-2 pb-2 pt-1 text-[16px] font-semibold">Settings</div>
        <label className="set-search">
          <Search width={14} height={14} />
          <input ref={searchField} type="search" className="field" placeholder="Search settings" aria-label="Search settings" value={query} onChange={(e) => type(e.target.value)} onKeyDown={(e) => e.key === "Escape" && type("")} spellCheck={false} />
        </label>
        {/* The sections as one list, for a page too narrow for them at its side. */}
        <select className="field set-nav-pick" aria-label="Section" value={shown ? sectionId : ""} onChange={(e) => openSection(e.target.value)}>
          {!shown && <option value="" disabled>Search results</option>}
          {GROUPS.map((g) => (
            <optgroup key={g.label} label={g.label}>
              {g.sections.map((s) => <option key={s.id} value={s.id}>{s.label}</option>)}
            </optgroup>
          ))}
        </select>
        <div className="set-nav-list">
        {GROUPS.map((g) => (
          <div key={g.label} className="mt-3">
            <div className="set-nav-label">{g.label}</div>
            {g.sections.map((s) => (
              <button key={s.id} className="row w-full" data-dim={(searching && !found.includes(s.id)) || undefined} data-active={(shown && sectionId === s.id) || undefined} onClick={() => openSection(s.id)}>
                <span className="icon-slot" style={{ color: shown && sectionId === s.id ? "var(--accent)" : "var(--muted)" }}><s.Icon width={16} height={16} /></span>
                <span className="title text-left">{s.label}</span>
              </button>
            ))}
          </div>
        ))}
        </div>
      </nav>
      <main className="set-main scroll min-h-0 min-w-0 flex-1 px-10 py-8">
        <div className="max-w-[680px]">
          {searching && !picked ? (
            <SearchResults query={query} sections={SECTIONS} body={body} open={openSection} onFound={onFound} none={found.length === 0} />
          ) : (
            <>
              <header className="mb-6 flex items-start gap-4">
                <span className="set-icon"><section.Icon width={22} height={22} /></span>
                <div>
                  <h1 ref={heading} tabIndex={-1} className="text-[22px] font-semibold tracking-[-0.01em] outline-none">{ownSpace?.title ?? section.title}</h1>
                  <p className="mt-1 max-w-[560px] text-[14px] leading-5 text-[var(--muted)]">{section.intro}</p>
                </div>
              </header>
              {searching && (
                <button className="set-found" onClick={() => { setPicked(false); searchField.current?.focus(); }} title="Back to everything the search found">
                  <Back width={15} height={15} />
                  Everything for “{query.trim()}”
                </button>
              )}
              <SearchedSection key={sectionId} query={searching ? query : ""} section={section} onFound={searching ? onSectionFound : undefined}>{body(sectionId)}</SearchedSection>
            </>
          )}
        </div>
      </main>
      </div>
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
    <Field side label="Keep archived tabs for" hint="Older ones are deleted from the archive for good. A tab can also be removed by hand in the archive view.">
      <div className="flex flex-wrap items-center gap-2">
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
    <Field side label="Default browser" hint={now === "yes" ? "Magnetite opens links from other apps." : "Links from other apps open in another browser."}>
      {now === "no" && <button className="btn" disabled={asking} onClick={ask}>{asking ? (IS_MAC ? "Waiting for macOS…" : "Opening Settings…") : "Make default"}</button>}
    </Field>
  );
}

/**
 * The folder downloads are saved to: the chosen one, or the system's.
 * "Choose…" opens the system's dialog; the app keeps what was chosen there.
 */
function DownloadFolder({ folder, missing, reset, chosen }: { folder: string; missing: boolean; reset: () => Promise<unknown>; chosen: () => Promise<unknown> }) {
  // Busy until the page shows what the app has now; a change or a read
  // that failed is said.
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const run = (change: Promise<unknown>) => {
    setBusy(true);
    setFailed(false);
    change.catch(() => setFailed(true)).finally(() => setBusy(false));
  };
  const choose = () => run(send({ cmd: "choose_download_folder" }).then(chosen));
  return (
    <Field side label="Save downloads to" hint={folder ? folder : "The Downloads folder of your system."}>
      <div className="flex gap-2">
        {folder && <button className="btn ghost" disabled={busy} onClick={() => run(reset())}>Reset</button>}
        <button className="btn" disabled={busy} onClick={choose}>Choose…</button>
      </div>
      {missing && folder && <p className="set-hint" style={{ color: "var(--warn)" }}>This folder is not there right now.</p>}
      {failed && <p className="set-hint" style={{ color: "var(--danger)" }}>That did not work. What shows here may be out of date; try again.</p>}
    </Field>
  );
}

/**
 * Deleting what websites left behind: Chromium's own dialog, for the logins
 * chosen here, to start with those of the space in front.
 */
function ClearBrowsingData({ info, reload }: { info: Info; reload: () => Promise<unknown> }) {
  const logins = info.profiles.filter((p) => !p.ephemeral);
  const inFront = info.spaces.find((s) => s.id === info.active_space)?.profile_id;
  const [chosen, setChosen] = useState<string | undefined>(undefined);
  // The logins chosen while they still exist (another window may have
  // deleted them), else those of the space in front, else the main ones.
  const profile = (logins.find((p) => p.id === chosen) ?? logins.find((p) => p.id === inFront) ?? logins.find((p) => p.is_default))?.id;
  // The app refuses logins deleted since this page last read them: say so,
  // and read again so the list offers those that are there.
  const [failed, setFailed] = useState(false);
  const open = () => {
    setFailed(false);
    send({ cmd: "open_engine_page", page: "clear_data", profile }).catch(() => {
      setFailed(true);
      void reload();
    });
  };
  return (
    <Field side label="Cookies, site data and cache" hint="Delete what websites have stored on this computer, for the last hour or for all time. Deleting cookies signs you out of websites. The dialog is Chromium's own; the history it names is not the one above.">
      <div className="flex flex-wrap gap-2">
        {logins.length > 1 && (
          <select className="field" value={profile} onChange={(e) => setChosen(e.target.value)} aria-label="Profile">
            {logins.map((p) => <option key={p.id} value={p.id}>{profileLabel(info, p)}</option>)}
          </select>
        )}
        <button className="btn" disabled={!profile} onClick={open}>Delete…</button>
      </div>
      {failed && <p className="set-hint" role="alert" style={{ color: "var(--danger)" }}>The dialog could not be opened for that profile. Choose again.</p>}
    </Field>
  );
}

/** A language tag as the app keeps one (`is_tag` in the engine's `languages.rs`). */
const LANGUAGE_TAG = /^[a-z]{2,3}(-[a-z]{4})?(-([a-z]{2}|\d{3}))?(-([a-z0-9]{5,8}|\d[a-z0-9]{3}))*$/i;
/** The most languages the app keeps (`MAX_LANGUAGES`). */
const MAX_LANGUAGES = 10;

/**
 * The languages websites are asked for, typed as tags in the order of
 * preference and saved when the field is left. Empty means the system's.
 */
function WebLanguages({ saved, save }: { saved: string[]; save: (languages: string[]) => Promise<unknown> }) {
  const [text, setText] = useState(saved.join(", "));
  // The app did not take the list: the field says so and keeps what was typed.
  const [refused, setRefused] = useState(false);
  // As the app keeps them: each once, whatever its case, and no more than it takes.
  const known = new Set<string>();
  const typed = text.split(/[\s,;]+/).filter((tag) => {
    const lower = tag.toLowerCase();
    if (!tag || known.has(lower)) return false;
    known.add(lower);
    return true;
  });
  const many = typed.length > MAX_LANGUAGES;
  // No language code, or one that names a variant twice.
  const twice = (tag: string) => tag.toLowerCase().split("-").some((part, i, all) => part.length > 3 && all.indexOf(part) !== i);
  const wrong = typed.filter((t) => !LANGUAGE_TAG.test(t) || twice(t));
  return (
    <Field label="Languages websites are asked for" hint="In the order you prefer them, as language codes: de, en-US, en. A website with several languages can choose by them. Empty: the languages of your system. A change counts from the next start of Magnetite.">
      <input
        className="field mono"
        value={text}
        placeholder="de, en-US, en"
        aria-label="Languages websites are asked for"
        aria-invalid={wrong.length > 0 || many || undefined}
        spellCheck={false}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => {
          setRefused(false);
          if (wrong.length === 0 && !many && typed.join() !== saved.join()) save(typed).catch(() => setRefused(true));
        }}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      />
      {wrong.length > 0 && <p className="set-hint" style={{ color: "var(--danger)" }}>Not saved: {wrong.join(", ")} {wrong.length === 1 ? "is no language code" : "are no language codes"}.</p>}
      {wrong.length === 0 && many && <p className="set-hint" style={{ color: "var(--danger)" }}>Not saved: no more than {MAX_LANGUAGES} languages.</p>}
      {/* Said aloud: it comes after the field was left. */}
      {refused && <p className="set-hint" role="alert" style={{ color: "var(--danger)" }}>Not saved: that did not work. Leave the field again to try once more.</p>}
    </Field>
  );
}

/** The most rules the app keeps (`MAX_LINK_RULES`). */
const MAX_LINK_RULES = 100;

/**
 * A rule's site as the app keeps it (`LinkRule::sanitised`): of what was
 * typed, which may be a whole address, the host without `www.` and the
 * path. Null without a host.
 */
function ruleSite(typed: string): string | null {
  const rest = typed.trim().replace(/^[a-z][a-z0-9+.-]*:\/\//i, "").split(/[?#]/)[0] ?? "";
  const slash = rest.indexOf("/");
  const host = (slash < 0 ? rest : rest.slice(0, slash)).split("@").pop()!.split(":")[0]!.replace(/^www\./i, "").toLowerCase();
  const path = slash < 0 ? "" : rest.slice(slash).replace(/\/+$/, "");
  return /^[a-z0-9.-]+$/.test(host) && host.includes(".") ? host + path : null;
}

/**
 * The rules for links from other apps: a site and the space its links open
 * in. Changes are saved when a field is left or a space chosen; a row that
 * would not be kept stays here, marked, and is left out until it is fixed.
 */
function LinkRules({ saved, spaces, save }: { saved: LinkRule[]; spaces: Info["spaces"]; save: (rules: LinkRule[]) => void }) {
  const [rows, setRows] = useState(saved);
  // What this editor knows to be saved. Saved rules that differ were changed
  // elsewhere (another Settings tab): they replace the rows, drafts included.
  const known = useRef(JSON.stringify(saved));
  useEffect(() => {
    const now = JSON.stringify(saved);
    if (now === known.current) return;
    known.current = now;
    setRows(saved);
  }, [saved]);
  const problem = (rule: LinkRule, all: LinkRule[]): string | null => {
    const site = ruleSite(rule.site);
    if (!site) return "Give a site, such as github.com or github.com/acme.";
    if (all.find((r) => ruleSite(r.site) === site) !== rule) return "Another rule has this site.";
    if (!spaces.some((s) => s.id === rule.space)) return "Choose a space; the one this rule had is gone.";
    return null;
  };
  const commit = (next: LinkRule[]) => {
    const valid = next.filter((r) => !problem(r, next)).map((r) => ({ site: ruleSite(r.site)!, space: r.space }));
    if (JSON.stringify(valid) === JSON.stringify(saved)) return;
    known.current = JSON.stringify(valid);
    save(valid);
  };
  const change = (i: number, patch: Partial<LinkRule>) => {
    const next = rows.map((r, j) => (j === i ? { ...r, ...patch } : r));
    setRows(next);
    return next;
  };
  return (
    <Field label="Links to these sites open in a space of their own" hint="A link from another app to a site listed here opens in its space, in that space's profile. A site stands for its subdomains; github.com/acme is only that part of GitHub. The most exact rule counts.">
      <div className="space-y-2">
        {rows.map((r, i) => {
          // A row just added says nothing until something is typed.
          const why = r.site ? problem(r, rows) : null;
          return (
            <div key={i}>
              <div className="flex flex-wrap items-center gap-2">
                <input className="field mono" style={{ flex: "1 1 160px", minWidth: 0 }} value={r.site} placeholder="github.com/acme" aria-label="Site" aria-invalid={!!why || undefined} onChange={(e) => change(i, { site: e.target.value })} onBlur={() => commit(rows)} />
                <span className="text-[13px] text-[var(--muted)]">opens in</span>
                <select className="field" style={{ width: 180 }} value={spaces.some((s) => s.id === r.space) ? r.space : ""} aria-label="Space" onChange={(e) => commit(change(i, { space: e.target.value }))}>
                  <option value="" disabled>a space…</option>
                  {spaces.map((s) => <option key={s.id} value={s.id}>{s.title}</option>)}
                </select>
                <button className="ib" title="Remove" aria-label={`Remove the rule for ${r.site || "a site"}`} onClick={() => { const next = rows.filter((_, j) => j !== i); setRows(next); commit(next); }}><Close width={14} height={14} /></button>
              </div>
              {why && <p className="set-hint" style={{ color: "var(--danger)" }}>Not saved: {why}</p>}
            </div>
          );
        })}
        <button className="btn" disabled={rows.length >= MAX_LINK_RULES} title={rows.length >= MAX_LINK_RULES ? `No more than ${MAX_LINK_RULES} rules are kept.` : undefined} onClick={() => setRows((list) => [...list, { site: "", space: spaces[0]?.id ?? "" }])}>Add rule</button>
      </div>
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
              <div className="flex flex-wrap items-center gap-2">
                <input className="field mono" style={{ width: 88 }} value={s.keyword} placeholder="keyword" aria-label="Keyword" aria-invalid={!!problem || undefined} onChange={(e) => change(i, { keyword: e.target.value })} onBlur={() => commit(rows)} spellCheck={false} />
                <input className="field" style={{ width: 140 }} value={s.name} placeholder="Name" aria-label="Name" onChange={(e) => change(i, { name: e.target.value })} onBlur={() => commit(rows)} />
                <input className="field" style={{ flex: "1 1 160px", minWidth: 0 }} value={s.url} placeholder="https://example.org/search?q=%s" aria-label="Search address" onChange={(e) => change(i, { url: e.target.value })} onBlur={() => commit(rows)} spellCheck={false} />
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
