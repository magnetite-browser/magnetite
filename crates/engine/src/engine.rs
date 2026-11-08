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

pub(crate) struct Inner {
    pub frame: Option<FrameState>,
    pub contents: HashMap<ContentId, ContentState>,
    pub next_id: ContentId,
    pub sidebar_visible: bool,
    pub router: Option<Arc<BrowserSideRouter>>,
    pub quitting: bool,
    /// The sidebar browser is created while the frame window is still being
    /// built; it is parked here until `FrameState` exists.
    pub pending_sidebar_browser: Option<Browser>,
    /// Content windows locked to a host (pinned tabs, favorites): user
    /// navigations to another host are redirected to the application.
    pub locks: HashMap<ContentId, String>,
    /// Which parts of Chromium's password manager and autofill are on
    /// (`autofill.rs`), for the profiles made from now on.
    pub autofill: crate::AutofillPrefs,
    /// Pages the app muted. Kept here because a page gets its browser a
    /// moment after it is asked for: the mute is applied once it has one.
    pub muted: HashSet<ContentId>,
    /// When one of our own pages was last reloaded after its renderer died,
    /// by browser identifier (`may_reload_ui`).
    pub ui_reloads: HashMap<i32, std::time::Instant>,
    /// The visible split (two or more pages), if any.
    pub split: Option<magnetite_core::split::SplitNode<ContentId>>,
    /// Content window shown as a peek overlay, if any.
    pub peek: Option<ContentId>,
    /// One CEF request context (cookies, storage, extensions) per profile id.
    pub contexts: HashMap<String, RequestContext>,
    /// Profile ids whose context Chromium has finished making: only then
    /// can a browser be made in it (`context_ready`).
    pub ready_contexts: HashSet<String>,
    /// Pages asked for while their profile's context was still being made
    /// (`create_content`); they get their window once it is ready.
    pub queued: Vec<crate::content::QueuedContent>,
    /// The page last shown alone (`show_content`) and not hidden since.
    pub shown: Option<ContentId>,
    /// Runtime override of `EngineConfig::chrome_toolbar` (settings change).
    pub chrome_toolbar: bool,
    /// Last draggable regions reported by the sidebar page (window coordinates).
    pub sidebar_drag: Vec<DraggableRegion>,
    /// Where DevTools open, and the docked ones per page.
    pub devtools_dock: crate::DevToolsDock,
    pub docked: HashMap<ContentId, crate::devdock::DockedDevTools>,
    /// Background of the page area (ARGB), for windows drawn in its gaps.
    pub chrome_color: u32,
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
    /// The sidebar floating over the pages while the docked one is hidden.
    pub float_bar: Option<crate::floatbar::FloatBar>,
    /// Space colour (ARGB): the outline of the focused page in a split.
    pub accent_color: u32,
    /// The page of the split on screen that has the keyboard.
    pub focused_pane: Option<ContentId>,
    /// Width of the docked sidebar (Settings → Appearance).
    pub sidebar_width: i32,
    /// Installed extensions per profile (`extensions.rs`).
    pub extensions: crate::extensions::ExtensionCache,
}

