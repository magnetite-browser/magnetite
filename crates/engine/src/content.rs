//! Content windows: one frameless child window per loaded tab, and the
//! top-level windows of mini windows (`mini.rs`).

use cef::*;

use magnetite_core::split::{Bounds, SplitNode};

use crate::client::{Kind, MagnetiteClient};
use crate::engine::Engine;
use crate::events::{ContentId, EngineEvent};

pub(crate) struct ContentState {
    pub window: Option<Window>,
    pub view: BrowserView,
    pub browser: Option<Browser>,
    /// Profile id of the page's context (`""` for the default context).
    pub profile: String,
    /// The context lives in memory only (a private profile): nothing about
    /// the page may reach the disk.
    pub ephemeral: bool,
    /// Keeps the `DevTools` observer of this page alive (see `devtools.rs`).
    pub devtools: Option<Registration>,
    /// The area the page fills when alone; docked DevTools take part of it.
    pub slot: Option<Rect>,
    /// `DevTools` id of the tab holding the page (`extensions.rs`).
    pub tab_target: Option<String>,
    /// The page is in a mini window: a window of its own with this row.
    pub mini: Option<crate::mini::MiniChrome>,
    /// The page began in a mini window and is a tab's now (`mini_to_tab`).
    pub adopted: bool,
}

/// A page asked for while its profile's context was still being made
/// (`create_content`): no view and no window yet.
pub(crate) struct QueuedContent {
    pub id: ContentId,
    pub url: String,
    pub profile: crate::ContextProfile,
    /// For a mini window.
    pub mini: bool,
}

/// Space between split pages (and around docked `DevTools`).
pub const SPLIT_GAP: i32 = 8;

pub(crate) fn bounds_of(r: &Rect) -> Bounds {
    Bounds {
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
    }
}

pub(crate) fn rect_of(b: Bounds) -> Rect {
    Rect {
        x: b.x,
        y: b.y,
        width: b.width,
        height: b.height,
    }
}

impl Engine {
    /// Create a hidden content window loading `url` in `profile`'s context.
    /// Returns its id; the browser itself reports `ContentCreated` once it
    /// exists. While Chromium is still making the profile's context (the
    /// first page of a profile each run), the id has no window yet: it gets
    /// one as soon as the context is ready (`context_ready`).
    pub fn create_content(&self, url: &str, profile: Option<&crate::ContextProfile>) -> ContentId {
        self.create_page(url, profile, false)
    }

    /// `create_content`, or a mini window (`mini`) that shows itself once
    /// it exists.
    pub(crate) fn create_page(
        &self,
        url: &str,
        profile: Option<&crate::ContextProfile>,
        mini: bool,
    ) -> ContentId {
        let id = {
            let mut inner = self.lock();
            let id = inner.next_id;
            inner.next_id += 1;
            id
        };
        crate::trace(&format!("create_content #{id}"));
        let context = self.request_context(profile);
        let waiting = profile.filter(|_| !self.context_is_ready(profile));
        if let Some(profile) = waiting {
            self.lock().queued.push(QueuedContent {
                id,
                url: url.to_string(),
                profile: profile.clone(),
                mini,
            });
        } else {
            self.build_content(id, url, profile, context, mini);
        }
        id
    }

    /// Chromium finished making profile `profile`'s context: the pages that
    /// waited for it get their windows, each where it was asked to be
    /// meanwhile.
    pub(crate) fn context_ready(&self, profile: &str) {
        let waiting: Vec<QueuedContent> = {
            let mut inner = self.lock();
            inner.ready_contexts.insert(profile.to_string());
            if inner.quitting {
                // No new windows while the others close.
                inner.queued.clear();
            }
            inner
                .queued
                .extract_if(.., |q| q.profile.id == profile)
                .collect()
        };
        for page in waiting {
            let context = self.request_context(Some(&page.profile));
            self.build_content(page.id, &page.url, Some(&page.profile), context, page.mini);
            self.show_late(page.id);
        }
    }

