//! The sidebar as a floating panel. While the docked sidebar is hidden, the
//! pointer reaching the window's left edge (`mac::set_edge_zone`,
//! `win::set_edge_zone`) slides a
//! second sidebar page (`magnetite://ui/?float`) over the pages; it hides again
//! when the pointer leaves it. Created on first use, then kept hidden.

use cef::*;

use crate::client::MagnetiteClient;
use crate::engine::Engine;

/// Room left free above the panel: the frame's traffic lights.
const TOP: i32 = 40;
/// Gap between the panel and the window edges.
const MARGIN: i32 = 6;
/// Width of the hover zone along the left edge (the page gap).
const EDGE: i32 = 8;

/// The floating sidebar's window and page.
pub(crate) struct FloatBar {
    pub window: Option<Window>,
    pub view: BrowserView,
    pub shown: bool,
}

impl Engine {
    /// Slide the floating sidebar out (`true`) or hide it. Does nothing while
    /// the docked sidebar is visible.
    pub fn show_float_sidebar(&self, show: bool) {
        if show && self.sidebar_visible() {
            return;
        }
        let Some(bounds) = self.float_bounds() else {
            return;
        };
        let existing = self.lock().float_bar.as_ref().map(|f| f.window.clone());
        match existing {
            Some(Some(window)) => {
                if show {
                    window.set_bounds(Some(&bounds));
                    show_quietly(&window);
                } else {
                    window.hide();
                }
            }
            None if show => self.create_float_sidebar(bounds),
            // Window still being created (it shows itself when ready, if
            // `shown` says so), or nothing to hide.
            _ => {}
        }
        if let Some(f) = self.lock().float_bar.as_mut() {
            f.shown = show;
        }
    }

    /// Whether the floating sidebar is out.
    pub fn float_sidebar_shown(&self) -> bool {
        self.lock().float_bar.as_ref().is_some_and(|f| f.shown)
    }

    /// The floating sidebar's page, once it exists.
    pub(crate) fn float_sidebar_browser(&self) -> Option<Browser> {
        self.lock()
            .float_bar
            .as_ref()
            .and_then(|f| f.view.browser())
    }

    /// Focus the floating sidebar (its pop-ups ask for focus).
    pub(crate) fn focus_float_sidebar(&self) {
        let handles = self
            .lock()
            .float_bar
            .as_ref()
            .and_then(|f| Some((f.window.clone()?, f.view.clone())));
        if let Some((window, view)) = handles {
            window.activate();
            view.request_focus();
        }
    }

    /// The hover zone exists exactly while the docked sidebar is hidden.
    pub(crate) fn update_edge_zone(&self) {
        let (frame, hidden) = (self.frame_window(), !self.sidebar_visible());
        let Some(frame) = frame else { return };
        let b = frame.bounds();
        let zone = Rect {
            x: b.x,
            y: b.y + TOP,
            width: EDGE,
            height: (b.height - TOP).max(1),
        };
        #[cfg(target_os = "macos")]
        crate::mac::set_edge_zone(frame.window_handle(), hidden.then_some(&zone));
        #[cfg(target_os = "windows")]
        {
            let _ = zone;
            crate::win::set_edge_zone(frame.window_handle(), hidden.then_some((EDGE, TOP)));
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let _ = (zone, hidden);
    }

    /// Below the traffic lights, along the left edge, the sidebar's width.
    fn float_bounds(&self) -> Option<Rect> {
        let frame = self.frame_window()?;
        let b = frame.bounds();
        Some(Rect {
            x: b.x + MARGIN,
            y: b.y + TOP,
            width: self.sidebar_width(),
            height: (b.height - TOP - MARGIN).max(100),
        })
    }

    fn create_float_sidebar(&self, bounds: Rect) {
        let mut client = MagnetiteClient::overlay();
        let mut delegate = FloatViewDelegate::new();
        let Some(view) = browser_view_create(
            Some(&mut client),
            Some(&CefString::from("magnetite://ui/?float")),
            Some(&BrowserSettings {
                background_color: self.lock().chrome_color,
                ..Default::default()
            }),
            None,
            None,
            Some(&mut delegate),
        ) else {
            return;
        };
        self.lock().float_bar = Some(FloatBar {
            window: None,
            view: view.clone(),
            shown: true,
        });
        let mut wd = FloatWindowDelegate::new(view, bounds);
        window_create_top_level(Some(&mut wd));
    }
}

/// Show the panel on top without taking the keyboard from the page: hovering
/// is not choosing. A click into the panel focuses it as usual.
pub(crate) fn show_quietly(window: &Window) {
    #[cfg(target_os = "macos")]
    crate::mac::without_taking_focus(|| {
        window.show();
        window.bring_to_top();
    });
    #[cfg(not(target_os = "macos"))]
    {
        window.show();
        window.bring_to_top();
    }
}

wrap_browser_view_delegate! {
    struct FloatViewDelegate {}
    impl ViewDelegate {}
    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
    }
}

wrap_window_delegate! {
    struct FloatWindowDelegate {
        view: BrowserView,
        bounds: Rect,
    }
    impl ViewDelegate {}
    impl PanelDelegate {}
    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let Some(window) = window else { return };
            window.set_to_fill_layout();
            let mut v = View::from(&self.view);
            window.add_child_view(Some(&mut v));
            window.set_bounds(Some(&self.bounds));
            #[cfg(target_os = "macos")]
            crate::mac::round_window_corners(window.window_handle(), 10.0, true);
            #[cfg(target_os = "windows")]
            crate::win::round_window_corners(window.window_handle(), true);
            let engine = Engine::global();
            let show = {
                let mut inner = engine.lock();
                let bar = inner.float_bar.as_mut();
                bar.map(|f| {
                    f.window = Some(window.clone());
                    f.shown
                })
            };
            if show == Some(true) {
                show_quietly(window);
            }
        }
        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            Engine::global().lock().float_bar = None;
        }
        fn parent_window(
            &self,
            _window: Option<&mut Window>,
            _is_menu: Option<&mut ::std::os::raw::c_int>,
            _can_activate_menu: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<Window> {
            Engine::global().frame_window()
        }
        fn initial_bounds(&self, _window: Option<&mut Window>) -> Rect { self.bounds.clone() }
        fn is_frameless(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 0 }
        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        // Not key, yet clickable at once: the first press is a real click
        // (macOS would otherwise spend it on activating the window).
        fn accepts_first_mouse(&self, _window: Option<&mut Window>) -> State { State::ENABLED }
        fn window_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
    }
}
