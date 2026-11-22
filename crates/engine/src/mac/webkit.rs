//! Pages in `WebKit`, the Mac's own web engine (`system.rs`): one `WKWebView`
//! per page, in a window of its own that lies over the page's window, where
//! a Chromium browser would be (`attach`). `WebKit` plays protected video
//! with Apple's `FairPlay`, which services that refuse Magnetite's Chromium
//! accept.
//!
//! Everything here runs on the main thread. The views are kept in a table of
//! this thread, by page id: the engine's own state holds none of them and
//! stays free to cross threads.
//!
//! Each profile has a data store of its own (cookies, storage, cache), named
//! by an identifier kept in `<data dir>/system-engine-stores.json`; a
//! private profile's store is in memory only. `WebKit` keeps the stores
//! outside the data folder, under `~/Library/WebKit/<bundle id>`.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use block2::{DynBlock, RcBlock};
use cef::{KeyEvent, KeyEventType};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{
    AllocAnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSBackingStoreType, NSEvent, NSEventModifierFlags, NSView, NSWindow,
    NSWindowDidResizeNotification, NSWindowOrderingMode, NSWindowStyleMask,
};
use objc2_foundation::{
    NSArray, NSBundle, NSData, NSDate, NSDictionary, NSError, NSHTTPCookie, NSHTTPURLResponse,
    NSKeyValueObservingOptions, NSNotification, NSNotificationCenter,
    NSObjectNSKeyValueObserverRegistration, NSRect, NSString, NSURL, NSURLRequest,
    NSURLRequestCachePolicy, NSURLSession, NSURLSessionConfiguration, NSURLSessionDataDelegate,
    NSURLSessionDataTask, NSURLSessionDelegate, NSURLSessionTask, NSURLSessionTaskDelegate, NSUUID,
    ns_string,
};
use objc2_web_kit::{
    WKNavigation, WKNavigationAction, WKNavigationActionPolicy, WKNavigationDelegate,
    WKNavigationType, WKUIDelegate, WKWebView, WKWebViewConfiguration, WKWebsiteDataRecord,
    WKWebsiteDataStore, WKWindowFeatures,
};

use crate::engine::Engine;
use crate::events::ContentId;

/// Whether this Mac can run pages in `WebKit` as Magnetite needs them: a data
/// store per profile exists since macOS 14.
pub(crate) fn available() -> bool {
    objc2::available!(macos = 14.0)
}

/// One page: its view, the object `WebKit` reports to, and, once the page
/// has its place on screen (`attach`), the window of AppKit's that holds the
/// view and the watch on the size of the page's own window.
struct Page {
    view: Retained<MagnetiteWebView>,
    delegate: Retained<PageDelegate>,
    window: Option<Retained<PageWindow>>,
    resized: Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

thread_local! {
    static PAGES: RefCell<HashMap<ContentId, Page>> = RefCell::new(HashMap::new());
    /// The data store of each profile that has shown a page this run.
    static STORES: RefCell<HashMap<String, Retained<WKWebsiteDataStore>>> = RefCell::new(HashMap::new());
}

/// What a page changes without loading anything, as `WebKit`'s properties
/// name it: watched, and reported when one differs from the last report.
const WATCHED: [&str; 5] = ["title", "URL", "loading", "canGoBack", "canGoForward"];

/// What was last reported of a page.
#[derive(Default, PartialEq, Eq)]
struct Reported {
    title: String,
    url: String,
    loading: bool,
    back: bool,
    forward: bool,
}

struct DelegateState {
    id: ContentId,
    reported: RefCell<Reported>,
    /// The address that could not be loaded, while the page shows the
    /// note about it: the page stands at that address.
    failed: RefCell<Option<String>>,
    /// The note about a failed load is about to load: that load is not a
    /// new page.
    failure_note: Cell<bool>,
    /// The address the main frame was last let go to: where a load that
    /// fails was heading, if the error does not say.
    heading: RefCell<String>,
    /// When the page's process last ended by itself.
    crashed: Cell<Option<Instant>>,
}

define_class!(
    /// The page's view: a `WKWebView` that tells the engine when it gets the
    /// keyboard and hands Magnetite's reserved shortcuts to the engine before
    /// the page sees them.
    #[unsafe(super(WKWebView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = ContentId]
    struct MagnetiteWebView;

    impl MagnetiteWebView {
        #[unsafe(method(becomeFirstResponder))]
        fn become_first_responder(&self) -> bool {
            // SAFETY: the method of the superclass, with its own signature.
            let became: bool = unsafe { msg_send![super(self), becomeFirstResponder] };
            if became {
                let id = *self.ivars();
                // Not from inside AppKit's focus change: it can run while
                // the engine shows the page.
                Engine::post(Box::new(move || Engine::global().system_focused(id)));
            }
            became
        }

        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            // SAFETY: as above.
            self.runs_shortcut(event)
                || unsafe { msg_send![super(self), performKeyEquivalent: event] }
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            if !self.runs_shortcut(event) {
                // SAFETY: as above.
                let _: () = unsafe { msg_send![super(self), keyDown: event] };
            }
        }

        /// A press in a page that does not have the keyboard is a click
        /// already, as in the other pages: the page's window is one of
        /// several that make up a Magnetite window, and a first press spent
        /// on making it the key window would look like a missed click.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }
    }
);

impl MagnetiteWebView {
    fn new(
        mtm: MainThreadMarker,
        id: ContentId,
        configuration: &WKWebViewConfiguration,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(id);
        let frame = NSRect::ZERO;
        // SAFETY: `WKWebView`'s designated initialiser.
        unsafe { msg_send![super(this), initWithFrame: frame, configuration: configuration] }
    }

    /// Whether the key is one of Magnetite's reserved shortcuts, which the
    /// engine then runs.
    fn runs_shortcut(&self, event: &NSEvent) -> bool {
        key_event(event).is_some_and(|key| Engine::global().system_key(*self.ivars(), &key))
    }
}

define_class!(
    /// The window that holds a page's view: a plain window of AppKit's
    /// without a frame, lying over the page's own window and attached to it.
    /// It takes the keyboard like the windows of the other pages, and like
    /// them is never the main window: that stays the frame's.
    #[unsafe(super(NSWindow))]
    #[thread_kind = MainThreadOnly]
    struct PageWindow;

    impl PageWindow {
        #[unsafe(method(canBecomeKeyWindow))]
        fn can_become_key(&self) -> bool {
            true
        }

        #[unsafe(method(canBecomeMainWindow))]
        fn can_become_main(&self) -> bool {
            false
        }
    }
);

