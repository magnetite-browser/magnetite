//! Chrome and other Chromium-based browsers: one space per profile with its
//! pinned tabs, tab groups (as folders), the other open tabs (as Today) and
//! its bookmarks, which stay bookmarks.

pub mod session;

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use magnetite_core::{FavoriteSet, Folder, Profile, SidebarNode, Space, TabItem};
use serde::Deserialize;

use crate::{Ids, ImportError, MAX_DEPTH, MAX_FILE_BYTES, SidebarSnapshot, importable, read_file};

/// Import every profile of the Chromium data folder `root` (the folder
/// holding `Local State`). `name` is the browser's display name.
pub fn import(root: &Path, name: &str) -> Result<SidebarSnapshot, ImportError> {
    let profiles = profiles(root)?;
    let single = profiles.len() == 1;
    let mut ids = Ids::new("chromium");
    let mut snap = SidebarSnapshot::new();
    for (i, (dir, title)) in profiles.into_iter().enumerate() {
        let profile_id = format!("profile-{i}");
        // An unreadable session (encrypted, locked) still leaves the bookmarks.
        let session = session::read_latest(&dir).unwrap_or(None);
        let bookmarks = read_bookmarks(&dir)?;
        let space = space(
            &mut ids,
            if single {
                name.to_string()
            } else {
                title.clone()
            },
            profile_id.clone(),
            session.as_ref(),
            bookmarks.as_ref(),
        )?;
        if space.pinned.is_empty() && space.today.is_empty() && space.bookmarks.is_empty() {
            continue;
        }
        snap.profiles.push(Profile {
            id: profile_id.clone(),
            name: title,
            is_default: i == 0,
            ephemeral: false,
        });
        snap.favorites.push(FavoriteSet {
            profile_id,
            items: vec![],
        });
        snap.spaces.push(space);
    }
    if snap.spaces.is_empty() {
        return Err(ImportError::Empty(name.to_string()));
    }
    Ok(snap)
}

/// The profile folders in the browser's own order, with their names.
/// Opera keeps its only profile in the data folder itself.
fn profiles(root: &Path) -> Result<Vec<(PathBuf, String)>, ImportError> {
    #[derive(Deserialize, Default)]
    struct LocalState {
        #[serde(default)]
        profile: ProfileList,
    }
    #[derive(Deserialize, Default)]
    struct ProfileList {
        #[serde(default)]
        info_cache: BTreeMap<String, ProfileInfo>,
        #[serde(default)]
        profiles_order: Vec<String>,
    }
    #[derive(Deserialize)]
    struct ProfileInfo {
        #[serde(default)]
        name: String,
    }

    let state: LocalState =
        read_json(&root.join("Local State"), "Local State")?.unwrap_or_default();
    let list = state.profile;
    let mut listed = HashSet::new();
    let found: Vec<_> = list
        .profiles_order
        .iter()
        .chain(list.info_cache.keys())
        .map(String::as_str)
        // Each folder once, however often the file names it.
        .filter(|d| listed.insert(*d))
        // Folder names come from the file: keep them to one plain component
        // (no `C:` either, which Windows would take for a drive).
        .filter(|d| !d.is_empty() && !d.contains(['/', '\\', ':']) && !matches!(*d, "." | ".."))
        .map(|d| {
            let title = list
                .info_cache
                .get(d)
                .map(|p| p.name.trim().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| d.to_string());
            (root.join(d), title)
        })
        .filter(|(dir, _)| dir.is_dir())
        .collect();
    if found.is_empty() && root.join("Bookmarks").is_file() {
        return Ok(vec![(root.to_path_buf(), "Default".into())]);
    }
    Ok(found)
}

/// A profile's `Bookmarks` file, or `None` if it has none.
#[derive(Deserialize)]
struct Bookmarks {
    roots: Roots,
}

#[derive(Deserialize)]
struct Roots {
    bookmark_bar: Option<Bookmark>,
    other: Option<Bookmark>,
    synced: Option<Bookmark>,
}

#[derive(Deserialize)]
struct Bookmark {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    name: String,
    url: Option<String>,
    #[serde(default)]
    children: Vec<Bookmark>,
    date_added: Option<String>,
}

fn read_bookmarks(dir: &Path) -> Result<Option<Bookmarks>, ImportError> {
    read_json(&dir.join("Bookmarks"), "Bookmarks")
}

