//! Mini windows: one page in a small window of its own, outside the frame,
//! with a slim row above it: back, forward, reload, the site's host (drags
//! the window), "Open in <space>", a menu of all spaces, and close.
//!
//! A mini window is a content window (`content.rs`) that starts top-level
//! instead of as a child of the frame, with the row as Views widgets above
//! its page. "Open in" keeps the window: the row goes, and the window is
//! attached to the frame like a tab's (`mini_to_tab`), so the page does not
//! reload. CEF never learns of that parent, so the window is attached again
//! whenever it is shown (`keep_attached`).

use cef::*;

use crate::engine::Engine;
use crate::events::{ContentId, EngineEvent};
use crate::glyphs::Glyph;

/// The row above a mini window's page, and the widgets that change.
pub(crate) struct MiniChrome {
    pub row: Panel,
    back: LabelButton,
    forward: LabelButton,
    host: LabelButton,
    open: LabelButton,
}

/// Height of the row (DIP).
const ROW_HEIGHT: i32 = 34;
/// Text on the row, and text of a button that cannot be used (ARGB).
const TEXT: u32 = 0xFFE4_E4E7;
const DIM: u32 = 0xFF6E_6E76;
/// Width and height of the row's icons (DIP).
const ICON: u16 = 16;
/// Longest host or address the row shows, in characters.
const HOST_CHARS: usize = 60;

/// What a button of the row does.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Back,
    Forward,
    Reload,
    /// The host: no action; the window is dragged by it.
    Host,
    Open,
    Close,
}

/// "Open in <space>" for the space shown first.
fn open_label(spaces: &[(String, String)]) -> String {
    spaces.first().map_or_else(
        || "Open as tab".to_string(),
        |(_, t)| format!("Open in {t}"),
    )
}

impl MiniChrome {
    /// The row for mini window `id` on `background` (ARGB), its open button
    /// naming the first of `spaces` (`(id, title)`).
    fn new(id: ContentId, background: u32, spaces: &[(String, String)]) -> Option<Self> {
        let row = panel_create(Some(&mut RowDelegate::new()))?;
        row.set_background_color(background);
        let layout = row.set_to_box_layout(Some(&BoxLayoutSettings {
            horizontal: 1,
            cross_axis_alignment: AxisAlignment::STRETCH,
            inside_border_insets: Insets {
                top: 2,
                left: 6,
                bottom: 2,
                right: 6,
            },
            between_child_spacing: 2,
            ..Default::default()
        }))?;
        let add = |action: Action, text: &str, tip: &str| {
            let button = label_button_create(
                Some(&mut MiniButton::new(id, action)),
                Some(&CefString::from(text)),
            )?;
            button.set_enabled_text_colors(TEXT);
            button.set_text_color(ButtonState::DISABLED, DIM);
            button.set_ink_drop_enabled(1);
            if !tip.is_empty() {
                button.set_tooltip_text(Some(&CefString::from(tip)));
            }
            row.add_child_view(Some(&mut View::from(&button)));
            Some(button)
        };
        // An icon button: the icon in the text's colour, dimmed while the
        // button cannot be used.
        let iconed = |button: &LabelButton, glyph: Glyph| {
            for (state, colour) in [(ButtonState::NORMAL, TEXT), (ButtonState::DISABLED, DIM)] {
                button.set_image(state, glyph.image(ICON, colour).as_mut());
            }
        };
        let icon = |action: Action, glyph: Glyph, tip: &str| {
            let button = add(action, "", tip)?;
            iconed(&button, glyph);
            Some(button)
        };
        let back = icon(Action::Back, Glyph::Back, "Back")?;
        let forward = icon(Action::Forward, Glyph::Forward, "Forward")?;
        icon(Action::Reload, Glyph::Reload, "Reload")?;
        let host = add(Action::Host, "", "")?;
        host.set_horizontal_alignment(HorizontalAlignment::LEFT);
        host.set_text_color(ButtonState::DISABLED, TEXT);
        host.set_ink_drop_enabled(0);
        host.set_enabled(0);
        layout.set_flex_for_view(Some(&mut View::from(&host)), 1);
        let open = add(Action::Open, &open_label(spaces), "Open as a tab (⌘O)")?;
        let menu = menu_button_create(
            Some(&mut SpaceMenuButton::new(id)),
            Some(&CefString::from("")),
        )?;
        iconed(&LabelButton::from(&menu), Glyph::Down);
        menu.set_tooltip_text(Some(&CefString::from("Open in another space")));
        row.add_child_view(Some(&mut View::from(&menu)));
        icon(Action::Close, Glyph::Close, "Close (⌘W)")?;
        back.set_enabled(0);
        forward.set_enabled(0);
        Some(Self {
            row,
            back,
            forward,
            host,
            open,
        })
    }
}

