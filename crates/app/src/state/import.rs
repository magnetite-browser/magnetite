//! Adding an imported sidebar next to the one that is there.

use std::collections::{HashMap, HashSet};

use magnetite_core::{Id, Profile, SidebarNode, SidebarSnapshot, TabItem};

use super::adopt::{drop_unsafe_tabs, sanitise_snapshot};
use super::{AppState, new_id, now};

/// What an import added (`AppState::add_import`).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Imported {
    /// The first new space, if any came.
    pub first: Option<Id>,
    pub spaces: usize,
    pub tabs: usize,
    pub bookmarks: usize,
}

/// Gives imported items fresh IDs and our profile IDs (`AppState::add_import`).
pub(super) struct Fresh {
    /// Imported profile ID to ours.
    profiles: HashMap<Id, Id>,
    /// Our default profile, for anything without a known profile.
    ours: Id,
    tabs: usize,
    bookmarks: usize,
}

impl Fresh {
    fn profile(&self, id: &str) -> Id {
        self.profiles.get(id).unwrap_or(&self.ours).clone()
    }

    fn tab(&mut self, mut tab: TabItem) -> TabItem {
        self.tabs += 1;
        tab.id = new_id();
        tab.profile_id = tab.profile_id.map(|p| self.profile(&p));
        tab
    }

    fn nodes(&mut self, nodes: &mut [SidebarNode]) {
        for node in nodes {
            match node {
                SidebarNode::Tab(t) => *t = self.tab(std::mem::take(t)),
                SidebarNode::Folder(f) => {
                    f.id = new_id();
                    self.nodes(&mut f.children);
                }
            }
        }
    }

    fn bookmarks(&mut self, nodes: &mut [SidebarNode]) {
        for node in nodes {
            match node {
                SidebarNode::Tab(t) => {
                    self.bookmarks += 1;
                    t.id = new_id();
                }
                SidebarNode::Folder(f) => {
                    f.id = new_id();
                    self.bookmarks(&mut f.children);
                }
            }
        }
    }
}

/// Imported Today tabs get the import time as their activity baseline, so
/// auto-archive does not sweep them away on the first tick. Tabs with
/// addresses Magnetite would not open are left out (`drop_unsafe_tabs`).
pub fn fresh_import(mut snap: SidebarSnapshot) -> SidebarSnapshot {
    sanitise_snapshot(&mut snap);
    drop_unsafe_tabs(&mut snap);
    let t = now();
    for space in &mut snap.spaces {
        for tab in &mut space.today {
            tab.last_active_at = Some(t);
        }
    }
    snap
}

impl AppState {
    /// The import of the first start: as `add_import`, but a space nobody
    /// has used yet (the only one, without a tab, a folder or a favorite
    /// beside it) makes room instead of standing in front of what comes in,
    /// so an imported "Personal" is not "Personal 2". If nothing comes in,
    /// it stays.
    pub fn add_first_import(&mut self, snap: SidebarSnapshot) -> Imported {
        let unused = self.sidebar.spaces.len() == 1
            && self.sidebar.spaces[0].pinned.is_empty()
            && self.sidebar.spaces[0].bookmarks.is_empty()
            && self.sidebar.all_tabs().next().is_none();
        let starter = unused.then(|| self.sidebar.spaces.remove(0));
        let added = self.add_import(snap);
        match (starter, &added.first) {
            (Some(starter), None) => self.sidebar.spaces.push(starter),
            (Some(starter), Some(first)) => {
                self.active_tab.remove(&starter.id);
                self.unsynced_spaces.remove(&starter.id);
                self.active_space.clone_from(first);
            }
            (None, _) => {}
        }
        added
    }

