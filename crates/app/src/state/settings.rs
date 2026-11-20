//! User preferences: what they are, their defaults, and the checks every
//! change goes through, whether it comes from the Settings page or from
//! another device.

use std::collections::{BTreeMap, HashMap};

use magnetite_core::{Id, UnixSeconds};
use magnetite_engine::in_domain;
use serde::{Deserialize, Serialize};

/// The longest retention period in days, for the archive and the history:
/// ten years.
pub const MAX_ARCHIVE_DAYS: u32 = 3650;

/// `Settings::auto_archive_hours` for "Never". Not 0: the setting syncs, and
/// versions from before this value raise 0 to one hour, while they read
/// this one as a period that never ends.
pub const NEVER_ARCHIVE: u32 = u32::MAX;

/// The current set of defaults. Raised when a default changes that
/// installations still on the old one should follow
/// (`Settings::with_current_defaults`).
const DEFAULTS: u32 = 1;

/// The unload time every installation began with before `DEFAULTS` 1.
const FIRST_UNLOAD_MINUTES: u32 = 30;

/// User preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Today tabs unused this long are archived. Minimum 1 h;
    /// `NEVER_ARCHIVE` turns it off.
    pub auto_archive_hours: u32,
    /// Archived tabs are deleted after this many days (up to
    /// `MAX_ARCHIVE_DAYS`); 0 keeps them until they are removed by hand.
    pub archive_days: u32,
    /// Visits stay in the history this many days (up to `MAX_ARCHIVE_DAYS`);
    /// 0 keeps none (`history.rs`).
    pub history_days: u32,
    /// Hidden tabs lose their window after this; 0 disables unloading.
    pub unload_minutes: u32,
    /// Which set of defaults these settings began with (`DEFAULTS`); 0 for
    /// settings saved before defaults were counted. Not synced.
    #[serde(default)]
    pub defaults: u32,
    /// Search URL template with `%s` for the query.
    pub search_template: String,
    /// Keywords that search one site from the command bar (`gh rust`).
    pub site_searches: Vec<SiteSearch>,
    /// Links from other apps: "mini" (a mini window, the default), "tab" or
    /// "peek". Synced: a version that does not know a value takes the
    /// default (`sanitised`).
    pub external_links: String,
    /// Which space a link from another app opens in, by its site
    /// (`LinkRule`). This device's own, never synced.
    pub link_rules: Vec<LinkRule>,
    /// The user's own CSS and JavaScript per site (Settings → Custom CSS &
    /// JS). This device's own, never synced.
    pub site_code: Vec<magnetite_engine::SiteCode>,
    /// The shortcuts the user gave other keys, or none (Settings →
    /// Shortcuts); the rest have their default keys. Synced.
    pub shortcuts: magnetite_engine::Overrides,
    /// The shortcuts whose keys go first to someone else than by default:
    /// to the page, or to Magnetite (Settings → Shortcuts). Synced.
    pub shortcut_priority: magnetite_engine::Priorities,
    /// Sites (registrable domains) whose pages get the keys of every
    /// shortcut first, whatever the shortcut's own choice (the site panel,
    /// Settings → Shortcuts). Synced.
    pub shortcut_sites: Vec<String>,
    /// The system-wide quick search key (⌥⌘N) is registered. This
    /// device's own choice, never synced.
    pub quick_search: bool,
    /// Show Chrome's toolbar row in pages (extension buttons).
    pub chrome_toolbar: bool,
    /// How the sidebar and Magnetite's own pages look.
    pub appearance: Appearance,
    /// Pop a playing video out (picture-in-picture) when leaving its tab.
    pub auto_pip: bool,
    /// Several windows on one sidebar (New Window, ⌘N; `state/windows.rs`).
    /// Off unless turned on: it is new, and untested on macOS and Windows.
    pub several_windows: bool,
    /// Remove tracking parameters (utm_*, fbclid, …) when copying links.
    pub strip_trackers: bool,
    /// A web panel closes by itself when a page behind it is used
    /// (`web-panels.md`); off: it stays until it is closed.
    pub panel_autoclose: bool,
    /// Where DevTools open: right of or below the page, or a separate window.
    pub devtools_dock: magnetite_engine::DevToolsDock,
    /// The size docked DevTools were dragged to, per side. This device's
    /// own: screens differ.
    pub devtools_size: magnetite_engine::DevToolsSize,
    #[serde(default)]
    pub sync: SyncSettings,
    /// Extensions (ids) shown in the sidebar in the default profile; all of
    /// them are in the menu bar's Extensions menu. Also what another
    /// profile shows until it has a list of its own (`profile_pins`).
    pub pinned_extensions: Vec<String>,
    /// The same for the other profiles, by profile id: an extension is
    /// installed profile by profile, and so is its place in the sidebar
    /// (`AppState::pins`).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub profile_pins: HashMap<String, Vec<String>>,
    /// Chromium's own pins as last seen, per profile id (`""`: default), so
    /// only changes made there carry over (`follow_chromium_pins`).
    pub chromium_pins_seen: HashMap<String, Vec<String>>,
    /// Look for a new release at start and once a day (`update.rs`).
    pub check_updates: bool,
    /// What a click on the sidebar's archive and downloads button opens.
    pub library_opens: LibraryView,
    /// The order `Ctrl+Tab` goes through the tabs in. This device's own
    /// choice, never synced.
    pub tab_order: TabOrder,
    /// Chromium's password manager and autofill: which parts are on. Not
    /// synced: what a device saves is that device's.
    pub autofill: magnetite_engine::AutofillPrefs,
    /// Tell websites not to sell or share the user's data (Global Privacy
    /// Control). Not synced, as the other settings of this kind.
    pub global_privacy_control: bool,
    /// Where downloads go, and whether to ask each time. Not synced: a
    /// folder is this computer's.
    pub downloads: magnetite_engine::DownloadPrefs,
    /// The zoom, in percent, a site has until it is zoomed on its own: one
    /// of Chromium's steps. Not synced: screens differ.
    pub page_zoom: u32,
    /// Spell check, and the languages websites are asked for. Not synced:
    /// a device has its own keyboard and reader.
    pub languages: magnetite_engine::LanguagePrefs,
    /// Smart folders allowed to fetch on this device (`smart_key`): those made
    /// or edited here, or turned on here. A folder another device synced
    /// waits, so the words alone cannot make this device's accounts run a
    /// query. Never synced. `None` before the first start that knew this.
    #[serde(default)]
    pub smart_allowed: Option<Vec<String>>,
    /// The ad blocker. Not synced: each device downloads its own lists.
    pub adblock: AdblockSettings,
    /// Developer features: the row for local pages, the hosts that count
    /// as local. Not synced.
    pub developer: crate::dev::DeveloperSettings,
    /// Which sites the system's own web engine shows instead of Chromium.
    /// Not synced: only some systems have such an engine.
    pub system_engine: SystemEngine,
}

