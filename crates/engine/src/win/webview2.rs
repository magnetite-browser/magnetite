//! Pages in WebView2, Edge's engine, which ships with Windows (`system.rs`):
//! one WebView2 controller per page, in a plain child window of ours that
//! fills the page's own window (`attach`). WebView2 plays protected video
//! with `PlayReady` and Edge's Widevine, which services that refuse
//! Magnetite's Chromium accept.
//!
//! Everything here runs on CEF's UI thread, which pumps the thread's
//! messages: WebView2 reports there. The views are kept in a table of this
//! thread, by page id: the engine's own state holds none of them.
//!
//! The loader is linked statically (`WebView2LoaderStatic.lib`, from the
//! SDK the bindings carry), so Magnetite needs no DLL of it and starts
//! where WebView2 is missing: `available` then says no.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use cef::{KeyEvent, KeyEventType};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_COLOR, COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN,
    COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN, COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC,
    ICoreWebView2, ICoreWebView2_15, ICoreWebView2Controller, ICoreWebView2Controller2,
    ICoreWebView2Environment, ICoreWebView2Environment10, ICoreWebView2EnvironmentOptions,
};
use webview2_com::{
    AcceleratorKeyPressedEventHandler, CoreWebView2EnvironmentOptions,
    CreateCoreWebView2ControllerCompletedHandler, CreateCoreWebView2EnvironmentCompletedHandler,
    DocumentTitleChangedEventHandler, FaviconChangedEventHandler, FocusChangedEventHandler,
    HistoryChangedEventHandler, NavigationCompletedEventHandler, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler, SourceChangedEventHandler, WindowCloseRequestedEventHandler,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, HWND_TOP, MoveWindow,
    RegisterClassW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos, WINDOW_EX_STYLE,
    WM_MOVE, WM_SIZE, WM_WINDOWPOSCHANGED, WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_VISIBLE,
};
use windows_core::{BOOL, HRESULT, HSTRING, Interface, PCWSTR, PWSTR, w};

use crate::engine::Engine;
use crate::events::ContentId;

#[link(name = "WebView2LoaderStatic", kind = "static")]
unsafe extern "system" {
    #[link_name = "CreateCoreWebView2EnvironmentWithOptions"]
    fn create_environment(
        browser_folder: PCWSTR,
        user_data_folder: PCWSTR,
        options: *mut c_void,
        handler: *mut c_void,
    ) -> HRESULT;
    #[link_name = "GetAvailableCoreWebView2BrowserVersionString"]
    fn available_version(browser_folder: PCWSTR, version: *mut PWSTR) -> HRESULT;
}

/// The oldest WebView2 that has everything used here: profiles by name in
/// one data folder, and deleting one.
const OLDEST: (u32, u32) = (120, 2210);

/// Whether WebView2 is installed, in a version that has what Magnetite uses.
pub(crate) fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let mut version = PWSTR::null();
        // SAFETY: the loader writes a string it allocated, or nothing.
        let found = unsafe { available_version(PCWSTR::null(), &raw mut version) }.is_ok();
        let text = owned(version);
        let ok = found && text.as_deref().is_some_and(new_enough);
        crate::trace(&format!("system engine: WebView2 {}", if ok { "available" } else { "missing or too old" }));
        ok
    })
}

/// Whether WebView2 `version` ("157.0.4341.60") is `OLDEST` or newer.
fn new_enough(version: &str) -> bool {
    let mut parts = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let (major, _, build) = (parts.next(), parts.next(), parts.next());
    (major.unwrap_or(0), build.unwrap_or(0)) >= OLDEST
}

/// A string WebView2 allocated, copied, and the original freed.
fn owned(text: PWSTR) -> Option<String> {
    if text.is_null() {
        return None;
    }
    // SAFETY: a NUL-terminated string the call just handed over; freed once.
    unsafe {
        let copy = text.to_string().ok();
        CoTaskMemFree(Some(text.as_ptr().cast::<c_void>().cast_const()));
        copy
    }
}

/// A string getter of WebView2's, called and its answer copied.
fn get(call: impl FnOnce(*mut PWSTR) -> windows_core::Result<()>) -> Option<String> {
    let mut text = PWSTR::null();
    call(&raw mut text).ok()?;
    owned(text)
}

