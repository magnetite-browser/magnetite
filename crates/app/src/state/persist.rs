//! Loading the state at start and saving it: `state.json` in the data
//! directory, and what a first start begins with.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use magnetite_core::split::SplitNode;
use magnetite_core::{Id, Profile, SidebarSnapshot, Space};
use serde::{Deserialize, Serialize};

use super::folders::smart_key;
use super::import::fresh_import;
use super::settings::Settings;
use super::{AppState, ArcImport, ArchivedTab, TabRuntime, new_id, now};

#[derive(Serialize, Deserialize)]
pub(super) struct Persisted {
    sidebar: SidebarSnapshot,
    active_space: Id,
    #[serde(default)]
    active_tab: HashMap<Id, Id>,
    #[serde(default)]
    archive: Vec<ArchivedTab>,
    #[serde(default)]
    favicons: HashMap<Id, String>,
    #[serde(default, deserialize_with = "splits_compat")]
    splits: Vec<SplitNode<Id>>,
    #[serde(default)]
    settings: Settings,
    #[serde(default)]
    downloads: Vec<crate::downloads::Download>,
    #[serde(default)]
    deleted_profiles: Vec<Id>,
    #[serde(default)]
    unsynced_spaces: BTreeSet<Id>,
}

/// Reads saved splits in the current shape (a list) and the earlier one
/// (one split per space id), so an update never drops the saved sidebar.
pub(super) fn splits_compat<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<SplitNode<Id>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Saved {
        List(Vec<SplitNode<Id>>),
        PerSpace(HashMap<Id, SplitNode<Id>>),
    }
    Ok(match Saved::deserialize(d)? {
        Saved::List(list) => list,
        Saved::PerSpace(map) => map.into_values().collect(),
    })
}

pub(super) fn import_arc() -> Option<SidebarSnapshot> {
    // Tests never read the owner's real Arc data.
    if cfg!(test) {
        return None;
    }
    let path = magnetite_importer::default_sidebar_path()?;
    let snap = magnetite_importer::import_file(&path).ok()?;
    (!snap.spaces.is_empty()).then(|| fresh_import(snap))
}

/// The sidebar of a new installation: the default profile and one space.
pub(super) fn default_sidebar() -> SidebarSnapshot {
    let mut snap = SidebarSnapshot::new();
    snap.profiles.push(Profile {
        id: "default".into(),
        name: "Default".into(),
        is_default: true,
        ephemeral: false,
    });
    snap.favorites.push(magnetite_core::FavoriteSet {
        profile_id: "default".into(),
        items: vec![],
    });
    snap.spaces.push(Space {
        id: new_id(),
        title: "Personal".into(),
        icon: Some("🏠".into()),
        accent: Some(magnetite_core::Color {
            red: 0.45,
            green: 0.55,
            blue: 1.0,
        }),
        profile_id: "default".into(),
        pinned: vec![],
        // No tab: a new installation contacts no site before the user
        // opens one.
        today: vec![],
        bookmarks: Vec::new(),
    });
    snap
}