/// The services known to play protected video only in a browser build that
/// Google's Widevine programme has verified, which Magnetite's Chromium is
/// not: they refuse it and play in the system's engine. By host, each with
/// its subdomains. Ships with each release; never fetched from anywhere.
pub const PROTECTED_VIDEO_SITES: &[&str] = &[
    "netflix.com",
    "disneyplus.com",
    "max.com",
    "hbomax.com",
    "primevideo.com",
    "hulu.com",
];

/// The most sites of their own a user names for the system's engine.
const MAX_SYSTEM_ENGINE_SITES: usize = 100;

/// Which sites open in the system's own web engine (Settings → Protected
/// video; `magnetite_engine`'s `system.rs`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SystemEngine {
    /// The services of `PROTECTED_VIDEO_SITES` do.
    pub known: bool,
    /// Sites the user named, by host in lower case without `www.`.
    pub sites: Vec<String>,
}

impl Default for SystemEngine {
    fn default() -> Self {
        Self {
            known: true,
            sites: Vec::new(),
        }
    }
}

impl SystemEngine {
    /// Hosts only, each once, and none that the known services cover
    /// while those are on.
    fn sanitised(mut self) -> Self {
        let mut sites: Vec<String> = Vec::new();
        for site in std::mem::take(&mut self.sites) {
            if let Some(host) = site_host(&site)
                && !sites.contains(&host)
                && !(self.known && PROTECTED_VIDEO_SITES.contains(&host.as_str()))
                && sites.len() < MAX_SYSTEM_ENGINE_SITES
            {
                sites.push(host);
            }
        }
        self.sites = sites;
        self
    }

    /// Whether `host` is one of the known services, while those are on.
    pub fn knows(&self, host: &str) -> bool {
        self.known && (PROTECTED_VIDEO_SITES.iter()).any(|site| in_domain(host, site))
    }

    /// Whether `host` is under one of the user's own sites.
    pub fn names(&self, host: &str) -> bool {
        self.sites.iter().any(|site| in_domain(host, site))
    }

    /// Name `site` for the system's engine (the site panel's switch), or
    /// take every own site that covers `host` out again. An error if the
    /// list is full: the next check of the settings would drop the site
    /// again without a word.
    pub fn set_site(&mut self, host: &str, site: String, system: bool) -> Result<(), String> {
        let covers = |own: &String| in_domain(host, own);
        // Counted before anything changes: a site that is refused leaves
        // the list as it was.
        let others = self.sites.iter().filter(|own| !covers(own)).count();
        if system && others >= MAX_SYSTEM_ENGINE_SITES {
            return Err(format!(
                "The list holds {MAX_SYSTEM_ENGINE_SITES} sites already. Remove one in Settings → Protected video first."
            ));
        }
        self.sites.retain(|own| !covers(own));
        if system {
            self.sites.push(site);
        }
        Ok(())
    }

    /// Every site that opens in the system's engine.
    pub fn hosts(&self) -> Vec<String> {
        let known = if self.known {
            PROTECTED_VIDEO_SITES
        } else {
            &[]
        };
        (known.iter().map(ToString::to_string))
            .chain(self.sites.iter().cloned())
            .collect()
    }
}

/// The ad blocker (Settings → Ad blocking, `adblock.rs`). A new installation
/// starts with `recommended`; settings saved without any stay off, with no
/// list.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AdblockSettings {
    pub enabled: bool,
    /// The lists subscribed to (`adblock::LISTS` ids).
    pub lists: Vec<String>,
    /// Sites (registrable domains) where nothing is blocked.
    pub allowed_sites: Vec<String>,
}

impl AdblockSettings {
    /// What a new installation starts with: on, with the recommended lists.
    pub fn recommended() -> Self {
        Self {
            enabled: true,
            lists: crate::adblock::system_recommended(),
            allowed_sites: Vec::new(),
        }
    }

    /// Known lists only, and sites as lower-case names, each once.
    fn sanitised(mut self) -> Self {
        let mut lists: Vec<String> = Vec::new();
        for id in std::mem::take(&mut self.lists) {
            if crate::adblock::list(&id).is_some() && !lists.contains(&id) {
                lists.push(id);
            }
        }
        self.lists = lists;
        self.allowed_sites = site_names(&self.allowed_sites);
        self
    }
}

/// The site (registrable domain) a user means by `typed`: a name, or a
/// whole address, of which the host counts. `None` for what names no host.
pub fn typed_site(typed: &str) -> Option<String> {
    let typed = typed.trim().to_lowercase();
    // An IP address counts as it is; one of version 6 has colons of its
    // own, which an address without brackets would lose.
    let bare = typed.trim_matches(['[', ']']);
    if bare.parse::<std::net::IpAddr>().is_ok() {
        return Some(bare.to_string());
    }
    let host = if typed.contains("://") {
        magnetite_engine::host_of(&typed)?
    } else {
        magnetite_engine::host_of(&format!("https://{typed}"))?
    };
    let named = !host.is_empty()
        && (host.chars()).all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'));
    named.then(|| magnetite_engine::registrable_domain(&host))
}