/// Parse a JSON file; `None` when it does not exist.
fn read_json<T: serde::de::DeserializeOwned>(
    path: &Path,
    what: &str,
) -> Result<Option<T>, ImportError> {
    let bytes = match read_file(path, MAX_FILE_BYTES) {
        Ok(bytes) => bytes,
        Err(ImportError::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(e) => return Err(e),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| ImportError::Corrupt {
            what: what.to_string(),
            detail: e.to_string(),
        })
}

/// A tab showing `url`, titled `title` or else its address; pinned ones
/// return to `url`.
fn tab(ids: &mut Ids, url: &str, title: &str, pinned: bool) -> Result<TabItem, ImportError> {
    let title = if title.trim().is_empty() { url } else { title };
    Ok(TabItem {
        home_url: pinned.then(|| url.to_string()),
        ..TabItem::new(ids.next()?, url, title)
    })
}

fn space(
    ids: &mut Ids,
    title: String,
    profile_id: String,
    session: Option<&session::Session>,
    bookmarks: Option<&Bookmarks>,
) -> Result<Space, ImportError> {
    let mut pinned = Vec::new();
    let mut today = Vec::new();
    if let Some(s) = session {
        let mut groups: Vec<Vec<SidebarNode>> = vec![Vec::new(); s.groups.len()];
        for t in s.tabs.iter().filter(|t| importable(&t.url)) {
            match t.group.filter(|g| *g < groups.len()) {
                Some(g) => groups[g].push(SidebarNode::Tab(tab(ids, &t.url, &t.title, true)?)),
                None if t.pinned => {
                    pinned.push(SidebarNode::Tab(tab(ids, &t.url, &t.title, true)?));
                }
                None => {
                    let mut item = tab(ids, &t.url, &t.title, false)?;
                    item.last_active_at = t.last_active;
                    today.push(item);
                }
            }
        }
        for (group, children) in s.groups.iter().zip(groups) {
            if !children.is_empty() {
                pinned.push(folder(ids, &group.title, "Tab group", children)?);
            }
        }
    }
    let mut saved = Vec::new();
    if let Some(b) = bookmarks {
        let roots = [
            (&b.roots.bookmark_bar, "Bookmarks bar"),
            (&b.roots.other, "Other bookmarks"),
            (&b.roots.synced, "Mobile bookmarks"),
        ];
        for (root, name) in roots {
            if let Some(root) = root {
                let children = bookmark_nodes(ids, &root.children, 1)?;
                if !children.is_empty() {
                    saved.push(folder(ids, name, name, children)?);
                }
            }
        }
    }
    Ok(Space {
        id: ids.next()?,
        title,
        icon: None,
        accent: None,
        profile_id,
        pinned,
        today,
        bookmarks: saved,
    })
}

fn folder(
    ids: &mut Ids,
    title: &str,
    fallback: &str,
    children: Vec<SidebarNode>,
) -> Result<SidebarNode, ImportError> {
    Ok(SidebarNode::Folder(Folder {
        id: ids.next()?,
        title: if title.trim().is_empty() {
            fallback.to_string()
        } else {
            title.to_string()
        },
        icon: None,
        children,
        smart: None,
    }))
}

/// Bookmarks keep their name and their folders (empty ones are left out).
fn bookmark_nodes(
    ids: &mut Ids,
    list: &[Bookmark],
    depth: usize,
) -> Result<Vec<SidebarNode>, ImportError> {
    let mut out = Vec::new();
    for b in list {
        match (b.kind.as_str(), b.url.as_deref()) {
            ("url", Some(url)) if importable(url) => {
                let mut item = tab(ids, url, &b.name, false)?;
                item.custom_title = !b.name.trim().is_empty();
                item.created_at = b.date_added.as_deref().and_then(chromium_time);
                out.push(SidebarNode::Tab(item));
            }
            ("folder", _) if depth < MAX_DEPTH => {
                let children = bookmark_nodes(ids, &b.children, depth + 1)?;
                if !children.is_empty() {
                    out.push(folder(ids, &b.name, "Folder", children)?);
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// A Chromium timestamp (microseconds since 1601, as text) in Unix seconds.
fn chromium_time(text: &str) -> Option<i64> {
    text.parse().ok().and_then(session::unix_seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("magnetite-chromium-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    const BOOKMARKS: &str = r#"{"roots":{
        "bookmark_bar":{"type":"folder","name":"Lesezeichenleiste","children":[
            {"type":"url","name":"Docs","url":"https://docs.example.org/","date_added":"13370000000000000"},
            {"type":"url","name":"Settings","url":"chrome://settings/"},
            {"type":"folder","name":"Work","children":[{"type":"url","name":"","url":"https://work.example.org/"}]},
            {"type":"folder","name":"Empty","children":[]}]},
        "other":{"type":"folder","name":"Other","children":[]},
        "synced":{"type":"folder","name":"Mobile","children":[{"type":"url","name":"Phone","url":"http://m.example.org/"}]}}}"#;

    #[test]
    fn each_profile_becomes_a_space_with_its_bookmarks() {
        let root = scratch("profiles");
        write(
            &root.join("Local State"),
            r#"{"profile":{"profiles_order":["Default","Profile 1","../escape"],
                "info_cache":{"Default":{"name":"Personal"},"Profile 1":{"name":"Work"},"Profile 2":{"name":"Gone"}}}}"#,
        );
        write(&root.join("Default/Bookmarks"), BOOKMARKS);
        write(
            &root.join("Profile 1/Bookmarks"),
            r#"{"roots":{"bookmark_bar":{"type":"folder","children":[{"type":"url","name":"Jira","url":"https://jira.example.org/"}]}}}"#,
        );
        std::fs::create_dir_all(root.join("../escape")).unwrap();

        let snap = import(&root, "Google Chrome").unwrap();
        let titles: Vec<_> = snap.spaces.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Personal", "Work"],
            "profile order, missing folders skipped"
        );
        assert!(snap.profiles[0].is_default && !snap.profiles[1].is_default);
        assert_eq!(snap.spaces[1].profile_id, snap.profiles[1].id);

        let personal = &snap.spaces[0];
        assert!(personal.pinned.is_empty(), "bookmarks are no pinned tabs");
        let folders: Vec<_> = personal
            .bookmarks
            .iter()
            .map(|n| match n {
                SidebarNode::Folder(f) => f.title.as_str(),
                SidebarNode::Tab(_) => "tab",
            })
            .collect();
        assert_eq!(
            folders,
            ["Bookmarks bar", "Mobile bookmarks"],
            "empty roots left out"
        );
        let SidebarNode::Folder(bar) = &personal.bookmarks[0] else {
            unreachable!()
        };
        let SidebarNode::Tab(docs) = &bar.children[0] else {
            panic!("bookmark first")
        };
        assert_eq!((docs.title.as_str(), docs.custom_title), ("Docs", true));
        assert_eq!(docs.url, "https://docs.example.org/");
        assert_eq!(docs.home_url, None, "a bookmark has no home page");
        assert_eq!(docs.created_at, Some(1_725_526_400));
        assert_eq!(
            bar.children.len(),
            2,
            "chrome:// page and empty folder left out"
        );
        let SidebarNode::Folder(work) = &bar.children[1] else {
            panic!("folder kept")
        };
        let SidebarNode::Tab(unnamed) = &work.children[0] else {
            unreachable!()
        };
        assert_eq!(
            (unnamed.title.as_str(), unnamed.custom_title),
            ("https://work.example.org/", false)
        );
        let _ = std::fs::remove_dir_all(root.join("../escape"));
    }

    #[test]
    fn a_profile_listed_twice_is_imported_once() {
        let root = scratch("listed-twice");
        write(
            &root.join("Local State"),
            r#"{"profile":{"profiles_order":["Default","Default"],"info_cache":{"Default":{"name":"Personal"}}}}"#,
        );
        write(&root.join("Default/Bookmarks"), BOOKMARKS);
        let snap = import(&root, "Google Chrome").unwrap();
        let titles: Vec<_> = snap.spaces.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Google Chrome"], "one profile, so one space");
    }

    #[test]
    fn a_single_profile_is_named_after_the_browser_and_opera_has_it_in_the_root() {
        let root = scratch("opera");
        write(&root.join("Local State"), "{}");
        write(&root.join("Bookmarks"), BOOKMARKS);
        let snap = import(&root, "Opera").unwrap();
        assert_eq!(snap.spaces.len(), 1);
        assert_eq!(snap.spaces[0].title, "Opera");
    }

    #[test]
    fn nothing_to_import_is_an_error_and_a_broken_file_says_which() {
        let root = scratch("empty");
        write(
            &root.join("Local State"),
            r#"{"profile":{"profiles_order":["Default"]}}"#,
        );
        std::fs::create_dir_all(root.join("Default")).unwrap();
        assert!(matches!(import(&root, "Brave"), Err(ImportError::Empty(n)) if n == "Brave"));
        write(&root.join("Default/Bookmarks"), "{not json");
        assert!(
            matches!(import(&root, "Brave"), Err(ImportError::Corrupt { what, .. }) if what == "Bookmarks")
        );
    }
}
