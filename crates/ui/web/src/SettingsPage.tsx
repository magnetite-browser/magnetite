// Full-page settings, opened as its own tab (magnetite://ui/settings).
import { DeveloperSection } from "./settings/DeveloperSection";
import { ProtectedVideoSection } from "./settings/ProtectedVideoSection";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { AdblockIcon, Archive, Back, BookmarkIcon, BracesIcon, CameraIcon, Close, CodeIcon, CookieIcon, DevicesIcon, InfoIcon, PaletteIcon, Pin, PrivacyIcon, PuzzleIcon, Search, SettingsIcon, SharedIcon, SmartFolderIcon, type IconComponent, KeyIcon, KeyboardIcon, LinkIcon, VideoIcon } from "./Icons";
import { SEARCH_ENGINES } from "./settings/searchEngines";
import { EXTERNAL_LINKS } from "./settings/externalLinks";
import { ARCHIVE_AFTER, UNLOAD_AFTER } from "./settings/tabTimes";
import { accentCss, applyAppearance, IS_MAC, keyLabel, send, type DefaultBrowser, type LinkRule, type Settings, type SiteSearch } from "./bridge";
import { About } from "./settings/AboutSection";
import { AdblockSection } from "./settings/AdblockSection";
import { AutofillSection } from "./settings/AutofillSection";
import { AppearanceSection } from "./settings/AppearanceSection";
import { ExtensionsSection } from "./settings/ExtensionsSection";
import { Field, Group, Toggle } from "./settings/fields";
import { SpaceIcon } from "./Glyphs";
import { BookmarksSection } from "./settings/BookmarksSection";
import { ImportBrowsers } from "./settings/ImportBrowsers";
import { LanguageField } from "./settings/LanguageField";
import { PickList } from "./settings/PickList";
import { PermissionsSection } from "./settings/PermissionsSection";
import { SearchedSection, SearchResults } from "./settings/SearchResults";
import { ShortcutsSection } from "./settings/ShortcutsSection";
import { SiteCodeSection } from "./settings/SiteCodeSection";
import { FavoritesSection, ProfilePick, ProfilesSection, spaceOptions, SpacesSection } from "./settings/SpacesSection";
import { SmartFoldersSection } from "./settings/SmartFoldersSection";
import { SyncSection, type SyncJob } from "./settings/SyncSection";
import type { Info, SettingsPatch } from "./settings/types";
import { t } from "./i18n";

/** The Chrome Web Store's extensions section. */

type Section = { id: string; label: string; Icon: IconComponent; title: string; intro: string };

