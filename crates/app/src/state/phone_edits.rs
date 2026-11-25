//! Making a phone's change to the sidebar (`devices::Change`). The phone
//! shows it already; this device makes it as the user would here, and the
//! sidebar syncs after. Each change is made once: one that adds something
//! carries the new item's id, and an item with that id is not added again.

use magnetite_core::{FavoriteSet, SidebarNode, TabItem};
use magnetite_engine::ContentId;

use super::{AppState, now};
use crate::devices::Change;
use crate::url_policy;

/// What making a change did.
#[derive(Debug, Default, PartialEq)]
pub struct Made {
    /// The sidebar changed.
    pub changed: bool,
    /// Pages of tabs that went, for the caller to close.
    pub closed: Vec<ContentId>,
}

impl AppState {
    /// Make `change` in the sidebar, as far as it can be made: an address
    /// this device would not open, a space gone meanwhile or the private
    /// space as its target leave it undone.
    pub fn apply_phone_edit(&mut self, change: &Change) -> Made {
        let changed = |changed| Made {
            changed,
            closed: Vec::new(),
        };
        match change {
            Change::Pin {
                space,
                item,
                url,
                title,
            } => {
                if self.find_tab(item).is_some() || !self.phone_may_add(space, url) {
                    return Made::default();
                }
                let tab = phone_tab(item, url, title);
                let pinned = self.space_mut(space).map(|s| &mut s.pinned);
                changed(pinned.map(|p| p.push(SidebarNode::Tab(tab))).is_some())
            }
            Change::Favorite {
                space,
                item,
                url,
                title,
            } => {
                if self.find_tab(item).is_some() || !self.phone_may_add(space, url) {
                    return Made::default();
                }
                let Some(profile) = self.space(space).map(|s| s.profile_id.clone()) else {
                    return Made::default();
                };
                let tab = phone_tab(item, url, title);
                let lists = &mut self.sidebar.favorites;
                match lists.iter_mut().find(|f| f.profile_id == profile) {
                    Some(list) => list.items.push(tab),
                    None => lists.push(FavoriteSet {
                        profile_id: profile,
                        items: vec![tab],
                    }),
                }
                self.normalise_favorites();
                changed(true)
            }
            Change::NewSpace { item, title, icon } => {
                if self.space(item).is_some() {
                    return Made::default();
                }
                // As one made here (default profile, the desktop's look),
                // under the phone's id, which the phone shows it with.
                let Ok(id) = self.new_space(title, icon.clone(), None) else {
                    return Made::default();
                };
                if let Some(space) = self.space_mut(&id) {
                    space.id.clone_from(item);
                }
                changed(true)
            }
            Change::Rename { item, title } => {
                let title = title.trim();
                if title.is_empty() || self.is_private_space(item) {
                    return Made::default();
                }
                if let Some(space) = self.space_mut(item) {
                    space.title = title.into();
                } else if let Some(folder) = self.folder_mut(item) {
                    folder.title = title.into();
                } else if let Some(tab) = self.find_tab_mut(item) {
                    tab.custom_title = true;
                    tab.title = title.into();
                } else {
                    return Made::default();
                }
                changed(true)
            }
            Change::Remove { item } => {
                let favorite = self
                    .find_tab(item)
                    .is_some_and(|(space, _)| space.is_none());
                let closed = if self.is_pinned_tab(item) {
                    self.archive_pinned(item)
                } else if favorite {
                    self.archive_tab(item)
                } else {
                    return Made::default();
                };
                Made {
                    changed: true,
                    closed: closed.into_iter().collect(),
                }
            }
        }
    }

    /// Whether a phone may add `url` to `space`: an address this device
    /// opens, in a space that is there and not the private one.
    fn phone_may_add(&self, space: &str, url: &str) -> bool {
        url_policy::allowed(url, url_policy::Source::Stored)
            && self.space(space).is_some()
            && !self.is_private_space(space)
    }
}

/// A pinned tab or favorite as a phone asked for it: at home on `url`,
/// named `title` until its page says what it is called.
fn phone_tab(id: &str, url: &str, title: &str) -> TabItem {
    TabItem {
        home_url: Some(url.into()),
        created_at: Some(now()),
        ..TabItem::new(id, url, title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::state;

    fn pin(item: &str, url: &str) -> Change {
        Change::Pin {
            space: String::new(),
            item: item.into(),
            url: url.into(),
            title: "P".into(),
        }
    }

    /// `change` for the space in front of `s`.
    fn here(s: &AppState, change: Change) -> Change {
        match change {
            Change::Pin {
                item, url, title, ..
            } => Change::Pin {
                space: s.active_space.clone(),
                item,
                url,
                title,
            },
            Change::Favorite {
                item, url, title, ..
            } => Change::Favorite {
                space: s.active_space.clone(),
                item,
                url,
                title,
            },
            other => other,
        }
    }

    #[test]
    fn a_pinned_tab_from_a_phone_is_added_once_and_only_for_the_web() {
        let mut s = state();
        let change = here(&s, pin("t9", "https://p.example/"));
        assert!(s.apply_phone_edit(&change).changed);
        assert!(!s.apply_phone_edit(&change).changed, "the same edit again");
        assert!(s.is_pinned_tab("t9"));
        let (_, tab) = s.find_tab("t9").unwrap();
        assert_eq!(tab.home_url.as_deref(), Some("https://p.example/"));

        let local = here(&s, pin("t10", "file:///etc/passwd"));
        assert!(!s.apply_phone_edit(&local).changed);
        assert!(s.find_tab("t10").is_none());
    }

    #[test]
    fn a_favorite_from_a_phone_joins_its_spaces_profile() {
        let mut s = state();
        let change = here(
            &s,
            Change::Favorite {
                space: String::new(),
                item: "f9".into(),
                url: "https://f.example/".into(),
                title: "F".into(),
            },
        );
        assert!(s.apply_phone_edit(&change).changed);
        let profile = s.active_space().profile_id.clone();
        let list = s
            .sidebar
            .favorites
            .iter()
            .find(|f| f.profile_id == profile)
            .unwrap();
        assert!(list.items.iter().any(|t| t.id == "f9"));
    }

    #[test]
    fn a_new_space_keeps_the_phones_id_and_a_rename_reaches_it() {
        let mut s = state();
        let new = Change::NewSpace {
            item: "s9".into(),
            title: "Trip".into(),
            icon: Some("plane".into()),
        };
        assert!(s.apply_phone_edit(&new).changed);
        assert!(!s.apply_phone_edit(&new).changed);
        let rename = Change::Rename {
            item: "s9".into(),
            title: " Holiday ".into(),
        };
        assert!(s.apply_phone_edit(&rename).changed);
        let space = s.space("s9").unwrap();
        assert_eq!(
            (space.title.as_str(), space.icon.as_deref()),
            ("Holiday", Some("plane"))
        );
    }

    #[test]
    fn a_removed_pinned_tab_goes_to_the_archive_and_a_today_tab_stays() {
        let mut s = state();
        let made = s.apply_phone_edit(&Change::Remove { item: "p".into() });
        assert!(made.changed && !s.is_pinned_tab("p"));
        assert!(s.archive.iter().any(|a| a.tab.id == "p"));
        // Today tabs are not the phone's to remove.
        assert!(
            !s.apply_phone_edit(&Change::Remove { item: "a".into() })
                .changed
        );
        assert!(s.find_tab("a").is_some());
    }
}
