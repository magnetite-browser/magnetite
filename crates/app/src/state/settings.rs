//! User preferences: what they are, their defaults, and the checks every
//! change goes through, whether it comes from the Settings page or from
//! another device.

use std::collections::HashMap;

use magnetite_core::UnixSeconds;
use serde::{Deserialize, Serialize};

/// The longest the archive may keep a tab: ten years.
pub const MAX_ARCHIVE_DAYS: u32 = 3650;

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
    /// Today tabs unused this long are archived. Minimum 1 h.
    pub auto_archive_hours: u32,
    /// Archived tabs are deleted after this many days (1 to `MAX_ARCHIVE_DAYS`).
    pub archive_days: u32,
    /// Visits stay in the history this many days; 0 keeps none (`history.rs`).
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
    /// Links from other apps: "tab", "peek" or "mini" (a mini window).
    /// Synced: a version that does not know a value takes "tab"
    /// (`sanitised`).
    pub external_links: String,
    /// The system-wide quick search key (⌥⌘N) is registered. This
    /// device's own choice, never synced.
    pub quick_search: bool,
    /// Show Chrome's toolbar row in pages (extension buttons).
    pub chrome_toolbar: bool,
    /// How the sidebar and Magnetite's own pages look.
    pub appearance: Appearance,
    /// Pop a playing video out (picture-in-picture) when leaving its tab.
    pub auto_pip: bool,
    /// Remove tracking parameters (utm_*, fbclid, …) when copying links.
    pub strip_trackers: bool,
    /// Where DevTools open: right of or below the page, or a separate window.
    pub devtools_dock: magnetite_engine::DevToolsDock,
    #[serde(default)]
    pub sync: SyncSettings,
    /// Extensions (ids) shown in the sidebar; all of them are in the menu
    /// bar's Extensions menu.
    pub pinned_extensions: Vec<String>,
    /// Chromium's own pins as last seen, per profile id (`""`: default), so
    /// only changes made there carry over (`follow_chromium_pins`).
    pub chromium_pins_seen: HashMap<String, Vec<String>>,
    /// Look for a new release at start and once a day (`update.rs`).
    pub check_updates: bool,
    /// What a click on the sidebar's archive and downloads button opens.
    pub library_opens: LibraryView,
    /// Chromium's password manager and autofill: which parts are on. Not
    /// synced: what a device saves is that device's.
    pub autofill: magnetite_engine::AutofillPrefs,
    /// Smart folders allowed to fetch on this device (`smart_key`): those made
    /// or edited here, or turned on here. A folder another device synced
    /// waits, so the words alone cannot make this device's accounts run a
    /// query. Never synced. `None` before the first start that knew this.
    #[serde(default)]
    pub smart_allowed: Option<Vec<String>>,
    /// The ad blocker. Not synced: each device downloads its own lists.
    pub adblock: AdblockSettings,
}

/// The ad blocker (Settings → Ad blocking, `adblock.rs`). Off, with no list,
/// until the user turns it on and picks the lists.
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
    /// Known lists only, and sites as lower-case names, each once.
    fn sanitised(mut self) -> Self {
        let mut lists: Vec<String> = Vec::new();
        for id in std::mem::take(&mut self.lists) {
            if crate::adblock::list(&id).is_some() && !lists.contains(&id) {
                lists.push(id);
            }
        }
        self.lists = lists;
        let mut sites: Vec<String> = (self.allowed_sites.iter())
            .map(|s| s.trim().trim_end_matches('.').to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        sites.sort();
        sites.dedup();
        self.allowed_sites = sites;
        self
    }
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
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            text_scale: 100,
            density: Density::Comfortable,
            sidebar_width: 264,
        }
    }
}

impl Appearance {
    /// Keep values inside what the layout handles.
    pub fn clamped(mut self) -> Self {
        self.text_scale = self.text_scale.clamp(85, 130);
        self.sidebar_width = Self::sidebar_width(self.sidebar_width);
        self
    }

    /// The docked sidebar's width kept between 220 and 360 points.
    pub fn sidebar_width(width: u32) -> u32 {
        width.clamp(220, 360)
    }
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
}

impl Settings {
    /// The settings as Magnetite accepts them, whether from the Settings page
    /// or from another device: archiving after at least an hour, retention
    /// periods in range, appearance in range, search addresses on https with
    /// `%s` in them (site searches that are not, or repeat a keyword, are
    /// dropped), a known choice for external links.
    pub fn sanitised(mut self) -> Self {
        let defaults = Self::default();
        self.auto_archive_hours = self.auto_archive_hours.max(1);
        self.archive_days = self.archive_days.clamp(1, MAX_ARCHIVE_DAYS);
        self.history_days = self.history_days.min(MAX_ARCHIVE_DAYS);
        self.appearance = self.appearance.clamped();
        self.adblock = self.adblock.sanitised();
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
        self
    }
}

impl Settings {
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
    /// extensions, and the smart folders allowed on this device.
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
        Ok(Self {
            sync: self.sync.clone(),
            pinned_extensions: self.pinned_extensions.clone(),
            chromium_pins_seen: self.chromium_pins_seen.clone(),
            smart_allowed: self.smart_allowed.clone(),
            ..wanted.sanitised()
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
            external_links: "tab".into(),
            quick_search: true,
            chrome_toolbar: false,
            appearance: Appearance::default(),
            auto_pip: true,
            strip_trackers: true,
            devtools_dock: magnetite_engine::DevToolsDock::Right,
            sync: SyncSettings::default(),
            pinned_extensions: Vec::new(),
            chromium_pins_seen: HashMap::new(),
            check_updates: true,
            library_opens: LibraryView::Archive,
            autofill: magnetite_engine::AutofillPrefs::default(),
            smart_allowed: None,
            adblock: AdblockSettings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            .patched(patch(serde_json::json!({ "archive_days": 0 })))
            .unwrap();
        assert_eq!(next.archive_days, 1);
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
        assert_eq!(bad.search_template, Settings::default().search_template);
        assert_eq!(bad.external_links, "tab");
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
}
