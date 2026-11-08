//! Our controls on the floating video window (picture-in-picture), macOS.
//!
//! Chromium shows a video in picture-in-picture in its own floating window,
//! with controls we cannot change. While a page's video is in that window, a
//! nearly clear panel of ours lies exactly over it (a child window, so it
//! moves along) and takes the pointer, so Chromium's controls never appear.
//! While the pointer is over the video the panel shows Back to Tab, Minimize
//! and Close on top and 15 s back, play/pause and 15 s forward in the
//! middle. Dragging the video moves the window; dragging an edge or corner
//! resizes it with the video's proportions.
//!
//! Media reports drive it (`follow`): a report with `pip` puts the panel on
//! the video window and keeps the play button current; the end of
//! picture-in-picture or the window closing takes it off.

use std::cell::{Cell, RefCell};
use std::ptr::{self, NonNull};
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject, Sel};
use objc2::{ClassType, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSAnimationContext, NSApplication, NSBackingStoreType, NSBox,
    NSButton, NSColor, NSCursor, NSCursorFrameResizeDirections, NSCursorFrameResizePosition,
    NSEvent, NSFloatingWindowLevel, NSFont, NSFontWeightMedium, NSFontWeightSemibold, NSImage,
    NSImageSymbolConfiguration, NSPanel, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindow,
    NSWindowCollectionBehavior, NSWindowDidResizeNotification, NSWindowOrderingMode,
    NSWindowStyleMask, NSWindowWillCloseNotification,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSPoint, NSRect, NSSize, NSString};

use crate::engine::Engine;
use crate::events::{ContentId, EngineEvent};
use crate::mac::plain_box;
use crate::media::{MediaAction, MediaState};

/// Sizes in points.
const MARGIN: f64 = 10.0;
const PILL_HEIGHT: f64 = 26.0;
const PILL_PADDING: f64 = 10.0;
const PILL_GAP: f64 = 6.0;
const PILL_RADIUS: f64 = 8.0;
const PLAY: f64 = 52.0;
const SEEK: f64 = 40.0;
const CENTER_GAP: f64 = 18.0;
/// Band along each edge, and square at each corner, that resizes the window.
const EDGE: f64 = 6.0;
const CORNER: f64 = 16.0;
/// The controls hide when the pointer rests this long (seconds).
const IDLE: f64 = 2.5;

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
    const ALL: [Self; 6] = [
        Self::BackToTab,
        Self::Minimize,
        Self::Close,
        Self::Back,
        Self::PlayPause,
        Self::Forward,
    ];

    /// The labelled buttons of the top row.
    fn on_top(self) -> bool {
        matches!(self, Self::BackToTab | Self::Minimize | Self::Close)
    }

    fn label(self, playing: bool) -> &'static str {
        match self {
            Self::BackToTab => "Back to Tab",
            Self::Minimize => "Minimize",
            Self::Close => "Close",
            Self::Back => "Back 15 seconds",
            Self::PlayPause if playing => "Pause",
            Self::PlayPause => "Play",
            Self::Forward => "Forward 15 seconds",
        }
    }

    fn symbol(self, playing: bool) -> &'static str {
        match self {
            Self::BackToTab => "arrow.up.left",
            Self::Minimize => "minus",
            Self::Close => "xmark",
            Self::Back => "gobackward.15",
            Self::PlayPause if playing => "pause.fill",
            Self::PlayPause => "play.fill",
            Self::Forward => "goforward.15",
        }
    }
}

/// A rectangle in points. In the panel the origin is top-left; for windows
/// it is the `AppKit` screen space (origin bottom-left, y up).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Area {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Area {
    fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

impl From<NSRect> for Area {
    fn from(r: NSRect) -> Self {
        Self {
            x: r.origin.x,
            y: r.origin.y,
            w: r.size.width,
            h: r.size.height,
        }
    }
}

impl From<Area> for NSRect {
    fn from(a: Area) -> Self {
        NSRect::new(NSPoint::new(a.x, a.y), NSSize::new(a.w, a.h))
    }
}

