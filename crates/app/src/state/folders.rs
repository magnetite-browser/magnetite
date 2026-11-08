//! Folders of the pinned tabs, and which smart folders may fetch on this
//! device.

use magnetite_core::smart::SmartFolder;
use magnetite_core::{Folder, Id, SidebarNode};
use magnetite_engine::ContentId;

use super::AppState;

/// Folder `id` in a pinned tree, at any depth.
pub(crate) fn folder_in_tree_mut<'a>(
    nodes: &'a mut [SidebarNode],
    id: &str,
) -> Option<&'a mut Folder> {
    for node in nodes {
        if let SidebarNode::Folder(f) = node {
            if f.id == id {
                return Some(f);
            }
            if let Some(found) = folder_in_tree_mut(&mut f.children, id) {
                return Some(found);
            }
        }
    }
    None
}

/// The folder whose direct children include tab `tab`.
pub(super) fn parent_folder_mut<'a>(
    nodes: &'a mut [SidebarNode],
    tab: &str,
) -> Option<&'a mut Folder> {
    for node in nodes {
        if let SidebarNode::Folder(f) = node {
            if f.children
                .iter()
                .any(|c| matches!(c, SidebarNode::Tab(t) if t.id == tab))
            {
                return Some(f);
            }
            if let Some(found) = parent_folder_mut(&mut f.children, tab) {
                return Some(found);
            }
        }
    }
    None
}

pub(super) fn smart_in_tree(nodes: &[SidebarNode], out: &mut Vec<(Id, SmartFolder)>) {
    for node in nodes {
        if let SidebarNode::Folder(f) = node {
            if let Some(smart) = &f.smart {
                out.push((f.id.clone(), smart.clone()));
            }
            smart_in_tree(&f.children, out);
        }
    }
}

/// A smart folder's approval key: the folder and what it asks which account.
/// Another device changing the query or account needs a new approval here.
pub fn smart_key(id: &str, sf: &SmartFolder) -> String {
    use sha2::Digest as _;
    let what = serde_json::to_vec(&(&sf.source, &sf.account)).unwrap_or_default();
    let digest = sha2::Sha256::digest(&what);
    let short = digest[..8].iter().fold(String::new(), |mut out, b| {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
        out
    });
    format!("{id}:{short}")
}

impl AppState {
    /// Folder `id` in any space.
    pub fn folder_mut(&mut self, id: &str) -> Option<&mut Folder> {
        self.sidebar
            .spaces
            .iter_mut()
            .find_map(|s| folder_in_tree_mut(&mut s.pinned, id))
    }

    /// The folder holding tab `tab` directly, in any space.
    pub fn parent_folder_mut(&mut self, tab: &str) -> Option<&mut Folder> {
        self.sidebar
            .spaces
            .iter_mut()
            .find_map(|s| parent_folder_mut(&mut s.pinned, tab))
    }

    /// May smart folder `id` fetch on this device (see `Settings::smart_allowed`)?
    pub fn smart_is_allowed(&self, id: &str, sf: &SmartFolder) -> bool {
        self.settings
            .smart_allowed
            .as_ref()
            .is_none_or(|keys| keys.contains(&smart_key(id, sf)))
    }

    /// Let smart folder `id`, as it is configured now, fetch on this device.
    pub fn allow_smart(&mut self, id: &str) {
        let Some((_, sf)) = self.smart_folders().into_iter().find(|(f, _)| f == id) else {
            return;
        };
        let keys = self.settings.smart_allowed.get_or_insert_with(Vec::new);
        let prefix = format!("{id}:");
        keys.retain(|k| !k.starts_with(&prefix));
        keys.push(smart_key(id, &sf));
    }

    /// Every smart folder with its definition: `(folder id, smart)`.
    pub fn smart_folders(&self) -> Vec<(Id, SmartFolder)> {
        let mut out = Vec::new();
        for s in &self.sidebar.spaces {
            smart_in_tree(&s.pinned, &mut out);
        }
        out
    }

    /// Remove folder `id`. Its tabs stay where the folder was. From a smart
    /// folder the items its service listed go with it: they are the
    /// service's, not the user's tabs (what the user put into the folder
    /// stays). Returns the page windows to close.
    pub fn delete_folder(&mut self, id: &str) -> Vec<ContentId> {
        use magnetite_core::sidebar::dissolve_folder;
        use magnetite_core::smart::item_key;
        let mut listed: Vec<Id> = Vec::new();
        let mut was_smart = false;
        if let Some(folder) = self.folder_mut(id)
            && let Some(smart) = folder.smart.take()
        {
            was_smart = true;
            folder.children.retain(|node| match node {
                SidebarNode::Tab(t) if smart.items.iter().any(|i| i == item_key(t)) => {
                    listed.push(t.id.clone());
                    false
                }
                _ => true,
            });
        }
        if was_smart {
            self.smart_status.remove(id);
            if let Some(keys) = &mut self.settings.smart_allowed {
                let prefix = format!("{id}:");
                keys.retain(|k| !k.starts_with(&prefix));
            }
        }
        let _ = self
            .sidebar
            .spaces
            .iter_mut()
            .find_map(|sp| dissolve_folder(&mut sp.pinned, id));
        listed.iter().filter_map(|t| self.forget_tab(t)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::{state, tab};

    #[test]
    fn a_deleted_smart_folder_takes_only_what_its_service_listed() {
        use magnetite_core::smart::{ReviewShow, SmartFolder, SmartSource};
        let mut s = state();
        let listed = "https://github.com/acme/app/pull/1";
        let source = SmartSource::GitHub {
            host: "github.com".into(),
            show: ReviewShow::PullRequests,
            filter: String::new(),
        };
        s.sidebar.spaces[0]
            .pinned
            .push(SidebarNode::Folder(magnetite_core::Folder {
                id: "f".into(),
                title: "Reviews".into(),
                icon: None,
                children: vec![
                    SidebarNode::Tab(magnetite_core::TabItem {
                        home_url: Some(listed.into()),
                        ..tab("pr1")
                    }),
                    // Put there by the user.
                    SidebarNode::Tab(tab("notes")),
                ],
                smart: Some(SmartFolder {
                    items: vec![listed.into()],
                    ..SmartFolder::new(source)
                }),
            }));
        s.runtime.insert(
            "pr1".into(),
            crate::state::TabRuntime {
                content: Some(9),
                ..Default::default()
            },
        );
        assert_eq!(s.delete_folder("f"), [9], "the listed item's page closes");
        let ids: Vec<&str> = s.sidebar.spaces[0]
            .pinned
            .iter()
            .map(SidebarNode::id)
            .collect();
        assert_eq!(ids, ["p", "notes"]);
        assert!(s.find_tab("pr1").is_none());
    }

    #[test]
    fn deleting_a_folder_keeps_its_tabs_where_it_was() {
        let mut s = state();
        s.sidebar.spaces[0]
            .pinned
            .push(SidebarNode::Folder(magnetite_core::Folder {
                id: "f".into(),
                title: "Reading".into(),
                icon: None,
                children: vec![SidebarNode::Tab(tab("x")), SidebarNode::Tab(tab("y"))],
                smart: None,
            }));
        s.sidebar.spaces[0].pinned.push(SidebarNode::Tab(tab("z")));
        assert_eq!(s.delete_folder("f"), Vec::<ContentId>::new());
        let ids: Vec<&str> = s.sidebar.spaces[0]
            .pinned
            .iter()
            .map(SidebarNode::id)
            .collect();
        assert_eq!(ids, ["p", "x", "y", "z"]);
    }
}