/// One page.
struct Page {
    /// Where it loads first, until it has a view.
    url: String,
    profile: String,
    ephemeral: bool,
    /// The window of ours that holds the view, a child of the page's own.
    host: Option<HWND>,
    /// The page's own window, CEF's, whose size `host` follows.
    parent: Option<HWND>,
    controller: Option<ICoreWebView2Controller>,
    view: Option<ICoreWebView2>,
    /// Asked for the keyboard before the view was there.
    wants_focus: bool,
}

/// WebView2's environment: one for Magnetite's data folder.
enum Environment {
    None,
    /// Being made; the pages waiting for it.
    Coming(Vec<ContentId>),
    Ready(ICoreWebView2Environment),
    Failed,
}

thread_local! {
    static PAGES: RefCell<HashMap<ContentId, Page>> = RefCell::new(HashMap::new());
    static ENVIRONMENT: RefCell<Environment> = const { RefCell::new(Environment::None) };
    static DATA_DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Make page `id`, to load `url` in `profile`'s data. The view itself comes
/// once the page has its window (`attach`). False if WebView2 is missing.
pub(crate) fn create(id: ContentId, url: &str, data_dir: &Path, profile: &str, ephemeral: bool) -> bool {
    if !available() {
        return false;
    }
    DATA_DIR.with_borrow_mut(|d| {
        d.get_or_insert_with(|| data_dir.to_path_buf());
    });
    let page = Page {
        url: url.to_string(),
        profile: profile.to_string(),
        ephemeral,
        host: None,
        parent: None,
        controller: None,
        view: None,
        wants_focus: false,
    };
    PAGES.with_borrow_mut(|p| p.insert(id, page));
    true
}

/// The class of the windows that hold a view.
const HOST_CLASS: PCWSTR = w!("MagnetiteSystemPage");

fn register_host_class() {
    static REGISTERED: OnceLock<()> = OnceLock::new();
    REGISTERED.get_or_init(|| {
        // SAFETY: a class with our procedure, registered once for the module.
        unsafe {
            let instance = GetModuleHandleW(None).unwrap_or_default();
            let class = WNDCLASSW {
                lpfnWndProc: Some(host_procedure),
                hInstance: instance.into(),
                lpszClassName: HOST_CLASS,
                ..Default::default()
            };
            RegisterClassW(&raw const class);
        }
    });
}

/// The holder of a view: its size is the view's.
unsafe extern "system" fn host_procedure(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_SIZE {
        let controller = PAGES.with(|pages| {
            let pages = pages.try_borrow().ok()?;
            pages.values().find(|p| p.host == Some(hwnd)).and_then(|p| p.controller.clone())
        });
        if let Some(controller) = controller {
            let mut rect = RECT::default();
            // SAFETY: our own window; a controller alive in the table.
            unsafe {
                let _ = GetClientRect(hwnd, &raw mut rect);
                let _ = controller.SetBounds(rect);
            }
        }
    }
    // SAFETY: the default handling for everything else.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

/// Watches the page's own window: its holder keeps its size, and the view
/// hears when it moved (popups and the input method place themselves by it).
unsafe extern "system" fn parent_procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass: usize,
    data: usize,
) -> LRESULT {
    let id = ContentId::try_from(data).unwrap_or_default();
    match message {
        WM_SIZE => fit_host(id),
        WM_MOVE | WM_WINDOWPOSCHANGED => {
            if let Some(controller) = controller_of(id) {
                // SAFETY: a controller alive in the table.
                let _ = unsafe { controller.NotifyParentWindowPositionChanged() };
            }
        }
        _ => {}
    }
    // SAFETY: the window's own handling.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

/// Size page `id`'s holder to its parent's inside.
fn fit_host(id: ContentId) {
    let Some((host, parent)) = PAGES.with_borrow(|p| p.get(&id).and_then(|page| page.host.zip(page.parent))) else {
        return;
    };
    let mut rect = RECT::default();
    // SAFETY: two live windows of this thread.
    unsafe {
        let _ = GetClientRect(parent, &raw mut rect);
        let _ = MoveWindow(host, 0, 0, rect.right - rect.left, rect.bottom - rect.top, true);
    }
}

fn controller_of(id: ContentId) -> Option<ICoreWebView2Controller> {
    PAGES.with_borrow(|p| p.get(&id).and_then(|page| page.controller.clone()))
}

fn view_of(id: ContentId) -> Option<ICoreWebView2> {
    PAGES.with_borrow(|p| p.get(&id).and_then(|page| page.view.clone()))
}

/// Give page `id` its place on screen: a window of ours inside the page's
/// own (`handle`, which CEF made and the engine shows, hides, moves and
/// sizes), filling it, with the view in it. As a child it moves, hides and
/// is cut round with its parent.
pub(crate) fn attach(id: ContentId, handle: cef::sys::cef_window_handle_t) {
    let parent = HWND(handle.0.cast());
    if parent.is_invalid() || !PAGES.with_borrow(|p| p.contains_key(&id)) {
        return;
    }
    register_host_class();
    let mut rect = RECT::default();
    // SAFETY: CEF's live window; a child window of our class in it.
    let host = unsafe {
        let _ = GetClientRect(parent, &raw mut rect);
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            HOST_CLASS,
            PCWSTR::null(),
            WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN,
            0,
            0,
            rect.right - rect.left,
            rect.bottom - rect.top,
            Some(parent),
            None,
            GetModuleHandleW(None).ok().map(Into::into),
            None,
        )
    };
    let Ok(host) = host else {
        crate::trace(&format!("system page #{id}: no window for it"));
        return;
    };
    // SAFETY: our window, above the page window's own children; the watch
    // on the parent ends in `close`.
    unsafe {
        let _ = SetWindowPos(host, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        let _ = SetWindowSubclass(parent, Some(parent_procedure), SUBCLASS, usize::try_from(id).unwrap_or(0));
    }
    PAGES.with_borrow_mut(|p| {
        if let Some(page) = p.get_mut(&id) {
            page.host = Some(host);
            page.parent = Some(parent);
        }
    });
    with_environment(id);
}

/// The id of our watch on a page's own window.
const SUBCLASS: usize = 0x4D47_5732;

/// Make the view of page `id` once the environment is there.
fn with_environment(id: ContentId) {
    let (ready, start) = ENVIRONMENT.with_borrow_mut(|env| match env {
        Environment::Ready(env) => (Some(env.clone()), false),
        Environment::Coming(waiting) => {
            waiting.push(id);
            (None, false)
        }
        Environment::Failed => (None, false),
        Environment::None => {
            *env = Environment::Coming(vec![id]);
            (None, true)
        }
    });
    // Outside the table: the loader may report within the call.
    if start {
        start_environment();
    }
    if let Some(env) = ready {
        make_view(&env, id);
    }
}

/// Where WebView2 keeps the data of Magnetite's system pages.
fn user_data_folder() -> Option<PathBuf> {
    DATA_DIR.with_borrow(|d| d.as_ref().map(|d| d.join("system-engine")))
}

fn start_environment() {
    let Some(folder) = user_data_folder() else { return };
    let options: ICoreWebView2EnvironmentOptions = CoreWebView2EnvironmentOptions::default().into();
    let done = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(|result, env| {
        let waiting = ENVIRONMENT.with_borrow_mut(|state| {
            let waiting = match state {
                Environment::Coming(w) => std::mem::take(w),
                _ => Vec::new(),
            };
            *state = match (result, env) {
                (Ok(()), Some(env)) => Environment::Ready(env),
                _ => {
                    crate::trace("system engine: WebView2 could not start");
                    Environment::Failed
                }
            };
            waiting
        });
        let env = ENVIRONMENT.with_borrow(|s| match s {
            Environment::Ready(env) => Some(env.clone()),
            _ => None,
        });
        for id in waiting {
            match &env {
                Some(env) => make_view(env, id),
                None => Engine::global().system_failed(id, page_url(id), false),
            }
        }
        Ok(())
    }));
    let folder = HSTRING::from(folder.as_os_str());
    // SAFETY: the loader keeps references to the options and the handler.
    let started = unsafe { create_environment(PCWSTR::null(), PCWSTR(folder.as_ptr()), options.as_raw(), done.as_raw()) };
    if started.is_err() {
        crate::trace("system engine: WebView2 could not start");
        ENVIRONMENT.with_borrow_mut(|s| *s = Environment::Failed);
    }
}

fn page_url(id: ContentId) -> String {
    PAGES.with_borrow(|p| p.get(&id).map(|page| page.url.clone()).unwrap_or_default())
}

/// The name of `profile`'s WebView2 profile. TODO: kept in a file, as on
/// macOS; for the spike, derived from the id.
fn profile_name(profile: &str, ephemeral: bool) -> String {
    let base: String = profile.chars().filter(char::is_ascii_alphanumeric).take(40).collect();
    match (ephemeral, base.is_empty()) {
        (true, _) => format!("private-{base}"),
        (false, true) => "default".to_string(),
        (false, false) => format!("p-{base}"),
    }
}

fn make_view(env: &ICoreWebView2Environment, id: ContentId) {
    let Some((host, profile, ephemeral)) =
        PAGES.with_borrow(|p| p.get(&id).and_then(|page| page.host.map(|h| (h, page.profile.clone(), page.ephemeral))))
    else {
        return;
    };
    let done = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(move |result, controller| {
        match (result, controller) {
            (Ok(()), Some(controller)) => view_made(id, controller),
            _ => {
                crate::trace(&format!("system page #{id}: WebView2 made no view"));
                Engine::global().system_failed(id, page_url(id), false);
            }
        }
        Ok(())
    }));
    // SAFETY: a live environment and window; WebView2 keeps the handler.
    let asked = unsafe {
        env.cast::<ICoreWebView2Environment10>().and_then(|env10| {
            let options = env10.CreateCoreWebView2ControllerOptions()?;
            options.SetProfileName(&HSTRING::from(profile_name(&profile, ephemeral)))?;
            options.SetIsInPrivateModeEnabled(ephemeral)?;
            env10.CreateCoreWebView2ControllerWithOptions(host, &options, &done)
        })
    };
    if asked.is_err() {
        crate::trace(&format!("system page #{id}: WebView2 made no view"));
    }
}

