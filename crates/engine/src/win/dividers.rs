//! Dividers in the gaps between split pages: drag to resize, double-click
//! to even out (what `MagnetiteDividerView` in `mac/chrome.rs` does).
//!
//! Each divider is a small window of the frame, all but invisible, that
//! only takes the mouse: the gap behind it shows the frame's chrome. The
//! frame itself cannot take it there, because its page area drags the window.

use std::cell::RefCell;
use std::ffi::c_void;

use cef::{Point, Rect, display_convert_screen_point_from_pixels};

use crate::engine::Engine;
use crate::events::{DividerAction, EngineEvent};

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
    fn DestroyWindow(hwnd: Hwnd) -> i32;
    fn SetWindowPos(hwnd: Hwnd, after: Hwnd, x: i32, y: i32, w: i32, h: i32, flags: u32) -> i32;
    fn SetLayeredWindowAttributes(hwnd: Hwnd, key: u32, alpha: u8, flags: u32) -> i32;
    fn LoadCursorW(instance: *mut c_void, name: *const u16) -> *mut c_void;
    fn SetCursor(cursor: *mut c_void) -> *mut c_void;
    fn SetCapture(hwnd: Hwnd) -> Hwnd;
    fn GetCapture() -> Hwnd;
    fn ReleaseCapture() -> i32;
    fn GetCursorPos(point: *mut [i32; 2]) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
}

/// Double clicks arrive as such.
const CS_DBLCLKS: u32 = 0x8;
const WS_POPUP: u32 = 0x8000_0000;
/// See-through by an alpha of its own, never active, not in the taskbar.
const EX_STYLE: u32 = 0x0008_0000 | 0x0800_0000 | 0x80;
const LWA_ALPHA: u32 = 2;
const SWP_NOACTIVATE: u32 = 0x10;
const SWP_SHOWWINDOW: u32 = 0x40;
const WM_SETCURSOR: u32 = 0x20;
const WM_MOUSEACTIVATE: u32 = 0x21;
const WM_MOUSEMOVE: u32 = 0x200;
const WM_LBUTTONDOWN: u32 = 0x201;
const WM_LBUTTONUP: u32 = 0x202;
const WM_LBUTTONDBLCLK: u32 = 0x203;
const WM_CAPTURECHANGED: u32 = 0x215;
const MA_NOACTIVATE: isize = 3;
/// The system's arrows for resizing sideways and up and down.
const IDC_SIZEWE: usize = 32644;
const IDC_SIZENS: usize = 32645;

/// Which divider a window is: as `magnetite_core::split::Divider`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DividerKey {
    pub path: Vec<usize>,
    pub index: usize,
    /// Pages side by side (a vertical divider); else stacked.
    pub row: bool,
}

struct Divider {
    hwnd: Hwnd,
    key: DividerKey,
    /// The mouse button went down on it and is still held.
    dragging: bool,
}

