//! Windows-only window looks: rounded corners through the Desktop Window
//! Manager (DWM) for floating windows, and a window region for pages.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

mod default_browser;
mod dividers;
mod edge;
mod hotkey;
mod layers;
mod running;
mod share;
mod video;
pub(crate) mod webview2;

pub use default_browser::{handles, open_default_apps};
pub use dividers::{DividerKey, set_dividers};
pub use edge::set_edge_zone;
pub use hotkey::set_quick_search_hotkey;
pub use layers::{hide_peek_backdrop, set_focus_ring, show_peek_backdrop};
pub use running::{installer_ready, mark_running};
pub use share::{copy_png, copy_text, mouse_point, save_png, shift_held};
pub use video::brand_video_window;

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: *mut c_void,
        attribute: u32,
        value: *const c_void,
        size: u32,
    ) -> i32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn SetWindowLongPtrW(hwnd: *mut c_void, index: i32, value: isize) -> isize;
    fn GetWindowRect(hwnd: *mut c_void, rect: *mut [i32; 4]) -> i32;
    fn GetDpiForWindow(hwnd: *mut c_void) -> u32;
    fn SetWindowRgn(hwnd: *mut c_void, region: *mut c_void, redraw: i32) -> i32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateRoundRectRgn(x1: i32, y1: i32, x2: i32, y2: i32, w: i32, h: i32) -> *mut c_void;
}

const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWA_BORDER_COLOR: u32 = 34;
/// Windows 11 only, as `0x00BBGGRR`.
const DWMWA_CAPTION_COLOR: u32 = 35;
const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
/// No backdrop: DWM paints nothing of its own behind the window's content.
const DWMSBT_NONE: u32 = 1;
const DWMWCP_DONOTROUND: u32 = 1;
/// Windows' own radius (8 px); the only other choice is 4 px.
const DWMWCP_ROUND: u32 = 2;
/// Corner radius of a page, in points at 100 % scaling (as on macOS).
const PAGE_RADIUS: i32 = 10;
const DWMWA_COLOR_NONE: u32 = 0xFFFF_FFFE;
/// The line around a floating window, as `0x00BBGGRR`: a grey just lighter
/// than the window, so it shows on a dark page without looking drawn on.
const FLOATING_BORDER: u32 = 0x0036_3636;

/// The colour of the frame around the pages, as `0x00BBGGRR`: what a new
/// page window takes for its own frame (`match_frame`).
static FRAME_COLOR: AtomicU32 = AtomicU32::new(colorref(0xFF1B_1B1E));

/// `0xAARRGGBB` as the `0x00BBGGRR` Windows wants.
const fn colorref(argb: u32) -> u32 {
    ((argb & 0xFF) << 16) | (argb & 0xFF00) | ((argb >> 16) & 0xFF)
}

/// The frame around the pages changed its colour to `argb`: the page
/// windows `pages` and those made from now on follow.
pub fn set_frame_color(argb: u32, pages: impl IntoIterator<Item = cef::sys::cef_window_handle_t>) {
    FRAME_COLOR.store(colorref(argb), Ordering::Relaxed);
    for page in pages {
        match_frame(page);
    }
}

/// Give a page window's own frame the colour of the frame around the pages.
///
/// On Windows 11 a page showed white in its cut-off corners and as a line
/// along its top edge (issue 239). Taken to be the frame the window still
/// has to Windows without a title bar: DWM draws it behind the page, white
/// unless told a colour, and the page's region does not cut it. In the
/// colour around the page it does not show.
fn match_frame(handle: cef::sys::cef_window_handle_t) {
    set(
        handle,
        DWMWA_CAPTION_COLOR,
        FRAME_COLOR.load(Ordering::Relaxed),
    );
}

