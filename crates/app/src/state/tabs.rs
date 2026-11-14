//! Tabs in the state: finding them, taking them out, the archive and its
//! retention, and deleting a space.

use std::collections::HashSet;

use magnetite_core::{Id, SidebarNode, Space, TabItem, UnixSeconds};
use magnetite_engine::{ContentId, MemoryPressure};

use super::settings::MAX_ARCHIVE_DAYS;
use super::{AppState, ArchivedTab, Peek, TabRuntime, new_id, now};

pub(super) fn find_in_tree_mut<'a>(
    nodes: &'a mut [SidebarNode],
    id: &str,
) -> Option<&'a mut TabItem> {
    for node in nodes {
        match node {
            SidebarNode::Tab(t) if t.id == id => return Some(t),
            SidebarNode::Folder(f) => {
                if let Some(t) = find_in_tree_mut(&mut f.children, id) {
                    return Some(t);
                }
            }
            SidebarNode::Tab(_) => {}
        }
    }
    None
}

impl AppState {
    /// The tab to show on switching to space `id`: the one last used there if
    /// it is still in that space or a favorite, else the space's first tab.
    pub fn tab_for_space(&self, id: &str) -> Option<Id> {
        let space = self.space(id)?;
        self.active_tab
            .get(id)
            .filter(|t| match self.find_tab(t) {
                Some((None, _)) => true,
                Some((Some(sp), _)) => sp.id == id,
                None => false,
            })
            .cloned()
            .or_else(|| {
                space
                    .pinned
                    .iter()
                    .flat_map(SidebarNode::tabs)
                    .chain(space.today.iter())
                    .next()
                    .map(|t| t.id.clone())
            })
    }

    /// Take a tab's content window away (it will be recreated on demand);
    /// returns it for the caller to close (`TabRuntime::unload`).
    pub fn detach_content(&mut self, tab_id: &str) -> Option<ContentId> {
        let c = self.runtime.get_mut(tab_id)?.unload()?;
        self.content_to_tab.remove(&c);
        Some(c)
    }

    /// Find a tab anywhere (favorites, any space) with the space it belongs to.
    pub fn find_tab(&self, id: &str) -> Option<(Option<&Space>, &TabItem)> {
        for f in &self.sidebar.favorites {
            if let Some(t) = f.items.iter().find(|t| t.id == id) {
                return Some((None, t));
            }
        }
        for s in &self.sidebar.spaces {
            if let Some(t) = s.find_tab(id) {
                return Some((Some(s), t));
            }
        }
        None
    }

    /// The page outside the sidebar in window `id`: the peek, or a mini
    /// window's page.
    pub fn page_outside(&self, id: ContentId) -> Option<&Peek> {
        (self.peek.iter())
            .chain(self.minis.iter())
            .find(|p| p.content == id)
    }

    pub fn page_outside_mut(&mut self, id: ContentId) -> Option<&mut Peek> {
        (self.peek.iter_mut())
            .chain(self.minis.iter_mut())
            .find(|p| p.content == id)
    }

    /// What is known about the page in window `id`: the peek's or a mini
    /// window's while the page is one, else its tab's. Page events are
    /// recorded here, so a page that is kept as a tab knows what it was
    /// doing.
    pub fn page_runtime(&mut self, id: ContentId) -> Option<&mut TabRuntime> {
        if self.page_outside(id).is_some() {
            return self.page_outside_mut(id).map(|p| &mut p.runtime);
        }
        let tab = self.content_to_tab.get(&id)?.clone();
        Some(self.runtime.entry(tab).or_default())
    }

    /// Turn the peek into a Today tab of the active space, reusing its
    /// window and what is known about its page. Returns the tab's id.
    pub fn keep_peek(&mut self) -> Option<Id> {
        let p = self.peek.take()?;
        let space = self.active_space.clone();
        Some(self.keep_page(p, &space))
    }

