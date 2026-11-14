//! Clipboard, save dialog, pointer and keys on Windows (what `mac/share.rs` does
//! with `AppKit`).

use std::ffi::c_void;
use std::path::PathBuf;

use cef::*;

type Hwnd = *mut c_void;

/// `OPENFILENAMEW`.
#[repr(C)]
struct OpenFileName {
    size: u32,
    owner: Hwnd,
    instance: *mut c_void,
    filter: *const u16,
    custom_filter: *mut u16,
    max_custom_filter: u32,
    filter_index: u32,
    file: *mut u16,
    max_file: u32,
    file_title: *mut u16,
    max_file_title: u32,
    initial_dir: *const u16,
    title: *const u16,
    flags: u32,
    file_offset: u16,
    file_extension: u16,
    default_extension: *const u16,
    custom_data: isize,
    hook: *mut c_void,
    template_name: *const u16,
    reserved_pointer: *mut c_void,
    reserved: u32,
    flags_ex: u32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn OpenClipboard(owner: Hwnd) -> i32;
    fn EmptyClipboard() -> i32;
    fn CloseClipboard() -> i32;
    fn SetClipboardData(format: u32, data: *mut c_void) -> *mut c_void;
    fn RegisterClipboardFormatW(name: *const u16) -> u32;
    fn GetCursorPos(point: *mut [i32; 2]) -> i32;
    fn GetKeyState(key: i32) -> i16;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> *mut c_void;
    fn GlobalLock(memory: *mut c_void) -> *mut c_void;
    fn GlobalUnlock(memory: *mut c_void) -> i32;
    fn GlobalFree(memory: *mut c_void) -> *mut c_void;
}

#[link(name = "comdlg32")]
unsafe extern "system" {
    fn GetSaveFileNameW(dialog: *mut OpenFileName) -> i32;
}

const GMEM_MOVEABLE: u32 = 2;
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
/// Ask before replacing a file, the folder must exist, leave the working
/// directory alone, the Explorer-style dialog.
const SAVE_FLAGS: u32 = 0x2 | 0x800 | 0x8 | 0x0008_0000;
/// Characters a path may have, with the NUL.
const MAX_PATH_CHARS: usize = 1024;

/// `text` as UTF-16 with the NUL Windows wants at the end.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// Replace the clipboard's content by `formats`, each a format and its bytes.
fn put(formats: &[(u32, &[u8])]) -> bool {
    // SAFETY: the clipboard is opened and closed here. Each block of global
    // memory is as long as its bytes; the clipboard owns it once
    // `SetClipboardData` took it, and it is freed here if not.
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return false;
        }
        EmptyClipboard();
        let mut all = true;
        for (format, bytes) in formats {
            let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
            let to = if memory.is_null() {
                std::ptr::null_mut()
            } else {
                GlobalLock(memory)
            };
            if to.is_null() {
                if !memory.is_null() {
                    GlobalFree(memory);
                }
                all = false;
                continue;
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), to.cast::<u8>(), bytes.len());
            GlobalUnlock(memory);
            if SetClipboardData(*format, memory).is_null() {
                GlobalFree(memory);
                all = false;
            }
        }
        CloseClipboard();
        all
    }
}

/// Put `text` on the clipboard.
pub fn copy_text(text: &str) -> bool {
    let bytes: Vec<u8> = wide(text).into_iter().flat_map(u16::to_le_bytes).collect();
    put(&[(CF_UNICODETEXT, &bytes)])
}

/// Put a PNG on the clipboard: as PNG for the programs that take it so
/// (browsers, office programs) and as a plain bitmap for the rest.
pub fn copy_png(png: &[u8]) -> bool {
    // SAFETY: a NUL-terminated name; Windows hands out the same number for
    // it in every program.
    let png_format = unsafe { RegisterClipboardFormatW(wide("PNG").as_ptr()) };
    let bitmap = bitmap_of(png);
    let mut formats: Vec<(u32, &[u8])> = Vec::new();
    if png_format != 0 {
        formats.push((png_format, png));
    }
    if let Some(bitmap) = bitmap.as_deref() {
        formats.push((CF_DIB, bitmap));
    }
    !formats.is_empty() && put(&formats)
}