/// Page `id`'s view is there: it reports to the engine, fills its holder,
/// and loads its first address.
fn view_made(id: ContentId, controller: ICoreWebView2Controller) {
    // The page may have closed while the view was being made.
    if !PAGES.with_borrow(|p| p.contains_key(&id)) {
        // SAFETY: a controller of ours nobody else holds.
        let _ = unsafe { controller.Close() };
        return;
    }
    // SAFETY: a live controller, on the thread it was made on.
    let view = unsafe { controller.CoreWebView2() };
    let Ok(view) = view else { return };
    if let Ok(c2) = controller.cast::<ICoreWebView2Controller2>() {
        // The page's own background until it paints: the frame's dark.
        let dark = COREWEBVIEW2_COLOR { A: 255, R: 0x1B, G: 0x1B, B: 0x1E };
        // SAFETY: as above.
        let _ = unsafe { c2.SetDefaultBackgroundColor(dark) };
    }
    let (url, wants_focus) = PAGES.with_borrow_mut(|p| {
        let Some(page) = p.get_mut(&id) else { return (String::new(), false) };
        page.controller = Some(controller.clone());
        page.view = Some(view.clone());
        (page.url.clone(), page.wants_focus)
    });
    listen(id, &controller, &view);
    fit_host(id);
    if let Some(host) = PAGES.with_borrow(|p| p.get(&id).and_then(|page| page.host)) {
        let mut rect = RECT::default();
        // SAFETY: our window and a live controller.
        unsafe {
            let _ = GetClientRect(host, &raw mut rect);
            let _ = controller.SetBounds(rect);
            let _ = controller.SetIsVisible(true);
        }
    }
    load(id, &url);
    if wants_focus {
        focus(id);
    }
}

