//! Several windows on one sidebar. Every window shows the same spaces and
//! tabs; what differs is the space and tab in front, the peek, the web
//! panel and whether the sidebar is out. `AppState` holds these for one window at a time, the
//! current one (`AppState::window`), in its own fields; the other windows'
//! wait in `AppState::views`. A tab is on screen in one window at most: the
//! window that shows it takes it from the others (`claim_panes`).

use std::collections::HashMap;

use magnetite_core::Id;
use magnetite_engine::WindowId;
use serde::{Deserialize, Serialize};

use super::{AppState, Peek};

/// What a window that is not the current one shows.
#[derive(Debug)]
pub struct View {
    pub active_space: Id,
    pub active_tab: HashMap<Id, Id>,
    pub peek: Option<Peek>,
    pub web_panel: Option<Id>,
    pub sidebar_visible: bool,
}

/// A window other than the current one as `state.json` keeps it: what
/// it shows, for opening it again at the next start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedWindow {
    pub active_space: Id,
    #[serde(default)]
    pub active_tab: HashMap<Id, Id>,
}

impl AppState {
    /// Swap the current window's view with `view`.
    fn swap_view(&mut self, view: &mut View) {
        std::mem::swap(&mut self.active_space, &mut view.active_space);
        std::mem::swap(&mut self.active_tab, &mut view.active_tab);
        std::mem::swap(&mut self.peek, &mut view.peek);
        std::mem::swap(&mut self.web_panel, &mut view.web_panel);
        std::mem::swap(&mut self.sidebar_visible, &mut view.sidebar_visible);
    }

    /// Make `win` the current window. False, and nothing changes, if it is
    /// the current one already or there is no such window.
    pub fn enter(&mut self, win: WindowId) -> bool {
        let Some(mut view) = self.views.remove(&win) else {
            return false;
        };
        self.swap_view(&mut view);
        let before = std::mem::replace(&mut self.window, win);
        self.views.insert(before, view);
        self.repair_view();
        true
    }

    /// While a window waited, its space or front tabs may have gone (a
    /// deleted space, a sidebar from sync): it keeps what is left, the
    /// first space for a space.
    fn repair_view(&mut self) {
        if !self
            .sidebar
            .spaces
            .iter()
            .any(|s| s.id == self.active_space)
        {
            self.active_space.clone_from(&self.sidebar.spaces[0].id);
        }
        let mut active_tab = std::mem::take(&mut self.active_tab);
        active_tab.retain(|_, tab| self.find_tab(tab).is_some());
        self.active_tab = active_tab;
    }

    /// A new window `win`, which is the current one from here on: the same
    /// space in front, no tab yet; the space beside it for the private
    /// space, which stays in the window it is listed in.
    pub fn open_window(&mut self, win: WindowId) {
        if win == self.window || self.views.contains_key(&win) {
            return;
        }
        let mut view = View {
            active_space: self.lasting_space(&self.active_space),
            active_tab: HashMap::new(),
            peek: None,
            web_panel: None,
            sidebar_visible: true,
        };
        self.swap_view(&mut view);
        let before = std::mem::replace(&mut self.window, win);
        self.views.insert(before, view);
    }

    /// Window `win` closed. If it was the current one, `next` is now.
    pub fn close_window(&mut self, win: WindowId, next: WindowId) {
        if win == self.window {
            // Its view goes with it.
            if self.enter(next) {
                self.views.remove(&win);
            }
        } else {
            self.views.remove(&win);
        }
    }

    /// Every window: the current one and the others.
    pub fn windows(&self) -> Vec<WindowId> {
        let mut ids: Vec<WindowId> = self.views.keys().copied().collect();
        ids.push(self.window);
        ids.sort_unstable();
        ids
    }

    /// The windows as `state.json` keeps them: the one the next start
    /// begins with, which is the current one, and the others, with those
    /// not opened again yet. The private space's window of its own is not
    /// among them, as it ends with the run: if it is the current one, the
    /// first other window begins the next start.
    pub(super) fn lasting_windows(&self) -> (SavedWindow, Vec<SavedWindow>) {
        let saved = |active_space: &Id, active_tab: &HashMap<Id, Id>| SavedWindow {
            active_space: self.lasting_space(active_space),
            active_tab: active_tab.clone(),
        };
        let own = self.private_window();
        let mut ids: Vec<&WindowId> = (self.views.keys()).filter(|id| Some(**id) != own).collect();
        ids.sort_unstable();
        let mut others: Vec<SavedWindow> = (ids.into_iter().map(|id| &self.views[id]))
            .map(|v| saved(&v.active_space, &v.active_tab))
            .collect();
        let first = if own == Some(self.window) && !others.is_empty() {
            others.remove(0)
        } else {
            saved(&self.active_space, &self.active_tab)
        };
        others.extend(self.reopen.iter().cloned());
        (first, others)
    }

    /// Window `win`, just made, shows what `saved` showed. It is the
    /// current one from here on.
    pub fn reopen_window(&mut self, win: WindowId, saved: SavedWindow) {
        self.open_window(win);
        if self.window == win {
            self.active_space = saved.active_space;
            self.active_tab = saved.active_tab;
        }
    }

    /// Saved windows that name a space or tab that is gone keep only what
    /// is left: the first space for a space.
    pub(super) fn check_saved_windows(&mut self) {
        let first = self.sidebar.spaces[0].id.clone();
        let mut reopen = std::mem::take(&mut self.reopen);
        for w in &mut reopen {
            if !self.sidebar.spaces.iter().any(|s| s.id == w.active_space) {
                w.active_space.clone_from(&first);
            }
            w.active_tab.retain(|_, tab| self.find_tab(tab).is_some());
        }
        self.reopen = reopen;
    }

