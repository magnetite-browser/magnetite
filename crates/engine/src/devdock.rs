//! `DevTools` in windows of our own. F12 / Cmd+Option+I in a page makes
//! Chromium create a `DevTools` browser view for it (Views-hosted pages get
//! Views-hosted `DevTools`). We put it into our window: docked right of or
//! below the page (which shrinks to make room), or a separate normal window.
//! Either way `DevTools`' own main menu (⋮) offers the dock choices and close
//! (`devtools_menu.js`); switching between docked and separate reopens
//! `DevTools` in the other kind of window.

use cef::*;
use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::events::{ContentId, DevToolsAction, DividerAction, EngineEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevToolsDock {
    #[default]
    Right,
    Bottom,
    /// Chromium's own separate `DevTools` window.
    Window,
}

impl DevToolsDock {
    /// The name `DevTools`' menu (`devtools_menu.js`) and the settings use,
    /// as serialised (`"right"`).
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Bottom => "bottom",
            Self::Window => "window",
        }
    }

    /// The dock `as_str` names.
    pub(crate) fn from_str(name: &str) -> Option<Self> {
        [Self::Right, Self::Bottom, Self::Window]
            .into_iter()
            .find(|d| d.as_str() == name)
    }
}

/// The size of docked `DevTools` the user dragged (the divider between page
/// and `DevTools`), in DIP: their width docked right, their height below.
/// `None`: the default share of the page area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DevToolsSize {
    pub right: Option<u32>,
    pub bottom: Option<u32>,
}

pub(crate) struct DockedDevTools {
    pub window: Option<Window>,
    /// The gap between page and `DevTools` (screen coordinates) while both
    /// are on screen: a divider sits in it.
    pub gap: Option<Rect>,
    pub shown: bool,
    /// Device mode (`device.rs`), while on.
    pub device: Option<crate::device::DeviceMode>,
    pub browser_id: Option<i32>,
    /// A separate window, not docked beside the page.
    pub separate: bool,
    /// The `DevTools` page, to tell its menu the current dock.
    pub view: BrowserView,
}

/// Script that adds the dock choices and close to `DevTools`' main menu.
const MENU_JS: &str = include_str!("devtools_menu.js");
/// Console messages from that menu start with this.
const MENU_MARK: &str = "__magnetite_devtools__:";

/// Space between the page and its DevTools, like between split panes.
const GAP: i32 = 8;

/// How much of the page area docked `DevTools` take at first, and the least
/// room `DevTools` and the page keep (DIP), docked right / below.
const RIGHT_SHARE: f64 = 0.42;
const RIGHT_MIN_TOOLS: i32 = 240;
const RIGHT_MIN_PAGE: i32 = 320;
const BOTTOM_SHARE: f64 = 0.4;
const BOTTOM_MIN_TOOLS: i32 = 120;
const BOTTOM_MIN_PAGE: i32 = 160;

/// The extent of docked `DevTools` along an `extent` long side: `wanted`
/// (or `share` of it), keeping at least `min_tools` for them and
/// `min_page` (and the gap) for the page. When both do not fit, `DevTools`
/// keep theirs as far as the side allows; they never pass its end, so the
/// page is at worst empty, never of negative size.
fn tools_extent(
    extent: i32,
    wanted: Option<u32>,
    share: f64,
    min_tools: i32,
    min_page: i32,
) -> i32 {
    // Rounded and clamped to the slot, so the casts stay in range.
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let wanted = wanted.map_or((f64::from(extent) * share) as i32, |w| {
        w.min(i32::MAX as u32) as i32
    });
    wanted
        .min(extent - min_page - GAP)
        .max(min_tools)
        .min(extent - GAP)
        .max(0)
}

impl Engine {
    /// Open (or focus) DevTools for page `id`; they dock per `set_devtools_dock`.
    pub fn content_show_devtools(&self, id: ContentId) {
        self.content_inspect(id, None);
    }

