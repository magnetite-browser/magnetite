//! `CefApp` implementations for the browser process and the helper.

use std::sync::Arc;

use cef::wrapper::message_router::{MessageRouterRendererSideHandlerCallbacks, RendererSideRouter};
use cef::*;

use crate::engine::Engine;
use crate::events::EngineEvent;
use crate::{router, scheme};

/// Our picture-in-picture controls (pip.rs) cover Chromium's video window,
/// which shows the page's address until the user clicks into it; under our
/// controls that click never arrives. Treating every video as trusted keeps
/// the address off, as Safari's and Firefox's players do. Chromium marks the
/// feature as one for debugging, so a future version may drop it.
#[cfg(target_os = "macos")]
const PIP_WITHOUT_ADDRESS: &str = "VideoPipForceTrustedForMediaPlaybackForTesting";

/// The switch with which Chromium keeps unpacked extensions on while
/// Developer mode is off. Only Magnetite's CEF knows it: the name must stay
/// the one `extension-manifest-v2.patch` in magnetite-cef checks, or the
/// setting silently does nothing.
pub(crate) const KEEP_UNPACKED_EXTENSIONS: &str = "magnetite-keep-unpacked-extensions";

/// Add `feature` to `--enable-features`, keeping features already listed.
#[cfg(target_os = "macos")]
fn enable_feature(cmd: &mut CommandLine, feature: &str) {
    add_to_list(cmd, "enable-features", feature);
}

/// Chromium features that contact Google by themselves and that Magnetite
/// has no use for, switched off. Seen in the engine's network log of a
/// fresh start and one sign-in; what the engine still contacts is listed in
/// `docs/design/security.md` ("Network").
const NO_CALLS_HOME: [&str; 6] = [
    // Asks Google's autofill server what the fields of every form it sees
    // are for. Chromium's local rules decide instead.
    "AutofillServerCommunication",
    // Asks www.google.com whether the address field may offer "AI Mode".
    // Magnetite has no such field.
    "AimEnabled",
    // Asks a Google server for the time, to judge certificate errors by.
    // The system clock does.
    "NetworkTimeServiceQuerying",
    // Keeps a connection to Chromium's own search engine (Google) open, so
    // a search from its address field starts faster. Magnetite searches
    // from the command bar, with the engine chosen in Settings.
    "PreconnectToSearch",
    // Fetch a model from www.gstatic.com for every new profile, which
    // guesses whether its user would decline a translation. Translation
    // is off (`engine.rs`, `prepare_context`).
    "TranslateRankerQuery",
    "TranslateRankerEnforcement",
];

/// Add `item` to the comma-separated list switch `switch`.
fn add_to_list(cmd: &mut CommandLine, switch: &str, item: &str) {
    let name = CefString::from(switch);
    let listed = CefString::from(&cmd.switch_value(Some(&name))).to_string();
    let value = if listed.is_empty() {
        item.to_string()
    } else {
        format!("{listed},{item}")
    };
    cmd.append_switch_with_value(Some(&name), Some(&CefString::from(value.as_str())));
}

/// The patch to CEF that makes a window with a parent a subsurface of it
/// under Wayland (magnetite-cef, `patches/`). Without it CEF stops the
/// program when it makes such a window, which every page is (ADR-0012).
#[cfg(target_os = "linux")]
const WAYLAND_PATCH: &str = "wayland-child-windows.patch";

/// The patch to CEF that gives a window another parent when it is shown
/// (magnetite-cef, `patches/`): a page moves to another window with it.
#[cfg(target_os = "linux")]
const REPARENT_PATCH: &str = "window-reparent.patch";

/// The patch to CEF that rounds a window's corners (magnetite-cef,
/// `patches/`).
#[cfg(target_os = "linux")]
const CORNERS_PATCH: &str = "rounded-corners.patch";

/// The colour id that a CEF with `CORNERS_PATCH` takes a window's corner
/// radius under, in place of a colour.
#[cfg(target_os = "linux")]
const CORNER_RADIUS_ID: i32 = -1;

/// The patch to CEF that lets a page be see-through and rounds the pages in
/// a window of their own (magnetite-cef, `patches/`).
#[cfg(target_os = "linux")]
const PAGES_PATCH: &str = "transparent-pages.patch";

/// The colour id that a CEF with `PAGES_PATCH` takes the corner radius of
/// the pages in a window under, in place of a colour.
#[cfg(target_os = "linux")]
const PAGE_CORNER_RADIUS_ID: i32 = -2;

