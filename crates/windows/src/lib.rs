//! Magnetite as a DLL for Windows.
//!
//! Chromium's sandbox on Windows needs the process to start in CEF's own
//! `bootstrap.exe`, which then loads the application from a DLL of the same
//! name and calls one of the entry points below with the sandbox information
//! (CEF's `include/cef_sandbox_win.h`). `scripts/build.sh` packages
//! `bootstrap.exe` as `magnetite.exe` and this library as `magnetite.dll`.
//! Chromium starts its other processes (renderer, GPU, utility) the same way.
#![cfg(windows)]
#![allow(non_snake_case)] // the names CEF's bootstrap looks up

use std::ffi::{c_char, c_int, c_void};

/// Entry point for `bootstrap.exe` (a windows application, no console).
#[unsafe(no_mangle)]
pub extern "C" fn RunWinMain(
    _instance: *mut c_void,
    _cmd_line: *mut u16,
    _cmd_show: c_int,
    sandbox_info: *mut c_void,
    _version_info: *mut c_void,
) -> c_int {
    magnetite::run_from_bootstrap(sandbox_info)
}

/// Entry point for `bootstrapc.exe` (a console application: development,
/// to see the log).
#[unsafe(no_mangle)]
pub extern "C" fn RunConsoleMain(
    _argc: c_int,
    _argv: *mut *mut c_char,
    sandbox_info: *mut c_void,
    _version_info: *mut c_void,
) -> c_int {
    magnetite::run_from_bootstrap(sandbox_info)
}
