//! Tabs: opening, showing, closing and moving them, and the minute tick
//! that unloads and archives.

use magnetite_core::sidebar::Section;
use magnetite_engine::{ContentId, Engine, host_of};

use crate::state::{AppState, folder_in_tree_mut, now};
use crate::{lock, trace, url_policy};

use super::{MagnetiteApp, WELCOME_URL};

/// Drag and drop: put tab `id` at `index` of `section` (of space
/// `space_id`); `folder` places it inside a pinned folder of that space.
/// Where `move_tab` puts a tab.
pub(super) struct Destination<'a> {
    pub(super) space: &'a str,
    pub(super) section: Section,
    pub(super) index: usize,
    /// A folder of the pinned tabs.
    pub(super) folder: Option<&'a str>,
    /// For the favorites: the profile whose list takes the tab (default: the
    /// profile the tab runs in).
    pub(super) profile: Option<&'a str>,
}

pub(super) fn move_tab(s: &mut AppState, id: &str, to: &Destination) {
    let Destination {
        space: space_id,
        section,
        index,
        folder,
        ..
    } = *to;
    // Worked out before the tab is taken out: a destination that is not
    // there must not lose the tab.
    let list = match section {
        Section::Favorites => {
            let exists = |p: &str| s.sidebar.profiles.iter().any(|x| x.id == p);
            let profile = (to.profile.map(str::to_owned))
                .filter(|p| exists(p))
                .or_else(|| s.profile_for_tab(id))
                .unwrap_or_else(|| s.default_profile_id());
            Some(profile)
        }
        Section::Pinned | Section::Today if s.space(space_id).is_none() => return,
        Section::Pinned | Section::Today => None,
    };
    let Some((mut tab, _)) = s.take_tab(id) else {
        return;
    };
    match section {
        Section::Favorites => {
            tab.home_url.get_or_insert_with(|| tab.url.clone());
            // A favorite runs in the profile of its list.
            tab.profile_id = None;
            let profile_id = list.unwrap_or_default();
            let lists = &mut s.sidebar.favorites;
            let at = lists
                .iter()
                .position(|f| f.profile_id == profile_id)
                .unwrap_or_else(|| {
                    lists.push(magnetite_core::FavoriteSet {
                        profile_id,
                        items: vec![],
                    });
                    lists.len() - 1
                });
            let i = index.min(lists[at].items.len());
            lists[at].items.insert(i, tab);
        }
        Section::Pinned => {
            tab.home_url.get_or_insert_with(|| tab.url.clone());
            let Some(sp) = s.space_mut(space_id) else {
                return;
            };
            let node = magnetite_core::SidebarNode::Tab(tab);
            if let Some(f) = folder.and_then(|fid| folder_in_tree_mut(&mut sp.pinned, fid)) {
                let i = index.min(f.children.len());
                f.children.insert(i, node);
            } else {
                let i = index.min(sp.pinned.len());
                sp.pinned.insert(i, node);
            }
        }
        Section::Today => {
            // From Pinned or the favorites: its time in Today starts now, or
            // the minute tick would archive it for the hours before.
            if tab.home_url.take().is_some() {
                tab.last_active_at = Some(now());
            }
            if let Some(sp) = s.space_mut(space_id) {
                let i = index.min(sp.today.len());
                sp.today.insert(i, tab);
            }
        }
    }
    // A list that got its first favorite takes its place; one that lost its
    // last is gone.
    s.normalise_favorites();
}

impl MagnetiteApp {
    /// Make `tab_id` the active tab of its space (switching spaces if needed),
    /// creating its content window on first use.
    pub(super) fn activate_tab(&self, engine: &Engine, tab_id: &str) {
        self.close_peek(engine);
        let space_id = self.with_state(|s| {
            let (space, _) = s.find_tab(tab_id)?;
            Some(space.map_or_else(|| s.active_space.clone(), |sp| sp.id.clone()))
        });
        let Some(space_id) = space_id else {
            trace(|| format!("activate_tab {tab_id}: not found"));
            return;
        };

        self.with_state(|s| {
            s.active_space.clone_from(&space_id);
            s.active_tab.insert(space_id, tab_id.to_string());
            if let Some(t) = s.find_tab_mut(tab_id) {
                t.last_active_at = Some(now());
                t.unread = false;
            }
        });
        self.present(engine);
    }

