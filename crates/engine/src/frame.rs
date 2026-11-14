//! The frame window: the top bar view (when on) above the body, which holds
//! the sidebar view and the content panel.

use cef::*;

use crate::client::MagnetiteClient;
use crate::engine::Engine;
use crate::events::{ContentId, EngineEvent};

pub(crate) struct FrameState {
    pub window: Window,
    /// The row below the top bar: sidebar view and content panel.
    pub body: Panel,
    pub sidebar_view: BrowserView,
    /// The top bar's view, made the first time it is turned on.
    pub top_bar_view: Option<BrowserView>,
    pub top_bar_browser: Option<Browser>,
    pub content_panel: Panel,
    pub sidebar_browser: Option<Browser>,
    /// The hint in the content panel for a window without a page.
    pub empty: Option<crate::empty::EmptyHint>,
}

impl Engine {
    /// Create the frame window. Call once, after `EngineEvent::Ready`.
    pub fn create_frame(&self) {
        let mut delegate = FrameWindowDelegate::new();
        window_create_top_level(Some(&mut delegate));
    }

    /// Gap between the page and the frame edges / sidebar, in DIP.
    pub const CONTENT_INSET: i32 = 8;
    /// How far the window-drag strip reaches into the top of the pages.
    const GRAB_OVERLAP: i32 = 6;
    /// Room for the traffic lights when the sidebar is hidden.
    const TRAFFIC_LIGHTS: i32 = 80;
    /// Gap above the pages while the sidebar is hidden in a normal window:
    /// the traffic lights sit in it. Pages are windows above the frame and
    /// would cover them otherwise.
    const BUTTONS_BAR: i32 = 34;
    /// Height of the top bar (`magnetite://ui/topbar`).
    pub const TOP_BAR_HEIGHT: i32 = 42;

    /// Keep the window-drag strip along the top of the page area, above the
    /// page windows (see `mac::set_grab_strip`).
    pub(crate) fn update_grab_strip(&self) {
        let frame = {
            let inner = self.lock();
            inner
                .frame
                .as_ref()
                .map(|f| (f.window.clone(), inner.sidebar_visible))
        };
        let Some((window, sidebar)) = frame else {
            return;
        };
        let panel = self.content_panel_in_screen();
        let skip = if sidebar { 0 } else { Self::TRAFFIC_LIGHTS };
        let strip = Rect {
            x: panel.x + skip,
            y: self.content_rect().y - Self::CONTENT_INSET,
            width: (panel.width - skip).max(1),
            height: Self::CONTENT_INSET + Self::GRAB_OVERLAP,
        };
        #[cfg(target_os = "macos")]
        crate::mac::set_grab_strip(window.window_handle(), &strip);
        #[cfg(not(target_os = "macos"))]
        let _ = (window, strip);
        self.update_edge_zone();
    }

