//! macOS glue: what CEF does not do for us on the Mac.
//!
//! This file holds the `NSApplication` subclass that implements CEF's
//! protocols and the app delegate (links from other apps, reopen, terminate;
//! it installs our menu bar, `menubar.rs`), adapted from the cef-rs
//! `cefsimple` example (Apache-2.0 OR MIT). Next to them: tweaks to CEF's
//! own windows (pinned, rounded corners, the focus ring of a split page,
//! hidden or shown, shown without taking focus, focused without AppKit's
//! key view loop), the watch on two-finger
//! swipes, stale tooltips, pages wrongly marked covered, the cursor of a
//! window without the keyboard, and helpers for the
//! child modules:
//!
//! - `chrome`: our child windows over the page area (drag strip, peek
//!   backdrop, the hidden sidebar's edge, split dividers);
//! - `default_browser`: whether Magnetite opens links from other apps;
//! - `hotkey`: the system-wide quick search key;
//! - `pill`: the link-hover status pill;
//! - `pressure`: the system's memory pressure signal;
//! - `quit`: Cmd+Q quits on the second press;
//! - `share`: the share menu, the pasteboard and the save panel;
//! - `webkit`: pages in `WebKit`, the system's own web engine.
//!
//! Everything runs on the main thread (CEF's UI thread on macOS). CEF's
//! screen rectangles have a top-left origin; `screen_rect` turns them into
//! AppKit's, whose origin is the primary screen's bottom-left.

mod chrome;
mod default_browser;
mod hotkey;
mod pill;
mod pressure;
mod quit;
mod share;
pub(crate) mod webkit;

pub use chrome::{
    DividerKey, hide_peek_backdrop, set_dividers, set_edge_zone, set_grab_strip, set_panel_close,
    show_peek_backdrop,
};
pub use default_browser::{handles, set_handler};
pub use hotkey::set_quick_search_hotkey;
pub use pill::{hide_status_pill, set_status_pill};
pub use pressure::watch_memory_pressure;
pub use share::{copy_file, copy_png, copy_text, save_png, share_png, show_share_picker};

