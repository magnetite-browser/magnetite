//! Which app opens web links from other apps, and asking macOS to make it
//! Magnetite (`NSWorkspace`; macOS asks the user to confirm).

use std::sync::Mutex;

use block2::RcBlock;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSBundle, NSError, NSString, NSURL};

/// Whether Magnetite opens `scheme` links; `None` when it does not run as
/// an app bundle (a bare binary cannot be anyone's default). macOS keeps
/// the default by bundle identifier and names any copy of the app it knows
/// (a second download, the one Homebrew unpacked), so the identifier is
/// compared, not the path.
pub fn handles(scheme: &str) -> Option<bool> {
    let own = NSBundle::mainBundle().bundleIdentifier()?;
    let probe = NSURL::URLWithString(&NSString::from_str(&format!("{scheme}://example.org/")))?;
    let handler = NSWorkspace::sharedWorkspace().URLForApplicationToOpenURL(&probe);
    let id = handler.and_then(|h| NSBundle::bundleWithURL(&h)?.bundleIdentifier());
    Some(id.is_some_and(|id| id.isEqualToString(&own)))
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
