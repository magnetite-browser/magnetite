//! The system clipboard on Linux, for `Engine::copy_text` and `copy_png`.
//! Under Wayland through the compositor's data-control protocol where it
//! has one (KDE, wlroots), else through X11, also `XWayland`'s (GNOME).
//!
//! On Linux the program that copied hands the content to the one that
//! pastes, so the clipboard stays open for the whole run: under X11 a
//! thread answers paste requests, under Wayland a thread per copy does,
//! until the next copy replaces it. `hand_over` at the end of the run
//! leaves the last copy to the desktop's clipboard manager.

use std::sync::{Mutex, PoisonError};

use arboard::{Clipboard, Error, ImageData};

static CLIPBOARD: Mutex<Option<Clipboard>> = Mutex::new(None);

/// Run `set` on the clipboard, opening it first if needed. A failed
/// write closes it, so that the next copy starts over (the X server or the
/// compositor may have gone).
fn write(set: impl FnOnce(&mut Clipboard) -> Result<(), Error>) -> bool {
    let mut open = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
    let clipboard = open.take().map_or_else(Clipboard::new, Ok);
    match clipboard.and_then(|mut c| set(&mut c).map(|()| c)) {
        Ok(c) => {
            *open = Some(c);
            true
        }
        Err(e) => {
            eprintln!("magnetite: could not copy: {e}");
            false
        }
    }
}

pub fn copy_text(text: &str) -> bool {
    write(|c| c.set_text(text))
}

/// The clipboard takes images as pixels: `png` is decoded first.
pub fn copy_png(png: &[u8]) -> bool {
    let Ok(image) = image::load_from_memory_with_format(png, image::ImageFormat::Png) else {
        return false;
    };
    let image = image.into_rgba8();
    let (width, height) = image.dimensions();
    write(|c| {
        c.set_image(ImageData {
            width: width as usize,
            height: height as usize,
            bytes: image.into_raw().into(),
        })
    })
}

/// Close the clipboard as Magnetite quits. Under X11 this asks the desktop's
/// clipboard manager, if it has one, to take the last copy, so that it can
/// still be pasted (waits up to 100 ms).
pub fn hand_over() {
    *CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner) = None;
}