use crate::engine::Engine;
use crate::events::EngineEvent;
use cef::application_mac::{CefAppProtocol, CrAppControlProtocol, CrAppProtocol};
use objc2::{
    ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, extern_methods,
    msg_send,
    rc::Retained,
    runtime::{AnyObject, Bool, NSObject, NSObjectProtocol, ProtocolObject, Sel},
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
        /// already running: the window shows, also after its close hid it.
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        unsafe fn application_should_handle_reopen(&self, _sender: &NSApplication, _has_visible_windows: Bool) -> Bool {
            Engine::global().show_frame();
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
        unsafe fn terminate(&self, _sender: Option<&AnyObject>) {
            let engine = Engine::global();
            if engine.is_quitting() { return; }
            // Cmd+Q counts on the second press only.
            if !quit::confirmed(self.mtm()) { return; }
            engine.emit(EngineEvent::QuitRequested);
        }

        /// The window with the keyboard, as accessibility sees it. That is
        /// mostly a page, which is a part of the frame (`mark_as_part`):
        /// tools that move or resize "the focused window" get the frame.
        #[unsafe(method_id(accessibilityFocusedWindow))]
        unsafe fn accessibility_focused_window(&self) -> Option<Retained<AnyObject>> {
            let focused: Option<Retained<AnyObject>> =
                msg_send![super(self), accessibilityFocusedWindow];
            focused.map(|focused| match focused.downcast::<NSWindow>() {
                Ok(window) => whole_window(window).into(),
                Err(other) => other,
            })
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
    mark_as_part(handle);
}

/// Tell accessibility that the window holding the view `handle` is a part of
/// the window it is a child of, not a window of its own:
///
/// - AppKit calls a frameless window that can take the keyboard a dialog,
///   which window switchers (`AltTab`) list next to the frame: one more
///   "Magnetite" for every loaded page. Hence the subrole.
/// - Tools that move or resize "the window under the pointer" walk up from
///   the element there to the first `AXWindow` and set its position. As a
///   group whose parent is the frame, a page leads them to the frame; as a
///   window it was moved out of the frame alone.
pub fn mark_as_part(handle: cef::sys::cef_window_handle_t) {
    if handle.is_null() {
        return;
    }
    // SAFETY: CEF hands us a live NSView*.
    let view: &objc2_app_kit::NSView = unsafe { &*handle.cast() };
    if let Some(window) = view.window() {
        mark_window_as_part(&window);
    }
}

/// `mark_as_part` for the window itself.
fn mark_window_as_part(window: &NSWindow) {
    let subrole = objc2_foundation::ns_string!("AXUnknown");
    teach_windows_parts();
    // SAFETY: `setAccessibilitySubrole:` takes an NSString; the mark is a
    // static string kept under a key of our own.
    unsafe {
        let _: () = msg_send![window, setAccessibilitySubrole: subrole];
        objc2::ffi::objc_setAssociatedObject(
            std::ptr::from_ref::<NSWindow>(window).cast_mut().cast(),
            part_key(),
            std::ptr::from_ref(part_role()).cast_mut().cast(),
            objc2::ffi::OBJC_ASSOCIATION_RETAIN_NONATOMIC,
        );
    }
}

/// The accessibility role of a window marked by `mark_as_part`.
fn part_role() -> &'static objc2_foundation::NSString {
    objc2_foundation::ns_string!("AXGroup")
}

/// The key a window's mark is kept under (its address is the key).
fn part_key() -> *const std::ffi::c_void {
    static KEY: u8 = 0;
    std::ptr::from_ref(&KEY).cast()
}

/// The window that `window` is a part of: its parent, if `mark_as_part` was
/// called for it. `None` too while a marked window has no parent (hidden, or
/// shown on its own as a detached overlay is): it is a window then.
fn whole_of(window: &NSWindow) -> Option<Retained<NSWindow>> {
    // SAFETY: reads what `mark_window_as_part` stored, or null.
    let mark = unsafe {
        objc2::ffi::objc_getAssociatedObject(std::ptr::from_ref(window).cast(), part_key())
    };
    if mark.is_null() {
        return None;
    }
    window.parentWindow()
}

type RoleImp = unsafe extern "C-unwind" fn(&NSWindow, Sel) -> *const objc2_foundation::NSString;
type ParentImp = unsafe extern "C-unwind" fn(&NSWindow, Sel) -> *mut AnyObject;

/// AppKit's own answers, which unmarked windows keep.
static WINDOW_ACCESSIBILITY: std::sync::OnceLock<(RoleImp, ParentImp)> = std::sync::OnceLock::new();

/// Make every `NSWindow` answer accessibility's role and parent as a part of
/// its parent window while it is one (`whole_of`); once per process.
///
/// On `NSWindow` and by a mark, not by AppKit's setters or a subclass of the
/// window's class: the parent has to be asked for each time (a page window
/// has none when it is created and gets another when it changes windows),
/// and Chromium's windows are observed, which rules out giving them a class.
fn teach_windows_parts() {
    use objc2::ffi::{class_getInstanceMethod, class_replaceMethod, method_getTypeEncoding};
    use objc2::runtime::Imp;

    extern "C-unwind" fn role(this: &NSWindow, cmd: Sel) -> *const objc2_foundation::NSString {
        if whole_of(this).is_some() {
            return part_role();
        }
        let (original, _) = WINDOW_ACCESSIBILITY
            .get()
            .expect("kept before the replacements are installed");
        // SAFETY: AppKit's method, called as AppKit would.
        unsafe { original(this, cmd) }
    }
    extern "C-unwind" fn parent(this: &NSWindow, cmd: Sel) -> *mut AnyObject {
        if let Some(whole) = whole_of(this) {
            return Retained::autorelease_ptr(whole).cast();
        }
        let (_, original) = WINDOW_ACCESSIBILITY
            .get()
            .expect("kept before the replacements are installed");
        // SAFETY: AppKit's method, called as AppKit would.
        unsafe { original(this, cmd) }
    }

    static INSTALLED: std::sync::Once = std::sync::Once::new();
    let class = NSWindow::class();
    let (role_sel, parent_sel) = (sel!(accessibilityRole), sel!(accessibilityParent));
    // AppKit's answers are kept before either replacement is in place: a
    // replacement may be called the moment it is.
    WINDOW_ACCESSIBILITY.get_or_init(|| {
        let implementation = |sel: Sel| {
            class
                .instance_method(sel)
                .expect("NSWindow answers the accessibility protocol")
                .implementation()
        };
        // SAFETY: these are the implementations of the two getters.
        unsafe {
            (
                std::mem::transmute::<Imp, RoleImp>(implementation(role_sel)),
                std::mem::transmute::<Imp, ParentImp>(implementation(parent_sel)),
            )
        }
    });
    // SAFETY: the replacements have the types of the getters they replace,
    // and are installed on `NSWindow` itself even where it only inherits
    // the method.
    INSTALLED.call_once(|| unsafe {
        let class = std::ptr::from_ref(class).cast_mut();
        for (sel, imp) in [
            (role_sel, std::mem::transmute::<RoleImp, Imp>(role)),
            (parent_sel, std::mem::transmute::<ParentImp, Imp>(parent)),
        ] {
            let types = method_getTypeEncoding(class_getInstanceMethod(class, sel));
            class_replaceMethod(class, sel, imp, types);
        }
    });
}

/// `window`, or the window it is a part of (`mark_as_part`), however deep.
fn whole_window(window: Retained<NSWindow>) -> Retained<NSWindow> {
    let mut whole = window;
    while let Some(parent) = whole_of(&whole) {
        whole = parent;
    }
    whole
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
    set_corner_radius(handle, radius, [false; 4]);
}

/// What `topCornerRounded` and `bottomCornerRounded` of the frame views of
/// CEF's windows answered before `corner_rounded` took their place.
static CORNERS_ROUNDED: std::sync::OnceLock<[objc2::runtime::Imp; 2]> = std::sync::OnceLock::new();

type CornerRounded = unsafe extern "C-unwind" fn(&AnyObject, Sel) -> Bool;

/// `topCornerRounded` and `bottomCornerRounded` of a window's frame view:
/// no for the window of a square page (`set_system_corner`), else what the
/// class answered before.
extern "C-unwind" fn corner_rounded(frame: &AnyObject, cmd: Sel) -> Bool {
    // SAFETY: reads what `set_system_corner` stored, or null.
    let square = unsafe {
        objc2::ffi::objc_getAssociatedObject(std::ptr::from_ref(frame).cast(), square_key())
    };
    if !square.is_null() {
        return Bool::NO;
    }
    let Some([top, bottom]) = CORNERS_ROUNDED.get() else {
        return Bool::YES;
    };
    let before = if cmd == sel!(topCornerRounded) {
        top
    } else {
        bottom
    };
    // SAFETY: the implementation this one took the place of, with its type.
    unsafe { std::mem::transmute::<objc2::runtime::Imp, CornerRounded>(*before)(frame, cmd) }
}

/// The key a square window's mark is kept under (its address is the key).
fn square_key() -> *const std::ffi::c_void {
    static KEY: u8 = 0;
    std::ptr::from_ref(&KEY).cast()
}

/// Make the class of `frame` (the frame view of one of CEF's windows) ask
/// `corner_rounded` whether its corners are round. False if that cannot be
/// done; the windows keep the system's corners then.
fn teach_frames_corners(frame: &objc2_app_kit::NSView) -> bool {
    use objc2::runtime::{AnyClass, Imp};
    let ours = corner_rounded as CornerRounded;
    // SAFETY: same calling convention; called only through the runtime.
    let ours: Imp = unsafe { std::mem::transmute::<CornerRounded, Imp>(ours) };
    // SAFETY: `class` returns the class the view was made with, not the one
    // key-value observing put in front of it.
    let class: &AnyClass = unsafe { msg_send![frame, class] };
    let sels = [sel!(topCornerRounded), sel!(bottomCornerRounded)];
    // Not public API: without both, and the message that makes AppKit ask
    // them again, there is nothing to do.
    let [Some(top), Some(bottom)] = sels.map(|sel| class.instance_method(sel)) else {
        return false;
    };
    if class
        .instance_method(sel!(windowCornerMaskChanged))
        .is_none()
    {
        return false;
    }
    let before = [top.implementation(), bottom.implementation()];
    if before.iter().all(|imp| std::ptr::fn_addr_eq(*imp, ours)) {
        return true;
    }
    // One class of frame views only: what it answered before is kept once.
    let kept = CORNERS_ROUNDED.get_or_init(|| before);
    if !std::iter::zip(kept, &before).all(|(a, b)| std::ptr::fn_addr_eq(*a, *b)) {
        crate::trace("square page: a second frame view class, left as it is");
        return false;
    }
    // SAFETY: adds methods with the types of the inherited ones to a class
    // that does not define them itself; the main thread, where AppKit sends
    // them.
    let added = [(sels[0], top), (sels[1], bottom)].map(|(sel, method)| unsafe {
        objc2::ffi::class_addMethod(
            std::ptr::from_ref(class).cast_mut(),
            sel,
            ours,
            objc2::ffi::method_getTypeEncoding(method),
        )
        .as_bool()
    });
    added == [true, true]
}

/// macOS rounds a page's window itself (16 pt since macOS 26), whatever its
/// layer says: take that corner away for a square page, or give it back.
/// The window's frame view decides it and there is no public API to tell it
/// otherwise, so where the frame view does not answer as expected the page
/// keeps the system's corners.
fn set_system_corner(window: &NSWindow, round: bool) {
    // SAFETY: the frame view of a live window, on the main thread.
    let Some(frame) = window.contentView().and_then(|c| unsafe { c.superview() }) else {
        return;
    };
    let key = square_key();
    // SAFETY: a mark under a key of our own on a live view: a static
    // string, or none.
    unsafe {
        let object: *mut AnyObject = Retained::as_ptr(&frame).cast_mut().cast();
        let marked = !objc2::ffi::objc_getAssociatedObject(object.cast_const(), key).is_null();
        if marked != round || (!round && !teach_frames_corners(&frame)) {
            return;
        }
        let mark = if round {
            std::ptr::null_mut()
        } else {
            std::ptr::from_ref(part_role()).cast_mut().cast()
        };
        objc2::ffi::objc_setAssociatedObject(
            object,
            key,
            mark,
            objc2::ffi::OBJC_ASSOCIATION_RETAIN_NONATOMIC,
        );
        // AppKit asks the frame view about its corners only when told
        // that they changed (`teach_frames_corners` saw that it can be).
        let _: () = msg_send![&*frame, windowCornerMaskChanged];
    }
}

/// Change the corner radius of a page window rounded before
/// (`round_window_corners`), and of a system page's own window over it.
/// `handle` is the window's content `NSView`. 0 makes it square, but for
/// its corners that lie in a corner of the frame window (`in_window_corner`:
/// top left, top right, bottom left, bottom right): those follow the frame
/// window's own round corners, which a page would stand out of.
pub fn set_corner_radius(
    handle: cef::sys::cef_window_handle_t,
    radius: f64,
    in_window_corner: [bool; 4],
) {
    use objc2_app_kit::NSView;
    if handle.is_null() {
        return;
    }
    // SAFETY: CEF hands us a live NSView of a page window; main thread.
    let view: &NSView = unsafe { &*handle.cast::<NSView>() };
    let Some(window) = view.window() else { return };
    set_system_corner(&window, radius > 0.0);
    // `CACornerMask`, in a view's layer: the lower corners come first.
    let (radius, corners) = if radius > 0.0 {
        (radius, 0b1111_usize)
    } else {
        let [top_left, top_right, bottom_left, bottom_right] = in_window_corner;
        let corners = [bottom_left, bottom_right, top_left, top_right];
        let mask = (corners.iter().enumerate())
            .fold(0, |mask, (bit, on)| mask | (usize::from(*on) << bit));
        (system_corner(&root_window(&window)), mask)
    };
    for content in [window.contentView(), webkit::holder_in(&window)] {
        // SAFETY: CoreAnimation's setters on the layer of a view alive here.
        unsafe {
            let layer: Option<Retained<AnyObject>> =
                content.and_then(|content| msg_send![&*content, layer]);
            if let Some(layer) = layer {
                let _: () = msg_send![&*layer, setCornerRadius: radius];
                let _: () = msg_send![&*layer, setMaskedCorners: corners];
            }
        }
    }
}

/// The corner radius macOS gives `window` itself, 0 if it does not say:
/// there is no public API, so it is read only if the window answers
/// `_cornerRadius`.
fn system_corner(window: &NSWindow) -> f64 {
    let sel = sel!(_cornerRadius);
    // SAFETY: asked only after the window says it implements the getter,
    // which returns a CGFloat.
    unsafe {
        let has: bool = msg_send![window, respondsToSelector: sel];
        if has {
            msg_send![window, _cornerRadius]
        } else {
            0.0
        }
    }
}

/// Watches two-finger swipes, once, on the UI (main) thread:
///
/// - each one that begins is reported to the sidebars (`Engine::swipe_began`),
///   which cannot tell a new swipe from the momentum of the last one;
/// - on the floating video window (picture-in-picture) a swipe flings it to
///   the screen edge in the swipe's direction; diagonal swipes reach a
///   corner. Quicker than dragging.
pub fn install_swipe_monitor() {
    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventPhase};
    use std::ptr::NonNull;

    let handler = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit passes a live event for the duration of the call.
        let ev = unsafe { event.as_ref() };
        if ev.phase().contains(NSEventPhase::Began) {
            // Not from inside an AppKit callback.
            Engine::post(Box::new(|| Engine::global().swipe_began()));
        }
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

/// A window that loses the keyboard loses the tooltips of its pages.
/// Chromium takes a page's tooltip changes only while its window has the
/// keyboard and drops the rest, also the change that takes a tooltip away.
/// A tooltip set just before (by the click in the floating sidebar that
/// shows a tab, say) stayed for good, and showed wherever the pointer
/// rested on that page. Installed once on the UI (main) thread.
pub fn clear_tooltips_without_keyboard() {
    use block2::RcBlock;
    use objc2_app_kit::{NSWindow, NSWindowDidResignKeyNotification};
    use objc2_foundation::{NSNotification, NSNotificationCenter};
    use std::ptr::NonNull;

    let resigned = RcBlock::new(|notification: NonNull<NSNotification>| {
        // SAFETY: AppKit passes a live notification for the duration of the call.
        let window = unsafe { notification.as_ref() }
            .object()
            .and_then(|o| o.downcast::<NSWindow>().ok());
        if let Some(root) = window.and_then(|w| w.contentView()) {
            clear_tooltips(&root);
        }
    });
    // SAFETY: an AppKit notification name; the block runs on the main thread,
    // where windows post it.
    let observer = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSWindowDidResignKeyNotification),
            None,
            None,
            &resigned,
        )
    };
    // The observer lives as long as the app.
    std::mem::forget(observer);
}

