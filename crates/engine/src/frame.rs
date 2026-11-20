//! The frame window: the top bar view (when on) above the body, which holds
//! the sidebar view and, right of it, the developer bar (while the page in
//! front has developer tools) above the content panel.

use cef::*;
use serde::{Deserialize, Serialize};

use crate::client::MagnetiteClient;
use crate::engine::{Engine, WindowId, WindowState};
use crate::events::{ContentId, EngineEvent};

/// When the frame shows around the pages: the gap to the window's edges and
/// the sidebar, and the pages' rounded corners. Without it a page reaches
/// the window's edges, with square corners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageFrame {
    #[default]
    Always,
    /// Not while the window is full screen.
    Windowed,
    Never,
}

impl PageFrame {
    /// Whether a window that is `fullscreen` or not has the frame.
    fn shows(self, fullscreen: bool) -> bool {
        match self {
            Self::Always => true,
            Self::Windowed => !fullscreen,
            Self::Never => false,
        }
    }
}

pub(crate) struct FrameState {
    pub window: Window,
    /// The row below the top bar: sidebar view and content panel.
    pub body: Panel,
    pub sidebar_view: BrowserView,
    /// The top bar's view, made the first time it is turned on.
    pub top_bar_view: Option<BrowserView>,
    pub top_bar_browser: Option<Browser>,
    /// Right of the sidebar: the developer bar above the content panel.
    pub pages_column: Panel,
    /// The developer bar's view, made the first time it shows.
    pub dev_bar_view: Option<BrowserView>,
    pub dev_bar_browser: Option<Browser>,
    pub content_panel: Panel,
    pub sidebar_browser: Option<Browser>,
    /// The hint in the content panel for a window without a page.
    pub empty: Option<crate::empty::EmptyPage>,
    /// The window's own buttons in a row at the top of the pages' column,
    /// where the system draws none (`caption.rs`).
    pub caption: Option<crate::caption::Caption>,
}

impl Engine {
    /// Create a frame window: the first one after `EngineEvent::Ready`, any
    /// further one as a new window, which is the current one from here on.
    /// `EngineEvent::FrameCreated` follows.
    pub fn create_frame(&self) -> WindowId {
        let win = {
            let mut inner = self.lock();
            // The first window is there from the start, without its frame.
            if inner.win.frame.is_some() {
                let id = inner.next_window;
                inner.next_window += 1;
                let before = std::mem::replace(&mut inner.win, WindowState::new());
                let before_id = inner.set_current(self, id);
                inner.parked.insert(before_id, before);
            }
            inner.current
        };
        let mut delegate = FrameWindowDelegate::new(win);
        window_create_top_level(Some(&mut delegate));
        win
    }

