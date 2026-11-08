//! JavaScript <-> Rust bridge for the sidebar page, built on CEF's message
//! router, renamed for us (`window.magnetiteQuery({request, onSuccess,
//! onFailure})`, see `config`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cef::wrapper::message_router::{
    BrowserSideCallback, BrowserSideHandler, BrowserSideRouter, MessageRouterBrowserSide,
    MessageRouterConfig, MessageRouterRendererSide, RendererSideRouter,
};
use cef::*;

use crate::engine::Engine;

pub fn config() -> MessageRouterConfig {
    MessageRouterConfig {
        js_query_function: "magnetiteQuery".into(),
        js_cancel_function: "magnetiteQueryCancel".into(),
        ..Default::default()
    }
}

/// Browser-process side: forwards queries to the delegate.
pub fn create_browser_router() -> Arc<BrowserSideRouter> {
    let router = BrowserSideRouter::new(config());
    router.add_handler(Arc::new(UiQueryHandler), true);
    router
}

struct UiQueryHandler;

impl BrowserSideHandler for UiQueryHandler {
    fn on_query_str(
        &self,
        browser: Option<Browser>,
        frame: Option<Frame>,
        _query_id: i64,
        request: &str,
        _persistent: bool,
        callback: Arc<Mutex<dyn BrowserSideCallback>>,
    ) -> bool {
        // Only Magnetite's own pages, as top-level documents, may drive the
        // app. The frame URL is the browser process's committed URL, so a web
        // page cannot fake it.
        let shown = frame
            .as_ref()
            .filter(|f| f.is_main() != 0)
            .map(|f| CefString::from(&f.url()).to_string())
            .unwrap_or_default();
        let loaded = browser
            .as_ref()
            .and_then(|b| Engine::global().loaded_url(b.identifier()));
        let Some(origin) = query_origin(&shown, loaded) else {
            if let Ok(cb) = callback.lock() {
                cb.failure(403, "forbidden");
            }
            return true;
        };
        // Handle on the next loop iteration: creating browsers synchronously
        // inside the IPC callback deadlocks Chrome-style browser creation.
        let sender = browser.as_ref().map_or(0, ImplBrowser::identifier);
        let mut task = QueryTask::new(origin, sender, request.to_string(), callback);
        post_task(ThreadId::UI, Some(&mut task));
        true
    }
}

/// `magnetite://ui/...`: the sidebar and internal pages, the only callers allowed.
pub fn is_ui_url(url: &str) -> bool {
    url.starts_with("magnetite://ui/")
}

/// The address the app is told a query comes from, or `None` for a caller
/// that may not ask: the one the page was `loaded` from, as long as the
/// window still shows our UI. Not the address `shown`: a page of ours can
/// change that without loading anything (`history.replaceState`), and the
/// app keeps some commands to one page by it.
fn query_origin(shown: &str, loaded: Option<String>) -> Option<String> {
    loaded.filter(|loaded| is_ui_url(shown) && is_ui_url(loaded))
}

impl Engine {
    /// The main frame of browser `id` loaded a new document from `url`.
    pub(crate) fn note_loaded_url(&self, id: i32, url: String) {
        self.lock().loaded_urls.insert(id, url);
    }

    pub(crate) fn loaded_url(&self, id: i32) -> Option<String> {
        self.lock().loaded_urls.get(&id).cloned()
    }

    /// Browser `id` is gone.
    pub(crate) fn forget_loaded_url(&self, id: i32) {
        self.lock().loaded_urls.remove(&id);
    }

    /// Whether the query being handled comes from an overlay page that has
    /// been replaced by a newer overlay while it still ran. Its last words
    /// ("close the overlay", sent when it lost the keyboard) are not about
    /// the one that is open now.
    pub fn query_from_replaced_overlay(&self) -> bool {
        let Some(sender) = SENDER.get() else {
            return false;
        };
        let open = self
            .lock()
            .overlay
            .as_ref()
            .and_then(|w| w.child_view_at(0)?.as_browser_view()?.browser())
            .map(|b| b.identifier());
        open.is_some_and(|open| open != sender)
    }
}

/// Renderer-process side (lives in the helper executable).
pub fn create_renderer_router() -> Arc<RendererSideRouter> {
    RendererSideRouter::new(config())
}

type Callback = Arc<Mutex<dyn BrowserSideCallback>>;

thread_local! {
    /// The query being handled right now (UI thread), until it is deferred.
    static CURRENT: RefCell<Option<Callback>> = const { RefCell::new(None) };
    /// Browser identifier of the page whose query is being handled.
    static SENDER: std::cell::Cell<Option<i32>> = const { std::cell::Cell::new(None) };
    /// Deferred queries by ticket, waiting for `finish_reply`.
    static DEFERRED: RefCell<HashMap<u64, Callback>> = RefCell::new(HashMap::new());
}

/// Ticket for a query answered later (see `Engine::defer_reply`).
pub type ReplyTicket = u64;

/// Take the query being handled out of the immediate answer path.
pub(crate) fn defer_reply() -> Option<ReplyTicket> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let callback = CURRENT.with(|c| c.borrow_mut().take())?;
    let ticket = NEXT.fetch_add(1, Ordering::Relaxed);
    DEFERRED.with(|d| d.borrow_mut().insert(ticket, callback));
    Some(ticket)
}

/// Answer a deferred query. Must run on the UI thread.
pub(crate) fn finish_reply(ticket: ReplyTicket, result: Result<String, String>) {
    if let Some(callback) = DEFERRED.with(|d| d.borrow_mut().remove(&ticket)) {
        answer(&callback, result);
    }
}

fn answer(callback: &Callback, result: Result<String, String>) {
    if let Ok(cb) = callback.lock() {
        match result {
            Ok(response) => cb.success_str(&response),
            Err(message) => cb.failure(1, &message),
        }
    }
}

wrap_task! {
    struct QueryTask {
        origin: String,
        sender: i32,
        request: String,
        callback: Callback,
    }
    impl Task {
        fn execute(&self) {
            let engine = Engine::global();
            CURRENT.with(|c| *c.borrow_mut() = Some(self.callback.clone()));
            SENDER.set(Some(self.sender));
            let result = engine.delegate.on_ui_query(engine, &self.origin, &self.request);
            SENDER.set(None);
            // Still current: answer now. Taken: the delegate answers later.
            if let Some(callback) = CURRENT.with(|c| c.borrow_mut().take()) {
                answer(&callback, result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::query_origin;

    #[test]
    fn a_page_is_named_by_where_it_was_loaded_from() {
        let from =
            |shown: &str, loaded: Option<&str>| query_origin(shown, loaded.map(str::to_string));
        assert_eq!(
            from(
                "magnetite://ui/settings#sync",
                Some("magnetite://ui/settings#general")
            )
            .as_deref(),
            Some("magnetite://ui/settings#general")
        );
        // The sidebar after `history.replaceState(null, "", "/settings")`.
        assert_eq!(
            from("magnetite://ui/settings", Some("magnetite://ui/")).as_deref(),
            Some("magnetite://ui/")
        );
        // A website, a window that left our UI, and one nothing is known about.
        assert_eq!(
            from("https://example.org/", Some("https://example.org/")),
            None
        );
        assert_eq!(from("https://example.org/", Some("magnetite://ui/")), None);
        assert_eq!(from("magnetite://ui/", None), None);
    }
}
