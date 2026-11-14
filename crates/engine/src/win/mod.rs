//! Windows-only window looks: rounded corners through the Desktop Window
//! Manager (DWM) for floating windows, and a window region for pages.

use std::ffi::c_void;

mod default_browser;
mod dividers;
mod edge;
mod hotkey;
mod layers;
mod running;
mod share;
mod video;

pub use default_browser::{handles, open_default_apps};
pub use dividers::{DividerKey, set_dividers};
pub use edge::set_edge_zone;
pub use hotkey::set_quick_search_hotkey;
pub use layers::{hide_peek_backdrop, set_focus_ring, show_peek_backdrop};
pub use running::mark_running;
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
const DWMWCP_DONOTROUND: u32 = 1;
/// Windows' own radius (8 px); the only other choice is 4 px.
const DWMWCP_ROUND: u32 = 2;
/// Corner radius of a page, in points at 100 % scaling (as on macOS).
const PAGE_RADIUS: i32 = 10;
const DWMWA_COLOR_NONE: u32 = 0xFFFF_FFFE;
/// The line around a floating window, as `0x00BBGGRR`: a grey just lighter
/// than the window, so it shows on a dark page without looking drawn on.
const FLOATING_BORDER: u32 = 0x0036_3636;

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
pub fn round_window_corners(handle: cef::sys::cef_window_handle_t, shadow: bool) {
    if shadow {
        set(handle, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND);
        set(handle, DWMWA_BORDER_COLOR, FLOATING_BORDER);
    } else {
        set(handle, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND);
        set(handle, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE);
        clip_page(handle);
    }
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
