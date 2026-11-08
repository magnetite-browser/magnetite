//! Serde mirror of Firefox's session file (`sessionstore.jsonlz4`) and of
//! Zen's sidebar file (`zen-sessions.jsonlz4`), which has the shape of one
//! session window. Only the fields we use are named; missing ones default,
//! `null` lists are empty and flags accept `true` or `"true"` (older Zen
//! wrote tab attributes as strings).

use serde::{Deserialize, Deserializer};
use serde_json::Value;

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Session {
    #[serde(deserialize_with = "list")]
    pub windows: Vec<Window>,
    /// Tab groups the user saved and closed (Firefox 137+).
    #[serde(deserialize_with = "list")]
    pub saved_groups: Vec<SavedGroup>,
}

/// A browser window, or Zen's sidebar: tabs in strip order plus, for Zen,
/// its spaces and folders.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Window {
    #[serde(deserialize_with = "list")]
    pub tabs: Vec<Tab>,
    /// Firefox tab groups; tabs point here with `groupId`.
    #[serde(deserialize_with = "list")]
    pub groups: Vec<Group>,
    #[serde(deserialize_with = "list")]
    pub spaces: Vec<ZenSpace>,
    #[serde(deserialize_with = "list")]
    pub folders: Vec<ZenFolder>,
    #[serde(deserialize_with = "flag")]
    pub is_private: bool,
    #[serde(deserialize_with = "flag")]
    pub is_popup: bool,
    #[serde(deserialize_with = "flag")]
    pub is_taskbar_tab: bool,
    #[serde(deserialize_with = "flag")]
    pub is_zen_unsynced: bool,
}

impl Window {
    /// A regular window, not a private one, a popup or a web app.
    pub fn is_normal(&self) -> bool {
        !(self.is_private || self.is_popup || self.is_taskbar_tab || self.is_zen_unsynced)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Tab {
    /// Back/forward history.
    #[serde(deserialize_with = "list")]
    pub entries: Vec<Entry>,
    /// 1-based position of the current entry in `entries`.
    pub index: Option<i64>,
    #[serde(deserialize_with = "flag")]
    pub pinned: bool,
    /// Unix milliseconds.
    pub last_accessed: Option<f64>,
    pub group_id: Option<String>,
    /// Zen: the space (workspace UUID) the tab belongs to.
    pub zen_workspace: Option<String>,
    #[serde(deserialize_with = "flag")]
    pub zen_essential: bool,
    /// Zen keeps one hidden placeholder tab in every folder.
    #[serde(deserialize_with = "flag")]
    pub zen_is_empty: bool,
    /// Zen's Glance: a page previewed over the current one.
    #[serde(deserialize_with = "flag")]
    pub zen_is_glance: bool,
    /// Zen: the name the user gave the tab.
    pub zen_static_label: Option<String>,
    /// Zen: where a pinned tab returns on reset.
    #[serde(rename = "_zenPinnedInitialState")]
    pub zen_pinned_initial_state: Option<PinnedState>,
}

impl Tab {
    /// The history entry the tab shows; without an index, the last one.
    pub fn current(&self) -> Option<&Entry> {
        let last = self.entries.len().checked_sub(1)?;
        let index = self.index.map_or(last, |i| {
            usize::try_from(i.saturating_sub(1)).map_or(0, |i| i.min(last))
        });
        self.entries.get(index)
    }

    /// Zen: the page a pinned tab was pinned with.
    pub fn pinned_entry(&self) -> Option<&Entry> {
        self.zen_pinned_initial_state.as_ref()?.entry.as_ref()
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub url: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PinnedState {
    pub entry: Option<Entry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Group {
    pub id: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SavedGroup {
    pub name: Option<String>,
    #[serde(deserialize_with = "list")]
    pub tabs: Vec<ClosedTab>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ClosedTab {
    pub state: Tab,
}

/// A Zen space ("workspace").
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ZenSpace {
    pub uuid: Option<String>,
    pub name: Option<String>,
    /// An emoji or an icon URL (`chrome://…/name.svg`).
    pub icon: Option<String>,
    pub theme: Option<Theme>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Theme {
    #[serde(deserialize_with = "list")]
    pub gradient_colors: Vec<GradientColor>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GradientColor {
    /// `[r, g, b]` in 0..=255, or a CSS colour for custom ones.
    pub c: Value,
    #[serde(deserialize_with = "flag")]
    pub is_primary: bool,
}

/// A Zen folder (a tab group) or, with `splitViewGroup`, a split view
/// inside a folder.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ZenFolder {
    pub id: Option<String>,
    pub name: Option<String>,
    /// The folder this one is nested in.
    pub parent_id: Option<String>,
    /// An icon URL, or empty.
    pub user_icon: Option<String>,
    #[serde(deserialize_with = "flag")]
    pub split_view_group: bool,
}

/// `true` or `"true"`; anything else is false.
fn flag<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Bool(b) => b,
        Value::String(s) => s == "true",
        _ => false,
    })
}

/// A list, or empty for `null`.
pub(super) fn list<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Vec<T>, D::Error> {
    Ok(Option::deserialize(d)?.unwrap_or_default())
}
