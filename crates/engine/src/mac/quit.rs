//! Cmd+Q quits on the second press. The first one shows a hint in the middle
//! of the screen and counts for `AGAIN_WITHIN`; one stray press, next to
//! Cmd+W, then ends nothing. Quit chosen in a menu (ours or the Dock's)
//! quits at once.

use super::{borderless_window, fade, plain_box};
use crate::engine::Engine;
use objc2::{MainThreadMarker, MainThreadOnly, msg_send, rc::Retained};
use objc2_app_kit::{NSApp, NSEventType, NSScreen, NSTextField, NSWindow};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

/// How long after the first press a second one quits, and the hint shows.
const AGAIN_WITHIN_MS: i64 = 1500;
const AGAIN_WITHIN: Duration = Duration::from_millis(AGAIN_WITHIN_MS.unsigned_abs());

const HINT: &str = "Press ⌘Q again to quit";
/// Hint height, corner radius and side padding (DIP).
const HEIGHT: f64 = 48.0;
const RADIUS: f64 = 14.0;
const PADDING: f64 = 22.0;

thread_local! {
    static FIRST_PRESS: Cell<Option<Instant>> = const { Cell::new(None) };
    static HINT_WINDOW: RefCell<Option<Retained<NSWindow>>> = const { RefCell::new(None) };
}

/// Whether the `terminate:` being handled may quit: always when it comes
/// from a menu, and from the keyboard on the second press. The first press
/// shows the hint instead.
pub(super) fn confirmed(mtm: MainThreadMarker) -> bool {
    let Some(event) = NSApp(mtm).currentEvent() else {
        return true;
    };
    if event.r#type() != NSEventType::KeyDown {
        return true;
    }
    // Any other key is a menu item chosen with the keyboard. (Only a key
    // event may be asked for its characters.)
    let key = event.charactersIgnoringModifiers();
    if !key.is_some_and(|key| key.to_string().eq_ignore_ascii_case("q")) {
        return true;
    }
    // Held keys: neither a second press nor a new first one.
    if event.isARepeat() {
        return false;
    }
    let now = Instant::now();
    if second_press(FIRST_PRESS.replace(Some(now)), now) {
        return true;
    }
    show_hint(mtm);
    Engine::post_delayed(Box::new(hide_hint), AGAIN_WITHIN_MS);
    false
}

/// Whether a press at `now` follows the one at `first` closely enough.
fn second_press(first: Option<Instant>, now: Instant) -> bool {
    first.is_some_and(|first| now.duration_since(first) <= AGAIN_WITHIN)
}

/// Show the hint in the middle of the screen with the keyboard, above
/// everything, full-screen windows included. It never takes clicks.
fn show_hint(mtm: MainThreadMarker) {
    let Some(screen) = NSScreen::mainScreen(mtm) else {
        return;
    };
    HINT_WINDOW.with(|cell| {
        let mut cell = cell.borrow_mut();
        let window = cell.get_or_insert_with(|| create_hint(mtm));
        let (area, size) = (screen.visibleFrame(), window.frame().size);
        window.setFrameOrigin(NSPoint::new(
            area.origin.x + (area.size.width - size.width) / 2.0,
            area.origin.y + (area.size.height - size.height) / 2.0,
        ));
        window.orderFrontRegardless();
        fade(window, 1.0, 0.1);
    });
}

/// Fade the hint out, unless a later first press shows it anew.
fn hide_hint() {
    let waiting = FIRST_PRESS
        .get()
        .is_some_and(|first| first.elapsed() < AGAIN_WITHIN);
    if waiting {
        return;
    }
    HINT_WINDOW.with(|cell| {
        if let Some(window) = cell.borrow().as_ref() {
            fade(window, 0.0, 0.25);
        }
    });
}

fn create_hint(mtm: MainThreadMarker) -> Retained<NSWindow> {
    use objc2_app_kit::{NSColor, NSFont, NSStatusWindowLevel, NSView, NSWindowCollectionBehavior};
    use objc2_foundation::NSString;
    let label = NSTextField::labelWithString(&NSString::from_str(HINT), mtm);
    // SAFETY: a constant of AppKit's.
    let weight = unsafe { objc2_app_kit::NSFontWeightMedium };
    label.setFont(Some(&NSFont::systemFontOfSize_weight(17.0, weight)));
    label.setTextColor(Some(&NSColor::colorWithWhite_alpha(1.0, 0.92)));
    label.sizeToFit();
    let text = label.frame().size;
    let size = NSSize::new(text.width + 2.0 * PADDING, HEIGHT);
    let rect = NSRect::new(NSPoint::new(0.0, 0.0), size);

    let window = borderless_window(mtm, rect, true);
    window.setIgnoresMouseEvents(true);
    window.setAlphaValue(0.0);
    window.setLevel(NSStatusWindowLevel);
    window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    // SAFETY: initWithFrame: is NSView's designated initialiser.
    let root: Retained<NSView> = unsafe { msg_send![NSView::alloc(mtm), initWithFrame: rect] };
    let back = plain_box(mtm, RADIUS, &NSColor::colorWithWhite_alpha(0.11, 0.94));
    back.setBorderColor(&NSColor::colorWithWhite_alpha(1.0, 0.1));
    back.setBorderWidth(1.0);
    back.setFrame(rect);
    label.setFrame(NSRect::new(
        NSPoint::new(PADDING, (HEIGHT - text.height) / 2.0),
        text,
    ));
    root.addSubview(&back);
    root.addSubview(&label);
    window.setContentView(Some(&root));
    window
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_close_second_press_quits() {
        let first = Instant::now();
        assert!(!second_press(None, first));
        assert!(second_press(Some(first), first + AGAIN_WITHIN / 2));
        assert!(!second_press(Some(first), first + AGAIN_WITHIN * 2));
    }
}
