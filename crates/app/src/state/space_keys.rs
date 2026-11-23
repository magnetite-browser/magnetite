//! The keys a space has of its own (Settings → Shortcuts): pressing them
//! goes to that space. They are saved with the space (`Space::shortcut`),
//! so they sync and go with it.

use magnetite_core::Space;
use magnetite_engine::{Combo, KeysError};

use super::AppState;

impl AppState {
    /// The spaces whose own keys count, each with them, in every window:
    /// a space another window lists is gone to there (`window_for_space`).
    /// Keys that cannot be a shortcut do not count (they may come from
    /// another device), nor keys an action has, nor those an earlier space
    /// has. The private space has none.
    pub fn space_keys(&self) -> Vec<(&Space, Combo)> {
        let mut found: Vec<(&Space, Combo)> = Vec::new();
        for space in (self.sidebar.spaces.iter()).filter(|sp| !self.is_private_space(&sp.id)) {
            let combo = (space.shortcut.as_deref())
                .and_then(|keys| keys.parse::<Combo>().ok())
                .map(Combo::here)
                .filter(|combo| combo.refused().is_none())
                .filter(|combo| self.settings.shortcuts.action_for(*combo).is_none())
                .filter(|combo| found.iter().all(|(_, taken)| taken != combo));
            if let Some(combo) = combo {
                found.push((space, combo));
            }
        }
        found
    }

    /// The space that has the keys `combo`.
    pub fn space_for_keys(&self, combo: Combo) -> Option<&Space> {
        let combo = combo.here();
        (self.space_keys().into_iter()).find_map(|(space, own)| (own == combo).then_some(space))
    }

    /// Give space `id` the keys `combo` (`None`: no keys). A shortcut or
    /// space that had these keys loses them. Refused for keys with a fixed
    /// meaning. Without such a space nothing changes.
    pub fn set_space_shortcut(&mut self, id: &str, combo: Option<Combo>) -> Result<(), KeysError> {
        if self.space(id).is_none() {
            return Ok(());
        }
        if let Some(combo) = combo {
            if let Some(why) = combo.refused() {
                return Err(why);
            }
            self.settings.shortcuts.release(combo);
            let keys = combo.here();
            for space in &mut self.sidebar.spaces {
                let same = |own: &String| own.parse::<Combo>().is_ok_and(|c| c.here() == keys);
                if space.shortcut.as_ref().is_some_and(same) {
                    space.shortcut = None;
                }
            }
        }
        if let Some(space) = self.space_mut(id) {
            space.shortcut = combo.map(|c| c.to_string());
        }
        Ok(())
    }

    /// Take from the spaces the keys an action has got since: the action
    /// runs on them, so they would stay with the space unused.
    pub fn drop_shadowed_space_keys(&mut self) {
        let shortcuts = &self.settings.shortcuts;
        for space in &mut self.sidebar.spaces {
            let shadowed = |own: &String| {
                (own.parse::<Combo>()).is_ok_and(|c| shortcuts.action_for(c).is_some())
            };
            if space.shortcut.as_ref().is_some_and(shadowed) {
                space.shortcut = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use magnetite_engine::Action;

    use super::*;
    use crate::state::testing::state;

    fn combo(keys: &str) -> Combo {
        keys.parse().unwrap()
    }

    /// A state with the spaces `a` and `b`.
    fn two_spaces() -> AppState {
        let mut s = state();
        let mut b = s.sidebar.spaces[0].clone();
        b.pinned.clear();
        b.today.clear();
        s.sidebar.spaces.push(b);
        for (space, id) in s.sidebar.spaces.iter_mut().zip(["a", "b"]) {
            space.id = id.into();
        }
        s.active_space = "a".into();
        s
    }

    #[test]
    fn keys_go_to_one_space_or_one_action() {
        let mut s = two_spaces();
        s.set_space_shortcut("a", Some(combo("alt+shift+W")))
            .unwrap();
        assert_eq!(s.space_for_keys(combo("alt+shift+W")).unwrap().id, "a");

        // Another space takes the keys: the first has none.
        s.set_space_shortcut("b", Some(combo("alt+shift+W")))
            .unwrap();
        assert_eq!(s.space_for_keys(combo("alt+shift+W")).unwrap().id, "b");
        assert_eq!(s.space("a").unwrap().shortcut, None);

        // A space takes an action's keys: the action has none.
        let new_tab = Action::NewTab.default_combo().unwrap();
        s.set_space_shortcut("a", Some(new_tab)).unwrap();
        assert_eq!(s.settings.shortcuts.combo_of(Action::NewTab), None);
        assert_eq!(s.space_for_keys(new_tab).unwrap().id, "a");

        // A space that is not there takes nothing from the one that is.
        let keys = combo("alt+shift+W");
        s.set_space_shortcut("b", Some(keys)).unwrap();
        s.set_space_shortcut("gone", Some(keys)).unwrap();
        assert_eq!(s.space_for_keys(keys).unwrap().id, "b");

        // The action takes them back: the space has none.
        s.settings.shortcuts.reset(Action::NewTab);
        s.drop_shadowed_space_keys();
        assert_eq!(s.space("a").unwrap().shortcut, None);
        assert!(s.space_for_keys(new_tab).is_none());
    }

    #[test]
    fn keys_that_cannot_be_a_shortcut_do_not_count() {
        let mut s = two_spaces();
        assert!(s.set_space_shortcut("a", Some(combo("primary+C"))).is_err());
        assert!(s.set_space_shortcut("a", Some(combo("shift+W"))).is_err());
        // From another device: fixed keys, no keys at all, and a second
        // space on the keys of the first.
        s.space_mut("a").unwrap().shortcut = Some("primary+C".into());
        assert_eq!(s.space_keys().len(), 0);
        s.space_mut("a").unwrap().shortcut = Some("nonsense".into());
        assert_eq!(s.space_keys().len(), 0);
        s.space_mut("a").unwrap().shortcut = Some("alt+shift+W".into());
        s.space_mut("b").unwrap().shortcut = Some("alt+shift+W".into());
        assert_eq!(s.space_keys().len(), 1);
        assert_eq!(s.space_for_keys(combo("alt+shift+W")).unwrap().id, "a");
    }
}
