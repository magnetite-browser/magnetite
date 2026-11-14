//! Our child windows over the page area. Pages are separate windows that
//! swallow the pointer, so where the frame itself must react, a small
//! transparent window of ours lies above them: the invisible drag strip
//! along the top, the dimmed backdrop and buttons behind a peek, the
//! window's left edge that slides out the hidden sidebar, and the dividers
//! between split pages.

use super::{attach_above, borderless_window, fade, plain_box, screen_rect};
use crate::engine::Engine;
use crate::events::EngineEvent;
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::AnyObject, sel,
};
use objc2_app_kit::NSEvent;
use std::cell::Cell;

define_class!(
    /// Invisible strip along the top of the page area. Pages are separate
    /// windows that swallow clicks, so this view (in a transparent child
    /// window above them) drags the whole Magnetite window on press and zooms it
    /// on double-click, like a title bar.
    #[unsafe(super(objc2_app_kit::NSView))]
    #[thread_kind = MainThreadOnly]
    struct MagnetiteGrabView;

    impl MagnetiteGrabView {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            let Some(strip) = self.window() else { return };
            let target = strip.parentWindow().unwrap_or(strip);
            if event.clickCount() >= 2 {
                target.performZoom(None);
            } else {
                target.performWindowDragWithEvent(event);
            }
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }
    }
);

thread_local! {
    static GRAB_STRIP: std::cell::RefCell<Option<Retained<objc2_app_kit::NSWindow>>> = const { std::cell::RefCell::new(None) };
}

/// Put the drag strip at `at` (CEF screen coordinates: top-left origin, DIP)
/// and above the other child windows of the window whose content view is
/// `frame_handle`.
pub fn set_grab_strip(frame_handle: cef::sys::cef_window_handle_t, at: &cef::Rect) {
    use objc2_app_kit::NSView;
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if frame_handle.is_null() {
        return;
    }
    // SAFETY: CEF hands us the frame's live content NSView; main thread.
    let view: &NSView = unsafe { &*frame_handle.cast::<NSView>() };
    let (Some(frame_window), Some(rect)) = (view.window(), screen_rect(mtm, at)) else {
        return;
    };
    GRAB_STRIP.with(|g| {
        let mut g = g.borrow_mut();
        let strip = g.get_or_insert_with(|| {
            let win = borderless_window(mtm, rect, false);
            // Catch clicks even though nothing is drawn.
            win.setIgnoresMouseEvents(false);
            // SAFETY: initWithFrame: is NSView's designated initialiser.
            let grab: Retained<MagnetiteGrabView> =
                unsafe { msg_send![MagnetiteGrabView::alloc(mtm), initWithFrame: rect] };
            win.setContentView(Some(&grab));
            win
        });
        strip.setFrame_display(rect, false);
        // Attaching orders the strip in; never show it without its frame.
        if !frame_window.isVisible() || frame_window.isMiniaturized() {
            frame_window.removeChildWindow(strip);
            strip.orderOut(None);
            return;
        }
        // Re-attach so it is the topmost child, above pages that were just raised.
        attach_above(&frame_window, strip);
    });
}

define_class!(
    /// Covers the page area behind a peek: dims it, closes the peek when
    /// clicked, and receives the actions of the peek's buttons.
    #[unsafe(super(objc2_app_kit::NSView))]
    #[thread_kind = MainThreadOnly]
    struct MagnetitePeekBackdrop;

    impl MagnetitePeekBackdrop {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, _event: &NSEvent) {
            peek_command(crate::events::Shortcut::ClosePeek);
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(closePeek:))]
        fn close_peek(&self, _sender: Option<&AnyObject>) {
            peek_command(crate::events::Shortcut::ClosePeek);
        }

        #[unsafe(method(keepPeek:))]
        fn keep_peek(&self, _sender: Option<&AnyObject>) {
            peek_command(crate::events::Shortcut::PeekToTab);
        }

        #[unsafe(method(splitPeek:))]
        fn split_peek(&self, _sender: Option<&AnyObject>) {
            peek_command(crate::events::Shortcut::PeekToSplit);
        }
    }
);

fn peek_command(shortcut: crate::events::Shortcut) {
    Engine::global().emit(EngineEvent::Shortcut {
        from: None,
        shortcut,
    });
}

/// The dimmed page area behind a peek and its column of round buttons.
struct PeekBackdrop {
    window: Retained<objc2_app_kit::NSWindow>,
    dim: Retained<objc2_app_kit::NSBox>,
    buttons: Vec<Retained<objc2_app_kit::NSBox>>,
}

