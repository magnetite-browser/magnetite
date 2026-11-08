//! The notification panel: a small window at the top right of the page area
//! with our UI at `magnetite://ui/notifications` (downloads, updates). It
//! never takes the keyboard. The app opens it when there is news; its page
//! decides what to show, sizes the window to fit and closes it (height 0)
//! when nothing is left.

use cef::*;

use crate::engine::Engine;
use crate::overlay::Slot;

const URL: &str = "magnetite://ui/notifications";
const WIDTH: i32 = 360;
/// Distance from the page area's top and right edges.
const MARGIN: i32 = 8;
/// Height until the page has measured itself: one running download.
const FIRST_HEIGHT: i32 = 84;

impl Engine {
    /// Open the panel if it is closed; its page then shows the news.
    pub fn ensure_notifications(&self) {
        if self.lock().notifications.is_some() {
            return;
        }
        self.lock().notifications_height = FIRST_HEIGHT;
        let bounds = self.notifications_bounds(FIRST_HEIGHT);
        self.create_overlay(bounds, URL, 12.0, true, false, Slot::Notifications, false);
    }

    /// The panel's page needs `height` points; 0 closes the panel.
    pub fn set_notifications_height(&self, height: i32) {
        if height <= 0 {
            let window = self.lock().notifications.take();
            if let Some(w) = window {
                w.close();
            }
            return;
        }
        self.lock().notifications_height = height;
        self.place_notifications();
    }

    /// Run `js` in the panel's page (our UI only), if the panel is open.
    pub fn notifications_eval(&self, js: &str) {
        let browser = self
            .lock()
            .notifications
            .as_ref()
            .and_then(|w| w.child_view_at(0)?.as_browser_view()?.browser());
        if let Some(frame) = browser.and_then(|b| b.main_frame()) {
            self.eval_in_ui(&frame, js);
        }
    }

    /// Keep the panel at the page area's top right and above the pages
    /// (call after pages were placed or raised).
    pub(crate) fn place_notifications(&self) {
        let (window, height) = {
            let inner = self.lock();
            (inner.notifications.clone(), inner.notifications_height)
        };
        let Some(window) = window else { return };
        window.set_bounds(Some(&self.notifications_bounds(height)));
        window.layout();
        crate::floatbar::show_quietly(&window);
    }

    fn notifications_bounds(&self, height: i32) -> Rect {
        let area = self.content_rect();
        let width = WIDTH.min(area.width - 2 * MARGIN).max(200);
        Rect {
            x: area.x + area.width - width - MARGIN,
            y: area.y + MARGIN,
            width,
            height: height.clamp(1, (area.height - 2 * MARGIN).max(1)),
        }
    }
}
