//! The engine facade and its shared state.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};

use cef::wrapper::message_router::BrowserSideRouter;
use cef::*;

use crate::config::EngineConfig;
use crate::content::ContentState;
use crate::events::{ContentId, EngineEvent};
use crate::frame::FrameState;

/// Implemented by the application. All methods run on the CEF UI thread.
pub trait EngineDelegate: Send + Sync + 'static {
    fn on_event(&self, engine: &Engine, event: EngineEvent);
    /// A command from one of Magnetite's own pages (`window.magnetiteQuery`);
    /// `origin` is the URL that page was loaded from (`magnetite://ui/...`),
    /// so the app can keep some commands to one page. Returns the response JSON or
    /// an error message.
    fn on_ui_query(&self, engine: &Engine, origin: &str, request: &str) -> Result<String, String>;
}

/// Browser context a page runs in, for pages outside the default context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextProfile {
    pub id: String,
    /// In memory only: logins and storage are gone when Magnetite quits.
    pub ephemeral: bool,
}

/// Identifies one of Magnetite's main windows (a frame with its sidebar
/// and pages).
pub type WindowId = u32;

/// What belongs to one main window. The engine works on one window at a
/// time, the current one (`Inner::win`); the others wait in `Inner::parked`
/// (`Engine::enter`).
pub(crate) struct WindowState {
    pub frame: Option<FrameState>,
    pub sidebar_visible: bool,
    /// The sidebar browser is created while the frame window is still being
    /// built; it is parked here until `FrameState` exists.
    pub pending_sidebar_browser: Option<Browser>,
    /// The visible split (two or more pages), if any.
    pub split: Option<magnetite_core::split::SplitNode<ContentId>>,
    /// Content window shown as a peek overlay, if any.
    pub peek: Option<ContentId>,
    /// Content window shown as a web panel: over the pages, at the
    /// sidebar's edge of the page area, if any.
    pub web_panel: Option<ContentId>,
    /// The width asked for the web panel, in points; none: the usual.
    pub web_panel_width: Option<u32>,
    /// The page last shown alone (`show_content`) and not hidden since.
    pub shown: Option<ContentId>,
    /// Last draggable regions reported by the sidebar page (its own
    /// coordinates: the body's).
    pub sidebar_drag: Vec<DraggableRegion>,
    /// Last draggable regions reported by the top bar page (window coordinates).
    pub top_bar_drag: Vec<DraggableRegion>,
    /// The developer bar shows above this window's pages
    /// (`set_dev_bar_visible`).
    pub dev_bar_visible: bool,
    /// The split view's dividers as last laid out (`update_dividers`),
    /// shown together with the docked DevTools' ones (`show_dividers`).
    pub split_dividers: Vec<magnetite_core::split::Divider>,
    /// Background of the page area (ARGB), for windows drawn in its gaps.
    pub chrome_color: u32,
    /// The sidebar floating over the pages while the docked one is hidden.
    pub float_bar: Option<crate::floatbar::FloatBar>,
    /// Space colour (ARGB): the outline of the focused page in a split.
    pub accent_color: u32,
    /// The page of the split on screen that has the keyboard.
    pub focused_pane: Option<ContentId>,
}

impl WindowState {
    /// A window that has no frame yet.
    pub(crate) fn new() -> Self {
        Self {
            frame: None,
            sidebar_visible: true,
            pending_sidebar_browser: None,
            split: None,
            peek: None,
            web_panel: None,
            web_panel_width: None,
            shown: None,
            sidebar_drag: Vec::new(),
            top_bar_drag: Vec::new(),
            dev_bar_visible: false,
            split_dividers: Vec::new(),
            focused_pane: None,
            float_bar: None,
            accent_color: 0xFF78_82FF,
            chrome_color: 0xFF1B_1B1E,
        }
    }
}

