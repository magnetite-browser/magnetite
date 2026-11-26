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

/// Whether `url`, which the page in `frame` wants in a new tab or window,
/// is a link to another app. No tab is made for those: the page's own
/// document goes there, and Chromium asks whether to open the app.
fn page_asks_for_app(kind: Kind, frame: Option<&Frame>, url: Option<&CefString>) -> bool {
    let (Some(frame), Some(url)) = (frame, url.map(CefString::to_string)) else {
        return false;
    };
    let asks = url_source(kind) == UrlSource::Page && crate::applink::is_app_link(&url);
    if asks {
        crate::applink::open_from(frame, &url);
    }
    asks
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
        fn dialog_handler(&self) -> Option<DialogHandler> {
            Some(crate::downloads::MagnetiteDialogHandler::new())
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
            // Every frame of a page may report what it captures.
            if self.kind == Kind::Content
                && let (Some(frame), Some(msg)) = (frame.as_deref(), message.as_ref())
                && CefString::from(&msg.name()).to_string() == crate::tabshare::MESSAGE
            {
                let text = |index| msg.argument_list().map(|a| CefString::from(&a.string(index)).to_string()).unwrap_or_default();
                Engine::global().on_share_report(browser.as_deref(), frame, &text(0), &text(1));
                return 1;
            }
            // A page's main frame says that its protected playback failed.
            if self.kind == Kind::Content
                && let (Some(frame), Some(msg)) = (frame.as_deref(), message.as_ref())
                && frame.is_main() != 0
                && CefString::from(&msg.name()).to_string() == crate::protected::MESSAGE
            {
                let reported = msg.argument_list().map(|a| CefString::from(&a.string(0)).to_string()).unwrap_or_default();
                Engine::global().on_protected_report(browser.as_deref(), frame, &reported);
                return 1;
            }
            // Magnetite's CEF: an extension pinned or unpinned the page's tab.
            if self.kind == Kind::Content
                && source_process == ProcessId::BROWSER
                && let Some(msg) = message.as_ref()
                && CefString::from(&msg.name()).to_string() == crate::pinning::PINNED_MESSAGE
            {
                let pinned = msg.argument_list().is_some_and(|a| a.bool(0) != 0);
                Engine::global().on_extension_pinned(browser.as_deref(), pinned);
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
            level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            line: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            if self.kind == Kind::Content {
                // Errors only. The app counts them for local pages alone.
                let engine = Engine::global();
                if *level.as_ref() as i32 >= sys::cef_log_severity_t::LOGSEVERITY_ERROR as i32
                    && let Some(id) = engine.content_id_for(browser.as_deref())
                {
                    engine.emit(EngineEvent::ConsoleError { id });
                }
                return 0;
            }
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
        /// Chromium shows a cursor only in the window that has the keyboard;
        /// ours also show it while a page has it (`show_cursor_without_focus`).
        fn on_cursor_change(
            &self,
            browser: Option<&mut Browser>,
            cursor: OsCursor,
            _type: CursorType,
            _custom_cursor_info: Option<&CursorInfo>,
        ) -> ::std::os::raw::c_int {
            #[cfg(target_os = "macos")]
            if let Some(host) = browser.and_then(|b| b.host()) {
                // SAFETY: the view of a browser that is alive in this
                // callback, and the `NSCursor` CEF reports with it; CEF
                // holds both until the callback returns.
                unsafe { crate::mac::show_cursor_without_focus(host.window_handle(), cursor) };
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (browser, cursor);
            0
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

/// First entry of a CEF string list (see `strings`).
pub(crate) fn first_string(list: &mut CefStringList) -> Option<String> {
    strings(list).into_iter().next()
}

/// The entries of a CEF string list, via the C API (the wrapper owns the list).
pub(crate) fn strings(list: &mut CefStringList) -> Vec<String> {
    let raw: *mut sys::_cef_string_list_t = list.into();
    // SAFETY: `raw` is a live list for the duration of the callback.
    unsafe {
        (0..sys::cef_string_list_size(raw))
            .map_while(|index| {
                // SAFETY: an all-zero cef_string_t is the documented empty string.
                let mut value: sys::cef_string_t = std::mem::zeroed();
                if sys::cef_string_list_value(raw, index, &mut value) == 0 {
                    return None;
                }
                let s = if value.str_.is_null() {
                    String::new()
                } else {
                    String::from_utf16_lossy(std::slice::from_raw_parts(value.str_, value.length))
                };
                sys::cef_string_utf16_clear(&mut value);
                Some(s)
            })
            .collect()
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
            frame: Option<&mut Frame>,
            popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            crate::trace(&format!("popup asked: disposition {}", target_disposition.get_raw()));
            // An extension's new tab, not a page's popup: taken as it is, or
            // left to Chromium, whose window then goes the old way
            // (`stray.rs`). Never opened from here as well.
            if popup_id == crate::content::EXTENSION_TAB_POPUP {
                let url = target_url.map(CefString::to_string).unwrap_or_default();
                let background = target_disposition == WindowOpenDisposition::NEW_BACKGROUND_TAB;
                let wishes = crate::content::extension_tab_wishes(extra_info);
                let taken = self.kind == Kind::Content
                    && Engine::global().expect_extension_tab(browser.as_deref(), url, background, wishes);
                return i32::from(!taken);
            }
            // What was expected of an earlier popup that never came.
            Engine::global().forget_pending_tab();
            if page_asks_for_app(self.kind, frame.as_deref(), target_url) {
                return 1;
            }
            // A page's popup window (`window.open` with window features)
            // opens as a real window, as in other browsers: sign-in popups
            // ("Sign in with Google") talk back to their opener and fail
            // when they get no window. `ContentViewDelegate` gives it one.
            if self.kind == Kind::Content && target_disposition == WindowOpenDisposition::NEW_POPUP {
                return 0;
            }
            let url = target_url.map(CefString::to_string).unwrap_or_default();
            // A tab the page opens in front is the page's own popup, shown
            // as a tab: the page gets its window (`expect_page_tab`).
            if self.kind == Kind::Content
                && target_disposition == WindowOpenDisposition::NEW_FOREGROUND_TAB
                && Engine::global().expect_page_tab(browser.as_deref(), &url)
            {
                return 0;
            }
            // Never let Chromium open its own windows: route to the app as a tab.
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

        fn on_before_popup_aborted(&self, _browser: Option<&mut Browser>, _popup_id: ::std::os::raw::c_int) {
            // The popup a tab was expected of (`expect_page_tab`) is not made.
            Engine::global().forget_pending_tab();
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
            if let Some(b) = browser.as_deref() {
                crate::scriptlets::page_closed(b.identifier());
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
                // A click into a page closes an open palette.
                engine.blur_overlay();
                // The window of the page the user turns to is the current one.
                let home = engine.lock().home_of(id);
                engine.enter(home);
                // The outline follows the keyboard at once, before the app hears of it.
                engine.update_focus_ring(Some(id));
                #[cfg(target_os = "linux")]
                engine.keep_web_panel_above(id);
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
            // The page behind the tabs may have loaded after they were
            // placed: it learns where they are.
            // In its own window, current or not.
            if self.kind == Kind::Overlay && frame.is_main() != 0 {
                let engine = Engine::global();
                if let Some(win) = engine.window_of_browser(browser.identifier()) {
                    engine.in_window(win, || engine.update_page_shadows());
                }
            }
            // The user's own CSS and JavaScript for the site.
            if self.kind == Kind::Content && frame.is_main() != 0 {
                let engine = Engine::global();
                engine.put_site_style(frame);
                engine.run_site_script(frame);
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
                let url = CefString::from(&f.url()).to_string();
                Engine::keep_own_page_unzoomed(b, &url);
                engine.note_loaded_url(b.identifier(), url);
            }
            if self.kind != Kind::Content { return; }
            if let (Some(id), Some(f)) = (engine.content_id_for(browser.as_deref()), frame) {
                engine.note_origin(id, &CefString::from(&f.url()).to_string());
                // A new page in the tab: the old one's connection state no
                // longer holds, and nothing of it plays or captures any more.
                if f.is_main() != 0 {
                    engine.emit_site_state(id);
                    engine.on_media_report(browser.as_deref(), "null");
                    engine.end_captures(id);
                    // Before the page shows anything: the user's own CSS.
                    engine.put_site_style(f);
                }
            }
        }
        fn on_load_error(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            _error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            if self.kind != Kind::Content || frame.is_some_and(|f| f.is_main() == 0) { return; }
            let code = *error_code.as_ref() as i32;
            // Given up before anything changed (a download, a load that was
            // stopped): the tab still shows the page it showed.
            if code == sys::cef_errorcode_t::ERR_ABORTED as i32 { return; }
            let engine = Engine::global();
            if let Some(id) = engine.content_id_for(browser.as_deref()) {
                let refused = code == sys::cef_errorcode_t::ERR_CONNECTION_REFUSED as i32;
                let url = failed_url.map(CefString::to_string).unwrap_or_default();
                // The error page stands at the failed address, and CEF
                // reports no address change for it.
                engine.update_mini_address(id, &url);
                engine.emit(EngineEvent::LoadFailed { id, url, refused });
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
            // A page headed for a site of the system's engine changes its
            // engine (`system.rs`) instead of going there. Only GET, as
            // above; a page's popup window and a mini window stay Chromium's.
            if self.kind == Kind::Content && main
                && let Some(r) = request.as_ref()
                && CefString::from(&r.method()).to_string() == "GET"
                && let url = CefString::from(&r.url()).to_string()
                && engine.wants_system(&url)
                && let Some(id) = engine.content_id_for(browser.as_deref())
                && !engine.is_mini(id)
            {
                Engine::post(Box::new(move || Engine::global().change_engine(id, &url)));
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
            is_navigation: ::std::os::raw::c_int,
            _is_download: ::std::os::raw::c_int,
            request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<ResourceRequestHandler> {
            // A link to another app: Chromium asks whether to open it.
            if is_navigation != 0
                && request.as_ref().is_some_and(|r| crate::applink::is_app_link(&CefString::from(&r.url()).to_string()))
            {
                return Some(crate::applink::handler(browser.as_deref(), request_initiator));
            }
            crate::blocker::resource_request_handler(browser.as_deref(), request.as_deref(), request_initiator)
        }

        fn on_open_urlfrom_tab(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            target_url: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            crate::trace(&format!("open from tab: disposition {}", target_disposition.get_raw()));
            if page_asks_for_app(self.kind, frame.as_deref(), target_url) {
                return 1;
            }
            // "Open Link in Incognito Window": Magnetite has no such window,
            // and an ordinary tab in the same profile would be the opposite.
            if target_disposition == WindowOpenDisposition::OFF_THE_RECORD {
                return 1;
            }
            // A new window asked for here is a Shift+click on a link (on
            // macOS that is a popup request instead): it asks for a new
            // window as a page does and so gets a peek. "Open Link in New
            // Window", which looks the same here, never arrives: the menu
            // command opens its mini window itself (`on_context_menu_command`).
            Engine::global().request_open(
                browser.as_deref(),
                target_url.map(CefString::to_string).unwrap_or_default(),
                url_source(self.kind),
                target_disposition == WindowOpenDisposition::NEW_BACKGROUND_TAB,
                target_disposition == WindowOpenDisposition::NEW_WINDOW,
                false,
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
            // What the page played or captured died with its renderer.
            if self.kind == Kind::Content {
                engine.on_media_report(browser.as_deref(), "null");
                if let Some(id) = engine.content_id_for(browser.as_deref()) {
                    engine.end_captures(id);
                }
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
            let shortcut = match keys::shortcut_for_chrome_command(command_id) {
                keys::ChromeCommand::Pass => return 0,
                keys::ChromeCommand::Drop => return 1,
                keys::ChromeCommand::Run(shortcut) => shortcut,
            };
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

/// The cursor CEF hands along; its type differs per OS.
#[cfg(target_os = "macos")]
type OsCursor = *mut u8;
#[cfg(not(target_os = "macos"))]
type OsCursor = CursorHandle;

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
            is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            #[cfg(not(target_os = "macos"))]
            if let Some(event) = event {
                keys::note_modifiers(event);
            }
            let engine = Engine::global();
            // The sidebar's and the top bar's shortcuts run here, not in their pages: the keys are
            // the user's (`keymap.rs`), and a hidden sidebar can still hold
            // focus while Chromium drops input to hidden pages.
            // The floating sidebar is an overlay window; the other overlays
            // (the command bar, the capture tool) keep their keys.
            let floating = self.kind == Kind::Overlay
                && engine.float_sidebar_browser().is_some_and(|float| {
                    browser.as_deref().is_some_and(|b| b.identifier() == float.identifier())
                });
            if matches!(self.kind, Kind::Sidebar | Kind::TopBar) || floating {
                let Some(shortcut) = event.and_then(keys::any_shortcut) else { return 0 };
                engine.emit(EngineEvent::Shortcut { from: None, shortcut });
                return 1;
            }
            if self.kind != Kind::Content { return 0; }
            let Some(event) = event else { return 0 };
            // Most keys are no shortcut: find the page only for one that is.
            let from = || engine.content_id_for(browser.as_deref());
            let site = keys::page_first_site(event, || page_address(browser.as_deref()));
            let (shortcut, from) = if let Some(shortcut) = keys::shortcut_for_key(event, site) {
                (shortcut, from())
            } else {
                // Peek keys count only in the peek; in a mini window `Cmd+O`
                // ("Open in") does too.
                let Some(shortcut) = keys::peek_shortcut(event) else {
                    // The page first, by the user's choice (for the shortcut
                    // or for the site): Chromium would
                    // run its own command for some of these keys without
                    // asking the page. As a shortcut of ours it goes to the
                    // page, and to `on_key_event` if the page leaves it.
                    if keys::yielded_by_choice(event, site) && let Some(ours) = is_keyboard_shortcut {
                        *ours = 1;
                    }
                    return 0;
                };
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
            let Some(event) = event else { return 0 };
            let site = keys::page_first_site(event, || page_address(browser.as_deref()));
            let Some(shortcut) = keys::yieldable_shortcut(event, site) else { return 0 };
            let engine = Engine::global();
            let from = engine.content_id_for(browser.as_deref());
            if from.is_none() { return 0; }
            engine.emit(EngineEvent::Shortcut { from, shortcut });
            1
        }
    }
}

/// The address of the page `browser` shows (its main frame).
fn page_address(browser: Option<&Browser>) -> Option<String> {
    let frame = browser?.main_frame()?;
    Some(CefString::from(&frame.url()).to_string())
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

/// Command id of "Open Link in Peek" in a page's menu: the first id CEF
/// leaves to us.
const OPEN_LINK_IN_PEEK: ::std::os::raw::c_int = sys::cef_menu_id_t::MENU_ID_USER_FIRST as _;

/// Put "Open Link in Peek" below "Open Link in New Window" in the menu `m`
/// of the page `browser`: a Shift+click for the mouse. Chromium has that item
/// only for a link it can open. The peek and a web panel go without, since a
/// Shift+click there opens a tab.
fn add_open_link_in_peek(m: &MenuModel, browser: Option<&Browser>) {
    let Some(new_window) = keys::command_id_for("IDC_CONTENT_CONTEXT_OPENLINKNEWWINDOW") else {
        return;
    };
    let Ok(at) = usize::try_from(m.index_of(new_window)) else {
        return;
    };
    let engine = Engine::global();
    let page = engine.content_id_for(browser);
    if page.is_none() || page == engine.peek_content() || page == engine.web_panel_content() {
        return;
    }
    m.insert_item_at(
        at + 1,
        OPEN_LINK_IN_PEEK,
        Some(&CefString::from(magnetite_ui::t!("app.peek.open_link"))),
    );
}

/// The selected text `params` were made for.
fn selection_of(params: Option<&ContextMenuParams>) -> String {
    params
        .map(|p| CefString::from(&p.selection_text()).to_string())
        .unwrap_or_default()
}

/// Name the search engine of Settings → Search in Chromium's "Search Google
/// for “…”" of the menu `m`; `on_context_menu_command` then searches with
/// it (`web_search.rs`).
fn name_search_engine(m: &MenuModel, params: Option<&ContextMenuParams>) {
    let Some(search) = keys::command_id_for("IDC_CONTENT_CONTEXT_SEARCHWEBFOR") else {
        return;
    };
    if m.index_of(search) < 0 {
        return;
    }
    if let Some(label) = crate::web_search::menu_label(&selection_of(params)) {
        m.set_label(search, Some(&CefString::from(label.as_str())));
    }
}

wrap_context_menu_handler! {
    struct MagnetiteContextMenuHandler {
        kind: Kind,
    }
    impl ContextMenuHandler {
        /// Our UI pages draw their own menus, also the ones shown as a tab
        /// (Settings, Downloads): CEF's menu there keeps only its text editing
        /// items (`keep_text_editing`). Pages lose "Open Link in Incognito
        /// Window", which opens nothing here (`on_open_urlfrom_tab`), and gain
        /// "Open Link in Peek". Their search item, and that of a page's popup
        /// window, names the search engine of Settings.
        fn on_before_context_menu(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            let Some(m) = model else { return };
            let page_url = params.as_deref().map(|p| CefString::from(&p.page_url()).to_string()).unwrap_or_default();
            if matches!(self.kind, Kind::Sidebar | Kind::TopBar | Kind::Overlay) {
                // These pages are Alloy style: CEF lists the standard items
                // with its own ids, not Chromium's `IDC_*`, Undo to Select All
                // as one block.
                use sys::cef_menu_id_t::{MENU_ID_SELECT_ALL, MENU_ID_UNDO};
                let editing = MENU_ID_UNDO as i32..=MENU_ID_SELECT_ALL as i32;
                keep_text_editing(m, |id| editing.contains(&id));
            } else if self.kind == Kind::Content && crate::router::is_ui_url(&page_url) {
                // A tab is Chrome style: the items carry Chromium's ids.
                let editing: Vec<i32> = CHROME_TEXT_EDITING.iter().filter_map(|name| keys::command_id_for(name)).collect();
                keep_text_editing(m, |id| editing.contains(&id));
            } else if self.kind == Kind::Content {
                if let Some(incognito) = keys::command_id_for("IDC_CONTENT_CONTEXT_OPENLINKOFFTHERECORD") {
                    m.remove(incognito);
                }
                add_open_link_in_peek(m, browser.as_deref());
            }
            if matches!(self.kind, Kind::Content | Kind::Stray) {
                name_search_engine(m, params.as_deref());
            }
        }
        /// "Inspect" in a page's menu opens `DevTools` through us, at the
        /// clicked element, so they dock and get our menu items. "Open Link
        /// in New Window" opens a mini window through us: Chromium would ask
        /// for it just as for a Shift+click (`on_open_urlfrom_tab`). "Open
        /// Link in Peek" asks for what a Shift+click asks for. "Search … for"
        /// asks for a new tab that searches with the engine of Settings,
        /// where Chromium would search Google.
        fn on_context_menu_command(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            command_id: ::std::os::raw::c_int,
            _event_flags: EventFlags,
        ) -> ::std::os::raw::c_int {
            if self.kind == Kind::Content && command_id == OPEN_LINK_IN_PEEK {
                let Some(url) = params.map(|p| CefString::from(&p.link_url()).to_string()) else { return 0 };
                Engine::global().request_open(browser.as_deref(), url, UrlSource::Page, false, true, false);
                return 1;
            }
            if matches!(self.kind, Kind::Content | Kind::Stray)
                && Some(command_id) == keys::command_id_for("IDC_CONTENT_CONTEXT_SEARCHWEBFOR")
            {
                let Some(url) = crate::web_search::menu_address(&selection_of(params.as_deref())) else { return 0 };
                Engine::global().request_open(browser.as_deref(), url, url_source(self.kind), false, false, false);
                return 1;
            }
            // A page's popup window (`Kind::Stray`) has the item too.
            if matches!(self.kind, Kind::Content | Kind::Stray)
                && Some(command_id) == keys::command_id_for("IDC_CONTENT_CONTEXT_OPENLINKNEWWINDOW")
            {
                let Some(url) = params.map(|p| CefString::from(&p.link_url()).to_string()) else { return 0 };
                Engine::global().request_open(browser.as_deref(), url, url_source(self.kind), false, false, true);
                return 1;
            }
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

/// Chromium's names for the text editing items of a Chrome style page's menu.
const CHROME_TEXT_EDITING: [&str; 8] = [
    "IDC_CONTENT_CONTEXT_UNDO",
    "IDC_CONTENT_CONTEXT_REDO",
    "IDC_CONTENT_CONTEXT_CUT",
    "IDC_CONTENT_CONTEXT_COPY",
    "IDC_CONTENT_CONTEXT_PASTE",
    "IDC_CONTENT_CONTEXT_PASTE_AND_MATCH_STYLE",
    "IDC_CONTENT_CONTEXT_DELETE",
    "IDC_CONTENT_CONTEXT_SELECTALL",
];

/// Trims the menu of one of our UI pages to the items for which `editing`
/// holds: a text field there gets Cut, Copy and Paste, a selection Copy.
/// Anywhere else nothing is left, and no menu shows at all.
fn keep_text_editing(m: &MenuModel, editing: impl Fn(i32) -> bool) {
    let separator = |i| m.type_at(i) == MenuItemType::SEPARATOR;
    for i in (0..m.count()).rev() {
        if !separator(i) && !editing(m.command_id_at(i)) {
            m.remove_at(i);
        }
    }
    // A separator stays only between two groups of items.
    for i in (0..m.count()).rev() {
        if separator(i) && (i + 1 == m.count() || separator(i + 1)) {
            m.remove_at(i);
        }
    }
    if m.count() > 0 && separator(0) {
        m.remove_at(0);
    }
}

wrap_drag_handler! {
    struct MagnetiteDragHandler {
        kind: Kind,
    }
    impl DragHandler {
        /// `-webkit-app-region: drag` areas of the sidebar and the top bar
        /// page become their frame window's draggable regions.
        fn on_draggable_regions_changed(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            regions: Option<&[DraggableRegion]>,
        ) {
            if frame.is_some_and(|f| f.is_main() == 0) || !matches!(self.kind, Kind::Sidebar | Kind::TopBar) {
                return;
            }
            let regions = regions.map(<[DraggableRegion]>::to_vec).unwrap_or_default();
            let engine = Engine::global();
            // The developer bar drags nothing: it is no part of the frame's
            // draggable area (`apply_drag_regions`).
            let id = browser.as_ref().map(|b| b.identifier());
            if id.is_some() && engine.dev_bar_browser().map(|b| b.identifier()) == id {
                return;
            }
            // The window whose sidebar or top bar this is, current or not.
            let win = browser
                .and_then(|b| engine.window_of_browser(b.identifier()))
                .unwrap_or_else(|| engine.current_window());
            engine.in_window(win, || {
                if self.kind == Kind::Sidebar {
                    engine.lock().win.sidebar_drag = regions;
                } else {
                    engine.lock().win.top_bar_drag = regions;
                }
                engine.apply_drag_regions();
            });
        }
    }
}