    /// The window every call that names none works on: the one the user
    /// used last, unless `enter` chose another.
    pub fn current_window(&self) -> WindowId {
        self.current.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Every window, in the order they were made.
    pub fn windows(&self) -> Vec<WindowId> {
        let mut inner = self.lock();
        let mut ids: Vec<WindowId> = inner.windows_mut().map(|(id, _)| id).collect();
        ids.sort_unstable();
        ids
    }

    /// Make `win` the current window. False if there is no such window.
    pub fn enter(&self, win: WindowId) -> bool {
        let mut inner = self.lock();
        if inner.current == win {
            return true;
        }
        let Some(next) = inner.parked.remove(&win) else {
            return false;
        };
        let before = std::mem::replace(&mut inner.win, next);
        let before_id = inner.set_current(self, win);
        inner.parked.insert(before_id, before);
        true
    }

    /// Run `f` with `win` as the current window, then go back to the window
    /// that was current, unless `f` itself moved on to another one. `None`
    /// if there is no such window. Never call it with the engine locked.
    pub fn in_window<R>(&self, win: WindowId, f: impl FnOnce() -> R) -> Option<R> {
        let before = self.current_window();
        if !self.enter(win) {
            return None;
        }
        let result = f();
        if self.current_window() == win {
            self.enter(before);
        }
        Some(result)
    }

    /// The window whose sidebar, top bar, floating sidebar or empty page is browser
    /// `id`, or whose page it is.
    pub(crate) fn window_of_browser(&self, id: i32) -> Option<WindowId> {
        let is = |b: Option<&Browser>| b.is_some_and(|b| b.identifier() == id);
        let mut inner = self.lock();
        if let Some(home) = (inner.contents.values())
            .find(|c| is(c.browser.as_ref()))
            .map(|c| c.home)
        {
            return Some(home);
        }
        inner.windows_mut().find_map(|(win, w)| {
            let frame = w.frame.as_ref();
            let float = w.float_bar.as_ref().and_then(|f| f.view.browser());
            let empty = frame.and_then(|f| f.empty.as_ref()?.browser());
            (is(frame.and_then(|f| f.sidebar_browser.as_ref()))
                || is(empty.as_ref())
                || is(frame.and_then(|f| f.top_bar_browser.as_ref()))
                || is(frame.and_then(|f| f.dev_bar_browser.as_ref()))
                || is(float.as_ref()))
            .then_some(win)
        })
    }

    /// Close window `win`; its pages stay loaded in another one where they
    /// can move (`leave_window`). The last window does not close this way:
    /// it hides on macOS (`hide_frame`) and asks to quit elsewhere.
    pub fn close_window(&self, win: WindowId) {
        let frame = {
            let inner = self.lock();
            if inner.parked.is_empty() {
                return;
            }
            (inner.window(win))
                .and_then(|w| w.frame.as_ref())
                .map(|f| f.window.clone())
        };
        if let Some(frame) = frame {
            frame.close();
        }
    }

    /// Gap between the page and the frame edges / sidebar, in DIP, while
    /// the frame around the pages shows (`PageFrame`).
    pub const CONTENT_INSET: i32 = 8;
    /// Corner radius of a page inside the frame, in DIP.
    pub(crate) const PAGE_RADIUS: i32 = 10;
    /// How far the window-drag strip reaches into the top of the pages.
    const GRAB_OVERLAP: i32 = 6;
    /// Height of the top bar (`magnetite://ui/topbar`).
    pub const TOP_BAR_HEIGHT: i32 = 42;
    /// Height of the developer bar (`magnetite://ui/devbar`).
    pub const DEV_BAR_HEIGHT: i32 = 36;

    /// Keep the window-drag strip along the top of the page area, above the
    /// page windows (see `mac::set_grab_strip`).
    pub(crate) fn update_grab_strip(&self) {
        let Some(window) = self.frame_window() else {
            return;
        };
        let panel = self.content_panel_in_screen();
        let strip = Rect {
            height: self.content_inset() + Self::GRAB_OVERLAP,
            ..panel
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
    /// panel minus an inset so the page floats inside the frame, and
    /// without what a docked web panel takes (`WEB_PANEL_DOCKED`). With the
    /// sidebar hidden and no top bar the pages cover the window's own
    /// buttons: the floating sidebar has them then.
    pub fn content_rect(&self) -> Rect {
        let area = self.page_area();
        // Its own statement: `page_area` locks too.
        let docked = {
            let inner = self.lock();
            (inner.win.web_panel).map(|_| inner.win.web_panel_width)
        };
        match docked.filter(|_| crate::content::WEB_PANEL_DOCKED) {
            Some(width) => crate::content::beside_web_panel(&area, width),
            None => area,
        }
    }

    /// `content_rect` with the room a docked web panel takes: the whole
    /// page area, which the panel is placed in.
    pub(crate) fn page_area(&self) -> Rect {
        self.inset(&self.content_panel_in_screen())
    }

    /// Whether the current window has the frame around its pages.
    pub(crate) fn page_frame_shows(inner: &crate::engine::Inner) -> bool {
        let fullscreen = (inner.win.frame.as_ref()).is_some_and(|f| f.window.is_fullscreen() != 0);
        inner.page_frame.shows(fullscreen)
    }

    /// The gap around the current window's pages: none without the frame.
    fn content_inset(&self) -> i32 {
        if Self::page_frame_shows(&self.lock()) {
            Self::CONTENT_INSET
        } else {
            0
        }
    }

    /// The corner radius page `id` has where it is placed now: square
    /// without the frame, unless it floats over the pages (a peek, a web
    /// panel that is not docked).
    pub(crate) fn corner_radius(&self, id: ContentId) -> i32 {
        let inner = self.lock();
        let floats = inner.win.peek == Some(id)
            || (!crate::content::WEB_PANEL_DOCKED && inner.win.web_panel == Some(id));
        if floats || Self::page_frame_shows(&inner) {
            Self::PAGE_RADIUS
        } else {
            0
        }
    }

    /// Which corners of `rect` (on screen) lie in a corner of the current
    /// window: top left, top right, bottom left, bottom right. None in full
    /// screen, where the window has no round corners.
    #[cfg(target_os = "macos")]
    pub(crate) fn in_window_corner(&self, rect: &Rect) -> [bool; 4] {
        let Some(window) = self.frame_window().filter(|w| w.is_fullscreen() == 0) else {
            return [false; 4];
        };
        let b = window.bounds();
        let (left, top) = (rect.x == b.x, rect.y == b.y);
        let right = rect.x + rect.width == b.x + b.width;
        let bottom = rect.y + rect.height == b.y + b.height;
        [left && top, right && top, left && bottom, right && bottom]
    }

    /// When the frame around the pages shows, in every window.
    pub fn set_page_frame(&self, frame: PageFrame) {
        if std::mem::replace(&mut self.lock().page_frame, frame) == frame {
            return;
        }
        for win in self.windows() {
            self.in_window(win, || self.apply_page_frame());
        }
        // Windows has one outline for all windows, and the last window gone
        // through took it: it is the current window's.
        let focus = self.lock().win.focused_pane;
        self.update_focus_ring(focus);
    }

    /// The frame around the current window's pages came or went (the
    /// setting, full screen): the pages take their new place and corners.
    fn apply_page_frame(&self) {
        self.reposition_contents();
        self.apply_drag_regions();
        // The outline of a split's focused page has the page's corners.
        let focus = self.lock().win.focused_pane;
        self.update_focus_ring(focus);
    }

    /// The content panel `r` without the frame's inset around the pages.
    fn inset(&self, r: &Rect) -> Rect {
        let i = self.content_inset();
        Rect {
            x: r.x + i,
            y: r.y + i,
            width: (r.width - 2 * i).max(1),
            height: (r.height - 2 * i).max(1),
        }
    }

    /// What the page behind the tabs shows beside a docked web panel, in
    /// the window (`Empty.tsx`): the gap beyond it, the grip to resize it,
    /// and the button to close it, in reach while no page lies over it.
    fn docked_controls_in_window(&self) -> Option<[Rect; 2]> {
        let width = {
            let inner = self.lock();
            inner
                .win
                .web_panel
                .filter(|_| crate::content::WEB_PANEL_DOCKED)?;
            inner.win.web_panel_width
        };
        let area = self.inset(&self.content_panel_in_window());
        let panel = crate::content::web_panel_rect(&area, width);
        let grip = Rect {
            x: panel.x + panel.width,
            width: crate::content::SPLIT_GAP,
            ..panel.clone()
        };
        let side = i32::from(crate::panel_close::SIZE);
        let button = Rect {
            x: grip.x + grip.width + crate::panel_close::MARGIN,
            y: panel.y + crate::panel_close::MARGIN,
            width: side,
            height: side,
        };
        Some([grip, button])
    }

    /// A docked web panel opened, closed or changed its width: the pages
    /// take the room beside it, and the gap between them is the grip.
    pub(crate) fn dock_web_panel(&self) {
        self.reposition_contents();
        self.apply_drag_regions();
    }

    /// Draggable areas of the frame: the top bar's and the sidebar page's
    /// `app-region: drag` zones plus the whole content panel. Pages sit in
    /// child windows above the panel, so only the gaps around them (top
    /// edge, sides, split gutters) end up moving the window. Not the gap
    /// and the button beside a docked web panel: a drag in the gap resizes
    /// the panel, a click on the button closes it.
    pub(crate) fn apply_drag_regions(&self) {
        let (window, sidebar, top_bar) = {
            let inner = self.lock();
            let Some(f) = inner.win.frame.as_ref() else {
                return;
            };
            let sidebar = if inner.win.sidebar_visible {
                inner.win.sidebar_drag.clone()
            } else {
                Vec::new()
            };
            let top_bar = if inner.top_bar_visible {
                inner.win.top_bar_drag.clone()
            } else {
                Vec::new()
            };
            (f.window.clone(), sidebar, top_bar)
        };
        let caption = Self::caption_height(&self.lock());
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
        if caption > 0 {
            // The row of our own window buttons, at the window's top above
            // the pages: it drags the window, the buttons take clicks.
            let panel = self.content_panel_in_window();
            let row = Rect {
                y: 0,
                height: caption,
                ..panel
            };
            let width = crate::caption::Caption::BUTTONS_WIDTH.min(row.width);
            let buttons = Rect {
                x: row.x + row.width - width,
                width,
                ..row.clone()
            };
            regions.push(DraggableRegion {
                bounds: row,
                draggable: 1,
            });
            regions.push(DraggableRegion {
                bounds: buttons,
                draggable: 0,
            });
        }
        let controls = self.docked_controls_in_window().into_iter().flatten();
        regions.extend(controls.map(|bounds| DraggableRegion {
            bounds,
            draggable: 0,
        }));
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
    /// the top bar and the developer bar. Worked out from the window's size, not asked of the
    /// panel: Chromium lays out the nested body lazily, so the panel can
    /// still report where it was before a change.
    fn content_panel_in_window(&self) -> Rect {
        let inner = self.lock();
        let Some(f) = inner.win.frame.as_ref() else {
            return Rect::default();
        };
        let client = f.window.bounds();
        let x = if inner.win.sidebar_visible {
            inner.sidebar_width
        } else {
            0
        };
        let top_bar = if inner.top_bar_visible {
            Self::TOP_BAR_HEIGHT
        } else {
            0
        };
        let dev_bar = if inner.win.dev_bar_visible {
            Self::DEV_BAR_HEIGHT
        } else {
            0
        };
        let y = top_bar + Self::caption_height(&inner) + dev_bar;
        Rect {
            x,
            y,
            width: (client.width - x).max(1),
            height: (client.height - y).max(1),
        }
    }

    /// The height of the row of our own window buttons above the current
    /// window's pages (`caption.rs`), 0 without one: a full screen window
    /// has none, and the top bar has the buttons itself.
    fn caption_height(inner: &crate::engine::Inner) -> i32 {
        let row = (inner.win.frame.as_ref())
            .is_some_and(|f| f.caption.is_some() && f.window.is_fullscreen() == 0);
        if row && !inner.top_bar_visible {
            crate::caption::Caption::HEIGHT
        } else {
            0
        }
    }

    /// The buttons' row follows the window: maximised or not, and whether
    /// it shows at all (`caption_height`). The callers place the pages anew.
    pub(crate) fn sync_caption(&self) {
        // Not under the lock: a new layout asks the views' delegates,
        // which ask the engine.
        let frame = {
            let inner = self.lock();
            let shown = Self::caption_height(&inner) > 0;
            (inner.win.frame.as_ref())
                .and_then(|f| Some((f.window.clone(), f.caption.clone()?, shown)))
        };
        if let Some((window, caption, shown)) = frame {
            caption.sync(window.is_maximized() != 0, shown);
        }
    }

    /// `content_panel_in_window` in screen coordinates.
    pub(crate) fn content_panel_in_screen(&self) -> Rect {
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
        self.lock().top_bar_visible = visible;
        // One setting for every window.
        for win in self.windows() {
            self.in_window(win, || self.show_top_bar(visible));
        }
    }

    /// The current window's top bar follows the setting.
    pub(crate) fn show_top_bar(&self, visible: bool) {
        let frame = {
            let inner = self.lock();
            inner
                .win
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
                let Some(view) = create_bar_view("topbar", Self::TOP_BAR_HEIGHT) else {
                    return;
                };
                // Before the view is added: its browser is matched to its
                // window by it (`on_browser_created`).
                if let Some(f) = self.lock().win.frame.as_mut() {
                    f.top_bar_view = Some(view.clone());
                }
                window.add_child_view_at(Some(&mut View::from(&view)), 0);
            }
            None => {}
        }
        // The top bar has the window's buttons: our row above the pages goes.
        self.sync_caption();
        window.layout();
        self.reposition_contents();
        self.apply_drag_regions();
        self.update_edge_zone();
        self.align_hidden_toolbars();
        // A floating sidebar that is out moves below the bar, or back up.
        if self.float_sidebar_shown() {
            self.show_float_sidebar(true);
        }
    }

    /// Show the developer bar above the current window's pages, or hide it.
    /// Its page is made the first time. The pages move down by its height.
    pub fn set_dev_bar_visible(&self, visible: bool) {
        let frame = {
            let mut inner = self.lock();
            if std::mem::replace(&mut inner.win.dev_bar_visible, visible) == visible {
                return;
            }
            let frame = inner.win.frame.as_ref();
            frame.map(|f| {
                (
                    f.window.clone(),
                    f.pages_column.clone(),
                    f.dev_bar_view.clone(),
                    f.caption.is_some(),
                )
            })
        };
        let Some((window, column, view, caption)) = frame else {
            return;
        };
        match view {
            Some(view) => View::from(&view).set_visible(visible.into()),
            None if visible => {
                let Some(view) = create_bar_view("devbar", Self::DEV_BAR_HEIGHT) else {
                    return;
                };
                // Before the view is added, as for the top bar.
                if let Some(f) = self.lock().win.frame.as_mut() {
                    f.dev_bar_view = Some(view.clone());
                }
                // Below the row of the window's buttons, if there is one.
                column.add_child_view_at(Some(&mut View::from(&view)), caption.into());
            }
            None => {}
        }
        window.layout();
        self.reposition_contents();
        self.apply_drag_regions();
        self.update_edge_zone();
    }

    /// The window buttons of the top bar (Linux and Windows) and of the
    /// floating sidebar (macOS, where the pages cover the window's own).
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

    /// Enter full screen, or leave it.
    pub fn toggle_fullscreen_frame(&self) {
        if let Some(w) = self.frame_window() {
            w.set_fullscreen((w.is_fullscreen() == 0).into());
        }
    }

    /// Close the frame window, as the window manager's close does: the last
    /// one hides on macOS and asks to quit elsewhere (`can_close`).
    pub fn close_frame(&self) {
        if let Some(w) = self.frame_window() {
            w.close();
        }
    }

    /// The last window was closed: on macOS it only goes out of sight, with
    /// its pages as they are, and Magnetite runs on without a window until
    /// `show_frame`. `EngineEvent::FrameHidden` follows.
    #[cfg(target_os = "macos")]
    fn hide_frame(&self) {
        let Some(window) = self.frame_window() else {
            return;
        };
        self.lock().frame_hidden = true;
        window.hide();
        self.emit(EngineEvent::FrameHidden);
    }

    /// Whether the last window is closed while Magnetite runs on.
    pub fn frame_hidden(&self) -> bool {
        self.lock().frame_hidden
    }

    /// Show the frame window, also after its close hid it (`hide_frame`).
    pub fn show_frame(&self) {
        self.lock().frame_hidden = false;
        if let Some(w) = self.frame_window() {
            w.show();
        }
    }

    /// Open `url` in an overlay below the top bar (the developer bar with
    /// `dev`), `width` by `height`, its top-left at (`x`, `y`) in that
    /// bar's page's coordinates: the bars' pop-ups (site controls, menus,
    /// panels), which their short pages cannot hold.
    pub fn open_bar_popup(
        &self,
        dev: bool,
        url: &str,
        (x, y): (i32, i32),
        (width, height): (i32, i32),
    ) -> bool {
        let Some(bar) = (self.lock().win.frame.as_ref()).and_then(|f| {
            let view = if dev {
                &f.dev_bar_view
            } else {
                &f.top_bar_view
            };
            view.as_ref().map(|v| View::from(v).bounds_in_screen())
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
            inner.win.sidebar_visible = visible;
            inner
                .win
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
        {
            let mut inner = self.lock();
            if inner.sidebar_width == width {
                return;
            }
            inner.sidebar_width = width;
        }
        // One width for every window.
        for win in self.windows() {
            self.in_window(win, || {
                let frame = (self.lock().win.frame.as_ref())
                    .map(|f| (f.window.clone(), f.sidebar_view.clone()));
                if let Some((window, sidebar)) = frame {
                    // The box layout caches the sidebar's preferred width.
                    View::from(&sidebar).invalidate_layout();
                    window.layout();
                    self.reposition_contents();
                    self.apply_drag_regions();
                }
            });
        }
    }

    /// Colour of the chrome behind the page cards (ARGB). The sidebar page
    /// paints the same colour so there is no seam.
    pub fn set_chrome_color(&self, argb: u32) {
        self.lock().win.chrome_color = argb;
        let panels = (self.lock().win.frame.as_ref()).map(|f| {
            [
                f.content_panel.clone(),
                f.body.clone(),
                f.pages_column.clone(),
            ]
        });
        for panel in panels.into_iter().flatten() {
            panel.set_background_color(argb);
        }
        // Not under the lock: the buttons' callbacks read the colour.
        let caption = (self.lock().win.frame.as_ref()).and_then(|f| f.caption.clone());
        if let Some(caption) = caption {
            caption.set_background(argb);
        }
        // A page window's own frame shows at its corners.
        #[cfg(target_os = "windows")]
        {
            let pages: Vec<_> = {
                let inner = self.lock();
                let pages = inner.contents.values().filter_map(|c| c.window.clone());
                let tools = inner.docked.values().filter_map(|d| d.window.clone());
                pages.chain(tools).collect()
            };
            crate::win::set_frame_color(argb, pages.iter().map(ImplWindow::window_handle));
        }
    }

    pub fn sidebar_visible(&self) -> bool {
        self.lock().win.sidebar_visible
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
            .win
            .frame
            .as_ref()
            .map(|f| (f.window.clone(), f.sidebar_view.clone()));
        if let Some((window, view)) = handles {
            window.activate();
            view.request_focus();
            if let Some(b) = self
                .lock()
                .win
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
        let (split, peek, panel, panel_width) = {
            let inner = self.lock();
            let win = &inner.win;
            (
                win.split.clone(),
                win.peek,
                win.web_panel,
                win.web_panel_width,
            )
        };
        if let Some(p) = panel {
            let rect = crate::content::web_panel_rect(&self.page_area(), panel_width);
            self.place(p, &rect);
        }
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
        // Mini windows are not in the frame; the other pages are in the
        // window they live in.
        let current = self.current_window();
        let ids: Vec<ContentId> = (self.lock().contents.iter())
            .filter(|(_, c)| c.mini.is_none() && c.home == current)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if Some(id) != peek && Some(id) != panel {
                self.place(id, &rect);
            }
        }
        if panel.is_some() && !crate::content::WEB_PANEL_DOCKED {
            // The panel moved or changed its width: its grip and button
            // follow.
            self.show_dividers();
        }
        self.sync_panel_close();
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
/// The view of a bar of Magnetite's own UI at `path` (`topbar`, `devbar`),
/// `height` points high.
fn create_bar_view(path: &str, height: i32) -> Option<BrowserView> {
    let engine = Engine::global();
    let mut client = MagnetiteClient::top_bar();
    let mut delegate = BarViewDelegate::new(height);
    let settings = BrowserSettings {
        background_color: engine.lock().win.chrome_color,
        ..Default::default()
    };
    // Next to the sidebar's page, also on a development server.
    let url = format!("{}/{path}", engine.config().ui_url.trim_end_matches('/'));
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
    struct BarViewDelegate {
        height: i32,
    }
    impl ViewDelegate {
        // A size with no width counts as none: CEF would use the page's own.
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 1, height: self.height }
        }
        fn minimum_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 1, height: self.height }
        }
        fn maximum_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: i32::MAX, height: self.height }
        }
        // A column layout asks this, not the preferred size.
        fn height_for_width(&self, _view: Option<&mut View>, _width: ::std::os::raw::c_int) -> ::std::os::raw::c_int {
            self.height
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
            // No popups from a bar's page.
            1
        }
    }
}

wrap_panel_delegate! {
    struct BodyPanelDelegate {}
    impl ViewDelegate {}
    impl PanelDelegate {}
}

/// The smallest page area the frame can be resized to.
const MIN_PAGE: Size = Size {
    width: 320,
    height: 240,
};

wrap_panel_delegate! {
    struct ContentPanelDelegate {}
    impl ViewDelegate {
        fn minimum_size(&self, _view: Option<&mut View>) -> Size { MIN_PAGE }
    }
    impl PanelDelegate {}
}

wrap_window_delegate! {
    struct FrameWindowDelegate {
        win: WindowId,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size { Size { width: 1280, height: 860 } }
        // The pages are windows of their own: in a frame narrower than the
        // sidebar and a page, they would hang beside it.
        fn minimum_size(&self, _view: Option<&mut View>) -> Size {
            let engine = Engine::global();
            let sidebar = if engine.sidebar_visible() { engine.sidebar_width() } else { 0 };
            Size { width: sidebar + MIN_PAGE.width, height: MIN_PAGE.height + 40 }
        }
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

            // Right of the sidebar, top to bottom: the row of our own window
            // buttons where the system draws none, the developer bar (added
            // when it first shows), the content panel.
            let mut pd = BodyPanelDelegate::new();
            let pages = panel_create(Some(&mut pd)).expect("pages column");
            pages.set_background_color(0xFF1B_1B1E);
            let pages_layout = pages
                .set_to_box_layout(Some(&BoxLayoutSettings {
                    horizontal: 0,
                    cross_axis_alignment: AxisAlignment::STRETCH,
                    ..Default::default()
                }))
                .expect("box layout");
            let caption = if crate::caption::Caption::wanted() {
                crate::caption::Caption::new(self.win)
            } else {
                None
            };
            if let Some(caption) = &caption {
                pages.add_child_view(Some(&mut caption.view()));
            }
            let mut cv = View::from(&content);
            pages.add_child_view(Some(&mut cv));
            pages_layout.set_flex_for_view(Some(&mut cv), 1);

            let mut sv = View::from(&sidebar);
            let mut pv = View::from(&pages);
            body.add_child_view(Some(&mut sv));
            body.add_child_view(Some(&mut pv));
            layout.set_flex_for_view(Some(&mut pv), 1);
            let mut bv = View::from(&body);
            window.add_child_view(Some(&mut bv));
            column.set_flex_for_view(Some(&mut bv), 1);
            window.set_title(Some(&CefString::from("Magnetite")));
            // The taskbar, its preview and Alt+Tab show the window's own
            // icon: Chromium's, unless it is set. macOS shows the app's.
            #[cfg(not(target_os = "macos"))]
            if let Some(icon) = crate::empty::app_icon() {
                window.set_window_icon(Some(&mut icon.clone()));
                window.set_window_app_icon(Some(&mut icon.clone()));
            }
            window.center_window(Some(&Size { width: 1280, height: 860 }));
            window.show();
            window.layout();

            let chrome = engine.lock().win.chrome_color;
            let empty = crate::empty::EmptyPage::new(&content, chrome);
            let mut inner = engine.lock();
            let Some(state) = inner.window_mut(self.win) else { return };
            let sidebar_browser = state.pending_sidebar_browser.take();
            state.frame = Some(FrameState {
                window: window.clone(),
                body,
                sidebar_view: sidebar,
                top_bar_view: None,
                top_bar_browser: None,
                pages_column: pages,
                dev_bar_view: None,
                dev_bar_browser: None,
                empty,
                caption,
                content_panel: content,
                sidebar_browser,
            });
            drop(inner);
            // Windows ignores `show` while the window is still being created
            // (the call above): the frame stayed invisible under its pages.
            #[cfg(target_os = "windows")]
            {
                let window = window.clone();
                Engine::post(Box::new(move || window.show()));
            }
            let win = self.win;
            engine.in_window(win, || {
                // Turned on before the window existed (the saved setting).
                if engine.top_bar_visible() {
                    engine.show_top_bar(true);
                }
                engine.emit(EngineEvent::FrameCreated { win });
            });
        }

        fn on_window_bounds_changed(&self, _window: Option<&mut Window>, new_bounds: Option<&Rect>) {
            let engine = Engine::global();
            engine.in_window(self.win, || {
                // Our pages draw one background across the window: each
                // needs the window's size, which only some of them see change.
                if let Some(b) = new_bounds {
                    engine.ui_eval_all(&format!(
                        "window.__magnetite && window.__magnetite.windowSize({}, {})",
                        b.width, b.height
                    ));
                }
                engine.sync_caption();
                engine.reposition_contents();
                engine.apply_drag_regions();
                engine.update_edge_zone();
                if engine.float_sidebar_shown() {
                    engine.show_float_sidebar(true);
                }
                engine.emit(EngineEvent::FrameBoundsChanged);
            });
        }

        // Entering or leaving full screen takes or brings back the row of
        // our own window buttons above the pages, and the frame around
        // the pages where it shows only outside full screen.
        fn on_window_fullscreen_transition(&self, _window: Option<&mut Window>, is_completed: ::std::os::raw::c_int) {
            if is_completed != 0 {
                let engine = Engine::global();
                engine.in_window(self.win, || {
                    engine.sync_caption();
                    engine.apply_page_frame();
                });
            }
        }

        // The window the user turns to is the current one.
        fn on_window_activation_changed(&self, _window: Option<&mut Window>, active: ::std::os::raw::c_int) {
            if active != 0 {
                let engine = Engine::global();
                // In front again, whichever way: no longer closed.
                engine.lock().frame_hidden = false;
                engine.enter(self.win);
                engine.notifications_follow();
            }
        }

        // The window is gone. Its pages moved to another one before
        // (`can_close`), or closed with it (child windows close with their
        // parent and report it themselves). The last one ends the message
        // loop.
        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            let engine = Engine::global();
            let last = {
                let mut inner = engine.lock();
                if inner.current == self.win {
                    let next = inner.parked.keys().min().copied();
                    if let Some((id, state)) = next.and_then(|id| Some((id, inner.parked.remove(&id)?))) {
                        inner.win = state;
                        inner.set_current(engine, id);
                        false
                    } else {
                        inner.win.frame = None;
                        true
                    }
                } else {
                    inner.parked.remove(&self.win);
                    false
                }
            };
            if last {
                quit_message_loop();
            } else {
                engine.emit(EngineEvent::WindowClosed { win: self.win });
            }
        }