/// Where the buttons go on a `w` × `h` video. `labelled` are the widths of
/// the top buttons with their labels; when the three do not fit side by
/// side they show only their icons (`compact`). Live video has nothing to
/// seek in, so it gets no seek buttons.
fn layout(w: f64, h: f64, labelled: [f64; 3], live: bool) -> (bool, Vec<(Button, Area)>) {
    let compact = labelled.iter().sum::<f64>() + 2.0 * MARGIN + 3.0 * PILL_GAP > w;
    let width = |i: usize| if compact { PILL_HEIGHT } else { labelled[i] };
    let pill = |x: f64, w: f64| Area {
        x,
        y: MARGIN,
        w,
        h: PILL_HEIGHT,
    };
    let close_x = w - MARGIN - width(2);
    let minimize_x = close_x - PILL_GAP - width(1);
    let round = |cx: f64, size: f64| Area {
        x: cx - size / 2.0,
        y: h / 2.0 - size / 2.0,
        w: size,
        h: size,
    };
    let mut places = vec![
        (Button::BackToTab, pill(MARGIN, width(0))),
        (Button::Minimize, pill(minimize_x, width(1))),
        (Button::Close, pill(close_x, width(2))),
        (Button::PlayPause, round(w / 2.0, PLAY)),
    ];
    if !live {
        let offset = PLAY / 2.0 + CENTER_GAP + SEEK / 2.0;
        places.push((Button::Back, round(w / 2.0 - offset, SEEK)));
        places.push((Button::Forward, round(w / 2.0 + offset, SEEK)));
    }
    (compact, places)
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
fn edges_at(w: f64, h: f64, x: f64, y: f64) -> Option<Edges> {
    let corner_x = x < CORNER || x > w - CORNER;
    let corner_y = y < CORNER || y > h - CORNER;
    let edges = Edges {
        left: x < EDGE || (corner_y && x < CORNER),
        right: x > w - EDGE || (corner_y && x > w - CORNER),
        top: y < EDGE || (corner_x && y < CORNER),
        bottom: y > h - EDGE || (corner_x && y > h - CORNER),
    };
    (edges != Edges::default()).then_some(edges)
}

/// The window frame (`AppKit` screen space) after dragging `edges` of `start`
/// by `dx`, `dy` (pointer travel, y up). The video keeps its proportions and
/// the opposite edges stay put; the width stays within `min_w..=max_w`.
fn resized(start: Area, edges: Edges, dx: f64, dy: f64, min_w: f64, max_w: f64) -> Area {
    let ratio = start.w / start.h;
    let from_x = start.w + if edges.right { dx } else { -dx };
    let from_y = (start.h + if edges.top { dy } else { -dy }) * ratio;
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
            start.y
        } else {
            start.y + start.h - h
        },
        w,
        h,
    }
}

define_class!(
    /// The panel's content: takes every pointer event over the video.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    struct MagnetitePipView;

    impl MagnetitePipView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDownCanMoveWindow))]
        fn mouse_down_can_move_window(&self) -> bool {
            false
        }

        // The buttons inside only draw; this view handles the pointer.
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> *mut NSView {
            ptr::from_ref(self).cast_mut().cast()
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, event: &NSEvent) {
            pointer_moved(self.point(event));
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            pointer_moved(self.point(event));
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            pointer_left();
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            pressed(self.point(event));
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, _event: &NSEvent) {
            dragged();
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            released(self.point(event));
        }

        #[unsafe(method(hideIdle:))]
        fn hide_idle(&self, _sender: Option<&AnyObject>) {
            idle();
        }

        #[unsafe(method(retryAttach:))]
        fn retry_attach(&self, _sender: Option<&AnyObject>) {
            attach(self.mtm());
        }
    }
);

impl MagnetitePipView {
    /// The event's position in this view (top-left origin).
    fn point(&self, event: &NSEvent) -> (f64, f64) {
        let p = self.convertPoint_fromView(event.locationInWindow(), None);
        (p.x, p.y)
    }

    /// Run `action` on this view after `seconds`, replacing one still pending.
    fn perform_later(&self, action: Sel, seconds: f64) {
        self.cancel(action);
        // SAFETY: both selectors exist on this class and take one object.
        unsafe {
            let _: () = msg_send![self, performSelector: action, withObject: ptr::null::<AnyObject>(), afterDelay: seconds];
        }
    }

