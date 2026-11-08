//! One DevTools-protocol channel per page, in process. Requests carry our own
//! message ids; replies arrive on the UI thread through an observer that is
//! registered the first time a page is used. Pages never see any of this:
//! no domain they could detect (`Runtime`, `Network`) is ever enabled.

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
        let Some(host) = self.content_browser_host(id) else {
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
