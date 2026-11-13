//! Short-lived overlay windows above the page area: a frameless child window
//! of the frame with an Alloy browser showing a `magnetite://ui` page. Used by
//! the capture tool (covering one page) and the command palette (centred,
//! sized to its content). At most one exists; it closes when its task is done.
//! The quick search is the palette in a window of its own (not the frame's
//! child), on the screen with the pointer and above other apps' windows.
//! The notification panel (`notifications.rs`) is built the same way but
//! has a slot of its own.

use cef::*;

use crate::client::MagnetiteClient;
use crate::engine::Engine;
use crate::events::ContentId;

impl Engine {
    /// Screen bounds (DIP) of page `id`'s window.
    pub fn content_bounds(&self, id: ContentId) -> Option<Rect> {
        Some(self.lock().contents.get(&id)?.window.clone()?.bounds())
    }

    /// Open `url` in an overlay exactly covering page `id`. Replaces an
    /// overlay that is already open.
    pub fn open_overlay(&self, over: ContentId, url: &str) -> bool {
        let Some(bounds) = self.content_bounds(over) else {
            return false;
        };
        self.open_overlay_with(bounds, url, 10.0, false, true)
    }

    /// Open `url` as a floating palette centred over the page area, `width`
    /// wide (narrower if the area is), starting `height` tall; the page can
    /// grow it with `resize_overlay`.
    pub fn open_palette(&self, url: &str, width: i32, height: i32) -> bool {
        let area = self.content_rect();
        let width = width.min(area.width - 48).max(320);
        // About a sixth down: where the eye already is, above the page's middle.
        let top = area.y + area.height / 6;
        let bounds = Rect {
            x: area.x + (area.width - width) / 2,
            y: top,
            width,
            height,
        };
        self.open_overlay_with(bounds, url, 14.0, true, true)
    }

    /// Open `url` as the quick search: a floating palette of its own, near
    /// the top of the screen with the pointer and above other apps' windows,
    /// `width` wide, starting `height` tall.
    pub fn open_quick_search(&self, url: &str, width: i32, height: i32) -> bool {
        let area = crate::mini::pointer_screen_area();
        let width = width.min(area.width - 48).max(320);
        let bounds = Rect {
            x: area.x + (area.width - width) / 2,
            y: area.y + area.height / 5,
            width,
            height,
        };
        #[cfg(target_os = "macos")]
        let outside = !crate::mac::app_is_active();
        #[cfg(not(target_os = "macos"))]
        let outside = false;
        self.drop_overlay();
        {
            let mut inner = self.lock();
            inner.overlay_detached = true;
            inner.quick_from_outside = outside;
        }
        self.create_overlay(bounds, url, 14.0, true, true, Slot::Overlay, true)
    }

    /// Change the open overlay's height (kept inside the page area, or for
    /// the quick search, inside its screen).
    pub fn resize_overlay(&self, height: i32) {
        let (window, detached) = {
            let inner = self.lock();
            (inner.overlay.clone(), inner.overlay_detached)
        };
        if let Some(w) = window {
            let mut b = w.bounds();
            let screen = || {
                display_get_nearest_point(Some(&Point { x: b.x, y: b.y }), 0).map(|d| d.work_area())
            };
            let area = if detached { screen() } else { None };
            let area = area.unwrap_or_else(|| self.content_rect());
            b.height = height.clamp(40, (area.y + area.height - b.y - 16).max(40));
            w.set_bounds(Some(&b));
        }
    }

    pub(crate) fn open_overlay_with(
        &self,
        bounds: Rect,
        url: &str,
        radius: f64,
        shadow: bool,
        activate: bool,
    ) -> bool {
        self.drop_overlay();
        self.lock().overlay_detached = false;
        self.create_overlay(bounds, url, radius, shadow, activate, Slot::Overlay, false)
    }

    /// Create a frameless window over the page area with our UI at `url`,
    /// filling `slot` once it exists; `detached`, a window of its own above
    /// other apps' windows instead of the frame's child.
    #[allow(clippy::unused_self, clippy::too_many_arguments)] // an Engine method like its siblings
    pub(crate) fn create_overlay(
        &self,
        bounds: Rect,
        url: &str,
        radius: f64,
        shadow: bool,
        activate: bool,
        slot: Slot,
        detached: bool,
    ) -> bool {
        let mut client = MagnetiteClient::overlay();
        let mut delegate = OverlayViewDelegate::new();
        let Some(view) = browser_view_create(
            Some(&mut client),
            Some(&CefString::from(url)),
            Some(&BrowserSettings {
                background_color: 0xFF00_0000,
                ..Default::default()
            }),
            None,
            None,
            Some(&mut delegate),
        ) else {
            return false;
        };
        let mut wd =
            OverlayWindowDelegate::new(view, bounds, radius, shadow, activate, slot, detached);
        window_create_top_level(Some(&mut wd));
        true
    }