thread_local! {
    static PEEK_BACKDROP: std::cell::RefCell<Option<PeekBackdrop>> = const { std::cell::RefCell::new(None) };
}

/// Size of a peek button, the gap between buttons and to the peek (DIP).
const PEEK_BUTTON: f64 = 32.0;
const PEEK_BUTTON_GAP: f64 = 8.0;

/// Dim `area` (the pages) behind the peek window `peek_handle` and show the
/// peek's buttons (close, open as tab, split) in a column right of `peek`.
/// Rectangles are CEF screen coordinates (top-left origin, DIP); the windows
/// are children of the window whose content view is `frame_handle`.
pub fn show_peek_backdrop(
    frame_handle: cef::sys::cef_window_handle_t,
    peek_handle: cef::sys::cef_window_handle_t,
    area: &cef::Rect,
    peek: &cef::Rect,
) {
    use objc2_app_kit::NSView;
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if frame_handle.is_null() || peek_handle.is_null() {
        return;
    }
    // SAFETY: CEF hands us live content NSViews of its windows; main thread.
    let (frame_view, peek_view): (&NSView, &NSView) = unsafe {
        (
            &*frame_handle.cast::<NSView>(),
            &*peek_handle.cast::<NSView>(),
        )
    };
    let (Some(frame_window), Some(rect)) = (frame_view.window(), screen_rect(mtm, area)) else {
        return;
    };
    PEEK_BACKDROP.with(|cell| {
        let mut cell = cell.borrow_mut();
        let b = cell.get_or_insert_with(|| create_peek_backdrop(mtm));
        let appearing = !b.window.isVisible();
        b.window.setFrame_display(rect, true);
        b.dim
            .setFrame(NSRect::new(NSPoint::new(0.0, 0.0), rect.size));
        // Buttons stack down from the peek's top edge, just right of it.
        let x = f64::from(peek.x + peek.width - area.x) + PEEK_BUTTON_GAP + 2.0;
        let top = f64::from(peek.y - area.y);
        for (i, button) in b.buttons.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let from_top = top + i as f64 * (PEEK_BUTTON + PEEK_BUTTON_GAP);
            button.setFrame(NSRect::new(
                NSPoint::new(x, rect.size.height - from_top - PEEK_BUTTON),
                NSSize::new(PEEK_BUTTON, PEEK_BUTTON),
            ));
        }
        // Order: pages, then the backdrop, then the peek on top.
        attach_above(&frame_window, &b.window);
        if let Some(peek_window) = peek_view.window() {
            attach_above(&frame_window, &peek_window);
        }
        if appearing {
            b.window.setAlphaValue(0.0);
            fade(&b.window, 1.0, 0.16);
        }
    });
}

/// Remove the peek backdrop (the peek closed or became a tab).
pub fn hide_peek_backdrop() {
    PEEK_BACKDROP.with(|cell| {
        if let Some(b) = cell.borrow().as_ref() {
            if let Some(parent) = b.window.parentWindow() {
                parent.removeChildWindow(&b.window);
            }
            b.window.orderOut(None);
        }
    });
}