/// The work area (CEF screen coordinates, DIP) of the screen with the
/// pointer, where the quick search and new mini windows appear.
pub(crate) fn pointer_screen_area() -> Rect {
    #[cfg(target_os = "macos")]
    let display = display_get_nearest_point(Some(&crate::mac::mouse_point()), 0);
    // Windows tells the pointer's place in pixels, CEF's screens are in points.
    #[cfg(target_os = "windows")]
    let display = crate::win::mouse_point().and_then(|at| display_get_nearest_point(Some(&at), 1));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let display: Option<Display> = None;
    display.or_else(display_get_primary).map_or_else(
        || Rect {
            x: 0,
            y: 0,
            width: 1280,
            height: 800,
        },
        |d| d.work_area(),
    )
}

impl Engine {
    /// Open `url` in a new mini window in `profile`'s context. The window
    /// comes to the front with the keyboard in the page. Returns the page's id.
    pub fn create_mini(&self, url: &str, profile: Option<&crate::ContextProfile>) -> ContentId {
        // A pick from the quick search: the app stays in front.
        self.lock().quick_from_outside = false;
        self.create_page(url, profile, true)
    }

    /// The row for new mini window `id`, in the current chrome colour.
    pub(crate) fn mini_chrome(&self, id: ContentId) -> Option<MiniChrome> {
        let (background, spaces) = {
            let inner = self.lock();
            (inner.chrome_color, inner.mini_spaces.clone())
        };
        MiniChrome::new(id, background, &spaces)
    }

    /// Where a new mini window goes: centred on the screen with the
    /// pointer, each further one a little lower and to the right.
    pub(crate) fn mini_rect(&self) -> Rect {
        let area = pointer_screen_area();
        let open = self
            .lock()
            .contents
            .values()
            .filter(|c| c.mini.is_some())
            .count();
        let step = i32::try_from(open % 6).unwrap_or(0) * 28;
        let width = 880.min(area.width - 80).max(320);
        let height = 680.min(area.height - 80).max(240);
        Rect {
            x: area.x + (area.width - width) / 2 + step,
            y: area.y + (area.height - height) / 2 + step,
            width,
            height,
        }
    }

    /// Whether page `id` is in a mini window.
    pub fn is_mini(&self, id: ContentId) -> bool {
        self.lock()
            .contents
            .get(&id)
            .is_some_and(|c| c.mini.is_some())
    }

    /// The mini window that has the keyboard, if one has: the menu bar's
    /// commands act on its page.
    #[cfg(target_os = "macos")] // used by the menu bar
    pub(crate) fn key_mini(&self) -> Option<ContentId> {
        let minis: Vec<(ContentId, Window)> = self
            .lock()
            .contents
            .iter()
            .filter(|(_, c)| c.mini.is_some())
            .filter_map(|(id, c)| Some((*id, c.window.clone()?)))
            .collect();
        minis
            .into_iter()
            .find(|(_, w)| w.is_active() != 0)
            .map(|(id, _)| id)
    }

