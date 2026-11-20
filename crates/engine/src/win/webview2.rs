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
//! WebView2 keeps its data in one folder inside Magnetite's data folder
//! (`<data dir>/system-engine`), with a WebView2 profile per Magnetite
//! profile under a name kept in `<data dir>/system-engine-stores.json`. A
//! private profile's pages are `InPrivate`: in memory only.
//!
//! The loader is linked statically (`WebView2LoaderStatic.lib`, from the
//! SDK the bindings carry), so Magnetite needs no DLL of it and starts
//! where WebView2 is missing: `available` then says no.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use cef::{KeyEvent, KeyEventType};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_COLOR, COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN,
    COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN, COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC,
    COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
    COREWEBVIEW2_WEB_ERROR_STATUS, COREWEBVIEW2_WEB_ERROR_STATUS_CANNOT_CONNECT,
    COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_COMMON_NAME_IS_INCORRECT,
    COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_EXPIRED,
    COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_IS_INVALID,
    COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_REVOKED,
    COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED, COREWEBVIEW2_WEB_ERROR_STATUS_DISCONNECTED,
    COREWEBVIEW2_WEB_ERROR_STATUS_HOST_NAME_NOT_RESOLVED,
    COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED, COREWEBVIEW2_WEB_ERROR_STATUS_TIMEOUT,
    ICoreWebView2, ICoreWebView2_2, ICoreWebView2_4, ICoreWebView2_13, ICoreWebView2_15,
    ICoreWebView2Controller, ICoreWebView2Controller2, ICoreWebView2Environment,
    ICoreWebView2Environment10, ICoreWebView2EnvironmentOptions, ICoreWebView2Profile2,
    ICoreWebView2Settings8,
};
use webview2_com::{
    AcceleratorKeyPressedEventHandler, CallDevToolsProtocolMethodCompletedHandler,
    ClearBrowsingDataCompletedHandler, ContainsFullScreenElementChangedEventHandler,
    CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, DocumentTitleChangedEventHandler,
    DownloadStartingEventHandler, FaviconChangedEventHandler, FocusChangedEventHandler,
    GetCookiesCompletedHandler, HistoryChangedEventHandler, NavigationCompletedEventHandler,
    NavigationStartingEventHandler, NewWindowRequestedEventHandler, ProcessFailedEventHandler,
    SourceChangedEventHandler, WindowCloseRequestedEventHandler, ZoomFactorChangedEventHandler,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::Com::{CoCreateGuid, CoTaskMemFree};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, HWND_TOP, IsWindowVisible,
    MoveWindow, RegisterClassW, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SetWindowPos, WINDOW_EX_STYLE, WINDOWPOS, WM_MOVE, WM_SIZE,
    WM_WINDOWPOSCHANGED, WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_POPUP, WS_VISIBLE,
};
use windows_core::{BOOL, HRESULT, HSTRING, Interface, PCWSTR, PWSTR, w};

use crate::engine::Engine;
use crate::events::ContentId;

pub(crate) use super::icon_fetch::fetch_icon;

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
/// one data folder, and clearing a profile's data.
const OLDEST: (u32, u32) = (120, 2210);

/// Whether WebView2 is installed, in a version that has what Magnetite uses.
pub(crate) fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let mut version = PWSTR::null();
        // SAFETY: the loader writes a string it allocated, or nothing.
        let found = unsafe { available_version(PCWSTR::null(), &raw mut version) }.is_ok();
        let ok = found && owned(version).as_deref().is_some_and(new_enough);
        crate::trace(if ok {
            "system engine: WebView2 is there"
        } else {
            "system engine: WebView2 is missing or too old"
        });
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
    /// Where it loads first, until it has a view; then the address it was
    /// last at, to load again if WebView2 has to start anew.
    url: String,
    /// The Magnetite profile it belongs to (`""`: the default one), the
    /// WebView2 profile it is in (`store`), and whether that is `InPrivate`.
    profile: String,
    store: String,
    private: bool,
    /// The window of ours that holds the view, a child of the page's own.
    host: Option<HWND>,
    /// The page's own window, CEF's, whose size `host` follows.
    parent: Option<HWND>,
    controller: Option<ICoreWebView2Controller>,
    view: Option<ICoreWebView2>,
    /// Asked for the keyboard before the view was there.
    wants_focus: bool,
    /// The address that could not be loaded, while the page shows the
    /// note about it: the page stands at that address.
    failed: Option<String>,
    /// The failed load was refused for its certificate.
    failed_certificate: bool,
    /// The note about a failed load is about to load: let it.
    note_coming: bool,
    /// The navigation the page is at (WebView2's id), and whether it is
    /// still loading: what an older navigation reports changes neither.
    navigation: u64,
    loading: bool,
    /// When the page's process last ended by itself.
    crashed: Option<Instant>,
}

/// WebView2's environment: one for Magnetite's data folder.
enum Environment {
    None,
    /// Being made; what waits for it.
    Coming(Vec<Waiting>),
    Ready(ICoreWebView2Environment),
    Failed,
}

/// What waits for the environment.
enum Waiting {
    Page(ContentId),
    Job(Box<dyn FnOnce(Option<ICoreWebView2Environment>)>),
}

thread_local! {
    static PAGES: RefCell<HashMap<ContentId, Page>> = RefCell::new(HashMap::new());
    static ENVIRONMENT: RefCell<Environment> = const { RefCell::new(Environment::None) };
    static DATA_DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    /// The `InPrivate` store of each profile whose data is in memory this
    /// run: all its pages share one, as a private space's pages share one
    /// session.
    static IN_MEMORY: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
    /// Which environment the views belong to; a new one after WebView2's own
    /// process ended. What a view of an older one reports is not acted on.
    static GENERATION: Cell<u64> = const { Cell::new(0) };
    /// When WebView2's own process last ended by itself.
    static BROWSER_ENDED: Cell<Option<Instant>> = const { Cell::new(None) };
}

// ---- data stores ----

/// The file that names each profile's WebView2 profile: profile id → name.
fn stores_file(data_dir: &Path) -> PathBuf {
    data_dir.join("system-engine-stores.json")
}

/// The folder WebView2 keeps everything in.
fn user_data_folder(data_dir: &Path) -> PathBuf {
    data_dir.join("system-engine")
}

/// Where WebView2 keeps the profile `name` in `folder` (seen on Windows 11,
/// WebView2 157).
fn profile_folder(folder: &Path, name: &str) -> PathBuf {
    folder.join("EBWebView").join(format!("WV2Profile_{name}"))
}