    /// Close the overlay. A quick search opened from another app and
    /// closed without a pick gives that app the keyboard back.
    pub fn close_overlay(&self) {
        let back = {
            let mut inner = self.lock();
            let back =
                inner.overlay.is_some() && inner.overlay_detached && inner.quick_from_outside;
            inner.quick_from_outside = false;
            back
        };
        self.drop_overlay();
        #[cfg(target_os = "macos")]
        if back {
            crate::mac::deactivate_app();
        }
        #[cfg(not(target_os = "macos"))]
        let _ = back;
    }

    /// The keyboard went to a page while an overlay is open: its page is
    /// told with a `blur`, which closes the palette as a click elsewhere
    /// does on macOS. There the system sends it; Windows and Linux do not
    /// when the click goes to another window of ours.
    pub(crate) fn blur_overlay(&self) {
        if cfg!(target_os = "macos") {
            return;
        }
        let browser = (self.lock().overlay.as_ref())
            .and_then(|w| w.child_view_at(0)?.as_browser_view()?.browser());
        if let Some(frame) = browser.and_then(|b| b.main_frame()) {
            self.eval_in_ui(&frame, "window.dispatchEvent(new Event('blur'))");
        }
    }

    /// Close the overlay window, if one is open.
    fn drop_overlay(&self) {
        let window = self.lock().overlay.take();
        if let Some(w) = window {
            w.close();
        }
    }
}

wrap_browser_view_delegate! {
    struct OverlayViewDelegate {}
    impl ViewDelegate {}
    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
        fn on_popup_browser_view_created(
            &self,
            _browser_view: Option<&mut BrowserView>,
            _popup_browser_view: Option<&mut BrowserView>,
            _is_devtools: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            1
        }
    }
}

/// Where the engine keeps an overlay window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    /// Capture tool, palette, dialogs: one at a time.
    Overlay,
    /// The notification panel: never takes the keyboard.
    Notifications,
}

impl Slot {
    fn get(self, inner: &mut crate::engine::Inner) -> &mut Option<Window> {
        match self {
            Self::Overlay => &mut inner.overlay,
            Self::Notifications => &mut inner.notifications,
        }
    }
}

wrap_window_delegate! {
    struct OverlayWindowDelegate {
        view: BrowserView,
        bounds: Rect,
        radius: f64,
        shadow: bool,
        activate: bool,
        slot: Slot,
        detached: bool,
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
            crate::mac::round_window_corners(window.window_handle(), self.radius, self.shadow);
            #[cfg(target_os = "windows")]
            crate::win::round_window_corners(window.window_handle(), self.shadow);
            *self.slot.get(&mut Engine::global().lock()) = Some(window.clone());
            if self.slot == Slot::Notifications {
                crate::floatbar::show_quietly(window);
                return;
            }
            // The quick search shows over other apps' windows too.
            if self.detached {
                window.set_always_on_top(1);
            }
            window.show();
            // Key window, so Esc reaches it and a click elsewhere cancels.
            if self.activate {
                window.activate();
                self.view.request_focus();
            }
            // Above the pages: on Windows a new window of the frame is not
            // by itself above the page that had the keyboard (in a split,
            // the palette opened behind it). macOS orders child windows.
            #[cfg(not(target_os = "macos"))]
            window.bring_to_top();
        }
        // Another program came to the front: the palette's page is told
        // with a `blur`, as macOS does by itself. (A click into one of our
        // pages is told by `blur_overlay`.)
        fn on_window_activation_changed(&self, _window: Option<&mut Window>, active: ::std::os::raw::c_int) {
            if active != 0 || cfg!(target_os = "macos") {
                return;
            }
            if let Some(frame) = self.view.browser().and_then(|b| b.main_frame()) {
                Engine::global().eval_in_ui(&frame, "window.dispatchEvent(new Event('blur'))");
            }
        }
        fn on_window_destroyed(&self, window: Option<&mut Window>) {
            let engine = Engine::global();
            let mut inner = engine.lock();
            let slot = self.slot.get(&mut inner);
            if let (Some(w), Some(open)) = (window, slot.as_ref())
                && View::from(open).is_same(Some(&mut View::from(&*w))) != 0
            {
                *slot = None;
            }
        }
        fn parent_window(
            &self,
            _window: Option<&mut Window>,
            _is_menu: Option<&mut ::std::os::raw::c_int>,
            _can_activate_menu: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<Window> {
            if self.detached {
                return None;
            }
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