pub(crate) struct Inner {
    /// The current window's state: what every call that names no window
    /// works on.
    pub win: WindowState,
    /// The current window's id.
    pub current: WindowId,
    /// The other windows.
    pub parked: HashMap<WindowId, WindowState>,
    /// The id the next window gets.
    pub next_window: WindowId,
    pub contents: HashMap<ContentId, ContentState>,
    pub next_id: ContentId,
    pub router: Option<Arc<BrowserSideRouter>>,
    pub quitting: bool,
    /// Content windows locked to a host (pinned tabs, favorites): user
    /// navigations to another host are redirected to the application.
    pub locks: HashMap<ContentId, String>,
    /// Which parts of Chromium's password manager and autofill are on
    /// (`autofill.rs`), for the profiles made from now on.
    pub autofill: crate::AutofillPrefs,
    /// Pages the app muted. Kept here because a page gets its browser a
    /// moment after it is asked for: the mute is applied once it has one.
    pub muted: HashSet<ContentId>,
    /// Which tabs pages capture (`tabshare.rs`).
    pub shares: crate::tabshare::Shares,
    /// Pages shown as on a phone (`set_mobile_view`). Kept here for the
    /// same reason, and to give it back after device mode.
    pub mobile: HashSet<ContentId>,
    /// When one of our own pages was last reloaded after its renderer died,
    /// by browser identifier (`may_reload_ui`).
    pub ui_reloads: HashMap<i32, std::time::Instant>,
    /// One CEF request context (cookies, storage, extensions) per profile id.
    pub contexts: HashMap<String, RequestContext>,
    /// Browsers in windows Chromium made, not we: a page's popup window,
    /// an extension's pop-up window. `quit` closes them; each one left open
    /// would keep the app running.
    pub foreign: HashMap<i32, Browser>,
    /// Keeps the watches on each profile's Chromium preferences alive
    /// (`extensions::watch_pins`, `autofill::watch`).
    pub pref_watches: Vec<Registration>,
    /// Profile ids whose context Chromium has finished making: only then
    /// can a browser be made in it (`context_ready`).
    pub ready_contexts: HashSet<String>,
    /// Pages asked for while their profile's context was still being made
    /// (`create_content`); they get their window once it is ready.
    pub queued: Vec<crate::content::QueuedContent>,
    /// Runtime override of `EngineConfig::chrome_toolbar` (settings change).
    pub chrome_toolbar: bool,
    /// The top bar is on (`set_top_bar_visible`).
    pub top_bar_visible: bool,
    /// Where DevTools open, and the docked ones per page.
    pub devtools_dock: crate::DevToolsDock,
    pub docked: HashMap<ContentId, crate::devdock::DockedDevTools>,
    /// The panel `DevTools` that are opening show first, per page.
    pub pending_panels: HashMap<ContentId, crate::DevPanel>,
    /// The docked DevTools' size the user dragged, per side.
    pub devtools_size: crate::DevToolsSize,
    /// The device device mode used last (`device.rs`), to start with next.
    pub last_device: Option<crate::device::DeviceConfig>,
    /// The open overlay window (capture tool), if any.
    pub overlay: Option<Window>,
    /// The overlay is the quick search: a window of its own, not the frame's.
    pub overlay_detached: bool,
    /// The quick search opened while another app was in front: closing it
    /// without a pick gives that app the keyboard back (`close_overlay`).
    pub quick_from_outside: bool,
    /// Spaces a mini window offers to open its page in, `(id, title)`
    /// (`set_mini_spaces`).
    pub mini_spaces: Vec<(String, String)>,
    /// The notification panel (`notifications.rs`), once made.
    pub notifications: Option<Window>,
    /// Its height as its page asked.
    pub notifications_height: i32,
    /// The window the notification panel is in.
    pub notifications_win: WindowId,
    /// Origins seen per profile id this session (clear site data).
    pub origins: HashMap<String, std::collections::BTreeSet<String>>,
    /// The address each window's page was loaded from, by browser
    /// identifier. The bridge names a page by it (`router::query_origin`):
    /// the address a page shows can change without anything being loaded.
    pub loaded_urls: HashMap<i32, String>,
    /// Permission requests per page window this session (site panel): the
    /// origin of the page when it was asked, the origin that asked, and what
    /// for. The first keeps a page from listing what an earlier page in the
    /// same tab asked for.
    pub permission_requests:
        HashMap<ContentId, std::collections::BTreeSet<(String, String, crate::PermissionKind)>>,
    /// Width of the docked sidebar (Settings → Appearance).
    pub sidebar_width: i32,
    /// Installed extensions per profile (`extensions.rs`).
    pub extensions: crate::extensions::ExtensionCache,
}

