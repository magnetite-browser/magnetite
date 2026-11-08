//! The frame window: sidebar view + content panel.

use cef::*;

use crate::client::MagnetiteClient;
use crate::engine::Engine;
use crate::events::{ContentId, EngineEvent};

pub(crate) struct FrameState {
    pub window: Window,
    pub sidebar_view: BrowserView,
    pub content_panel: Panel,
    pub sidebar_browser: Option<Browser>,
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

    /// Keep the window-drag strip along the top of the page area, above the
    /// page windows (see `mac::set_grab_strip`).
    pub(crate) fn update_grab_strip(&self) {
        let frame = {
            let inner = self.lock();
            inner.frame.as_ref().map(|f| {
                (
                    f.window.clone(),
                    f.content_panel.bounds_in_screen(),
                    inner.sidebar_visible,
                )
            })
        };
        let Some((window, panel, sidebar)) = frame else {
            return;
        };
        let skip = if sidebar { 0 } else { Self::TRAFFIC_LIGHTS };
        let strip = Rect {
            x: panel.x + skip,
            y: panel.y,
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
    /// panel minus an inset so the page floats inside the frame.
    pub fn content_rect(&self) -> Rect {
        let r = self
            .lock()
            .frame
            .as_ref()
            .map(|f| f.content_panel.bounds_in_screen())
            .unwrap_or_default();
        let i = Self::CONTENT_INSET;
        Rect {
            x: r.x + i,
            y: r.y + i,
            width: (r.width - 2 * i).max(1),
            height: (r.height - 2 * i).max(1),
        }
    }

    /// Draggable areas of the frame: the sidebar page's `app-region: drag`
    /// zones plus the whole content panel. Pages sit in child windows above
    /// the panel, so only the gaps around them (top edge, sides, split
    /// gutters) end up moving the window.
    pub(crate) fn apply_drag_regions(&self) {
        let (window, panel, sidebar) = {
            let inner = self.lock();
            let Some(f) = inner.frame.as_ref() else {
                return;
            };
            let sidebar = if inner.sidebar_visible {
                inner.sidebar_drag.clone()
            } else {
                Vec::new()
            };
            (f.window.clone(), f.content_panel.clone(), sidebar)
        };
        let mut regions = vec![DraggableRegion {
            bounds: panel.bounds(),
            draggable: 1,
        }];
        regions.extend(sidebar);
        window.set_draggable_regions(Some(&regions));
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
        if let Some(panel) = self.lock().frame.as_ref().map(|f| f.content_panel.clone()) {
            panel.set_background_color(argb);
        }
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
            let layout = window
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
            window.add_child_view(Some(&mut sv));
            window.add_child_view(Some(&mut cv));
            layout.set_flex_for_view(Some(&mut cv), 1);
            window.set_title(Some(&CefString::from("magnetite")));
            window.center_window(Some(&Size { width: 1280, height: 860 }));
            window.show();
            window.layout();

            let mut inner = engine.lock();
            let sidebar_browser = inner.pending_sidebar_browser.take();
            inner.frame = Some(FrameState {
                window: window.clone(),
                sidebar_view: sidebar,
                content_panel: content,
                sidebar_browser,
            });
            drop(inner);
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
