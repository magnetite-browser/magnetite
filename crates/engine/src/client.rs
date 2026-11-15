//! The `CefClient` shared by sidebar and content browsers. Handlers translate
//! CEF callbacks into [`EngineEvent`]s and feed the message router.

use cef::sys;
use cef::wrapper::message_router::MessageRouterBrowserSideHandlerCallbacks;
use cef::*;

use crate::engine::Engine;
use crate::events::{EngineEvent, Shortcut, UrlSource};
use crate::keys;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Sidebar,
    /// The top bar above the sidebar and the pages (`frame.rs`).
    TopBar,
    Content,
    /// A `magnetite://ui` page in an overlay window above a page (capture tool).
    Overlay,
    /// `DevTools` opened by us for a page (`content_show_devtools`).
    DevTools,
    /// A window Chromium opened by itself, such as an extension's new tab:
    /// becomes one of our tabs (`Engine::adopt_stray`).
    Stray,
}

/// Our own UI asks with the app's trust; everything else is a page.
fn url_source(kind: Kind) -> UrlSource {
    if matches!(kind, Kind::Sidebar | Kind::TopBar | Kind::Overlay) {
        UrlSource::Ui
    } else {
        UrlSource::Page
    }
}

wrap_client! {
    pub struct MagnetiteClient {
        kind: Kind,
    }

    impl Client {
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(MagnetiteDisplayHandler::new(self.kind))
        }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(MagnetiteLifeSpanHandler::new(self.kind))
        }
        fn load_handler(&self) -> Option<LoadHandler> {
            Some(MagnetiteLoadHandler::new(self.kind))
        }
        fn request_handler(&self) -> Option<RequestHandler> {
            Some(MagnetiteRequestHandler::new(self.kind))
        }
        fn command_handler(&self) -> Option<CommandHandler> {
            Some(MagnetiteCommandHandler::new(self.kind))
        }
        fn focus_handler(&self) -> Option<FocusHandler> {
            matches!(self.kind, Kind::Content | Kind::DevTools).then(MagnetiteFocusHandler::new)
        }
        fn permission_handler(&self) -> Option<PermissionHandler> {
            (self.kind == Kind::Content).then(MagnetitePermissionHandler::new)
        }
        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(MagnetiteKeyboardHandler::new(self.kind))
        }
        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            Some(MagnetiteContextMenuHandler::new(self.kind))
        }
        fn download_handler(&self) -> Option<DownloadHandler> {
            (self.kind == Kind::Content).then(crate::downloads::MagnetiteDownloadHandler::new)
        }
        fn drag_handler(&self) -> Option<DragHandler> {
            Some(MagnetiteDragHandler::new(self.kind))
        }
        fn on_process_message_received(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> ::std::os::raw::c_int {
            if self.kind == Kind::Content
                && frame.as_ref().is_some_and(|f| f.is_main() != 0)
                && let Some(msg) = message.as_ref()
                && CefString::from(&msg.name()).to_string() == crate::media::REPORT_MESSAGE
            {
                let json = msg.argument_list().map(|a| CefString::from(&a.string(0)).to_string()).unwrap_or_default();
                Engine::global().on_media_report(browser.as_deref(), &json);
                return 1;
            }
            let router = Engine::global().lock().router.clone();
            match router {
                Some(r) => r.on_process_message_received(browser.cloned(), frame.cloned(), source_process, message.cloned()).into(),
                None => 0,
            }
        }
    }
}

impl MagnetiteClient {
    pub fn sidebar() -> Client {
        MagnetiteClient::new(Kind::Sidebar)
    }
    pub fn top_bar() -> Client {
        MagnetiteClient::new(Kind::TopBar)
    }
    pub fn content() -> Client {
        MagnetiteClient::new(Kind::Content)
    }
    pub fn overlay() -> Client {
        MagnetiteClient::new(Kind::Overlay)
    }
    pub fn devtools() -> Client {
        MagnetiteClient::new(Kind::DevTools)
    }
    pub fn stray() -> Client {
        MagnetiteClient::new(Kind::Stray)
    }
}

