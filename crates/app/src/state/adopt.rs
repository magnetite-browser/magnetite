//! Taking over a sidebar that arrived by sync, and what of this device's
//! sidebar is sent: nothing from elsewhere is trusted as it comes.

use std::collections::{BTreeSet, HashSet};

use magnetite_core::{SidebarNode, SidebarSnapshot, TabItem, UnixSeconds};
use magnetite_engine::ContentId;

use super::{AppState, now};

/// Remove tabs whose addresses Magnetite would not open from data that came
/// from elsewhere (sync, an import): `javascript:`, `data:`, internal
/// pages. Folders keep their other children.
pub fn drop_unsafe_tabs(snap: &mut SidebarSnapshot) {
    use crate::url_policy::{Source, allowed};
    retain_tabs(snap, &|t| {
        allowed(&t.url, Source::Stored)
            && t.home_url
                .as_deref()
                .is_none_or(|h| allowed(h, Source::Stored))
    });
}

/// Keep only the tabs and bookmarks `keep` says yes to, wherever they are.
/// Folders keep their other children.
fn retain_tabs(snap: &mut SidebarSnapshot, keep: &dyn Fn(&TabItem) -> bool) {
    fn nodes(list: &mut Vec<SidebarNode>, keep: &dyn Fn(&TabItem) -> bool) {
        list.retain(|n| match n {
            SidebarNode::Tab(t) => keep(t),
            SidebarNode::Folder(_) => true,
        });
        for n in list.iter_mut() {
            if let SidebarNode::Folder(f) = n {
                nodes(&mut f.children, keep);
            }
        }
    }
    for set in &mut snap.favorites {
        set.items.retain(|t| keep(t));
    }
    for space in &mut snap.spaces {
        space.today.retain(|t| keep(t));
        nodes(&mut space.pinned, keep);
        nodes(&mut space.bookmarks, keep);
    }
}

/// The ids of every tab and folder in `nodes`, into `ids`.
fn node_ids(nodes: &[SidebarNode], ids: &mut HashSet<String>) {
    for n in nodes {
        ids.insert(n.id().to_string());
        if let SidebarNode::Folder(f) = n {
            node_ids(&f.children, ids);
        }
    }
}

/// Remove every tab and folder whose id is in `taken` from `nodes`.
fn drop_taken(nodes: &mut Vec<SidebarNode>, taken: &HashSet<String>) {
    nodes.retain(|n| !taken.contains(n.id()));
    for n in nodes.iter_mut() {
        if let SidebarNode::Folder(f) = n {
            drop_taken(&mut f.children, taken);
        }
    }
}

/// Make data from elsewhere (sync, imports) safe to keep: colours finite and
/// in range (a non-finite one would be saved as `null` and make `state.json`
/// unreadable), timestamps no later than tomorrow, smart folder settings in
/// the Settings page's ranges, IDs unique (later duplicates are dropped),
/// profiles with an ID each, and every tab what its place says it is: a
/// favorite or pinned tab has a home page, a Today tab has none. (Without
/// its home page a private pinned tab would be saved where it went.) A
/// bookmark is an address with a name and nothing else, and a bookmark
/// folder is never a smart folder.
pub fn sanitise_snapshot(snap: &mut SidebarSnapshot) {
    /// `kept`: favorites, which keep a home page; Today tabs have none.
    fn tabs(list: &mut Vec<TabItem>, seen: &mut HashSet<String>, latest: UnixSeconds, kept: bool) {
        list.retain_mut(|t| {
            for time in [&mut t.created_at, &mut t.last_active_at] {
                *time = time.map(|v| v.clamp(0, latest));
            }
            if kept {
                t.home_url.get_or_insert_with(|| t.url.clone());
            } else {
                t.home_url = None;
            }
            seen.insert(t.id.clone())
        });
    }
    fn nodes(list: &mut Vec<SidebarNode>, seen: &mut HashSet<String>, latest: UnixSeconds) {
        list.retain_mut(|n| match n {
            SidebarNode::Tab(t) => {
                for time in [&mut t.created_at, &mut t.last_active_at] {
                    *time = time.map(|v| v.clamp(0, latest));
                }
                t.home_url.get_or_insert_with(|| t.url.clone());
                seen.insert(t.id.clone())
            }
            SidebarNode::Folder(f) => {
                if let Some(smart) = &mut f.smart {
                    smart.limit = smart.limit.clamp(1, 100);
                    smart.refresh_minutes = smart.refresh_minutes.clamp(1, 24 * 60);
                }
                nodes(&mut f.children, seen, latest);
                seen.insert(f.id.clone())
            }
        });
    }
    fn bookmarks(list: &mut Vec<SidebarNode>, seen: &mut HashSet<String>, latest: UnixSeconds) {
        list.retain_mut(|n| match n {
            SidebarNode::Tab(t) => {
                t.created_at = t.created_at.map(|v| v.clamp(0, latest));
                t.last_active_at = None;
                t.home_url = None;
                t.profile_id = None;
                t.unread = false;
                t.keep_active = false;
                seen.insert(t.id.clone())
            }
            SidebarNode::Folder(f) => {
                f.smart = None;
                bookmarks(&mut f.children, seen, latest);
                seen.insert(f.id.clone())
            }
        });
    }
    let latest = now() + 24 * 60 * 60;
    let mut seen = HashSet::new();
    for set in &mut snap.favorites {
        tabs(&mut set.items, &mut seen, latest, true);
    }
    let mut profiles = HashSet::new();
    snap.profiles
        .retain(|p| !p.id.is_empty() && profiles.insert(p.id.clone()));
    let mut spaces = HashSet::new();
    snap.spaces.retain(|sp| spaces.insert(sp.id.clone()));
    for space in &mut snap.spaces {
        space.accent = space.accent.map(magnetite_core::Color::sanitised);
        nodes(&mut space.pinned, &mut seen, latest);
        tabs(&mut space.today, &mut seen, latest, false);
        bookmarks(&mut space.bookmarks, &mut seen, latest);
    }
}