/// `sites` as lower-case names, sorted, each once.
fn site_names(sites: &[String]) -> Vec<String> {
    let mut sites: Vec<String> = (sites.iter())
        .map(|s| s.trim().trim_end_matches('.').to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    sites.sort();
    sites.dedup();
    sites
}

/// How the sidebar and Magnetite's own pages look (Settings → Appearance).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Appearance {
    /// Text size in percent of the default, 85 to 130.
    pub text_scale: u32,
    /// Height and spacing of sidebar rows.
    pub density: Density,
    /// Width of the docked sidebar in points, 220 to 360.
    pub sidebar_width: u32,
    /// Navigation, the address and the window buttons in a bar across the
    /// top of the window, not at the top of the sidebar.
    pub top_bar: bool,
    /// A row of extension icons below the sidebar's address, in place of
    /// the icons in its title band.
    pub extension_bar: bool,
    /// How each space looks beyond its colour, by space id. A space without
    /// an entry has the default look. Kept on this device.
    pub looks: BTreeMap<String, SpaceLook>,
}

/// One space's look beyond its colour (Settings → Appearance).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SpaceLook {
    /// How much of the space colour is mixed into the window, in percent,
    /// 0 to 30.
    pub tint: u32,
    /// What is drawn over the sidebar's colour.
    pub sidebar_effect: SidebarEffect,
    /// When the user's own sidebar image was stored (milliseconds), `None`
    /// without one. Part of the image's address, so pages load a new one.
    pub sidebar_image: Option<u64>,
    /// The same for the image behind the empty window.
    pub empty_image: Option<u64>,
}

impl Default for SpaceLook {
    fn default() -> Self {
        Self {
            tint: 8,
            sidebar_effect: SidebarEffect::None,
            sidebar_image: None,
            empty_image: None,
        }
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            text_scale: 100,
            density: Density::Comfortable,
            sidebar_width: 264,
            top_bar: false,
            extension_bar: false,
            looks: BTreeMap::new(),
        }
    }
}

impl Appearance {
    /// Keep values inside what the layout handles.
    pub fn clamped(mut self) -> Self {
        self.text_scale = self.text_scale.clamp(85, 130);
        self.sidebar_width = Self::sidebar_width(self.sidebar_width);
        for look in self.looks.values_mut() {
            look.tint = look.tint.min(30);
        }
        self
    }

    /// The look of space `id`: its own, or the default one.
    pub fn look(&self, id: &str) -> SpaceLook {
        self.looks.get(id).cloned().unwrap_or_default()
    }

    /// The docked sidebar's width kept between 220 and 360 points.
    pub fn sidebar_width(width: u32) -> u32 {
        width.clamp(220, 360)
    }
}

/// An effect over the sidebar's colour, or the user's own image.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SidebarEffect {
    #[default]
    None,
    Grain,
    Glow,
    Aurora,
    Fade,
    /// The stored image (`SpaceLook::sidebar_image`).
    Image,
}

/// Row height and spacing in the sidebar.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    #[default]
    Comfortable,
    Spacious,
}

/// The order `Ctrl+Tab` and `Ctrl+Shift+Tab` go through the tabs in.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TabOrder {
    /// By last use: one press goes to the tab used before this one.
    #[default]
    Recent,
    /// As the sidebar lists them.
    Sidebar,
}

/// A view behind the sidebar's archive and downloads button. While something
/// downloads, a click opens Downloads whatever the setting says.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LibraryView {
    #[default]
    Archive,
    Downloads,
    Bookmarks,
    History,
}

/// A site search: `keyword`, a space and a query in the command bar search
/// `url` (`%s` for the query) instead of the search engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SiteSearch {
    pub keyword: String,
    pub name: String,
    pub url: String,
}

/// The most site searches kept.
const MAX_SITE_SEARCHES: usize = 50;

/// The site searches a new installation starts with.
fn builtin_site_searches() -> Vec<SiteSearch> {
    [
        ("gh", "GitHub", "https://github.com/search?q=%s"),
        (
            "w",
            "Wikipedia",
            "https://en.wikipedia.org/wiki/Special:Search?search=%s",
        ),
        (
            "yt",
            "YouTube",
            "https://www.youtube.com/results?search_query=%s",
        ),
        ("mdn", "MDN", "https://developer.mozilla.org/search?q=%s"),
        ("crates", "crates.io", "https://crates.io/search?q=%s"),
        ("npm", "npm", "https://www.npmjs.com/search?q=%s"),
        (
            "so",
            "Stack Overflow",
            "https://stackoverflow.com/search?q=%s",
        ),
        ("ddg", "DuckDuckGo", "https://duckduckgo.com/?q=%s"),
    ]
    .into_iter()
    .map(|(keyword, name, url)| SiteSearch {
        keyword: keyword.into(),
        name: name.into(),
        url: url.into(),
    })
    .collect()
}

/// Whether `template` may be searched with: https, with `%s` in it, an
/// address Magnetite opens from stored data.
fn usable_search(template: &str) -> bool {
    template.contains("%s")
        && template.starts_with("https://")
        && crate::url_policy::allowed(template, crate::url_policy::Source::Stored)
}

impl SiteSearch {
    /// This search as Magnetite keeps it, or `None` if it cannot be used:
    /// a lower-case keyword of letters, digits, `-` or `_` (at most 20), a
    /// name (cut at 60 characters) and a usable search address.
    fn sanitised(self) -> Option<Self> {
        let keyword = self.keyword.trim().to_lowercase();
        let name: String = self.name.trim().chars().take(60).collect();
        let url = self.url.trim().to_string();
        let keyword_ok = (1..=20).contains(&keyword.chars().count())
            && keyword
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_');
        (keyword_ok && !name.is_empty() && usable_search(&url)).then_some(Self {
            keyword,
            name,
            url,
        })
    }
}

