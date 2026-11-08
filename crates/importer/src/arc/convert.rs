//! Map the raw Arc structures onto the magnetite model.

use std::collections::{HashMap, HashSet};

use magnetite_core::{Color, Folder, Profile, SidebarNode, SidebarSnapshot, Space, TabItem};
use serde_json::Value;

use super::apple_to_unix;
use super::raw::{self, Item};
use crate::{DEFAULT_PROFILE, ImportError, MAX_DEPTH, MAX_ITEMS, default_profile_snapshot};

pub fn to_snapshot(file: &raw::SidebarFile) -> Result<SidebarSnapshot, ImportError> {
    let container = file
        .sidebar
        .containers
        .iter()
        .find(|c| !c.items.0.is_empty())
        .ok_or(ImportError::NoContainer)?;

    let mut walk = Walker {
        items: container
            .items
            .0
            .iter()
            .map(|i| (i.id.as_str(), i))
            .collect(),
        seen: HashSet::new(),
        count: 0,
    };

    let mut snapshot = default_profile_snapshot();

    // Favorites ("top apps"): the string entry of topAppsContainerIDs is the
    // container item whose children are the favorites.
    for id in string_ids(&container.top_apps_container_ids) {
        snapshot.favorites[0]
            .items
            .extend(walk.tabs_under(&id, true)?);
    }

    let mut profiles = ProfileIds::default();
    let mut space_ids = HashSet::new();
    for space in &container.spaces.0 {
        if !space_ids.insert(space.id.as_str()) {
            continue; // listed twice
        }
        walk.count_item()?;
        let profile_id = profile_for(space, &mut profiles, &mut snapshot);
        let (pinned_id, unpinned_id) = space_containers(space);
        let pinned = match pinned_id {
            Some(id) => walk.nodes_under(&id, true)?,
            None => Vec::new(),
        };
        let today = match unpinned_id {
            Some(id) => walk.tabs_under(&id, false)?,
            None => Vec::new(),
        };

        snapshot.spaces.push(Space {
            id: space.id.clone(),
            title: space.title.clone(),
            icon: icon_name(&space.custom_info.icon_type),
            accent: accent_color(&space.custom_info.window_theme),
            profile_id,
            pinned,
            today,
            bookmarks: Vec::new(),
        });
    }

    Ok(snapshot)
}

/// Arc profile ids mapped to ids that are safe as directory names: an id of
/// `[A-Za-z0-9_-]` is kept (lowercased, as folders may ignore case); any
/// other, or one that would meet an id already given out, becomes
/// `arc-profile-N`. Each Arc id maps to one id and no two share one.
struct ProfileIds {
    by_arc: HashMap<String, String>,
    taken: HashSet<String>,
    /// Every `arc-profile-N` below this `N` is taken.
    next: usize,
}

impl Default for ProfileIds {
    fn default() -> Self {
        Self {
            by_arc: HashMap::new(),
            taken: HashSet::from([DEFAULT_PROFILE.to_string()]),
            next: 1,
        }
    }
}

impl ProfileIds {
    /// Our id for Arc profile `arc`, and whether it was given out just now.
    fn map(&mut self, arc: &str) -> (String, bool) {
        if let Some(id) = self.by_arc.get(arc) {
            return (id.clone(), false);
        }
        let safe = (1..=64).contains(&arc.len())
            && arc
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        let id = Some(arc.to_ascii_lowercase())
            .filter(|id| safe && !self.taken.contains(id))
            .unwrap_or_else(|| {
                // The lowest free number. The search goes on where the last
                // one ended, so many profiles do not make it start over.
                loop {
                    let id = format!("arc-profile-{}", self.next);
                    if !self.taken.contains(&id) {
                        break id;
                    }
                    self.next += 1;
                }
            });
        self.taken.insert(id.clone());
        self.by_arc.insert(arc.to_string(), id.clone());
        (id, true)
    }
}

