//! Our own connection to the X server, for what CEF has no call for: a
//! stacking hint (`panel_stack.rs`) and where the pointer is (`edge.rs`).
//! Loaded at run time: under Wayland there may be no libX11 to load, and
//! the build needs no X11 headers. Used on the UI thread only.

use std::ffi::{CStr, c_char, c_int, c_long, c_uchar, c_uint, c_ulong, c_void};
use std::sync::OnceLock;

/// An X11 window or atom: a resource id.
pub(crate) type Xid = c_ulong;

/// Xlib's names for the property types read here.
const XA_CARDINAL: Xid = 6;
const XA_WINDOW: Xid = 33;

type OpenDisplay = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type SetTransient = unsafe extern "C" fn(*mut c_void, Xid, Xid) -> c_int;
type ErrorHandler = unsafe extern "C" fn(*mut c_void, *mut c_void) -> c_int;
type SetErrorHandler = unsafe extern "C" fn(Option<ErrorHandler>) -> Option<ErrorHandler>;
type SyncDisplay = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type QueryPointer = unsafe extern "C" fn(
    *mut c_void,
    Xid,
    *mut Xid,
    *mut Xid,
    *mut c_int,
    *mut c_int,
    *mut c_int,
    *mut c_int,
    *mut c_uint,
) -> c_int;
type GetGeometry = unsafe extern "C" fn(
    *mut c_void,
    Xid,
    *mut Xid,
    *mut c_int,
    *mut c_int,
    *mut c_uint,
    *mut c_uint,
    *mut c_uint,
    *mut c_uint,
) -> c_int;
type InternAtom = unsafe extern "C" fn(*mut c_void, *const c_char, c_int) -> Xid;
type GetProperty = unsafe extern "C" fn(
    *mut c_void,
    Xid,
    Xid,
    c_long,
    c_long,
    c_int,
    Xid,
    *mut Xid,
    *mut c_int,
    *mut c_ulong,
    *mut c_ulong,
    *mut *mut c_uchar,
) -> c_int;
type Free = unsafe extern "C" fn(*mut c_void) -> c_int;
type DefaultRoot = unsafe extern "C" fn(*mut c_void) -> Xid;

/// The Xlib calls needed, and our connection.
struct Xlib {
    /// The `Display`, as an address so that the static may hold it.
    display: usize,
    set_transient: SetTransient,
    set_error_handler: SetErrorHandler,
    sync: SyncDisplay,
    query_pointer: QueryPointer,
    get_geometry: GetGeometry,
    intern_atom: InternAtom,
    get_property: GetProperty,
    free: Free,
    default_root: DefaultRoot,
}

fn xlib() -> Option<&'static Xlib> {
    static XLIB: OnceLock<Option<Xlib>> = OnceLock::new();
    XLIB.get_or_init(|| {
        // SAFETY: the names are C strings, and each symbol has the signature
        // Xlib documents for it.
        unsafe {
            let lib = libc::dlopen(c"libX11.so.6".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if lib.is_null() {
                return None;
            }
            let symbol = |name: &CStr| {
                let found = libc::dlsym(lib, name.as_ptr());
                (!found.is_null()).then_some(found)
            };
            macro_rules! load {
                ($name:literal as $kind:ty) => {
                    std::mem::transmute::<*mut c_void, $kind>(symbol($name)?)
                };
            }
            let open = load!(c"XOpenDisplay" as OpenDisplay);
            let display = open(std::ptr::null());
            if display.is_null() {
                return None;
            }
            Some(Xlib {
                display: display as usize,
                set_transient: load!(c"XSetTransientForHint" as SetTransient),
                set_error_handler: load!(c"XSetErrorHandler" as SetErrorHandler),
                sync: load!(c"XSync" as SyncDisplay),
                query_pointer: load!(c"XQueryPointer" as QueryPointer),
                get_geometry: load!(c"XGetGeometry" as GetGeometry),
                intern_atom: load!(c"XInternAtom" as InternAtom),
                get_property: load!(c"XGetWindowProperty" as GetProperty),
                free: load!(c"XFree" as Free),
                default_root: load!(c"XDefaultRootWindow" as DefaultRoot),
            })
        }
    })
    .as_ref()
}

