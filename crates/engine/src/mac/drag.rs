//! Files out of our pages and into the trash: a finished download dragged
//! from the list as Finder drags a file (the same session Finder starts, so
//! web pages, other apps and folders take it), and moved to the Trash.

use std::cell::OnceCell;
use std::path::Path;

use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSDragOperation, NSDraggingContext, NSDraggingItem, NSDraggingSession,
    NSDraggingSource, NSEvent, NSEventModifierFlags, NSEventType, NSView, NSWorkspace,
};
use objc2_foundation::{NSArray, NSFileManager, NSPoint, NSRect, NSSize, NSString, NSURL};

/// Side of the file's icon under the pointer while it is dragged, in points.
const ICON: f64 = 48.0;

define_class!(
    /// Says what a file dragged out of a page may become: a copy, wherever
    /// it lands (a page uploads it, Finder copies it).
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    struct MagnetiteFileDragSource;

    unsafe impl NSObjectProtocol for MagnetiteFileDragSource {}

    unsafe impl NSDraggingSource for MagnetiteFileDragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn operation_mask(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            NSDragOperation::Copy
        }
    }
);

thread_local! {
    /// One source for every drag: AppKit does not promise to keep it alive.
    static SOURCE: OnceCell<Retained<MagnetiteFileDragSource>> = const { OnceCell::new() };
}

/// Start dragging the file at `path` from the window whose content view is
/// `handle`, at the pointer. False when the pointer's button is up already
/// (a drag started then would follow the pointer until the next click) or
/// the file cannot be named.
pub fn drag_file(handle: cef::sys::cef_window_handle_t, path: &Path) -> bool {
    let (Some(mtm), Some(path)) = (MainThreadMarker::new(), path.to_str()) else {
        return false;
    };
    if handle.is_null() || NSEvent::pressedMouseButtons() & 1 == 0 {
        return false;
    }
    // SAFETY: CEF hands us the window's live content NSView; main thread.
    let view: &NSView = unsafe { &*handle.cast::<NSView>() };
    let Some(window) = view.window() else {
        return false;
    };
    let path = NSString::from_str(path);
    let item = NSDraggingItem::initWithPasteboardWriter(
        NSDraggingItem::alloc(),
        ProtocolObject::from_ref(&*NSURL::fileURLWithPath(&path)),
    );
    let icon = NSWorkspace::sharedWorkspace().iconForFile(&path);
    icon.setSize(NSSize::new(ICON, ICON));
    let pointer = window.mouseLocationOutsideOfEventStream();
    let at = view.convertPoint_fromView(pointer, None);
    let frame = NSRect::new(
        NSPoint::new(at.x - ICON / 2.0, at.y - ICON / 2.0),
        NSSize::new(ICON, ICON),
    );
    // SAFETY: the contents of a dragging frame may be an NSImage.
    unsafe { item.setDraggingFrame_contents(frame, Some(&icon)) };
    // The page took the button press; AppKit wants a drag event to start
    // from, at the pointer, in this window.
    let time = NSApplication::sharedApplication(mtm)
        .currentEvent()
        .map_or(0.0, |e| e.timestamp());
    let Some(event) = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        NSEventType::LeftMouseDragged,
        pointer,
        NSEventModifierFlags::empty(),
        time,
        window.windowNumber(),
        None,
        0,
        1,
        1.0,
    ) else {
        return false;
    };
    let source = SOURCE.with(|s| {
        s.get_or_init(|| unsafe { msg_send![MagnetiteFileDragSource::alloc(mtm), init] })
            .clone()
    });
    view.beginDraggingSessionWithItems_event_source(
        &NSArray::from_retained_slice(&[item]),
        &event,
        ProtocolObject::from_ref(&*source),
    );
    true
}

/// Move the file at `path` to the Trash, where Finder's "Put Back" returns it.
pub fn trash(path: &Path) -> std::io::Result<()> {
    let name = path.to_str().ok_or(std::io::ErrorKind::InvalidInput)?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(name));
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|e| std::io::Error::other(e.localizedDescription().to_string()))
}
