//! Keeps a floating window (the web panel, a peek) above the pages on Linux,
//! where the two display systems need opposite things:
//!
//! - X11: page windows are managed by the window manager, which raises one
//!   that is shown or clicked. A window that is transient for the page in
//!   front stays above it, whatever happens to that page.
//! - Wayland: page windows are subsurfaces of the frame, in the order they
//!   were shown, and Chromium has no call to raise one. Showing a window
//!   again makes a new subsurface, which goes on top.

use std::ffi::{c_char, c_int, c_ulong, c_void};
use std::sync::OnceLock;

use cef::*;

/// The three Xlib calls needed. Loaded at run time: under Wayland there may
/// be no libX11 to load, and the build needs no X11 headers.
struct Xlib {
    open: OpenDisplay,
    set_transient: SetTransient,
    close: CloseDisplay,
}

type OpenDisplay = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type SetTransient = unsafe extern "C" fn(*mut c_void, c_ulong, c_ulong) -> c_int;
type CloseDisplay = unsafe extern "C" fn(*mut c_void) -> c_int;

fn xlib() -> Option<&'static Xlib> {
    static XLIB: OnceLock<Option<Xlib>> = OnceLock::new();
    XLIB.get_or_init(|| {
        // SAFETY: the names are C strings, and each symbol has the signature
        // Xlib documents for it.
        unsafe {
            let lib = libc::dlopen(c"libX11.so.6".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if lib.is_null() {
                return None;
            }
            let symbol = |name: &std::ffi::CStr| {
                let found = libc::dlsym(lib, name.as_ptr());
                (!found.is_null()).then_some(found)
            };
            Some(Xlib {
                open: std::mem::transmute::<*mut c_void, OpenDisplay>(symbol(c"XOpenDisplay")?),
                set_transient: std::mem::transmute::<*mut c_void, SetTransient>(symbol(
                    c"XSetTransientForHint",
                )?),
                close: std::mem::transmute::<*mut c_void, CloseDisplay>(symbol(c"XCloseDisplay")?),
            })
        }
    })
    .as_ref()
}

/// Make X11 window `window` transient for `parent`. Its own short connection:
/// this runs on a tab switch, and closing it sends the request.
fn set_transient(window: &Window, parent: &Window) {
    let Some(x) = xlib() else { return };
    // SAFETY: the display is checked and closed here, and an X error for a
    // window that just went is handled by Chromium's error handler.
    unsafe {
        let display = (x.open)(std::ptr::null());
        if display.is_null() {
            return;
        }
        (x.set_transient)(display, window.window_handle(), parent.window_handle());
        (x.close)(display);
    }
}

/// Put `window` above the pages, the last of which, `top`, was shown last.
/// `reshown`: a page was newly shown, so that under Wayland it lies above
/// `window` now.
pub(crate) fn keep_above(window: &Window, top: Option<&Window>, reshown: bool) {
    if crate::cef_app::runs_on_x11() {
        if let Some(top) = top {
            set_transient(window, top);
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
        set_transient(window, top);
        top.bring_to_top();
    }
    window.bring_to_top();
}

/// `window` is a plain page again: transient for the frame, as CEF made it.
pub(crate) fn release(window: &Window, frame: &Window) {
    if crate::cef_app::runs_on_x11() {
        set_transient(window, frame);
    }
}
