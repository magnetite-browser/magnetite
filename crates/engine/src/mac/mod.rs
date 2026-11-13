//! macOS glue: what CEF does not do for us on the Mac.
//!
//! This file holds the `NSApplication` subclass that implements CEF's
//! protocols and the app delegate (links from other apps, reopen, terminate;
//! it installs our menu bar, `menubar.rs`), adapted from the cef-rs
//! `cefsimple` example (Apache-2.0 OR MIT). Next to them: tweaks to CEF's
//! own windows (pinned, rounded corners, the focus ring of a split page,
//! hidden or shown, shown without taking focus), swipes that fling the
//! floating video to a screen edge, and helpers for the child modules:
//!
//! - `chrome`: our child windows over the page area (drag strip, peek
//!   backdrop, the hidden sidebar's edge, split dividers);
//! - `default_browser`: whether Magnetite opens links from other apps;
//! - `hotkey`: the system-wide quick search key;
//! - `pill`: the link-hover status pill;
//! - `pressure`: the system's memory pressure signal;
//! - `share`: the share menu, the pasteboard and the save panel.
//!
//! Everything runs on the main thread (CEF's UI thread on macOS). CEF's
//! screen rectangles have a top-left origin; `screen_rect` turns them into
//! AppKit's, whose origin is the primary screen's bottom-left.

mod chrome;
mod default_browser;
mod hotkey;
mod pill;
mod pressure;
mod share;

pub use chrome::{
    DividerKey, hide_peek_backdrop, set_dividers, set_edge_zone, set_grab_strip, show_peek_backdrop,
};
pub use default_browser::{handles, set_handler};
pub use hotkey::set_quick_search_hotkey;
pub use pill::{hide_status_pill, set_status_pill};
pub use pressure::watch_memory_pressure;
pub use share::{copy_png, copy_text, save_png, share_png, show_share_picker};

use crate::engine::Engine;
use crate::events::EngineEvent;
use cef::ImplWindow;
use cef::application_mac::{CefAppProtocol, CrAppControlProtocol, CrAppProtocol};
use objc2::{
    ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, extern_methods,
    msg_send,
    rc::Retained,
    runtime::{AnyObject, Bool, NSObject, NSObjectProtocol, ProtocolObject},
    sel,
};
use objc2_app_kit::{
    NSApp, NSApplication, NSApplicationDelegate, NSApplicationTerminateReply, NSBox, NSColor,
    NSEvent, NSWindow,
};
use objc2_foundation::{NSObjectNSThreadPerformAdditions, NSRect};
use std::cell::Cell;

define_class! {
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    pub struct MagnetiteAppDelegate;

    impl MagnetiteAppDelegate {
        /// Create the application on the UI thread: our menu bar.
        #[unsafe(method(createApplication:))]
        unsafe fn create_application(&self, _object: Option<&AnyObject>) {
            let mtm = MainThreadMarker::new().expect("Not running on the main thread");
            let app = NSApp(mtm);
            assert!(app.isKindOfClass(MagnetiteApplication::class()));
            assert!(
                app.delegate()
                    .unwrap()
                    .isKindOfClass(MagnetiteAppDelegate::class())
            );
            crate::menubar::install(mtm);
        }
    }

    unsafe impl NSObjectProtocol for MagnetiteAppDelegate {}

    unsafe impl NSApplicationDelegate for MagnetiteAppDelegate {
        /// Links from other applications (we are the default browser) and
        /// `open -a`. Delivered to the application as OpenUrlRequested; the
        /// app brings the main window forward, unless the link opens in a
        /// mini window.
        #[unsafe(method(application:openURLs:))]
        unsafe fn application_open_urls(&self, _app: &NSApplication, urls: &objc2_foundation::NSArray<objc2_foundation::NSURL>) {
            for url in urls {
                if let Some(s) = url.absoluteString() {
                    Engine::global().emit(EngineEvent::OpenUrlRequested { from: None, url: s.to_string(), background: false, new_window: false, own_window: false, locked: false, source: crate::events::UrlSource::OtherApp, context: None });
                }
            }
        }

        #[unsafe(method(applicationShouldTerminate:))]
        unsafe fn application_should_terminate(&self, _sender: &NSApplication) -> NSApplicationTerminateReply {
            NSApplicationTerminateReply::TerminateNow
        }

        /// Called when the user clicks the app dock icon while the application is
        /// already running.
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        unsafe fn application_should_handle_reopen(&self, _sender: &NSApplication, _has_visible_windows: Bool) -> Bool {
            if let Some(window) = Engine::global().frame_window() {
                window.show();
            }
            Bool::NO
        }

        /// Requests that any state restoration archive be created with secure encoding
        /// (macOS 12+ only). See https://crrev.com/c737387656 for details. This also
        /// fixes an issue with macOS default behavior incorrectly restoring windows
        /// after hard reset (holding down the power button).
        #[unsafe(method(applicationSupportsSecureRestorableState:))]
        unsafe fn application_supports_secure_restorable_state(&self, _sender: &NSApplication) -> Bool {
            Bool::YES
        }
    }

}

