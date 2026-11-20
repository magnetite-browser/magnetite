/// Identifies a content window (one loaded tab) for the engine's lifetime.
pub type ContentId = u64;

/// Commands from keyboard shortcuts in content windows and from the menu bar.
/// The sidebar handles its own keys in JavaScript and sends them as UI
/// queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shortcut {
    NewTab,
    /// `Cmd+N`: another window on the same sidebar.
    NewWindow,
    /// `Cmd+Shift+W`: close the window (the last one quits).
    CloseWindow,
    CloseTab,
    FocusAddress,
    ToggleSidebar,
    NextTab,
    PrevTab,
    ReopenClosed,
    TogglePin,
    NewSpace,
    /// `Cmd+Shift+P`: open the private space, or go to it.
    PrivateSpace,
    /// `Cmd+Shift+X`: close the private space at once, from any
    /// space, with everything opened from it.
    ClosePrivateSpace,
    /// `Cmd+Shift+D`: hide the private space, from any space: it stays
    /// open but shows nowhere until `PrivateSpace` goes to it again.
    HidePrivateSpace,
    NextSpace,
    PrevSpace,
    /// 1-based, `Cmd+1` to `Cmd+9`.
    SelectTab(u8),
    /// 1-based, `Ctrl+1` to `Ctrl+9` (`Alt` outside macOS).
    SelectSpace(u8),
    /// `Ctrl+0` (`Alt+0` outside macOS): the list of all spaces by name,
    /// to choose one from.
    SpacePicker,
    /// Keys a space has of its own were pressed (Settings → Shortcuts);
    /// the app knows whose they are.
    SpaceKeys(crate::keymap::Combo),
    /// Escape inside the peek window.
    ClosePeek,
    /// `Cmd+O` inside the peek window: keep the page as a tab.
    PeekToTab,
    /// The peek's split button: keep the page as a tab beside the current one.
    PeekToSplit,
    /// `Cmd+Shift+C`: copy the page's link (the page may claim the key).
    CopyLink,
    /// `Cmd+Shift+O`: open the site panel (the page may claim the key).
    SitePanel,
    /// `Cmd+Option+P`: close the open web panel, or open the one used
    /// last (the page may claim the key).
    WebPanel,
    /// `Cmd+Shift+2`: start the capture tool (the page may claim the key).
    Capture,
    /// `Cmd+Shift+=`: open a page beside the current one (split view).
    NewSplit,
    /// `Cmd+Option+I` / F12: open or close the page's `DevTools`.
    ToggleDevTools,
    /// `Cmd+Option+J`, `Cmd+Option+C`, `Cmd+Option+E`: open the page's
    /// `DevTools` on that panel, or start their element picker.
    DevPanel(DevPanel),
    /// `Cmd+Option+R`: clear the site's storage and load the page anew,
    /// past the cache (the page may claim the key).
    DevReset,
    /// Menu bar, `Cmd+,`: open Settings.
    Settings,
    /// Menu bar, `Cmd+Option+L` (Chromium's downloads command): the sidebar's
    /// download list.
    Downloads,
    /// Menu bar: reload the page in front (Chromium takes `Cmd+R` itself).
    Reload,
    /// Menu bar, `Cmd+[` / `Cmd+]`: go back or forward in the page in front.
    Back,
    Forward,
    /// Menu bar: zoom the page in front a step in or out, or back to 100 %.
    ZoomIn,
    ZoomOut,
    ZoomReset,
    /// `Cmd+P`: print the page in front (the page may claim the key).
    Print,
    /// `Cmd+F`: find in the page in front (the page may claim the key).
    Find,
    /// `Cmd+G` / `Cmd+Shift+G`: the next or previous match (the page may
    /// claim the keys).
    FindNext,
    FindPrevious,
    /// Menu bar, `Cmd+Y`: the history view in the sidebar.
    History,
    /// `Cmd+Option+N`, system-wide or from the menu bar: the quick search,
    /// whose pick opens in a mini window.
    NewMini,
}

