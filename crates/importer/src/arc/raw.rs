//! Serde mirror of `StorableSidebar.json`, version 1. Only the fields we use
//! are named; everything else is ignored. Lists that Arc stores as
//! alternating `[id, object, id, object, …]` are parsed by `Alternating`.

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct SidebarFile {
    pub sidebar: Sidebar,
}

#[derive(Debug, Deserialize)]
pub struct Sidebar {
    #[serde(default)]
    pub containers: Vec<Container>,
}

/// One entry of `sidebar.containers`. The first is usually `{"global": {}}`,
/// the second carries the real data; we just take whichever has items.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Container {
    pub items: Alternating<Item>,
    pub spaces: Alternating<Space>,
    /// `[{"default": true}, "<container id>"]`
    #[serde(rename = "topAppsContainerIDs")]
    pub top_apps_container_ids: Vec<Value>,
}

/// Arc writes heterogeneous lists `[String, T, String, T, …]`; we keep the
/// objects and drop the interleaved ids (each object carries its own `id`).
#[derive(Debug, Default)]
pub struct Alternating<T>(pub Vec<T>);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Alternating<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = Vec::<Value>::deserialize(deserializer)?;
        let mut out = Vec::new();
        for v in values {
            if v.is_object() {
                out.push(T::deserialize(v).map_err(serde::de::Error::custom)?);
            }
        }
        Ok(Alternating(out))
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Space {
    pub id: String,
    pub title: String,
    /// `["pinned", "<id>", "unpinned", "<id>"]` (older files) or the
    /// `newContainerIDs` variant with object markers; both are handled.
    #[serde(rename = "containerIDs")]
    pub container_ids: Vec<Value>,
    #[serde(rename = "newContainerIDs")]
    pub new_container_ids: Vec<Value>,
    pub custom_info: CustomInfo,
    pub profile: Value,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomInfo {
    pub icon_type: Value,
    pub window_theme: Value,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Item {
    pub id: String,
    pub children_ids: Vec<String>,
    /// Custom title set by the user; empty when the page title is used.
    pub title: Option<String>,
    pub created_at: Option<f64>,
    pub is_unread: bool,
    pub data: ItemData,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ItemData {
    pub tab: Option<Tab>,
    pub list: Option<List>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Tab {
    #[serde(rename = "savedURL")]
    pub saved_url: String,
    pub saved_title: Option<String>,
    pub time_last_active_at: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct List {
    pub custom_info: CustomInfo,
    /// Set for Arc's Live Folders: `{"dataSource": {"github": {}}, ...}`.
    pub automatic_live_folder_data: Option<Value>,
}
