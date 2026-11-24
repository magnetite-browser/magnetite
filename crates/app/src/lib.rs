//! magnetite: wires the engine, the domain model and the sidebar UI.

mod adblock;
mod app;
mod capture;
mod dev;
mod devices;
mod downloads;
mod files;
mod smart;
mod state;
mod sync;
mod update;
mod url_policy;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use clap::Parser;
use magnetite_engine::EngineConfig;

#[derive(Parser)]
// `--version` also tells a running Magnetite which version a system update
// put in its place (`update::SystemBinary`).
#[command(name = "magnetite", version)]
struct Cli {
    /// Directory holding profile data and state. Defaults to the platform app-data dir.
    #[arg(long)]
    data_dir: Option<PathBuf>,
    /// Directory with the built sidebar UI (defaults to the bundle's Resources/ui or crates/ui/web/dist).
    #[arg(long, env = "MAGNETITE_UI_DIR")]
    ui_dir: Option<PathBuf>,
    /// Load the sidebar from this URL instead of `magnetite://ui/` (e.g. a Vite dev server).
    #[arg(long, env = "MAGNETITE_UI_URL")]
    ui_url: Option<String>,
    /// Development: skip the macOS keychain for cookie encryption.
    #[arg(long)]
    mock_keychain: bool,
    /// Unpacked extension directory to load (repeatable).
    #[arg(long = "load-extension")]
    load_extension: Vec<PathBuf>,
    /// Take Arc's sidebar on this start, in place of the saved one.
    #[arg(long, conflicts_with = "no_import")]
    import_arc: bool,
    /// No welcome page on the first start: a plain sidebar (tests).
    #[arg(long)]
    no_import: bool,
    /// Show Chrome's toolbar inside pages (reach extension popups).
    #[arg(long)]
    chrome_toolbar: bool,
    /// Open this URL as a new tab after start (also used by tests).
    #[arg(long)]
    open: Option<String>,
    /// Addresses or files to open, as links from another app (`%U` in the
    /// desktop entry).
    urls: Vec<String>,
}

/// The command line. CEF helper processes never reach here (separate
/// executable), but Chromium may still hand us switches we do not know:
/// those are left out one by one, so that our own options keep counting. (A
/// test run that lost `--data-dir` this way would open the real profile.)
/// Anything else that is wrong ends the program with clap's message.
fn parse_cli(args: impl IntoIterator<Item = String>) -> Result<Cli, clap::Error> {
    use clap::error::{ContextKind, ContextValue, ErrorKind};
    let mut args: Vec<String> = args
        .into_iter()
        .filter(|a| !a.starts_with("--type="))
        .collect();
    loop {
        let err = match Cli::try_parse_from(&args) {
            Ok(cli) => return Ok(cli),
            Err(e) => e,
        };
        let unknown = match (err.kind(), err.get(ContextKind::InvalidArg)) {
            (ErrorKind::UnknownArgument, Some(ContextValue::String(arg))) => arg.clone(),
            _ => return Err(err),
        };
        let name = unknown.split_once('=').map_or(unknown.as_str(), |(n, _)| n);
        let Some(at) = args
            .iter()
            .skip(1)
            .position(|a| a.split_once('=').map_or(a.as_str(), |(n, _)| n) == name)
        else {
            return Err(err);
        };
        args.remove(at + 1);
    }
}

/// Run a helper process (renderer, GPU, utility). Returns its exit code.
pub fn run_helper() -> i32 {
    magnetite_engine::run_helper()
}

/// Windows: run the process CEF's bootstrap started, with the sandbox
/// information it made. Chromium starts its other processes the same way
/// (`--type=…`), so this is the browser or a helper. Returns the exit code.
#[cfg(windows)]
pub fn run_from_bootstrap(sandbox_info: *mut std::ffi::c_void) -> i32 {
    magnetite_engine::set_sandbox_info(sandbox_info);
    if is_helper(std::env::args()) {
        run_helper()
    } else {
        run()
    }
}

