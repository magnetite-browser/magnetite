//! The system-wide quick search key, ⌥⌘N, through Carbon's
//! `RegisterEventHotKey`: macOS reports only that combination to us, from
//! any app, without the accessibility permission a key monitor would need.
//! A press reaches the app as `Shortcut::NewMini`. Another app that holds
//! the combination already wins; Magnetite then does without (traced).

use std::cell::Cell;
use std::ffi::c_void;

use crate::engine::Engine;
use crate::events::{EngineEvent, Shortcut};

type OsStatus = i32;
type EventTargetRef = *mut c_void;
type EventRef = *mut c_void;
type EventHandlerCallRef = *mut c_void;
type EventHandlerRef = *mut c_void;
type EventHotKeyRef = *mut c_void;
type EventHandler = extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OsStatus;

#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

#[repr(C)]
struct EventHotKeyId {
    signature: u32,
    id: u32,
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> EventTargetRef;
    fn InstallEventHandler(
        target: EventTargetRef,
        handler: EventHandler,
        num_types: usize,
        list: *const EventTypeSpec,
        user_data: *mut c_void,
        out_ref: *mut EventHandlerRef,
    ) -> OsStatus;
    fn RegisterEventHotKey(
        key_code: u32,
        modifiers: u32,
        id: EventHotKeyId,
        target: EventTargetRef,
        options: u32,
        out_ref: *mut EventHotKeyRef,
    ) -> OsStatus;
    fn UnregisterEventHotKey(hot_key: EventHotKeyRef) -> OsStatus;
}

/// `kEventClassKeyboard` and `kEventHotKeyPressed`.
const KEYBOARD: u32 = u32::from_be_bytes(*b"keyb");
const HOT_KEY_PRESSED: u32 = 5;
/// `kVK_ANSI_N`, and `cmdKey | optionKey`.
const KEY_N: u32 = 0x2D;
const CMD_OPTION: u32 = (1 << 8) | (1 << 11);
/// Names our hot key to Carbon (any four characters of ours).
const SIGNATURE: u32 = u32::from_be_bytes(*b"MgQS");

thread_local! {
    /// The registered key, while it is.
    static HOT_KEY: Cell<EventHotKeyRef> = const { Cell::new(std::ptr::null_mut()) };
    /// The press handler is installed (once, for the app's lifetime).
    static HANDLER: Cell<bool> = const { Cell::new(false) };
}

extern "C" fn pressed(
    _call: EventHandlerCallRef,
    _event: EventRef,
    _data: *mut c_void,
) -> OsStatus {
    Engine::global().emit(EngineEvent::Shortcut {
        from: None,
        shortcut: Shortcut::NewMini,
    });
    0
}

/// Register ⌥⌘N system-wide (`on`), or let it go. Main thread only.
pub fn set_quick_search_hotkey(on: bool) {
    let registered = !HOT_KEY.get().is_null();
    if on == registered {
        return;
    }
    if !on {
        // SAFETY: the reference came from RegisterEventHotKey and is
        // released once.
        let status = unsafe { UnregisterEventHotKey(HOT_KEY.replace(std::ptr::null_mut())) };
        crate::trace(&format!("quick search key released ({status})"));
        return;
    }
    // SAFETY: Carbon calls on the main thread with valid out pointers; the
    // handler is a plain function that lives as long as the process.
    unsafe {
        if !HANDLER.get() {
            let spec = EventTypeSpec {
                event_class: KEYBOARD,
                event_kind: HOT_KEY_PRESSED,
            };
            let mut handler = std::ptr::null_mut();
            let status = InstallEventHandler(
                GetApplicationEventTarget(),
                pressed,
                1,
                &spec,
                std::ptr::null_mut(),
                &mut handler,
            );
            if status != 0 {
                crate::trace(&format!(
                    "cannot listen for the quick search key ({status})"
                ));
                return;
            }
            HANDLER.set(true);
        }
        let mut hot_key = std::ptr::null_mut();
        let id = EventHotKeyId {
            signature: SIGNATURE,
            id: 1,
        };
        let status = RegisterEventHotKey(
            KEY_N,
            CMD_OPTION,
            id,
            GetApplicationEventTarget(),
            0,
            &mut hot_key,
        );
        if status == 0 {
            HOT_KEY.set(hot_key);
        } else {
            // Most likely another app holds the combination.
            crate::trace(&format!("quick search key not registered ({status})"));
        }
    }
}
