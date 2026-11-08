//! Which app opens web links from other apps, and asking macOS to make it
//! Magnetite (`NSWorkspace`; macOS asks the user to confirm).

use std::sync::Mutex;

use block2::RcBlock;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSBundle, NSError, NSString, NSURL};

/// Whether this app opens `scheme` links; `None` when it does not run as
/// an app bundle (a bare binary cannot be anyone's default).
pub fn handles(scheme: &str) -> Option<bool> {
    let bundle = NSBundle::mainBundle();
    bundle.bundleIdentifier()?;
    let probe = NSURL::URLWithString(&NSString::from_str(&format!("{scheme}://example.org/")))?;
    let handler = NSWorkspace::sharedWorkspace().URLForApplicationToOpenURL(&probe);
    let path = |url: &NSURL| {
        url.path()
            .map(|p| p.to_string().trim_end_matches('/').to_string())
    };
    Some(handler.is_some_and(|h| path(&h).is_some() && path(&h) == path(&bundle.bundleURL())))
}

/// Ask macOS to open `scheme` links with this app. `done` runs once the
/// user answered macOS's question, on a thread of macOS's choosing.
pub fn set_handler(scheme: &str, done: Box<dyn FnOnce() + Send>) {
    let app = NSBundle::mainBundle().bundleURL();
    let done = Mutex::new(Some(done));
    let block = RcBlock::new(move |_error: *mut NSError| {
        let next = done.lock().ok().and_then(|mut d| d.take());
        if let Some(f) = next {
            f();
        }
    });
    NSWorkspace::sharedWorkspace()
        .setDefaultApplicationAtURL_toOpenURLsWithScheme_completionHandler(
            &app,
            &NSString::from_str(scheme),
            Some(&block),
        );
}