    /// Open `DevTools` for page `id`, inspecting the element at `at` (page
    /// coordinates) if given. Our client gets their load and console events.
    pub fn content_inspect(&self, id: ContentId, at: Option<&Point>) {
        if let Some(host) = self.content_browser_host(id) {
            let mut client = crate::client::MagnetiteClient::devtools();
            host.show_dev_tools(None, Some(&mut client), None, at);
        }
    }

    /// The page whose `DevTools` run in `browser`, if any.
    pub(crate) fn devtools_owner(&self, browser: Option<&Browser>) -> Option<ContentId> {
        let ident = browser.map(ImplBrowser::identifier)?;
        self.lock()
            .docked
            .iter()
            .find(|(_, d)| d.browser_id == Some(ident))
            .map(|(id, _)| *id)
    }

    /// Close page `id`'s DevTools.
    pub fn content_close_devtools(&self, id: ContentId) {
        if let Some(host) = self.content_browser_host(id) {
            host.close_dev_tools();
        }
    }

    pub fn content_has_devtools(&self, id: ContentId) -> bool {
        self.content_browser_host(id)
            .is_some_and(|h| h.has_dev_tools() != 0)
    }

    /// Pages whose `DevTools` are open in the other kind of window than
    /// `dock` asks for (docked vs separate): they need reopening.
    pub fn devtools_to_reopen(&self, dock: DevToolsDock) -> Vec<ContentId> {
        let separate = dock == DevToolsDock::Window;
        self.lock()
            .docked
            .iter()
            .filter(|(_, d)| d.separate != separate)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Where DevTools opens; open docked DevTools follow at once.
    pub fn set_devtools_dock(&self, dock: DevToolsDock) {
        self.lock().devtools_dock = dock;
        self.reposition_contents();
        let pages: Vec<BrowserView> = self
            .lock()
            .docked
            .values()
            .map(|d| d.view.clone())
            .collect();
        for view in pages {
            tell_dock(&view, dock);
        }
    }

    /// The `DevTools` page in `browser` finished loading: add our items to
    /// its main menu. Other browsers are left alone.
    pub(crate) fn devtools_loaded(&self, browser: &Browser) {
        let owner = self.devtools_owner(Some(browser));
        let (view, dock) = {
            let inner = self.lock();
            let view = owner
                .and_then(|id| inner.docked.get(&id))
                .map(|d| d.view.clone());
            (view, inner.devtools_dock)
        };
        if let (Some(view), Some(frame)) = (view, browser.main_frame()) {
            frame.execute_java_script(Some(&CefString::from(MENU_JS)), None, 0);
            tell_dock(&view, dock);
        }
    }

    /// A console message from a page: if it is a choice from our `DevTools`
    /// menu (and the page is one of our `DevTools`), act on it. Returns
    /// whether it was one (then it is not logged).
    pub(crate) fn devtools_menu_message(&self, browser: Option<&Browser>, message: &str) -> bool {
        let Some(choice) = message.strip_prefix(MENU_MARK) else {
            return false;
        };
        let id = self.devtools_owner(browser);
        if choice == "device" {
            if let Some(id) = id {
                self.toggle_device(id);
            }
            return true;
        }
        let action = match choice {
            "close" => DevToolsAction::Close,
            dock => match DevToolsDock::from_str(dock) {
                Some(dock) => DevToolsAction::Dock(dock),
                None => return false,
            },
        };
        if let Some(id) = id {
            self.emit(EngineEvent::DevToolsMenu { id, action });
        }
        true
    }

    /// The page's and its DevTools' rectangles inside `slot`, and the gap
    /// between them.
    fn split_slot(&self, id: ContentId, slot: &Rect) -> (Rect, Option<(Rect, Rect)>) {
        let (docked, dock, size) = {
            let inner = self.lock();
            let docked = inner.docked.get(&id).is_some_and(|d| !d.separate);
            (docked, inner.devtools_dock, inner.devtools_size)
        };
        if !docked {
            return (slot.clone(), None);
        }
        if dock == DevToolsDock::Bottom {
            let h = tools_extent(
                slot.height,
                size.bottom,
                BOTTOM_SHARE,
                BOTTOM_MIN_TOOLS,
                BOTTOM_MIN_PAGE,
            );
            let page = Rect {
                height: slot.height - h - GAP,
                ..slot.clone()
            };
            let gap = Rect {
                y: slot.y + page.height,
                height: GAP,
                ..slot.clone()
            };
            let tools = Rect {
                y: slot.y + page.height + GAP,
                height: h,
                ..slot.clone()
            };
            (page, Some((tools, gap)))
        } else {
            let w = tools_extent(
                slot.width,
                size.right,
                RIGHT_SHARE,
                RIGHT_MIN_TOOLS,
                RIGHT_MIN_PAGE,
            );
            let page = Rect {
                width: slot.width - w - GAP,
                ..slot.clone()
            };
            let gap = Rect {
                x: slot.x + page.width,
                width: GAP,
                ..slot.clone()
            };
            let tools = Rect {
                x: slot.x + page.width + GAP,
                width: w,
                ..slot.clone()
            };
            (page, Some((tools, gap)))
        }
    }

    /// The divider beside page `id`'s docked `DevTools` was dragged to
    /// `pos` (screen DIP along the dock's axis), let go or double-clicked
    /// (back to the default size). Let go, the app keeps the size.
    // The divider is a native view on macOS only (`mac/chrome.rs`).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn devtools_divider(&self, id: ContentId, action: DividerAction) {
        let (slot, dock) = {
            let inner = self.lock();
            (
                inner.contents.get(&id).and_then(|c| c.slot.clone()),
                inner.devtools_dock,
            )
        };
        let Some(slot) = slot else { return };
        match action {
            DividerAction::Move(pos) => {
                // The pointer stays in the middle of the gap.
                let (extent, wanted) = if dock == DevToolsDock::Bottom {
                    (slot.height, slot.y + slot.height - pos - GAP / 2)
                } else {
                    (slot.width, slot.x + slot.width - pos - GAP / 2)
                };
                let wanted = u32::try_from(wanted.max(0)).ok();
                {
                    let size = &mut self.lock().devtools_size;
                    if dock == DevToolsDock::Bottom {
                        let h = tools_extent(
                            extent,
                            wanted,
                            BOTTOM_SHARE,
                            BOTTOM_MIN_TOOLS,
                            BOTTOM_MIN_PAGE,
                        );
                        size.bottom = u32::try_from(h).ok();
                    } else {
                        let w = tools_extent(
                            extent,
                            wanted,
                            RIGHT_SHARE,
                            RIGHT_MIN_TOOLS,
                            RIGHT_MIN_PAGE,
                        );
                        size.right = u32::try_from(w).ok();
                    }
                }
                self.place(id, &slot);
            }
            DividerAction::Release | DividerAction::Equalize => {
                if action == DividerAction::Equalize {
                    let mut inner = self.lock();
                    let size = &mut inner.devtools_size;
                    if dock == DevToolsDock::Bottom {
                        size.bottom = None;
                    } else {
                        size.right = None;
                    }
                }
                // Other pages' docked DevTools take the size as well.
                self.reposition_contents();
                let size = self.lock().devtools_size;
                self.emit(EngineEvent::DevToolsResized(size));
            }
        }
    }