impl AppState {
    /// Take what another device synced. The sidebar replaces ours (last writer
    /// wins), but a device that holds the words cannot hurt this one through
    /// it: this device's profiles stay (their cookies and logins go only when
    /// the user deletes the profile here), spaces kept on this device alone
    /// stay untouched (`keep_unsynced`), tabs on screen that the other
    /// device never had stay as Today tabs and keep their privacy, pages of
    /// tabs that still exist stay open, and nothing that arrived loads before
    /// the user opens it. A sidebar without spaces is refused and its version
    /// not counted as applied. Returns the pages to close.
    pub fn adopt_pull(
        &mut self,
        sidebar: Option<SidebarSnapshot>,
        settings: Option<crate::sync::SyncedSettingsPublic>,
        versions: Vec<(String, u64)>,
    ) -> Vec<ContentId> {
        let mut contents = Vec::new();
        let mut refused = false;
        match sidebar {
            Some(remote) if remote.spaces.is_empty() => refused = true,
            Some(remote) => {
                let old_copy = self.holds_unsynced_space(&remote);
                contents = self.adopt_sidebar(remote);
                // What this device would send now is what it took, with the
                // little it keeps of its own: only a change after this is
                // worth writing (`sidebar_for_sync`). Not if the chain still
                // holds a space this device keeps to itself: then it writes,
                // so the space leaves the other devices.
                self.synced_sidebar = (!old_copy).then(|| self.sidebar_for_sync());
            }
            None => {}
        }
        if let Some(st) = settings {
            st.apply(&mut self.settings);
        }
        for (name, v) in versions {
            if refused && name == crate::sync::SIDEBAR {
                continue;
            }
            // A stored version never goes down (rollback protection).
            let known = self.settings.sync.versions.entry(name).or_insert(0);
            *known = (*known).max(v);
        }
        self.settings.sync.last_sync_at = Some(now());
        self.settings.sync.last_error = refused
            .then(|| "Ignored a synced sidebar without spaces; waiting for a usable one.".into());
        contents
    }

