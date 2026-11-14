//! Whether Magnetite is Windows' default browser, and asking for it (what
//! `mac/default_browser.rs` does through Launch Services).
//!
//! The installer registers Magnetite as a browser, for the current user or
//! for all users as it was asked to install
//! (`packaging/windows/magnetite.iss`). Windows lets no program make itself
//! the default: the user chooses in Settings, which is opened at Magnetite's
//! entry.

use std::ffi::c_void;

#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegGetValueW(
        key: isize,
        subkey: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut c_void,
        size: *mut u32,
    ) -> i32;
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteW(
        owner: *mut c_void,
        operation: *const u16,
        file: *const u16,
        parameters: *const u16,
        directory: *const u16,
        show: i32,
    ) -> isize;
}

/// `HKEY_CURRENT_USER`.
const CURRENT_USER: isize = 0x8000_0001_u32.cast_signed() as isize;
/// `HKEY_LOCAL_MACHINE`.
const LOCAL_MACHINE: isize = 0x8000_0002_u32.cast_signed() as isize;
/// Only a string will do.
const RRF_RT_REG_SZ: u32 = 0x2;
const SW_SHOWNORMAL: i32 = 1;
/// The name links are registered under (the installer's `MagnetiteURL`).
const PROG_ID: &str = "MagnetiteURL";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// A string of the registry under `root`, `None` if it is not there.
fn string(root: isize, subkey: &str, value: &str) -> Option<String> {
    let (subkey, value) = (wide(subkey), wide(value));
    let mut data = [0u16; 512];
    let mut size = u32::try_from(size_of_val(&data)).ok()?;
    // SAFETY: NUL-terminated names, and `size` bytes of room for the data.
    let status = unsafe {
        RegGetValueW(
            root,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            data.as_mut_ptr().cast(),
            &raw mut size,
        )
    };
    if status != 0 {
        return None;
    }
    let end = data.iter().position(|c| *c == 0).unwrap_or(data.len());
    Some(String::from_utf16_lossy(&data[..end]))
}

/// Whom Magnetite is registered as a browser for.
#[derive(Clone, Copy)]
enum Registered {
    User,
    Machine,
}

/// Where the installer registered Magnetite: `None` when it did not (not
/// installed: a build run from its folder). An installation for the user
/// counts before one for all users, as Windows reads it.
fn registered() -> Option<Registered> {
    let command = format!("Software\\Classes\\{PROG_ID}\\shell\\open\\command");
    if string(CURRENT_USER, &command, "").is_some() {
        Some(Registered::User)
    } else if string(LOCAL_MACHINE, &command, "").is_some() {
        Some(Registered::Machine)
    } else {
        None
    }
}

/// Whether links of `scheme` ("http", "https") from other programs open in
/// Magnetite: `None` when Magnetite is not registered as a browser. The
/// choice is always the user's own.
pub fn handles(scheme: &str) -> Option<bool> {
    registered()?;
    let choice = string(
        CURRENT_USER,
        &format!(
            "Software\\Microsoft\\Windows\\Shell\\Associations\\UrlAssociations\\{scheme}\\UserChoice"
        ),
        "ProgId",
    );
    Some(choice.as_deref() == Some(PROG_ID))
}

/// Open Windows' Settings where the user makes Magnetite the default browser.
pub fn open_default_apps() {
    // Settings finds the entry by where it is registered.
    let page = match registered() {
        Some(Registered::Machine) => "ms-settings:defaultapps?registeredAppMachine=Magnetite",
        _ => "ms-settings:defaultapps?registeredAppUser=Magnetite",
    };
    let (open, page) = (wide("open"), wide(page));
    // SAFETY: NUL-terminated strings; Windows starts its own Settings.
    unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            open.as_ptr(),
            page.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
    }
}