fn create_peek_backdrop(mtm: MainThreadMarker) -> PeekBackdrop {
    use objc2_app_kit::{
        NSButton, NSColor, NSFontWeightSemibold, NSImage, NSImageSymbolConfiguration,
    };
    use objc2_foundation::{NSRect, NSString};
    let window = borderless_window(mtm, NSRect::ZERO, false);
    // SAFETY: initWithFrame: is NSView's designated initialiser.
    let root: Retained<MagnetitePeekBackdrop> =
        unsafe { msg_send![MagnetitePeekBackdrop::alloc(mtm), initWithFrame: NSRect::ZERO] };
    // Same corners as the page cards it covers.
    let dim = plain_box(mtm, 10.0, &NSColor::colorWithWhite_alpha(0.0, 0.45));
    root.addSubview(&dim);
    // SAFETY: a static AppKit constant.
    let weight = unsafe { NSFontWeightSemibold };
    let config = NSImageSymbolConfiguration::configurationWithPointSize_weight(13.0, weight);
    let buttons = [
        ("xmark", "Close (Esc)", sel!(closePeek:)),
        (
            "arrow.up.left.and.arrow.down.right",
            "Open as tab (⌘O)",
            sel!(keepPeek:),
        ),
        (
            "rectangle.split.2x1",
            "Open in split view",
            sel!(splitPeek:),
        ),
    ]
    .into_iter()
    .map(|(symbol, tip, action)| {
        let circle = plain_box(
            mtm,
            PEEK_BUTTON / 2.0,
            &NSColor::colorWithWhite_alpha(0.12, 0.94),
        );
        circle.setBorderColor(&NSColor::colorWithWhite_alpha(1.0, 0.14));
        circle.setBorderWidth(1.0);
        let tip = NSString::from_str(tip);
        let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(symbol),
            Some(&tip),
        )
        .and_then(|i| i.imageWithSymbolConfiguration(&config));
        if let Some(image) = image {
            let target: &AnyObject = &root;
            // SAFETY: the target (the backdrop view) outlives its buttons.
            let button = unsafe {
                NSButton::buttonWithImage_target_action(&image, Some(target), Some(action), mtm)
            };
            button.setBordered(false);
            button.setContentTintColor(Some(&NSColor::colorWithWhite_alpha(1.0, 0.88)));
            button.setToolTip(Some(&tip));
            circle.setContentView(Some(&button));
        }
        root.addSubview(&circle);
        circle
    })
    .collect();
    window.setContentView(Some(&root));
    PeekBackdrop {
        window,
        dim,
        buttons,
    }
}

define_class!(
    /// The window's left edge while the sidebar is hidden: the pointer
    /// arriving there slides the sidebar out (`Engine::show_float_sidebar`);
    /// pressing drags the window, as the gap did before.
    #[unsafe(super(objc2_app_kit::NSView))]
    #[thread_kind = MainThreadOnly]
    struct MagnetiteEdgeView;

    impl MagnetiteEdgeView {
        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            // Not from inside an AppKit callback: windows may be created.
            Engine::post(Box::new(|| Engine::global().show_float_sidebar(true)));
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if let Some(parent) = self.window().and_then(|w| w.parentWindow()) {
                parent.performWindowDragWithEvent(event);
            }
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }
    }
);

thread_local! {
    static EDGE_ZONE: std::cell::RefCell<Option<Retained<objc2_app_kit::NSWindow>>> = const { std::cell::RefCell::new(None) };
}

/// Put the sidebar's hover zone at `at` (CEF screen coordinates, DIP) as a
/// child of the window whose content view is `frame_handle`, or remove it
/// (`None`).
pub fn set_edge_zone(frame_handle: cef::sys::cef_window_handle_t, at: Option<&cef::Rect>) {
    use objc2::AnyThread as _;
    use objc2_app_kit::{NSTrackingArea, NSTrackingAreaOptions, NSView};
    use objc2_foundation::NSRect;
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(at) = at else {
        EDGE_ZONE.with(|z| {
            if let Some(win) = z.borrow().as_ref() {
                if let Some(parent) = win.parentWindow() {
                    parent.removeChildWindow(win);
                }
                win.orderOut(None);
            }
        });
        return;
    };
    if frame_handle.is_null() {
        return;
    }
    // SAFETY: CEF hands us the frame's live content NSView; main thread.
    let view: &NSView = unsafe { &*frame_handle.cast::<NSView>() };
    let (Some(frame_window), Some(rect)) = (view.window(), screen_rect(mtm, at)) else {
        return;
    };
    EDGE_ZONE.with(|z| {
        let mut z = z.borrow_mut();
        let win = z.get_or_insert_with(|| {
            let win = borderless_window(mtm, rect, false);
            win.setIgnoresMouseEvents(false);
            // SAFETY: initWithFrame: is NSView's designated initialiser.
            let edge: Retained<MagnetiteEdgeView> =
                unsafe { msg_send![MagnetiteEdgeView::alloc(mtm), initWithFrame: rect] };
            let options = NSTrackingAreaOptions::MouseEnteredAndExited
                | NSTrackingAreaOptions::ActiveAlways
                | NSTrackingAreaOptions::InVisibleRect;
            // SAFETY: the view owns its tracking area and outlives it.
            let area = unsafe {
                NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(),
                    NSRect::ZERO,
                    options,
                    Some(&edge),
                    None,
                )
            };
            edge.addTrackingArea(&area);
            win.setContentView(Some(&edge));
            win
        });
        win.setFrame_display(rect, false);
        if !frame_window.isVisible() || frame_window.isMiniaturized() {
            frame_window.removeChildWindow(win);
            win.orderOut(None);
            return;
        }
        attach_above(&frame_window, win);
    });
}