/// `profile` is `{"default": true}` or `{"custom": {"_0": <profile>}}`,
/// where the profile is its id or an object naming it (`id`, or the
/// profile's folder as `directoryBasename`). Custom profiles are registered
/// once, under a safe id (see `ProfileIds`).
fn profile_for(space: &raw::Space, ids: &mut ProfileIds, snapshot: &mut SidebarSnapshot) -> String {
    let custom = space
        .profile
        .get("custom")
        .and_then(|c| c.get("_0").or(Some(c)))
        .and_then(|c| {
            c.as_str().or_else(|| {
                ["id", "directoryBasename"]
                    .iter()
                    .find_map(|key| c.get(key).and_then(Value::as_str))
            })
        });
    let Some(arc_id) = custom else {
        return DEFAULT_PROFILE.into();
    };
    let (id, new) = ids.map(arc_id);
    if new {
        let n = snapshot.profiles.len();
        snapshot.profiles.push(Profile {
            id: id.clone(),
            name: format!("Profile {n}"),
            is_default: false,
            ephemeral: false,
        });
    }
    id
}

/// `containerIDs` is `["pinned", "<id>", "unpinned", "<id>"]`. Newer files
/// also carry `newContainerIDs` with object markers like `{"pinned": {}}`
/// followed by the id. Both are scanned; the marker decides the role.
fn space_containers(space: &raw::Space) -> (Option<String>, Option<String>) {
    let mut pinned = None;
    let mut unpinned = None;
    for list in [&space.container_ids, &space.new_container_ids] {
        let mut role: Option<&str> = None;
        for v in list {
            match v {
                Value::String(s) if s == "pinned" || s == "unpinned" => role = Some(s.as_str()),
                Value::Object(o) if o.contains_key("pinned") => role = Some("pinned"),
                Value::Object(o) if o.contains_key("unpinned") => role = Some("unpinned"),
                Value::String(id) => {
                    match role.take() {
                        Some("pinned") => pinned.get_or_insert(id.clone()),
                        Some("unpinned") => unpinned.get_or_insert(id.clone()),
                        _ => continue,
                    };
                }
                _ => {}
            }
        }
    }
    (pinned, unpinned)
}

fn string_ids(values: &[Value]) -> Vec<String> {
    values
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// Follows `childrenIds` from container items. Every Arc item becomes at
/// most one node, so cycles and items listed twice are skipped; nesting and
/// the number of items are bounded (`MAX_DEPTH`, `MAX_ITEMS`).
struct Walker<'a> {
    items: HashMap<&'a str, &'a Item>,
    /// Items already turned into a node (or being walked).
    seen: HashSet<&'a str>,
    /// Tabs, folders and spaces so far.
    count: usize,
}

impl<'a> Walker<'a> {
    /// Count one more tab, folder or space; more than `MAX_ITEMS` is refused.
    fn count_item(&mut self) -> Result<(), ImportError> {
        self.count += 1;
        if self.count > MAX_ITEMS {
            return Err(ImportError::TooManyItems(MAX_ITEMS));
        }
        Ok(())
    }

    /// The children of container item `id` as a pinned tree (tabs and
    /// folders, recursively).
    fn nodes_under(&mut self, id: &str, pinned: bool) -> Result<Vec<SidebarNode>, ImportError> {
        match self.items.get(id).copied() {
            Some(container) if self.seen.insert(container.id.as_str()) => {
                self.children(container, pinned, 0)
            }
            _ => Ok(Vec::new()),
        }
    }

    /// Flattened tabs under container item `id` (folders are descended into).
    fn tabs_under(&mut self, id: &str, pinned: bool) -> Result<Vec<TabItem>, ImportError> {
        Ok(self
            .nodes_under(id, pinned)?
            .iter()
            .flat_map(|n| n.tabs().cloned().collect::<Vec<_>>())
            .collect())
    }

