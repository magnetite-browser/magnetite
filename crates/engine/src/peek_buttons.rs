//! The peek's buttons on Linux: close, open as tab and open in split view,
//! round buttons in a column just right of the peek, as on macOS
//! (`mac::show_peek_backdrop`). Like the web panel's close button
//! (`panel_close.rs`) they are overlay views of the page window behind
//! them, not windows of their own: the window manager orders the page
//! windows itself, and a button window would end up behind the page.

use cef::*;

use crate::engine::Engine;
use crate::events::{ContentId, EngineEvent, Shortcut};
use crate::glyphs::Glyph;
use crate::panel_close::SIZE;
use magnetite_ui::t;

/// Between the buttons, and between the column and the peek (as on macOS).
const GAP: i32 = 8;
/// Room around the disc inside a button: a button draws its picture
/// within a border of its own.
const PAD: i32 = 8;
/// The icon, and the disc under it: plain, and under the pointer.
const ICON: u32 = 0xE0FF_FFFF;
const DISC: u32 = 0xF01F_1F1F;
const DISC_HOVERED: u32 = 0xFF3C_3C3E;

/// The buttons from the top: picture, label, what a press does.
fn buttons() -> [(Glyph, &'static str, Shortcut); 3] {
    [
        (Glyph::Close, t!("app.peek.close"), Shortcut::ClosePeek),
        (
            Glyph::Expand,
            t!("app.peek.open_as_tab"),
            Shortcut::PeekToTab,
        ),
        (Glyph::Split, t!("app.peek.split"), Shortcut::PeekToSplit),
    ]
}

impl Engine {
    /// Show the buttons beside the open peek, or remove them when there is
    /// none: call after the peek opened, moved or ended.
    pub(crate) fn sync_peek_buttons(&self) {
        let peek = self.lock().win.peek;
        let column = peek.and_then(|id| {
            let p = self.page_window(id)?.bounds();
            let at = Point {
                x: p.x + p.width + GAP,
                y: p.y,
            };
            Some((self.page_behind(id, &at)?, at))
        });
        let old = {
            let mut inner = self.lock();
            let same_host = column.as_ref().map(|((host, _), _)| *host);
            inner
                .win
                .peek_buttons
                .take_if(|(host, _)| Some(*host) != same_host)
        };
        for overlay in old.into_iter().flat_map(|(_, overlays)| overlays) {
            if overlay.is_valid() != 0 {
                overlay.destroy();
            }
        }
        let Some(((host, window), at)) = column else {
            return;
        };
        let origin = window.bounds();
        let side = i32::from(SIZE) + 2 * PAD;
        let place = |overlay: &OverlayController, i: i32| {
            overlay.set_bounds(Some(&Rect {
                x: at.x - origin.x - PAD,
                y: at.y - origin.y + i * (i32::from(SIZE) + GAP) - PAD,
                width: side,
                height: side,
            }));
        };
        // Over the same page as before: they only move. Not under the
        // lock: views may call back into the engine while they lay out.
        let kept = self
            .lock()
            .win
            .peek_buttons
            .as_ref()
            .map(|(_, o)| o.clone());
        if let Some(overlays) = kept {
            for (i, overlay) in (0..).zip(&overlays) {
                place(overlay, i);
            }
            return;
        }
        let mut overlays = Vec::new();
        for (i, (glyph, label, shortcut)) in (0..).zip(buttons()) {
            let Some(button) = label_button_create(
                Some(&mut PeekButton::new(shortcut)),
                Some(&CefString::from("")),
            ) else {
                continue;
            };
            for (state, disc) in [
                (ButtonState::NORMAL, DISC),
                (ButtonState::HOVERED, DISC_HOVERED),
                (ButtonState::PRESSED, DISC_HOVERED),
            ] {
                button.set_image(state, glyph.image_on_disc(SIZE, ICON, disc).as_mut());
            }
            button.set_ink_drop_enabled(0);
            // The keyboard stays in the peek.
            button.set_focusable(0);
            button.set_tooltip_text(Some(&CefString::from(label)));
            button.set_accessible_name(Some(&CefString::from(label)));
            let Some(overlay) =
                window.add_overlay_view(Some(&mut View::from(&button)), DockingMode::CUSTOM, 0)
            else {
                continue;
            };
            place(&overlay, i);
            overlay.set_visible(1);
            overlays.push(overlay);
        }
        self.lock().win.peek_buttons = Some((host, overlays));
    }

    /// The page on screen under `at` (screen coordinates), other than the
    /// peek itself: the page shown alone, or a page of the split.
    fn page_behind(&self, peek: ContentId, at: &Point) -> Option<(ContentId, Window)> {
        let pages = {
            let inner = self.lock();
            match inner.win.split.as_ref().filter(|t| t.pane_count() > 1) {
                Some(tree) => tree.ids(),
                None => inner.win.shown.into_iter().collect(),
            }
        };
        pages.into_iter().filter(|id| *id != peek).find_map(|id| {
            let window = self.page_window(id)?;
            let b = window.bounds();
            let inside =
                (b.x..b.x + b.width).contains(&at.x) && (b.y..b.y + b.height).contains(&at.y);
            inside.then_some((id, window))
        })
    }
}

wrap_button_delegate! {
    struct PeekButton {
        shortcut: Shortcut,
    }
    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            let side = i32::from(SIZE) + 2 * PAD;
            Size { width: side, height: side }
        }
    }
    impl ButtonDelegate {
        fn on_button_pressed(&self, _button: Option<&mut Button>) {
            let shortcut = self.shortcut;
            // Not from inside the button, which goes with the peek.
            Engine::post(Box::new(move || {
                Engine::global().emit(EngineEvent::Shortcut { from: None, shortcut });
            }));
        }
    }
}