    /// Put page `id`, whose window was made late, where it was asked to be
    /// while it had none: the peek, in the split on screen, or shown alone.
    fn show_late(&self, id: ContentId) {
        let (peek, split, focus, shown) = {
            let inner = self.lock();
            (
                inner.peek,
                inner.split.clone(),
                inner.focused_pane,
                inner.shown,
            )
        };
        if peek == Some(id) {
            self.show_peek(id);
        } else if let Some(tree) = split.filter(|t| t.contains(&id)) {
            self.show_split(&tree, focus);
        } else if shown == Some(id) {
            self.show_content(id);
        }
    }

    /// Make page `id`'s browser view and its window (hidden), loading `url`
    /// in `context` (`profile`'s). A `mini` window has its row and shows
    /// itself.
    fn build_content(
        &self,
        id: ContentId,
        url: &str,
        profile: Option<&crate::ContextProfile>,
        mut context: Option<RequestContext>,
        mini: bool,
    ) {
        let mut client = MagnetiteClient::content();
        let mut delegate = ContentViewDelegate::new(self.chrome_toolbar());
        let settings = BrowserSettings {
            // macOS draws its own link preview (`mac::set_status_pill`).
            chrome_status_bubble: if cfg!(target_os = "macos") {
                State::DISABLED
            } else {
                State::DEFAULT
            },
            ..Default::default()
        };
        // CEF makes no view in a context that is not initialised. Not a
        // panic: this also runs inside CEF's callback (`context_ready`).
        let Some(view) = browser_view_create(
            Some(&mut client),
            Some(&CefString::from(url)),
            Some(&settings),
            None,
            context.as_mut(),
            Some(&mut delegate),
        ) else {
            crate::trace(&format!("create_content #{id}: no browser view"));
            self.forget_content(id);
            self.emit(EngineEvent::ContentClosed { id });
            return;
        };
        // Before the lock below: the row reads the engine's state itself.
        let mini_chrome = if mini { self.mini_chrome(id) } else { None };
        self.lock().contents.insert(
            id,
            ContentState {
                window: None,
                view: view.clone(),
                browser: None,
                profile: profile.map(|p| p.id.clone()).unwrap_or_default(),
                ephemeral: profile.is_some_and(|p| p.ephemeral),
                devtools: None,
                slot: None,
                tab_target: None,
                mini: mini_chrome,
                adopted: false,
            },
        );

        let bounds = mini.then(|| self.mini_rect());
        let mut wd = ContentWindowDelegate::new(id, view, bounds);
        window_create_top_level(Some(&mut wd));
        crate::trace(&format!("create_content #{id} window created"));
    }

    pub fn show_content(&self, id: ContentId) {
        self.end_peek_if(|p| p == id);
        {
            let mut inner = self.lock();
            inner.split = None;
            inner.shown = Some(id);
        }
        self.update_focus_ring(None);
        self.update_dividers(None);
        let rect = self.content_rect();
        let Some((window, view)) = self.content_handles(id) else {
            return;
        };
        self.place(id, &rect);
        window.show();
        self.keep_attached(id, &window);
        window.bring_to_top();
        self.show_docked(id, true);
        self.update_grab_strip();
        self.place_notifications();
        #[cfg(target_os = "macos")]
        crate::mac::hide_status_pill();
        view.request_focus();
        self.raise_peek();
    }

    /// Show a split: every page of `tree` in its tile, `focus` focused.
    pub fn show_split(&self, tree: &SplitNode<ContentId>, focus: Option<ContentId>) {
        self.end_peek_if(|p| tree.contains(&p));
        {
            let mut inner = self.lock();
            inner.split = Some(tree.clone());
            inner.shown = None;
        }
        self.layout_split(tree);
        for id in tree.ids() {
            if let Some((window, _)) = self.content_handles(id) {
                window.show();
                self.keep_attached(id, &window);
                window.bring_to_top();
                self.show_docked(id, true);
            }
        }
        self.update_dividers(Some(tree));
        self.update_grab_strip();
        self.place_notifications();
        self.update_focus_ring(focus);
        #[cfg(target_os = "macos")]
        crate::mac::hide_status_pill();
        if let Some((_, view)) = focus.and_then(|f| self.content_handles(f)) {
            view.request_focus();
        }
        self.raise_peek();
    }

