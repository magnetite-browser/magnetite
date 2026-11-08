//! Importers for other browsers' sidebars, tabs and bookmarks.
//!
//! Each reads the user's own local files (nothing leaves the machine) and
//! converts them into a [`SidebarSnapshot`]: Arc's `StorableSidebar.json`
//! (no Arc code or assets involved, see `docs/spec/arc-sidebar-format.md`),
//! Chromium-based browsers (`chromium`) and Firefox or Zen (`firefox`).
//! Parsing is deliberately tolerant: unknown fields are ignored, missing ones
//! default. IDs in a snapshot are only unique within it; the app gives
//! imported items fresh IDs when it adds them. See `docs/spec/import.md`.

pub mod arc;
pub mod bookmarks_file;
pub mod chromium;
pub mod export;
pub mod firefox;

use std::io::Read as _;
use std::path::{Path, PathBuf};

pub use magnetite_core::SidebarSnapshot;
use magnetite_core::{FavoriteSet, Profile};

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("not a sidebar file: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("sidebar file has no item container")]
    NoContainer,
    #[error("sidebar folders are nested more than {0} deep")]
    TooDeep(usize),
    #[error("sidebar file has more than {0} tabs and folders")]
    TooManyItems(usize),
    /// A file of another browser that does not have the expected shape.
    #[error("cannot read {what}: {detail}")]
    Corrupt { what: String, detail: String },
    /// The browser's data folder has no profile with anything to import.
    #[error("{0} has nothing to import")]
    Empty(String),
}

/// Only web pages come along: the other browser's own pages do not exist
/// here, and Magnetite does not open local files from stored data. A web
/// address is what the app takes for one when it adds the import
/// (`crates/app/src/url_policy.rs`): `http` or `https`, then `//`.
pub(crate) fn importable(url: &str) -> bool {
    url.split_once(':').is_some_and(|(scheme, rest)| {
        (scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http"))
            && rest.starts_with("//")
    })
}

/// The profile of Arc's default spaces, and the only one of Firefox and Zen.
pub(crate) const DEFAULT_PROFILE: &str = "default";
/// Deepest folder nesting an import keeps: deeper Arc, Firefox and Zen files
/// are refused, deeper Chromium bookmark folders left out.
pub(crate) const MAX_DEPTH: usize = 32;
/// Most tabs, folders and spaces one import produces; more is refused.
pub(crate) const MAX_ITEMS: usize = 100_000;

/// Hands out IDs unique within one import (`<prefix>-1`, `<prefix>-2`, …)
/// for browsers whose items have none we can use, and refuses more than
/// `MAX_ITEMS` of them.
pub(crate) struct Ids {
    prefix: &'static str,
    /// IDs handed out so far.
    count: usize,
}

impl Ids {
    pub(crate) fn new(prefix: &'static str) -> Self {
        Self { prefix, count: 0 }
    }

    pub(crate) fn next(&mut self) -> Result<String, ImportError> {
        self.count += 1;
        if self.count > MAX_ITEMS {
            return Err(ImportError::TooManyItems(MAX_ITEMS));
        }
        Ok(format!("{}-{}", self.prefix, self.count))
    }
}

/// A snapshot with the default profile (`DEFAULT_PROFILE`) and its still
/// empty favorites.
pub(crate) fn default_profile_snapshot() -> SidebarSnapshot {
    SidebarSnapshot {
        profiles: vec![Profile {
            id: DEFAULT_PROFILE.into(),
            name: "Default".into(),
            is_default: true,
            ephemeral: false,
        }],
        favorites: vec![FavoriteSet {
            profile_id: DEFAULT_PROFILE.into(),
            items: Vec::new(),
        }],
        ..SidebarSnapshot::new()
    }
}

/// What kind of data a browser keeps, and so which importer reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Arc,
    /// Chrome and the browsers built on Chromium: same files.
    Chromium,
    Firefox,
    /// A Firefox fork with workspaces, essentials and folders.
    Zen,
}