/// The names in the file; none where there is no file yet. A file that
/// cannot be read, or holds something else, is an error and not an empty
/// list: its profiles exist, and nothing may act as if they did not.
fn read_store_names(data_dir: &Path) -> std::io::Result<BTreeMap<String, String>> {
    match std::fs::read(stores_file(data_dir)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(std::io::Error::other),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e),
    }
}

/// What a caller hears when the file of names cannot be read.
const STORES_UNREADABLE: &str = "The list of WebView2's data stores could not be read.";

/// Keep `names` in the file. False if they could not be written.
fn write_store_names(data_dir: &Path, names: &BTreeMap<String, String>) -> bool {
    let path = stores_file(data_dir);
    let tmp = path.with_extension("tmp");
    let written = serde_json::to_vec_pretty(names)
        .ok()
        .and_then(|json| std::fs::write(&tmp, json).ok())
        .and_then(|()| std::fs::rename(&tmp, &path).ok());
    if written.is_none() {
        crate::trace("system engine: cannot keep the list of its data stores");
    }
    written.is_some()
}

/// A new profile name: `m` and 32 hex digits, which WebView2 takes as a
/// profile name and Windows as a folder name.
fn fresh_name() -> Option<String> {
    // SAFETY: a plain call that fills in a GUID.
    let guid = unsafe { CoCreateGuid() }.ok()?;
    Some(format!("m{:032x}", guid.to_u128()))
}

/// The WebView2 profile of `profile` (`""`: the default profile), and
/// whether it is `InPrivate`. On disk under its kept name, made and kept on
/// first use. In memory only for a private profile, and for this run where
/// a name could not be kept or the file not read: a profile under a name
/// that is written nowhere could never be found again, to open or delete.
fn store(data_dir: &Path, profile: &str, ephemeral: bool) -> (String, bool) {
    let in_memory = || {
        let name = IN_MEMORY.with_borrow_mut(|names| {
            names
                .entry(profile.to_string())
                .or_insert_with(|| format!("private-{}", fresh_name().unwrap_or_default()))
                .clone()
        });
        (name, true)
    };
    if ephemeral {
        return in_memory();
    }
    let Ok(mut names) = read_store_names(data_dir) else {
        crate::trace("system engine: the list of its data stores cannot be read");
        return in_memory();
    };
    if let Some(name) = names.get(profile) {
        return (name.clone(), false);
    }
    match fresh_name() {
        Some(name) => {
            names.insert(profile.to_string(), name.clone());
            if write_store_names(data_dir, &names) {
                (name, false)
            } else {
                in_memory()
            }
        }
        None => in_memory(),
    }
}

/// Whether WebView2 runs in this run: its profiles' folders are in use.
fn started() -> bool {
    !ENVIRONMENT.with_borrow(|e| matches!(e, Environment::None))
}

/// The profiles `ids` are gone: so are their WebView2 profiles. Asked at
/// startup, before any page, while nothing has the folders open. A name
/// leaves the file only once its folder is gone; one that could not be
/// removed is asked for again at the next start, when the app names its
/// deleted profiles again.
pub(crate) fn forget_stores(data_dir: &Path, ids: &[String]) {
    if ids.is_empty() || started() {
        return;
    }
    let Ok(mut names) = read_store_names(data_dir) else {
        crate::trace("system engine: the list of its data stores cannot be read");
        return;
    };
    let folder = user_data_folder(data_dir);
    let mut changed = false;
    for profile in ids {
        let Some(name) = names.get(profile).cloned() else {
            continue;
        };
        let path = profile_folder(&folder, &name);
        let gone = match std::fs::remove_dir_all(&path) {
            Ok(()) => true,
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
        };
        if gone {
            names.remove(profile);
            changed = true;
        } else {
            crate::trace("system engine: a profile's data store could not be removed");
        }
    }
    if changed {
        write_store_names(data_dir, &names);
    }
}

/// The private profiles `ids` have no page left. WebView2 lets go of an
/// `InPrivate` profile's data when its last view closes; a page made for
/// one of them later starts with a store of its own.
pub(crate) fn release_stores(ids: &[String]) {
    IN_MEMORY.with_borrow_mut(|names| names.retain(|profile, _| !ids.contains(profile)));
}

/// Delete what WebView2 keeps for `profile`: everything (`domain` none), or
/// the cookies of the site `domain` and its subdomains and the storage of
/// its own addresses. `done` hears how many cookies went with a site's
/// data, or why nothing was deleted: a profile whose name cannot be read is
/// not reported as cleared.
pub(crate) fn clear_data(
    data_dir: &Path,
    profile: &str,
    ephemeral: bool,
    domain: Option<&str>,
    done: Cleared,
) {
    let nothing = domain.map(|_| 0);
    if !available() {
        return done(Ok(nothing));
    }
    // The stores that hold the profile's data: each one its open pages use
    // (one, unless the list of names became unreadable during the run and
    // a page went to memory), reached through one of their views, and the
    // one on disk under its kept name where no page uses that. A run whose
    // list cannot be read must not pass a store on disk for cleared.
    let mut open: Vec<(String, ICoreWebView2)> = Vec::new();
    PAGES.with_borrow(|pages| {
        for page in pages.values().filter(|p| p.profile == profile) {
            if let Some(view) = &page.view
                && !open.iter().any(|(store, _)| *store == page.store)
            {
                open.push((page.store.clone(), view.clone()));
            }
        }
    });
    let mut on_disk = None;
    if !ephemeral {
        let Ok(names) = read_store_names(data_dir) else {
            return done(Err(STORES_UNREADABLE.to_string()));
        };
        on_disk = names
            .get(profile)
            .filter(|name| !open.iter().any(|(store, _)| store == *name))
            .cloned();
    }
    // A profile that never used WebView2 (or a private one without a page,
    // whose data went with it) has nothing to delete, and gets no store
    // made for the question.
    if open.is_empty() && on_disk.is_none() {
        return done(Ok(nothing));
    }
    let left = open.len() + usize::from(on_disk.is_some());
    let all = Rc::new(RefCell::new(Combined {
        left,
        cookies: 0,
        failed: None,
        done: Some(done),
    }));
    let one = move |result: Result<Option<usize>, String>| {
        let mut all = all.borrow_mut();
        match result {
            Ok(count) => all.cookies += count.unwrap_or(0),
            Err(why) => all.failed = Some(why),
        }
        all.left -= 1;
        if all.left == 0
            && let Some(done) = all.done.take()
        {
            let result = match all.failed.take() {
                Some(why) => Err(why),
                None => Ok(nothing.map(|_| all.cookies)),
            };
            drop(all);
            done(result);
        }
    };
    let domain = domain.map(str::to_string);
    for (_, view) in open {
        crate::trace("system engine: clearing data through an open page");
        clear_with(&view, domain.clone(), Box::new(one.clone()));
    }
    let Some(name) = on_disk else { return };
    DATA_DIR.with_borrow_mut(|d| {
        d.get_or_insert_with(|| data_dir.to_path_buf());
    });
    // A view of the profile's own, out of sight, for the time it takes.
    crate::trace("system engine: clearing data through a hidden view");
    with_scratch_view(
        name,
        Box::new(move |view| match view {
            Some((view, finish)) => clear_with(
                &view,
                domain,
                Box::new(move |result| {
                    finish();
                    one(result);
                }),
            ),
            None => one(Err(
                "WebView2 could not open the profile's data.".to_string()
            )),
        }),
    );
}