    /// Pages were raised (`show_content`, `show_split`): an open peek goes
    /// back above them, with its backdrop and the keyboard.
    fn raise_peek(&self) {
        // Its own statement: `content_handles` locks again.
        let peek = self.lock().peek;
        let Some(id) = peek else { return };
        let Some((window, view)) = self.content_handles(id) else {
            return;
        };
        window.bring_to_top();
        self.update_peek_backdrop();
        // The backdrop went above the peek's docked DevTools too.
        self.show_docked(id, true);
        view.request_focus();
    }

    /// Re-tile the visible split after its sizes changed (divider drag),
    /// without raising or refocusing anything.
    pub fn update_split(&self, tree: &SplitNode<ContentId>) {
        self.lock().split = Some(tree.clone());
        self.layout_split(tree);
        self.update_dividers(Some(tree));
        // The outline follows the resized page.
        let focus = self.lock().focused_pane;
        self.update_focus_ring(focus);
    }

    /// Place the pages of `tree` in the content area.
    pub(crate) fn layout_split(&self, tree: &SplitNode<ContentId>) {
        for (id, b) in tree.layout(self.split_area(), SPLIT_GAP) {
            self.place(id, &rect_of(b));
        }
    }

    /// The area a split fills, for turning divider drags into sizes.
    pub fn split_area(&self) -> Bounds {
        bounds_of(&self.content_rect())
    }

    /// Show `id` as a peek overlay: centred over the content area, above the
    /// panes. It stays until `hide_content`/`close_content`.
    pub fn show_peek(&self, id: ContentId) {
        self.lock().peek = Some(id);
        let Some((window, view)) = self.content_handles(id) else {
            return;
        };
        self.place(id, &peek_rect(&self.content_rect()));
        window.show();
        window.bring_to_top();
        self.show_docked(id, true);
        self.update_peek_backdrop();
        self.place_notifications();
        view.request_focus();
    }

    /// Dim the pages behind the open peek and show its buttons (macOS).
    pub(crate) fn update_peek_backdrop(&self) {
        // Two statements: a guard taken inside a statement lives to its end,
        // and `frame_window` locks again.
        let peek = self.lock().peek;
        let frame = self.frame_window();
        let (Some(id), Some(frame)) = (peek, frame) else {
            return;
        };
        let Some((window, _)) = self.content_handles(id) else {
            return;
        };
        #[cfg(target_os = "macos")]
        crate::mac::show_peek_backdrop(
            frame.window_handle(),
            window.window_handle(),
            &self.content_rect(),
            &window.bounds(),
        );
        #[cfg(not(target_os = "macos"))]
        let _ = (frame, window);
    }

    /// The peek page stops being a peek (closed, hidden, or now a tab) when
    /// `ends` says so for it.
    fn end_peek_if(&self, ends: impl FnOnce(ContentId) -> bool) {
        let ended = {
            let mut inner = self.lock();
            let ended = inner.peek.is_some_and(ends);
            if ended {
                inner.peek = None;
            }
            ended
        };
        #[cfg(target_os = "macos")]
        if ended {
            crate::mac::hide_peek_backdrop();
        }
        #[cfg(not(target_os = "macos"))]
        let _ = ended;
    }

    pub fn peek_content(&self) -> Option<ContentId> {
        self.lock().peek
    }

    pub fn hide_content(&self, id: ContentId) {
        self.end_peek_if(|p| p == id);
        // A split that loses a page is no longer on screen.
        let was_split = {
            let mut inner = self.lock();
            if inner.shown == Some(id) {
                inner.shown = None;
            }
            let was_split = inner.split.as_ref().is_some_and(|t| t.contains(&id));
            if was_split {
                inner.split = None;
            }
            was_split
        };
        if was_split {
            self.update_focus_ring(None);
            self.update_dividers(None);
        }
        if let Some((window, _)) = self.content_handles(id) {
            window.hide();
        }
        self.show_docked(id, false);
    }