/// Where `DevTools` open (`Engine::content_show_panel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevPanel {
    Elements,
    Console,
    Network,
    Sources,
    Application,
    /// The Elements panel with the element picker started.
    Picker,
}

impl DevPanel {
    /// The panel's name in the `DevTools` frontend.
    pub(crate) fn frontend_name(self) -> &'static str {
        match self {
            Self::Elements | Self::Picker => "elements",
            Self::Console => "console",
            Self::Network => "network",
            Self::Sources => "sources",
            Self::Application => "resources",
        }
    }
}

/// A choice from our items in the `DevTools` menu (`EngineEvent::DevToolsMenu`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevToolsAction {
    /// Dock there from now on (the setting), or open in their own window.
    Dock(crate::DevToolsDock),
    Close,
}

/// Where a download is (`DownloadReport`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadState {
    InProgress,
    Finished,
    /// Cancelled or interrupted.
    Failed,
}

/// One download as Chromium reports it (`EngineEvent::Download`).
#[derive(Debug, Clone)]
pub struct DownloadReport {
    /// Chromium's id: unique per profile while Magnetite runs.
    pub id: u32,
    /// Context id of the profile (`""`: the default one), if known.
    pub context: Option<String>,
    /// The file's name (final once finished).
    pub name: String,
    /// Where the file is, once finished.
    pub path: Option<std::path::PathBuf>,
    pub received: i64,
    /// Size, when the server said.
    pub total: Option<i64>,
    pub state: DownloadState,
}

/// Who asked for a URL (`EngineEvent::OpenUrlRequested`); the app decides by
/// it which kinds of address to open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlSource {
    /// A web page (or an extension page, or `DevTools`).
    Page,
    /// Magnetite's own UI.
    Ui,
    /// Another app (we are the default browser), or `open -a`.
    OtherApp,
    /// A window Chromium opened itself, such as an extension's new tab.
    Chromium,
}

/// A choice from the menu bar's Extensions menu (`EngineEvent::ExtensionMenu`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionMenuAction {
    /// Open the extension (its popup) for the page in front.
    Open(String),
    /// Show it in the sidebar, or stop showing it there.
    TogglePin(String),
    /// `chrome://extensions`.
    Manage,
    /// The Chrome Web Store, to install more.
    Store,
}

/// What happened to a split divider (`EngineEvent::SplitDivider`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DividerAction {
    /// Dragged to this screen position (DIP) along the split's axis.
    Move(i32),
    /// Let go after dragging.
    Release,
    /// Double-clicked: give the split equal sizes.
    Equalize,
}

/// How short of memory the system says it is (`EngineEvent::MemoryPressure`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressure {
    /// Memory is getting tight.
    Warning,
    /// The system is about to end processes to get memory back.
    Critical,
}

