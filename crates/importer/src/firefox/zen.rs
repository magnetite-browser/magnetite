//! Zen's sidebar. Since Zen 1.18 it lives in `zen-sessions.jsonlz4` (and in
//! every session window): the tabs of all spaces in strip order, the folders
//! and the spaces. Essentials are tabs with `zenEssential`, a tab names its
//! space in `zenWorkspace`, and folders are tab groups: the tab names its
//! folder (`groupId`), a folder its parent (`parentId`). Zen's source:
//! `src/zen/sessionstore/ZenSessionManager.sys.mjs`, `storeDataForSessionStore`
//! in `src/zen/folders/ZenFolders.mjs` and the `TabState` patch.

use std::collections::HashMap;

use magnetite_core::{Color, SidebarNode, SidebarSnapshot, TabItem};
use serde_json::Value;

use super::session::{Theme, Window, ZenFolder};
use super::{folder, new_space, sidebar_tab};
use crate::{Ids, ImportError, MAX_DEPTH, default_profile_snapshot};

/// Each space with its pinned tree and unpinned tabs (Today), essentials as
/// favorites, and the bookmark folders at the end of the first space.
pub fn snapshot(
    sidebar: &Window,
    name: &str,
    bookmarks: Vec<SidebarNode>,
    ids: &mut Ids,
) -> Result<SidebarSnapshot, ImportError> {
    let mut snapshot = default_profile_snapshot();
    let mut space_of = HashMap::new();
    for zen in &sidebar.spaces {
        let Some(uuid) = zen.uuid.as_deref() else {
            continue;
        };
        if space_of.contains_key(uuid) {
            continue;
        }
        space_of.insert(uuid, snapshot.spaces.len());
        let title = zen
            .name
            .as_deref()
            .filter(|n| !n.is_empty())
            .unwrap_or("Space");
        let mut space = new_space(ids, title)?;
        space.icon = zen.icon.clone().filter(|i| is_emoji(i));
        space.accent = zen.theme.as_ref().and_then(accent);
        snapshot.spaces.push(space);
    }
    if snapshot.spaces.is_empty() {
        snapshot.spaces.push(new_space(ids, name)?);
    }

    let mut tree = Tree {
        folders: sidebar
            .folders
            .iter()
            .filter_map(|f| Some((f.id.as_deref()?, f)))
            .collect(),
        lists: HashMap::new(),
    };
    for tab in sidebar.tabs.iter().filter(|t| !t.zen_is_glance) {
        // Tabs of a space we do not know go to the first one.
        let space = tab
            .zen_workspace
            .as_deref()
            .and_then(|w| space_of.get(w))
            .copied()
            .unwrap_or(0);
        let in_folder = tab.group_id.as_deref().and_then(|g| tree.folder_of(g));
        if tab.zen_is_empty {
            // A folder's placeholder: places even an empty folder.
            if let Some(id) = in_folder {
                tree.place(id, space);
            }
            continue;
        }
        let pinned = tab.pinned || tab.zen_essential;
        let Some(item) = sidebar_tab(ids, tab, pinned)? else {
            continue;
        };
        if tab.zen_essential {
            snapshot.favorites[0].items.push(item);
        } else if !pinned {
            snapshot.spaces[space].today.push(item);
        } else {
            let parent = match in_folder {
                Some(id) => tree.place(id, space),
                None => Parent::Space(space),
            };
            tree.lists.entry(parent).or_default().push(Entry::Tab(item));
        }
    }
    for (i, space) in snapshot.spaces.iter_mut().enumerate() {
        space.pinned = tree.build(Parent::Space(i), ids, 0)?;
    }
    snapshot.spaces[0].bookmarks = bookmarks;
    Ok(snapshot)
}