    /// The spaces "Open in" offers, `(id, title)`, the one its button names
    /// first. Open mini windows follow.
    pub fn set_mini_spaces(&self, spaces: Vec<(String, String)>) {
        let (rows, label) = {
            let mut inner = self.lock();
            if inner.mini_spaces == spaces {
                return;
            }
            let label = open_label(&spaces);
            inner.mini_spaces = spaces;
            let rows: Vec<(Panel, LabelButton)> = (inner.contents.values())
                .filter_map(|c| c.mini.as_ref())
                .map(|m| (m.row.clone(), m.open.clone()))
                .collect();
            (rows, label)
        };
        for (row, open) in rows {
            open.set_text(Some(&CefString::from(label.as_str())));
            View::from(&row).invalidate_layout();
            row.layout();
        }
    }

    /// Mini window `id`'s page went to `url`: the row shows its host.
    pub(crate) fn update_mini_address(&self, id: ContentId, url: &str) {
        let host = self.lock().contents.get(&id).and_then(|c| {
            let m = c.mini.as_ref()?;
            Some(m.host.clone())
        });
        if let Some(host) = host {
            let text: String = crate::client::host_of(url)
                .unwrap_or_else(|| url.to_string())
                .chars()
                .take(HOST_CHARS)
                .collect();
            host.set_text(Some(&CefString::from(text.as_str())));
        }
    }

    /// Mini window `id`'s page can go back or forward (or not) now.
    pub(crate) fn update_mini_nav(&self, id: ContentId, back: bool, forward: bool) {
        let buttons = self.lock().contents.get(&id).and_then(|c| {
            let m = c.mini.as_ref()?;
            Some((m.back.clone(), m.forward.clone()))
        });
        if let Some((b, f)) = buttons {
            b.set_enabled(back.into());
            f.set_enabled(forward.into());
        }
    }

    /// Mini window `id`'s page has the title `title`: the window takes it
    /// (Window menu, Mission Control).
    pub(crate) fn update_mini_title(&self, id: ContentId, title: &str) {
        let window = self
            .lock()
            .contents
            .get(&id)
            .filter(|c| c.mini.is_some())
            .and_then(|c| c.window.clone());
        if let Some(w) = window {
            w.set_title(Some(&CefString::from(title)));
        }
    }

    /// Mini window `id` becomes a tab's page window: its row goes, and it
    /// is shaped and attached like the others. It is hidden until the app
    /// shows the tab (`show_content`).
    pub fn mini_to_tab(&self, id: ContentId) {
        let (chrome, window) = {
            let mut inner = self.lock();
            let Some(c) = inner.contents.get_mut(&id) else {
                return;
            };
            let chrome = c.mini.take();
            c.adopted |= chrome.is_some();
            (chrome, c.window.clone())
        };
        let (Some(chrome), Some(window)) = (chrome, window) else {
            return;
        };
        window.set_draggable_regions(None);
        window.remove_child_view(Some(&mut View::from(&chrome.row)));
        window.set_title(Some(&CefString::from("")));
        window.layout();
        window.hide();
        #[cfg(target_os = "macos")]
        {
            let handle = window.window_handle();
            crate::mac::round_window_corners(handle, 10.0, false);
            crate::mac::pin_window(handle);
        }
        #[cfg(target_os = "windows")]
        crate::win::round_window_corners(window.window_handle(), false);
    }