/// Take the tooltip off every Chromium page view below `view`.
fn clear_tooltips(view: &objc2_app_kit::NSView) {
    use objc2_foundation::NSString;
    // Chromium's own way to set a page view's tooltip (`ToolTipBaseView`).
    if view.respondsToSelector(sel!(setToolTipAtMousePoint:)) {
        let none: Option<&NSString> = None;
        // SAFETY: the view implements the selector; it takes a nullable string.
        let _: () = unsafe { msg_send![view, setToolTipAtMousePoint: none] };
    }
    for child in view.subviews() {
        clear_tooltips(&child);
    }
}

/// Chromium marks a window "covered" (`WebContentsOcclusionCheckerMac`) and
/// treats its page as hidden: nothing is drawn, no input is taken. Its check
/// judges our page windows (child windows of the frame) one by one from what
/// macOS reports for each, and those reports lag the frame's. A mark that
/// stays on a page on screen leaves it dead until its tab is shown again.
///
/// Take the mark off the page window whose content view is `handle` if it
/// cannot be right: the window is on screen above a frame macOS reports as
/// visible, and macOS reports the page as visible too or Magnetite is the app
/// in front. True if there was a mark to take off.
pub fn uncover_page(handle: cef::sys::cef_window_handle_t) -> bool {
    use objc2_app_kit::{NSView, NSWindowOcclusionState};
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    if handle.is_null() {
        return false;
    }
    // SAFETY: CEF hands us a live NSView; we are on the main thread.
    let view: &NSView = unsafe { &*handle.cast::<NSView>() };
    let Some(window) = view.window() else {
        return false;
    };
    let Some(frame) = window.parentWindow() else {
        return false;
    };
    let seen = |w: &NSWindow| w.occlusionState().contains(NSWindowOcclusionState::Visible);
    if !window.isVisible() || !seen(&frame) {
        return false;
    }
    if !seen(&window) && !NSApplication::sharedApplication(mtm).isActive() {
        return false;
    }
    // Chromium's own accessors for the mark (a category on NSWindow); the
    // setter tells the page's view, which shows the page again.
    if !window.respondsToSelector(sel!(isOccluded))
        || !window.respondsToSelector(sel!(setOccluded:))
    {
        return false;
    }
    // SAFETY: the window implements both selectors, a BOOL getter and setter.
    unsafe {
        let covered: Bool = msg_send![&*window, isOccluded];
        if !covered.as_bool() {
            return false;
        }
        let _: () = msg_send![&*window, setOccluded: Bool::NO];
    }
    true
}

