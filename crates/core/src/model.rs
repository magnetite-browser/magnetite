//! Data model shared by the importer, the store and the UI.
//!
//! IDs are opaque strings (UUIDs in practice). Timestamps are Unix seconds
//! so the model stays free of a date-time dependency.

use serde::{Deserialize, Serialize};

pub type Id = String;
pub type UnixSeconds = i64;

/// An isolated browsing context: cookies, storage, history, extensions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: Id,
    pub name: String,
    #[serde(default)]
    pub is_default: bool,
    /// "Private" isolation: nothing is written to disk, logins are gone when
    /// the browser quits.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ephemeral: bool,
    /// The colour that tells the profile apart in the sidebar: its
    /// favorites' tiles are tinted with it, and so is its name beside a
    /// space. `None`: plain, as the default profile is until it is given
    /// one; every other lasting profile gets one (`color_profiles`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
}

/// The colours profiles get by themselves, the first one that no profile
/// has yet: the space colours of Settings (`SPACE_COLOURS` in the web UI)
/// without the default one, ordered so that neighbours differ.
pub const PROFILE_COLORS: [Color; 10] = [
    Color::rgb(77, 148, 255),
    Color::rgb(245, 150, 70),
    Color::rgb(95, 196, 120),
    Color::rgb(235, 110, 170),
    Color::rgb(235, 195, 70),
    Color::rgb(170, 120, 240),
    Color::rgb(45, 190, 170),
    Color::rgb(240, 95, 95),
    Color::rgb(170, 205, 80),
    Color::rgb(150, 150, 165),
];

/// sRGB colour, 0.0 to 1.0 per channel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

impl Color {
    /// The colour of `red`, `green` and `blue` from 0 to 255.
    #[must_use]
    #[allow(clippy::cast_lossless)] // `f32::from` is no `const fn`.
    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red: red as f32 / 255.0,
            green: green as f32 / 255.0,
            blue: blue as f32 / 255.0,
        }
    }

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
    /// A favorite that opens as a web panel: over the page in front, not
    /// as a tab in its place. Counts for favorites only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub panel: bool,
    /// The width the web panel was dragged to, in points; none: the usual.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel_width: Option<u32>,
    /// The web panel's page is shown as on a phone (a mobile browser's
    /// identity).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub panel_mobile: bool,
    /// A favorite that shows in these spaces only; empty: in every space.
    /// Counts for favorites only (`SidebarSnapshot::favorite_shows`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub only_in: Vec<Id>,
}

impl TabItem {
    /// No web panel any more: a plain tab, without a panel's settings.
    pub fn end_panel(&mut self) {
        self.panel = false;
        self.panel_width = None;
        self.panel_mobile = false;
    }

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
    /// A picture of the user's own, shown in place of `icon`: a small WebP
    /// file as base64 (the app keeps it within `MAX_ICON_IMAGE_BYTES`).
    /// Part of the space, so it syncs with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<Color>,
    pub profile_id: Id,
    /// The keys that go to this space, as Settings → Shortcuts saves keys
    /// (`ctrl+shift+W`). The app reads them and leaves out what cannot be
    /// a shortcut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
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

    /// Give every lasting profile but the default one a colour, if it has
    /// none: the first of `PROFILE_COLORS` that no profile has, or once all
    /// are taken the one fewest profiles have. A profile that has a colour
    /// keeps it.
    pub fn color_profiles(&mut self) {
        for at in 0..self.profiles.len() {
            let p = &self.profiles[at];
            if p.is_default || p.ephemeral || p.color.is_some() {
                continue;
            }
            let users = |c: &Color| self.profiles.iter().filter(|p| p.color == Some(*c)).count();
            self.profiles[at].color = PROFILE_COLORS.iter().min_by_key(|c| users(c)).copied();
        }
    }

    /// Whether favorite `tab` shows in space `space`: in every space, unless
    /// it names the spaces it is for. Spaces that are gone (deleted, or of
    /// another device) do not count: a favorite left with none shows
    /// everywhere rather than nowhere.
    pub fn favorite_shows(&self, tab: &TabItem, space: &str) -> bool {
        let mut named = (tab.only_in.iter())
            .filter(|id| self.spaces.iter().any(|s| s.id == **id))
            .peekable();
        named.peek().is_none() || named.any(|id| id == space)
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

    /// Call `f` on every tab item and bookmark in the snapshot.
    pub fn for_each_tab_mut(&mut self, f: &mut dyn FnMut(&mut TabItem)) {
        fn nodes(list: &mut [SidebarNode], f: &mut dyn FnMut(&mut TabItem)) {
            for n in list {
                match n {
                    SidebarNode::Tab(t) => f(t),
                    SidebarNode::Folder(folder) => nodes(&mut folder.children, f),
                }
            }
        }
        for t in self
            .favorites
            .iter_mut()
            .flat_map(|set| set.items.iter_mut())
        {
            f(t);
        }
        for space in &mut self.spaces {
            nodes(&mut space.pinned, f);
            space.today.iter_mut().for_each(&mut *f);
            nodes(&mut space.bookmarks, f);
        }
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