    /// The tabs the current window shows leave the other windows: one that
    /// showed any of them (alone or in a split) has no tab in front of its
    /// space any more. Returns those windows.
    pub fn claim_panes(&mut self) -> Vec<WindowId> {
        let mine = self.visible_panes();
        let here = self.window;
        let mut robbed = Vec::new();
        for win in self.windows() {
            if win == here || !self.enter(win) {
                continue;
            }
            if self.visible_panes().iter().any(|t| mine.contains(t)) {
                let space = self.active_space.clone();
                self.active_tab.remove(&space);
                robbed.push(win);
            }
        }
        self.enter(here);
        robbed
    }
}

#[cfg(test)]
mod tests {
    use magnetite_core::Space;
    use magnetite_core::split::Side;

    use super::super::AppState;
    use super::super::testing::{state, tab};

    #[test]
    fn a_new_window_starts_in_the_same_space_without_a_tab() {
        let mut s = state();
        let space = s.active_space.clone();
        assert!(s.active_tab_id().is_some());
        s.open_window(2);
        assert_eq!(s.window, 2);
        assert_eq!(s.active_space, space);
        assert_eq!(s.active_tab_id(), None);
        // The first window kept its tab.
        assert!(s.enter(1));
        assert!(s.active_tab_id().is_some());
    }

    #[test]
    fn a_tab_shown_in_one_window_leaves_the_other() {
        let mut s = state();
        let tab = s.active_tab_id().expect("a tab in front").to_string();
        s.open_window(2);
        let space = s.active_space.clone();
        s.active_tab.insert(space, tab.clone());
        assert_eq!(s.claim_panes(), vec![1]);
        // Still the second window, which shows the tab; the first shows none.
        assert_eq!(s.window, 2);
        assert_eq!(s.visible_panes(), vec![tab]);
        s.enter(1);
        assert_eq!(s.visible_panes(), Vec::<String>::new());
    }

    #[test]
    fn pages_on_screen_in_a_background_window_stay_in_use() {
        let mut s = state();
        assert!(s.split_insert(None, "c", Side::Right));
        let shown = s.visible_panes();
        s.open_window(2);
        let in_use = s.pages_in_use();
        assert!(shown.iter().all(|tab| in_use.contains(tab)));
    }

    #[test]
    fn windows_showing_different_tabs_keep_them() {
        let mut s = state();
        let first = s.visible_panes();
        s.open_window(2);
        assert_eq!(s.claim_panes(), Vec::<u32>::new());
        s.enter(1);
        assert_eq!(s.visible_panes(), first);
    }

    #[test]
    fn closing_the_current_window_goes_on_in_another() {
        let mut s = state();
        let tab = s.active_tab_id().map(str::to_owned);
        s.open_window(2);
        s.close_window(2, 1);
        assert_eq!(s.window, 1);
        assert_eq!(s.windows(), vec![1]);
        assert_eq!(s.active_tab_id().map(str::to_owned), tab);
    }

    #[test]
    fn a_tab_shown_in_one_window_leaves_a_split_in_the_other() {
        let mut s = state();
        assert!(s.split_insert(None, "c", Side::Right));
        assert_eq!(s.visible_panes(), vec!["b", "c"]);
        s.open_window(2);
        // The split's other page, not the first window's front tab.
        let space = s.active_space.clone();
        s.active_tab.insert(space, "b".into());
        assert_eq!(s.claim_panes(), vec![1]);
        s.enter(1);
        assert_eq!(s.visible_panes(), Vec::<String>::new());
    }

    #[test]
    fn closing_a_window_keeps_its_tabs_in_the_sidebar() {
        let mut s = state();
        let tabs: Vec<String> = s.sidebar.all_tabs().map(|t| t.id.clone()).collect();
        s.open_window(2);
        let space = s.active_space.clone();
        s.active_tab.insert(space, "a".into());
        s.claim_panes();
        s.close_window(2, 1);
        let after: Vec<String> = s.sidebar.all_tabs().map(|t| t.id.clone()).collect();
        assert_eq!(after, tabs);
    }

    /// A second space "two" with a Today tab "t".
    fn with_second_space(s: &mut AppState) {
        s.sidebar.spaces.push(Space {
            id: "two".into(),
            pinned: vec![],
            today: vec![tab("t")],
            ..s.sidebar.spaces[0].clone()
        });
    }

    #[test]
    fn a_window_whose_space_was_deleted_meanwhile_shows_the_first_space() {
        let mut s = state();
        let first = s.active_space.clone();
        with_second_space(&mut s);
        s.open_window(2);
        s.active_space = "two".into();
        s.active_tab.insert("two".into(), "t".into());
        s.enter(1);
        s.delete_space("two");
        assert!(s.enter(2));
        assert_eq!(s.active_space, first);
        assert_eq!(s.active_space().id, first);
        assert_eq!(s.active_tab.get("two"), None);
    }

    #[test]
    fn a_window_keeps_only_the_front_tabs_still_there_when_it_comes_back() {
        let mut s = state();
        let first = s.active_space.clone();
        with_second_space(&mut s);
        s.open_window(2);
        s.active_tab.insert(first.clone(), "a".into());
        s.active_tab.insert("two".into(), "t".into());
        s.enter(1);
        s.archive_tab("a");
        assert!(s.enter(2));
        assert_eq!(s.active_tab_id(), None);
        assert_eq!(s.active_tab.get("two").map(String::as_str), Some("t"));
    }
}