/// A browser with importable data on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// Stable key, used by the UI ("chrome", "zen", ...).
    pub key: &'static str,
    pub name: &'static str,
    pub kind: Kind,
    /// Its data folder (for Arc: the sidebar file).
    pub path: PathBuf,
}

/// Browsers we know, with their data folders per platform: macOS below
/// `~/Library/Application Support`, Linux below `~` and Windows below
/// `%LOCALAPPDATA%` (or `%APPDATA%` where marked).
struct Known {
    key: &'static str,
    name: &'static str,
    kind: Kind,
    macos: &'static str,
    linux: &'static str,
    windows: &'static str,
    windows_roaming: bool,
}

const KNOWN: &[Known] = &[
    Known {
        key: "chrome",
        name: "Google Chrome",
        kind: Kind::Chromium,
        macos: "Google/Chrome",
        linux: ".config/google-chrome",
        windows: "Google/Chrome/User Data",
        windows_roaming: false,
    },
    Known {
        key: "brave",
        name: "Brave",
        kind: Kind::Chromium,
        macos: "BraveSoftware/Brave-Browser",
        linux: ".config/BraveSoftware/Brave-Browser",
        windows: "BraveSoftware/Brave-Browser/User Data",
        windows_roaming: false,
    },
    Known {
        key: "edge",
        name: "Microsoft Edge",
        kind: Kind::Chromium,
        macos: "Microsoft Edge",
        linux: ".config/microsoft-edge",
        windows: "Microsoft/Edge/User Data",
        windows_roaming: false,
    },
    Known {
        key: "vivaldi",
        name: "Vivaldi",
        kind: Kind::Chromium,
        macos: "Vivaldi",
        linux: ".config/vivaldi",
        windows: "Vivaldi/User Data",
        windows_roaming: false,
    },
    Known {
        key: "opera",
        name: "Opera",
        kind: Kind::Chromium,
        macos: "com.operasoftware.Opera",
        linux: ".config/opera",
        windows: "Opera Software/Opera Stable",
        windows_roaming: true,
    },
    Known {
        key: "chromium",
        name: "Chromium",
        kind: Kind::Chromium,
        macos: "Chromium",
        linux: ".config/chromium",
        windows: "Chromium/User Data",
        windows_roaming: false,
    },
    Known {
        key: "zen",
        name: "Zen",
        kind: Kind::Zen,
        macos: "zen",
        linux: ".zen",
        windows: "zen",
        windows_roaming: true,
    },
    Known {
        key: "firefox",
        name: "Firefox",
        kind: Kind::Firefox,
        macos: "Firefox",
        linux: ".mozilla/firefox",
        windows: "Mozilla/Firefox",
        windows_roaming: true,
    },
];

fn data_folder(k: &Known) -> Option<PathBuf> {
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Application Support").join(k.macos))
    } else if cfg!(target_os = "windows") {
        let var = if k.windows_roaming {
            "APPDATA"
        } else {
            "LOCALAPPDATA"
        };
        std::env::var_os(var).map(|b| PathBuf::from(b).join(k.windows))
    } else {
        home().map(|h| h.join(k.linux))
    }
}

/// Browsers with importable data on this machine, Arc first. Only checks
/// that their data folders look right; reads nothing else.
pub fn installed() -> Vec<Installed> {
    let arc = default_sidebar_path()
        .filter(|p| p.is_file())
        .map(|path| Installed {
            key: "arc",
            name: "Arc",
            kind: Kind::Arc,
            path,
        });
    let others = KNOWN.iter().filter_map(|k| {
        let path = data_folder(k)?;
        let marker = match k.kind {
            Kind::Chromium => "Local State",
            Kind::Firefox | Kind::Zen => "profiles.ini",
            Kind::Arc => return None,
        };
        path.join(marker).is_file().then_some(Installed {
            key: k.key,
            name: k.name,
            kind: k.kind,
            path,
        })
    });
    arc.into_iter().chain(others).collect()
}

