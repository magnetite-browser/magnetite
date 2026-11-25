//! The peek's backdrop on Linux, as on macOS (`mac::show_peek_backdrop`):
//! the windows behind the peek are dimmed, a click on them closes it, and a
//! column of round buttons just right of it closes it, opens it as a tab or
//! in split view. Like the web panel's close button (`panel_close.rs`) the
//! dim and the buttons are overlay views of the windows behind the peek, not
//! windows of their own: the window manager orders the page windows itself,
//! and a window of ours would end up behind a page. A window's overlays
//! stack in the order they were made, so the dims come before the buttons.

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
/// Over the windows behind the peek: black at 45 %, as on macOS and Windows.
const DIM: u32 = 0x7300_0000;

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

/// Each window behind the peek with the part of it to dim, in its own
/// coordinates.
type Behind = Vec<(Window, Rect)>;

/// The dim and the buttons of the open peek, with the windows they are
/// overlays of.
#[derive(Clone)]
pub(crate) struct Backdrop {
    /// Each window behind the peek and the dim over it.
    dims: Vec<(Window, OverlayController)>,
    /// The window under the buttons, and the buttons from the top.
    buttons: Option<(Window, Vec<OverlayController>)>,
}

impl Engine {
    /// Dim the windows behind the open peek and show its buttons, or remove
    /// both when there is none: call after the peek opened, moved or ended,
    /// and after the pages behind it moved.
    pub(crate) fn sync_peek_backdrop(&self) {
        let peek = self.lock().win.peek;
        let behind = peek.map(|id| self.behind_peek(id)).unwrap_or_default();
        // The column's top left, on the screen, and the topmost window there.
        let column = peek.and_then(|id| {
            let p = self.page_window(id)?.bounds();
            let at = Point {
                x: p.x + p.width + GAP,
                y: p.y,
            };
            let (window, _) = behind
                .iter()
                .rev()
                .find(|(w, _)| contains(&w.bounds(), &at))?;
            Some((window.clone(), at))
        });
        // Over the same windows as before: it only moves. Not under the
        // lock: views may call back into the engine while they lay out.
        let held = self.lock().win.peek_backdrop.clone();
        if let Some(held) = held.filter(|b| b.fits(&behind, column.as_ref().map(|(w, _)| w))) {
            held.place(&behind, column.as_ref().map(|(_, at)| at));
            return;
        }
        let old = self.lock().win.peek_backdrop.take();
        if let Some(old) = old {
            old.destroy();
        }
        if !behind.is_empty() {
            let new = Backdrop::new(&behind, column);
            self.lock().win.peek_backdrop = Some(new);
        }
    }

    /// The windows behind peek `peek`, from the bottom: the pages on screen
    /// (the page shown alone, or those of the split) and the web panel, each
    /// with its docked `DevTools`. Of the web panel's window only the page is
    /// dimmed: the strip beside it is see-through, over a dimmed page.
    fn behind_peek(&self, peek: ContentId) -> Behind {
        let (pages, panel) = {
            let inner = self.lock();
            let pages = match inner.win.split.as_ref().filter(|t| t.pane_count() > 1) {
                Some(tree) => tree.ids(),
                None => inner.win.shown.into_iter().collect(),
            };
            (pages, inner.win.web_panel)
        };
        let whole = |window: &Window| {
            let b = window.bounds();
            Rect {
                x: 0,
                y: 0,
                width: b.width,
                height: b.height,
            }
        };
        let mut behind = Vec::new();
        let pages = pages.into_iter().filter(|id| Some(*id) != panel);
        for id in pages.chain(panel).filter(|id| *id != peek) {
            let Some(window) = self.page_window(id) else {
                continue;
            };
            let (left, right) = self.panel_strip_room(&window);
            let mut page = whole(&window);
            page.x = left;
            page.width -= left + right;
            behind.push((window, page));
            for docked in self.docked_windows(id) {
                let all = whole(&docked);
                behind.push((docked, all));
            }
        }
        behind
    }
}

impl Backdrop {
    /// Dim each window of `behind`, then put the buttons over `column`'s
    /// window, the column's top left at its screen point.
    fn new(behind: &Behind, column: Option<(Window, Point)>) -> Self {
        let dims = behind
            .iter()
            .filter_map(|(window, part)| {
                let overlay = dim_over(window)?;
                overlay.set_bounds(Some(part));
                overlay.set_visible(1);
                Some((window.clone(), overlay))
            })
            .collect();
        let buttons = column.map(|(window, at)| {
            let overlays = column_over(&window);
            for (i, overlay) in (0..).zip(&overlays) {
                place_button(overlay, &window, &at, i);
                overlay.set_visible(1);
            }
            (window, overlays)
        });
        Self { dims, buttons }
    }