    pub(super) fn adopt_sidebar(&mut self, mut remote: SidebarSnapshot) -> Vec<ContentId> {
        sanitise_snapshot(&mut remote);
        drop_unsafe_tabs(&mut remote);
        // A profile deleted on this device is not taken back from the other
        // device's list. One the arriving sidebar uses is: it was put to use
        // there, that sidebar is the one taken, and the deletion is
        // forgotten.
        (self.deleted_profiles).retain(|id| !remote.uses_profile(id));
        (remote.profiles).retain(|p| !self.deleted_profiles.contains(&p.id));
        // What is only on this device stays as it is.
        self.keep_unsynced(&mut remote);
        // The other device's profiles, before this device's are added.
        let theirs: HashSet<String> = remote.profiles.iter().map(|p| p.id.clone()).collect();
        // A profile's privacy is this device's: sync never makes a private
        // profile keep data on disk, nor the other way round. So is which
        // profile is the default one: a private profile named the default
        // would run its pages in the default profile's store on disk.
        for r in &mut remote.profiles {
            match self.sidebar.profiles.iter().find(|p| p.id == r.id) {
                Some(local) => {
                    r.ephemeral = local.ephemeral;
                    r.is_default = local.is_default;
                }
                None => r.is_default = false,
            }
        }
        for p in &self.sidebar.profiles {
            if !remote.profiles.iter().any(|r| r.id == p.id) {
                remote.profiles.push(p.clone());
            }
        }
        // Pages that stay open: what they show and where they run, from this
        // device (another device's address for a tab must not label the page
        // that is loaded here).
        let open: Vec<(String, TabItem, Option<String>, bool)> = self
            .runtime
            .iter()
            .filter(|(_, r)| r.content.is_some())
            .filter_map(|(id, _)| {
                let (_, item) = self.find_tab(id)?;
                Some((
                    id.clone(),
                    item.clone(),
                    self.profile_for_tab(id),
                    self.is_private_tab(id),
                ))
            })
            .collect();
        // Favorites of profiles only this device has stay. For a profile
        // the other device has too, its list counts: none there means the
        // last favorite was removed.
        for set in &self.sidebar.favorites {
            if !theirs.contains(&set.profile_id) {
                remote.favorites.push(set.clone());
            }
        }
        let remote_ids: HashSet<String> = remote.all_tabs().map(|t| t.id.clone()).collect();
        // Tabs the other device never had survive as Today tabs: the one in
        // front in each space (the Settings tab used to join, a page opened
        // meanwhile), and those that are never sent and so cannot come back
        // (private Today tabs, Magnetite's own pages). Each keeps the
        // profile it runs in; a private one stays private.
        let sent: HashSet<String> = (self.sidebar_for_sync().all_tabs())
            .map(|t| t.id.clone())
            .collect();
        let in_front = (self.active_tab.iter()).map(|(space, t)| (space.clone(), t.clone(), true));
        let never_sent = (self.sidebar.all_tabs())
            .filter(|t| !sent.contains(&t.id))
            .map(|t| {
                let space = self
                    .find_tab(&t.id)
                    .and_then(|(sp, _)| sp.map(|sp| sp.id.clone()));
                (
                    space.unwrap_or_else(|| self.active_space.clone()),
                    t.id.clone(),
                    false,
                )
            });
        let mut seen = HashSet::new();
        let keep: Vec<(String, TabItem, Option<String>, bool, bool)> = in_front
            .chain(never_sent)
            .filter(|(_, t, _)| !remote_ids.contains(t) && seen.insert(t.clone()))
            .filter_map(|(space, t, front)| {
                let (_, item) = self.find_tab(&t)?;
                let mut item = item.clone();
                // A Today tab now: it has no home page.
                item.home_url = None;
                Some((
                    space,
                    item,
                    self.profile_for_tab(&t),
                    self.is_private_tab(&t),
                    front,
                ))
            })
            .collect();
        // (space, tab) of the kept tabs that were in front.
        let mut placed: Vec<(String, String)> = Vec::new();
        let mut kept: Vec<String> = Vec::new();
        for (space, mut item, profile, private, front) in keep {
            let pos = remote
                .spaces
                .iter()
                .position(|x| x.id == space)
                .unwrap_or(0);
            if let Some(target) = remote.spaces.get_mut(pos) {
                // Its own profile only where it is not the space's.
                let differs = profile.as_deref() != Some(target.profile_id.as_str());
                item.profile_id = profile.filter(|_| private || differs);
                kept.push(item.id.clone());
                if front {
                    placed.push((target.id.clone(), item.id.clone()));
                }
                target.today.insert(0, item);
            }
        }
        let alive: HashSet<String> = remote_ids.into_iter().chain(kept).collect();
        let mut contents: Vec<ContentId> = self
            .runtime
            .iter()
            .filter(|(id, _)| !alive.contains(*id))
            .filter_map(|(_, r)| r.content)
            .collect();
        self.runtime.retain(|id, _| alive.contains(id));
        self.content_to_tab.retain(|_, t| alive.contains(t));
        self.splits
            .retain(|s| s.ids().iter().all(|t| alive.contains(t)));
        let spaces: HashSet<&str> = remote.spaces.iter().map(|sp| sp.id.as_str()).collect();
        self.active_tab
            .retain(|space, t| spaces.contains(space.as_str()) && alive.contains(t));
        self.active_tab.extend(placed.iter().cloned());
        if !spaces.contains(self.active_space.as_str()) {
            self.active_space = placed
                .first()
                .map(|(sp, _)| sp.clone())
                .or_else(|| remote.spaces.first().map(|sp| sp.id.clone()))
                .unwrap_or_default();
        }
        self.sidebar = remote;
        self.normalise_favorites();
        for (id, here, profile, private) in open {
            if !alive.contains(&id) {
                continue;
            }
            // Moved to another profile, or made private or not, elsewhere:
            // the page closes and opens in that profile when the user opens
            // the tab. A private page never goes on as a lasting one.
            if self.profile_for_tab(&id) != profile || self.is_private_tab(&id) != private {
                contents.extend(self.detach_content(&id));
                // Out of its split too: shown there, it would load by itself.
                self.split_remove(&id);
                continue;
            }
            if let Some(t) = self.find_tab_mut(&id) {
                t.url = here.url;
                if !t.custom_title {
                    t.title = here.title;
                }
            }
        }
        // Nothing that arrived by sync loads by itself: a tab without a page
        // here waits until the user opens it.
        if let Some(t) = self.active_tab_id().map(str::to_owned)
            && self.runtime.get(&t).and_then(|r| r.content).is_none()
        {
            let space = self.active_space.clone();
            self.active_tab.remove(&space);
        }
        contents
    }

    /// Keep space `id` on this device alone (`synced` false), or sync it
    /// like the others again. False if nothing changed.
    pub fn set_space_synced(&mut self, id: &str, synced: bool) -> bool {
        if self.space(id).is_none() {
            return false;
        }
        if synced {
            self.unsynced_spaces.remove(id)
        } else {
            self.unsynced_spaces.insert(id.to_string())
        }
    }

    /// The profiles that stay on this device with the spaces kept here
    /// (`unsynced_spaces`): those that only such spaces use. A profile that
    /// a synced space uses too is needed on the other devices, and the
    /// default profile is on every device.
    pub fn unsynced_profiles(&self) -> BTreeSet<String> {
        let kept = |sp: &&magnetite_core::Space| self.unsynced_spaces.contains(&sp.id);
        let default = self.default_profile_id();
        let synced: HashSet<&str> = (self.sidebar.spaces.iter())
            .filter(|sp| !kept(sp))
            .map(|sp| sp.profile_id.as_str())
            .collect();
        (self.sidebar.spaces.iter())
            .filter(kept)
            .map(|sp| &sp.profile_id)
            .filter(|p| **p != default && !synced.contains(p.as_str()))
            .cloned()
            .collect()
    }