    /// Turn the mini window's page in window `content` into a Today tab of
    /// space `space` (the active space if there is no such space), like a
    /// kept peek. Returns the tab's id.
    pub fn keep_mini(&mut self, content: ContentId, space: &str) -> Option<Id> {
        let pos = self.minis.iter().position(|m| m.content == content)?;
        let page = self.minis.remove(pos);
        let space = if self.space(space).is_some() {
            space.to_string()
        } else {
            self.active_space.clone()
        };
        Some(self.keep_page(page, &space))
    }

    /// Page `p` becomes a Today tab at the top of space `space`: its window
    /// and what is known about it carry over, and it stays in its profile
    /// (its page already runs there), also when that is not the space's.
    fn keep_page(&mut self, p: Peek, space: &str) -> Id {
        let id = self.new_today_tab(space, &p.url, &p.title);
        let space_profile = self.space(space).map(|sp| sp.profile_id.clone());
        if space_profile.as_deref() != Some(p.profile.as_str())
            && let Some(t) = self.find_tab_mut(&id)
        {
            t.profile_id = Some(p.profile.clone());
        }
        self.content_to_tab.insert(p.content, id.clone());
        self.runtime.insert(id.clone(), p.runtime);
        id
    }

    pub fn find_tab_mut(&mut self, id: &str) -> Option<&mut TabItem> {
        for f in &mut self.sidebar.favorites {
            if let Some(t) = f.items.iter_mut().find(|t| t.id == id) {
                return Some(t);
            }
        }
        for s in &mut self.sidebar.spaces {
            if let Some(t) = s.today.iter_mut().find(|t| t.id == id) {
                return Some(t);
            }
            if let Some(t) = find_in_tree_mut(&mut s.pinned, id) {
                return Some(t);
            }
        }
        None
    }

    /// Ordered tabs of the active space as shown: pinned (depth first) then Today.
    pub fn visible_tab_ids(&self) -> Vec<Id> {
        let s = self.active_space();
        s.pinned
            .iter()
            .flat_map(SidebarNode::tabs)
            .chain(s.today.iter())
            .map(|t| t.id.clone())
            .collect()
    }

    /// The tab `Ctrl+Tab` (`forward`) or `Ctrl+Shift+Tab` goes to: the next
    /// or previous row of `visible_tab_ids`, wrapping around. From a tab
    /// with no row there (a favorite), the first or the last row.
    pub fn cycle_tab(&self, forward: bool) -> Option<Id> {
        let order = self.visible_tab_ids();
        let n = order.len();
        let pos = (self.active_tab_id()).and_then(|a| order.iter().position(|t| t == a));
        let next = match (pos, forward) {
            (Some(p), true) => (p + 1) % n,
            (Some(p), false) => (p + n - 1) % n,
            (None, true) => 0,
            (None, false) => n.checked_sub(1)?,
        };
        order.get(next).cloned()
    }

    /// Remove a tab from wherever it is. Returns it and its space (None for favorites).
    pub fn take_tab(&mut self, id: &str) -> Option<(TabItem, Option<Id>)> {
        for f in &mut self.sidebar.favorites {
            if let Some(pos) = f.items.iter().position(|t| t.id == id) {
                return Some((f.items.remove(pos), None));
            }
        }
        for s in &mut self.sidebar.spaces {
            if let Some(pos) = s.today.iter().position(|t| t.id == id) {
                return Some((s.today.remove(pos), Some(s.id.clone())));
            }
            if let Some(SidebarNode::Tab(t)) =
                magnetite_core::sidebar::remove_from_tree(&mut s.pinned, id)
            {
                return Some((t, Some(s.id.clone())));
            }
        }
        None
    }

    pub fn new_today_tab(&mut self, space_id: &str, url: &str, title: &str) -> Id {
        let id = new_id();
        let tab = TabItem {
            created_at: Some(now()),
            last_active_at: Some(now()),
            ..TabItem::new(id.clone(), url, title)
        };
        if let Some(s) = self.space_mut(space_id) {
            s.today.insert(0, tab);
        }
        id
    }