    fn cancel(&self, action: Sel) {
        // SAFETY: NSObject's class method; target, selector and nil object.
        unsafe {
            let _: () = msg_send![NSObject::class(), cancelPreviousPerformRequestsWithTarget: self, selector: action, object: ptr::null::<AnyObject>()];
        }
    }
}

define_class!(
    /// A view whose y axis points down, like the layout's.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    struct MagnetiteFlippedView;

    impl MagnetiteFlippedView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

/// The panel, made once and reused.
struct Panel {
    window: Retained<NSPanel>,
    root: Retained<MagnetitePipView>,
    /// Everything that fades in and out with the pointer.
    controls: Retained<MagnetiteFlippedView>,
    dim: Retained<NSBox>,
    buttons: Vec<(Button, Retained<NSBox>, Retained<NSButton>)>,
    /// Widths of the top buttons with their labels.
    labelled: [f64; 3],
    shown: bool,
    session: Option<Session>,
}

/// The video in picture-in-picture that the panel is on.
struct Session {
    owner: ContentId,
    video: Weak<NSWindow>,
    playing: bool,
    live: bool,
    observers: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

/// What a press on the panel turned into.
#[derive(Clone, Copy)]
enum Gesture {
    Press(Button),
    Move {
        pointer: NSPoint,
        origin: NSPoint,
    },
    Resize {
        pointer: NSPoint,
        start: Area,
        edges: Edges,
    },
}

thread_local! {
    static PANEL: RefCell<Option<Panel>> = const { RefCell::new(None) };
    /// The latest report that wants the controls: page, playing, live.
    static WANTED: Cell<Option<(ContentId, bool, bool)>> = const { Cell::new(None) };
    /// Tries left to find the video window, which can lag the report.
    static TRIES: Cell<u8> = const { Cell::new(0) };
    /// A page whose picture-in-picture we just ended, and when: its late
    /// reports that still say `pip` must not bring the panel back.
    static ENDING: Cell<Option<(ContentId, Instant)>> = const { Cell::new(None) };
    static GESTURE: Cell<Option<Gesture>> = const { Cell::new(None) };
    /// The page we last asked to enter picture-in-picture, and when.
    static REQUESTED: Cell<Option<(ContentId, Instant)>> = const { Cell::new(None) };
    static HOVER: Cell<Option<Button>> = const { Cell::new(None) };
}

/// We asked page `id` to enter picture-in-picture (`content_picture_in_picture`).
pub(crate) fn requested(id: ContentId) {
    REQUESTED.set(Some((id, Instant::now())));
}

/// Whether page `id` may own the floating video window. Its own report
/// cannot be trusted alone (pages can fake it), so: the page already owns
/// it, we asked it into picture-in-picture moments ago, or it is on screen
/// (a page starts picture-in-picture only from a click in it).
fn may_own(id: ContentId) -> bool {
    owner() == Some(id)
        || REQUESTED
            .get()
            .is_some_and(|(page, at)| page == id && at.elapsed() < Duration::from_secs(5))
        || Engine::global().content_shown(id)
}

/// Follow a media report from page `id`: put the controls on its video
/// window, update them, or take them off (when it reports media that is no
/// longer in picture-in-picture).
pub(crate) fn follow(id: ContentId, media: Option<&MediaState>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
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
        TRIES.set(8);
        attach(mtm);
        return;
    }
    // A report without media (the video ended, is muted or loads a new
    // source) is not the end of picture-in-picture: the video window closing
    // is (`watch`). The panel stays, so the page's next report is accepted.
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

/// The floating video window `window` stands for: itself, or the video
/// under our panel. For gestures on the video (`mac::install_pip_swipe`).
pub(crate) fn video_window_for(window: &NSWindow) -> Option<Retained<NSWindow>> {
    if is_video_window(window) {
        return Some(window.retain());
    }
    let ours = PANEL.with(|cell| {
        cell.try_borrow().is_ok_and(|p| {
            p.as_ref()
                .is_some_and(|p| ptr::eq(ptr::from_ref::<NSWindow>(&p.window), window))
        })
    });
    if ours { window.parentWindow() } else { None }
}

/// Chromium's video window: floating, on every Space, nobody's child.
fn is_video_window(window: &NSWindow) -> bool {
    window.isVisible()
        && window.level() == NSFloatingWindowLevel
        && window.parentWindow().is_none()
        && window
            .collectionBehavior()
            .contains(NSWindowCollectionBehavior::CanJoinAllSpaces)
}

fn owner() -> Option<ContentId> {
    PANEL.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|p| p.session.as_ref().map(|s| s.owner))
    })
}

