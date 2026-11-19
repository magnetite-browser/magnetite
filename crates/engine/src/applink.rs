//! Links to other apps: addresses with a scheme no browser page is loaded
//! from (`mailto:`, `zoommtg:`, a desktop app's own at the end of a sign-in
//! in the browser).
//!
//! Chromium has the answer to those: it asks "Open <app>?" and, on a yes,
//! gives the address to the system. CEF lets it only when the request has a
//! handler that says so (`on_protocol_execution`), and only for a navigation
//! a page's document started. Without that the load fails, the tab shows an
//! error at the app's address and the app never hears of it.
//!
//! So every navigation to such an address gets the handler here. Where
//! Magnetite would have to load the address itself, a document asks instead,
//! and nothing is ever opened without Chromium's question: the page that
//! wants it in a new tab or a popup (`open_from`), or the empty document of
//! a new tab whose first address redirected there.

use std::collections::BTreeSet;
use std::sync::Mutex;

use cef::*;

/// Schemes Chromium or Magnetite load themselves; an address of any other
/// scheme is for another app.
const LOADED_HERE: [&str; 17] = [
    "http",
    "https",
    "about",
    "blob",
    "data",
    "file",
    "filesystem",
    "javascript",
    "view-source",
    "ws",
    "wss",
    "magnetite",
    "chrome",
    "chrome-extension",
    "chrome-untrusted",
    "chrome-search",
    "devtools",
];

/// Whether `url` is a link to another app.
pub(crate) fn is_app_link(url: &str) -> bool {
    let Some((scheme, _)) = url.split_once(':') else {
        return false;
    };
    let valid = scheme
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid
        && !LOADED_HERE
            .iter()
            .any(|known| scheme.eq_ignore_ascii_case(known))
}

/// What a document runs to go to app link `url`.
fn go_to(url: &str) -> Option<String> {
    debug_assert!(is_app_link(url));
    let address = serde_json::to_string(url).ok()?;
    Some(format!("location.assign({address})"))
}

/// The document in `frame` goes to app link `url`, as if the page had done
/// so itself: Chromium asks whether to open the app, and the page stays.
/// Gives the page nothing it could not do without us.
pub(crate) fn open_from(frame: &Frame, url: &str) {
    if let Some(js) = go_to(url) {
        frame.execute_java_script(Some(&CefString::from(js.as_str())), None, 0);
    }
}

/// Browsers whose empty document was sent to an app link and whose
/// navigation there has not been seen yet.
static SENT: Mutex<BTreeSet<i32>> = Mutex::new(BTreeSet::new());

/// The handler for a navigation of `browser` to an app link. `initiator` is
/// the origin of the document that started it; without one (none, or the
/// `null` of a document without an origin) Magnetite did: a new tab's
/// address that redirected there.
pub(crate) fn handler(
    browser: Option<&Browser>,
    initiator: Option<&CefString>,
) -> ResourceRequestHandler {
    let from_page = initiator.is_some_and(|i| !matches!(i.to_string().as_str(), "" | "null"));
    // The navigation an empty tab's document was sent on comes without an
    // origin too: it is not sent round again.
    let sent = browser.is_some_and(|b| SENT.lock().is_ok_and(|mut s| s.remove(&b.identifier())));
    AppLink::new(!from_page && !sent)
}

wrap_resource_request_handler! {
    struct AppLink {
        // Magnetite started the navigation: CEF does not pass those on.
        unasked: bool,
    }
    impl ResourceRequestHandler {
        // The binding's own answer would cancel the request, and a cancelled
        // one is not offered to the system.
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            ReturnValue::CONTINUE
        }

        // Asked once the load has failed for want of anything here that
        // loads the scheme.
        fn on_protocol_execution(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            allow_os_execution: Option<&mut ::std::os::raw::c_int>,
        ) {
            if let Some(allow) = allow_os_execution {
                *allow = 1;
            }
            // A tab that shows nothing yet has its empty document ask. One
            // with a page stays as it is: the question would name that
            // page's site, which did not ask.
            let (true, Some(browser), Some(frame), Some(request)) = (self.unasked, browser, frame, request) else {
                return;
            };
            let shown = CefString::from(&frame.url()).to_string();
            let empty = frame.is_main() != 0 && (shown.is_empty() || shown == "about:blank");
            if !empty || !SENT.lock().is_ok_and(|mut s| s.insert(browser.identifier())) {
                return;
            }
            let (browser, js) = (browser.clone(), go_to(&CefString::from(&request.url()).to_string()));
            // The channel that reaches an empty document is for the UI thread.
            crate::engine::Engine::post(Box::new(move || {
                if let (Some(host), Some(js)) = (browser.host(), js) {
                    crate::media::evaluate(&host, &js, false);
                }
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_app_link;

    #[test]
    fn only_schemes_nothing_here_loads_are_app_links() {
        for url in [
            "t3code://app/?code=1&state=2",
            "mailto:a@example.org",
            "zoommtg://zoom.us/join?confno=1",
            "ms-settings:display",
        ] {
            assert!(is_app_link(url), "{url}");
        }
        for url in [
            "https://example.org/a:b",
            "HTTP://example.org",
            "about:blank",
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            "data:text/html,x",
            "blob:https://example.org/uuid",
            "magnetite://ui/settings",
            "chrome://settings",
            "/relative/path",
            "example.org",
            " t3code://app",
            "1x://app",
        ] {
            assert!(!is_app_link(url), "{url}");
        }
    }
}