/// The answers of a profile's stores, gathered for one answer.
struct Combined {
    left: usize,
    cookies: usize,
    failed: Option<String>,
    done: Option<Cleared>,
}

/// What hears whether data was deleted: how many cookies went, where that
/// is counted.
type Cleared = Box<dyn FnOnce(Result<Option<usize>, String>)>;

/// The answer when WebView2 deleted nothing.
fn not_cleared() -> Result<Option<usize>, String> {
    Err("WebView2 could not delete the data.".to_string())
}

/// Delete through `view` what its profile keeps (`domain` none), or what
/// belongs to the site `domain`. `done` hears of it exactly once, also
/// when WebView2 turns the request down.
fn clear_with(view: &ICoreWebView2, domain: Option<String>, done: Cleared) {
    let done = Rc::new(RefCell::new(Some(done)));
    let finish = {
        let done = done.clone();
        move |result: Result<Option<usize>, String>| {
            if let Some(done) = done.borrow_mut().take() {
                done(result);
            }
        }
    };
    let asked = match domain {
        None => clear_everything(view, finish.clone()),
        Some(domain) => clear_site(view, domain, finish.clone()),
    };
    if let Err(e) = asked {
        crate::trace(&format!(
            "system engine: data could not be cleared ({})",
            e.code().0
        ));
        finish(not_cleared());
    }
}

/// Everything the view's profile keeps.
fn clear_everything(
    view: &ICoreWebView2,
    finish: impl Fn(Result<Option<usize>, String>) + 'static,
) -> windows_core::Result<()> {
    // SAFETY: a live view; WebView2 keeps the handler until it reports.
    unsafe {
        let profile = view
            .cast::<ICoreWebView2_13>()?
            .Profile()?
            .cast::<ICoreWebView2Profile2>()?;
        profile.ClearBrowsingDataAll(&ClearBrowsingDataCompletedHandler::create(Box::new(
            move |result| {
                finish(result.map(|()| None).or_else(|_| not_cleared()));
                Ok(())
            },
        )))
    }
}

/// The cookies of `domain` and its subdomains, counted and deleted one by
/// one, then the storage of its own addresses.
fn clear_site(
    view: &ICoreWebView2,
    domain: String,
    finish: impl Fn(Result<Option<usize>, String>) + Clone + 'static,
) -> windows_core::Result<()> {
    // SAFETY: a live view; WebView2 keeps the handler until it reports.
    unsafe {
        let manager = view.cast::<ICoreWebView2_2>()?.CookieManager()?;
        let (view, deleter) = (view.clone(), manager.clone());
        manager.GetCookies(
            w!(""),
            &GetCookiesCompletedHandler::create(Box::new(move |result, list| {
                let removed = result.and_then(|()| {
                    let Some(list) = list else {
                        return Ok((0, false));
                    };
                    let mut count = 0u32;
                    list.Count(&raw mut count)?;
                    let (mut removed, mut missed) = (0usize, false);
                    for index in 0..count {
                        let cookie = list.GetValueAtIndex(index)?;
                        let of_site = get(|d| cookie.Domain(d))
                            .is_some_and(|d| crate::sitedata::in_domain(&d, &domain));
                        // By name, domain and path: a cookie object from the
                        // list does not match a cookie of the host alone.
                        if of_site {
                            let name = get(|n| cookie.Name(n)).unwrap_or_default();
                            let domain = get(|d| cookie.Domain(d)).unwrap_or_default();
                            let path = get(|p| cookie.Path(p)).unwrap_or_default();
                            // The others are still tried; the answer says
                            // that one stayed.
                            if deleter
                                .DeleteCookiesWithDomainAndPath(
                                    &HSTRING::from(name),
                                    &HSTRING::from(domain),
                                    &HSTRING::from(path),
                                )
                                .is_ok()
                            {
                                removed += 1;
                            } else {
                                missed = true;
                            }
                        }
                    }
                    crate::trace(&format!(
                        "system engine: {removed} of {count} cookies are the site's"
                    ));
                    Ok((removed, missed))
                });
                if let Err(e) = &removed {
                    crate::trace(&format!("system engine: no cookies read ({})", e.code().0));
                }
                match removed {
                    // The answer once the storage is gone too: the hidden
                    // view a profile without pages is cleared through
                    // closes with it.
                    Ok((removed, missed)) => {
                        let finish = finish.clone();
                        clear_site_storage(
                            &view,
                            &domain,
                            Box::new(move |stored| {
                                finish(if missed || !stored {
                                    Err("Some of the site's data could not be deleted.".to_string())
                                } else {
                                    Ok(Some(removed))
                                });
                            }),
                        );
                    }
                    Err(_) => finish(not_cleared()),
                }
                Ok(())
            })),
        )
    }
}

/// Delete the storage (local storage, `IndexedDB`, caches, service workers)
/// of `domain` through the `DevTools` protocol of `view`, which reaches its
/// profile's data and no other: at the site's own addresses, and at those
/// of the site's pages that are open. WebView2 lists no data by site, so
/// storage of another subdomain that no page shows stays.
/// `done` hears, once every address has answered, whether all of it went.
fn clear_site_storage(view: &ICoreWebView2, domain: &str, done: Box<dyn FnOnce(bool)>) {
    let open: Vec<String> = PAGES.with_borrow(|pages| {
        (pages.values())
            .filter_map(|page| origin_of(&page.url))
            .filter(|(_, host)| crate::sitedata::in_domain(host, domain))
            .map(|(origin, _)| origin)
            .collect()
    });
    let mut origins = site_origins(domain);
    for origin in open {
        if !origins.contains(&origin) {
            origins.push(origin);
        }
    }
    let left = Rc::new(Cell::new(origins.len()));
    let failed = Rc::new(Cell::new(false));
    let done = Rc::new(RefCell::new(Some(done)));
    let answered = move |left: &Cell<usize>, failed: &Cell<bool>, ok: bool| {
        if !ok {
            failed.set(true);
        }
        left.set(left.get() - 1);
        if left.get() == 0
            && let Some(done) = done.borrow_mut().take()
        {
            done(!failed.get());
        }
    };
    for origin in origins {
        let params = serde_json::json!({ "origin": origin, "storageTypes": "all" }).to_string();
        let (l, f, a) = (left.clone(), failed.clone(), answered.clone());
        let handler =
            CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, _| {
                a(&l, &f, result.is_ok());
                Ok(())
            }));
        // SAFETY: a live view; WebView2 keeps the handler until it reports.
        let asked = unsafe {
            view.CallDevToolsProtocolMethod(
                w!("Storage.clearDataForOrigin"),
                &HSTRING::from(params),
                &handler,
            )
        };
        if asked.is_err() {
            answered(&left, &failed, false);
        }
    }
}

