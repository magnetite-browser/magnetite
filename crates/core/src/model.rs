//! Data model shared by the importer, the store and the UI.
//!
//! IDs are opaque strings (UUIDs in practice). Timestamps are Unix seconds
//! so the model stays free of a date-time dependency.

use serde::{Deserialize, Serialize};

pub type Id = String;
pub type UnixSeconds = i64;

/// An isolated browsing context: cookies, storage, history, extensions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub id: Id,
    pub name: String,
    #[serde(default)]
    pub is_default: bool,
    /// "Private" isolation: nothing is written to disk, logins are gone when
    /// the browser quits.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ephemeral: bool,
}

/// sRGB colour, 0.0 to 1.0 per channel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

impl Color {
    /// The colour with every channel between 0 and 1. A channel that is not
    /// a number (it would be saved as `null`, and the file could not be read
    /// again) becomes mid grey.
    #[must_use]
    pub fn sanitised(self) -> Self {
        let channel = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.5
            }
        };
        Self {
            red: channel(self.red),
            green: channel(self.green),
            blue: channel(self.blue),
        }
    }
}

/// A sidebar entry pointing at a page. Used for favorites, pinned tabs and
/// Today tabs alike; the kind is given by where the item lives.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabItem {
    pub id: Id,
    /// Current URL. For pinned items this may differ from `home_url`.
    pub url: String,
    /// Page title as last seen, or the user's custom name if `custom_title`.
    pub title: String,
    #[serde(default)]
    pub custom_title: bool,
    /// Pinned items and favorites return here on "reset". `None` for Today tabs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<UnixSeconds>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<UnixSeconds>,
    #[serde(default)]
    pub unread: bool,
    /// Profile override (cookies, logins). `None` follows the space's
    /// profile; favorites use it to run one site with several accounts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<Id>,
    /// "Keep active": the tab's page is never unloaded for being out of
    /// sight, and a Today tab is not archived by itself.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep_active: bool,
}

impl TabItem {
    /// A tab with just an id, address and title; set the other fields with
    /// struct update syntax (`TabItem { home_url: …, ..TabItem::new(…) }`).
    pub fn new(id: impl Into<Id>, url: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            url: url.into(),
            title: title.into(),
            ..Self::default()
        }
    }
}

/// A folder in the pinned section. Folders nest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: Id,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub children: Vec<SidebarNode>,
    /// Set when the folder fills itself from a source (`smart` module).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smart: Option<crate::smart::SmartFolder>,
}

/// One row of the pinned tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SidebarNode {
    Tab(TabItem),
    Folder(Folder),
}

/// A named, themed set of pinned and Today tabs, with its bookmarks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Space {
    pub id: Id,
    pub title: String,
    /// Emoji or a named icon from our icon set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<Color>,
    pub profile_id: Id,
    #[serde(default)]
    pub pinned: Vec<SidebarNode>,
    #[serde(default)]
    pub today: Vec<TabItem>,
    /// Saved addresses, in folders. Not tabs: a bookmark has no page of its
    /// own and no home address, and opening one makes a new Today tab in
    /// this space. They are no part of `all_tabs`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bookmarks: Vec<SidebarNode>,
}

/// Favorites belong to a profile and are shared by all its spaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FavoriteSet {
    pub profile_id: Id,
    #[serde(default)]
    pub items: Vec<TabItem>,
}

/// Everything the sidebar shows, in one serialisable snapshot. This is also
/// the importer's output format (`format = "magnetite-sidebar"`, `version = 1`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidebarSnapshot {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub favorites: Vec<FavoriteSet>,
    /// In display order.
    #[serde(default)]
    pub spaces: Vec<Space>,
}

impl SidebarSnapshot {
    pub const FORMAT: &'static str = "magnetite-sidebar";
    pub const VERSION: u32 = 1;

    pub fn new() -> Self {
        Self {
            format: Self::FORMAT.to_string(),
            version: Self::VERSION,
            profiles: Vec::new(),
            favorites: Vec::new(),
            spaces: Vec::new(),
        }
    }

    /// Whether anything in the sidebar runs in profile `id`: a space, a
    /// favorite, or a tab that keeps a profile of its own.
    pub fn uses_profile(&self, id: &str) -> bool {
        self.spaces.iter().any(|s| s.profile_id == id)
            || self
                .favorites
                .iter()
                .any(|f| f.profile_id == id && !f.items.is_empty())
            || self.all_tabs().any(|t| t.profile_id.as_deref() == Some(id))
    }

    /// Every bookmark in the snapshot, space by space, depth first.
    pub fn all_bookmarks(&self) -> impl Iterator<Item = &TabItem> {
        self.spaces
            .iter()
            .flat_map(|s| s.bookmarks.iter().flat_map(SidebarNode::tabs))
    }

    /// Every tab item in the snapshot, in sidebar order: favorites, then per
    /// space pinned (depth first) and Today.
    pub fn all_tabs(&self) -> impl Iterator<Item = &TabItem> {
        let favs = self.favorites.iter().flat_map(|f| f.items.iter());
        let spaces = self.spaces.iter().flat_map(|s| {
            s.pinned
                .iter()
                .flat_map(SidebarNode::tabs)
                .chain(s.today.iter())
        });
        favs.chain(spaces)
    }
}

impl Default for SidebarSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

impl SidebarNode {
    /// Depth-first iterator over the tabs below (and including) this node.
    pub fn tabs(&self) -> Box<dyn Iterator<Item = &TabItem> + '_> {
        match self {
            SidebarNode::Tab(t) => Box::new(std::iter::once(t)),
            SidebarNode::Folder(f) => Box::new(f.children.iter().flat_map(SidebarNode::tabs)),
        }
    }

    pub fn id(&self) -> &str {
        match self {
            SidebarNode::Tab(t) => &t.id,
            SidebarNode::Folder(f) => &f.id,
        }
    }
}