/// A rule for links from other apps: one to `site` opens in space `space`.
/// `site` is a host, or a host and the start of a path (`github.com/acme`),
/// in lower case without `www.`; it stands for its subdomains too.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkRule {
    pub site: String,
    pub space: Id,
}

/// The most link rules kept.
const MAX_LINK_RULES: usize = 100;

/// The host of an http(s) address or of a rule's site, without `www.`, and
/// the path after it (up to `?` or `#`, without the `/` at its end).
fn host_and_path(address: &str) -> Option<(String, &str)> {
    // A scheme stands at the very start; "://" further in belongs to a path.
    let scheme = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
    };
    let rest = match address.split_once("://") {
        Some((before, rest)) if scheme(before) => rest,
        _ => address,
    };
    let rest = rest.split(['?', '#']).next()?;
    let (authority, path) = match rest.find('/') {
        Some(at) => rest.split_at(at),
        None => (rest, ""),
    };
    let host = authority.rsplit('@').next()?.split(':').next()?;
    let host = host.to_ascii_lowercase();
    let host = host.trim_start_matches("www.").to_string();
    let fine = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '.';
    (host.contains('.') && host.chars().all(fine)).then(|| (host, path.trim_end_matches('/')))
}

/// `path` with its `.` and `..` segments resolved, as a browser does
/// before it asks for the page (also written `%2e`): `/a/./b/../c` is
/// `/a/c`. Without the `/` at its end, like `host_and_path`'s paths.
fn without_dot_segments(path: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    for segment in path.split('/').skip(1) {
        match segment.to_ascii_lowercase().replace("%2e", ".").as_str() {
            "." => {}
            ".." => {
                kept.pop();
            }
            _ => kept.push(segment),
        }
    }
    let joined: String = kept.iter().flat_map(|segment| ["/", segment]).collect();
    joined.trim_end_matches('/').to_string()
}

impl LinkRule {
    /// This rule as Magnetite keeps it, or `None` without a site: what was
    /// typed may be a whole address (`https://www.GitHub.com/acme/`), of
    /// which the host and the path stay (`github.com/acme`).
    fn sanitised(self) -> Option<Self> {
        let (host, path) = host_and_path(self.site.trim())?;
        Some(Self {
            site: format!("{host}{path}"),
            space: self.space,
        })
    }

    /// How closely the rule fits `url`, if it does: the lengths of its host
    /// and its path. It fits an http(s) address on its host or a subdomain
    /// of it, under its path.
    fn fit(&self, url: &str) -> Option<(usize, usize)> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return None;
        }
        let (host, path) = host_and_path(url)?;
        // As the browser will load it: `/acme/../other` is `/other`.
        let path = without_dot_segments(path);
        let (site, under) = host_and_path(&self.site)?;
        let on_host = host == site || host.ends_with(&format!(".{site}"));
        // `/acme` is above `/acme/app`, not above `/acmecorp`.
        let below = path
            .strip_prefix(under)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'));
        (on_host && below).then_some((site.len(), under.len()))
    }
}

impl Settings {
    /// The space a link from another app to `url` opens in, if a rule says
    /// so: of several rules that fit, the one with the longest host, then
    /// the longest path. Only rules whose space `exists` count: one whose
    /// space is gone must not stand in the way of a less exact rule.
    pub fn link_space(&self, url: &str, exists: impl Fn(&Id) -> bool) -> Option<&Id> {
        (self.link_rules.iter())
            .filter(|rule| exists(&rule.space))
            .filter_map(|rule| Some((rule.fit(url)?, rule)))
            .max_by_key(|(fit, _)| *fit)
            .map(|(_, rule)| &rule.space)
    }
}

/// The most sites with CSS and JavaScript of the user's own, and the most
/// bytes of either for one site.
const MAX_CODE_SITES: usize = 200;
const MAX_SITE_CODE: usize = 256 * 1024;

/// The host a site was named by, in lower case and without `www.`: what
/// was typed may be a whole address. `None` if it names no host.
fn site_host(typed: &str) -> Option<String> {
    // The host as `host_and_path` finds it: a scheme counts only at the start.
    let (host, _) = host_and_path(typed.trim())?;
    Some(host)
}

/// Sync configuration. Keys are kept out of `state.json` (see `sync.rs`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SyncSettings {
    pub enabled: bool,
    pub server_url: String,
    /// Last server version we are in sync with, per collection.
    pub versions: HashMap<String, u64>,
    pub last_sync_at: Option<UnixSeconds>,
    pub last_error: Option<String>,
    /// This device started its chain. Only that device writes into a chain
    /// that holds nothing yet; one that joined waits for the first data.
    pub started_here: bool,
    /// This install among the chain's devices; made up when it first syncs.
    pub device_id: String,
    /// Tabs of other devices taken over here that their devices may still
    /// list (`devices` module).
    pub taken: Vec<String>,
}

