//! Bookmarks: per space a tree of saved addresses in folders
//! (`Space::bookmarks`). They are not tabs: a bookmark has no page of its
//! own, and opening one makes a new Today tab in the space it belongs to.

use std::collections::HashSet;

use magnetite_core::sidebar::remove_from_tree;
use magnetite_core::{Folder, Id, SidebarNode, Space, TabItem};

use super::folders::folder_in_tree_mut;
use super::{AppState, new_id, now};
use crate::url_policy::{Source, allowed};

/// The tab or folder `id` in `nodes`, at any depth.
fn node_mut<'a>(nodes: &'a mut [SidebarNode], id: &str) -> Option<&'a mut SidebarNode> {
    for node in nodes {
        if node.id() == id {
            return Some(node);
        }
        if let SidebarNode::Folder(f) = node
            && let Some(found) = node_mut(&mut f.children, id)
        {
            return Some(found);
        }
    }
    None
}

impl AppState {
    /// Bookmark `id` and the space it belongs to.
    pub fn bookmark(&self, id: &str) -> Option<(&Space, &TabItem)> {
        self.sidebar.spaces.iter().find_map(|space| {
            let mut all = space.bookmarks.iter().flat_map(SidebarNode::tabs);
            all.find(|b| b.id == id).map(|b| (space, b))
        })
    }

    /// Save the page of tab `tab` as a bookmark of the tab's space, at the
    /// top of its list (a favorite belongs to no space: the active one
    /// takes it). `None` if there is no such tab, the space has that
    /// address already, or it is none Magnetite opens from a saved list.
    pub fn add_bookmark(&mut self, tab: &str) -> Option<Id> {
        let (space, item) = self.find_tab(tab)?;
        let space = space.map_or_else(|| self.active_space.clone(), |s| s.id.clone());
        let (url, title) = (item.url.clone(), item.title.clone());
        if !allowed(&url, Source::Stored) {
            return None;
        }
        let space = self.space_mut(&space)?;
        let saved = |b: &TabItem| b.url == url;
        if space
            .bookmarks
            .iter()
            .flat_map(SidebarNode::tabs)
            .any(saved)
        {
            return None;
        }
        let id = new_id();
        let bookmark = TabItem {
            created_at: Some(now()),
            ..TabItem::new(id.clone(), url, title)
        };
        space.bookmarks.insert(0, SidebarNode::Tab(bookmark));
        Some(id)
    }

    /// Add the bookmarks and folders read from a bookmarks file to the top
    /// of the bookmarks of space `space`, with fresh ids. Left out: an
    /// address the space has already, one Magnetite does not open from a
    /// saved list, and a folder with nothing left in it. Returns how many
    /// bookmarks came in.
    pub fn add_bookmarks(&mut self, space: &str, nodes: Vec<SidebarNode>) -> usize {
        /// `nodes` without what `seen` has or `allowed` refuses.
        fn fresh(nodes: Vec<SidebarNode>, seen: &mut HashSet<String>) -> Vec<SidebarNode> {
            let keep = |node| match node {
                SidebarNode::Tab(bookmark) => {
                    let new =
                        allowed(&bookmark.url, Source::Stored) && seen.insert(bookmark.url.clone());
                    new.then(|| {
                        SidebarNode::Tab(TabItem {
                            created_at: bookmark.created_at.or_else(|| Some(now())),
                            custom_title: bookmark.custom_title,
                            ..TabItem::new(new_id(), bookmark.url, bookmark.title)
                        })
                    })
                }
                SidebarNode::Folder(folder) => {
                    let children = fresh(folder.children, seen);
                    (!children.is_empty()).then(|| {
                        SidebarNode::Folder(Folder {
                            id: new_id(),
                            title: folder.title,
                            icon: None,
                            children,
                            smart: None,
                        })
                    })
                }
            };
            nodes.into_iter().filter_map(keep).collect()
        }
        let Some(space) = self.space_mut(space) else {
            return 0;
        };
        let mut seen: HashSet<String> = (space.bookmarks.iter())
            .flat_map(SidebarNode::tabs)
            .map(|b| b.url.clone())
            .collect();
        let added = fresh(nodes, &mut seen);
        let count = added.iter().flat_map(SidebarNode::tabs).count();
        space.bookmarks.splice(0..0, added);
        count
    }

    /// Remove bookmark or bookmark folder `id`, a folder with all it holds.
    pub fn remove_bookmark(&mut self, id: &str) -> bool {
        (self.sidebar.spaces.iter_mut())
            .any(|space| remove_from_tree(&mut space.bookmarks, id).is_some())
    }