    /// Make the content windows match the state: create windows for the
    /// visible panes, lay them out, hide everything else.
    pub(super) fn present(&self, engine: &Engine) {
        let panes = self.with_state(|s| s.visible_panes());
        let mut contents = Vec::new();
        for tab_id in &panes {
            let c = if let Some(c) = self.content_of(tab_id) {
                c
            } else {
                let (url, home, profile) = self.with_state(|s| {
                    let (url, home) = s
                        .find_tab(tab_id)
                        .map(|(_, t)| (t.url.clone(), t.home_url.clone()))
                        .unwrap_or_default();
                    // Magnetite's own pages are served by the default context only.
                    let profile = if url.starts_with("magnetite://") {
                        None
                    } else {
                        s.context(s.profile_for_tab(tab_id).as_deref())
                    };
                    (url, home, profile)
                });
                let c = engine.create_content(&url, profile.as_ref());
                self.with_state(|s| {
                    s.content_to_tab.insert(c, tab_id.clone());
                    let rt = s.runtime.entry(tab_id.clone()).or_default();
                    rt.content = Some(c);
                    rt.loaded = true;
                });
                engine.set_content_lock(c, home.as_deref().and_then(host_of));
                c
            };
            contents.push(c);
        }
        let others: Vec<ContentId> = self.with_state(|s| {
            s.runtime
                .values()
                .filter_map(|r| r.content)
                .filter(|c| !contents.contains(c))
                .collect()
        });
        for c in others {
            engine.hide_content(c);
        }
        match (self.split_on_screen(&panes, &contents), contents.as_slice()) {
            (_, []) => {}
            (Some((tree, focus)), _) => engine.show_split(&tree, focus),
            (None, [one, ..]) => engine.show_content(*one),
        }
        // A page that leaves the screen was in use until now: its unload and
        // auto-archive periods start here, not when it was selected.
        let left: Vec<ContentId> = lock(&self.visible)
            .iter()
            .filter(|c| !contents.contains(c))
            .copied()
            .collect();
        if !left.is_empty() {
            self.with_state(|s| {
                let now = now();
                for c in left {
                    if let Some(id) = s.content_to_tab.get(&c).cloned()
                        && let Some(t) = s.find_tab_mut(&id)
                    {
                        t.last_active_at = Some(now);
                    }
                }
            });
        }
        self.follow_pictures(engine, &contents);
        // A capture never moves to another tab: drop it if its page left the screen.
        let stale = {
            let state = lock(&self.capture);
            state
                .session
                .as_ref()
                .is_some_and(|(c, _)| !contents.contains(c))
        };
        if stale {
            engine.close_overlay();
            lock(&self.capture).session = None;
        }
        self.push_state(engine);
    }

    /// Pop-out: a video playing in a tab that just got hidden goes
    /// picture-in-picture; coming back to its tab ends picture-in-picture.
    pub(super) fn follow_pictures(&self, engine: &Engine, shown: &[ContentId]) {
        let previous = std::mem::replace(&mut *lock(&self.visible), shown.to_vec());
        let (auto_pip, media) = self.with_state(|s| {
            let media: std::collections::HashMap<ContentId, magnetite_engine::MediaState> = s
                .content_to_tab
                .iter()
                .filter_map(|(c, t)| Some((*c, s.runtime.get(t)?.media.clone()?)))
                .collect();
            (s.settings.auto_pip, media)
        });
        for c in previous.iter().filter(|c| !shown.contains(c)) {
            if auto_pip && media.get(c).is_some_and(|m| m.playing && m.video && !m.pip) {
                engine.content_picture_in_picture(*c, true);
            }
        }
        for c in shown.iter().filter(|c| !previous.contains(c)) {
            if media.get(c).is_some_and(|m| m.pip) {
                engine.content_picture_in_picture(*c, false);
            }
        }
    }

    pub(super) fn activate_space(&self, engine: &Engine, space_id: &str) {
        let switched = self.with_state(|s| {
            // A stale id (a space just deleted or replaced by sync) changes
            // nothing: the active space must always exist.
            s.space(space_id)?;
            let target = s.tab_for_space(space_id);
            // Before activating the tab: a favorite belongs to no space and
            // opens in the active one.
            s.active_space = space_id.to_string();
            Some(target)
        });
        match switched {
            Some(Some(tab)) => self.activate_tab(engine, &tab),
            // A space without tabs: every page leaves the screen, the other
            // pages of a split and a peek included.
            Some(None) => {
                self.close_peek(engine);
                self.present(engine);
            }
            None => {}
        }
    }

    /// Open `url` (typed, or from Magnetite's UI) as a new Today tab (see
    /// `open_url_from`).
    pub(super) fn open_url(&self, engine: &Engine, url: &str, background: bool) -> Option<String> {
        self.open_url_from(engine, url, background, url_policy::Source::User, None)
    }