impl Settings {
    /// The settings as Magnetite accepts them, whether from the Settings page
    /// or from another device: archiving after at least an hour, retention
    /// periods in range, appearance in range, search addresses on https with
    /// `%s` in them (site searches that are not, or repeat a keyword, are
    /// dropped), a known choice for external links, link rules with a site
    /// each (one per site), own CSS and JavaScript only for sites named by
    /// their host (one entry per site, each part at most `MAX_SITE_CODE`
    /// long).
    pub fn sanitised(mut self) -> Self {
        let defaults = Self::default();
        self.auto_archive_hours = self.auto_archive_hours.max(1);
        self.archive_days = self.archive_days.min(MAX_ARCHIVE_DAYS);
        self.history_days = self.history_days.min(MAX_ARCHIVE_DAYS);
        self.appearance = self.appearance.clamped();
        self.adblock = self.adblock.sanitised();
        self.shortcut_sites = site_names(&self.shortcut_sites);
        self.developer = self.developer.sanitised();
        self.system_engine = self.system_engine.sanitised();
        if !usable_search(&self.search_template) {
            self.search_template = defaults.search_template;
        }
        let mut sites: Vec<SiteSearch> = Vec::new();
        for site in std::mem::take(&mut self.site_searches) {
            if let Some(site) = site.sanitised()
                && !sites.iter().any(|s| s.keyword == site.keyword)
                && sites.len() < MAX_SITE_SEARCHES
            {
                sites.push(site);
            }
        }
        self.site_searches = sites;
        if !matches!(self.external_links.as_str(), "tab" | "peek" | "mini") {
            self.external_links = defaults.external_links;
        }
        let mut rules: Vec<LinkRule> = Vec::new();
        for rule in std::mem::take(&mut self.link_rules) {
            if let Some(rule) = rule.sanitised()
                && !rules.iter().any(|r| r.site == rule.site)
                && rules.len() < MAX_LINK_RULES
            {
                rules.push(rule);
            }
        }
        self.link_rules = rules;
        let mut code: Vec<magnetite_engine::SiteCode> = Vec::new();
        for mut entry in std::mem::take(&mut self.site_code) {
            let small = entry.css.len() <= MAX_SITE_CODE && entry.js.len() <= MAX_SITE_CODE;
            if let Some(site) = site_host(&entry.site)
                && small
                && !code.iter().any(|c| c.site == site)
                && code.len() < MAX_CODE_SITES
            {
                entry.site = site;
                code.push(entry);
            }
        }
        self.site_code = code;
        self.shortcuts = self.shortcuts.sanitised();
        // A folder is a whole path; anything else means Chromium's own.
        if !std::path::Path::new(&self.downloads.folder).is_absolute() {
            self.downloads.folder.clear();
        }
        if !magnetite_engine::ZOOM_STEPS.contains(&self.page_zoom) {
            self.page_zoom = defaults.page_zoom;
        }
        self.languages = self.languages.sanitised();
        self
    }
}

impl Settings {
    /// The shortcuts that have keys, each with them as text.
    pub fn shortcut_keys(&self) -> impl Iterator<Item = (magnetite_engine::Action, String)> {
        (magnetite_engine::Action::ALL.into_iter())
            .filter_map(|action| Some((action, self.shortcuts.combo_of(action)?.to_string())))
    }

    /// Settings read from this device's `state.json`, following defaults
    /// that changed since they were saved: pages unloaded after 30 minutes
    /// off screen, which read as slow tabs. That time may have been chosen
    /// rather than kept; the two cannot be told apart, and Settings sets it
    /// back.
    pub fn with_current_defaults(mut self) -> Self {
        if self.defaults < 1 && self.unload_minutes == FIRST_UNLOAD_MINUTES {
            self.unload_minutes = Self::default().unload_minutes;
        }
        self.defaults = DEFAULTS;
        self
    }