/// `png` as a device-independent bitmap (`CF_DIB`): a 40-byte header and
/// the pixels as BGRA, bottom row first.
fn bitmap_of(png: &[u8]) -> Option<Vec<u8>> {
    let image = image_create()?;
    if image.add_png(1.0, Some(png)) == 0 {
        return None;
    }
    let (mut width, mut height) = (0, 0);
    let pixels = image.as_bitmap(
        1.0,
        ColorType::BGRA_8888,
        AlphaType::PREMULTIPLIED,
        Some(&mut width),
        Some(&mut height),
    )?;
    let mut top_down = vec![0u8; pixels.size()];
    pixels.data(Some(&mut top_down), 0);
    dib(&top_down, width, height)
}

/// A `CF_DIB` from `top_down` BGRA pixels, `width` by `height`.
fn dib(top_down: &[u8], width: i32, height: i32) -> Option<Vec<u8>> {
    let row = usize::try_from(width).ok()?.checked_mul(4)?;
    let rows = usize::try_from(height).ok()?;
    if row == 0 || rows == 0 || top_down.len() != row.checked_mul(rows)? {
        return None;
    }
    let mut out = Vec::with_capacity(40 + top_down.len());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    out.extend_from_slice(&[0; 24]); // uncompressed, the rest unused
    for line in top_down.chunks_exact(row).rev() {
        out.extend_from_slice(line);
    }
    Some(out)
}

/// Ask where to save a PNG, suggesting `name`, and write it there. `None`
/// when the user cancelled or the file could not be written.
pub fn save_png(owner: cef::sys::cef_window_handle_t, png: &[u8], name: &str) -> Option<PathBuf> {
    let mut file = wide(name);
    if file.len() > MAX_PATH_CHARS {
        return None;
    }
    file.resize(MAX_PATH_CHARS, 0);
    let filter = wide("PNG image (*.png)\0*.png\0");
    let extension = wide("png");
    // SAFETY: a plain C struct, all zero but for what is set below.
    let mut dialog: OpenFileName = unsafe { std::mem::zeroed() };
    dialog.size = u32::try_from(size_of::<OpenFileName>()).ok()?;
    dialog.owner = owner.0.cast();
    dialog.filter = filter.as_ptr();
    dialog.file = file.as_mut_ptr();
    dialog.max_file = u32::try_from(file.len()).ok()?;
    dialog.flags = SAVE_FLAGS;
    dialog.default_extension = extension.as_ptr();
    // SAFETY: the strings outlive the call and `file` has `max_file`
    // characters of room for the chosen path.
    if unsafe { GetSaveFileNameW(&raw mut dialog) } == 0 {
        return None;
    }
    let end = file.iter().position(|c| *c == 0)?;
    let path = PathBuf::from(String::from_utf16(&file[..end]).ok()?);
    std::fs::write(&path, png).ok()?;
    Some(path)
}

/// Whether a Shift key is down, as of the input being handled now.
pub fn shift_held() -> bool {
    const VK_SHIFT: i32 = 0x10;
    // SAFETY: a plain query. The top bit says the key is down.
    let state = unsafe { GetKeyState(VK_SHIFT) };
    state < 0
}

/// Where the pointer is, in screen pixels.
pub fn mouse_point() -> Option<Point> {
    let mut at = [0i32; 2];
    // SAFETY: `at` is the two `i32` of a POINT.
    (unsafe { GetCursorPos(&raw mut at) } != 0).then_some(Point { x: at[0], y: at[1] })
}

#[cfg(test)]
mod tests {
    use super::dib;

    #[test]
    fn a_bitmap_for_the_clipboard_has_its_rows_bottom_up() {
        // Two pixels wide, two rows: the top row is 1s, the bottom row 2s.
        let top_down = [[1u8; 8], [2u8; 8]].concat();
        let out = dib(&top_down, 2, 2).unwrap();
        assert_eq!(out.len(), 40 + 16);
        assert_eq!(out[..4], 40u32.to_le_bytes());
        assert_eq!(out[4..8], 2i32.to_le_bytes());
        assert_eq!(out[14..16], 32u16.to_le_bytes());
        assert_eq!(out[40..48], [2u8; 8]);
        assert_eq!(out[48..56], [1u8; 8]);
        // Pixels that do not fill the picture are refused.
        assert!(dib(&top_down, 3, 2).is_none());
    }
}
