//! Web panels in the state: favorites and pinned tabs that open over the
//! page in front instead of taking its place (`TabItem::panel`).

use magnetite_core::{Id, SidebarNode, TabItem};

use super::AppState;

impl AppState {
    /// Whether tab `id` has a lasting place in the sidebar: a favorite or
    /// a pinned tab. Only those open as web panels; a Today tab comes and
    /// goes.
    fn keeps_its_place(&self, id: &str) -> bool {
        match self.find_tab(id) {
            Some((None, _)) => true,
            Some((Some(space), _)) => space.today.iter().all(|t| t.id != id),
            None => false,
        }
    }

    /// Whether tab `id` is a favorite or pinned tab that opens as a web
    /// panel.
    pub fn is_web_panel(&self, id: &str) -> bool {
        self.find_tab(id).is_some_and(|(_, t)| t.panel) && self.keeps_its_place(id)
    }

    /// The web panel its key opens: of the favorites and the active space's
    /// pinned tabs that are panels, the one used last; if none was opened
    /// yet, the first in the sidebar.
    pub fn latest_web_panel(&self) -> Option<Id> {
        let favorites = self.sidebar.favorites.iter().flat_map(|f| f.items.iter());
        let pinned = (self.active_space().pinned.iter()).flat_map(SidebarNode::tabs);
        let mut panels: Vec<&TabItem> = favorites.chain(pinned).filter(|t| t.panel).collect();
        panels.sort_by_key(|t| std::cmp::Reverse(t.last_active_at));
        panels.first().map(|t| t.id.clone())
    }

    /// Favorite or pinned tab `id` opens as a web panel from now on
    /// (`panel`), or as a tab again. False if nothing changed: no such
    /// tab, a Today tab, or it opened that way already.
    pub fn set_web_panel(&mut self, id: &str, panel: bool) -> bool {
        let may = self.keeps_its_place(id);
        let Some(tab) = self.find_tab_mut(id).filter(|_| may) else {
            return false;
        };
        if tab.panel == panel {
            return false;
        }
        if panel {
            tab.panel = true;
        } else {
            tab.end_panel();
        }
        self.settle_panels();
        true
    }

    /// A tab that opens as a web panel is no page of the page area: it is
    /// the tab in front of no space, in no window, and in no split. One
    /// that became a panel while it was (here, or on another device)
    /// leaves them.
    pub fn settle_panels(&mut self) {
        let favorites = self.sidebar.favorites.iter().flat_map(|f| f.items.iter());
        let pinned = (self.sidebar.spaces.iter())
            .flat_map(|s| s.pinned.iter())
            .flat_map(SidebarNode::tabs);
        let panels: Vec<Id> = favorites
            .chain(pinned)
            .filter(|t| t.panel)
            .map(|t| t.id.clone())
            .collect();
        for id in &panels {
            self.split_remove(id);
        }
        self.active_tab.retain(|_, tab| !panels.contains(tab));
        for view in self.views.values_mut() {
            view.active_tab.retain(|_, tab| !panels.contains(tab));
        }
    }
}

#[cfg(test)]
mod tests {
    use magnetite_core::split::Side;

    use crate::state::testing::{state, tab};

    #[test]
    fn a_favorite_that_becomes_a_panel_leaves_the_page_area() {
        let mut s = state();
        s.sidebar.favorites[0].items = vec![tab("f")];
        let space = s.active_space.clone();
        assert!(s.split_insert(Some("b"), "f", Side::Right));
        assert_eq!(s.visible_panes(), ["b", "f"]);

        assert!(s.set_web_panel("f", true));
        assert!(s.is_web_panel("f"));
        assert_eq!(s.active_tab.get(&space), None);
        assert_eq!(s.visible_panes(), [] as [&str; 0]);
        // And it does not come back as a page.
        assert!(!s.split_insert(Some("b"), "f", Side::Right));
        assert!(!s.tabs_by_last_use().contains(&"f".to_string()));

        // A plain favorite again, it keeps none of the panel's settings.
        let f = s.find_tab_mut("f").unwrap();
        (f.panel_width, f.panel_mobile) = (Some(500), true);
        assert!(s.set_web_panel("f", false));
        let (_, f) = s.find_tab("f").unwrap();
        assert_eq!(
            (f.panel, f.panel_width, f.panel_mobile),
            (false, None, false)
        );
    }

    #[test]
    fn the_key_opens_the_panel_used_last() {
        let mut s = state();
        assert_eq!(s.latest_web_panel(), None);
        s.sidebar.favorites[0].items = vec![tab("f"), tab("g")];
        for id in ["f", "g", "p"] {
            assert!(s.set_web_panel(id, true));
        }
        // None opened yet: the first in the sidebar.
        assert_eq!(s.latest_web_panel().as_deref(), Some("f"));
        s.find_tab_mut("f").unwrap().last_active_at = Some(10);
        s.find_tab_mut("p").unwrap().last_active_at = Some(20);
        assert_eq!(s.latest_web_panel().as_deref(), Some("p"));
    }

    #[test]
    fn today_tabs_do_not_open_as_panels() {
        let mut s = state();
        assert!(!s.set_web_panel("a", true));
        assert!(!s.is_web_panel("a"));
        // A flag that came along with a tab (another device's sidebar) does
        // not count in Today.
        s.find_tab_mut("a").unwrap().panel = true;
        assert!(!s.is_web_panel("a"));
    }

    #[test]
    fn a_pinned_panel_has_no_row_to_switch_to() {
        let mut s = state();
        assert!(s.visible_tab_ids().contains(&"p".to_string()));
        assert!(s.set_web_panel("p", true));
        assert!(s.is_web_panel("p"));
        // Not in the sidebar's round, not by last use.
        assert!(!s.visible_tab_ids().contains(&"p".to_string()));
        assert!(!s.tabs_by_last_use().contains(&"p".to_string()));
        // And no page to put another beside.
        assert!(!s.split_insert(Some("p"), "a", Side::Right));
        assert_eq!(s.visible_panes(), ["b"]);
    }
}
