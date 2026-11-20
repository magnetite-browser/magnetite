//! Pages in the system's own web engine instead of Chromium.
//!
//! Some services play protected video only in a browser build that Google's
//! Widevine programme has verified, which Magnetite's Chromium is not. The
//! system's engine has protected playback of its own, so the sites the app
//! names (`Engine::set_system_engine_sites`) get their page from it: `WebKit`
//! on macOS (`mac/webkit.rs`), WebView2 on Windows (`win/webview2.rs`).
//! Linux has no such engine.
//!
//! A system page is a page like any other to the rest of the engine and to
//! the app: the same id, the same window in the frame (so it is shown,
//! split, peeked and closed the same way) and the same events. What differs
//! is what fills the window, and what a page of that engine cannot do: it
//! runs no Chrome extensions and none of Magnetite's hooks into Chromium
//! (ad blocking, the media row, `DevTools`, capture, find in page), and it
//! signs in apart from the Chromium pages of its profile.
//!
//! Which engine a page has is decided by the address it is given: when it
//! is made, when the app sends it elsewhere, and when a Chromium page is
//! about to go to one of the named sites. A page changes engine in place:
//! the old window goes, a new one comes under the same id. A system page
//! keeps its engine wherever its own links lead, so a sign-in that passes
//! through another site stays in one cookie store.

use cef::ImplWindow;

use crate::content::{ContentState, ContentWindowDelegate, PageView};
use crate::engine::{ContextProfile, Engine};
use crate::events::{ContentId, EngineEvent};
use crate::site::SiteState;

#[cfg(target_os = "macos")]
pub(crate) use crate::mac::webkit as backend;
#[cfg(target_os = "windows")]
pub(crate) use crate::win::webview2 as backend;

/// No system engine is hosted here: `available` says so, and nothing else
/// is ever asked.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) mod backend {
    use crate::events::ContentId;
    use std::path::Path;

    pub(crate) fn available() -> bool {
        false
    }
    pub(crate) fn create(_: ContentId, _: &str, _: &Path, _: &str, _: bool) -> bool {
        false
    }
    pub(crate) fn attach(_: ContentId, _: cef::sys::cef_window_handle_t) {}
    pub(crate) fn close(_: ContentId) {}
    pub(crate) fn load(_: ContentId, _: &str) {}
    pub(crate) fn go_back(_: ContentId) {}
    pub(crate) fn go_forward(_: ContentId) {}
    pub(crate) fn reload(_: ContentId) {}
    pub(crate) fn stop(_: ContentId) {}
    pub(crate) fn focus(_: ContentId) {}
    pub(crate) fn zoom(_: ContentId) -> Option<u32> {
        None
    }
    pub(crate) fn set_zoom(_: ContentId, _: u32) {}
    pub(crate) fn secure(_: ContentId) -> Option<bool> {
        None
    }
    pub(crate) fn address_of(_: ContentId) -> Option<String> {
        None
    }
    // The signature of the backends, which keep the path.
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn fetch_icon(_: &str, _: std::path::PathBuf) {}
    pub(crate) fn forget_stores(_: &Path, _: &[String]) {}
    pub(crate) fn release_stores(_: &[String]) {}
    pub(crate) fn clear_data(
        _: &Path,
        _: &str,
        _: bool,
        _: Option<&str>,
        done: Box<dyn FnOnce(Result<Option<usize>, String>)>,
    ) {
        done(Ok(None));
    }
}

/// Site state of system page `id`. The engine has no zoom per site: a page
/// starts at 100 percent.
pub(crate) fn site_state(id: ContentId) -> Option<SiteState> {
    Some(SiteState {
        secure: backend::secure(id),
        cert_error: false,
        zoom: backend::zoom(id)?,
        base_zoom: 100,
        system_engine: true,
    })
}

/// Whether `host` is one of `sites` or a subdomain of one.
pub(crate) fn listed(sites: &[String], host: &str) -> bool {
    sites
        .iter()
        .any(|site| crate::sitedata::in_domain(host, site))
}

impl Engine {
    /// Whether this system has an engine of its own that Magnetite can show
    /// pages in.
    pub fn system_engine_available() -> bool {
        backend::available()
    }

    /// The system's engine as the user reads of it: its name, and what it
    /// is. Empty where there is none.
    pub fn system_engine_name() -> (&'static str, &'static str) {
        if cfg!(target_os = "macos") {
            ("WebKit", "the Mac's own web engine")
        } else if cfg!(target_os = "windows") {
            (
                "WebView2",
                "Microsoft Edge's engine, which comes with Windows",
            )
        } else {
            ("", "")
        }
    }

    /// The sites whose pages the system's engine shows from now on: hosts,
    /// each standing for its subdomains too. None: every page is
    /// Chromium's. Pages that are open keep their engine until they load
    /// another address.
    pub fn set_system_engine_sites(&self, sites: Vec<String>) {
        self.lock().system_sites = sites;
    }

