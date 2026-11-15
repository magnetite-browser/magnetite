//! Scriptlets in pages: the small scripts filter lists ask for
//! (`example.com##+js(…)`), which must run in a page before the page's own
//! scripts do. Which script a page at some address gets is the blocker's
//! business (`blocker.rs`); this is how it gets there (ADR-0013).
//!
//! Chromium runs a script in every new document of a page when asked to
//! through the page's `DevTools` channel
//! (`Page.addScriptToEvaluateOnNewDocument`). That script belongs to one
//! address, so it is exchanged when the page goes elsewhere: the request for
//! the new document waits on CEF's IO thread (`Exchange`) until Chromium has
//! the script for it, which the UI thread sees to (`apply`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cef::*;
use serde_json::{Value, json};

use crate::engine::Engine;

/// The pages that have, or are about to get, a script, by browser
/// identifier. Read on the IO thread, changed on both.
static PAGES: Mutex<Option<HashMap<i32, Page>>> = Mutex::new(None);
/// Whether `PAGES` has ever held a page: until then a request costs no lock.
static IN_USE: AtomicBool = AtomicBool::new(false);

/// How long a document's request waits for the exchange. Chromium answers
/// within a millisecond or two unless the page is busy in a script of its
/// own; then the document loads with the script it had.
const WAIT_MS: i64 = 300;

#[derive(Default)]
struct Page {
    /// The script the page's next document is to get; empty for none.
    wanted: String,
    /// The script Chromium has for the page.
    applied: String,
    /// What Chromium calls that script, to remove it by.
    identifier: Option<String>,
    /// An exchange is on its way to Chromium.
    busy: bool,
    /// The `Page` domain is on: Chromium runs the scripts only then.
    enabled: bool,
    /// Requests waiting until `applied` is `wanted`.
    waiting: Vec<Hold>,
}

fn pages() -> MutexGuard<'static, Option<HashMap<i32, Page>>> {
    PAGES.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A request for a document, waiting; let go once, by whoever comes first.
#[derive(Clone)]
struct Hold(Arc<Mutex<Option<Callback>>>);

impl Hold {
    fn release(&self) {
        let callback = self.0.lock().ok().and_then(|mut c| c.take());
        if let Some(callback) = callback {
            callback.cont();
        }
    }
}

/// Whether any page has a script, so that documents' requests must be
/// looked at even when the lists in use have no scriptlets (any more).
pub(crate) fn in_use() -> bool {
    IN_USE.load(Ordering::Relaxed)
}

/// The handler for the request of a page's document. `script_for` gives the
/// script a page at an address is to get (empty for none).
pub(crate) fn exchange(script_for: fn(&str) -> String) -> ResourceRequestHandler {
    Exchange::new(script_for)
}

/// Page `browser` is to run `script` in its next document. Returns whether
/// Chromium does not have it yet; then `hold` is let go once it has.
fn want(browser: i32, script: String, hold: Hold) -> bool {
    let mut pages = pages();
    let known = pages.as_ref().and_then(|p| p.get(&browser));
    let same = match known {
        Some(page) => page.wanted == script && page.applied == script,
        None => script.is_empty(),
    };
    if same {
        return false;
    }
    IN_USE.store(true, Ordering::Relaxed);
    let page = pages.get_or_insert_default().entry(browser).or_default();
    page.wanted = script;
    page.waiting.push(hold);
    drop(pages);
    Engine::post(Box::new(move || apply(browser)));
    true
}

/// Bring Chromium's script for page `browser` to the one wanted, then let
/// the waiting requests go. UI thread.
fn apply(browser: i32) {
    let (remove, add, enable) = {
        let mut pages = pages();
        let Some(page) = pages.as_mut().and_then(|p| p.get_mut(&browser)) else {
            return;
        };
        if page.busy {
            // The exchange under way comes back here.
            return;
        }
        if page.applied == page.wanted {
            let waiting = std::mem::take(&mut page.waiting);
            drop(pages);
            waiting.iter().for_each(Hold::release);
            return;
        }
        page.busy = true;
        let enable = !std::mem::replace(&mut page.enabled, true);
        (page.identifier.take(), page.wanted.clone(), enable)
    };
    let engine = Engine::global();
    let Some(id) = engine.content_id_of(browser) else {
        // No page of ours (a window Chromium opened by itself): no script.
        page_closed(browser);
        return;
    };
    // Chromium handles a page's calls in the order sent: only the last
    // one's answer is waited for.
    let ignore = || Box::new(|_| {});
    let done = move |script: String| {
        Box::new(move |result: Result<Value, String>| {
            let identifier = result
                .ok()
                .and_then(|v| v.get("identifier")?.as_str().map(str::to_string));
            exchanged(browser, script, identifier);
        })
    };
    if enable {
        engine.devtools(id, "Page.enable", &json!({}), ignore());
    }
    let remove = remove.map(|identifier| json!({ "identifier": identifier }));
    const REMOVE: &str = "Page.removeScriptToEvaluateOnNewDocument";
    match (remove, add.is_empty()) {
        (Some(old), true) => engine.devtools(id, REMOVE, &old, done(add)),
        (old, _) => {
            if let Some(old) = old {
                engine.devtools(id, REMOVE, &old, ignore());
            }
            let params = json!({ "source": add });
            engine.devtools(
                id,
                "Page.addScriptToEvaluateOnNewDocument",
                &params,
                done(add),
            );
        }
    }
}

/// Chromium answered: it has `script` for page `browser` now, under
/// `identifier`. An error counts as well, or the exchange would be tried
/// forever: removing a script answers "Script not found" although the
/// script is gone afterwards (CEF 156).
fn exchanged(browser: i32, script: String, identifier: Option<String>) {
    if let Some(page) = pages().as_mut().and_then(|p| p.get_mut(&browser)) {
        page.applied = script;
        page.identifier = identifier;
        page.busy = false;
    }
    // Wanted may have changed meanwhile.
    apply(browser);
}

/// Page `browser` closed: its requests still waiting are let go.
pub(crate) fn page_closed(browser: i32) {
    if !in_use() {
        return;
    }
    let page = pages().as_mut().and_then(|p| p.remove(&browser));
    if let Some(page) = page {
        page.waiting.iter().for_each(Hold::release);
    }
}

// Asked for the request of a page's document: the page's script becomes the
// one of the address asked for. CEF asks for a handler again after each
// redirect, so the address the document ends up at counts.
wrap_resource_request_handler! {
    struct Exchange {
        script_for: fn(&str) -> String,
    }
    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            callback: Option<&mut Callback>,
        ) -> ReturnValue {
            let (Some(browser), Some(request), Some(callback)) = (browser, request, callback) else {
                return ReturnValue::CONTINUE;
            };
            let url = CefString::from(&request.url()).to_string();
            let hold = Hold(Arc::new(Mutex::new(Some(callback.clone()))));
            if !want(browser.identifier(), (self.script_for)(&url), hold.clone()) {
                return ReturnValue::CONTINUE;
            }
            Engine::post_delayed(Box::new(move || hold.release()), WAIT_MS);
            ReturnValue::CONTINUE_ASYNC
        }
    }
}