    /// Whether tab `id` has media, playing or paused: such a tab is never
    /// unloaded or archived in the background, which would stop the music.
    fn has_media(&self, id: &str) -> bool {
        self.runtime.get(id).is_some_and(|r| r.media.is_some())
    }

    /// The Today tabs to archive now: out of sight for longer than the
    /// auto-archive period (counted from creation if never shown). Not the
    /// tabs in `active` (on screen, or the one a space returns to), not
    /// those with media, and not those kept active.
    pub fn stale_today_tabs(&self, now: UnixSeconds, active: &HashSet<Id>) -> Vec<Id> {
        let period = i64::from(self.settings.auto_archive_hours) * 3600;
        (self.sidebar.spaces.iter())
            .flat_map(|space| space.today.iter())
            .filter(|t| !t.keep_active && !active.contains(&t.id) && !self.has_media(&t.id))
            .filter(|t| {
                now.saturating_sub(t.last_active_at.or(t.created_at).unwrap_or(now)) > period
            })
            .map(|t| t.id.clone())
            .collect()
    }

    /// The tabs whose page is to be unloaded now: out of sight for longer
    /// than the unload time of the settings (0: never). Only Today tabs, and
    /// the same ones are spared as in `stale_today_tabs`: favorites and
    /// pinned tabs are the pages one comes back to all day, so they stay
    /// ready.
    pub fn tabs_to_unload(&self, now: UnixSeconds, active: &HashSet<Id>) -> Vec<Id> {
        let limit = match self.settings.unload_minutes {
            0 => return Vec::new(),
            minutes => i64::from(minutes) * 60,
        };
        (self.idle_pages(now, active).into_iter())
            .filter(|(_, last_active)| now - last_active > limit)
            .map(|(id, _)| id)
            .collect()
    }

    /// The tabs whose page is to be unloaded because the system is short of
    /// memory, whatever the unload time of the settings: at a warning the
    /// half that was out of sight the longest (at least one), at the
    /// critical level all that `tabs_to_unload` could ever pick.
    pub fn tabs_to_unload_under_pressure(
        &self,
        level: MemoryPressure,
        now: UnixSeconds,
        active: &HashSet<Id>,
    ) -> Vec<Id> {
        let mut idle = self.idle_pages(now, active);
        idle.sort_by_key(|(_, last_active)| *last_active);
        let count = match level {
            MemoryPressure::Warning => idle.len().div_ceil(2),
            MemoryPressure::Critical => idle.len(),
        };
        idle.into_iter().take(count).map(|(id, _)| id).collect()
    }

    /// The tabs with a page that may be unloaded, each with the time it was
    /// last on screen (`now` if never).
    fn idle_pages(&self, now: UnixSeconds, active: &HashSet<Id>) -> Vec<(Id, UnixSeconds)> {
        (self.runtime.iter())
            .filter(|(id, r)| r.content.is_some() && !active.contains(*id) && !self.has_media(id))
            .filter_map(|(id, _)| {
                let tab = (self.sidebar.spaces.iter())
                    .flat_map(|space| space.today.iter())
                    .find(|t| t.id == *id)?;
                (!tab.keep_active).then(|| (id.clone(), tab.last_active_at.unwrap_or(now)))
            })
            .collect()
    }

    /// The pages never archived or unloaded by themselves: those on screen
    /// (all of a split's) and the tab each other space returns to.
    pub fn pages_in_use(&self) -> HashSet<Id> {
        let mut active: HashSet<Id> = self.active_tab.values().cloned().collect();
        active.extend(self.visible_panes());
        active
    }

