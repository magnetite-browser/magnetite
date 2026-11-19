//! The private space: a space for browsing that leaves nothing behind.
//! There is at most one. It is not saved and not synced, and every tab
//! opened in it runs in an in-memory profile of its own, so no tab sees
//! the cookies, logins or storage of another. A page opened from a private
//! tab stays in that tab's profile (a sign-in window has to). Closing the
//! space closes its tabs; a profile goes once nothing runs in it. Hidden,
//! it stays open but is listed nowhere until it is opened again.

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

    /// Whether `id` is the private space while that is hidden.
    pub fn is_hidden_space(&self, id: &str) -> bool {
        self.private_hidden && self.is_private_space(id)
    }

    /// The private space, unless it is hidden: what the sidebar hears of.
    pub fn shown_private_space(&self) -> Option<&str> {
        self.private_space
            .as_deref()
            .filter(|_| !self.private_hidden)
    }

    /// The spaces that are listed and can be gone to: all but a hidden
    /// private space.
    pub fn listed_spaces(&self) -> impl Iterator<Item = &Space> {
        (self.sidebar.spaces.iter()).filter(|sp| !self.is_hidden_space(&sp.id))
    }

    /// Hide the private space: it stays open with its pages, but no list
    /// names it or its tabs, and a window that showed it goes on in the
    /// space before it. Opening the private space shows it again. Returns
    /// the pages of its tabs, to silence; `None` if there is no private
    /// space to hide.
    pub fn hide_private_space(&mut self) -> Option<Vec<ContentId>> {
        let id = self.shown_private_space()?.to_string();
        self.private_hidden = true;
        let before = self.beside_private(&id);
        let shown = std::iter::once(&mut self.active_space)
            .chain(self.views.values_mut().map(|v| &mut v.active_space));
        for space in shown.filter(|space| **space == id) {
            space.clone_from(&before);
        }
        // A playing page is paused by the caller; as paused it is not
        // popped out as a picture when it leaves the screen.
        let space = self.space(&id)?;
        let tabs: Vec<Id> = space.today.iter().map(|t| t.id.clone()).collect();
        let mut contents = vec![];
        for tab in tabs {
            let Some(runtime) = self.runtime.get_mut(&tab) else {
                continue;
            };
            if let Some(media) = &mut runtime.media {
                media.playing = false;
            }
            contents.extend(runtime.content);
        }
        Some(contents)
    }

    /// The private space, opened if there is none yet: the last of the
    /// spaces, empty; shown again if it was hidden. Returns its id.
    pub fn open_private_space(&mut self) -> Id {
        self.private_hidden = false;
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
            icon_image: None,
            accent: Some(Color {
                red: 0.62,
                green: 0.62,
                blue: 0.68,
            }),
            profile_id,
            shortcut: None,
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
        self.private_hidden = false;
        let before = self.beside_private(&id);
        let spaces = &self.sidebar.spaces;
        let at = spaces.iter().position(|sp| sp.id == id).unwrap_or(0);
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
    pub fn lasting_space(&self, space: &Id) -> Id {
        if !self.is_private_space(space) {
            return space.clone();
        }
        self.beside_private(space)
    }

    /// The space that takes the place of the private space `id` where that
    /// is gone: the one before it, or the one after it if it is the first
    /// (another space was moved behind it). There always is one
    /// (`delete_space`).
    fn beside_private(&self, id: &str) -> Id {
        let spaces = &self.sidebar.spaces;
        let at = spaces.iter().position(|sp| sp.id == id).unwrap_or(0);
        let beside = if at == 0 { 1 } else { at - 1 };
        spaces.get(beside).unwrap_or(&spaces[0]).id.clone()
    }

    /// The space a kept page of profile `profile` (a peek, a mini window)
    /// becomes a tab of when `space` is asked for. Nothing crosses the
    /// private space's border: a private page stays in the private space,
    /// and another page goes to the space beside it instead.
    pub(super) fn keep_space(&self, profile: &str, space: &str) -> Id {
        match &self.private_space {
            Some(private) if self.private_profiles.contains(profile) => private.clone(),
            Some(private) if private == space => self.beside_private(private),
            _ => space.to_string(),
        }
    }

    /// The other windows that show the private space.
    pub fn other_windows_in_private_space(&self) -> Vec<magnetite_engine::WindowId> {
        (self.views.iter())
            .filter(|(_, v)| self.is_private_space(&v.active_space))
            .map(|(win, _)| *win)
            .collect()
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
    fn a_hidden_private_space_is_listed_nowhere_and_comes_back_as_it_was() {
        let mut s = state();
        let first = s.active_space.clone();
        assert_eq!(s.hide_private_space(), None, "nothing to hide");
        let private = s.open_private_space();
        let tab = s.new_today_tab(&private, "https://example.org/secret", "secret");
        let runtime = s.runtime.entry(tab.clone()).or_default();
        runtime.content = Some(7);
        runtime.media = Some(magnetite_engine::MediaState {
            playing: true,
            video: true,
            ..Default::default()
        });
        s.active_space.clone_from(&private);

        assert_eq!(s.hide_private_space(), Some(vec![7]));
        assert_eq!(s.active_space, first);
        // Noted as paused at once, before its page leaves the screen: a
        // playing video would be popped out as a picture (`follow_pictures`).
        assert!(!s.runtime[&tab].media.as_ref().unwrap().playing);
        let ui = serde_json::to_value(s.ui_state(&[])).unwrap();
        assert!(ui["private_space"].is_null());
        assert!(!ui["sidebar"].to_string().contains("secret"));
        assert!(s.listed_spaces().all(|sp| sp.id != private));
        assert_eq!(s.search("secret").len(), 0);
        // Still open: the tab and its profile are in use.
        assert!(s.in_private_space(&tab));
        assert_eq!(s.release_private_profiles(), [] as [String; 0]);
        assert_eq!(s.hide_private_space(), None, "hidden already");

        assert_eq!(s.open_private_space(), private);
        assert!(s.listed_spaces().any(|sp| sp.id == private));
        assert_eq!(s.ui_state(&[]).private_space, Some(private.as_str()));
        assert_eq!(s.search("secret").len(), 1);
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

    #[test]
    fn the_private_space_closes_also_as_the_first_space_and_is_no_reason_to_delete_the_last() {
        let mut s = state();
        let private = s.open_private_space();
        // The only other spaces go behind it.
        s.move_space(&private, 0);
        let second = s.sidebar.spaces[1].id.clone();
        s.active_space.clone_from(&private);
        assert_eq!(s.lasting_space(&private), second);
        s.close_private_space();
        assert_eq!(s.active_space, second);

        // One space and the private one: the one space stays.
        while s.sidebar.spaces.len() > 1 {
            let last = s.sidebar.spaces.last().unwrap().id.clone();
            s.delete_space(&last);
        }
        let only = s.sidebar.spaces[0].id.clone();
        s.open_private_space();
        s.delete_space(&only);
        assert_eq!(s.sidebar.spaces[0].id, only);
    }

    #[test]
    fn a_tab_opened_or_restored_stays_on_its_side_and_no_private_id_is_saved() {
        let mut s = state();
        let first = s.active_space.clone();
        let plain = s.active_space().profile_id.clone();
        let private = s.open_private_space();
        let tab = s.new_today_tab(&private, "https://example.org/1", "1");
        let of_tab = s.profile_for_tab(&tab).unwrap();
        let url = "https://example.org/opened";

        // A private page opens a tab while another space is in front.
        let opened = s.open_tab_from(Some(&of_tab), url, url).unwrap();
        assert!(s.in_private_space(&opened));
        assert_eq!(s.profile_for_tab(&opened).unwrap(), of_tab);
        // A plain page does so while the private space is in front.
        s.active_space.clone_from(&private);
        let opened = s.open_tab_from(Some(&plain), url, url).unwrap();
        assert_eq!(s.find_tab(&opened).unwrap().0.unwrap().id, first);
        assert!(!s.is_private_tab(&opened));
        // The user's own tab is one of the space in front.
        let own = s.open_tab_from(None, url, url).unwrap();
        assert!(s.in_private_space(&own));

        // An archived tab whose space is gone is restored beside the
        // private space, not into it.
        let gone = s.new_today_tab(&first, url, url);
        s.archive_tab(&gone);
        s.archive[0].space_id = "gone".into();
        let restored = s.restore_archived(0).unwrap();
        assert_eq!(s.find_tab(&restored).unwrap().0.unwrap().id, first);

        // What Chromium pins is not noted for a profile of the space.
        assert!(!s.follow_chromium_pins(&of_tab, vec!["vault".into()]));
        assert!(s.settings.chromium_pins_seen.is_empty());

        // The space is closed, a page of it not yet: it opens nothing.
        s.close_private_space();
        assert_eq!(s.open_tab_from(Some(&of_tab), url, url), None);
    }

    #[test]
    fn a_kept_page_stays_on_its_side_and_another_windows_peek_keeps_its_profile() {
        let mut s = state();
        let first = s.active_space.clone();
        let plain = s.active_space().profile_id.clone();
        let private = s.open_private_space();
        let tab = s.new_today_tab(&private, "https://example.org/1", "1");
        let of_tab = s.profile_for_tab(&tab).unwrap();
        let page = |content, profile: &str| Peek {
            content,
            url: "https://example.org/kept".into(),
            title: String::new(),
            runtime: TabRuntime::default(),
            profile: profile.into(),
        };
        // A private mini window asked into another space, and a plain one
        // asked into the private space.
        s.minis = vec![page(7, &of_tab), page(8, &plain)];
        let kept = s.keep_mini(7, &first).unwrap();
        assert!(s.in_private_space(&kept));
        assert_eq!(s.profile_for_tab(&kept).unwrap(), of_tab);
        let kept = s.keep_mini(8, &private).unwrap();
        assert!(!s.in_private_space(&kept) && !s.is_private_tab(&kept));

        // The tabs close; a peek of the profile is left in another window.
        for t in s.space(&private).unwrap().today.clone() {
            s.archive_tab(&t.id);
        }
        s.open_window(2);
        s.peek = Some(page(9, &of_tab));
        s.enter(1);
        assert!(!s.release_private_profiles().contains(&of_tab));
        assert_eq!(s.close_private_space(), [9]);
    }
}