        // The only window does not close: on macOS it hides and Magnetite
        // runs on, elsewhere closing it is quitting, which the app does
        // after saving. Any other window closes, its pages going to the
        // window that is current next.
        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            let engine = Engine::global();
            if engine.is_quitting() {
                return 1;
            }
            // As `on_window_destroyed` chooses it.
            let next = {
                let inner = engine.lock();
                if inner.current == self.win {
                    inner.parked.keys().min().copied()
                } else {
                    Some(inner.current)
                }
            };
            if let Some(next) = next {
                engine.leave_window(self.win, next);
                1
            } else {
                #[cfg(target_os = "macos")]
                engine.hide_frame();
                #[cfg(not(target_os = "macos"))]
                engine.emit(EngineEvent::QuitRequested);
                0
            }
        }

        fn on_key_event(&self, _window: Option<&mut Window>, event: Option<&KeyEvent>) -> ::std::os::raw::c_int {
            let Some(shortcut) = event.and_then(crate::keys::any_shortcut) else { return 0 };
            let engine = Engine::global();
            engine.in_window(self.win, || engine.emit(EngineEvent::Shortcut { from: None, shortcut }));
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
        fn linux_window_properties(&self, _window: Option<&mut Window>, properties: Option<&mut LinuxWindowProperties>) -> ::std::os::raw::c_int {
            set_linux_window_properties(properties)
        }
    }
}