/// Everything the engine reports to the application.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// A choice in page `id`'s `DevTools` menu: dock elsewhere or close.
    DevToolsMenu {
        id: ContentId,
        action: DevToolsAction,
    },
    /// The divider beside docked DevTools was let go or double-clicked:
    /// their size per side, to keep.
    DevToolsResized(crate::DevToolsSize),
    /// The web panel showing page `id` was dragged to `width` points and
    /// let go, or double-clicked at its edge (none: the usual width).
    WebPanelResized {
        id: ContentId,
        width: Option<u32>,
    },
    /// The close button of the open web panel was pressed.
    WebPanelClose,
    /// A divider between split pages was dragged, let go or double-clicked.
    /// `path` and `index` name it as in `magnetite_core::split::Divider`.
    SplitDivider {
        path: Vec<usize>,
        index: usize,
        action: DividerAction,
    },
    /// Page `id` logged an error to its console: an uncaught exception, a
    /// failed request, `console.error`.
    ConsoleError {
        id: ContentId,
    },
    /// A picture of page `id`, which is being recorded
    /// (`Engine::screencast`): a JPEG, base64.
    ScreencastFrame {
        id: ContentId,
        jpeg: String,
    },
    /// Page `id` could not load `url` and shows an error page at that
    /// address; no `UrlChanged` comes for it. `refused`: nothing listens at
    /// that address (a development server that is not running).
    LoadFailed {
        id: ContentId,
        url: String,
        refused: bool,
    },
    /// CEF is initialised; the delegate may create the frame now.
    Ready,
    /// The frame window and the sidebar view exist.
    /// Window `win` has its frame: its sidebar loads, pages can be shown.
    FrameCreated {
        win: crate::WindowId,
    },
    /// Window `win` closed, one of several; its pages report their own
    /// closing (`ContentClosed`).
    WindowClosed {
        win: crate::WindowId,
    },
    /// The sidebar page finished loading (safe to push state).
    SidebarLoaded,
    FrameBoundsChanged,
    ContentCreated {
        id: ContentId,
    },
    ContentClosed {
        id: ContentId,
    },
    TitleChanged {
        id: ContentId,
        title: String,
    },
    UrlChanged {
        id: ContentId,
        url: String,
    },
    FaviconChanged {
        id: ContentId,
        url: Option<String>,
    },
    /// A page got keyboard focus.
    ContentFocused {
        id: ContentId,
    },
    /// What the page plays changed; `None` when nothing plays any more.
    MediaChanged {
        id: ContentId,
        media: Option<crate::MediaState>,
    },
    /// A choice from the menu bar's Extensions menu.
    ExtensionMenu(ExtensionMenuAction),
    /// Chromium's own extension pins changed ("Pin to toolbar"): read
    /// [`crate::Engine::extensions`] again.
    ExtensionPinsChanged,
    /// The password and autofill switches were changed on one of Chromium's
    /// own pages, to these: keep them and hand them back with
    /// [`crate::Engine::set_autofill`], which sets every profile alike.
    AutofillChanged(crate::AutofillPrefs),
    /// A download started, moved on (about every percent), finished or failed.
    Download(DownloadReport),
    /// A page asks for a permission; answer with
    /// [`crate::Engine::answer_permission_prompt`].
    PermissionPrompt(crate::PermissionPrompt),
    /// A permission prompt is over: answered, or gone with a navigation or
    /// its page.
    PermissionPromptClosed {
        id: u64,
    },
    /// "Back to Tab" on the picture-in-picture controls: show page `id`
    /// (the engine already put its video back into the page).
    BackToTab {
        id: ContentId,
    },
    /// Connection or zoom of a page changed (after loads and zoom keys).
    SiteChanged {
        id: ContentId,
        state: crate::SiteState,
    },
    /// The page started or stopped using the camera or microphone.
    MediaAccessChanged {
        id: ContentId,
        camera: bool,
        microphone: bool,
    },
    /// Another page (or the page itself) started or stopped sharing this
    /// tab's picture (`tabshare.rs`).
    ShareChanged {
        id: ContentId,
        shared: bool,
    },
    LoadingChanged {
        id: ContentId,
        loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
    },
    /// Something asks for `url` in a new tab: a page (target=_blank, middle
    /// click, …), another app, or a window Chromium opened itself. `new_window`
    /// is set for Shift+click / window.open with a window disposition,
    /// `own_window` for "Open Link in New Window" in the page's menu; `locked`
    /// when the source tab is domain-locked. `context` is the profile id of
    /// the opener's context (`""`: the default context; `None`: no opener),
    /// so the new tab keeps the opener's cookies and nothing else.
    OpenUrlRequested {
        from: Option<ContentId>,
        url: String,
        background: bool,
        new_window: bool,
        /// The user chose "Open Link in New Window" in the page's menu.
        own_window: bool,
        locked: bool,
        source: UrlSource,
        context: Option<String>,
    },
    Shortcut {
        from: Option<ContentId>,
        shortcut: Shortcut,
    },
    /// Magnetite was started again while running (a link from another app,
    /// `magnetite <url>` in a terminal, the menu entry). `argv` is that
    /// start's whole command line, `cwd` the folder it ran in.
    Relaunched {
        argv: Vec<String>,
        cwd: Option<std::path::PathBuf>,
    },
    /// The user asked to quit (Quit in a menu, the second Cmd+Q, SIGTERM;
    /// on Linux and Windows also closing the last window). The delegate
    /// should save state and call [`crate::Engine::quit`].
    QuitRequested,
    /// The last window was closed and Magnetite runs on without one (macOS).
    /// The delegate should save state, as before quitting.
    FrameHidden,
    /// Fires roughly once a minute for housekeeping (auto-archive, unloading).
    Tick,
    /// The system is short of memory: pages nobody looks at should go.
    MemoryPressure(MemoryPressure),
    /// "Open in" on mini window `id`: make its page a tab of `space` (the
    /// space shown first, `Engine::set_mini_spaces`, when `None`).
    MiniToTab {
        id: ContentId,
        space: Option<String>,
    },
    /// Enter in mini window `id`'s address field: `input` is what was
    /// typed, an address or a search.
    MiniNavigate {
        id: ContentId,
        input: String,
    },
}