impl Inner {
    /// Window `id`'s state, current or parked.
    pub(crate) fn window(&self, id: WindowId) -> Option<&WindowState> {
        if id == self.current {
            Some(&self.win)
        } else {
            self.parked.get(&id)
        }
    }

    pub(crate) fn window_mut(&mut self, id: WindowId) -> Option<&mut WindowState> {
        if id == self.current {
            Some(&mut self.win)
        } else {
            self.parked.get_mut(&id)
        }
    }

    /// Every window with its id, the current one first.
    pub(crate) fn windows_mut(&mut self) -> impl Iterator<Item = (WindowId, &mut WindowState)> {
        std::iter::once((self.current, &mut self.win))
            .chain(self.parked.iter_mut().map(|(id, w)| (*id, w)))
    }

    /// The window page `id` lives in (the current one for a page unknown).
    pub(crate) fn home_of(&self, id: ContentId) -> WindowId {
        self.contents.get(&id).map_or(self.current, |c| c.home)
    }
}

impl Inner {
    /// Window `id` is the current one now; `engine` notes it for
    /// `Engine::current_window`.
    pub(crate) fn set_current(&mut self, engine: &Engine, id: WindowId) -> WindowId {
        engine
            .current
            .store(id, std::sync::atomic::Ordering::Relaxed);
        std::mem::replace(&mut self.current, id)
    }
}

/// The running engine: one global instance (`Engine::global`).
pub struct Engine {
    pub(crate) inner: Mutex<Inner>,
    /// `Inner::current` again, to read without the lock (`current_window`):
    /// events are sent from code that holds it.
    pub(crate) current: std::sync::atomic::AtomicU32,
    pub(crate) delegate: Arc<dyn EngineDelegate>,
    pub(crate) config: EngineConfig,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();

impl Engine {
    /// Make the global instance; `run` calls this once, before CEF starts.
    pub(crate) fn init(config: EngineConfig, delegate: Arc<dyn EngineDelegate>) {
        let engine = Engine {
            inner: Mutex::new(Inner {
                win: WindowState::new(),
                current: 1,
                parked: HashMap::new(),
                next_window: 2,
                contents: HashMap::new(),
                next_id: 1,
                router: None,
                quitting: false,
                locks: HashMap::new(),
                autofill: crate::AutofillPrefs::default(),
                muted: HashSet::new(),
                shares: crate::tabshare::Shares::default(),
                mobile: HashSet::new(),
                ui_reloads: HashMap::new(),
                contexts: HashMap::new(),
                foreign: HashMap::new(),
                pref_watches: Vec::new(),
                ready_contexts: HashSet::new(),
                queued: Vec::new(),
                chrome_toolbar: config.chrome_toolbar,
                top_bar_visible: false,
                devtools_dock: crate::DevToolsDock::default(),
                docked: HashMap::new(),
                pending_panels: HashMap::new(),
                devtools_size: crate::DevToolsSize::default(),
                last_device: None,
                overlay: None,
                overlay_detached: false,
                quick_from_outside: false,
                mini_spaces: Vec::new(),
                notifications: None,
                notifications_height: 0,
                notifications_win: 1,
                origins: HashMap::new(),
                loaded_urls: HashMap::new(),
                permission_requests: HashMap::new(),
                sidebar_width: config.sidebar_width,
                extensions: crate::extensions::ExtensionCache::default(),
            }),
            current: std::sync::atomic::AtomicU32::new(1),
            delegate,
            config,
        };
        // Only the first call counts.
        let _ = ENGINE.set(engine);
    }

    /// The engine instance, once `init` made it.
    pub fn try_global() -> Option<&'static Engine> {
        ENGINE.get()
    }

