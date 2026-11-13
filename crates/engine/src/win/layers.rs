//! What macOS draws as layers on or behind a page, as see-through windows
//! of the frame: the outline of the focused page of a split
//! (`mac::set_focus_ring`) and the dimmed pages behind a peek
//! (`mac::show_peek_backdrop`).

use std::cell::{Cell, RefCell};
use std::ffi::c_void;

use cef::Rect;

use crate::engine::Engine;
use crate::events::{EngineEvent, Shortcut};

type Hwnd = *mut c_void;

type WndProc =
    unsafe extern "system" fn(hwnd: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize;

/// `WNDCLASSW`.
#[repr(C)]
struct WindowClass {
    style: u32,
    procedure: WndProc,
    class_extra: i32,
    window_extra: i32,
    instance: *mut c_void,
    icon: *mut c_void,
    cursor: *mut c_void,
    background: *mut c_void,
    menu_name: *const u16,
    class_name: *const u16,
}

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Extent {
    width: i32,
    height: i32,
}

#[repr(C)]
struct BlendFunction {
    op: u8,
    flags: u8,
    alpha: u8,
    format: u8,
}

#[repr(C)]
struct BitmapInfoHeader {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    x_per_meter: i32,
    y_per_meter: i32,
    colours_used: u32,
    colours_important: u32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassW(class: *const WindowClass) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class: *const u16,
        name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        owner: Hwnd,
        menu: *mut c_void,
        instance: *mut c_void,
        param: *mut c_void,
    ) -> Hwnd;
    fn DefWindowProcW(hwnd: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize;
    fn SetWindowPos(hwnd: Hwnd, after: Hwnd, x: i32, y: i32, w: i32, h: i32, flags: u32) -> i32;
    fn ShowWindow(hwnd: Hwnd, command: i32) -> i32;
    fn IsWindow(hwnd: Hwnd) -> i32;
    fn IsWindowVisible(hwnd: Hwnd) -> i32;
    fn GetWindow(hwnd: Hwnd, command: u32) -> Hwnd;
    fn GetWindowRect(hwnd: Hwnd, rect: *mut [i32; 4]) -> i32;
    fn GetDpiForWindow(hwnd: Hwnd) -> u32;
    fn SetWindowRgn(hwnd: Hwnd, region: *mut c_void, redraw: i32) -> i32;
    fn SetLayeredWindowAttributes(hwnd: Hwnd, key: u32, alpha: u8, flags: u32) -> i32;
    fn UpdateLayeredWindow(
        hwnd: Hwnd,
        screen: *mut c_void,
        position: *const Point,
        size: *const Extent,
        source: *mut c_void,
        source_at: *const Point,
        key: u32,
        blend: *const BlendFunction,
        flags: u32,
    ) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn GetStockObject(object: i32) -> *mut c_void;
    fn CreateRoundRectRgn(x1: i32, y1: i32, x2: i32, y2: i32, w: i32, h: i32) -> *mut c_void;
    fn CreateCompatibleDC(dc: *mut c_void) -> *mut c_void;
    fn CreateDIBSection(
        dc: *mut c_void,
        info: *const BitmapInfoHeader,
        usage: u32,
        bits: *mut *mut u8,
        section: *mut c_void,
        offset: u32,
    ) -> *mut c_void;
    fn SelectObject(dc: *mut c_void, object: *mut c_void) -> *mut c_void;
    fn DeleteObject(object: *mut c_void) -> i32;
    fn DeleteDC(dc: *mut c_void) -> i32;
}

const BLACK_BRUSH: i32 = 4;
const GW_OWNER: u32 = 4;
const SW_HIDE: i32 = 0;
const WS_POPUP: u32 = 0x8000_0000;
/// See-through, never active, not in the taskbar.
const LAYERED: u32 = 0x0008_0000 | 0x0800_0000 | 0x80;
/// And every click goes through to what is below.
const CLICK_THROUGH: u32 = 0x20;
const LWA_ALPHA: u32 = 2;
const ULW_ALPHA: u32 = 2;
const AC_SRC_ALPHA: u8 = 1;
const SWP_NOSIZE: u32 = 0x1;
const SWP_NOMOVE: u32 = 0x2;
const SWP_NOZORDER: u32 = 0x4;
const SWP_NOACTIVATE: u32 = 0x10;
const SWP_SHOWWINDOW: u32 = 0x40;
const WM_MOUSEACTIVATE: u32 = 0x21;
const WM_LBUTTONDOWN: u32 = 0x201;
const MA_NOACTIVATE: isize = 3;

/// Corner radius of a page and width of the outline, in points at 100 %
/// scaling.
const PAGE_RADIUS: i32 = 10;
const RING_WIDTH: i32 = 2;
/// How dark the pages behind a peek get, of 255 (macOS: 45 %).
const DIM_ALPHA: u8 = 115;

/// The outline's window and what it was last drawn for.
struct Ring {
    hwnd: Hwnd,
    /// The page it outlines; null while there is none.
    page: Hwnd,
    argb: u32,
    drawn: (i32, i32, u32),
}

thread_local! {
    static RING: RefCell<Option<Ring>> = const { RefCell::new(None) };
    /// The window that dims the pages behind a peek, and its size.
    static BACKDROP: Cell<(isize, (i32, i32))> = const { Cell::new((0, (0, 0))) };
    static CLASS: Cell<bool> = const { Cell::new(false) };
}

/// A see-through window of our class, owned by `owner`, hidden.
fn layer(owner: Hwnd, click_through: bool) -> Hwnd {
    let name: Vec<u16> = "MagnetiteLayer\0".encode_utf16().collect();
    // SAFETY: a class with our window procedure, registered once for this
    // module, and a window of it; the name is NUL-terminated and copied.
    unsafe {
        if !CLASS.replace(true) {
            let class = WindowClass {
                style: 0,
                procedure: on_message,
                class_extra: 0,
                window_extra: 0,
                instance: GetModuleHandleW(std::ptr::null()),
                icon: std::ptr::null_mut(),
                cursor: std::ptr::null_mut(),
                // What the backdrop is filled with.
                background: GetStockObject(BLACK_BRUSH),
                menu_name: std::ptr::null(),
                class_name: name.as_ptr(),
            };
            RegisterClassW(&raw const class);
        }
        CreateWindowExW(
            if click_through {
                LAYERED | CLICK_THROUGH
            } else {
                LAYERED
            },
            name.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            owner,
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null_mut(),
        )
    }
}

unsafe extern "system" fn on_message(
    hwnd: Hwnd,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    match message {
        // A click does not take the keyboard from the page.
        WM_MOUSEACTIVATE => return MA_NOACTIVATE,
        // A click beside the peek closes it.
        WM_LBUTTONDOWN if BACKDROP.get().0 == hwnd as isize => {
            Engine::global().emit(EngineEvent::Shortcut {
                from: None,
                shortcut: Shortcut::ClosePeek,
            });
            return 0;
        }
        _ => {}
    }
    // SAFETY: the default handling for everything else.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

/// Outline the page whose window is `page` in `argb` (the focused page of a
/// split), or remove the outline (`None`). There is one outline at most.
pub fn set_focus_ring(page: Option<cef::sys::cef_window_handle_t>, argb: u32) {
    RING.with_borrow_mut(|ring| {
        let Some(page) = page.map(|p| p.0.cast::<c_void>()).filter(|p| !p.is_null()) else {
            if let Some(ring) = ring.as_mut() {
                ring.page = std::ptr::null_mut();
                // SAFETY: our own window.
                unsafe { ShowWindow(ring.hwnd, SW_HIDE) };
            }
            return;
        };
        // SAFETY: a plain query on a live window.
        let owner = unsafe { GetWindow(page, GW_OWNER) };
        let ring = ring.get_or_insert_with(|| Ring {
            hwnd: layer(owner, true),
            page,
            argb,
            drawn: (0, 0, 0),
        });
        ring.page = page;
        ring.argb = argb;
        place_ring(ring);
    });
}

/// The page `page` moved or changed its size: its outline follows.
pub(super) fn follow_ring(page: Hwnd) {
    RING.with(|ring| {
        if let Ok(mut ring) = ring.try_borrow_mut()
            && let Some(ring) = ring.as_mut()
            && ring.page == page
        {
            place_ring(ring);
        }
    });
}

fn place_ring(ring: &mut Ring) {
    let mut rect = [0i32; 4];
    // SAFETY: queries on a window that may be gone (then they fail), and
    // calls on our own window.
    unsafe {
        if ring.hwnd.is_null()
            || IsWindow(ring.page) == 0
            || IsWindowVisible(ring.page) == 0
            || GetWindowRect(ring.page, &raw mut rect) == 0
        {
            ShowWindow(ring.hwnd, SW_HIDE);
            return;
        }
        let size = (rect[2] - rect[0], rect[3] - rect[1]);
        let dpi = i32::try_from(GetDpiForWindow(ring.page))
            .unwrap_or(96)
            .max(96);
        if ring.drawn == (size.0, size.1, ring.argb) {
            SetWindowPos(
                ring.hwnd,
                std::ptr::null_mut(),
                rect[0],
                rect[1],
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        } else if draw_ring(ring.hwnd, (rect[0], rect[1]), size, ring.argb, dpi) {
            ring.drawn = (size.0, size.1, ring.argb);
        }
        // Above its page: on top of the frame's windows as they are now.
        SetWindowPos(
            ring.hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

/// Paint the outline of a page of `size` pixels into the window `hwnd` and
/// put it at `at`.
fn draw_ring(hwnd: Hwnd, at: (i32, i32), size: (i32, i32), argb: u32, dpi: i32) -> bool {
    let Ok(len) = usize::try_from(size.0 * size.1 * 4) else {
        return false;
    };
    if len == 0 {
        return false;
    }
    let header = BitmapInfoHeader {
        size: 40,
        width: size.0,
        // Negative: the first row is the top one.
        height: -size.1,
        planes: 1,
        bit_count: 32,
        compression: 0,
        size_image: 0,
        x_per_meter: 0,
        y_per_meter: 0,
        colours_used: 0,
        colours_important: 0,
    };
    let mut bits: *mut u8 = std::ptr::null_mut();
    // SAFETY: GDI objects made, used and freed here. `bits` points at
    // `len` bytes for as long as the bitmap lives.
    unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        let bitmap = CreateDIBSection(
            dc,
            &raw const header,
            0,
            &raw mut bits,
            std::ptr::null_mut(),
            0,
        );
        if dc.is_null() || bitmap.is_null() || bits.is_null() {
            DeleteObject(bitmap);
            DeleteDC(dc);
            return false;
        }
        ring_pixels(
            std::slice::from_raw_parts_mut(bits, len),
            size,
            PAGE_RADIUS * dpi / 96,
            RING_WIDTH * dpi / 96,
            argb,
        );
        let position = Point { x: at.0, y: at.1 };
        let extent = Extent {
            width: size.0,
            height: size.1,
        };
        let origin = Point { x: 0, y: 0 };
        let blend = BlendFunction {
            op: 0,
            flags: 0,
            alpha: 255,
            format: AC_SRC_ALPHA,
        };
        let old = SelectObject(dc, bitmap);
        let ok = UpdateLayeredWindow(
            hwnd,
            std::ptr::null_mut(),
            &raw const position,
            &raw const extent,
            dc,
            &raw const origin,
            0,
            &raw const blend,
            ULW_ALPHA,
        );
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        ok != 0
    }
}

/// Fill `pixels` (BGRA with the colours already multiplied by the alpha,
/// `size` wide and high) with a line `width` pixels wide along the inside
/// of a rounded rectangle that fills the picture, in the colour `argb`.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn ring_pixels(pixels: &mut [u8], size: (i32, i32), radius: i32, width: i32, argb: u32) {
    let half = (size.0 as f32 / 2.0, size.1 as f32 / 2.0);
    let radius = (radius as f32).min(half.0).min(half.1);
    let channel = |shift: u32| ((argb >> shift) & 0xFF) as f32;
    let columns = usize::try_from(size.0).unwrap_or(0).max(1);
    for (i, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = (i % columns) as f32 + 0.5 - half.0;
        let y = (i / columns) as f32 + 0.5 - half.1;
        // Distance from the rounded rectangle's edge: negative inside.
        let qx = x.abs() - (half.0 - radius);
        let qy = y.abs() - (half.1 - radius);
        let edge = qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius;
        // Inside the edge and within the line's width of it, with soft ends.
        let alpha = (0.5 - edge).clamp(0.0, 1.0) * (edge + width as f32 + 0.5).clamp(0.0, 1.0);
        let part = |shift: u32| (channel(shift) * alpha).round() as u8;
        *pixel = [part(0), part(8), part(16), (alpha * 255.0).round() as u8];
    }
}

/// Dim `area` (the pages, in screen pixels) behind the peek window `peek`:
/// a window of the frame `frame` right below the peek. A click on it closes
/// the peek.
pub fn show_peek_backdrop(
    frame: cef::sys::cef_window_handle_t,
    peek: cef::sys::cef_window_handle_t,
    area: &Rect,
) {
    let (frame, peek): (Hwnd, Hwnd) = (frame.0.cast(), peek.0.cast());
    if frame.is_null() || peek.is_null() {
        return;
    }
    let (mut hwnd, mut size) = BACKDROP.get();
    if hwnd == 0 {
        let made = layer(frame, false);
        if made.is_null() {
            return;
        }
        // SAFETY: our own window: one alpha for all of it.
        unsafe { SetLayeredWindowAttributes(made, 0, DIM_ALPHA, LWA_ALPHA) };
        hwnd = made as isize;
    }
    // SAFETY: our own window, placed after the live peek window; the system
    // owns the region once `SetWindowRgn` took it.
    unsafe {
        if size != (area.width, area.height) {
            size = (area.width, area.height);
            let dpi = i32::try_from(GetDpiForWindow(frame)).unwrap_or(96).max(96);
            let diameter = 2 * PAGE_RADIUS * dpi / 96;
            let region = CreateRoundRectRgn(0, 0, size.0 + 1, size.1 + 1, diameter, diameter);
            if !region.is_null() {
                SetWindowRgn(hwnd as Hwnd, region, 1);
            }
        }
        SetWindowPos(
            hwnd as Hwnd,
            peek,
            area.x,
            area.y,
            area.width,
            area.height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
    BACKDROP.set((hwnd, size));
}

/// Remove the peek backdrop (the peek closed or became a tab).
pub fn hide_peek_backdrop() {
    let (hwnd, _) = BACKDROP.get();
    if hwnd != 0 {
        // SAFETY: our own window.
        unsafe { ShowWindow(hwnd as Hwnd, SW_HIDE) };
    }
}

#[cfg(test)]
mod tests {
    use super::ring_pixels;

    #[test]
    fn the_outline_runs_along_the_inside_of_the_edge() {
        let (width, height) = (40usize, 30usize);
        let mut pixels = vec![0u8; width * height * 4];
        ring_pixels(&mut pixels, (40, 30), 6, 2, 0xFF11_2233);
        let at = |x: usize, y: usize| &pixels[(y * width + x) * 4..][..4];
        // On the line: the colour as blue, green, red, fully there.
        assert_eq!(at(20, 0), [0x33, 0x22, 0x11, 0xFF]);
        assert_eq!(at(0, 15), [0x33, 0x22, 0x11, 0xFF]);
        // Inside the page and outside the rounded corner: nothing.
        assert_eq!(at(20, 15), [0, 0, 0, 0]);
        assert_eq!(at(0, 0), [0, 0, 0, 0]);
    }
}
