//! Sharing a folder as a link. The folder's titles and addresses travel in
//! the link itself, so nothing is uploaded and nothing is stored anywhere.
//!
//! The link is `SHARE_PAGE#v1.<data>`; `<data>` is base64url (no padding) of
//! raw deflate of the JSON written by `Shared`. `SHARE_PAGE` is a page of
//! Magnetite's own (`Shared.tsx`) that lists the folder and offers to add it.

use std::fmt;
use std::io::{Read as _, Write as _};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use magnetite_core::links::clean_link;
use magnetite_core::{Folder, SidebarNode, TabItem};
use serde::{Deserialize, Serialize};

use super::folders::MAX_DEPTH;
use super::{AppState, new_id, now};

/// The page a shared link opens.
pub const SHARE_PAGE: &str = "magnetite://ui/shared";
/// Format marker in front of the data, so the format can change later.
const VERSION: &str = "v1.";
/// A link carries at most this many tabs, made or read.
const MAX_TABS: usize = 500;
/// Longest title and address taken from a link, in characters.
const MAX_TITLE: usize = 200;
const MAX_URL: usize = 2000;
/// A link unpacks to at most this many bytes of JSON.
const MAX_JSON: usize = 512 * 1024;

/// A folder as a link carries it: title, icon, and what it holds.
#[derive(Debug, Serialize, Deserialize)]
struct Shared {
    #[serde(rename = "t")]
    title: String,
    #[serde(rename = "i", default, skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(rename = "n", default)]
    nodes: Vec<Node>,
}

/// A tab has an address (`u`), a folder has not.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Node {
    Tab {
        #[serde(rename = "t")]
        title: String,
        #[serde(rename = "u")]
        url: String,
    },
    Folder(Shared),
}

/// Why a folder was not shared or a link not added; the text is shown as is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareError {
    /// No tab with a web address in the folder.
    Empty,
    TooMany,
    /// Not a shared link, or one this version cannot read.
    Unreadable,
    Private,
}

impl fmt::Display for ShareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "This folder has no tabs to share",
            Self::TooMany => "Too many tabs for one link",
            Self::Unreadable => "This link could not be read",
            Self::Private => "The private space keeps no folders",
        })
    }
}

fn is_web(url: &str) -> bool {
    url.len() <= MAX_URL
        && url::Url::parse(url).is_ok_and(|u| matches!(u.scheme(), "http" | "https"))
}

/// `folder` as a link carries it. Tabs are shared with the address they
/// return to, without tracking parameters; pages that are not on the web
/// (settings, files) are left out. `tabs` counts what went in.
fn pack(folder: &Folder, tabs: &mut usize) -> Shared {
    let nodes = (folder.children.iter())
        .filter_map(|node| match node {
            SidebarNode::Tab(t) => {
                let url = clean_link(t.home_url.as_deref().unwrap_or(&t.url)).0;
                is_web(&url).then(|| {
                    *tabs += 1;
                    Node::Tab {
                        title: t.title.clone(),
                        url,
                    }
                })
            }
            SidebarNode::Folder(f) => Some(Node::Folder(pack(f, tabs))),
        })
        .collect();
    Shared {
        title: folder.title.clone(),
        icon: folder.icon.clone(),
        nodes,
    }
}

/// The link that shares `folder`.
pub fn share_link(folder: &Folder) -> Result<String, ShareError> {
    let mut tabs = 0;
    let shared = pack(folder, &mut tabs);
    match tabs {
        0 => return Err(ShareError::Empty),
        n if n > MAX_TABS => return Err(ShareError::TooMany),
        _ => {}
    }
    let json = serde_json::to_vec(&shared).map_err(|_| ShareError::Unreadable)?;
    let mut packed = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
    let data = (packed.write_all(&json))
        .and_then(|()| packed.finish())
        .map_err(|_| ShareError::Unreadable)?;
    Ok(format!(
        "{SHARE_PAGE}#{VERSION}{}",
        URL_SAFE_NO_PAD.encode(data)
    ))
}

/// The data part of a shared link.
fn data_of(url: &str) -> Option<&str> {
    url.strip_prefix(SHARE_PAGE)?.strip_prefix('#')
}

