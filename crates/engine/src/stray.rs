//! Windows Chromium opens by itself: an extension's new tab
//! (`chrome.tabs.create`), its pop-up window, an options page. Chromium
//! gives them a plain Chrome window outside our layout; we hide it at once
//! and open its address as one of our tabs instead. A pop-up window
//! (`chrome.windows.create({type: "popup"})`) stays a window, as in other
//! browsers: the extension keeps watching it, and closing it would end
//! what it is for (Bitwarden's passkey window falls back to the browser's
//! own dialog when its window goes).

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

/// Whether an extension's pop-up window may load `url` as it is: its own
/// pages, web pages and an empty page.
fn popup_may_show(url: &str) -> bool {
    ["chrome-extension://", "https://", "http://"]
        .iter()
        .any(|p| url.starts_with(p))
        || url == "about:blank"
}

impl Engine {
    /// A stray browser starts loading `url`: open that as our tab and close
    /// the browser (a new tab page means nothing to open). `DevTools` that
    /// Chromium opens itself (Inspect on an extension popup) keep their
    /// window, and so does an extension's pop-up window. Returns whether
    /// the load is cancelled.
    pub(crate) fn adopt_stray(&self, browser: Option<&mut Browser>, url: &str) -> bool {
        if url.starts_with("devtools://") {
            if let Some(browser) = browser {
                set_shown(browser, true);
            }
            return false;
        }
        // Only the addresses an extension's window is for: anything else
        // (`magnetite://`, `javascript:`, `data:`, …) goes the tab's way
        // below, where the app's address policy decides.
        #[cfg(target_os = "macos")]
        if let Some(host) = browser.as_deref().and_then(ImplBrowser::host)
            && popup_may_show(url)
            && crate::mac::is_popup_window(host.window_handle())
        {
            // Shown with the keyboard: it usually asks for something
            // (Bitwarden's master password, a passkey to pick).
            crate::mac::focus_window_of(host.window_handle());
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

#[cfg(test)]
mod tests {
    use super::popup_may_show;

    #[test]
    fn a_popup_window_shows_extension_and_web_pages_only() {
        assert!(popup_may_show(
            "chrome-extension://abc/popup/index.html?uilocation=popout"
        ));
        assert!(popup_may_show("https://example.org/"));
        assert!(popup_may_show("about:blank"));
        for url in [
            "magnetite://ui/settings",
            "javascript:alert(1)",
            "data:text/html,x",
            "chrome://settings",
            "file:///etc/passwd",
        ] {
            assert!(!popup_may_show(url), "{url}");
        }
    }
}