/// The background colour that asks a CEF with `PAGES_PATCH` for a
/// see-through page: alpha 0, but not 0 itself, which means unset.
#[cfg(target_os = "linux")]
pub(crate) const CLEAR_BACKGROUND: u32 = 0x00FF_FFFF;

/// Whether the CEF in use can make a page see-through and round the pages in
/// a window (`PAGES_PATCH`).
#[cfg(target_os = "linux")]
pub(crate) fn clear_pages() -> bool {
    static PATCHED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PATCHED.get_or_init(|| cef_patches().is_some_and(|patches| lists_patch(&patches, PAGES_PATCH)))
}

/// Give the pages in window `window` corners of their own, `radius` DIP (0:
/// square), for a page that does not fill its window: the window's own
/// corners (`set_corner_radius`) are not the page's then. Needs
/// `PAGES_PATCH`; without it nothing changes.
#[cfg(target_os = "linux")]
pub(crate) fn set_page_corner_radius(window: &Window, radius: i32) {
    if clear_pages() {
        window.set_theme_color(PAGE_CORNER_RADIUS_ID, u32::try_from(radius).unwrap_or(0));
    }
}

/// Give page window `window` corners of `radius` DIP, 0 for square ones
/// (the counterpart of `mac::set_corner_radius`). Chromium cuts the window
/// and shows the frame outside the corners. Needs `CORNERS_PATCH` in the
/// CEF in use; without it the corners stay square.
#[cfg(target_os = "linux")]
pub(crate) fn set_corner_radius(window: &Window, radius: i32) {
    static PATCHED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let patched = PATCHED
        .get_or_init(|| cef_patches().is_some_and(|patches| lists_patch(&patches, CORNERS_PATCH)));
    if *patched {
        window.set_theme_color(CORNER_RADIUS_ID, u32::try_from(radius).unwrap_or(0));
    }
}

/// The list of Magnetite's patches in the CEF in use, one file name a line.
/// Magnetite's CEF build carries it, and packaging puts it next to the
/// executable; the official CEF has none.
#[cfg(target_os = "linux")]
const CEF_PATCHES_FILE: &str = "magnetite-patches.txt";

/// Whether `patches`, the text of `CEF_PATCHES_FILE`, lists `patch`.
#[cfg(target_os = "linux")]
fn lists_patch(patches: &str, patch: &str) -> bool {
    patches.lines().any(|line| line.trim() == patch)
}

/// The platform Chromium runs on when the command line names none: Wayland
/// in a Wayland session if the CEF in use can do child windows there, else
/// X11 wherever an X server is (in a Wayland session: `XWayland`). `None`
/// leaves the choice to Chromium.
#[cfg(target_os = "linux")]
fn default_platform(
    wayland_session: bool,
    x_server: bool,
    cef_has_patch: bool,
) -> Option<&'static str> {
    if wayland_session && cef_has_patch {
        Some("wayland")
    } else if x_server {
        Some("x11")
    } else {
        None
    }
}

/// The text of `CEF_PATCHES_FILE`, if the CEF in use has one.
#[cfg(target_os = "linux")]
fn cef_patches() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    std::fs::read_to_string(exe.with_file_name(CEF_PATCHES_FILE)).ok()
}

/// Pick Chromium's platform (`default_platform`) unless an
/// `--ozone-platform` on the command line did.
#[cfg(target_os = "linux")]
fn choose_platform(cmd: &mut CommandLine) {
    let set = |var: &str| std::env::var_os(var).is_some_and(|v| !v.is_empty());
    let name = CefString::from("ozone-platform");
    if cmd.has_switch(Some(&name)) == 0 {
        let cef_has_patch =
            cef_patches().is_some_and(|patches| lists_patch(&patches, WAYLAND_PATCH));
        if let Some(platform) =
            default_platform(set("WAYLAND_DISPLAY"), set("DISPLAY"), cef_has_patch)
        {
            cmd.append_switch_with_value(Some(&name), Some(&CefString::from(platform)));
        }
    }
}

/// Whether Chromium runs as an X11 client here, not as a Wayland one.
#[cfg(target_os = "linux")]
pub(crate) fn runs_on_x11() -> bool {
    static X11: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *X11.get_or_init(|| {
        command_line_get_global().is_some_and(|cmd| {
            let name = CefString::from("ozone-platform");
            CefString::from(&cmd.switch_value(Some(&name))).to_string() == "x11"
        })
    })
}