/// Which divider a native divider view is. `row`: the panes on its two
/// sides are side by side (a vertical divider); else stacked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DividerKey {
    /// Between split pages: as `magnetite_core::split::Divider`.
    Split {
        path: Vec<usize>,
        index: usize,
        row: bool,
    },
    /// Between page `id` and its docked `DevTools` (`devdock.rs`).
    DevTools {
        id: crate::events::ContentId,
        row: bool,
    },
    /// Beside page `id` in device mode, to resize it (`device.rs`).
    Device {
        id: crate::events::ContentId,
        edge: crate::device::Edge,
    },
}

impl DividerKey {
    /// Whether it moves across (a vertical bar); the corner moves both ways.
    fn row(&self) -> bool {
        match self {
            Self::Split { row, .. } | Self::DevTools { row, .. } => *row,
            Self::Device { edge, .. } => *edge != crate::device::Edge::Bottom,
        }
    }
}

pub struct DividerIvars {
    key: std::cell::RefCell<DividerKey>,
    grip: Retained<objc2_app_kit::NSBox>,
    dragging: Cell<bool>,
    hovering: Cell<bool>,
}

define_class!(
    /// A divider in the gap between split pages, or between a page and its
    /// docked DevTools: drag to resize both sides, double-click to even out
    /// the split or give DevTools their default size. Positions come from the
    /// pointer's place on screen, so the window may move under the pointer
    /// while dragging. Transparent: the gap colour shows through; a grip
    /// appears on hover.
    #[unsafe(super(objc2_app_kit::NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = DividerIvars]
    pub struct MagnetiteDividerView;

    impl MagnetiteDividerView {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            if event.clickCount() >= 2 {
                self.report(crate::events::DividerAction::Equalize);
                return;
            }
            self.ivars().dragging.set(true);
            self.show_grip();
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, _event: &NSEvent) {
            self.cursor().set();
            let Some(mtm) = MainThreadMarker::new() else { return };
            let at = NSEvent::mouseLocation();
            // AppKit's y grows upwards from the primary screen's bottom.
            let height = objc2_app_kit::NSScreen::screens(mtm).firstObject().map_or(0.0, |s| s.frame().size.height);
            #[allow(clippy::cast_possible_truncation)]
            let (x, y) = (at.x.round() as i32, (height - at.y).round() as i32);
            // A resize handle in device mode takes both; a divider its axis.
            let key = self.ivars().key.borrow().clone();
            if let DividerKey::Device { id, edge } = key {
                Engine::global().device_handle(id, edge, x, y, crate::events::DividerAction::Move(x));
                return;
            }
            self.report(crate::events::DividerAction::Move(if key.row() { x } else { y }));
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, _event: &NSEvent) {
            if self.ivars().dragging.replace(false) {
                self.report(crate::events::DividerAction::Release);
            }
            self.show_grip();
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            self.ivars().hovering.set(true);
            self.cursor().set();
            self.show_grip();
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.ivars().hovering.set(false);
            if !self.ivars().dragging.get() {
                objc2_app_kit::NSCursor::arrowCursor().set();
            }
            self.show_grip();
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }
    }
);