    /// The sidebar as sent to the sync server: what is saved, without the
    /// tabs another device would not take (Magnetite's own pages, local
    /// files). The server then never holds what a device leaves out again,
    /// which would look like a change on every round. And without what is
    /// kept on this device alone: the spaces marked so (`unsynced_spaces`)
    /// and, of the profiles that only they use (`unsynced_profiles`), the
    /// profile, its favorites and tabs elsewhere that run in it.
    pub fn sidebar_for_sync(&self) -> SidebarSnapshot {
        let mut snap = self.lasting_sidebar();
        drop_unsafe_tabs(&mut snap);
        if !self.unsynced_spaces.is_empty() {
            let here = self.unsynced_profiles();
            (snap.spaces).retain(|sp| !self.unsynced_spaces.contains(&sp.id));
            snap.profiles.retain(|p| !here.contains(&p.id));
            snap.favorites.retain(|f| !here.contains(&f.profile_id));
            retain_tabs(&mut snap, &|t| {
                !t.profile_id.as_ref().is_some_and(|p| here.contains(p))
            });
        }
        snap
    }

    /// Whether `remote` still holds a space that this device keeps to
    /// itself: an old copy, written before the space was taken out of sync
    /// here or by a device that has not heard of it.
    fn holds_unsynced_space(&self, remote: &SidebarSnapshot) -> bool {
        (remote.spaces.iter()).any(|sp| self.unsynced_spaces.contains(&sp.id))
    }

    /// Put what this device keeps to itself into `remote`, the sidebar that
    /// is about to replace ours, in place of what `remote` says about it:
    ///
    /// - the spaces kept here (`unsynced_spaces`): ours stay. A copy of one
    ///   in `remote` is old and dropped (`holds_unsynced_space`);
    /// - the profiles that only they use (`unsynced_profiles`) and those
    ///   profiles' favorites: ours count, whatever the other device lists;
    /// - a tab or folder of such a space that `remote` has elsewhere moved
    ///   there.
    ///
    /// Spaces and profiles keep their places in the lists.
    fn keep_unsynced(&self, remote: &mut SidebarSnapshot) {
        let kept = &self.unsynced_spaces;
        if kept.is_empty() {
            return;
        }
        let here = self.unsynced_profiles();
        remote.spaces.retain(|sp| !kept.contains(&sp.id));
        remote.profiles.retain(|p| !here.contains(&p.id));
        remote.favorites.retain(|f| !here.contains(&f.profile_id));
        let mut taken: HashSet<String> = remote.all_tabs().map(|t| t.id.clone()).collect();
        for sp in &remote.spaces {
            node_ids(&sp.pinned, &mut taken);
        }
        for (at, profile) in self.sidebar.profiles.iter().enumerate() {
            if here.contains(&profile.id) {
                let at = at.min(remote.profiles.len());
                remote.profiles.insert(at, profile.clone());
            }
        }
        for list in &self.sidebar.favorites {
            if here.contains(&list.profile_id) {
                let mut list = list.clone();
                list.items.retain(|t| !taken.contains(&t.id));
                remote.favorites.push(list);
            }
        }
        for (at, space) in self.sidebar.spaces.iter().enumerate() {
            if kept.contains(&space.id) {
                let mut space = space.clone();
                drop_taken(&mut space.pinned, &taken);
                space.today.retain(|t| !taken.contains(&t.id));
                remote.spaces.insert(at.min(remote.spaces.len()), space);
            }
        }
    }