    fn children(
        &mut self,
        parent: &'a Item,
        pinned: bool,
        depth: usize,
    ) -> Result<Vec<SidebarNode>, ImportError> {
        if depth > MAX_DEPTH {
            return Err(ImportError::TooDeep(MAX_DEPTH));
        }
        let mut nodes = Vec::new();
        for id in &parent.children_ids {
            let Some(child) = self.items.get(id.as_str()).copied() else {
                continue;
            };
            if !self.seen.insert(child.id.as_str()) {
                continue; // a cycle, or listed twice
            }
            self.count_item()?;
            if let Some(tab) = &child.data.tab {
                nodes.push(SidebarNode::Tab(tab_item(child, tab, pinned)));
            } else if let Some(list) = &child.data.list {
                let children = self.children(child, pinned, depth + 1)?;
                // A live folder's tabs are its source items: the first refresh
                // drops the ones that are gone.
                let smart = list
                    .automatic_live_folder_data
                    .as_ref()
                    .and_then(live_folder)
                    .map(|mut sf| {
                        sf.items = children
                            .iter()
                            .flat_map(SidebarNode::tabs)
                            .map(|t| magnetite_core::smart::item_key(t).to_string())
                            .collect();
                        sf
                    });
                nodes.push(SidebarNode::Folder(Folder {
                    id: child.id.clone(),
                    title: child
                        .title
                        .clone()
                        .filter(|t| !t.is_empty())
                        .unwrap_or_else(|| "Untitled".into()),
                    icon: icon_name(&list.custom_info.icon_type),
                    children,
                    smart,
                }));
            } else {
                // Split views, easels, documents: no equivalent yet. Their
                // tab children are still imported as plain tabs.
                let nested = self.children(child, pinned, depth + 1)?;
                if !nested.is_empty() {
                    nodes.push(SidebarNode::Folder(Folder {
                        id: child.id.clone(),
                        title: child.title.clone().unwrap_or_default(),
                        icon: None,
                        children: nested,
                        smart: None,
                    }));
                }
            }
        }
        Ok(nodes)
    }
}

fn tab_item(item: &Item, tab: &raw::Tab, pinned: bool) -> TabItem {
    let custom = item.title.as_deref().filter(|t| !t.is_empty());
    let title = custom
        .or(tab.saved_title.as_deref())
        .filter(|t| !t.is_empty())
        .unwrap_or(tab.saved_url.as_str())
        .to_string();
    TabItem {
        custom_title: custom.is_some(),
        home_url: pinned.then(|| tab.saved_url.clone()),
        created_at: item.created_at.map(apple_to_unix),
        last_active_at: tab.time_last_active_at.map(apple_to_unix),
        unread: item.is_unread,
        ..TabItem::new(&item.id, &tab.saved_url, title)
    }
}

/// `{"icon": "flash"}` or `{"emoji": "🚀"}` (and `emoji_v2` variants).
/// Arc Live Folder data to a smart folder, for the sources Magnetite can read.
/// Arc's GitHub folder lists your open pull requests and review requests.
fn live_folder(data: &Value) -> Option<magnetite_core::smart::SmartFolder> {
    use magnetite_core::smart::{ReviewShow, SmartFolder, SmartSource};
    let source = data.get("dataSource")?.as_object()?;
    source.contains_key("github").then(|| {
        SmartFolder::new(SmartSource::GitHub {
            host: "github.com".into(),
            show: ReviewShow::PullRequests,
            filter: String::new(),
        })
    })
}

fn icon_name(icon_type: &Value) -> Option<String> {
    let obj = icon_type.as_object()?;
    for key in ["emoji", "emoji_v2", "icon"] {
        if let Some(v) = obj.get(key) {
            if let Some(s) = v.as_str() {
                return Some(s.to_string());
            }
            if let Some(s) = v.get("_0").and_then(Value::as_str) {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// Walk the theme blob for the first object with red/green/blue keys.
#[allow(clippy::cast_possible_truncation)] // colour channels, f32 is plenty
fn accent_color(theme: &Value) -> Option<Color> {
    fn walk(v: &Value) -> Option<Color> {
        match v {
            Value::Object(o) => {
                if let (Some(r), Some(g), Some(b)) = (
                    o.get("red").and_then(Value::as_f64),
                    o.get("green").and_then(Value::as_f64),
                    o.get("blue").and_then(Value::as_f64),
                ) {
                    return Some(Color {
                        red: r.clamp(0.0, 1.0) as f32,
                        green: g.clamp(0.0, 1.0) as f32,
                        blue: b.clamp(0.0, 1.0) as f32,
                    });
                }
                o.values().find_map(walk)
            }
            Value::Array(a) => a.iter().find_map(walk),
            _ => None,
        }
    }
    walk(theme)
}
