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

/// Run as an X11 client, also in a Wayland session (through `XWayland`).
/// Under Wayland, CEF stops the program when it makes a window with a
/// parent, which every page is (ADR-0012). An `--ozone-platform` on the
/// command line wins, and without an X server Chromium keeps its choice.
/// Notes the outcome for `wayland()`.
#[cfg(target_os = "linux")]
fn through_xwayland(cmd: &mut CommandLine) {
    let name = CefString::from("ozone-platform");
    if cmd.has_switch(Some(&name)) == 0
        && std::env::var_os("DISPLAY").is_some_and(|d| !d.is_empty())
    {
        cmd.append_switch_with_value(Some(&name), Some(&CefString::from("x11")));
    }
    let platform = CefString::from(&cmd.switch_value(Some(&name))).to_string();
    let wayland = platform == "wayland"
        || (platform.is_empty() && std::env::var_os("WAYLAND_DISPLAY").is_some());
    WAYLAND.store(wayland, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(target_os = "linux")]
static WAYLAND: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether Chromium runs as a Wayland client, so that the frame's child
/// windows are subsurfaces of it (ADR-0012). Always false off Linux.
pub(crate) fn wayland() -> bool {
    #[cfg(target_os = "linux")]
    return WAYLAND.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(not(target_os = "linux"))]
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
            if !config.load_extensions.is_empty() {
                let list = config
                    .load_extensions
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                cmd.append_switch_with_value(Some(&CefString::from("load-extension")), Some(&CefString::from(list.as_str())));
            }
            #[cfg(target_os = "macos")]
            enable_feature(cmd, PIP_WITHOUT_ADDRESS);
            #[cfg(target_os = "linux")]
            through_xwayland(cmd);
            // Our private DevTools session (`devpipe.rs`): fds 3 and 4.
            if crate::devpipe::is_open() {
                cmd.append_switch(Some(&CefString::from("remote-debugging-pipe")));
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
            scheme::install_factory(engine.config().ui_dir.clone(), crate::favicon::cache_dir(&engine.config().data_dir));
            engine.lock().router = Some(router::create_browser_router());
            engine.start_tick();
            #[cfg(target_os = "macos")]
            crate::mac::install_pip_swipe();
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
