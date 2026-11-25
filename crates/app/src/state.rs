//! Application state: the sidebar snapshot plus per-tab runtime data, and
//! its persistence. Pure data and logic; no engine calls here.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use magnetite_core::split::SplitNode;
use magnetite_core::{Id, SidebarSnapshot, Space, TabItem, UnixSeconds};
use magnetite_engine::{ContentId, WindowId};
use serde::{Deserialize, Serialize};

mod adopt;
mod bookmarks;
mod folders;
mod history;
pub mod import;
mod panels;
mod persist;
mod phone_edits;
mod private;
mod profiles;
mod search;
mod settings;
mod share;
mod space_keys;
mod splits;
mod tabs;
#[cfg(test)]
pub(crate) mod testing;
mod windows;

pub use adopt::icon_image_ok;
pub(crate) use folders::folder_in_tree_mut;
pub use history::{History, Visit};
pub use private::ShowIn;
pub use profiles::ChosenProfile;
pub use settings::{
    AdblockSettings, Appearance, NewTabLinks, NewTabPlace, PROTECTED_VIDEO_SITES, Settings,
    SidebarEffect, SiteSearch, SyncSettings, SystemEngine, TabOrder, typed_site,
};
pub use share::read_link as read_shared;
pub use tabs::TabSwitch;

/// What the sidebar UI receives on every change.
#[derive(Serialize)]
pub struct UiState<'a> {
    pub sidebar: Cow<'a, SidebarSnapshot>,
    pub active_space: &'a str,
    /// The private space, while one is open (`private.rs`).
    pub private_space: Option<&'a str>,
    pub active_tab: Option<&'a str>,
    pub runtime: &'a HashMap<Id, TabRuntime>,
    pub sidebar_visible: bool,
    pub archive_count: usize,
    /// Tabs open on other devices of the sync chain; a click takes one over.
    pub device_tabs: &'a [crate::devices::RemoteTab],
    /// Visits in the history: the history view asks again when it changes.
    pub history_count: usize,
    /// Tabs currently on screen in the active space (one, or a split's pages).
    pub panes: Vec<Id>,
    /// The split on screen (two or more pages), if any.
    pub split: Option<SplitNode<Id>>,
    /// Every split, on screen or not; the sidebar shows each as one row.
    pub splits: Vec<SplitNode<Id>>,
    pub peek: Option<&'a Peek>,
    /// The favorite whose web panel is open.
    pub web_panel: Option<&'a str>,
    /// The tabs `Ctrl+Tab` is going through and where it stands, while
    /// Ctrl is held: what the switcher's list shows.
    pub switcher: Option<&'a TabSwitch>,
    pub settings: &'a Settings,
    /// The keys each shortcut has now (as `Combo` saves them), for the
    /// hints beside buttons and menu entries; one without keys is left out.
    pub keys: std::collections::BTreeMap<magnetite_engine::Action, String>,
    /// The keys that go to a space, by space; one without keys of its own
    /// is left out.
    pub space_keys: std::collections::BTreeMap<&'a str, String>,
    /// Chrome colour behind the page cards, `#rrggbb`, derived from the space accent.
    pub chrome: String,
    /// Connected smart folder accounts (never their tokens).
    pub accounts: Vec<crate::smart::Account>,
    /// Refresh state per smart folder id.
    pub smart: &'a HashMap<Id, SmartStatus>,
    /// Installed extensions of the profile the page in front uses.
    pub extensions: &'a [magnetite_engine::ExtensionInfo],
    /// Those of them pinned to the sidebar in that profile, in order.
    pub pinned_extensions: &'a [String],
    /// The tabs whose rows offer to share them in place of the tab a page
    /// shares now (`AppState::share_targets`); none while the CEF in use
    /// cannot switch a share.
    pub share_targets: Vec<&'a str>,
}

/// Refresh state of a smart folder, for the sidebar and the dialog.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SmartStatus {
    pub fetching: bool,
    /// Last attempt, successful or not; spaces out retries.
    pub checked_at: Option<UnixSeconds>,
    /// Last successful refresh.
    pub updated_at: Option<UnixSeconds>,
    pub error: Option<String>,
    /// No account is connected for the folder's service and server.
    pub needs_account: bool,
    /// Synced from another device and not turned on here yet.
    pub needs_approval: bool,
}

