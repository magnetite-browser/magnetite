//! Folders of the pinned tabs, and which smart folders may fetch on this
//! device.

use magnetite_core::sidebar::remove_from_tree;
use magnetite_core::smart::SmartFolder;
use magnetite_core::{Folder, Id, SidebarNode};
use magnetite_engine::ContentId;

use super::bookmarks::find;
use super::{AppState, new_id};

/// How many folders may stand inside one another, as in an import.
pub(super) const MAX_DEPTH: usize = 32;

/// Folder `id` in `nodes` and how many folders deep it stands: 1 at the top
/// level.
fn find_folder<'a>(nodes: &'a [SidebarNode], id: &str) -> Option<(&'a Folder, usize)> {
    nodes.iter().find_map(|node| match node {
        SidebarNode::Folder(f) if f.id == id => Some((f, 1)),
        SidebarNode::Folder(f) => find_folder(&f.children, id).map(|(f, depth)| (f, depth + 1)),
        SidebarNode::Tab(_) => None,
    })
}

/// The levels of folders in `node`, itself included: 0 for a tab.
fn height(node: &SidebarNode) -> usize {
    match node {
        SidebarNode::Folder(f) => 1 + f.children.iter().map(height).max().unwrap_or(0),
        SidebarNode::Tab(_) => 0,
    }
}

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

    /// A new, empty folder at the top of folder `parent`, else at the top of
    /// the active space's pinned tabs (also when `parent` is gone or already
    /// `MAX_DEPTH` deep). An empty `title` gives "Untitled".
    pub fn new_folder(&mut self, title: &str, parent: Option<&str>) {
        let title = match title.trim() {
            "" => magnetite_ui::t!("common.untitled"),
            title => title,
        };
        let folder = SidebarNode::Folder(Folder {
            id: new_id(),
            title: title.into(),
            icon: None,
            children: vec![],
            smart: None,
        });
        let takes_one = |s: &Self, p: &str| {
            (s.sidebar.spaces.iter())
                .find_map(|sp| find_folder(&sp.pinned, p))
                .is_some_and(|(parent, depth)| parent.smart.is_none() && depth < MAX_DEPTH)
        };
        let parent = parent.filter(|p| takes_one(self, p));
        if let Some(parent) = parent.and_then(|p| self.folder_mut(p)) {
            parent.children.insert(0, folder);
        } else {
            let space = self.active_space.clone();
            if let Some(sp) = self.space_mut(&space) {
                sp.pinned.insert(0, folder);
            }
        }
    }

    /// Move folder `id` of the active space to place `index` of folder
    /// `parent`, else of the top level of Pinned. `index` counts the list
    /// without the moved folder; one past the end is the end. A folder does
    /// not go into itself, into a smart folder (that one lists what its
    /// service has) or deeper than `MAX_DEPTH`. False if nothing moved.
    pub fn move_folder(&mut self, id: &str, parent: Option<&str>, index: usize) -> bool {
        let space = self.active_space.clone();
        let Some(sp) = self.space_mut(&space) else {
            return false;
        };
        let Some(moved @ SidebarNode::Folder(_)) = sp.pinned.iter().find_map(|n| find(n, id))
        else {
            return false;
        };
        let depth = match parent {
            Some(p) if find(moved, p).is_some() => return false,
            Some(p) => match find_folder(&sp.pinned, p) {
                Some((parent, depth)) if parent.smart.is_none() => depth,
                _ => return false,
            },
            None => 0,
        };
        if depth + height(moved) > MAX_DEPTH {
            return false;
        }
        let Some(node) = remove_from_tree(&mut sp.pinned, id) else {
            return false;
        };
        // Checked above: `parent` is there, outside `node`.
        let list = match parent.and_then(|p| folder_in_tree_mut(&mut sp.pinned, p)) {
            Some(parent) => &mut parent.children,
            None => &mut sp.pinned,
        };
        list.insert(index.min(list.len()), node);
        true
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

    fn folder(id: &str, children: Vec<SidebarNode>) -> SidebarNode {
        SidebarNode::Folder(magnetite_core::Folder {
            id: id.into(),
            title: id.into(),
            icon: None,
            children,
            smart: None,
        })
    }

    /// The pinned items of the first space as text: `p, f[x, g[]]`.
    fn outline(s: &AppState) -> String {
        fn nodes(list: &[SidebarNode]) -> String {
            let parts: Vec<String> = list
                .iter()
                .map(|n| match n {
                    SidebarNode::Tab(t) => t.id.clone(),
                    SidebarNode::Folder(f) => format!("{}[{}]", f.id, nodes(&f.children)),
                })
                .collect();
            parts.join(", ")
        }
        nodes(&s.sidebar.spaces[0].pinned)
    }

    #[test]
    fn a_folder_moves_into_another_and_out_again() {
        let mut s = state();
        let pinned = &mut s.sidebar.spaces[0].pinned;
        pinned.push(folder("f", vec![SidebarNode::Tab(tab("x"))]));
        pinned.push(folder("g", vec![]));
        assert!(s.move_folder("g", Some("f"), 1));
        assert_eq!(outline(&s), "p, f[x, g[]]");
        assert!(s.move_folder("g", None, 0));
        assert_eq!(outline(&s), "g[], p, f[x]");
    }

    #[test]
    fn a_folder_does_not_go_into_itself_or_below_itself() {
        let mut s = state();
        (s.sidebar.spaces[0].pinned).push(folder("f", vec![folder("g", vec![])]));
        assert!(!s.move_folder("f", Some("f"), 0));
        assert!(!s.move_folder("f", Some("g"), 0));
        assert!(!s.move_folder("f", Some("gone"), 0));
        assert!(!s.move_folder("p", Some("f"), 0), "a tab is not a folder");
        assert_eq!(outline(&s), "p, f[g[]]");
        // A smart folder lists what its service has.
        use magnetite_core::smart::{SmartFolder, SmartSource};
        let source = SmartSource::YouTrack {
            url: "https://youtrack.example.org".into(),
            query: String::new(),
        };
        s.folder_mut("f").unwrap().smart = Some(SmartFolder::new(source));
        assert!(s.move_folder("g", None, 0));
        assert!(!s.move_folder("g", Some("f"), 0));
        s.new_folder("", Some("f"));
        assert_eq!(
            s.sidebar.spaces[0].pinned.len(),
            4,
            "the new one is beside it"
        );
    }

    #[test]
    fn folders_do_not_nest_deeper_than_an_import_allows() {
        let mut s = state();
        let chain = (0..MAX_DEPTH)
            .rev()
            .fold(vec![], |inner, n| vec![folder(&format!("d{n}"), inner)]);
        let pinned = &mut s.sidebar.spaces[0].pinned;
        pinned.extend(chain);
        pinned.push(folder("g", vec![]));
        let deepest = format!("d{}", MAX_DEPTH - 1);
        assert!(!s.move_folder("g", Some(&deepest), 0));
        assert!(s.move_folder("g", Some(&format!("d{}", MAX_DEPTH - 2)), 0));
        // No room below the deepest folder: the new one goes to the top.
        s.new_folder("", Some(&deepest));
        assert!(
            matches!(&s.sidebar.spaces[0].pinned[0], SidebarNode::Folder(f) if f.title == "Untitled")
        );
    }

    #[test]
    fn a_new_folder_goes_to_the_top_of_its_parent() {
        let mut s = state();
        (s.sidebar.spaces[0].pinned).push(folder("f", vec![SidebarNode::Tab(tab("x"))]));
        s.new_folder(" Work ", Some("f"));
        let SidebarNode::Folder(f) = &s.sidebar.spaces[0].pinned[1] else {
            panic!("f is a folder");
        };
        assert!(matches!(&f.children[0], SidebarNode::Folder(new) if new.title == "Work"));
        assert_eq!(f.children.len(), 2);
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
