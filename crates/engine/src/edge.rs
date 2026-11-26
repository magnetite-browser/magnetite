//! The hover zone along the frame's edge on the sidebar's side on Linux,
//! which slides the floating sidebar out while the docked one is hidden
//! (`floatbar.rs`). The two display systems need different things:
//!
//! - X11: as on Windows, a timer looks where the pointer is. Nothing lies
//!   over the pages, so the window manager has nothing to put in the wrong
//!   order, and the frame's resize border keeps working. A finger on a
//!   touch screen moves the pointer as well.
//! - Wayland: a program hears of the pointer only over its own windows. A
//!   slim see-through page of our UI (`magnetite://ui/edge`) lies over the
//!   zone in a window of its own, as on macOS, and says when the pointer or
//!   a finger arrives.

use std::cell::{Cell, RefCell};

use cef::*;

use crate::client::MagnetiteClient;
use crate::engine::{Engine, WindowId};

/// The zone's page under Wayland.
const EDGE_URL: &str = "magnetite://ui/edge";
/// How often the pointer is looked at under X11: quick enough to feel
/// immediate, as on Windows.
const EVERY_MS: i64 = 80;
/// Under Wayland the frame draws its own frame, and its outer points resize
/// it while it is neither maximized nor full screen (CEF's frameless
/// window); the zone's window leaves them to it.
const RESIZE_BORDER: i32 = 4;

/// The zone's window and page under Wayland, one per frame.
pub(crate) struct EdgeWindow {
    pub window: Option<Window>,
    pub view: BrowserView,
    /// Where it is to be (screen DIP); none while the docked sidebar shows.
    zone: Option<Rect>,
}

/// A frame whose zone is watched under X11.
struct Watched {
    win: WindowId,
    frame: Window,
    /// Screen DIP.
    zone: Rect,
    /// The pointer was in the zone at the last look: the sidebar slides out
    /// when the pointer comes in, not for as long as it stays.
    inside: bool,
}

thread_local! {
    static WATCHED: RefCell<Vec<Watched>> = const { RefCell::new(Vec::new()) };
    static LOOKING: Cell<bool> = const { Cell::new(false) };
}

impl Engine {
    /// Put the current window's hover zone, along the edge of `frame`, at
    /// `zone` (screen DIP), or remove it (`None`).
    pub(crate) fn set_edge_zone(&self, frame: &Window, zone: Option<&Rect>) {
        if crate::cef_app::runs_on_x11() {
            watch(self.current_window(), frame, zone);
            return;
        }
        // Over a page (no frame around the pages) the page covers the
        // border anyway.
        let border =
            self.content_inset() > 0 && frame.is_maximized() == 0 && frame.is_fullscreen() == 0;
        let zone = zone.map(|zone| {
            if border {
                clear_of_border(zone, &frame.bounds())
            } else {
                zone.clone()
            }
        });
        self.place_edge_window(zone.as_ref());
    }

    /// Pages or a web panel were raised (`show_content`, `show_split`,
    /// `show_web_panel`): what the hidden sidebar has over them goes back
    /// above them. Under Wayland that is the zone's window where it lies
    /// over the pages (no frame around them); and the floating sidebar while
    /// it is out, unless a finger brought it out: a page shown now takes the
    /// keyboard, which sends that one away anyway (`page_took_keyboard`).
    /// `reshown`: as for `panel_stack::keep_above`.
    pub(crate) fn raise_hidden_sidebar(&self, reshown: bool) {
        if reshown && !crate::cef_app::runs_on_x11() && self.content_inset() == 0 {
            let edge = (self.lock().win.edge.as_ref()).and_then(|e| e.window.clone());
            if let Some(window) = edge.filter(|w| w.is_visible() != 0) {
                crate::panel_stack::keep_above(&window, None, true);
            }
        }
        let float = (self.lock().win.float_bar.as_ref())
            .filter(|f| f.shown)
            .map(|f| (f.window.clone(), f.touched));
        match float {
            Some((_, true)) => self.show_float_sidebar(false),
            Some((Some(window), false)) => {
                // Shown again under Wayland, it would take the keyboard from
                // the page just shown.
                if reshown && !crate::cef_app::runs_on_x11() {
                    self.quiet_float_sidebar();
                }
                crate::panel_stack::keep_above(&window, self.top_page().as_ref(), reshown);
            }
            _ => {}
        }
    }

    /// Whether one of our windows is active: a frame, a floating sidebar, a
    /// page or panel, `DevTools`. Chromium follows that under any window
    /// manager, also one that names no active window (`_NET_ACTIVE_WINDOW`).
    fn ours_active(&self) -> bool {
        let windows: Vec<Window> = {
            let mut inner = self.lock();
            let frames: Vec<Window> = (inner.windows_mut())
                .flat_map(|(_, w)| {
                    let frame = w.frame.as_ref().map(|f| f.window.clone());
                    frame
                        .into_iter()
                        .chain(w.float_bar.as_ref().and_then(|f| f.window.clone()))
                })
                .collect();
            let pages = inner.contents.values().filter_map(|c| c.window.clone());
            let tools = inner.docked.values().filter_map(|d| d.window.clone());
            frames.into_iter().chain(pages).chain(tools).collect()
        };
        windows.iter().any(|w| w.is_active() != 0)
    }