/// A page outside the sidebar: the one in the peek overlay, or one in a
/// mini window (`AppState::minis`). Not part of the sidebar until the user
/// keeps it as a tab.
#[derive(Debug, Clone, Serialize)]
pub struct Peek {
    #[serde(skip)]
    pub content: ContentId,
    pub url: String,
    pub title: String,
    /// What is known about the page, as for a tab's (history, what it
    /// plays, its connection). A peek kept as a tab starts with it.
    pub runtime: TabRuntime,
    /// Profile id the page runs in.
    #[serde(skip)]
    pub profile: String,
}

/// What a peek lies over (`peek.md`): the tab in front when it opened,
/// which it stays with, or a window with no tab in front, until one is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PeekOver {
    Tab(Id),
    Window(WindowId),
}

impl PeekOver {
    /// Over `tab`, the tab in front of window `win`, or over `win` itself
    /// while it has none.
    pub fn front(tab: Option<&Id>, win: WindowId) -> Self {
        tab.map_or(Self::Window(win), |t| Self::Tab(t.clone()))
    }
}

/// The phone of a responsive split: a second page of tab `tab`, shown right
/// of it while that tab is on screen. No tab of its own: it is in no
/// sidebar, archive or sync, and goes with its tab.
#[derive(Debug, Clone)]
pub struct Phone {
    pub tab: Id,
    pub page: Peek,
    /// Its width in points.
    pub width: i32,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TabRuntime {
    #[serde(skip)]
    pub content: Option<ContentId>,
    pub loaded: bool,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub favicon: Option<String>,
    /// What the page plays, if anything (see `magnetite_engine::MediaState`).
    pub media: Option<magnetite_engine::MediaState>,
    pub muted: bool,
    /// Connection and zoom of the page (address row chip, site panel).
    pub site: Option<magnetite_engine::SiteState>,
    /// The page uses the camera / microphone right now.
    pub camera: bool,
    pub microphone: bool,
    /// A page shares this tab's picture right now (screen sharing).
    #[serde(default)]
    pub shared: bool,
    /// This tab's page shares a tab right now (another one, or its own).
    #[serde(default)]
    pub sharing: bool,
    /// For developers: whether the page is a local one, its console errors,
    /// what it is shown as (`dev.rs`).
    #[serde(default)]
    pub dev: crate::dev::DevState,
    /// The tab whose page opened this one and under which it was placed
    /// (`AppState::place_below_opener`). In memory only.
    #[serde(skip)]
    pub opener: Option<Id>,
}

impl TabRuntime {
    /// Forget the tab's page window (made again when the tab is shown): the
    /// tab is no longer loaded, loading or playing, nor muted, and nothing
    /// is known about its connection or its use of camera and microphone,
    /// and it is no longer shared or sharing.
    /// Returns the window for the caller to close.
    pub fn unload(&mut self) -> Option<ContentId> {
        self.loaded = false;
        self.loading = false;
        self.media = None;
        self.muted = false;
        self.site = None;
        self.camera = false;
        self.microphone = false;
        self.shared = false;
        self.sharing = false;
        self.dev = crate::dev::DevState::default();
        self.content.take()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivedTab {
    pub tab: TabItem,
    pub space_id: Id,
    pub archived_at: UnixSeconds,
}

/// What a start does about other browsers' data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcImport {
    /// Nothing by itself: the first start shows the welcome page, where the
    /// user chooses what to bring along.
    Ask,
    /// Arc's sidebar in place of the saved one, on this start (`--import-arc`).
    Now,
    /// Nothing, and no welcome page (`--no-import`, for tests: a plain
    /// sidebar, and no look at which browsers are installed).
    Never,
}

pub struct AppState {
    pub sidebar: SidebarSnapshot,
    /// The window `active_space`, `active_tab`, `peek`, `web_panel` and
    /// `sidebar_visible` are of: the current one (`windows.rs`).
    pub window: WindowId,
    /// What the other windows show.
    pub views: HashMap<WindowId, windows::View>,
    /// The other windows of the last run, opened again at start.
    pub reopen: Vec<windows::SavedWindow>,
    pub active_space: Id,
    /// Active tab per space.
    pub active_tab: HashMap<Id, Id>,
    pub runtime: HashMap<Id, TabRuntime>,
    pub content_to_tab: HashMap<ContentId, Id>,
    pub archive: Vec<ArchivedTab>,
    /// Pages visited on this device (`history.json`, saved apart).
    pub history: History,
    /// Split views: which tabs tile the page area together, and how. A tab
    /// is in at most one; the one holding the active tab is on screen.
    pub splits: Vec<SplitNode<Id>>,
    /// The peeks, by what they lie over. The one over what is in front of
    /// a window shows there (`AppState::peek`); the others wait hidden
    /// until their tab is in front again. In memory only.
    pub peeks: HashMap<PeekOver, Peek>,
    /// The favorite whose web panel is open (`panels.rs`). In memory only.
    pub web_panel: Option<Id>,
    /// Pages in mini windows, in the order they opened. Like the peek, they
    /// live in memory only.
    pub minis: Vec<Peek>,
    /// The phones of responsive splits (`dev_responsive`), in memory only.
    pub phones: Vec<Phone>,
    pub settings: Settings,
    /// Tabs in the order they were last chosen, the latest first: what
    /// `Ctrl+Tab` goes through. In memory only; a tab not in it counts by
    /// its `last_active_at`.
    pub recent_tabs: Vec<Id>,
    /// The round `Ctrl+Tab` is on while Ctrl is held (`TabSwitch`).
    pub switcher: Option<TabSwitch>,
    /// Folders the sidebar shows closed: it keeps that itself and reports
    /// it (`set_folder_open`). `Ctrl+Tab` skips their tabs. In memory only.
    pub closed_folders: HashSet<Id>,
    pub sidebar_visible: bool,
    /// Smart folder accounts of this device (tokens on disk, owner-only).
    pub accounts: crate::smart::Accounts,
    pub smart_status: HashMap<Id, SmartStatus>,
    pub downloads: crate::downloads::Downloads,
    /// The sidebar as this device would have sent it right after it last
    /// took one from sync: while what it would send now is still this, the
    /// device has changed nothing and writes nothing. Not saved.
    pub synced_sidebar: Option<SidebarSnapshot>,
    /// The tabs open on the chain's other devices, as of the last sync.
    /// Not saved.
    pub device_tabs: Vec<crate::devices::RemoteTab>,
    /// Nothing was saved in this data folder before this start. Not saved.
    pub first_start: bool,
    /// Spaces kept on this device alone ("Only on this device"). The
    /// device's own choice, never synced: such a space is not sent
    /// (`sidebar_for_sync`), and a sidebar that arrives leaves it as it is
    /// (`adopt_sidebar`). A profile that only such spaces use stays with
    /// them (`unsynced_profiles`).
    pub unsynced_spaces: BTreeSet<Id>,
    /// Profiles deleted on this device, oldest first. The device's own
    /// memory, never synced: a deleted profile is not taken back from
    /// another device's list (`adopt_sidebar`), and its data folder goes at
    /// the next start (`deleted_profile_data`).
    pub deleted_profiles: Vec<Id>,
    /// The private space, while one is open (`private.rs`). Not saved, like
    /// the space itself.
    pub private_space: Option<Id>,
    /// The private space is hidden (`hide_private_space`): open, with its
    /// pages, but listed nowhere and in front in no window. Not saved.
    pub private_hidden: bool,
    /// The window the private space is listed in while it is open: the one
    /// that opened it or last went to it. No other window lists it
    /// (`lists_space`). Not saved.
    pub private_home: WindowId,
    /// The private space has its window to itself (Settings → General,
    /// "Private space in its own window"): that window lists no other
    /// space, and the private space ends with it. Not saved.
    pub private_alone: bool,
    /// The in-memory profiles made for the private space: the space's own
    /// and one per tab. Not saved, never synced, and each is dropped once
    /// nothing runs in it (`release_private_profiles`).
    pub private_profiles: BTreeSet<Id>,
    path: PathBuf,
    saver: persist::Saver,
}

pub fn now() -> UnixSeconds {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

pub fn new_id() -> Id {
    // Time + counter is unique enough for local ids and keeps us dependency-free.
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}", now(), n ^ u64::from(std::process::id()))
}

impl AppState {
    pub fn data_dir(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new("."))
    }

