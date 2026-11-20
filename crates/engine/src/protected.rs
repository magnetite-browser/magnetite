//! Noticing that a Chromium page starts protected playback.
//!
//! Magnetite's Chromium plays Widevine video, but not for the services that
//! ask for a browser build Google has verified (`system.rs`), and nothing
//! tells the two kinds apart from outside. So a page that asks Widevine for
//! a licence request says so (`protected_hook.js`), and the app decides
//! whether its site moves to the system's engine
//! (`EngineEvent::ProtectedPlayback`).
//!
//! The report is untrusted, since a renderer sends it: it carries nothing,
//! and the page's address is the one the browser knows. Any page can make
//! it by using the interface, which costs it its Chromium page and nothing
//! of the user's.

use cef::*;

use crate::engine::Engine;
use crate::events::EngineEvent;

pub const MESSAGE: &str = "magnetite.protected";
const HOOK_JS: &str = include_str!("protected_hook.js");

/// Whether pages are watched at all: only where a system engine can be
/// hosted to move them to.
const WATCHED: bool = cfg!(any(target_os = "macos", target_os = "windows"));

impl Engine {
    /// The page in `browser` started protected playback in its main frame
    /// `frame`. A mini window stays Chromium's, so it does not tell.
    pub(crate) fn on_protected_report(&self, browser: Option<&Browser>, frame: &Frame) {
        if !Self::system_engine_available() {
            return;
        }
        let Some(id) = self.content_id_for(browser) else {
            return;
        };
        if self.is_mini(id) || self.is_system_page(id) {
            return;
        }
        let url = CefString::from(&frame.url()).to_string();
        let context = self.content_context(id);
        self.emit(EngineEvent::ProtectedPlayback { id, url, context });
    }
}

// ---- renderer process ----

/// Install the page script in a web page's main frame. Called from
/// `on_context_created`, before any page script runs. A player in a frame
/// from elsewhere does not move the page around it.
pub(crate) fn install(frame: &Frame, context: &V8Context) {
    if !WATCHED
        || frame.is_main() == 0
        || !crate::media::is_web_url(&CefString::from(&frame.url()).to_string())
        || !crate::media::is_own_world(frame, context)
    {
        return;
    }
    let mut handler = ProtectedReport::new();
    let Some(report) =
        v8_value_create_function(Some(&CefString::from("report")), Some(&mut handler))
    else {
        return;
    };
    if context.enter() == 0 {
        return;
    }
    let mut factory = None;
    let mut exception = None;
    let ok = context.eval(
        Some(&CefString::from(HOOK_JS)),
        None,
        1,
        Some(&mut factory),
        Some(&mut exception),
    ) != 0;
    if let Some(factory) = factory.filter(|_| ok) {
        factory.execute_function(None, Some(&[Some(report)]));
    }
    context.exit();
}

wrap_v8_handler! {
    struct ProtectedReport {}
    impl V8Handler {
        fn execute(
            &self,
            _name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            _arguments: Option<&[Option<V8Value>]>,
            _retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> ::std::os::raw::c_int {
            let Some(frame) = v8_context_get_current_context().and_then(|c| c.frame()) else { return 1 };
            let Some(mut msg) = process_message_create(Some(&CefString::from(MESSAGE))) else { return 1 };
            frame.send_process_message(ProcessId::BROWSER, Some(&mut msg));
            1
        }
    }
}