/// Whether `url` is an `http://` or `https://` address: all a page's main
/// frame may be sent to.
fn is_web_address(url: &str) -> bool {
    (url.split_once(':')).is_some_and(|(scheme, _)| {
        scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
    })
}

/// What page `id` reports, as the engine's events.
fn listen(id: ContentId, controller: &ICoreWebView2Controller, view: &ICoreWebView2) {
    let mut token = 0i64;
    // SAFETY: live objects; WebView2 keeps each handler until the view is
    // closed (`close`).
    unsafe {
        let _ = view.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let url = get(|u| args.Uri(u)).unwrap_or_default();
                if !is_web_address(&url) {
                    args.SetCancel(true)?;
                    return Ok(());
                }
                let mut by_user = BOOL::default();
                args.IsUserInitiated(&raw mut by_user)?;
                if by_user.as_bool() && Engine::global().system_leaves_lock(id, &url) {
                    args.SetCancel(true)?;
                    return Ok(());
                }
                Engine::global().system_started(id, &url);
                report_loading(id, true);
                Ok(())
            })),
            &raw mut token,
        );
        let _ = view.add_SourceChanged(
            &SourceChangedEventHandler::create(Box::new(move |view, _| {
                if let Some(url) = view.and_then(|v| get(|u| v.Source(u))).filter(|u| !u.is_empty()) {
                    Engine::global().system_url(id, url);
                }
                Ok(())
            })),
            &raw mut token,
        );
        let _ = view.add_DocumentTitleChanged(
            &DocumentTitleChangedEventHandler::create(Box::new(move |view, _| {
                if let Some(title) = view.and_then(|v| get(|t| v.DocumentTitle(t))) {
                    Engine::global().system_title(id, title);
                }
                Ok(())
            })),
            &raw mut token,
        );
        let _ = view.add_HistoryChanged(
            &HistoryChangedEventHandler::create(Box::new(move |_, _| {
                report_loading(id, false);
                Ok(())
            })),
            &raw mut token,
        );
        let _ = view.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |_, _| {
                report_loading(id, false);
                Ok(())
            })),
            &raw mut token,
        );
        let _ = view.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                args.SetHandled(true)?;
                let url = get(|u| args.Uri(u)).unwrap_or_default();
                if is_web_address(&url) {
                    Engine::global().system_open(id, url, false, false);
                }
                Ok(())
            })),
            &raw mut token,
        );
        let _ = view.add_WindowCloseRequested(
            &WindowCloseRequestedEventHandler::create(Box::new(move |_, _| {
                Engine::post(Box::new(move || Engine::global().close_content(id)));
                Ok(())
            })),
            &raw mut token,
        );
        if let Ok(view15) = view.cast::<ICoreWebView2_15>() {
            let _ = view15.add_FaviconChanged(
                &FaviconChangedEventHandler::create(Box::new(move |view, _| {
                    let icon = view
                        .and_then(|v| v.cast::<ICoreWebView2_15>().ok())
                        .and_then(|v| get(|u| v.FaviconUri(u)))
                        .filter(|u| u.starts_with("https://"));
                    if let Some(icon) = icon {
                        Engine::global().system_favicon(id, &icon);
                    }
                    Ok(())
                })),
                &raw mut token,
            );
        }
        let _ = controller.add_GotFocus(
            &FocusChangedEventHandler::create(Box::new(move |_, _| {
                Engine::post(Box::new(move || Engine::global().system_focused(id)));
                Ok(())
            })),
            &raw mut token,
        );
        let _ = controller.add_AcceleratorKeyPressed(
            &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
                let mut key = 0u32;
                args.KeyEventKind(&raw mut kind)?;
                args.VirtualKey(&raw mut key)?;
                let down = kind == COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN || kind == COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN;
                if down && Engine::global().system_key(id, &key_event(key)) {
                    args.SetHandled(true)?;
                }
                Ok(())
            })),
            &raw mut token,
        );
    }
}

