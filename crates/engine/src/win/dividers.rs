//! Dividers in the gaps between split pages, between a page and its docked
//! `DevTools`, and the resize handles of a page in device mode: drag to
//! resize, double-click to even out or go back to the default size (what
//! `MagnetiteDividerView` in `mac/chrome.rs` does).
//!
//! Each divider is a small window of the frame, all but invisible, that
//! takes the mouse: the gap behind it shows the frame's chrome. The frame
//! itself cannot take it there, because its page area drags the window.
//! Under the pointer it shows a short line (a grip), longer and in brass
//! while dragged, as on macOS; the handles of a page in device mode show
//! theirs faintly all the time, so that they can be found.

use std::cell::RefCell;
use std::ffi::c_void;

use cef::{Point, Rect, display_convert_screen_point_from_pixels};

use crate::device::Edge;
use crate::engine::Engine;
use crate::events::{ContentId, DividerAction, EngineEvent};

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
    fn GetDpiForWindow(hwnd: Hwnd) -> u32;
    fn TrackMouseEvent(track: *mut TrackMouse) -> i32;
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
const SWP_NOSIZE: u32 = 0x1;
const SWP_NOMOVE: u32 = 0x2;
const SWP_NOZORDER: u32 = 0x4;
const TME_LEAVE: u32 = 2;
const WM_MOUSELEAVE: u32 = 0x2A3;
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

/// The system's arrows for resizing both ways at once.
const IDC_SIZENWSE: usize = 32642;

/// Which divider a window is (as `mac::DividerKey`). `row`: the panes on
/// its two sides are side by side (a vertical divider); else stacked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DividerKey {
    /// Between split pages: as `magnetite_core::split::Divider`.
    Split {
        path: Vec<usize>,
        index: usize,
        row: bool,
    },
    /// Between page `id` and its docked `DevTools` (`devdock.rs`).
    DevTools { id: ContentId, row: bool },
    /// Beside page `id` in device mode, to resize it (`device.rs`).
    Device { id: ContentId, edge: Edge },
}

impl DividerKey {
    /// Whether it moves across (a vertical bar); the corner moves both ways.
    fn row(&self) -> bool {
        match self {
            Self::Split { row, .. } | Self::DevTools { row, .. } => *row,
            Self::Device { edge, .. } => *edge != Edge::Bottom,
        }
    }

    /// The system's cursor that shows which way it moves.
    fn cursor(&self) -> usize {
        match self {
            Self::Device {
                edge: Edge::Corner, ..
            } => IDC_SIZENWSE,
            _ if self.row() => IDC_SIZEWE,
            _ => IDC_SIZENS,
        }
    }
}

/// `TRACKMOUSEEVENT`.
#[repr(C)]
struct TrackMouse {
    size: u32,
    flags: u32,
    hwnd: Hwnd,
    hover_time: u32,
}

/// How a divider's grip shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
    Hidden,
    /// A handle in device mode, away from the pointer.
    Faint,
    Hover,
    Dragging,
}

struct Divider {
    hwnd: Hwnd,
    key: DividerKey,
    /// Where it is on screen: x, y, width, height in pixels.
    at: [i32; 4],
    /// The mouse button went down on it and is still held.
    dragging: bool,
    /// The pointer is on it.
    hovering: bool,
}

impl Divider {
    fn look(&self) -> Look {
        if self.dragging {
            Look::Dragging
        } else if self.hovering {
            Look::Hover
        } else if matches!(self.key, DividerKey::Device { .. }) {
            Look::Faint
        } else {
            Look::Hidden
        }
    }
}

/// Thickness and length of the grip in points: longer while dragged.
const GRIP: i32 = 2;
const GRIP_LONG: i32 = 32;
const GRIP_DRAGGED: i32 = 56;
/// The corner handle shows a dot of this size.
const GRIP_DOT: i32 = 4;

/// Fill `pixels` (BGRA with the colours already multiplied by the alpha,
/// `size` wide and high) with the grip of a divider: a line in the middle,
/// along the divider. The rest has an alpha of 1 of 255: nothing to see,
/// yet the window takes the mouse there (a fully clear one does not).
fn grip_pixels(pixels: &mut [u8], size: (i32, i32), key: &DividerKey, look: Look, dpi: i32) {
    let scaled = |points: i32| (points * dpi / 96).max(1);
    let long = scaled(if look == Look::Dragging {
        GRIP_DRAGGED
    } else {
        GRIP_LONG
    });
    let (width, height) = match key {
        DividerKey::Device {
            edge: Edge::Corner, ..
        } => (scaled(GRIP_DOT), scaled(GRIP_DOT)),
        _ if key.row() => (scaled(GRIP), long),
        _ => (long, scaled(GRIP)),
    };
    let (width, height) = (width.min(size.0), height.min(size.1));
    let (left, top) = ((size.0 - width) / 2, (size.1 - height) / 2);
    // White at 35 % (20 % when faint), brass while dragged.
    let colour = match look {
        Look::Hidden => [0, 0, 0, 1],
        Look::Faint => [51, 51, 51, 51],
        Look::Hover => [89, 89, 89, 89],
        Look::Dragging => [102, 166, 214, 255],
    };
    let columns = usize::try_from(size.0).unwrap_or(0).max(1);
    for (i, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let x = i32::try_from(i % columns).unwrap_or(i32::MAX);
        let y = i32::try_from(i / columns).unwrap_or(i32::MAX);
        let inside = x >= left && x < left + width && y >= top && y < top + height;
        *pixel = if inside { colour } else { [0, 0, 0, 1] };
    }
}

