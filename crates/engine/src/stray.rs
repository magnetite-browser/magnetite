//! Windows Chromium opens by itself: an extension's new tab
//! (`chrome.tabs.create`), its pop-out window, an options page. Chromium
//! gives them a plain Chrome window outside our layout; we hide it at once
//! and open its address as one of our tabs instead.

use cef::*;

use crate::engine::Engine;
use crate::events::{EngineEvent, UrlSource};

/// A stray browser exists: keep its Chrome window off screen until its
/// first load says what it is.
pub(crate) fn hide(browser: &Browser) {
    set_shown(browser, false);
}

fn set_shown(browser: &Browser, shown: bool) {
    #[cfg(target_os = "macos")]
    if let Some(host) = browser.host() {
        crate::mac::show_window_of(host.window_handle(), shown);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (browser, shown);
}

impl Engine {
    /// A stray browser starts loading `url`: open that as our tab and close
    /// the browser (a new tab page means nothing to open). `DevTools` that
    /// Chromium opens itself (Inspect on an extension popup) keep their
    /// window. Returns whether the load is cancelled.
    pub(crate) fn adopt_stray(&self, browser: Option<&mut Browser>, url: &str) -> bool {
        if url.starts_with("devtools://") {
            if let Some(browser) = browser {
                set_shown(browser, true);
            }
            return false;
        }
        let mut context = None;
        if let Some(browser) = browser {
            hide(browser);
            if let Some(host) = browser.host() {
                context = host.request_context().and_then(|c| self.context_id_of(&c));
                // Not from inside the navigation callback.
                Engine::post(Box::new(move || host.close_browser(1)));
            }
        }
        // Opening it in some other profile would hand the page that
        // profile's cookies; a context we do not know gets nothing.
        if context.is_none() {
            crate::trace("stray window from an unknown context: not opened");
            return true;
        }
        let blank = url.is_empty()
            || url == "about:blank"
            || [
                "chrome://newtab",
                "chrome://new-tab-page",
                "chrome-search://",
            ]
            .iter()
            .any(|p| url.starts_with(p));
        if !blank {
            self.emit(EngineEvent::OpenUrlRequested {
                from: None,
                url: url.to_string(),
                background: false,
                new_window: false,
                own_window: false,
                locked: false,
                source: UrlSource::Chromium,
                context,
            });
        }
        true
    }
}