/** The groups of the list; the first one, the overall settings, has no name. */
const GROUPS: { label: string; sections: Section[] }[] = [
  {
    label: "",
    sections: [
      { id: "general", label: t("settings.sections.general.label"), Icon: SettingsIcon, title: t("settings.sections.general.label"), intro: t("settings.sections.general.intro") },
      { id: "appearance", label: t("settings.sections.appearance.label"), Icon: PaletteIcon, title: t("settings.sections.appearance.label"), intro: t("settings.sections.appearance.intro") },
      { id: "shortcuts", label: t("settings.sections.shortcuts.label"), Icon: KeyboardIcon, title: t("settings.sections.shortcuts.label"), intro: t("settings.sections.shortcuts.intro") },
    ],
  },
  {
    label: t("settings.groups.browsing"),
    sections: [
      { id: "tabs", label: t("settings.sections.tabs.label"), Icon: Archive, title: t("settings.sections.tabs.label"), intro: t("settings.sections.tabs.intro") },
      { id: "links", label: t("settings.sections.links.label"), Icon: LinkIcon, title: t("settings.sections.links.label"), intro: t("settings.sections.links.intro") },
      { id: "search", label: t("settings.sections.search.label"), Icon: Search, title: t("settings.sections.search.label"), intro: t("settings.sections.search.intro") },
      { id: "adblock", label: t("settings.sections.adblock.label"), Icon: AdblockIcon, title: t("settings.sections.adblock.label"), intro: t("settings.sections.adblock.intro") },
      { id: "video", label: t("settings.sections.video.label"), Icon: VideoIcon, title: t("settings.sections.video.label"), intro: t("settings.sections.video.intro") },
      { id: "permissions", label: t("settings.sections.permissions.label"), Icon: CameraIcon, title: t("settings.sections.permissions.label"), intro: t("settings.sections.permissions.intro") },
      { id: "autofill", label: t("settings.sections.autofill.label"), Icon: KeyIcon, title: t("settings.sections.autofill.label"), intro: t("settings.sections.autofill.intro") },
      { id: "code", label: t("settings.sections.code.label"), Icon: BracesIcon, title: t("settings.sections.code.label"), intro: t("settings.sections.code.intro") },
    ],
  },
  {
    label: t("settings.groups.organization"),
    sections: [
      { id: "spaces", label: t("settings.sections.spaces.label"), Icon: SharedIcon, title: t("settings.sections.spaces.label"), intro: t("settings.sections.spaces.intro") },
      { id: "profiles", label: t("settings.sections.profiles.label"), Icon: CookieIcon, title: t("settings.sections.profiles.label"), intro: t("settings.sections.profiles.intro") },
      { id: "favorites", label: t("settings.sections.favorites.label"), Icon: Pin, title: t("settings.sections.favorites.label"), intro: t("settings.sections.favorites.intro") },
      { id: "bookmarks", label: t("settings.sections.bookmarks.label"), Icon: BookmarkIcon, title: t("settings.sections.bookmarks.label"), intro: t("settings.sections.bookmarks.intro") },
      { id: "smart", label: t("settings.sections.smart.label"), Icon: SmartFolderIcon, title: t("settings.sections.smart.label"), intro: t("settings.sections.smart.intro") },
    ],
  },
  {
    label: t("settings.groups.services"),
    sections: [
      { id: "sync", label: t("settings.sections.sync.label"), Icon: DevicesIcon, title: t("settings.sections.sync.label"), intro: t("settings.sections.sync.intro") },
      { id: "extensions", label: t("settings.sections.extensions.label"), Icon: PuzzleIcon, title: t("settings.sections.extensions.label"), intro: t("settings.sections.extensions.intro") },
    ],
  },
  {
    label: t("settings.groups.advanced"),
    sections: [
      { id: "developer", label: t("settings.sections.developer.label"), Icon: CodeIcon, title: t("settings.sections.developer.label"), intro: t("settings.sections.developer.intro") },
      { id: "data", label: t("settings.sections.data.label"), Icon: PrivacyIcon, title: t("settings.sections.data.label"), intro: t("settings.sections.data.intro") },
      { id: "about", label: t("settings.sections.about.label"), Icon: InfoIcon, title: t("settings.sections.about.title"), intro: t("settings.sections.about.intro") },
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
    document.title = t("common.settings");
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
  // Text size and density of this page, as saved.
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
  // Ctrl+Tab is the Control key on a Mac too; elsewhere that key has its name in the page's language.
  const ctrl = IS_MAC ? "Ctrl" : t("settings.keys.ctrl");
  /** The settings of one section. */
  const body = (sectionId: string) => (
    <>
      {sectionId === "general" && (
        <>
          <Group title={t("settings.general.windows.title")}>
          <Toggle label={t("settings.general.windows.several.label")} hint={t("settings.general.windows.several.hint")} checked={settings.several_windows} onChange={(v) => update({ several_windows: v })} />
          {settings.several_windows && <Toggle label={t("settings.general.windows.private_window.label")} hint={t("settings.general.windows.private_window.hint")} checked={settings.private_window} onChange={(v) => update({ private_window: v })} />}
          <Toggle label={t("settings.general.windows.panel_autoclose.label")} hint={t("settings.general.windows.panel_autoclose.hint")} checked={settings.panel_autoclose} onChange={(v) => update({ panel_autoclose: v })} />
          <Toggle label={t("settings.general.windows.auto_pip.label")} hint={t("settings.general.windows.auto_pip.hint")} checked={settings.auto_pip} onChange={(v) => update({ auto_pip: v })} />
          </Group>
          <Group title={t("settings.general.downloads.title")}>
            {/* Each sends its own part only: the other one may be newer than this page knows. */}
            <DownloadFolder folder={settings.downloads.folder} missing={info.download_folder_missing} reset={() => send({ cmd: "set_settings", settings: { downloads: { folder: "" } } }).then(reload)} chosen={reload} />
            <Toggle label={t("settings.general.downloads.ask.label")} hint={t("settings.general.downloads.ask.hint")} checked={settings.downloads.ask} onChange={(ask) => void send({ cmd: "set_settings", settings: { downloads: { ask } } }).then(reload, reload).catch(() => {})} />
          </Group>
          <Group title={t("settings.general.languages.title")}>
            <LanguageField chosen={settings.language} system={info.system_language} save={(language) => update({ language })} />
            <Toggle label={t("settings.general.languages.spellcheck.label")} hint={t("settings.general.languages.spellcheck.hint")} checked={settings.languages.spellcheck} onChange={(spellcheck) => void send({ cmd: "set_settings", settings: { languages: { spellcheck } } }).then(reload, reload).catch(() => {})} />
            <WebLanguages key={settings.languages.languages.join()} saved={settings.languages.languages} save={(languages) => send({ cmd: "set_settings", settings: { languages: { languages } } }).then(() => void reload().catch(() => undefined))} />
          </Group>
          <Group title={t("settings.general.command_bar.title")}>
          <Toggle label={t("settings.general.command_bar.quick_search.label", { keys: keyLabel("⌥⌘N") })} hint={t("settings.general.command_bar.quick_search.hint")} checked={settings.quick_search} onChange={(v) => update({ quick_search: v })} />
          </Group>
          <Group title={t("settings.general.first_start.title")}>
          <Field side label={t("settings.general.first_start.welcome.label")} hint={t("settings.general.first_start.welcome.hint")}>
            <button className="btn" onClick={() => void send({ cmd: "new_tab", url: "magnetite://ui/welcome" })}>{t("settings.general.first_start.welcome.show")}</button>
          </Field>
          </Group>
        </>
      )}
      {sectionId === "links" && (
        <>
          <Group title={t("settings.links.other_apps.title")}>
          {info.default_browser !== "unknown" && <DefaultBrowserCard key={info.default_browser} state={info.default_browser} />}
          <Field side label={t("settings.links.other_apps.open_in.label")} hint={t("settings.links.other_apps.open_in.hint")}>
            <select className="field" value={settings.external_links} onChange={(e) => update({ external_links: e.target.value as Settings["external_links"] })}>
              {EXTERNAL_LINKS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
            </select>
          </Field>
          <LinkRules saved={settings.link_rules} spaces={info.spaces} save={(rules) => update({ link_rules: rules })} />
          </Group>
          <Group title={t("settings.links.pages.title")}>
          <Field side label={t("settings.links.pages.new_tab.label")} hint={t("settings.links.pages.new_tab.hint", { key: keyLabel("⌘") })}>
            <select className="field" value={settings.new_tab_links} onChange={(e) => update({ new_tab_links: e.target.value as Settings["new_tab_links"] })}>
              <option value="inactive">{t("settings.links.pages.new_tab.inactive")}</option>
              <option value="load">{t("settings.links.pages.new_tab.load")}</option>
              <option value="switch">{t("settings.links.pages.new_tab.switch")}</option>
            </select>
          </Field>
          </Group>
          <Group title={t("settings.links.copied.title")}>
          <Toggle label={t("settings.links.copied.strip_trackers.label")} hint={t("settings.links.copied.strip_trackers.hint")} checked={settings.strip_trackers} onChange={(v) => update({ strip_trackers: v })} />
          </Group>
        </>
      )}
      {sectionId === "tabs" && (
        <>
          <Group title={t("settings.tabs.tidying.title")} hint={t("settings.tabs.tidying.hint")}>
          <Field side label={t("settings.tabs.tidying.archive_after.label")} hint={t("settings.tabs.tidying.archive_after.hint")}>
            <select className="field" value={settings.auto_archive_hours} onChange={(e) => update({ auto_archive_hours: Number(e.target.value) })}>
              {ARCHIVE_AFTER.map((a) => <option key={a.hours} value={a.hours}>{a.label}</option>)}
            </select>
          </Field>
          <Retention
            key={settings.archive_days}
            label={t("settings.tabs.tidying.keep_archived.label")}
            hint={t("settings.tabs.tidying.keep_archived.hint")}
            presets={[30, 60, 90]}
            special={t("settings.tabs.tidying.keep_archived.forever")}
            days={settings.archive_days}
            save={(days) => update({ archive_days: days })}
          />
          <Field side label={t("settings.tabs.tidying.unload_after.label")} hint={t("settings.tabs.tidying.unload_after.hint")}>
            <select className="field" value={settings.unload_minutes} onChange={(e) => update({ unload_minutes: Number(e.target.value) })}>
              {UNLOAD_AFTER.map((u) => <option key={u.minutes} value={u.minutes}>{u.label}</option>)}
            </select>
          </Field>
          <Toggle label={t("settings.tabs.tidying.load_pinned.label")} hint={t("settings.tabs.tidying.load_pinned.hint")} checked={settings.load_pinned_at_start} onChange={(v) => update({ load_pinned_at_start: v })} />
          <Field side label={t("settings.tabs.tidying.archived.label")} hint={t("settings.tabs.tidying.archived.hint", { number: info.archive_count })}>
            <button className="btn" onClick={() => send({ cmd: "reopen_closed" })}>{t("settings.tabs.tidying.archived.reopen")}</button>
          </Field>
          </Group>
          <Group title={t("settings.tabs.getting_around.title")}>
          <Field side label={t("settings.tabs.getting_around.ctrl_tab.label", { ctrl })} hint={t("settings.tabs.getting_around.ctrl_tab.hint", { ctrl })}>
            <div className="seg">
              {([["recent", t("settings.tabs.getting_around.ctrl_tab.recent")], ["sidebar", t("settings.tabs.getting_around.ctrl_tab.sidebar")]] as const).map(([order, label]) => (
                <button key={order} data-on={settings.tab_order === order || undefined} aria-pressed={settings.tab_order === order} onClick={() => update({ tab_order: order })}>{label}</button>
              ))}
            </div>
          </Field>
          <Field side label={t("settings.tabs.getting_around.library.label")} hint={t("settings.tabs.getting_around.library.hint")}>
            <select className="field" value={settings.library_opens} onChange={(e) => update({ library_opens: e.target.value as Settings["library_opens"] })}>
              <option value="archive">{t("settings.tabs.getting_around.library.archive")}</option>
              <option value="downloads">{t("settings.tabs.getting_around.library.downloads")}</option>
              <option value="bookmarks">{t("settings.tabs.getting_around.library.bookmarks")}</option>
              <option value="history">{t("settings.tabs.getting_around.library.history")}</option>
            </select>
          </Field>
          </Group>
        </>
      )}
      {sectionId === "appearance" && <AppearanceSection info={info} update={update} open={openSection} />}
      {sectionId === "smart" && <SmartFoldersSection />}
      {sectionId === "autofill" && <AutofillSection info={info} update={update} />}
      {sectionId === "adblock" && <AdblockSection info={info} update={update} reload={reload} />}
      {sectionId === "video" && <ProtectedVideoSection info={info} update={update} />}
      {sectionId === "search" && (
        <>
          <Group>
          <Field side label={t("settings.search.engine.label")} hint={t("settings.search.engine.hint")}>
            {/* Found by the names of the engines too, which only the list holds. */}
            <span hidden data-search-name>{SEARCH_ENGINES.map((e) => e.label).join(" ")}</span>
            <select className="field" aria-label={t("settings.search.engine.label")} value={SEARCH_ENGINES.some((e) => e.template === settings.search_template) ? settings.search_template : ""} onChange={(e) => e.target.value && update({ search_template: e.target.value })}>
              {SEARCH_ENGINES.map((e) => <option key={e.template} value={e.template}>{e.label}</option>)}
              {!SEARCH_ENGINES.some((e) => e.template === settings.search_template) && <option value="">{t("settings.search.engine.own")}</option>}
            </select>
          </Field>
          <SearchAddress key={settings.search_template} saved={settings.search_template} save={(template) => update({ search_template: template })} />
          </Group>
          <Group>
          <SiteSearches saved={settings.site_searches} save={(sites) => update({ site_searches: sites })} />
          </Group>
        </>
      )}
      {sectionId === "permissions" && <PermissionsSection info={info} reload={reload} />}
      {sectionId === "spaces" && <SpacesSection info={info} update={update} reload={reload} item={place.item} open={openSection} />}
      {sectionId === "profiles" && <ProfilesSection info={info} reload={reload} open={openSection} />}
      {sectionId === "favorites" && <FavoritesSection info={info} reload={reload} />}
      {sectionId === "bookmarks" && <BookmarksSection key={place.id === "bookmarks" ? place.item : undefined} info={info} reload={reload} item={place.id === "bookmarks" ? place.item : undefined} />}
      {sectionId === "developer" && (
        <>
          <Group>
        <Field label={t("settings.developer.dock.label")} hint={t("settings.developer.dock.hint")}>
          <div className="seg">
            {([["right", t("settings.developer.dock.right")], ["bottom", t("settings.developer.dock.bottom")], ["window", t("settings.developer.dock.window")]] as const).map(([v, label]) => (
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
          <Group title={t("settings.extensions.inside_pages.title")}>
          <Toggle label={t("settings.extensions.inside_pages.chrome_toolbar.label")} hint={t("settings.extensions.inside_pages.chrome_toolbar.hint")} checked={settings.chrome_toolbar} onChange={(v) => update({ chrome_toolbar: v })} />
          </Group>
          <Group title={t("settings.extensions.unpacked.title")}>
          <Toggle label={t("settings.extensions.unpacked.keep.label")} hint={t("settings.extensions.unpacked.keep.hint")} checked={settings.keep_unpacked_extensions} onChange={(v) => update({ keep_unpacked_extensions: v })} />
          </Group>
        </>
      )}
      {sectionId === "sync" && <SyncSection info={info} reload={reload} job={syncJob} setJob={setSyncJob} />}
      {sectionId === "data" && (
        <>
          <Group title={t("settings.data.browsing.title")}>
          <Retention
            key={settings.history_days}
            label={t("settings.data.browsing.history.label")}
            hint={t("settings.data.browsing.history.hint", { limit: 25_000 })}
            presets={[7, 30, 90, 365]}
            special={t("settings.data.browsing.history.none")}
            days={settings.history_days}
            save={(days) => update({ history_days: days })}
          />
          <Field side label={t("settings.data.browsing.visited.label")} hint={t("settings.data.browsing.visited.hint", { number: info.history_count })}>
            <ClearHistory key={info.history_count} disabled={info.history_count === 0} onClear={() => send({ cmd: "clear_history" }).then(() => void reload())} />
          </Field>
            <ClearBrowsingData info={info} reload={reload} />
          </Group>
          <Group title={t("settings.data.import.title")}>
          <ImportBrowsers />
          <Field side label={t("settings.data.import.bookmarks.label")} hint={t("settings.data.import.bookmarks.hint")}>
            <button className="btn" onClick={() => openSection("bookmarks")}>{t("settings.data.import.bookmarks.open")}</button>
          </Field>
          </Group>
          <Group title={t("settings.data.told.title")}>
            <Toggle label={t("settings.data.told.gpc.label")} hint={t("settings.data.told.gpc.hint")} checked={settings.global_privacy_control} onChange={(v) => update({ global_privacy_control: v })} />
          </Group>
          <Group title={t("settings.data.talks.title")} hint={t("settings.data.talks.hint")}>
            <ul className="set-list set-talks">
              <li data-search-row><b>{t("settings.data.talks.sites.name")}</b><span>{t("settings.data.talks.sites.what")}</span></li>
              <li data-search-row><b>{t("settings.data.talks.services.name")}</b><span>{t("settings.data.talks.services.what")}</span></li>
              <li data-search-row><b>{t("settings.data.talks.filter_lists.name")}</b><span>{t("settings.data.talks.filter_lists.what")}</span></li>
              <li data-search-row><b>GitHub</b><span>{t("settings.data.talks.github.what")}</span></li>
              <li data-search-row><b>{t("settings.data.talks.components.name")}</b><span>{t("settings.data.talks.components.what")}</span></li>
              <li data-search-row><b>{t("settings.data.talks.password_check.name")}</b><span>{t("settings.data.talks.password_check.what")}</span></li>
            </ul>
          </Group>
          <Group title={t("settings.data.computer.title")}>
          <Field label={t("settings.data.computer.where.label")} hint={t("settings.data.computer.where.hint")}>
            <DataFolder path={info.data_dir} />
          </Field>
          {info.keychain === "test" && (
            <Field
              label={t("settings.data.computer.mock_keychain.label")}
              hint={t("settings.data.computer.mock_keychain.hint")}
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
        <div className="set-nav-title px-2 pb-2 pt-1 text-[16px] font-semibold">{t("common.settings")}</div>
        <label className="set-search">
          <Search width={14} height={14} />
          <input ref={searchField} type="search" className="field" placeholder={t("settings.page.search")} aria-label={t("settings.page.search")} value={query} onChange={(e) => type(e.target.value)} onKeyDown={(e) => e.key === "Escape" && type("")} spellCheck={false} />
        </label>
        {/* The sections as one list, for a page too narrow for them at its side. */}
        <select className="field set-nav-pick" aria-label={t("settings.page.section")} value={shown ? sectionId : ""} onChange={(e) => openSection(e.target.value)}>
          {!shown && <option value="" disabled>{t("settings.page.search_results")}</option>}
          {GROUPS.map((g) => {
            const options = g.sections.map((s) => <option key={s.id} value={s.id}>{s.label}</option>);
            return g.label ? <optgroup key={g.label} label={g.label}>{options}</optgroup> : options;
          })}
        </select>
        <div className="set-nav-list">
        {GROUPS.map((g) => (
          <div key={g.label} className="mt-3">
            {g.label && <div className="set-nav-label">{g.label}</div>}
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
              {/* Above the name: it leads away from the space, and is none of its settings. */}
              {ownSpace && (
                <button className="set-found" onClick={() => openSection("spaces")}>
                  <Back width={15} height={15} />
                  {t("settings.page.all_spaces")}
                </button>
              )}
              <header className="mb-6 flex items-start gap-4">
                {ownSpace ? (
                  <span className="set-icon" style={{ color: accentCss(ownSpace.accent ?? undefined) }}><SpaceIcon space={ownSpace} size={22} /></span>
                ) : (
                  <span className="set-icon"><section.Icon width={22} height={22} /></span>
                )}
                <div>
                  <h1 ref={heading} tabIndex={-1} className="text-[22px] font-semibold tracking-[-0.01em] outline-none">{ownSpace?.title ?? section.title}</h1>
                  <p className="mt-1 max-w-[560px] text-[14px] leading-5 text-[var(--muted)]">{ownSpace ? t("settings.page.space_intro") : section.intro}</p>
                </div>
              </header>
              {searching && (
                <button className="set-found" onClick={() => { setPicked(false); searchField.current?.focus(); }} title={t("settings.page.back_to_results")}>
                  <Back width={15} height={15} />
                  {t("settings.page.everything_for", { query: query.trim() })}
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
    <Field label={t("settings.search.custom.label")} hint={t("settings.search.custom.hint")}>
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
      {ignored && <p className="set-hint" style={{ color: "var(--danger)" }}>{t("settings.search.custom.invalid")}</p>}
    </Field>
  );
}

const MAX_RETENTION_DAYS = 3650;

/** What the Settings page calls a retention period of `days`. */
const daysLabel = (days: number) => (days === 365 ? t("settings.times.years", { count: 1 }) : t("settings.times.days", { count: days }));

/**
 * How long the archive or the history keeps its records: one of `presets`
 * (days), a number of days typed in, or `special`, the choice saved as 0
 * days. A typed number is saved on Enter or when the field is left, not
 * while typing: a shorter period deletes records within a minute.
 */
function Retention({ label, hint, presets, special, days, save }: {
  label: string;
  hint: string;
  presets: number[];
  special: string;
  days: number;
  save: (days: number) => void;
}) {
  const listed = days === 0 || presets.includes(days);
  const [custom, setCustom] = useState(!listed);
  const [text, setText] = useState(String(days || presets[0]));
  const typed = Number(text);
  const valid = Number.isInteger(typed) && typed >= 1 && typed <= MAX_RETENTION_DAYS;
  const commit = () => {
    if (valid && typed !== days) save(typed);
  };
  return (
    <Field side label={label} hint={hint}>
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
          {presets.map((d) => <option key={d} value={d}>{daysLabel(d)}</option>)}
          <option value="custom">{t("settings.retention.custom")}</option>
          <option value={0}>{special}</option>
        </select>
        {custom && (
          <>
            <input
              className="field"
              style={{ width: 88 }}
              inputMode="numeric"
              value={text}
              aria-label={t("settings.retention.days")}
              aria-invalid={!valid || undefined}
              autoFocus={listed}
              onChange={(e) => setText(e.target.value.replace(/\D/g, "").slice(0, 4))}
              onBlur={commit}
              onKeyDown={(e) => e.key === "Enter" && commit()}
            />
            <span className="shrink-0 text-[var(--muted)]">{t("settings.retention.days_unit")}</span>
          </>
        )}
      </div>
      {custom && !valid && <p className="set-hint" style={{ color: "var(--danger)" }}>{t("settings.retention.invalid", { max: String(MAX_RETENTION_DAYS) })}</p>}
    </Field>
  );
}

/** "Clear history", after a second click. */
function ClearHistory({ disabled, onClear }: { disabled: boolean; onClear: () => void }) {
  const [confirming, setConfirming] = useState(false);
  return (
    <div className="flex gap-2">
      <button className="btn" disabled={disabled} data-danger={confirming || undefined} onClick={() => (confirming ? onClear() : setConfirming(true))} onBlur={() => setConfirming(false)}>
        {confirming ? t("settings.data.browsing.visited.clear_all") : t("settings.data.browsing.visited.clear")}
      </button>
      <button className="btn" onClick={() => send({ cmd: "show_history" })}>{t("settings.data.browsing.visited.show")}</button>
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
    <Field side label={t("settings.links.other_apps.default_browser.label")} hint={now === "yes" ? t("settings.links.other_apps.default_browser.yes") : t("settings.links.other_apps.default_browser.no")}>
      {now === "no" && <button className="btn" disabled={asking} onClick={ask}>{asking ? (IS_MAC ? t("settings.links.other_apps.default_browser.waiting_mac") : t("settings.links.other_apps.default_browser.opening_settings")) : t("settings.links.other_apps.default_browser.make_default")}</button>}
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
    <Field side label={t("settings.general.downloads.folder.label")} hint={folder ? folder : t("settings.general.downloads.folder.system")}>
      <div className="flex gap-2">
        {folder && <button className="btn ghost" disabled={busy} onClick={() => run(reset())}>{t("common.reset")}</button>}
        <button className="btn" disabled={busy} onClick={choose}>{t("common.choose")}</button>
      </div>
      {missing && folder && <p className="set-hint" style={{ color: "var(--warn)" }}>{t("settings.general.downloads.folder.missing")}</p>}
      {failed && <p className="set-hint" style={{ color: "var(--danger)" }}>{t("settings.general.downloads.folder.failed")}</p>}
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
    <Field side label={t("settings.data.browsing.site_data.label")} hint={t("settings.data.browsing.site_data.hint")}>
      <div className="flex flex-wrap gap-2">
        {logins.length > 1 && (
          <ProfilePick info={info} profiles={logins} value={profile} onChoose={setChosen} />
        )}
        <button className="btn" disabled={!profile} onClick={open}>{t("settings.data.browsing.site_data.delete")}</button>
      </div>
      {failed && <p className="set-hint" role="alert" style={{ color: "var(--danger)" }}>{t("settings.data.browsing.site_data.failed")}</p>}
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
  const wrong = typed.filter((tag) => !LANGUAGE_TAG.test(tag) || twice(tag));
  return (
    <Field label={t("settings.general.languages.web.label")} hint={t("settings.general.languages.web.hint")}>
      <input
        className="field mono"
        value={text}
        placeholder="de, en-US, en"
        aria-label={t("settings.general.languages.web.label")}
        aria-invalid={wrong.length > 0 || many || undefined}
        spellCheck={false}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => {
          setRefused(false);
          if (wrong.length === 0 && !many && typed.join() !== saved.join()) save(typed).catch(() => setRefused(true));
        }}
        onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      />
      {wrong.length > 0 && <p className="set-hint" style={{ color: "var(--danger)" }}>{t("settings.general.languages.web.not_codes", { count: wrong.length, codes: wrong.join(", ") })}</p>}
      {wrong.length === 0 && many && <p className="set-hint" style={{ color: "var(--danger)" }}>{t("settings.general.languages.web.over_max", { max: MAX_LANGUAGES })}</p>}
      {/* Said aloud: it comes after the field was left. */}
      {refused && <p className="set-hint" role="alert" style={{ color: "var(--danger)" }}>{t("settings.general.languages.web.refused")}</p>}
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
    if (!site) return t("settings.links.other_apps.rules.no_site");
    if (all.find((r) => ruleSite(r.site) === site) !== rule) return t("settings.links.other_apps.rules.taken");
    if (!spaces.some((s) => s.id === rule.space)) return t("settings.links.other_apps.rules.space_gone");
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
    <Field label={t("settings.links.other_apps.rules.label")} hint={t("settings.links.other_apps.rules.hint")}>
      <div className="space-y-2">
        {rows.map((r, i) => {
          // A row just added says nothing until something is typed.
          const why = r.site ? problem(r, rows) : null;
          return (
            <div key={i}>
              <div className="flex flex-wrap items-center gap-2">
                <input className="field mono" style={{ flex: "1 1 160px", minWidth: 0 }} value={r.site} placeholder="github.com/acme" aria-label={t("settings.links.other_apps.rules.site")} aria-invalid={!!why || undefined} onChange={(e) => change(i, { site: e.target.value })} onBlur={() => commit(rows)} />
                <span className="text-[13px] text-[var(--muted)]">{t("settings.links.other_apps.rules.opens_in")}</span>
                <PickList options={spaceOptions(spaces)} chosen={spaces.some((s) => s.id === r.space) ? [r.space] : []} none={t("settings.links.other_apps.rules.choose_space")} label={t("settings.links.other_apps.rules.space_for", { site: r.site || t("settings.links.other_apps.rules.this_site") })} width={180} align="right" onChoose={([to]) => to && commit(change(i, { space: to }))} />
                <button className="ib" title={t("common.remove")} aria-label={t("settings.links.other_apps.rules.remove", { site: r.site || t("settings.links.other_apps.rules.a_site") })} onClick={() => { const next = rows.filter((_, j) => j !== i); setRows(next); commit(next); }}><Close width={14} height={14} /></button>
              </div>
              {why && <p className="set-hint" style={{ color: "var(--danger)" }}>{t("settings.not_saved", { reason: why })}</p>}
            </div>
          );
        })}
        <button className="btn" disabled={rows.length >= MAX_LINK_RULES} title={rows.length >= MAX_LINK_RULES ? t("settings.links.other_apps.rules.max", { max: MAX_LINK_RULES }) : undefined} onClick={() => setRows((list) => [...list, { site: "", space: spaces[0]?.id ?? "" }])}>{t("settings.links.other_apps.rules.add")}</button>
      </div>
    </Field>
  );
}

const KEYWORD = /^[\p{L}\p{N}_-]{1,20}$/u;

/** Why a site search would not be kept (`SiteSearch::sanitised` in the app), or null. */
function siteSearchProblem(site: SiteSearch, all: SiteSearch[]): string | null {
  const keyword = site.keyword.trim().toLowerCase();
  if (!KEYWORD.test(keyword)) return t("settings.search.sites.bad_keyword");
  if (all.find((s) => s.keyword.trim().toLowerCase() === keyword) !== site) return t("settings.search.sites.keyword_taken");
  if (!site.name.trim()) return t("settings.search.sites.no_name");
  if (!(site.url.trim().startsWith("https://") && site.url.includes("%s"))) return t("settings.search.sites.bad_address");
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
    <Field label={t("settings.search.sites.label")} hint={t("settings.search.sites.hint")}>
      <div className="space-y-2">
        {rows.map((s, i) => {
          // A row just added says nothing until something is typed.
          const problem = s.keyword || s.name || s.url ? siteSearchProblem(s, rows) : null;
          return (
            <div key={i}>
              <div className="flex flex-wrap items-center gap-2">
                <input className="field mono" style={{ width: 88 }} value={s.keyword} placeholder={t("settings.search.sites.keyword_placeholder")} aria-label={t("settings.search.sites.keyword")} aria-invalid={!!problem || undefined} onChange={(e) => change(i, { keyword: e.target.value })} onBlur={() => commit(rows)} spellCheck={false} />
                <input className="field" style={{ width: 140 }} value={s.name} placeholder={t("settings.search.sites.name")} aria-label={t("settings.search.sites.name")} onChange={(e) => change(i, { name: e.target.value })} onBlur={() => commit(rows)} />
                <input className="field" style={{ flex: "1 1 160px", minWidth: 0 }} value={s.url} placeholder="https://example.org/search?q=%s" aria-label={t("settings.search.sites.address")} onChange={(e) => change(i, { url: e.target.value })} onBlur={() => commit(rows)} spellCheck={false} />
                <button className="ib" title={t("common.remove")} aria-label={t("settings.search.sites.remove", { name: s.name || t("settings.search.sites.site_search") })} onClick={() => remove(i)}><Close width={14} height={14} /></button>
              </div>
              {problem && <p className="set-hint" style={{ color: "var(--danger)" }}>{t("settings.not_saved", { reason: problem })}</p>}
            </div>
          );
        })}
        <button className="btn" onClick={() => setRows((list) => [...list, { keyword: "", name: "", url: "" }])}>{t("settings.search.sites.add")}</button>
      </div>
    </Field>
  );
}

/** The path of the data folder: text to select, a button that copies it and one that shows the folder in the file manager. */
function DataFolder({ path }: { path: string }) {
  const [said, setSaid] = useState<string>();
  const copy = () =>
    send({ cmd: "copy_data_dir" }).then(
      (r) => setSaid(r.ok ? t("common.copied") : t("settings.data.computer.where.copy_failed")),
      () => setSaid(t("settings.data.computer.where.copy_failed")),
    );
  useEffect(() => {
    if (!said) return;
    const timer = window.setTimeout(() => setSaid(undefined), 2000);
    return () => window.clearTimeout(timer);
  }, [said]);
  return (
    <div className="flex items-center gap-2">
      <code className="set-path min-w-0 flex-1 rounded-lg bg-white/5 px-3 py-2 text-[12px] text-[var(--secondary)]">{path}</code>
      <button className="btn flex-none" onClick={() => void copy()}>
        <span aria-live="polite">{said ?? t("common.copy")}</span>
      </button>
      <button className="btn flex-none" title={t("settings.data.computer.where.open_hint")} onClick={() => void send({ cmd: "open_data_dir" }).catch(() => setSaid(t("settings.data.computer.where.open_failed")))}>{t("common.open")}</button>
    </div>
  );
}