impl MagnetiteAppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = MagnetiteAppDelegate::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

/// Instance variables of `MagnetiteApplication`.
#[derive(Default)]
pub struct MagnetiteApplicationIvars {
    handling_send_event: Cell<Bool>,
}

define_class!(
    /// A `NSApplication` subclass that implements the required CEF protocols.
    ///
    /// This class provides the necessary `CefAppProtocol` conformance to
    /// ensure that events are handled correctly by the Chromium framework on macOS.
    #[unsafe(super(NSApplication))]
    #[ivars = MagnetiteApplicationIvars]
    pub struct MagnetiteApplication;

    impl MagnetiteApplication {
        #[unsafe(method(sendEvent:))]
        unsafe fn send_event(&self, event: &NSEvent) {
            let was_sending_event = self.is_handling_send_event();
            if !was_sending_event {
                self.set_handling_send_event(true);
            }

            let _: () = msg_send![super(self), sendEvent:event];

            if !was_sending_event {
                self.set_handling_send_event(false);
            }
        }

        /// |-terminate:| is the entry point for orderly "quit" operations in Cocoa. This
        /// includes the application menu's quit menu item and keyboard equivalent, the
        /// application's dock icon menu's quit menu item, "quit" (not "force quit") in
        /// the Activity Monitor, and quits triggered by user logout and system restart
        /// and shutdown.
        ///
        /// The default |-terminate:| implementation ends the process by calling exit(),
        /// and thus never leaves the main run loop. This is unsuitable for Chromium
        /// since Chromium depends on leaving the main run loop to perform an orderly
        /// shutdown. We support the normal |-terminate:| interface by overriding the
        /// default implementation. Our implementation, which is very specific to the
        /// needs of Chromium, works by asking the application delegate to terminate
        /// using its |-tryToTerminateApplication:| method.
        ///
        /// |-tryToTerminateApplication:| differs from the standard
        /// |-applicationShouldTerminate:| in that no special event loop is run in the
        /// case that immediate termination is not possible (e.g., if dialog boxes
        /// allowing the user to cancel have to be shown). Instead, this method tries to
        /// close all browsers by calling CloseBrowser(false) via
        /// ClientHandler::CloseAllBrowsers. Calling CloseBrowser will result in a call
        /// to ClientHandler::DoClose and execution of |-performClose:| on the NSWindow.
        /// DoClose sets a flag that is used to differentiate between new close events
        /// (e.g., user clicked the window close button) and in-progress close events
        /// (e.g., user approved the close window dialog). The NSWindowDelegate
        /// |-windowShouldClose:| method checks this flag and either calls
        /// CloseBrowser(false) in the case of a new close event or destructs the
        /// NSWindow in the case of an in-progress close event.
        /// ClientHandler::OnBeforeClose will be called after the CEF NSView hosted in
        /// the NSWindow is dealloc'ed.
        ///
        /// After the final browser window has closed ClientHandler::OnBeforeClose will
        /// begin actual tear-down of the application by calling CefQuitMessageLoop.
        /// This ends the NSApplication event loop and execution then returns to the
        /// main() function for cleanup before application termination.
        ///
        /// The standard |-applicationShouldTerminate:| is not supported, and code paths
        /// leading to it must be redirected.
        #[unsafe(method(terminate:))]
        unsafe fn terminate(&self, _sender: &AnyObject) {
            let engine = Engine::global();
            if engine.is_quitting() { return; }
            engine.emit(EngineEvent::QuitRequested);
        }
    }

    unsafe impl CrAppControlProtocol for MagnetiteApplication {
        #[unsafe(method(setHandlingSendEvent:))]
        unsafe fn _set_handling_send_event(&self, handling_send_event: Bool) {
            self.ivars().handling_send_event.set(handling_send_event);
        }
    }

    unsafe impl CrAppProtocol for MagnetiteApplication {
        #[unsafe(method(isHandlingSendEvent))]
        unsafe fn _is_handling_send_event(&self) -> Bool {
            self.ivars().handling_send_event.get()
        }
    }

    unsafe impl CefAppProtocol for MagnetiteApplication {}
);