impl AppState {
    pub fn load_or_default(data_dir: &Path, import: ArcImport) -> Self {
        let path = data_dir.join("state.json");
        // No file at all; one that could not be read is no first start.
        let first_start = !path.exists();
        let persisted = Self::read(&path);
        let deleted_profiles = persisted
            .as_ref()
            .map(|p| p.deleted_profiles.clone())
            .unwrap_or_default();
        let unsynced_spaces = persisted
            .as_ref()
            .map(|p| p.unsynced_spaces.clone())
            .unwrap_or_default();
        // `--import-arc`: Arc's sidebar takes the place of the saved one. The
        // archive, settings and downloads stay, and without a sidebar file
        // of Arc nothing changes.
        let replacement = (import == ArcImport::Now)
            .then(import_arc)
            .flatten()
            .filter(|sidebar| !sidebar.spaces.is_empty());
        let replaced = replacement.is_some() && persisted.is_some();
        let (sidebar, active_space, active_tab, archive, favicons, splits, settings, downloads) =
            match (persisted, replacement) {
                (Some(p), Some(sidebar)) => {
                    let space = sidebar.spaces[0].id.clone();
                    (
                        sidebar,
                        space,
                        HashMap::new(),
                        p.archive,
                        HashMap::new(),
                        Vec::new(),
                        p.settings.sanitised(),
                        p.downloads,
                    )
                }
                (Some(p), None) => (
                    p.sidebar,
                    p.active_space,
                    p.active_tab,
                    p.archive,
                    p.favicons,
                    p.splits,
                    p.settings.sanitised(),
                    p.downloads,
                ),
                // Nothing saved: a plain sidebar. Nothing is read from
                // another browser unless the user asks for it (the welcome
                // page, Settings, `--import-arc`).
                (None, replacement) => {
                    let sidebar = replacement.unwrap_or_else(default_sidebar);
                    let space = sidebar.spaces[0].id.clone();
                    (
                        sidebar,
                        space,
                        HashMap::new(),
                        Vec::new(),
                        HashMap::new(),
                        Vec::new(),
                        Settings::default(),
                        Vec::new(),
                    )
                }
            };
        let mut runtime = HashMap::new();
        for (tab, icon) in favicons {
            runtime.insert(
                tab,
                TabRuntime {
                    favicon: Some(icon),
                    ..Default::default()
                },
            );
        }
        let mut state = Self {
            sidebar,
            active_space,
            active_tab,
            runtime,
            content_to_tab: HashMap::new(),
            archive,
            history: super::History::load(data_dir),
            splits,
            peek: None,
            minis: Vec::new(),
            settings,
            sidebar_visible: true,
            accounts: crate::smart::Accounts::load(data_dir),
            smart_status: HashMap::new(),
            downloads: crate::downloads::Downloads::restore(downloads),
            deleted_profiles,
            unsynced_spaces,
            first_start,
            synced_sidebar: None,
            path,
        };
        // Only a space that exists can be kept on this device.
        let spaces = &state.sidebar.spaces;
        state
            .unsynced_spaces
            .retain(|id| spaces.iter().any(|sp| &sp.id == id));
        if !state
            .sidebar
            .spaces
            .iter()
            .any(|s| s.id == state.active_space)
        {
            state.active_space.clone_from(&state.sidebar.spaces[0].id);
        }
        // Folders from before smart_allowed existed were all made here.
        if state.settings.smart_allowed.is_none() {
            let keys = state
                .smart_folders()
                .iter()
                .map(|(id, sf)| smart_key(id, sf))
                .collect();
            state.settings.smart_allowed = Some(keys);
        }
        state.normalise_favorites();
        // A tab that was not saved (a private one) cannot be a space's
        // active tab at the next start.
        let gone: Vec<Id> = (state.active_tab.values())
            .filter(|tab| state.find_tab(tab).is_none())
            .cloned()
            .collect();
        state.active_tab.retain(|_, tab| !gone.contains(tab));
        if replaced {
            // The smart folders of a sidebar imported here may fetch here.
            for (id, _) in state.smart_folders() {
                state.allow_smart(&id);
            }
        }
        state
    }