    pub fn close_content(&self, id: ContentId) {
        self.end_peek_if(|p| p == id);
        self.close_docked(id);
        if let Some((window, _)) = self.content_handles(id) {
            window.close();
            return;
        }
        // Still waiting for its profile's context: no window will close, so
        // the page is forgotten and reported closed here.
        let queued = {
            let mut inner = self.lock();
            let before = inner.queued.len();
            inner.queued.retain(|q| q.id != id);
            inner.queued.len() != before
        };
        if queued {
            self.forget_content(id);
            self.emit(EngineEvent::ContentClosed { id });
        }
    }

    /// Everything the engine keeps about page `id` goes: its browser or
    /// window closed, or it never got one. Calls still waiting for the page
    /// get their error. Returns whether the page had a view. Never call it
    /// with the engine locked.
    fn forget_content(&self, id: ContentId) -> bool {
        // A page can close itself (`window.close()`) while it is the peek.
        self.end_peek_if(|p| p == id);
        let state = {
            let mut inner = self.lock();
            inner.permission_requests.remove(&id);
            inner.locks.remove(&id);
            inner.muted.remove(&id);
            if inner.focused_pane == Some(id) {
                inner.focused_pane = None;
            }
            if inner.shown == Some(id) {
                inner.shown = None;
            }
            inner.contents.remove(&id)
        };
        crate::devtools::page_closed(id);
        state.is_some()
    }

    pub fn content_load_url(&self, id: ContentId, url: &str) {
        if let Some(frame) = self.content_browser(id).and_then(|b| b.main_frame()) {
            frame.load_url(Some(&CefString::from(url)));
        } else if let Some(page) = self.lock().queued.iter_mut().find(|q| q.id == id) {
            // No window yet: it starts with this address.
            page.url = url.to_string();
        }
    }

    pub fn content_go_back(&self, id: ContentId) {
        if let Some(b) = self.content_browser(id) {
            b.go_back();
        }
    }
    pub fn content_go_forward(&self, id: ContentId) {
        if let Some(b) = self.content_browser(id) {
            b.go_forward();
        }
    }
    pub fn content_reload(&self, id: ContentId) {
        if let Some(b) = self.content_browser(id) {
            b.reload();
        }
    }
    /// Print page `id`: Chromium's print preview over the page.
    pub fn content_print(&self, id: ContentId) {
        if let Some(host) = self.content_browser_host(id) {
            host.print();
        }
    }
    pub fn content_stop(&self, id: ContentId) {
        if let Some(b) = self.content_browser(id) {
            b.stop_load();
        }
    }
    pub fn content_focus(&self, id: ContentId) {
        if let Some((_, view)) = self.content_handles(id) {
            view.request_focus();
        }
    }

    /// Colour of the space on screen (ARGB): outlines the focused split page.
    pub fn set_accent_color(&self, argb: u32) {
        let focus = {
            let mut inner = self.lock();
            if inner.accent_color == argb {
                return;
            }
            inner.accent_color = argb;
            inner.focused_pane
        };
        self.update_focus_ring(focus);
    }

