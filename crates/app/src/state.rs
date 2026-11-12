//! Application state: the sidebar snapshot plus per-tab runtime data, and
//! its persistence. Pure data and logic; no engine calls here.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use magnetite_core::split::SplitNode;
use magnetite_core::{Id, SidebarSnapshot, Space, TabItem, UnixSeconds};
use magnetite_engine::ContentId;
use serde::{Deserialize, Serialize};

mod adopt;
mod bookmarks;
mod folders;
mod history;
mod import;
mod persist;
mod profiles;
mod search;
mod settings;
mod splits;
mod tabs;
#[cfg(test)]
mod testing;

pub(crate) use folders::folder_in_tree_mut;
pub use history::{History, Visit};
pub use settings::{Appearance, Settings, SiteSearch, SyncSettings};

/// What the sidebar UI receives on every change.
#[derive(Serialize)]
pub struct UiState<'a> {
    pub sidebar: &'a SidebarSnapshot,
    pub active_space: &'a str,
    pub active_tab: Option<&'a str>,
    pub runtime: &'a HashMap<Id, TabRuntime>,
    pub sidebar_visible: bool,
    pub archive_count: usize,
    /// Visits in the history: the history view asks again when it changes.
    pub history_count: usize,
    /// Tabs currently on screen in the active space (one, or a split's pages).
    pub panes: Vec<Id>,
    /// The split on screen (two or more pages), if any.
    pub split: Option<SplitNode<Id>>,
    /// Every split, on screen or not; the sidebar shows each as one row.
    pub splits: Vec<SplitNode<Id>>,
    pub peek: Option<&'a Peek>,
    pub settings: &'a Settings,
    /// Chrome colour behind the page cards, `#rrggbb`, derived from the space accent.
    pub chrome: String,
    /// Connected smart folder accounts (never their tokens).
    pub accounts: Vec<crate::smart::Account>,
    /// Refresh state per smart folder id.
    pub smart: &'a HashMap<Id, SmartStatus>,
    /// Installed extensions of the profile the page in front uses.
    pub extensions: &'a [magnetite_engine::ExtensionInfo],
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
}

impl TabRuntime {
    /// Forget the tab's page window (made again when the tab is shown): the
    /// tab is no longer loaded, loading or playing, nor muted, and nothing
    /// is known about its connection or its use of camera and microphone.
    /// Returns the window for the caller to close.
    pub fn unload(&mut self) -> Option<ContentId> {
        self.loaded = false;
        self.loading = false;
        self.media = None;
        self.muted = false;
        self.site = None;
        self.camera = false;
        self.microphone = false;
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
    pub peek: Option<Peek>,
    /// Pages in mini windows, in the order they opened. Like the peek, they
    /// live in memory only.
    pub minis: Vec<Peek>,
    pub settings: Settings,
    pub sidebar_visible: bool,
    /// Smart folder accounts of this device (tokens on disk, owner-only).
    pub accounts: crate::smart::Accounts,
    pub smart_status: HashMap<Id, SmartStatus>,
    pub downloads: crate::downloads::Downloads,
    /// The sidebar as this device would have sent it right after it last
    /// took one from sync: while what it would send now is still this, the
    /// device has changed nothing and writes nothing. Not saved.
    pub synced_sidebar: Option<SidebarSnapshot>,
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
            sidebar: &self.sidebar,
            active_space: &self.active_space,
            active_tab: self.active_tab.get(&self.active_space).map(String::as_str),
            runtime: &self.runtime,
            sidebar_visible: self.sidebar_visible,
            archive_count: self.archive.len(),
            history_count: self.history.len(),
            panes: self.visible_panes(),
            split: self.visible_split(),
            splits: self.live_splits().collect(),
            peek: self.peek.as_ref(),
            settings: &self.settings,
            chrome: format!("#{:06x}", self.chrome_rgb()),
            accounts: self.accounts.list(),
            smart: &self.smart_status,
            extensions,
        }
    }

    /// Follow pins made in Chromium's own UI ("Pin to toolbar" on its
    /// extensions page) for `profile`: an extension Chromium newly pins is
    /// pinned here too, one it unpins leaves the sidebar. Pins changed here
    /// stay as they are until Chromium's change. Returns whether any changed.
    pub fn follow_chromium_pins(&mut self, profile: &str, chromium: Vec<String>) -> bool {
        let seen = self
            .settings
            .chromium_pins_seen
            .entry(profile.to_string())
            .or_default();
        if *seen == chromium {
            return false;
        }
        let pinned = &mut self.settings.pinned_extensions;
        for id in chromium.iter().filter(|id| !seen.contains(id)) {
            if !pinned.contains(id) {
                pinned.push(id.clone());
            }
        }
        pinned.retain(|id| chromium.contains(id) || !seen.contains(id));
        *seen = chromium;
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

    /// Mix 8% of the active space accent into the base graphite, keeping
    /// every space's chrome at the same lightness.
    pub fn chrome_rgb(&self) -> u32 {
        const BASE: (f32, f32, f32) = (27.0, 27.0, 30.0); // #1b1b1e
        let accent = self.space(&self.active_space).and_then(|s| s.accent);
        let (r, g, b) = match accent {
            Some(c) => (
                BASE.0 * 0.92 + c.red * 255.0 * 0.08,
                BASE.1 * 0.92 + c.green * 255.0 * 0.08,
                BASE.2 * 0.92 + c.blue * 255.0 * 0.08,
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
        // Another profile has its own record.
        assert!(s.follow_chromium_pins("work", ids(&["vault"])));
        assert_eq!(s.settings.pinned_extensions, ids(&["ours", "vault"]));
    }
}
