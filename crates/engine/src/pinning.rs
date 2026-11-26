//! Which pages are pinned to extensions (`tab.pinned`,
//! `chrome.tabs.query({pinned: true})`). Magnetite keeps its pinned tabs and
//! favorites itself, and every page is a Chromium window of its own: the
//! app says which pages are pinned (`Engine::set_content_pinned`) and
//! Chromium's tab of each is pinned to match (`tab-strip-pinned.patch`).
//! An extension that pins or unpins a tab (`chrome.tabs.update`) is passed
//! on to the app (`extension-tab.patch`, `EngineEvent::ExtensionPinnedTab`).

use cef::*;

use crate::content::ContentState;
use crate::engine::Engine;
use crate::events::{ContentId, EngineEvent};

/// What Magnetite's CEF sends a page's client, with the browser process as
/// the source, when an extension pinned or unpinned the page's tab: one
/// bool.
pub(crate) const PINNED_MESSAGE: &str = "magnetite.tab-pinned";
/// What it takes on a page's frame to pin or unpin the page's tab: one bool.
const SET_PINNED_MESSAGE: &str = "magnetite.set-tab-pinned";

/// Tell Chromium through `frame` whether its page's tab is pinned.
fn tell(frame: &Frame, pinned: bool) {
    if !crate::cef_app::can_pin_tabs() {
        return;
    }
    let name = CefString::from(SET_PINNED_MESSAGE);
    let Some(mut msg) = process_message_create(Some(&name)) else {
        return;
    };
    if let Some(args) = msg.argument_list() {
        args.set_bool(0, i32::from(pinned));
    }
    frame.send_process_message(ProcessId::BROWSER, Some(&mut msg));
}

impl Engine {
    /// Page `id` is pinned to extensions, or not. Passed on to Chromium
    /// only when it differs from what Chromium has; a page without its
    /// browser yet is told once it has one (`tell_pinned_on_create`).
    pub fn set_content_pinned(&self, id: ContentId, pinned: bool) {
        let frame = {
            let mut inner = self.lock();
            let changed = if pinned {
                inner.pinned.insert(id)
            } else {
                inner.pinned.remove(&id)
            };
            if !changed {
                return;
            }
            (inner.contents.get(&id))
                .and_then(ContentState::tab_browser)
                .and_then(|b| b.main_frame())
        };
        if let Some(frame) = frame {
            tell(&frame, pinned);
        }
    }

    /// Page `id` got its browser, or a system page its shadow, whose tab
    /// Chromium makes unpinned: one meant to be pinned is told so.
    pub(crate) fn tell_pinned_on_create(&self, id: ContentId, browser: &Browser) {
        if self.lock().pinned.contains(&id)
            && let Some(frame) = browser.main_frame()
        {
            tell(&frame, true);
        }
    }

    /// Magnetite's CEF says an extension pinned or unpinned the tab of the
    /// page in `browser`, or of the system page it is the shadow of
    /// (`PINNED_MESSAGE`): Chromium has it so now, and the app pins or
    /// unpins the page's tab.
    pub(crate) fn on_extension_pinned(&self, browser: Option<&Browser>, pinned: bool) {
        let Some(id) = self.tab_owner(browser) else {
            return;
        };
        {
            let mut inner = self.lock();
            if pinned {
                inner.pinned.insert(id);
            } else {
                inner.pinned.remove(&id);
            }
        }
        self.emit(EngineEvent::ExtensionPinnedTab { id, pinned });
    }
}