    /// These settings with `patch` laid over them (what a Settings page
    /// changed), checked like any others. A page sends only what it changed:
    /// its copy of the rest may be old. What pages never change stays as it
    /// is here whatever the patch says: everything about sync, the pinned
    /// extensions, the smart folders allowed on this device, and the
    /// shortcuts' keys and who gets them first, which have commands of
    /// their own (`set_shortcut`, `set_shortcut_first`).
    /// The download folder too, which only the system's dialog names
    /// (`choose_download_folder`); a patch can only take it away. And the
    /// stamps of the spaces' own images, which `set_appearance_image`
    /// sets: a patch carries those its page last saw, of a
    /// picture that may be stored or removed since.
    pub fn patched(
        &self,
        patch: serde_json::Map<String, serde_json::Value>,
    ) -> Result<Self, String> {
        fn lay_over(base: &mut serde_json::Value, patch: serde_json::Value) {
            match (base, patch) {
                (serde_json::Value::Object(base), serde_json::Value::Object(patch)) => {
                    for (key, value) in patch {
                        lay_over(base.entry(key).or_insert(serde_json::Value::Null), value);
                    }
                }
                (base, patch) => *base = patch,
            }
        }
        let mut merged = serde_json::to_value(self).map_err(|e| e.to_string())?;
        lay_over(&mut merged, serde_json::Value::Object(patch));
        let wanted: Self = serde_json::from_value(merged).map_err(|e| e.to_string())?;
        let mut wanted = wanted.sanitised();
        if !wanted.downloads.folder.is_empty() {
            wanted.downloads.folder.clone_from(&self.downloads.folder);
        }
        for (id, look) in &mut wanted.appearance.looks {
            let kept = self.appearance.look(id);
            look.sidebar_image = kept.sidebar_image;
            look.empty_image = kept.empty_image;
            if look.sidebar_image.is_none() && look.sidebar_effect == SidebarEffect::Image {
                look.sidebar_effect = SidebarEffect::None;
            }
        }
        Ok(Self {
            sync: self.sync.clone(),
            pinned_extensions: self.pinned_extensions.clone(),
            profile_pins: self.profile_pins.clone(),
            chromium_pins_seen: self.chromium_pins_seen.clone(),
            smart_allowed: self.smart_allowed.clone(),
            shortcuts: self.shortcuts.clone(),
            shortcut_priority: self.shortcut_priority.clone(),
            shortcut_sites: self.shortcut_sites.clone(),
            ..wanted
        })
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_archive_hours: 12,
            archive_days: 90,
            history_days: 90,
            unload_minutes: 240,
            defaults: DEFAULTS,
            search_template: "https://duckduckgo.com/?q=%s".into(),
            site_searches: builtin_site_searches(),
            external_links: "mini".into(),
            link_rules: Vec::new(),
            site_code: Vec::new(),
            shortcuts: magnetite_engine::Overrides::default(),
            shortcut_priority: magnetite_engine::Priorities::default(),
            shortcut_sites: Vec::new(),
            quick_search: true,
            chrome_toolbar: false,
            appearance: Appearance::default(),
            auto_pip: true,
            several_windows: false,
            strip_trackers: true,
            panel_autoclose: false,
            devtools_dock: magnetite_engine::DevToolsDock::Right,
            devtools_size: magnetite_engine::DevToolsSize::default(),
            sync: SyncSettings::default(),
            pinned_extensions: Vec::new(),
            profile_pins: HashMap::new(),
            chromium_pins_seen: HashMap::new(),
            check_updates: true,
            library_opens: LibraryView::Archive,
            tab_order: TabOrder::Recent,
            autofill: magnetite_engine::AutofillPrefs::default(),
            global_privacy_control: false,
            downloads: magnetite_engine::DownloadPrefs::default(),
            page_zoom: 100,
            languages: magnetite_engine::LanguagePrefs::default(),
            smart_allowed: None,
            adblock: AdblockSettings::default(),
            developer: crate::dev::DeveloperSettings::default(),
            system_engine: SystemEngine::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(site: &str, space: &str) -> LinkRule {
        LinkRule {
            site: site.into(),
            space: space.into(),
        }
    }

    #[test]
    fn a_typed_site_is_kept_by_its_name() {
        let site = |typed: &str| typed_site(typed);
        assert_eq!(
            site("https://www.Figma.com/design/1").as_deref(),
            Some("figma.com")
        );
        assert_eq!(site(" app.example.org ").as_deref(), Some("example.org"));
        assert_eq!(site("localhost:3000").as_deref(), Some("localhost"));
        assert_eq!(site("not a site"), None);
        assert_eq!(site(" "), None);
        // An IP address stays whole, typed again too.
        for typed in ["[2001:db8::1]", "2001:db8::1", "http://[2001:db8::1]:8080/"] {
            assert_eq!(site(typed).as_deref(), Some("2001:db8::1"), "{typed}");
        }
        assert_eq!(site("127.0.0.1:8080").as_deref(), Some("127.0.0.1"));
    }

    #[test]
    fn a_settings_patch_leaves_the_shortcut_sites() {
        let before = Settings {
            shortcut_sites: vec!["example.org".into()],
            ..Settings::default()
        };
        let patch = serde_json::json!({ "shortcut_sites": ["example.com"] });
        let serde_json::Value::Object(patch) = patch else {
            unreachable!()
        };
        let after = before.patched(patch).expect("a patch");
        assert_eq!(after.shortcut_sites, ["example.org"]);
    }

    #[test]
    fn a_link_goes_to_the_space_of_the_rule_that_fits_it_best() {
        let settings = Settings {
            link_rules: vec![
                rule("github.com", "personal"),
                rule("github.com/acme", "work"),
                rule("mail.example.org", "mail"),
            ],
            ..Settings::default()
        };
        let space = |url: &str| settings.link_space(url, |_| true).map(String::as_str);
        assert_eq!(
            space("https://github.com/acme/app/pull/1?x=1"),
            Some("work")
        );
        assert_eq!(space("https://github.com/acme"), Some("work"));
        assert_eq!(space("https://github.com/acmecorp/app"), Some("personal"));
        assert_eq!(space("https://www.github.com/"), Some("personal"));
        assert_eq!(space("https://gist.github.com/someone"), Some("personal"));
        assert_eq!(space("http://mail.example.org:8080/inbox"), Some("mail"));
        assert_eq!(space("https://example.org/"), None);
        assert_eq!(space("https://notgithub.com/acme"), None);
        assert_eq!(space("file:///github.com/acme"), None);
        // The page that loads counts, not the address as written.
        assert_eq!(
            space("https://github.com/acme/../other/repo"),
            Some("personal")
        );
        assert_eq!(
            space("https://github.com/acme/%2E%2e/other"),
            Some("personal")
        );
        assert_eq!(
            space("https://github.com/other/./../acme/app/"),
            Some("work")
        );
        // The space of the closest rule is gone: the next one counts.
        let without_work = settings.link_space("https://github.com/acme/app", |id| id != "work");
        assert_eq!(without_work.map(String::as_str), Some("personal"));
    }

    #[test]
    fn link_rules_keep_a_site_each_as_host_and_path() {
        let kept = Settings {
            link_rules: vec![
                rule(" https://WWW.GitHub.com/acme/ ", "work"),
                rule("github.com/acme", "again"),
                rule("not a site", "x"),
                rule("", "x"),
            ],
            ..Settings::default()
        }
        .sanitised();
        assert_eq!(kept.link_rules, [rule("github.com/acme", "work")]);
        // An address inside a path is part of the path.
        let nested = rule("archive.example/https://target.example", "x").sanitised();
        assert_eq!(
            nested.unwrap().site,
            "archive.example/https://target.example"
        );
    }

    #[test]
    fn own_code_is_kept_per_host_and_within_limits() {
        let code = |site: &str, css: &str| magnetite_engine::SiteCode {
            site: site.into(),
            css: css.into(),
            js: String::new(),
            enabled: true,
        };
        let kept = Settings {
            site_code: vec![
                code(" https://WWW.Example.org/some/page ", "a"),
                code("example.org", "again"),
                code("not a site", "x"),
                code("example.net", &"x".repeat(MAX_SITE_CODE + 1)),
            ],
            ..Settings::default()
        }
        .sanitised();
        assert_eq!(kept.site_code, [code("example.org", "a")]);
        // An address inside a path is part of the path, not the site.
        assert_eq!(
            site_host("archive.example/https://target.example").as_deref(),
            Some("archive.example")
        );
    }

    #[test]
    fn a_patch_can_take_the_download_folder_away_but_not_name_one() {
        let mut start = Settings::default();
        assert_eq!(start.downloads, magnetite_engine::DownloadPrefs::default());
        // A whole path, as each system writes one.
        let chosen = if cfg!(windows) {
            r"C:\chosen"
        } else {
            "/tmp/chosen"
        };
        start.downloads.folder = chosen.into();
        let patch = |value: serde_json::Value| {
            let serde_json::Value::Object(patch) = value else {
                panic!("an object");
            };
            start.patched(patch).unwrap().downloads
        };
        let asked = patch(serde_json::json!({ "downloads": { "ask": true } }));
        assert_eq!((asked.folder.as_str(), asked.ask), (chosen, true));
        let other = if cfg!(windows) { r"C:\other" } else { "/etc" };
        let named = patch(serde_json::json!({ "downloads": { "folder": other } }));
        assert_eq!(named.folder, chosen, "only the dialog names a folder");
        let reset = patch(serde_json::json!({ "downloads": { "folder": "" } }));
        assert_eq!(reset.folder, "");
        // A path that is not whole is none.
        let mut odd = Settings::default();
        odd.downloads.folder = "Downloads".into();
        assert_eq!(odd.sanitised().downloads.folder, "");
    }

    #[test]
    fn the_page_zoom_is_one_of_chromiums_steps() {
        let zoom = |pct: u32| {
            let s = Settings {
                page_zoom: pct,
                ..Settings::default()
            };
            s.sanitised().page_zoom
        };
        assert_eq!(Settings::default().page_zoom, 100);
        assert_eq!(zoom(125), 125);
        assert_eq!(zoom(0), 100);
        assert_eq!(zoom(123), 100);
        assert_eq!(zoom(5000), 100);
    }

    #[test]
    fn autofill_switches_start_safe_and_change_one_at_a_time() {
        let start = Settings::default();
        assert!(start.autofill.passwords && !start.autofill.breach_check);
        let patch = serde_json::json!({ "autofill": { "cards": false } });
        let serde_json::Value::Object(patch) = patch else {
            unreachable!()
        };
        let changed = start.patched(patch).unwrap();
        assert!(!changed.autofill.cards);
        assert!(changed.autofill.passwords && changed.autofill.addresses);
        assert!(!changed.autofill.breach_check);
    }

    #[test]
    fn a_page_changes_only_what_it_sends() {
        let mut saved = Settings {
            archive_days: 3650,
            ..Settings::default()
        };
        saved.appearance.sidebar_width = 300;
        saved.sync.enabled = true;
        saved.sync.server_url = "https://sync.example.org".into();
        let patch = |json: serde_json::Value| match json {
            serde_json::Value::Object(map) => map,
            _ => unreachable!("a patch is an object"),
        };
        // One toggle from a page with an old copy: nothing else moves.
        let next = saved
            .patched(patch(serde_json::json!({ "auto_pip": false })))
            .unwrap();
        assert!(!next.auto_pip);
        assert_eq!(next.archive_days, 3650);
        assert_eq!(next.appearance.sidebar_width, 300);
        // A part of a nested setting leaves its other parts alone.
        let next = saved
            .patched(patch(
                serde_json::json!({ "appearance": { "text_scale": 110 } }),
            ))
            .unwrap();
        assert_eq!(
            (next.appearance.text_scale, next.appearance.sidebar_width),
            (110, 300)
        );
        // No page turns sync off or points it at another server this way.
        let next = saved
            .patched(patch(serde_json::json!({ "sync": { "enabled": false, "server_url": "https://x.example" } })))
            .unwrap();
        assert!(next.sync.enabled);
        assert_eq!(next.sync.server_url, "https://sync.example.org");
        // Values are checked as always.
        let next = saved
            .patched(patch(serde_json::json!({ "archive_days": 4000 })))
            .unwrap();
        assert_eq!(next.archive_days, MAX_ARCHIVE_DAYS);
    }

    #[test]
    fn settings_saved_with_the_first_unload_time_get_the_current_one() {
        let saved = |json: serde_json::Value| -> Settings {
            serde_json::from_value::<Settings>(json)
                .unwrap()
                .with_current_defaults()
        };
        // Saved before defaults were counted: 30 minutes was the default.
        assert_eq!(
            saved(serde_json::json!({ "unload_minutes": 30 })).unload_minutes,
            240
        );
        // Anything else was chosen.
        assert_eq!(
            saved(serde_json::json!({ "unload_minutes": 10 })).unload_minutes,
            10
        );
        assert_eq!(
            saved(serde_json::json!({ "unload_minutes": 0 })).unload_minutes,
            0
        );
        // Once only: 30 minutes chosen after that stays.
        let chosen = saved(serde_json::json!({ "unload_minutes": 30, "defaults": 1 }));
        assert_eq!(chosen.unload_minutes, 30);
    }

    #[test]
    fn settings_from_the_page_or_another_device_are_checked() {
        let bad = Settings {
            auto_archive_hours: 0,
            search_template: "http://search.example/?q=%s".into(),
            external_links: "window".into(),
            ..Settings::default()
        }
        .sanitised();
        assert_eq!(bad.auto_archive_hours, 1);
        let never = Settings {
            auto_archive_hours: NEVER_ARCHIVE,
            ..Settings::default()
        };
        assert_eq!(never.sanitised().auto_archive_hours, NEVER_ARCHIVE);
        assert_eq!(bad.search_template, Settings::default().search_template);
        assert_eq!(bad.external_links, "mini");
        for known in ["tab", "peek", "mini"] {
            let s = Settings {
                external_links: known.into(),
                ..Settings::default()
            };
            assert_eq!(s.sanitised().external_links, known);
        }
        // Saved before the quick search existed: it starts on.
        let older: Settings = serde_json::from_str(r#"{"external_links":"peek"}"#).unwrap();
        assert!(older.quick_search);
        let fine = Settings {
            search_template: "https://kagi.com/search?q=%s".into(),
            ..Settings::default()
        }
        .sanitised();
        assert_eq!(fine.search_template, "https://kagi.com/search?q=%s");
    }

    /// Appearance saved before the top bar existed keeps its values and
    /// leaves the top bar off; a top bar turned on stays on once saved.
    #[test]
    fn appearance_saved_before_the_top_bar_loads_with_it_off() {
        let older: Appearance =
            serde_json::from_str(r#"{"text_scale":110,"density":"compact","sidebar_width":300}"#)
                .unwrap();
        assert_eq!(
            older,
            Appearance {
                text_scale: 110,
                density: Density::Compact,
                sidebar_width: 300,
                ..Appearance::default()
            }
        );
        let on = Appearance {
            top_bar: true,
            ..older
        };
        let saved = serde_json::to_string(&on).unwrap();
        assert_eq!(serde_json::from_str::<Appearance>(&saved).unwrap(), on);
    }

    #[test]
    fn the_known_services_open_in_the_system_engine_until_turned_off() {
        let engine = Settings::default().system_engine;
        assert_eq!(engine.hosts(), PROTECTED_VIDEO_SITES);
        let own = SystemEngine {
            known: false,
            sites: vec!["tv.example.org".into()],
        };
        assert_eq!(own.hosts(), ["tv.example.org"]);
    }

    #[test]
    fn a_known_service_can_be_named_alone_while_the_known_ones_are_off() {
        let mut engine = SystemEngine {
            known: false,
            sites: Vec::new(),
        };
        engine
            .set_site("www.netflix.com", "netflix.com".into(), true)
            .unwrap();
        // As after any later change of the settings.
        let kept = engine.sanitised();
        assert_eq!(kept.sites, ["netflix.com"]);
        assert!(kept.names("www.netflix.com") && !kept.knows("www.netflix.com"));
        // With the known services on again they cover it.
        let known = SystemEngine {
            known: true,
            ..kept
        }
        .sanitised();
        assert!(known.sites.is_empty() && known.knows("www.netflix.com"));
    }

    #[test]
    fn the_site_panel_cannot_name_more_sites_than_are_kept() {
        let mut engine = SystemEngine {
            known: true,
            sites: (0..MAX_SYSTEM_ENGINE_SITES)
                .map(|n| format!("tv{n}.example.org"))
                .collect(),
        };
        let full = engine.sites.clone();
        assert!(
            engine
                .set_site("more.example.org", "more.example.org".into(), true)
                .is_err()
        );
        assert_eq!(engine.sites, full);
        // Taking one out, and naming one that is there already, still work.
        engine
            .set_site("tv0.example.org", "tv0.example.org".into(), true)
            .unwrap();
        engine
            .set_site("tv1.example.org", "tv1.example.org".into(), false)
            .unwrap();
        assert_eq!(engine.sites.len(), MAX_SYSTEM_ENGINE_SITES - 1);
    }

    #[test]
    fn own_sites_for_the_system_engine_are_hosts_each_once() {
        let typed = SystemEngine {
            known: true,
            sites: vec![
                "https://www.TV.example.org/watch?v=1".into(),
                "tv.example.org".into(),
                "not a host".into(),
                // One of the known services: the switch above covers it.
                "netflix.com".into(),
                "music.example.com".into(),
            ],
        };
        let kept = typed.sanitised();
        assert_eq!(kept.sites, ["tv.example.org", "music.example.com"]);
    }

    #[test]
    fn the_ad_blocker_starts_off_and_keeps_only_known_lists() {
        assert_eq!(Settings::default().adblock, AdblockSettings::default());
        let chosen = AdblockSettings {
            enabled: true,
            lists: vec![
                "easylist".into(),
                "made-up".into(),
                "easyprivacy".into(),
                "easylist".into(),
            ],
            allowed_sites: vec![
                " News.example. ".into(),
                "news.example".into(),
                String::new(),
            ],
        };
        let kept = chosen.sanitised();
        assert_eq!(kept.lists, ["easylist", "easyprivacy"]);
        assert_eq!(kept.allowed_sites, ["news.example"]);
    }

    #[test]
    fn site_searches_are_checked() {
        let site = |keyword: &str, name: &str, url: &str| SiteSearch {
            keyword: keyword.into(),
            name: name.into(),
            url: url.into(),
        };
        let checked = Settings {
            site_searches: vec![
                site(" GH ", "GitHub", "https://github.com/search?q=%s"),
                site("gh", "Again", "https://example.org/?q=%s"),
                site("plain", "Plain", "http://example.org/?q=%s"),
                site("noquery", "No query", "https://example.org/"),
                site("two words", "Space", "https://example.org/?q=%s"),
                site("x.y", "Dot", "https://example.org/?q=%s"),
                site("nameless", " ", "https://example.org/?q=%s"),
                site("js", "Script", "javascript:alert(%s)"),
            ],
            ..Settings::default()
        }
        .sanitised();
        assert_eq!(
            checked.site_searches,
            [site("gh", "GitHub", "https://github.com/search?q=%s")]
        );
        assert!(
            Settings::default()
                .site_searches
                .iter()
                .all(|s| s.clone().sanitised().as_ref() == Some(s))
        );
    }

    #[test]
    fn a_patch_keeps_the_stamps_of_own_images() {
        let mut start = Settings::default();
        start.appearance.looks.insert(
            "work".into(),
            SpaceLook {
                empty_image: Some(7),
                ..SpaceLook::default()
            },
        );
        // A page that last saw a sidebar image and no empty-window image saves the strength.
        let serde_json::Value::Object(stale) = serde_json::json!({ "appearance": { "looks": { "work": {
            "tint": 20, "sidebar_effect": "image", "sidebar_image": 3, "empty_image": null,
        } } } }) else {
            unreachable!()
        };
        let look = start.patched(stale).unwrap().appearance.look("work");
        assert_eq!(look.tint, 20);
        assert_eq!(look.empty_image, Some(7));
        assert_eq!(look.sidebar_image, None);
        assert_eq!(look.sidebar_effect, SidebarEffect::None);
    }
}