    pub fn ui_state<'a>(
        &'a self,
        extensions: &'a [magnetite_engine::ExtensionInfo],
    ) -> UiState<'a> {
        UiState {
            sidebar: self.ui_sidebar(),
            active_space: &self.active_space,
            private_space: self.shown_private_space().filter(|id| self.lists_space(id)),
            active_tab: self.active_tab.get(&self.active_space).map(String::as_str),
            runtime: &self.runtime,
            sidebar_visible: self.sidebar_visible,
            archive_count: self.archive.len(),
            device_tabs: &self.device_tabs,
            history_count: self.history.len(),
            panes: self.visible_panes(),
            split: self.visible_split(),
            splits: self.live_splits().collect(),
            peek: self.peek(),
            web_panel: self.web_panel.as_deref(),
            switcher: self.switcher.as_ref(),
            settings: &self.settings,
            keys: self.settings.shortcut_keys().collect(),
            space_keys: (self.space_keys().into_iter())
                .map(|(space, keys)| (space.id.as_str(), keys.to_string()))
                .collect(),
            chrome: format!("#{:06x}", self.chrome_rgb()),
            accounts: self.accounts.list(),
            smart: &self.smart_status,
            extensions,
            pinned_extensions: self.pins(&self.front_context().map(|c| c.id).unwrap_or_default()),
            share_targets: if magnetite_engine::can_share_tab_instead() {
                self.share_targets()
            } else {
                Vec::new()
            },
        }
    }

    /// The sidebar as the current window's UI lists it: without the
    /// profiles of the private space, which are no choice for anything
    /// else, and with the spaces this window lists (`lists_space`).
    fn ui_sidebar(&self) -> Cow<'_, SidebarSnapshot> {
        // An open private space has a profile of its own.
        if self.private_profiles.is_empty() {
            return Cow::Borrowed(&self.sidebar);
        }
        let mut sidebar = self.sidebar.clone();
        (sidebar.profiles).retain(|p| !self.private_profiles.contains(&p.id));
        (sidebar.spaces).retain(|sp| self.lists_space(&sp.id));
        Cow::Owned(sidebar)
    }

    /// The key of profile `profile`'s pins: none for the default profile
    /// (also named by `""`), whose list is `settings.pinned_extensions`, and
    /// for a private profile, which runs the default profile's extensions.
    fn pin_key(&self, profile: &str) -> Option<String> {
        let other = (self.sidebar.profiles.iter())
            .any(|p| p.id == profile && !p.is_default && !p.ephemeral);
        (other && !self.private_profiles.contains(profile)).then(|| profile.to_string())
    }

    /// The extensions pinned to the sidebar in profile `profile`. A profile
    /// that never had a pin changed shows those of the default profile.
    pub fn pins(&self, profile: &str) -> &[String] {
        let own = self
            .pin_key(profile)
            .and_then(|key| self.settings.profile_pins.get(&key));
        own.unwrap_or(&self.settings.pinned_extensions)
    }

    /// The same, to change. From the first change on every lasting profile
    /// has its own list (started as what it showed until then), so a pin in
    /// one profile is not one in another.
    pub fn pins_mut(&mut self, profile: &str) -> &mut Vec<String> {
        let shown = self.settings.pinned_extensions.clone();
        for p in &self.sidebar.profiles {
            if !p.is_default && !p.ephemeral && !self.private_profiles.contains(&p.id) {
                let own = self.settings.profile_pins.entry(p.id.clone());
                own.or_insert_with(|| shown.clone());
            }
        }
        match self.pin_key(profile) {
            Some(key) => self.settings.profile_pins.entry(key).or_insert(shown),
            None => &mut self.settings.pinned_extensions,
        }
    }

    /// Follow pins made in Chromium's own UI ("Pin to toolbar" on its
    /// extensions page) for `profile`: an extension Chromium newly pins is
    /// pinned here too, one it unpins leaves the sidebar. Pins changed here
    /// stay as they are until Chromium's change. Returns whether any changed.
    /// A profile of the private space is not followed: its id would be saved
    /// with what was seen, and its pins are the default profile's anyway.
    pub fn follow_chromium_pins(&mut self, profile: &str, chromium: Vec<String>) -> bool {
        if self.private_profiles.contains(profile) {
            return false;
        }
        let seen = (self.settings.chromium_pins_seen.get(profile))
            .cloned()
            .unwrap_or_default();
        if seen == chromium {
            return false;
        }
        let pinned = self.pins_mut(profile);
        for id in chromium.iter().filter(|id| !seen.contains(id)) {
            if !pinned.contains(id) {
                pinned.push(id.clone());
            }
        }
        pinned.retain(|id| chromium.contains(id) || !seen.contains(id));
        (self.settings.chromium_pins_seen).insert(profile.to_string(), chromium);
        true
    }

    /// The active space's colour as `0xRRGGBB` (the sidebar's default blue
    /// when the space has none): outlines the focused page of a split.
    pub fn accent_rgb(&self) -> u32 {
        let Some(c) = self.space(&self.active_space).and_then(|s| s.accent) else {
            return 0x78_82FF;
        };
        // Clamped to 0..=255 first, so the cast is exact.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
        (byte(c.red) << 16) | (byte(c.green) << 8) | byte(c.blue)
    }

    /// Mix the active space accent into the base graphite, as much as
    /// Settings → Appearance says (8% unless changed), keeping every space's
    /// chrome at the same lightness.
    pub fn chrome_rgb(&self) -> u32 {
        const BASE: (f32, f32, f32) = (27.0, 27.0, 30.0); // #1b1b1e
        let accent = self.space(&self.active_space).and_then(|s| s.accent);
        // At most 30, so the cast is exact.
        #[allow(clippy::cast_precision_loss)]
        let tint = self
            .settings
            .appearance
            .look(&self.active_space)
            .tint
            .min(30) as f32
            / 100.0;
        let mix = |base: f32, accent: f32| base * (1.0 - tint) + accent * 255.0 * tint;
        let (r, g, b) = match accent {
            Some(c) => (
                mix(BASE.0, c.red),
                mix(BASE.1, c.green),
                mix(BASE.2, c.blue),
            ),
            None => BASE,
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ch = |v: f32| v.round().clamp(0.0, 255.0) as u32;
        (ch(r) << 16) | (ch(g) << 8) | ch(b)
    }

    pub fn space(&self, id: &str) -> Option<&Space> {
        self.sidebar.spaces.iter().find(|s| s.id == id)
    }

    pub fn space_mut(&mut self, id: &str) -> Option<&mut Space> {
        self.sidebar.spaces.iter_mut().find(|s| s.id == id)
    }

    pub fn active_space(&self) -> &Space {
        self.space(&self.active_space).expect("active space exists")
    }

    pub fn active_tab_id(&self) -> Option<&str> {
        self.active_tab.get(&self.active_space).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use crate::state::testing::state;

    #[test]
    fn pins_made_in_chromium_carry_over_once() {
        let mut s = state();
        let ids = |v: &[&str]| v.iter().map(|x| (*x).to_string()).collect::<Vec<_>>();
        s.settings.pinned_extensions = ids(&["ours"]);
        // Chromium pins "vault": it joins the sidebar.
        assert!(s.follow_chromium_pins("", ids(&["vault"])));
        assert_eq!(s.settings.pinned_extensions, ids(&["ours", "vault"]));
        // Removed here, still pinned in Chromium: stays removed.
        s.settings.pinned_extensions = ids(&["ours"]);
        assert!(!s.follow_chromium_pins("", ids(&["vault"])));
        assert_eq!(s.settings.pinned_extensions, ids(&["ours"]));
        // Chromium unpins "vault" and pins "ours" (already pinned here).
        s.settings.pinned_extensions = ids(&["ours", "vault"]);
        assert!(s.follow_chromium_pins("", ids(&["ours"])));
        assert_eq!(s.settings.pinned_extensions, ids(&["ours"]));
        // Another profile has its own record, and its own pins: they start
        // as the default profile's and part from them with the first change.
        s.sidebar.profiles.push(magnetite_core::Profile {
            id: "work".into(),
            name: "Work".into(),
            is_default: false,
            ephemeral: false,
        });
        assert_eq!(s.pins("work"), ids(&["ours"]));
        assert!(s.follow_chromium_pins("work", ids(&["vault"])));
        assert_eq!(s.pins("work"), ids(&["ours", "vault"]));
        assert_eq!(s.pins(""), ids(&["ours"]));
        s.pins_mut("").clear();
        assert_eq!(s.pins("work"), ids(&["ours", "vault"]));
        // A profile that is gone, or unknown, has the default profile's.
        assert_eq!(s.pins("nope"), ids(&[]));
    }
}