    /// The sidebar as saved: Today tabs of private profiles stay in memory,
    /// like their pages (pinned tabs and favorites, which the user keeps on
    /// purpose, stay).
    pub fn lasting_sidebar(&self) -> SidebarSnapshot {
        let mut snap = self.sidebar.clone();
        // Private pinned tabs and favorites keep their home address, not
        // where they went: the page's later addresses and titles are
        // browsing in a private profile.
        let forget = |t: &mut TabItem| {
            if self.is_private_tab(&t.id) {
                if let Some(home) = &t.home_url {
                    t.url.clone_from(home);
                }
                if !t.custom_title {
                    t.title.clone_from(&t.url);
                }
            }
        };
        fn walk(nodes: &mut [SidebarNode], forget: &dyn Fn(&mut TabItem)) {
            for node in nodes {
                match node {
                    SidebarNode::Tab(t) => forget(t),
                    SidebarNode::Folder(f) => walk(&mut f.children, forget),
                }
            }
        }
        for set in &mut snap.favorites {
            set.items.iter_mut().for_each(forget);
        }
        for space in &mut snap.spaces {
            space.today.retain(|t| !self.is_private_tab(&t.id));
            walk(&mut space.pinned, &forget);
        }
        snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnetite_core::{Folder, Profile};

    use crate::state::TabRuntime;
    use crate::state::persist::default_sidebar;
    use crate::state::testing::{state, tab};

    #[test]
    fn private_pinned_tabs_keep_only_their_home_address() {
        let mut s = state();
        s.sidebar.profiles.push(Profile {
            id: "private".into(),
            name: "Private".into(),
            is_default: false,
            ephemeral: true,
        });
        if let SidebarNode::Tab(t) = &mut s.sidebar.spaces[0].pinned[0] {
            t.profile_id = Some("private".into());
            t.url = "https://example.org/p/secret-page".into();
            t.title = "Secret page".into();
        }
        let lasting = s.lasting_sidebar();
        let SidebarNode::Tab(t) = &lasting.spaces[0].pinned[0] else {
            panic!("pinned tab expected");
        };
        assert_eq!(t.url, "https://example.org/p");
        assert_eq!(t.title, "https://example.org/p");
    }

    /// A snapshot as another device would sync it: one space, its own tabs.
    fn remote(tabs: &[&str]) -> SidebarSnapshot {
        let mut snap = default_sidebar();
        snap.spaces[0].today = tabs.iter().map(|t| tab(t)).collect();
        snap
    }

    #[test]
    fn a_pull_keeps_open_pages_this_devices_profiles_and_private_tabs() {
        let mut s = state();
        // A private profile only this device has, and its tab on screen.
        s.sidebar.profiles.push(Profile {
            id: "secret".into(),
            name: "Private".into(),
            is_default: false,
            ephemeral: true,
        });
        let sid = s.active_space.clone();
        s.sidebar.spaces[0].today.push(TabItem {
            profile_id: Some("secret".into()),
            ..tab("p1")
        });
        s.active_tab.insert(sid, "p1".into());
        for (id, page) in [("a", 1), ("p1", 2), ("c", 3)] {
            s.runtime.insert(
                id.into(),
                TabRuntime {
                    content: Some(page),
                    ..Default::default()
                },
            );
        }

        // The other device has "a" and a new "x", but neither "c" nor "p1".
        let close = s.adopt_pull(Some(remote(&["a", "x"])), None, vec![("sidebar".into(), 4)]);

        assert_eq!(
            close,
            vec![3],
            "only the page of the tab that is gone closes"
        );
        assert!(
            s.sidebar.profiles.iter().any(|p| p.id == "secret"),
            "local profile kept"
        );
        assert!(s.is_private_tab("p1"), "kept tab stays private");
        assert!(
            !s.lasting_sidebar().all_tabs().any(|t| t.id == "p1"),
            "and is neither saved nor synced"
        );
        assert_eq!(s.runtime["a"].content, Some(1), "surviving page stays open");
        assert!(!s.runtime.contains_key("x"), "nothing new loads by itself");
        assert_eq!(s.settings.sync.versions["sidebar"], 4);
    }

    #[test]
    fn what_a_device_never_sends_survives_a_pull() {
        let mut s = state();
        s.sidebar.profiles.push(Profile {
            id: "secret".into(),
            name: "Private".into(),
            is_default: false,
            ephemeral: true,
        });
        // Neither in front: a Settings tab and a private Today tab.
        s.sidebar.spaces[0].today.push(TabItem {
            url: "magnetite://ui/settings".into(),
            ..tab("settings")
        });
        s.sidebar.spaces[0].today.push(TabItem {
            profile_id: Some("secret".into()),
            ..tab("hidden")
        });
        for (id, page) in [("settings", 1), ("hidden", 2)] {
            s.runtime.insert(
                id.into(),
                TabRuntime {
                    content: Some(page),
                    ..Default::default()
                },
            );
        }
        let sent = s.sidebar_for_sync();
        assert!(
            !sent
                .all_tabs()
                .any(|t| t.id == "settings" || t.id == "hidden")
        );
        // The chain's sidebar is this device's own, as another device took
        // it and wrote it back: nothing of this device's goes.
        let close = s.adopt_pull(Some(sent), None, vec![]);
        assert!(close.is_empty(), "{close:?}");
        assert!(s.find_tab("settings").is_some() && s.find_tab("hidden").is_some());
        assert!(s.is_private_tab("hidden"));
        // And there is nothing to write: what it would send is what it took.
        assert_eq!(s.synced_sidebar, Some(s.sidebar_for_sync()));
    }

    #[test]
    fn a_private_pinned_tab_never_tells_where_it_went() {
        let mut s = state();
        s.sidebar.profiles.push(Profile {
            id: "secret".into(),
            name: "Private".into(),
            is_default: false,
            ephemeral: true,
        });
        s.sidebar.spaces[0].profile_id = "secret".into();
        if let SidebarNode::Tab(t) = &mut s.sidebar.spaces[0].pinned[0] {
            t.url = "https://example.org/p/secret-page".into();
        }
        s.runtime.insert(
            "p".into(),
            TabRuntime {
                content: Some(1),
                ..Default::default()
            },
        );
        // Another device holding the sync code takes the home page away.
        let mut theirs = s.lasting_sidebar();
        if let SidebarNode::Tab(t) = &mut theirs.spaces[0].pinned[0] {
            t.home_url = None;
        }
        s.adopt_pull(Some(theirs), None, vec![]);
        let saved = serde_json::to_string(&s.lasting_sidebar()).unwrap();
        assert!(!saved.contains("secret-page"), "{saved}");
    }

    #[test]
    fn which_profile_is_the_default_one_is_this_devices() {
        let mut s = state();
        s.sidebar.profiles.push(Profile {
            id: "secret".into(),
            name: "Private".into(),
            is_default: false,
            ephemeral: true,
        });
        let mut theirs = remote(&["a"]);
        theirs.profiles[0].is_default = false;
        theirs.profiles.push(Profile {
            id: "secret".into(),
            name: "Private".into(),
            is_default: true,
            ephemeral: false,
        });
        s.adopt_pull(Some(theirs), None, vec![]);
        // Still a context of its own, in memory: not the default store.
        assert!(s.context(Some("secret")).is_some_and(|c| c.ephemeral));
        assert_eq!(s.default_profile_id(), "default");
    }

    #[test]
    fn a_page_that_must_reload_leaves_its_split() {
        let mut s = state();
        s.sidebar.profiles.push(Profile {
            id: "work".into(),
            name: "Work".into(),
            is_default: false,
            ephemeral: false,
        });
        let space = s.active_space.clone();
        assert!(s.split_insert(Some("a"), "b", magnetite_core::split::Side::Right));
        s.active_tab.insert(space, "a".into());
        for (id, page) in [("a", 1), ("b", 2)] {
            s.runtime.insert(
                id.into(),
                TabRuntime {
                    content: Some(page),
                    ..Default::default()
                },
            );
        }
        // Another device moved "b" to another profile.
        let mut theirs = s.lasting_sidebar();
        let b = theirs.spaces[0]
            .today
            .iter_mut()
            .find(|t| t.id == "b")
            .unwrap();
        b.profile_id = Some("work".into());
        let close = s.adopt_pull(Some(theirs), None, vec![]);
        assert_eq!(close, vec![2]);
        // Shown in the split, it would load in the other profile by itself.
        assert_eq!(s.visible_panes(), vec!["a"]);
    }

    #[test]
    fn a_favorite_removed_elsewhere_does_not_come_back() {
        let mut s = state();
        s.sidebar.profiles.push(Profile {
            id: "work".into(),
            name: "Work".into(),
            is_default: false,
            ephemeral: false,
        });
        s.sidebar.favorites.push(magnetite_core::FavoriteSet {
            profile_id: "work".into(),
            items: vec![TabItem {
                home_url: Some("https://example.org/chat".into()),
                ..tab("chat")
            }],
        });
        // The other device moved the last favorite of "work" into Today.
        let mut theirs = s.lasting_sidebar();
        theirs.favorites.retain(|f| f.profile_id != "work");
        theirs.spaces[0].today.push(tab("chat"));
        s.adopt_pull(Some(theirs), None, vec![]);
        assert!(
            !s.sidebar
                .favorites
                .iter()
                .any(|f| f.items.iter().any(|t| t.id == "chat"))
        );
        assert_eq!(s.sidebar.all_tabs().filter(|t| t.id == "chat").count(), 1);
    }

    #[test]
    fn a_pull_cannot_make_a_private_profile_lasting() {
        let mut s = state();
        let private = |ephemeral| Profile {
            id: "secret".into(),
            name: "Private".into(),
            is_default: false,
            ephemeral,
        };
        s.sidebar.profiles.push(private(true));
        let sid = s.active_space.clone();
        s.sidebar.spaces[0].today.push(TabItem {
            profile_id: Some("secret".into()),
            ..tab("p1")
        });
        s.active_tab.insert(sid, "p1".into());
        s.runtime.insert(
            "p1".into(),
            TabRuntime {
                content: Some(7),
                ..Default::default()
            },
        );
        // Another device holding the sync code says the profile is not private.
        let mut snap = remote(&["a"]);
        snap.profiles.push(private(false));
        let close = s.adopt_pull(Some(snap), None, vec![]);
        assert!(close.is_empty(), "{close:?}");
        assert!(
            s.sidebar
                .profiles
                .iter()
                .any(|p| p.id == "secret" && p.ephemeral)
        );
        assert!(s.is_private_tab("p1"));
        assert!(
            !s.lasting_sidebar().all_tabs().any(|t| t.id == "p1"),
            "the private page is neither saved nor synced"
        );
    }

    #[test]
    fn open_pages_keep_their_address_and_close_when_moved_to_another_profile() {
        let mut s = state();
        s.sidebar.profiles.push(Profile {
            id: "work".into(),
            name: "Work".into(),
            is_default: false,
            ephemeral: false,
        });
        for (id, page) in [("a", 1), ("b", 2)] {
            s.runtime.insert(
                id.into(),
                TabRuntime {
                    content: Some(page),
                    ..Default::default()
                },
            );
        }
        let mut snap = remote(&["a", "b"]);
        // Another device labels the page open here as someone else's site,
        // and moves the other tab to another profile.
        snap.spaces[0].today[0].url = "https://bank.example/login".into();
        snap.spaces[0].today[0].title = "Bank".into();
        snap.spaces[0].today[1].profile_id = Some("work".into());
        let close = s.adopt_pull(Some(snap), None, vec![]);

        let (_, a) = s.find_tab("a").unwrap();
        assert_eq!(
            (a.url.as_str(), a.title.as_str()),
            ("https://example.org/a", "a"),
            "the address and title are the page's"
        );
        assert_eq!(s.runtime["a"].content, Some(1), "and it stays open");
        assert_eq!(close, vec![2], "the moved page closes");
        assert_eq!(s.runtime["b"].content, None);
    }

    #[test]
    fn a_pulled_sidebar_without_spaces_is_refused_and_not_counted() {
        let mut s = state();
        let before = s.sidebar.clone();
        let mut empty = remote(&[]);
        empty.spaces.clear();
        s.adopt_pull(Some(empty), None, vec![("sidebar".into(), 9)]);
        assert_eq!(s.sidebar, before);
        assert!(!s.settings.sync.versions.contains_key("sidebar"));
        assert!(
            s.settings.sync.last_error.is_some(),
            "the refusal stays visible"
        );
    }

    #[test]
    fn foreign_data_is_made_safe_to_save() {
        let mut snap = remote(&["a", "a", "b"]);
        snap.spaces[0].accent = Some(magnetite_core::Color {
            red: f32::INFINITY,
            green: f32::NAN,
            blue: 2.0,
        });
        snap.spaces[0].today[2].last_active_at = Some(i64::MAX);
        sanitise_snapshot(&mut snap);
        let c = snap.spaces[0].accent.unwrap();
        assert_eq!((c.red, c.green, c.blue), (0.5, 0.5, 1.0));
        let ids: Vec<_> = snap.spaces[0].today.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["a", "b"], "duplicate ID dropped");
        assert!(snap.spaces[0].today[1].last_active_at.unwrap() <= now() + 86_400);
        // Saving and reading it back works (no `null` for a colour).
        let json = serde_json::to_string(&snap).unwrap();
        assert!(serde_json::from_str::<SidebarSnapshot>(&json).is_ok());
    }

