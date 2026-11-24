//! Mini windows: one page in a small window of its own, outside the frame,
//! with a slim row above it: back, forward, reload, the address with copy
//! link over its right end, a gap the window is dragged by, "Open in <space>", a menu of all spaces, and close.
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
use magnetite_ui::t;

/// The row above a mini window's page, and the widgets that change.
pub(crate) struct MiniChrome {
    pub row: Panel,
    back: LabelButton,
    forward: LabelButton,
    address: Textfield,
    copy: LabelButton,
    open: LabelButton,
    /// The page's address: what the field shows while it has the keyboard.
    url: String,
    /// The user changed the field's text since it last showed the address:
    /// a page that moves on then leaves the field alone.
    edited: bool,
}

/// Height of the row (DIP).
const ROW_HEIGHT: i32 = 42;
/// Width of the gap the window is dragged by (DIP).
const GRIP_WIDTH: i32 = 28;
/// Width of the copy button, which lies over the field's right end (DIP).
const COPY_WIDTH: i32 = 30;
/// Text on the row, and text of a button that cannot be used (ARGB).
const TEXT: u32 = 0xFFE4_E4E7;
const DIM: u32 = 0xFF6E_6E76;
/// Width and height of the row's icons (DIP).
const ICON: u16 = 16;
/// How long the copy button shows a tick after a copy (ms).
const COPIED_MS: i64 = 1500;

/// What a button of the row does.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Back,
    Forward,
    Reload,
    Copy,
    Open,
    Close,
}

/// `over` laid on `under` (ARGB) at `percent` opacity; the result is opaque.
fn blend(under: u32, over: u32, percent: u32) -> u32 {
    let [_, under, over] = [0, under, over].map(u32::to_be_bytes);
    let mix = |i: usize| {
        let mixed = (u32::from(under[i]) * (100 - percent) + u32::from(over[i]) * percent) / 100;
        u8::try_from(mixed).unwrap_or(u8::MAX)
    };
    u32::from_be_bytes([0xFF, mix(1), mix(2), mix(3)])
}

/// The address field's fill on a row in `background`: a little lighter.
fn field_fill(background: u32) -> u32 {
    blend(background, TEXT, 8)
}

/// Give mini window `window` the address field's colours: the row's text
/// colour on the field's fill, and neither outline nor focus ring: the
/// selected address shows where the keyboard is. A theme change resets them,
/// so the window's delegate calls this again then.
pub(crate) fn colour_window(window: &Window) {
    use cef::sys::cef_color_id_t as Id;
    let fill = field_fill(Engine::global().lock().win.chrome_color);
    for (id, colour) in [
        (Id::CEF_ColorTextfieldBackground, fill),
        (Id::CEF_ColorTextfieldForeground, TEXT),
        (Id::CEF_ColorTextfieldForegroundPlaceholder, DIM),
        (Id::CEF_ColorTextfieldOutline, fill),
        (Id::CEF_ColorFocusableBorderFocused, fill),
    ] {
        window.set_theme_color(id as ::std::os::raw::c_int, colour);
    }
}

/// What the address field shows for `url`: the address without `https://`
/// and a lone `/`. Read back as typed input it is the same address again, so
/// the field can be edited in place (which is why `www.` stays).
fn shown_address(url: &str) -> &str {
    let rest = url.strip_prefix("https://").unwrap_or(url);
    match rest.strip_suffix('/') {
        Some(host) if !host.contains(['/', '?', '#']) => host,
        _ => rest,
    }
}

/// Make `button` an icon button showing `glyph`: in the text's colour,
/// dimmed while the button cannot be used or its window is not in front.
fn iconed(button: &LabelButton, glyph: Glyph) {
    for (state, colour) in [(ButtonState::NORMAL, TEXT), (ButtonState::DISABLED, DIM)] {
        button.set_image(state, glyph.image(ICON, colour).as_mut());
    }
}

/// The address field shows `url` from its start: a long address is cut off
/// at the end, not at the host.
fn show_address(field: &Textfield, url: &str) {
    field.set_text(Some(&CefString::from(shown_address(url))));
    field.select_range(Some(&Range { from: 0, to: 0 }));
}

