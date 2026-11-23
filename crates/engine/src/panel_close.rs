//! The close button of a web panel outside macOS: a cross on a disc beside
//! the panel's top corner, over the page next to or under it, always in the
//! same place. It is an overlay view of that page's own window, so it needs
//! no place in the stacking order. While no page is there, the page behind
//! the tabs draws the button and the grip (`Empty.tsx`). On macOS the button
//! is a native window (`mac::set_panel_close`).
//!
//! On Linux the panel floats over the page, so the grip to resize it lies
//! over that page too: a small window of our UI kept above the panel
//! (`panel_stack.rs`).

use cef::*;

use crate::SidebarSide;
use crate::content::{SPLIT_GAP, WEB_PANEL_DOCKED, web_panel_rect};
use crate::engine::Engine;
use crate::events::{ContentId, EngineEvent};
use crate::glyphs::Glyph;

/// The grip's page.
#[cfg(target_os = "linux")]
const GRIP_URL: &str = "magnetite://ui/panel-grip";
/// The grip's size, in points. Fixed: a window that cannot be resized
/// (`OverlayWindowDelegate`) keeps its first one. Not narrower: Chromium
/// keeps the outer points of a frameless window for itself, and at 8 wide
/// no pointer event reached the page.
#[cfg(target_os = "linux")]
const GRIP_WIDTH: i32 = 16;
#[cfg(target_os = "linux")]
const GRIP_HEIGHT: i32 = 72;
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
    /// Keep the close button (and on Linux the grip) at the web panel's
    /// edge: call after pages were shown, hidden or laid out.
    pub(crate) fn sync_panel_close(&self) {
        if cfg!(target_os = "macos") {
            return;
        }
        let place = self.panel_close_place();
        #[cfg(target_os = "linux")]
        self.sync_panel_grip(place.is_some());
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
        button.set_tooltip_text(Some(&CefString::from("Close the panel")));
        button.set_accessible_name(Some(&CefString::from("Close the panel")));
        let overlay =
            window.add_overlay_view(Some(&mut View::from(&button)), DockingMode::TOP_LEFT, 0);
        let Some(overlay) = overlay else { return };
        overlay.set_insets(Some(&insets));
        overlay.size_to_preferred_size();
        overlay.set_visible(1);
        self.lock().win.panel_close = Some((id, overlay));
    }

    /// The grip in the middle of the floating panel's far edge, while a page lies under
    /// it (`over_page`) and no peek over it.
    #[cfg(target_os = "linux")]
    fn sync_panel_grip(&self, over_page: bool) {
        let (grip, panel) = {
            let inner = self.lock();
            let panel = (inner.win.web_panel)
                .filter(|_| over_page && inner.win.peek.is_none())
                .and_then(|p| inner.contents.get(&p)?.window.clone());
            (inner.win.panel_grip.clone(), panel)
        };
        let side = self.sidebar_side();
        let place = panel.as_ref().map(|p| {
            let edge = crate::content::web_panel_grip(&p.bounds(), side);
            Rect {
                // Wider than the strip, beside the panel all the same.
                x: match side {
                    SidebarSide::Left => edge.x,
                    SidebarSide::Right => edge.x + edge.width - GRIP_WIDTH,
                },
                y: edge.y + (edge.height - GRIP_HEIGHT) / 2,
                width: GRIP_WIDTH,
                height: GRIP_HEIGHT,
            }
        });
        match (grip, place) {
            (Some(grip), Some(place)) => {
                // Moved under the pointer during a drag, the window would
                // change the screen position Wayland reports for it, and
                // the width would run away.
                if !self.lock().win.panel_grip_dragged {
                    grip.set_bounds(Some(&place));
                }
                if grip.is_visible() == 0 {
                    grip.show();
                    self.panel_grip_shown();
                }
            }
            (Some(grip), None) => {
                grip.hide();
                // A drag cut off by the panel going: no release comes.
                self.lock().win.panel_grip_dragged = false;
            }
            (None, Some(place)) => {
                self.create_overlay(
                    place,
                    GRIP_URL,
                    0.0,
                    false,
                    false,
                    crate::overlay::Slot::PanelGrip,
                    false,
                );
            }
            (None, None) => {}
        }
    }

    /// The grip's window is new or shown again: it goes above the panel,
    /// and the keyboard stays with the page that has it.
    #[cfg(not(target_os = "linux"))]
    #[allow(clippy::unused_self)] // the Linux one is an Engine method
    pub(crate) fn panel_grip_shown(&self) {}

    #[cfg(target_os = "linux")]
    pub(crate) fn panel_grip_shown(&self) {
        self.raise_panel_grip(false);
        self.grip_returns_keyboard();
    }

    /// The grip does not keep the keyboard (shown, or used under a window
    /// manager that activates what is clicked): it goes back to the page
    /// that had it, if that is still on screen.
    #[cfg(target_os = "linux")]
    pub(crate) fn grip_returns_keyboard(&self) {
        let focused = self.lock().win.last_focused;
        if let Some((window, view)) = focused.and_then(|id| self.content_handles(id))
            && window.is_visible() != 0
        {
            view.request_focus();
        }
    }

    /// Keep the grip above the panel. `reshown`: the panel was shown again,
    /// so that under Wayland it lies over the grip now.
    #[cfg(target_os = "linux")]
    pub(crate) fn raise_panel_grip(&self, reshown: bool) {
        let (grip, panel) = {
            let inner = self.lock();
            let panel = (inner.win.web_panel).and_then(|p| inner.contents.get(&p)?.window.clone());
            (inner.win.panel_grip.clone(), panel)
        };
        if let (Some(grip), Some(panel)) = (grip, panel)
            && grip.is_visible() != 0
        {
            crate::panel_stack::keep_above(&grip, Some(&panel), reshown);
        }
    }

    /// The grip was dragged to `x` on the screen (the panel follows); `done`
    /// on release keeps the width.
    pub fn drag_web_panel_grip(&self, x: i32, done: bool) {
        #[cfg(target_os = "linux")]
        let grip = GRIP_WIDTH;
        #[cfg(not(target_os = "linux"))]
        let grip = crate::content::WEB_PANEL_GRIP;
        // The pointer stays in the middle of the grip.
        let wanted = self.web_panel_width_at(x, grip);
        self.lock().win.panel_grip_dragged = !done;
        self.resize_web_panel(u32::try_from(wanted.max(0)).ok(), done);
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
