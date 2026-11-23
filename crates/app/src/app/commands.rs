//! What the sidebar and Magnetite's own pages can ask for: the list of
//! commands and what each one does.

use std::collections::HashMap;
use std::path::Path;

use magnetite_core::sidebar::Section;
use magnetite_core::smart::{Service, SmartFolder, SmartSource};
use magnetite_core::split::Side;
use magnetite_core::{Color, SidebarNode, Space, TabItem};
use magnetite_engine::{Engine, MediaAction, Shortcut};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::state::import::{SpaceChoice, choose_spaces, preview};
use crate::state::{AppState, ArcImport, SidebarEffect, SpaceProfile, icon_image_ok, new_id, now};
use crate::sync::{self, Job};
use crate::{capture, downloads, lock, url_policy};

use super::addresses::urlencode;
use super::tabs::{Destination, move_tabs};
use super::{
    LATE_DROP, LinkVariant, MagnetiteApp, PROMPT_SETTLE, PaletteMode, WELCOME_URL, for_ui, global,
};

/// The largest bookmarks file `ImportBookmarks` reads: 16 MiB.
const MAX_BOOKMARKS_FILE: usize = 16 << 20;

/// What `CopyDownload` puts on the clipboard.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum DownloadCopy {
    File,
    Path,
    /// The web address it was downloaded from.
    Link,
}

/// The kinds of file bookmarks are exported as.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum BookmarkFormat {
    Html,
    Json,
    Csv,
}

/// Chromium's own pages: for what its password manager and autofill hold,
/// and its dialog that deletes cookies, site data and the cache.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum EnginePage {
    Passwords,
    Addresses,
    Cards,
    ClearData,
    /// Chromium's page of installed extensions, and the store to get them
    /// from: Settings → Extensions opens them as a tab in the profile it
    /// is about.
    Extensions,
    WebStore,
}

impl EnginePage {
    fn url(self) -> &'static str {
        match self {
            Self::Passwords => "chrome://password-manager/passwords",
            Self::Addresses => "chrome://settings/contactInfo",
            Self::Cards => "chrome://settings/payments",
            Self::ClearData => "chrome://settings/clearBrowserData",
            Self::Extensions => "chrome://extensions",
            Self::WebStore => "https://chromewebstore.google.com/category/extensions",
        }
    }
}

/// The most an own image may weigh as base64: about 6 MB of WebP.
const MAX_IMAGE_BASE64: usize = 8 * 1024 * 1024;

/// Where an own image of the user goes (Settings → Appearance, the space dialog).
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImageSlot {
    /// Behind the empty window.
    Empty,
    /// Over the sidebar's colour.
    Sidebar,
}

impl ImageSlot {
    /// The file of this slot's image for space `space`, served as
    /// `magnetite://ui/user-images/<slot>-<space>`. `None` for an id that
    /// would not make a plain file name.
    fn file(self, data_dir: &Path, space: &str) -> Option<std::path::PathBuf> {
        let plain = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
        let slot = match self {
            Self::Empty => "empty",
            Self::Sidebar => "sidebar",
        };
        (!space.is_empty() && space.chars().all(plain)).then(|| {
            magnetite_engine::user_image_dir(data_dir).join(format!("{slot}-{space}.webp"))
        })
    }
}

/// The top bar's pop-ups (`magnetite://ui/popup?kind=…`).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum TopBarPopup {
    /// Site controls for the page in front.
    Site,
    /// Copy link, original link or Markdown.
    Copy,
    /// A pinned extension's menu.
    Extension,
    /// The extensions without an icon in the top bar.
    Extensions,
    /// What the page in front is shown as (the developer tools).
    Emulate,
    /// The time zone the page in front sees.
    Timezone,
}