/// The running engine: one global instance (`Engine::global`).
pub struct Engine {
    pub(crate) inner: Mutex<Inner>,
    pub(crate) delegate: Arc<dyn EngineDelegate>,
    pub(crate) config: EngineConfig,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();

impl Engine {
    /// Make the global instance; `run` calls this once, before CEF starts.
    pub(crate) fn init(config: EngineConfig, delegate: Arc<dyn EngineDelegate>) {
        let engine = Engine {
            inner: Mutex::new(Inner {
                frame: None,
                contents: HashMap::new(),
                next_id: 1,
                sidebar_visible: true,
                router: None,
                quitting: false,
                pending_sidebar_browser: None,
                locks: HashMap::new(),
                autofill: crate::AutofillPrefs::default(),
                muted: HashSet::new(),
                ui_reloads: HashMap::new(),
                split: None,
                peek: None,
                contexts: HashMap::new(),
                ready_contexts: HashSet::new(),
                queued: Vec::new(),
                shown: None,
                chrome_toolbar: config.chrome_toolbar,
                sidebar_drag: Vec::new(),
                devtools_dock: crate::DevToolsDock::default(),
                docked: HashMap::new(),
                chrome_color: 0xFF1B_1B1E,
                overlay: None,
                overlay_detached: false,
                quick_from_outside: false,
                mini_spaces: Vec::new(),
                notifications: None,
                notifications_height: 0,
                origins: HashMap::new(),
                loaded_urls: HashMap::new(),
                permission_requests: HashMap::new(),
                float_bar: None,
                accent_color: 0xFF78_82FF,
                focused_pane: None,
                sidebar_width: config.sidebar_width,
                extensions: crate::extensions::ExtensionCache::default(),
            }),
            delegate,
            config,
        };
        // Only the first call counts.
        let _ = ENGINE.set(engine);
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
    pub(crate) fn emit(&self, event: EngineEvent) {
        Self::post(Box::new(move || {
            let engine = Engine::global();
            engine.delegate.on_event(engine, event);
        }));
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Inner> {
        lock(&self.inner)
    }

    /// The frame (main) window, once it exists.
    pub(crate) fn frame_window(&self) -> Option<Window> {
        self.lock().frame.as_ref().map(|f| f.window.clone())
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
        if let Some(frame) = browser.and_then(|b| b.main_frame()) {
            self.eval_in_ui(&frame, js);
        }
    }

    /// Run `js` in every sidebar page, shown or not: state pushes.
    pub fn ui_eval_all(&self, js: &str) {
        for browser in [self.docked_sidebar_browser(), self.float_sidebar_browser()]
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

    fn docked_sidebar_browser(&self) -> Option<Browser> {
        self.lock()
            .frame
            .as_ref()
            .and_then(|f| f.sidebar_browser.clone())
    }

    /// Close every window; the message loop ends once the frame is gone.
    /// Callers save their state first: if the frame is still there after
    /// `QUIT_GRACE_MS` (a page or extension popup can hold its window
    /// open), the process ends without the rest of the shutdown.
    pub fn quit(&self) {
        let contents: Vec<Window> = {
            let mut inner = self.lock();
            inner.quitting = true;
            inner
                .contents
                .values()
                .filter_map(|c| c.window.clone())
                .collect()
        };
        let frame = self.frame_window();
        for w in contents {
            w.close();
        }
        if let Some(w) = frame {
            w.close();
        }
        Self::post_delayed(
            Box::new(|| {
                if Engine::global().lock().frame.is_some() {
                    eprintln!(
                        "magnetite: windows did not close within {QUIT_GRACE_MS} ms; exiting"
                    );
                    // SAFETY: ends the process at once, skipping exit handlers
                    // that would wait on the stuck shutdown.
                    unsafe { libc::_exit(0) };
                }
            }),
            QUIT_GRACE_MS,
        );
    }

    pub(crate) fn is_quitting(&self) -> bool {
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
    /// everything in memory until Magnetite quits. Chromium finishes a new
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
const QUIT_GRACE_MS: i64 = 5000;

fn schedule_tick(delay_ms: i64) {
    Engine::post_delayed(Box::new(tick), delay_ms);
}

/// Poll the signal flag often; emit the housekeeping tick once a minute.
fn tick() {
    let engine = Engine::global();
    if engine.is_quitting() {
        return;
    }
    if TERM_REQUESTED.swap(false, std::sync::atomic::Ordering::SeqCst) {
        engine.emit(EngineEvent::QuitRequested);
        return;
    }
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
    // Password manager and autofill as the user's settings have them.
    let autofill = Engine::global().lock().autofill;
    if !autofill.apply(ctx) {
        crate::trace("cannot set the password and autofill preferences");
    }
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
    }
}