/// Whether Chromium started this process as one of its helpers
/// (`--type=…`). What follows `--single-argument` is a link from another
/// program and never says so: a link that reads `--type=renderer` must
/// start the browser, which then refuses it as an address.
#[cfg_attr(not(windows), allow(dead_code))]
fn is_helper(args: impl Iterator<Item = String>) -> bool {
    args.take_while(|a| a != SINGLE_ARGUMENT)
        .any(|a| a.starts_with("--type="))
}

/// Run the browser. Blocks until it quits; returns the process exit code.
pub fn run() -> i32 {
    // Before anything opens a file: the ad blocker compiles its lists on a
    // thread while the engine starts, and a file of its took one of the
    // pipe's descriptors, after which no extension opened for that run.
    magnetite_engine::reserve_devtools_pipe();
    // Windows starts its default browser with the link as the one argument
    // after `--single-argument` (`packaging/windows/magnetite.iss`): that
    // is a link from another program, never options of ours.
    let (args, link) = split_link(std::env::args().collect());
    let cli = parse_cli(args).unwrap_or_else(|e| e.exit());

    let data_dir = cli.data_dir.unwrap_or_else(default_data_dir);
    // Sidebar, tokens and Chromium's profiles: the user's only.
    if let Err(e) = files::private_dir(&data_dir) {
        eprintln!("magnetite: cannot make {} private: {e}", data_dir.display());
    }
    let ui_dir = cli.ui_dir.unwrap_or_else(default_ui_dir);
    let ui_url = cli
        .ui_url
        .unwrap_or_else(|| EngineConfig::SIDEBAR_URL.to_string());

    let config = EngineConfig {
        data_dir: data_dir.clone(),
        ui_dir,
        ui_url,
        sidebar_width: 264,
        use_mock_keychain: keychain_mode(&data_dir, cli.mock_keychain),
        // Filled in below, once the settings are read.
        accept_languages: String::new(),
        keep_unpacked_extensions: false,
        load_extensions: extension_dirs(
            &cli.load_extension,
            &std::env::current_dir().unwrap_or_default(),
        ),
        chrome_toolbar: cli.chrome_toolbar,
        helper: helper_path(),
    };
    let import = if cli.import_arc {
        state::ArcImport::Now
    } else if cli.no_import {
        state::ArcImport::Never
    } else {
        state::ArcImport::Ask
    };
    let cwd = std::env::current_dir().unwrap_or_default();
    let launch_urls = (cli.urls.iter().chain(&link))
        .map(|a| app::launch_url(a, &cwd))
        .collect();
    // Before anything is named: a first start names its space.
    magnetite_ui::set_language(magnetite_ui::resolve(
        state::AppState::saved_language(&data_dir).as_deref(),
        sys_locale::get_locales(),
    ));
    let delegate = Arc::new(app::MagnetiteApp::new(
        data_dir,
        packaged_scriptlets(),
        import,
        cli.open,
        launch_urls,
    ));
    app::install_global(&delegate);
    let config = EngineConfig {
        accept_languages: delegate.accept_languages(),
        keep_unpacked_extensions: delegate.keep_unpacked_extensions(),
        ..config
    };
    magnetite_engine::run(config, delegate)
}

/// The marker Chromium's command lines know: what follows is one argument,
/// whatever it looks like. A link from another program is handed over so,
/// and can then not smuggle in options (`--data-dir`, Chromium's switches).
const SINGLE_ARGUMENT: &str = "--single-argument";

/// Split the command line at `--single-argument`: our own arguments before
/// it, and the link after it (the arguments after it joined by spaces, as
/// a link with spaces would have been split). Only Windows hands links
/// over so: elsewhere the command line stays whole.
pub(crate) fn split_link(args: Vec<String>) -> (Vec<String>, Option<String>) {
    if cfg!(windows) {
        split_at_marker(args)
    } else {
        (args, None)
    }
}

fn split_at_marker(mut args: Vec<String>) -> (Vec<String>, Option<String>) {
    let Some(at) = args.iter().position(|a| a == SINGLE_ARGUMENT) else {
        return (args, None);
    };
    let link = args.split_off(at)[1..].join(" ");
    (args, Some(link).filter(|l| !l.is_empty()))
}