/// Bring the panel in line with `WANTED`: update the buttons when it is on
/// that page's video already, else find the video window and lie over it
/// (trying again shortly while it is not there yet).
fn attach(mtm: MainThreadMarker) {
    let Some((owner, playing, live)) = WANTED.get() else {
        return;
    };
    let same = with_panel(mtm, |p| {
        let Some(s) = p.session.as_mut() else {
            return false;
        };
        if s.owner != owner || s.video.load().is_none() {
            return false;
        }
        s.playing = playing;
        s.live = live;
        lay_out(p);
        true
    });
    if same {
        WANTED.set(None);
        return;
    }
    // A window another page's video still holds is not up for grabs on a
    // report alone (pages can fake those): only when we asked this page into
    // picture-in-picture. When Chromium really switches videos, it makes a
    // new window, which the retry below picks up.
    let held = PANEL.with(|cell| {
        cell.try_borrow().ok().and_then(|p| {
            let s = p.as_ref()?.session.as_ref()?;
            (s.owner != owner).then(|| s.video.load()).flatten()
        })
    });
    let asked = REQUESTED
        .get()
        .is_some_and(|(page, at)| page == owner && at.elapsed() < Duration::from_secs(5));
    let video = NSApplication::sharedApplication(mtm)
        .windows()
        .iter()
        .find(|w| is_video_window(w))
        .filter(|w| asked || held.as_ref().is_none_or(|h| !ptr::eq(&**h, &**w)));
    let Some(video) = video else {
        if TRIES.get() > 0 {
            TRIES.set(TRIES.get() - 1);
            with_panel(mtm, |p| p.root.perform_later(sel!(retryAttach:), 0.25));
        }
        return;
    };
    detach();
    WANTED.set(None);
    let observers = watch(&video);
    with_panel(mtm, |p| {
        p.window.setFrame_display(video.frame(), false);
        p.window.setLevel(video.level());
        p.window.setCollectionBehavior(video.collectionBehavior());
        // SAFETY: both windows are alive; ordering a child above its parent.
        unsafe { video.addChildWindow_ordered(&p.window, NSWindowOrderingMode::Above) };
        p.session = Some(Session {
            owner,
            video: Weak::from_retained(&video),
            playing,
            live,
            observers,
        });
        lay_out(p);
        // Shown briefly when the video pops out, so they can be found.
        set_shown(p, true);
        p.root.perform_later(sel!(hideIdle:), IDLE);
    });
}

/// Follow the video window's size, and its closing.
fn watch(video: &NSWindow) -> Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>> {
    let center = NSNotificationCenter::defaultCenter();
    let object: &AnyObject = video;
    let resized = RcBlock::new(|_: NonNull<NSNotification>| sync_frame());
    let closing = RcBlock::new(|_: NonNull<NSNotification>| detach());
    // SAFETY: AppKit notification names; the blocks run on the main thread
    // (posted by the window there) and only touch thread-local state.
    unsafe {
        vec![
            center.addObserverForName_object_queue_usingBlock(
                Some(NSWindowDidResizeNotification),
                Some(object),
                None,
                &resized,
            ),
            center.addObserverForName_object_queue_usingBlock(
                Some(NSWindowWillCloseNotification),
                Some(object),
                None,
                &closing,
            ),
        ]
    }
}

/// Take the panel off the video window.
fn detach() {
    PANEL.with(|cell| {
        let Ok(mut cell) = cell.try_borrow_mut() else {
            return;
        };
        let Some(p) = cell.as_mut() else { return };
        let Some(s) = p.session.take() else { return };
        let center = NSNotificationCenter::defaultCenter();
        for o in &s.observers {
            // SAFETY: tokens returned by addObserverForName on this center.
            unsafe { center.removeObserver(o.as_ref()) };
        }
        if let Some(video) = s.video.load() {
            video.removeChildWindow(&p.window);
        }
        p.window.orderOut(None);
        p.root.cancel(sel!(hideIdle:));
        p.shown = false;
        p.controls.setAlphaValue(0.0);
    });
    GESTURE.set(None);
    HOVER.set(None);
}