wrap_display_handler! {
    struct MagnetiteDisplayHandler {
        kind: Kind,
    }
    impl DisplayHandler {
        fn on_media_access_change(&self, browser: Option<&mut Browser>, has_video_access: ::std::os::raw::c_int, has_audio_access: ::std::os::raw::c_int) {
            if self.kind != Kind::Content { return; }
            let engine = Engine::global();
            if let Some(id) = engine.content_id_for(browser.as_deref()) {
                engine.emit(EngineEvent::MediaAccessChanged { id, camera: has_video_access != 0, microphone: has_audio_access != 0 });
            }
        }
        fn on_title_change(&self, browser: Option<&mut Browser>, title: Option<&CefString>) {
            if self.kind != Kind::Content { return; }
            let engine = Engine::global();
            if let Some(id) = engine.content_id_for(browser.as_deref()) {
                let title = title.map(CefString::to_string).unwrap_or_default();
                engine.update_mini_title(id, &title);
                engine.emit(EngineEvent::TitleChanged { id, title });
            }
        }
        fn on_address_change(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, url: Option<&CefString>) {
            if self.kind != Kind::Content { return; }
            if frame.is_some_and(|f| f.is_main() == 0) { return; }
            let engine = Engine::global();
            if let Some(id) = engine.content_id_for(browser.as_deref()) {
                let url = url.map(CefString::to_string).unwrap_or_default();
                engine.update_mini_address(id, &url);
                engine.emit(EngineEvent::UrlChanged { id, url });
            }
        }
        fn on_console_message(
            &self,
            browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            _source: Option<&CefString>,
            _line: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            if self.kind != Kind::DevTools { return 0; }
            let text = message.map(CefString::to_string).unwrap_or_default();
            Engine::global().devtools_menu_message(browser.as_deref(), &text).into()
        }
        fn on_status_message(&self, browser: Option<&mut Browser>, value: Option<&CefString>) {
            if self.kind != Kind::Content { return; }
            let engine = Engine::global();
            if let Some(id) = engine.content_id_for(browser.as_deref()) {
                engine.show_status(id, &value.map(CefString::to_string).unwrap_or_default());
            }
        }
        fn on_favicon_urlchange(&self, browser: Option<&mut Browser>, icon_urls: Option<&mut CefStringList>) {
            if self.kind != Kind::Content { return; }
            let engine = Engine::global();
            if let Some(id) = engine.content_id_for(browser.as_deref()) {
                let url = icon_urls.and_then(first_string);
                if let Some(u) = &url {
                    engine.cache_favicon(id, u);
                }
                // The sidebar loads icons in the default context: for a page
                // in another profile that would send the default profile's
                // cookies to its site. Those show the cached copy instead.
                let url = url.filter(|_| engine.content_context(id).is_some_and(|c| c.is_empty()));
                engine.emit(EngineEvent::FaviconChanged { id, url });
            }
        }
    }
}

/// First entry of a CEF string list, via the C API (the wrapper owns the list).
fn first_string(list: &mut CefStringList) -> Option<String> {
    let raw: *mut sys::_cef_string_list_t = list.into();
    // SAFETY: `raw` is a live list for the duration of the callback.
    unsafe {
        if sys::cef_string_list_size(raw) == 0 {
            return None;
        }
        // SAFETY: an all-zero cef_string_t is the documented empty string.
        let mut value: sys::cef_string_t = std::mem::zeroed();
        if sys::cef_string_list_value(raw, 0, &mut value) == 0 {
            return None;
        }
        let s = if value.str_.is_null() {
            String::new()
        } else {
            String::from_utf16_lossy(std::slice::from_raw_parts(value.str_, value.length))
        };
        sys::cef_string_utf16_clear(&mut value);
        Some(s)
    }
}

