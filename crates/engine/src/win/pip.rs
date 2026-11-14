//! Our controls on the floating video window (picture-in-picture), Windows:
//! what `pip.rs` does on macOS.
//!
//! Chromium shows a video in picture-in-picture in its own floating window,
//! with controls we cannot change. While a page's video is in that window, a
//! nearly clear window of ours lies exactly over it and takes the pointer,
//! so Chromium's controls never appear. While the pointer is over the video
//! it shows Back to Tab, Minimize and Close on top and 15 s back, play/pause
//! and 15 s forward in the middle. Dragging the video moves the window;
//! dragging an edge or corner resizes it with the video's proportions.
//!
//! Media reports drive it (`follow`): a report with `pip` puts the panel on
//! the video window and keeps the play button current; the end of
//! picture-in-picture or the window closing takes it off.
//!
//! Unlike on macOS the buttons are drawn here, pixel by pixel, into a picture
//! with its own transparency: the top ones show their signs only, without
//! labels. And the panel does not move with the video by itself: a timer
//! keeps it in place and notices the video window closing.

// Pixels and points are converted all over this file, within screen sizes.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::time::{Duration, Instant};

use cef::ImplWindow;

use crate::engine::Engine;
use crate::events::{ContentId, EngineEvent};
use crate::media::{MediaAction, MediaState};

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

/// `TRACKMOUSEEVENT`.
#[repr(C)]
struct TrackMouse {
    size: u32,
    flags: u32,
    hwnd: Hwnd,
    hover_time: u32,
}

type EnumProc = unsafe extern "system" fn(hwnd: Hwnd, param: isize) -> i32;

/// As `edge.rs` declares it; the panel's timers send `WM_TIMER` instead.
type TimerProc = unsafe extern "system" fn(hwnd: Hwnd, message: u32, id: usize, time: u32);

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
    fn IsWindow(hwnd: Hwnd) -> i32;
    fn IsWindowVisible(hwnd: Hwnd) -> i32;
    fn IsIconic(hwnd: Hwnd) -> i32;
    fn GetWindow(hwnd: Hwnd, command: u32) -> Hwnd;
    fn GetWindowRect(hwnd: Hwnd, rect: *mut [i32; 4]) -> i32;
    fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> isize;
    fn GetWindowThreadProcessId(hwnd: Hwnd, process: *mut u32) -> u32;
    fn GetClassNameW(hwnd: Hwnd, name: *mut u16, max: i32) -> i32;
    fn GetDpiForWindow(hwnd: Hwnd) -> u32;
    fn GetSystemMetrics(index: i32) -> i32;
    fn EnumWindows(callback: EnumProc, param: isize) -> i32;
    fn SetWindowPos(hwnd: Hwnd, after: Hwnd, x: i32, y: i32, w: i32, h: i32, flags: u32) -> i32;
    fn SetTimer(hwnd: Hwnd, id: usize, milliseconds: u32, callback: Option<TimerProc>) -> usize;
    fn SetCapture(hwnd: Hwnd) -> Hwnd;
    fn ReleaseCapture() -> i32;
    fn GetCursorPos(point: *mut [i32; 2]) -> i32;
    fn LoadCursorW(instance: *mut c_void, name: *const u16) -> *mut c_void;
    fn SetCursor(cursor: *mut c_void) -> *mut c_void;
    fn TrackMouseEvent(track: *mut TrackMouse) -> i32;
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
    fn GetCurrentProcessId() -> u32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
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

const GW_OWNER: u32 = 4;
const GWL_EXSTYLE: i32 = -20;
const WS_EX_TOPMOST: isize = 0x8;
const WS_POPUP: u32 = 0x8000_0000;
/// See-through by its picture, above other windows, never active, not in
/// the taskbar.
const EX_STYLE: u32 = 0x0008_0000 | 0x8 | 0x0800_0000 | 0x80;
const ULW_ALPHA: u32 = 2;
const AC_SRC_ALPHA: u8 = 1;
const SWP_NOSIZE: u32 = 0x1;
const SWP_NOMOVE: u32 = 0x2;
const SWP_NOZORDER: u32 = 0x4;
const SWP_NOACTIVATE: u32 = 0x10;
const SWP_SHOWWINDOW: u32 = 0x40;
const WM_SETCURSOR: u32 = 0x20;
const WM_MOUSEACTIVATE: u32 = 0x21;
const WM_TIMER: u32 = 0x113;
const WM_MOUSEMOVE: u32 = 0x200;
const WM_LBUTTONDOWN: u32 = 0x201;
const WM_LBUTTONUP: u32 = 0x202;
const WM_CAPTURECHANGED: u32 = 0x215;
const WM_MOUSELEAVE: u32 = 0x2A3;
const MA_NOACTIVATE: isize = 3;
const TME_LEAVE: u32 = 2;
const SM_CXSCREEN: i32 = 0;
/// The system's cursors: arrow, hand, and the arrows for resizing.
const IDC_ARROW: usize = 32512;
const IDC_SIZENWSE: usize = 32642;
const IDC_SIZENESW: usize = 32643;
const IDC_SIZEWE: usize = 32644;
const IDC_SIZENS: usize = 32645;
const IDC_HAND: usize = 32649;

