//! What the sidebar and Magnetite's own pages can ask for: the list of
//! commands and what each one does.

use std::path::Path;

use magnetite_core::sidebar::Section;
use magnetite_core::smart::{Service, SmartFolder, SmartSource};
use magnetite_core::split::Side;
use magnetite_core::{Color, Space, TabItem};
use magnetite_engine::{Engine, MediaAction, Shortcut};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::state::{AppState, ArcImport, new_id};
use crate::sync::{self, Job};
use crate::{capture, downloads, lock, url_policy};

use super::addresses::urlencode;
use super::tabs::{Destination, move_tab};
use super::{
    LATE_DROP, LinkVariant, MagnetiteApp, PROMPT_SETTLE, PaletteMode, WELCOME_URL, for_ui, global,
};

/// The largest bookmarks file `ImportBookmarks` reads: 16 MiB.
const MAX_BOOKMARKS_FILE: usize = 16 << 20;

/// Chromium's own pages for what its password manager and autofill hold.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EnginePage {
    Passwords,
    Addresses,
    Cards,
}

impl EnginePage {
    fn url(self) -> &'static str {
        match self {
            Self::Passwords => "chrome://password-manager/passwords",
            Self::Addresses => "chrome://settings/contactInfo",
            Self::Cards => "chrome://settings/payments",
        }
    }
}