/// The origin of web address `url` (`scheme://host[:port]`) and its host.
fn origin_of(url: &str) -> Option<(String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    if !is_web_address(url) {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => authority,
    };
    (!host.is_empty()).then(|| {
        (
            format!("{}://{authority}", scheme.to_ascii_lowercase()),
            host.to_ascii_lowercase(),
        )
    })
}

/// The addresses a site's storage is kept under, as far as they can be
/// named without a list: the site itself and its `www.`, each over both
/// schemes.
fn site_origins(domain: &str) -> Vec<String> {
    let hosts = [domain.to_string(), format!("www.{domain}")];
    hosts
        .iter()
        .flat_map(|host| [format!("https://{host}"), format!("http://{host}")])
        .collect()
}

/// What runs with a view made for it alone, and what closes that view.
type ScratchJob = Box<dyn FnOnce(Option<(ICoreWebView2, Box<dyn FnOnce()>)>)>;

/// Make a view of the WebView2 profile `name` in a window of ours that is
/// never shown, and hand it to `job` with what closes it again; `None` if
/// none could be made.
fn with_scratch_view(name: String, job: ScratchJob) {
    register_host_class();
    // SAFETY: a window of our class, never shown, destroyed by `finish`.
    let window = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            HOST_CLASS,
            PCWSTR::null(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            GetModuleHandleW(None).ok().map(Into::into),
            None,
        )
    };
    let Ok(window) = window else { return job(None) };
    let job = Rc::new(Cell::new(Some(job)));
    on_environment(Waiting::Job(Box::new(move |env| {
        let fail = {
            let job = job.clone();
            move || {
                // SAFETY: our window.
                let _ = unsafe { DestroyWindow(window) };
                if let Some(job) = job.take() {
                    job(None);
                }
            }
        };
        let Some(env) = env else { return fail() };
        let made = {
            let job = job.clone();
            CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
                move |result, controller| {
                    let view = controller
                        .as_ref()
                        .and_then(|c| unsafe { c.CoreWebView2() }.ok());
                    match (result, controller, view, job.take()) {
                        (Ok(()), Some(controller), Some(view), Some(job)) => {
                            let finish = Box::new(move || {
                                // SAFETY: what was made here, closed once.
                                unsafe {
                                    let _ = controller.Close();
                                    let _ = DestroyWindow(window);
                                }
                            });
                            job(Some((view, finish)));
                        }
                        (_, controller, _, job) => {
                            if let Some(controller) = controller {
                                // SAFETY: as above.
                                let _ = unsafe { controller.Close() };
                            }
                            // SAFETY: our window.
                            let _ = unsafe { DestroyWindow(window) };
                            if let Some(job) = job {
                                job(None);
                            }
                        }
                    }
                    Ok(())
                },
            ))
        };
        // SAFETY: a live environment and window; WebView2 keeps the handler.
        let asked = unsafe {
            env.cast::<ICoreWebView2Environment10>().and_then(|env10| {
                let options = env10.CreateCoreWebView2ControllerOptions()?;
                options.SetProfileName(&HSTRING::from(name.as_str()))?;
                env10.CreateCoreWebView2ControllerWithOptions(window, &options, &made)
            })
        };
        if asked.is_err() {
            fail();
        }
    })));
}

// ---- the environment ----

/// Run `waiting` once the environment is there (`None` where it could not
/// start), starting it if this is the first ask.
fn on_environment(waiting: Waiting) {
    let (ready, start, waiting) = ENVIRONMENT.with_borrow_mut(|env| match env {
        Environment::Ready(env) => (Some(env.clone()), false, Some(waiting)),
        Environment::Coming(list) => {
            list.push(waiting);
            (None, false, None)
        }
        Environment::Failed => (None, false, Some(waiting)),
        Environment::None => {
            *env = Environment::Coming(vec![waiting]);
            (None, true, None)
        }
    });
    // Outside the table: the loader may report within the call.
    if start {
        start_environment();
    }
    if let Some(waiting) = waiting {
        run(waiting, ready.as_ref());
    }
}

fn run(waiting: Waiting, env: Option<&ICoreWebView2Environment>) {
    match (waiting, env) {
        (Waiting::Page(id), Some(env)) => make_view(env, id),
        (Waiting::Page(id), None) => Engine::global().system_failed(id, page_url(id), false),
        (Waiting::Job(job), env) => job(env.cloned()),
    }
}

fn start_environment() {
    let Some(folder) = DATA_DIR.with_borrow(|d| d.as_deref().map(user_data_folder)) else {
        return;
    };
    let options: ICoreWebView2EnvironmentOptions = CoreWebView2EnvironmentOptions::default().into();
    let done = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(|result, env| {
        let waiting = ENVIRONMENT.with_borrow_mut(|state| {
            let waiting = match state {
                Environment::Coming(w) => std::mem::take(w),
                _ => Vec::new(),
            };
            *state = if let (Ok(()), Some(env)) = (result, env) {
                Environment::Ready(env)
            } else {
                crate::trace("system engine: WebView2 could not start");
                Environment::Failed
            };
            waiting
        });
        let env = ENVIRONMENT.with_borrow(|s| match s {
            Environment::Ready(env) => Some(env.clone()),
            _ => None,
        });
        for waiting in waiting {
            run(waiting, env.as_ref());
        }
        Ok(())
    }));
    let folder = HSTRING::from(folder.as_os_str());
    // SAFETY: the loader keeps references to the options and the handler.
    let started = unsafe {
        create_environment(
            PCWSTR::null(),
            PCWSTR(folder.as_ptr()),
            options.as_raw(),
            done.as_raw(),
        )
    };
    if started.is_err() {
        crate::trace("system engine: WebView2 could not start");
        let waiting =
            ENVIRONMENT.with_borrow_mut(|s| match std::mem::replace(s, Environment::Failed) {
                Environment::Coming(w) => w,
                _ => Vec::new(),
            });
        for waiting in waiting {
            run(waiting, None);
        }
    }
}

// ---- pages ----

