//! A sign that Magnetite runs, for its installer.
//!
//! A running Magnetite holds only some of its files open. An installer or
//! uninstaller that went ahead all the same replaced or removed the rest
//! under it: the window stayed, without its sidebar page and its icon. The
//! installer (`AppMutex` in `packaging/windows/magnetite.iss`) looks for
//! these names and asks to close Magnetite first.

use std::ffi::c_void;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateMutexW(attributes: *const c_void, owned: i32, name: *const u16) -> *mut c_void;
}

/// The names the installer looks for: this session's, and one for every
/// session of the computer (an installation for all users).
const NAMES: [&str; 2] = ["MagnetiteRunning", "Global\\MagnetiteRunning"];

/// Say that Magnetite runs, until the process ends.
pub fn mark_running() {
    for name in NAMES {
        let name: Vec<u16> = name.encode_utf16().chain([0]).collect();
        // SAFETY: a NUL-terminated name. The handle is never closed: the
        // mutex is there as long as the process, and Windows frees it then.
        unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    }
}