/// Name our windows `magnetite` for the desktop: the Wayland app ID and the
/// X11 window class, so docks and task bars match them to
/// `magnetite.desktop` (its `StartupWMClass`) and show its icon. Without
/// them CEF leaves Chromium's defaults. For `linux_window_properties`.
pub(crate) fn set_linux_window_properties(
    properties: Option<&mut LinuxWindowProperties>,
) -> ::std::os::raw::c_int {
    let Some(p) = properties else { return 0 };
    p.wayland_app_id = cef_owned("magnetite");
    p.wm_class_class = cef_owned("magnetite");
    p.wm_class_name = cef_owned("magnetite");
    1
}

/// `text` as a string CEF allocates and frees itself, for a struct handed
/// back to CEF. The binding passes only such borrowed strings through: one
/// made with `CefString::from(&str)` reaches CEF empty and is freed again by
/// Rust (a clone of it twice: a double free).
fn cef_owned(text: &str) -> CefString {
    // SAFETY: an all-zero cef_string_t is the documented empty string; the
    // conversion gives it a buffer and the function that frees it.
    let mut raw: sys::_cef_string_utf16_t = unsafe { std::mem::zeroed() };
    unsafe { sys::cef_string_utf8_to_utf16(text.as_ptr().cast(), text.len(), &mut raw) };
    CefString::from(raw)
}

#[cfg(test)]
mod tests {
    use super::PageFrame;

    #[test]
    fn the_frame_goes_in_full_screen_only_where_it_is_set_so() {
        for (frame, windowed, fullscreen) in [
            (PageFrame::Always, true, true),
            (PageFrame::Windowed, true, false),
            (PageFrame::Never, false, false),
        ] {
            assert_eq!(frame.shows(false), windowed, "{frame:?} in a window");
            assert_eq!(frame.shows(true), fullscreen, "{frame:?} in full screen");
        }
    }
}