    /// Whether it lies over the windows of `behind`, the buttons over
    /// `column`'s, and all its overlays are still there (a window that
    /// closed took its own along).
    fn fits(&self, behind: &Behind, column: Option<&Window>) -> bool {
        let valid = |o: &OverlayController| o.is_valid() != 0;
        let dims = self.dims.len() == behind.len()
            && (self.dims.iter().zip(behind))
                .all(|((window, dim), (wanted, _))| same(window, wanted) && valid(dim));
        let buttons = match (&self.buttons, column) {
            (Some((window, overlays)), Some(wanted)) => {
                same(window, wanted) && overlays.iter().all(valid)
            }
            (None, None) => true,
            _ => false,
        };
        dims && buttons
    }

    /// Fit the dims to the parts of `behind` (the same windows) and move the
    /// buttons' column to the screen point `column`.
    fn place(&self, behind: &Behind, column: Option<&Point>) {
        for ((_, dim), (_, part)) in self.dims.iter().zip(behind) {
            dim.set_bounds(Some(part));
        }
        if let (Some((window, overlays)), Some(at)) = (&self.buttons, column) {
            for (i, overlay) in (0..).zip(overlays) {
                place_button(overlay, window, at, i);
            }
        }
    }

    fn destroy(self) {
        let buttons = self.buttons.into_iter().flat_map(|(_, overlays)| overlays);
        for overlay in self.dims.into_iter().map(|(_, dim)| dim).chain(buttons) {
            if overlay.is_valid() != 0 {
                overlay.destroy();
            }
        }
    }
}

/// A dim over `window`, hidden until placed: a button in the dim's colour
/// (`PeekDim`), so that a click on it closes the peek.
fn dim_over(window: &Window) -> Option<OverlayController> {
    let dim = label_button_create(Some(&mut PeekDim::new()), Some(&CefString::from("")))?;
    dim.set_ink_drop_enabled(0);
    // The keyboard stays in the peek.
    dim.set_focusable(0);
    dim.set_accessible_name(Some(&CefString::from(t!("app.peek.close"))));
    window.add_overlay_view(Some(&mut View::from(&dim)), DockingMode::CUSTOM, 0)
}

/// The buttons over `window`, from the top, hidden until placed.
fn column_over(window: &Window) -> Vec<OverlayController> {
    let mut overlays = Vec::new();
    for (glyph, label, shortcut) in buttons() {
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
        if let Some(overlay) =
            window.add_overlay_view(Some(&mut View::from(&button)), DockingMode::CUSTOM, 0)
        {
            overlays.push(overlay);
        }
    }
    overlays
}

/// Put button `i` of the column whose top left is screen point `at` into
/// place over `window`.
fn place_button(overlay: &OverlayController, window: &Window, at: &Point, i: i32) {
    let origin = window.bounds();
    let side = i32::from(SIZE) + 2 * PAD;
    overlay.set_bounds(Some(&Rect {
        x: at.x - origin.x - PAD,
        y: at.y - origin.y + i * (i32::from(SIZE) + GAP) - PAD,
        width: side,
        height: side,
    }));
}

fn contains(b: &Rect, at: &Point) -> bool {
    (b.x..b.x + b.width).contains(&at.x) && (b.y..b.y + b.height).contains(&at.y)
}

fn same(a: &Window, b: &Window) -> bool {
    View::from(a).is_same(Some(&mut View::from(b))) != 0
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
            press(self.shortcut);
        }
    }
}

wrap_button_delegate! {
    struct PeekDim {}
    impl ViewDelegate {
        // Views drop a background set before this one was in a window, and
        // again when the theme changes.
        fn on_theme_changed(&self, view: Option<&mut View>) {
            if let Some(view) = view {
                view.set_background_color(DIM);
            }
        }
    }
    impl ButtonDelegate {
        fn on_button_pressed(&self, _button: Option<&mut Button>) {
            press(Shortcut::ClosePeek);
        }
    }
}

/// Act on a press of a peek button or the dim. Not from inside the button,
/// which goes with the peek.
fn press(shortcut: Shortcut) {
    Engine::post(Box::new(move || {
        Engine::global().emit(EngineEvent::Shortcut {
            from: None,
            shortcut,
        });
    }));
}