define_class!(
    /// What `WebKit` reports a page's loads, new windows and changes to.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = DelegateState]
    struct PageDelegate;

    unsafe impl NSObjectProtocol for PageDelegate {}

    unsafe impl WKNavigationDelegate for PageDelegate {
        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        fn decide(
            &self,
            _web_view: &WKWebView,
            action: &WKNavigationAction,
            decision: &DynBlock<dyn Fn(WKNavigationActionPolicy)>,
        ) {
            let policy = if self.allows(action) {
                WKNavigationActionPolicy::Allow
            } else {
                WKNavigationActionPolicy::Cancel
            };
            decision.call((policy,));
        }

        #[unsafe(method(webView:didStartProvisionalNavigation:))]
        fn started(&self, web_view: &WKWebView, _navigation: Option<&WKNavigation>) {
            let state = self.ivars();
            if !state.failure_note.replace(false) {
                state.failed.replace(None);
                Engine::global().system_started(state.id, &address(web_view));
            }
            self.report(web_view);
        }

        #[unsafe(method(webView:didCommitNavigation:))]
        fn committed(&self, web_view: &WKWebView, _navigation: Option<&WKNavigation>) {
            self.report(web_view);
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        fn finished(&self, web_view: &WKWebView, _navigation: Option<&WKNavigation>) {
            self.report(web_view);
            if self.ivars().failed.borrow().is_none() {
                self.find_icon(web_view);
            }
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn failed_early(
            &self,
            web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
            error: &NSError,
        ) {
            self.load_failed(web_view, error);
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn failed(&self, web_view: &WKWebView, _navigation: Option<&WKNavigation>, error: &NSError) {
            self.load_failed(web_view, error);
        }

        #[unsafe(method(webViewWebContentProcessDidTerminate:))]
        fn process_ended(&self, web_view: &WKWebView) {
            // Once: a page that ends its process whenever it loads would
            // otherwise load for ever.
            let state = self.ivars();
            let again = state
                .crashed
                .replace(Some(Instant::now()))
                .is_none_or(|last| last.elapsed() > CRASH_PAUSE);
            crate::trace(&format!("system page #{}: its process ended", state.id));
            if again {
                // SAFETY: a live view, on the main thread.
                unsafe { web_view.reload() };
            }
        }
    }

    unsafe impl WKUIDelegate for PageDelegate {
        /// A link or script asks for a new window: the app opens a tab (or
        /// a peek) for it, if it is the web's (`is_web_address`). A window
        /// for anything else (`about:blank` to write into, a `blob:` of
        /// this page that no other page could read) opens nothing. The
        /// page gets no window object back, so a sign-in window that
        /// reports to its opener cannot.
        #[unsafe(method_id(webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:))]
        fn new_window(
            &self,
            _web_view: &WKWebView,
            _configuration: &WKWebViewConfiguration,
            action: &WKNavigationAction,
            features: &WKWindowFeatures,
        ) -> Option<Retained<WKWebView>> {
            // SAFETY: live objects WebKit passes for the call.
            let (url, sized) = unsafe { (request_address(&action.request()), features.width().is_some()) };
            if let Some(url) = url.filter(|url| is_web_address(url)) {
                Engine::global().system_open(self.ivars().id, url, false, sized);
            }
            None
        }

        #[unsafe(method(webViewDidClose:))]
        fn closed_itself(&self, _web_view: &WKWebView) {
            let id = self.ivars().id;
            Engine::post(Box::new(move || Engine::global().close_content(id)));
        }
    }

    impl PageDelegate {
        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        fn observed(
            &self,
            _key_path: Option<&NSString>,
            object: Option<&AnyObject>,
            _change: Option<&NSDictionary<NSString, AnyObject>>,
            _context: *mut std::ffi::c_void,
        ) {
            if let Some(web_view) = object.and_then(|o| o.downcast_ref::<WKWebView>()) {
                self.report(web_view);
            }
        }
    }
);

/// A page whose process ended by itself is loaded again unless it ended
/// less than this long ago.
const CRASH_PAUSE: Duration = Duration::from_secs(10);

/// `NSURLErrorCancelled`: a load given up for another one.
const CANCELLED: isize = -999;
/// `NSURLErrorCannotConnectToHost`: nothing listens at the address.
const REFUSED: isize = -1004;
/// `WebKit`'s "frame load interrupted": what a load that became a download,
/// or that `allows` turned down, ends with.
const INTERRUPTED: isize = 102;

impl PageDelegate {
    fn new(mtm: MainThreadMarker, id: ContentId) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateState {
            id,
            reported: RefCell::default(),
            failed: RefCell::new(None),
            failure_note: Cell::new(false),
            heading: RefCell::default(),
            crashed: Cell::new(None),
        });
        // SAFETY: `NSObject`'s initialiser.
        unsafe { msg_send![super(this), init] }
    }

    /// Whether the page may go where `action` leads. What it may not is
    /// handed to the app where a tab is meant, and dropped otherwise.
    fn allows(&self, action: &WKNavigationAction) -> bool {
        let id = self.ivars().id;
        // SAFETY: live objects WebKit passes for the call.
        let (main, url, kind, flags, get, download) = unsafe {
            let request = action.request();
            (
                // No target frame: a new window, decided where it is asked for.
                action.targetFrame().map(|f| f.isMainFrame()),
                request_address(&request),
                action.navigationType(),
                action.modifierFlags(),
                request.HTTPMethod().is_none_or(|m| m.to_string() == "GET"),
                action.shouldPerformDownload(),
            )
        };
        if download {
            crate::trace(&format!("system page #{id}: a download, not started"));
            return false;
        }
        let (Some(true), Some(url)) = (main, url) else {
            return true;
        };
        if !is_web_address(&url) {
            // The one page that is not the web's: the note about a failed
            // load, when it loads and when the history comes back to it.
            let back = kind == WKNavigationType::BackForward || kind == WKNavigationType::Reload;
            return url == FAILURE_NOTE_ADDRESS && (self.ivars().failure_note.get() || back);
        }
        if !(kind == WKNavigationType::LinkActivated && get) {
            self.ivars().heading.replace(url);
            return true;
        }
        let engine = Engine::global();
        let command = flags.contains(NSEventModifierFlags::Command);
        let shift = flags.contains(NSEventModifierFlags::Shift);
        // A click with a modifier, as in the other pages: Cmd opens a tab
        // behind (with Shift in front), Shift alone asks for a window.
        if command || shift {
            engine.system_open(id, url, command && !shift, !command);
            return false;
        }
        let stays = !engine.system_leaves_lock(id, &url);
        if stays {
            self.ivars().heading.replace(url);
        }
        stays
    }

    /// Tell the engine what changed since the last report.
    fn report(&self, web_view: &WKWebView) {
        let state = self.ivars();
        // SAFETY: a live view, on the main thread.
        let now = unsafe {
            Reported {
                title: web_view.title().map(|t| t.to_string()).unwrap_or_default(),
                // The note about a failed load stands for the address that failed.
                url: (state.failed.borrow().clone()).unwrap_or_else(|| address(web_view)),
                loading: web_view.isLoading(),
                back: web_view.canGoBack(),
                forward: web_view.canGoForward(),
            }
        };
        let (title, url, nav) = {
            let was = state.reported.borrow();
            (
                (was.title != now.title).then(|| now.title.clone()),
                (was.url != now.url && !now.url.is_empty()).then(|| now.url.clone()),
                (was.loading, was.back, was.forward) != (now.loading, now.back, now.forward),
            )
        };
        let (loading, back, forward) = (now.loading, now.back, now.forward);
        state.reported.replace(now);
        let engine = Engine::global();
        if let Some(url) = url {
            engine.system_url(state.id, url);
        }
        if let Some(title) = title {
            engine.system_title(state.id, title);
        }
        if nav {
            engine.system_loading(state.id, loading, back, forward);
        }
    }

    /// A load failed: the app hears of it, and the page says so.
    fn load_failed(&self, web_view: &WKWebView, error: &NSError) {
        let code = error.code();
        if code == CANCELLED || code == INTERRUPTED {
            self.report(web_view);
            return;
        }
        let state = self.ivars();
        let named = (error.userInfo())
            .objectForKey(ns_string!("NSErrorFailingURLStringKey"))
            .and_then(|v| v.downcast::<NSString>().ok())
            .map(|s| s.to_string());
        // A first load that fails leaves the view without an address.
        let url = named
            .or_else(|| Some(state.heading.borrow().clone()))
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| address(web_view));
        let reason = error.localizedDescription().to_string();
        crate::trace(&format!("system page #{}: load failed ({code})", state.id));
        state.failed.replace(Some(url.clone()));
        state.failure_note.set(true);
        let note = NSString::from_str(&failure_note(&url, &reason));
        // SAFETY: a live view, on the main thread.
        unsafe { web_view.loadHTMLString_baseURL(&note, None) };
        Engine::global().system_failed(state.id, url, code == REFUSED);
    }

    /// Ask the page for its icon's address and hand it to the engine, if
    /// the page is still the one that was asked: the answer comes later,
    /// and the tab may have gone elsewhere by then.
    fn find_icon(&self, web_view: &WKWebView) {
        let id = self.ivars().id;
        let asked = address(web_view);
        let found = RcBlock::new(move |value: *mut AnyObject, _error: *mut NSError| {
            // SAFETY: WebKit passes the script's result, or null.
            let url = unsafe { value.as_ref() }
                .and_then(|v| v.downcast_ref::<NSString>())
                .map(ToString::to_string);
            if let Some(url) = url.filter(|_| address_of(id).as_deref() == Some(asked.as_str())) {
                Engine::global().system_favicon(id, &url);
            }
        });
        // SAFETY: a live view, on the main thread.
        unsafe {
            web_view.evaluateJavaScript_completionHandler(ns_string!(ICON_SCRIPT), Some(&found));
        }
    }
}

