//! The private space: a space for browsing that leaves nothing behind.
//! There is at most one. It is not saved and not synced, and every tab
//! opened in it runs in an in-memory profile of its own, so no tab sees
//! the cookies, logins or storage of another. A page opened from a private
//! tab stays in that tab's profile (a sign-in window has to). Closing the
//! space closes its tabs; a profile goes once nothing runs in it.

use magnetite_core::{Color, Id, Profile, SidebarNode, Space};
use magnetite_engine::ContentId;

use super::{AppState, new_id};

impl AppState {
    pub fn is_private_space(&self, id: &str) -> bool {
        self.private_space.as_deref() == Some(id)
    }

    /// Whether tab `id` is one of the private space.
    pub fn in_private_space(&self, id: &str) -> bool {
        self.find_tab(id)
            .is_some_and(|(space, _)| space.is_some_and(|sp| self.is_private_space(&sp.id)))
    }

    /// The private space, opened if there is none yet: the last of the
    /// spaces, empty. Returns its id.
    pub fn open_private_space(&mut self) -> Id {
        if let Some(id) = &self.private_space {
            return id.clone();
        }
        let id = new_id();
        // The space's own profile: what a tab without a profile of its own
        // would run in. In memory too, so nothing in this space can reach
        // a profile that is kept.
        let profile_id = self.new_private_profile();
        self.sidebar.spaces.push(Space {
            id: id.clone(),
            title: "Private".into(),
            icon: None,
            accent: Some(Color {
                red: 0.62,
                green: 0.62,
                blue: 0.68,
            }),
            profile_id,
            pinned: vec![],
            today: vec![],
            bookmarks: vec![],
        });
        self.private_space = Some(id.clone());
        id
    }

    /// A new in-memory profile for the private space.
    pub(super) fn new_private_profile(&mut self) -> Id {
        let id = new_id();
        self.sidebar.profiles.push(Profile {
            id: id.clone(),
            name: "Private".into(),
            is_default: false,
            ephemeral: true,
        });
        self.private_profiles.insert(id.clone());
        id
    }

    /// Close the private space: its tabs close without a record, and so do
    /// the peeks and mini windows opened from them. A window that showed it
    /// goes on in the space before it. Returns the page windows to close.
    pub fn close_private_space(&mut self) -> Vec<ContentId> {
        let Some(id) = self.private_space.take() else {
            return vec![];
        };
        let spaces = &self.sidebar.spaces;
        let at = spaces.iter().position(|sp| sp.id == id).unwrap_or(0);
        let before = spaces[at.saturating_sub(1)].id.clone();
        let tabs: Vec<Id> = (spaces[at].pinned.iter())
            .flat_map(SidebarNode::tabs)
            .chain(spaces[at].today.iter())
            .map(|t| t.id.clone())
            .collect();
        // Their profiles are in memory, so `archive_tab` keeps nothing.
        let mut contents: Vec<ContentId> =
            tabs.iter().filter_map(|t| self.archive_tab(t)).collect();
        let private = |profile: &str| self.private_profiles.contains(profile);
        let peeks = (self.peek.iter()).chain(self.views.values().filter_map(|v| v.peek.as_ref()));
        contents.extend(
            peeks
                .chain(self.minis.iter())
                .filter(|p| private(&p.profile))
                .map(|p| p.content),
        );
        self.sidebar.spaces.retain(|sp| sp.id != id);
        self.active_tab.remove(&id);
        let shown = std::iter::once(&mut self.active_space)
            .chain(self.views.values_mut().map(|v| &mut v.active_space));
        for space in shown.filter(|space| **space == id) {
            space.clone_from(&before);
        }
        for view in self.views.values_mut() {
            view.active_tab.remove(&id);
        }
        contents
    }

    /// The profiles of the private space that nothing runs in any more (a
    /// closed tab's, all of them once the space is closed) are dropped from
    /// the list. Returns them, for the engine to drop their data.
    pub fn release_private_profiles(&mut self) -> Vec<Id> {
        let unused: Vec<Id> = (self.private_profiles.iter())
            .filter(|id| !self.profile_in_use(id))
            .cloned()
            .collect();
        for id in &unused {
            self.private_profiles.remove(id);
            self.sidebar.profiles.retain(|p| &p.id != id);
        }
        unused
    }