    /// Whether a page at `url` belongs in the system's engine.
    pub(crate) fn wants_system(&self, url: &str) -> bool {
        backend::available()
            && crate::client::host_of(url)
                .is_some_and(|host| listed(&self.lock().system_sites, &host))
    }

    /// Whether page `id` is shown by the system's engine.
    pub fn is_system_page(&self, id: ContentId) -> bool {
        (self.lock().contents.get(&id)).is_some_and(|c| matches!(c.view, PageView::System(_)))
    }

    /// Make page `id` in the system's engine, in a window of the kind every
    /// page has (hidden), loading `url` in `profile`'s own data store.
    pub(crate) fn build_system_content(
        &self,
        id: ContentId,
        url: &str,
        profile: Option<&ContextProfile>,
    ) {
        let profile_id = profile.map(|p| p.id.clone()).unwrap_or_default();
        let ephemeral = profile.is_some_and(|p| p.ephemeral);
        if !backend::create(id, url, &self.config().data_dir, &profile_id, ephemeral) {
            crate::trace(&format!("create_content #{id}: no system page"));
            self.emit(EngineEvent::ContentClosed { id });
            return;
        }
        let serial = self.next_serial();
        self.lock().contents.insert(
            id,
            ContentState {
                home: self.current_window(),
                window: None,
                view: PageView::System(id),
                browser: None,
                profile: profile_id,
                ephemeral,
                devtools: None,
                slot: None,
                tab_target: None,
                mini: None,
                adopted: false,
                serial,
            },
        );
        let mut delegate = ContentWindowDelegate::new(id, PageView::System(id), None, serial);
        cef::window_create_top_level(Some(&mut delegate));
        self.emit(EngineEvent::ContentCreated { id });
    }

    /// Page `id` gets the other engine, the one `url` belongs in, and loads
    /// `url` there: where it was on screen, with the lock, the mute and the
    /// profile it had. What the old page played, shared or had asked is
    /// over, as after any load.
    pub(crate) fn change_engine(&self, id: ContentId, url: &str) {
        self.close_docked(id);
        let (old, shares) = {
            let mut inner = self.lock();
            let shares = inner.shares.forget(id);
            inner.permission_requests.remove(&id);
            (inner.contents.remove(&id), shares)
        };
        let Some(old) = old else { return };
        crate::trace(&format!("page #{id} changes its engine"));
        self.emit_shares(shares);
        crate::devtools::page_closed(id);
        self.emit(EngineEvent::MediaChanged { id, media: None });
        self.emit(EngineEvent::MediaAccessChanged {
            id,
            camera: false,
            microphone: false,
        });
        if matches!(old.view, PageView::System(_)) {
            backend::close(id);
        }
        // The old window closes on its own time; its delegate finds another
        // page under its id by then and leaves it alone (`serial`).
        if let Some(window) = &old.window {
            window.hide();
            window.close();
        }
        let profile = (!old.profile.is_empty() || old.ephemeral).then(|| ContextProfile {
            id: old.profile.clone(),
            ephemeral: old.ephemeral,
        });
        // Not before the old page is out of the table: the new one takes
        // its place there.
        let build = || {
            self.make_page(id, url, profile.as_ref(), false);
            self.show_late(id);
        };
        if self.in_window(old.home, build).is_none() {
            build();
        }
    }

    /// Zoom system page `id` to `percent`.
    pub(crate) fn system_set_zoom(&self, id: ContentId, percent: u32) {
        backend::set_zoom(id, percent);
        self.emit_site_state(id);
    }

    /// The profile of page `id`, as the app names one; none for the
    /// default profile.
    pub(crate) fn content_profile(&self, id: ContentId) -> Option<ContextProfile> {
        let inner = self.lock();
        let c = inner.contents.get(&id)?;
        (!c.profile.is_empty() || c.ephemeral).then(|| ContextProfile {
            id: c.profile.clone(),
            ephemeral: c.ephemeral,
        })
    }

    /// The profiles `ids` are gone for good: so is what the system's engine
    /// kept for them.
    pub(crate) fn forget_system_stores(&self, ids: &[String]) {
        backend::forget_stores(&self.config().data_dir, ids);
    }

    /// Delete what the system's engine keeps for `profile` (none: the
    /// default profile): everything (`domain` none), or what belongs to the
    /// site `domain`. `done` hears, on the UI thread, how many cookies went
    /// (where that is known: for one site), or why nothing was deleted.
    pub fn clear_system_data(
        &self,
        profile: Option<&ContextProfile>,
        domain: Option<&str>,
        done: Box<dyn FnOnce(Result<Option<usize>, String>)>,
    ) {
        let (id, ephemeral) = profile.map_or(("", false), |p| (p.id.as_str(), p.ephemeral));
        backend::clear_data(&self.config().data_dir, id, ephemeral, domain, done);
    }
}

/// What a system page reports, each as the event a Chromium page's handlers
/// (`client.rs`) send for the same thing. Compiled where a backend exists
/// to report.
#[cfg(any(target_os = "macos", target_os = "windows"))]
impl Engine {
    /// The view of page `id` got the keyboard.
    pub(crate) fn system_focused(&self, id: ContentId) {
        if !self.is_system_page(id) {
            return;
        }
        self.blur_overlay();
        let home = self.lock().home_of(id);
        self.enter(home);
        self.update_focus_ring(Some(id));
        self.emit(EngineEvent::ContentFocused { id });
    }