/// The page's icon as its document names it (of several the one nearest to
/// the size the sidebar keeps, and none that is a drawing: the cache holds
/// pictures), else the site's `favicon.ico`.
const ICON_SCRIPT: &str = "(()=>{const size=l=>parseInt((l.sizes&&l.sizes.value||'').split('x')[0])\
||(l.rel.includes('apple')?180:16);const icons=[...document.querySelectorAll(\
'link[rel~=\"icon\"],link[rel=\"apple-touch-icon\"]')].filter(l=>l.href&&!/svg/.test(l.type)\
&&!/\\.svg([?#]|$)/.test(l.href)).sort((a,b)=>Math.abs(size(a)-64)-Math.abs(size(b)-64));\
return icons[0]?icons[0].href:location.origin+'/favicon.ico'})()";

/// Where the note about a failed load stands for `WebKit`: it is loaded
/// from a text, without an address of its own.
const FAILURE_NOTE_ADDRESS: &str = "about:blank";

/// Whether `url` is an `http://` or `https://` address: all a page's main
/// frame may be sent to. Anything else (`mailto:`, an app's own scheme, a
/// `data:` or `blob:` address) is not opened from here.
fn is_web_address(url: &str) -> bool {
    (url.split_once(':')).is_some_and(|(scheme, _)| {
        scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
    })
}