    /// The docked `DevTools`' size per side, as the user last left it.
    pub fn set_devtools_size(&self, size: DevToolsSize) {
        self.lock().devtools_size = size;
        self.reposition_contents();
    }

    /// Lay out page `id` (and its docked DevTools) in `slot`, the area the
    /// page would fill alone.
    pub(crate) fn place(&self, id: ContentId, slot: &Rect) {
        let (window, tools_window) = {
            let mut inner = self.lock();
            let window = inner.contents.get_mut(&id).and_then(|c| {
                c.slot = Some(slot.clone());
                c.window.clone()
            });
            (window, inner.docked.get(&id).and_then(|d| d.window.clone()))
        };
        let (page, tools) = self.split_slot(id, slot);
        let page = self.device_rect(id, page);
        if let Some(w) = window {
            w.set_bounds(Some(&page));
        }
        if let (Some(w), Some((r, _))) = (tools_window, &tools) {
            w.set_bounds(Some(r));
        }
        if let Some(d) = self.lock().docked.get_mut(&id) {
            d.gap = tools.map(|(_, gap)| gap);
        }
        self.show_dividers();
    }

    /// Show or hide page `id`'s docked DevTools and its device bar along
    /// with the page (a separate DevTools window stays as it is).
    pub(crate) fn show_docked(&self, id: ContentId, show: bool) {
        let window = {
            let mut inner = self.lock();
            inner.docked.get_mut(&id).and_then(|d| {
                d.shown = show;
                d.window.clone().filter(|_| !d.separate)
            })
        };
        self.show_dividers();
        self.show_device_bar(id, show);
        if let Some(w) = window {
            if show {
                w.show();
                w.bring_to_top();
            } else {
                w.hide();
            }
        }
    }