    /// Outline page `focus` if it is part of the split on screen; every other
    /// page (and a page shown alone) has none.
    pub(crate) fn update_focus_ring(&self, focus: Option<ContentId>) {
        let (pages, accent) = {
            let mut inner = self.lock();
            let in_split = |id: ContentId| {
                inner
                    .split
                    .as_ref()
                    .is_some_and(|t| t.pane_count() > 1 && t.contains(&id))
            };
            let focus = focus.filter(|f| in_split(*f));
            let pages: Vec<(Window, bool)> = inner
                .contents
                .iter()
                .filter_map(|(id, c)| Some((c.window.clone()?, Some(*id) == focus)))
                .collect();
            inner.focused_pane = focus;
            (pages, inner.accent_color)
        };
        #[cfg(target_os = "macos")]
        for (window, ring) in pages {
            crate::mac::set_focus_ring(window.window_handle(), ring.then_some(accent));
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (pages, accent);
    }

    /// Whether page `id`'s window is on screen.
    #[cfg(target_os = "macos")] // used by the picture-in-picture controls
    pub(crate) fn content_shown(&self, id: ContentId) -> bool {
        self.lock()
            .contents
            .get(&id)
            .and_then(|c| c.window.clone())
            .is_some_and(|w| w.is_visible() != 0)
    }

    /// Give page `id` keyboard focus (its window comes forward).
    pub fn focus_content(&self, id: ContentId) {
        if let Some((window, view)) = self.content_handles(id) {
            window.activate();
            view.request_focus();
        }
    }

    fn content_handles(&self, id: ContentId) -> Option<(Window, BrowserView)> {
        let inner = self.lock();
        let c = inner.contents.get(&id)?;
        Some((c.window.clone()?, c.view.clone()))
    }

    pub(crate) fn content_browser(&self, id: ContentId) -> Option<Browser> {
        self.lock()
            .contents
            .get(&id)
            .and_then(|c| c.browser.clone())
    }

    /// Which content window owns `browser`, by browser identifier.
    pub(crate) fn content_id_for(&self, browser: Option<&Browser>) -> Option<ContentId> {
        let ident = browser?.identifier();
        self.lock()
            .contents
            .iter()
            .find(|(_, c)| c.browser.as_ref().is_some_and(|b| b.identifier() == ident))
            .map(|(id, _)| *id)
    }

    pub(crate) fn on_browser_created(&self, kind: Kind, browser: Option<&mut Browser>) {
        let Some(browser) = browser else { return };
        let mut browser = browser.clone();
        match kind {
            Kind::Sidebar => {
                let mut inner = self.lock();
                match inner.frame.as_mut() {
                    Some(f) => f.sidebar_browser = Some(browser),
                    None => inner.pending_sidebar_browser = Some(browser),
                }
            }
            Kind::Overlay | Kind::DevTools => {}
            Kind::Stray => {
                crate::stray::hide(&browser);
                self.lock().foreign.insert(browser.identifier(), browser);
            }
            Kind::Content => {
                let Some(view) = browser_view_get_for_browser(Some(&mut browser)) else {
                    return;
                };
                let mut matched = None;
                {
                    let mut inner = self.lock();
                    let mut target = View::from(&view);
                    for (id, c) in &mut inner.contents {
                        if View::from(&c.view).is_same(Some(&mut target)) != 0 {
                            c.browser = Some(browser.clone());
                            matched = Some(*id);
                            break;
                        }
                    }
                    // Muted before it had a browser (`content_set_muted`).
                    if matched.is_some_and(|id| inner.muted.contains(&id))
                        && let Some(host) = browser.host()
                    {
                        host.set_audio_muted(1);
                    }
                }
                match matched {
                    Some(id) => self.emit(EngineEvent::ContentCreated { id }),
                    // None of our pages: a page's popup window.
                    None => {
                        self.lock().foreign.insert(browser.identifier(), browser);
                    }
                }
            }
        }
    }

    pub(crate) fn on_browser_closed(&self, kind: Kind, browser: Option<&Browser>) {
        if let Some(b) = browser {
            self.forget_loaded_url(b.identifier());
            let mut inner = self.lock();
            inner.ui_reloads.remove(&b.identifier());
            inner.foreign.remove(&b.identifier());
        }
        if self.on_devtools_closed(browser) || kind != Kind::Content {
            return;
        }
        let Some(id) = self.content_id_for(browser) else {
            return;
        };
        self.forget_content(id);
        self.emit(EngineEvent::ContentClosed { id });
    }
}

wrap_browser_view_delegate! {
    struct ContentViewDelegate {
        toolbar: bool,
    }
    impl ViewDelegate {
        /// Chrome toolbars exist only once the view is inside a window; attach
        /// ours from a posted task (doing it re-entrantly crashes).
        fn on_window_changed(&self, view: Option<&mut View>, added: ::std::os::raw::c_int) {
            if added == 0 {
                return;
            }
            let Some(view) = view else { return };
            let Some(bv) = view.as_browser_view() else { return };
            let shown = self.toolbar;
            Engine::post(Box::new(move || attach_toolbar(&bv, shown)));
        }
    }
    impl BrowserViewDelegate {
        // Always there: extension popups anchor to it (see `attach_toolbar`).
        fn chrome_toolbar_type(&self, _bv: Option<&mut BrowserView>) -> ChromeToolbarType {
            ChromeToolbarType::NORMAL
        }
        fn browser_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::CHROME }
        fn delegate_for_popup_browser_view(
            &self,
            _browser_view: Option<&mut BrowserView>,
            _settings: Option<&BrowserSettings>,
            _client: Option<&mut Client>,
            is_devtools: ::std::os::raw::c_int,
        ) -> Option<BrowserViewDelegate> {
            (is_devtools != 0).then(crate::devdock::DevToolsViewDelegate::new)
        }
        fn on_popup_browser_view_created(
            &self,
            browser_view: Option<&mut BrowserView>,
            popup_browser_view: Option<&mut BrowserView>,
            is_devtools: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            // DevTools dock beside the page unless the user wants Chromium's
            // own window. A page's popup window (the only other popup that
            // gets here, see on_before_popup) gets CEF's default window.
            match (is_devtools != 0, browser_view, popup_browser_view) {
                (true, Some(source), Some(tools)) => i32::from(Engine::global().dock_devtools(source, tools)),
                _ => 0,
            }
        }
    }
}