    /// Show page `id`'s status text (the hovered link) as a pill near the
    /// bottom of that page; empty text hides it. Elsewhere Chromium's own
    /// status bubble stays on.
    #[cfg_attr(not(target_os = "macos"), allow(clippy::unused_self))]
    pub(crate) fn show_status(&self, id: ContentId, text: &str) {
        #[cfg(target_os = "macos")]
        {
            let frame = self.frame_window();
            // The pill is the frame's: none over a mini window.
            let page = (self.lock().contents.get(&id))
                .filter(|c| c.mini.is_none())
                .and_then(|c| c.window.clone());
            // Background tabs report status too; only a page on screen shows it.
            if let (Some(frame), Some(page)) = (frame, page.filter(|w| w.is_visible() != 0)) {
                crate::mac::set_status_pill(frame.window_handle(), &page.bounds(), text);
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (id, text);
    }

    /// Screen rectangle (DIP) the content windows must cover: the content
    /// panel minus an inset so the page floats inside the frame, and minus
    /// the room for the traffic lights when no sidebar holds them (a full
    /// screen window has none).
    pub fn content_rect(&self) -> Rect {
        let r = self.content_panel_in_screen();
        let buttons = {
            let inner = self.lock();
            !inner.sidebar_visible
                && !inner.top_bar_visible
                && (inner.frame.as_ref()).is_some_and(|f| f.window.is_fullscreen() == 0)
        };
        let i = Self::CONTENT_INSET;
        let top = if buttons { Self::BUTTONS_BAR } else { i };
        Rect {
            x: r.x + i,
            y: r.y + top,
            width: (r.width - 2 * i).max(1),
            height: (r.height - top - i).max(1),
        }
    }

    /// Draggable areas of the frame: the top bar's and the sidebar page's
    /// `app-region: drag` zones plus the whole content panel. Pages sit in
    /// child windows above the panel, so only the gaps around them (top
    /// edge, sides, split gutters) end up moving the window.
    pub(crate) fn apply_drag_regions(&self) {
        let (window, sidebar, top_bar) = {
            let inner = self.lock();
            let Some(f) = inner.frame.as_ref() else {
                return;
            };
            let sidebar = if inner.sidebar_visible {
                inner.sidebar_drag.clone()
            } else {
                Vec::new()
            };
            let top_bar = if inner.top_bar_visible {
                inner.top_bar_drag.clone()
            } else {
                Vec::new()
            };
            (f.window.clone(), sidebar, top_bar)
        };
        // The sidebar page reports bounds within itself, below the top bar.
        let below = |mut r: DraggableRegion| {
            r.bounds.y += self.top_bar_offset();
            r
        };
        let mut regions = vec![DraggableRegion {
            bounds: self.content_panel_in_window(),
            draggable: 1,
        }];
        regions.extend(sidebar.into_iter().map(below));
        regions.extend(top_bar);
        window.set_draggable_regions(Some(&regions));
    }

    /// How far the body sits below the window's top: the top bar's height
    /// while it is on.
    pub(crate) fn top_bar_offset(&self) -> i32 {
        if self.top_bar_visible() {
            Self::TOP_BAR_HEIGHT
        } else {
            0
        }
    }

    /// The content panel's place in the window: right of the sidebar, below
    /// the top bar. Worked out from the window's size, not asked of the
    /// panel: Chromium lays out the nested body lazily, so the panel can
    /// still report where it was before a change.
    fn content_panel_in_window(&self) -> Rect {
        let inner = self.lock();
        let Some(f) = inner.frame.as_ref() else {
            return Rect::default();
        };
        let client = f.window.bounds();
        let x = if inner.sidebar_visible {
            inner.sidebar_width
        } else {
            0
        };
        let y = if inner.top_bar_visible {
            Self::TOP_BAR_HEIGHT
        } else {
            0
        };
        Rect {
            x,
            y,
            width: (client.width - x).max(1),
            height: (client.height - y).max(1),
        }
    }

    /// `content_panel_in_window` in screen coordinates.
    fn content_panel_in_screen(&self) -> Rect {
        let client = (self.frame_window())
            .map(|w| w.bounds())
            .unwrap_or_default();
        let r = self.content_panel_in_window();
        Rect {
            x: client.x + r.x,
            y: client.y + r.y,
            ..r
        }
    }

    pub fn top_bar_visible(&self) -> bool {
        self.lock().top_bar_visible
    }

    /// Show the top bar above the sidebar and the pages, or hide it. Its
    /// page is made the first time.
    pub fn set_top_bar_visible(&self, visible: bool) {
        let frame = {
            let mut inner = self.lock();
            inner.top_bar_visible = visible;
            inner
                .frame
                .as_ref()
                .map(|f| (f.window.clone(), f.top_bar_view.clone()))
        };
        let Some((window, view)) = frame else {
            return;
        };
        match view {
            Some(view) => View::from(&view).set_visible(visible.into()),
            None if visible => {
                let Some(view) = create_top_bar_view() else {
                    return;
                };
                window.add_child_view_at(Some(&mut View::from(&view)), 0);
                if let Some(f) = self.lock().frame.as_mut() {
                    f.top_bar_view = Some(view);
                }
            }
            None => {}
        }
        window.layout();
        self.reposition_contents();
        self.apply_drag_regions();
        self.update_edge_zone();
    }

    /// The top bar's window buttons (Linux and Windows; macOS has its own).
    pub fn minimize_frame(&self) {
        if let Some(w) = self.frame_window() {
            w.minimize();
        }
    }

    /// Maximize the frame window, or restore it when it is maximized.
    pub fn toggle_maximize_frame(&self) {
        if let Some(w) = self.frame_window() {
            if w.is_maximized() != 0 {
                w.restore();
            } else {
                w.maximize();
            }
        }
    }

    /// Close the frame window: asks to quit, as the window manager's close does.
    pub fn close_frame(&self) {
        if let Some(w) = self.frame_window() {
            w.close();
        }
    }

    /// Open `url` in an overlay below the top bar, `width` by `height`, its
    /// top-left at (`x`, `y`) in the top bar page's coordinates: the top
    /// bar's pop-ups (site controls, menus), which its short page cannot hold.
    pub fn open_top_bar_popup(&self, url: &str, x: i32, y: i32, width: i32, height: i32) -> bool {
        let Some(bar) = (self.lock().frame.as_ref()).and_then(|f| {
            f.top_bar_view
                .as_ref()
                .map(|v| View::from(v).bounds_in_screen())
        }) else {
            return false;
        };
        // Kept inside the window's width.
        let x = x.clamp(0, (bar.width - width).max(0));
        let bounds = Rect {
            x: bar.x + x,
            y: bar.y + y,
            width,
            height,
        };
        self.open_overlay_with(bounds, url, 12.0, true, true)
    }

    /// Open the system share menu for `url` at a point of the frame window
    /// (DIP from its top-left; the sidebar page's client coordinates).
    pub fn share_url(&self, url: &str, x: f64, y: f64) {
        let window = self.frame_window();
        #[cfg(target_os = "macos")]
        if let Some(w) = window {
            crate::mac::show_share_picker(w.window_handle(), url, x, y);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (window, url, x, y);
    }

    pub fn set_sidebar_visible(&self, visible: bool) {
        let frame = {
            let mut inner = self.lock();
            inner.sidebar_visible = visible;
            inner
                .frame
                .as_ref()
                .map(|f| (f.window.clone(), f.sidebar_view.clone()))
        };
        if let Some((window, view)) = frame {
            View::from(&view).set_visible(visible.into());
            window.layout();
            self.reposition_contents();
            self.apply_drag_regions();
        }
        if visible {
            self.show_float_sidebar(false);
        }
        self.update_edge_zone();
    }

    /// Width of the docked sidebar.
    pub fn sidebar_width(&self) -> i32 {
        self.lock().sidebar_width
    }

    /// Resize the docked sidebar (the pages give or take the room).
    pub fn set_sidebar_width(&self, width: u32) {
        let width = i32::try_from(width).unwrap_or(264);
        let frame = {
            let mut inner = self.lock();
            if inner.sidebar_width == width {
                return;
            }
            inner.sidebar_width = width;
            inner
                .frame
                .as_ref()
                .map(|f| (f.window.clone(), f.sidebar_view.clone()))
        };
        if let Some((window, sidebar)) = frame {
            // The box layout caches the sidebar's preferred width.
            View::from(&sidebar).invalidate_layout();
            window.layout();
            self.reposition_contents();
            self.apply_drag_regions();
        }
    }

    /// Colour of the chrome behind the page cards (ARGB). The sidebar page
    /// paints the same colour so there is no seam.
    pub fn set_chrome_color(&self, argb: u32) {
        self.lock().chrome_color = argb;
        let panels =
            (self.lock().frame.as_ref()).map(|f| (f.content_panel.clone(), f.body.clone()));
        if let Some((content, body)) = panels {
            content.set_background_color(argb);
            body.set_background_color(argb);
        }
        self.colour_empty_hint(argb);
    }

    pub fn sidebar_visible(&self) -> bool {
        self.lock().sidebar_visible
    }

    /// Bring the frame window to the front and focus the sidebar page (the
    /// floating one while it is out).
    pub fn focus_sidebar(&self) {
        if self.float_sidebar_shown() {
            self.focus_float_sidebar();
            return;
        }
        let handles = self
            .lock()
            .frame
            .as_ref()
            .map(|f| (f.window.clone(), f.sidebar_view.clone()));
        if let Some((window, view)) = handles {
            window.activate();
            view.request_focus();
            if let Some(b) = self
                .lock()
                .frame
                .as_ref()
                .and_then(|f| f.sidebar_browser.clone())
                && let Some(h) = b.host()
            {
                h.set_focus(1);
            }
        }
    }

    pub(crate) fn reposition_contents(&self) {
        let (split, peek) = {
            let inner = self.lock();
            (inner.split.clone(), inner.peek)
        };
        if let Some(p) = peek {
            self.place(p, &crate::content::peek_rect(&self.content_rect()));
            self.update_peek_backdrop();
        }
        if let Some(tree) = split.filter(|t| t.pane_count() > 1) {
            self.layout_split(&tree);
            self.update_dividers(Some(&tree));
            self.update_grab_strip();
            self.place_notifications();
            return;
        }
        let rect = self.content_rect();
        // Mini windows are not in the frame.
        let ids: Vec<ContentId> = (self.lock().contents.iter())
            .filter(|(_, c)| c.mini.is_none())
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if Some(id) != peek {
                self.place(id, &rect);
            }
        }
        self.update_grab_strip();
        self.place_notifications();
    }
}

wrap_browser_view_delegate! {
    struct SidebarViewDelegate {}
    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: Engine::global().sidebar_width(), height: 600 }
        }
        fn minimum_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: Engine::global().sidebar_width(), height: 0 }
        }
    }
    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
        fn on_popup_browser_view_created(
            &self,
            _browser_view: Option<&mut BrowserView>,
            _popup_browser_view: Option<&mut BrowserView>,
            _is_devtools: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            // No popups from the sidebar page.
            1
        }
    }
}