/// The video window changed size: so does the panel.
fn sync_frame() {
    PANEL.with(|cell| {
        let Ok(mut cell) = cell.try_borrow_mut() else {
            return;
        };
        let Some(p) = cell.as_mut() else { return };
        let Some(video) = p.session.as_ref().and_then(|s| s.video.load()) else {
            return;
        };
        p.window.setFrame_display(video.frame(), true);
        lay_out(p);
    });
}

fn with_panel<R>(mtm: MainThreadMarker, f: impl FnOnce(&mut Panel) -> R) -> R {
    PANEL.with(|cell| {
        let mut cell = cell.borrow_mut();
        f(cell.get_or_insert_with(|| create_panel(mtm)))
    })
}

/// Place the buttons for the panel's size and the video's state.
fn lay_out(p: &Panel) {
    let Some(s) = p.session.as_ref() else { return };
    let size = p.window.frame().size;
    let all = NSRect::new(NSPoint::new(0.0, 0.0), size);
    p.controls.setFrame(all);
    p.dim.setFrame(all);
    let (compact, places) = layout(size.width, size.height, p.labelled, s.live);
    for (button, frame, inner) in &p.buttons {
        match places.iter().find(|(b, _)| b == button) {
            Some((_, area)) => {
                frame.setFrame((*area).into());
                frame.setHidden(false);
            }
            None => frame.setHidden(true),
        }
        if button.on_top() {
            let title = if compact {
                String::new()
            } else {
                labelled_title(*button)
            };
            inner.setTitle(&NSString::from_str(&title));
        } else if *button == Button::PlayPause
            && let Some(image) = symbol(*button, s.playing)
        {
            inner.setImage(Some(&image));
        }
    }
}

fn button_at(p: &Panel, x: f64, y: f64) -> Option<Button> {
    let s = p.session.as_ref()?;
    if !p.shown {
        return None;
    }
    let size = p.window.frame().size;
    let (_, places) = layout(size.width, size.height, p.labelled, s.live);
    places
        .into_iter()
        .find(|(_, a)| a.contains(x, y))
        .map(|(b, _)| b)
}

fn pointer_moved((x, y): (f64, f64)) {
    PANEL.with(|cell| {
        let Ok(mut cell) = cell.try_borrow_mut() else {
            return;
        };
        let Some(p) = cell.as_mut() else { return };
        if p.session.is_none() {
            return;
        }
        set_shown(p, true);
        let hover = button_at(p, x, y);
        if hover != HOVER.get() {
            HOVER.set(hover);
            for (button, frame, _) in &p.buttons {
                frame.setFillColor(&fill(*button, hover == Some(*button)));
            }
            let playing = p.session.as_ref().is_some_and(|s| s.playing);
            let tip = hover.map(|b| NSString::from_str(b.label(playing)));
            p.root.setToolTip(tip.as_deref());
        }
        let size = p.window.frame().size;
        cursor(hover, edges_at(size.width, size.height, x, y)).set();
        p.root.perform_later(sel!(hideIdle:), IDLE);
    });
}

fn pointer_left() {
    HOVER.set(None);
    if GESTURE.get().is_some() {
        return;
    }
    PANEL.with(|cell| {
        if let Ok(mut cell) = cell.try_borrow_mut()
            && let Some(p) = cell.as_mut()
        {
            for (button, frame, _) in &p.buttons {
                frame.setFillColor(&fill(*button, false));
            }
            p.root.cancel(sel!(hideIdle:));
            set_shown(p, false);
        }
    });
}

/// The pointer rested: the controls go, unless it is on one of them.
fn idle() {
    if GESTURE.get().is_some() || HOVER.get().is_some() {
        return;
    }
    PANEL.with(|cell| {
        if let Ok(mut cell) = cell.try_borrow_mut()
            && let Some(p) = cell.as_mut()
        {
            set_shown(p, false);
        }
    });
}