wrap_window_delegate! {
    struct ContentWindowDelegate {
        id: ContentId,
        view: BrowserView,
        // Where a mini window opens; `None` for a tab's page window.
        mini: Option<Rect>,
    }
    impl ViewDelegate {}
    impl PanelDelegate {}
    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let Some(window) = window else { return };
            crate::trace(&format!("content window #{} on_window_created", self.id));
            // Vertical box: [toolbar row, if any] + browser view (flex).
            let layout = window
                .set_to_box_layout(Some(&BoxLayoutSettings { horizontal: 0, cross_axis_alignment: AxisAlignment::STRETCH, ..Default::default() }))
                .expect("content layout");
            let mut v = View::from(&self.view);
            window.add_child_view(Some(&mut v));
            layout.set_flex_for_view(Some(&mut v), 1);
            crate::trace(&format!("content window #{} view added", self.id));
            let engine = Engine::global();
            let row = {
                let mut inner = engine.lock();
                let c = inner.contents.get_mut(&self.id);
                c.and_then(|c| {
                    c.window = Some(window.clone());
                    c.mini.as_ref().map(|m| m.row.clone())
                })
            };
            // A mini window: its row above the page (added after the page,
            // which must be the window's first Chrome-style view), and it
            // shows itself, in front, with the keyboard in the page.
            if let Some(bounds) = &self.mini {
                if let Some(row) = row {
                    window.add_child_view_at(Some(&mut View::from(&row)), 0);
                }
                window.set_bounds(Some(bounds));
                #[cfg(target_os = "macos")]
                crate::mac::round_window_corners(window.window_handle(), 10.0, true);
                #[cfg(target_os = "windows")]
                crate::win::round_window_corners(window.window_handle(), true);
                window.show();
                window.activate();
                self.view.request_focus();
                return;
            }
            // Hidden until the app activates the tab.
            window.set_bounds(Some(&engine.content_rect()));
            #[cfg(target_os = "macos")]
            crate::mac::round_window_corners(window.window_handle(), 10.0, false);
            #[cfg(target_os = "windows")]
            crate::win::round_window_corners(window.window_handle(), false);
            #[cfg(target_os = "macos")]
            crate::mac::pin_window(window.window_handle());
            window.show();
            window.hide();
        }
        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            let engine = Engine::global();
            if engine.forget_content(self.id) {
                engine.emit(EngineEvent::ContentClosed { id: self.id });
            }
        }
        fn parent_window(
            &self,
            _window: Option<&mut Window>,
            _is_menu: Option<&mut ::std::os::raw::c_int>,
            _can_activate_menu: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<Window> {
            // A mini window is a window of its own.
            if self.mini.is_some() {
                return None;
            }
            Engine::global().frame_window()
        }
        fn initial_bounds(&self, _window: Option<&mut Window>) -> Rect {
            self.mini.clone().unwrap_or_else(|| Engine::global().content_rect())
        }
        fn can_resize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { self.mini.is_some().into() }
        fn can_minimize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { self.mini.is_some().into() }
        fn is_frameless(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 0 }
        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn on_key_event(&self, _window: Option<&mut Window>, event: Option<&KeyEvent>) -> ::std::os::raw::c_int {
            let Some(shortcut) = event.and_then(|e| crate::keys::shortcut_for_key(e).or_else(|| crate::keys::yieldable_shortcut(e))) else { return 0 };
            Engine::global().emit(EngineEvent::Shortcut { from: Some(self.id), shortcut });
            1
        }
        // Not key, yet clickable at once: the first press is a real click
        // (macOS would otherwise spend it on activating the window).
        fn accepts_first_mouse(&self, _window: Option<&mut Window>) -> State { State::ENABLED }
        fn window_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::CHROME }
    }
}