    /// Under Wayland: the zone's window at `zone`, or hidden (`None`).
    fn place_edge_window(&self, zone: Option<&Rect>) {
        let existing = (self.lock().win.edge.as_mut()).map(|edge| {
            edge.zone = zone.cloned();
            edge.window.clone()
        });
        match (existing, zone) {
            (Some(Some(window)), Some(zone)) => {
                window.set_bounds(Some(zone));
                if window.is_visible() == 0 {
                    window.show();
                }
            }
            (Some(Some(window)), None) => window.hide(),
            (None, Some(zone)) => self.create_edge_window(zone),
            // Still being made (it shows itself when ready if `zone` still
            // says so), or nothing to hide.
            _ => {}
        }
    }

    /// See-through where the CEF in use can do it (`cef_app::clear_pages`),
    /// as macOS's invisible window is; else in the colour the frame has now
    /// (the CEF releases with Wayland support can).
    fn create_edge_window(&self, zone: &Rect) {
        let background = if crate::cef_app::clear_pages() {
            crate::cef_app::CLEAR_BACKGROUND
        } else {
            self.lock().win.chrome_color
        };
        let Some(view) = browser_view_create(
            Some(&mut MagnetiteClient::overlay()),
            Some(&CefString::from(EDGE_URL)),
            Some(&BrowserSettings {
                background_color: background,
                ..Default::default()
            }),
            None,
            None,
            Some(&mut EdgeViewDelegate::new()),
        ) else {
            return;
        };
        self.lock().win.edge = Some(EdgeWindow {
            window: None,
            view: view.clone(),
            zone: Some(zone.clone()),
        });
        let mut delegate = EdgeWindowDelegate::new(view, zone.clone(), self.current_window());
        window_create_top_level(Some(&mut delegate));
    }
}

/// Watch `frame`'s zone (window `win`) under X11, or stop (`None`). A
/// pointer already resting in a new zone counts once it leaves and comes
/// back, as under Wayland (`Edge.tsx`).
fn watch(win: WindowId, frame: &Window, zone: Option<&Rect>) {
    WATCHED.with_borrow_mut(|watched| {
        let before =
            (watched.iter().position(|w| w.win == win)).map(|at| watched.swap_remove(at).inside);
        if let Some(zone) = zone {
            let inside = before.unwrap_or_else(|| pointer_in(frame, zone));
            watched.push(Watched {
                win,
                frame: frame.clone(),
                zone: zone.clone(),
                inside,
            });
        }
    });
    if zone.is_some() && !LOOKING.replace(true) {
        Engine::post_delayed(Box::new(look), EVERY_MS);
    }
}

/// One look at the pointer; again in `EVERY_MS` while a zone is watched.
/// Only the zone of the window the user works in (the current one) counts,
/// as on the other systems, which have one zone: another window of ours may
/// lie over this one's edge. And only while one of our windows is active:
/// another program's window may lie over it (`ours_active`).
fn look() {
    let engine = Engine::global();
    let current = engine.current_window();
    let arrived = WATCHED.with_borrow_mut(|watched| {
        watched.retain(|w| w.frame.is_valid() != 0);
        // Asked once a look, and only with the pointer in a zone.
        let mut ours = None;
        let mut arrived = Vec::new();
        for w in watched.iter_mut() {
            let inside = w.win == current
                && pointer_in(&w.frame, &w.zone)
                && *ours.get_or_insert_with(|| engine.ours_active());
            if inside && !w.inside {
                arrived.push(w.win);
            }
            w.inside = inside;
        }
        arrived
    });
    for win in arrived {
        engine.in_window(win, || engine.show_float_sidebar(true));
    }
    if WATCHED.with_borrow(Vec::is_empty) {
        LOOKING.set(false);
    } else {
        Engine::post_delayed(Box::new(look), EVERY_MS);
    }
}

/// Whether the pointer is in `zone` of `frame`, a frame on screen.
fn pointer_in(frame: &Window, zone: &Rect) -> bool {
    if frame.is_visible() == 0 || frame.is_minimized() != 0 {
        return false;
    }
    crate::x11::pointer_over(frame.window_handle())
        .is_some_and(|(at, width)| in_zone(at, width, &frame.bounds(), zone))
}

/// Whether the pointer, `at` pixels from the top left corner of a frame
/// `width` pixels wide whose bounds are `frame`, is in `zone`. Both
/// rectangles in screen DIP.
fn in_zone(at: (i32, i32), width: u32, frame: &Rect, zone: &Rect) -> bool {
    if frame.width <= 0 || width == 0 {
        return false;
    }
    let scale = f64::from(width) / f64::from(frame.width);
    let x = f64::from(frame.x) + f64::from(at.0) / scale;
    let y = f64::from(frame.y) + f64::from(at.1) / scale;
    (f64::from(zone.x)..f64::from(zone.x + zone.width)).contains(&x)
        && (f64::from(zone.y)..f64::from(zone.y + zone.height)).contains(&y)
}

