//! Firefox and Zen: open tabs, pinned tabs and bookmarks from a profile's
//! session file and bookmark backups; for Zen also workspaces (as spaces),
//! essentials (as favorites) and folders.
//!
//! Only JSON files are read, no SQLite: the session (`sessionstore.jsonlz4`
//! after a quit, `sessionstore-backups/recovery.jsonlz4` while running), the
//! newest bookmark backup (written about daily, so it can miss the latest
//! bookmarks) and Zen's sidebar (`zen-sessions.jsonlz4`, see `zen`). Most
//! are compressed (`mozlz4`). Input is untrusted: files, nesting and the
//! number of items are capped.

mod bookmarks;
mod mozlz4;
mod session;
mod zen;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use magnetite_core::{Folder, SidebarNode, Space, TabItem};
use serde::de::DeserializeOwned;

use crate::{
    DEFAULT_PROFILE, Ids, ImportError, SidebarSnapshot, default_profile_snapshot, importable,
    read_file,
};
use session::{Session, Tab, Window};

/// Import the default profile of the Firefox or Zen data folder `root`
/// (the folder holding `profiles.ini`). `name` is the browser's display name.
pub fn import(root: &Path, name: &str, zen: bool) -> Result<SidebarSnapshot, ImportError> {
    let empty = || ImportError::Empty(name.to_string());
    let profile = default_profile(root)?.ok_or_else(empty)?;
    let mut ids = Ids::new("ff");
    let bookmarks = match read_first::<bookmarks::Node>(&bookmark_backups(&profile)?)? {
        Some(tree) => bookmarks::folders(&tree, &mut ids)?,
        None => Vec::new(),
    };
    let snapshot = if zen {
        // `clean.jsonlz4` is Zen's copy of the previous save. Zen before 1.18
        // kept spaces in places.sqlite: its session windows have tabs but no
        // spaces, so all tabs land in one space.
        let files = [
            profile.join("zen-sessions.jsonlz4"),
            profile.join("zen-sessions-backup/clean.jsonlz4"),
        ];
        let sidebar = match read_first::<Window>(&files)? {
            Some(sidebar) if !sidebar.spaces.is_empty() => sidebar,
            _ => read_session(&profile)?
                .windows
                .into_iter()
                .find(Window::is_normal)
                .unwrap_or_default(),
        };
        zen::snapshot(&sidebar, name, bookmarks, &mut ids)?
    } else {
        firefox(&read_session(&profile)?, name, bookmarks, &mut ids)?
    };
    if snapshot.all_tabs().next().is_none() && snapshot.all_bookmarks().next().is_none() {
        return Err(empty());
    }
    Ok(snapshot)
}

/// One space titled `name`: pinned tabs, then each tab group as a folder
/// (open ones, then saved ones). The other open tabs are Today, and the
/// bookmark folders are the space's bookmarks.
fn firefox(
    session: &Session,
    name: &str,
    bookmarks: Vec<SidebarNode>,
    ids: &mut Ids,
) -> Result<SidebarSnapshot, ImportError> {
    let mut space = new_space(ids, name)?;
    let mut groups = Vec::new();
    // Group id to its index in `groups`, once the group has a tab.
    let mut placed = HashMap::new();
    for window in session.windows.iter().filter(|w| w.is_normal()) {
        let names: HashMap<_, _> = window
            .groups
            .iter()
            .filter_map(|g| Some((g.id.as_deref()?, g.name.as_deref())))
            .collect();
        for tab in &window.tabs {
            let group = tab
                .group_id
                .as_deref()
                .and_then(|id| Some((id, *names.get(id)?)));
            let Some(item) = sidebar_tab(ids, tab, tab.pinned || group.is_some())? else {
                continue;
            };
            match group {
                _ if tab.pinned => space.pinned.push(SidebarNode::Tab(item)),
                Some((id, title)) => {
                    let i = if let Some(&i) = placed.get(id) {
                        i
                    } else {
                        groups.push(folder(ids, title, Vec::new())?);
                        placed.insert(id, groups.len() - 1);
                        groups.len() - 1
                    };
                    groups[i].children.push(SidebarNode::Tab(item));
                }
                None => space.today.push(item),
            }
        }
    }
    for saved in &session.saved_groups {
        let mut children = Vec::new();
        for closed in &saved.tabs {
            if let Some(item) = sidebar_tab(ids, &closed.state, true)? {
                children.push(SidebarNode::Tab(item));
            }
        }
        if !children.is_empty() {
            groups.push(folder(ids, saved.name.as_deref(), children)?);
        }
    }
    space
        .pinned
        .extend(groups.into_iter().map(SidebarNode::Folder));
    space.bookmarks = bookmarks;

    let mut snapshot = default_profile_snapshot();
    snapshot.spaces.push(space);
    Ok(snapshot)
}

/// A session tab as a sidebar item, or `None` for internal pages. Pinned
/// items return to where Zen pinned them, else to their current page.
fn sidebar_tab(ids: &mut Ids, tab: &Tab, pinned: bool) -> Result<Option<TabItem>, ImportError> {
    fn page(e: &session::Entry) -> Option<(&str, &session::Entry)> {
        Some((e.url.as_deref().filter(|u| importable(u))?, e))
    }
    let home = pinned.then(|| tab.pinned_entry().and_then(page)).flatten();
    let Some((url, entry)) = tab.current().and_then(page).or(home) else {
        return Ok(None);
    };
    let label = tab.zen_static_label.as_deref().filter(|l| !l.is_empty());
    let title = entry.title.as_deref().filter(|t| !t.is_empty());
    Ok(Some(TabItem {
        custom_title: label.is_some(),
        home_url: pinned.then(|| home.map_or(url, |(u, _)| u).to_string()),
        last_active_at: tab.last_accessed.and_then(|ms| unix(ms, 1e3)),
        ..TabItem::new(ids.next()?, url, label.or(title).unwrap_or(url))
    }))
}