/// Look for pages wrongly marked covered (`uncover_page`) whenever a window's
/// occlusion changes or Magnetite comes to the front. Installed once on the
/// UI (main) thread; the housekeeping poll looks too, for a mark that came
/// without either.
pub fn watch_covered_pages() {
    use block2::RcBlock;
    use objc2_app_kit::{
        NSApplicationDidBecomeActiveNotification, NSWindowDidChangeOcclusionStateNotification,
    };
    use objc2_foundation::{NSNotification, NSNotificationCenter};
    use std::ptr::NonNull;

    thread_local! {
        /// A look is already on its way: notifications come in bursts.
        static PENDING: Cell<bool> = const { Cell::new(false) };
    }
    let changed = RcBlock::new(|_: NonNull<NSNotification>| {
        if PENDING.replace(true) {
            return;
        }
        // Not from inside the notification: taking a mark off posts one too.
        Engine::post(Box::new(|| {
            PENDING.set(false);
            Engine::global().uncover_shown_pages();
        }));
    });
    // SAFETY: AppKit notification names; the block runs on the main thread,
    // where windows and the application post them.
    for name in unsafe {
        [
            NSWindowDidChangeOcclusionStateNotification,
            NSApplicationDidBecomeActiveNotification,
        ]
    } {
        let observer = unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(name),
                None,
                None,
                &changed,
            )
        };
        // The observer lives as long as the app.
        std::mem::forget(observer);
    }
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