/// `zone` without the frame's resize border (`RESIZE_BORDER`), for a frame
/// whose bounds are `frame`. Both in screen DIP.
fn clear_of_border(zone: &Rect, frame: &Rect) -> Rect {
    let left = zone.x.max(frame.x + RESIZE_BORDER);
    let right = (zone.x + zone.width).min(frame.x + frame.width - RESIZE_BORDER);
    let top = zone.y.max(frame.y + RESIZE_BORDER);
    let bottom = (zone.y + zone.height).min(frame.y + frame.height - RESIZE_BORDER);
    Rect {
        x: left,
        y: top,
        width: (right - left).max(1),
        height: (bottom - top).max(1),
    }
}

wrap_browser_view_delegate! {
    struct EdgeViewDelegate {}
    impl ViewDelegate {
        // As the window's (below): CEF paints the view in the theme's colour
        // after each theme change.
        fn on_theme_changed(&self, view: Option<&mut View>) {
            if let Some(view) = view
                && crate::cef_app::clear_pages()
            {
                view.set_background_color(0);
            }
        }
    }
    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
    }
}

wrap_window_delegate! {
    struct EdgeWindowDelegate {
        view: BrowserView,
        bounds: Rect,
        win: WindowId,
    }
    impl ViewDelegate {
        // CEF paints a window in the theme's colour after each theme change:
        // see-through again, as the page in it is.
        fn on_theme_changed(&self, view: Option<&mut View>) {
            if let Some(view) = view
                && crate::cef_app::clear_pages()
            {
                view.set_background_color(0);
            }
        }
    }
    impl PanelDelegate {}
    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let Some(window) = window else { return };
            window.set_to_fill_layout();
            let mut v = View::from(&self.view);
            window.add_child_view(Some(&mut v));
            let engine = Engine::global();
            let zone = engine.in_window(self.win, || {
                let mut inner = engine.lock();
                let edge = inner.win.edge.as_mut()?;
                edge.window = Some(window.clone());
                edge.zone.clone()
            });
            if let Some(zone) = zone.flatten() {
                window.set_bounds(Some(&zone));
                window.show();
            }
        }
        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            let engine = Engine::global();
            engine.in_window(self.win, || engine.lock().win.edge = None);
        }
        fn parent_window(
            &self,
            _window: Option<&mut Window>,
            _is_menu: Option<&mut ::std::os::raw::c_int>,
            _can_activate_menu: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<Window> {
            let engine = Engine::global();
            engine.in_window(self.win, || engine.frame_window()).flatten()
        }
        fn initial_bounds(&self, _window: Option<&mut Window>) -> Rect { self.bounds.clone() }
        // Else its first size would be its only one (see the page windows'
        // `can_resize`): the zone changes with the frame's height and frame.
        fn can_resize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn is_frameless(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 0 }
        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn window_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::ALLOY }
    }
}

#[cfg(test)]
mod tests {
    use super::{clear_of_border, in_zone};
    use cef::Rect;

    const fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn the_pointer_is_measured_from_the_frame_in_its_pixels() {
        let frame = rect(100, 50, 800, 600);
        let zone = rect(100, 90, 8, 560);
        assert!(in_zone((0, 40), 800, &frame, &zone));
        assert!(in_zone((7, 599), 800, &frame, &zone));
        // Beside it, above it, below the frame.
        assert!(!in_zone((8, 300), 800, &frame, &zone));
        assert!(!in_zone((0, 39), 800, &frame, &zone));
        assert!(!in_zone((0, 600), 800, &frame, &zone));
    }

    #[test]
    fn at_twice_the_scale_the_zone_is_twice_as_many_pixels_wide() {
        let frame = rect(100, 50, 800, 600);
        let zone = rect(100, 50, 8, 600);
        assert!(in_zone((15, 10), 1600, &frame, &zone));
        assert!(!in_zone((16, 10), 1600, &frame, &zone));
    }

    #[test]
    fn with_the_sidebar_on_the_right_the_zone_is_the_right_edge() {
        let frame = rect(100, 50, 800, 600);
        let zone = rect(897, 50, 3, 600);
        assert!(in_zone((797, 10), 800, &frame, &zone));
        assert!(in_zone((799, 10), 800, &frame, &zone));
        assert!(!in_zone((796, 10), 800, &frame, &zone));
        assert!(!in_zone((0, 10), 800, &frame, &zone));
    }

    #[test]
    fn the_zones_window_leaves_the_frames_resize_border_free() {
        let frame = rect(100, 50, 800, 600);
        let parts = |r: Rect| (r.x, r.y, r.width, r.height);
        // Left, without the top bar: in from the side, the top and the bottom.
        let left = clear_of_border(&rect(100, 50, 8, 600), &frame);
        assert_eq!(parts(left), (104, 54, 4, 592));
        // Right, below a top bar: only the side and the bottom.
        let right = clear_of_border(&rect(892, 92, 8, 558), &frame);
        assert_eq!(parts(right), (892, 92, 4, 554));
    }
}
