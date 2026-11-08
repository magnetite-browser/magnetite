//! Whether Magnetite is the system's default browser, and asking for it.
//! macOS only so far; elsewhere the answer is "unknown".

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
        #[cfg(not(target_os = "macos"))]
        DefaultBrowser::Unknown
    }

    /// Ask the system to make Magnetite the default browser, for `http`
    /// and `https`; macOS asks the user to confirm. Only on the user's own
    /// click. `done` gets the outcome on the UI thread.
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
        #[cfg(not(target_os = "macos"))]
        done(DefaultBrowser::Unknown);
    }
}