    #[test]
    fn smart_folders_from_another_device_wait_until_turned_on_here() {
        use magnetite_core::smart::{ReviewShow, SmartFolder, SmartSource};
        let mut s = state();
        s.settings.smart_allowed = Some(vec![]);
        let source = |filter: &str| SmartSource::GitHub {
            host: "github.com".into(),
            show: ReviewShow::PullRequests,
            filter: filter.into(),
        };
        s.sidebar.spaces[0].pinned.push(SidebarNode::Folder(Folder {
            id: "f".into(),
            title: "PRs".into(),
            icon: None,
            children: vec![],
            smart: Some(SmartFolder::new(source(""))),
        }));
        let sf = |s: &AppState| s.smart_folders()[0].1.clone();
        assert!(!s.smart_is_allowed("f", &sf(&s)), "arrived by sync: waits");
        s.allow_smart("f");
        assert!(s.smart_is_allowed("f", &sf(&s)));
        // Another device changes what it asks for: waits again.
        if let SidebarNode::Folder(f) = &mut s.sidebar.spaces[0].pinned[1] {
            f.smart = Some(SmartFolder::new(source("org:secret")));
        }
        assert!(!s.smart_is_allowed("f", &sf(&s)));
    }

    fn work() -> Profile {
        Profile {
            id: "work".into(),
            name: "Work".into(),
            is_default: false,
            ephemeral: false,
        }
    }

