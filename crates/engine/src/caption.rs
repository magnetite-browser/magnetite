//! The window's own buttons (minimise, maximise, close) where the system
//! draws none into a frameless window: Windows and Linux. macOS has its
//! traffic lights.
//!
//! They sit where those systems have them, at the top right: a row above the
//! page area, in the frame window itself, so it moves and resizes with the
//! frame. (A small window of its own trailed behind the frame's edge while
//! it was resized.) The rest of the row drags the window
//! (`apply_drag_regions`).

use cef::*;

use crate::engine::Engine;

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
            Self::Minimize => "Minimize",
            Self::Maximize => "Maximize or restore",
            Self::Close => "Close",
        }
    }
}

/// The page area with the buttons' row above it.
#[derive(Clone)]
pub(crate) struct Caption {
    /// Row and page area, one above the other: what the frame lays out
    /// beside the sidebar.
    column: Panel,
    row: Panel,
    buttons: [(Action, LabelButton); 3],
}

impl Caption {
    /// Whether the window's buttons are ours to draw.
    pub(crate) const fn wanted() -> bool {
        !cfg!(target_os = "macos")
    }

    /// Stack the buttons' row and `content` (the page area) in a column.
    pub(crate) fn new(content: &Panel) -> Option<Self> {
        let column = panel_create(None)?;
        let layout = column.set_to_box_layout(Some(&BoxLayoutSettings {
            horizontal: 0,
            cross_axis_alignment: AxisAlignment::STRETCH,
            ..Default::default()
        }))?;
        let row = panel_create(None)?;
        row.set_to_box_layout(Some(&BoxLayoutSettings {
            horizontal: 1,
            main_axis_alignment: AxisAlignment::END,
            cross_axis_alignment: AxisAlignment::STRETCH,
            ..Default::default()
        }))?;
        let button = |action: Action| {
            let button = label_button_create(
                Some(&mut CaptionButton::new(action)),
                Some(&CefString::from(action.sign(false))),
            )?;
            let font = if cfg!(target_os = "windows") {
                "Segoe UI Symbol, 13px"
            } else {
                "Sans, 13px"
            };
            button.set_font_list(Some(&CefString::from(font)));
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
        column.add_child_view(Some(&mut View::from(&row)));
        let mut content = View::from(content);
        column.add_child_view(Some(&mut content));
        layout.set_flex_for_view(Some(&mut content), 1);
        Some(Self {
            column,
            row,
            buttons,
        })
    }

    /// What the frame lays out beside the sidebar.
    pub(crate) fn view(&self) -> View {
        View::from(&self.column)
    }

    /// Row and buttons on the chrome's colour `argb`.
    pub(crate) fn set_background(&self, argb: u32) {
        self.column.set_background_color(argb);
        self.row.set_background_color(argb);
        for (action, button) in &self.buttons {
            let hot = matches!(button.state(), ButtonState::HOVERED | ButtonState::PRESSED);
            paint(button, *action, hot, argb);
        }
    }

    /// The window was maximised or restored: the middle button says what a
    /// click does now. A full screen window has no row.
    pub(crate) fn sync(&self, maximized: bool, fullscreen: bool) {
        for (action, button) in &self.buttons {
            if *action == Action::Maximize {
                button.set_text(Some(&CefString::from(action.sign(maximized))));
            }
        }
        if (self.row.is_visible() != 0) == fullscreen {
            self.row.set_visible((!fullscreen).into());
            self.column.layout();
        }
    }

    /// The row and, inside it, the buttons, both in the frame window's
    /// coordinates: the row drags the window, the buttons take clicks.
    pub(crate) fn regions(&self) -> (Rect, Rect) {
        // The column is the frame's child, the row the column's.
        let column = self.column.bounds();
        let row = self.row.bounds();
        let row = Rect {
            x: column.x + row.x,
            y: column.y + row.y,
            width: row.width,
            height: if self.row.is_visible() == 0 {
                0
            } else {
                row.height
            },
        };
        let width = (3 * BUTTON.width).min(row.width);
        let buttons = Rect {
            x: row.x + row.width - width,
            width,
            ..row.clone()
        };
        (row, buttons)
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
    }
    impl ViewDelegate {}
    impl ButtonDelegate {
        fn on_button_pressed(&self, _button: Option<&mut Button>) {
            let action = self.action;
            // From a task: closing the window from inside its own button's
            // callback would tear the button down under it.
            Engine::post(Box::new(move || {
                let engine = Engine::global();
                match action {
                    Action::Minimize => engine.minimize_frame(),
                    Action::Maximize => engine.toggle_maximize_frame(),
                    Action::Close => engine.close_frame(),
                }
            }));
        }
        fn on_button_state_changed(&self, button: Option<&mut Button>) {
            let Some(button) = button.and_then(|b| b.as_label_button()) else { return };
            let hot = matches!(button.state(), ButtonState::HOVERED | ButtonState::PRESSED);
            let chrome = Engine::global().lock().chrome_color;
            paint(&button, self.action, hot, chrome);
        }
    }
}
