//! The window's own buttons (minimise, maximise, close) where the system
//! draws none into a frameless window: Windows. macOS has its traffic
//! lights, and Linux leaves them to the top bar.
//!
//! They sit where Windows has them, at the top right: a row above the page
//! area, in the frame window itself, so it moves and resizes with the
//! frame. (A small window of its own trailed behind the frame's edge while
//! it was resized.) The rest of the row drags the window
//! (`apply_drag_regions`). With the top bar on, that has the buttons and
//! the row goes.

use cef::*;

use crate::engine::{Engine, WindowId};

/// A button's size: Windows' own caption buttons.
const BUTTON: Size = Size {
    width: 46,
    height: 32,
};
const CLOSE_HOVER: u32 = 0xFFC4_2B1C;
const WHITE: u32 = 0xFFFF_FFFF;
/// The signs at rest; brighter under the pointer.
const MUTED: u32 = 0xFF9D_9B98;
const BRIGHT: u32 = 0xFFF0_EFEC;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Minimize,
    Maximize,
    Close,
}

impl Action {
    /// The button's sign, in plain characters every system has a font for.
    /// (The signs of Windows' icon fonts are private-use characters, which
    /// Chromium's labels draw as "unknown character".)
    fn sign(self, maximized: bool) -> &'static str {
        match (self, maximized) {
            (Self::Minimize, _) => "\u{2500}",
            (Self::Maximize, false) => "\u{25A1}",
            (Self::Maximize, true) => "\u{25A3}",
            (Self::Close, _) => "\u{2715}",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Minimize => magnetite_ui::t!("app.window_buttons.minimize"),
            Self::Maximize => magnetite_ui::t!("app.window_buttons.maximize"),
            Self::Close => magnetite_ui::t!("common.close"),
        }
    }
}

/// The row of the window's buttons.
#[derive(Clone)]
pub(crate) struct Caption {
    row: Panel,
    buttons: [(Action, LabelButton); 3],
}

impl Caption {
    /// The row's height.
    pub(crate) const HEIGHT: i32 = BUTTON.height;
    /// The width the buttons take at the row's right end.
    pub(crate) const BUTTONS_WIDTH: i32 = 3 * BUTTON.width;

    /// Whether the window's buttons are ours to draw.
    pub(crate) const fn wanted() -> bool {
        cfg!(target_os = "windows")
    }

    /// The row of frame window `win`, with its buttons at the right end.
    pub(crate) fn new(win: WindowId) -> Option<Self> {
        let row = panel_create(None)?;
        row.set_to_box_layout(Some(&BoxLayoutSettings {
            horizontal: 1,
            main_axis_alignment: AxisAlignment::END,
            cross_axis_alignment: AxisAlignment::STRETCH,
            ..Default::default()
        }))?;
        let button = |action: Action| {
            let button = label_button_create(
                Some(&mut CaptionButton::new(action, win)),
                Some(&CefString::from(action.sign(false))),
            )?;
            button.set_font_list(Some(&CefString::from("Segoe UI Symbol, 13px")));
            button.set_horizontal_alignment(HorizontalAlignment::CENTER);
            button.set_minimum_size(Some(&BUTTON));
            button.set_maximum_size(Some(&BUTTON));
            // The hover colours are ours (`paint`).
            button.set_ink_drop_enabled(0);
            button.set_focusable(0);
            let name = CefString::from(action.name());
            button.set_tooltip_text(Some(&name));
            button.set_accessible_name(Some(&name));
            row.add_child_view(Some(&mut View::from(&button)));
            Some((action, button))
        };
        let buttons = [
            button(Action::Minimize)?,
            button(Action::Maximize)?,
            button(Action::Close)?,
        ];
        Some(Self { row, buttons })
    }

    /// What the frame lays out above the pages.
    pub(crate) fn view(&self) -> View {
        View::from(&self.row)
    }

    /// Row and buttons on the chrome's colour `argb`.
    pub(crate) fn set_background(&self, argb: u32) {
        self.row.set_background_color(argb);
        for (action, button) in &self.buttons {
            let hot = matches!(button.state(), ButtonState::HOVERED | ButtonState::PRESSED);
            paint(button, *action, hot, argb);
        }
    }

    /// The window was maximised or restored: the middle button says what a
    /// click does now. `shown`: whether the window has the row at all.
    pub(crate) fn sync(&self, maximized: bool, shown: bool) {
        for (action, button) in &self.buttons {
            if *action == Action::Maximize {
                button.set_text(Some(&CefString::from(action.sign(maximized))));
            }
        }
        if (self.row.is_visible() != 0) != shown {
            self.row.set_visible(shown.into());
            if let Some(column) = self.row.parent_view() {
                column.invalidate_layout();
            }
        }
    }
}

/// A button's colours: the chrome's at rest, lighter under the pointer, and
/// red for closing as on Windows.
fn paint(button: &LabelButton, action: Action, hot: bool, chrome: u32) {
    let lighter = |shift: u32| {
        let channel = (chrome >> shift) & 0xFF;
        (channel + (0xFF - channel) / 10) << shift
    };
    let (background, text) = match (hot, action) {
        (false, _) => (chrome, MUTED),
        (true, Action::Close) => (CLOSE_HOVER, WHITE),
        (true, _) => (0xFF00_0000 | lighter(16) | lighter(8) | lighter(0), BRIGHT),
    };
    button.set_background_color(background);
    button.set_enabled_text_colors(text);
}

wrap_button_delegate! {
    struct CaptionButton {
        action: Action,
        // The window the button is in: the one it acts on.
        win: WindowId,
    }
    impl ViewDelegate {}
    impl ButtonDelegate {
        fn on_button_pressed(&self, _button: Option<&mut Button>) {
            let (action, win) = (self.action, self.win);
            // From a task: closing the window from inside its own button's
            // callback would tear the button down under it.
            Engine::post(Box::new(move || {
                let engine = Engine::global();
                // The button's own window, whichever is current by now.
                engine.in_window(win, || match action {
                    Action::Minimize => engine.minimize_frame(),
                    Action::Maximize => engine.toggle_maximize_frame(),
                    Action::Close => engine.close_frame(),
                });
            }));
        }
        fn on_button_state_changed(&self, button: Option<&mut Button>) {
            let Some(button) = button.and_then(|b| b.as_label_button()) else { return };
            let hot = matches!(button.state(), ButtonState::HOVERED | ButtonState::PRESSED);
            let chrome = Engine::global().lock().win.chrome_color;
            paint(&button, self.action, hot, chrome);
        }
    }
}
