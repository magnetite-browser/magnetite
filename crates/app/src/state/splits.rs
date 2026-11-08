//! Split views in the state: which pages share the screen.

use magnetite_core::Id;
use magnetite_core::split::{Side, SplitNode};

use super::AppState;

impl AppState {
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
    /// `target` is in or a new one. `tab` leaves any split it was in. False
    /// if that split is full or already holds `tab`.
    pub fn split_insert(&mut self, target: Option<&str>, tab: &str, side: Side) -> bool {
        let Some(target) = target
            .map(str::to_owned)
            .or_else(|| self.active_tab_id().map(str::to_owned))
        else {
            return false;
        };
        if target == tab || self.find_tab(tab).is_none() {
            return false;
        }
        let mut tree = self
            .splits
            .iter()
            .find(|t| t.contains(&target))
            .cloned()
            .unwrap_or_else(|| SplitNode::pane(target.clone()));
        if !tree.insert(&target, tab.to_string(), side) {
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
}