/// Commands the sidebar UI can send (`magnetiteQuery` with a JSON request).
#[derive(Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub(super) enum UiCommand {
    Ready,
    ActivateTab {
        id: String,
    },
    CloseTab {
        id: String,
    },
    NewTab {
        url: String,
        #[serde(default)]
        background: bool,
        /// Open next to the current tab in split view…
        #[serde(default)]
        split: bool,
        /// …on this side (default right).
        #[serde(default)]
        side: Option<Side>,
    },
    Navigate {
        url: String,
    },
    GoBack,
    GoForward,
    /// Open an extension (its popup) for the page in front.
    OpenExtension {
        id: String,
    },
    /// Show an extension in the sidebar, or stop showing it there.
    PinExtension {
        id: String,
        pinned: bool,
    },
    /// Open an extension's options page as a tab.
    ExtensionOptions {
        id: String,
    },
    /// Reload the active page, or the tab `id`.
    Reload {
        #[serde(default)]
        id: Option<String>,
    },
    /// System share menu for a tab's page at a point of the sidebar.
    Share {
        id: String,
        x: f64,
        y: f64,
    },
    SwitchSpace {
        id: String,
    },
    NextSpace,
    PrevSpace,
    Pin {
        id: String,
    },
    Unpin {
        id: String,
    },
    ResetTab {
        id: String,
    },
    /// "Keep active": spare a tab from unloading and auto-archive, or let
    /// it sleep again.
    SetKeepActive {
        id: String,
        keep: bool,
    },
    RenameTab {
        id: String,
        title: String,
    },
    NewSpace {
        title: String,
        #[serde(default)]
        icon: Option<String>,
    },
    RenameSpace {
        id: String,
        title: String,
        #[serde(default)]
        icon: Option<String>,
    },
    /// Give space `id` a colour, or its default back (`None`).
    SetSpaceColor {
        id: String,
        #[serde(default)]
        color: Option<Color>,
    },
    DeleteSpace {
        id: String,
    },
    /// Reorder the spaces: put `id` at `index`, counted without it.
    MoveSpace {
        id: String,
        index: usize,
    },
    NewFolder {
        title: String,
    },
    RenameFolder {
        id: String,
        title: String,
    },
    /// Remove folder `id`; its tabs stay (a smart folder's items go with it).
    DeleteFolder {
        id: String,
    },
    /// Give folder `id` a glyph as its icon, or the default one back (`None`).
    SetFolderIcon {
        id: String,
        #[serde(default)]
        icon: Option<String>,
    },
    MoveTab {
        id: String,
        space: String,
        section: Section,
        index: usize,
        #[serde(default)]
        folder: Option<String>,
        /// For the favorites: the profile whose list takes the tab (default:
        /// the profile the tab runs in).
        #[serde(default)]
        profile: Option<String>,
    },
    MoveFolder {
        id: String,
        index: usize,
    },
    /// Show tab `id` beside the active tab (`side`, default right).
    AddSplit {
        id: String,
        #[serde(default)]
        side: Option<Side>,
    },
    /// Drag and drop: put `tab` beside `target` (default: active tab) on `side`.
    SplitInsert {
        #[serde(default)]
        target: Option<String>,
        tab: String,
        side: Side,
    },
    /// A sidebar tab started dragging: show drop zones over the pages.
    SplitDragStart {
        tab: String,
    },
    /// The drop-zone page asks what to draw.
    SplitDropFrame,
    /// Dropped on a zone: `tab` (the dragged one) beside `target` on `side`.
    SplitDrop {
        target: String,
        side: Side,
    },
    /// The drag ended (dropped anywhere, or cancelled).
    SplitDragEnd,
    /// Double-click on a divider: equal sizes for that split.
    SplitEqualize {
        path: Vec<usize>,
    },
    RemoveSplit {
        id: String,
    },
    /// End the split holding tab `id` (default: the one on screen).
    SeparateAll {
        #[serde(default)]
        id: Option<String>,
    },
    /// Archive the Today tabs of the current space, all but `keep`.
    ArchiveToday {
        #[serde(default)]
        keep: Option<String>,
    },
    ReopenClosed,
    RestoreArchived {
        id: String,
    },
    /// Delete the archived tab `id` from the archive for good.
    RemoveArchived {
        id: String,
    },
    Archive {
        #[serde(default)]
        query: String,
    },
    Search {
        query: String,
    },
    /// The history, newest first: the newest 500 visits matching `query`.
    History {
        #[serde(default)]
        query: String,
    },
    /// Forget visit `id`.
    RemoveHistory {
        id: String,
    },
    /// Forget every visit.
    ClearHistory,
    /// "Open history" from the palette: show it in the sidebar.
    ShowHistory,
    /// Open typed input or an address from Magnetite's UI as a peek over
    /// the page in front.
    OpenPeek {
        url: String,
    },
    /// Open typed input or an address from Magnetite's UI (the quick
    /// search) in a new mini window.
    OpenMini {
        url: String,
    },
    /// Ask the system to make Magnetite the default browser (answers when
    /// the user has answered the system).
    MakeDefaultBrowser,
    ClosePeek,
    PeekToTab,
    PeekToSplit,
    /// Open the smart folder dialog for folder `id`, or to create one.
    SmartFolderDialog {
        #[serde(default)]
        id: Option<String>,
    },
    /// Check a token with the service and keep the account (answers later).
    SmartConnect {
        service: Service,
        host: String,
        token: String,
    },
    SmartDisconnect {
        id: String,
    },
    /// What `source` lists right now, for the dialog's preview (answers later).
    SmartPreview {
        source: SmartSource,
        #[serde(default)]
        account: Option<String>,
    },
    /// Create (no `id`) or change a smart folder; answers with its id.
    SmartSave {
        #[serde(default)]
        id: Option<String>,
        title: String,
        source: SmartSource,
        #[serde(default)]
        account: Option<String>,
        limit: u32,
        refresh_minutes: u32,
    },
    SmartRefresh {
        id: String,
    },
    /// Stop updating folder `id`; it and its tabs stay as they are.
    SmartUnlink {
        id: String,
    },
    /// Take tab `id` out of its smart folder for good.
    SmartHide {
        id: String,
    },
    /// Let smart folder `id`, synced from another device, fetch here.
    SmartAllow {
        id: String,
    },
    /// What a Settings page changed: only the settings it sends change
    /// (`Settings::patched`).
    SetSettings {
        settings: serde_json::Map<String, Value>,
    },
    /// Settings page (own tab) asks for everything it shows.
    GetSettings,
    /// Start a new sync chain (fresh sync code) on `server`.
    SyncCreate {
        server: String,
        #[serde(default)]
        invite: Option<String>,
    },
    /// Join an existing chain with its 24 words.
    SyncJoin {
        server: String,
        words: String,
    },
    SyncNow,
    /// The 24 words of this device's sync code (settings page only).
    SyncCode,
    /// Stop syncing on this device; the chain stays.
    SyncLeave,
    /// Delete the chain and its data from the server.
    SyncDeleteChain,
    NewProfile {
        name: String,
        /// Private isolation: in memory only.
        #[serde(default)]
        ephemeral: bool,
    },
    RenameProfile {
        id: String,
        name: String,
    },
    /// Only unused, non-default profiles; their data is deleted at next start.
    DeleteProfile {
        id: String,
    },
    /// Keep a space on this device alone (`synced` false: it is no longer
    /// sent, and a sidebar that arrives leaves it alone), or sync it again.
    SetSpaceSynced {
        id: String,
        synced: bool,
    },
    SetSpaceProfile {
        id: String,
        profile: String,
    },
    /// Copy the focused page's link: clean (trackers removed), original or
    /// Markdown. `notify`: also tell the sidebar, for a caller that is gone
    /// before the answer arrives (the command bar).
    CopyLink {
        #[serde(default)]
        variant: LinkVariant,
        #[serde(default)]
        notify: bool,
    },
    /// Toggle DevTools for the focused page.
    ToggleDevtools,
    /// Zoom the focused page to `percent` (100 resets).
    SetZoom {
        percent: u32,
    },
    /// Capture tool: freeze the focused page and open the overlay.
    CaptureStart,
    /// Overlay asks for the frozen frame.
    CaptureFrame,
    /// Overlay picked an element or region (CSS px of the frozen frame).
    CaptureSelect {
        rect: [f64; 4],
    },
    CaptureVisible,
    CaptureFull,
    CaptureCancel,
    /// Result toast actions.
    /// Copy the last capture. `notify` as for `CopyLink`.
    CaptureCopy {
        #[serde(default)]
        notify: bool,
    },
    CaptureSave,
    CaptureShare {
        x: f64,
        y: f64,
    },
    CaptureDismiss,
    /// Site panel: whether ads are blocked on the site of the page at `url`.
    SiteAds {
        url: String,
    },
    /// Site panel: block ads on the site of the page at `url` again, or not.
    SetSiteAds {
        url: String,
        blocking: bool,
    },
    /// Settings, welcome page: the ad blocker on or off.
    SetAdblockEnabled {
        enabled: bool,
    },
    /// Settings, welcome page: subscribe to filter list `id`, or not.
    SetAdblockList {
        id: String,
        subscribed: bool,
    },
    /// Site panel: permission rows of the focused page.
    SitePermissions,
    SetPermission {
        origin: String,
        kind: magnetite_engine::PermissionKind,
        value: magnetite_engine::PermissionValue,
    },
    /// What "Clear cookies and site storage" would touch (for the confirmation).
    SiteDataScope,
    /// Clear it; the result arrives as `siteDataCleared` in the sidebar.
    /// Clear cookies and storage of the focused page's site; `domain` is the
    /// one the user confirmed (`SiteDataScope`).
    ClearSiteData {
        domain: String,
    },
    /// Copy a favorite next to itself (e.g. to log in with a second account).
    DuplicateFavorite {
        id: String,
    },
    /// Give one tab (a favorite) its own profile; `None` follows the space.
    SetTabProfile {
        id: String,
        profile: Option<String>,
    },
    /// Show one of Chromium's own pages (saved passwords, addresses, cards)
    /// as a peek, in the logins of `profile` (none: the default profile).
    OpenEnginePage {
        page: EnginePage,
        #[serde(default)]
        profile: Option<String>,
    },
    ToggleSidebar,
    /// A sidebar pop-up opened: give the sidebar keyboard focus.
    FocusSidebar,
    /// Slide the floating sidebar out or hide it (its page leaves on pointer-out).
    FloatSidebar {
        show: bool,
    },
    /// The welcome page is done. The browsers chosen there (`import`, keys
    /// of `ImportSources`) are read now, not when they were picked; then the
    /// page closes. If one cannot be read nothing changes, and the answer
    /// names it (`failed`) for the page to show. Making Magnetite the
    /// default browser, if chosen there, is asked for once it closes.
    FinishWelcome {
        #[serde(default)]
        import: Vec<String>,
        /// Also ask the system to make Magnetite the default browser.
        #[serde(default)]
        default_browser: bool,
    },
    /// Command palette centred over the page. `side` of a split (default
    /// right); `tab`: the favorite a new profile is for.
    OpenPalette {
        mode: PaletteMode,
        #[serde(default)]
        side: Option<Side>,
        #[serde(default)]
        tab: Option<String>,
    },
    PaletteResize {
        height: i32,
    },
    PaletteClose,
    /// The palette page asks for the state the sidebar gets pushed.
    GetState,
    /// "Open archive" from the palette: show it in the sidebar.
    ShowArchive,
    /// The download list (`downloads.rs`).
    Downloads,
    /// Open download `id`'s file, or show it in the file manager.
    OpenDownload {
        id: String,
        #[serde(default)]
        reveal: bool,
    },
    ClearDownloads,
    /// "Downloads" from the palette: show the list in the sidebar.
    ShowDownloads,
    /// Show the bookmarks in the sidebar.
    ShowBookmarks,
    /// Save a tab's page to the bookmarks of its space; without `id`, the
    /// active tab's. The sidebar says what happened.
    AddBookmark {
        #[serde(default)]
        id: Option<String>,
    },
    /// Open a bookmark as a new tab of the space it belongs to, which
    /// comes to the front.
    OpenBookmark {
        id: String,
        #[serde(default)]
        background: bool,
    },
    /// Remove a bookmark, or a bookmark folder with all it holds.
    RemoveBookmark {
        id: String,
    },
    /// Rename a bookmark or a bookmark folder.
    RenameBookmark {
        id: String,
        title: String,
    },
    /// A new bookmark folder in `space`: inside `parent`, else at the top.
    NewBookmarkFolder {
        space: String,
        #[serde(default)]
        parent: Option<String>,
        #[serde(default)]
        title: String,
    },
    /// Move a bookmark or bookmark folder to place `index` (counted without
    /// it) of `folder`, else of the top level, in `space`'s bookmarks.
    MoveBookmark {
        id: String,
        space: String,
        #[serde(default)]
        folder: Option<String>,
        index: usize,
    },
    /// The notification panel's page needs this height (0: hide it).
    NotificationsResize {
        height: i32,
    },
    /// The permission prompts the notification panel shows.
    PermissionPrompts,
    /// The answer to permission prompt `id` (notification panel only).
    AnswerPermission {
        id: String,
        answer: magnetite_engine::PromptAnswer,
    },
    /// Dragging the sidebar's edge: `width` follows the pointer; `done`
    /// (on release) saves it like Settings → Appearance.
    SidebarWidth {
        width: u32,
        #[serde(default)]
        done: bool,
    },
    /// Where the update check stands (`update.rs`).
    UpdateStatus,
    /// Check for a new release now; answers when done.
    CheckUpdates,
    /// Install the release found and restart (else open its page).
    InstallUpdate,
    /// The release notes of the update found, in a new tab.
    OpenUpdatePage,
    /// Browsers on this computer we can import from.
    ImportSources,
    /// Add the spaces, tabs and bookmarks of the browser `key` (see
    /// `ImportSources`) next to ours.
    ImportFrom {
        key: String,
    },
    /// Add the bookmarks of a bookmarks file (`html`: its text, as Safari
    /// and other browsers export it) to the current space.
    ImportBookmarks {
        html: String,
    },
    /// Media row and tab speaker buttons; `id` is a tab id.
    Media {
        id: String,
        action: MediaAction,
    },
    SetMuted {
        id: String,
        muted: bool,
    },
    PictureInPicture {
        id: String,
    },
    Shortcut {
        shortcut: Shortcut,
    },
}

