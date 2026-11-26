//! Keeps a floating window (the web panel, a peek, the floating sidebar)
//! above the pages on Linux, where the two display systems need opposite
//! things:
//!
//! - X11: page windows are managed by the window manager, which raises one
//!   that is shown or clicked. A window that is transient for the page in
//!   front stays above it, whatever happens to that page.
//! - Wayland: page windows are subsurfaces of the frame, in the order they
//!   were shown, and Chromium has no call to raise one. Showing a window
//!   again makes a new subsurface, which goes on top.

use cef::*;

use crate::x11::set_transient;

/// Put `window` above the pages, the last of which, `top`, was shown last.
/// `reshown`: a page was newly shown, so that under Wayland it lies above
/// `window` now.
pub(crate) fn keep_above(window: &Window, top: Option<&Window>, reshown: bool) {
    if crate::cef_app::runs_on_x11() {
        if let Some(top) = top {
            set_transient(window.window_handle(), top.window_handle());
        }
        window.bring_to_top();
    } else if reshown {
        window.hide();
        window.show();
    }
}

/// Under X11, once the window manager has mapped page `top`: raise the page
/// and `window` with it. `KWin` does not move a window for a new transient
/// hint, and turns down its raise; a raise of the parent brings it along.
pub(crate) fn settle_above(window: &Window, top: Option<&Window>) {
    if let Some(top) = top {
        set_transient(window.window_handle(), top.window_handle());
        top.bring_to_top();
    }
    window.bring_to_top();
}

/// `window` is a plain page again: transient for the frame, as CEF made it.
pub(crate) fn release(window: &Window, frame: &Window) {
    if crate::cef_app::runs_on_x11() {
        set_transient(window.window_handle(), frame.window_handle());
    }
}