fn short(text: &str, fallback: &str) -> String {
    match text.trim() {
        "" => fallback.into(),
        text => text.chars().take(MAX_TITLE).collect(),
    }
}

/// A folder of the sidebar from what a link carried, with ids of its own.
/// A link is foreign input: anything but web addresses, too many tabs or
/// folders nested deeper than the sidebar allows make it unreadable.
fn unpack(shared: Shared, depth: usize, tabs: &mut usize) -> Result<Folder, ShareError> {
    if depth > MAX_DEPTH {
        return Err(ShareError::Unreadable);
    }
    let children = (shared.nodes.into_iter())
        .map(|node| match node {
            Node::Tab { title, url } => {
                *tabs += 1;
                if *tabs > MAX_TABS || !is_web(&url) {
                    return Err(ShareError::Unreadable);
                }
                let host = url::Url::parse(&url)
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_string))
                    .unwrap_or_default();
                Ok(SidebarNode::Tab(TabItem {
                    home_url: Some(url.clone()),
                    created_at: Some(now()),
                    ..TabItem::new(new_id(), url, short(&title, &host))
                }))
            }
            Node::Folder(f) => unpack(f, depth + 1, tabs).map(SidebarNode::Folder),
        })
        .collect::<Result<_, _>>()?;
    Ok(Folder {
        id: new_id(),
        title: short(&shared.title, "Shared folder"),
        // Only names of the built-in glyphs draw anything; those are short.
        icon: shared.icon.filter(|i| i.len() <= 64),
        children,
        smart: None,
    })
}

/// The folder a shared link carries.
pub fn read_link(url: &str) -> Result<Folder, ShareError> {
    let unreadable = ShareError::Unreadable;
    let data = data_of(url)
        .and_then(|d| d.strip_prefix(VERSION))
        .ok_or(unreadable)?;
    let packed = URL_SAFE_NO_PAD.decode(data).map_err(|_| unreadable)?;
    let mut json = Vec::new();
    flate2::read::DeflateDecoder::new(packed.as_slice())
        .take(MAX_JSON as u64 + 1)
        .read_to_end(&mut json)
        .map_err(|_| unreadable)?;
    if json.len() > MAX_JSON {
        return Err(unreadable);
    }
    let shared: Shared = serde_json::from_slice(&json).map_err(|_| unreadable)?;
    let mut tabs = 0;
    let folder = unpack(shared, 1, &mut tabs)?;
    if tabs == 0 {
        return Err(unreadable);
    }
    Ok(folder)
}

impl AppState {
    /// The link that shares folder `id`.
    pub fn share_folder(&mut self, id: &str) -> Result<String, ShareError> {
        let folder = self.folder_mut(id).ok_or(ShareError::Empty)?;
        share_link(folder)
    }

