//! The system-wide quick search key (what `mac/hotkey.rs` does with Carbon):
//! Ctrl+Alt+N, also while another program is in front.
//!
//! Windows sends a registered key to a window. Ours is one nobody sees, made
//! for these messages only.

use std::cell::Cell;
use std::ffi::c_void;

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
        parent: Hwnd,
        menu: *mut c_void,
        instance: *mut c_void,
        param: *mut c_void,
    ) -> Hwnd;
    fn DefWindowProcW(hwnd: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize;
    fn RegisterHotKey(hwnd: Hwnd, id: i32, modifiers: u32, key: u32) -> i32;
    fn UnregisterHotKey(hwnd: Hwnd, id: i32) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
}

/// The parent of a window that only takes messages.
const HWND_MESSAGE: isize = -3;
const WM_HOTKEY: u32 = 0x312;
const MOD_ALT: u32 = 0x1;
const MOD_CONTROL: u32 = 0x2;
/// Holding the keys down is one press.
const MOD_NOREPEAT: u32 = 0x4000;
const KEY_N: u32 = 0x4E;
const QUICK_SEARCH: i32 = 1;

thread_local! {
    /// The window the key's messages go to, once made.
    static WINDOW: Cell<isize> = const { Cell::new(0) };
    static REGISTERED: Cell<bool> = const { Cell::new(false) };
}

unsafe extern "system" fn on_message(
    hwnd: Hwnd,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if message == WM_HOTKEY && wparam == QUICK_SEARCH.cast_unsigned() as usize {
        Engine::global().emit(EngineEvent::Shortcut {
            from: None,
            shortcut: Shortcut::NewMini,
        });
        return 0;
    }
    // SAFETY: the default handling for everything else.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

/// The message window, made on first use.
fn window() -> Hwnd {
    if WINDOW.get() != 0 {
        return WINDOW.get() as Hwnd;
    }
    let name: Vec<u16> = "MagnetiteHotKey\0".encode_utf16().collect();
    // SAFETY: a class with our window procedure, registered once for this
    // module, and a message-only window of it; the name is NUL-terminated
    // and copied by Windows.
    let hwnd = unsafe {
        let class = WindowClass {
            style: 0,
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
        CreateWindowExW(
            0,
            name.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE as Hwnd,
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null_mut(),
        )
    };
    WINDOW.set(hwnd as isize);
    hwnd
}

/// Register Ctrl+Alt+N system-wide (`on`), or let it go. UI thread only.
/// Taken by another program, it stays unregistered (traced).
pub fn set_quick_search_hotkey(on: bool) {
    if on == REGISTERED.get() {
        return;
    }
    let hwnd = window();
    if hwnd.is_null() {
        crate::trace("cannot listen for the quick search key");
        return;
    }
    // SAFETY: our own window; the key is ours until it is unregistered.
    unsafe {
        if on {
            let ok = RegisterHotKey(
                hwnd,
                QUICK_SEARCH,
                MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
                KEY_N,
            ) != 0;
            REGISTERED.set(ok);
            if !ok {
                crate::trace("the quick search key is taken by another program");
            }
        } else {
            UnregisterHotKey(hwnd, QUICK_SEARCH);
            REGISTERED.set(false);
        }
    }
}