    /// A key pressed in page `id`, before the page sees it. True if it is
    /// one of Magnetite's reserved shortcuts (or a peek's key, in the peek):
    /// the app runs it and the page does not get the key. A site that gets
    /// the shortcuts' keys first (`keys::page_first_site`) does not here:
    /// nothing would run a shortcut the page left alone.
    pub(crate) fn system_key(&self, id: ContentId, event: &cef::KeyEvent) -> bool {
        use crate::keys;
        let shortcut = keys::shortcut_for_key(event, false)
            .or_else(|| keys::peek_shortcut(event).filter(|_| self.peek_content() == Some(id)));
        let Some(shortcut) = shortcut else {
            return false;
        };
        self.emit(EngineEvent::Shortcut {
            from: Some(id),
            shortcut,
        });
        true
    }

    /// Page `id` began to load `url`: a new page in the tab.
    pub(crate) fn system_started(&self, id: ContentId, url: &str) {
        self.note_origin(id, url);
        self.emit_site_state(id);
        self.emit(EngineEvent::MediaChanged { id, media: None });
    }

    pub(crate) fn system_url(&self, id: ContentId, url: String) {
        self.emit(EngineEvent::UrlChanged { id, url });
    }

    pub(crate) fn system_title(&self, id: ContentId, title: String) {
        self.emit(EngineEvent::TitleChanged { id, title });
    }

    pub(crate) fn system_loading(&self, id: ContentId, loading: bool, back: bool, forward: bool) {
        self.emit(EngineEvent::LoadingChanged {
            id,
            loading,
            can_go_back: back,
            can_go_forward: forward,
        });
        if !loading {
            self.emit_site_state(id);
        }
    }

    /// Page `id` shows an element of its own full screen (a video), or no
    /// more. WebView2 only says so: its page's window is made full screen,
    /// and the view fills it. `WebKit` makes a window of its own.
    #[cfg(target_os = "windows")]
    pub(crate) fn system_fullscreen(&self, id: ContentId, on: bool) {
        let window = (self.lock().contents.get(&id)).and_then(|c| c.window.clone());
        if let Some(window) = window {
            window.set_fullscreen(i32::from(on));
        }
    }

    pub(crate) fn system_failed(&self, id: ContentId, url: String, refused: bool) {
        self.emit(EngineEvent::LoadFailed { id, url, refused });
    }

    /// Page `id` names `icon_url` as its icon.
    pub(crate) fn system_favicon(&self, id: ContentId, icon_url: &str) {
        self.cache_favicon(id, icon_url);
        // As for a Chromium page: the sidebar loads an icon in the default
        // profile, so a page of another profile shows the kept copy.
        let url = (self.content_context(id))
            .is_some_and(|c| c.is_empty())
            .then(|| icon_url.to_string());
        self.emit(EngineEvent::FaviconChanged { id, url });
    }

    /// Page `id` asks for `url` in a new tab (`background`: behind the
    /// page) or in a window of its own.
    pub(crate) fn system_open(
        &self,
        id: ContentId,
        url: String,
        background: bool,
        new_window: bool,
    ) {
        self.emit(EngineEvent::OpenUrlRequested {
            from: Some(id),
            url,
            background,
            new_window,
            own_window: false,
            locked: self.content_lock(id).is_some(),
            source: crate::events::UrlSource::Page,
            context: self.content_context(id),
        });
    }

    /// A click in page `id` leads to `url`. True if the page is locked to
    /// its site (a pinned tab, a favorite) and `url` is another one: the
    /// app opens it elsewhere and the page stays.
    pub(crate) fn system_leaves_lock(&self, id: ContentId, url: &str) -> bool {
        let leaves = (self.content_lock(id))
            .zip(crate::client::host_of(url))
            .is_some_and(|(lock, host)| !crate::client::same_site(&host, &lock));
        if leaves {
            self.emit(EngineEvent::OpenUrlRequested {
                from: Some(id),
                url: url.to_string(),
                background: false,
                new_window: false,
                own_window: false,
                locked: true,
                source: crate::events::UrlSource::Page,
                context: self.content_context(id),
            });
        }
        leaves
    }
}

#[cfg(test)]
mod tests {
    use super::listed;

    #[test]
    fn a_site_stands_for_its_subdomains_only() {
        let sites = ["netflix.com".to_string(), "tv.example.org".to_string()];
        assert!(listed(&sites, "netflix.com"));
        assert!(listed(&sites, "www.netflix.com"));
        assert!(listed(&sites, "help.netflix.com"));
        assert!(!listed(&sites, "notnetflix.com"));
        assert!(!listed(&sites, "netflix.com.example.org"));
        assert!(!listed(&sites, "example.org"));
        assert!(!listed(&[], "netflix.com"));
    }
}