impl MagnetiteApplication {
    extern_methods! {
        #[unsafe(method(sharedApplication))]
        fn shared_application() -> Retained<Self>;

        #[unsafe(method(setHandlingSendEvent:))]
        fn set_handling_send_event(&self, handling_send_event: bool);

        #[unsafe(method(isHandlingSendEvent))]
        fn is_handling_send_event(&self) -> bool;
    }
}

pub fn setup_application() {
    // Initialize the MagnetiteApplication instance.
    // SAFETY: mtm ensures that here is the main thread.
    let _ = MagnetiteApplication::shared_application();

    // If there was an invocation to NSApp prior to here,
    // then the NSApp will not be a MagnetiteApplication.
    // The following assertion ensures that this doesn't happen.
    assert!(
        NSApp(MainThreadMarker::new().expect("Not running on the main thread"))
            .isKindOfClass(MagnetiteApplication::class())
    );
}

pub fn setup_app_delegate() -> Retained<MagnetiteAppDelegate> {
    let mtm = MainThreadMarker::new().expect("Not running on the main thread");

    // Create the application delegate.
    let simple_delegate = MagnetiteAppDelegate::new(mtm);
    let delegate_proto =
        ProtocolObject::<dyn NSApplicationDelegate>::from_retained(simple_delegate.clone());
    let app = NSApp(MainThreadMarker::new().expect("Not running on the main thread"));
    assert!(app.isKindOfClass(MagnetiteApplication::class()));
    app.setDelegate(Some(&delegate_proto));
    assert!(
        app.delegate()
            .unwrap()
            .isKindOfClass(MagnetiteAppDelegate::class())
    );

    unsafe {
        simple_delegate.performSelectorOnMainThread_withObject_waitUntilDone(
            sel!(createApplication:),
            None,
            false,
        );
    }

    simple_delegate
}

/// Content windows are children of the frame and must only move with it.
/// Chrome's frameless window still lets the user drag it by empty chrome
/// areas, so AppKit is told the window is not user-movable at all.
pub fn pin_window(handle: cef::sys::cef_window_handle_t) {
    if handle.is_null() {
        return;
    }
    // SAFETY: CEF hands us a live NSView*; `setMovable:` takes a BOOL.
    unsafe {
        let view: &AnyObject = &*handle.cast::<AnyObject>();
        let window: *mut AnyObject = msg_send![view, window];
        if window.is_null() {
            return;
        }
        let window: &AnyObject = &*window;
        let _: () = msg_send![window, setMovable: Bool::NO];
        let _: () = msg_send![window, setMovableByWindowBackground: Bool::NO];
    }
}

