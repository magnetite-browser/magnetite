use std::path::PathBuf;

/// Startup configuration for the browser process.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Directory for CEF's profile data (cookies, cache, extensions).
    pub data_dir: PathBuf,
    /// Directory with the built sidebar UI (served at `magnetite://ui/`).
    pub ui_dir: PathBuf,
    /// URL the sidebar view loads. Defaults to `magnetite://ui/`; a dev server
    /// URL works too.
    pub ui_url: String,
    /// Width of the sidebar in DIP.
    pub sidebar_width: i32,
    /// Development only: avoid the macOS keychain prompt for unsigned builds.
    pub use_mock_keychain: bool,
    /// Unpacked extension directories to load (`--load-extension`).
    pub load_extensions: Vec<PathBuf>,
    /// Show Chrome's toolbar (address bar, extension buttons) at the top of
    /// every content window: Chromium's own way to reach extension popups,
    /// next to the sidebar's buttons and the Extensions menu.
    pub chrome_toolbar: bool,
    /// Executable for Chromium's other processes (renderer, GPU). macOS
    /// finds the helper apps in the bundle; elsewhere it must be named, or
    /// Chromium starts the main executable, which lacks the page-side setup.
    pub helper: Option<PathBuf>,
}

impl EngineConfig {
    pub const SIDEBAR_URL: &'static str = "magnetite://ui/";

    /// Chromium's root cache folder: all its profile data lives below.
    pub(crate) fn cef_root(&self) -> PathBuf {
        self.data_dir.join("cef")
    }

    /// Chromium's folder for the default context. Chromium always keeps it in
    /// `Default`: CEF ignores any other child folder given as `cache_path`.
    /// The capital matters on case-sensitive file systems (Linux).
    pub(crate) fn default_profile_dir(&self) -> PathBuf {
        self.cef_root().join("Default")
    }

    /// Chromium's folder for profile `id`: directly below the root, the only
    /// place where Chromium keeps a profile on disk (anywhere else it falls
    /// back to one in memory).
    pub(crate) fn profile_dir(&self, id: &str) -> PathBuf {
        self.cef_root()
            .join(format!("{PROFILE_DIR_PREFIX}{}", profile_dir_name(id)))
    }
}

/// Start of every profile folder's name, which nothing else in Chromium's
/// root folder has.
const PROFILE_DIR_PREFIX: &str = "profile-";

/// Folder name for profile `id`, a different one for every id (two profiles
/// must never share cookies). Ids of letters, digits, `-` and `_` keep their
/// name (every id Magnetite makes); any other id is spelled out in hex behind
/// `x-`, which no kept name starts with.
fn profile_dir_name(id: &str) -> String {
    let plain = !id.is_empty()
        && !id.starts_with("x-")
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if plain {
        id.to_string()
    } else {
        id.bytes().fold(String::from("x-"), |mut name, b| {
            use std::fmt::Write;
            let _ = write!(name, "{b:02x}");
            name
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{EngineConfig, profile_dir_name};

    #[test]
    fn every_profile_id_gets_its_own_folder() {
        let id = "5f1c2b9e-0d4e-4e7a-9a51-1b2c3d4e5f60";
        assert_eq!(profile_dir_name(id), id);
        let names = ["work", "w/ork", "w.ork", "", "x-776f726b", "../work"].map(profile_dir_name);
        for (i, a) in names.iter().enumerate() {
            assert!(!a.contains('/') && !a.contains('.'), "{a}");
            assert!(names[i + 1..].iter().all(|b| a != b), "{a} twice");
        }
        let config = config("/data".into());
        // Where Chromium keeps the default profile's data, capital D included.
        assert_eq!(
            config.default_profile_dir(),
            std::path::Path::new("/data/cef/Default")
        );
        // Directly below Chromium's root: only there a profile reaches the disk.
        assert_eq!(
            config.profile_dir("work"),
            std::path::Path::new("/data/cef/profile-work")
        );
    }

    fn config(data_dir: std::path::PathBuf) -> EngineConfig {
        EngineConfig {
            data_dir,
            ui_dir: "/ui".into(),
            ui_url: EngineConfig::SIDEBAR_URL.into(),
            sidebar_width: 264,
            use_mock_keychain: false,
            load_extensions: vec![],
            chrome_toolbar: false,
            helper: None,
        }
    }
}