    pub(crate) fn close_docked(&self, id: ContentId) {
        let entry = self.lock().docked.remove(&id);
        self.show_dividers();
        let Some(mut entry) = entry else { return };
        // Device mode ends with the DevTools.
        if let Some(mode) = entry.device.take() {
            self.end_device(id, mode);
        }
        if let Some(w) = entry.window {
            w.close();
        }
    }

    /// Chromium created DevTools for the page in `source`: put them into a
    /// window of ours, docked or separate as the setting says. False if the
    /// page is not one of ours.
    pub(crate) fn dock_devtools(&self, source: &BrowserView, tools: &BrowserView) -> bool {
        let (id, separate) = {
            let inner = self.lock();
            let mut target = View::from(source);
            let page = inner
                .contents
                .iter()
                .find(|(_, c)| View::from(&c.view).is_same(Some(&mut target)) != 0);
            // A mini window has no room to dock them: their own window.
            let separate = inner.devtools_dock == DevToolsDock::Window
                || page.is_some_and(|(_, c)| c.mini.is_some());
            (page.map(|(id, _)| *id), separate)
        };
        let Some(id) = id else { return false };
        let browser_id = tools.browser().map(|b| b.identifier());
        self.lock().docked.insert(
            id,
            DockedDevTools {
                window: None,
                gap: None,
                shown: false,
                device: None,
                browser_id,
                separate,
                view: tools.clone(),
            },
        );
        let mut delegate = DevToolsWindowDelegate::new(id, tools.clone(), separate);
        window_create_top_level(Some(&mut delegate));
        true
    }

    /// A DevTools browser closed (F12 again, its own close button): give the
    /// page its space back. Returns true if it was a docked one.
    pub(crate) fn on_devtools_closed(&self, browser: Option<&Browser>) -> bool {
        let Some(id) = self.devtools_owner(browser) else {
            return false;
        };
        self.close_docked(id);
        let slot = self.lock().contents.get(&id).and_then(|c| c.slot.clone());
        if let Some(slot) = slot {
            self.place(id, &slot);
        }
        true
    }
}

/// Tell the `DevTools` menu in `view` which dock choice is current.
fn tell_dock(view: &BrowserView, dock: DevToolsDock) {
    let key = dock.as_str();
    if let Some(frame) = view.browser().and_then(|b| b.main_frame()) {
        frame.execute_java_script(
            Some(&CefString::from(
                format!("window.__magnetiteDock = '{key}'; window.__magnetiteScan?.();").as_str(),
            )),
            None,
            0,
        );
    }
}

wrap_browser_view_delegate! {
    pub(crate) struct DevToolsViewDelegate {}
    impl ViewDelegate {}
    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::CHROME }
        fn chrome_toolbar_type(&self, _bv: Option<&mut BrowserView>) -> ChromeToolbarType { ChromeToolbarType::NONE }
    }
}