    /// Close tab `id` into the archive. Tabs of private profiles just close
    /// (nothing of them is kept), and so do Magnetite's own pages (Settings,
    /// the welcome page): there is nothing in them to come back to. Returns
    /// its page window, if it had one, for the caller to close.
    pub fn archive_tab(&mut self, id: &str) -> Option<ContentId> {
        let private = self.is_private_tab(id);
        if let Some((mut tab, space)) = self.take_tab(id)
            && !private
            && !tab.url.starts_with("magnetite://")
        {
            // It comes back as a Today tab, which has no home page (a pinned
            // tab gets here when its space is deleted), and as one that may
            // sleep: closing it ended "Keep active".
            tab.home_url = None;
            tab.keep_active = false;
            self.archive.push(ArchivedTab {
                tab,
                space_id: space.unwrap_or_else(|| self.active_space.clone()),
                archived_at: now(),
            });
        }
        self.forget_tab(id)
    }

    /// Take the archive record at `pos` back into Today of its space (the
    /// active space if that one is gone). Returns the tab's id. A tab that
    /// is in the sidebar again (a sync brought it back) is not added a second
    /// time, and a profile of its own that no longer exists is dropped: with
    /// it the tab would count as private and be gone at the next start.
    pub fn restore_archived(&mut self, pos: usize) -> Option<Id> {
        if pos >= self.archive.len() {
            return None;
        }
        let ArchivedTab {
            mut tab, space_id, ..
        } = self.archive.remove(pos);
        let id = tab.id.clone();
        if self.find_tab(&id).is_some() {
            return Some(id);
        }
        let known = |p: &Id| self.sidebar.profiles.iter().any(|x| &x.id == p);
        if tab.profile_id.as_ref().is_some_and(|p| !known(p)) {
            tab.profile_id = None;
        }
        let space = if self.space(&space_id).is_some() {
            space_id
        } else {
            self.active_space.clone()
        };
        self.space_mut(&space)?.today.insert(0, tab);
        Some(id)
    }

    /// Delete archived tabs older than the retention period
    /// (`Settings::archive_days`). True if any went.
    pub fn prune_archive(&mut self, now: UnixSeconds) -> bool {
        let keep = i64::from(self.settings.archive_days.clamp(1, MAX_ARCHIVE_DAYS)) * 86_400;
        let before = self.archive.len();
        self.archive
            .retain(|a| now.saturating_sub(a.archived_at) <= keep);
        self.archive.len() != before
    }

    /// Drop what is kept about a tab that left the sidebar: its page, its
    /// place in a split and as a space's active tab. Returns its page window
    /// for the caller to close.
    pub(super) fn forget_tab(&mut self, id: &str) -> Option<ContentId> {
        let content = self.runtime.remove(id).and_then(|r| r.content);
        if let Some(c) = content {
            self.content_to_tab.remove(&c);
        }
        self.split_remove(id);
        for tabs in self.active_tab.values_mut() {
            if tabs == id {
                tabs.clear();
            }
        }
        self.active_tab.retain(|_, t| !t.is_empty());
        content
    }

    /// Delete space `id` (never the last one). Its tabs go to the archive
    /// like closed ones (`archive_tab`, so a private profile's leave nothing
    /// behind). Returns the page windows to close.
    pub fn delete_space(&mut self, id: &str) -> Vec<ContentId> {
        if self.sidebar.spaces.len() <= 1 {
            return vec![];
        }
        let Some(space) = self.space(id) else {
            return vec![];
        };
        let tabs: Vec<Id> = space
            .pinned
            .iter()
            .flat_map(SidebarNode::tabs)
            .chain(space.today.iter())
            .map(|t| t.id.clone())
            .collect();
        let contents = tabs.iter().filter_map(|t| self.archive_tab(t)).collect();
        self.sidebar.spaces.retain(|sp| sp.id != id);
        self.unsynced_spaces.remove(id);
        self.active_tab.remove(id);
        if self.active_space == id {
            self.active_space = self.sidebar.spaces[0].id.clone();
        }
        // A private profile made for this space has nothing left to do.
        self.prune_private_profiles();
        contents
    }