/// Give a frameless child window rounded corners, a clear background and a
/// shadow only if `shadow`, so the page floats inside the frame. `handle` is
/// the window's content `NSView`.
pub fn round_window_corners(handle: cef::sys::cef_window_handle_t, radius: f64, shadow: bool) {
    if handle.is_null() {
        return;
    }
    // SAFETY: CEF hands us a live NSView*; we only send well-known AppKit
    // and CoreAnimation messages.
    unsafe {
        let view: &AnyObject = &*handle.cast::<AnyObject>();
        let window: *mut AnyObject = msg_send![view, window];
        if window.is_null() {
            return;
        }
        let window: &AnyObject = &*window;
        let _: () = msg_send![window, setOpaque: Bool::NO];
        let _: () = msg_send![window, setHasShadow: Bool::from(shadow)];
        let clear: *mut AnyObject = msg_send![objc2::class!(NSColor), clearColor];
        let _: () = msg_send![window, setBackgroundColor: clear];
        let content: *mut AnyObject = msg_send![window, contentView];
        if content.is_null() {
            return;
        }
        let content: &AnyObject = &*content;
        let _: () = msg_send![content, setWantsLayer: Bool::YES];
        let layer: *mut AnyObject = msg_send![content, layer];
        if !layer.is_null() {
            let _: () = msg_send![&*layer, setCornerRadius: radius];
            let _: () = msg_send![&*layer, setMasksToBounds: Bool::YES];
            // Continuous (squircle) corners. The outline of a focused split
            // page is a separate layer (`set_focus_ring`).
            let curve = objc2_foundation::ns_string!("continuous");
            let _: () = msg_send![&*layer, setCornerCurve: curve];
        }
    }
}

/// Two-finger swipes on the floating video window (picture-in-picture) fling
/// it to the screen edge in the swipe's direction; diagonal swipes reach a
/// corner. Quicker than dragging. Installed once on the UI (main) thread.
pub fn install_pip_swipe() {
    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask};
    use std::ptr::NonNull;

    let handler = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit passes a live event for the duration of the call.
        let ev = unsafe { event.as_ref() };
        if let Some(mtm) = MainThreadMarker::new()
            && let Some(window) = ev.window(mtm)
            && let Some(video) = crate::pip::video_window_for(&window)
        {
            pip_swipe(ev, &video);
        }
        event.as_ptr()
    });
    // SAFETY: the block returns the event it was given (never dangling).
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::ScrollWheel, &handler)
    };
    // The monitor lives as long as the app.
    std::mem::forget(monitor);
}

thread_local! {
    /// Finger travel of the current swipe on the floating video, in points.
    static PIP_TRAVEL: std::cell::Cell<(f64, f64)> = const { std::cell::Cell::new((0.0, 0.0)) };
}

fn pip_swipe(ev: &objc2_app_kit::NSEvent, window: &objc2_app_kit::NSWindow) {
    use objc2_app_kit::NSEventPhase;
    let phase = ev.phase();
    if phase.contains(NSEventPhase::Began) {
        PIP_TRAVEL.with(|t| t.set((0.0, 0.0)));
    }
    if phase.contains(NSEventPhase::Changed) {
        // Finger direction: with natural scrolling the deltas follow the
        // fingers, otherwise they are inverted. y grows downwards here.
        let sign = if ev.isDirectionInvertedFromDevice() {
            1.0
        } else {
            -1.0
        };
        PIP_TRAVEL.with(|t| {
            let (x, y) = t.get();
            t.set((
                x + sign * ev.scrollingDeltaX(),
                y + sign * ev.scrollingDeltaY(),
            ));
        });
    }
    if phase.contains(NSEventPhase::Ended) {
        let (dx, dy) = PIP_TRAVEL.with(|t| t.replace((0.0, 0.0)));
        fling_window(window, dx, dy);
    }
}

/// Move `window` towards the visible-frame edge(s) the finger travel points
/// at. Travel under 30 pt on an axis keeps that axis where it is.
fn fling_window(window: &objc2_app_kit::NSWindow, dx: f64, dy: f64) {
    const MIN_TRAVEL: f64 = 30.0;
    const MARGIN: f64 = 16.0;
    let Some(screen) = window.screen() else {
        return;
    };
    let area = screen.visibleFrame();
    let mut frame = window.frame();
    if dx.abs() >= MIN_TRAVEL {
        frame.origin.x = if dx > 0.0 {
            area.origin.x + area.size.width - frame.size.width - MARGIN
        } else {
            area.origin.x + MARGIN
        };
    }
    if dy.abs() >= MIN_TRAVEL {
        // AppKit's y axis points up: a downward swipe means a smaller origin.
        frame.origin.y = if dy > 0.0 {
            area.origin.y + MARGIN
        } else {
            area.origin.y + area.size.height - frame.size.height - MARGIN
        };
    }
    if dx.abs() >= MIN_TRAVEL || dy.abs() >= MIN_TRAVEL {
        window.setFrame_display_animate(frame, true, true);
    }
}