/// A press on a shown button presses it; elsewhere it starts moving or
/// resizing the video, and brings faded controls back.
fn pressed((x, y): (f64, f64)) {
    let gesture = PANEL.with(|cell| {
        let mut cell = cell.try_borrow_mut().ok()?;
        let p = cell.as_mut()?;
        let video = p.session.as_ref()?.video.load()?;
        if let Some(button) = button_at(p, x, y) {
            return Some(Gesture::Press(button));
        }
        set_shown(p, true);
        p.root.perform_later(sel!(hideIdle:), IDLE);
        let pointer = NSEvent::mouseLocation();
        let frame = video.frame();
        let size = p.window.frame().size;
        Some(match edges_at(size.width, size.height, x, y) {
            Some(edges) => Gesture::Resize {
                pointer,
                start: frame.into(),
                edges,
            },
            None => Gesture::Move {
                pointer,
                origin: frame.origin,
            },
        })
    });
    GESTURE.set(gesture);
}

fn dragged() {
    let Some(gesture) = GESTURE.get() else { return };
    let video = PANEL.with(|cell| {
        let cell = cell.try_borrow().ok()?;
        cell.as_ref()?.session.as_ref()?.video.load()
    });
    let Some(video) = video else { return };
    // Outside the panel's borrow: resizing the video resizes the panel.
    let now = NSEvent::mouseLocation();
    match gesture {
        Gesture::Press(_) => {}
        Gesture::Move { pointer, origin } => video.setFrameOrigin(NSPoint::new(
            origin.x + now.x - pointer.x,
            origin.y + now.y - pointer.y,
        )),
        Gesture::Resize {
            pointer,
            start,
            edges,
        } => {
            let min_w = video
                .contentMinSize()
                .width
                .max(video.minSize().width)
                .max(160.0);
            let max_w = video
                .screen()
                .map_or(f64::MAX, |s| s.visibleFrame().size.width);
            let frame = resized(
                start,
                edges,
                now.x - pointer.x,
                now.y - pointer.y,
                min_w,
                max_w,
            );
            video.setFrame_display(frame.into(), true);
        }
    }
}

fn released((x, y): (f64, f64)) {
    let Some(Gesture::Press(button)) = GESTURE.take() else {
        return;
    };
    let over = PANEL.with(|cell| {
        let cell = cell.try_borrow().ok()?;
        button_at(cell.as_ref()?, x, y)
    });
    if over == Some(button) {
        press(button);
    }
}

