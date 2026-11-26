//! The Chromium tab behind a system page, for the profile's extensions.
//!
//! An extension's action runs for a Chromium tab, and its popup hangs on
//! that tab's toolbar (`extensions.rs`). A system page (`system.rs`) has
//! neither, so its window holds a Chromium view of its own under the
//! system's view: the shadow. It is never seen and loads nothing from the
//! network. Every request of it is answered with an empty document, at the
//! address the system page shows, so an extension asked for the tab in
//! front finds the right site (a password manager its logins for it) and
//! its popup opens where it does on any other page.
//!
//! What the extension does to the tab reaches the empty document, not the
//! page: nothing is filled in or changed there.

use cef::*;

use crate::content::ContentViewDelegate;
use crate::engine::Engine;
use crate::events::ContentId;

/// Whether system pages get a shadow here: where extensions open from
/// Magnetite's own UI (`devpipe.rs`) and a system engine exists.
pub(crate) const HOSTED: bool = cfg!(target_os = "macos");

/// A system page's shadow: its view, and its browser once Chromium made it.
pub(crate) struct Shadow {
    pub view: BrowserView,
    pub browser: Option<Browser>,
    /// The extension asked for before the browser was there
    /// (`Engine::open_extension`): opened as soon as it is.
    pub asked: Option<String>,
}

impl Shadow {
    pub(crate) fn is(&self, view: &mut View) -> bool {
        View::from(&self.view).is_same(Some(view)) != 0
    }
}

/// What a shadow shows at every address.
const EMPTY_DOCUMENT: &[u8] = b"<!doctype html><title></title>";

fn is_web(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// A view for the shadow of a system page at `url`, in `context`. None if
/// Chromium makes no view (a context that is not ready).
pub(crate) fn view(url: &str, mut context: Option<RequestContext>) -> Option<BrowserView> {
    let url = if is_web(url) { url } else { "about:blank" };
    browser_view_create(
        Some(&mut ShadowClient::new()),
        Some(&CefString::from(url)),
        Some(&BrowserSettings::default()),
        None,
        context.as_mut(),
        Some(&mut ContentViewDelegate::new(false)),
    )
}

impl Engine {
    /// The system page whose shadow `browser` is.
    pub(crate) fn shadow_owner(&self, browser: Option<&Browser>) -> Option<ContentId> {
        let identifier = browser?.identifier();
        (self.lock().contents.iter())
            .find(|(_, c)| {
                (c.shadow.as_ref())
                    .and_then(|s| s.browser.as_ref())
                    .is_some_and(|b| b.identifier() == identifier)
            })
            .map(|(id, _)| *id)
    }

    /// The browser that stands for page `id` among Chromium's tabs: its
    /// own, or a system page's shadow.
    pub(crate) fn tab_host(&self, id: ContentId) -> Option<BrowserHost> {
        let inner = self.lock();
        let c = inner.contents.get(&id)?;
        let shadow = || c.shadow.as_ref()?.browser.clone();
        c.browser.clone().or_else(shadow)?.host()
    }

    /// Bring the shadow of system page `id` to the address the page shows.
    pub(crate) fn sync_shadow(&self, id: ContentId) {
        let Some(url) = crate::system::backend::address_of(id).filter(|u| is_web(u)) else {
            return;
        };
        let browser =
            (self.lock().contents.get(&id)).and_then(|c| c.shadow.as_ref()?.browser.clone());
        let Some(frame) = browser.and_then(|b| b.main_frame()) else {
            return;
        };
        if CefString::from(&frame.url()).to_string() != url {
            frame.load_url(Some(&CefString::from(url.as_str())));
        }
    }
}

wrap_client! {
    struct ShadowClient {}
    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(ShadowLifeSpan::new())
        }
        fn request_handler(&self) -> Option<RequestHandler> {
            Some(ShadowRequests::new())
        }
        fn focus_handler(&self) -> Option<FocusHandler> {
            Some(ShadowFocus::new())
        }
    }
}

// Chromium gives the shadow the keyboard when its window becomes the key
// window: with an extension's popup, and again when the popup closes. The
// keys belong to the page, so the system's view takes them back.
wrap_focus_handler! {
    struct ShadowFocus {}
    impl FocusHandler {
        fn on_got_focus(&self, browser: Option<&mut Browser>) {
            if let Some(id) = Engine::global().shadow_owner(browser.as_deref()) {
                Engine::post(Box::new(move || crate::system::backend::focus(id)));
            }
        }
    }
}

wrap_life_span_handler! {
    struct ShadowLifeSpan {}
    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let Some(browser) = browser else { return };
            let Some(mut view) = browser_view_get_for_browser(Some(browser)).map(|v| View::from(&v)) else {
                return;
            };
            let engine = Engine::global();
            let found = (engine.lock().contents.iter_mut()).find_map(|(id, c)| {
                let shadow = c.shadow.as_mut().filter(|s| s.is(&mut view))?;
                shadow.browser = Some(browser.clone());
                Some((*id, shadow.asked.take()))
            });
            let Some((id, asked)) = found else { return };
            // The page may have moved on while Chromium made the browser.
            engine.sync_shadow(id);
            if let Some(extension) = asked {
                engine.open_extension(id, &extension);
            }
        }

        fn on_before_popup(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let engine = Engine::global();
            let url = target_url.map(CefString::to_string).unwrap_or_default();
            let background = target_disposition == WindowOpenDisposition::NEW_BACKGROUND_TAB;
            // A tab an extension opens from its popup: a page of ours, as
            // from any other page (`MagnetiteLifeSpanHandler`).
            if popup_id == crate::content::EXTENSION_TAB_POPUP {
                let wishes = crate::content::extension_tab_wishes(extra_info);
                return i32::from(!engine.expect_extension_tab(browser.as_deref(), url, background, wishes));
            }
            // Anything else asked for from the empty document opens as the
            // system page's own link would.
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            if let Some(id) = engine.shadow_owner(browser.as_deref()).filter(|_| is_web(&url)) {
                engine.system_open(id, url, background, false);
            }
            1
        }
    }
}

wrap_request_handler! {
    struct ShadowRequests {}
    impl RequestHandler {
        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _is_navigation: ::std::os::raw::c_int,
            _is_download: ::std::os::raw::c_int,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<ResourceRequestHandler> {
            Some(EmptyDocuments::new())
        }
    }
}

// Answers every request with the empty document: a shadow never reaches
// the network.
wrap_resource_request_handler! {
    struct EmptyDocuments {}
    impl ResourceRequestHandler {
        // The default answer would cancel the request.
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            ReturnValue::CONTINUE
        }

        fn resource_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            Some(crate::scheme::bytes_handler(EMPTY_DOCUMENT.to_vec(), "text/html"))
        }
    }
}