/// Close the window the keyboard is in, if it is neither the window holding
/// the view `frame` nor attached to it (pages, overlays, docked DevTools):
/// a window Chromium opened itself, or DevTools in a window of their own.
/// True if there was such a window.
pub fn close_key_window_outside(frame: cef::sys::cef_window_handle_t) -> bool {
    use objc2_app_kit::NSView;
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    if frame.is_null() {
        return false;
    }
    // SAFETY: CEF hands us a live NSView; we are on the main thread.
    let view: &NSView = unsafe { &*frame.cast::<NSView>() };
    let (Some(ours), Some(key)) = (
        view.window(),
        NSApplication::sharedApplication(mtm).keyWindow(),
    ) else {
        return false;
    };
    let mut window = Some(key.clone());
    while let Some(w) = window {
        if w == ours {
            return false;
        }
        window = w.parentWindow();
    }
    key.performClose(None);
    true
}

/// Take the window holding the view `handle` off screen, or bring it back.
pub fn show_window_of(handle: cef::sys::cef_window_handle_t, shown: bool) {
    use objc2_app_kit::NSView;
    if handle.is_null() || MainThreadMarker::new().is_none() {
        return;
    }
    // SAFETY: CEF hands us a live NSView; we are on the main thread.
    let view: &NSView = unsafe { &*handle.cast::<NSView>() };
    if let Some(window) = view.window() {
        if shown {
            window.orderFront(None);
        } else {
            window.orderOut(None);
        }
    }
}

/// Whether the view `handle` sits in a window without a tab strip: the
/// pop-up window an extension opens with `chrome.windows.create({type:
/// "popup"})`. Chromium puts the tab strip of its other windows (a normal
/// window, the one a new tab lands in) into the title bar, which needs a
/// full-size content view; a pop-up window keeps a plain title bar.
pub fn is_popup_window(handle: cef::sys::cef_window_handle_t) -> bool {
    use objc2_app_kit::{NSView, NSWindowStyleMask};
    if handle.is_null() || MainThreadMarker::new().is_none() {
        return false;
    }
    // SAFETY: CEF hands us a live NSView; we are on the main thread.
    let view: &NSView = unsafe { &*handle.cast::<NSView>() };
    view.window().is_some_and(|w| {
        !w.styleMask()
            .contains(NSWindowStyleMask::FullSizeContentView)
    })
}

/// Bring the window holding the view `handle` to the front with the keyboard.
pub fn focus_window_of(handle: cef::sys::cef_window_handle_t) {
    use objc2_app_kit::NSView;
    if handle.is_null() || MainThreadMarker::new().is_none() {
        return;
    }
    // SAFETY: CEF hands us a live NSView; we are on the main thread.
    let view: &NSView = unsafe { &*handle.cast::<NSView>() };
    if let Some(window) = view.window() {
        window.makeKeyAndOrderFront(None);
    }
}

/// Run `f` (which shows a window) without moving keyboard focus: whatever
/// window was key before stays key, so typing keeps going to the page.
pub fn without_taking_focus(f: impl FnOnce()) {
    let key =
        MainThreadMarker::new().and_then(|mtm| NSApplication::sharedApplication(mtm).keyWindow());
    f();
    if let Some(key) = key {
        key.makeKeyWindow();
    }
}

/// The corner radius a page window shows: macOS rounds titled windows itself
/// (16 pt since macOS 26) and that clips more than our layer's `own` radius
/// when it is larger. The window's radius has no public API, so it is read
/// only if the window answers `_cornerRadius`; otherwise ours is assumed.
fn visible_corner(view: &objc2_app_kit::NSView, own: f64) -> f64 {
    let Some(window) = view.window() else {
        return own;
    };
    let sel = objc2::sel!(_cornerRadius);
    // SAFETY: asked only after the window says it implements the getter,
    // which returns a CGFloat.
    let system: f64 = unsafe {
        let has: bool = msg_send![&*window, respondsToSelector: sel];
        if has {
            msg_send![&*window, _cornerRadius]
        } else {
            0.0
        }
    };
    own.max(system)
}

