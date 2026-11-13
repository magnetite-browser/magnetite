//! Process entry points: the browser process and the helper (renderer, GPU,
//! utility) processes.

use std::sync::Arc;

use cef::args::Args;
use cef::*;

use crate::cef_app::{BrowserApp, HelperApp};
use crate::config::EngineConfig;
use crate::engine::{Engine, EngineDelegate};

/// Windows: the sandbox information CEF's `bootstrap.exe` made for this
/// process; null everywhere else.
static SANDBOX_INFO: std::sync::atomic::AtomicPtr<u8> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

/// Windows: hand over the sandbox information of CEF's bootstrap, before
/// [`run`] or [`run_helper`]. Without it Chromium's sandboxed processes
/// (the GPU process first) do not start.
#[cfg(target_os = "windows")]
pub fn set_sandbox_info(info: *mut std::ffi::c_void) {
    SANDBOX_INFO.store(info.cast(), std::sync::atomic::Ordering::SeqCst);
}

/// Whether CEF's bootstrap started this process (Windows packages).
pub fn started_by_bootstrap() -> bool {
    !sandbox_info().is_null()
}

fn sandbox_info() -> *mut u8 {
    SANDBOX_INFO.load(std::sync::atomic::Ordering::SeqCst)
}

/// Run the browser process. Blocks until the last window closes.
/// Returns the process exit code.
pub fn run(config: EngineConfig, delegate: Arc<dyn EngineDelegate>) -> i32 {
    #[cfg(target_os = "macos")]
    let _loader = {
        let loader =
            library_loader::LibraryLoader::new(&std::env::current_exe().expect("exe path"), false);
        assert!(loader.load(), "failed to load the CEF framework");
        loader
    };
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    #[cfg(target_os = "macos")]
    crate::mac::setup_application();

    let args = Args::new();
    let main_args = args.as_main_args();

    // The main executable is only ever the browser process on macOS.
    let code = execute_process(Some(main_args), None::<&mut App>, sandbox_info());
    if code >= 0 {
        return code;
    }

    if let Err(e) = std::fs::create_dir_all(&config.data_dir) {
        crate::trace(&format!("cannot create {}: {e}", config.data_dir.display()));
    }
    // Before CEF starts: Chromium picks up the pipe's fds during start-up.
    if !crate::devpipe::open() {
        crate::trace("no DevTools pipe: fds 3 and 4 are taken");
    }
    let root_cache = config.cef_root();
    let default_cache = config.default_profile_dir();
    let helper = config
        .helper
        .as_ref()
        .map(|p| CefString::from(p.to_string_lossy().as_ref()))
        .unwrap_or_default();
    Engine::init(config, delegate);

    let mut app = BrowserApp::new();
    let settings = Settings {
        browser_subprocess_path: helper,
        no_sandbox: i32::from(!cfg!(feature = "sandbox")),
        root_cache_path: CefString::from(root_cache.to_string_lossy().as_ref()),
        cache_path: CefString::from(default_cache.to_string_lossy().as_ref()),
        log_severity: LogSeverity::WARNING,
        // Development builds only: Chrome DevTools Protocol on localhost, so
        // tests can drive pages without synthetic input. Any local program
        // could drive it, so release builds ignore the variable.
        remote_debugging_port: std::env::var("MAGNETITE_DEVTOOLS_PORT")
            .ok()
            .filter(|_| cfg!(debug_assertions))
            .and_then(|p| p.parse().ok())
            .unwrap_or(0),
        ..Default::default()
    };
    // 0 by design when another Magnetite already runs on this data folder,
    // and when CEF cannot start: leave with CEF's exit code, calling nothing
    // else of CEF.
    if initialize(
        Some(main_args),
        Some(&settings),
        Some(&mut app),
        sandbox_info(),
    ) != 1
    {
        return get_exit_code();
    }

    #[cfg(target_os = "macos")]
    let _app_delegate = crate::mac::setup_app_delegate();

    run_message_loop();
    shutdown();
    0
}

/// Entry point of the helper executable.
pub fn run_helper() -> i32 {
    let args = Args::new();

    #[cfg(all(target_os = "macos", feature = "sandbox"))]
    let _sandbox = {
        let mut sandbox = cef::sandbox::Sandbox::new();
        sandbox.initialize(args.as_main_args());
        sandbox
    };

    #[cfg(target_os = "macos")]
    let _loader = {
        let loader =
            library_loader::LibraryLoader::new(&std::env::current_exe().expect("exe path"), true);
        assert!(loader.load(), "failed to load the CEF framework");
        loader
    };
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let mut app = HelperApp::create();
    execute_process(Some(args.as_main_args()), Some(&mut app), sandbox_info())
}
