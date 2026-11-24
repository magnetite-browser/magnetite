//! The close button of a web panel outside macOS: a cross on a disc beside
//! the panel's top corner, over the page next to or under it, always in the
//! same place. It is an overlay view of that page's own window, so it needs
//! no place in the stacking order. While no page is there, the page behind
//! the tabs draws the button, and on Windows the gap that resizes a docked
//! panel (`Empty.tsx`). On macOS the button is a native window
//! (`mac::set_panel_close`).
//!
//! On Linux the panel floats over the page, and the strip to resize it is
//! part of the panel's own window: a slim page of our UI, an overlay along
//! the window's far edge, beyond the panel's page, where the gap beside a
//! docked panel would be. So it needs no place among the windows, goes
//! wherever the panel goes, and the pointer is measured from an edge that
//! stays put while the panel's width changes.

use cef::*;

use crate::SidebarSide;
#[cfg(target_os = "linux")]
use crate::client::MagnetiteClient;
#[cfg(target_os = "linux")]
use crate::content::PageView;
use crate::content::{SPLIT_GAP, WEB_PANEL_DOCKED, web_panel_rect};
use crate::engine::Engine;
use crate::events::{ContentId, EngineEvent};
use crate::glyphs::Glyph;

/// The strip's page.
#[cfg(target_os = "linux")]
const STRIP_URL: &str = "magnetite://ui/panel-grip";
/// Where the strip is: the web panel whose window it is an overlay of, and
/// the sidebar's side it was laid out for.
#[cfg(target_os = "linux")]
pub(crate) type StripPlace = (ContentId, OverlayController, SidebarSide);
/// The disc's width, in points.
pub(crate) const SIZE: u16 = 32;
/// The disc's distance from the top and near edges of the page it is over.
pub(crate) const MARGIN: i32 = 8;
/// Room around the disc inside the button: a button draws its picture
/// within a border of its own.
const PAD: i32 = 8;
/// The cross, and the disc under it: plain, and under the pointer.
const CROSS: u32 = 0xE0FF_FFFF;
const DISC: u32 = 0xF01F_1F1F;
const DISC_HOVERED: u32 = 0xFF3C_3C3E;

impl Engine {
    /// Keep the close button (and on Linux the strip) at the web panel's
    /// edge: call after pages were shown, hidden or laid out.
    pub(crate) fn sync_panel_close(&self) {
        if cfg!(target_os = "macos") {
            return;
        }
        #[cfg(target_os = "linux")]
        self.sync_panel_strip();
        let place = self.panel_close_place();
        let old = {
            let mut inner = self.lock();
            if inner.win.panel_close.as_ref().map(|(id, _)| *id)
                == place.as_ref().map(|(id, _)| *id)
            {
                // The panel may have a new width.
                if let (Some((_, overlay)), Some((_, insets))) = (&inner.win.panel_close, &place) {
                    overlay.set_insets(Some(insets));
                }
                return;
            }
            inner.win.panel_close.take()
        };
        if let Some((_, overlay)) = old
            && overlay.is_valid() != 0
        {
            overlay.destroy();
        }
        let Some((id, insets, window)) =
            place.and_then(|(id, insets)| Some((id, insets, self.page_window(id)?)))
        else {
            return;
        };
        let Some(button) = label_button_create(
            Some(&mut PanelCloseButton::new()),
            Some(&CefString::from("")),
        ) else {
            return;
        };
        for (state, disc) in [
            (ButtonState::NORMAL, DISC),
            (ButtonState::HOVERED, DISC_HOVERED),
            (ButtonState::PRESSED, DISC_HOVERED),
        ] {
            button.set_image(
                state,
                Glyph::Close.image_on_disc(SIZE, CROSS, disc).as_mut(),
            );
        }
        button.set_ink_drop_enabled(0);
        // The keyboard stays in the page.
        button.set_focusable(0);
        let label = CefString::from(magnetite_ui::t!("app.panel.close"));
        button.set_tooltip_text(Some(&label));
        button.set_accessible_name(Some(&label));
        let overlay =
            window.add_overlay_view(Some(&mut View::from(&button)), DockingMode::TOP_LEFT, 0);
        let Some(overlay) = overlay else { return };
        overlay.set_insets(Some(&insets));
        overlay.size_to_preferred_size();
        overlay.set_visible(1);
        self.lock().win.panel_close = Some((id, overlay));
    }