/// The pinned trees of all spaces, built while walking the tabs in strip
/// order: a folder goes where it is first met, below its parent.
struct Tree<'a> {
    folders: HashMap<&'a str, &'a ZenFolder>,
    /// The children of each space and of each folder placed so far.
    lists: HashMap<Parent<'a>, Vec<Entry<'a>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Parent<'a> {
    Space(usize),
    Folder(&'a str),
}

enum Entry<'a> {
    Tab(TabItem),
    Folder(&'a str),
}

impl<'a> Tree<'a> {
    fn is_folder(&self, id: &str) -> bool {
        self.folders.get(id).is_some_and(|f| !f.split_view_group)
    }

    /// The folder a tab of group `group` is in: the group itself, or the
    /// folder holding its split view. `None` outside folders.
    fn folder_of(&self, group: &'a str) -> Option<&'a str> {
        let entry = self.folders.get(group).copied()?;
        if entry.split_view_group {
            entry.parent_id.as_deref().filter(|p| self.is_folder(p))
        } else {
            Some(group)
        }
    }

    /// Place folder `id`, and the parents it is nested in, into space
    /// `space` unless already placed. Returns the folder as a parent.
    fn place(&mut self, id: &'a str, space: usize) -> Parent<'a> {
        let mut chain = Vec::new();
        let mut parent = Parent::Space(space);
        let mut next = Some(id);
        while let Some(f) = next {
            if self.lists.contains_key(&Parent::Folder(f)) {
                parent = Parent::Folder(f);
                break;
            }
            if chain.contains(&f) || chain.len() > MAX_DEPTH {
                break; // a `parentId` cycle, or absurd nesting
            }
            chain.push(f);
            next = self
                .folders
                .get(f)
                .copied()
                .and_then(|x| x.parent_id.as_deref())
                .filter(|p| self.is_folder(p));
        }
        for f in chain.into_iter().rev() {
            self.lists.entry(parent).or_default().push(Entry::Folder(f));
            self.lists.insert(Parent::Folder(f), Vec::new());
            parent = Parent::Folder(f);
        }
        parent
    }

    /// The nodes below `parent`, folders filled recursively. Each list is
    /// taken once, so every folder appears once.
    fn build(
        &mut self,
        parent: Parent<'a>,
        ids: &mut Ids,
        depth: usize,
    ) -> Result<Vec<SidebarNode>, ImportError> {
        if depth > MAX_DEPTH {
            return Err(ImportError::TooDeep(MAX_DEPTH));
        }
        let mut nodes = Vec::new();
        for entry in self.lists.remove(&parent).unwrap_or_default() {
            nodes.push(match entry {
                Entry::Tab(tab) => SidebarNode::Tab(tab),
                Entry::Folder(id) => {
                    let zen = self.folders.get(id).copied();
                    let children = self.build(Parent::Folder(id), ids, depth + 1)?;
                    let mut folder = folder(ids, zen.and_then(|f| f.name.as_deref()), children)?;
                    folder.icon = zen
                        .and_then(|f| f.user_icon.clone())
                        .filter(|i| is_emoji(i));
                    SidebarNode::Folder(folder)
                }
            });
        }
        Ok(nodes)
    }
}

/// Zen icons are emoji or icon URLs (`chrome://…/name.svg`); we keep emoji.
fn is_emoji(icon: &str) -> bool {
    icon.chars().count() <= 16 && !icon.contains(':') && !icon.is_ascii()
}

/// The primary colour of a space's gradient.
#[allow(clippy::cast_possible_truncation)] // colour channels, f32 is plenty
fn accent(theme: &Theme) -> Option<Color> {
    let colors = &theme.gradient_colors;
    let color = colors.iter().find(|c| c.is_primary).or(colors.first())?;
    let [r, g, b] = color.c.as_array()?.as_slice() else {
        return None;
    };
    let channel = |v: &Value| v.as_f64().map(|v| (v / 255.0).clamp(0.0, 1.0) as f32);
    Some(Color {
        red: channel(r)?,
        green: channel(g)?,
        blue: channel(b)?,
    })
}