    /// The engine instance. Panics before `init`.
    pub fn global() -> &'static Engine {
        ENGINE.get().expect("engine not initialised")
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Deliver an event to the delegate on the next message-loop iteration,
    /// so delegates are never called while an engine API call is on the
    /// stack (which would deadlock their own state locks).
    #[allow(clippy::unused_self)] // an Engine method like its siblings
    ///
    /// The delegate hears of it in the window that is current now: an event
    /// from a window in the background is handled there.
    pub(crate) fn emit(&self, event: EngineEvent) {
        let win = self.current_window();
        Self::post(Box::new(move || {
            let engine = Engine::global();
            let mut event = Some(event);
            let handle = || {
                if let Some(event) = event.take() {
                    engine.delegate.on_event(engine, event);
                }
            };
            // A window closed in the meantime: the current one hears of it.
            if engine.in_window(win, handle).is_none()
                && let Some(event) = event
            {
                engine.delegate.on_event(engine, event);
            }
        }));
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Inner> {
        lock(&self.inner)
    }

    /// The current window's frame, once it exists.
    pub(crate) fn frame_window(&self) -> Option<Window> {
        self.lock().win.frame.as_ref().map(|f| f.window.clone())
    }

    /// Run `f` on the CEF UI thread (from any thread). Used by background
    /// workers to hand results back to the application.
    pub fn post(f: Box<dyn FnOnce() + Send>) {
        let mut task = Closure::new(Arc::new(Mutex::new(Some(f))));
        post_task(ThreadId::UI, Some(&mut task));
    }

    /// Whether the page of browser `id`, one of ours whose renderer just
    /// died, should be loaded again: not when the last reload was under ten
    /// seconds ago, so a page that dies while loading is not reloaded forever.
    pub(crate) fn may_reload_ui(&self, id: i32) -> bool {
        let now = std::time::Instant::now();
        let mut inner = self.lock();
        let recent = inner
            .ui_reloads
            .get(&id)
            .is_some_and(|last| now.duration_since(*last) < std::time::Duration::from_secs(10));
        if !recent {
            inner.ui_reloads.insert(id, now);
        }
        !recent
    }

    /// Run `f` on the CEF UI thread in `delay_ms` milliseconds (from any thread).
    pub fn post_delayed(f: Box<dyn FnOnce() + Send>, delay_ms: i64) {
        let mut task = Closure::new(Arc::new(Mutex::new(Some(f))));
        post_delayed_task(ThreadId::UI, Some(&mut task), delay_ms);
    }

    /// Answer the UI query being handled later, e.g. after a network call:
    /// call from inside `on_ui_query`, then pass the ticket to
    /// `finish_reply` (on the UI thread, see `post`). What `on_ui_query`
    /// returns is then ignored.
    pub fn defer_reply(&self) -> Option<crate::router::ReplyTicket> {
        crate::router::defer_reply()
    }

    /// Answer a query deferred with `defer_reply`. UI thread only.
    pub fn finish_reply(&self, ticket: crate::router::ReplyTicket, result: Result<String, String>) {
        crate::router::finish_reply(ticket, result);
    }

    /// Run `js` in the sidebar the user sees: the floating one while it is
    /// out, else the docked one. For events such as "open the site panel".
    pub fn ui_eval(&self, js: &str) {
        let browser = if self.float_sidebar_shown() {
            self.float_sidebar_browser()
        } else {
            self.docked_sidebar_browser()
        };
        // The top bar holds the address row while it is on.
        let top_bar = self
            .top_bar_visible()
            .then(|| self.top_bar_browser())
            .flatten();
        let dev_bar = self.dev_bar_browser();
        for frame in [browser, top_bar, dev_bar]
            .into_iter()
            .flatten()
            .filter_map(|b| b.main_frame())
        {
            self.eval_in_ui(&frame, js);
        }
    }

    /// Run `js` in every sidebar page, the top bar and the empty page, shown
    /// or not, and in the overlay (a pop-up of the top bar shows live state):
    /// state pushes.
    pub fn ui_eval_all(&self, js: &str) {
        let overlay = (self.lock().overlay.clone())
            .and_then(|w| w.child_view_at(0)?.as_browser_view()?.browser());
        for browser in [
            self.docked_sidebar_browser(),
            self.float_sidebar_browser(),
            self.top_bar_browser(),
            self.dev_bar_browser(),
            self.empty_browser(),
            overlay,
        ]
        .into_iter()
        .flatten()
        {
            if let Some(frame) = browser.main_frame() {
                self.eval_in_ui(&frame, js);
            }
        }
    }

    /// Run `js` in page `id` if it shows our UI (a Settings tab): for news
    /// such a page shows but does not ask for.
    pub fn content_ui_eval(&self, id: ContentId, js: &str) {
        if let Some(frame) = self.content_browser(id).and_then(|b| b.main_frame()) {
            self.eval_in_ui(&frame, js);
        }
    }

    /// Run `js` in `frame` only while it shows our UI: state and calls carry
    /// the user's tabs, which no other page may see.
    pub(crate) fn eval_in_ui(&self, frame: &Frame, js: &str) {
        if self.is_own_ui(&CefString::from(&frame.url()).to_string()) {
            frame.execute_java_script(Some(&CefString::from(js)), None, 0);
        }
    }

    /// `url` is Magnetite's UI: `magnetite://ui/`, or the development server
    /// the sidebar was pointed at (`--ui-url`).
    pub(crate) fn is_own_ui(&self, url: &str) -> bool {
        let dev = &self.config.ui_url;
        crate::router::is_ui_url(url)
            || (dev != crate::EngineConfig::SIDEBAR_URL
                && !dev.is_empty()
                && url.starts_with(dev.as_str()))
    }

    fn top_bar_browser(&self) -> Option<Browser> {
        self.lock()
            .win
            .frame
            .as_ref()
            .and_then(|f| f.top_bar_browser.clone())
    }

    pub(crate) fn dev_bar_browser(&self) -> Option<Browser> {
        self.lock()
            .win
            .frame
            .as_ref()
            .and_then(|f| f.dev_bar_browser.clone())
    }

    fn docked_sidebar_browser(&self) -> Option<Browser> {
        self.lock()
            .win
            .frame
            .as_ref()
            .and_then(|f| f.sidebar_browser.clone())
    }

    /// Close every window, ours and those Chromium made (`foreign`); the
    /// message loop ends once they are gone. Callers save their state
    /// first: if the process still runs `QUIT_GRACE_MS` later (a window we
    /// do not know of, a shutdown that waits on one), it ends without the
    /// rest of the shutdown.
    pub fn quit(&self) {
        let (contents, foreign): (Vec<Window>, Vec<Browser>) = {
            let mut inner = self.lock();
            inner.quitting = true;
            (
                inner
                    .contents
                    .values()
                    .filter_map(|c| c.window.clone())
                    .collect(),
                inner.foreign.values().cloned().collect(),
            )
        };
        let frames: Vec<Window> = (self.lock().windows_mut())
            .filter_map(|(_, w)| w.frame.as_ref().map(|f| f.window.clone()))
            .collect();
        for b in foreign {
            if let Some(host) = b.host() {
                host.close_browser(1);
            }
        }
        for w in contents {
            w.close();
        }
        for w in frames {
            w.close();
        }
        // Not a task on the message loop: the loop may have ended and still
        // not returned (a window we do not know of), or `shutdown` may wait
        // on such a window. The process ends once this thread wakes.
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(QUIT_GRACE_MS));
            eprintln!("magnetite: still running {QUIT_GRACE_MS} ms after quitting; exiting");
            // SAFETY: ends the process at once, skipping exit handlers that
            // would wait on the stuck shutdown.
            unsafe { libc::_exit(0) };
        });
    }

    pub fn is_quitting(&self) -> bool {
        self.lock().quitting
    }

    /// Toggle Chrome's toolbar row for content windows created from now on.
    pub fn set_chrome_toolbar(&self, on: bool) {
        self.lock().chrome_toolbar = on;
    }

    pub fn chrome_toolbar(&self) -> bool {
        self.lock().chrome_toolbar
    }

    /// The request context for `profile`; `None` is the global context.
    /// Other profiles get their own cache directory below the root cache
    /// path (separate cookies, storage and extensions); ephemeral ones keep
    /// everything in memory, until `forget_contexts` or until Magnetite
    /// quits. Chromium finishes a new
    /// context a little later (`context_is_ready`).
    pub(crate) fn request_context(
        &self,
        profile: Option<&ContextProfile>,
    ) -> Option<RequestContext> {
        let profile = profile.filter(|p| !p.id.is_empty())?;
        if let Some(ctx) = self.lock().contexts.get(&profile.id) {
            return Some(ctx.clone());
        }
        let settings = if profile.ephemeral {
            // An empty cache path makes CEF create an in-memory context.
            RequestContextSettings::default()
        } else {
            let dir = self.config.profile_dir(&profile.id);
            if let Err(e) = std::fs::create_dir_all(&dir) {
                crate::trace(&format!("cannot create {}: {e}", dir.display()));
            }
            RequestContextSettings {
                cache_path: CefString::from(dir.to_string_lossy().as_ref()),
                persist_session_cookies: 1,
                ..Default::default()
            }
        };
        let mut handler = ContextSetup::new(profile.id.clone());
        let ctx = request_context_create_context(Some(&settings), Some(&mut handler))?;
        self.lock().contexts.insert(profile.id.clone(), ctx.clone());
        Some(ctx)
    }

    /// Let go of the in-memory contexts of the profiles `ids`, which nothing
    /// uses any more: Chromium drops such a context, with its cookies and
    /// storage, once its last page has closed. A popup window a page of
    /// theirs opened (`foreign`) would keep the context alive: it closes.
    pub fn forget_contexts(&self, ids: &[String]) {
        let popups: Vec<Browser> = self.lock().foreign.values().cloned().collect();
        for host in popups.iter().filter_map(ImplBrowser::host) {
            let context = host.request_context().and_then(|c| self.context_id_of(&c));
            if context.is_some_and(|id| ids.contains(&id)) {
                host.close_browser(1);
            }
        }
        let mut inner = self.lock();
        for id in ids {
            if inner.contexts.remove(id).is_some() {
                crate::trace(&format!("letting go of in-memory context {id}"));
            }
            inner.ready_contexts.remove(id);
        }
    }

    /// Whether a browser can be made in `profile`'s context now: always in
    /// the default one, in another once Chromium reported it initialised
    /// (`ContextSetup`).
    pub(crate) fn context_is_ready(&self, profile: Option<&ContextProfile>) -> bool {
        profile
            .filter(|p| !p.id.is_empty())
            .is_none_or(|p| self.lock().ready_contexts.contains(&p.id))
    }

    /// Profile id of page `id`'s context (`""`: the default context).
    pub(crate) fn content_context(&self, id: ContentId) -> Option<String> {
        self.lock().contents.get(&id).map(|c| c.profile.clone())
    }

    /// Profile id of request context `context` (`""`: the default one),
    /// found by identity, so in-memory (private) contexts are found too.
    /// `None` for a context Magnetite did not make.
    pub(crate) fn context_id_of(&self, context: &RequestContext) -> Option<String> {
        let same = |c: &RequestContext| c.is_same(Some(&mut context.clone())) != 0;
        if request_context_get_global_context().is_some_and(|g| same(&g)) {
            return Some(String::new());
        }
        self.lock()
            .contexts
            .iter()
            .find(|(_, c)| same(c))
            .map(|(id, _)| id.clone())
    }

    /// Delete the on-disk data of the profiles `ids` (deleted by the user);
    /// a profile without a folder is fine. Call before any page of those
    /// profiles was opened in this run (at startup): Chromium keeps the
    /// folder of a context it has made until it quits, and writes to it
    /// again.
    pub fn remove_profile_dirs(&self, ids: &[String]) {
        for id in ids {
            let path = self.config.profile_dir(id);
            if path.is_dir() {
                remove_profile_dir(&path);
            }
        }
    }

    /// Lock a content window to `host` (`None` removes the lock).
    pub fn set_content_lock(&self, id: ContentId, host: Option<String>) {
        let mut inner = self.lock();
        match host {
            Some(h) => {
                inner.locks.insert(id, h);
            }
            None => {
                inner.locks.remove(&id);
            }
        }
    }

    pub(crate) fn content_lock(&self, id: ContentId) -> Option<String> {
        self.lock().locks.get(&id).cloned()
    }

    /// Start the housekeeping timer and the SIGTERM watcher.
    #[allow(clippy::unused_self)] // an Engine method like its siblings
    pub(crate) fn start_tick(&self) {
        // SAFETY: installing a signal handler that only stores a flag.
        unsafe {
            libc::signal(
                libc::SIGTERM,
                sigterm_handler as *const () as libc::sighandler_t,
            );
            libc::signal(
                libc::SIGINT,
                sigterm_handler as *const () as libc::sighandler_t,
            );
        }
        schedule_tick(1000);
    }
}