/// Sizes in points at 100 % scaling.
const MARGIN: f32 = 10.0;
const PILL: f32 = 26.0;
const PILL_GAP: f32 = 6.0;
const PILL_RADIUS: f32 = 8.0;
const PLAY: f32 = 52.0;
const SEEK: f32 = 40.0;
const CENTER_GAP: f32 = 18.0;
/// Band along each edge, and square at each corner, that resizes the window.
const EDGE: f32 = 6.0;
const CORNER: f32 = 16.0;
/// The smallest video, in points.
const MIN_WIDTH: f32 = 160.0;
/// The controls hide when the pointer rests this long.
const IDLE: Duration = Duration::from_millis(2500);
/// How often the panel looks at the video window.
const TICK_MS: u32 = 100;
const TIMER: usize = 1;

/// The controls, top row first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    BackToTab,
    Minimize,
    Close,
    Back,
    PlayPause,
    Forward,
}

impl Button {
    /// The buttons of the top row.
    fn on_top(self) -> bool {
        matches!(self, Self::BackToTab | Self::Minimize | Self::Close)
    }
}

/// A rectangle in pixels, the origin at the top left.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Area {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Area {
    fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// Where the buttons go on a `w` × `h` video (pixels) at `scale` pixels a
/// point. Live video has nothing to seek in, so it gets no seek buttons.
fn layout(w: f32, h: f32, scale: f32, live: bool) -> Vec<(Button, Area)> {
    let pill = |x: f32| Area {
        x,
        y: MARGIN * scale,
        w: PILL * scale,
        h: PILL * scale,
    };
    let close_x = w - (MARGIN + PILL) * scale;
    let round = |cx: f32, size: f32| Area {
        x: cx - size * scale / 2.0,
        y: h / 2.0 - size * scale / 2.0,
        w: size * scale,
        h: size * scale,
    };
    let mut places = vec![
        (Button::BackToTab, pill(MARGIN * scale)),
        (Button::Minimize, pill(close_x - (PILL_GAP + PILL) * scale)),
        (Button::Close, pill(close_x)),
        (Button::PlayPause, round(w / 2.0, PLAY)),
    ];
    if !live {
        let offset = (PLAY / 2.0 + CENTER_GAP + SEEK / 2.0) * scale;
        places.push((Button::Back, round(w / 2.0 - offset, SEEK)));
        places.push((Button::Forward, round(w / 2.0 + offset, SEEK)));
    }
    places
}

/// Edges a press grabs to resize the window.
#[allow(clippy::struct_excessive_bools)] // one flag per edge
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Edges {
    left: bool,
    right: bool,
    top: bool,
    bottom: bool,
}

/// The edges a press at (`x`, `y`) on a `w` × `h` video grabs: a band along
/// each edge, two edges near a corner; `None` inside.
fn edges_at(w: f32, h: f32, scale: f32, x: f32, y: f32) -> Option<Edges> {
    let (edge, corner) = (EDGE * scale, CORNER * scale);
    let corner_x = x < corner || x > w - corner;
    let corner_y = y < corner || y > h - corner;
    let edges = Edges {
        left: x < edge || (corner_y && x < corner),
        right: x > w - edge || (corner_y && x > w - corner),
        top: y < edge || (corner_x && y < corner),
        bottom: y > h - edge || (corner_x && y > h - corner),
    };
    (edges != Edges::default()).then_some(edges)
}

/// The window's place (screen pixels, y down) after dragging `edges` of
/// `start` by `dx`, `dy`. The video keeps its proportions and the opposite
/// edges stay put; the width stays within `min_w..=max_w`.
fn resized(start: Area, edges: Edges, dx: f32, dy: f32, min_w: f32, max_w: f32) -> Area {
    let ratio = start.w / start.h;
    let from_x = start.w + if edges.right { dx } else { -dx };
    let from_y = (start.h + if edges.bottom { dy } else { -dy }) * ratio;
    let w = match (edges.left || edges.right, edges.top || edges.bottom) {
        (true, true) => from_x.max(from_y),
        (true, false) => from_x,
        (false, true) => from_y,
        (false, false) => start.w,
    }
    .clamp(min_w, max_w.max(min_w));
    let h = w / ratio;
    Area {
        x: if edges.left {
            start.x + start.w - w
        } else {
            start.x
        },
        y: if edges.top {
            start.y + start.h - h
        } else {
            start.y
        },
        w,
        h,
    }
}

/// A picture of `width` × `height` pixels, BGRA with the colours already
/// multiplied by the alpha, that shapes are painted onto.
struct Canvas<'a> {
    pixels: &'a mut [u8],
    width: i32,
    height: i32,
}