/// Outline the page window whose content view is `handle` in `argb` (the
/// focused page of a split), or remove the outline (`None`). The outline is
/// its own layer on top of the page, inset by a point and concentric with the
/// corner the page really shows (`visible_corner`), so nothing clips it and
/// the curve matches the page's.
pub fn set_focus_ring(handle: cef::sys::cef_window_handle_t, argb: Option<u32>) {
    use objc2_app_kit::NSView;
    use objc2_foundation::{NSArray, NSString};
    const NAME: &str = "magnetite-focus-ring";
    const WIDTH: f64 = 2.0;
    const INSET: f64 = 1.0;
    if handle.is_null() {
        return;
    }
    // SAFETY: CEF hands us a live NSView of a page window; main thread.
    let view: &NSView = unsafe { &*handle.cast::<NSView>() };
    let Some(content) = view.window().and_then(|w| w.contentView()) else {
        return;
    };
    content.setWantsLayer(true);
    // SAFETY: CoreAnimation messages on the content view's layer and a
    // sublayer we own, with the types CALayer declares.
    unsafe {
        let Some(host): Option<Retained<AnyObject>> = msg_send![&*content, layer] else {
            return;
        };
        let sublayers: Option<Retained<NSArray<AnyObject>>> = msg_send![&*host, sublayers];
        let existing = sublayers.and_then(|list| {
            list.iter().find(|l| {
                let name: Option<Retained<NSString>> = msg_send![&**l, name];
                name.is_some_and(|n| n.to_string() == NAME)
            })
        });
        let Some(c) = argb else {
            if let Some(ring) = existing {
                let _: () = msg_send![&*ring, removeFromSuperlayer];
            }
            return;
        };
        let ring = existing.unwrap_or_else(|| {
            let ring: Retained<AnyObject> = msg_send![objc2::class!(CALayer), layer];
            let _: () = msg_send![&*ring, setName: &*NSString::from_str(NAME)];
            // Above Chromium's layers, and resized with the page.
            let _: () = msg_send![&*ring, setZPosition: 1000.0_f64];
            let _: () = msg_send![&*ring, setAutoresizingMask: 2_u32 | 16_u32];
            let curve = objc2_foundation::ns_string!("continuous");
            let _: () = msg_send![&*ring, setCornerCurve: curve];
            let _: () = msg_send![&*host, addSublayer: &*ring];
            ring
        });
        let bounds: objc2_foundation::NSRect = msg_send![&*host, bounds];
        let radius: f64 = msg_send![&*host, cornerRadius];
        let frame = objc2_foundation::NSRect::new(
            objc2_foundation::NSPoint::new(bounds.origin.x + INSET, bounds.origin.y + INSET),
            objc2_foundation::NSSize::new(
                bounds.size.width - 2.0 * INSET,
                bounds.size.height - 2.0 * INSET,
            ),
        );
        let _: () = msg_send![&*ring, setFrame: frame];
        let _: () =
            msg_send![&*ring, setCornerRadius: (visible_corner(view, radius) - INSET).max(0.0)];
        // Crisp on every screen: the layer draws at the window's pixel density.
        let scale = view.window().map_or(1.0, |w| w.backingScaleFactor());
        let _: () = msg_send![&*ring, setContentsScale: scale];
        let channel = |shift: u32| f64::from((c >> shift) & 0xFF) / 255.0;
        let color = objc2_core_graphics::CGColor::new_srgb(
            channel(16),
            channel(8),
            channel(0),
            channel(24),
        );
        let _: () = msg_send![&*ring, setBorderColor: &*color];
        let _: () = msg_send![&*ring, setBorderWidth: WIDTH];
    }
}

/// A borderless, transparent window for one of our child windows over the
/// frame: kept alive by us (not released on close), not in the Window menu,
/// with a clear background and a shadow only if `shadow`.
fn borderless_window(mtm: MainThreadMarker, rect: NSRect, shadow: bool) -> Retained<NSWindow> {
    use objc2_app_kit::{NSBackingStoreType, NSWindowStyleMask};
    // SAFETY: plain borderless window creation on the main thread.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: we keep the window alive ourselves.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setOpaque(false);
    window.setHasShadow(shadow);
    window.setExcludedFromWindowsMenu(true);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window
}

