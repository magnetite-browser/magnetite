//! Which tabs are being shared (captured by a page through
//! `getDisplayMedia`), for the mark on the tab.
//!
//! CEF does not say which tab a capture shows, so the pages tell: the
//! renderer gives every web page a capture handle, a random text placed
//! with `setCaptureHandleConfig`, and names it to the browser. A page that
//! captures a tab reads that tab's handle from its track and reports it
//! (`tabshare_hook.js`). A tab is shared while a live report names its
//! handle.
//!
//! Reports are untrusted, since a renderer sends them. The handle is random
//! so that a page can only name a tab it really captures.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hasher};

use cef::*;

use crate::engine::Engine;
use crate::events::{ContentId, EngineEvent};

pub const MESSAGE: &str = "magnetite.tabshare";
const HOOK_JS: &str = include_str!("tabshare_hook.js");
const HANDLE_PREFIX: &str = "magnetite-tab-";
/// Most tabs one frame can report as captured.
const MAX_CAPTURES: usize = 16;

fn is_handle(text: &str) -> bool {
    text.strip_prefix(HANDLE_PREFIX).is_some_and(|hex| {
        hex.len() == 32 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

/// What the browser knows about captures of tabs.
#[derive(Default)]
pub(crate) struct Shares {
    /// The handle each page gave itself.
    handles: HashMap<ContentId, String>,
    /// Per capturing frame (by its identifier): its page and the handles of
    /// the tabs it captures.
    captures: HashMap<String, (ContentId, Vec<String>)>,
    /// The pages last told to be shared.
    shared: HashSet<ContentId>,
}

impl Shares {
    /// Page `id` now carries `handle` (a new document in the tab).
    fn set_handle(&mut self, id: ContentId, handle: String) -> Vec<(ContentId, bool)> {
        self.handles.insert(id, handle);
        self.settle()
    }

    /// Frame `frame` of page `id` captures the tabs with `handles` (none:
    /// it stopped).
    fn set_captures(
        &mut self,
        frame: String,
        id: ContentId,
        handles: Vec<String>,
    ) -> Vec<(ContentId, bool)> {
        if handles.is_empty() {
            self.captures.remove(&frame);
        } else {
            self.captures.insert(frame, (id, handles));
        }
        self.settle()
    }

    /// Page `id` captures nothing any more: it shows a new document or its
    /// renderer died.
    fn end_captures(&mut self, id: ContentId) -> Vec<(ContentId, bool)> {
        self.captures.retain(|_, (page, _)| *page != id);
        self.settle()
    }

    /// Page `id` is gone.
    pub(crate) fn forget(&mut self, id: ContentId) -> Vec<(ContentId, bool)> {
        self.handles.remove(&id);
        self.shared.remove(&id);
        self.end_captures(id)
    }

    /// The pages whose state changed since the last call, with the new one.
    fn settle(&mut self) -> Vec<(ContentId, bool)> {
        let now: HashSet<ContentId> = (self.handles.iter())
            .filter(|(_, handle)| self.captures.values().any(|(_, c)| c.contains(handle)))
            .map(|(id, _)| *id)
            .collect();
        let changes = (now.symmetric_difference(&self.shared))
            .map(|id| (*id, now.contains(id)))
            .collect();
        self.shared = now;
        changes
    }
}

impl Engine {
    /// A report of the page script from `frame` (untrusted). Only a main
    /// frame may name the tab's handle; any frame may capture.
    pub(crate) fn on_share_report(
        &self,
        browser: Option<&Browser>,
        frame: &Frame,
        kind: &str,
        text: &str,
    ) {
        let Some(id) = self.content_id_for(browser) else {
            return;
        };
        let changes = match kind {
            "handle" if frame.is_main() != 0 && is_handle(text) => {
                self.lock().shares.set_handle(id, text.to_string())
            }
            "captures" => {
                let handles: Vec<String> = (text.split(',').filter(|h| is_handle(h)))
                    .take(MAX_CAPTURES)
                    .map(str::to_string)
                    .collect();
                let frame = CefString::from(&frame.identifier()).to_string();
                self.lock().shares.set_captures(frame, id, handles)
            }
            _ => return,
        };
        self.emit_shares(changes);
    }

    /// Page `id` shows a new document, or its renderer died: what it
    /// captured is no longer captured.
    pub(crate) fn end_captures(&self, id: ContentId) {
        let changes = self.lock().shares.end_captures(id);
        self.emit_shares(changes);
    }

    pub(crate) fn emit_shares(&self, changes: Vec<(ContentId, bool)>) {
        for (id, shared) in changes {
            self.emit(EngineEvent::ShareChanged { id, shared });
        }
    }
}

// ---- renderer process ----

/// A handle no page can guess: 128 bits from the randomly keyed hashers of
/// the standard library.
fn new_handle() -> String {
    let random = || {
        std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish()
    };
    format!("{HANDLE_PREFIX}{:016x}{:016x}", random(), random())
}

/// Install the page script in a frame of a web page. Called from
/// `on_context_created`, before any page script runs.
pub(crate) fn install(frame: &Frame, context: &V8Context) {
    if !crate::media::is_web_url(&CefString::from(&frame.url()).to_string())
        || !crate::media::is_own_world(frame, context)
    {
        return;
    }
    let mut handler = ShareReport::new();
    let Some(report) =
        v8_value_create_function(Some(&CefString::from("report")), Some(&mut handler))
    else {
        return;
    };
    // Only a tab's main frame can be captured as a tab.
    let handle = if frame.is_main() != 0 {
        v8_value_create_string(Some(&CefString::from(new_handle().as_str())))
    } else {
        v8_value_create_null()
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
        factory.execute_function(None, Some(&[Some(report), handle]));
    }
    context.exit();
}

wrap_v8_handler! {
    struct ShareReport {}
    impl V8Handler {
        fn execute(
            &self,
            _name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            _retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> ::std::os::raw::c_int {
            let text = |index: usize| {
                arguments
                    .and_then(|a| a.get(index).cloned().flatten())
                    .filter(|v| v.is_string() != 0)
                    .map(|v| CefString::from(&v.string_value()).to_string())
            };
            let (Some(kind), Some(value)) = (text(0), text(1)) else { return 1 };
            let Some(frame) = v8_context_get_current_context().and_then(|c| c.frame()) else { return 1 };
            let Some(mut msg) = process_message_create(Some(&CefString::from(MESSAGE))) else { return 1 };
            if let Some(args) = msg.argument_list() {
                args.set_string(0, Some(&CefString::from(kind.as_str())));
                args.set_string(1, Some(&CefString::from(value.as_str())));
            }
            frame.send_process_message(ProcessId::BROWSER, Some(&mut msg));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tab_is_shared_while_a_live_capture_names_its_handle() {
        let (meeting, slides): (ContentId, ContentId) = (1, 2);
        let handle = new_handle();
        assert!(is_handle(&handle) && !is_handle("magnetite-tab-123"));

        let mut shares = Shares::default();
        assert_eq!(shares.set_handle(slides, handle.clone()), []);
        let frame = || "7-ABC".to_string();
        assert_eq!(
            shares.set_captures(frame(), meeting, vec![handle.clone()]),
            [(slides, true)]
        );
        // A new page in the shared tab: the old handle no longer counts,
        // the capturing page then reports the new one.
        let next = new_handle();
        assert_eq!(shares.set_handle(slides, next.clone()), [(slides, false)]);
        assert_eq!(
            shares.set_captures(frame(), meeting, vec![next]),
            [(slides, true)]
        );
        // The capturing page leaves.
        assert_eq!(shares.end_captures(meeting), [(slides, false)]);
        assert_eq!(shares.forget(meeting), []);
    }
}
