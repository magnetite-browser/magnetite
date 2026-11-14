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

#[link(name = "kernel32")]
unsafe extern "system" {
    fn OpenMutexW(access: u32, inherit: i32, name: *const u16) -> *mut c_void;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

/// The right to wait for a mutex: enough to learn that it is there, and
/// granted to this user also for a mutex of the installer run as an
/// administrator.
const SYNCHRONIZE: u32 = 0x0010_0000;

/// The name the installer holds once it runs as an update and waits for
/// Magnetite to quit (`InitializeSetup` in `magnetite.iss`).
const INSTALLER_READY: &str = "MagnetiteUpdateReady";

/// Whether the installer of an update stands ready: started, allowed to
/// install (after Windows asked for an administrator, where needed), and
/// waiting for Magnetite to quit.
pub fn installer_ready() -> bool {
    let name: Vec<u16> = INSTALLER_READY.encode_utf16().chain([0]).collect();
    // SAFETY: a NUL-terminated name; the handle is closed again at once.
    unsafe {
        let handle = OpenMutexW(SYNCHRONIZE, 0, name.as_ptr());
        if handle.is_null() {
            return false;
        }
        CloseHandle(handle);
    }
    true
}

/// Say that Magnetite runs, until the process ends.
pub fn mark_running() {
    for name in NAMES {
        let name: Vec<u16> = name.encode_utf16().chain([0]).collect();
        // SAFETY: a NUL-terminated name. The handle is never closed: the
        // mutex is there as long as the process, and Windows frees it then.
        unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    }
}