/// Whether a page window can get another window's frame as its parent
/// (`Engine::can_move_pages`). macOS does it natively (`mac::attach_child`);
/// Linux needs `REPARENT_PATCH` in the CEF in use; Windows is not built yet.
pub(crate) fn can_reparent() -> bool {
    // TODO(macos): untested, written without a Mac (not even compiled):
    // check that a page moves between windows (`mac::attach_child`), and the
    // New Window / Close Window menu items.
    #[cfg(target_os = "macos")]
    return true;
    #[cfg(target_os = "linux")]
    return cef_patches().is_some_and(|patches| lists_patch(&patches, REPARENT_PATCH));
    // TODO(windows): untested, no Windows machine yet. `window-reparent.patch`
    // reparents on every platform; once Windows builds, check that a page
    // moves between windows there and return true.
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    false
}

wrap_app! {
    pub struct BrowserApp {}

    impl App {
        fn on_before_command_line_processing(&self, process_type: Option<&CefString>, command_line: Option<&mut CommandLine>) {
            let Some(cmd) = command_line else { return };
            if process_type.is_some_and(|p| !p.to_string().is_empty()) { return; }
            let config = Engine::global().config();
            if config.use_mock_keychain {
                cmd.append_switch(Some(&CefString::from("use-mock-keychain")));
            }
            // The global context is the `Default` profile, always. Without
            // this Chromium starts with the profile used last: after a
            // click in a window of another profile that one would be the
            // default context, and every space of the main profile would
            // run in it, its cookies and extensions shared.
            cmd.append_switch_with_value(
                Some(&CefString::from("profile-directory")),
                Some(&CefString::from(crate::config::DEFAULT_PROFILE_DIR)),
            );
            if !config.load_extensions.is_empty() {
                let list = config
                    .load_extensions
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                cmd.append_switch_with_value(Some(&CefString::from("load-extension")), Some(&CefString::from(list.as_str())));
            }
            if config.keep_unpacked_extensions {
                cmd.append_switch(Some(&CefString::from(KEEP_UNPACKED_EXTENSIONS)));
            }
            #[cfg(target_os = "macos")]
            enable_feature(cmd, PIP_WITHOUT_ADDRESS);
            #[cfg(target_os = "linux")]
            choose_platform(cmd);
            // Our private DevTools session (`devpipe.rs`): fds 3 and 4, on
            // Windows the two handles named here.
            if crate::devpipe::is_open() {
                cmd.append_switch(Some(&CefString::from("remote-debugging-pipe")));
                #[cfg(windows)]
                if let Some(handles) = crate::devpipe::chrome_handles() {
                    cmd.append_switch_with_value(
                        Some(&CefString::from("remote-debugging-io-pipes")),
                        Some(&CefString::from(handles)),
                    );
                }
            }
            for feature in NO_CALLS_HOME {
                add_to_list(cmd, "disable-features", feature);
            }
            // A DevTools pipe or port makes Chromium tell pages it is under
            // automation (`navigator.webdriver`), and bot checks (Cloudflare,
            // reCAPTCHA) then fail. Magnetite is driven by its user.
            add_to_list(cmd, "disable-blink-features", "AutomationControlled");
        }

        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            if let Some(r) = registrar { scheme::register_schemes(r); }
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(MagnetiteBrowserProcessHandler::new())
        }

        fn resource_bundle_handler(&self) -> Option<ResourceBundleHandler> {
            Some(crate::strings::Strings::new())
        }
    }
}

wrap_browser_process_handler! {
    struct MagnetiteBrowserProcessHandler {}
    impl BrowserProcessHandler {
        // Windows Chromium opens by itself (an extension's new tab) get
        // our client, which turns them into tabs (`stray.rs`).
        fn default_client(&self) -> Option<Client> {
            Some(crate::client::MagnetiteClient::stray())
        }
        fn on_context_initialized(&self) {
            let engine = Engine::global();
            scheme::install_factory(&engine.config().data_dir, engine.config().ui_dir.clone());
            engine.lock().router = Some(router::create_browser_router());
            engine.start_tick();
            #[cfg(target_os = "macos")]
            crate::mac::install_swipe_monitor();
            #[cfg(target_os = "macos")]
            crate::mac::clear_tooltips_without_keyboard();
            #[cfg(target_os = "macos")]
            crate::mac::watch_covered_pages();
            #[cfg(target_os = "macos")]
            crate::mac::watch_memory_pressure();
            engine.emit(EngineEvent::Ready);
        }
        // A second start hands its command line to the running Magnetite.
        // Handled here, or Chromium would open a window of its own.
        fn on_already_running_app_relaunch(&self, command_line: Option<&mut CommandLine>, current_directory: Option<&CefString>) -> ::std::os::raw::c_int {
            let mut list = CefStringList::new();
            if let Some(cmd) = command_line {
                cmd.argv(Some(&mut list));
            }
            let cwd = current_directory.map(|d| std::path::PathBuf::from(d.to_string())).filter(|d| d.is_absolute());
            Engine::global().emit(EngineEvent::Relaunched { argv: list.into_iter().collect(), cwd });
            1
        }
    }
}