/// The address `request` asks for, if it has one.
///
/// # Safety
/// `request` is a live request, on the main thread.
unsafe fn request_address(request: &NSURLRequest) -> Option<String> {
    (request.URL())
        .and_then(|u| u.absoluteString())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

/// The address `web_view` shows (empty before its first load).
fn address(web_view: &WKWebView) -> String {
    // SAFETY: a live view, on the main thread.
    unsafe { web_view.URL() }
        .and_then(|u| u.absoluteString())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

/// The page shown where `url` could not be loaded: what failed and why,
/// in plain words, with nothing of the text left for the browser to read
/// as markup.
fn failure_note(url: &str, reason: &str) -> String {
    let plain = |text: &str| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    format!(
        "<!doctype html><meta charset=utf-8><meta name=color-scheme content=\"light dark\">\
<title>{title}</title><style>body{{font:15px -apple-system,sans-serif;margin:18vh auto 0;max-width:34em;\
padding:0 2em;line-height:1.5}}h1{{font-size:20px}}p{{opacity:.7;overflow-wrap:anywhere}}</style>\
<h1>This page could not be loaded</h1><p>{reason}</p><p>{url}</p>",
        title = plain(url),
        reason = plain(reason),
        url = plain(url),
    )
}

/// The key press `event` as the engine's shortcut tables read keys: Windows
/// key codes, CEF's modifier flags. `None` for a key no shortcut uses.
fn key_event(event: &NSEvent) -> Option<KeyEvent> {
    use cef::sys::cef_event_flags_t as Flag;
    let held = event.modifierFlags();
    let mut modifiers = 0;
    for (mac, cef) in [
        (NSEventModifierFlags::Command, Flag::EVENTFLAG_COMMAND_DOWN),
        (NSEventModifierFlags::Shift, Flag::EVENTFLAG_SHIFT_DOWN),
        (NSEventModifierFlags::Option, Flag::EVENTFLAG_ALT_DOWN),
        (NSEventModifierFlags::Control, Flag::EVENTFLAG_CONTROL_DOWN),
    ] {
        if held.contains(mac) {
            modifiers |= cef.0;
        }
    }
    let typed = event
        .charactersIgnoringModifiers()
        .and_then(|s| s.to_string().chars().next());
    Some(KeyEvent {
        type_: KeyEventType::RAWKEYDOWN,
        modifiers,
        windows_key_code: windows_key(typed, event.keyCode())?,
        ..Default::default()
    })
}

/// The Windows key code of a key: by the character it types where it types
/// one (so a shortcut follows the keyboard layout), else by where it lies.
fn windows_key(typed: Option<char>, key_code: u16) -> Option<i32> {
    const PUNCTUATION: [(char, i32); 11] = [
        (';', 0xBA),
        ('=', 0xBB),
        (',', 0xBC),
        ('-', 0xBD),
        ('.', 0xBE),
        ('/', 0xBF),
        ('`', 0xC0),
        ('[', 0xDB),
        ('\\', 0xDC),
        (']', 0xDD),
        ('\'', 0xDE),
    ];
    // Apple's key codes (`kVK_…`) of the keys that type no character, and
    // of the digits: with Shift a digit key types its symbol.
    const BY_PLACE: [(u16, i32); 37] = [
        (0x1D, 0x30),
        (0x12, 0x31),
        (0x13, 0x32),
        (0x14, 0x33),
        (0x15, 0x34),
        (0x17, 0x35),
        (0x16, 0x36),
        (0x1A, 0x37),
        (0x1C, 0x38),
        (0x19, 0x39),
        (0x33, 0x08), // Backspace
        (0x30, 0x09), // Tab
        (0x24, 0x0D), // Enter
        (0x35, 0x1B), // Escape
        (0x31, 0x20), // Space
        (0x74, 0x21), // Page Up
        (0x79, 0x22), // Page Down
        (0x77, 0x23), // End
        (0x73, 0x24), // Home
        (0x7B, 0x25), // Left
        (0x7E, 0x26), // Up
        (0x7C, 0x27), // Right
        (0x7D, 0x28), // Down
        (0x75, 0x2E), // Delete
        (0x7A, 0x70), // F1 …
        (0x78, 0x71),
        (0x63, 0x72),
        (0x76, 0x73),
        (0x60, 0x74),
        (0x61, 0x75),
        (0x62, 0x76),
        (0x64, 0x77),
        (0x65, 0x78),
        (0x6D, 0x79),
        (0x67, 0x7A),
        (0x6F, 0x7B), // … F12
        (0x4C, 0x0D), // Enter of the number pad
    ];
    if let Some(c) = typed {
        let upper = c.to_ascii_uppercase();
        if upper.is_ascii_uppercase() || upper.is_ascii_digit() {
            return Some(upper as i32);
        }
        if let Some((_, code)) = PUNCTUATION.iter().find(|(p, _)| *p == c) {
            return Some(*code);
        }
    }
    (BY_PLACE.iter())
        .find(|(place, _)| *place == key_code)
        .map(|(_, code)| *code)
}

/// The file that names each profile's data store: profile id → identifier.
fn stores_file(data_dir: &Path) -> PathBuf {
    data_dir.join("system-engine-stores.json")
}

/// The identifiers in the file; none where there is no file yet. A file
/// that cannot be read, or holds something else, is an error and not an
/// empty list: its stores exist, and nothing may act as if they did not.
fn read_store_ids(data_dir: &Path) -> std::io::Result<BTreeMap<String, String>> {
    match std::fs::read(stores_file(data_dir)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(std::io::Error::other),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e),
    }
}

/// What a caller hears when the file of identifiers cannot be read.
const STORES_UNREADABLE: &str = "The list of WebKit's data stores could not be read.";

/// Keep `ids` in the file. False if they could not be written.
fn write_store_ids(data_dir: &Path, ids: &BTreeMap<String, String>) -> bool {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let path = stores_file(data_dir);
    let tmp = path.with_extension("tmp");
    let written = serde_json::to_vec_pretty(ids).ok().and_then(|json| {
        let mut file = (std::fs::OpenOptions::new())
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .ok()?;
        file.write_all(&json).ok()?;
        std::fs::rename(&tmp, &path).ok()
    });
    if written.is_none() {
        crate::trace("system engine: cannot keep the list of its data stores");
    }
    written.is_some()
}

/// The identifier of `profile`'s data store, made and kept on first use.
/// None if a new one could not be kept, or the file cannot be read: a
/// store under an identifier that is written nowhere could never be found
/// again, to open or to delete.
fn store_identifier(data_dir: &Path, profile: &str) -> Option<Retained<NSUUID>> {
    let Ok(mut ids) = read_store_ids(data_dir) else {
        crate::trace("system engine: the list of its data stores cannot be read");
        return None;
    };
    let known = (ids.get(profile))
        .and_then(|id| NSUUID::initWithUUIDString(NSUUID::alloc(), &NSString::from_str(id)));
    known.or_else(|| {
        let fresh = NSUUID::new();
        ids.insert(profile.to_string(), fresh.UUIDString().to_string());
        write_store_ids(data_dir, &ids).then_some(fresh)
    })
}

/// The data store of `profile` (`""`: the default profile): on disk under
/// the profile's identifier. In memory only for a private profile, and for
/// this run where the identifier could not be kept.
fn store(
    mtm: MainThreadMarker,
    data_dir: &Path,
    profile: &str,
    ephemeral: bool,
) -> Retained<WKWebsiteDataStore> {
    if let Some(store) = STORES.with_borrow(|s| s.get(profile).cloned()) {
        return store;
    }
    let identifier = (!ephemeral)
        .then(|| store_identifier(data_dir, profile))
        .flatten();
    // SAFETY: on the main thread (`mtm`); `available` holds, or no page
    // would be made.
    let store = unsafe {
        match identifier {
            Some(identifier) => WKWebsiteDataStore::dataStoreForIdentifier(&identifier, mtm),
            None => WKWebsiteDataStore::nonPersistentDataStore(mtm),
        }
    };
    STORES.with_borrow_mut(|s| s.insert(profile.to_string(), store.clone()));
    store
}

/// The profiles `ids` are gone: so are their data stores. A store's
/// identifier leaves the file only once `WebKit` has removed the store: one
/// that could not be removed (a page still shows it) is asked for again at
/// the next start, when the app names its deleted profiles again.
pub(crate) fn forget_stores(data_dir: &Path, ids: &[String]) {
    // Stores by identifier, and the call that removes one, exist where
    // `available` holds; elsewhere nothing was ever made.
    let Some(mtm) = MainThreadMarker::new().filter(|_| available()) else {
        return;
    };
    let Ok(known) = read_store_ids(data_dir) else {
        crate::trace("system engine: the list of its data stores cannot be read");
        return;
    };
    for profile in ids {
        STORES.with_borrow_mut(|s| s.remove(profile));
        let uuid = (known.get(profile))
            .and_then(|id| NSUUID::initWithUUIDString(NSUUID::alloc(), &NSString::from_str(id)));
        let Some(uuid) = uuid else { continue };
        let (data_dir, profile) = (data_dir.to_path_buf(), profile.clone());
        let done = RcBlock::new(move |error: *mut NSError| {
            if !error.is_null() {
                crate::trace("system engine: a profile's data store could not be removed");
                return;
            }
            if let Ok(mut ids) = read_store_ids(&data_dir)
                && ids.remove(&profile).is_some()
            {
                write_store_ids(&data_dir, &ids);
            }
        });
        // SAFETY: on the main thread; WebKit copies the block.
        unsafe {
            WKWebsiteDataStore::removeDataStoreForIdentifier_completionHandler(&uuid, &done, mtm);
        }
    }
}

/// The private profiles `ids` have no page left: their stores, which are
/// in memory only, go.
pub(crate) fn release_stores(ids: &[String]) {
    STORES.with_borrow_mut(|stores| {
        // SAFETY: on the main thread, where the table lives.
        stores.retain(|profile, store| !ids.contains(profile) || unsafe { store.isPersistent() });
    });
}

/// Delete what `WebKit` keeps for `profile`: everything (`domain` none), or
/// the cookies, storage and cache of the site `domain` and its subdomains.
/// `done` hears how many cookies went with a site's data, or why nothing
/// was deleted: a profile whose store on disk cannot be named
/// (`read_store_ids`) is not reported as cleared.
pub(crate) fn clear_data(
    data_dir: &Path,
    profile: &str,
    ephemeral: bool,
    domain: Option<&str>,
    done: Box<dyn FnOnce(Result<Option<usize>, String>)>,
) {
    let nothing = domain.map(|_| 0);
    let Some(mtm) = MainThreadMarker::new().filter(|_| available()) else {
        return done(Ok(nothing));
    };
    // The stores that hold the profile's data: the one its pages use this
    // run, and the one on disk under its identifier where that is another.
    // A run that began while the file could not be read uses a store in
    // memory, and the one on disk must not pass for cleared with it.
    let in_use = STORES.with_borrow(|s| s.get(profile).cloned());
    let mut stores: Vec<Retained<WKWebsiteDataStore>> = in_use.iter().cloned().collect();
    if !ephemeral {
        let Ok(ids) = read_store_ids(data_dir) else {
            return done(Err(STORES_UNREADABLE.to_string()));
        };
        // SAFETY: on the main thread, where the stores live.
        let on_disk = in_use.is_some_and(|store| unsafe { store.isPersistent() });
        let recorded = (ids.get(profile))
            .and_then(|id| NSUUID::initWithUUIDString(NSUUID::alloc(), &NSString::from_str(id)));
        if let Some(recorded) = recorded.filter(|_| !on_disk) {
            // SAFETY: on the main thread; `available` holds.
            stores.push(unsafe { WKWebsiteDataStore::dataStoreForIdentifier(&recorded, mtm) });
        }
    }
    // A profile that never used WebKit has nothing to delete, and gets no
    // store made for the question.
    if stores.is_empty() {
        return done(Ok(nothing));
    }
    let left = Rc::new(Cell::new(stores.len()));
    let cookies = Rc::new(Cell::new(0));
    let done = Rc::new(Cell::new(Some(done)));
    let site = domain.is_some();
    for store in stores {
        let (left, cookies, done) = (left.clone(), cookies.clone(), done.clone());
        let cleared = move |count: usize| {
            cookies.set(cookies.get() + count);
            left.set(left.get() - 1);
            // `done` is taken by the last store alone.
            if left.get() == 0
                && let Some(done) = done.take()
            {
                done(Ok(site.then(|| cookies.get())));
            }
        };
        clear_store(mtm, &store, domain, Rc::new(cleared));
    }
}

/// Delete everything in `store`, or what belongs to the site `domain`.
/// `done` hears how many of the site's cookies went (0 when everything
/// goes: they are not counted then).
fn clear_store(
    mtm: MainThreadMarker,
    store: &Retained<WKWebsiteDataStore>,
    domain: Option<&str>,
    done: Rc<dyn Fn(usize)>,
) {
    // SAFETY: on the main thread; WebKit copies the blocks.
    unsafe {
        let types = WKWebsiteDataStore::allWebsiteDataTypes(mtm);
        let Some(domain) = domain.map(str::to_string) else {
            // The block keeps the store: one that nothing else holds (the
            // one on disk beside a store in use) would be gone before it
            // is done, and `done` would never be called.
            let kept = store.clone();
            let removed = RcBlock::new(move || {
                let _ = &kept;
                done(0);
            });
            let ever = NSDate::distantPast();
            store.removeDataOfTypes_modifiedSince_completionHandler(&types, &ever, &removed);
            return;
        };
        // First the count of the site's cookies, then everything of the
        // site, cookies included.
        let from = store.clone();
        let counted = RcBlock::new(move |cookies: NonNull<NSArray<NSHTTPCookie>>| {
            let count = (cookies.as_ref().iter())
                .filter(|c| crate::sitedata::in_domain(&c.domain().to_string(), &domain))
                .count();
            let (site, store, kinds, done) =
                (domain.clone(), from.clone(), types.clone(), done.clone());
            let found = RcBlock::new(move |records: NonNull<NSArray<WKWebsiteDataRecord>>| {
                let of_site: Vec<Retained<WKWebsiteDataRecord>> = (records.as_ref().iter())
                    .filter(|r| crate::sitedata::in_domain(&r.displayName().to_string(), &site))
                    .collect();
                // Kept by the last block too, as above.
                let (done, kept) = (done.clone(), store.clone());
                let removed = RcBlock::new(move || {
                    let _ = &kept;
                    done(count);
                });
                let of_site = NSArray::from_retained_slice(&of_site);
                store
                    .removeDataOfTypes_forDataRecords_completionHandler(&kinds, &of_site, &removed);
            });
            from.fetchDataRecordsOfTypes_completionHandler(&types, &found);
        });
        store.httpCookieStore().getAllCookies(&counted);
    }
}

/// The largest icon a site may hand over: more is not an icon.
const ICON_LIMIT: usize = 1 << 20;

/// Whether an icon may be fetched from `url`: over an encrypted connection
/// only, and with no name or password in the address.
fn fetchable(url: &NSURL) -> bool {
    let https = url
        .scheme()
        .is_some_and(|s| s.to_string().eq_ignore_ascii_case("https"));
    https && url.user().is_none() && url.password().is_none()
}

/// One icon on its way: where it is kept, and what has arrived of it (none
/// once that was more than `ICON_LIMIT`).
struct IconFetchState {
    path: PathBuf,
    body: Mutex<Option<Vec<u8>>>,
}

define_class!(
    /// What the session fetching an icon reports to, on a thread of its
    /// own: a redirect is followed only to an address `fetchable` allows,
    /// the fetch ends when more than an icon arrives, and what arrived is
    /// kept as the icon once it is complete.
    #[unsafe(super(NSObject))]
    #[ivars = IconFetchState]
    struct IconFetch;

    unsafe impl NSObjectProtocol for IconFetch {}

    unsafe impl NSURLSessionDelegate for IconFetch {}

    unsafe impl NSURLSessionTaskDelegate for IconFetch {
        #[unsafe(method(URLSession:task:willPerformHTTPRedirection:newRequest:completionHandler:))]
        fn redirected(
            &self,
            _session: &NSURLSession,
            _task: &NSURLSessionTask,
            _response: &NSHTTPURLResponse,
            request: &NSURLRequest,
            follow: &DynBlock<dyn Fn(*mut NSURLRequest)>,
        ) {
            let allowed = request.URL().is_some_and(|url| fetchable(&url));
            follow.call((if allowed {
                std::ptr::from_ref(request).cast_mut()
            } else {
                std::ptr::null_mut()
            },));
        }

        #[unsafe(method(URLSession:task:didCompleteWithError:))]
        fn completed(
            &self,
            _session: &NSURLSession,
            _task: &NSURLSessionTask,
            error: Option<&NSError>,
        ) {
            let state = self.ivars();
            let body = state.body.lock().ok().and_then(|mut body| body.take());
            let png = body
                .filter(|_| error.is_none())
                .and_then(|bytes| png_of(&NSData::with_bytes(&bytes)));
            if let Some(png) = png {
                crate::favicon::store_icon(&state.path, &png);
            }
        }
    }

    unsafe impl NSURLSessionDataDelegate for IconFetch {
        #[unsafe(method(URLSession:dataTask:didReceiveData:))]
        fn received(&self, _session: &NSURLSession, task: &NSURLSessionDataTask, data: &NSData) {
            let Ok(mut body) = self.ivars().body.lock() else {
                return;
            };
            match body.as_mut() {
                Some(bytes) if bytes.len() + data.length() <= ICON_LIMIT => {
                    bytes.extend_from_slice(&data.to_vec());
                }
                _ => {
                    *body = None;
                    task.cancel();
                }
            }
        }
    }
);

/// Fetch the image at `icon_url` and keep it as a PNG at `path`. The
/// request is made outside every profile's store, in a session that has no
/// place for cookies: it carries none and keeps none, also across a
/// redirect.
pub(crate) fn fetch_icon(icon_url: &str, path: PathBuf) {
    let url = NSURL::URLWithString(&NSString::from_str(icon_url)).filter(|url| fetchable(url));
    let Some(url) = url else { return };
    let configuration = NSURLSessionConfiguration::ephemeralSessionConfiguration();
    configuration.setHTTPCookieStorage(None);
    configuration.setHTTPShouldSetCookies(false);
    let fetch = IconFetch::alloc().set_ivars(IconFetchState {
        path,
        body: Mutex::new(Some(Vec::new())),
    });
    // SAFETY: `NSObject`'s initialiser.
    let fetch: Retained<IconFetch> = unsafe { msg_send![super(fetch), init] };
    // SAFETY: the session keeps its delegate until its one task is done.
    let session = unsafe {
        NSURLSession::sessionWithConfiguration_delegate_delegateQueue(
            &configuration,
            Some(ProtocolObject::from_ref(&*fetch)),
            None,
        )
    };
    session.dataTaskWithURL(&url).resume();
    session.finishTasksAndInvalidate();
}

/// The image in `data` (PNG, ICO, JPEG, GIF, …) as a PNG: of an icon file
/// with several sizes, the largest.
fn png_of(data: &NSData) -> Option<Vec<u8>> {
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep};
    let largest = NSBitmapImageRep::imageRepsWithData(data)
        .iter()
        .filter_map(|rep| rep.downcast::<NSBitmapImageRep>().ok())
        .max_by_key(|rep| rep.pixelsWide())?;
    // SAFETY: a bitmap that is ours alone, and no properties.
    let png = unsafe {
        largest.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }?;
    Some(png.to_vec())
}