/// `at` (CEF screen coordinates: top-left origin, DIP) in AppKit's screen
/// coordinates, whose origin is the bottom-left of the primary screen.
/// `None` while there is no screen.
fn screen_rect(mtm: MainThreadMarker, at: &cef::Rect) -> Option<NSRect> {
    use objc2_app_kit::NSScreen;
    use objc2_foundation::{NSPoint, NSSize};
    let primary = NSScreen::screens(mtm).firstObject()?;
    let (w, h) = (f64::from(at.width), f64::from(at.height));
    let y = primary.frame().size.height - f64::from(at.y) - h;
    Some(NSRect::new(
        NSPoint::new(f64::from(at.x), y),
        NSSize::new(w, h),
    ))
}

/// (Re-)attach `child` to `frame` as its topmost child window, which orders
/// it in.
fn attach_above(frame: &NSWindow, child: &NSWindow) {
    use objc2_app_kit::NSWindowOrderingMode;
    frame.removeChildWindow(child);
    // SAFETY: both windows are alive; ordering a child above its parent.
    unsafe { frame.addChildWindow_ordered(child, NSWindowOrderingMode::Above) };
}

/// Animate `window`'s opacity to `alpha` over `seconds`.
fn fade(window: &NSWindow, alpha: f64, seconds: f64) {
    use objc2_app_kit::{NSAnimatablePropertyContainer, NSAnimationContext};
    NSAnimationContext::beginGrouping();
    NSAnimationContext::currentContext().setDuration(seconds);
    window.animator().setAlphaValue(alpha);
    NSAnimationContext::endGrouping();
}

/// A plain box (no title, margins or border) with corner `radius` and filled
/// with `fill`: the shapes behind our native buttons, dims and grips. Also
/// used by the picture-in-picture panel (`pip.rs`).
pub(crate) fn plain_box(mtm: MainThreadMarker, radius: f64, fill: &NSColor) -> Retained<NSBox> {
    use objc2_app_kit::{NSBoxType, NSTitlePosition};
    use objc2_foundation::NSSize;
    // SAFETY: initWithFrame: is NSBox's designated initialiser.
    let b: Retained<NSBox> = unsafe { msg_send![NSBox::alloc(mtm), initWithFrame: NSRect::ZERO] };
    b.setBoxType(NSBoxType::Custom);
    b.setTitlePosition(NSTitlePosition::NoTitle);
    b.setContentViewMargins(NSSize::new(0.0, 0.0));
    b.setCornerRadius(radius);
    b.setFillColor(fill);
    b.setBorderWidth(0.0);
    b
}

/// Where the pointer is, in CEF screen coordinates (top-left origin, DIP).
pub fn mouse_point() -> cef::Point {
    use objc2_app_kit::NSScreen;
    let at = NSEvent::mouseLocation();
    let height = MainThreadMarker::new()
        .and_then(|mtm| NSScreen::screens(mtm).firstObject())
        .map_or(0.0, |s| s.frame().size.height);
    // Screen points fit in i32.
    #[allow(clippy::cast_possible_truncation)]
    cef::Point {
        x: at.x.round() as i32,
        y: (height - at.y).round() as i32,
    }
}

/// Make the window holding the view `child` the topmost child window of the
/// one holding `frame` (a mini window's page window that became a tab's).
pub fn attach_child(frame: cef::sys::cef_window_handle_t, child: cef::sys::cef_window_handle_t) {
    use objc2_app_kit::NSView;
    if frame.is_null() || child.is_null() || MainThreadMarker::new().is_none() {
        return;
    }
    // SAFETY: CEF hands us live content NSViews; we are on the main thread.
    let (frame, child): (&NSView, &NSView) =
        unsafe { (&*frame.cast::<NSView>(), &*child.cast::<NSView>()) };
    if let (Some(parent), Some(window)) = (frame.window(), child.window())
        && window.parentWindow().as_ref() != Some(&parent)
    {
        attach_above(&parent, &window);
    }
}

/// Whether Magnetite is the app in front.
pub fn app_is_active() -> bool {
    MainThreadMarker::new().is_some_and(|mtm| NSApplication::sharedApplication(mtm).isActive())
}

/// Hand the keyboard back to the app that was in front before Magnetite.
pub fn deactivate_app() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).deactivate();
    }
}