wrap_life_span_handler! {
    struct MagnetiteLifeSpanHandler {
        kind: Kind,
    }
    impl LifeSpanHandler {
        fn on_before_popup(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            // A page's popup window (`window.open` with window features)
            // opens as a real window, as in other browsers: sign-in popups
            // ("Sign in with Google") talk back to their opener and fail
            // when they get no window. `ContentViewDelegate` gives it one.
            if self.kind == Kind::Content && target_disposition == WindowOpenDisposition::NEW_POPUP {
                return 0;
            }
            // Never let Chromium open its own windows: route to the app as a tab.
            let url = target_url.map(CefString::to_string).unwrap_or_default();
            if !url.is_empty() {
                Engine::global().request_open(
                    browser.as_deref(),
                    url,
                    url_source(self.kind),
                    target_disposition == WindowOpenDisposition::NEW_BACKGROUND_TAB,
                    target_disposition == WindowOpenDisposition::NEW_WINDOW || target_disposition == WindowOpenDisposition::NEW_POPUP,
                    false,
                );
            }
            1
        }

        fn on_after_created(&self, browser: Option<&mut Browser>) {
            // The default profile, through its first browser: a fresh handle
            // from `request_context_get_global_context` is not ready yet.
            static PREPARED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !PREPARED.load(std::sync::atomic::Ordering::Relaxed)
                && let Some(ctx) = browser.as_deref().and_then(ImplBrowser::host).and_then(|h| h.request_context())
                && ctx.is_global() != 0
                && crate::engine::prepare_context(&ctx)
            {
                PREPARED.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            Engine::global().on_browser_created(self.kind, browser);
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            let engine = Engine::global();
            if let Some(r) = engine.lock().router.clone() {
                r.on_before_close(browser.as_deref().cloned());
            }
            engine.on_browser_closed(self.kind, browser.as_deref());
        }
    }
}

// A page got keyboard focus (clicked into, tabbed into): in split view that
// page becomes the active tab. Docked DevTools count as their page, so keys
// typed there act on it and not on the pane focused before.
wrap_focus_handler! {
    struct MagnetiteFocusHandler {}
    impl FocusHandler {
        fn on_got_focus(&self, browser: Option<&mut Browser>) {
            let engine = Engine::global();
            let page = engine
                .content_id_for(browser.as_deref())
                .or_else(|| engine.devtools_owner(browser.as_deref()));
            if let Some(id) = page {
                // The outline follows the keyboard at once, before the app hears of it.
                engine.update_focus_ring(Some(id));
                engine.emit(EngineEvent::ContentFocused { id });
            }
        }
    }
}

// Records permission requests for the site panel. Prompts go to the app
// (`prompts.rs`); camera and microphone requests reach them through
// Chromium's default handling (we return 0 there).
wrap_permission_handler! {
    struct MagnetitePermissionHandler {}
    impl PermissionHandler {
        fn on_request_media_access_permission(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            _callback: Option<&mut MediaAccessCallback>,
        ) -> ::std::os::raw::c_int {
            let engine = Engine::global();
            if let (Some(id), Some(origin)) = (engine.content_id_for(browser.as_deref()), requesting_origin) {
                engine.note_permission_prompt(id, &origin.to_string(), requested_permissions, true);
            }
            0
        }
        fn on_show_permission_prompt(
            &self,
            browser: Option<&mut Browser>,
            prompt_id: u64,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> ::std::os::raw::c_int {
            let engine = Engine::global();
            // Not a page (an extension's own window): Chromium's bubble.
            let (Some(id), Some(origin)) =
                (engine.content_id_for(browser.as_deref()), requesting_origin.map(ToString::to_string))
            else {
                return 0;
            };
            engine.note_permission_prompt(id, &origin, requested_permissions, false);
            // The notification panel belongs to the frame: a mini window's
            // page asks with Chromium's own bubble.
            if engine.is_mini(id) {
                return 0;
            }
            engine
                .take_permission_prompt(id, prompt_id, &origin, requested_permissions, callback)
                .into()
        }
        fn on_dismiss_permission_prompt(
            &self,
            _browser: Option<&mut Browser>,
            prompt_id: u64,
            _result: PermissionRequestResult,
        ) {
            Engine::global().permission_prompt_closed(prompt_id);
        }
    }
}

wrap_load_handler! {
    struct MagnetiteLoadHandler {
        kind: Kind,
    }
    impl LoadHandler {
        fn on_load_end(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, _http_status_code: ::std::os::raw::c_int) {
            let (Some(browser), Some(frame)) = (browser, frame) else { return };
            if self.kind == Kind::DevTools && frame.is_main() != 0 {
                Engine::global().devtools_loaded(browser);
            }
        }
        fn on_load_start(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, _transition_type: TransitionType) {
            let engine = Engine::global();
            // Every kind of window: the bridge names a page by the address
            // it was loaded from. CEF does not call this for changes inside
            // a document (`history.replaceState`, fragments).
            if let (Some(b), Some(f)) = (browser.as_deref(), frame.as_deref())
                && f.is_main() != 0
            {
                engine.note_loaded_url(b.identifier(), CefString::from(&f.url()).to_string());
            }
            if self.kind != Kind::Content { return; }
            if let (Some(id), Some(f)) = (engine.content_id_for(browser.as_deref()), frame) {
                engine.note_origin(id, &CefString::from(&f.url()).to_string());
                // A new page in the tab: the old one's connection state no
                // longer holds, and nothing of it plays any more.
                if f.is_main() != 0 {
                    engine.emit_site_state(id);
                    engine.on_media_report(browser.as_deref(), "null");
                }
            }
        }
        fn on_loading_state_change(
            &self,
            browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            can_go_back: ::std::os::raw::c_int,
            can_go_forward: ::std::os::raw::c_int,
        ) {
            let engine = Engine::global();
            match self.kind {
                Kind::Content => {
                    if let Some(id) = engine.content_id_for(browser.as_deref()) {
                        engine.update_mini_nav(id, can_go_back != 0, can_go_forward != 0);
                        engine.emit(EngineEvent::LoadingChanged {
                            id,
                            loading: is_loading != 0,
                            can_go_back: can_go_back != 0,
                            can_go_forward: can_go_forward != 0,
                        });
                        if is_loading == 0 {
                            engine.emit_site_state(id);
                        }
                    }
                }
                Kind::Sidebar => {
                    if is_loading == 0 {
                        engine.emit(EngineEvent::SidebarLoaded);
                    }
                }
                Kind::TopBar | Kind::Overlay | Kind::DevTools | Kind::Stray => {}
            }
        }
    }
}

wrap_request_handler! {
    struct MagnetiteRequestHandler {
        kind: Kind,
    }
    impl RequestHandler {
        fn on_before_browse(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            user_gesture: ::std::os::raw::c_int,
            is_redirect: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let engine = Engine::global();
            let main = frame.as_ref().is_some_and(|f| f.is_main() != 0);
            // The sidebar, the floating sidebar and overlays show our UI only:
            // a link dropped on them must not turn them into a web page that
            // then receives the app's state.
            if matches!(self.kind, Kind::Sidebar | Kind::TopBar | Kind::Overlay) && main {
                let url = request.as_ref().map(|r| CefString::from(&r.url()).to_string()).unwrap_or_default();
                if !engine.is_own_ui(&url) {
                    return 1;
                }
            }
            if self.kind == Kind::Stray && main {
                let url = request.as_ref().map(|r| CefString::from(&r.url()).to_string()).unwrap_or_default();
                return engine.adopt_stray(browser, &url).into();
            }
            // Domain lock: a user click that leaves a pinned tab's host opens
            // elsewhere (the application decides where) instead of navigating.
            // Only GET: reopening a form's POST elsewhere would lose its body.
            if self.kind == Kind::Content && user_gesture != 0 && is_redirect == 0 && main
                && request.as_ref().is_some_and(|r| CefString::from(&r.method()).to_string() == "GET")
                && let Some(id) = engine.content_id_for(browser.as_deref())
                && let Some(lock) = engine.content_lock(id)
                && let Some(url) = request.as_ref().map(|r| CefString::from(&r.url()).to_string())
                && let Some(host) = host_of(&url)
                && !same_site(&host, &lock)
            {
                let context = engine.content_context(id);
                engine.emit(EngineEvent::OpenUrlRequested { from: Some(id), url, background: false, new_window: false, own_window: false, locked: true, source: UrlSource::Page, context });
                return 1;
            }
            if let Some(r) = engine.lock().router.clone() {
                r.on_before_browse(browser.cloned(), frame.cloned());
            }
            0
        }

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

        fn on_open_urlfrom_tab(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            target_url: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            // "Open Link in Incognito Window": Magnetite has no such window,
            // and an ordinary tab in the same profile would be the opposite.
            if target_disposition == WindowOpenDisposition::OFF_THE_RECORD {
                return 1;
            }
            Engine::global().request_open(
                browser.as_deref(),
                target_url.map(CefString::to_string).unwrap_or_default(),
                url_source(self.kind),
                target_disposition == WindowOpenDisposition::NEW_BACKGROUND_TAB,
                false,
                target_disposition == WindowOpenDisposition::NEW_WINDOW,
            );
            1
        }

        fn on_render_process_terminated(
            &self,
            browser: Option<&mut Browser>,
            _status: TerminationStatus,
            _error_code: ::std::os::raw::c_int,
            _error_string: Option<&CefString>,
        ) {
            let engine = Engine::global();
            // What the page played died with its renderer.
            if self.kind == Kind::Content {
                engine.on_media_report(browser.as_deref(), "null");
            }
            if let Some(r) = engine.lock().router.clone() {
                r.on_render_process_terminated(browser.as_deref().cloned());
            }
            // Our own pages have no "this page crashed" view to reload from:
            // without this the sidebar stays blank until the app restarts.
            if matches!(self.kind, Kind::Sidebar | Kind::TopBar | Kind::Overlay)
                && let Some(browser) = browser.as_deref().cloned()
                && engine.may_reload_ui(browser.identifier())
            {
                Engine::post_delayed(Box::new(move || browser.reload()), 500);
            }
        }
    }
}

wrap_command_handler! {
    struct MagnetiteCommandHandler {
        kind: Kind,
    }
    impl CommandHandler {
        fn on_chrome_command(
            &self,
            browser: Option<&mut Browser>,
            command_id: ::std::os::raw::c_int,
            _disposition: WindowOpenDisposition,
        ) -> ::std::os::raw::c_int {
            let engine = Engine::global();
            if keys::is_zoom_command(command_id) {
                // Chromium zooms; report the new level once it applied.
                if let Some(id) = engine.content_id_for(browser.as_deref()) {
                    engine.emit_site_state_soon(id);
                }
                return 0;
            }
            // A window Chromium opened itself (the DevTools of an extension
            // popup): its keys are its own, not commands for our tabs.
            if self.kind == Kind::Stray { return 0; }
            let Some(shortcut) = keys::shortcut_for_chrome_command(command_id) else { return 0 };
            // Keys in DevTools act on their page.
            let owner = engine.devtools_owner(browser.as_deref());
            let from = engine.content_id_for(browser.as_deref()).or(owner);
            // A page's popup window is no tab of ours: Cmd+W closes it.
            if self.kind == Kind::Content && from.is_none() { return 0; }
            engine.emit(EngineEvent::Shortcut { from, shortcut });
            1
        }
    }
}

/// The native key event CEF hands along (unused); its type differs per OS.
#[cfg(target_os = "macos")]
type OsKeyEvent<'a> = *mut u8;
#[cfg(target_os = "linux")]
type OsKeyEvent<'a> = Option<&'a mut sys::XEvent>;
#[cfg(target_os = "windows")]
type OsKeyEvent<'a> = Option<&'a mut sys::MSG>;

wrap_keyboard_handler! {
    struct MagnetiteKeyboardHandler {
        kind: Kind,
    }
    impl KeyboardHandler {
        /// Runs before the renderer sees the key, in every browser. Our
        /// shortcuts all carry Cmd/Ctrl, so pages never lose plain typing.
        fn on_pre_key_event(
            &self,
            browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: OsKeyEvent<'_>,
            _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let engine = Engine::global();
            // A hidden sidebar can still hold focus, and Chromium drops input
            // to hidden pages: its shortcuts (Cmd+S to show it again) run here.
            if self.kind == Kind::Sidebar && !engine.sidebar_visible() {
                let Some(shortcut) = event.and_then(|e| keys::shortcut_for_key(e).or_else(|| keys::yieldable_shortcut(e))) else { return 0 };
                engine.emit(EngineEvent::Shortcut { from: None, shortcut });
                return 1;
            }
            if self.kind != Kind::Content { return 0; }
            let Some(event) = event else { return 0 };
            // Most keys are no shortcut: find the page only for one that is.
            let from = || engine.content_id_for(browser.as_deref());
            let (shortcut, from) = if let Some(shortcut) = keys::shortcut_for_key(event) {
                (shortcut, from())
            } else {
                // Peek keys count only in the peek; in a mini window `Cmd+O`
                // ("Open in") does too.
                let Some(shortcut) = keys::peek_shortcut(event) else { return 0 };
                let from = from();
                let peek = from.is_some() && from == engine.peek_content();
                let mini = shortcut == Shortcut::PeekToTab && from.is_some_and(|id| engine.is_mini(id));
                if !peek && !mini { return 0; }
                (shortcut, from)
            };
            // A page's popup window is no tab of ours: its keys are its own.
            if from.is_none() { return 0; }
            engine.emit(EngineEvent::Shortcut { from, shortcut });
            1
        }
        /// Runs after the page had the key and did not handle it: Magnetite
        /// shortcuts that web apps may want for themselves.
        fn on_key_event(
            &self,
            browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: OsKeyEvent<'_>,
        ) -> ::std::os::raw::c_int {
            if self.kind != Kind::Content { return 0; }
            let Some(shortcut) = event.and_then(keys::yieldable_shortcut) else { return 0 };
            let engine = Engine::global();
            let from = engine.content_id_for(browser.as_deref());
            if from.is_none() { return 0; }
            engine.emit(EngineEvent::Shortcut { from, shortcut });
            1
        }
    }
}

/// Host part of an http(s) URL, lower-cased, without a leading `www.`.
pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    if !url.starts_with("http") {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?.rsplit('@').next()?;
    // An IPv6 address is in brackets and has colons of its own.
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next()?,
        None => authority.split(':').next()?,
    };
    Some(host.trim_start_matches("www.").to_ascii_lowercase())
}

impl Engine {
    /// Hand the app an address a page (`browser`) wants opened in a new tab
    /// or window: from that page and in its profile, or, for `DevTools`, in
    /// the profile of the page they inspect. A page locked to its site
    /// (pinned tab, favorite) says so, so the app can peek instead.
    fn request_open(
        &self,
        browser: Option<&Browser>,
        url: String,
        source: UrlSource,
        background: bool,
        new_window: bool,
        own_window: bool,
    ) {
        let owner = self.devtools_owner(browser);
        let from = self.content_id_for(browser);
        let locked = from.is_some_and(|id| self.content_lock(id).is_some());
        let context = match from.or(owner) {
            Some(id) => self.content_context(id),
            // Our own pages: the app picks the profile.
            None if source == UrlSource::Ui => None,
            // A page that is none of ours: a page's popup window (a link in
            // a sign-in popup). Its own profile, found from its browser;
            // another profile would get its address and lose its session,
            // so a profile we do not know opens nothing.
            None => {
                let context = browser
                    .and_then(ImplBrowser::host)
                    .and_then(|h| h.request_context())
                    .and_then(|c| self.context_id_of(&c));
                if context.is_none() {
                    crate::trace("link from a window of an unknown profile: not opened");
                    return;
                }
                context
            }
        };
        self.emit(EngineEvent::OpenUrlRequested {
            from,
            url,
            background,
            new_window,
            own_window,
            locked,
            source,
            context,
        });
    }
}

/// Same registrable site (Public Suffix List, `sitedata::registrable_domain`).
pub(crate) fn same_site(a: &str, b: &str) -> bool {
    crate::sitedata::registrable_domain(a) == crate::sitedata::registrable_domain(b)
}

wrap_context_menu_handler! {
    struct MagnetiteContextMenuHandler {
        kind: Kind,
    }
    impl ContextMenuHandler {
        /// The sidebar page draws its own menus; never show CEF's default one
        /// there. Pages lose "Open Link in Incognito Window", which opens
        /// nothing here (`on_open_urlfrom_tab`).
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            let Some(m) = model else { return };
            if matches!(self.kind, Kind::Sidebar | Kind::TopBar | Kind::Overlay) {
                m.clear();
            } else if self.kind == Kind::Content
                && let Some(incognito) = keys::command_id_for("IDC_CONTENT_CONTEXT_OPENLINKOFFTHERECORD")
            {
                m.remove(incognito);
            }
        }
        /// "Inspect" in a page's menu opens `DevTools` through us, at the
        /// clicked element, so they dock and get our menu items.
        fn on_context_menu_command(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            command_id: ::std::os::raw::c_int,
            _event_flags: EventFlags,
        ) -> ::std::os::raw::c_int {
            if self.kind != Kind::Content || Some(command_id) != keys::command_id_for("IDC_CONTENT_CONTEXT_INSPECTELEMENT") {
                return 0;
            }
            let engine = Engine::global();
            let Some(id) = engine.content_id_for(browser.as_deref()) else { return 0 };
            let at = params.map(|p| Point { x: p.xcoord(), y: p.ycoord() });
            engine.content_inspect(id, at.as_ref());
            1
        }
    }
}

wrap_drag_handler! {
    struct MagnetiteDragHandler {
        kind: Kind,
    }
    impl DragHandler {
        /// `-webkit-app-region: drag` areas of the sidebar and the top bar
        /// page become the frame window's draggable regions.
        fn on_draggable_regions_changed(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            regions: Option<&[DraggableRegion]>,
        ) {
            if frame.is_some_and(|f| f.is_main() == 0) {
                return;
            }
            let regions = regions.map(<[DraggableRegion]>::to_vec).unwrap_or_default();
            let engine = Engine::global();
            match self.kind {
                Kind::Sidebar => engine.lock().sidebar_drag = regions,
                Kind::TopBar => engine.lock().top_bar_drag = regions,
                _ => return,
            }
            engine.apply_drag_regions();
        }
    }
}