/// Takes an X error on our connection (a window that just went) instead
/// of Xlib's own handler, which ends the process.
unsafe extern "C" fn ignore_error(_display: *mut c_void, _event: *mut c_void) -> c_int {
    0
}

/// Run `f` on our connection, with its X errors ignored until every
/// request it made was answered (`XSync`); then the handler before is back.
/// `None` without an X server.
fn with_display<R>(f: impl FnOnce(&Xlib, *mut c_void) -> R) -> Option<R> {
    let x = xlib()?;
    let display = x.display as *mut c_void;
    // SAFETY: the display is open for the life of the process and used on
    // this thread only.
    unsafe {
        let before = (x.set_error_handler)(Some(ignore_error));
        let result = f(x, display);
        (x.sync)(display, 0);
        (x.set_error_handler)(before);
        Some(result)
    }
}

/// Make window `window` transient for `parent`.
pub(crate) fn set_transient(window: Xid, parent: Xid) {
    // SAFETY: plain request on our open display.
    with_display(|x, display| unsafe { (x.set_transient)(display, window, parent) });
}

/// Where the pointer is from window `window`'s top left corner, in pixels,
/// and the window's width in pixels. `None` without an X server, for a
/// window that is gone, or while the pointer is on another screen.
pub(crate) fn pointer_over(window: Xid) -> Option<((i32, i32), u32)> {
    with_display(|x, display| {
        let (mut root, mut child) = (0, 0);
        let (mut root_x, mut root_y, mut at_x, mut at_y) = (0, 0, 0, 0);
        let mut mask = 0;
        let (mut gx, mut gy, mut width, mut height, mut border, mut depth) = (0, 0, 0, 0, 0, 0);
        // SAFETY: plain queries on our open display into locals.
        let (same_screen, measured) = unsafe {
            let same = (x.query_pointer)(
                display,
                window,
                &raw mut root,
                &raw mut child,
                &raw mut root_x,
                &raw mut root_y,
                &raw mut at_x,
                &raw mut at_y,
                &raw mut mask,
            );
            let measured = (x.get_geometry)(
                display,
                window,
                &raw mut root,
                &raw mut gx,
                &raw mut gy,
                &raw mut width,
                &raw mut height,
                &raw mut border,
                &raw mut depth,
            );
            (same, measured)
        };
        (same_screen != 0 && measured != 0).then_some(((at_x, at_y), width))
    })
    .flatten()
}

/// Whether the window the window manager has active is one of this
/// process's (`_NET_ACTIVE_WINDOW`, `_NET_WM_PID`): another program's
/// window may lie over ours. `None` if it does not say.
pub(crate) fn active_window_is_ours() -> Option<bool> {
    with_display(|x, display| {
        // SAFETY: plain queries on our open display.
        unsafe {
            let root = (x.default_root)(display);
            let active = property(x, display, root, c"_NET_ACTIVE_WINDOW", XA_WINDOW)?;
            if active == 0 {
                return Some(false);
            }
            // Chromium names its process on every window it makes.
            let pid = property(x, display, active, c"_NET_WM_PID", XA_CARDINAL);
            Some(pid == Some(c_ulong::from(std::process::id())))
        }
    })
    .flatten()
}

/// The first value of 32-bit property `name` of type `kind` on `window`.
///
/// # Safety
///
/// `display` is `x`'s open display, on this thread.
unsafe fn property(
    x: &Xlib,
    display: *mut c_void,
    window: Xid,
    name: &CStr,
    kind: Xid,
) -> Option<c_ulong> {
    // Only an atom that exists: a window manager that set it.
    let atom = unsafe { (x.intern_atom)(display, name.as_ptr(), 1) };
    if atom == 0 {
        return None;
    }
    let (mut actual, mut format, mut items, mut after) = (0, 0, 0, 0);
    let mut data: *mut c_uchar = std::ptr::null_mut();
    let status = unsafe {
        (x.get_property)(
            display,
            window,
            atom,
            0,
            1,
            0,
            kind,
            &raw mut actual,
            &raw mut format,
            &raw mut items,
            &raw mut after,
            &raw mut data,
        )
    };
    // Xlib hands 32-bit values as longs.
    let value = (status == 0 && format == 32 && items > 0 && !data.is_null())
        .then(|| unsafe { data.cast::<c_ulong>().read_unaligned() });
    if !data.is_null() {
        unsafe { (x.free)(data.cast()) };
    }
    value
}