/// Delete a profile's folder `path` with everything in it.
fn remove_profile_dir(path: &std::path::Path) {
    crate::trace(&format!(
        "removing data of deleted profile {}",
        path.display()
    ));
    if let Err(e) = std::fs::remove_dir_all(path) {
        crate::trace(&format!("cannot remove {}: {e}", path.display()));
    }
}

/// Lock `m`, also after a panic elsewhere left it poisoned: the engine's
/// state stays usable, and a stuck UI thread would be worse.
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

static TERM_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn sigterm_handler(_sig: libc::c_int) {
    TERM_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// How long closing the windows may take before `quit` gives up on them.
const QUIT_GRACE_MS: u64 = 5000;

fn schedule_tick(delay_ms: i64) {
    Engine::post_delayed(Box::new(tick), delay_ms);
}

/// Poll the signal flag often, and on macOS look for pages wrongly marked
/// covered; emit the housekeeping tick once a minute.
fn tick() {
    let engine = Engine::global();
    if engine.is_quitting() {
        return;
    }
    if TERM_REQUESTED.swap(false, std::sync::atomic::Ordering::SeqCst) {
        engine.emit(EngineEvent::QuitRequested);
        return;
    }
    #[cfg(target_os = "macos")]
    engine.uncover_shown_pages();
    static POLLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    if POLLS
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .is_multiple_of(120)
    {
        engine.emit(EngineEvent::Tick);
    }
    schedule_tick(500);
}

/// A closure `Engine::post` or `Engine::post_delayed` runs once on the UI thread.
type PostedFn = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;

wrap_task! {
    struct Closure {
        f: PostedFn,
    }
    impl Task {
        fn execute(&self) {
            if let Some(f) = self.f.lock().ok().and_then(|mut g| g.take()) {
                f();
            }
        }
    }
}

/// Chromium preferences every profile needs in Magnetite (the default one
/// gets them when its first browser exists, `client.rs`).
/// Returns whether they could be set.
pub(crate) fn prepare_context(ctx: &RequestContext) -> bool {
    // Chromium's download bubble would pop out of the hidden toolbar at the
    // page's corner; the notification panel tells about downloads instead.
    let set = crate::autofill::set_bool_pref(ctx, "download_bubble.partial_view_enabled", false);
    if !set {
        crate::trace("cannot turn off Chromium's download bubble");
    }
    // Chromium's translation is Google's: the text of a translated page goes
    // to Google. It also needs a Google API key, which this build has not
    // got, so today it offers nothing either way. Off, so that a build with
    // a key does not start offering it without a switch in Settings.
    if !crate::autofill::set_bool_pref(ctx, "translate.enabled", false) {
        crate::trace("cannot turn off Chromium's translation");
    }
    // Safe Browsing needs a Google API key as well: without one Google
    // refuses the update of its lists, and it never warns about anything.
    // Off, so the browser does not ask Google in vain.
    if !crate::autofill::set_bool_pref(ctx, "safebrowsing.enabled", false) {
        crate::trace("cannot turn off Chromium's Safe Browsing");
    }
    crate::extensions::watch_pins(ctx);
    // Password manager and autofill as the user's settings have them.
    let autofill = Engine::global().lock().autofill;
    if !autofill.apply(ctx) {
        crate::trace("cannot set the password and autofill preferences");
    }
    crate::autofill::watch(ctx);
    set
}

// Prepares profile `profile`'s context once Chromium has made it (a
// profile on disk loads asynchronously), then lets its pages be built.
wrap_request_context_handler! {
    struct ContextSetup {
        profile: String,
    }
    impl RequestContextHandler {
        fn on_request_context_initialized(&self, request_context: Option<&mut RequestContext>) {
            if let Some(ctx) = request_context {
                prepare_context(ctx);
            }
            Engine::global().context_ready(&self.profile);
        }

        // Asked for requests no browser's handler took: a service worker's.
        fn resource_request_handler(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _is_navigation: ::std::os::raw::c_int,
            _is_download: ::std::os::raw::c_int,
            request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<ResourceRequestHandler> {
            crate::blocker::resource_request_handler(browser.as_deref(), request.as_deref(), request_initiator)
        }
    }
}