/// Safari's version on this Mac, which `WebKit`'s own user agent leaves out:
/// with it a page reads the view as the Safari it is the engine of. Sites
/// that offer `FairPlay` look for it.
fn safari_version() -> String {
    NSBundle::bundleWithPath(ns_string!("/Applications/Safari.app"))
        .and_then(|safari| {
            safari.objectForInfoDictionaryKey(ns_string!("CFBundleShortVersionString"))
        })
        .and_then(|version| version.downcast::<NSString>().ok())
        .map_or_else(|| "17.6".to_string(), |v| v.to_string())
}

/// Make page `id`'s view, in `profile`'s data store, loading `url`. It is
/// not in a window yet (`attach`). False if it could not be made.
pub(crate) fn create(
    id: ContentId,
    url: &str,
    data_dir: &Path,
    profile: &str,
    ephemeral: bool,
) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let store = store(mtm, data_dir, profile, ephemeral);
    // SAFETY: WebKit's objects, made and set up on the main thread.
    let (view, delegate) = unsafe {
        let configuration = WKWebViewConfiguration::new(mtm);
        configuration.setWebsiteDataStore(&store);
        let app = NSString::from_str(&format!("Version/{} Safari/605.1.15", safari_version()));
        configuration.setApplicationNameForUserAgent(Some(&app));
        let preferences = configuration.preferences();
        // A video's own full-screen button.
        preferences.setElementFullscreenEnabled(true);
        // No check of addresses against a list of fraudulent sites, as in
        // Magnetite's Chromium pages: it asks a service.
        preferences.setFraudulentWebsiteWarningEnabled(false);
        let view = MagnetiteWebView::new(mtm, id, &configuration);
        let delegate = PageDelegate::new(mtm, id);
        view.setNavigationDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        view.setUIDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        view.setAllowsBackForwardNavigationGestures(true);
        for key in WATCHED {
            view.addObserver_forKeyPath_options_context(
                &delegate,
                &NSString::from_str(key),
                NSKeyValueObservingOptions::New,
                std::ptr::null_mut(),
            );
        }
        (view, delegate)
    };
    let page = Page {
        view,
        delegate,
        window: None,
        resized: None,
    };
    PAGES.with_borrow_mut(|p| p.insert(id, page));
    load(id, url);
    true
}