/// Lock `m`, also after a panic elsewhere left it poisoned: the data behind
/// the app's locks stays usable, and a stuck UI would be worse.
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Debug tracing to stderr when `MAGNETITE_LOG` is set; `msg` is only
/// built then. Names only, never personal data (`docs/design/security.md`).
pub(crate) fn trace(msg: impl FnOnce() -> String) {
    if std::env::var_os("MAGNETITE_LOG").is_some() {
        eprintln!("magnetite: {}", msg());
    }
}

/// Whether Chromium uses its fixed test key instead of the macOS keychain.
/// Chromium encrypts cookies and signs its protected settings with that key;
/// switching between the two makes it reset those settings (which uninstalls
/// extensions) and leaves cookies unreadable. So a data folder keeps the mode
/// it was used with: `--mock-keychain` once marks it, and later starts without
/// the flag (from the Dock) stay in that mode.
fn keychain_mode(data_dir: &std::path::Path, asked: bool) -> bool {
    let marker = data_dir.join("mock-keychain");
    if asked {
        std::fs::create_dir_all(data_dir).ok();
        std::fs::write(&marker, "").ok();
    }
    asked || marker.exists()
}

/// The folders `--load-extension` names, read as Chromium reads them: one
/// value may hold several with commas between, a leading `~` is the home
/// folder (not on Windows), and a relative one counts from `cwd` (stays
/// relative when the working folder is unknown). The same
/// list goes to Chromium and tells which command-line extensions this run
/// loads (`extensions::installed`).
fn extension_dirs(args: &[PathBuf], cwd: &std::path::Path) -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from);
    let resolve = |dir: &str| match (dir.strip_prefix('~'), &home) {
        (Some(rest), Some(home)) if cfg!(unix) && (rest.is_empty() || rest.starts_with('/')) => {
            home.join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(dir),
    };
    args.iter()
        .flat_map(|arg| {
            let arg = arg.to_string_lossy();
            arg.split(',')
                .filter(|d| !d.is_empty())
                .map(|d| cwd.join(resolve(d)))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn default_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    let (base, name) = (
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support")),
        "Magnetite",
    );
    #[cfg(windows)]
    let (base, name) = (
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        "Magnetite",
    );
    #[cfg(all(unix, not(target_os = "macos")))]
    let (base, name) = (
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))),
        "magnetite",
    );
    let base = base.unwrap_or_else(std::env::temp_dir);
    let dir = base.join(name);
    // The browser was called Forge until November 2026: take its data along.
    let old = base.join("forge-browser");
    if !dir.exists() && old.is_dir() {
        match std::fs::rename(&old, &dir) {
            Ok(()) => eprintln!("magnetite: moved {} to {}", old.display(), dir.display()),
            Err(e) => eprintln!("magnetite: cannot move {}: {e}", old.display()),
        }
    }
    dir
}

