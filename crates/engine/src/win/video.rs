//! Chromium's floating video window (picture-in-picture) on Windows.
//!
//! It is Chromium's own window, with Chromium's controls, and it has a
//! button of its own in the taskbar, which showed Chromium's icon. When a
//! page reports picture-in-picture, the window is found and given
//! Magnetite's icon. It can lag the report: a timer looks for it for a
//! moment.

use std::cell::Cell;
use std::ffi::c_void;

use cef::ImplWindow;

use crate::engine::Engine;

type Hwnd = *mut c_void;

type EnumProc = unsafe extern "system" fn(hwnd: Hwnd, param: isize) -> i32;

type TimerProc = unsafe extern "system" fn(hwnd: Hwnd, message: u32, id: usize, time: u32);

#[link(name = "user32")]
unsafe extern "system" {
    fn EnumWindows(callback: EnumProc, param: isize) -> i32;
    fn IsWindowVisible(hwnd: Hwnd) -> i32;
    fn GetWindow(hwnd: Hwnd, command: u32) -> Hwnd;
    fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> isize;
    fn GetWindowThreadProcessId(hwnd: Hwnd, process: *mut u32) -> u32;
    fn GetClassNameW(hwnd: Hwnd, name: *mut u16, max: i32) -> i32;
    fn SendMessageW(hwnd: Hwnd, message: u32, wparam: usize, lparam: isize) -> isize;
    fn SetTimer(hwnd: Hwnd, id: usize, milliseconds: u32, callback: TimerProc) -> usize;
    fn KillTimer(hwnd: Hwnd, id: usize) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcessId() -> u32;
    fn GetModuleFileNameW(module: *mut c_void, name: *mut u16, size: u32) -> u32;
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn ExtractIconExW(
        file: *const u16,
        index: i32,
        large: *mut *mut c_void,
        small: *mut *mut c_void,
        count: u32,
    ) -> u32;
}

const GW_OWNER: u32 = 4;
const GWL_EXSTYLE: i32 = -20;
const WS_EX_TOPMOST: isize = 0x8;
const WM_SETICON: u32 = 0x80;
const ICON_SMALL: usize = 0;
const ICON_BIG: usize = 1;
/// How often and how many times the window is looked for after a report.
const EVERY_MS: u32 = 200;
const TRIES: u8 = 15;

thread_local! {
    /// The running timer and the looks it has left.
    static SEARCH: Cell<(usize, u8)> = const { Cell::new((0, 0)) };
    /// The program's icon, large and small, read from its file once.
    static ICONS: Cell<Option<(isize, isize)>> = const { Cell::new(None) };
    /// The window that has the icon already.
    static DONE: Cell<isize> = const { Cell::new(0) };
}

/// A page reports a video in picture-in-picture: its window gets our icon.
pub fn brand_video_window() {
    if look() {
        return;
    }
    let (timer, _) = SEARCH.get();
    if timer == 0 {
        // SAFETY: a timer of this thread without a window: Windows calls
        // `on_timer` from this thread's message loop until it is killed.
        let timer = unsafe { SetTimer(std::ptr::null_mut(), 0, EVERY_MS, on_timer) };
        SEARCH.set((timer, TRIES));
    } else {
        SEARCH.set((timer, TRIES));
    }
}

unsafe extern "system" fn on_timer(_hwnd: Hwnd, _message: u32, id: usize, _time: u32) {
    let (timer, left) = SEARCH.get();
    if id != timer {
        return;
    }
    if look() || left <= 1 {
        SEARCH.set((0, 0));
        // SAFETY: the timer this function was called for.
        unsafe { KillTimer(std::ptr::null_mut(), timer) };
    } else {
        SEARCH.set((timer, left - 1));
    }
}

/// Give the video window our icon; whether it was there.
fn look() -> bool {
    let Some(video) = find_video_window() else {
        return false;
    };
    if DONE.get() == video as isize {
        return true;
    }
    let Some((large, small)) = icons() else {
        return true;
    };
    // SAFETY: a live window of this process and icons that live as long as
    // the process; the window does not own them.
    unsafe {
        SendMessageW(video, WM_SETICON, ICON_BIG, large);
        SendMessageW(video, WM_SETICON, ICON_SMALL, small);
    }
    DONE.set(video as isize);
    true
}

/// The icon of the program's file, large and small.
fn icons() -> Option<(isize, isize)> {
    if let Some(icons) = ICONS.get() {
        return Some(icons);
    }
    let mut path = [0u16; 1024];
    let (mut large, mut small) = (std::ptr::null_mut(), std::ptr::null_mut());
    // SAFETY: `path` has room for its length in characters and stays
    // NUL-terminated (a longer path is cut and then finds no icon); the two
    // pointers take one icon each.
    unsafe {
        let length = GetModuleFileNameW(std::ptr::null_mut(), path.as_mut_ptr(), 1023);
        if length == 0 {
            return None;
        }
        ExtractIconExW(path.as_ptr(), 0, &raw mut large, &raw mut small, 1);
    }
    if large.is_null() || small.is_null() {
        return None;
    }
    let icons = (large as isize, small as isize);
    ICONS.set(Some(icons));
    Some(icons)
}

/// Whether `hwnd` is one of the windows Magnetite itself makes through CEF.
fn ours(hwnd: Hwnd) -> bool {
    let engine = Engine::global();
    let inner = engine.lock();
    let is = |w: &cef::Window| w.window_handle().0.cast::<c_void>() == hwnd;
    inner.frame.as_ref().is_some_and(|f| is(&f.window))
        || inner.overlay.as_ref().is_some_and(is)
        || inner.notifications.as_ref().is_some_and(is)
        || (inner.float_bar.as_ref()).is_some_and(|f| f.window.as_ref().is_some_and(is))
        || (inner.contents.values()).any(|c| c.window.as_ref().is_some_and(is))
}

/// Chromium's video window: one of Chromium's own windows in this process,
/// visible, above other programs' windows, nobody's, and not one of ours
/// (the quick search is above other programs too).
fn is_video_window(hwnd: Hwnd) -> bool {
    let mut process = 0u32;
    let mut class = [0u16; 32];
    // SAFETY: plain queries on a window that may be gone (then they fail).
    let chromium = unsafe {
        GetWindowThreadProcessId(hwnd, &raw mut process);
        let length = GetClassNameW(hwnd, class.as_mut_ptr(), 32);
        let length = usize::try_from(length).unwrap_or(0).min(class.len());
        process == GetCurrentProcessId()
            && String::from_utf16_lossy(&class[..length]) == "Chrome_WidgetWin_1"
            && IsWindowVisible(hwnd) != 0
            && GetWindow(hwnd, GW_OWNER).is_null()
            && GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOPMOST != 0
    };
    chromium && !ours(hwnd)
}

/// The video window on screen, if there is one.
fn find_video_window() -> Option<Hwnd> {
    unsafe extern "system" fn collect(hwnd: Hwnd, param: isize) -> i32 {
        // SAFETY: `param` is the list `find_video_window` passed in, alive
        // for the whole of `EnumWindows`.
        unsafe { (*(param as *mut Vec<isize>)).push(hwnd as isize) };
        1
    }
    let mut all: Vec<isize> = Vec::new();
    // SAFETY: `collect` only pushes onto `all`, which outlives the call.
    unsafe { EnumWindows(collect, (&raw mut all) as isize) };
    all.into_iter()
        .map(|h| h as Hwnd)
        .find(|h| is_video_window(*h))
}
