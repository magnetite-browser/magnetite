//! The sidebar as a floating panel. While the docked sidebar is hidden, the
//! pointer reaching the window's edge on the sidebar's side
//! (`mac::set_edge_zone`, `win::set_edge_zone`, `edge.rs` on Linux) slides a
//! second sidebar page (`magnetite://ui/?float`) over the pages; it hides again
//! when the pointer leaves it. Created on first use, then kept hidden.

use cef::*;

use crate::SidebarSide;
use crate::client::MagnetiteClient;
use crate::engine::Engine;

/// Gap between the panel and the window edges while the frame around the
/// pages shows; without it the panel reaches them, as the pages do.
const MARGIN: i32 = 6;
/// Width of the hover zone along the window's edge: the gap beside the
/// pages while the frame around them shows.
const EDGE: i32 = 8;
/// Its width where a page reaches the window's edge (no frame): a thin
/// strip, so the page's own edge, its scrollbar on the right, stays in reach.
const EDGE_OVER_PAGE: i32 = 3;

/// How long after the floating sidebar slid out the keyboard it gets for
/// being shown goes back to the page (Linux): the window manager hands it
/// over a moment later, after it mapped the window.
#[cfg(target_os = "linux")]
const QUIET: std::time::Duration = std::time::Duration::from_millis(500);

/// The floating sidebar's window and page.
pub(crate) struct FloatBar {
    pub window: Option<Window>,
    pub view: BrowserView,
    pub shown: bool,
    /// A finger brought it out (`touch_float_sidebar`).
    pub touched: bool,
    /// Until when it gives the keyboard back to the page (Linux,
    /// `float_sidebar_activated`).
    #[cfg(target_os = "linux")]
    pub quiet_until: Option<std::time::Instant>,
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
        let existing = self.lock().win.float_bar.as_ref().map(|f| f.window.clone());
        match existing {
            Some(Some(window)) => {
                if show {
                    window.set_bounds(Some(&bounds));
                    // Also called while it is out: it follows the window.
                    #[cfg(target_os = "linux")]
                    if window.is_visible() == 0 {
                        self.quiet_float_sidebar();
                    }
                    show_quietly(&window);
                    #[cfg(target_os = "linux")]
                    crate::panel_stack::keep_above(&window, self.top_page().as_ref(), false);
                } else {
                    window.hide();
                }
            }
            None if show => self.create_float_sidebar(bounds),
            // Window still being created (it shows itself when ready, if
            // `shown` says so), or nothing to hide.
            _ => {}
        }
        if let Some(f) = self.lock().win.float_bar.as_mut() {
            f.shown = show;
            f.touched &= show;
        }
    }

    /// A finger brought the floating sidebar out (the hover zone's page
    /// under Wayland). It takes the keyboard, and goes again when a page
    /// takes it back (`page_took_keyboard`): a finger leaves no pointer
    /// behind that could move out of it.
    pub fn touch_float_sidebar(&self) {
        self.show_float_sidebar(true);
        if let Some(f) = self.lock().win.float_bar.as_mut() {
            f.touched = f.shown;
            #[cfg(target_os = "linux")]
            {
                f.quiet_until = None;
            }
        }
        self.focus_float_sidebar();
    }

    /// A page of the current window got the keyboard: a floating sidebar a
    /// finger brought out goes.
    pub(crate) fn page_took_keyboard(&self) {
        let touched = (self.lock().win.float_bar.as_ref()).is_some_and(|f| f.shown && f.touched);
        if touched {
            self.show_float_sidebar(false);
        }
    }

    /// Whether the floating sidebar is out.
    pub fn float_sidebar_shown(&self) -> bool {
        self.lock().win.float_bar.as_ref().is_some_and(|f| f.shown)
    }

    /// The floating sidebar's page, once it exists.
    pub(crate) fn float_sidebar_browser(&self) -> Option<Browser> {
        self.lock()
            .win
            .float_bar
            .as_ref()
            .and_then(|f| f.view.browser())
    }

    /// Focus the floating sidebar (its pop-ups ask for focus).
    pub(crate) fn focus_float_sidebar(&self) {
        let handles = self
            .lock()
            .win
            .float_bar
            .as_ref()
            .and_then(|f| Some((f.window.clone()?, f.view.clone())));
        if let Some((window, view)) = handles {
            window.activate();
            view.request_focus();
        }
    }

    /// The floating sidebar is about to slide out on Linux: the keyboard it
    /// gets for being shown goes back to the page or panel that had it
    /// (`float_sidebar_activated`). Chromium under Wayland and the window
    /// manager under X11 give a window that is shown the keyboard; hovering
    /// is not choosing (`show_quietly`).
    #[cfg(target_os = "linux")]
    pub(crate) fn quiet_float_sidebar(&self) {
        if let Some(f) = self.lock().win.float_bar.as_mut() {
            f.quiet_until = Some(std::time::Instant::now() + QUIET);
        }
    }

    /// The floating sidebar's window got the keyboard: back to the page if
    /// that came from sliding out (`quiet_float_sidebar`), not from a click.
    /// To the page or panel that had it if it is on screen, else to the page
    /// in front.
    #[cfg(target_os = "linux")]
    fn float_sidebar_activated(&self) {
        let quiet = (self.lock().win.float_bar.as_mut())
            .and_then(|f| f.quiet_until.take())
            .is_some_and(|until| std::time::Instant::now() < until);
        if !quiet {
            return;
        }
        let had = self.lock().win.last_focused;
        let page = [had, self.top_page_id()]
            .into_iter()
            .flatten()
            .find_map(|id| {
                self.content_handles(id)
                    .filter(|(w, _)| w.is_visible() != 0)
            });
        if let Some((_, view)) = page {
            view.request_focus();
        }
    }

    /// The hover zone exists exactly while the docked sidebar is hidden, at
    /// the edge of the sidebar's side.
    pub(crate) fn update_edge_zone(&self) {
        let (frame, hidden) = (self.frame_window(), !self.sidebar_visible());
        let Some(frame) = frame else { return };
        let side = self.sidebar_side();
        let b = frame.bounds();
        let width = if self.content_inset() > 0 {
            EDGE
        } else {
            EDGE_OVER_PAGE
        };
        // Not beside the top bar: its ends hold the window's buttons.
        let top = self.top_bar_offset();
        let zone = Rect {
            x: match side {
                SidebarSide::Left => b.x,
                SidebarSide::Right => b.x + b.width - width,
            },
            y: b.y + top,
            width,
            height: (b.height - top).max(1),
        };
        #[cfg(target_os = "macos")]
        crate::mac::set_edge_zone(frame.window_handle(), hidden.then_some(&zone));
        #[cfg(target_os = "windows")]
        {
            let _ = zone;
            let at = hidden.then_some((width, top, side));
            crate::win::set_edge_zone(frame.window_handle(), at);
        }
        #[cfg(target_os = "linux")]
        self.set_edge_zone(&frame, hidden.then_some(&zone));
    }

    /// Along the edge of the sidebar's side, the window's height (below the
    /// top bar while it is on), the sidebar's width. Without the frame it
    /// reaches the window's edge, so the thin hover zone there lies under
    /// it and the pointer leaving the zone enters the panel, which hides
    /// when the pointer leaves it.
    fn float_bounds(&self) -> Option<Rect> {
        let frame = self.frame_window()?;
        let b = frame.bounds();
        let margin = if self.content_inset() > 0 { MARGIN } else { 0 };
        let top = self.top_bar_offset() + margin;
        let width = self.sidebar_width();
        Some(Rect {
            x: match self.sidebar_side() {
                SidebarSide::Left => b.x + margin,
                SidebarSide::Right => b.x + b.width - margin - width,
            },
            y: b.y + top,
            width,
            height: (b.height - top - margin).max(100),
        })
    }

    fn create_float_sidebar(&self, bounds: Rect) {
        let mut client = MagnetiteClient::overlay();
        let mut delegate = FloatViewDelegate::new();
        let Some(view) = browser_view_create(
            Some(&mut client),
            Some(&CefString::from("magnetite://ui/?float")),
            Some(&BrowserSettings {
                background_color: self.lock().win.chrome_color,
                ..Default::default()
            }),
            None,
            None,
            Some(&mut delegate),
        ) else {
            return;
        };
        self.lock().win.float_bar = Some(FloatBar {
            window: None,
            view: view.clone(),
            shown: true,
            touched: false,
            #[cfg(target_os = "linux")]
            quiet_until: Some(std::time::Instant::now() + QUIET),
        });
        let mut wd = FloatWindowDelegate::new(view, bounds);
        window_create_top_level(Some(&mut wd));
    }
}