    /// Put space `id` at `index` of the spaces, counted without it (past the
    /// end: last). The current space stays the current one.
    pub fn move_space(&mut self, id: &str, index: usize) {
        let spaces = &mut self.sidebar.spaces;
        if let Some(from) = spaces.iter().position(|sp| sp.id == id) {
            let space = spaces.remove(from);
            spaces.insert(index.min(spaces.len()), space);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::{state, tab};

    #[test]
    fn tab_cycling_wraps_and_starts_at_the_ends_from_a_favorite() {
        let mut s = state();
        // Rows: pinned p, then Today a, b, c; b is active.
        assert_eq!(s.cycle_tab(true).as_deref(), Some("c"));
        assert_eq!(s.cycle_tab(false).as_deref(), Some("a"));
        let space = s.active_space.clone();
        s.active_tab.insert(space.clone(), "c".into());
        assert_eq!(s.cycle_tab(true).as_deref(), Some("p"), "wraps around");
        s.sidebar.favorites[0].items.push(tab("fav"));
        s.active_tab.insert(space, "fav".into());
        assert_eq!(s.cycle_tab(true).as_deref(), Some("p"));
        assert_eq!(s.cycle_tab(false).as_deref(), Some("c"));
    }

    #[test]
    fn switching_space_shows_its_last_tab_only_if_it_is_still_there() {
        let mut s = state();
        let first = s.active_space.clone();
        s.sidebar.spaces.push(Space {
            id: "two".into(),
            pinned: vec![],
            today: vec![tab("t")],
            ..s.sidebar.spaces[0].clone()
        });
        s.sidebar.favorites[0].items.push(tab("fav"));
        assert_eq!(s.tab_for_space("two").as_deref(), Some("t"), "first tab");
        s.active_tab.insert("two".into(), "fav".into());
        assert_eq!(s.tab_for_space("two").as_deref(), Some("fav"));
        // "a" lives in the first space: showing it would switch back there.
        s.active_tab.insert("two".into(), "a".into());
        assert_eq!(s.tab_for_space("two").as_deref(), Some("t"));
        assert_eq!(s.tab_for_space(&first).as_deref(), Some("b"));
        assert_eq!(s.tab_for_space("gone"), None);
    }

    #[test]
    fn moving_a_space_counts_the_index_without_it() {
        let mut s = state();
        let first = s.active_space.clone();
        for id in ["two", "three"] {
            s.sidebar.spaces.push(Space {
                id: id.into(),
                pinned: vec![],
                today: vec![],
                ..s.sidebar.spaces[0].clone()
            });
        }
        let order = |s: &AppState| -> Vec<String> {
            s.sidebar.spaces.iter().map(|sp| sp.id.clone()).collect()
        };
        s.move_space(&first, 1);
        assert_eq!(order(&s), ["two", first.as_str(), "three"]);
        s.move_space("three", 0);
        assert_eq!(order(&s), ["three", "two", first.as_str()]);
        s.move_space("three", 9);
        assert_eq!(order(&s), ["two", first.as_str(), "three"], "past the end");
        s.move_space("gone", 0);
        assert_eq!(order(&s), ["two", first.as_str(), "three"]);
        assert_eq!(s.active_space, first, "the current space stays");
    }

    #[test]
    fn a_kept_peek_knows_what_its_page_was_doing() {
        let mut s = state();
        s.peek = Some(crate::state::Peek {
            content: 7,
            url: "https://example.org/video".into(),
            title: "Video".into(),
            runtime: TabRuntime {
                content: Some(7),
                loaded: true,
                ..TabRuntime::default()
            },
            profile: s.active_space().profile_id.clone(),
        });
        // Events of the peek's page go to the peek, not to a tab.
        let rt = s.page_runtime(7).expect("the peek");
        rt.can_go_back = true;
        rt.camera = true;
        assert!(s.runtime.values().all(|r| !r.can_go_back));
        assert!(s.page_runtime(8).is_none(), "no such page");

        let id = s.keep_peek().expect("a tab");
        assert!(s.peek.is_none());
        let kept = &s.runtime[&id];
        assert_eq!(kept.content, Some(7));
        assert!(kept.loaded && kept.can_go_back && kept.camera);
        // From now on the page's events go to the tab.
        s.page_runtime(7).expect("the tab").can_go_back = false;
        assert!(!s.runtime[&id].can_go_back);
    }

    #[test]
    fn a_mini_window_becomes_a_tab_of_the_chosen_space_with_its_page() {
        let mut s = state();
        let first = s.active_space.clone();
        s.sidebar.spaces.push(Space {
            id: "work".into(),
            pinned: vec![],
            today: vec![],
            profile_id: "work-profile".into(),
            ..s.sidebar.spaces[0].clone()
        });
        let page = |content| Peek {
            content,
            url: format!("https://example.org/{content}"),
            title: "Mini".into(),
            runtime: TabRuntime {
                content: Some(content),
                loaded: true,
                ..TabRuntime::default()
            },
            profile: s.active_space().profile_id.clone(),
        };
        s.minis = vec![page(7), page(8)];
        // Events of a mini window's page go to it, not to a tab.
        s.page_runtime(8).expect("the mini window").can_go_back = true;
        assert!(s.runtime.values().all(|r| !r.can_go_back));

        let id = s.keep_mini(8, "work").expect("a tab");
        assert_eq!(s.minis.len(), 1, "the other mini window stays");
        assert_eq!(s.sidebar.spaces[1].today[0].id, id, "top of Today in work");
        let tab = &s.sidebar.spaces[1].today[0];
        assert_eq!(tab.url, "https://example.org/8");
        // Its page runs in the first space's profile and stays there.
        assert_eq!(
            tab.profile_id.as_deref(),
            Some(s.sidebar.spaces[0].profile_id.as_str())
        );
        assert_eq!(s.content_to_tab[&8], id);
        assert!(s.runtime[&id].can_go_back && s.runtime[&id].content == Some(8));

        // A space that is gone: the active one takes it, in its own profile.
        let id = s.keep_mini(7, "gone").expect("a tab");
        assert_eq!(s.sidebar.spaces[0].today[0].id, id);
        assert_eq!(s.sidebar.spaces[0].id, first);
        assert_eq!(s.find_tab(&id).unwrap().1.profile_id, None);
        assert_eq!(s.keep_mini(7, &first), None, "no such mini window any more");
    }

    #[test]
    fn only_today_tabs_not_kept_active_unload_or_archive_by_themselves() {
        let mut s = state();
        s.settings.auto_archive_hours = 12;
        s.settings.unload_minutes = 30;
        let long_ago = now() - 2 * 24 * 3600;
        // Three Today tabs with a page, none looked at for two days.
        for (id, page) in [("a", 1), ("b", 2), ("c", 3)] {
            s.find_tab_mut(id).unwrap().last_active_at = Some(long_ago);
            let runtime = TabRuntime {
                content: Some(page),
                loaded: true,
                ..TabRuntime::default()
            };
            s.runtime.insert(id.into(), runtime);
        }
        // `b` is the one its space returns to; `c` is kept active.
        let active: HashSet<Id> = HashSet::from(["b".to_string()]);
        s.find_tab_mut("c").unwrap().keep_active = true;
        assert_eq!(s.stale_today_tabs(now(), &active), ["a"]);
        assert_eq!(s.tabs_to_unload(now(), &active), ["a"]);
        // Let go again, it goes the way of the others.
        s.find_tab_mut("c").unwrap().keep_active = false;
        let mut unload = s.tabs_to_unload(now(), &active);
        unload.sort();
        assert_eq!(unload, ["a", "c"]);
        // Favorites and pinned tabs stay loaded without being asked to.
        s.sidebar.favorites[0].items.push(TabItem {
            last_active_at: Some(long_ago),
            ..tab("fav")
        });
        s.sidebar.spaces[0].pinned.push(SidebarNode::Tab(TabItem {
            last_active_at: Some(long_ago),
            ..tab("pin")
        }));
        for (id, page) in [("fav", 4), ("pin", 5)] {
            let runtime = TabRuntime {
                content: Some(page),
                loaded: true,
                ..TabRuntime::default()
            };
            s.runtime.insert(id.into(), runtime);
        }
        let mut unload = s.tabs_to_unload(now(), &active);
        unload.sort();
        assert_eq!(unload, ["a", "c"]);
        // "Never unload" in Settings spares every page.
        s.settings.unload_minutes = 0;
        assert_eq!(s.tabs_to_unload(now(), &active), Vec::<Id>::new());
        // A system short of memory does not: first the page unused the
        // longest goes, then all that are out of sight.
        s.find_tab_mut("c").unwrap().last_active_at = Some(long_ago - 60);
        let warned = s.tabs_to_unload_under_pressure(MemoryPressure::Warning, now(), &active);
        assert_eq!(warned, ["c"]);
        let critical = s.tabs_to_unload_under_pressure(MemoryPressure::Critical, now(), &active);
        assert_eq!(critical, ["c", "a"]);
        // Closing a kept tab ends it: back from the archive it may sleep.
        s.find_tab_mut("c").unwrap().keep_active = true;
        s.archive_tab("c");
        assert!(!s.archive.last().unwrap().tab.keep_active);
    }

    #[test]
    fn magnetites_own_pages_close_without_a_trace() {
        let mut s = state();
        s.sidebar.spaces[0]
            .today
            .push(TabItem::new("w", "magnetite://ui/welcome", "Welcome"));
        s.archive_tab("w");
        assert!(s.find_tab("w").is_none());
        assert_eq!(s.archive.len(), 0);
    }

    #[test]
    fn archive_removes_tab_and_clears_active_pointer() {
        let mut s = state();
        s.archive_tab("b");
        assert_eq!(s.archive.len(), 1);
        assert!(s.find_tab("b").is_none());
        assert!(s.active_tab_id().is_none());
        assert_eq!(s.visible_tab_ids(), vec!["p", "a", "c"]);
    }

    #[test]
    fn a_restored_tab_is_never_in_the_sidebar_twice() {
        let mut s = state();
        s.archive_tab("a");
        // A sync brought the tab back while it was in the archive.
        s.sidebar.spaces[0].today.push(tab("a"));
        assert_eq!(s.restore_archived(0).as_deref(), Some("a"));
        let today = &s.sidebar.spaces[0].today;
        assert_eq!(today.iter().filter(|t| t.id == "a").count(), 1);
        assert!(s.archive.is_empty());
        assert_eq!(s.restore_archived(0), None, "nothing left to restore");
    }

    #[test]
    fn archived_tabs_are_deleted_after_the_retention_period() {
        let mut s = state();
        s.settings.archive_days = 30;
        let day = 86_400;
        for (id, age_days) in [("old", 31), ("edge", 30), ("new", 1)] {
            s.archive.push(ArchivedTab {
                tab: tab(id),
                space_id: s.active_space.clone(),
                archived_at: 100 * day - age_days * day,
            });
        }
        assert!(s.prune_archive(100 * day));
        let kept: Vec<&str> = s.archive.iter().map(|a| a.tab.id.as_str()).collect();
        assert_eq!(kept, ["edge", "new"]);
        assert!(!s.prune_archive(100 * day), "nothing more to delete");
    }

    #[test]
    fn a_pinned_tab_of_a_deleted_space_comes_back_as_a_today_tab() {
        let mut s = state();
        s.archive_tab("p");
        let entry = s.archive.pop().unwrap();
        assert_eq!(entry.tab.id, "p");
        assert_eq!(
            entry.tab.home_url, None,
            "restored, it must not count as pinned"
        );
    }

    #[test]
    fn take_tab_finds_pinned_and_today_tabs() {
        let mut s = state();
        let (t, space) = s.take_tab("p").unwrap();
        assert_eq!(t.id, "p");
        assert_eq!(space.as_deref(), Some(s.active_space.as_str()));
        assert!(s.take_tab("p").is_none());
        assert!(s.take_tab("a").is_some());
    }
}