/// Import everything we can read from `browser`.
pub fn import(browser: &Installed) -> Result<SidebarSnapshot, ImportError> {
    match browser.kind {
        Kind::Arc => import_file(&browser.path),
        Kind::Chromium => chromium::import(&browser.path, browser.name),
        Kind::Firefox => firefox::import(&browser.path, browser.name, false),
        Kind::Zen => firefox::import(&browser.path, browser.name, true),
    }
}

/// Default location of the sidebar file on this platform, if known.
pub fn default_sidebar_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home).join("Library/Application Support/Arc/StorableSidebar.json")
        })
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA").map(|base| {
            PathBuf::from(base).join("Packages/TheBrowserCompany.Arc_ttt1ap7aakyb4/LocalCache/Local/Arc/StorableSidebar.json")
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// The largest file an importer reads whole: far above a real profile's,
/// and a bound on what a tampered one can make the import allocate.
pub(crate) const MAX_FILE_BYTES: u64 = 256 << 20;

/// Read `path` whole, refusing anything but a regular file (a FIFO would
/// block the import for good) and a file larger than `limit` bytes
/// (`MAX_FILE_BYTES`).
fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    if !std::fs::metadata(path)?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::FileTooLarge,
            format!("larger than {} MiB", limit >> 20),
        ));
    }
    Ok(bytes)
}

/// `read_bounded`, failing with an `ImportError::Read` that names `path`.
pub(crate) fn read_file(path: &Path, limit: u64) -> Result<Vec<u8>, ImportError> {
    read_bounded(path, limit).map_err(|source| ImportError::Read {
        path: path.to_path_buf(),
        source,
    })
}

/// Parse a sidebar file from disk.
pub fn import_file(path: &Path) -> Result<SidebarSnapshot, ImportError> {
    let bytes = read_file(path, MAX_FILE_BYTES)?;
    let raw: arc::raw::SidebarFile = serde_json::from_slice(&bytes)?;
    arc::convert::to_snapshot(&raw)
}

/// Parse sidebar JSON text.
pub fn import_str(json: &str) -> Result<SidebarSnapshot, ImportError> {
    let raw: arc::raw::SidebarFile = serde_json::from_str(json)?;
    arc::convert::to_snapshot(&raw)
}

#[cfg(test)]
mod tests {
    use super::{ImportError, MAX_ITEMS};

    /// What the importers keep is what the app opens from an import: an
    /// address it would drop must not count as a tab (or keep a folder).
    #[test]
    fn web_addresses_need_their_slashes_to_be_importable() {
        assert!(super::importable("https://example.org/a"));
        assert!(super::importable("HTTP://example.org"));
        assert!(!super::importable("https:example.org"));
        assert!(!super::importable("http:/example.org"));
        assert!(!super::importable("about:blank"));
    }

    #[test]
    fn ids_count_from_one_and_stop_after_max_items() {
        let mut ids = super::Ids::new("ff");
        assert_eq!(ids.next().unwrap(), "ff-1");
        ids.count = MAX_ITEMS - 1;
        assert_eq!(ids.next().unwrap(), format!("ff-{MAX_ITEMS}"));
        assert!(matches!(
            ids.next(),
            Err(ImportError::TooManyItems(MAX_ITEMS))
        ));
    }

    #[test]
    fn whole_file_reads_stop_at_the_limit() {
        let dir = std::env::temp_dir().join(format!("magnetite-read-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Bookmarks");
        std::fs::write(&path, b"12345").unwrap();
        assert_eq!(super::read_bounded(&path, 5).unwrap(), b"12345");
        assert_eq!(
            super::read_bounded(&path, 4).unwrap_err().kind(),
            std::io::ErrorKind::FileTooLarge
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Opening a FIFO for reading waits for a writer that never comes.
    #[cfg(unix)]
    #[test]
    fn a_fifo_is_refused_instead_of_read() {
        let dir = std::env::temp_dir().join(format!("magnetite-fifo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fifo = dir.join("StorableSidebar.json");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(made.success());
        assert_eq!(
            super::read_bounded(&fifo, 1024).unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