/// 86% of the content area, centred.
pub(crate) fn peek_rect(area: &Rect) -> Rect {
    // Room on both sides for the peek's button column (mac::show_peek_backdrop).
    let w = (area.width * 86 / 100).min(area.width - 2 * 52);
    let h = area.height * 90 / 100;
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Width the hidden toolbar is laid out at: narrow, so its extension
/// buttons (where popups anchor) sit near the page's top-left corner.
const HIDDEN_TOOLBAR_WIDTH: i32 = 400;
/// Chromium's toolbar height, to shift it just out of sight.
const TOOLBAR_HEIGHT: i32 = 40;

/// Put page `view`'s Chrome toolbar into its window: a row above the page
/// when `shown`, else out of sight. Runs from a posted task, once the view
/// is in its window.
fn attach_toolbar(view: &BrowserView, shown: bool) {
    let Some(mut toolbar) = view.chrome_toolbar() else {
        return;
    };
    let Some(window) = view.window() else { return };
    toolbar.set_visible(1);
    if shown {
        window.add_child_view_at(Some(&mut toolbar), 0);
        window.layout();
        return;
    }
    // Not shown, but extension popups and Chrome's bubbles still
    // anchor to it. It stays in the window's own views (Chromium tears
    // it down with the window; elsewhere, closing crashes): a row one
    // point tall above the page holds a narrow box, so the toolbar's
    // buttons sit near the page's top-left corner, and the box shifts
    // the toolbar up just out of sight. (Panels fill their children,
    // hence boxes instead of plain bounds.)
    let Some(row) = panel_create(Some(&mut ToolbarHost::new(1))) else {
        return;
    };
    row.set_to_box_layout(Some(&BoxLayoutSettings {
        horizontal: 1,
        ..Default::default()
    }));
    let mut row_view = View::from(&row);
    window.add_child_view_at(Some(&mut row_view), 0);
    let Some(holder) = panel_create(Some(&mut ToolbarHost::new(HIDDEN_TOOLBAR_WIDTH))) else {
        return;
    };
    let mut holder_view = View::from(&holder);
    row.add_child_view(Some(&mut holder_view));
    holder.set_to_box_layout(Some(&BoxLayoutSettings {
        horizontal: 0,
        cross_axis_alignment: AxisAlignment::STRETCH,
        inside_border_insets: Insets {
            top: -TOOLBAR_HEIGHT - 1,
            left: 0,
            bottom: 0,
            right: 0,
        },
        ..Default::default()
    }));
    holder.add_child_view(Some(&mut toolbar));
    window.layout();
}

// A box one point tall, `width` wide, around the hidden toolbar.
wrap_panel_delegate! {
    struct ToolbarHost {
        width: i32,
    }
    impl ViewDelegate {
        // An empty size would mean "as big as the toolbar inside".
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: self.width, height: 1 }
        }
    }
    impl PanelDelegate {}
}