fn set(handle: cef::sys::cef_window_handle_t, attribute: u32, value: u32) {
    if handle.0.is_null() {
        return;
    }
    // SAFETY: CEF hands us a live HWND; DWM copies the four bytes of `value`
    // during the call. An attribute Windows 10 does not know only fails.
    unsafe {
        DwmSetWindowAttribute(
            handle.0.cast(),
            attribute,
            (&raw const value).cast(),
            u32::BITS / 8,
        );
    }
}

/// Give a frameless window rounded corners, so it floats inside the frame
/// (the counterpart of `mac::round_window_corners`).
///
/// A floating window (`shadow`: palette, mini window, sidebar over the page)
/// is rounded by DWM: Windows 11 only and at its own radius, with a line
/// around it; DWM adds a shadow to such windows itself. A page gets no
/// shadow: it would darken the chrome around the page, more so while the
/// page is the active window. So a page is cut round with a window region
/// instead (`clip_page`), which DWM neither rounds nor shades.
///
/// The region cuts what Chromium draws, but not the backdrop Windows 11
/// fills such a window with first (light or dark grey, as the system's
/// appearance): in the corners that backdrop would show instead of the
/// frame, so a page has none.
pub fn round_window_corners(handle: cef::sys::cef_window_handle_t, shadow: bool) {
    if shadow {
        set(handle, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND);
        set(handle, DWMWA_BORDER_COLOR, FLOATING_BORDER);
    } else {
        set(handle, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND);
        set(handle, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE);
        match_frame(handle);
        set(handle, DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_NONE);
        clip_page(handle);
    }
}

/// The index of a window's owner for `SetWindowLongPtrW`.
const GWLP_HWNDPARENT: i32 = -8;

/// Make the page window `child` a window of the frame `frame`, as the
/// pages made for tabs are from the start (the counterpart of
/// `mac::attach_child`). A mini window is nobody's; turned into a tab it
/// must go with the frame: minimised with it, above it, and without a
/// button of its own in the taskbar.
pub fn attach_child(frame: cef::sys::cef_window_handle_t, child: cef::sys::cef_window_handle_t) {
    if frame.0.is_null() || child.0.is_null() {
        return;
    }
    // SAFETY: two live windows of this process; Windows only notes the owner.
    unsafe { SetWindowLongPtrW(child.0.cast(), GWLP_HWNDPARENT, frame.0 as isize) };
}

/// A frameless window that stays a plain rectangle (the device bar above a
/// page): Windows 11 would round it and draw a line around it.
pub fn square_window(handle: cef::sys::cef_window_handle_t) {
    set(handle, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND);
    set(handle, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE);
}

/// Cut a page's window to a rounded rectangle of its present size. The
/// region does not follow the window: call again after every resize.
pub fn clip_page(handle: cef::sys::cef_window_handle_t) {
    if handle.0.is_null() {
        return;
    }
    let hwnd = handle.0.cast::<c_void>();
    let mut rect = [0i32; 4];
    // SAFETY: CEF hands us a live HWND; `rect` is the four `i32` of a RECT.
    // The system owns the region once `SetWindowRgn` took it.
    unsafe {
        if GetWindowRect(hwnd, &raw mut rect) == 0 {
            return;
        }
        let (width, height) = (rect[2] - rect[0], rect[3] - rect[1]);
        let dpi = i32::try_from(GetDpiForWindow(hwnd)).unwrap_or(96).max(96);
        let diameter = 2 * PAGE_RADIUS * dpi / 96;
        // The region's right and bottom edges are exclusive, and a round
        // rectangle's are drawn one pixel inside them.
        let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, diameter, diameter);
        if !region.is_null() {
            SetWindowRgn(hwnd, region, 1);
        }
    }
    // The outline of a split's focused page is a window of its own.
    layers::follow_ring(hwnd);
}

#[cfg(test)]
mod tests {
    use super::colorref;

    #[test]
    fn a_colour_turns_into_blue_green_red_without_alpha() {
        assert_eq!(colorref(0xFF11_2233), 0x0033_2211);
    }
}