    /// Keep the strip at the far edge of the open web panel's window, beside
    /// its page, and in no other window.
    #[cfg(target_os = "linux")]
    fn sync_panel_strip(&self) {
        let (wanted, strip) = {
            let inner = self.lock();
            let wanted = (inner.win.web_panel).and_then(|id| {
                let c = inner.contents.get(&id)?;
                let PageView::Chromium(page) = &c.view else {
                    return None;
                };
                Some((id, c.window.clone()?, page.clone()))
            });
            (wanted, inner.win.panel_strip.clone())
        };
        let (view, held) = match strip {
            Some(strip) => strip,
            // The first panel of this window: a strip of its own.
            None => match wanted.as_ref().and_then(|_| self.new_panel_strip()) {
                Some(view) => (view, None),
                None => return,
            },
        };
        let side = self.sidebar_side();
        if let (Some((id, window, _)), Some((holder, overlay, was))) = (&wanted, &held)
            && id == holder
            && *was == side
            && overlay.is_valid() != 0
        {
            // Where it is: only the panel's width or height may be new.
            overlay.set_bounds(Some(&strip_bounds(window, side)));
            return;
        }
        // The window it was part of is a plain page's again (or the
        // sidebar changed sides, and it goes to the panel's other edge).
        if let Some((holder, overlay, _)) = held {
            if overlay.is_valid() != 0 {
                overlay.destroy();
            }
            if let Some((window, PageView::Chromium(page))) = self.content_handles(holder)
                && window.is_valid() != 0
            {
                crate::content::lay_out_page_window(&window, &page, (0, 0));
                crate::cef_app::set_page_corner_radius(&window, 0);
            }
        }
        let Some((id, window, page)) = wanted else {
            self.lock().win.panel_strip = Some((view, None));
            return;
        };
        // One that went with the window it was part of is made again.
        let Some(view) = Some(view)
            .filter(|view| view.is_valid() != 0)
            .or_else(|| self.new_panel_strip())
        else {
            return;
        };
        let overlay = window.add_overlay_view(Some(&mut View::from(&view)), DockingMode::CUSTOM, 0);
        let Some(overlay) = overlay else {
            self.lock().win.panel_strip = Some((view, None));
            return;
        };
        self.lock().win.panel_strip = Some((view, Some((id, overlay.clone(), side))));
        crate::content::lay_out_page_window(&window, &page, strip_room(side));
        // The window is see-through beside the page, as the strip is
        // (`ContentWindowDelegate::on_theme_changed`): the panel keeps its
        // round corners there.
        crate::cef_app::set_page_corner_radius(&window, self.corner_radius(id));
        overlay.set_bounds(Some(&strip_bounds(&window, side)));
        overlay.set_visible(1);
    }

    /// Page `id`'s window has a new size, maybe one the window manager
    /// chose (`KWin` under X11 at first keeps a new window's old one): the
    /// strip stays along its far edge if it is the web panel's.
    #[cfg(target_os = "linux")]
    pub(crate) fn fit_panel_strip(&self, id: ContentId) {
        let home = self.lock().home_of(id);
        self.in_window(home, || {
            let overlay = (self.lock().win.panel_strip.as_ref())
                .and_then(|(_, held)| held.clone())
                .filter(|(holder, _, _)| *holder == id);
            if let (Some((_, overlay, side)), Some(window)) = (overlay, self.page_window(id))
                && overlay.is_valid() != 0
            {
                overlay.set_bounds(Some(&strip_bounds(&window, side)));
            }
        });
    }

    /// The room page window `window` leaves beside its page for the strip
    /// (left, right): none unless it is the web panel's.
    #[cfg(target_os = "linux")]
    pub(crate) fn panel_strip_room(&self, window: &Window) -> (i32, i32) {
        let held = (self.lock().win.panel_strip.as_ref())
            .and_then(|(_, held)| held.as_ref().map(|(id, _, side)| (*id, *side)));
        let room = held.and_then(|(id, side)| {
            let w = self.page_window(id)?;
            (View::from(&w).is_same(Some(&mut View::from(window))) != 0).then(|| strip_room(side))
        });
        room.unwrap_or((0, 0))
    }

    /// A new strip, for a panel's window to take. See-through where the CEF
    /// in use can do it (`cef_app::clear_pages`), as the divider beside a
    /// panel on macOS is; else in the gap's colour.
    #[cfg(target_os = "linux")]
    fn new_panel_strip(&self) -> Option<BrowserView> {
        let clear = crate::cef_app::clear_pages();
        let background = if clear {
            crate::cef_app::CLEAR_BACKGROUND
        } else {
            self.lock().win.chrome_color
        };
        let url = if clear {
            format!("{STRIP_URL}?clear")
        } else {
            STRIP_URL.to_owned()
        };
        browser_view_create(
            Some(&mut MagnetiteClient::overlay()),
            Some(&CefString::from(url.as_str())),
            Some(&BrowserSettings {
                background_color: background,
                ..Default::default()
            }),
            None,
            None,
            Some(&mut PanelStripDelegate::new()),
        )
    }

    /// The strip's page, for state pushes.
    #[cfg(target_os = "linux")]
    pub(crate) fn panel_strip_browser(&self) -> Option<Browser> {
        let strip = self
            .lock()
            .win
            .panel_strip
            .as_ref()
            .map(|(view, _)| view.clone());
        strip?.browser()
    }

    /// The strip does not keep the keyboard (pressed, or under a window
    /// manager that activates the panel's window when it is clicked): it
    /// goes back to the page or panel that had it, if that is still on
    /// screen.
    #[cfg(target_os = "linux")]
    pub(crate) fn grip_returns_keyboard(&self) {
        let focused = self.lock().win.last_focused;
        if let Some((window, view)) = focused.and_then(|id| self.content_handles(id))
            && window.is_visible() != 0
        {
            view.request_focus();
        }
    }

