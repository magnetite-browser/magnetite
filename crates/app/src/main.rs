//! magnetite: wires the engine, the domain model and the sidebar UI.

mod adblock;
mod app;
mod capture;
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
#[command(name = "magnetite")]
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

fn main() {
    let cli = parse_cli(std::env::args()).unwrap_or_else(|e| e.exit());

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
        load_extensions: cli.load_extension,
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
    let delegate = Arc::new(app::MagnetiteApp::new(data_dir, import, cli.open));
    app::install_global(&delegate);
    std::process::exit(magnetite_engine::run(config, delegate));
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
/// processes on Linux and Windows (macOS uses the bundle's helper apps).
fn helper_path() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let helper = exe.with_file_name(format!("magnetite-helper{}", std::env::consts::EXE_SUFFIX));
    helper.is_file().then_some(helper)
}

/// macOS bundle: `<App>.app/Contents/Resources/ui`. Linux and Windows
/// packages: `ui` next to the executable. Dev: `crates/ui/web/dist`.
fn default_ui_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let packaged = if cfg!(target_os = "macos") {
            dir.join("../Resources/ui")
        } else {
            dir.join("ui")
        };
        if packaged.is_dir() {
            return packaged;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ui/web/dist")
}

#[cfg(test)]
mod tests {
    use super::parse_cli;

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
}
