//! Profiles: which one a tab runs in, private profiles, and the favorites,
//! which are kept as one list per profile.

use magnetite_core::{Id, TabItem};

use super::{AppState, new_id, now};

/// How many deleted profiles a device remembers.
const MAX_DELETED_PROFILES: usize = 256;

/// The profile a new space or favorite runs in (`AppState::new_space`,
/// `AppState::add_favorite`; without one, the default profile):
/// `{"id": …}`, `{"new": name}` or `"private"`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChosenProfile {
    /// A profile there is already.
    Id(Id),
    /// A new lasting profile by this name.
    New(String),
    /// A private profile of its own: in memory, gone when Magnetite quits.
    Private,
}

impl AppState {
    /// The id of the profile `chosen` for something new named `name`; a
    /// profile it asks for is made now (a private one is named
    /// "`name` (private)", in the user's language). Errs, making nothing,
    /// when the profile chosen is gone or is one of the private space's.
    pub(super) fn chosen_profile(
        &mut self,
        chosen: Option<ChosenProfile>,
        name: &str,
    ) -> Result<Id, &'static str> {
        Ok(match chosen {
            None => self.default_profile_id(),
            Some(ChosenProfile::Id(id)) => {
                let known = self.sidebar.profiles.iter().any(|p| p.id == id);
                // The private space's profiles are for nothing else.
                if !known || self.private_profiles.contains(&id) {
                    return Err(magnetite_ui::t!("app.errors.profile_gone"));
                }
                id
            }
            Some(ChosenProfile::New(name)) => self.add_profile(&name, false),
            Some(ChosenProfile::Private) => self.add_profile(
                &magnetite_ui::t!("app.names.private_profile", name = name),
                true,
            ),
        })
    }

    /// Add a favorite for `url` (an address already checked), named `host`
    /// until its page says what it is called, last in the list of the
    /// profile `chosen` (a private one is named after the site). The
    /// favorite and its profile come in one step: no save or sync sees the
    /// one without the other. Returns the favorite's id and its profile's.
    pub fn add_favorite(
        &mut self,
        url: &str,
        host: &str,
        chosen: Option<ChosenProfile>,
    ) -> Result<(Id, Id), &'static str> {
        let site = host.strip_prefix("www.").unwrap_or(host);
        let profile_id = self.chosen_profile(chosen, site)?;
        let id = new_id();
        let favorite = TabItem {
            home_url: Some(url.into()),
            created_at: Some(now()),
            ..TabItem::new(id.clone(), url, host)
        };
        let lists = &mut self.sidebar.favorites;
        match lists.iter_mut().find(|f| f.profile_id == profile_id) {
            Some(list) => list.items.push(favorite),
            None => lists.push(magnetite_core::FavoriteSet {
                profile_id: profile_id.clone(),
                items: vec![favorite],
            }),
        }
        self.normalise_favorites();
        Ok((id, profile_id))
    }

    /// Give favorite `id` the address `url` (already checked) to go to and
    /// to return to. One without a name of its own is named `host` until
    /// its page says what it is called. False if `id` is no favorite.
    pub fn set_favorite_address(&mut self, id: &str, url: &str, host: &str) -> bool {
        let favorites = self.sidebar.favorites.iter_mut();
        let Some(tab) = (favorites.flat_map(|f| f.items.iter_mut())).find(|t| t.id == id) else {
            return false;
        };
        tab.home_url = Some(url.into());
        if !tab.custom_title {
            tab.title = host.strip_prefix("www.").unwrap_or(host).into();
        }
        true
    }

    /// Engine context of the page in front: the one the active tab runs in
    /// (a favorite without a profile of its own runs in the default one, as
    /// `present` opens it), else the active space's. Extensions are per
    /// profile.
    pub fn front_context(&self) -> Option<magnetite_engine::ContextProfile> {
        match self.active_tab_id() {
            Some(tab) => self.context(self.profile_for_tab(tab).as_deref()),
            None => self.context(Some(&self.active_space().profile_id)),
        }
    }

    /// Profile a tab's page runs in: the tab's own override if that profile
    /// still exists, else its space's profile (`None`: the default context).
    pub fn profile_for_tab(&self, id: &str) -> Option<String> {
        let (space, tab) = self.find_tab(id)?;
        let exists = |p: &String| self.sidebar.profiles.iter().any(|x| &x.id == p);
        tab.profile_id
            .clone()
            .filter(exists)
            .or_else(|| space.map(|s| s.profile_id.clone()))
            // A favorite runs in the profile of the list it is in.
            .or_else(|| {
                let lists = &self.sidebar.favorites;
                let list = lists.iter().find(|f| f.items.iter().any(|t| t.id == id))?;
                Some(list.profile_id.clone()).filter(exists)
            })
    }

    /// Put the favorites in order: one list per profile, in the order of the
    /// profiles, each favorite in the list of the profile it runs in. Only
    /// the default profile's list stays while empty. Favorites that still
    /// name a profile of their own (saved by an earlier version, or just
    /// given one) move to that profile's list; lists of profiles that are
    /// gone join the default profile's. Profiles without a colour get one
    /// (`SidebarSnapshot::color_profiles`).
    pub fn normalise_favorites(&mut self) {
        // The profiles' own order first: one saved by an earlier version, or
        // come from a device with one, has no colour yet.
        self.sidebar.color_profiles();
        let default = self.default_profile_id();
        let profiles: Vec<Id> = self.sidebar.profiles.iter().map(|p| p.id.clone()).collect();
        let mut lists: Vec<magnetite_core::FavoriteSet> = profiles
            .iter()
            .map(|id| magnetite_core::FavoriteSet {
                profile_id: id.clone(),
                items: vec![],
            })
            .collect();
        for set in std::mem::take(&mut self.sidebar.favorites) {
            for mut tab in set.items {
                let own = tab.profile_id.clone();
                let wanted = match &own {
                    // An unknown profile of its own stays with the tab, which
                    // then counts as private (`is_private_tab`).
                    Some(p) if profiles.contains(p) => {
                        tab.profile_id = None;
                        p
                    }
                    _ => &set.profile_id,
                };
                let target = if profiles.contains(wanted) {
                    wanted
                } else {
                    &default
                };
                if let Some(list) = lists.iter_mut().find(|l| &l.profile_id == target) {
                    list.items.push(tab);
                }
            }
        }
        lists.retain(|l| !l.items.is_empty() || l.profile_id == default);
        self.sidebar.favorites = lists;
        self.settle_panels();
    }

    /// Tab `id` runs in a private (in-memory) profile. A tab whose own
    /// profile no longer exists counts as private: better kept off disk and
    /// out of sync than leaked.
    pub fn is_private_tab(&self, id: &str) -> bool {
        let dangling = self.find_tab(id).is_some_and(|(_, t)| {
            t.profile_id
                .as_ref()
                .is_some_and(|p| !self.sidebar.profiles.iter().any(|x| &x.id == p))
        });
        dangling
            || self.profile_for_tab(id).is_some_and(|p| {
                self.sidebar
                    .profiles
                    .iter()
                    .any(|x| x.id == p && x.ephemeral)
            })
    }

    /// Whether engine context `context` (`""`: the default one) belongs to
    /// a private profile; an unknown context counts as private.
    pub fn is_private_context(&self, context: Option<&str>) -> bool {
        match context {
            Some("") => false,
            Some(c) => self.profile_for_context(c).is_none_or(|id| {
                self.sidebar
                    .profiles
                    .iter()
                    .any(|p| p.id == id && p.ephemeral)
            }),
            None => true,
        }
    }

    /// Profile id for an engine context id (`""`: the default context), if
    /// that profile still exists.
    pub fn profile_for_context(&self, context: &str) -> Option<String> {
        self.sidebar
            .profiles
            .iter()
            .find(|p| {
                if context.is_empty() {
                    p.is_default
                } else {
                    p.id == context
                }
            })
            .map(|p| p.id.clone())
    }

    /// Add a profile by `name` ("New profile" when blank); `ephemeral`: in
    /// memory only. Returns its id.
    pub fn add_profile(&mut self, name: &str, ephemeral: bool) -> Id {
        let id = new_id();
        let name = match name.trim() {
            "" => magnetite_ui::t!("app.names.new_profile"),
            name => name,
        };
        self.sidebar.profiles.push(magnetite_core::Profile {
            id: id.clone(),
            name: name.into(),
            is_default: false,
            ephemeral,
            color: None,
        });
        self.sidebar.color_profiles();
        id
    }

    /// Put profile `id` at place `index` of the lasting profiles, counted
    /// without it (past the end: last of them). Private profiles are not
    /// sorted: they are on no list. The lists of favorites follow, which
    /// stand in the order of the profiles.
    pub fn move_profile(&mut self, id: &str, index: usize) {
        let profiles = &mut self.sidebar.profiles;
        let Some(from) = profiles.iter().position(|p| p.id == id && !p.ephemeral) else {
            return;
        };
        let profile = profiles.remove(from);
        let lasting = |p: &magnetite_core::Profile| !p.ephemeral;
        let at = (profiles.iter().enumerate())
            .filter(|(_, p)| lasting(p))
            .map(|(at, _)| at)
            .nth(index)
            .unwrap_or_else(|| profiles.iter().rposition(lasting).map_or(0, |at| at + 1));
        profiles.insert(at, profile);
        self.normalise_favorites();
    }

    /// Id of the default profile.
    pub fn default_profile_id(&self) -> Id {
        let profiles = &self.sidebar.profiles;
        profiles
            .iter()
            .find(|p| p.is_default)
            .or_else(|| profiles.first())
            .map_or_else(|| "default".to_string(), |p| p.id.clone())
    }

    /// Engine context for a profile id; the default profile (and unknown
    /// ids) map to the global context.
    pub fn context(&self, profile_id: Option<&str>) -> Option<magnetite_engine::ContextProfile> {
        let p = self
            .sidebar
            .profiles
            .iter()
            .find(|p| Some(p.id.as_str()) == profile_id)?;
        (!p.is_default).then(|| magnetite_engine::ContextProfile {
            id: p.id.clone(),
            ephemeral: p.ephemeral,
        })
    }

    /// Whether something runs in a profile: a space, a favorite, a tab
    /// opened from a page of that profile (it keeps the profile), a peek
    /// or a mini window.
    pub fn profile_in_use(&self, profile_id: &str) -> bool {
        self.sidebar.uses_profile(profile_id)
            || (self.peeks.values())
                .chain(self.minis.iter())
                .any(|p| p.profile == profile_id)
    }

    /// Delete profile `id` on this device, unless it is the default one or
    /// still in use. Its data folder goes at the next start, before any page
    /// could open it (`deleted_profile_data`). False if the profile stays.
    pub fn delete_profile(&mut self, id: &str) -> bool {
        let profiles = &self.sidebar.profiles;
        if !profiles.iter().any(|p| p.id == id && !p.is_default) || self.profile_in_use(id) {
            return false;
        }
        self.forget_profile(id);
        true
    }

    /// Take profile `id` off the list and remember that it was deleted
    /// here (`deleted_profiles`): the newest `MAX_DELETED_PROFILES` count.
    fn forget_profile(&mut self, id: &str) {
        self.sidebar.profiles.retain(|p| p.id != id);
        self.deleted_profiles.retain(|d| d != id);
        self.deleted_profiles.push(id.to_string());
        let extra = (self.deleted_profiles.len()).saturating_sub(MAX_DELETED_PROFILES);
        self.deleted_profiles.drain(..extra);
    }

    /// The profiles deleted on this device, for the caller to remove their
    /// data folders (at the start, before any page opens; a folder that is
    /// gone already is fine). Never one that is on the list of profiles.
    /// Only these are ever removed: a state file that could not be read
    /// names no deletions, and so costs no profile its data.
    pub fn deleted_profile_data(&self) -> Vec<Id> {
        let listed = |id: &Id| self.sidebar.profiles.iter().any(|p| &p.id == id);
        (self.deleted_profiles.iter())
            .filter(|id| !listed(id))
            .cloned()
            .collect()
    }

    /// Private profiles exist for one space or favorite; drop them once
    /// nothing uses them. A deletion like any other: remembered, so the
    /// profile is not taken back from another device's list. Not those of
    /// the private space: no other device hears of them, and they go their
    /// own way (`release_private_profiles`).
    pub fn prune_private_profiles(&mut self) {
        let unused: Vec<String> = self
            .sidebar
            .profiles
            .iter()
            .filter(|p| p.ephemeral && !self.private_profiles.contains(&p.id))
            .filter(|p| !self.profile_in_use(&p.id))
            .map(|p| p.id.clone())
            .collect();
        for id in unused {
            self.forget_profile(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use magnetite_core::{Profile, Space, TabItem};

    use super::{ChosenProfile, MAX_DELETED_PROFILES};
    use crate::state::TabRuntime;
    use crate::state::testing::{state, tab};

    #[test]
    fn favorite_profile_overrides_space_and_ignores_deleted_profiles() {
        let mut s = state();
        s.sidebar.profiles.push(Profile {
            id: "work".into(),
            name: "Work".into(),
            is_default: false,
            ephemeral: false,
            color: None,
        });
        s.sidebar.favorites[0].items.push(TabItem {
            profile_id: Some("work".into()),
            ..tab("fav")
        });
        s.sidebar.favorites[0].items.push(TabItem {
            profile_id: Some("gone".into()),
            ..tab("stale")
        });
        let space_profile = s.sidebar.spaces[0].profile_id.clone();
        assert_eq!(s.profile_for_tab("fav").as_deref(), Some("work"));
        // A favorite naming a profile that is gone runs in the profile of its list.
        assert_eq!(s.profile_for_tab("stale"), Some(s.default_profile_id()));
        assert_eq!(s.profile_for_tab("a"), Some(space_profile));
    }

    #[test]
    fn a_new_favorite_comes_in_its_private_profile_and_a_gone_profile_makes_nothing() {
        let mut s = state();
        let url = "https://www.example.org/";
        let (id, profile) = s
            .add_favorite(url, "www.example.org", Some(ChosenProfile::Private))
            .unwrap();
        let made = s.sidebar.profiles.iter().find(|p| p.id == profile).unwrap();
        assert_eq!(
            (made.name.as_str(), made.ephemeral),
            ("example.org (private)", true)
        );
        assert_eq!(s.profile_for_tab(&id), Some(profile));
        assert!(s.is_private_tab(&id), "never in the main profile");

        let profiles = s.sidebar.profiles.len();
        let favorites = s.sidebar.favorites.clone();
        let gone = Some(ChosenProfile::Id("gone".into()));
        assert!(s.add_favorite(url, "www.example.org", gone).is_err());
        assert_eq!(s.sidebar.profiles.len(), profiles);
        assert_eq!(s.sidebar.favorites, favorites);
    }

    #[test]
    fn a_favorite_takes_a_new_address_and_keeps_a_name_of_its_own() {
        let mut s = state();
        let (id, _) = s
            .add_favorite("https://example.org/", "example.org", None)
            .unwrap();
        assert!(s.set_favorite_address(&id, "https://www.example.com/a", "www.example.com"));
        let tab = s.find_tab(&id).unwrap().1;
        assert_eq!(tab.home_url.as_deref(), Some("https://www.example.com/a"));
        assert_eq!(tab.title, "example.com", "named after its new site");
        let tab = s.find_tab_mut(&id).unwrap();
        tab.custom_title = true;
        tab.title = "Mail".into();
        assert!(s.set_favorite_address(&id, "https://example.net/", "example.net"));
        assert_eq!(s.find_tab(&id).unwrap().1.title, "Mail");
        // A tab that is no favorite has no address to set here.
        assert!(!s.set_favorite_address("a", "https://example.net/", "example.net"));
        assert_eq!(s.find_tab("a").unwrap().1.home_url, None);
    }

    #[test]
    fn private_today_tabs_are_not_saved_synced_or_archived() {
        let mut s = state();
        s.sidebar.profiles.push(Profile {
            id: "private".into(),
            name: "Private".into(),
            is_default: false,
            ephemeral: true,
            color: None,
        });
        s.sidebar.spaces[0].today[0].profile_id = Some("private".into());
        let lasting = s.lasting_sidebar();
        let ids: Vec<&str> = lasting.spaces[0]
            .today
            .iter()
            .map(|t| t.id.as_str())
            .collect();
        assert_eq!(ids, ["b", "c"], "the private tab stays in memory");
        assert_eq!(
            s.sidebar.spaces[0].today.len(),
            3,
            "but is still in the sidebar"
        );
        s.archive_tab("a");
        s.archive_tab("b");
        let archived: Vec<&str> = s.archive.iter().map(|a| a.tab.id.as_str()).collect();
        assert_eq!(
            archived,
            ["b"],
            "a closed private tab is gone, not archived"
        );
        // Nor is one of a deleted space; the others go to the archive.
        s.sidebar.spaces.push(Space {
            id: "two".into(),
            pinned: vec![],
            today: vec![
                TabItem {
                    profile_id: Some("private".into()),
                    ..tab("d")
                },
                tab("e"),
            ],
            ..s.sidebar.spaces[0].clone()
        });
        s.runtime.insert(
            "e".into(),
            TabRuntime {
                content: Some(5),
                ..Default::default()
            },
        );
        s.content_to_tab.insert(5, "e".into());
        assert_eq!(s.delete_space("two"), vec![5], "its open page closes");
        assert!(s.space("two").is_none() && s.content_to_tab.is_empty());
        let archived: Vec<&str> = s.archive.iter().map(|a| a.tab.id.as_str()).collect();
        assert_eq!(archived, ["b", "e"]);
    }

    #[test]
    fn isolation_levels_map_to_contexts_and_private_profiles_are_cleaned_up() {
        let mut s = state();
        let default_id = s.sidebar.profiles[0].id.clone();
        let private = |id: &str| Profile {
            id: id.into(),
            name: id.into(),
            is_default: false,
            ephemeral: true,
            color: None,
        };
        s.sidebar.profiles.push(private("used"));
        s.sidebar.profiles.push(private("orphan"));
        s.sidebar.favorites[0].items.push(TabItem {
            profile_id: Some("used".into()),
            ..tab("fav")
        });
        // Shared = the global context, private = an in-memory one.
        assert_eq!(s.context(Some(&default_id)), None);
        assert_eq!(
            s.context(Some("used")),
            Some(magnetite_engine::ContextProfile {
                id: "used".into(),
                ephemeral: true
            })
        );
        assert!(
            s.profile_in_use(&default_id)
                && s.profile_in_use("used")
                && !s.profile_in_use("orphan")
        );
        s.prune_private_profiles();
        let ids: Vec<&str> = s.sidebar.profiles.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec![default_id.as_str(), "used"]);
    }

    #[test]
    fn a_profile_is_in_use_while_a_tab_or_peek_runs_in_it() {
        let mut s = state();
        s.sidebar.profiles.push(Profile {
            id: "secret".into(),
            name: "Secret".into(),
            is_default: false,
            ephemeral: true,
            color: None,
        });
        // A tab opened from a private favorite keeps its profile.
        s.find_tab_mut("a").unwrap().profile_id = Some("secret".into());
        assert!(s.profile_in_use("secret"));
        s.prune_private_profiles();
        assert!(!s.delete_profile("secret"));
        assert!(s.sidebar.profiles.iter().any(|p| p.id == "secret"));
        // Free again: it can go, and its data is due for removal once.
        s.find_tab_mut("a").unwrap().profile_id = None;
        assert!(s.delete_profile("secret"));
        assert_eq!(s.deleted_profile_data(), ["secret"]);
    }

    #[test]
    fn deleted_profiles_are_remembered_once_and_the_oldest_make_room() {
        let mut s = state();
        let profile = |id: &str, ephemeral| Profile {
            id: id.into(),
            name: id.into(),
            is_default: false,
            ephemeral,
            color: None,
        };
        // A private profile dropped after its last use counts as deleted.
        s.sidebar.profiles.push(profile("secret", true));
        s.prune_private_profiles();
        assert_eq!(s.deleted_profiles, ["secret"]);
        for n in 0..=MAX_DELETED_PROFILES {
            let id = format!("p{n}");
            s.sidebar.profiles.push(profile(&id, false));
            assert!(s.delete_profile(&id));
        }
        assert_eq!(s.deleted_profiles.len(), MAX_DELETED_PROFILES);
        assert_eq!(s.deleted_profiles[0], "p1", "the two oldest made room");
        // The default profile cannot be deleted, so its data never goes.
        let default_id = s.default_profile_id();
        assert!(!s.delete_profile(&default_id));
        assert!(!s.deleted_profile_data().contains(&default_id));
    }

    #[test]
    fn a_lasting_profile_gets_a_colour_of_its_own_and_keeps_it() {
        use magnetite_core::PROFILE_COLORS;
        let mut s = state();
        let color = |s: &crate::state::AppState, id: &str| {
            s.sidebar
                .profiles
                .iter()
                .find(|p| p.id == id)
                .and_then(|p| p.color)
        };
        let (work, play) = (s.add_profile("Work", false), s.add_profile("Play", false));
        let private = s.add_profile("Private", true);
        assert_eq!(color(&s, &work), Some(PROFILE_COLORS[0]));
        assert_eq!(color(&s, &play), Some(PROFILE_COLORS[1]));
        // The default profile is the plain one, and a private one is on no list.
        assert_eq!(color(&s, &s.default_profile_id()), None);
        assert_eq!(color(&s, &private), None);
        // One saved by an earlier version has none: it gets the first that
        // is free, and the others keep theirs.
        s.sidebar.profiles.push(Profile {
            id: "old".into(),
            name: "Old".into(),
            is_default: false,
            ephemeral: false,
            color: None,
        });
        s.sidebar.profiles.retain(|p| p.id != work);
        s.normalise_favorites();
        assert_eq!(color(&s, "old"), Some(PROFILE_COLORS[0]));
        assert_eq!(color(&s, &play), Some(PROFILE_COLORS[1]));
    }

    #[test]
    fn favorites_are_one_list_per_profile() {
        let mut s = state();
        let default = s.default_profile_id();
        s.sidebar.profiles.push(Profile {
            id: "work".into(),
            name: "Work".into(),
            is_default: false,
            ephemeral: false,
            color: None,
        });
        // Saved by an earlier version: one list, a favorite naming its own
        // profile, and a second list for a profile that is gone.
        s.sidebar.favorites[0].items = vec![
            tab("mail"),
            TabItem {
                profile_id: Some("work".into()),
                ..tab("chat")
            },
        ];
        s.sidebar.favorites.push(magnetite_core::FavoriteSet {
            profile_id: "gone".into(),
            items: vec![tab("news")],
        });
        s.normalise_favorites();
        let lists: Vec<(&str, Vec<&str>)> = s
            .sidebar
            .favorites
            .iter()
            .map(|f| {
                (
                    f.profile_id.as_str(),
                    f.items.iter().map(|t| t.id.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(
            lists,
            [
                (default.as_str(), vec!["mail", "news"]),
                ("work", vec!["chat"])
            ]
        );
        // A favorite runs in the profile of its list and names none itself.
        assert_eq!(s.profile_for_tab("chat").as_deref(), Some("work"));
        assert_eq!(s.find_tab("chat").unwrap().1.profile_id, None);
        assert!(s.profile_in_use("work"));
    }

    #[test]
    fn profiles_are_sorted_among_the_lasting_ones_and_the_favorites_follow() {
        let mut s = state();
        let default = s.default_profile_id();
        let profile = |id: &str, ephemeral| Profile {
            id: id.into(),
            name: id.into(),
            is_default: false,
            ephemeral,
            color: None,
        };
        s.sidebar.profiles.push(profile("secret", true));
        s.sidebar.profiles.push(profile("work", false));
        s.sidebar.profiles.push(profile("club", false));
        s.sidebar.favorites[0].items.push(tab("mail"));
        s.sidebar.favorites[0].items.push(TabItem {
            profile_id: Some("club".into()),
            ..tab("chat")
        });
        s.normalise_favorites();
        let lasting = |s: &crate::state::AppState| -> Vec<String> {
            let profiles = s.sidebar.profiles.iter().filter(|p| !p.ephemeral);
            profiles.map(|p| p.id.clone()).collect()
        };
        // Places count the lasting profiles only, without the one that moves.
        s.move_profile("club", 0);
        assert_eq!(lasting(&s), ["club", default.as_str(), "work"]);
        let lists: Vec<&str> = (s.sidebar.favorites.iter())
            .map(|f| f.profile_id.as_str())
            .collect();
        assert_eq!(lists, ["club", default.as_str()], "the lists follow");
        s.move_profile(&default, 9);
        assert_eq!(lasting(&s), ["club", "work", default.as_str()]);
        assert_eq!(s.default_profile_id(), default, "still the default one");
        s.move_profile("club", 1);
        assert_eq!(lasting(&s), ["work", "club", default.as_str()]);
        // A private profile is on no list, and an unknown one is nothing.
        let before = s.sidebar.profiles.clone();
        s.move_profile("secret", 0);
        s.move_profile("gone", 0);
        assert_eq!(s.sidebar.profiles, before);
    }

    #[test]
    fn a_favorite_without_a_profile_is_in_front_in_the_default_one() {
        let mut s = state();
        s.sidebar.profiles.push(Profile {
            id: "work".into(),
            name: "Work".into(),
            is_default: false,
            ephemeral: false,
            color: None,
        });
        s.sidebar.spaces[0].profile_id = "work".into();
        s.sidebar.favorites[0].items.push(tab("fav"));
        let space = s.active_space.clone();
        // A tab of the space runs in the space's profile.
        assert_eq!(s.front_context().map(|c| c.id).as_deref(), Some("work"));
        // The favorite runs in the default profile, whatever the space uses.
        s.active_tab.insert(space, "fav".into());
        assert_eq!(s.front_context().map(|c| c.id), None);
    }
}