/// Page `id`'s view, if it has one. Never held across a call into `WebKit`
/// that can report back.
fn view_of(id: ContentId) -> Option<Retained<MagnetiteWebView>> {
    PAGES.with_borrow(|p| p.get(&id).map(|page| page.view.clone()))
}

/// Give page `id` its place on screen: over the window whose content view
/// is `handle` (the page's own, which CEF made and the engine shows, hides,
/// moves and sizes), in a window attached to that one. The window follows
/// its parent by itself; its size is kept the parent's.
///
/// Not inside CEF's window. Its content view answers every hit test with
/// itself, so a view laid into it never gets the mouse, and a view laid
/// beside the content view gets presses but almost none of the pointer's
/// moves: `WebKit` then shows no hover and no cursor of its own. In a
/// window of AppKit's the view is what it is in any other app.
pub(crate) fn attach(id: ContentId, handle: cef::sys::cef_window_handle_t) {
    let (Some(view), Some(mtm)) = (view_of(id), MainThreadMarker::new()) else {
        return;
    };
    if handle.is_null() {
        return;
    }
    // SAFETY: CEF hands us a live `NSView*`.
    let host: &NSView = unsafe { &*handle.cast() };
    let Some(parent) = host.window() else { return };
    // SAFETY: a window without a frame, kept by the page until `close`.
    let window: Retained<PageWindow> = unsafe {
        let window: Retained<PageWindow> = msg_send![
            PageWindow::alloc(mtm),
            initWithContentRect: parent.frame(),
            styleMask: NSWindowStyleMask::Borderless,
            backing: NSBackingStoreType::Buffered,
            defer: false
        ];
        window.setReleasedWhenClosed(false);
        window
    };
    window.setMovable(false);
    window.setHasShadow(false);
    let Some(content) = window.contentView() else {
        return;
    };
    view.setFrame(content.bounds());
    view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    content.addSubview(&view);
    // The corners and the clear background of the page's own window.
    // SAFETY: CoreAnimation's getter on the layer of a view alive here.
    let radius: f64 = unsafe {
        let layer: Option<Retained<AnyObject>> = msg_send![host, layer];
        layer.map_or(0.0, |layer| msg_send![&*layer, cornerRadius])
    };
    let content_handle: cef::sys::cef_window_handle_t =
        Retained::as_ptr(&content).cast_mut().cast();
    super::round_window_corners(content_handle, radius, false);
    super::mark_as_part(content_handle);
    // SAFETY: two windows alive here; the parent lets go of the child in
    // `close`.
    unsafe { parent.addChildWindow_ordered(&window, NSWindowOrderingMode::Above) };

    let own = window.clone();
    let follow = RcBlock::new(move |notification: NonNull<NSNotification>| {
        // SAFETY: AppKit passes a live notification for the call.
        let parent = unsafe { notification.as_ref() }
            .object()
            .and_then(|o| o.downcast::<NSWindow>().ok());
        if let Some(parent) = parent {
            own.setFrame_display(parent.frame(), true);
        }
    });
    // SAFETY: the centre copies the block; the watch ends in `close`.
    let resized = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSWindowDidResizeNotification),
            Some(&parent),
            None,
            &follow,
        )
    };
    PAGES.with_borrow_mut(|p| {
        if let Some(page) = p.get_mut(&id) {
            page.window = Some(window);
            page.resized = Some(resized);
        }
    });
}

