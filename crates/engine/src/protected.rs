//! Noticing that protected playback fails in a Chromium page.
//!
//! Magnetite's Chromium plays Widevine video, but not for the services that
//! ask for a browser build Google has verified (`system.rs`): their licence
//! servers refuse it, each with an error page of its own. What they share
//! is that the page asks Widevine for a licence request and gets no usable
//! key: its player closes the key session after the refusal, or no key
//! arrives in time. A page says so when that happens (`protected_hook.js`), and
//! the app decides whether its site moves to the system's engine
//! (`EngineEvent::ProtectedPlaybackFailed`). A service that plays here gets
//! its key and is left alone.
//!
//! The report is untrusted, since a renderer sends it. It names the address
//! of the document it is about, and counts only while the browser knows the
//! frame at that same address: a report that arrives after the tab went on
//! to another page says nothing about that page. Any page can make it by
//! using the interface, which costs it its Chromium page and nothing of the
//! user's.

use cef::*;

use crate::engine::Engine;
use crate::events::EngineEvent;

pub const MESSAGE: &str = "magnetite.protected";
const HOOK_JS: &str = include_str!("protected_hook.js");
/// The longest address a report is taken for.
const MAX_ADDRESS: usize = 4096;

/// Whether pages are watched at all: only where a system engine can be
/// hosted to move them to.
const WATCHED: bool = cfg!(any(target_os = "macos", target_os = "windows"));

impl Engine {
    /// Protected playback failed in the document at `reported`, in the main
    /// frame `frame` of the page in `browser`. A mini window stays
    /// Chromium's, so it does not tell.
    pub(crate) fn on_protected_report(
        &self,
        browser: Option<&Browser>,
        frame: &Frame,
        reported: &str,
    ) {
        let url = CefString::from(&frame.url()).to_string();
        if !Self::system_engine_available() || reported.len() > MAX_ADDRESS || reported != url {
            return;
        }
        let Some(id) = self.content_id_for(browser) else {
            return;
        };
        if self.is_mini(id) || self.is_system_page(id) {
            return;
        }
        let context = self.content_context(id);
        self.emit(EngineEvent::ProtectedPlaybackFailed { id, url, context });
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
            // The document the report is about, for the browser to compare.
            if let Some(args) = msg.argument_list() {
                args.set_string(0, Some(&CefString::from(&frame.url())));
            }
            frame.send_process_message(ProcessId::BROWSER, Some(&mut msg));
            1
        }
    }
}