wrap_window_delegate! {
    struct DevToolsWindowDelegate {
        id: ContentId,
        view: BrowserView,
        // A normal window of its own instead of docked beside the page.
        separate: bool,
    }
    impl ViewDelegate {}
    impl PanelDelegate {}
    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let Some(window) = window else { return };
            let engine = Engine::global();
            window.set_to_fill_layout();
            let mut v = View::from(&self.view);
            window.add_child_view(Some(&mut v));
            let page_visible = {
                let mut inner = engine.lock();
                let visible = inner.contents.get(&self.id).and_then(|c| c.window.clone()).is_some_and(|w| w.is_visible() != 0);
                if let Some(d) = inner.docked.get_mut(&self.id) {
                    d.window = Some(window.clone());
                    d.shown = visible;
                }
                visible
            };
            if self.separate {
                window.set_title(Some(&CefString::from("DevTools")));
                window.center_window(Some(&Size { width: 960, height: 680 }));
                window.show();
                window.activate();
                self.view.request_focus();
                return;
            }
            let slot = engine.lock().contents.get(&self.id).and_then(|c| c.slot.clone());
            #[cfg(target_os = "macos")]
            {
                crate::mac::round_window_corners(window.window_handle(), 10.0, false);
                crate::mac::pin_window(window.window_handle());
            }
            if let Some(slot) = slot {
                engine.place(self.id, &slot);
            }
            window.show();
            if page_visible {
                window.bring_to_top();
                self.view.request_focus();
            } else {
                window.hide();
            }
        }
        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            let engine = Engine::global();
            // Only this window's entry: when DevTools were reopened at once,
            // the entry is already the new window's.
            let still_docked = {
                let mut inner = engine.lock();
                let mut mine = View::from(&self.view);
                let ours = inner
                    .docked
                    .get(&self.id)
                    .is_some_and(|d| View::from(&d.view).is_same(Some(&mut mine)) != 0);
                let removed = ours.then(|| inner.docked.remove(&self.id)).flatten();
                drop(inner);
                // Device mode ends with the DevTools.
                if let Some(mode) = removed.and_then(|d| d.device) {
                    engine.end_device(self.id, mode);
                }
                ours
            };
            engine.show_dividers();
            let slot = engine.lock().contents.get(&self.id).and_then(|c| c.slot.clone());
            if still_docked && let Some(slot) = slot {
                engine.place(self.id, &slot);
            }
        }
        fn parent_window(
            &self,
            _window: Option<&mut Window>,
            _is_menu: Option<&mut ::std::os::raw::c_int>,
            _can_activate_menu: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<Window> {
            if self.separate {
                return None;
            }
            Engine::global().frame_window()
        }
        fn is_frameless(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { (!self.separate).into() }
        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { self.separate.into() }
        // Resizable under Wayland, as a page is (content.rs `can_resize`).
        fn can_resize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { (self.separate || crate::cef_app::wayland()).into() }
        fn can_maximize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { self.separate.into() }
        fn can_minimize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { self.separate.into() }
        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int { 1 }
        fn window_runtime_style(&self) -> RuntimeStyle { RuntimeStyle::CHROME }
    }
}

#[cfg(test)]
mod tests {
    use super::{DevToolsDock, tools_extent};

    #[test]
    fn docked_devtools_keep_room_for_the_page() {
        let right = |extent, wanted| tools_extent(extent, wanted, 0.42, 240, 320);
        assert_eq!(right(1000, None), 420, "the default share");
        assert_eq!(right(1000, Some(500)), 500, "as dragged");
        assert_eq!(
            right(1000, Some(900)),
            1000 - 320 - 8,
            "the page keeps its least"
        );
        assert_eq!(right(1000, Some(10)), 240, "DevTools keep theirs");
        assert_eq!(right(400, Some(300)), 240, "no room for both: DevTools win");
        assert_eq!(
            right(200, None),
            192,
            "a narrow split pane: within the pane"
        );
        assert_eq!(right(4, None), 0, "never negative");
    }

    #[test]
    fn dock_names_match_the_settings() {
        for dock in [
            DevToolsDock::Right,
            DevToolsDock::Bottom,
            DevToolsDock::Window,
        ] {
            let json = serde_json::to_string(&dock).unwrap();
            assert_eq!(json, format!("\"{}\"", dock.as_str()));
            assert_eq!(DevToolsDock::from_str(dock.as_str()), Some(dock));
        }
        assert_eq!(DevToolsDock::from_str("close"), None);
    }
}
