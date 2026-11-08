//! The link-hover status pill. On macOS Chromium's own status bubble is off
//! (`content.rs`); the hovered link shows instead in a rounded pill near the
//! bottom of the page: a frameless child window of the frame that fades in
//! and out, never takes the pointer, and hops above the pointer rather than
//! cover it.

use super::{attach_above, borderless_window, fade, plain_box, screen_rect};
use objc2::{MainThreadMarker, MainThreadOnly, msg_send, rc::Retained};
use objc2_app_kit::{NSBox, NSEvent, NSTextField, NSWindow};

/// The link-preview pill: a frameless child window with a rounded box and a
/// one-line label, created on first use and faded in and out after that.
struct StatusPill {
    window: Retained<NSWindow>,
    pill: Retained<NSBox>,
    label: Retained<NSTextField>,
}

thread_local! {
    static STATUS_PILL: std::cell::RefCell<Option<StatusPill>> = const { std::cell::RefCell::new(None) };
}

/// Pill height, its gap to the page's bottom edge and its side padding (DIP).
const PILL_HEIGHT: f64 = 24.0;
const PILL_MARGIN: f64 = 10.0;
const PILL_PADDING: f64 = 12.0;

/// Show `text` (usually the hovered link) in a pill centred near the bottom
/// of `page` (CEF screen coordinates: top-left origin, DIP), above the other
/// child windows of the window whose content view is `frame_handle`. Empty
/// `text` fades it out. The pill never takes clicks, and moves above the
/// pointer when it would cover it.
pub fn set_status_pill(frame_handle: cef::sys::cef_window_handle_t, page: &cef::Rect, text: &str) {
    use objc2_app_kit::NSView;
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if text.is_empty() {
        hide_status_pill();
        return;
    }
    if frame_handle.is_null() {
        return;
    }
    // SAFETY: CEF hands us the frame's live content NSView; main thread.
    let view: &NSView = unsafe { &*frame_handle.cast::<NSView>() };
    let (Some(frame_window), Some(page)) = (view.window(), screen_rect(mtm, page)) else {
        return;
    };
    STATUS_PILL.with(|cell| {
        let mut cell = cell.borrow_mut();
        let p = cell.get_or_insert_with(|| create_status_pill(mtm));
        p.label.setStringValue(&NSString::from_str(text));
        p.label.sizeToFit();
        let text_size = p.label.frame().size;
        let max_width = (page.size.width - 32.0).max(80.0);
        let width = (text_size.width + 2.0 * PILL_PADDING).min(max_width);
        // Centred, `PILL_MARGIN` above the page's bottom edge (AppKit's y
        // grows upwards).
        let mut rect = NSRect::new(
            NSPoint::new(
                page.origin.x + (page.size.width - width) / 2.0,
                page.origin.y + PILL_MARGIN,
            ),
            NSSize::new(width, PILL_HEIGHT),
        );
        // Never hide what the pointer is on: hop above it instead.
        let mouse = NSEvent::mouseLocation();
        let near = mouse.x > rect.origin.x - 8.0
            && mouse.x < rect.origin.x + width + 8.0
            && mouse.y > rect.origin.y - 8.0
            && mouse.y < rect.origin.y + PILL_HEIGHT + 8.0;
        if near {
            rect.origin.y = mouse.y + 18.0;
        }
        p.window.setFrame_display(rect, true);
        let local = NSRect::new(NSPoint::new(0.0, 0.0), rect.size);
        p.pill.setFrame(local);
        p.label.setFrame(NSRect::new(
            NSPoint::new(PILL_PADDING, (PILL_HEIGHT - text_size.height) / 2.0),
            NSSize::new(width - 2.0 * PILL_PADDING, text_size.height),
        ));
        attach_above(&frame_window, &p.window);
        fade(&p.window, 1.0, 0.08);
    });
}

/// Fade the link-preview pill out (a page hid, or the pointer left the link).
pub fn hide_status_pill() {
    STATUS_PILL.with(|cell| {
        if let Some(p) = cell.borrow().as_ref() {
            fade(&p.window, 0.0, 0.18);
        }
    });
}

fn create_status_pill(mtm: MainThreadMarker) -> StatusPill {
    use objc2_app_kit::{NSColor, NSFont, NSLineBreakMode, NSView};
    use objc2_foundation::{NSRect, ns_string};
    let zero = NSRect::ZERO;
    let window = borderless_window(mtm, zero, true);
    window.setIgnoresMouseEvents(true);
    window.setAlphaValue(0.0);
    // SAFETY: initWithFrame: is NSView's designated initialiser.
    let root: Retained<NSView> = unsafe { msg_send![NSView::alloc(mtm), initWithFrame: zero] };
    let pill = plain_box(
        mtm,
        PILL_HEIGHT / 2.0,
        &NSColor::colorWithWhite_alpha(0.11, 0.94),
    );
    pill.setBorderColor(&NSColor::colorWithWhite_alpha(1.0, 0.1));
    pill.setBorderWidth(1.0);
    let label = NSTextField::labelWithString(ns_string!(""), mtm);
    label.setFont(Some(&NSFont::systemFontOfSize(12.0)));
    label.setTextColor(Some(&NSColor::colorWithWhite_alpha(1.0, 0.86)));
    label.setMaximumNumberOfLines(1);
    // Long links keep their start and their end.
    label.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    root.addSubview(&pill);
    root.addSubview(&label);
    window.setContentView(Some(&root));
    StatusPill {
        window,
        pill,
        label,
    }
}