/// Tell the engine page `id` loads (or not), and where its history leads.
fn report_loading(id: ContentId, loading: bool) {
    let Some(view) = view_of(id) else { return };
    let (mut back, mut forward) = (BOOL::default(), BOOL::default());
    // SAFETY: a live view.
    unsafe {
        let _ = view.CanGoBack(&raw mut back);
        let _ = view.CanGoForward(&raw mut forward);
    }
    Engine::global().system_loading(id, loading, back.as_bool(), forward.as_bool());
}

/// A key pressed in a page, as the engine's shortcut tables read keys:
/// the Windows key code, CEF's modifier flags.
fn key_event(virtual_key: u32) -> KeyEvent {
    use cef::sys::cef_event_flags_t as Flag;
    // SAFETY: the keyboard state of this thread, which got the key.
    let held = |key| unsafe { GetKeyState(i32::from(key)) } < 0;
    let mut modifiers = 0;
    for (key, flag) in [
        (VK_CONTROL.0, Flag::EVENTFLAG_CONTROL_DOWN),
        (VK_SHIFT.0, Flag::EVENTFLAG_SHIFT_DOWN),
        (VK_MENU.0, Flag::EVENTFLAG_ALT_DOWN),
    ] {
        if held(key) {
            modifiers |= flag.0;
        }
    }
    KeyEvent {
        type_: KeyEventType::RAWKEYDOWN,
        #[allow(clippy::cast_sign_loss, clippy::unnecessary_cast)]
        modifiers: modifiers as u32,
        windows_key_code: i32::try_from(virtual_key).unwrap_or(0),
        ..Default::default()
    }
}

