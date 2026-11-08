//! Handing things to macOS: the share menu (`AirDrop`, Mail, Messages, …)
//! for a link or a picture, the general pasteboard and the save panel.

use objc2::{MainThreadMarker, rc::Retained};

thread_local! {
    /// The open share picker; AppKit does not retain it while its menu shows.
    static SHARE_PICKER: std::cell::RefCell<Option<Retained<objc2_app_kit::NSSharingServicePicker>>> =
        const { std::cell::RefCell::new(None) };
}

/// Show the macOS share menu (`AirDrop`, Mail, Messages, …) for `url` at
/// (`x`, `y`) in DIP from the top-left of the window whose content view is
/// `handle`.
pub fn show_share_picker(handle: cef::sys::cef_window_handle_t, url: &str, x: f64, y: f64) {
    use objc2::AnyThread;
    use objc2_app_kit::{NSSharingServicePicker, NSView};
    use objc2_foundation::{NSArray, NSPoint, NSRect, NSRectEdge, NSSize, NSString, NSURL};
    if handle.is_null() {
        return;
    }
    // SAFETY: CEF hands us the window's live content NSView; we are on the
    // main thread (CEF's UI thread on macOS).
    let view: &NSView = unsafe { &*handle.cast::<NSView>() };
    let Some(nsurl) = NSURL::URLWithString(&NSString::from_str(url)) else {
        return;
    };
    // SAFETY: an array of NSURL is an array of objects.
    let items: Retained<NSArray> =
        unsafe { Retained::cast_unchecked(NSArray::from_retained_slice(&[nsurl])) };
    // SAFETY: `initWithItems:` takes any objects; NSURL is a supported item.
    let picker =
        unsafe { NSSharingServicePicker::initWithItems(NSSharingServicePicker::alloc(), &items) };
    let y = if view.isFlipped() {
        y
    } else {
        view.bounds().size.height - y
    };
    let at = NSRect::new(NSPoint::new(x, y), NSSize::new(1.0, 1.0));
    picker.showRelativeToRect_ofView_preferredEdge(at, view, NSRectEdge::MaxY);
    SHARE_PICKER.with(|p| *p.borrow_mut() = Some(picker));
}

/// Share menu for a PNG image at (`x`, `y`) of the window whose content view
/// is `handle` (see `show_share_picker`).
pub fn share_png(handle: cef::sys::cef_window_handle_t, png: &[u8], x: f64, y: f64) {
    use objc2::AnyThread;
    use objc2_app_kit::{NSImage, NSSharingServicePicker, NSView};
    use objc2_foundation::{NSArray, NSData, NSPoint, NSRect, NSRectEdge, NSSize};
    if handle.is_null() {
        return;
    }
    // SAFETY: CEF hands us the window's live content NSView; main thread.
    let view: &NSView = unsafe { &*handle.cast::<NSView>() };
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(png)) else {
        return;
    };
    // SAFETY: an array of NSImage is an array of objects.
    let items: Retained<NSArray> =
        unsafe { Retained::cast_unchecked(NSArray::from_retained_slice(&[image])) };
    // SAFETY: `initWithItems:` takes any objects; NSImage is a supported item.
    let picker =
        unsafe { NSSharingServicePicker::initWithItems(NSSharingServicePicker::alloc(), &items) };
    let y = if view.isFlipped() {
        y
    } else {
        view.bounds().size.height - y
    };
    picker.showRelativeToRect_ofView_preferredEdge(
        NSRect::new(NSPoint::new(x, y), NSSize::new(1.0, 1.0)),
        view,
        NSRectEdge::MaxY,
    );
    SHARE_PICKER.with(|p| *p.borrow_mut() = Some(picker));
}

/// Put plain text on the general pasteboard.
pub fn copy_text(text: &str) -> bool {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    // SAFETY: NSPasteboardTypeString is an immutable AppKit constant.
    pb.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString })
}

/// Put a PNG image on the general pasteboard.
pub fn copy_png(png: &[u8]) -> bool {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG};
    use objc2_foundation::NSData;
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    // SAFETY: NSPasteboardTypePNG is an immutable AppKit constant.
    pb.setData_forType(Some(&NSData::with_bytes(png)), unsafe {
        NSPasteboardTypePNG
    })
}

/// Ask where to save `png` (default file name `name`) and write it there.
/// Returns the path, or `None` if the user cancelled or writing failed.
pub fn save_png(png: &[u8], name: &str) -> Option<std::path::PathBuf> {
    use objc2_app_kit::NSSavePanel;
    use objc2_foundation::NSString;
    let mtm = MainThreadMarker::new()?;
    let panel = NSSavePanel::savePanel(mtm);
    panel.setNameFieldStringValue(&NSString::from_str(name));
    // NSModalResponseOK
    if panel.runModal() != 1 {
        return None;
    }
    let path = std::path::PathBuf::from(panel.URL()?.path()?.to_string());
    std::fs::write(&path, png).ok()?;
    Some(path)
}