/// The top bar's view: our UI at `magnetite://ui/topbar`, full width.
fn create_top_bar_view() -> Option<BrowserView> {
    let engine = Engine::global();
    let mut client = MagnetiteClient::top_bar();
    let mut delegate = TopBarViewDelegate::new();
    let settings = BrowserSettings {
        background_color: engine.lock().chrome_color,
        ..Default::default()
    };
    // Next to the sidebar's page, also on a development server.
    let url = format!("{}/topbar", engine.config().ui_url.trim_end_matches('/'));
    browser_view_create(
        Some(&mut client),
        Some(&CefString::from(url.as_str())),
        Some(&settings),
        None,
        None,
        Some(&mut delegate),
    )
}

wrap_browser_view_delegate! {
    struct TopBarViewDelegate {}
    impl ViewDelegate {
        // A size with no width counts as none: CEF would use the page's own.
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 1, height: Engine::TOP_BAR_HEIGHT }
        }
        fn minimum_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 1, height: Engine::TOP_BAR_HEIGHT }
        }
        fn maximum_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: i32::MAX, height: Engine::TOP_BAR_HEIGHT }
        }
        // The window's column layout asks this, not the preferred size.
        fn height_for_width(&self, _view: Option<&mut View>, _width: ::std::os::raw::c_int) -> ::std::os::raw::c_int {
            Engine::TOP_BAR_HEIGHT
        }
    }
    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
        fn on_popup_browser_view_created(
            &self,
            _browser_view: Option<&mut BrowserView>,
            _popup_browser_view: Option<&mut BrowserView>,
            _is_devtools: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            // No popups from the top bar page.
            1
        }
    }
}