/// Make page `id`, to load `url` in `profile`'s data. The view itself comes
/// once the page has its window (`attach`). False if WebView2 is missing.
pub(crate) fn create(
    id: ContentId,
    url: &str,
    data_dir: &Path,
    profile: &str,
    ephemeral: bool,
) -> bool {
    if !available() {
        return false;
    }
    DATA_DIR.with_borrow_mut(|d| {
        d.get_or_insert_with(|| data_dir.to_path_buf());
    });
    let (store, private) = store(data_dir, profile, ephemeral);
    let page = Page {
        url: url.to_string(),
        profile: profile.to_string(),
        store,
        private,
        host: None,
        parent: None,
        controller: None,
        view: None,
        wants_focus: false,
        failed: None,
        failed_certificate: false,
        note_coming: false,
        navigation: 0,
        loading: false,
        crashed: None,
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
unsafe extern "system" fn host_procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_SIZE {
        let controller = PAGES.with(|pages| {
            let pages = pages.try_borrow().ok()?;
            pages
                .values()
                .find(|p| p.host == Some(hwnd))
                .and_then(|p| p.controller.clone())
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

/// Watches the page's own window: its holder keeps its size, the view hears
/// when it moved (popups and the input method place themselves by it), and
/// it renders only while the window is shown.
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
        WM_MOVE => moved(id),
        WM_WINDOWPOSCHANGED => {
            moved(id);
            // SAFETY: Windows passes the window's new position for the message.
            let flags = unsafe { (lparam.0 as *const WINDOWPOS).as_ref() }.map(|p| p.flags);
            if let Some(flags) = flags {
                if flags.contains(SWP_SHOWWINDOW) {
                    set_visible(id, true);
                } else if flags.contains(SWP_HIDEWINDOW) {
                    set_visible(id, false);
                }
            }
        }
        _ => {}
    }
    // SAFETY: the window's own handling.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

fn moved(id: ContentId) {
    if let Some(controller) = controller_of(id) {
        // SAFETY: a controller alive in the table.
        let _ = unsafe { controller.NotifyParentWindowPositionChanged() };
    }
}

/// A hidden page renders nothing, as a hidden Chromium page.
fn set_visible(id: ContentId, visible: bool) {
    if let Some(controller) = controller_of(id) {
        // SAFETY: a controller alive in the table.
        let _ = unsafe { controller.SetIsVisible(visible) };
    }
}

/// Size page `id`'s holder to its parent's inside.
fn fit_host(id: ContentId) {
    let Some((host, parent)) =
        PAGES.with_borrow(|p| p.get(&id).and_then(|page| page.host.zip(page.parent)))
    else {
        return;
    };
    let mut rect = RECT::default();
    // SAFETY: two live windows of this thread.
    unsafe {
        let _ = GetClientRect(parent, &raw mut rect);
        let _ = MoveWindow(
            host,
            0,
            0,
            rect.right - rect.left,
            rect.bottom - rect.top,
            true,
        );
    }
}

fn controller_of(id: ContentId) -> Option<ICoreWebView2Controller> {
    PAGES.with_borrow(|p| p.get(&id).and_then(|page| page.controller.clone()))
}

fn view_of(id: ContentId) -> Option<ICoreWebView2> {
    PAGES.with_borrow(|p| p.get(&id).and_then(|page| page.view.clone()))
}

fn page_url(id: ContentId) -> String {
    PAGES.with_borrow(|p| p.get(&id).map(|page| page.url.clone()).unwrap_or_default())
}

/// Give page `id` its place on screen: a window of ours inside the page's
/// own (`handle`, which CEF made and the engine shows, hides, moves and
/// sizes), filling it, with the view in it. As a child it moves, hides and
/// is cut round with its parent, and Windows hands it the mouse where it
/// lies: measured, every pointer move reaches the page.
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
        Engine::global().system_failed(id, page_url(id), false);
        return;
    };
    // SAFETY: our window, above the page window's own children; the watch
    // on the parent ends in `close`.
    unsafe {
        let _ = SetWindowPos(
            host,
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        let _ = SetWindowSubclass(
            parent,
            Some(parent_procedure),
            SUBCLASS,
            usize::try_from(id).unwrap_or(0),
        );
    }
    PAGES.with_borrow_mut(|p| {
        if let Some(page) = p.get_mut(&id) {
            page.host = Some(host);
            page.parent = Some(parent);
        }
    });
    on_environment(Waiting::Page(id));
}

/// The id of our watch on a page's own window.
const SUBCLASS: usize = 0x4D47_5732;

fn make_view(env: &ICoreWebView2Environment, id: ContentId) {
    let Some((host, store, private)) = PAGES.with_borrow(|p| {
        p.get(&id)
            .and_then(|page| page.host.map(|h| (h, page.store.clone(), page.private)))
    }) else {
        return;
    };
    let done = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
        move |result, controller| {
            if let (Ok(()), Some(controller)) = (result, controller) {
                view_made(id, &controller);
            } else {
                crate::trace(&format!("system page #{id}: WebView2 made no view"));
                Engine::global().system_failed(id, page_url(id), false);
            }
            Ok(())
        },
    ));
    // SAFETY: a live environment and window; WebView2 keeps the handler.
    let asked = unsafe {
        env.cast::<ICoreWebView2Environment10>().and_then(|env10| {
            let options = env10.CreateCoreWebView2ControllerOptions()?;
            options.SetProfileName(&HSTRING::from(store.as_str()))?;
            options.SetIsInPrivateModeEnabled(private)?;
            env10.CreateCoreWebView2ControllerWithOptions(host, &options, &done)
        })
    };
    if asked.is_err() {
        crate::trace(&format!("system page #{id}: WebView2 made no view"));
        Engine::global().system_failed(id, page_url(id), false);
    }
}

/// Page `id`'s view is there: it reports to the engine, fills its holder,
/// and loads its first address.
fn view_made(id: ContentId, controller: &ICoreWebView2Controller) {
    // The page may have closed while the view was being made.
    if !PAGES.with_borrow(|p| p.contains_key(&id)) {
        // SAFETY: a controller of ours nobody else holds.
        let _ = unsafe { controller.Close() };
        return;
    }
    // SAFETY: a live controller, on the thread it was made on.
    let Ok(view) = (unsafe { controller.CoreWebView2() }) else {
        crate::trace(&format!("system page #{id}: WebView2 made no view"));
        // SAFETY: as above; nobody else holds it.
        let _ = unsafe { controller.Close() };
        Engine::global().system_failed(id, page_url(id), false);
        return;
    };
    // SAFETY: as above.
    unsafe {
        if let Ok(c2) = controller.cast::<ICoreWebView2Controller2>() {
            // The page's own background until it paints: the frame's dark.
            let _ = c2.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                A: 255,
                R: 0x1B,
                G: 0x1B,
                B: 0x1E,
            });
        }
        if let Ok(settings) = view.Settings() {
            // No DevTools: a system page has Magnetite's none either.
            let _ = settings.SetAreDevToolsEnabled(false);
            // No check of addresses against a list of fraudulent sites, as
            // in Magnetite's Chromium pages: it asks a service.
            if let Ok(settings8) = settings.cast::<ICoreWebView2Settings8>() {
                let _ = settings8.SetIsReputationCheckingRequired(false);
            }
        }
    }
    let (url, wants_focus, host, parent) = PAGES.with_borrow_mut(|p| {
        let Some(page) = p.get_mut(&id) else {
            return (String::new(), false, None, None);
        };
        page.controller = Some(controller.clone());
        page.view = Some(view.clone());
        // A request for the keyboard is used once.
        let wants_focus = std::mem::take(&mut page.wants_focus);
        (page.url.clone(), wants_focus, page.host, page.parent)
    });
    listen(id, controller, &view);
    fit_host(id);
    if let Some(host) = host {
        let mut rect = RECT::default();
        // SAFETY: our window and a live controller; the page's own window.
        unsafe {
            let _ = GetClientRect(host, &raw mut rect);
            let _ = controller.SetBounds(rect);
            let shown = parent.is_some_and(|p| IsWindowVisible(p).as_bool());
            let _ = controller.SetIsVisible(shown);
        }
    }
    load(id, &url);
    if wants_focus {
        focus(id);
    }
}