/// Show the panel on top without taking the keyboard from the page: hovering
/// is not choosing. A click into the panel focuses it as usual, and the
/// pointer moves over it as over a focused window (hover, pointer-out).
pub(crate) fn show_quietly(window: &Window) {
    #[cfg(target_os = "macos")]
    {
        crate::mac::hover_without_focus(window.window_handle());
        crate::mac::without_taking_focus(|| {
            window.show();
            window.bring_to_top();
        });
    }
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
            #[cfg(target_os = "linux")]
            crate::cef_app::set_corner_radius(window, 10);
            #[cfg(target_os = "macos")]
            {
                crate::mac::mark_as_part(window.window_handle());
                crate::mac::no_key_loop_focus(window.window_handle());
            }
            let engine = Engine::global();
            let show = {
                let mut inner = engine.lock();
                let bar = inner.win.float_bar.as_mut();
                bar.map(|f| {
                    f.window = Some(window.clone());
                    f.shown
                })
            };
            if show == Some(true) {
                show_quietly(window);
                #[cfg(target_os = "linux")]
                crate::panel_stack::keep_above(window, engine.top_page().as_ref(), false);
            }
        }
        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            Engine::global().lock().win.float_bar = None;
        }
        fn on_window_activation_changed(&self, _window: Option<&mut Window>, active: ::std::os::raw::c_int) {
            // Not from inside the activation: the keyboard moves again.
            #[cfg(target_os = "linux")]
            if active != 0 {
                Engine::post(Box::new(|| Engine::global().float_sidebar_activated()));
            }
            #[cfg(not(target_os = "linux"))]
            let _ = active;
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
        // On Linux a window that cannot resize keeps its first size (see the
        // page windows' `can_resize`); the panel follows the frame's height.
        fn can_resize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { cfg!(target_os = "linux").into() }
        fn is_frameless(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 0 }
        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        // Not key, yet clickable at once: the first press is a real click
        // (macOS would otherwise spend it on activating the window).
        fn accepts_first_mouse(&self, _window: Option<&mut Window>) -> State { State::ENABLED }
        fn window_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
    }
}