/// The content view of the window a system page shows in, if `window` is
/// such a page's own window: what lies over it, and so what an outline of
/// the page has to be drawn on.
pub(crate) fn holder_in(window: &NSWindow) -> Option<Retained<NSView>> {
    PAGES.with_borrow(|pages| {
        (pages.values().filter_map(|page| page.window.as_ref()))
            .find(|own| own.parentWindow().is_some_and(|parent| &*parent == window))
            .and_then(|own| own.contentView())
    })
}

/// Page `id` is gone: `WebKit` stops reporting, and its window closes.
pub(crate) fn close(id: ContentId) {
    let Some(page) = PAGES.with_borrow_mut(|p| p.remove(&id)) else {
        return;
    };
    // SAFETY: the view and its delegate, alive until the end of this call.
    unsafe {
        for key in WATCHED {
            page.view
                .removeObserver_forKeyPath(&page.delegate, &NSString::from_str(key));
        }
        page.view.setNavigationDelegate(None);
        page.view.setUIDelegate(None);
        page.view.stopLoading();
        if let Some(resized) = &page.resized {
            NSNotificationCenter::defaultCenter()
                .removeObserver(ProtocolObject::as_ref(&**resized));
        }
    }
    page.view.removeFromSuperview();
    if let Some(window) = page.window {
        if let Some(parent) = window.parentWindow() {
            parent.removeChildWindow(&window);
        }
        window.close();
    }
}

/// The address page `id` shows.
pub(crate) fn address_of(id: ContentId) -> Option<String> {
    view_of(id)
        .map(|view| address(&view))
        .filter(|url| !url.is_empty())
}

/// Seconds a load past the cache may wait: `NSURLRequest`'s default.
const LOAD_TIMEOUT: f64 = 60.0;