thread_local! {
    /// The dividers on screen. Only the UI thread has any.
    static DIVIDERS: RefCell<Vec<Divider>> = const { RefCell::new(Vec::new()) };
    static CLASS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn class_name() -> Vec<u16> {
    "MagnetiteDivider\0".encode_utf16().collect()
}

/// Show exactly `dividers` (screen pixels) as windows of the frame `frame`.
/// Windows are reused by their place in the list, so a divider being
/// dragged stays the same window and the drag goes on.
pub fn set_dividers(frame: cef::sys::cef_window_handle_t, dividers: &[(Rect, DividerKey)]) {
    let frame: Hwnd = frame.0.cast();
    if frame.is_null() {
        return;
    }
    let name = class_name();
    // SAFETY: a class with our window procedure, registered once for this
    // module; its name is NUL-terminated and copied by Windows.
    unsafe {
        if !CLASS.replace(true) {
            let class = WindowClass {
                style: CS_DBLCLKS,
                procedure: on_message,
                class_extra: 0,
                window_extra: 0,
                instance: GetModuleHandleW(std::ptr::null()),
                icon: std::ptr::null_mut(),
                cursor: std::ptr::null_mut(),
                background: std::ptr::null_mut(),
                menu_name: std::ptr::null(),
                class_name: name.as_ptr(),
            };
            RegisterClassW(&raw const class);
        }
    }
    // The windows that are no longer needed are destroyed after the list is
    // let go: destroying one calls `on_message`, which reads the list.
    let mut gone = Vec::new();
    DIVIDERS.with_borrow_mut(|list| {
        while list.len() > dividers.len() {
            gone.extend(list.pop().map(|d| d.hwnd));
        }
        for (i, (_, key)) in dividers.iter().enumerate() {
            if let Some(d) = list.get_mut(i) {
                d.key = key.clone();
                continue;
            }
            // SAFETY: a window of our class, owned by the live frame.
            let hwnd = unsafe {
                CreateWindowExW(
                    EX_STYLE,
                    name.as_ptr(),
                    std::ptr::null(),
                    WS_POPUP,
                    0,
                    0,
                    0,
                    0,
                    frame,
                    std::ptr::null_mut(),
                    GetModuleHandleW(std::ptr::null()),
                    std::ptr::null_mut(),
                )
            };
            if hwnd.is_null() {
                break;
            }
            // SAFETY: our own window. An alpha of 1 of 255: nothing to
            // see, yet it takes the mouse (a fully clear window does not).
            unsafe { SetLayeredWindowAttributes(hwnd, 0, 1, LWA_ALPHA) };
            list.push(Divider {
                hwnd,
                key: key.clone(),
                dragging: false,
            });
        }
    });
    let windows: Vec<Hwnd> = DIVIDERS.with_borrow(|list| list.iter().map(|d| d.hwnd).collect());
    for (hwnd, (at, _)) in windows.into_iter().zip(dividers) {
        // SAFETY: our own windows; moving one may call `on_message`, which
        // finds the list free.
        unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                at.x,
                at.y,
                at.width,
                at.height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
    for hwnd in gone {
        // SAFETY: a window we made and took off the list.
        unsafe { DestroyWindow(hwnd) };
    }
}

/// What `on_message` needs to know of the divider `hwnd`, with its
/// `dragging` set to `dragging` if given. `None` for a window not on the
/// list, or while `set_dividers` works on the list.
fn divider(hwnd: Hwnd, dragging: Option<bool>) -> Option<(DividerKey, bool)> {
    DIVIDERS.with(|list| {
        let mut list = list.try_borrow_mut().ok()?;
        let d = list.iter_mut().find(|d| d.hwnd == hwnd)?;
        let was = d.dragging;
        if let Some(now) = dragging {
            d.dragging = now;
        }
        Some((d.key.clone(), was))
    })
}

fn report(key: DividerKey, action: DividerAction) {
    Engine::global().emit(EngineEvent::SplitDivider {
        path: key.path,
        index: key.index,
        action,
    });
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
        WM_SETCURSOR => {
            if let Some((key, _)) = divider(hwnd, None) {
                let arrows = if key.row { IDC_SIZEWE } else { IDC_SIZENS };
                // SAFETY: one of the system's own cursors, named by number.
                unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), arrows as *const u16)) };
                return 1;
            }
        }
        WM_LBUTTONDOWN => {
            if divider(hwnd, Some(true)).is_some() {
                // SAFETY: our own window; the mouse stays with it until the
                // button is let go, wherever the pointer goes.
                unsafe { SetCapture(hwnd) };
            }
            return 0;
        }
        WM_LBUTTONDBLCLK => {
            if let Some((key, _)) = divider(hwnd, Some(false)) {
                report(key, DividerAction::Equalize);
            }
            return 0;
        }
        WM_MOUSEMOVE => {
            // SAFETY: plain queries; `at` is a POINT.
            let mut at = [0i32; 2];
            let ours = unsafe { GetCapture() == hwnd && GetCursorPos(&raw mut at) != 0 };
            if ours && let Some((key, true)) = divider(hwnd, None) {
                // The app counts in points, Windows in pixels.
                let at =
                    display_convert_screen_point_from_pixels(Some(&Point { x: at[0], y: at[1] }));
                report(
                    key.clone(),
                    DividerAction::Move(if key.row { at.x } else { at.y }),
                );
            }
            return 0;
        }
        WM_LBUTTONUP => {
            // SAFETY: gives the mouse back; `WM_CAPTURECHANGED` follows.
            unsafe { ReleaseCapture() };
            return 0;
        }
        WM_CAPTURECHANGED => {
            if let Some((key, true)) = divider(hwnd, Some(false)) {
                report(key, DividerAction::Release);
            }
            return 0;
        }
        _ => {}
    }
    // SAFETY: the default handling for everything else.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