/// Keep AppKit from choosing the page's focus when the window holding the
/// view `handle` is shown. A window shown without a first responder (its
/// page view went away with a navigation, or it never had the keyboard)
/// gets one from its key view loop, "as if by Tab", and Chromium answers by
/// focusing the page's first element: most sites then show their "Skip to
/// content" link. An initial first responder stops that walk. The content
/// view (the Views root) declines the role, so the window has none until
/// Views focuses the page directly (`request_focus`).
pub fn no_key_loop_focus(handle: cef::sys::cef_window_handle_t) {
    use objc2_app_kit::NSView;
    if handle.is_null() || MainThreadMarker::new().is_none() {
        return;
    }
    // SAFETY: CEF hands us a live NSView; we are on the main thread.
    let view: &NSView = unsafe { &*handle.cast::<NSView>() };
    if let Some(window) = view.window() {
        window.setInitialFirstResponder(window.contentView().as_deref());
    }
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

thread_local! {
    /// The windows `hover_without_focus` was asked for, by address.
    static HOVER_WINDOWS: std::cell::RefCell<std::collections::HashSet<usize>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// `isMainWindow` as CEF's window class had it before `hover_without_focus`.
static IS_MAIN_WINDOW: std::sync::OnceLock<objc2::runtime::Imp> = std::sync::OnceLock::new();

type IsMainWindow = unsafe extern "C-unwind" fn(&AnyObject, objc2::runtime::Sel) -> Bool;

/// `isMainWindow` of CEF's windows: yes for the windows of
/// `hover_without_focus`, else what the class answered before.
extern "C-unwind" fn main_for_the_pointer(window: &AnyObject, cmd: objc2::runtime::Sel) -> Bool {
    if HOVER_WINDOWS.with(|w| w.borrow().contains(&std::ptr::from_ref(window).addr())) {
        return Bool::YES;
    }
    let Some(before) = IS_MAIN_WINDOW.get() else {
        return Bool::NO;
    };
    // SAFETY: the implementation this one took the place of, with its type.
    unsafe { std::mem::transmute::<objc2::runtime::Imp, IsMainWindow>(*before)(window, cmd) }
}

/// Let the window holding the view `handle` take pointer movement while
/// another window has the keyboard. Chromium drops mouse moves in a window
/// that is neither key nor main and tells its page the pointer left
/// (`-[RenderWidgetHostViewCocoa shouldIgnoreMouseEvent:]`): a panel shown
/// `without_taking_focus` then shows no hover, and the floating sidebar,
/// which leaves with the pointer, hides on the first move.
///
/// From now on the window answers `isMainWindow` with yes. Which window is
/// key or main for AppKit does not change. The answer comes from a method
/// added to the window's class (CEF's, which inherits AppKit's); the window
/// cannot get a class of its own, since key-value observing already gave it
/// one.
pub fn hover_without_focus(handle: cef::sys::cef_window_handle_t) {
    use objc2::runtime::{AnyClass, Imp};
    use objc2_app_kit::NSView;
    if handle.is_null() || MainThreadMarker::new().is_none() {
        return;
    }
    // SAFETY: CEF hands us a live NSView; we are on the main thread.
    let view: &NSView = unsafe { &*handle.cast::<NSView>() };
    let Some(window) = view.window() else {
        return;
    };
    let ours = main_for_the_pointer as IsMainWindow;
    // SAFETY: same calling convention; called only through the runtime.
    let ours: Imp = unsafe { std::mem::transmute::<IsMainWindow, Imp>(ours) };
    // SAFETY: `class` returns the class the window was made with, not the
    // one key-value observing put in front of it.
    let class: &AnyClass = unsafe { msg_send![&*window, class] };
    let Some(method) = class.instance_method(sel!(isMainWindow)) else {
        return;
    };
    let before = method.implementation();
    if !std::ptr::fn_addr_eq(before, ours) {
        // One class of windows only: what it answered before is kept once.
        if !std::ptr::fn_addr_eq(*IS_MAIN_WINDOW.get_or_init(|| before), before) {
            crate::trace("hover without focus: a second window class, left as it is");
            return;
        }
        // SAFETY: adds a method with the type of the inherited one to a
        // class that does not define it itself; the main thread, where
        // AppKit sends it.
        let added = unsafe {
            objc2::ffi::class_addMethod(
                std::ptr::from_ref(class).cast_mut(),
                sel!(isMainWindow),
                ours,
                objc2::ffi::method_getTypeEncoding(method),
            )
        };
        if !added.as_bool() {
            crate::trace("hover without focus: the window class has its own isMainWindow");
            return;
        }
    }
    HOVER_WINDOWS.with(|w| w.borrow_mut().insert(Retained::as_ptr(&window).addr()));
}

/// Show `cursor` (the `NSCursor` a page of ours asked for) although the
/// window holding the view `handle` does not have the keyboard. Chromium
/// sets a page's cursor only in the key window
/// (`-[RenderWidgetHostViewCocoa shouldChangeCurrentCursor]`), and with the
/// keyboard in a page that is the page's window: the sidebar still showed
/// its hover, but never the resize arrows on its edge or the hand on a row.
///
/// Only while the keyboard is in a window of the same family (the frame, its
/// pages and panels) and this window is the one under the pointer; Chromium
/// keeps doing the rest, also the arrow when the pointer leaves the view.
///
/// # Safety
///
/// `handle` is null or a live `NSView`, and `cursor` is null or a live
/// `NSCursor`, both for the length of the call.
pub unsafe fn show_cursor_without_focus(handle: cef::sys::cef_window_handle_t, cursor: *mut u8) {
    use objc2_app_kit::{NSCursor, NSView, NSWindow};
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if handle.is_null() || cursor.is_null() {
        return;
    }
    // SAFETY: the caller passes a live NSView; we are on the main thread.
    let view: &NSView = unsafe { &*handle.cast::<NSView>() };
    let (Some(window), Some(key)) = (
        view.window(),
        NSApplication::sharedApplication(mtm).keyWindow(),
    ) else {
        return;
    };
    if window.isKeyWindow() || root_window(&window) != root_window(&key) {
        return;
    }
    let under_pointer =
        NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(NSEvent::mouseLocation(), 0, mtm);
    if under_pointer != window.windowNumber() {
        return;
    }
    // SAFETY: the caller passes a live `NSCursor`.
    let cursor: &NSCursor = unsafe { &*cursor.cast::<NSCursor>() };
    cursor.set();
}

/// The window `window` is attached to, through all its parents.
pub(crate) fn root_window(window: &objc2_app_kit::NSWindow) -> Retained<objc2_app_kit::NSWindow> {
    use objc2::Message as _;
    let mut root = window.retain();
    while let Some(parent) = root.parentWindow() {
        root = parent;
    }
    root
}

/// The corner radius a page window shows: macOS rounds titled windows itself
/// (16 pt since macOS 26) and that clips more than our layer's `own` radius
/// when it is larger. The window's radius has no public API, so it is read
/// only if the window answers `_cornerRadius`; otherwise ours is assumed.
fn visible_corner(view: &objc2_app_kit::NSView, own: f64) -> f64 {
    let Some(window) = view.window() else {
        return own;
    };
    // A square page's window still names the system's radius.
    // SAFETY: the frame view of a live window; reads our own mark, or null.
    let square = (window.contentView().and_then(|c| unsafe { c.superview() })).is_some_and(|f| {
        let frame: *const AnyObject = Retained::as_ptr(&f).cast();
        !unsafe { objc2::ffi::objc_getAssociatedObject(frame, square_key()) }.is_null()
    });
    if square {
        return own;
    }
    let system = system_corner(&window);
    own.max(system)
}

/// Outline the page window whose content view is `handle` in `argb` (the
/// focused page of a split), or remove the outline (`None`). The outline is
/// its own layer on top of the page, along its very edge and with the corner
/// the page really shows (`visible_corner`): no rim of the page shows outside
/// it, and the curve matches the page's.
pub fn set_focus_ring(handle: cef::sys::cef_window_handle_t, argb: Option<u32>) {
    use objc2_app_kit::NSView;
    use objc2_foundation::{NSArray, NSString};
    const NAME: &str = "magnetite-focus-ring";
    const WIDTH: f64 = 2.0;
    if handle.is_null() {
        return;
    }
    // SAFETY: CEF hands us a live NSView of a page window; main thread.
    let view: &NSView = unsafe { &*handle.cast::<NSView>() };
    // A system page's own view lies over the content view: the outline is
    // drawn on what holds it.
    let Some(content) = view
        .window()
        .and_then(|w| webkit::holder_in(&w).or_else(|| w.contentView()))
    else {
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
        let corners: usize = msg_send![&*host, maskedCorners];
        let _: () = msg_send![&*ring, setFrame: bounds];
        let _: () = msg_send![&*ring, setCornerRadius: visible_corner(view, radius)];
        let _: () = msg_send![&*ring, setMaskedCorners: corners];
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
    mark_window_as_part(&window);
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
        // A page that moves here from another of our windows leaves that one.
        // TODO(macos): untested (see `cef_app::can_reparent`).
        if let Some(before) = window.parentWindow() {
            before.removeChildWindow(&window);
        }
        attach_above(&parent, &window);
    }
}

/// Whether Magnetite is the app in front.
pub fn app_is_active() -> bool {
    MainThreadMarker::new().is_some_and(|mtm| NSApplication::sharedApplication(mtm).isActive())
}

/// Whether those of Cmd, Control and Option that are asked for are all down
/// right now, whichever app has the keyboard (the keyboard's state, not
/// that of the last event).
pub fn modifiers_held(command: bool, control: bool, option: bool) -> bool {
    use objc2_app_kit::NSEventModifierFlags as Flags;
    let mut wanted = Flags::empty();
    for (asked, flag) in [
        (command, Flags::Command),
        (control, Flags::Control),
        (option, Flags::Option),
    ] {
        if asked {
            wanted |= flag;
        }
    }
    NSEvent::modifierFlags_class().contains(wanted)
}

/// Hand the keyboard back to the app that was in front before Magnetite.
pub fn deactivate_app() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).deactivate();
    }
}