impl EngineEvent {
    /// The event's kind, for logs: never its payload, which carries the
    /// user's addresses, titles and file names.
    pub fn name(&self) -> &'static str {
        match self {
            Self::DevToolsMenu { .. } => "DevToolsMenu",
            Self::SplitDivider { .. } => "SplitDivider",
            Self::DevToolsResized(_) => "DevToolsResized",
            Self::WebPanelResized { .. } => "WebPanelResized",
            Self::WebPanelClose => "WebPanelClose",
            Self::ConsoleError { .. } => "ConsoleError",
            Self::LoadFailed { .. } => "LoadFailed",
            Self::ScreencastFrame { .. } => "ScreencastFrame",
            Self::Ready => "Ready",
            Self::FrameCreated { .. } => "FrameCreated",
            Self::WindowClosed { .. } => "WindowClosed",
            Self::SidebarLoaded => "SidebarLoaded",
            Self::FrameBoundsChanged => "FrameBoundsChanged",
            Self::ContentCreated { .. } => "ContentCreated",
            Self::ContentClosed { .. } => "ContentClosed",
            Self::TitleChanged { .. } => "TitleChanged",
            Self::UrlChanged { .. } => "UrlChanged",
            Self::FaviconChanged { .. } => "FaviconChanged",
            Self::ContentFocused { .. } => "ContentFocused",
            Self::MediaChanged { .. } => "MediaChanged",
            Self::ShareChanged { .. } => "ShareChanged",
            Self::ExtensionMenu(..) => "ExtensionMenu",
            Self::ExtensionPinsChanged => "ExtensionPinsChanged",
            Self::AutofillChanged(_) => "AutofillChanged",
            Self::Download(_) => "Download",
            Self::PermissionPrompt(_) => "PermissionPrompt",
            Self::PermissionPromptClosed { .. } => "PermissionPromptClosed",
            Self::BackToTab { .. } => "BackToTab",
            Self::SiteChanged { .. } => "SiteChanged",
            Self::MediaAccessChanged { .. } => "MediaAccessChanged",
            Self::LoadingChanged { .. } => "LoadingChanged",
            Self::OpenUrlRequested { .. } => "OpenUrlRequested",
            Self::Shortcut { .. } => "Shortcut",
            Self::Relaunched { .. } => "Relaunched",
            Self::QuitRequested => "QuitRequested",
            Self::FrameHidden => "FrameHidden",
            Self::Tick => "Tick",
            Self::MemoryPressure(_) => "MemoryPressure",
            Self::MiniToTab { .. } => "MiniToTab",
            Self::MiniNavigate { .. } => "MiniNavigate",
        }
    }
}