impl Canvas<'_> {
    /// Paint grey `level` (0 black to 1 white) at `alpha` over the pixel,
    /// as far as `coverage` (0 to 1) says the shape covers it.
    fn blend(&mut self, x: i32, y: i32, level: f32, alpha: f32, coverage: f32) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height || coverage <= 0.0 {
            return;
        }
        let a = (alpha * coverage).clamp(0.0, 1.0);
        let at = ((y * self.width + x) * 4) as usize;
        let Some(pixel) = self.pixels.get_mut(at..at + 4) else {
            return;
        };
        for channel in &mut pixel[..3] {
            *channel = (level * a * 255.0 + f32::from(*channel) * (1.0 - a)).round() as u8;
        }
        pixel[3] = (a * 255.0 + f32::from(pixel[3]) * (1.0 - a)).round() as u8;
    }

    /// Paint every pixel of `area` (grown by a pixel for soft edges) that
    /// `distance` says is inside the shape: `distance` is how far the pixel's
    /// middle is outside the shape's edge, negative inside.
    fn fill(&mut self, area: Area, level: f32, alpha: f32, distance: impl Fn(f32, f32) -> f32) {
        let (left, top) = (area.x.floor() as i32 - 1, area.y.floor() as i32 - 1);
        let (right, bottom) = (
            (area.x + area.w).ceil() as i32 + 1,
            (area.y + area.h).ceil() as i32 + 1,
        );
        for y in top..bottom {
            for x in left..right {
                let d = distance(x as f32 + 0.5, y as f32 + 0.5);
                self.blend(x, y, level, alpha, (0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// A rectangle with corners of `radius`.
    fn round_rect(&mut self, area: Area, radius: f32, level: f32, alpha: f32) {
        let half = (area.w / 2.0, area.h / 2.0);
        let centre = (area.x + half.0, area.y + half.1);
        let radius = radius.min(half.0).min(half.1);
        self.fill(area, level, alpha, |x, y| {
            let qx = (x - centre.0).abs() - (half.0 - radius);
            let qy = (y - centre.1).abs() - (half.1 - radius);
            qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
        });
    }

    /// A line from `from` to `to`, `width` wide, with round ends.
    fn line(&mut self, from: (f32, f32), to: (f32, f32), width: f32, level: f32, alpha: f32) {
        let area = Area {
            x: from.0.min(to.0) - width,
            y: from.1.min(to.1) - width,
            w: (from.0 - to.0).abs() + 2.0 * width,
            h: (from.1 - to.1).abs() + 2.0 * width,
        };
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let length = (dx * dx + dy * dy).max(f32::EPSILON);
        self.fill(area, level, alpha, |x, y| {
            let along = (((x - from.0) * dx + (y - from.1) * dy) / length).clamp(0.0, 1.0);
            (x - from.0 - along * dx).hypot(y - from.1 - along * dy) - width / 2.0
        });
    }

    /// A triangle with the corners `a`, `b`, `c`, given clockwise.
    fn triangle(&mut self, a: (f32, f32), b: (f32, f32), c: (f32, f32), level: f32, alpha: f32) {
        let area = Area {
            x: a.0.min(b.0).min(c.0),
            y: a.1.min(b.1).min(c.1),
            w: a.0.max(b.0).max(c.0) - a.0.min(b.0).min(c.0),
            h: a.1.max(b.1).max(c.1) - a.1.min(b.1).min(c.1),
        };
        // How far a point is outside the edge from `p` to `q`.
        let outside = |p: (f32, f32), q: (f32, f32), x: f32, y: f32| {
            let (dx, dy) = (q.0 - p.0, q.1 - p.1);
            ((y - p.1) * dx - (x - p.0) * dy) / dx.hypot(dy).max(f32::EPSILON)
        };
        self.fill(area, level, alpha, |x, y| {
            -outside(a, b, x, y)
                .min(outside(b, c, x, y))
                .min(outside(c, a, x, y))
        });
    }
}

/// Paint the panel of a `width` × `height` video: all of it just not clear
/// (a fully clear pixel lets the pointer through to Chromium's controls),
/// and, if `shown`, the dimmed video and the buttons at `places`.
fn paint(
    canvas: &mut Canvas,
    scale: f32,
    places: &[(Button, Area)],
    shown: bool,
    hover: Option<Button>,
    playing: bool,
) {
    let all = Area {
        x: 0.0,
        y: 0.0,
        w: canvas.width as f32,
        h: canvas.height as f32,
    };
    for pixel in canvas.pixels.as_chunks_mut::<4>().0 {
        *pixel = [0, 0, 0, 1];
    }
    if !shown {
        return;
    }
    canvas.round_rect(all, 0.0, 0.0, 0.2);
    for &(button, area) in places {
        let hot = hover == Some(button);
        // The fills of the macOS controls.
        let (level, alpha) = match (button.on_top(), hot) {
            (true, false) => (0.08, 0.62),
            (true, true) => (0.24, 0.8),
            (false, false) => (0.08, 0.5),
            (false, true) => (0.24, 0.72),
        };
        let radius = if button.on_top() {
            PILL_RADIUS * scale
        } else {
            area.w / 2.0
        };
        canvas.round_rect(area, radius, level, alpha);
        sign(canvas, button, area, scale, playing);
    }
}

/// Draw `button`'s sign in white in the middle of `area`.
fn sign(canvas: &mut Canvas, button: Button, area: Area, scale: f32, playing: bool) {
    const WHITE: (f32, f32) = (1.0, 0.92);
    let (cx, cy) = (area.x + area.w / 2.0, area.y + area.h / 2.0);
    // Half the sign's size, and its lines' width.
    let r = area.w * if button.on_top() { 0.2 } else { 0.19 };
    let width = 1.6 * scale;
    let mut line = |from: (f32, f32), to: (f32, f32)| {
        canvas.line(
            (cx + from.0 * r, cy + from.1 * r),
            (cx + to.0 * r, cy + to.1 * r),
            width,
            WHITE.0,
            WHITE.1,
        );
    };
    match button {
        // An arrow up and to the left: back into the page.
        Button::BackToTab => {
            line((1.0, 1.0), (-1.0, -1.0));
            line((-1.0, -1.0), (0.3, -1.0));
            line((-1.0, -1.0), (-1.0, 0.3));
        }
        Button::Minimize => line((-1.0, 0.0), (1.0, 0.0)),
        Button::Close => {
            line((-1.0, -1.0), (1.0, 1.0));
            line((-1.0, 1.0), (1.0, -1.0));
        }
        // Two arrowheads: 15 seconds back or forward.
        Button::Back | Button::Forward => {
            let way = if button == Button::Back { -1.0 } else { 1.0 };
            for start in [-0.9, 0.1] {
                line((way * start, -0.9), (way * (start + 0.8), 0.0));
                line((way * (start + 0.8), 0.0), (way * start, 0.9));
            }
        }
        Button::PlayPause if playing => {
            for x in [-0.55, 0.55] {
                canvas.round_rect(
                    Area {
                        x: cx + (x - 0.25) * r,
                        y: cy - r,
                        w: 0.5 * r,
                        h: 2.0 * r,
                    },
                    scale,
                    WHITE.0,
                    WHITE.1,
                );
            }
        }
        Button::PlayPause => canvas.triangle(
            (cx - 0.7 * r, cy - r),
            (cx + 1.0 * r, cy),
            (cx - 0.7 * r, cy + r),
            WHITE.0,
            WHITE.1,
        ),
    }
}

/// What the panel shows: where it is and how large (x, y, width, height
/// in screen pixels), and the state of its controls.
#[derive(Clone, Copy, PartialEq)]
struct Drawn {
    place: [i32; 4],
    shown: bool,
    hover: Option<Button>,
    playing: bool,
    live: bool,
}

/// The panel on a video in picture-in-picture.
struct Panel {
    hwnd: Hwnd,
    owner: ContentId,
    video: Hwnd,
    playing: bool,
    live: bool,
    /// The controls are showing (the pointer is over the video).
    shown: bool,
    hover: Option<Button>,
    /// When the pointer last moved: the controls hide `IDLE` later.
    moved: Instant,
    /// The pointer is over the panel (`WM_MOUSELEAVE` is asked for).
    inside: bool,
    /// What the panel was last drawn as.
    drawn: Option<Drawn>,
}

impl Panel {
    fn scale(&self) -> f32 {
        // SAFETY: a plain query on our own window.
        unsafe { GetDpiForWindow(self.hwnd) }.max(96) as f32 / 96.0
    }
}

/// What a press on the panel turned into.
#[derive(Clone, Copy)]
enum Gesture {
    Press(Button),
    Move {
        pointer: [i32; 2],
        origin: [i32; 2],
    },
    Resize {
        pointer: [i32; 2],
        start: Area,
        edges: Edges,
    },
}

thread_local! {
    static PANEL: RefCell<Option<Panel>> = const { RefCell::new(None) };
    /// The latest report that wants the controls: page, playing, live.
    static WANTED: Cell<Option<(ContentId, bool, bool)>> = const { Cell::new(None) };
    /// Until when the video window is looked for: it can lag the report.
    static SEARCH: Cell<Option<Instant>> = const { Cell::new(None) };
    /// A page whose picture-in-picture we just ended, and when: its late
    /// reports that still say `pip` must not bring the panel back.
    static ENDING: Cell<Option<(ContentId, Instant)>> = const { Cell::new(None) };
    static GESTURE: Cell<Option<Gesture>> = const { Cell::new(None) };
    /// The page we last asked to enter picture-in-picture, and when.
    static REQUESTED: Cell<Option<(ContentId, Instant)>> = const { Cell::new(None) };
    /// A window that only runs the timer while the video window is looked for.
    static SEEKER: Cell<isize> = const { Cell::new(0) };
    static CLASS: Cell<bool> = const { Cell::new(false) };
}

/// We asked page `id` to enter picture-in-picture (`content_picture_in_picture`).
pub(crate) fn requested(id: ContentId) {
    REQUESTED.set(Some((id, Instant::now())));
}

fn asked(id: ContentId) -> bool {
    REQUESTED
        .get()
        .is_some_and(|(page, at)| page == id && at.elapsed() < Duration::from_secs(5))
}

fn owner() -> Option<ContentId> {
    PANEL.with(|cell| cell.try_borrow().ok()?.as_ref().map(|p| p.owner))
}

/// Whether page `id` may own the floating video window. Its own report
/// cannot be trusted alone (pages can fake it), so: the page already owns
/// it, we asked it into picture-in-picture moments ago, or it is on screen
/// (a page starts picture-in-picture only from a click in it).
fn may_own(id: ContentId) -> bool {
    owner() == Some(id) || asked(id) || Engine::global().content_shown(id)
}

/// Follow a media report from page `id`: put the controls on its video
/// window, update them, or take them off (when it reports media that is no
/// longer in picture-in-picture).
pub(crate) fn follow(id: ContentId, media: Option<&MediaState>) {
    let pip = media.filter(|m| m.pip && may_own(id));
    if let Some((page, at)) = ENDING.get() {
        if page == id && pip.is_some() && at.elapsed() < Duration::from_secs(2) {
            return;
        }
        if page == id {
            ENDING.set(None);
        }
    }
    if let Some(m) = pip {
        WANTED.set(Some((id, m.playing, m.live)));
        SEARCH.set(Some(Instant::now() + Duration::from_secs(2)));
        attach();
        return;
    }
    // A report without media (the video ended, is muted or loads a new
    // source) is not the end of picture-in-picture: the video window closing
    // is (`tick`). The panel stays, so the page's next report is accepted.
    if media.is_none() {
        return;
    }
    if WANTED.get().is_some_and(|w| w.0 == id) {
        WANTED.set(None);
    }
    if owner() == Some(id) {
        detach();
    }
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
/// visible, above other programs' windows, nobody's, and not one of ours.
fn is_video_window(hwnd: Hwnd) -> bool {
    let mut process = 0u32;
    let mut class = [0u16; 32];
    // SAFETY: plain queries on a window that may be gone (then they fail).
    let chromium = unsafe {
        GetWindowThreadProcessId(hwnd, &raw mut process);
        let length = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
        process == GetCurrentProcessId()
            && String::from_utf16_lossy(&class[..length.max(0) as usize]) == "Chrome_WidgetWin_1"
            && IsWindowVisible(hwnd) != 0
            && IsIconic(hwnd) == 0
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

/// A window of our class owned by `owner`: the panel (`layered`) or the
/// window that only runs the search timer.
fn window(owner: Hwnd, layered: bool) -> Hwnd {
    let name: Vec<u16> = "MagnetitePip\0".encode_utf16().collect();
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
                background: std::ptr::null_mut(),
                menu_name: std::ptr::null(),
                class_name: name.as_ptr(),
            };
            RegisterClassW(&raw const class);
        }
        CreateWindowExW(
            if layered { EX_STYLE } else { 0 },
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

/// Bring the panel in line with `WANTED`: update the buttons when it is on
/// that page's video already, else find the video window and lie over it
/// (the timer tries again while it is not there yet).
fn attach() {
    let Some((owner, playing, live)) = WANTED.get() else {
        return;
    };
    let same = PANEL.with(|cell| {
        let Ok(mut cell) = cell.try_borrow_mut() else {
            return false;
        };
        let Some(p) = cell.as_mut().filter(|p| p.owner == owner) else {
            return false;
        };
        p.playing = playing;
        p.live = live;
        draw(p);
        true
    });
    if same {
        WANTED.set(None);
        SEARCH.set(None);
        return;
    }
    // A window another page's video still holds is not up for grabs on a
    // report alone (pages can fake those): only when we asked this page into
    // picture-in-picture. When Chromium really switches videos, it makes a
    // new window, which the next try picks up.
    let held = PANEL.with(|cell| cell.try_borrow().ok()?.as_ref().map(|p| p.video));
    let video = find_video_window().filter(|v| asked(owner) || held != Some(*v));
    let Some(video) = video else {
        // The timer of a window of its own keeps looking for a while.
        if SEEKER.get() == 0 {
            let seeker = window(std::ptr::null_mut(), false);
            SEEKER.set(seeker as isize);
            // SAFETY: our own window; its timer messages go to `on_message`.
            unsafe { SetTimer(seeker, TIMER, TICK_MS, None) };
        }
        return;
    };
    detach();
    WANTED.set(None);
    SEARCH.set(None);
    let hwnd = window(video, true);
    if hwnd.is_null() {
        return;
    }
    let mut panel = Panel {
        hwnd,
        owner,
        video,
        playing,
        live,
        // Shown briefly when the video pops out, so they can be found.
        shown: true,
        hover: None,
        moved: Instant::now(),
        inside: false,
        drawn: None,
    };
    draw(&mut panel);
    // SAFETY: our own window; its timer messages go to `on_message`.
    unsafe { SetTimer(hwnd, TIMER, TICK_MS, None) };
    PANEL.set(Some(panel));
}

/// Take the panel off the video window.
fn detach() {
    let panel = PANEL.with(|cell| cell.try_borrow_mut().ok().and_then(|mut cell| cell.take()));
    GESTURE.set(None);
    if let Some(panel) = panel {
        // SAFETY: a window we made; the panel is off the list, so the
        // messages this sends find nothing to do.
        unsafe { DestroyWindow(panel.hwnd) };
    }
}

/// Put the panel exactly over the video window and paint it, if where it
/// is, how large it is or what it shows changed.
fn draw(p: &mut Panel) {
    let mut rect = [0i32; 4];
    // SAFETY: a plain query on a window that may be gone (then it fails).
    if unsafe { GetWindowRect(p.video, &raw mut rect) } == 0 {
        return;
    }
    let (width, height) = (rect[2] - rect[0], rect[3] - rect[1]);
    let state = Drawn {
        place: [rect[0], rect[1], width, height],
        shown: p.shown,
        hover: p.hover,
        playing: p.playing,
        live: p.live,
    };
    if p.drawn == Some(state) || width <= 0 || height <= 0 {
        return;
    }
    // Only moved: the picture is as it was.
    if let Some(before) = p.drawn
        && before.place[2..] == state.place[2..]
        && (Drawn {
            place: state.place,
            ..before
        }) == state
    {
        // SAFETY: our own window.
        unsafe {
            SetWindowPos(
                p.hwnd,
                std::ptr::null_mut(),
                rect[0],
                rect[1],
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        p.drawn = Some(state);
        return;
    }
    let scale = p.scale();
    let places = layout(width as f32, height as f32, scale, p.live);
    let header = BitmapInfoHeader {
        size: 40,
        width,
        // Negative: the first row is the top one.
        height: -height,
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
    // `width * height` pixels of four bytes for as long as the bitmap lives.
    let ok = unsafe {
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
            return;
        }
        let mut canvas = Canvas {
            pixels: std::slice::from_raw_parts_mut(bits, (width * height * 4) as usize),
            width,
            height,
        };
        paint(&mut canvas, scale, &places, p.shown, p.hover, p.playing);
        let position = Point {
            x: rect[0],
            y: rect[1],
        };
        let extent = Extent { width, height };
        let origin = Point { x: 0, y: 0 };
        let blend = BlendFunction {
            op: 0,
            flags: 0,
            alpha: 255,
            format: AC_SRC_ALPHA,
        };
        let old = SelectObject(dc, bitmap);
        let ok = UpdateLayeredWindow(
            p.hwnd,
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
        SetWindowPos(
            p.hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        ok != 0
    };
    if ok {
        p.drawn = Some(state);
    }
}

/// The timer: follow the video window, notice it closing, hide the controls
/// when the pointer rests, and keep looking for a video window that is wanted.
fn tick(hwnd: Hwnd) {
    if hwnd as isize == SEEKER.get() {
        let waiting =
            WANTED.get().is_some() && SEARCH.get().is_some_and(|until| Instant::now() < until);
        if waiting {
            attach();
        }
        if !waiting || WANTED.get().is_none() {
            SEEKER.set(0);
            // SAFETY: a window we made; its timer goes with it.
            unsafe { DestroyWindow(hwnd) };
        }
        return;
    }
    let gone = PANEL.with(|cell| {
        let Ok(mut cell) = cell.try_borrow_mut() else {
            return false;
        };
        let Some(p) = cell.as_mut().filter(|p| p.hwnd == hwnd) else {
            return false;
        };
        // SAFETY: plain queries on a window that may be gone.
        if unsafe { IsWindow(p.video) == 0 || IsWindowVisible(p.video) == 0 } {
            return true;
        }
        let resting = GESTURE.get().is_none() && p.hover.is_none();
        if p.shown && resting && p.moved.elapsed() > IDLE {
            p.shown = false;
        }
        draw(p);
        false
    });
    if gone {
        detach();
    }
}

fn button_at(p: &Panel, x: f32, y: f32) -> Option<Button> {
    if !p.shown {
        return None;
    }
    let size = p.drawn?.place;
    layout(size[2] as f32, size[3] as f32, p.scale(), p.live)
        .into_iter()
        .find(|(_, a)| a.contains(x, y))
        .map(|(b, _)| b)
}

/// The pointer's place in the message, in the panel's pixels.
fn point(lparam: isize) -> (f32, f32) {
    let x = (lparam & 0xFFFF) as u16 as i16;
    let y = ((lparam >> 16) & 0xFFFF) as u16 as i16;
    (f32::from(x), f32::from(y))
}

fn screen_pointer() -> [i32; 2] {
    let mut at = [0i32; 2];
    // SAFETY: `at` is the two `i32` of a POINT.
    unsafe { GetCursorPos(&raw mut at) };
    at
}

fn pointer_moved(hwnd: Hwnd, (x, y): (f32, f32)) {
    if let Some(gesture) = GESTURE.get() {
        dragged(gesture);
        return;
    }
    PANEL.with(|cell| {
        let Ok(mut cell) = cell.try_borrow_mut() else {
            return;
        };
        let Some(p) = cell.as_mut().filter(|p| p.hwnd == hwnd) else {
            return;
        };
        if !p.inside {
            p.inside = true;
            let mut track = TrackMouse {
                size: size_of::<TrackMouse>() as u32,
                flags: TME_LEAVE,
                hwnd,
                hover_time: 0,
            };
            // SAFETY: our own window: `WM_MOUSELEAVE` when the pointer goes.
            unsafe { TrackMouseEvent(&raw mut track) };
        }
        p.shown = true;
        p.moved = Instant::now();
        p.hover = button_at(p, x, y);
        draw(p);
    });
}

fn pointer_left(hwnd: Hwnd) {
    if GESTURE.get().is_some() {
        return;
    }
    PANEL.with(|cell| {
        if let Ok(mut cell) = cell.try_borrow_mut()
            && let Some(p) = cell.as_mut().filter(|p| p.hwnd == hwnd)
        {
            p.inside = false;
            p.hover = None;
            p.shown = false;
            draw(p);
        }
    });
}

/// The cursor for where the pointer is: a hand on a button, arrows on an
/// edge or corner, else the arrow.
fn set_cursor(hwnd: Hwnd) {
    let name = PANEL.with(|cell| {
        let cell = cell.try_borrow().ok()?;
        let p = cell.as_ref().filter(|p| p.hwnd == hwnd)?;
        let size = p.drawn?.place;
        let at = screen_pointer();
        let (x, y) = ((at[0] - size[0]) as f32, (at[1] - size[1]) as f32);
        if button_at(p, x, y).is_some() {
            return Some(IDC_HAND);
        }
        let e = edges_at(size[2] as f32, size[3] as f32, p.scale(), x, y)?;
        Some(match (e.top, e.bottom, e.left, e.right) {
            (true, _, true, _) | (_, true, _, true) => IDC_SIZENWSE,
            (true, _, _, true) | (_, true, true, _) => IDC_SIZENESW,
            (true, ..) | (_, true, ..) => IDC_SIZENS,
            _ => IDC_SIZEWE,
        })
    });
    // SAFETY: one of the system's own cursors, named by number.
    unsafe {
        SetCursor(LoadCursorW(
            std::ptr::null_mut(),
            name.unwrap_or(IDC_ARROW) as *const u16,
        ))
    };
}

/// A press on a shown button presses it; elsewhere it starts moving or
/// resizing the video, and brings hidden controls back.
fn pressed(hwnd: Hwnd, (x, y): (f32, f32)) {
    let gesture = PANEL.with(|cell| {
        let mut cell = cell.try_borrow_mut().ok()?;
        let p = cell.as_mut().filter(|p| p.hwnd == hwnd)?;
        if let Some(button) = button_at(p, x, y) {
            return Some(Gesture::Press(button));
        }
        p.shown = true;
        p.moved = Instant::now();
        draw(p);
        let size = p.drawn?.place;
        let pointer = screen_pointer();
        Some(
            match edges_at(size[2] as f32, size[3] as f32, p.scale(), x, y) {
                Some(edges) => Gesture::Resize {
                    pointer,
                    start: Area {
                        x: size[0] as f32,
                        y: size[1] as f32,
                        w: size[2] as f32,
                        h: size[3] as f32,
                    },
                    edges,
                },
                None => Gesture::Move {
                    pointer,
                    origin: [size[0], size[1]],
                },
            },
        )
    });
    GESTURE.set(gesture);
    if gesture.is_some() {
        // SAFETY: our own window: the mouse stays with it until the button
        // is let go, wherever the pointer goes.
        unsafe { SetCapture(hwnd) };
    }
}

/// The pointer moved with the button down: the video moves or resizes, and
/// the panel with it.
fn dragged(gesture: Gesture) {
    let Some((video, scale)) = PANEL.with(|cell| {
        let cell = cell.try_borrow().ok()?;
        cell.as_ref().map(|p| (p.video, p.scale()))
    }) else {
        return;
    };
    let now = screen_pointer();
    // Outside the panel's borrow: moving windows sends messages.
    // SAFETY: Chromium's own window of this process, moved as the user drags.
    unsafe {
        match gesture {
            Gesture::Press(_) => return,
            Gesture::Move { pointer, origin } => {
                SetWindowPos(
                    video,
                    std::ptr::null_mut(),
                    origin[0] + now[0] - pointer[0],
                    origin[1] + now[1] - pointer[1],
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            Gesture::Resize {
                pointer,
                start,
                edges,
            } => {
                let to = resized(
                    start,
                    edges,
                    (now[0] - pointer[0]) as f32,
                    (now[1] - pointer[1]) as f32,
                    MIN_WIDTH * scale,
                    GetSystemMetrics(SM_CXSCREEN) as f32,
                );
                SetWindowPos(
                    video,
                    std::ptr::null_mut(),
                    to.x.round() as i32,
                    to.y.round() as i32,
                    to.w.round() as i32,
                    to.h.round() as i32,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }
    PANEL.with(|cell| {
        if let Ok(mut cell) = cell.try_borrow_mut()
            && let Some(p) = cell.as_mut()
        {
            p.moved = Instant::now();
            draw(p);
        }
    });
}

fn released(hwnd: Hwnd, (x, y): (f32, f32)) {
    let gesture = GESTURE.take();
    // SAFETY: gives the mouse back.
    unsafe { ReleaseCapture() };
    let Some(Gesture::Press(button)) = gesture else {
        return;
    };
    let over = PANEL.with(|cell| {
        let cell = cell.try_borrow().ok()?;
        button_at(cell.as_ref().filter(|p| p.hwnd == hwnd)?, x, y)
    });
    if over == Some(button) {
        press(button);
    }
}

/// Do what `button` says for the page whose video is shown.
fn press(button: Button) {
    let Some((owner, playing)) = PANEL.with(|cell| {
        let cell = cell.try_borrow().ok()?;
        cell.as_ref().map(|p| (p.owner, p.playing))
    }) else {
        return;
    };
    let engine = Engine::global();
    match button {
        Button::BackToTab | Button::Minimize | Button::Close => {
            ENDING.set(Some((owner, Instant::now())));
            detach();
            if button == Button::Close {
                engine.content_close_picture_in_picture(owner);
            } else {
                engine.content_picture_in_picture(owner, false);
            }
            if button == Button::BackToTab {
                engine.emit(EngineEvent::BackToTab { id: owner });
            }
        }
        Button::PlayPause => {
            // Flip the sign now; the page's next report confirms it.
            PANEL.with(|cell| {
                if let Ok(mut cell) = cell.try_borrow_mut()
                    && let Some(p) = cell.as_mut()
                {
                    p.playing = !playing;
                    draw(p);
                }
            });
            let action = if playing {
                MediaAction::Pause
            } else {
                MediaAction::Play
            };
            engine.content_media(owner, action);
        }
        Button::Back => engine.content_media(owner, MediaAction::SeekBackward),
        Button::Forward => engine.content_media(owner, MediaAction::SeekForward),
    }
}

unsafe extern "system" fn on_message(
    hwnd: Hwnd,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    match message {
        // A click on the video does not take the keyboard from the page.
        WM_MOUSEACTIVATE => return MA_NOACTIVATE,
        WM_TIMER => {
            tick(hwnd);
            return 0;
        }
        WM_SETCURSOR => {
            set_cursor(hwnd);
            return 1;
        }
        WM_MOUSEMOVE => {
            pointer_moved(hwnd, point(lparam));
            return 0;
        }
        WM_MOUSELEAVE => {
            pointer_left(hwnd);
            return 0;
        }
        WM_LBUTTONDOWN => {
            pressed(hwnd, point(lparam));
            return 0;
        }
        WM_LBUTTONUP => {
            released(hwnd, point(lparam));
            return 0;
        }
        // Another window took the mouse in the middle of a drag.
        WM_CAPTURECHANGED => {
            if !matches!(GESTURE.get(), Some(Gesture::Press(_))) {
                GESTURE.set(None);
            }
            return 0;
        }
        _ => {}
    }
    // SAFETY: the default handling for everything else.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(places: &[(Button, Area)], b: Button) -> Option<Area> {
        places.iter().find(|(x, _)| *x == b).map(|(_, a)| *a)
    }

    #[test]
    fn the_buttons_sit_in_the_corners_and_the_middle() {
        let places = layout(480.0, 270.0, 1.0, false);
        let back = place(&places, Button::BackToTab).unwrap();
        let minimize = place(&places, Button::Minimize).unwrap();
        let close = place(&places, Button::Close).unwrap();
        assert!((back.x - MARGIN).abs() < 1e-3);
        assert!((close.x + close.w - (480.0 - MARGIN)).abs() < 1e-3);
        assert!(back.x + back.w < minimize.x && minimize.x + minimize.w < close.x);
        let play = place(&places, Button::PlayPause).unwrap();
        assert!(play.contains(240.0, 135.0));
        // Twice the pixels a point: twice the sizes.
        let doubled = layout(960.0, 540.0, 2.0, false);
        assert!((place(&doubled, Button::Close).unwrap().w - 2.0 * close.w).abs() < 1e-3);
    }

    #[test]
    fn live_video_has_no_seek_buttons() {
        let places = layout(480.0, 270.0, 1.0, true);
        assert!(place(&places, Button::Back).is_none());
        assert!(place(&places, Button::Forward).is_none());
        assert!(place(&places, Button::PlayPause).is_some());
    }

    #[test]
    fn edges_and_corners_resize_the_middle_moves() {
        assert_eq!(edges_at(480.0, 270.0, 1.0, 240.0, 135.0), None);
        let left = edges_at(480.0, 270.0, 1.0, 2.0, 135.0).unwrap();
        assert!(left.left && !left.right && !left.top && !left.bottom);
        let corner = edges_at(480.0, 270.0, 1.0, 470.0, 262.0).unwrap();
        assert!(corner.right && corner.bottom && !corner.left && !corner.top);
    }

    #[test]
    fn resizing_keeps_proportions_and_the_opposite_corner() {
        let start = Area {
            x: 100.0,
            y: 100.0,
            w: 480.0,
            h: 270.0,
        };
        // The bottom right corner, dragged out: the top left corner stays.
        let edges = Edges {
            right: true,
            bottom: true,
            ..Edges::default()
        };
        let to = resized(start, edges, 160.0, 0.0, 160.0, 4000.0);
        assert!((to.w - 640.0).abs() < 1e-3 && (to.h - 360.0).abs() < 1e-3);
        assert!((to.x - 100.0).abs() < 1e-3 && (to.y - 100.0).abs() < 1e-3);
        // The top left corner, dragged in (at a corner the larger of the two
        // ways counts): the bottom right corner stays.
        let edges = Edges {
            left: true,
            top: true,
            ..Edges::default()
        };
        let to = resized(start, edges, 160.0, 90.0, 160.0, 4000.0);
        assert!((to.w - 320.0).abs() < 1e-3);
        assert!((to.x + to.w - 580.0).abs() < 1e-3 && (to.y + to.h - 370.0).abs() < 1e-3);
        // Never narrower than the smallest video.
        assert!((resized(start, edges, 4000.0, 4000.0, 160.0, 4000.0).w - 160.0).abs() < 1e-3);
    }

    #[test]
    fn the_panel_is_never_fully_clear_and_shows_its_buttons() {
        let (width, height) = (240, 135);
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let places = layout(width as f32, height as f32, 1.0, false);
        let alpha = |pixels: &[u8], x: i32, y: i32| pixels[((y * width + x) * 4 + 3) as usize];
        let mut canvas = Canvas {
            pixels: &mut pixels,
            width,
            height,
        };
        // Hidden: the pointer still lands on the panel everywhere.
        paint(&mut canvas, 1.0, &places, false, None, true);
        assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 1));
        let mut canvas = Canvas {
            pixels: &mut pixels,
            width,
            height,
        };
        paint(&mut canvas, 1.0, &places, true, None, true);
        assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] >= 1));
        // A button is more solid than the dimmed video beside it.
        let play = place(&places, Button::PlayPause).unwrap();
        let on_button = alpha(
            &pixels,
            (play.x + 4.0) as i32,
            (play.y + play.h / 2.0) as i32,
        );
        assert!(on_button > alpha(&pixels, 120, 120));
    }
}