    /// Add an imported snapshot next to the current sidebar: its spaces
    /// follow ours with fresh IDs, its default profile becomes our default
    /// and its other profiles new ones, and its favorites join the matching
    /// sets (addresses already there are skipped). Returns the first new
    /// space and how many spaces and tabs were added.
    pub fn add_import(&mut self, snap: SidebarSnapshot) -> Imported {
        let snap = fresh_import(snap);
        let had_smart: HashSet<Id> = self.smart_folders().into_iter().map(|(id, _)| id).collect();
        let ours = self.default_profile_id();
        let mut fresh = Fresh {
            profiles: HashMap::new(),
            ours,
            tabs: 0,
            bookmarks: 0,
        };
        for p in snap.profiles {
            let id = if p.is_default {
                fresh.ours.clone()
            } else {
                let id = new_id();
                self.sidebar.profiles.push(Profile {
                    id: id.clone(),
                    name: p.name,
                    is_default: false,
                    ephemeral: p.ephemeral,
                });
                id
            };
            fresh.profiles.insert(p.id, id);
        }
        for set in snap.favorites {
            let profile = fresh.profile(&set.profile_id);
            let found = self
                .sidebar
                .favorites
                .iter()
                .position(|f| f.profile_id == profile);
            let at = found.unwrap_or_else(|| {
                self.sidebar.favorites.push(magnetite_core::FavoriteSet {
                    profile_id: profile,
                    items: vec![],
                });
                self.sidebar.favorites.len() - 1
            });
            for item in set.items {
                let known = &self.sidebar.favorites[at].items;
                if !known.iter().any(|t| t.url == item.url) {
                    let item = fresh.tab(item);
                    self.sidebar.favorites[at].items.push(item);
                }
            }
        }
        let count = snap.spaces.len();
        let mut first = None;
        // Titles in use, and per title the number to try next: a file with
        // thousands of spaces of one name must not take minutes to number.
        let mut taken: HashSet<String> =
            (self.sidebar.spaces.iter().map(|s| s.title.clone())).collect();
        let mut next: HashMap<String, usize> = HashMap::new();
        for mut space in snap.spaces {
            space.id = new_id();
            first.get_or_insert_with(|| space.id.clone());
            // "Personal" next to "Personal" would be ambiguous: number it.
            if taken.contains(&space.title) {
                let base = space.title.clone();
                let n = next.entry(base.clone()).or_insert(2);
                while taken.contains(&format!("{base} {n}")) {
                    *n += 1;
                }
                space.title = format!("{base} {n}");
            }
            taken.insert(space.title.clone());
            space.profile_id = fresh.profile(&space.profile_id);
            space.today = space.today.into_iter().map(|t| fresh.tab(t)).collect();
            fresh.nodes(&mut space.pinned);
            fresh.bookmarks(&mut space.bookmarks);
            self.sidebar.spaces.push(space);
        }
        self.normalise_favorites();
        // Smart folders that came with the import were asked for here, by
        // importing: they may fetch on this device. (Without a list every
        // folder may.)
        if self.settings.smart_allowed.is_some() {
            let brought = self.smart_folders().into_iter().map(|(id, _)| id);
            for id in brought
                .filter(|id| !had_smart.contains(id))
                .collect::<Vec<_>>()
            {
                self.allow_smart(&id);
            }
        }
        Imported {
            first,
            spaces: count,
            tabs: fresh.tabs,
            bookmarks: fresh.bookmarks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnetite_core::{Folder, Space};

    use crate::state::testing::{state, tab};

    #[test]
    fn the_first_import_takes_the_place_of_a_space_nobody_used() {
        let space = |title: &str, tabs: &[&str]| Space {
            id: title.into(),
            title: title.into(),
            icon: None,
            accent: None,
            profile_id: "p0".into(),
            pinned: vec![],
            today: tabs.iter().map(|t| tab(t)).collect(),
            bookmarks: Vec::new(),
        };
        let import = |spaces: Vec<Space>| {
            let mut snap = SidebarSnapshot::new();
            snap.profiles = vec![Profile {
                id: "p0".into(),
                name: "Default".into(),
                is_default: true,
                ephemeral: false,
            }];
            snap.spaces = spaces;
            snap
        };
        // A new installation: one space, "Personal", and nothing in it.
        let mut s = state();
        s.sidebar.spaces[0].today.clear();
        s.sidebar.spaces[0].pinned.clear();
        s.sidebar.spaces[0].title = "Personal".into();
        let starter = s.sidebar.spaces[0].id.clone();

        // Nothing comes in: the space stays.
        assert_eq!(s.add_first_import(import(vec![])).spaces, 0);
        assert_eq!(s.sidebar.spaces.len(), 1);
        assert_eq!(s.active_space, starter);

        let added = s.add_first_import(import(vec![space("Personal", &["x"]), space("Work", &[])]));
        assert_eq!((added.spaces, added.tabs), (2, 1));
        let first = added.first;
        let titles: Vec<&str> = s
            .sidebar
            .spaces
            .iter()
            .map(|sp| sp.title.as_str())
            .collect();
        assert_eq!(
            titles,
            ["Personal", "Work"],
            "in its place, under its own name"
        );
        assert_eq!(Some(&s.active_space), first.as_ref());
        assert!(s.sidebar.spaces.iter().all(|sp| sp.id != starter));

        // A second import finds spaces in use: it adds, and numbers the clash.
        s.add_first_import(import(vec![space("Personal", &["y"])]));
        let titles: Vec<&str> = s
            .sidebar
            .spaces
            .iter()
            .map(|sp| sp.title.as_str())
            .collect();
        assert_eq!(titles, ["Personal", "Work", "Personal 2"]);
    }

    #[test]
    fn an_import_adds_spaces_next_to_ours_with_fresh_ids() {
        let mut s = state();
        let ours = s.sidebar.profiles[0].id.clone();
        s.sidebar.favorites[0].items.push(tab("dup"));
        let before = s.sidebar.spaces.len();
        let mut snap = SidebarSnapshot::new();
        snap.profiles = vec![
            Profile {
                id: "p0".into(),
                name: "Personal".into(),
                is_default: true,
                ephemeral: false,
            },
            Profile {
                id: "p1".into(),
                name: "Work".into(),
                is_default: false,
                ephemeral: false,
            },
        ];
        snap.favorites = vec![magnetite_core::FavoriteSet {
            profile_id: "p0".into(),
            items: vec![tab("fav"), tab("dup")],
        }];
        let space = |id: &str, profile: &str| Space {
            id: id.into(),
            title: id.into(),
            icon: None,
            accent: None,
            profile_id: profile.into(),
            pinned: vec![],
            today: vec![],
            bookmarks: Vec::new(),
        };
        snap.spaces = vec![
            Space {
                pinned: vec![SidebarNode::Folder(Folder {
                    id: "folder".into(),
                    title: "Reading".into(),
                    icon: None,
                    children: vec![SidebarNode::Tab(tab("a"))],
                    smart: None,
                })],
                today: vec![
                    tab("b"),
                    TabItem {
                        url: "javascript:alert(1)".into(),
                        ..tab("evil")
                    },
                ],
                bookmarks: vec![SidebarNode::Folder(Folder {
                    id: "marks".into(),
                    title: "Dev".into(),
                    icon: None,
                    children: vec![
                        SidebarNode::Tab(tab("kept")),
                        SidebarNode::Tab(TabItem {
                            url: "javascript:alert(2)".into(),
                            ..tab("bookmarklet")
                        }),
                    ],
                    smart: None,
                })],
                ..space("x", "p0")
            },
            Space {
                today: vec![tab("c")],
                ..space("y", "p1")
            },
        ];
        snap.spaces[1].title.clone_from(&s.sidebar.spaces[0].title);

        let imported = s.add_import(snap);
        assert_eq!(
            (imported.spaces, imported.tabs, imported.bookmarks),
            (2, 4, 1),
            "a, b, c and the new favorite; one bookmark"
        );
        let added = &s.sidebar.spaces[before..];
        assert_eq!(imported.first.as_deref(), Some(added[0].id.as_str()));
        // The bookmark came along under a new id; the script address did not.
        let saved: Vec<_> = added[0]
            .bookmarks
            .iter()
            .flat_map(SidebarNode::tabs)
            .collect();
        assert_eq!(saved.len(), 1);
        assert!(saved[0].id != "kept" && saved[0].url == "https://example.org/kept");
        assert_eq!(added[0].profile_id, ours, "their default profile is ours");
        let work = s
            .sidebar
            .profiles
            .iter()
            .find(|p| p.name == "Work")
            .unwrap();
        assert!(!work.is_default);
        assert_eq!(added[1].profile_id, work.id);
        let numbered = format!("{} 2", s.sidebar.spaces[0].title);
        assert_eq!(added[1].title, numbered, "same name numbered");
        assert_eq!(added[0].today.len(), 1, "unsafe address dropped");
        let favs: Vec<_> = s.sidebar.favorites[0]
            .items
            .iter()
            .map(|t| t.title.as_str())
            .collect();
        assert_eq!(favs, ["dup", "fav"], "known address not added twice");
        let mut ids: Vec<_> = s.sidebar.all_tabs().map(|t| t.id.clone()).collect();
        ids.extend(s.sidebar.spaces.iter().map(|sp| sp.id.clone()));
        let count = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), count, "every ID is unique");
        let renamed = added.iter().all(|sp| {
            !["x", "y"].contains(&sp.id.as_str())
                && sp
                    .today
                    .iter()
                    .all(|t| !["b", "c"].contains(&t.id.as_str()))
        });
        assert!(renamed, "imported IDs replaced");
    }

    #[test]
    fn fresh_import_resets_activity_of_today_tabs_only() {
        let snap = fresh_import(state().sidebar);
        let before = now() - 5;
        for t in &snap.spaces[0].today {
            assert!(t.last_active_at.unwrap() >= before);
        }
        let pinned = snap.spaces[0]
            .pinned
            .iter()
            .flat_map(SidebarNode::tabs)
            .next()
            .unwrap();
        assert_eq!(pinned.last_active_at, Some(1));
    }
}