/// Whether `url` is an `http://` or `https://` address: all a page's main
/// frame may be sent to. Anything else (`mailto:`, an app's own scheme, a
/// `data:` or `blob:` address) is not opened from here.
fn is_web_address(url: &str) -> bool {
    (url.split_once(':')).is_some_and(|(scheme, _)| {
        scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
    })
}

/// Whether a key is held now, for this thread (which the page's input
/// shares, its window being a child of ours).
fn held(key: u16) -> bool {
    // SAFETY: a plain query of the keyboard state.
    unsafe { GetKeyState(i32::from(key)) < 0 }
}

/// What page `id` reports, as the engine's events.
fn listen(id: ContentId, controller: &ICoreWebView2Controller, view: &ICoreWebView2) {
    let mut token = 0i64;
    let generation = GENERATION.get();
    // SAFETY: live objects; WebView2 keeps each handler until the view is
    // closed (`close`).
    unsafe {
        let _ = view.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                let url = get(|u| args.Uri(u)).unwrap_or_default();
                if !is_web_address(&url) {
                    // The one page that is not the web's: the note about a
                    // failed load, which `load_failed` sends.
                    let note = PAGES.with_borrow_mut(|p| {
                        p.get_mut(&id)
                            .is_some_and(|page| std::mem::take(&mut page.note_coming))
                    });
                    if !note {
                        args.SetCancel(true)?;
                    }
                    return Ok(());
                }
                let mut by_user = BOOL::default();
                args.IsUserInitiated(&raw mut by_user)?;
                if by_user.as_bool() && Engine::global().system_leaves_lock(id, &url) {
                    args.SetCancel(true)?;
                    return Ok(());
                }
                let mut navigation = 0u64;
                args.NavigationId(&raw mut navigation)?;
                PAGES.with_borrow_mut(|p| {
                    if let Some(page) = p.get_mut(&id) {
                        page.failed = None;
                        page.failed_certificate = false;
                        page.url.clone_from(&url);
                        page.navigation = navigation;
                        page.loading = true;
                    }
                });
                Engine::global().system_started(id, &url);
                report_loading(id, true);
                Ok(())
            })),
            &raw mut token,
        );
        let _ = view.add_SourceChanged(
            &SourceChangedEventHandler::create(Box::new(move |_, _| {
                if let Some(url) = address_of(id) {
                    PAGES.with_borrow_mut(|p| {
                        if let Some(page) = p.get_mut(&id).filter(|page| page.failed.is_none()) {
                            page.url.clone_from(&url);
                        }
                    });
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
                // Back and forward changed; whether the page loads did not.
                let loading = PAGES.with_borrow(|p| p.get(&id).is_some_and(|page| page.loading));
                report_loading(id, loading);
                Ok(())
            })),
            &raw mut token,
        );
        let _ = view.add_NavigationCompleted(
            &NavigationCompletedEventHandler::create(Box::new(move |view, args| {
                let (Some(view), Some(args)) = (view, args) else {
                    return Ok(());
                };
                // A navigation another one took the place of: the page is
                // at the newer one, which still loads.
                let mut navigation = 0u64;
                args.NavigationId(&raw mut navigation)?;
                let current = PAGES.with_borrow_mut(|p| {
                    p.get_mut(&id)
                        .filter(|page| page.navigation == navigation)
                        .map(|page| page.loading = false)
                        .is_some()
                });
                if !current {
                    return Ok(());
                }
                let mut ok = BOOL::default();
                args.IsSuccess(&raw mut ok)?;
                let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                args.WebErrorStatus(&raw mut status)?;
                if !ok.as_bool() {
                    load_failed(id, &view, status);
                }
                report_loading(id, false);
                Ok(())
            })),
            &raw mut token,
        );
        let _ = view.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                // No window of WebView2's: the app opens a tab (or a peek)
                // for an address of the web's, and the page gets no window
                // object back.
                args.SetHandled(true)?;
                let url = get(|u| args.Uri(u)).unwrap_or_default();
                if !is_web_address(&url) {
                    return Ok(());
                }
                let sized = args.WindowFeatures().is_ok_and(|f| {
                    let mut has = BOOL::default();
                    f.HasSize(&raw mut has).is_ok() && has.as_bool()
                });
                // A click with a modifier, as in the other pages: Ctrl opens
                // a tab behind (with Shift in front), Shift alone a window.
                let (control, shift) = (held(VK_CONTROL.0), held(VK_SHIFT.0));
                Engine::global().system_open(
                    id,
                    url,
                    control && !shift,
                    sized || (shift && !control),
                );
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
        let _ = view.add_ContainsFullScreenElementChanged(
            &ContainsFullScreenElementChangedEventHandler::create(Box::new(move |view, _| {
                let mut full = BOOL::default();
                if let Some(view) = view {
                    view.ContainsFullScreenElement(&raw mut full)?;
                }
                let full = full.as_bool();
                Engine::post(Box::new(move || {
                    Engine::global().system_fullscreen(id, full);
                }));
                Ok(())
            })),
            &raw mut token,
        );
        let _ = view.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                if let Some(args) = args {
                    args.ProcessFailedKind(&raw mut kind)?;
                }
                if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
                    browser_ended(generation);
                } else {
                    page_crashed(id);
                }
                Ok(())
            })),
            &raw mut token,
        );
        if let Ok(view4) = view.cast::<ICoreWebView2_4>() {
            let _ = view4.add_DownloadStarting(
                &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                    // Downloads are Chromium's pages' (the download list);
                    // here none starts.
                    if let Some(args) = args {
                        args.SetCancel(true)?;
                        args.SetHandled(true)?;
                    }
                    crate::trace(&format!("system page #{id}: a download, not started"));
                    Ok(())
                })),
                &raw mut token,
            );
        }
        if let Ok(view15) = view.cast::<ICoreWebView2_15>() {
            let _ = view15.add_FaviconChanged(
                &FaviconChangedEventHandler::create(Box::new(move |view, _| {
                    let icon = view
                        .and_then(|v| v.cast::<ICoreWebView2_15>().ok())
                        .and_then(|v| get(|u| v.FaviconUri(u)))
                        .filter(|u| u.starts_with("https://"));
                    let failed =
                        PAGES.with_borrow(|p| p.get(&id).is_some_and(|page| page.failed.is_some()));
                    if let Some(icon) = icon.filter(|_| !failed) {
                        Engine::global().system_favicon(id, &icon);
                    }
                    Ok(())
                })),
                &raw mut token,
            );
        }
        // WebView2's own keys (Ctrl and plus, minus, 0) zoom too: the site
        // panel hears of it as of any zoom.
        let _ = controller.add_ZoomFactorChanged(
            &ZoomFactorChangedEventHandler::create(Box::new(move |_, _| {
                Engine::post(Box::new(move || Engine::global().emit_site_state(id)));
                Ok(())
            })),
            &raw mut token,
        );
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
                let down = kind == COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                    || kind == COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN;
                // Magnetite's reserved shortcuts, before the page sees the key.
                if down && Engine::global().system_key(id, &key_event(key)) {
                    args.SetHandled(true)?;
                }
                Ok(())
            })),
            &raw mut token,
        );
    }
}

