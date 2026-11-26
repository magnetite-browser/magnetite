//! Windows Chromium opens by itself: an extension's pop-up window, an
//! options page, a welcome page. Chromium gives them a plain Chrome window
//! outside our layout; we hide it at once and open its address as one of
//! our tabs instead. A tab from `chrome.tabs.create`, and a normal window
//! from `chrome.windows.create` with addresses to show, come this way only
//! from a CEF without `extension-tab.patch`; one with it hands us each
//! tab's page instead (`adopt_extension_tab`). A pop-up window
//! (`chrome.windows.create({type: "popup"})`) stays a window, as in other
//! browsers: the extension keeps watching it, and closing it would end
//! what it is for (Bitwarden's passkey window falls back to the browser's
//! own dialog when its window goes). On Linux, where CEF does not say which
//! kind of window Chromium made, every window showing an extension's own
//! page stays a window.

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
/// pages, web pages and an empty page. Only macOS can tell such windows
/// apart (see `adopt_stray` for Linux).
#[cfg(any(target_os = "macos", test))]
fn popup_may_show(url: &str) -> bool {
    ["chrome-extension://", "https://", "http://"]
        .iter()
        .any(|p| url.starts_with(p))
        || url == "about:blank"
}

/// Whether `url` is an empty page or Chromium's new tab page: a tab asked
/// for without an address, which means nothing to open here.
fn is_blank(url: &str) -> bool {
    url.is_empty()
        || url == "about:blank"
        || [
            "chrome://newtab",
            "chrome://new-tab-page",
            "chrome-search://",
        ]
        .iter()
        .any(|p| url.starts_with(p))
}

/// Whether `url` is one of `pages`, the pages extensions show as the new
/// tab page: Chromium loads such a page with nothing added to its address.
fn is_new_tab_page(url: &str, pages: &[String]) -> bool {
    let page = url.split(['?', '#']).next().unwrap_or(url);
    pages.iter().any(|p| p == page)
}

/// Whether a tab `browser` is to show `url` in is a new tab without an
/// address, which opens nothing here: an empty page, Chromium's new tab
/// page, or the page an extension of `browser`'s profile shows in its
/// place. Chromium opens a window with such a tab by itself after an
/// install from the Chrome Web Store, to show "added to Chromium" at.
pub(crate) fn opens_nothing(browser: Option<&Browser>, url: &str) -> bool {
    is_blank(url)
        || browser
            .and_then(ImplBrowser::host)
            .and_then(|h| h.request_context())
            .is_some_and(|c| is_new_tab_page(url, &crate::extensions::new_tab_pages(&c)))
}

impl Engine {
    /// A stray browser starts loading `url`: open that as our tab and close
    /// the browser (a new tab page, Chromium's or an extension's, means
    /// nothing to open). `DevTools` that Chromium opens itself (Inspect on
    /// an extension popup) keep their window, and so does an extension's
    /// pop-up window. Returns whether the load is cancelled.
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
        let blank = opens_nothing(browser.as_deref(), url);
        // Linux has no tab strip to look for: an extension's own page keeps
        // the window it was given (Chromium shows it focused). Its options
        // page, when set to open in a tab, then opens as a window too.
        #[cfg(target_os = "linux")]
        if url.starts_with("chrome-extension://") && !blank {
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
    use super::{is_new_tab_page, popup_may_show};

    #[test]
    fn an_extensions_new_tab_page_is_told_from_its_other_pages() {
        let pages = ["chrome-extension://abc/newtab.html".to_string()];
        assert!(is_new_tab_page(
            "chrome-extension://abc/newtab.html",
            &pages
        ));
        assert!(is_new_tab_page(
            "chrome-extension://abc/newtab.html#top",
            &pages
        ));
        assert!(!is_new_tab_page(
            "chrome-extension://abc/options.html",
            &pages
        ));
        assert!(!is_new_tab_page("chrome-extension://abc/newtab.html", &[]));
    }

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