/// `magnetite-helper` next to the executable, for Chromium's other
/// processes on Linux and Windows (macOS uses the bundle's helper apps; a
/// Windows package starts them through CEF's bootstrap like the browser,
/// which the sandbox needs).
fn helper_path() -> Option<PathBuf> {
    if cfg!(target_os = "macos") || magnetite_engine::started_by_bootstrap() {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let helper = exe.with_file_name(format!("magnetite-helper{}", std::env::consts::EXE_SUFFIX));
    helper.is_file().then_some(helper)
}

/// The folder a package keeps `name` in: `<App>.app/Contents/Resources` in
/// the macOS bundle, next to the executable in Linux and Windows packages.
fn packaged(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    Some(if cfg!(target_os = "macos") {
        dir.join("../Resources").join(name)
    } else {
        dir.join(name)
    })
}

/// The package's `ui` folder. Dev: `crates/ui/web/dist`.
fn default_ui_dir() -> PathBuf {
    (packaged("ui").filter(|dir| dir.is_dir()))
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ui/web/dist"))
}

/// The scriptlets' code for the ad blocker (`adblock.rs`), a file of its
/// own in every package. A run from the checkout has none.
fn packaged_scriptlets() -> Option<PathBuf> {
    packaged("scriptlets/scriptlets.json").filter(|file| file.is_file())
}

#[cfg(test)]
mod tests {
    use super::{extension_dirs, is_helper, parse_cli, split_at_marker as split_link};
    use std::path::PathBuf;

    #[test]
    fn extension_folders_are_read_as_chromium_reads_them() {
        let cwd = PathBuf::from("/work");
        let args = [PathBuf::from("/a,rel/b"), PathBuf::from("/c")];
        assert_eq!(
            extension_dirs(&args, &cwd),
            [
                PathBuf::from("/a"),
                PathBuf::from("/work/rel/b"),
                PathBuf::from("/c")
            ]
        );
        #[cfg(unix)]
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            assert_eq!(
                extension_dirs(&[PathBuf::from("~/ext")], &cwd),
                [home.join("ext")]
            );
        }
    }

    #[test]
    fn a_link_never_makes_the_process_a_helper() {
        let args = |list: &[&str]| list.iter().map(|a| (*a).to_string()).collect::<Vec<_>>();
        assert!(is_helper(
            args(&["magnetite", "--type=renderer"]).into_iter()
        ));
        assert!(!is_helper(args(&["magnetite", "--no-import"]).into_iter()));
        assert!(!is_helper(
            args(&["magnetite", "--single-argument", "--type=renderer"]).into_iter()
        ));
    }

    #[test]
    fn a_link_from_another_program_is_never_an_option() {
        let args = |list: &[&str]| list.iter().map(|a| (*a).to_string()).collect::<Vec<_>>();
        let (ours, link) = split_link(args(&[
            "magnetite",
            "--no-import",
            "--single-argument",
            "https://example.org/a b",
        ]));
        assert_eq!(ours, args(&["magnetite", "--no-import"]));
        assert_eq!(link.as_deref(), Some("https://example.org/a b"));
        // What comes after the marker stays the link, whatever it looks like.
        let (ours, link) = split_link(args(&[
            "magnetite",
            "--single-argument",
            "--data-dir=/x",
            "--no-sandbox",
        ]));
        assert_eq!(ours, args(&["magnetite"]));
        assert_eq!(link.as_deref(), Some("--data-dir=/x --no-sandbox"));
        assert_eq!(
            split_link(args(&["magnetite", "--single-argument"])).1,
            None
        );
        assert_eq!(split_link(args(&["magnetite", "--open", "x"])).1, None);
    }

    fn cli(args: &[&str]) -> Result<super::Cli, clap::Error> {
        parse_cli(args.iter().map(|a| (*a).to_string()))
    }

    #[test]
    fn switches_we_do_not_know_do_not_cost_our_own() {
        let parsed = cli(&[
            "magnetite",
            "--data-dir",
            "/x",
            "--remote-debugging-port=1",
            "--no-import",
            "--disable-gpu",
            "--mock-keychain",
        ])
        .unwrap();
        assert_eq!(parsed.data_dir.as_deref(), Some(std::path::Path::new("/x")));
        assert!(parsed.no_import && parsed.mock_keychain);
        // A real mistake is an error, not a start on the default profile.
        assert!(cli(&["magnetite", "--data-dir"]).is_err());
    }

    /// Links and files from other apps arrive as plain arguments
    /// (`magnetite %U`): they are kept, in order, next to our own options and
    /// switches we do not know, and an option's value is never one of them.
    #[test]
    fn addresses_from_other_apps_survive_next_to_options_and_unknown_switches() {
        let parsed = cli(&[
            "magnetite",
            "--enable-features=Foo",
            "--data-dir",
            "/tmp/d",
            "https://a.example/",
            "page.html",
        ])
        .unwrap();
        assert_eq!(parsed.urls, ["https://a.example/", "page.html"]);
        assert_eq!(
            parsed.data_dir.as_deref(),
            Some(std::path::Path::new("/tmp/d"))
        );
    }
}