    fn lists(s: &AppState, profile: &str) -> bool {
        s.sidebar.profiles.iter().any(|p| p.id == profile)
    }

    #[test]
    fn a_profile_deleted_here_does_not_come_back_and_stays_elsewhere() {
        let (mut a, mut b) = (state(), state());
        a.sidebar.profiles.push(work());
        b.sidebar.profiles.push(work());
        // A deletes the profile and writes; B takes A's sidebar and keeps
        // its own profile: a pull never removes one.
        assert!(a.delete_profile("work"));
        b.adopt_pull(Some(a.sidebar_for_sync()), None, vec![]);
        assert!(lists(&b, "work"));
        assert_eq!(b.deleted_profile_data(), Vec::<String>::new());
        // B writes later, with the profile on its list; A takes that
        // sidebar: the profile is not back, and its data may still go.
        a.adopt_pull(Some(b.sidebar_for_sync()), None, vec![]);
        assert!(!lists(&a, "work"));
        assert_eq!(a.deleted_profile_data(), ["work"]);
        assert_eq!(a.synced_sidebar, Some(a.sidebar_for_sync()), "no news");
    }

    #[test]
    fn a_deleted_profile_returns_with_a_sidebar_that_uses_it() {
        let mut s = state();
        s.sidebar.profiles.push(work());
        assert!(s.delete_profile("work"));
        // Meanwhile the other device gave its space that profile.
        let mut snap = remote(&["a"]);
        snap.profiles.push(work());
        snap.spaces[0].profile_id = "work".into();
        s.adopt_pull(Some(snap), None, vec![]);
        assert!(lists(&s, "work"));
        assert_eq!(s.deleted_profiles, Vec::<String>::new(), "forgotten");
        assert_eq!(s.deleted_profile_data(), Vec::<String>::new());
    }

    /// A state with the profile "work", a space "office" in it between two
    /// others (tab `w1` pinned, `w2` in Today) and a favorite `wf`.
    fn with_office() -> AppState {
        let mut s = state();
        s.sidebar.profiles.push(work());
        let space = |id: &str, profile: &str| magnetite_core::Space {
            id: id.into(),
            title: id.into(),
            profile_id: profile.into(),
            pinned: vec![],
            today: vec![],
            ..s.sidebar.spaces[0].clone()
        };
        let default_id = s.default_profile_id();
        let mut office = space("office", "work");
        office.pinned = vec![SidebarNode::Tab(TabItem {
            home_url: Some("https://example.org/w1".into()),
            ..tab("w1")
        })];
        office.today = vec![tab("w2")];
        let hobby = space("hobby", &default_id);
        s.sidebar.spaces.extend([office, hobby]);
        s.sidebar.favorites.push(magnetite_core::FavoriteSet {
            profile_id: "work".into(),
            items: vec![TabItem {
                home_url: Some("https://example.org/wf".into()),
                ..tab("wf")
            }],
        });
        s
    }

    fn space_ids(snap: &SidebarSnapshot) -> Vec<&str> {
        snap.spaces.iter().map(|sp| sp.id.as_str()).collect()
    }