wrap_app! {
    pub struct HelperApp {
        router: Arc<RendererSideRouter>,
    }
    impl App {
        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            if let Some(r) = registrar { scheme::register_schemes(r); }
        }
        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(MagnetiteRenderProcessHandler::new(self.router.clone()))
        }
    }
}

impl HelperApp {
    pub fn create() -> App {
        HelperApp::new(router::create_renderer_router())
    }
}

wrap_render_process_handler! {
    struct MagnetiteRenderProcessHandler {
        router: Arc<RendererSideRouter>,
    }
    impl RenderProcessHandler {
        fn on_context_created(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, context: Option<&mut V8Context>) {
            // `magnetiteQuery` exists only in Magnetite's own top-level pages;
            // the browser side checks again.
            let url = frame.as_ref().map(|f| CefString::from(&f.url()).to_string()).unwrap_or_default();
            let main = frame.as_ref().is_some_and(|f| f.is_main() != 0);
            if main && router::is_ui_url(&url) {
                self.router.on_context_created(browser.cloned(), frame.cloned(), context.cloned());
            } else if let (Some(b), Some(f), Some(c)) = (browser.as_deref(), frame.as_deref(), context.as_deref()) {
                crate::media::install(b, f, c);
                crate::tabshare::install(f, c);
                crate::protected::install(f, c);
            }
        }
        fn on_context_released(&self, browser: Option<&mut Browser>, frame: Option<&mut Frame>, context: Option<&mut V8Context>) {
            if let (Some(b), Some(c)) = (browser.as_deref(), context.as_deref()) {
                crate::media::release(b, c);
            }
            self.router.on_context_released(browser.cloned(), frame.cloned(), context.cloned());
        }
        fn on_process_message_received(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> ::std::os::raw::c_int {
            if let (Some(b), Some(m)) = (browser.as_deref(), message.as_deref())
                && crate::media::on_message(b, m)
            {
                return 1;
            }
            self.router
                .on_process_message_received(browser.cloned(), frame.cloned(), Some(source_process), message.cloned())
                .into()
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{default_platform, lists_patch};

    /// A name that is only part of a line must not count: Magnetite would
    /// then run on Wayland with a CEF that stops at the first page.
    #[test]
    fn lists_patch_wants_a_whole_line() {
        let patch = "wayland-child-windows.patch";

        assert!(lists_patch("wayland-child-windows.patch\n", patch));
        assert!(lists_patch(
            "other.patch\nwayland-child-windows.patch",
            patch
        ));
        assert!(lists_patch(
            "other.patch\r\n  wayland-child-windows.patch\t\r\n",
            patch
        ));

        assert!(!lists_patch("", patch));
        assert!(!lists_patch("other.patch\n", patch));
        assert!(!lists_patch("not-wayland-child-windows.patch.bak\n", patch));
        assert!(!lists_patch("wayland-child-windows.patch.bak\n", patch));
        assert!(!lists_patch("old-wayland-child-windows.patch\n", patch));
        assert!(!lists_patch(
            "wayland-child-windows.patch other.patch\n",
            patch
        ));
    }

    /// Wayland only with the patch, else X11 where an X server is: the
    /// wrong choice stops the program at the first page or at the start.
    #[test]
    fn default_platform_is_wayland_only_with_the_patch() {
        // (Wayland session, X server, CEF has the patch)
        assert_eq!(default_platform(true, false, true), Some("wayland"));
        assert_eq!(default_platform(true, true, true), Some("wayland"));

        assert_eq!(default_platform(true, true, false), Some("x11"));
        assert_eq!(default_platform(false, true, false), Some("x11"));
        assert_eq!(default_platform(false, true, true), Some("x11"));

        assert_eq!(default_platform(true, false, false), None);
        assert_eq!(default_platform(false, false, false), None);
        assert_eq!(default_platform(false, false, true), None);
    }
}