    /// The space saved in place of `space` as the one in front: the private
    /// space is gone at the next start, so the one before it.
    pub(super) fn lasting_space(&self, space: &Id) -> Id {
        if !self.is_private_space(space) {
            return space.clone();
        }
        let spaces = &self.sidebar.spaces;
        let at = spaces.iter().position(|sp| &sp.id == space).unwrap_or(0);
        spaces[at.saturating_sub(1)].id.clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::state::testing::state;
    use crate::state::{Peek, TabRuntime};

    #[test]
    fn every_private_tab_runs_in_an_in_memory_profile_of_its_own() {
        let mut s = state();
        let private = s.open_private_space();
        assert_eq!(
            s.open_private_space(),
            private,
            "there is one private space"
        );
        assert_eq!(s.sidebar.spaces.last().unwrap().id, private);
        let one = s.new_today_tab(&private, "https://example.org/1", "1");
        let two = s.new_today_tab(&private, "https://example.org/2", "2");
        let profile = |s: &super::AppState, tab: &str| s.profile_for_tab(tab).unwrap();
        assert_ne!(profile(&s, &one), profile(&s, &two));
        assert_ne!(profile(&s, &one), s.space(&private).unwrap().profile_id);
        for tab in [&one, &two] {
            let context = s.context(Some(&profile(&s, tab))).unwrap();
            assert!(context.ephemeral && s.is_private_tab(tab));
        }
        // A tab of another space is as before.
        let space = s.sidebar.spaces[0].id.clone();
        let plain = s.new_today_tab(&space, "https://example.org/3", "3");
        assert_eq!(s.find_tab(&plain).unwrap().1.profile_id, None);
    }

    #[test]
    fn nothing_of_the_private_space_is_saved_or_synced() {
        let mut s = state();
        let first = s.active_space.clone();
        let private = s.open_private_space();
        let tab = s.new_today_tab(&private, "https://example.org/secret", "secret");
        s.active_space.clone_from(&private);
        s.runtime.insert(
            tab.clone(),
            TabRuntime {
                favicon: Some("https://example.org/favicon.ico".into()),
                ..Default::default()
            },
        );
        for snap in [s.lasting_sidebar(), s.sidebar_for_sync()] {
            assert!(snap.spaces.iter().all(|sp| sp.id != private));
            assert!(snap.profiles.iter().all(|p| !p.ephemeral));
            assert!(snap.all_tabs().all(|t| t.id != tab));
        }
        let saved = serde_json::to_string(&s.saved_form()).unwrap();
        assert!(!saved.contains("secret") && !saved.contains("favicon.ico"));
        // The next start goes on in the space before it.
        assert_eq!(s.lasting_space(&private), first);
    }

    #[test]
    fn closing_the_private_space_leaves_nothing_and_a_closed_tab_takes_its_profile() {
        let mut s = state();
        let first = s.active_space.clone();
        let profiles = s.sidebar.profiles.len();
        let private = s.open_private_space();
        let one = s.new_today_tab(&private, "https://example.org/1", "1");
        let two = s.new_today_tab(&private, "https://example.org/2", "2");
        s.active_space.clone_from(&private);
        let of_one = s.profile_for_tab(&one).unwrap();
        // Nothing to drop while every profile is in use.
        assert_eq!(s.release_private_profiles(), [] as [String; 0]);
        s.archive_tab(&one);
        assert_eq!(s.release_private_profiles(), std::slice::from_ref(&of_one));
        assert!(s.context(Some(&of_one)).is_none() && s.archive.is_empty());
        // A peek opened from the other tab runs in that tab's profile and
        // closes with the space.
        let of_two = s.profile_for_tab(&two).unwrap();
        s.runtime.entry(two.clone()).or_default().content = Some(7);
        s.content_to_tab.insert(7, two);
        s.peek = Some(Peek {
            content: 8,
            url: "https://example.org/peek".into(),
            title: String::new(),
            runtime: TabRuntime::default(),
            profile: of_two,
        });
        let mut closed = s.close_private_space();
        closed.sort_unstable();
        assert_eq!(closed, [7, 8]);
        assert_eq!(s.active_space, first);
        assert!(s.private_space.is_none() && s.archive.is_empty());
        // The engine reports the peek closed; then its profile goes too.
        s.peek = None;
        assert_eq!(s.release_private_profiles().len(), 2);
        assert_eq!(s.sidebar.profiles.len(), profiles);
        assert!(s.private_profiles.is_empty());
        // Unlike a deleted profile, none of them is remembered.
        assert_eq!(s.deleted_profiles, [] as [String; 0]);
    }

    #[test]
    fn a_sidebar_from_sync_leaves_the_private_space_as_it_is() {
        let mut s = state();
        let private = s.open_private_space();
        let tab = s.new_today_tab(&private, "https://example.org/1", "1");
        let profile = s.profile_for_tab(&tab).unwrap();
        s.prune_private_profiles();
        let remote = s.sidebar_for_sync();
        s.adopt_sidebar(remote);
        assert_eq!(s.sidebar.spaces.last().unwrap().id, private);
        let (space, _) = s.find_tab(&tab).unwrap();
        assert_eq!(space.unwrap().id, private);
        assert_eq!(s.profile_for_tab(&tab).unwrap(), profile);
        assert!(s.is_private_tab(&tab));
    }
}
