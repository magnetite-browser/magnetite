//! The hover zone along the frame's left edge that slides the floating
//! sidebar out while the docked one is hidden (`floatbar.rs`).
//!
//! macOS puts an invisible window with a tracking area there. Here a timer
//! on the UI thread looks where the pointer is: nothing sits in front of the
//! frame's edge, so its resize border keeps working.

use std::cell::Cell;
use std::ffi::c_void;

use crate::engine::Engine;

type Hwnd = *mut c_void;

type TimerProc = unsafe extern "system" fn(hwnd: Hwnd, message: u32, id: usize, time: u32);

#[link(name = "user32")]
unsafe extern "system" {
    fn SetTimer(hwnd: Hwnd, id: usize, milliseconds: u32, callback: Option<TimerProc>) -> usize;
    fn GetCursorPos(point: *mut [i32; 2]) -> i32;
    fn GetWindowRect(hwnd: Hwnd, rect: *mut [i32; 4]) -> i32;
    fn GetDpiForWindow(hwnd: Hwnd) -> u32;
    fn IsWindowVisible(hwnd: Hwnd) -> i32;
    fn IsIconic(hwnd: Hwnd) -> i32;
    fn GetForegroundWindow() -> Hwnd;
    fn GetWindowThreadProcessId(hwnd: Hwnd, process: *mut u32) -> u32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcessId() -> u32;
}

/// How often the pointer is looked at: quick enough to feel immediate.
const EVERY_MS: u32 = 80;

/// The zone: the frame's window, and the zone's width and distance from the
/// frame's top in points at 100 % scaling.
#[derive(Clone, Copy)]
struct Zone {
    frame: isize,
    width: i32,
    top: i32,
}

thread_local! {
    static ZONE: Cell<Option<Zone>> = const { Cell::new(None) };
    /// The pointer was in the zone at the last look: the sidebar slides out
    /// when the pointer comes in, not for as long as it stays.
    static INSIDE: Cell<bool> = const { Cell::new(false) };
    static TIMER: Cell<bool> = const { Cell::new(false) };
}

/// Watch the left edge of the frame `frame`, `width` points wide and from
/// `top` points below the frame's top edge down (`Some((width, top))`), or
/// stop watching (`None`).
pub fn set_edge_zone(frame: cef::sys::cef_window_handle_t, zone: Option<(i32, i32)>) {
    let frame = frame.0 as isize;
    ZONE.set(
        zone.filter(|_| frame != 0)
            .map(|(width, top)| Zone { frame, width, top }),
    );
    if ZONE.get().is_none() {
        INSIDE.set(false);
        return;
    }
    if !TIMER.replace(true) {
        // SAFETY: a timer of this thread without a window: Windows calls
        // `look` from this thread's message loop. It runs for the life of
        // the process and does nothing while there is no zone.
        unsafe { SetTimer(std::ptr::null_mut(), 0, EVERY_MS, Some(look)) };
    }
}

unsafe extern "system" fn look(_hwnd: Hwnd, _message: u32, _id: usize, _time: u32) {
    let Some(zone) = ZONE.get() else { return };
    let inside = pointer_in(zone);
    if inside && !INSIDE.replace(inside) {
        // Not from inside the timer's callback: windows may be created.
        Engine::post(Box::new(|| Engine::global().show_float_sidebar(true)));
    }
    INSIDE.set(inside);
}

/// Whether the pointer is in `zone` of a frame that is on screen and whose
/// program is the one in front (another program's window may cover it).
fn pointer_in(zone: Zone) -> bool {
    let frame = zone.frame as Hwnd;
    let (mut at, mut rect, mut process) = ([0i32; 2], [0i32; 4], 0u32);
    // SAFETY: plain queries; `at` is a POINT, `rect` a RECT.
    let dpi = unsafe {
        if IsWindowVisible(frame) == 0
            || IsIconic(frame) != 0
            || GetCursorPos(&raw mut at) == 0
            || GetWindowRect(frame, &raw mut rect) == 0
        {
            return false;
        }
        GetWindowThreadProcessId(GetForegroundWindow(), &raw mut process);
        if process != GetCurrentProcessId() {
            return false;
        }
        i32::try_from(GetDpiForWindow(frame)).unwrap_or(96).max(96)
    };
    in_zone(at, rect, zone.width * dpi / 96, zone.top * dpi / 96)
}

/// Whether the point `at` is in the strip `width` pixels wide along the left
/// edge of `rect` (left, top, right, bottom), from `top` pixels below its
/// top edge down.
fn in_zone(at: [i32; 2], rect: [i32; 4], width: i32, top: i32) -> bool {
    (rect[0]..rect[0] + width).contains(&at[0]) && (rect[1] + top..rect[3]).contains(&at[1])
}

#[cfg(test)]
mod tests {
    use super::in_zone;

    #[test]
    fn the_zone_is_the_left_edge_below_the_top_row() {
        let frame = [100, 50, 900, 650];
        assert!(in_zone([100, 300], frame, 8, 40));
        assert!(in_zone([107, 649], frame, 8, 40));
        // Beside it, above it, below the frame, left of the frame.
        assert!(!in_zone([108, 300], frame, 8, 40));
        assert!(!in_zone([100, 89], frame, 8, 40));
        assert!(!in_zone([100, 650], frame, 8, 40));
        assert!(!in_zone([99, 300], frame, 8, 40));
    }
}
