//! The close button of a docked web panel: a cross on a disc beside the
//! panel's top corner, over the page next to it, always in the same place.
//! It is an overlay view of that page's own window, not a window of its
//! own: outside macOS the window manager orders the page windows itself,
//! and a button window would end up behind the page it lies on. While no
//! page is beside the panel, the page behind the tabs draws the button
//! (`Empty.tsx`). On macOS the button is a native window
//! (`mac::set_panel_close`).

use cef::*;

use crate::content::WEB_PANEL_DOCKED;
use crate::engine::Engine;
use crate::events::{ContentId, EngineEvent};
use crate::glyphs::Glyph;

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
    /// Keep the close button over the page beside a docked web panel: call
    /// after pages were shown, hidden or laid out.
    pub(crate) fn sync_panel_close(&self) {
        if !WEB_PANEL_DOCKED {
            return;
        }
        let host = self.panel_close_host();
        let old = {
            let mut inner = self.lock();
            if inner.win.panel_close.as_ref().map(|(id, _)| *id) == host {
                return;
            }
            inner.win.panel_close.take()
        };
        if let Some((_, overlay)) = old
            && overlay.is_valid() != 0
        {
            overlay.destroy();
        }
        let Some((id, window)) = host.and_then(|id| Some((id, self.page_window(id)?))) else {
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
        overlay.set_insets(Some(&Insets {
            top: MARGIN - PAD,
            left: MARGIN - PAD,
            ..Default::default()
        }));
        overlay.size_to_preferred_size();
        overlay.set_visible(1);
        self.lock().win.panel_close = Some((id, overlay));
    }

    /// The page the button lies over: the one alone on screen, or the first
    /// of a split, which is at its top left. None without a docked panel.
    fn panel_close_host(&self) -> Option<ContentId> {
        let inner = self.lock();
        let panel = inner.win.web_panel?;
        let split = inner.win.split.as_ref().filter(|t| t.pane_count() > 1);
        let host = match split {
            Some(tree) => tree.ids().into_iter().next(),
            None => inner.win.shown,
        };
        host.filter(|id| *id != panel)
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