    /// The saved state, if there is a usable one. A file that is there but
    /// cannot be used (unreadable, not text, damaged, or without spaces, which
    /// cannot be shown) is moved aside: starting fresh would overwrite it on
    /// the next save.
    pub(super) fn read(path: &Path) -> Option<Persisted> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => return Self::keep_aside(path, &e.to_string()),
        };
        // Saved before the rename from Forge: its own pages were forge://.
        let text = String::from_utf8_lossy(&bytes).replace("\"forge://", "\"magnetite://");
        match serde_json::from_str::<Persisted>(&text) {
            Ok(p) if p.sidebar.spaces.is_empty() => Self::keep_aside(path, "it has no spaces"),
            Ok(p) => Some(p),
            Err(e) => Self::keep_aside(path, &e.to_string()),
        }
    }

    fn keep_aside(path: &Path, why: &str) -> Option<Persisted> {
        let aside = path.with_extension(format!("json.unreadable-{}", now()));
        eprintln!(
            "magnetite: cannot use {} ({why}); kept as {}",
            path.display(),
            aside.display()
        );
        std::fs::rename(path, aside).ok();
        None
    }

    pub fn persist(&self) {
        let favicons = self
            .runtime
            .iter()
            .filter_map(|(id, r)| r.favicon.clone().map(|f| (id.clone(), f)))
            .collect();
        let p = Persisted {
            sidebar: self.lasting_sidebar(),
            active_space: self.active_space.clone(),
            active_tab: self.active_tab.clone(),
            archive: self.archive.clone(),
            favicons,
            splits: self.splits.clone(),
            settings: self.settings.clone(),
            downloads: self.downloads.lasting(),
            deleted_profiles: self.deleted_profiles.clone(),
            unsynced_spaces: self.unsynced_spaces.clone(),
        };
        let write = || -> std::io::Result<()> {
            if let Some(dir) = self.path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            crate::files::write_private(&self.path, &serde_json::to_vec_pretty(&p)?)
        };
        if let Err(e) = write() {
            eprintln!("magnetite: cannot save state: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::state;

    #[test]
    fn a_saved_sidebar_without_spaces_starts_fresh() {
        let dir = crate::files::test_dir("empty-sidebar");
        std::fs::create_dir_all(&dir).unwrap();
        let mut snap = serde_json::to_value(default_sidebar()).unwrap();
        snap["spaces"] = serde_json::json!([]);
        let saved = serde_json::json!({ "sidebar": snap, "active_space": "gone" });
        std::fs::write(dir.join("state.json"), saved.to_string()).unwrap();
        let s = AppState::load_or_default(&dir, ArcImport::Never);
        assert_ne!(s.sidebar.spaces, vec![]);
        assert_eq!(s.active_space, s.sidebar.spaces[0].id);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unreadable_state_file_is_kept_aside_not_overwritten() {
        let dir = crate::files::test_dir("unreadable");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("state.json"), r#"{"sidebar": null}"#).unwrap();
        let s = AppState::load_or_default(&dir, ArcImport::Never);
        assert!(!s.sidebar.spaces.is_empty(), "starts fresh");
        let aside: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.starts_with("state.json.unreadable-"))
            .collect();
        assert_eq!(aside.len(), 1, "{aside:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saved_splits_load_from_the_list_and_the_earlier_per_space_shape() {
        let split = r#"{"kind":"split","axis":"row","children":[{"kind":"pane","id":"a"},{"kind":"pane","id":"b"}],"sizes":[0.5,0.5]}"#;
        let snap = serde_json::to_string(&state().sidebar).unwrap();
        for saved in [format!("[{split}]"), format!(r#"{{"space-1":{split}}}"#)] {
            let json = format!(r#"{{"sidebar":{snap},"active_space":"x","splits":{saved}}}"#);
            let p: Persisted = serde_json::from_str(&json).unwrap();
            assert_eq!(p.splits.len(), 1);
            assert_eq!(p.splits[0].ids(), vec!["a", "b"]);
        }
    }

    #[test]
    fn a_state_file_that_cannot_be_used_is_kept_aside() {
        for (name, content) in [
            ("not-text", &[0xff_u8, 0xfe, 0x00][..]),
            (
                "no-spaces",
                br#"{"sidebar":{"format":"magnetite-sidebar","version":1,"spaces":[]},"active_space":"x"}"#,
            ),
        ] {
            let dir = crate::files::test_dir(&format!("aside-{name}"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("state.json"), content).unwrap();
            let s = AppState::load_or_default(&dir, ArcImport::Never);
            assert!(!s.sidebar.spaces.is_empty(), "{name}: starts fresh");
            let kept = std::fs::read_dir(&dir)
                .unwrap()
                .filter_map(Result::ok)
                .filter(|e| {
                    let file = e.file_name();
                    file.to_string_lossy().starts_with("state.json.unreadable-")
                })
                .count();
            assert_eq!(kept, 1, "{name}: the old file is still there");
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn loaded_state_is_checked_like_any_other() {
        let mut s = state();
        s.settings.auto_archive_hours = 0;
        // A private Today tab in front: it is not saved.
        s.sidebar.profiles.push(Profile {
            id: "secret".into(),
            name: "Secret".into(),
            is_default: false,
            ephemeral: true,
        });
        s.find_tab_mut("b").unwrap().profile_id = Some("secret".into());
        s.deleted_profiles.push("old".into());
        // Kept on this device: the space that exists, not one that is gone.
        let space = s.active_space.clone();
        s.unsynced_spaces = [space.clone(), "gone".to_string()].into();
        s.persist();
        let dir = s.data_dir().to_path_buf();
        let loaded = AppState::load_or_default(&dir, ArcImport::Never);
        assert_eq!(loaded.settings.auto_archive_hours, 1);
        assert_eq!(
            loaded.active_tab_id(),
            None,
            "the unsaved tab is not active"
        );
        assert_eq!(loaded.deleted_profiles, ["old"]);
        assert_eq!(loaded.unsynced_spaces, [space].into());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_first_start_takes_nothing_from_another_browser() {
        let s = state();
        let dir = s.data_dir().to_path_buf();
        // Nothing saved yet: the first start, with a plain sidebar.
        let fresh = AppState::load_or_default(&dir, ArcImport::Ask);
        assert!(fresh.first_start);
        let titles: Vec<&str> = fresh
            .sidebar
            .spaces
            .iter()
            .map(|sp| sp.title.as_str())
            .collect();
        assert_eq!(titles, ["Personal"]);
        assert_eq!(fresh.sidebar.all_tabs().count(), 0, "and it opens no site");
        fresh.persist();
        assert!(!AppState::load_or_default(&dir, ArcImport::Ask).first_start);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persist_roundtrip() {
        let mut s = state();
        s.settings.auto_archive_hours = 24;
        s.persist();
        let dir = s.data_dir().to_path_buf();
        let loaded = AppState::load_or_default(&dir, ArcImport::Never);
        assert_eq!(loaded.settings.auto_archive_hours, 24);
        assert_eq!(loaded.sidebar, s.sidebar);
        assert_eq!(loaded.active_tab_id(), Some("b"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
