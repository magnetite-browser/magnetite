//! Whether Magnetite is the system's default browser, and asking for it:
//! on macOS and Windows; elsewhere the answer is "unknown". And links that
//! other programs hand over.

use crate::engine::Engine;

/// Whether links from other apps open in Magnetite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultBrowser {
    Yes,
    No,
    /// Not known here (another system, or not running as an app bundle).
    Unknown,
}

impl From<Option<bool>> for DefaultBrowser {
    fn from(handles: Option<bool>) -> Self {
        match handles {
            Some(true) => Self::Yes,
            Some(false) => Self::No,
            None => Self::Unknown,
        }
    }
}

impl Engine {
    /// Whether `https` links from other apps open in Magnetite.
    pub fn default_browser(&self) -> DefaultBrowser {
        #[cfg(target_os = "macos")]
        return crate::mac::handles("https").into();
        #[cfg(target_os = "windows")]
        return crate::win::handles("https").into();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        DefaultBrowser::Unknown
    }

    /// Another program handed over a link (it clicked one while Magnetite
    /// is the default browser): the app opens it where its settings say.
    pub fn open_link_from_other_app(&self, url: String) {
        self.emit(crate::events::EngineEvent::OpenUrlRequested {
            from: None,
            url,
            background: false,
            new_window: false,
            own_window: false,
            locked: false,
            source: crate::events::UrlSource::OtherApp,
            context: None,
        });
    }

    /// Ask the system to make Magnetite the default browser, for `http`
    /// and `https`; macOS asks the user to confirm, Windows has the user
    /// choose in its Settings, which open. Only on the user's own click.
    /// `done` gets the outcome on the UI thread; on Windows that is how
    /// things are before the user chose.
    pub fn make_default_browser(&self, done: Box<dyn FnOnce(DefaultBrowser) + Send>) {
        #[cfg(target_os = "macos")]
        {
            use crate::mac::{handles, set_handler};
            // Not an app bundle: there is nothing macOS could register.
            if handles("https").is_none() {
                return done(DefaultBrowser::Unknown);
            }
            // On the UI thread once macOS has its answer for `http`. macOS
            // usually moves `https` along with it; if not, it is asked for
            // separately, but not after the user said no to `http`.
            let after_http = move || {
                let engine = Engine::global();
                if handles("http") == Some(true) && handles("https") == Some(false) {
                    let finish = || done(Engine::global().default_browser());
                    set_handler("https", Box::new(move || Engine::post(Box::new(finish))));
                } else {
                    done(engine.default_browser());
                }
            };
            set_handler("http", Box::new(move || Engine::post(Box::new(after_http))));
        }
        #[cfg(target_os = "windows")]
        {
            // Not installed: Windows has no entry of Magnetite to choose.
            if crate::win::handles("https").is_some() {
                crate::win::open_default_apps();
            }
            done(self.default_browser());
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        done(DefaultBrowser::Unknown);
    }
}