    /// Give bookmark or bookmark folder `id` the name `title`; an empty one
    /// is refused.
    pub fn rename_bookmark(&mut self, id: &str, title: &str) -> bool {
        let title = title.trim();
        if title.is_empty() {
            return false;
        }
        let node =
            (self.sidebar.spaces.iter_mut()).find_map(|space| node_mut(&mut space.bookmarks, id));
        match node {
            Some(SidebarNode::Tab(bookmark)) => {
                bookmark.title = title.to_string();
                bookmark.custom_title = true;
            }
            Some(SidebarNode::Folder(folder)) => folder.title = title.to_string(),
            None => return false,
        }
        true
    }

    /// A new, empty bookmark folder in space `space`: at the top of folder
    /// `parent`, or of the space's list.
    pub fn new_bookmark_folder(
        &mut self,
        space: &str,
        parent: Option<&str>,
        title: &str,
    ) -> Option<Id> {
        let space = self.space_mut(space)?;
        let list = match parent {
            Some(parent) => &mut folder_in_tree_mut(&mut space.bookmarks, parent)?.children,
            None => &mut space.bookmarks,
        };
        let id = new_id();
        let title = match title.trim() {
            "" => "New folder",
            given => given,
        };
        list.insert(
            0,
            SidebarNode::Folder(Folder {
                id: id.clone(),
                title: title.to_string(),
                icon: None,
                children: Vec::new(),
                smart: None,
            }),
        );
        Some(id)
    }

    /// Move bookmark or bookmark folder `id` to place `index` of folder
    /// `folder` (or of the top level) in the bookmarks of space `space`.
    /// `index` counts the list without the moved item; one past the end is
    /// the end. A folder does not go into itself. False if nothing moved.
    pub fn move_bookmark(
        &mut self,
        id: &str,
        space: &str,
        folder: Option<&str>,
        index: usize,
    ) -> bool {
        let Some(target) = self.space(space) else {
            return false;
        };
        let moved = (self.sidebar.spaces.iter())
            .flat_map(|sp| sp.bookmarks.iter())
            .find_map(|n| find(n, id));
        let Some(moved) = moved else {
            return false;
        };
        if let Some(folder) = folder {
            let is_folder =
                |n: &SidebarNode| matches!(find(n, folder), Some(SidebarNode::Folder(_)));
            if !target.bookmarks.iter().any(is_folder) || find(moved, folder).is_some() {
                return false;
            }
        }
        let Some(node) =
            (self.sidebar.spaces.iter_mut()).find_map(|sp| remove_from_tree(&mut sp.bookmarks, id))
        else {
            return false;
        };
        // Checked above: the space and the folder are there, outside `node`.
        let Some(target) = self.space_mut(space) else {
            return false;
        };
        let list = match folder.and_then(|f| folder_in_tree_mut(&mut target.bookmarks, f)) {
            Some(folder) => &mut folder.children,
            None => &mut target.bookmarks,
        };
        list.insert(index.min(list.len()), node);
        true
    }
}

