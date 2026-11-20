//! One DevTools-protocol channel per page, in process. Requests carry our own
//! message ids; replies arrive on the UI thread through an observer that is
//! registered the first time a page is used. The same observer gets the two
//! events we ask for: the browser's log entries of a watched page
//! (`watch_errors`) and the pictures of a page being recorded
//! (`screencast`). Pages never see any of this: the one domain a page
//! could detect, `Runtime`, is never enabled.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use cef::*;
use serde_json::Value;

use crate::engine::Engine;
use crate::events::ContentId;

/// Called once with the method's result, or the protocol error message.
pub type Reply = Box<dyn FnOnce(Result<Value, String>)>;

thread_local! {
    /// Replies still waiting, by message id, with the page they were sent to.
    static PENDING: RefCell<HashMap<i32, (ContentId, Reply)>> = RefCell::new(HashMap::new());
    /// Our ids start high so they never meet ids CEF assigns itself.
    static NEXT_ID: Cell<i32> = const { Cell::new(1_000_000) };
}

impl Engine {
    /// Run DevTools-protocol `method` with `params` in page `id`. `reply` runs
    /// on the UI thread; it gets an error if the page is gone.
    pub fn devtools(&self, id: ContentId, method: &str, params: &Value, reply: Reply) {
        self.devtools_at(self.content_browser_host(id), id, method, params, reply);
    }

    /// `devtools` in `host`, a browser that belongs to page `id`.
    pub(crate) fn devtools_at(
        &self,
        host: Option<BrowserHost>,
        id: ContentId,
        method: &str,
        params: &Value,
        reply: Reply,
    ) {
        let Some(host) = host else {
            reply(Err("page is gone".into()));
            return;
        };
        self.ensure_devtools_observer(id, &host);
        let msg_id = NEXT_ID.with(|n| {
            let v = n.get();
            n.set(if v == i32::MAX { 1_000_000 } else { v + 1 });
            v
        });
        let msg =
            serde_json::json!({ "id": msg_id, "method": method, "params": params }).to_string();
        PENDING.with(|p| p.borrow_mut().insert(msg_id, (id, reply)));
        if host.send_dev_tools_message(Some(msg.as_bytes())) == 0
            && let Some((_, reply)) = PENDING.with(|p| p.borrow_mut().remove(&msg_id))
        {
            reply(Err("could not send to the page".into()));
        }
    }

    /// Report page `id`'s failed requests and other errors of the browser's
    /// own (`EngineEvent::ConsoleError`), which never reach the console
    /// handler; what the page logs itself does. Holds for the page's
    /// lifetime, across its navigations.
    pub fn watch_errors(&self, id: ContentId) {
        self.devtools(id, "Log.enable", &serde_json::json!({}), Box::new(|_| {}));
    }

    /// Send page `id`'s picture whenever it changes
    /// (`EngineEvent::ScreencastFrame`, JPEG, at most 1920 px a side), or
    /// stop doing so. Only a page on screen draws pictures.
    pub fn screencast(&self, id: ContentId, on: bool, reply: Reply) {
        if on {
            let params = serde_json::json!({
                "format": "jpeg", "quality": 85, "maxWidth": 1920, "maxHeight": 1920,
            });
            self.devtools(id, "Page.startScreencast", &params, reply);
        } else {
            self.devtools(id, "Page.stopScreencast", &serde_json::json!({}), reply);
        }
    }

    fn ensure_devtools_observer(&self, id: ContentId, host: &BrowserHost) {
        let missing = self
            .lock()
            .contents
            .get(&id)
            .is_some_and(|c| c.devtools.is_none());
        if missing {
            let mut observer = Observer::new();
            let registration = host.add_dev_tools_message_observer(Some(&mut observer));
            if let Some(c) = self.lock().contents.get_mut(&id) {
                c.devtools = registration;
            }
        }
    }
}

/// Page `id` closed: its calls still waiting will never be answered, so
/// their replies run now, with an error. UI thread, outside the engine lock
/// (a reply may call the engine).
pub(crate) fn page_closed(id: ContentId) {
    let replies: Vec<Reply> = PENDING.with(|p| {
        let mut pending = p.borrow_mut();
        let mut ids: Vec<i32> = pending
            .iter()
            .filter(|(_, (page, _))| *page == id)
            .map(|(msg_id, _)| *msg_id)
            .collect();
        ids.sort_unstable();
        ids.iter()
            .filter_map(|msg_id| pending.remove(msg_id))
            .map(|(_, reply)| reply)
            .collect()
    });
    for reply in replies {
        reply(Err("the page is gone".into()));
    }
}

wrap_dev_tools_message_observer! {
    struct Observer {}
    impl DevToolsMessageObserver {
        fn on_dev_tools_event(
            &self,
            browser: Option<&mut Browser>,
            method: Option<&CefString>,
            params: Option<&[u8]>,
        ) {
            // The two events we ask for (`watch_errors`, `screencast`).
            let method = method.map(CefString::to_string);
            let engine = Engine::global();
            if method.as_deref() == Some("Page.screencastFrame") {
                let frame: Value = params.and_then(|p| serde_json::from_slice(p).ok()).unwrap_or(Value::Null);
                let Some(id) = engine.content_id_for(browser.as_deref()) else { return };
                // The page sends its next picture once this one is taken.
                let taken = serde_json::json!({ "sessionId": frame["sessionId"] });
                engine.devtools(id, "Page.screencastFrameAck", &taken, Box::new(|_| {}));
                if let Value::String(jpeg) = &frame["data"] {
                    engine.emit(crate::events::EngineEvent::ScreencastFrame { id, jpeg: jpeg.clone() });
                }
                return;
            }
            if method.as_deref() != Some("Log.entryAdded") { return; }
            let entry: Value = params.and_then(|p| serde_json::from_slice(p).ok()).unwrap_or(Value::Null);
            if entry["entry"]["level"] == "error"
                && let Some(id) = engine.content_id_for(browser.as_deref())
            {
                engine.emit(crate::events::EngineEvent::ConsoleError { id });
            }
        }
        fn on_dev_tools_method_result(
            &self,
            _browser: Option<&mut Browser>,
            message_id: ::std::os::raw::c_int,
            success: ::std::os::raw::c_int,
            result: Option<&[u8]>,
        ) {
            let Some((_, reply)) = PENDING.with(|p| p.borrow_mut().remove(&message_id)) else { return };
            let value: Value = result.and_then(|r| serde_json::from_slice(r).ok()).unwrap_or(Value::Null);
            if success != 0 {
                reply(Ok(value));
            } else {
                let msg = value.get("message").and_then(Value::as_str).unwrap_or("protocol error");
                reply(Err(msg.to_string()));
            }
        }
    }
}