    /// Open bookmark `id` as a new Today tab of the space it belongs to,
    /// which comes to the front first.
    pub(super) fn open_bookmark(&self, engine: &Engine, id: &str, background: bool) {
        let found = self.with_state(|s| {
            let (space, bookmark) = s.bookmark(id)?;
            Some((
                space.id.clone(),
                bookmark.url.clone(),
                s.active_space.clone(),
            ))
        });
        let Some((space, url, active)) = found else {
            return;
        };
        if space != active {
            self.activate_space(engine, &space);
        }
        // A saved address, perhaps from another device: not typed input.
        self.open_url_from(engine, &url, background, url_policy::Source::Stored, None);
    }

    /// Open `url` from `source` as a new Today tab of the active space, in
    /// the opener's `context` (profile id, `""` the default) when there is
    /// one, so a page opened from a private or separate profile does not get
    /// the space's cookies. Addresses the policy refuses are not opened.
    /// Returns the tab's id; internal pages are singletons and return the
    /// existing tab.
    pub(super) fn open_url_from(
        &self,
        engine: &Engine,
        url: &str,
        background: bool,
        source: url_policy::Source,
        context: Option<&str>,
    ) -> Option<String> {
        // Only typed input becomes a search; anything else is taken as given.
        let url = if source == url_policy::Source::User {
            self.normalise_url(url)
        } else {
            url.trim().to_string()
        };
        if !url_policy::allowed(&url, source) {
            trace(|| format!("not opening {source:?} address with this scheme"));
            return None;
        }
        let title = if url.starts_with("magnetite://ui/settings") {
            "Settings".to_string()
        } else if url.starts_with(WELCOME_URL) {
            "Welcome".to_string()
        } else {
            url.clone()
        };
        // An internal page that is open already, asked for at another place.
        let mut revisit = false;
        let id = self.with_state(|s| {
            let space = s.active_space.clone();
            // Internal pages are singletons: focus an existing one instead.
            // The part after `#` is a place in the page (a section of
            // Settings), not another page.
            let page = |u: &str| u.split_once('#').map_or(u, |(page, _)| page).to_string();
            if url.starts_with("magnetite://")
                && let Some(existing) = s
                    .active_space()
                    .today
                    .iter()
                    .find(|t| page(&t.url) == page(&url))
                    .map(|t| t.id.clone())
            {
                revisit = s.find_tab(&existing).is_some_and(|(_, t)| t.url != url);
                return existing;
            }
            let id = s.new_today_tab(&space, &url, &title);
            let space_profile = s.active_space().profile_id.clone();
            // An override only when it differs from the space's profile.
            let profile = context
                .and_then(|c| s.profile_for_context(c))
                .filter(|p| *p != space_profile);
            if let Some(t) = s.find_tab_mut(&id) {
                t.unread = background;
                t.profile_id = profile;
            }
            id
        });
        if background {
            self.push_state(engine);
        } else {
            self.activate_tab(engine, &id);
        }
        // The open page goes to the place asked for (Settings follows it).
        if revisit && let Some(c) = self.content_of(&id) {
            engine.content_load_url(c, &url);
        }
        Some(id)
    }

    /// Load `url` in the page of tab `tab_id`. Magnetite's own pages are
    /// served by the default profile only (`present`), so a tab that changes
    /// between such a page and a website gets a new page window in the right
    /// profile: reusing the old one would run the website in the default
    /// profile, also in a private space.
    pub(super) fn load_in_tab(&self, engine: &Engine, tab_id: &str, url: &str) {
        let ours = |u: &str| u.starts_with("magnetite://");
        let (content, swap) = self.with_state(|s| {
            let content = s.runtime.get(tab_id).and_then(|r| r.content);
            let swap = s
                .find_tab(tab_id)
                .is_some_and(|(_, t)| ours(&t.url) != ours(url));
            (content, swap)
        });
        match content {
            Some(c) if swap => {
                self.with_state(|s| {
                    if let Some(t) = s.find_tab_mut(tab_id) {
                        t.url = url.to_string();
                    }
                    s.detach_content(tab_id);
                });
                engine.close_content(c);
                self.present(engine);
            }
            Some(c) => engine.content_load_url(c, url),
            None => {}
        }
    }

