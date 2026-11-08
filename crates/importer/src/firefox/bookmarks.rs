//! Bookmark backups (`bookmarkbackups/bookmarks-<date>_….jsonlz4`): one JSON
//! tree whose root holds the toolbar, menu, "Other bookmarks" and mobile
//! roots. Format: `BookmarkJSONUtils.sys.mjs` in Firefox's
//! `toolkit/components/places`.

use magnetite_core::{SidebarNode, TabItem};
use serde::Deserialize;

use super::{folder, unix};
use crate::{Ids, ImportError, MAX_DEPTH, importable};

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Node {
    pub title: Option<String>,
    /// Set for bookmarks.
    pub uri: Option<String>,
    /// 1 bookmark, 2 folder, 3 separator.
    pub type_code: Option<i64>,
    /// Which root this is (`toolbarFolder`, …); only on roots.
    pub root: Option<String>,
    /// Unix microseconds.
    pub date_added: Option<f64>,
    #[serde(deserialize_with = "super::session::list")]
    pub children: Vec<Node>,
}

/// The roots we import, in sidebar order, with their folder names.
const ROOTS: [(&str, &str); 4] = [
    ("toolbarFolder", "Bookmarks toolbar"),
    ("bookmarksMenuFolder", "Bookmarks menu"),
    ("unfiledBookmarksFolder", "Other bookmarks"),
    ("mobileFolder", "Mobile bookmarks"),
];

const FOLDER: i64 = 2;

/// The non-empty roots of `tree`, each as a folder keeping its nesting.
pub fn folders(tree: &Node, ids: &mut Ids) -> Result<Vec<SidebarNode>, ImportError> {
    let mut folders = Vec::new();
    for (key, title) in ROOTS {
        let Some(root) = tree
            .children
            .iter()
            .find(|n| n.root.as_deref() == Some(key))
        else {
            continue;
        };
        let children = nodes(&root.children, ids, 1)?;
        if !children.is_empty() {
            folders.push(SidebarNode::Folder(folder(ids, Some(title), children)?));
        }
    }
    Ok(folders)
}

/// Bookmarks and folders of `list`; separators, queries (`place:`) and
/// other internal links are left out.
fn nodes(list: &[Node], ids: &mut Ids, depth: usize) -> Result<Vec<SidebarNode>, ImportError> {
    if depth > MAX_DEPTH {
        return Err(ImportError::TooDeep(MAX_DEPTH));
    }
    let mut out = Vec::new();
    for node in list {
        let name = node.title.as_deref().filter(|t| !t.is_empty());
        if let Some(url) = &node.uri {
            if importable(url) {
                out.push(SidebarNode::Tab(TabItem {
                    custom_title: name.is_some(),
                    created_at: node.date_added.and_then(|us| unix(us, 1e6)),
                    ..TabItem::new(ids.next()?, url, name.unwrap_or(url))
                }));
            }
        } else if node.type_code == Some(FOLDER) {
            let children = nodes(&node.children, ids, depth + 1)?;
            out.push(SidebarNode::Folder(folder(ids, name, children)?));
        }
    }
    Ok(out)
}