    /// The strip beyond the web panel was dragged so that its middle is at
    /// `x` on the screen (the panel follows); `done` on release keeps the
    /// width. No `x`: it was pressed (`done` false), or let go without a
    /// move.
    pub fn drag_web_panel_grip(&self, x: Option<i32>, done: bool) {
        match (x, done) {
            (Some(x), _) => {
                let wanted = self.web_panel_width_at(x, crate::content::WEB_PANEL_GRIP);
                self.resize_web_panel(u32::try_from(wanted.max(0)).ok(), done);
            }
            #[cfg(target_os = "linux")]
            (None, false) => self.panel_strip_pressed(),
            #[cfg(target_os = "linux")]
            (None, true) => self.grip_returns_keyboard(),
            #[cfg(not(target_os = "linux"))]
            (None, _) => {}
        }
    }

    /// The strip was pressed. The click gave the panel's window the
    /// keyboard a moment ago, before the strip's page heard of it: the page
    /// that had it before is the one to get it back (`grip_returns_keyboard`).
    #[cfg(target_os = "linux")]
    fn panel_strip_pressed(&self) {
        let mut inner = self.lock();
        let win = &mut inner.win;
        if let Some((before, at)) = win.keyboard_before
            && win.web_panel.is_some()
            && win.last_focused == win.web_panel
            && at.elapsed() < std::time::Duration::from_secs(1)
        {
            win.last_focused = before;
        }
    }

    /// The page the button lies over, and where in it. Beside a docked
    /// panel on the left: by the top left corner of the page alone on
    /// screen or of the first of a split. Beside one on the right, as
    /// beyond a floating one. Beyond a floating one, which puts it in the same
    /// place: in the page under that place, which in a split of narrow
    /// panes is not the first. None without a panel.
    fn panel_close_place(&self) -> Option<(ContentId, Insets)> {
        let (panel, width, pages) = {
            let inner = self.lock();
            let panel = inner.win.web_panel?;
            let pages = match inner.win.split.as_ref().filter(|t| t.pane_count() > 1) {
                Some(tree) => tree.ids(),
                None => inner.win.shown.into_iter().collect(),
            };
            (panel, inner.win.web_panel_width, pages)
        };
        let pages: Vec<_> = pages.into_iter().filter(|id| *id != panel).collect();
        let first = *pages.first()?;
        let corner = Insets {
            top: MARGIN - PAD,
            left: MARGIN - PAD,
            ..Default::default()
        };
        let sidebar = self.sidebar_side();
        if WEB_PANEL_DOCKED && sidebar == SidebarSide::Left {
            return Some((first, corner));
        }
        // The disc's top left, on the screen.
        let area = self.page_area();
        let panel_width = web_panel_rect(&area, width, sidebar).width;
        let size = i32::from(SIZE);
        let x = match sidebar {
            SidebarSide::Left => area.x + panel_width + SPLIT_GAP + MARGIN,
            SidebarSide::Right => area.x + area.width - panel_width - SPLIT_GAP - MARGIN - size,
        };
        let y = area.y + MARGIN;
        let (host, page) = pages
            .iter()
            .filter_map(|id| Some((*id, self.page_window(*id)?.bounds())))
            .find(|(_, b)| b.x <= x && x + size <= b.x + b.width && b.y <= y && y < b.y + b.height)
            .or_else(|| Some((first, self.page_window(first)?.bounds())))?;
        Some((
            host,
            Insets {
                top: y - page.y - PAD,
                left: x - page.x - PAD,
                ..Default::default()
            },
        ))
    }
}

/// Where the strip goes in web panel window `window`, the sidebar on its
/// `side`: along the far edge, `WEB_PANEL_GRIP` wide, in the room the page
/// leaves there (`strip_room`).
#[cfg(target_os = "linux")]
fn strip_bounds(window: &Window, side: SidebarSide) -> Rect {
    let bounds = window.bounds();
    let width = crate::content::WEB_PANEL_GRIP;
    Rect {
        x: match side {
            SidebarSide::Left => bounds.width - width,
            SidebarSide::Right => 0,
        },
        y: 0,
        width,
        height: bounds.height,
    }
}

/// The room (left, right) a web panel's page leaves in its window for the
/// strip, the sidebar on its `side`.
#[cfg(target_os = "linux")]
fn strip_room(side: SidebarSide) -> (i32, i32) {
    let width = crate::content::WEB_PANEL_GRIP;
    match side {
        SidebarSide::Left => (0, width),
        SidebarSide::Right => (width, 0),
    }
}

#[cfg(target_os = "linux")]
wrap_browser_view_delegate! {
    struct PanelStripDelegate {}
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

wrap_button_delegate! {
    struct PanelCloseButton {}
    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            let side = i32::from(SIZE) + 2 * PAD;
            Size { width: side, height: side }
        }
    }
    impl ButtonDelegate {
        fn on_button_pressed(&self, _button: Option<&mut Button>) {
            // Not from inside the button, which goes with the panel.
            Engine::post(Box::new(|| {
                Engine::global().emit(EngineEvent::WebPanelClose);
            }));
        }
    }
}