    #[test]
    fn a_space_kept_on_this_device_is_not_sent() {
        let mut s = with_office();
        // A tab in a synced space that runs in the work profile.
        s.find_tab_mut("c").unwrap().profile_id = Some("work".into());
        let first = s.sidebar.spaces[0].id.clone();
        assert_eq!(
            space_ids(&s.sidebar_for_sync()),
            [&first, "office", "hobby"]
        );
        assert!(!s.set_space_synced("unknown", false));

        assert!(s.set_space_synced("office", false));
        assert!(!s.set_space_synced("office", false), "it is already");
        let sent = s.sidebar_for_sync();
        assert_eq!(space_ids(&sent), [&first, "hobby"]);
        // Only that space used the work profile: it stays here too, with
        // its favorites and the tab that runs in it.
        assert!(sent.profiles.iter().all(|p| p.id != "work"));
        assert!(sent.favorites.iter().all(|f| f.profile_id != "work"));
        let tabs: Vec<&str> = sent.all_tabs().map(|t| t.id.as_str()).collect();
        assert_eq!(tabs, ["p", "a", "b"], "nothing of the work profile");
        // Saved it still is.
        assert_eq!(space_ids(&s.lasting_sidebar()).len(), 3);

        // A space in the default profile: only the space stays.
        assert!(s.set_space_synced("hobby", false));
        let sent = s.sidebar_for_sync();
        assert_eq!(space_ids(&sent), [&first]);
        assert!(sent.profiles.iter().any(|p| p.is_default));
        assert!(s.set_space_synced("hobby", true));

        // A synced space uses the work profile as well: the other devices
        // need the profile, so it is sent again, with its favorites.
        s.sidebar.spaces[2].profile_id = "work".into();
        let sent = s.sidebar_for_sync();
        assert_eq!(space_ids(&sent), [&first, "hobby"]);
        assert!(sent.profiles.iter().any(|p| p.id == "work"));
        assert!(sent.favorites.iter().any(|f| f.profile_id == "work"));
        assert!(sent.all_tabs().any(|t| t.id == "c"));

        // Deleted, the space leaves no mark behind.
        s.delete_space("office");
        assert!(s.unsynced_spaces.is_empty());
    }

    #[test]
    fn a_space_kept_on_one_device_leaves_the_others_and_stays_there() {
        let (mut w, mut p) = (with_office(), with_office());
        assert!(w.set_space_synced("office", false));
        // The other device takes what is sent now: the space is gone there.
        // Its own profile and that profile's favorites stay, as for any
        // profile a sidebar does not name.
        p.adopt_pull(Some(w.sidebar_for_sync()), None, vec![]);
        assert!(!space_ids(&p.sidebar).contains(&"office"));
        assert!(lists(&p, "work"));

        // It writes later, still naming the profile and other favorites
        // for it. This device takes that and keeps what is its own, in place.
        let mut theirs = p.sidebar_for_sync();
        let list = theirs.favorites.iter_mut().find(|f| f.profile_id == "work");
        list.expect("its own list").items = vec![tab("elsewhere")];
        let before = w.sidebar.clone();
        w.adopt_pull(Some(theirs), None, vec![]);
        assert_eq!(space_ids(&w.sidebar), space_ids(&before));
        assert_eq!(w.sidebar.spaces[1], before.spaces[1], "the space as it was");
        assert_eq!(w.sidebar.profiles, before.profiles);
        let favorites: Vec<&str> = (w.sidebar.favorites.iter())
            .filter(|f| f.profile_id == "work")
            .flat_map(|f| f.items.iter().map(|t| t.id.as_str()))
            .collect();
        assert_eq!(
            favorites,
            ["wf"],
            "its own favorites, not the other device's"
        );
        assert_eq!(w.synced_sidebar, Some(w.sidebar_for_sync()), "no news");
    }

    #[test]
    fn an_old_copy_of_a_space_kept_here_is_dropped_and_written_over() {
        let mut w = with_office();
        // Another device wrote before it heard: it still holds the space,
        // with other tabs.
        let mut stale = w.sidebar_for_sync();
        stale.spaces[1].today = vec![tab("old")];
        assert!(w.set_space_synced("office", false));
        w.adopt_pull(Some(stale), None, vec![]);
        let office = &w.sidebar.spaces[1];
        assert_eq!(office.id, "office");
        assert_eq!(office.today.len(), 1);
        assert_eq!(office.today[0].id, "w2", "this device's copy");
        assert_eq!(w.sidebar.spaces.len(), 3, "once");
        assert_eq!(
            w.synced_sidebar, None,
            "it writes, so the others lose the copy"
        );
    }

    #[test]
    fn a_tab_the_chain_moved_out_of_a_space_kept_here_is_where_the_chain_has_it() {
        let mut w = with_office();
        assert!(w.set_space_synced("office", false));
        // Elsewhere the tab `w2` was moved to a synced space before that
        // device heard that the space is kept here.
        let mut theirs = w.sidebar_for_sync();
        theirs.spaces[0].today.push(tab("w2"));
        w.adopt_pull(Some(theirs), None, vec![]);
        assert_eq!(w.sidebar.spaces[1].today, vec![], "moved out");
        assert_eq!(w.sidebar.all_tabs().filter(|t| t.id == "w2").count(), 1);
        assert_eq!(w.sidebar.spaces[1].pinned.len(), 1, "the rest stays");
    }
}