    /// Page window `id` was just shown: if it began as a mini window, make
    /// it the frame's child again (CEF knows it as a window of its own, so
    /// hiding it lets go of the frame).
    pub(crate) fn keep_attached(&self, id: ContentId, window: &Window) {
        let adopted = self.lock().contents.get(&id).is_some_and(|c| c.adopted);
        let frame = self.frame_window();
        #[cfg(target_os = "macos")]
        if let (true, Some(frame)) = (adopted, frame) {
            crate::mac::attach_child(frame.window_handle(), window.window_handle());
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (adopted, frame, window);
    }

    /// Register the system-wide quick search key (⌥⌘N; macOS only), or
    /// let it go. Taken by another app, it stays unregistered (traced).
    #[allow(clippy::unused_self)] // an Engine method like its siblings
    pub fn set_quick_search_key(&self, on: bool) {
        #[cfg(target_os = "macos")]
        crate::mac::set_quick_search_hotkey(on);
        #[cfg(not(target_os = "macos"))]
        let _ = on;
    }

    /// Bring the main window to the front (and the app with it).
    pub fn activate_frame(&self) {
        if let Some(w) = self.frame_window() {
            w.activate();
        }
    }
}

// The row: as tall as `ROW_HEIGHT`, as wide as the window.
wrap_panel_delegate! {
    struct RowDelegate {}
    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: 320, height: ROW_HEIGHT }
        }
    }
    impl PanelDelegate {}
}

wrap_button_delegate! {
    struct MiniButton {
        id: ContentId,
        action: Action,
    }
    impl ViewDelegate {
        /// The host is where the window is dragged: it fills the row's
        /// middle, so it is the draggable region (the buttons stay clickable).
        fn on_layout_changed(&self, view: Option<&mut View>, new_bounds: Option<&Rect>) {
            if self.action != Action::Host {
                return;
            }
            let (Some(view), Some(bounds)) = (view, new_bounds) else { return };
            let mut origin = Point { x: 0, y: 0 };
            if view.convert_point_to_window(Some(&mut origin)) == 0 {
                return;
            }
            let Some(window) = view.window() else { return };
            let region = DraggableRegion {
                bounds: Rect { x: origin.x, y: origin.y, width: bounds.width, height: bounds.height },
                draggable: 1,
            };
            window.set_draggable_regions(Some(&[region]));
        }
    }
    impl ButtonDelegate {
        fn on_button_pressed(&self, _button: Option<&mut Button>) {
            let engine = Engine::global();
            let id = self.id;
            match self.action {
                Action::Back => engine.content_go_back(id),
                Action::Forward => engine.content_go_forward(id),
                Action::Reload => engine.content_reload(id),
                Action::Host => {}
                Action::Open => engine.emit(EngineEvent::MiniToTab { id, space: None }),
                // Not from inside the window's own button.
                Action::Close => Engine::post(Box::new(move || Engine::global().close_content(id))),
            }
        }
    }
}

wrap_menu_button_delegate! {
    struct SpaceMenuButton {
        id: ContentId,
    }
    impl ViewDelegate {}
    impl ButtonDelegate {}
    impl MenuButtonDelegate {
        /// A menu of every space; a pick opens the page there.
        fn on_menu_button_pressed(
            &self,
            menu_button: Option<&mut MenuButton>,
            screen_point: Option<&Point>,
            _button_pressed_lock: Option<&mut MenuButtonPressedLock>,
        ) {
            let Some(button) = menu_button else { return };
            let spaces = Engine::global().lock().mini_spaces.clone();
            let ids = spaces.iter().map(|(id, _)| id.clone()).collect();
            let Some(mut model) = menu_model_create(Some(&mut SpaceMenu::new(self.id, ids))) else {
                return;
            };
            for (i, (_, title)) in spaces.iter().enumerate() {
                let label = format!("Open in {title}");
                model.add_item(i32::try_from(i).unwrap_or(i32::MAX), Some(&CefString::from(label.as_str())));
            }
            button.show_menu(Some(&mut model), screen_point, MenuAnchorPosition::TOPRIGHT);
        }
    }
}

wrap_menu_model_delegate! {
    struct SpaceMenu {
        id: ContentId,
        spaces: Vec<String>,
    }
    impl MenuModelDelegate {
        fn execute_command(&self, _menu_model: Option<&mut MenuModel>, command_id: ::std::os::raw::c_int, _event_flags: EventFlags) {
            let space = usize::try_from(command_id).ok().and_then(|i| self.spaces.get(i).cloned());
            if space.is_some() {
                Engine::global().emit(EngineEvent::MiniToTab { id: self.id, space });
            }
        }
    }
}