/// Unix seconds from a count of `1/per_second` seconds; `None` for values
/// that are not a plausible date.
#[allow(clippy::cast_possible_truncation)] // sub-second precision is irrelevant
fn unix(value: f64, per_second: f64) -> Option<i64> {
    let seconds = value / per_second;
    (seconds > 0.0 && seconds < 1e11).then_some(seconds as i64)
}

/// A folder titled `title` ("Untitled" if there is none).
fn folder(
    ids: &mut Ids,
    title: Option<&str>,
    children: Vec<SidebarNode>,
) -> Result<Folder, ImportError> {
    Ok(Folder {
        id: ids.next()?,
        title: title
            .filter(|t| !t.is_empty())
            .unwrap_or("Untitled")
            .to_string(),
        icon: None,
        children,
        smart: None,
    })
}

fn new_space(ids: &mut Ids, title: &str) -> Result<Space, ImportError> {
    Ok(Space {
        id: ids.next()?,
        title: title.to_string(),
        icon: None,
        accent: None,
        profile_id: DEFAULT_PROFILE.into(),
        pinned: Vec::new(),
        today: Vec::new(),
        bookmarks: Vec::new(),
    })
}

/// The default profile's folder, from `profiles.ini`: the one the first
/// `[Install…]` section opens, else the `[Profile…]` marked `Default=1`,
/// else the first. Paths are relative to `root` with `IsRelative=1`.
fn default_profile(root: &Path) -> Result<Option<PathBuf>, ImportError> {
    let text = read_file(&root.join("profiles.ini"), mozlz4::MAX_BYTES as u64)?;
    let text = String::from_utf8_lossy(&text);
    let sections = ini(&text);
    fn get<'a>(kv: &[(&'a str, &'a str)], key: &str) -> Option<&'a str> {
        kv.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }
    let profiles: Vec<_> = sections
        .iter()
        .filter(|(name, _)| name.starts_with("Profile"))
        .map(|(_, kv)| kv.as_slice())
        .collect();
    let installed = sections
        .iter()
        .filter(|(name, _)| name.starts_with("Install"))
        .find_map(|(_, kv)| get(kv, "Default"));
    let chosen = match installed {
        Some(path) => {
            let profile = profiles.iter().find(|p| get(p, "Path") == Some(path));
            Some((path, profile.and_then(|p| get(p, "IsRelative"))))
        }
        None => profiles
            .iter()
            .find(|p| get(p, "Default") == Some("1"))
            .or(profiles.first())
            .and_then(|p| Some((get(p, "Path")?, get(p, "IsRelative")))),
    };
    Ok(chosen.map(|(path, relative)| {
        if relative.map_or(Path::new(path).is_relative(), |r| r == "1") {
            root.join(path)
        } else {
            PathBuf::from(path)
        }
    }))
}

/// `[section]` headers with their `key=value` lines.
fn ini(text: &str) -> Vec<(&str, Vec<(&str, &str)>)> {
    let mut sections: Vec<(&str, Vec<_>)> = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            sections.push((name, Vec::new()));
        } else if let (Some((key, value)), Some((_, kv))) =
            (line.split_once('='), sections.last_mut())
        {
            kv.push((key.trim(), value.trim()));
        }
    }
    sections
}

/// The session Firefox wrote last: `sessionstore.jsonlz4` when it quit,
/// `sessionstore-backups/recovery.jsonlz4` while it runs. Empty without one.
fn read_session(profile: &Path) -> Result<Session, ImportError> {
    let mut files = [
        profile.join("sessionstore.jsonlz4"),
        profile.join("sessionstore-backups/recovery.jsonlz4"),
    ];
    files.sort_by_key(|p| std::cmp::Reverse(p.metadata().and_then(|m| m.modified()).ok()));
    Ok(read_first(&files)?.unwrap_or_default())
}

/// Bookmark backups, newest first. Names start with the date
/// (`bookmarks-2026-11-07_…`), which is how Firefox orders them too. None
/// without a backup folder; one that cannot be listed is an error.
fn bookmark_backups(profile: &Path) -> Result<Vec<PathBuf>, ImportError> {
    let dir = profile.join("bookmarkbackups");
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(ImportError::Read { path: dir, source }),
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or_default();
            name.starts_with("bookmarks-")
                && (ext.eq_ignore_ascii_case("jsonlz4") || ext.eq_ignore_ascii_case("json"))
        })
        .collect();
    files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    Ok(files)
}

/// The first of `paths` that exists and parses; a broken one falls back to
/// the next. `None` if none exists, the first error if none parses.
fn read_first<T: DeserializeOwned>(paths: &[PathBuf]) -> Result<Option<T>, ImportError> {
    let mut error = None;
    for path in paths.iter().filter(|p| p.is_file()) {
        match read_json(path) {
            Ok(value) => return Ok(Some(value)),
            Err(e) => {
                error.get_or_insert(e);
            }
        }
    }
    error.map_or(Ok(None), Err)
}

/// Parse a JSON file, mozlz4-compressed or plain.
fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, ImportError> {
    let corrupt = |detail: String| ImportError::Corrupt {
        what: path.display().to_string(),
        detail,
    };
    let bytes = read_file(path, mozlz4::MAX_BYTES as u64)?;
    let json = mozlz4::decode(bytes).map_err(corrupt)?;
    serde_json::from_slice(&json).map_err(|e| corrupt(e.to_string()))
}
