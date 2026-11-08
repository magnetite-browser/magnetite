//! Operations on a `SidebarSnapshot` that the UI needs but that do not
//! depend on any engine: lookup and removal by id, moving tabs between
//! sections. Kept small on purpose; grow it with the features.

use serde::Deserialize;

use crate::model::{Folder, SidebarNode, Space, TabItem};

/// Where a tab lives. The UI names it in lowercase (`"pinned"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Section {
    Favorites,
    Pinned,
    Today,
}

/// Remove the node with `id` from a pinned tree. Returns the removed node.
pub fn remove_from_tree(nodes: &mut Vec<SidebarNode>, id: &str) -> Option<SidebarNode> {
    if let Some(pos) = nodes.iter().position(|n| n.id() == id) {
        return Some(nodes.remove(pos));
    }
    for node in nodes.iter_mut() {
        if let SidebarNode::Folder(folder) = node
            && let Some(found) = remove_from_tree(&mut folder.children, id)
        {
            return Some(found);
        }
    }
    None
}

/// Take the folder with `id` out of a pinned tree and leave its children
/// where it was. Returns the folder, now empty.
pub fn dissolve_folder(nodes: &mut Vec<SidebarNode>, id: &str) -> Option<Folder> {
    let here = |n: &SidebarNode| matches!(n, SidebarNode::Folder(f) if f.id == id);
    if let Some(pos) = nodes.iter().position(here)
        && let SidebarNode::Folder(mut folder) = nodes.remove(pos)
    {
        nodes.splice(pos..pos, std::mem::take(&mut folder.children));
        return Some(folder);
    }
    nodes.iter_mut().find_map(|n| match n {
        SidebarNode::Folder(f) => dissolve_folder(&mut f.children, id),
        SidebarNode::Tab(_) => None,
    })
}

impl Space {
    /// Find a tab anywhere in this space (Today or pinned, at any depth).
    pub fn find_tab(&self, id: &str) -> Option<&TabItem> {
        self.today
            .iter()
            .chain(self.pinned.iter().flat_map(SidebarNode::tabs))
            .find(|t| t.id == id)
    }

    /// Move a Today tab to the top of the pinned list. The pinned home URL
    /// becomes the current URL. No-op if the tab is not a Today tab.
    pub fn pin(&mut self, id: &str) -> bool {
        let Some(pos) = self.today.iter().position(|t| t.id == id) else {
            return false;
        };
        let mut tab = self.today.remove(pos);
        tab.home_url = Some(tab.url.clone());
        self.pinned.insert(0, SidebarNode::Tab(tab));
        true
    }

    /// Move a pinned tab (from any folder depth) to the top of Today.
    pub fn unpin(&mut self, id: &str) -> bool {
        match remove_from_tree(&mut self.pinned, id) {
            Some(SidebarNode::Tab(mut tab)) => {
                tab.home_url = None;
                self.today.insert(0, tab);
                true
            }
            Some(folder @ SidebarNode::Folder(_)) => {
                // Folders cannot be unpinned; put it back where it was (top).
                self.pinned.insert(0, folder);
                false
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Folder;

    fn tab(id: &str) -> TabItem {
        TabItem::new(id, format!("https://example.org/{id}"), id)
    }

    fn space() -> Space {
        Space {
            id: "s".into(),
            title: "S".into(),
            icon: None,
            accent: None,
            profile_id: "p".into(),
            pinned: vec![
                SidebarNode::Tab(tab("p1")),
                SidebarNode::Folder(Folder {
                    id: "f".into(),
                    title: "F".into(),
                    icon: None,
                    children: vec![SidebarNode::Tab(tab("p2"))],
                    smart: None,
                }),
            ],
            today: vec![tab("t1"), tab("t2")],
            bookmarks: Vec::new(),
        }
    }

    #[test]
    fn pin_moves_today_tab_to_top_of_pinned_and_sets_home_url() {
        let mut s = space();
        assert!(s.pin("t2"));
        assert_eq!(s.today.len(), 1);
        let t = s.find_tab("t2").unwrap();
        assert_eq!(t.home_url.as_deref(), Some("https://example.org/t2"));
        assert_eq!(s.pinned[0].id(), "t2");
    }

    #[test]
    fn unpin_finds_tabs_inside_folders() {
        let mut s = space();
        assert!(s.unpin("p2"));
        assert_eq!(s.today[0].id, "p2");
        assert!(s.today[0].home_url.is_none());
        assert_eq!(s.pinned.iter().flat_map(SidebarNode::tabs).count(), 1);
    }

    #[test]
    fn unpin_refuses_folders_and_unknown_ids() {
        let mut s = space();
        assert!(!s.unpin("f"));
        assert!(!s.unpin("nope"));
        assert_eq!(s.pinned.len(), 2);
    }
}