/// A page whose process ended by itself is loaded again unless it ended
/// less than this long ago.
const CRASH_PAUSE: Duration = Duration::from_secs(10);

/// WebView2's own process ended, and every view of environment
/// `generation` with it. Each view reports it: the first report of an
/// environment starts it anew, and each page gets a new view at the address
/// it was at; the others are of one that is gone already. Not again within
/// `CRASH_PAUSE`: a WebView2 that keeps ending would otherwise start for
/// ever, and its pages say they could not be loaded instead.
fn browser_ended(generation: u64) {
    if generation != GENERATION.get() {
        return;
    }
    GENERATION.set(generation + 1);
    crate::trace("system engine: WebView2's process ended");
    let again = BROWSER_ENDED
        .replace(Some(Instant::now()))
        .is_none_or(|last| last.elapsed() > CRASH_PAUSE);
    let ids: Vec<ContentId> = PAGES.with_borrow_mut(|pages| {
        for page in pages.values_mut() {
            page.controller = None;
            page.view = None;
        }
        pages
            .iter()
            .filter(|(_, p)| p.host.is_some())
            .map(|(id, _)| *id)
            .collect()
    });
    ENVIRONMENT.with_borrow_mut(|e| {
        if matches!(e, Environment::Ready(_)) {
            *e = Environment::None;
        }
    });
    for id in ids {
        if again {
            on_environment(Waiting::Page(id));
        } else {
            Engine::global().system_failed(id, page_url(id), false);
        }
    }
}

/// Page `id`'s own process ended.
fn page_crashed(id: ContentId) {
    crate::trace(&format!("system page #{id}: its process ended"));
    // Once: a page that ends its process whenever it loads would otherwise
    // load for ever.
    let again = PAGES.with_borrow_mut(|p| {
        p.get_mut(&id).is_some_and(|page| {
            page.crashed
                .replace(Instant::now())
                .is_none_or(|last| last.elapsed() > CRASH_PAUSE)
        })
    });
    if again {
        reload(id);
    }
}

/// A load failed: the app hears of it, and the page says so.
fn load_failed(id: ContentId, view: &ICoreWebView2, status: COREWEBVIEW2_WEB_ERROR_STATUS) {
    // Given up for another load, turned down by `NavigationStarting`, or a
    // download: not a failure to show.
    if status == COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED
        || status == COREWEBVIEW2_WEB_ERROR_STATUS_CONNECTION_ABORTED
    {
        return;
    }
    let url = page_url(id);
    if url.is_empty() {
        return;
    }
    crate::trace(&format!("system page #{id}: load failed ({})", status.0));
    PAGES.with_borrow_mut(|p| {
        if let Some(page) = p.get_mut(&id) {
            page.failed = Some(url.clone());
            page.failed_certificate = certificate_failed(status);
            page.note_coming = true;
        }
    });
    let note = failure_note(&url, reason(status));
    // SAFETY: a live view.
    let _ = unsafe { view.NavigateToString(&HSTRING::from(note)) };
    Engine::global().system_failed(
        id,
        url,
        status == COREWEBVIEW2_WEB_ERROR_STATUS_CANNOT_CONNECT,
    );
}

/// Whether a load failed for the site's certificate.
fn certificate_failed(status: COREWEBVIEW2_WEB_ERROR_STATUS) -> bool {
    [
        COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_COMMON_NAME_IS_INCORRECT,
        COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_EXPIRED,
        COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_IS_INVALID,
        COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_REVOKED,
    ]
    .contains(&status)
}

/// Why a load failed, in plain words.
fn reason(status: COREWEBVIEW2_WEB_ERROR_STATUS) -> &'static str {
    match status {
        s if s == COREWEBVIEW2_WEB_ERROR_STATUS_HOST_NAME_NOT_RESOLVED => {
            "The server's name could not be found."
        }
        s if s == COREWEBVIEW2_WEB_ERROR_STATUS_CANNOT_CONNECT => {
            "The server could not be reached."
        }
        s if s == COREWEBVIEW2_WEB_ERROR_STATUS_TIMEOUT => "The server took too long to answer.",
        s if s == COREWEBVIEW2_WEB_ERROR_STATUS_DISCONNECTED => {
            "The connection to the network was lost."
        }
        s if s == COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_EXPIRED
            || s == COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_IS_INVALID
            || s == COREWEBVIEW2_WEB_ERROR_STATUS_CERTIFICATE_REVOKED =>
        {
            "The site's certificate is not valid."
        }
        _ => "The connection to the server failed.",
    }
}