/// Load `url` in page `id`.
pub(crate) fn load(id: ContentId, url: &str) {
    let Some(view) = view_of(id) else { return };
    let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) else {
        crate::trace(&format!("system page #{id}: not an address, not loaded"));
        return;
    };
    // SAFETY: a live view, on the main thread.
    unsafe { view.loadRequest(&NSURLRequest::requestWithURL(&url)) };
}

/// `load`, with nothing taken from the cache.
fn load_past_cache(id: ContentId, url: &str) {
    let Some(view) = view_of(id) else { return };
    let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) else {
        crate::trace(&format!("system page #{id}: not an address, not loaded"));
        return;
    };
    let request = NSURLRequest::requestWithURL_cachePolicy_timeoutInterval(
        &url,
        NSURLRequestCachePolicy::ReloadIgnoringLocalCacheData,
        LOAD_TIMEOUT,
    );
    // SAFETY: a live view, on the main thread.
    unsafe { view.loadRequest(&request) };
}

pub(crate) fn go_back(id: ContentId) {
    if let Some(view) = view_of(id) {
        // SAFETY: a live view, on the main thread.
        unsafe { view.goBack() };
    }
}

pub(crate) fn go_forward(id: ContentId) {
    if let Some(view) = view_of(id) {
        // SAFETY: as above.
        unsafe { view.goForward() };
    }
}

/// Load page `id` again: the address that failed, where it shows the note
/// about a failed load.
pub(crate) fn reload(id: ContentId) {
    reload_page(id, false);
}

/// `reload`, past the cache.
pub(crate) fn hard_reload(id: ContentId) {
    reload_page(id, true);
}

fn reload_page(id: ContentId, past_cache: bool) {
    let failed = PAGES.with_borrow(|p| {
        (p.get(&id)).and_then(|page| page.delegate.ivars().failed.borrow().clone())
    });
    match (failed, view_of(id)) {
        (Some(url), _) if past_cache => load_past_cache(id, &url),
        (Some(url), _) => load(id, &url),
        (None, Some(view)) if past_cache => {
            // SAFETY: as above.
            unsafe { view.reloadFromOrigin() };
        }
        (None, Some(view)) => {
            // SAFETY: as above.
            unsafe { view.reload() };
        }
        (None, None) => {}
    }
}

pub(crate) fn stop(id: ContentId) {
    if let Some(view) = view_of(id) {
        // SAFETY: as above.
        unsafe { view.stopLoading() };
    }
}

/// Give page `id` the keyboard: its view within its window, and its window
/// among Magnetite's if the keyboard is with the page's own Magnetite
/// window already (the sidebar, another page), as showing a Chromium page
/// does. The keyboard is not taken from another app, nor from a window of
/// Magnetite's that is not part of this one (the quick search).
pub(crate) fn focus(id: ContentId) {
    let Some(view) = view_of(id) else { return };
    let (Some(window), Some(mtm)) = (view.window(), MainThreadMarker::new()) else {
        return;
    };
    window.makeFirstResponder(Some(&view));
    let key = objc2_app_kit::NSApplication::sharedApplication(mtm).keyWindow();
    if key.is_some_and(|key| super::root_window(&key) == super::root_window(&window)) {
        window.makeKeyWindow();
    }
}

/// Page `id`'s zoom in percent.
pub(crate) fn zoom(id: ContentId) -> Option<u32> {
    let view = view_of(id)?;
    // SAFETY: as above.
    let factor = unsafe { view.pageZoom() };
    // Clamped to what a page can be zoomed to, so the cast cannot wrap.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let percent = (factor * 100.0).round().clamp(1.0, 1000.0) as u32;
    Some(percent)
}

pub(crate) fn set_zoom(id: ContentId, percent: u32) {
    if let Some(view) = view_of(id) {
        // SAFETY: as above.
        unsafe { view.setPageZoom(f64::from(percent) / 100.0) };
    }
}

/// Whether everything page `id` shows came over an encrypted connection;
/// `None` for a page that has no network address.
pub(crate) fn secure(id: ContentId) -> Option<bool> {
    let view = view_of(id)?;
    let url = address(&view);
    if url.starts_with("https://") {
        // SAFETY: as above.
        Some(unsafe { view.hasOnlySecureContent() })
    } else if url.starts_with("http://") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{failure_note, is_web_address, read_store_ids, stores_file, windows_key};

    #[test]
    fn only_http_addresses_are_the_webs() {
        assert!(is_web_address("https://www.netflix.com/browse"));
        assert!(is_web_address("HTTP://example.org/"));
        for other in [
            "about:blank",
            "blob:https://example.org/1f0c",
            "data:text/html,<p>hi",
            "mailto:someone@example.org",
            "httpsx://example.org/",
            "example.org",
        ] {
            assert!(!is_web_address(other), "{other}");
        }
    }

    #[test]
    fn a_list_of_stores_that_cannot_be_read_is_not_an_empty_one() {
        let dir = std::env::temp_dir().join(format!("magnetite-stores-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // No file yet: nothing was ever kept.
        assert!(read_store_ids(&dir).unwrap().is_empty());
        std::fs::write(
            stores_file(&dir),
            r#"{"": "9056B7D5-9C68-4B1E-B4C3-5AF82173B8A3"}"#,
        )
        .unwrap();
        assert_eq!(read_store_ids(&dir).unwrap().len(), 1);
        std::fs::write(stores_file(&dir), "{ not what was written").unwrap();
        assert!(read_store_ids(&dir).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn keys_follow_the_layout_and_fall_back_to_their_place() {
        // A letter by what it types, whatever key that is on this layout.
        assert_eq!(windows_key(Some('t'), 0x11), Some(i32::from(b'T')));
        assert_eq!(windows_key(Some('ö'), 0x29), None);
        // Shift+1 types "!": the digit comes from the key's place.
        assert_eq!(windows_key(Some('!'), 0x12), Some(i32::from(b'1')));
        assert_eq!(windows_key(Some('['), 0x21), Some(0xDB));
        // Escape and the arrows type control characters.
        assert_eq!(windows_key(Some('\u{1b}'), 0x35), Some(0x1B));
        assert_eq!(windows_key(None, 0x7B), Some(0x25));
        assert_eq!(windows_key(None, 0x39), None);
    }

    #[test]
    fn the_failure_note_shows_text_as_text() {
        let note = failure_note("https://a.example/?q=<script>", "no <b>route</b> & more");
        assert!(note.contains("https://a.example/?q=&lt;script&gt;"));
        assert!(note.contains("no &lt;b&gt;route&lt;/b&gt; &amp; more"));
        assert!(!note.contains("<script>"));
    }
}