/// Do what `button` says for the page whose video is shown.
fn press(button: Button) {
    let Some((owner, playing)) = PANEL.with(|cell| {
        let cell = cell.try_borrow().ok()?;
        let s = cell.as_ref()?.session.as_ref()?;
        Some((s.owner, s.playing))
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
            // Flip the icon now; the page's next report confirms it.
            PANEL.with(|cell| {
                if let Ok(mut cell) = cell.try_borrow_mut()
                    && let Some(p) = cell.as_mut()
                {
                    if let Some(s) = p.session.as_mut() {
                        s.playing = !playing;
                    }
                    lay_out(p);
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

fn set_shown(p: &mut Panel, shown: bool) {
    if p.shown == shown {
        return;
    }
    p.shown = shown;
    NSAnimationContext::beginGrouping();
    NSAnimationContext::currentContext().setDuration(0.18);
    p.controls
        .animator()
        .setAlphaValue(if shown { 1.0 } else { 0.0 });
    NSAnimationContext::endGrouping();
}

/// Pointing hand on buttons, the resize cursor on edges (macOS 15 has public
/// ones; older systems keep the arrow), else the arrow.
fn cursor(hover: Option<Button>, edges: Option<Edges>) -> Retained<NSCursor> {
    if hover.is_some() {
        return NSCursor::pointingHandCursor();
    }
    let Some(e) = edges else {
        return NSCursor::arrowCursor();
    };
    // SAFETY: asking the class whether it implements a class method.
    let has: bool = unsafe {
        msg_send![NSCursor::class(), respondsToSelector: sel!(frameResizeCursorFromPosition:inDirections:)]
    };
    if !has {
        return NSCursor::arrowCursor();
    }
    let position = match (e.top, e.bottom, e.left, e.right) {
        (true, _, true, _) => NSCursorFrameResizePosition::TopLeft,
        (true, _, _, true) => NSCursorFrameResizePosition::TopRight,
        (_, true, true, _) => NSCursorFrameResizePosition::BottomLeft,
        (_, true, _, true) => NSCursorFrameResizePosition::BottomRight,
        (true, ..) => NSCursorFrameResizePosition::Top,
        (_, true, ..) => NSCursorFrameResizePosition::Bottom,
        (_, _, true, _) => NSCursorFrameResizePosition::Left,
        _ => NSCursorFrameResizePosition::Right,
    };
    NSCursor::frameResizeCursorFromPosition_inDirections(
        position,
        NSCursorFrameResizeDirections::All,
    )
}

/// A top button's title: its label a thin space away from the icon.
fn labelled_title(button: Button) -> String {
    format!("\u{2009}{}", button.label(false))
}

fn fill(button: Button, hover: bool) -> Retained<NSColor> {
    match (button.on_top(), hover) {
        (true, false) => NSColor::colorWithWhite_alpha(0.08, 0.62),
        (true, true) => NSColor::colorWithWhite_alpha(0.24, 0.8),
        (false, false) => NSColor::colorWithWhite_alpha(0.08, 0.5),
        (false, true) => NSColor::colorWithWhite_alpha(0.24, 0.72),
    }
}

fn symbol(button: Button, playing: bool) -> Option<Retained<NSImage>> {
    let (size, weight) = match button {
        // SAFETY (both): static AppKit constants.
        Button::PlayPause => (22.0, unsafe { NSFontWeightSemibold }),
        Button::Back | Button::Forward => (18.0, unsafe { NSFontWeightMedium }),
        _ => (11.0, unsafe { NSFontWeightSemibold }),
    };
    let config = NSImageSymbolConfiguration::configurationWithPointSize_weight(size, weight);
    let label = NSString::from_str(button.label(playing));
    NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(button.symbol(playing)),
        Some(&label),
    )
    .and_then(|i| i.imageWithSymbolConfiguration(&config))
}

fn create_panel(mtm: MainThreadMarker) -> Panel {
    use objc2::AnyThread as _;
    let zero = NSRect::ZERO;
    // Non-activating: a click on the video's controls leaves the frontmost
    // app frontmost.
    let window = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        zero,
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        NSBackingStoreType::Buffered,
        false,
    );
    // SAFETY: we keep the panel alive ourselves.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setOpaque(false);
    window.setHasShadow(false);
    // Nearly clear: the whole video takes the pointer, not only the buttons.
    window.setBackgroundColor(Some(&NSColor::colorWithWhite_alpha(0.0, 0.01)));
    window.setIgnoresMouseEvents(false);
    window.setExcludedFromWindowsMenu(true);
    window.setHidesOnDeactivate(false);
    window.setBecomesKeyOnlyIfNeeded(true);
    window.setMovable(false);
    // SAFETY: initWithFrame: is NSView's designated initialiser.
    let root: Retained<MagnetitePipView> =
        unsafe { msg_send![MagnetitePipView::alloc(mtm), initWithFrame: zero] };
    root.setWantsLayer(true);
    let options = NSTrackingAreaOptions::MouseEnteredAndExited
        | NSTrackingAreaOptions::MouseMoved
        | NSTrackingAreaOptions::ActiveAlways
        | NSTrackingAreaOptions::InVisibleRect;
    // SAFETY: the view owns its tracking area and outlives it.
    let area = unsafe {
        NSTrackingArea::initWithRect_options_owner_userInfo(
            NSTrackingArea::alloc(),
            zero,
            options,
            Some(&root),
            None,
        )
    };
    root.addTrackingArea(&area);
    // SAFETY: as above.
    let controls: Retained<MagnetiteFlippedView> =
        unsafe { msg_send![MagnetiteFlippedView::alloc(mtm), initWithFrame: zero] };
    controls.setAlphaValue(0.0);
    root.addSubview(&controls);
    let dim = plain_box(mtm, 0.0, &NSColor::colorWithWhite_alpha(0.0, 0.28));
    controls.addSubview(&dim);
    let mut labelled = [0.0; 3];
    let buttons = Button::ALL
        .into_iter()
        .map(|button| {
            let image = symbol(button, false).unwrap_or_default();
            let inner = if button.on_top() {
                let title = NSString::from_str(&labelled_title(button));
                // SAFETY: no target or action: the panel's view handles clicks.
                unsafe {
                    NSButton::buttonWithTitle_image_target_action(&title, &image, None, None, mtm)
                }
            } else {
                // SAFETY: as above.
                unsafe { NSButton::buttonWithImage_target_action(&image, None, None, mtm) }
            };
            inner.setBordered(false);
            inner.setImageHugsTitle(true);
            inner.setContentTintColor(Some(&NSColor::colorWithWhite_alpha(1.0, 0.94)));
            let radius = if button.on_top() {
                // SAFETY: a static AppKit constant.
                let weight = unsafe { NSFontWeightMedium };
                inner.setFont(Some(&NSFont::systemFontOfSize_weight(12.0, weight)));
                inner.sizeToFit();
                if let Some(i) = Button::ALL.iter().position(|b| *b == button) {
                    labelled[i] = inner.frame().size.width + 2.0 * PILL_PADDING;
                }
                PILL_RADIUS
            } else if button == Button::PlayPause {
                PLAY / 2.0
            } else {
                SEEK / 2.0
            };
            let frame = plain_box(mtm, radius, &fill(button, false));
            frame.setBorderColor(&NSColor::colorWithWhite_alpha(1.0, 0.12));
            frame.setBorderWidth(1.0);
            frame.setContentView(Some(&inner));
            controls.addSubview(&frame);
            (button, frame, inner)
        })
        .collect();
    window.setContentView(Some(&root));
    Panel {
        window,
        root,
        controls,
        dim,
        buttons,
        labelled,
        shown: false,
        session: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LABELLED: [f64; 3] = [104.0, 90.0, 72.0];

    fn place(places: &[(Button, Area)], b: Button) -> Option<Area> {
        places.iter().find(|(x, _)| *x == b).map(|(_, a)| *a)
    }

    #[test]
    fn top_buttons_lose_their_labels_when_narrow() {
        let (compact, places) = layout(480.0, 270.0, LABELLED, false);
        assert!(!compact);
        let close = place(&places, Button::Close).unwrap();
        assert!((close.x + close.w - (480.0 - MARGIN)).abs() < 1e-9);
        let (compact, places) = layout(260.0, 146.0, LABELLED, false);
        assert!(compact);
        let back = place(&places, Button::BackToTab).unwrap();
        let minimize = place(&places, Button::Minimize).unwrap();
        assert!(back.x + back.w < minimize.x);
    }

    #[test]
    fn live_video_has_no_seek_buttons() {
        let (_, places) = layout(480.0, 270.0, LABELLED, true);
        assert!(place(&places, Button::Back).is_none());
        let play = place(&places, Button::PlayPause).unwrap();
        assert!(play.contains(240.0, 135.0));
    }

    #[test]
    fn edges_and_corners_resize_the_middle_moves() {
        let corner = edges_at(400.0, 200.0, 10.0, 10.0).unwrap();
        assert!(corner.left && corner.top && !corner.right && !corner.bottom);
        let side = edges_at(400.0, 200.0, 397.0, 100.0).unwrap();
        assert!(side.right && !side.top && !side.bottom);
        assert!(edges_at(400.0, 200.0, 200.0, 100.0).is_none());
        assert!(edges_at(400.0, 200.0, 10.0, 100.0).is_none());
    }

    #[test]
    fn resizing_keeps_proportions_and_the_opposite_corner() {
        let start = Area {
            x: 100.0,
            y: 100.0,
            w: 320.0,
            h: 180.0,
        };
        let br = Edges {
            right: true,
            bottom: true,
            ..Edges::default()
        };
        // Pointer 80 right and 10 down: the wider move wins.
        let a = resized(start, br, 80.0, -10.0, 200.0, 2000.0);
        assert!((a.w - 400.0).abs() < 1e-9 && (a.h - 225.0).abs() < 1e-9);
        assert!((a.x - 100.0).abs() < 1e-9);
        assert!((a.y + a.h - 280.0).abs() < 1e-9, "top edge stays");
        let tl = Edges {
            left: true,
            top: true,
            ..Edges::default()
        };
        let a = resized(start, tl, 1000.0, -1000.0, 200.0, 2000.0);
        assert!((a.w - 200.0).abs() < 1e-9, "clamped to the minimum");
        assert!((a.x + a.w - 420.0).abs() < 1e-9 && (a.y - 100.0).abs() < 1e-9);
    }
}