impl MagnetiteApp {
    pub(super) fn handle_command(&self, engine: &Engine, cmd: UiCommand) -> Result<Value, String> {
        match cmd {
            UiCommand::Ready => {
                // Data of the profiles deleted on this device goes now,
                // before any page could open them (once: `Ready` repeats when
                // the sidebar reloads, and contexts may be open by then).
                // Only theirs: a profile is never removed for being unknown,
                // or a state file that could not be read would cost every
                // profile its logins.
                static PRUNED: std::sync::Once = std::sync::Once::new();
                PRUNED.call_once(|| {
                    let deleted = self.with_state(|s| s.deleted_profile_data());
                    engine.remove_profile_dirs(&deleted);
                });
                // Initial activation: restore the active tab of the active space.
                self.smart_refresh_due(engine);
                let target = self.with_state(|s| s.active_tab_id().map(str::to_owned));
                if let Some(t) = target {
                    self.activate_tab(engine, &t);
                } else {
                    let space = self.with_state(|s| s.active_space.clone());
                    self.activate_space(engine, &space);
                }
                // Once: `Ready` repeats when the sidebar reloads.
                static OPENED: std::sync::Once = std::sync::Once::new();
                if let Some(url) = self.open_on_start.clone() {
                    OPENED.call_once(|| {
                        self.open_url(engine, &url, false);
                    });
                }
                // The first start asks what to bring along; nothing is taken
                // from another browser without that.
                static WELCOMED: std::sync::Once = std::sync::Once::new();
                if self.import == ArcImport::Ask && self.with_state(|s| s.first_start) {
                    WELCOMED.call_once(|| {
                        self.open_url(engine, WELCOME_URL, false);
                    });
                }
            }
            UiCommand::ActivateTab { id } => self.activate_tab(engine, &id),
            UiCommand::CloseTab { id } => self.close_tab(engine, &id),
            UiCommand::NewTab {
                url,
                split: true,
                side,
                ..
            } => {
                // Nothing to split with: the pick opens as a tab.
                if self.with_state(|s| s.active_tab_id().is_none()) {
                    self.open_url(engine, &url, false);
                } else if let Some(id) = self.open_url(engine, &url, true) {
                    self.with_state(|s| s.find_tab_mut(&id).map(|t| t.unread = false));
                    self.split_add(engine, None, &id, side.unwrap_or(Side::Right));
                }
            }
            UiCommand::NewTab {
                url, background, ..
            } => {
                self.open_url(engine, &url, background);
            }
            UiCommand::Navigate { url } => {
                let url = self.normalise_url(&url);
                if !url_policy::allowed(&url, url_policy::Source::User) {
                    return Ok(json!({ "ok": false }));
                }
                // The address belongs to the tab: a peek over it ends.
                self.close_peek(engine);
                let loaded = self.with_state(|s| {
                    let tab = s.active_tab_id()?;
                    s.runtime.get(tab)?.content?;
                    Some(tab.to_string())
                });
                match loaded {
                    Some(tab) => self.load_in_tab(engine, &tab, &url),
                    None => {
                        self.open_url(engine, &url, false);
                    }
                }
            }
            // The page in front, a peek included, like the menu bar's items.
            UiCommand::GoBack => self.page_command(engine, Shortcut::Back),
            UiCommand::GoForward => self.page_command(engine, Shortcut::Forward),
            UiCommand::Reload { id: None } => self.page_command(engine, Shortcut::Reload),
            UiCommand::Reload { id: Some(id) } => {
                if let Some(c) = self.content_of(&id) {
                    engine.content_reload(c);
                }
            }
            UiCommand::Share { id, x, y } => {
                if let Some(url) = self.with_state(|s| s.find_tab(&id).map(|(_, t)| t.url.clone()))
                {
                    engine.share_url(&url, x, y);
                }
            }
            UiCommand::SwitchSpace { id } => self.activate_space(engine, &id),
            UiCommand::NextSpace => self.handle_shortcut(engine, Shortcut::NextSpace),
            UiCommand::PrevSpace => self.handle_shortcut(engine, Shortcut::PrevSpace),
            UiCommand::Pin { id } => self.set_pinned(engine, &id, true),
            UiCommand::Unpin { id } => self.set_pinned(engine, &id, false),
            UiCommand::ResetTab { id } => {
                let (home, content) = self.with_state(|s| {
                    let home = s.find_tab(&id).and_then(|(_, t)| t.home_url.clone());
                    (home, s.runtime.get(&id).and_then(|r| r.content))
                });
                match (home, content) {
                    (Some(home), Some(_)) => self.load_in_tab(engine, &id, &home),
                    // Not loaded: it opens at its home page when shown next.
                    (Some(home), None) => {
                        self.with_state(|s| {
                            if let Some(t) = s.find_tab_mut(&id) {
                                t.url = home;
                            }
                        });
                        self.push_state(engine);
                    }
                    _ => {}
                }
            }
            UiCommand::SetKeepActive { id, keep } => {
                let changed = self.with_state(|s| {
                    let tab = s.find_tab_mut(&id)?;
                    (tab.keep_active != keep).then(|| tab.keep_active = keep)
                });
                if changed.is_some() {
                    self.push_state(engine);
                }
            }
            UiCommand::RenameTab { id, title } => {
                self.with_state(|s| {
                    if let Some(t) = s.find_tab_mut(&id) {
                        t.custom_title = !title.trim().is_empty();
                        if t.custom_title {
                            t.title = title.trim().to_string();
                        }
                    }
                });
                self.push_state(engine);
            }
            UiCommand::NewSpace { title, icon } => {
                let id = self.with_state(|s| {
                    let id = new_id();
                    let profile = s.default_profile_id();
                    s.sidebar.spaces.push(Space {
                        id: id.clone(),
                        title: if title.trim().is_empty() {
                            "New space".into()
                        } else {
                            title.trim().into()
                        },
                        icon: icon.or_else(|| Some("✦".into())),
                        accent: Some(Color {
                            red: 0.55,
                            green: 0.5,
                            blue: 1.0,
                        }),
                        profile_id: profile,
                        pinned: vec![],
                        today: vec![],
                        bookmarks: Vec::new(),
                    });
                    id
                });
                self.activate_space(engine, &id);
            }
            UiCommand::RenameSpace { id, title, icon } => {
                self.with_state(|s| {
                    if let Some(sp) = s.space_mut(&id) {
                        if !title.trim().is_empty() {
                            sp.title = title.trim().into();
                        }
                        if icon.is_some() {
                            sp.icon = icon;
                        }
                    }
                });
                self.push_state(engine);
            }
            UiCommand::DeleteSpace { id } => {
                let contents = self.with_state(|s| s.delete_space(&id));
                for c in contents {
                    engine.close_content(c);
                }
                let space = self.with_state(|s| s.active_space.clone());
                self.activate_space(engine, &space);
            }
            UiCommand::MoveSpace { id, index } => {
                self.with_state(|s| s.move_space(&id, index));
                self.push_state(engine);
            }
            UiCommand::NewFolder { title } => {
                self.with_state(|s| {
                    let space = s.active_space.clone();
                    if let Some(sp) = s.space_mut(&space) {
                        sp.pinned.insert(
                            0,
                            magnetite_core::SidebarNode::Folder(magnetite_core::Folder {
                                id: new_id(),
                                title: if title.trim().is_empty() {
                                    "Untitled".into()
                                } else {
                                    title.trim().into()
                                },
                                icon: None,
                                children: vec![],
                                smart: None,
                            }),
                        );
                    }
                });
                self.push_state(engine);
            }
            UiCommand::RenameFolder { id, title } => {
                self.with_state(|s| {
                    if let Some(f) = s.folder_mut(&id) {
                        f.title = title.trim().to_string();
                    }
                });
                self.push_state(engine);
            }
            UiCommand::DeleteFolder { id } => {
                for c in self.with_state(|s| s.delete_folder(&id)) {
                    engine.close_content(c);
                }
                self.present(engine);
            }
            UiCommand::SetFolderIcon { id, icon } => {
                self.with_state(|s| {
                    if let Some(f) = s.folder_mut(&id) {
                        f.icon = icon;
                    }
                });
                self.push_state(engine);
            }
            UiCommand::MoveTab {
                id,
                space,
                section,
                index,
                folder,
                profile,
            } => {
                let stale = self.with_state(|s| {
                    let context =
                        |s: &AppState| s.context(s.profile_for_tab(&id).as_deref()).map(|c| c.id);
                    let before = context(s);
                    let to = Destination {
                        space: &space,
                        section,
                        index,
                        folder: folder.as_deref(),
                        profile: profile.as_deref(),
                    };
                    move_tab(s, &id, &to);
                    // In another profile now: its page must load again there,
                    // not go on with the cookies of the profile it left.
                    (context(s) != before)
                        .then(|| s.detach_content(&id))
                        .flatten()
                });
                // Moved to another space while in front: this space goes on
                // with a tab of its own.
                let moved_away = self.with_state(|s| {
                    let here = s.active_space.clone();
                    let away = s.active_tab_id() == Some(id.as_str())
                        && s.find_tab(&id)
                            .is_some_and(|(sp, _)| sp.is_some_and(|sp| sp.id != here));
                    if away {
                        s.active_tab.remove(&here);
                    }
                    away.then_some(here)
                });
                if let Some(c) = stale {
                    engine.close_content(c);
                }
                if let Some(space) = moved_away {
                    self.activate_space(engine, &space);
                } else if stale.is_some() {
                    self.present(engine);
                } else {
                    self.apply_lock(engine, &id);
                    self.push_state(engine);
                }
            }
            UiCommand::MoveFolder { id, index } => {
                self.with_state(|s| {
                    let space = s.active_space.clone();
                    if let Some(sp) = s.space_mut(&space)
                        && let Some(node) =
                            magnetite_core::sidebar::remove_from_tree(&mut sp.pinned, &id)
                    {
                        let i = index.min(sp.pinned.len());
                        sp.pinned.insert(i, node);
                    }
                });
                self.push_state(engine);
            }
            UiCommand::AddSplit { id, side } => {
                self.split_add(engine, None, &id, side.unwrap_or(Side::Right));
            }
            UiCommand::SplitInsert { target, tab, side } => {
                self.split_add(engine, target.as_deref(), &tab, side);
            }
            UiCommand::SplitDragStart { tab } => self.split_drag_start(engine, tab),
            UiCommand::SplitDropFrame => {
                let drag = lock(&self.split_drag);
                let Some(tab) = drag.tab.clone() else {
                    return Ok(json!({}));
                };
                let (title, url, full, panes) = self.with_state(|s| {
                    let item = s
                        .find_tab(&tab)
                        .map(|(_, t)| (t.title.clone(), t.url.clone()));
                    let shown = s.visible_split().map(|t| t.ids()).unwrap_or_default();
                    let full =
                        shown.len() >= magnetite_core::split::MAX_PANES && !shown.contains(&tab);
                    let panes: Vec<Value> = drag
                        .panes
                        .iter()
                        .map(|(t, r, img)| {
                            let title = s
                                .find_tab(t)
                                .map(|(_, x)| x.title.clone())
                                .unwrap_or_default();
                            json!({ "tab": t, "rect": r, "image": img, "title": title })
                        })
                        .collect();
                    let (title, url) = item.unwrap_or_default();
                    (title, url, full, panes)
                });
                return Ok(
                    json!({ "tab": tab, "title": title, "url": url, "full": full, "panes": panes }),
                );
            }
            UiCommand::SplitDrop { target, side } => {
                // The drop comes from the drop zones' page, the end of the
                // drag from the sidebar: either can arrive first.
                let just_ended = lock(&self.split_drag)
                    .ended
                    .take()
                    .filter(|(_, at)| at.elapsed() < LATE_DROP)
                    .map(|(tab, _)| tab);
                let tab = self.split_drag_end(engine).or(just_ended);
                if let Some(tab) = tab.filter(|t| *t != target) {
                    self.split_add(engine, Some(&target), &tab, side);
                }
            }
            UiCommand::SplitDragEnd => {
                let tab = self.split_drag_end(engine);
                lock(&self.split_drag).ended = tab.map(|t| (t, std::time::Instant::now()));
            }
            UiCommand::SplitEqualize { path } => self.split_equalize(engine, &path),
            UiCommand::RemoveSplit { id } => {
                self.with_state(|s| s.split_remove(&id));
                self.present(engine);
            }
            UiCommand::SeparateAll { id } => {
                self.with_state(|s| {
                    if let Some(tab) = id.or_else(|| s.active_tab_id().map(str::to_owned)) {
                        s.split_separate(&tab);
                    }
                });
                self.present(engine);
            }
            UiCommand::ArchiveToday { keep } => {
                let contents = self.with_state(|s| {
                    let space = s.active_space.clone();
                    let ids: Vec<String> = s
                        .space(&space)
                        .map(|sp| sp.today.iter().map(|t| t.id.clone()).collect())
                        .unwrap_or_default();
                    let ids: Vec<String> = ids
                        .into_iter()
                        .filter(|id| Some(id) != keep.as_ref())
                        .collect();
                    ids.iter()
                        .filter_map(|id| s.archive_tab(id))
                        .collect::<Vec<_>>()
                });
                for c in contents {
                    engine.close_content(c);
                }
                let space = self.with_state(|s| s.active_space.clone());
                self.activate_space(engine, &space);
            }
            UiCommand::ReopenClosed => self.reopen_closed(engine),
            UiCommand::RestoreArchived { id } => {
                let restored = self.with_state(|s| {
                    let pos = s.archive.iter().position(|a| a.tab.id == id)?;
                    s.restore_archived(pos)
                });
                if let Some(t) = restored {
                    self.activate_tab(engine, &t);
                }
            }
            UiCommand::RemoveArchived { id } => {
                self.with_state(|s| s.archive.retain(|a| a.tab.id != id));
                self.push_state(engine);
            }
            UiCommand::Archive { query } => {
                let q = query.to_lowercase();
                return Ok(self.with_state(|s| {
                    let items: Vec<Value> = s.archive.iter().rev()
                        .filter(|a| q.is_empty() || a.tab.title.to_lowercase().contains(&q) || a.tab.url.to_lowercase().contains(&q))
                        .take(300)
                        .map(|a| json!({"id": a.tab.id, "title": a.tab.title, "url": a.tab.url, "archived_at": a.archived_at, "space": s.space(&a.space_id).map(|sp| sp.title.clone())}))
                        .collect();
                    json!({"items": items})
                }));
            }
            UiCommand::Search { query } => {
                return Ok(json!({ "results": self.with_state(|s| s.search(&query)) }));
            }
            UiCommand::History { query } => {
                let q = query.to_lowercase();
                return Ok(self.with_state(|s| {
                    let items: Vec<&crate::state::Visit> = (s.history.newest())
                        .filter(|v| {
                            q.is_empty()
                                || v.title.to_lowercase().contains(&q)
                                || v.url.to_lowercase().contains(&q)
                        })
                        .take(500)
                        .collect();
                    json!({ "items": items })
                }));
            }
            UiCommand::RemoveHistory { id } => {
                let removed = self.with_state(|s| {
                    let removed = s.history.remove(&id);
                    s.history.save();
                    removed
                });
                if removed {
                    self.show_state(engine);
                }
            }
            UiCommand::ClearHistory => {
                self.with_state(|s| {
                    s.history.clear();
                    s.history.save();
                });
                self.show_state(engine);
            }
            UiCommand::ShowHistory => self.handle_shortcut(engine, Shortcut::History),
            UiCommand::OpenPeek { url } => {
                let url = self.normalise_url(&url);
                self.open_peek(engine, &url, url_policy::Source::User, None);
            }
            UiCommand::OpenMini { url } => {
                let url = self.normalise_url(&url);
                self.open_mini(engine, &url, url_policy::Source::User, None);
            }
            UiCommand::MakeDefaultBrowser => {
                if let Some(ticket) = engine.defer_reply() {
                    engine.make_default_browser(Box::new(move |state| {
                        let reply = json!({ "default_browser": state }).to_string();
                        Engine::global().finish_reply(ticket, Ok(reply));
                    }));
                }
            }
            UiCommand::ClosePeek => self.close_peek(engine),
            UiCommand::PeekToTab => self.peek_to_tab(engine),
            UiCommand::PeekToSplit => self.peek_to_split(engine),
            UiCommand::SmartFolderDialog { id } => {
                let url = id.map_or_else(
                    || "magnetite://ui/smart-folder".to_string(),
                    |id| format!("magnetite://ui/smart-folder?id={}", urlencode(&id)),
                );
                engine.open_palette(&url, 560, 120);
            }
            UiCommand::SmartConnect {
                service,
                host,
                token,
            } => Self::smart_connect(engine, service, host, token),
            UiCommand::SmartDisconnect { id } => {
                self.with_state(|s| s.accounts.remove(&id))
                    .map_err(|e| format!("Cannot remove the account: {e}"))?;
                self.show_state(engine);
            }
            UiCommand::SmartPreview { source, account } => {
                self.smart_preview(engine, source, account.as_deref())?;
            }
            UiCommand::SmartSave {
                id,
                title,
                source,
                account,
                limit,
                refresh_minutes,
            } => {
                let smart = SmartFolder {
                    account,
                    limit: limit.clamp(1, 100),
                    refresh_minutes: refresh_minutes.clamp(1, 24 * 60),
                    ..SmartFolder::new(source)
                };
                let id = self.smart_save(engine, id, &title, smart)?;
                return Ok(json!({ "id": id }));
            }
            UiCommand::SmartRefresh { id } => self.smart_fetch(engine, &id),
            UiCommand::SmartUnlink { id } => {
                self.with_state(|s| {
                    if let Some(f) = s.folder_mut(&id) {
                        f.smart = None;
                    }
                    s.smart_status.remove(&id);
                });
                self.push_state(engine);
            }
            UiCommand::SmartHide { id } => self.smart_hide(engine, &id),
            UiCommand::SmartAllow { id } => {
                self.with_state(|s| s.allow_smart(&id));
                self.smart_fetch(engine, &id);
            }
            UiCommand::NewProfile { name, ephemeral } => {
                let id = self.with_state(|s| {
                    let id = new_id();
                    s.sidebar.profiles.push(magnetite_core::Profile {
                        id: id.clone(),
                        name: if name.trim().is_empty() {
                            "New profile".into()
                        } else {
                            name.trim().into()
                        },
                        is_default: false,
                        ephemeral,
                    });
                    id
                });
                self.push_state(engine);
                return Ok(json!({"id": id}));
            }
            UiCommand::RenameProfile { id, name } => {
                self.with_state(|s| {
                    if let Some(p) = s.sidebar.profiles.iter_mut().find(|p| p.id == id)
                        && !name.trim().is_empty()
                    {
                        p.name = name.trim().into();
                    }
                });
                self.push_state(engine);
            }
            UiCommand::DeleteProfile { id } => {
                if !self.with_state(|s| s.delete_profile(&id)) {
                    return Err("profile is in use or the default".into());
                }
                self.push_state(engine);
            }
            UiCommand::SetSpaceSynced { id, synced } => {
                if self.with_state(|s| s.set_space_synced(&id, synced)) {
                    self.push_state(engine);
                    // The other devices hear of it now, not at the next round.
                    self.sync_now();
                }
            }
            UiCommand::SetSpaceProfile { id, profile } => {
                // Tabs of that space must reload in the new context.
                let contents = self.with_state(|s| {
                    if !s.sidebar.profiles.iter().any(|p| p.id == profile) {
                        return vec![];
                    }
                    let Some(sp) = s.space_mut(&id) else {
                        return vec![];
                    };
                    sp.profile_id.clone_from(&profile);
                    let ids: Vec<String> = sp
                        .pinned
                        .iter()
                        .flat_map(magnetite_core::SidebarNode::tabs)
                        .chain(sp.today.iter())
                        .map(|t| t.id.clone())
                        .collect();
                    s.prune_private_profiles();
                    ids.iter().filter_map(|t| s.detach_content(t)).collect()
                });
                for c in contents {
                    engine.close_content(c);
                }
                self.present(engine);
            }
            UiCommand::DuplicateFavorite { id } => {
                self.with_state(|s| {
                    for set in &mut s.sidebar.favorites {
                        if let Some(i) = set.items.iter().position(|t| t.id == id) {
                            let copy = TabItem {
                                id: new_id(),
                                profile_id: None,
                                ..set.items[i].clone()
                            };
                            set.items.insert(i + 1, copy);
                            return;
                        }
                    }
                });
                self.push_state(engine);
            }
            UiCommand::SetTabProfile { id, profile } => {
                let content = self.with_state(|s| {
                    // The default profile is the same as no override.
                    let profile = profile.filter(|p| {
                        s.sidebar
                            .profiles
                            .iter()
                            .any(|x| &x.id == p && !x.is_default)
                    });
                    if s.find_tab(&id).is_some_and(|(space, _)| space.is_none()) {
                        // A favorite runs in the profile of its list: it
                        // moves to the list of the profile chosen.
                        let target = profile.unwrap_or_else(|| s.default_profile_id());
                        if s.profile_for_tab(&id).as_deref() == Some(target.as_str()) {
                            return None;
                        }
                        s.find_tab_mut(&id)?.profile_id = Some(target);
                        s.normalise_favorites();
                    } else {
                        let t = s.find_tab_mut(&id)?;
                        if t.profile_id == profile {
                            return None;
                        }
                        t.profile_id = profile;
                    }
                    s.prune_private_profiles();
                    // The page must reload in the other context.
                    s.detach_content(&id)
                });
                if let Some(c) = content {
                    engine.close_content(c);
                }
                self.present(engine);
            }
            UiCommand::SyncCreate { server, invite } => {
                self.sync_connect(Job::Create { server, invite })?;
            }
            UiCommand::SyncJoin { server, words } => {
                self.sync_connect(Job::Join { server, words })?;
            }
            UiCommand::SyncNow => self.sync_now(),
            UiCommand::SyncCode => {
                return Ok(json!({ "words": sync::code_words(&self.data_dir) }));
            }
            UiCommand::SyncLeave => self.sync_client().submit(Job::Leave),
            UiCommand::SyncDeleteChain => {
                let server = self.with_state(|s| s.settings.sync.server_url.clone());
                self.sync_client().submit(Job::DeleteChain { server });
            }
            UiCommand::GetSettings => {
                let extensions = self.front_extensions(engine);
                let keychain = if engine.config().use_mock_keychain {
                    "test"
                } else {
                    "keychain"
                };
                let (chromium, cef) = magnetite_engine::engine_versions();
                let default_browser = engine.default_browser();
                let adblock = self.adblock_json();
                return Ok(self.with_state(|s| {
                    // What stays on this device: the spaces marked so, and
                    // the profiles that only they use.
                    let kept_profiles = s.unsynced_profiles();
                    json!({
                        "keychain": keychain,
                        "extensions": extensions,
                        "sync_signed_in": sync::signed_in(s.data_dir()),
                        "settings": for_ui(json!({ "settings": s.settings }))["settings"],
                        "profiles": s.sidebar.profiles.iter().map(|p| {
                            let mut profile = json!(p);
                            profile["synced"] = json!(!kept_profiles.contains(&p.id));
                            profile
                        }).collect::<Vec<_>>(),
                        "spaces": s.sidebar.spaces.iter().map(|sp| json!({"id": sp.id, "title": sp.title, "icon": sp.icon, "accent": sp.accent, "profile_id": sp.profile_id, "synced": !s.unsynced_spaces.contains(&sp.id)})).collect::<Vec<_>>(),
                        "active_space": s.active_space,
                        "favorites": s.sidebar.favorites.iter().flat_map(|f| f.items.iter()).map(|t| json!({"id": t.id, "title": t.title, "url": t.url, "profile_id": s.profile_for_tab(&t.id)})).collect::<Vec<_>>(),
                        "archive_count": s.archive.len(),
                        "history_count": s.history.len(),
                        "default_browser": default_browser,
                        "version": env!("CARGO_PKG_VERSION"),
                        "commit": option_env!("MAGNETITE_COMMIT"),
                        "repository": env!("CARGO_PKG_REPOSITORY"),
                        "engine": { "chromium": chromium, "cef": cef },
                        "data_dir": self.data_dir.display().to_string(),
                        "adblock": adblock,
                    })
                }));
            }
            UiCommand::SetSpaceColor { id, color } => {
                self.with_state(|s| {
                    if let Some(sp) = s.space_mut(&id) {
                        sp.accent = color.map(Color::sanitised);
                    }
                });
                self.push_state(engine);
            }
            UiCommand::SetSettings { settings: patch } => {
                let before = self.with_state(|s| s.settings.adblock.clone());
                let settings = self.with_state(|s| {
                    let settings = s.settings.patched(patch)?;
                    s.settings = settings.clone();
                    // A shorter history period (or none) counts at once.
                    if s.history.prune(settings.history_days, crate::state::now()) {
                        s.history.save();
                    }
                    Ok::<_, String>(settings)
                })?;
                engine.set_sidebar_width(settings.appearance.sidebar_width);
                engine
                    .set_chrome_toolbar(settings.chrome_toolbar || engine.config().chrome_toolbar);
                Self::apply_devtools_dock(engine, settings.devtools_dock);
                engine.set_autofill(settings.autofill);
                engine.set_quick_search_key(settings.quick_search);
                // Lists are fetched and compiled only when they changed.
                let after = &settings.adblock;
                if (after.enabled, &after.lists) != (before.enabled, &before.lists) {
                    self.apply_adblock(true);
                } else if after.allowed_sites != before.allowed_sites {
                    self.apply_adblock_sites();
                }
                self.push_state(engine);
            }
            UiCommand::OpenEnginePage { page, profile } => {
                // Over the page in front (Settings), in the logins asked
                // for: each profile has its own passwords and addresses.
                let context = profile.or_else(|| self.with_state(|s| Some(s.default_profile_id())));
                self.open_peek(
                    engine,
                    page.url(),
                    url_policy::Source::User,
                    context.as_deref(),
                );
            }
            UiCommand::ToggleSidebar => self.handle_shortcut(engine, Shortcut::ToggleSidebar),
            UiCommand::FocusSidebar => engine.focus_sidebar(),
            UiCommand::FloatSidebar { show } => {
                let had_focus = engine.float_sidebar_shown() && !show;
                engine.show_float_sidebar(show);
                // Keys go back to the page the floating sidebar took them from.
                if had_focus {
                    self.on_active_content(engine, Engine::focus_content);
                }
            }
            UiCommand::FinishWelcome {
                import,
                default_browser,
            } => {
                // Read everything first: a browser that cannot be read
                // leaves the sidebar as it is, and the page says which.
                let installed = magnetite_importer::installed();
                let mut read = Vec::new();
                let mut failed = Vec::new();
                for key in &import {
                    let browser = installed.iter().find(|b| b.key == key);
                    let name = browser.map_or(key.as_str(), |b| b.name);
                    let result = browser
                        .ok_or_else(|| "it is not on this computer".to_string())
                        .and_then(|b| magnetite_importer::import(b).map_err(|e| e.to_string()));
                    match result {
                        Ok(snap) => read.push(snap),
                        Err(error) => {
                            failed.push(json!({ "key": key, "name": name, "error": error }));
                        }
                    }
                }
                if !failed.is_empty() {
                    return Ok(json!({ "failed": failed }));
                }
                let pages: Vec<String> = self.with_state(|s| {
                    (s.sidebar.all_tabs())
                        .filter(|t| t.url.starts_with(WELCOME_URL))
                        .map(|t| t.id.clone())
                        .collect()
                });
                for id in pages {
                    self.close_tab(engine, &id);
                }
                // The first import takes the place of the starter space.
                let first = self.with_state(|s| {
                    let mut first = None;
                    for snap in read {
                        first = first.or(s.add_first_import(snap).first);
                    }
                    first
                });
                if let Some(space) = first {
                    self.activate_space(engine, &space);
                } else {
                    // Nothing to show yet: the command bar, for an address.
                    self.push_state(engine);
                    Self::open_palette(engine, PaletteMode::New, None, None);
                }
                if default_browser {
                    // macOS asks the user; nothing more to do with the answer.
                    engine.make_default_browser(Box::new(|_| {}));
                }
            }
            UiCommand::OpenPalette { mode, side, tab } => {
                Self::open_palette(engine, mode, side, tab.as_deref());
            }
            UiCommand::PaletteResize { height } => engine.resize_overlay(height),
            UiCommand::PaletteClose => {
                // A command bar that a new one replaced still says "close"
                // when it loses the keyboard: that is not about its successor.
                if !engine.query_from_replaced_overlay() {
                    engine.close_overlay();
                }
            }
            UiCommand::GetState => {
                let extensions = self.front_extensions(engine);
                return Ok(self.with_state(|s| for_ui(json!(s.ui_state(&extensions)))));
            }
            UiCommand::OpenExtension { id } => self.open_extension(engine, &id),
            UiCommand::PinExtension { id, pinned } => self.pin_extension(engine, &id, pinned),
            UiCommand::ExtensionOptions { id } => {
                let url = self
                    .front_extensions(engine)
                    .into_iter()
                    .find(|e| e.id == id)
                    .and_then(|e| e.options_url);
                if let Some(url) = url {
                    // In the profile of the page in front, where the
                    // extension is installed (`""`: the default one).
                    let context = self
                        .with_state(|s| s.front_context())
                        .map_or_else(String::new, |c| c.id);
                    let user = url_policy::Source::User;
                    self.open_url_from(engine, &url, false, user, Some(&context));
                }
            }
            UiCommand::ShowArchive => Self::ui_call(engine, "openArchive", &json!({})),
            UiCommand::Downloads => return Ok(self.with_state(|s| s.downloads.for_ui())),
            UiCommand::OpenDownload { id, reveal } => {
                let file = self.with_state(|s| s.downloads.file(&id).map(Path::to_path_buf));
                let opened = file
                    .ok_or(())
                    .and_then(|f| downloads::open(&f, reveal).map_err(|_| ()));
                if opened.is_err() {
                    let text = "That file was moved or deleted.";
                    Self::ui_call(engine, "notice", &json!({ "text": text }));
                    self.show_downloads(engine);
                }
            }
            UiCommand::ClearDownloads => {
                self.with_state(|s| {
                    s.downloads.clear();
                    s.persist();
                });
                self.show_downloads(engine);
            }
            UiCommand::ShowDownloads => self.handle_shortcut(engine, Shortcut::Downloads),
            UiCommand::ShowBookmarks => Self::ui_call(engine, "openBookmarks", &json!({})),
            UiCommand::AddBookmark { id } => {
                let saved = self.with_state(|s| {
                    let tab = id.or_else(|| s.active_tab_id().map(str::to_owned))?;
                    let bookmark = s.add_bookmark(&tab)?;
                    s.bookmark(&bookmark).map(|(space, _)| space.title.clone())
                });
                let text = match &saved {
                    Some(space) => format!("Saved to the bookmarks of {space}"),
                    None => "Not saved: it is a bookmark already, or there is no page to keep"
                        .to_string(),
                };
                if saved.is_some() {
                    self.push_state(engine);
                }
                Self::ui_call(engine, "notice", &json!({ "text": text }));
                return Ok(json!({ "saved": saved.is_some() }));
            }
            UiCommand::OpenBookmark { id, background } => {
                self.open_bookmark(engine, &id, background);
            }
            UiCommand::RemoveBookmark { id } => {
                if self.with_state(|s| s.remove_bookmark(&id)) {
                    self.push_state(engine);
                }
            }
            UiCommand::RenameBookmark { id, title } => {
                if self.with_state(|s| s.rename_bookmark(&id, &title)) {
                    self.push_state(engine);
                }
            }
            UiCommand::NewBookmarkFolder {
                space,
                parent,
                title,
            } => {
                let id =
                    self.with_state(|s| s.new_bookmark_folder(&space, parent.as_deref(), &title));
                if id.is_some() {
                    self.push_state(engine);
                }
                return Ok(json!({ "id": id }));
            }
            UiCommand::MoveBookmark {
                id,
                space,
                folder,
                index,
            } => {
                if self.with_state(|s| s.move_bookmark(&id, &space, folder.as_deref(), index)) {
                    self.push_state(engine);
                }
            }
            UiCommand::NotificationsResize { height } => engine.set_notifications_height(height),
            UiCommand::PermissionPrompts => return Ok(self.prompts_json()),
            UiCommand::AnswerPermission { id, answer } => {
                let Ok(id) = id.parse::<u64>() else {
                    return Ok(json!({ "ok": false }));
                };
                // Allow counts only for a card on screen that has stayed as
                // it is for `PROMPT_SETTLE`; anything else waits for a click.
                let key = id.to_string();
                let on_screen = || {
                    self.prompts_json()["items"]
                        .as_array()
                        .is_some_and(|items| items.iter().any(|i| i["id"].as_str() == Some(&key)))
                };
                let settled = self
                    .with_prompts(|p| p.changed_at.is_some_and(|at| at.elapsed() >= PROMPT_SETTLE));
                if answer == magnetite_engine::PromptAnswer::Allow && !(settled && on_screen()) {
                    return Ok(json!({ "ok": false }));
                }
                engine.answer_permission_prompt(id, answer);
                self.with_prompts(|p| p.open.retain(|q| q.id != id));
                self.show_prompts(engine);
            }
            UiCommand::SidebarWidth { width, done } => {
                let width = crate::state::Appearance::sidebar_width(width);
                engine.set_sidebar_width(width);
                if done {
                    self.with_state(|s| s.settings.appearance.sidebar_width = width);
                    self.push_state(engine);
                }
            }
            UiCommand::UpdateStatus => return Ok(self.update_json()),
            UiCommand::CheckUpdates => {
                if let Some(ticket) = engine.defer_reply() {
                    self.check_updates(engine, Some(ticket));
                }
            }
            UiCommand::InstallUpdate => self.install_update(engine),
            UiCommand::OpenUpdatePage => self.open_update_page(engine),
            UiCommand::Media { id, action } => {
                if let Some(c) = self.content_of(&id) {
                    engine.content_media(c, action);
                }
            }
            UiCommand::SetMuted { id, muted } => {
                if let Some(c) = self.content_of(&id) {
                    engine.content_set_muted(c, muted);
                    self.with_state(|s| {
                        if let Some(rt) = s.runtime.get_mut(&id) {
                            rt.muted = muted;
                        }
                    });
                    self.push_state(engine);
                }
            }
            UiCommand::PictureInPicture { id } => {
                if let Some(c) = self.content_of(&id) {
                    let pip = self.with_state(|s| {
                        s.runtime
                            .get(&id)
                            .and_then(|r| r.media.as_ref())
                            .is_some_and(|m| m.pip)
                    });
                    engine.content_picture_in_picture(c, !pip);
                }
            }
            UiCommand::ImportSources => {
                let sources: Vec<Value> = magnetite_importer::installed()
                    .iter()
                    .map(|b| json!({ "key": b.key, "name": b.name }))
                    .collect();
                return Ok(json!({ "sources": sources }));
            }
            UiCommand::ImportFrom { key } => {
                let browser = magnetite_importer::installed()
                    .into_iter()
                    .find(|b| b.key == key)
                    .ok_or("That browser is not on this computer.")?;
                let snap = magnetite_importer::import(&browser).map_err(|e| e.to_string())?;
                let added = self.with_state(|s| s.add_import(snap));
                match &added.first {
                    Some(space) => self.activate_space(engine, space),
                    None => self.push_state(engine),
                }
                return Ok(json!({
                    "spaces": added.spaces,
                    "tabs": added.tabs,
                    "bookmarks": added.bookmarks,
                }));
            }
            UiCommand::ImportBookmarks { html } => {
                if html.len() > MAX_BOOKMARKS_FILE {
                    return Err("That file is too large to be a bookmarks file.".into());
                }
                let found = magnetite_importer::bookmarks_file::bookmarks(&html)
                    .map_err(|e| e.to_string())?;
                let added = self.with_state(|s| {
                    let space = s.active_space.clone();
                    s.add_bookmarks(&space, found)
                });
                self.push_state(engine);
                return Ok(json!({ "bookmarks": added }));
            }
            UiCommand::Shortcut { shortcut } => self.handle_shortcut(engine, shortcut),
            UiCommand::CopyLink { variant, notify } => {
                let result = self.copy_link(engine, None, variant);
                if notify {
                    Self::ui_call(engine, "copied", &result);
                }
                return Ok(result);
            }
            UiCommand::CaptureStart => self.capture_start(engine, None),
            UiCommand::CaptureFrame => {
                let state = lock(&self.capture);
                return Ok(match state.session.as_ref() {
                    Some((_, f)) => json!({
                        "image": capture::data_url(&f.png),
                        "css_width": f.css_width,
                        "css_height": f.css_height,
                        "boxes": f.boxes,
                        "element_ok": f.element_ok,
                    }),
                    None => json!({}),
                });
            }
            UiCommand::CaptureSelect { rect } => self.capture_region(engine, Some(rect)),
            UiCommand::CaptureVisible => self.capture_region(engine, None),
            UiCommand::CaptureFull => self.capture_full(engine),
            UiCommand::CaptureCancel => {
                engine.close_overlay();
                lock(&self.capture).session = None;
            }
            UiCommand::CaptureCopy { notify } => {
                let copied = lock(&self.capture)
                    .result
                    .as_ref()
                    .is_some_and(|r| engine.copy_png(&r.png));
                if notify {
                    let text = if copied {
                        "Capture copied"
                    } else {
                        "No capture to copy"
                    };
                    Self::ui_call(engine, "notice", &json!({ "text": text }));
                }
                return Ok(json!({ "copied": copied }));
            }
            UiCommand::CaptureSave => {
                let png = lock(&self.capture).result.as_ref().map(|r| r.png.clone());
                let saved = png.and_then(|p| engine.save_png(&p, &capture::file_name()));
                return Ok(json!({ "saved": saved.map(|p| p.display().to_string()) }));
            }
            UiCommand::CaptureShare { x, y } => {
                let state = lock(&self.capture);
                if let Some(r) = state.result.as_ref() {
                    engine.share_png(&r.png, x, y);
                }
            }
            UiCommand::CaptureDismiss => {
                lock(&self.capture).result = None;
            }
            UiCommand::SiteAds { url } => return Ok(self.site_ads(&url)),
            UiCommand::SetSiteAds { url, blocking } => {
                self.set_site_ads(engine, &url, blocking);
                self.push_state(engine);
            }
            UiCommand::SetAdblockEnabled { enabled } => {
                self.set_adblock_enabled(enabled);
                self.push_state(engine);
            }
            UiCommand::SetAdblockList { id, subscribed } => {
                self.set_adblock_list(&id, subscribed);
                self.push_state(engine);
            }
            UiCommand::SitePermissions => {
                let rows = self
                    .focused_content(None)
                    .map(|c| engine.site_permissions(c))
                    .unwrap_or_default();
                return Ok(json!({ "rows": rows }));
            }
            UiCommand::SetPermission {
                origin,
                kind,
                value,
            } => {
                if let Some(c) = self.focused_content(None) {
                    engine.set_site_permission(c, &origin, kind, value);
                }
            }
            UiCommand::SiteDataScope => {
                let scope = self
                    .focused_content(None)
                    .and_then(|c| engine.site_data_scope(c));
                return Ok(match scope {
                    Some((domain, origins)) => json!({ "domain": domain, "origins": origins }),
                    None => json!({}),
                });
            }
            UiCommand::ClearSiteData { domain } => {
                if let Some(c) = self.focused_content(None) {
                    let profile = self.with_state(|s| {
                        s.content_to_tab.get(&c).and_then(|t| s.profile_for_tab(t))
                    });
                    engine.clear_site_data(
                        c,
                        &domain,
                        Box::new(move |report| {
                            let engine = Engine::global();
                            // Signed-out pages of that site in that profile reload.
                            if report.cookies.is_some()
                                && let Some(app) = global()
                            {
                                app.reload_site(engine, &report.domain, profile.as_deref());
                            }
                            Self::ui_call(engine, "siteDataCleared", &json!(report));
                        }),
                    );
                }
            }
            UiCommand::ToggleDevtools => self.toggle_devtools(engine, None),
            UiCommand::SetZoom { percent } => {
                if let Some(c) = self.focused_content(None) {
                    engine.set_content_zoom(c, percent.clamp(25, 500));
                }
            }
        }
        Ok(json!({"ok": true}))
    }
}