/// Page `id` is gone: its view closes, its holder goes, and the watch on its
/// window ends.
pub(crate) fn close(id: ContentId) {
    let Some(page) = PAGES.with_borrow_mut(|p| p.remove(&id)) else {
        return;
    };
    // SAFETY: what the page held, alive until here.
    unsafe {
        if let Some(controller) = page.controller {
            let _ = controller.Close();
        }
        if let Some(parent) = page.parent {
            let _ = RemoveWindowSubclass(parent, Some(parent_procedure), SUBCLASS);
        }
        if let Some(host) = page.host {
            let _ = DestroyWindow(host);
        }
    }
}

pub(crate) fn load(id: ContentId, url: &str) {
    if !is_web_address(url) {
        crate::trace(&format!("system page #{id}: not a web address, not loaded"));
        return;
    }
    match view_of(id) {
        // SAFETY: a live view.
        Some(view) => {
            let _ = unsafe { view.Navigate(&HSTRING::from(url)) };
        }
        None => PAGES.with_borrow_mut(|p| {
            if let Some(page) = p.get_mut(&id) {
                page.url = url.to_string();
            }
        }),
    }
}

pub(crate) fn go_back(id: ContentId) {
    if let Some(view) = view_of(id) {
        // SAFETY: a live view.
        let _ = unsafe { view.GoBack() };
    }
}

pub(crate) fn go_forward(id: ContentId) {
    if let Some(view) = view_of(id) {
        // SAFETY: a live view.
        let _ = unsafe { view.GoForward() };
    }
}

pub(crate) fn reload(id: ContentId) {
    if let Some(view) = view_of(id) {
        // SAFETY: a live view.
        let _ = unsafe { view.Reload() };
    }
}

pub(crate) fn stop(id: ContentId) {
    if let Some(view) = view_of(id) {
        // SAFETY: a live view.
        let _ = unsafe { view.Stop() };
    }
}

/// Give page `id` the keyboard.
pub(crate) fn focus(id: ContentId) {
    match controller_of(id) {
        // SAFETY: a live controller.
        Some(controller) => {
            let _ = unsafe { controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC) };
        }
        None => PAGES.with_borrow_mut(|p| {
            if let Some(page) = p.get_mut(&id) {
                page.wants_focus = true;
            }
        }),
    }
}

/// Page `id`'s zoom in percent.
pub(crate) fn zoom(id: ContentId) -> Option<u32> {
    let controller = controller_of(id)?;
    let mut factor = 1.0;
    // SAFETY: a live controller.
    unsafe { controller.ZoomFactor(&raw mut factor) }.ok()?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some((factor * 100.0).round().clamp(1.0, 1000.0) as u32)
}

pub(crate) fn set_zoom(id: ContentId, percent: u32) {
    if let Some(controller) = controller_of(id) {
        // SAFETY: a live controller.
        let _ = unsafe { controller.SetZoomFactor(f64::from(percent) / 100.0) };
    }
}

/// Whether page `id` came over an encrypted connection; `None` for a page
/// without a network address.
pub(crate) fn secure(id: ContentId) -> Option<bool> {
    let url = address_of(id)?;
    if url.starts_with("https://") {
        Some(true)
    } else if url.starts_with("http://") {
        Some(false)
    } else {
        None
    }
}

/// The address page `id` shows.
pub(crate) fn address_of(id: ContentId) -> Option<String> {
    let view = view_of(id)?;
    // SAFETY: a live view.
    get(|u| unsafe { view.Source(u) }).filter(|u| !u.is_empty() && u != "about:blank")
}

// TODO(spike): the icon fetch, the data stores and clearing them.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn fetch_icon(_: &str, _: PathBuf) {}
pub(crate) fn forget_stores(_: &Path, _: &[String]) {}
pub(crate) fn release_stores(_: &[String]) {}
pub(crate) fn clear_data(
    _: &Path,
    _: &str,
    _: bool,
    _: Option<&str>,
    done: Box<dyn FnOnce(Result<Option<usize>, String>)>,
) {
    done(Ok(None));
}

#[cfg(test)]
mod tests {
    use super::new_enough;

    #[test]
    fn a_runtime_must_have_profiles_by_name() {
        assert!(new_enough("157.0.4341.60"));
        assert!(new_enough("120.0.2210.55"));
        assert!(!new_enough("120.0.2209.0"));
        assert!(!new_enough("109.0.1518.140"));
        assert!(!new_enough(""));
    }
}