impl TopBarPopup {
    fn as_str(self) -> &'static str {
        match self {
            Self::Site => "site",
            Self::Copy => "copy",
            Self::Extension => "extension",
            Self::Extensions => "extensions",
            Self::Emulate => "emulate",
            Self::Timezone => "timezone",
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
    /// `profile`: the profile whose sidebar it is about (Settings →
    /// Extensions names one); without it, that of the page in front.
    PinExtension {
        id: String,
        pinned: bool,
        #[serde(default)]
        profile: Option<String>,
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
    /// Open the private space, or go to it if it is open.
    OpenPrivateSpace,
    /// Close the private space with everything opened from it.
    ClosePrivateSpace,
    /// Hide the private space: open, but shown nowhere until it is opened again.
    HidePrivateSpace,
    NextSpace,
    PrevSpace,
    Pin {
        id: String,
    },
    Unpin {
        id: String,
    },
    /// A pinned tab leaves its space for the archive (`remove_pinned`).
    RemovePinned {
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
    /// A favorite opens as a web panel from now on, or as a tab again.
    SetWebPanel {
        id: String,
        panel: bool,
    },
    /// Web panel `id`'s page is shown as on a phone, or as itself again.
    SetPanelMobile {
        id: String,
        mobile: bool,
    },
    /// Open favorite `id`'s web panel, or close it if it is the open one.
    ToggleWebPanel {
        id: String,
    },
    RenameTab {
        id: String,
        title: String,
    },
    /// A new space. It becomes the current one, unless `stay` is set
    /// (Settings → Spaces, whose page would go out of sight with it). It
    /// runs in `profile`, else in the default profile.
    NewSpace {
        title: String,
        #[serde(default)]
        icon: Option<String>,
        #[serde(default)]
        stay: bool,
        #[serde(default)]
        profile: Option<SpaceProfile>,
    },
    /// Give space `id` another name (an empty one keeps its name), another
    /// glyph, or both. A glyph takes the place of the space's own picture
    /// (`SetSpaceIconImage`).
    RenameSpace {
        id: String,
        #[serde(default)]
        title: String,
        #[serde(default)]
        icon: Option<String>,
    },
    /// Show a picture of the user's own in place of space `id`'s glyph
    /// (`data`: a WebP file as base64, within `MAX_ICON_IMAGE_BYTES`), or
    /// the glyph again (`None`). The picture is part of the space and syncs
    /// with it, hence the small size.
    SetSpaceIconImage {
        id: String,
        #[serde(default)]
        data: Option<String>,
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
    /// A new folder inside folder `parent`, else at the top of Pinned.
    NewFolder {
        title: String,
        #[serde(default)]
        parent: Option<String>,
    },
    RenameFolder {
        id: String,
        title: String,
    },
    /// The sidebar shows folder `id` open or closed (it keeps that itself).
    SetFolderOpen {
        id: String,
        open: bool,
    },
    /// Remove folder `id`; its tabs stay (a smart folder's items go with it).
    DeleteFolder {
        id: String,
    },
    /// Copy the link that shares folder `id` and its tabs.
    ShareFolder {
        id: String,
    },
    /// The folder shared link `url` carries, for its page; `null` if unreadable.
    ReadShared {
        url: String,
    },
    /// Add the folder of shared link `url` to the active space's pinned tabs;
    /// answers whether it was added.
    AddShared {
        url: String,
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
        /// The row of a split was moved: the pages of the split that stand
        /// with `id` go along (`split_group`), and `index` counts the list
        /// without them.
        #[serde(default)]
        split: bool,
    },
    /// Put folder `id` at `index` (counted without it) of folder `folder`,
    /// else of the top level of Pinned.
    MoveFolder {
        id: String,
        index: usize,
        #[serde(default)]
        folder: Option<String>,
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
    /// A page dragged inside its split's sidebar row: `tab` goes before
    /// (`left`) or after `target` in reading order, the layout stays.
    SplitReorder {
        target: String,
        tab: String,
        side: Side,
    },
    /// A sidebar tab started dragging: show drop zones over the pages.
    SplitDragStart {
        tab: String,
    },
    /// The drop-zone page asks what to draw.
    SplitDropFrame,
    /// Dropped on a zone: `tab` (the dragged one) beside `target` on `side`,
    /// or without a side in `target`'s place, which takes `tab`'s.
    SplitDrop {
        target: String,
        #[serde(default)]
        side: Option<Side>,
    },
    /// The drag ended (dropped anywhere, or cancelled).
    SplitDragEnd,
    /// Double-click on a divider: equal sizes for that split.
    SplitEqualize {
        path: Vec<usize>,
    },
    /// Arrange the split on screen as an even grid.
    SplitGrid,
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
    /// Take the tab `id` over from the device of the sync chain it is open
    /// on.
    TakeDeviceTab {
        id: String,
    },
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
    /// Let the user choose the folder downloads are saved to, in the
    /// system's dialog (answers with the settings' `downloads` once the
    /// dialog is closed; unchanged without a choice).
    ChooseDownloadFolder,
    ClosePeek,
    PeekToTab,
    PeekToSplit,
    /// Open the smart folder dialog for folder `id`, or to create one.
    SmartFolderDialog {
        #[serde(default)]
        id: Option<String>,
    },
    /// Open the settings dialog of space `id`: name, icon, colour, background.
    SpaceDialog {
        id: String,
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
    /// Put tab `id`, an item of its smart folder, away until `until`
    /// (Unix seconds; none: until it is woken by hand).
    SmartSnooze {
        id: String,
        #[serde(default)]
        until: Option<i64>,
    },
    /// Bring the item `url`, snoozed in smart folder `id`, back now.
    SmartWake {
        id: String,
        url: String,
    },
    /// Let smart folder `id`, synced from another device, fetch here.
    SmartAllow {
        id: String,
    },
    /// Store the user's own image for `slot` of space `space` (WebP,
    /// base64), or remove it (`None`). Settings → Appearance.
    SetAppearanceImage {
        slot: ImageSlot,
        space: String,
        #[serde(default)]
        data: Option<String>,
    },
    /// What a Settings page changed: only the settings it sends change
    /// (`Settings::patched`).
    SetSettings {
        settings: serde_json::Map<String, Value>,
    },
    /// Settings page (own tab) asks for everything it shows.
    GetSettings,
    /// Give shortcut `action` the keys `keys` (as `Combo` saves them), or
    /// none. An action that had these keys loses them. Answers
    /// `{"error": …}` for keys a shortcut cannot have.
    SetShortcut {
        action: magnetite_engine::Action,
        #[serde(default)]
        keys: Option<String>,
    },
    /// A key of shortcut `action` pressed in a page goes to `first`
    /// ("magnetite" or "page") before the other.
    SetShortcutFirst {
        action: magnetite_engine::Action,
        first: magnetite_engine::First,
    },
    /// Give space `id` the keys `keys` (as `Combo` saves them), or none.
    /// A shortcut or space that had these keys loses them. Answers
    /// `{"error": …}` for keys a shortcut cannot have.
    SetSpaceShortcut {
        id: String,
        #[serde(default)]
        keys: Option<String>,
    },
    /// Give shortcut `action` its default keys back, or every shortcut
    /// without `action`; then no space has keys of its own either.
    ResetShortcut {
        #[serde(default)]
        action: Option<magnetite_engine::Action>,
    },
    /// The Settings page waits for the keys of a shortcut (`on`): until it
    /// has them no shortcut runs, so they reach the page.
    RecordShortcut {
        on: bool,
    },
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
    /// Open the focused page's DevTools on `panel`.
    DevPanel {
        panel: magnetite_engine::DevPanel,
    },
    /// Load the focused page anew, past the cache; a local page loses its
    /// site data first.
    DevReset,
    /// Show the focused page as `patch` says (`dev::Emulation`'s fields).
    DevEmulate {
        patch: Value,
    },
    /// The programs listening on this computer's ports.
    DevServers,
    /// Open the page in front again, beside itself, with the logins of
    /// `profile`: two users of one site at once.
    DevSplitAs {
        profile: String,
    },
    /// Show the page in front a second time beside itself, as a phone that
    /// follows its address; or end that.
    DevResponsive,
    /// One picture of the page in front in light and dark at three sizes.
    DevCaptureSet,
    /// Record the page in front as a video, or end the recording that
    /// runs.
    DevRecord,
    /// A page that stays (the sidebar, the top bar) makes the video.
    DevRecordStart,
    /// The page making recording `id`: the next picture of the recorded
    /// page.
    DevRecordFrame {
        id: u64,
    },
    /// The same: a piece of the video (base64), MP4 or `WebM`.
    DevRecordData {
        id: u64,
        data: String,
        mp4: bool,
    },
    /// The same: the video is complete, or (`failed`) could not be written
    /// to its end.
    DevRecordEnd {
        id: u64,
        #[serde(default)]
        failed: bool,
    },
    /// Copy the last recording's file, or show it in the file manager.
    DevRecordFile {
        reveal: bool,
    },
    /// Zoom the focused page to `percent` (100 resets).
    SetZoom {
        percent: u32,
    },
    /// Capture tool: freeze the focused page and open the overlay.
    CaptureStart,
    /// Overlay asks for the frozen frame.
    CaptureFrame,
    /// Overlay picked an element or region (CSS px of the frozen frame).
    /// `context`: an element was picked and its description (selector,
    /// markup, styles, the picture's file) goes to the clipboard as text.
    CaptureSelect {
        rect: [f64; 4],
        #[serde(default)]
        context: bool,
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
    /// Site panel: whether the site of the page at `url` gets the keys of
    /// every shortcut first.
    SiteShortcuts {
        url: String,
    },
    /// Site panel and Settings: the site of the page at `url`, or the site
    /// `url` names, gets the keys of every shortcut first, or no longer.
    SetSiteShortcuts {
        url: String,
        page_first: bool,
    },
    /// Site panel: block ads on the site of the page at `url` again, or not.
    SetSiteAds {
        url: String,
        blocking: bool,
    },
    /// Site panel: whether the system's engine shows the site of the page
    /// at `url`.
    SiteEngine {
        url: String,
    },
    /// Site panel: show the site of the page at `url` in the system's
    /// engine from now on, or in Chromium again.
    SetSiteEngine {
        url: String,
        system: bool,
    },
    /// Settings: delete what the system's engine keeps, in every profile.
    ClearSystemEngineData,
    /// Settings, welcome page: the ad blocker on or off.
    SetAdblockEnabled {
        enabled: bool,
    },
    /// Settings, welcome page: subscribe to filter list `id`, or not.
    SetAdblockList {
        id: String,
        subscribed: bool,
    },
    /// Device bar: the device page `page` is shown as (`null`: none).
    DeviceGet {
        page: magnetite_engine::ContentId,
    },
    /// Device bar: show page `page` as `config`, or end device mode (`null`).
    DeviceSet {
        page: magnetite_engine::ContentId,
        config: Value,
    },
    /// Site panel: permission rows of the focused page.
    SitePermissions,
    /// Every site with a choice of its own for a permission, in the
    /// logins of `profile` (none: the default profile).
    AllPermissions {
        #[serde(default)]
        profile: Option<String>,
    },
    /// Set what site `origin` may do in the logins of `profile`, from the
    /// list in Settings.
    SetProfilePermission {
        #[serde(default)]
        profile: Option<String>,
        origin: String,
        kind: magnetite_engine::PermissionKind,
        value: magnetite_engine::PermissionValue,
    },
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
    /// Open the Chrome Web Store page of extension `id` as a tab that runs
    /// in profile `profile`: the way to install there what another profile
    /// has (Settings → Extensions).
    OpenExtensionStore {
        id: String,
        profile: String,
    },
    /// Favorite `id` to place `index` of its list (Settings → Favorites
    /// sorts a whole list, whatever the space in front shows of it).
    MoveFavorite {
        id: String,
        index: usize,
    },
    /// A new favorite for address `url`, last in the main profile's list.
    AddFavorite {
        url: String,
    },
    /// The pinned tabs of space `space` as bookmarks of that space; with
    /// `remove` they then leave Pinned (closed, into the archive).
    PinnedToBookmarks {
        space: String,
        #[serde(default)]
        remove: bool,
    },
    /// Remove every bookmark of space `space`.
    ClearBookmarks {
        space: String,
    },
    /// The spaces favorite `id` shows in; none: every space.
    SetFavoriteSpaces {
        id: String,
        #[serde(default)]
        spaces: Vec<String>,
    },
    /// Give one tab (a favorite) its own profile; `None` follows the space.
    SetTabProfile {
        id: String,
        profile: Option<String>,
    },
    /// Show one of Chromium's own pages (saved passwords, addresses, cards,
    /// deleting browsing data) as a peek, in the logins of `profile` (none: the default profile).
    OpenEnginePage {
        page: EnginePage,
        #[serde(default)]
        profile: Option<String>,
    },
    ToggleSidebar,
    /// Turn the top bar on or off (Settings → Appearance has it too).
    ToggleTopBar,
    /// The window buttons of the top bar (Linux and Windows) and of the
    /// floating sidebar (macOS).
    WindowMinimize,
    WindowMaximize,
    WindowFullscreen,
    WindowClose,
    /// Open one of the top bar's pop-ups below it, `width` by `height`, at
    /// (`x`, `y`) in the top bar page's coordinates.
    TopBarPopup {
        popup: TopBarPopup,
        /// The extension a menu is for, or those the list of extensions
        /// names, separated by commas.
        #[serde(default)]
        id: Option<String>,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        /// Asked by the developer bar, and placed below it.
        #[serde(default)]
        dev_bar: bool,
    },
    /// A sidebar pop-up opened: give the sidebar keyboard focus.
    FocusSidebar,
    /// Slide the floating sidebar out or hide it (its page leaves on pointer-out).
    FloatSidebar {
        show: bool,
    },
    /// The welcome page is done. The browsers chosen there (`import`, keys
    /// of `ImportSources`) are read again now, so what comes in is as new
    /// as it can be; then the page closes. `spaces` has what was chosen
    /// there for imported spaces (a name, an icon, leaving it out), per
    /// browser. If a browser cannot be read
    /// nothing changes, and the answer names it (`failed`) for the page to
    /// show. Making Magnetite the default browser, if chosen there, is
    /// asked for once it closes.
    FinishWelcome {
        #[serde(default)]
        import: Vec<String>,
        #[serde(default)]
        spaces: HashMap<String, Vec<SpaceChoice>>,
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
    /// Put download `id`'s file, its path or its link on the clipboard.
    CopyDownload {
        id: String,
        what: DownloadCopy,
    },
    /// Fetch failed download `id` again, as a new download.
    RetryDownload {
        id: String,
    },
    /// Pause, resume or cancel running download `id`.
    ControlDownload {
        id: String,
        what: magnetite_engine::DownloadControl,
    },
    /// Take download `id` off the list; the file stays.
    RemoveDownload {
        id: String,
    },
    /// Move download `id`'s file to the system's trash; the entry stays.
    TrashDownload {
        id: String,
    },
    /// Drag download `id`'s file out of the asking page, from the pointer
    /// (macOS: `Engine::drag_file`).
    DragDownload {
        id: String,
    },
    /// Let the asking page's hidden file chooser take download `id`'s file,
    /// so the page can drag it out (Windows, Linux: `Engine::offer_file`).
    OfferDownload {
        id: String,
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
    /// Dragging the grip on the page behind the tabs: `width` follows the
    /// pointer (none: the usual width); `done` (on release) keeps it for
    /// that panel.
    WebPanelWidth {
        #[serde(default)]
        width: Option<u32>,
        #[serde(default)]
        done: bool,
    },
    /// Dragging the grip over the page at a floating web panel's edge: `x`
    /// is the pointer on the screen; `done` (on release) keeps the width.
    WebPanelGrip {
        x: i32,
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
    /// Read the browser `key` (see `ImportSources`) and say what an import
    /// would add, space by space. Nothing is added.
    ImportPreview {
        key: String,
    },
    /// Add the spaces, tabs and bookmarks of the browser `key` (see
    /// `ImportSources`) next to ours.
    ImportFrom {
        key: String,
    },
    /// Add the bookmarks of a bookmarks file (`html`: its text, as Safari
    /// and other browsers export it) to space `space`, or to the current
    /// one.
    ImportBookmarks {
        html: String,
        #[serde(default)]
        space: Option<String>,
    },
    /// The bookmarks of the spaces `spaces`, or of every space when none
    /// is named, as the text of a file in `format`, with a name for the
    /// file.
    ExportBookmarks {
        format: BookmarkFormat,
        #[serde(default)]
        spaces: Vec<String>,
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
    /// The logins a command of Settings → Permissions names, as the engine
    /// takes them: none for the default profile, which no id or its own id
    /// names. An id that is no profile (any more) is an error: its choices
    /// must not be read from, or written to, the default profile in its
    /// place.
    fn permission_logins(
        &self,
        profile: Option<&str>,
    ) -> Result<Option<magnetite_engine::ContextProfile>, String> {
        let Some(id) = profile else { return Ok(None) };
        self.with_state(|s| {
            if s.sidebar.profiles.iter().any(|p| p.id == id) {
                Ok(s.context(Some(id)))
            } else {
                Err("That profile is gone.".into())
            }
        })
    }

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
                // A further window starts without a tab: the space's first
                // one may be on screen in another window, which would lose it.
                self.smart_refresh_due(engine);
                let (target, first_window) =
                    self.with_state(|s| (s.active_tab_id().map(str::to_owned), s.views.is_empty()));
                if let Some(t) = target {
                    self.activate_tab(engine, &t);
                } else if first_window {
                    let space = self.with_state(|s| s.active_space.clone());
                    self.activate_space(engine, &space);
                } else {
                    self.present(engine);
                }
                // Once: `Ready` repeats when the sidebar reloads.
                static OPENED: std::sync::Once = std::sync::Once::new();
                OPENED.call_once(|| {
                    self.load_pinned(engine);
                    if let Some(url) = self.open_on_start.clone() {
                        self.open_url(engine, &url, false);
                    }
                    self.open_from_other_app(engine, self.launch_urls.clone());
                });
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
            UiCommand::OpenPrivateSpace => self.open_private_space(engine),
            UiCommand::ClosePrivateSpace => self.close_private_space(engine),
            UiCommand::HidePrivateSpace => self.hide_private_space(engine),
            UiCommand::NextSpace => self.handle_shortcut(engine, Shortcut::NextSpace),
            UiCommand::PrevSpace => self.handle_shortcut(engine, Shortcut::PrevSpace),
            UiCommand::Pin { id } => self.set_pinned(engine, &id, true),
            UiCommand::Unpin { id } => self.set_pinned(engine, &id, false),
            UiCommand::RemovePinned { id } => self.remove_pinned(engine, &id),
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
            UiCommand::SetWebPanel { id, panel } => self.set_web_panel(engine, &id, panel),
            UiCommand::SetPanelMobile { id, mobile } => self.set_panel_mobile(engine, &id, mobile),
            UiCommand::ToggleWebPanel { id } => self.toggle_web_panel(engine, &id),
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
            UiCommand::NewSpace {
                title,
                icon,
                stay,
                profile,
            } => {
                let id = self.with_state(|s| s.new_space(&title, icon, profile))?;
                if stay {
                    self.push_state(engine);
                    self.tell_settings_pages(engine);
                    return Ok(json!({ "id": id }));
                }
                self.activate_space(engine, &id);
                self.tell_settings_pages(engine);
            }
            UiCommand::RenameSpace { id, title, icon } => {
                self.with_state(|s| {
                    // The private space keeps its name and icon.
                    if s.is_private_space(&id) {
                        return;
                    }
                    if let Some(sp) = s.space_mut(&id) {
                        if !title.trim().is_empty() {
                            sp.title = title.trim().into();
                        }
                        if icon.is_some() {
                            sp.icon = icon;
                            // A glyph is chosen in place of the own picture.
                            sp.icon_image = None;
                        }
                    }
                });
                self.push_state(engine);
                self.tell_settings_pages(engine);
            }
            UiCommand::DeleteSpace { id } if self.with_state(|s| s.is_private_space(&id)) => {
                self.close_private_space(engine);
            }
            UiCommand::DeleteSpace { id } => {
                let contents = self.with_state(|s| s.delete_space(&id));
                for c in contents {
                    engine.close_content(c);
                }
                // Its look and its own images go with it.
                if self.with_state(|s| s.space(&id).is_none()) {
                    self.with_state(|s| s.settings.appearance.looks.remove(&id));
                    for slot in [ImageSlot::Empty, ImageSlot::Sidebar] {
                        if let Some(file) = slot.file(&self.data_dir, &id) {
                            let _ = std::fs::remove_file(file);
                        }
                    }
                }
                let space = self.with_state(|s| s.active_space.clone());
                self.activate_space(engine, &space);
                self.tell_settings_pages(engine);
            }
            UiCommand::MoveSpace { id, index } => {
                self.with_state(|s| s.move_space(&id, index));
                self.push_state(engine);
                self.tell_settings_pages(engine);
            }
            UiCommand::NewFolder { title, parent } => {
                self.with_state(|s| s.new_folder(&title, parent.as_deref()));
                self.push_state(engine);
            }
            UiCommand::SetFolderOpen { id, open } => {
                self.with_state(|s| {
                    if open {
                        s.closed_folders.remove(&id);
                    } else {
                        s.closed_folders.insert(id);
                    }
                });
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
            UiCommand::ShareFolder { id } => {
                let text = match self.with_state(|s| s.share_folder(&id)) {
                    Ok(link) if engine.copy_text(&link) => "Link to folder copied".to_string(),
                    Ok(_) => "Could not copy".to_string(),
                    Err(e) => e.to_string(),
                };
                Self::ui_call(engine, "notice", &json!({ "text": text }));
            }
            UiCommand::ReadShared { url } => {
                return Ok(json!({ "folder": crate::state::read_shared(&url).ok() }));
            }
            UiCommand::AddShared { url } => {
                let added = self.with_state(|s| s.add_shared(&url));
                let text = match &added {
                    Ok(title) => format!("Added \u{201c}{title}\u{201d}"),
                    Err(e) => e.to_string(),
                };
                self.push_state(engine);
                Self::ui_call(engine, "notice", &json!({ "text": text }));
                return Ok(json!({ "added": added.is_ok() }));
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
                split,
            } => {
                let (ids, stale) = self.with_state(|s| {
                    let ids = if split {
                        s.split_group(&id)
                    } else {
                        vec![id.clone()]
                    };
                    let contexts = |s: &AppState| -> Vec<_> {
                        (ids.iter())
                            .map(|id| s.context(s.profile_for_tab(id).as_deref()).map(|c| c.id))
                            .collect()
                    };
                    let before = contexts(s);
                    let to = Destination {
                        space: &space,
                        section,
                        index,
                        folder: folder.as_deref(),
                        profile: profile.as_deref(),
                    };
                    move_tabs(s, &ids, &to);
                    // In another profile now: its page must load again there,
                    // not go on with the cookies of the profile it left.
                    let after = contexts(s);
                    let stale: Vec<_> = (ids.iter().zip(before.iter().zip(&after)))
                        .filter(|(_, (before, after))| before != after)
                        .filter_map(|(id, _)| s.detach_content(id))
                        .collect();
                    (ids, stale)
                });
                // Moved to another space while in front: this space goes on
                // with a tab of its own.
                let moved_away = self.with_state(|s| {
                    let here = s.active_space.clone();
                    let away = s.active_tab_id().is_some_and(|active| {
                        ids.iter().any(|id| id == active)
                            && s.find_tab(active)
                                .is_some_and(|(sp, _)| sp.is_some_and(|sp| sp.id != here))
                    });
                    if away {
                        s.active_tab.remove(&here);
                    }
                    away.then_some(here)
                });
                let reload = !stale.is_empty();
                for c in stale {
                    engine.close_content(c);
                }
                if let Some(space) = moved_away {
                    self.activate_space(engine, &space);
                } else if reload {
                    self.present(engine);
                } else {
                    for id in &ids {
                        self.apply_lock(engine, id);
                    }
                    self.push_state(engine);
                }
            }
            UiCommand::MoveFolder { id, index, folder } => {
                if self.with_state(|s| s.move_folder(&id, folder.as_deref(), index)) {
                    self.push_state(engine);
                }
            }
            UiCommand::AddSplit { id, side } => {
                self.split_add(engine, None, &id, side.unwrap_or(Side::Right));
            }
            UiCommand::SplitInsert { target, tab, side } => {
                self.split_add(engine, target.as_deref(), &tab, side);
            }
            UiCommand::SplitReorder { target, tab, side } => {
                self.with_state(|s| s.split_reorder(&target, &tab, side));
                // The same pages stay on screen in the same layout, only
                // their places change: tiling them again is enough, and far
                // quicker than showing the split anew.
                self.retile_split(engine);
                self.push_state(engine);
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
                match (tab.filter(|t| *t != target), side) {
                    (Some(tab), Some(side)) => self.split_add(engine, Some(&target), &tab, side),
                    (Some(tab), None) => {
                        self.with_state(|s| s.split_swap(&target, &tab));
                        self.present(engine);
                    }
                    (None, _) => {}
                }
            }
            UiCommand::SplitDragEnd => {
                let tab = self.split_drag_end(engine);
                lock(&self.split_drag).ended = tab.map(|t| (t, std::time::Instant::now()));
            }
            UiCommand::SplitEqualize { path } => self.split_equalize(engine, &path),
            UiCommand::SplitGrid => self.split_grid(engine),
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
            UiCommand::TakeDeviceTab { id } => self.take_device_tab_by_id(engine, &id),
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
            UiCommand::ChooseDownloadFolder => {
                // No window to open the dialog over is no choice made, and
                // is said: a dialog closed without one is not.
                if !engine.can_open_dialog() {
                    return Err("The dialog could not be opened.".into());
                }
                if let Some(ticket) = engine.defer_reply() {
                    let now = self.with_state(|s| s.settings.downloads.folder.clone());
                    let start = (!now.is_empty()).then(|| std::path::PathBuf::from(now));
                    engine.choose_folder(
                        "Save downloads to",
                        start.as_deref(),
                        Box::new(move |chosen| {
                            let engine = Engine::global();
                            let Some(app) = super::global() else {
                                return engine
                                    .finish_reply(ticket, Err("Magnetite is closing".into()));
                            };
                            // Chosen, and gone or no folder by now: said, where
                            // a dialog closed without a choice is not.
                            if chosen
                                .as_ref()
                                .is_some_and(|f| !(f.is_absolute() && f.is_dir()))
                            {
                                return engine.finish_reply(
                                    ticket,
                                    Err("That folder cannot be used.".into()),
                                );
                            }
                            let (downloads, changed) = app.with_state(|s| {
                                let before = s.settings.downloads.folder.clone();
                                if let Some(folder) = chosen {
                                    s.settings.downloads.folder =
                                        folder.to_string_lossy().into_owned();
                                }
                                let changed = s.settings.downloads.folder != before;
                                (s.settings.downloads.clone(), changed)
                            });
                            // A dialog closed without a choice changes nothing.
                            if changed {
                                engine.set_downloads(&downloads);
                                // Kept also if the app ends without a proper quit.
                                app.save_soon();
                            }
                            let reply = json!({ "downloads": downloads }).to_string();
                            engine.finish_reply(ticket, Ok(reply));
                        }),
                    );
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
            UiCommand::SpaceDialog { id } => {
                let url = format!("magnetite://ui/space?id={}", urlencode(&id));
                engine.open_palette(&url, 480, 120);
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
            UiCommand::SmartSnooze { id, until } => self.smart_snooze(engine, &id, until),
            UiCommand::SmartWake { id, url } => self.smart_wake(engine, &id, &url),
            UiCommand::SmartAllow { id } => {
                self.with_state(|s| s.allow_smart(&id));
                self.smart_fetch(engine, &id);
            }
            UiCommand::NewProfile { name, ephemeral } => {
                let id = self.with_state(|s| s.add_profile(&name, ephemeral));
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
                // The private space is never synced: there is nothing to set.
                if self.with_state(|s| !s.is_private_space(&id) && s.set_space_synced(&id, synced))
                {
                    self.push_state(engine);
                    // The other devices hear of it now, not at the next round.
                    self.sync_now();
                }
            }
            UiCommand::SetSpaceProfile { id, profile } => {
                // Tabs of that space must reload in the new context.
                let contents = self.with_state(|s| {
                    // The private space stays in memory, and its profiles
                    // are for nothing else.
                    let known = s.sidebar.profiles.iter().any(|p| p.id == profile);
                    if !known || s.is_private_space(&id) || s.private_profiles.contains(&profile) {
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
            UiCommand::OpenExtensionStore { id, profile } => {
                if !magnetite_engine::is_extension_id(&id) {
                    return Err("That is no extension.".into());
                }
                if self.with_state(|s| s.profile_for_context(&profile).is_none()) {
                    return Err("That profile is gone.".into());
                }
                let url = format!("https://chromewebstore.google.com/detail/{id}");
                let source = url_policy::Source::User;
                self.open_url_from(engine, &url, false, source, Some(&profile));
            }
            UiCommand::MoveFavorite { id, index } => {
                self.with_state(|s| {
                    for list in &mut s.sidebar.favorites {
                        if let Some(from) = list.items.iter().position(|t| t.id == id) {
                            let item = list.items.remove(from);
                            let to = index.min(list.items.len());
                            list.items.insert(to, item);
                            break;
                        }
                    }
                });
                self.push_state(engine);
                self.tell_settings_pages(engine);
            }
            UiCommand::AddFavorite { url } => {
                let url = self.normalise_url(&url);
                if url.starts_with("magnetite://")
                    || !url_policy::allowed(&url, url_policy::Source::User)
                {
                    return Err("That is no address of a web page.".into());
                }
                // Named by its site until its page says what it is called.
                let site = url.split_once("://").map_or(url.as_str(), |(_, rest)| rest);
                let host = site.split(['/', '?', '#']).next().unwrap_or(site);
                self.with_state(|s| {
                    let profile_id = s.default_profile_id();
                    let favorite = TabItem {
                        home_url: Some(url.clone()),
                        created_at: Some(now()),
                        ..TabItem::new(new_id(), &url, host)
                    };
                    let lists = &mut s.sidebar.favorites;
                    match lists.iter_mut().find(|f| f.profile_id == profile_id) {
                        Some(list) => list.items.push(favorite),
                        None => lists.push(magnetite_core::FavoriteSet {
                            profile_id,
                            items: vec![favorite],
                        }),
                    }
                    s.normalise_favorites();
                });
                self.push_state(engine);
                self.tell_settings_pages(engine);
            }
            UiCommand::PinnedToBookmarks { space, remove } => {
                if self.with_state(|s| s.space(&space).is_none() || s.is_private_space(&space)) {
                    return Err("That space is gone.".into());
                }
                let (added, pinned) = self.with_state(|s| {
                    let added = s.pinned_to_bookmarks(&space);
                    // What leaves Pinned: its tabs, not a smart folder's items.
                    (added, s.plain_pinned(&space))
                });
                if remove {
                    // As if each was unpinned and closed: it is in the
                    // archive, should the bookmark not be enough.
                    for id in &pinned {
                        self.with_state(|s| {
                            let to = Destination {
                                space: &space,
                                section: Section::Today,
                                index: 0,
                                folder: None,
                                profile: None,
                            };
                            move_tabs(s, std::slice::from_ref(id), &to);
                        });
                        self.close_tab(engine, id);
                    }
                }
                self.push_state(engine);
                self.tell_settings_pages(engine);
                return Ok(json!({ "bookmarks": added, "pinned": pinned.len() }));
            }
            UiCommand::ClearBookmarks { space } => {
                let removed = self.with_state(|s| s.clear_bookmarks(&space));
                self.push_state(engine);
                self.tell_settings_pages(engine);
                return Ok(json!({ "bookmarks": removed }));
            }
            UiCommand::SetFavoriteSpaces { id, spaces } => {
                self.with_state(|s| {
                    // Only spaces there are, and never the private one: it
                    // shows no favorites.
                    let spaces: Vec<String> = (spaces.into_iter())
                        .filter(|sp| s.space(sp).is_some() && !s.is_private_space(sp))
                        .collect();
                    let favorites = s.sidebar.favorites.iter_mut();
                    if let Some(t) = favorites
                        .flat_map(|f| f.items.iter_mut())
                        .find(|t| t.id == id)
                    {
                        t.only_in = spaces;
                    }
                });
                self.push_state(engine);
                self.tell_settings_pages(engine);
            }
            UiCommand::SetTabProfile { id, profile } => {
                let content = self.with_state(|s| {
                    // A tab of the private space keeps its own profile, and
                    // no other tab gets one of those.
                    let private = |p: &String| s.private_profiles.contains(p);
                    if s.in_private_space(&id) || profile.as_ref().is_some_and(private) {
                        return None;
                    }
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
                let words = sync::code_words(&self.data_dir);
                let server = self.with_state(|s| s.settings.sync.server_url.clone());
                // For a phone to scan: the server and the words in one link.
                let qr = words
                    .as_deref()
                    .and_then(|w| sync::qr_rows(&sync::join_link(&server, w)));
                return Ok(json!({ "words": words, "qr": qr }));
            }
            UiCommand::SyncLeave => self.sync_client().submit(Job::Leave),
            UiCommand::SyncDeleteChain => {
                let server = self.with_state(|s| s.settings.sync.server_url.clone());
                self.sync_client().submit(Job::DeleteChain { server });
            }
            UiCommand::GetSettings => {
                let extensions = self.front_extensions(engine);
                // And those of every lasting profile, for Settings →
                // Extensions: an extension is installed profile by profile.
                let lasting: Vec<String> = self.with_state(|s| {
                    let lasting = s.sidebar.profiles.iter().filter(|p| !p.ephemeral);
                    lasting.map(|p| p.id.clone()).collect()
                });
                let profile_extensions: Vec<_> = (lasting.iter())
                    .map(|id| {
                        let context = self.with_state(|s| s.context(Some(id)));
                        let pinned = self.with_state(|s| s.pins(id).to_vec());
                        json!({ "profile": id, "extensions": engine.extensions(context.as_ref()), "pinned": pinned })
                    })
                    .collect();
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
                    let space_keys = s.space_keys();
                    json!({
                        "keychain": keychain,
                        "extensions": extensions,
                        "profile_extensions": profile_extensions,
                        "sync_signed_in": sync::signed_in(s.data_dir()),
                        "settings": for_ui(json!({ "settings": s.settings }))["settings"],
                        "profiles": s.sidebar.profiles.iter().filter(|p| !s.private_profiles.contains(&p.id)).map(|p| {
                            let mut profile = json!(p);
                            profile["synced"] = json!(!kept_profiles.contains(&p.id));
                            profile
                        }).collect::<Vec<_>>(),
                        "spaces": s.sidebar.spaces.iter().filter(|sp| !s.is_private_space(&sp.id)).map(|sp| json!({"id": sp.id, "title": sp.title, "icon": sp.icon, "icon_image": sp.icon_image, "accent": sp.accent, "profile_id": sp.profile_id, "bookmarks": sp.bookmarks.iter().flat_map(SidebarNode::tabs).count(), "bookmark_tree": !sp.bookmarks.is_empty(), "pinned": s.plain_pinned(&sp.id).len(), "synced": !s.unsynced_spaces.contains(&sp.id), "keys": space_keys.iter().find_map(|(own, keys)| (own.id == sp.id).then(|| keys.to_string())), "shortcut": sp.shortcut})).collect::<Vec<_>>(),
                        "active_space": s.active_space,
                        "favorites": s.sidebar.favorites.iter().flat_map(|f| f.items.iter()).map(|t| json!({"id": t.id, "title": t.title, "url": t.url, "profile_id": s.profile_for_tab(&t.id), "only_in": t.only_in.iter().filter(|sp| s.space(sp).is_some()).collect::<Vec<_>>()})).collect::<Vec<_>>(),
                        "archive_count": s.archive.len(),
                        "history_count": s.history.len(),
                        "default_browser": default_browser,
                        "version": env!("CARGO_PKG_VERSION"),
                        "commit": option_env!("MAGNETITE_COMMIT"),
                        "repository": env!("CARGO_PKG_REPOSITORY"),
                        "engine": { "chromium": chromium, "cef": cef },
                        "data_dir": self.data_dir.display().to_string(),
                        // The chosen folder is not there: the page says so.
                        "download_folder_missing": !s.settings.downloads.folder.is_empty() && !std::path::Path::new(&s.settings.downloads.folder).is_dir(),
                        "adblock": adblock,
                        // Whether this system has an engine of its own for
                        // pages, and the services that open there.
                        "system_engine": {
                            "available": magnetite_engine::Engine::system_engine_available(),
                            "known_sites": crate::state::PROTECTED_VIDEO_SITES,
                            "name": magnetite_engine::Engine::system_engine_name().0,
                            "about": magnetite_engine::Engine::system_engine_name().1,
                        },
                        "shortcuts": magnetite_engine::Action::ALL.iter().map(|action| json!({
                            "action": action,
                            "keys": s.settings.shortcuts.combo_of(*action).map(|c| c.to_string()),
                            "default": action.default_combo().map(|c| c.to_string()),
                            "first": s.settings.shortcut_priority.first(*action),
                            "default_first": action.default_first(),
                        })).collect::<Vec<_>>(),
                    })
                }));
            }
            UiCommand::SetSpaceIconImage { id, data } => {
                if data.as_deref().is_some_and(|d| !icon_image_ok(d)) {
                    return Err("an icon is a WebP picture of at most 4 KB".into());
                }
                let set = self.with_state(|s| {
                    // The private space keeps its icon.
                    if s.is_private_space(&id) {
                        return false;
                    }
                    (s.space_mut(&id).map(|sp| sp.icon_image = data)).is_some()
                });
                if !set {
                    return Err("no such space".into());
                }
                self.push_state(engine);
            }
            UiCommand::SetSpaceColor { id, color } => {
                self.with_state(|s| {
                    if let Some(sp) = s.space_mut(&id) {
                        sp.accent = color.map(Color::sanitised);
                    }
                });
                self.push_state(engine);
            }
            UiCommand::SetAppearanceImage { slot, space, data } => {
                if !self.with_state(|s| s.space(&space).is_some()) {
                    return Err("no such space".into());
                }
                let file = slot.file(&self.data_dir, &space).ok_or("no such space")?;
                let stored = if let Some(data) = data {
                    use base64::Engine as _;
                    if data.len() > MAX_IMAGE_BASE64 {
                        return Err("that image is too large".into());
                    }
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .map_err(|_| "that is not an image")?;
                    // What the page sends: a WebP file ("RIFF....WEBP").
                    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WEBP") {
                        return Err("that is not an image".into());
                    }
                    (file.parent().map_or(Ok(()), crate::files::private_dir))
                        .and_then(|()| crate::files::write_private(&file, &bytes))
                        .map_err(|e| format!("the image could not be stored: {e}"))?;
                    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
                    Some(now.map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)))
                } else {
                    // Gone already is as good as removed; a picture that
                    // stays on disk keeps its setting.
                    match std::fs::remove_file(&file) {
                        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                            return Err(format!("the image could not be removed: {e}"));
                        }
                        _ => None,
                    }
                };
                self.with_state(|s| {
                    let a = s.settings.appearance.looks.entry(space).or_default();
                    match slot {
                        ImageSlot::Empty => a.empty_image = stored,
                        ImageSlot::Sidebar => {
                            a.sidebar_image = stored;
                            // A new image shows at once; without one its effect is off.
                            if stored.is_some() {
                                a.sidebar_effect = SidebarEffect::Image;
                            } else if a.sidebar_effect == SidebarEffect::Image {
                                a.sidebar_effect = SidebarEffect::None;
                            }
                        }
                    }
                });
                self.push_state(engine);
            }
            UiCommand::SetSettings { settings: patch } => {
                let before = self.with_state(|s| s.settings.adblock.clone());
                let downloads_before = self.with_state(|s| s.settings.downloads.clone());
                let (settings, code_changed) = self.with_state(|s| {
                    let settings = s.settings.patched(patch)?;
                    let code_changed = settings.site_code != s.settings.site_code;
                    s.settings = settings.clone();
                    // A shorter history period (or none) counts at once.
                    if s.history.prune(settings.history_days, crate::state::now()) {
                        s.history.save();
                    }
                    Ok::<_, String>((settings, code_changed))
                })?;
                engine.set_sidebar_width(settings.appearance.sidebar_width);
                engine.set_top_bar_visible(settings.appearance.top_bar);
                engine.set_page_frame(settings.appearance.page_frame);
                engine
                    .set_chrome_toolbar(settings.chrome_toolbar || engine.config().chrome_toolbar);
                Self::apply_devtools_dock(engine, settings.devtools_dock);
                engine.set_autofill(settings.autofill);
                magnetite_engine::set_global_privacy_control(settings.global_privacy_control);
                // Only when they changed.
                if settings.downloads != downloads_before {
                    engine.set_downloads(&settings.downloads);
                }
                // Written now so that it is there at the next start: Chromium
                // reads the level when it starts, before Magnetite could set it.
                engine.set_default_zoom(settings.page_zoom);
                engine.set_languages(&settings.languages);
                self.settle_private_engines();
                engine.set_system_engine_sites(self.system_engine_sites());
                engine.set_quick_search_key(settings.quick_search);
                self.dev_settings_changed(engine);
                // Open pages get changed CSS at once.
                if code_changed {
                    engine.set_site_code(settings.site_code.clone());
                }
                // Lists are fetched and compiled only when they changed.
                let after = &settings.adblock;
                if (after.enabled, &after.lists) != (before.enabled, &before.lists) {
                    self.apply_adblock(true);
                } else if after.allowed_sites != before.allowed_sites {
                    self.apply_adblock_sites();
                }
                self.push_state(engine);
            }
            UiCommand::SetShortcut { action, keys } => {
                if let Err(error) = self.set_shortcut(engine, action, keys.as_deref()) {
                    return Ok(json!({ "error": error.to_string() }));
                }
            }
            UiCommand::SetShortcutFirst { action, first } => {
                self.set_shortcut_first(engine, action, first);
            }
            UiCommand::SetSpaceShortcut { id, keys } => {
                if let Err(error) = self.set_space_shortcut(engine, &id, keys.as_deref()) {
                    return Ok(json!({ "error": error.to_string() }));
                }
            }
            UiCommand::ResetShortcut { action } => self.reset_shortcut(engine, action),
            UiCommand::RecordShortcut { on } => engine.record_shortcut_keys(on),
            UiCommand::OpenEnginePage { page, profile } => {
                // Over the page in front (Settings), in the logins asked
                // for: each profile has its own passwords and addresses.
                // Logins asked for that are gone are an error: the peek
                // would open in those of the space in front, and the page
                // may delete data.
                if let Some(id) = &profile
                    && self.with_state(|s| s.profile_for_context(id).is_none())
                {
                    return Err("That profile is gone.".into());
                }
                let context = profile.or_else(|| self.with_state(|s| Some(s.default_profile_id())));
                // The extension pages open as a tab of that profile: a tab
                // is where Chromium installs from, and its prompt names
                // the page it belongs to.
                if matches!(page, EnginePage::Extensions | EnginePage::WebStore) {
                    let source = url_policy::Source::User;
                    self.open_url_from(engine, page.url(), false, source, context.as_deref());
                    return Ok(json!({"ok": true}));
                }
                self.open_peek(
                    engine,
                    page.url(),
                    url_policy::Source::User,
                    context.as_deref(),
                );
            }
            UiCommand::ToggleSidebar => self.handle_shortcut(engine, Shortcut::ToggleSidebar),
            UiCommand::ToggleTopBar => {
                let on = self.with_state(|s| {
                    let a = &mut s.settings.appearance;
                    a.top_bar = !a.top_bar;
                    a.top_bar
                });
                engine.set_top_bar_visible(on);
                self.push_state(engine);
            }
            UiCommand::WindowMinimize => engine.minimize_frame(),
            UiCommand::WindowMaximize => engine.toggle_maximize_frame(),
            UiCommand::WindowFullscreen => engine.toggle_fullscreen_frame(),
            UiCommand::WindowClose => engine.close_frame(),
            UiCommand::TopBarPopup {
                popup,
                id,
                x,
                y,
                width,
                height,
                dev_bar,
            } => {
                let id = id.map(|id| format!("&id={}", urlencode(&id)));
                let url = format!(
                    "magnetite://ui/popup?kind={}{}",
                    popup.as_str(),
                    id.unwrap_or_default()
                );
                engine.open_bar_popup(dev_bar, &url, (x, y), (width, height));
            }
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
                spaces,
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
                        Ok(snap) => read.push(match spaces.get(key) {
                            Some(chosen) => choose_spaces(snap, chosen),
                            None => snap,
                        }),
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
                let recording = lock(&self.record).for_ui();
                return Ok(self.with_state(|s| super::ui_json(s, &extensions, recording)));
            }
            UiCommand::OpenExtension { id } => self.open_extension(engine, &id),
            UiCommand::PinExtension {
                id,
                pinned,
                profile,
            } => {
                // A profile named and gone meanwhile: not the default
                // profile's pins in its place.
                if let Some(p) = &profile
                    && self.with_state(|s| s.profile_for_context(p).is_none())
                {
                    return Err("That profile is gone.".into());
                }
                self.pin_extension(engine, &id, pinned, profile.as_deref());
                self.tell_settings_pages(engine);
            }
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
            UiCommand::CopyDownload {
                id,
                what: DownloadCopy::Link,
            } => {
                let link = self.with_state(|s| s.downloads.link(&id).map(str::to_string));
                let copied = link.is_some_and(|link| engine.copy_text(&link));
                let text = if copied {
                    "Link copied"
                } else {
                    "Could not copy"
                };
                Self::ui_call(engine, "notice", &json!({ "text": text }));
            }
            UiCommand::CopyDownload { id, what } => {
                let file = self.with_state(|s| s.downloads.file(&id).map(Path::to_path_buf));
                // The path of a file that is gone is of no use either.
                let text = match (file.filter(|f| f.exists()), what) {
                    (None, _) => {
                        self.show_downloads(engine);
                        "That file was moved or deleted."
                    }
                    (Some(f), DownloadCopy::File) if engine.copy_file(&f) => "File copied",
                    (Some(f), DownloadCopy::Path) if engine.copy_text(&f.to_string_lossy()) => {
                        "Path copied"
                    }
                    (Some(_), _) => "Could not copy",
                };
                Self::ui_call(engine, "notice", &json!({ "text": text }));
            }
            UiCommand::ControlDownload { id, what } => {
                let running = self.with_state(|s| {
                    let (context, chromium_id) = s.downloads.running(&id)?;
                    Some((context.map(str::to_string), chromium_id))
                });
                // Chromium's report of the change updates the list.
                let done = running.is_some_and(|(context, chromium_id)| {
                    engine.control_download(context.as_deref(), chromium_id, what)
                });
                if !done {
                    self.show_downloads(engine);
                }
            }
            UiCommand::RemoveDownload { id } => {
                self.with_state(|s| {
                    s.downloads.remove(&id);
                    s.persist();
                });
                self.show_downloads(engine);
            }
            UiCommand::TrashDownload { id } => {
                let file = self.with_state(|s| s.downloads.file(&id).map(Path::to_path_buf));
                let trashed = file.is_some_and(|f| magnetite_engine::move_to_trash(&f).is_ok());
                if !trashed {
                    let text = "Could not move that file to the trash.";
                    Self::ui_call(engine, "notice", &json!({ "text": text }));
                }
                // The entry stays and shows the file as deleted.
                self.show_downloads(engine);
            }
            // A drag takes a file that is there; the page hears whether it
            // started.
            UiCommand::DragDownload { id } => {
                let file = self.with_state(|s| s.downloads.file(&id).map(Path::to_path_buf));
                if !file.is_some_and(|f| f.exists() && engine.drag_file(&f)) {
                    return Err("nothing to drag".into());
                }
            }
            UiCommand::OfferDownload { id } => {
                let file = self.with_state(|s| s.downloads.file(&id).map(Path::to_path_buf));
                if !file.is_some_and(|f| f.exists() && engine.offer_file(&f)) {
                    return Err("nothing to offer".into());
                }
            }
            UiCommand::RetryDownload { id } => {
                let source = self.with_state(|s| {
                    let (context, url) = s.downloads.retry(&id)?;
                    Some((context.to_string(), url.to_string()))
                });
                let started =
                    source.is_some_and(|(context, url)| engine.start_download(&context, &url));
                if started {
                    // The new download takes its place in the list.
                    self.with_state(|s| {
                        s.downloads.remove(&id);
                        s.persist();
                    });
                } else {
                    let text = "Could not start that download again.";
                    Self::ui_call(engine, "notice", &json!({ "text": text }));
                }
                self.show_downloads(engine);
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
                    // The private space's bookmarks would go with it.
                    if s.in_private_space(&tab) {
                        return None;
                    }
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
            // Kept through `EngineEvent::WebPanelResized`.
            UiCommand::WebPanelWidth { width, done } => engine.resize_web_panel(width, done),
            UiCommand::WebPanelGrip { x, done } => engine.drag_web_panel_grip(x, done),
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
            UiCommand::ImportPreview { key } => {
                let browser = magnetite_importer::installed()
                    .into_iter()
                    .find(|b| b.key == key)
                    .ok_or("That browser is not on this computer.")?;
                let snap = magnetite_importer::import(&browser).map_err(|e| e.to_string())?;
                let (spaces, favorites) = preview(snap);
                return Ok(json!({ "spaces": spaces, "favorites": favorites }));
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
            UiCommand::ImportBookmarks { html, space } => {
                if html.len() > MAX_BOOKMARKS_FILE {
                    return Err("That file is too large to be a bookmarks file.".into());
                }
                // The HTML file browsers write, or Magnetite's own JSON or CSV.
                let found = magnetite_importer::bookmarks_file::bookmarks_any(&html)
                    .map_err(|e| e.to_string())?;
                let added = self.with_state(|s| {
                    let space = match space {
                        // A space named and gone meanwhile, or the private
                        // one: not the space in front in its place.
                        Some(id) if s.space(&id).is_none() || s.is_private_space(&id) => {
                            return Err("That space is gone.".to_string());
                        }
                        Some(id) => id,
                        None => s.active_space.clone(),
                    };
                    Ok(s.add_bookmarks(&space, found))
                })?;
                self.push_state(engine);
                return Ok(json!({ "bookmarks": added }));
            }
            UiCommand::ExportBookmarks {
                format,
                spaces: named,
            } => {
                use magnetite_importer::export;
                return self.with_state(|s| {
                    // The private space's bookmarks stay where they are.
                    let spaces: Vec<&Space> = (s.sidebar.spaces.iter())
                        .filter(|sp| !s.is_private_space(&sp.id))
                        .filter(|sp| named.is_empty() || named.contains(&sp.id))
                        .collect();
                    // Of several named, those that are still there count.
                    if spaces.is_empty() {
                        return Err(if named.len() > 1 { "Those spaces are gone." } else { "That space is gone." }.to_string());
                    }
                    let (text, ending) = match format {
                        BookmarkFormat::Html => (export::space_bookmarks_html(&spaces), "html"),
                        BookmarkFormat::Json => (export::space_bookmarks_json(&spaces), "json"),
                        BookmarkFormat::Csv => (export::space_bookmarks_csv(&spaces), "csv"),
                    };
                    let count: usize = (spaces.iter())
                        .map(|sp| sp.bookmarks.iter().flat_map(magnetite_core::SidebarNode::tabs).count())
                        .sum();
                    Ok(json!({ "text": text, "name": format!("magnetite-bookmarks.{ending}"), "bookmarks": count }))
                });
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
            UiCommand::CaptureSelect {
                rect,
                context: false,
            } => self.capture_region(engine, Some(rect)),
            UiCommand::CaptureSelect {
                rect,
                context: true,
            } => self.capture_context(engine, rect),
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
            UiCommand::SiteShortcuts { url } => return Ok(self.site_shortcuts(&url)),
            UiCommand::SetSiteShortcuts { url, page_first } => {
                self.set_site_shortcuts(engine, &url, page_first);
            }
            UiCommand::SiteEngine { url } => return Ok(self.site_engine(&url)),
            UiCommand::SetSiteEngine { url, system } => {
                self.set_site_engine(engine, &url, system)?;
            }
            UiCommand::ClearSystemEngineData => self.clear_system_engine_data(engine),
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
            UiCommand::DeviceGet { page } => return Ok(engine.device_config(page)),
            UiCommand::DeviceSet { page, config } => engine.set_device_config(page, config),
            UiCommand::AllPermissions { profile } => {
                let context = self.permission_logins(profile.as_deref())?;
                return Ok(json!({ "rows": engine.all_permissions(context.as_ref()) }));
            }
            UiCommand::SetProfilePermission {
                profile,
                origin,
                kind,
                value,
            } => {
                let context = self.permission_logins(profile.as_deref())?;
                if !engine.set_permission(context.as_ref(), &origin, kind, value) {
                    return Err("That is no website's address.".into());
                }
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
            UiCommand::DevPanel { panel } => self.dev_panel(engine, None, panel),
            UiCommand::DevReset => self.dev_reset(engine, None),
            UiCommand::DevEmulate { patch } => return self.dev_emulate(engine, patch),
            UiCommand::DevServers => Self::dev_servers(engine),
            UiCommand::DevSplitAs { profile } => self.dev_split_as(engine, &profile),
            UiCommand::DevResponsive => self.dev_responsive(engine),
            UiCommand::DevCaptureSet => self.capture_set(engine),
            UiCommand::DevRecordStart => return self.record_start(engine),
            UiCommand::DevRecord => self.record_toggle(engine),
            UiCommand::DevRecordFrame { id } => return Ok(self.record_frame(engine, id)),
            UiCommand::DevRecordData { id, data, mp4 } => self.record_data(id, &data, mp4)?,
            UiCommand::DevRecordEnd { id, failed } => self.record_end(engine, id, failed),
            UiCommand::DevRecordFile { reveal } => {
                let text = self.record_file(engine, reveal);
                if !text.is_empty() {
                    Self::ui_call(engine, "notice", &json!({ "text": text }));
                }
            }
            UiCommand::SetZoom { percent } => {
                if let Some(c) = self.focused_content(None) {
                    // 0: the zoom every site starts with (Settings → Appearance).
                    engine.set_content_zoom(c, (percent != 0).then_some(percent.clamp(25, 500)));
                }
            }
        }
        Ok(json!({"ok": true}))
    }
}