impl MagnetiteDividerView {
    fn new(
        mtm: MainThreadMarker,
        frame: objc2_foundation::NSRect,
        key: DividerKey,
    ) -> Retained<Self> {
        use objc2::AnyThread as _;
        use objc2_app_kit::{NSColor, NSTrackingArea, NSTrackingAreaOptions};
        use objc2_foundation::NSRect;
        let grip = plain_box(mtm, 1.0, &NSColor::colorWithWhite_alpha(1.0, 0.35));
        grip.setHidden(true);
        let this = Self::alloc(mtm).set_ivars(DividerIvars {
            key: std::cell::RefCell::new(key),
            grip,
            dragging: Cell::new(false),
            hovering: Cell::new(false),
        });
        // SAFETY: NSView's designated initialiser on our subclass.
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        this.addSubview(&this.ivars().grip);
        let options = NSTrackingAreaOptions::MouseEnteredAndExited
            | NSTrackingAreaOptions::ActiveAlways
            | NSTrackingAreaOptions::InVisibleRect;
        // SAFETY: the view owns its tracking area and outlives it.
        let area = unsafe {
            NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                NSRect::ZERO,
                options,
                Some(&this),
                None,
            )
        };
        this.addTrackingArea(&area);
        this
    }

    fn report(&self, action: crate::events::DividerAction) {
        let key = self.ivars().key.borrow().clone();
        let engine = Engine::global();
        match key {
            DividerKey::Split { path, index, .. } => engine.emit(EngineEvent::SplitDivider {
                path,
                index,
                action,
            }),
            DividerKey::DevTools { id, .. } => engine.devtools_divider(id, action),
            // Device handles move with both coordinates (`mouse_dragged`);
            // here only letting go counts.
            DividerKey::Device { id, edge } => engine.device_handle(id, edge, 0, 0, action),
        }
    }

    // The successors (`columnResizeCursorInDirections:`) need macOS 15.
    #[allow(deprecated)]
    fn cursor(&self) -> Retained<objc2_app_kit::NSCursor> {
        let corner = crate::device::Edge::Corner;
        if matches!(*self.ivars().key.borrow(), DividerKey::Device { edge, .. } if edge == corner) {
            return objc2_app_kit::NSCursor::crosshairCursor();
        }
        if self.ivars().key.borrow().row() {
            objc2_app_kit::NSCursor::resizeLeftRightCursor()
        } else {
            objc2_app_kit::NSCursor::resizeUpDownCursor()
        }
    }

    /// A short line in the middle of the gap while hovered, longer and in
    /// brass while dragging.
    fn show_grip(&self) {
        use objc2_foundation::{NSPoint, NSRect, NSSize};
        let iv = self.ivars();
        let (dragging, visible) = (iv.dragging.get(), iv.dragging.get() || iv.hovering.get());
        iv.grip.setHidden(!visible);
        let colour = if dragging {
            objc2_app_kit::NSColor::colorWithSRGBRed_green_blue_alpha(0.84, 0.65, 0.40, 1.0)
        } else {
            objc2_app_kit::NSColor::colorWithWhite_alpha(1.0, 0.35)
        };
        iv.grip.setFillColor(&colour);
        let b = self.bounds();
        let long = if dragging { 56.0 } else { 32.0 };
        let (w, h) = if iv.key.borrow().row() {
            (2.0, long)
        } else {
            (long, 2.0)
        };
        iv.grip.setFrame(NSRect::new(
            NSPoint::new((b.size.width - w) / 2.0, (b.size.height - h) / 2.0),
            NSSize::new(w, h),
        ));
    }
}

thread_local! {
    static DIVIDERS: std::cell::RefCell<Vec<(Retained<objc2_app_kit::NSWindow>, Retained<MagnetiteDividerView>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Show exactly `dividers` (CEF screen coordinates: top-left origin, DIP) as
/// children of the window whose content view is `frame_handle`. Windows are
/// reused by position in the list; a divider being dragged stays put in the
/// window order so the drag is never interrupted.
pub fn set_dividers(
    frame_handle: cef::sys::cef_window_handle_t,
    dividers: &[(cef::Rect, DividerKey)],
) {
    use objc2_app_kit::NSView;
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let frame_window = (!frame_handle.is_null())
        // SAFETY: CEF hands us the frame's live content NSView; main thread.
        .then(|| unsafe { &*frame_handle.cast::<NSView>() })
        .and_then(NSView::window);
    DIVIDERS.with(|list| {
        let mut list = list.borrow_mut();
        // Fewer dividers than before: drop the rest.
        let keep = dividers.len().min(list.len());
        for (win, _) in list.drain(keep..) {
            if let Some(parent) = win.parentWindow() {
                parent.removeChildWindow(&win);
            }
            win.orderOut(None);
        }
        let Some(frame_window) = frame_window else {
            return;
        };
        for (i, (at, key)) in dividers.iter().enumerate() {
            let Some(rect) = screen_rect(mtm, at) else {
                return;
            };
            if i == list.len() {
                let win = borderless_window(mtm, rect, false);
                // Catch the pointer even though the window draws nothing.
                win.setIgnoresMouseEvents(false);
                let view = MagnetiteDividerView::new(mtm, rect, key.clone());
                win.setContentView(Some(&view));
                list.push((win, view));
            }
            let (win, view) = &list[i];
            *view.ivars().key.borrow_mut() = key.clone();
            win.setFrame_display(rect, true);
            view.show_grip();
            if view.ivars().dragging.get() {
                continue;
            }
            attach_above(&frame_window, win);
        }
    });
}