/// The page shown where `url` could not be loaded: what failed and why,
/// in plain words, with nothing of the text left for the browser to read
/// as markup.
fn failure_note(url: &str, reason: &str) -> String {
    let plain = |text: &str| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    format!(
        "<!doctype html><meta charset=utf-8><meta name=color-scheme content=\"light dark\">\
<title>{title}</title><style>html{{background:#fff;color:#1b1b1e}}@media(prefers-color-scheme:dark){{\
html{{background:#1b1b1e;color:#ececee}}}}body{{font:15px 'Segoe UI',sans-serif;margin:18vh auto 0;\
max-width:34em;padding:0 2em;line-height:1.5}}h1{{font-size:20px}}p{{opacity:.7;overflow-wrap:anywhere}}</style>\
<h1>This page could not be loaded</h1><p>{reason}</p><p>{url}</p>",
        title = plain(url),
        reason = plain(reason),
        url = plain(url),
    )
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
        // The flags are `i32` on Windows; none is negative.
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

/// Load `url` in page `id`.
pub(crate) fn load(id: ContentId, url: &str) {
    if !is_web_address(url) {
        crate::trace(&format!("system page #{id}: not a web address, not loaded"));
        return;
    }
    match view_of(id) {
        Some(view) => {
            // SAFETY: a live view.
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

/// Load page `id` again: the address that failed, where it shows the note
/// about a failed load.
pub(crate) fn reload(id: ContentId) {
    let failed = PAGES.with_borrow(|p| p.get(&id).and_then(|page| page.failed.clone()));
    match (failed, view_of(id)) {
        (Some(url), _) => load(id, &url),
        (None, Some(view)) => {
            // SAFETY: a live view.
            let _ = unsafe { view.Reload() };
        }
        (None, None) => {}
    }
}

pub(crate) fn stop(id: ContentId) {
    if let Some(view) = view_of(id) {
        // SAFETY: a live view.
        let _ = unsafe { view.Stop() };
    }
}

/// Give page `id` the keyboard. Only the latest request counts: one made
/// before a page had its view is forgotten when another page is asked.
pub(crate) fn focus(id: ContentId) {
    PAGES.with_borrow_mut(|pages| {
        for page in pages.values_mut() {
            page.wants_focus = false;
        }
    });
    match controller_of(id) {
        Some(controller) => {
            // SAFETY: a live controller.
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
    // Clamped to what a page can be zoomed to, so the cast cannot wrap.
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
/// without a network address. WebView2 says nothing of mixed content on a
/// page: it blocks what an `https://` page would load insecurely. A page
/// that failed to load came over no connection: not secure where the
/// certificate was refused, else nothing to say.
pub(crate) fn secure(id: ContentId) -> Option<bool> {
    let failed = PAGES.with_borrow(|p| {
        p.get(&id)
            .filter(|page| page.failed.is_some())
            .map(|page| page.failed_certificate)
    });
    if let Some(certificate) = failed {
        return certificate.then_some(false);
    }
    let url = address_of(id)?;
    if url.starts_with("https://") {
        Some(true)
    } else if url.starts_with("http://") {
        Some(false)
    } else {
        None
    }
}

/// The address page `id` shows: the one that failed, while it shows the
/// note about that.
pub(crate) fn address_of(id: ContentId) -> Option<String> {
    if let Some(failed) = PAGES.with_borrow(|p| p.get(&id).and_then(|page| page.failed.clone())) {
        return Some(failed);
    }
    let view = view_of(id)?;
    // SAFETY: a live view.
    get(|u| unsafe { view.Source(u) }).filter(|u| is_web_address(u))
}

#[cfg(test)]
mod tests {
    use super::{
        failure_note, fresh_name, is_web_address, new_enough, read_store_names, site_origins,
        store, stores_file,
    };

    #[test]
    fn a_runtime_must_have_profiles_by_name() {
        assert!(new_enough("157.0.4341.60"));
        assert!(new_enough("120.0.2210.55"));
        assert!(!new_enough("120.0.2209.0"));
        assert!(!new_enough("109.0.1518.140"));
        assert!(!new_enough(""));
    }

    #[test]
    fn only_http_addresses_are_the_webs() {
        assert!(is_web_address("https://www.netflix.com/browse"));
        assert!(is_web_address("HTTP://example.org/"));
        for other in [
            "about:blank",
            "data:text/html,<p>hi",
            "mailto:someone@example.org",
            "httpsx://example.org/",
            "example.org",
        ] {
            assert!(!is_web_address(other), "{other}");
        }
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("magnetite-wv2-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_profile_keeps_its_name_and_a_private_one_gets_none_kept() {
        let dir = temp_dir("names");
        let (first, private) = store(&dir, "", false);
        assert!(!private);
        assert!(first.starts_with('m') && first.len() == 33, "{first}");
        assert_eq!(store(&dir, "", false), (first.clone(), false));
        let (other, _) = store(&dir, "work", false);
        assert_ne!(other, first);
        let (in_memory, private) = store(&dir, "secret", true);
        assert!(private && in_memory.starts_with("private-"));
        // A private profile's pages share one store, until it is released.
        assert_eq!(store(&dir, "secret", true).0, in_memory);
        super::release_stores(&["secret".to_string()]);
        assert_ne!(store(&dir, "secret", true).0, in_memory);
        // Only the two on disk are written down.
        assert_eq!(read_store_names(&dir).unwrap().len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_list_of_stores_that_cannot_be_read_is_not_an_empty_one() {
        let dir = temp_dir("unreadable");
        assert!(read_store_names(&dir).unwrap().is_empty());
        std::fs::write(stores_file(&dir), "{ not what was written").unwrap();
        assert!(read_store_names(&dir).is_err());
        // Nothing is opened under a name that is kept nowhere: in memory.
        let (_, private) = store(&dir, "", false);
        assert!(private);
        assert!(read_store_names(&dir).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn profile_names_are_fresh_each_time() {
        assert_ne!(fresh_name(), fresh_name());
    }

    #[test]
    fn a_sites_storage_is_cleared_at_its_own_addresses() {
        assert_eq!(
            site_origins("example.org"),
            [
                "https://example.org",
                "http://example.org",
                "https://www.example.org",
                "http://www.example.org"
            ]
        );
    }

    #[test]
    fn an_open_pages_origin_keeps_its_port() {
        use super::origin_of;
        assert_eq!(
            origin_of("http://localhost:8766/cookie?x=1"),
            Some(("http://localhost:8766".into(), "localhost".into()))
        );
        assert_eq!(
            origin_of("https://help.example.org/a#b"),
            Some(("https://help.example.org".into(), "help.example.org".into()))
        );
        assert_eq!(origin_of("about:blank"), None);
        assert_eq!(origin_of("https:///x"), None);
    }

    #[test]
    fn the_failure_note_shows_text_as_text() {
        let note = failure_note("https://a.example/?q=<script>", "no <b>route</b> & more");
        assert!(note.contains("https://a.example/?q=&lt;script&gt;"));
        assert!(note.contains("no &lt;b&gt;route&lt;/b&gt; &amp; more"));
        assert!(!note.contains("<script>"));
    }
}