    pub(super) fn close_tab(&self, engine: &Engine, tab_id: &str) {
        let (content, next, was_active, on_screen) = self.with_state(|s| {
            let was_active = s.active_tab_id() == Some(tab_id);
            let panes = s.visible_panes();
            let on_screen = panes.iter().any(|t| t == tab_id);
            let order = s.visible_tab_ids();
            let next = match order.iter().position(|t| t == tab_id) {
                Some(p) => order
                    .get(p + 1)
                    .or_else(|| p.checked_sub(1).and_then(|q| order.get(q))),
                // No row in this space (a favorite, or a page of another
                // space in the split): another page on screen, else the
                // space's first tab.
                None => panes.iter().find(|t| *t != tab_id).or(order.first()),
            }
            .cloned();
            // By where the tab is, not by its home page: a favorite, or
            // anything of a space that is not in Today.
            let is_pinned = s.find_tab(tab_id).is_some_and(|(space, _)| {
                space.is_none_or(|sp| !sp.today.iter().any(|t| t.id == tab_id))
            });
            let content = if is_pinned {
                // Pinned items and favorites stay; closing unloads them and
                // takes them out of a split.
                s.split_remove(tab_id);
                s.detach_content(tab_id)
            } else {
                s.archive_tab(tab_id)
            };
            // Nothing to move on to: no tab is in front (an unloaded pinned
            // tab must not stay "active" and load again by itself).
            if was_active && next.is_none() {
                let space = s.active_space.clone();
                s.active_tab.remove(&space);
            }
            (content, next, was_active, on_screen)
        });
        if let Some(c) = content {
            engine.close_content(c);
        }
        match (was_active, next) {
            (true, Some(n)) => self.activate_tab(engine, &n),
            // What is on screen changed: lay it out again.
            _ if on_screen => self.present(engine),
            _ => self.push_state(engine),
        }
    }

    /// Pin tab `id` at the top of its space's pinned tabs, or move it back
    /// to Today. Favorites belong to no space and stay as they are.
    pub(super) fn set_pinned(&self, engine: &Engine, id: &str, pinned: bool) {
        self.with_state(|s| {
            let space = s.find_tab(id).and_then(|(sp, _)| sp.map(|x| x.id.clone()));
            if let Some(sp) = space.and_then(|sid| s.space_mut(&sid)) {
                if pinned {
                    sp.pin(id);
                } else if sp.unpin(id)
                    && let Some(t) = sp.today.first_mut()
                {
                    // Its time in Today starts now: it must not be archived
                    // for the hours it was pinned and not looked at.
                    t.last_active_at = Some(now());
                }
            }
        });
        self.apply_lock(engine, id);
        self.push_state(engine);
    }

    /// Lock the page of `tab_id` to its home site, or free it, as the tab is
    /// pinned now (the minute tick would do it late).
    pub(super) fn apply_lock(&self, engine: &Engine, tab_id: &str) {
        let lock = self.with_state(|s| {
            let content = s.runtime.get(tab_id)?.content?;
            let host = s
                .find_tab(tab_id)
                .and_then(|(_, t)| t.home_url.clone())
                .as_deref()
                .and_then(host_of);
            Some((content, host))
        });
        if let Some((content, host)) = lock {
            engine.set_content_lock(content, host);
        }
    }

    pub(super) fn reopen_closed(&self, engine: &Engine) {
        let restored = self.with_state(|s| {
            let last = s.archive.len().checked_sub(1)?;
            s.restore_archived(last)
        });
        if let Some(id) = restored {
            self.activate_tab(engine, &id);
        }
    }

    /// Once a minute: archive stale Today tabs, prune the archive and the
    /// history (and save the history), unload long-hidden content windows,
    /// keep locks in sync with pin state.
    pub(super) fn housekeeping(&self, engine: &Engine) {
        let (to_close, changed, locks) = self.with_state(|s| {
            let now = now();
            // Never archived or unloaded: the pages on screen (all of a
            // split's) and the tab each other space returns to.
            let mut active: std::collections::HashSet<String> =
                s.active_tab.values().cloned().collect();
            active.extend(s.visible_panes());
            let mut to_close = Vec::new();
            let mut changed = false;
            // Auto-archive Today tabs not viewed for the period.
            for id in s.stale_today_tabs(now, &active) {
                to_close.extend(s.archive_tab(&id));
                changed = true;
            }
            // The archive and the history forget what is older than their
            // retention periods.
            changed |= s.prune_archive(now);
            changed |= s.history.prune(s.settings.history_days, now);
            s.history.save();
            // Unload content windows of tabs hidden for a long time.
            for id in s.tabs_to_unload(now, &active) {
                to_close.extend(s.detach_content(&id));
                changed = true;
            }
            // Locks follow the current pin state.
            let locks: Vec<(ContentId, Option<String>)> = s
                .runtime
                .iter()
                .filter_map(|(id, r)| r.content.map(|c| (id, c)))
                .map(|(id, c)| {
                    (
                        c,
                        s.find_tab(id)
                            .and_then(|(_, t)| t.home_url.clone())
                            .as_deref()
                            .and_then(host_of),
                    )
                })
                .collect();
            (to_close, changed, locks)
        });
        for c in to_close {
            engine.close_content(c);
        }
        for (c, host) in locks {
            engine.set_content_lock(c, host);
        }
        if changed {
            self.push_state(engine);
        } else {
            self.with_state(|s| s.persist());
        }
    }
}