wrap_panel_delegate! {
    struct BodyPanelDelegate {}
    impl ViewDelegate {}
    impl PanelDelegate {}
}

wrap_panel_delegate! {
    struct ContentPanelDelegate {}
    impl ViewDelegate {
        fn minimum_size(&self, _view: Option<&mut View>) -> Size { Size { width: 320, height: 240 } }
    }
    impl PanelDelegate {}
}

wrap_window_delegate! {
    struct FrameWindowDelegate {}

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size { Size { width: 1280, height: 860 } }
    }
    impl PanelDelegate {}
    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let Some(window) = window else { return };
            let engine = Engine::global();
            // Top to bottom: the top bar (added when turned on), the body.
            let column = window
                .set_to_box_layout(Some(&BoxLayoutSettings {
                    horizontal: 0,
                    cross_axis_alignment: AxisAlignment::STRETCH,
                    ..Default::default()
                }))
                .expect("box layout");
            let mut bd = BodyPanelDelegate::new();
            let body = panel_create(Some(&mut bd)).expect("body panel");
            body.set_background_color(0xFF1B_1B1E);
            let layout = body
                .set_to_box_layout(Some(&BoxLayoutSettings {
                    horizontal: 1,
                    cross_axis_alignment: AxisAlignment::STRETCH,
                    ..Default::default()
                }))
                .expect("box layout");

            let mut cd = ContentPanelDelegate::new();
            let content = panel_create(Some(&mut cd)).expect("content panel");
            content.set_background_color(0xFF1B_1B1E);

            let mut client = MagnetiteClient::sidebar();
            let mut sd = SidebarViewDelegate::new();
            let settings = BrowserSettings { background_color: 0xFF1B_1B1E, ..Default::default() };
            let sidebar = browser_view_create(
                Some(&mut client),
                Some(&CefString::from(engine.config().ui_url.as_str())),
                Some(&settings),
                None,
                None,
                Some(&mut sd),
            )
            .expect("sidebar view");

            let mut sv = View::from(&sidebar);
            let mut cv = View::from(&content);
            body.add_child_view(Some(&mut sv));
            body.add_child_view(Some(&mut cv));
            layout.set_flex_for_view(Some(&mut cv), 1);
            let mut bv = View::from(&body);
            window.add_child_view(Some(&mut bv));
            column.set_flex_for_view(Some(&mut bv), 1);
            window.set_title(Some(&CefString::from("magnetite")));
            window.center_window(Some(&Size { width: 1280, height: 860 }));
            window.show();
            window.layout();

            let mut inner = engine.lock();
            let sidebar_browser = inner.pending_sidebar_browser.take();
            inner.frame = Some(FrameState {
                window: window.clone(),
                body,
                sidebar_view: sidebar,
                top_bar_view: None,
                top_bar_browser: None,
                empty: crate::empty::EmptyHint::new(&content),
                content_panel: content,
                sidebar_browser,
            });
            drop(inner);
            // Turned on before the window existed (the saved setting).
            if engine.top_bar_visible() {
                engine.set_top_bar_visible(true);
            }
            engine.emit(EngineEvent::FrameCreated);
        }

        fn on_window_bounds_changed(&self, _window: Option<&mut Window>, _new_bounds: Option<&Rect>) {
            let engine = Engine::global();
            engine.reposition_contents();
            engine.apply_drag_regions();
            engine.update_edge_zone();
            if engine.float_sidebar_shown() {
                engine.show_float_sidebar(true);
            }
            engine.emit(EngineEvent::FrameBoundsChanged);
        }

        // Entering or leaving full screen changes the room above the pages.
        fn on_window_fullscreen_transition(&self, _window: Option<&mut Window>, is_completed: ::std::os::raw::c_int) {
            if is_completed != 0 {
                Engine::global().reposition_contents();
            }
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            let engine = Engine::global();
            engine.lock().frame = None;
            quit_message_loop();
        }

        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            let engine = Engine::global();
            if engine.is_quitting() {
                1
            } else {
                engine.emit(EngineEvent::QuitRequested);
                0
            }
        }

        fn on_key_event(&self, _window: Option<&mut Window>, event: Option<&KeyEvent>) -> ::std::os::raw::c_int {
            let Some(shortcut) = event.and_then(|e| crate::keys::shortcut_for_key(e).or_else(|| crate::keys::yieldable_shortcut(e))) else { return 0 };
            Engine::global().emit(EngineEvent::Shortcut { from: None, shortcut });
            1
        }

        fn accepts_first_mouse(&self, _window: Option<&mut Window>) -> State { State::ENABLED }
        fn can_resize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn can_maximize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn can_minimize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn is_frameless(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn titlebar_height(&self, _window: Option<&mut Window>, height: Option<&mut f32>) -> ::std::os::raw::c_int {
            if let Some(h) = height { *h = 42.0; }
            1
        }
        // The frame only hosts the Alloy-style sidebar view; Chrome-style content
        // lives in child windows. A Chrome-style frame without a Chrome-style view
        // never finishes initialising (stays fully transparent on macOS).
        fn window_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
    }
}