/// "Open in <space>" for the space shown first.
fn open_label(spaces: &[(String, String)]) -> String {
    spaces.first().map_or_else(
        || t!("app.mini.open_as_tab").to_string(),
        |(_, title)| t!("app.mini.open_in", space = title),
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
                top: 6,
                left: 6,
                bottom: 6,
                right: 6,
            },
            between_child_spacing: 2,
            ..Default::default()
        }))?;
        let add = |parent: &Panel, action: Action, text: &str, tip: &str| {
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
            parent.add_child_view(Some(&mut View::from(&button)));
            Some(button)
        };
        let icon = |parent: &Panel, action: Action, glyph: Glyph, tip: &str| {
            let button = add(parent, action, "", tip)?;
            iconed(&button, glyph);
            Some(button)
        };
        let back = icon(&row, Action::Back, Glyph::Back, t!("app.mini.back"))?;
        let forward = icon(
            &row,
            Action::Forward,
            Glyph::Forward,
            t!("app.mini.forward"),
        )?;
        icon(&row, Action::Reload, Glyph::Reload, t!("app.mini.reload"))?;
        let address = textfield_create(Some(&mut AddressField::new(id)))?;
        address.set_accessible_name(Some(&CefString::from(t!("app.mini.address"))));
        address.set_placeholder_text(Some(&CefString::from(t!("app.mini.address_placeholder"))));
        // The address bar: the field and, laid over its right end, the
        // copy button, as in the main window's address row. The field keeps
        // its text clear of the button.
        let insets = View::from(&address).insets();
        View::from(&address).set_insets(Some(&Insets {
            right: insets.right + COPY_WIDTH,
            ..insets
        }));
        let bar = panel_create(Some(&mut Bar::new()))?;
        let bar_layout = bar.set_to_box_layout(Some(&BoxLayoutSettings {
            horizontal: 1,
            cross_axis_alignment: AxisAlignment::STRETCH,
            between_child_spacing: -COPY_WIDTH,
            ..Default::default()
        }))?;
        bar.add_child_view(Some(&mut View::from(&address)));
        bar_layout.set_flex_for_view(Some(&mut View::from(&address)), 1);
        let copy = icon(&bar, Action::Copy, Glyph::Link, t!("app.mini.copy_link"))?;
        row.add_child_view(Some(&mut View::from(&bar)));
        layout.set_flex_for_view(Some(&mut View::from(&bar)), 1);
        // The gap after the bar: the window is dragged by it.
        let grip = panel_create(Some(&mut Grip::new()))?;
        row.add_child_view(Some(&mut View::from(&grip)));
        let open = add(
            &row,
            Action::Open,
            &open_label(spaces),
            t!("app.mini.open_as_tab_tip"),
        )?;
        let menu = menu_button_create(
            Some(&mut SpaceMenuButton::new(id)),
            Some(&CefString::from("")),
        )?;
        iconed(&LabelButton::from(&menu), Glyph::Down);
        menu.set_tooltip_text(Some(&CefString::from(t!("app.mini.other_space"))));
        row.add_child_view(Some(&mut View::from(&menu)));
        icon(&row, Action::Close, Glyph::Close, t!("app.mini.close"))?;
        back.set_enabled(0);
        forward.set_enabled(0);
        Some(Self {
            row,
            back,
            forward,
            address,
            copy,
            open,
            url: String::new(),
            edited: false,
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
            (inner.win.chrome_color, inner.mini_spaces.clone())
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

    /// Mini window `id`'s page went to `url`: the field shows it, unless
    /// something else was typed there meanwhile.
    pub(crate) fn update_mini_address(&self, id: ContentId, url: &str) {
        let field = self.lock().contents.get_mut(&id).and_then(|c| {
            let m = c.mini.as_mut()?;
            m.url = url.to_string();
            Some((m.address.clone(), m.edited))
        });
        let Some((field, edited)) = field else {
            return;
        };
        if View::from(&field).has_focus() == 0 || !edited {
            show_address(&field, url);
        }
    }

    /// Note whether mini window `id`'s address field holds something typed.
    fn set_mini_edited(&self, id: ContentId, edited: bool) {
        if let Some(m) = (self.lock().contents.get_mut(&id)).and_then(|c| c.mini.as_mut()) {
            m.edited = edited;
        }
    }

    /// Mini window `id`'s address field and its page's address.
    fn mini_address(&self, id: ContentId) -> Option<(Textfield, String)> {
        let inner = self.lock();
        let m = inner.contents.get(&id)?.mini.as_ref()?;
        Some((m.address.clone(), m.url.clone()))
    }

    /// `Cmd+L`: put the keyboard into mini window `id`'s address field with
    /// the address selected, to be copied or typed over. The selection ends
    /// at the start, so a long address stays in view from its start.
    pub fn focus_mini_address(&self, id: ContentId) {
        if let Some((field, _)) = self.mini_address(id) {
            View::from(&field).request_focus();
            field.select_all(1);
        }
    }

    /// Mini window `id`'s copy button shows `glyph`.
    fn set_mini_copy_icon(&self, id: ContentId, glyph: Glyph) {
        let copy =
            (self.lock().contents.get(&id)).and_then(|c| Some(c.mini.as_ref()?.copy.clone()));
        if let Some(copy) = copy {
            iconed(&copy, glyph);
        }
    }

    /// Mini window `id`'s copy button shows a tick for a moment.
    fn mini_copied(&self, id: ContentId) {
        self.set_mini_copy_icon(id, Glyph::Check);
        Self::post_delayed(
            Box::new(move || Engine::global().set_mini_copy_icon(id, Glyph::Link)),
            COPIED_MS,
        );
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
        #[cfg(target_os = "windows")]
        if let (true, Some(frame)) = (adopted, frame) {
            crate::win::attach_child(frame.window_handle(), window.window_handle());
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let _ = (adopted, frame, window);
    }

    /// Register the system-wide quick search key (⌥⌘N on macOS, Ctrl+Alt+N
    /// on Windows), or let it go. Taken by another app, it stays
    /// unregistered (traced).
    #[allow(clippy::unused_self)] // an Engine method like its siblings
    pub fn set_quick_search_key(&self, on: bool) {
        #[cfg(target_os = "macos")]
        crate::mac::set_quick_search_hotkey(on);
        #[cfg(target_os = "windows")]
        crate::win::set_quick_search_hotkey(on);
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let _ = on;
    }

    /// Bring the main window to the front (and the app with it), also when
    /// its close hid it.
    pub fn activate_frame(&self) {
        if self.frame_hidden() {
            self.show_frame();
        }
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

// The address bar's panel: nothing of its own.
wrap_panel_delegate! {
    struct Bar {}
    impl ViewDelegate {}
    impl PanelDelegate {}
}

// The gap in the row: the window's draggable region (the bar and the
// buttons beside it stay clickable).
wrap_panel_delegate! {
    struct Grip {}
    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size { width: GRIP_WIDTH, height: ROW_HEIGHT }
        }
        fn on_layout_changed(&self, view: Option<&mut View>, new_bounds: Option<&Rect>) {
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
    impl PanelDelegate {}
}

// The address field. It is edited in place: a click puts the cursor where it
// lands, and the text never changes under the pointer. Enter sends what the
// field holds to the app (`MiniNavigate`), Esc gives the keyboard back to
// the page, and without the keyboard the field shows the page's address
// again.
wrap_textfield_delegate! {
    struct AddressField {
        id: ContentId,
    }
    impl ViewDelegate {
        fn on_blur(&self, view: Option<&mut View>) {
            let Some(field) = view.and_then(|v| v.as_textfield()) else { return };
            let Some((_, url)) = Engine::global().mini_address(self.id) else { return };
            show_address(&field, &url);
            Engine::global().set_mini_edited(self.id, false);
        }
    }
    impl TextfieldDelegate {
        // Typing, pasting, cutting: the field holds the user's text from
        // now on, unless it is the page's address again.
        fn on_after_user_action(&self, textfield: Option<&mut Textfield>) {
            let Some(field) = textfield else { return };
            let engine = Engine::global();
            let Some((_, url)) = engine.mini_address(self.id) else { return };
            let typed = CefString::from(&field.text()).to_string();
            engine.set_mini_edited(self.id, typed != shown_address(&url));
        }
        fn on_key_event(&self, textfield: Option<&mut Textfield>, event: Option<&KeyEvent>) -> ::std::os::raw::c_int {
            let (Some(field), Some(event)) = (textfield, event) else { return 0 };
            if event.type_ != KeyEventType::RAWKEYDOWN {
                return 0;
            }
            let engine = Engine::global();
            let id = self.id;
            match event.windows_key_code {
                // Enter
                0x0D => {
                    let typed = CefString::from(&field.text()).to_string();
                    let Some((_, url)) = engine.mini_address(id) else { return 1 };
                    // Left as it was, the field means the page's own address.
                    let input = if typed == shown_address(&url) { url } else { typed };
                    if !input.trim().is_empty() {
                        engine.emit(EngineEvent::MiniNavigate { id, input });
                    }
                    1
                }
                // Esc
                0x1B => {
                    engine.focus_content(id);
                    1
                }
                _ => 0,
            }
        }
    }
}

wrap_button_delegate! {
    struct MiniButton {
        id: ContentId,
        action: Action,
    }
    impl ViewDelegate {
        // The copy button lies over the address field: it is see-through
        // (a view's background is reset with the theme).
        fn on_theme_changed(&self, view: Option<&mut View>) {
            if let (Action::Copy, Some(view)) = (self.action, view) {
                view.set_background_color(0);
            }
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
                // As the key does; the button answers with a tick.
                Action::Copy => {
                    engine.emit(EngineEvent::Shortcut { from: Some(id), shortcut: crate::Shortcut::CopyLink });
                    engine.mini_copied(id);
                }
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
                let label = t!("app.mini.open_in", space = title);
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

#[cfg(test)]
mod tests {
    use super::shown_address;

    #[test]
    fn the_shown_address_drops_what_every_address_has() {
        assert_eq!(shown_address("https://example.com/"), "example.com");
        assert_eq!(
            shown_address("https://www.example.com/a/?q=1#top"),
            "www.example.com/a/?q=1#top"
        );
        // A path's own last slash stays, and so does any other scheme.
        assert_eq!(shown_address("https://example.com/a/"), "example.com/a/");
        assert_eq!(
            shown_address("http://localhost:3000/"),
            "http://localhost:3000/"
        );
        assert_eq!(shown_address("about:blank"), "about:blank");
    }
}
