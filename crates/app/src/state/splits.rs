//! Split views in the state: which pages share the screen.

use magnetite_core::Id;
use magnetite_core::split::{Side, SplitNode};

use super::AppState;

/// The list of the sidebar a tab stands in.
#[derive(PartialEq)]
enum Home<'a> {
    /// The favorites of a profile.
    Favorites(&'a str),
    /// The pinned tabs of a space, its folders included.
    Pinned(&'a str),
    /// The Today tabs of a space.
    Today(&'a str),
}

impl AppState {
    /// Whether `id` is a pinned tab of a space, in a folder or not.
    pub fn is_pinned_tab(&self, id: &str) -> bool {
        matches!(self.home_of(id), Some(Home::Pinned(_)))
    }

    /// Whether extensions see tab `id` as pinned: a pinned tab or a
    /// favorite (`MagnetiteApp::tell_pinned`).
    pub fn reads_pinned(&self, id: &str) -> bool {
        matches!(self.home_of(id), Some(Home::Pinned(_) | Home::Favorites(_)))
    }

    /// Whether tab `id` is a favorite.
    pub fn is_favorite(&self, id: &str) -> bool {
        matches!(self.home_of(id), Some(Home::Favorites(_)))
    }

    fn home_of(&self, id: &str) -> Option<Home<'_>> {
        let favorites = &self.sidebar.favorites;
        if let Some(list) = (favorites.iter()).find(|f| f.items.iter().any(|t| t.id == id)) {
            return Some(Home::Favorites(&list.profile_id));
        }
        self.sidebar.spaces.iter().find_map(|sp| {
            if sp.today.iter().any(|t| t.id == id) {
                Some(Home::Today(&sp.id))
            } else {
                sp.find_tab(id).map(|_| Home::Pinned(&sp.id))
            }
        })
    }

    /// The pages that move with `tab` as one item: those of its split that
    /// stand in the same list (the favorites of one profile, the pinned or
    /// the Today tabs of one space), in reading order. A tab in no split is
    /// alone.
    pub fn split_group(&self, tab: &str) -> Vec<Id> {
        let home = self.home_of(tab);
        let split = self.live_splits().find(|t| t.contains(&tab.to_string()));
        let mut ids = split.map(|t| t.ids()).unwrap_or_default();
        ids.retain(|id| self.home_of(id) == home);
        if ids.is_empty() {
            ids.push(tab.to_string());
        }
        ids
    }

    /// The pages of the split holding `tab` if that split is kept: each of
    /// its pages is a favorite or a pinned tab. Such a split closes as one
    /// item and stays.
    pub fn kept_split(&self, tab: &str) -> Option<Vec<Id>> {
        let ids = self
            .live_splits()
            .find(|t| t.contains(&tab.to_string()))?
            .ids();
        ids.iter()
            .all(|id| !matches!(self.home_of(id), Some(Home::Today(_)) | None))
            .then_some(ids)
    }

    /// Splits as shown: tabs that no longer exist left out, and only splits
    /// that still have two or more pages.
    pub(super) fn live_splits(&self) -> impl Iterator<Item = SplitNode<Id>> + '_ {
        self.splits.iter().filter_map(|tree| {
            let mut tree = tree.clone();
            for id in tree.ids() {
                if self.find_tab(&id).is_none() {
                    tree = tree.remove(&id)?;
                }
            }
            (tree.pane_count() > 1).then_some(tree)
        })
    }

    /// The split holding the active tab: the one on screen.
    pub fn visible_split(&self) -> Option<SplitNode<Id>> {
        let active = self.active_tab_id()?.to_string();
        self.live_splits().find(|t| t.contains(&active))
    }

    /// The stored split holding the active tab, to resize it in place.
    pub fn visible_split_mut(&mut self) -> Option<&mut SplitNode<Id>> {
        let active = self.active_tab_id()?.to_string();
        self.splits.iter_mut().find(|t| t.contains(&active))
    }

    /// Tabs to show for the active space: the split's pages, or the active tab.
    pub fn visible_panes(&self) -> Vec<Id> {
        match (self.visible_split(), self.active_tab_id()) {
            (Some(tree), _) => tree.ids(),
            // A tab that is gone (not saved, or taken away by a sync) shows
            // nothing: a page window made for it would belong to no tab.
            (None, Some(active)) if self.find_tab(active).is_some() => vec![active.to_string()],
            (None, _) => vec![],
        }
    }

    /// Put `tab` beside `target` (default: the active tab), in the split
    /// `target` is in or a new one. `tab` leaves any split it was in; a page
    /// of the same split moves inside it. False if that split is full.
    pub fn split_insert(&mut self, target: Option<&str>, tab: &str, side: Side) -> bool {
        let Some(target) = target
            .map(str::to_owned)
            .or_else(|| self.active_tab_id().map(str::to_owned))
        else {
            return false;
        };
        // A web panel lies over the pages: it is no page of a split, as
        // the page put there or the one it is put beside.
        if target == tab
            || self.find_tab(tab).is_none()
            || self.is_web_panel(tab)
            || self.is_web_panel(&target)
        {
            return false;
        }
        let mut tree = self
            .splits
            .iter()
            .find(|t| t.contains(&target))
            .cloned()
            .unwrap_or_else(|| SplitNode::pane(target.clone()));
        let tab_id = tab.to_string();
        let placed = if tree.contains(&tab_id) {
            tree.relocate(&tab_id, &target, side)
        } else {
            tree.insert(&target, tab_id, side)
        };
        if !placed {
            return false;
        }
        self.split_remove(tab);
        self.splits.retain(|t| !t.contains(&target));
        self.splits.push(tree);
        // The page you dropped is the one you want to work in.
        self.active_tab
            .insert(self.active_space.clone(), tab.to_string());
        true
    }

    /// Move `tab` before or after `target` (`side`) in the reading order of
    /// the split both are in; its layout stays.
    pub fn split_reorder(&mut self, target: &str, tab: &str, side: Side) -> bool {
        let (target, tab) = (target.to_string(), tab.to_string());
        self.splits
            .iter_mut()
            .any(|t| t.reorder(&tab, &target, side))
    }

    /// Let `tab` and `target`, pages of one split, trade places.
    pub fn split_swap(&mut self, target: &str, tab: &str) -> bool {
        let (target, tab) = (target.to_string(), tab.to_string());
        let swapped = self.splits.iter_mut().any(|t| t.swap(&tab, &target));
        if swapped {
            self.active_tab.insert(self.active_space.clone(), tab);
        }
        swapped
    }

    /// Whether `tab` cannot join the split of `target` (default: the active
    /// tab) because that split holds as many pages as a split can.
    pub fn split_is_full(&self, target: Option<&str>, tab: &str) -> bool {
        let Some(target) = target.or_else(|| self.active_tab_id()).map(str::to_owned) else {
            return false;
        };
        self.splits.iter().any(|tree| {
            tree.contains(&target)
                && !tree.contains(&tab.to_string())
                && tree.pane_count() >= magnetite_core::split::MAX_PANES
        })
    }

    /// Take `tab` out of its split; a split left with one page ends.
    pub fn split_remove(&mut self, tab: &str) {
        let tab = tab.to_string();
        self.splits = std::mem::take(&mut self.splits)
            .into_iter()
            .filter_map(|tree| tree.remove(&tab).filter(|t| t.pane_count() > 1))
            .collect();
    }

    /// End the split holding `tab`; its pages become separate tabs again.
    pub fn split_separate(&mut self, tab: &str) {
        let tab = tab.to_string();
        self.splits.retain(|t| !t.contains(&tab));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::{state, tab};

    #[test]
    fn extensions_see_pinned_tabs_and_favorites_as_pinned() {
        let mut s = state();
        s.sidebar.favorites[0].items = vec![tab("f")];
        assert!(s.reads_pinned("p"), "a pinned tab");
        assert!(s.reads_pinned("f"), "a favorite");
        assert!(!s.reads_pinned("a"), "a Today tab");
        assert!(s.is_favorite("f") && !s.is_favorite("p"));
    }

    #[test]
    fn only_a_full_split_counts_as_full() {
        let mut s = state();
        s.sidebar.spaces[0].today.extend([tab("d"), tab("e")]);
        assert!(!s.split_is_full(None, "a"), "no split yet");
        for id in ["a", "c", "d"] {
            assert!(s.split_insert(Some("b"), id, Side::Right));
        }
        assert!(s.split_is_full(Some("b"), "e"));
        assert!(!s.split_is_full(Some("b"), "a"), "already one of its pages");
        assert!(!s.split_insert(Some("b"), "e", Side::Right));
    }

    #[test]
    fn splits_follow_the_active_tab_and_survive_archiving() {
        let mut s = state();
        assert_eq!(s.visible_panes(), vec!["b"]);
        assert!(s.split_insert(None, "c", Side::Right));
        assert_eq!(s.visible_panes(), vec!["b", "c"]);
        assert_eq!(
            s.active_tab_id(),
            Some("c"),
            "the dropped page becomes active"
        );
        assert!(s.split_insert(Some("b"), "a", Side::Bottom));
        assert_eq!(s.visible_panes(), vec!["b", "a", "c"]);
        // A tab outside the split shows alone.
        s.active_tab.insert(s.active_space.clone(), "p".into());
        assert_eq!(s.visible_panes(), vec!["p"]);
        // Archiving pages shrinks the split until it ends.
        s.active_tab.insert(s.active_space.clone(), "b".into());
        s.archive_tab("a");
        assert_eq!(s.visible_panes(), vec!["b", "c"]);
        s.archive_tab("c");
        assert_eq!(s.splits, vec![]);
        assert_eq!(s.visible_panes(), vec!["b"]);
    }

    #[test]
    fn a_page_moves_and_swaps_inside_its_split() {
        let mut s = state();
        assert!(s.split_insert(Some("a"), "b", Side::Right));
        assert!(s.split_insert(Some("b"), "c", Side::Right));
        assert!(s.split_insert(Some("a"), "c", Side::Left));
        assert_eq!(s.splits.len(), 1);
        assert_eq!(s.splits[0].ids(), vec!["c", "a", "b"]);
        assert!(s.split_swap("b", "c"));
        assert_eq!(s.splits[0].ids(), vec!["b", "a", "c"]);
        assert_eq!(s.active_tab_id(), Some("c"));
        assert!(!s.split_swap("p", "c"), "p is not in the split");
        assert!(s.split_reorder("b", "c", Side::Left));
        assert_eq!(s.splits[0].ids(), vec!["c", "b", "a"]);
    }

    #[test]
    fn several_splits_live_side_by_side_and_a_tab_moves_between_them() {
        let mut s = state();
        assert!(s.split_insert(Some("a"), "b", Side::Right));
        assert!(s.split_insert(Some("c"), "p", Side::Left));
        assert_eq!(s.splits.len(), 2, "a second split keeps the first");
        assert_eq!(s.visible_panes(), vec!["p", "c"]);
        // Dropping `p` next to `a` takes it out of the split with `c`, which ends.
        assert!(s.split_insert(Some("a"), "p", Side::Left));
        assert_eq!(s.splits.len(), 1);
        assert_eq!(s.visible_panes(), vec!["p", "a", "b"]);
        // Separating ends only the split holding the given tab.
        assert!(s.split_insert(Some("c"), "b", Side::Right));
        s.split_separate("c");
        assert_eq!(s.splits.len(), 1);
        assert_eq!(s.splits[0].ids(), vec!["p", "a"]);
    }

    #[test]
    fn the_pages_of_a_split_in_one_list_move_as_one_item() {
        let mut s = state();
        assert_eq!(s.split_group("a"), vec!["a"], "no split: alone");
        assert!(s.split_insert(Some("a"), "c", Side::Right));
        assert!(s.split_insert(Some("c"), "p", Side::Right));
        assert_eq!(s.split_group("c"), vec!["a", "c"]);
        assert_eq!(
            s.split_group("p"),
            vec!["p"],
            "the pinned page stands elsewhere"
        );
    }

    #[test]
    fn a_split_is_kept_once_none_of_its_pages_is_a_today_tab() {
        let mut s = state();
        assert!(s.split_insert(Some("p"), "a", Side::Right));
        assert_eq!(s.kept_split("p"), None);
        assert!(s.sidebar.spaces[0].pin("a"));
        assert_eq!(s.kept_split("a"), Some(vec!["p".into(), "a".into()]));
        assert_eq!(s.kept_split("b"), None, "in no split");
    }
}