/// The tab or folder `id` in `node` or below it.
fn find<'a>(node: &'a SidebarNode, id: &str) -> Option<&'a SidebarNode> {
    if node.id() == id {
        return Some(node);
    }
    match node {
        SidebarNode::Folder(f) => f.children.iter().find_map(|n| find(n, id)),
        SidebarNode::Tab(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::state;

    /// The bookmarks of space `at` as text: `a, Folder[b, c]`.
    fn outline(s: &AppState, at: usize) -> String {
        fn nodes(list: &[SidebarNode]) -> String {
            let parts: Vec<String> = list
                .iter()
                .map(|n| match n {
                    SidebarNode::Tab(t) => t.title.clone(),
                    SidebarNode::Folder(f) => format!("{}[{}]", f.title, nodes(&f.children)),
                })
                .collect();
            parts.join(", ")
        }
        nodes(&s.sidebar.spaces[at].bookmarks)
    }

    #[test]
    fn a_tab_is_saved_once_to_the_bookmarks_of_its_space() {
        let mut s = state();
        let id = s.add_bookmark("a").expect("saved");
        assert_eq!(outline(&s, 0), "a");
        let (space, saved) = s.bookmark(&id).expect("found");
        assert_eq!(space.id, s.sidebar.spaces[0].id);
        assert_eq!(saved.url, "https://example.org/a");
        assert!(saved.home_url.is_none() && saved.created_at.is_some());
        // The tab itself stays a tab, and the bookmark is none.
        assert!(s.find_tab("a").is_some() && s.find_tab(&id).is_none());
        assert_eq!(s.add_bookmark("a"), None, "that address is saved already");
        assert_eq!(s.add_bookmark("nothing"), None);
        // An address Magnetite would not open from a saved list is not saved.
        s.find_tab_mut("b").unwrap().url = "magnetite://ui/settings".into();
        assert_eq!(s.add_bookmark("b"), None);
        assert_eq!(outline(&s, 0), "a");
    }

    #[test]
    fn a_bookmarks_file_adds_what_the_space_has_not_got() {
        let mut s = state();
        let space = s.sidebar.spaces[0].id.clone();
        s.add_bookmark("a").unwrap();
        let link = |url: &str, title: &str| SidebarNode::Tab(TabItem::new("x", url, title));
        let folder = |title: &str, children| {
            SidebarNode::Folder(Folder {
                id: "x".into(),
                title: title.into(),
                icon: None,
                children,
                smart: None,
            })
        };
        let file = vec![
            folder(
                "Dev",
                vec![
                    link("https://example.org/docs", "Docs"),
                    link("https://example.org/a", "Have it"),
                    folder(
                        "Only known",
                        vec![link("https://example.org/docs", "Twice")],
                    ),
                ],
            ),
            link("magnetite://ui/settings", "Not from a file"),
            link("https://example.org/top", "Top"),
        ];
        assert_eq!(s.add_bookmarks(&space, file.clone()), 2);
        assert_eq!(outline(&s, 0), "Dev[Docs], Top, a");
        // Every item has an id of its own, and a second import adds nothing.
        let ids: HashSet<_> = (s.sidebar.spaces[0].bookmarks.iter())
            .flat_map(SidebarNode::tabs)
            .map(|b| b.id.clone())
            .collect();
        assert_eq!(ids.len(), 3);
        assert_eq!(s.add_bookmarks(&space, file), 0);
        assert_eq!(
            s.add_bookmarks("no such space", vec![link("https://x.org/", "x")]),
            0
        );
        assert_eq!(outline(&s, 0), "Dev[Docs], Top, a");
    }

    #[test]
    fn bookmarks_are_named_filed_moved_and_removed() {
        let mut s = state();
        let space = s.sidebar.spaces[0].id.clone();
        let a = s.add_bookmark("a").unwrap();
        let c = s.add_bookmark("c").unwrap();
        let dev = s.new_bookmark_folder(&space, None, " Dev ").unwrap();
        let docs = s.new_bookmark_folder(&space, Some(&dev), "").unwrap();
        assert_eq!(outline(&s, 0), "Dev[New folder[]], c, a");

        assert!(s.rename_bookmark(&docs, "Docs") && s.rename_bookmark(&a, "Alpha"));
        assert!(!s.rename_bookmark(&a, "  ") && !s.rename_bookmark("nothing", "x"));
        assert!(s.bookmark(&a).unwrap().1.custom_title);

        // Into a folder, to a place in it, and back to the top level.
        assert!(s.move_bookmark(&a, &space, Some(&docs), 0));
        assert!(s.move_bookmark(&c, &space, Some(&docs), 9));
        assert_eq!(outline(&s, 0), "Dev[Docs[Alpha, c]]");
        assert!(s.move_bookmark(&c, &space, None, 0));
        assert_eq!(outline(&s, 0), "c, Dev[Docs[Alpha]]");
        // A folder goes neither into itself nor into what it holds, and
        // nothing goes into a bookmark or to a place that is not there.
        assert!(!s.move_bookmark(&dev, &space, Some(&dev), 0));
        assert!(!s.move_bookmark(&dev, &space, Some(&docs), 0));
        assert!(!s.move_bookmark(&c, &space, Some(&a), 0));
        assert!(!s.move_bookmark(&c, "no such space", None, 0));
        assert_eq!(outline(&s, 0), "c, Dev[Docs[Alpha]]");

        // A folder moves with what it holds, also to another space.
        s.sidebar.spaces.push(Space {
            id: "two".into(),
            title: "Two".into(),
            pinned: vec![],
            today: vec![],
            bookmarks: vec![],
            ..s.sidebar.spaces[0].clone()
        });
        assert!(s.move_bookmark(&dev, "two", None, 0));
        assert_eq!(
            (outline(&s, 0), outline(&s, 1)),
            ("c".into(), "Dev[Docs[Alpha]]".into())
        );
        assert_eq!(s.bookmark(&a).unwrap().0.id, "two");

        // Removing a folder removes what it holds.
        assert!(s.remove_bookmark(&dev) && !s.remove_bookmark(&dev));
        assert!(s.bookmark(&a).is_none());
        assert!(s.remove_bookmark(&c));
        assert_eq!(
            (outline(&s, 0), outline(&s, 1)),
            (String::new(), String::new())
        );
    }
}