/// Paint the divider `hwnd` as it is now, where it is.
fn paint(hwnd: Hwnd) {
    let Some((key, look, at)) = with_divider(hwnd, |d| (d.key.clone(), d.look(), d.at)) else {
        return;
    };
    // SAFETY: a plain query on our own window.
    let dpi = i32::try_from(unsafe { GetDpiForWindow(hwnd) })
        .unwrap_or(96)
        .max(96);
    let size = (at[2], at[3]);
    super::layers::paint_layer(hwnd, (at[0], at[1]), size, |pixels| {
        grip_pixels(pixels, size, &key, look, dpi);
    });
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
        for (i, (at, key)) in dividers.iter().enumerate() {
            if let Some(d) = list.get_mut(i) {
                d.key = key.clone();
                d.at = [at.x, at.y, at.width, at.height];
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
            list.push(Divider {
                hwnd,
                key: key.clone(),
                at: [at.x, at.y, at.width, at.height],
                dragging: false,
                hovering: false,
            });
        }
    });
    let windows: Vec<Hwnd> = DIVIDERS.with_borrow(|list| list.iter().map(|d| d.hwnd).collect());
    for hwnd in windows {
        // Painting places the window as well.
        paint(hwnd);
        // SAFETY: our own windows; showing one may call `on_message`,
        // which finds the list free.
        unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
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
    let found = with_divider(hwnd, |d| {
        let was = d.dragging;
        if let Some(now) = dragging {
            d.dragging = now;
        }
        (d.key.clone(), was)
    });
    // The grip is longer and in brass while dragged.
    if let (Some((_, was)), Some(now)) = (&found, dragging)
        && *was != now
    {
        paint(hwnd);
    }
    found
}

/// Read or change the divider `hwnd`. `None` for a window not on the list,
/// or while `set_dividers` works on the list.
fn with_divider<R>(hwnd: Hwnd, work: impl FnOnce(&mut Divider) -> R) -> Option<R> {
    DIVIDERS.with(|list| {
        let mut list = list.try_borrow_mut().ok()?;
        list.iter_mut().find(|d| d.hwnd == hwnd).map(work)
    })
}

/// The pointer came onto the divider `hwnd` or left it: its grip shows or
/// goes, and Windows is asked to say when the pointer leaves.
fn hover(hwnd: Hwnd, on: bool) {
    if with_divider(hwnd, |d| std::mem::replace(&mut d.hovering, on)) != Some(!on) {
        return;
    }
    if on {
        let mut track = TrackMouse {
            size: 24,
            flags: TME_LEAVE,
            hwnd,
            hover_time: 0,
        };
        // SAFETY: a `TRACKMOUSEEVENT` of its stated size for our own window.
        unsafe { TrackMouseEvent(&raw mut track) };
    }
    paint(hwnd);
}

fn report(key: DividerKey, action: DividerAction) {
    let engine = Engine::global();
    match key {
        DividerKey::Split { path, index, .. } => engine.emit(EngineEvent::SplitDivider {
            path,
            index,
            action,
        }),
        DividerKey::DevTools { id, .. } => engine.devtools_divider(id, action),
        // Device handles move with both coordinates (`WM_MOUSEMOVE`); here
        // only letting go counts.
        DividerKey::Device { id, edge } => engine.device_handle(id, edge, 0, 0, action),
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
        WM_SETCURSOR => {
            if let Some((key, _)) = divider(hwnd, None) {
                // SAFETY: one of the system's own cursors, named by number.
                unsafe {
                    SetCursor(LoadCursorW(
                        std::ptr::null_mut(),
                        key.cursor() as *const u16,
                    ))
                };
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
        WM_MOUSELEAVE => {
            hover(hwnd, false);
            return 0;
        }
        WM_MOUSEMOVE => {
            hover(hwnd, true);
            // SAFETY: plain queries; `at` is a POINT.
            let mut at = [0i32; 2];
            let ours = unsafe { GetCapture() == hwnd && GetCursorPos(&raw mut at) != 0 };
            if ours && let Some((key, true)) = divider(hwnd, None) {
                // The app counts in points, Windows in pixels.
                let at =
                    display_convert_screen_point_from_pixels(Some(&Point { x: at[0], y: at[1] }));
                // A resize handle in device mode takes both; a divider its axis.
                if let DividerKey::Device { id, edge } = key {
                    let action = DividerAction::Move(at.x);
                    Engine::global().device_handle(id, edge, at.x, at.y, action);
                    return 0;
                }
                let along = if key.row() { at.x } else { at.y };
                report(key, DividerAction::Move(along));
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