    /// Put the folder of shared link `url` at the top of the active space's
    /// pinned tabs. Gives the folder's title.
    pub fn add_shared(&mut self, url: &str) -> Result<String, ShareError> {
        if self.is_private_space(&self.active_space) {
            return Err(ShareError::Private);
        }
        let folder = read_link(url)?;
        let title = folder.title.clone();
        let space = self.active_space.clone();
        let sp = self.space_mut(&space).ok_or(ShareError::Unreadable)?;
        sp.pinned.insert(0, SidebarNode::Folder(folder));
        Ok(title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::{state, tab};

    fn folder(title: &str, children: Vec<SidebarNode>) -> Folder {
        Folder {
            id: title.into(),
            title: title.into(),
            icon: None,
            children,
            smart: None,
        }
    }

    fn link_of(json: &str) -> String {
        let mut packed =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
        packed.write_all(json.as_bytes()).unwrap();
        let data = URL_SAFE_NO_PAD.encode(packed.finish().unwrap());
        format!("{SHARE_PAGE}#{VERSION}{data}")
    }

    #[test]
    fn a_shared_folder_comes_back_with_its_tabs_and_folders() {
        let mut s = state();
        let inner = folder("Inner", vec![SidebarNode::Tab(tab("deep"))]);
        let shared = Folder {
            icon: Some("book".into()),
            ..folder(
                "Reading",
                vec![
                    SidebarNode::Tab(TabItem {
                        // Shared with where it returns to, trackers removed.
                        home_url: Some("https://example.org/home?utm_source=x&id=1".into()),
                        ..tab("moved")
                    }),
                    SidebarNode::Tab(TabItem::new("set", "magnetite://ui/settings", "Settings")),
                    SidebarNode::Folder(inner),
                ],
            )
        };
        let link = share_link(&shared).unwrap();
        assert_eq!(s.add_shared(&link).as_deref(), Ok("Reading"));
        let SidebarNode::Folder(got) = &s.sidebar.spaces[0].pinned[0] else {
            panic!("no folder at the top of Pinned");
        };
        assert_eq!(got.icon.as_deref(), Some("book"));
        assert_ne!(got.id, "Reading", "the folder gets an id of its own");
        let [SidebarNode::Tab(t), SidebarNode::Folder(f)] = got.children.as_slice() else {
            panic!("expected a tab and a folder, got {:?}", got.children);
        };
        assert_eq!(t.url, "https://example.org/home?id=1");
        assert_eq!(t.home_url.as_deref(), Some(t.url.as_str()));
        assert_eq!(f.title, "Inner");
        assert!(matches!(f.children.as_slice(), [SidebarNode::Tab(t)] if t.title == "deep"));
    }

    #[test]
    fn a_folder_without_web_pages_is_not_shared() {
        let only_settings = folder(
            "Empty",
            vec![SidebarNode::Tab(TabItem::new(
                "s",
                "magnetite://ui/settings",
                "S",
            ))],
        );
        assert_eq!(share_link(&only_settings), Err(ShareError::Empty));
    }

    #[test]
    fn a_link_with_anything_but_web_addresses_is_refused() {
        let unreadable = Err(ShareError::Unreadable);
        for json in [
            r#"{"t":"X","n":[{"t":"a","u":"javascript:alert(1)"}]}"#,
            r#"{"t":"X","n":[{"t":"a","u":"file:///etc/passwd"}]}"#,
            r#"{"t":"X","n":[{"t":"a","u":"magnetite://ui/settings"}]}"#,
            r#"{"t":"X","n":[]}"#,
            "not json",
        ] {
            assert_eq!(
                read_link(&link_of(json)).map(|f| f.title),
                unreadable,
                "{json}"
            );
        }
        for link in [
            format!("{SHARE_PAGE}#v9.AAAA"),
            format!("{SHARE_PAGE}#v1.***"),
            "https://example.org/shared#v1.AAAA".to_string(),
            "magnetite://ui/settings#v1.AAAA".to_string(),
        ] {
            assert_eq!(read_link(&link).map(|f| f.title), unreadable, "{link}");
        }
        let ok = link_of(r#"{"t":"","n":[{"t":" ","u":"https://example.org/a"}]}"#);
        let got = read_link(&ok).unwrap();
        assert_eq!(got.title, "Shared folder");
        assert!(
            matches!(got.children.as_slice(), [SidebarNode::Tab(t)] if t.title == "example.org")
        );
    }

    #[test]
    fn a_link_that_unpacks_too_large_or_too_deep_is_refused() {
        let tabs = r#"{"t":"a","u":"https://example.org/"},"#.repeat(MAX_TABS + 1);
        let many = format!(r#"{{"t":"X","n":[{}]}}"#, tabs.trim_end_matches(','));
        assert_eq!(
            read_link(&link_of(&many)).map(|f| f.title),
            Err(ShareError::Unreadable)
        );

        let deep = (0..MAX_DEPTH).fold(
            r#"{"t":"a","u":"https://example.org/"}"#.to_string(),
            |inner, _| format!(r#"{{"t":"F","n":[{inner}]}}"#),
        );
        assert!(read_link(&link_of(&deep)).is_ok());
        let deeper = format!(r#"{{"t":"F","n":[{deep}]}}"#);
        assert_eq!(
            read_link(&link_of(&deeper)).map(|f| f.title),
            Err(ShareError::Unreadable)
        );

        let huge = format!(r#"{{"t":"{}","n":[]}}"#, "a".repeat(MAX_JSON));
        assert_eq!(
            read_link(&link_of(&huge)).map(|f| f.title),
            Err(ShareError::Unreadable)
        );
    }
}
