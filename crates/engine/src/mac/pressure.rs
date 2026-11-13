//! The system's memory pressure signal, passed on as
//! `EngineEvent::MemoryPressure` so the app can unload pages early.

use crate::engine::Engine;
use crate::events::{EngineEvent, MemoryPressure};
use std::ffi::c_void;

/// `DISPATCH_MEMORYPRESSURE_WARN` and `_CRITICAL` of `<dispatch/source.h>`.
const WARN: usize = 0x02;
const CRITICAL: usize = 0x04;

/// `struct dispatch_source_type_s`, only ever used by address.
#[repr(C)]
struct SourceType {
    _opaque: [u8; 0],
}

unsafe extern "C" {
    static _dispatch_source_type_memorypressure: SourceType;
    fn dispatch_get_global_queue(identifier: isize, flags: usize) -> *mut c_void;
    fn dispatch_source_create(
        kind: *const SourceType,
        handle: usize,
        mask: usize,
        queue: *mut c_void,
    ) -> *mut c_void;
    fn dispatch_set_context(object: *mut c_void, context: *mut c_void);
    fn dispatch_source_set_event_handler_f(
        source: *mut c_void,
        handler: extern "C" fn(*mut c_void),
    );
    fn dispatch_source_get_data(source: *mut c_void) -> usize;
    fn dispatch_resume(object: *mut c_void);
}

/// Start listening, for the rest of the process's life. Call once the
/// engine exists.
pub fn watch_memory_pressure() {
    // SAFETY: plain libdispatch calls with a valid source type and queue.
    // The source is never released, and its context is the source itself,
    // so the pointer `on_pressure` gets stays valid.
    unsafe {
        let source = dispatch_source_create(
            &raw const _dispatch_source_type_memorypressure,
            0,
            WARN | CRITICAL,
            dispatch_get_global_queue(0, 0),
        );
        if source.is_null() {
            return;
        }
        dispatch_set_context(source, source);
        dispatch_source_set_event_handler_f(source, on_pressure);
        dispatch_resume(source);
    }
}

/// Runs on a libdispatch thread; `emit` hands the event to the UI thread.
extern "C" fn on_pressure(source: *mut c_void) {
    // SAFETY: `source` is the live source set as its own context above.
    let level = unsafe { dispatch_source_get_data(source) };
    let level = if level & CRITICAL != 0 {
        MemoryPressure::Critical
    } else {
        MemoryPressure::Warning
    };
    Engine::global().emit(EngineEvent::MemoryPressure(level));
}
