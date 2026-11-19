//! Tabs: opening, showing, closing and moving them, and the minute tick
//! that unloads and archives.

use magnetite_core::sidebar::Section;
use magnetite_engine::{ContentId, Engine, MemoryPressure, host_of};

use crate::state::{AppState, TabOrder, folder_in_tree_mut, now};
use crate::{lock, trace, url_policy};

use super::{DOWNLOADS_URL, MagnetiteApp, WELCOME_URL, global};

/// The list of the tabs `Ctrl+Tab` goes through (`Switcher.tsx`).
const SWITCHER_URL: &str = "magnetite://ui/switcher";
/// Its width and the height of one of its rows, in points at the normal
/// text size (`.switcher .result`).
const SWITCHER_WIDTH: f64 = 440.0;
const SWITCHER_ROW: f64 = 40.0;
/// How long the modifier is held before the list shows.
const SWITCHER_DELAY_MS: i64 = 150;
/// How often a round looks whether the modifier is still held.
const SWITCHER_WATCH_MS: i64 = 30;

/// Drag and drop: put tab `id` at `index` of `section` (of space
/// `space_id`); `folder` places it inside a pinned folder of that space.
/// Where `move_tab` puts a tab.
#[derive(Clone, Copy)]
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

/// Put the tabs `ids` at `to`, one after the other in their order; the
/// index counts the list without them.
pub(super) fn move_tabs(s: &mut AppState, ids: &[String], to: &Destination) {
    if let [id] = ids {
        return move_tab(s, id, to);
    }
    // First all to the end of the list: one still standing above the place
    // would shift it.
    let end = Destination {
        index: usize::MAX,
        ..*to
    };
    for id in ids {
        move_tab(s, id, &end);
    }
    for (i, id) in ids.iter().enumerate() {
        let index = to.index.saturating_add(i);
        move_tab(s, id, &Destination { index, ..*to });
    }
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
    // Nothing moves into the private space or out of it, and inside it
    // there are only Today tabs: a tab kept elsewhere would outlive it.
    let from = s
        .find_tab(id)
        .and_then(|(sp, _)| sp.map(|sp| sp.id.clone()));
    let private = |space: Option<&str>| space.is_some_and(|sp| s.is_private_space(sp));
    let to_private = section != Section::Favorites && private(Some(space_id));
    if private(from.as_deref()) != to_private || (to_private && section != Section::Today) {
        return;
    }
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
            // Only favorites and pinned tabs open as web panels.
            tab.end_panel();
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
        // A favorite that opens as a web panel never takes the page area.
        if self.with_state(|s| s.is_web_panel(tab_id)) {
            self.open_web_panel(engine, tab_id);
            return;
        }
        self.close_peek(engine);
        let space_id = self.with_state(|s| {
            let (space, _) = s.find_tab(tab_id)?;
            Some(space.map_or_else(|| s.active_space.clone(), |sp| sp.id.clone()))
        });
        let Some(space_id) = space_id else {
            trace(|| format!("activate_tab {tab_id}: not found"));
            return;
        };

        let switching = self.with_state(|s| {
            // A tab of the hidden private space is gone to only by the
            // user's own doing (a private mini window kept as a tab), which
            // shows the space again.
            if s.is_hidden_space(&space_id) {
                s.private_hidden = false;
            }
            s.active_space.clone_from(&space_id);
            s.active_tab.insert(space_id, tab_id.to_string());
            s.note_tab_use(tab_id);
            if let Some(t) = s.find_tab_mut(tab_id) {
                t.last_active_at = Some(now());
                t.unread = false;
            }
            // Chosen some other way while Ctrl is held: the round is over.
            s.switcher.take().is_some()
        });
        if switching {
            engine.close_overlay();
        }
        self.present(engine);
    }

    /// One press of `Ctrl+Tab` or `Ctrl+Shift+Tab` in the order Settings
    /// say. By last use the first press begins a round, which lasts while
    /// the modifier is held (`watch_tab_switch`): the tab changes when it
    /// is let go, and a round longer than `SWITCHER_DELAY_MS` shows its
    /// list.
    pub(super) fn cycle_tab(&self, engine: &Engine, forward: bool) {
        if self.with_state(|s| s.settings.tab_order) == TabOrder::Sidebar {
            if let Some(t) = self.with_state(|s| s.cycle_tab(forward)) {
                self.activate_tab(engine, &t);
            }
            return;
        }
        // The round a first press began, if this was one.
        let began = self.with_state(|s| {
            let began = s.switcher.is_none();
            if !s.switch_tab(forward) {
                return None;
            }
            let round = s.switcher.as_ref()?.round;
            Some(began.then_some(round))
        });
        match began {
            Some(Some(round)) => {
                Self::watch_tab_switch();
                // A quick press and release just switches, without the list:
                // it shows only if this round is still on after the delay.
                Engine::post_delayed(
                    Box::new(move || {
                        let Some(app) = global() else { return };
                        let size = app.with_state(|s| {
                            let on = s.switcher.as_ref().filter(|r| r.round == round)?;
                            // The page zooms with the text size; so must its window.
                            let scale =
                                f64::from(s.settings.appearance.clone().clamped().text_scale)
                                    / 100.0;
                            let rows = f64::from(u32::try_from(on.tabs.len()).unwrap_or(1));
                            let points = |n: f64| (n * scale).ceil();
                            Some((points(SWITCHER_WIDTH), points(SWITCHER_ROW * rows + 12.0)))
                        });
                        if let Some((width, height)) = size {
                            #[allow(clippy::cast_possible_truncation)] // a few hundred points
                            Engine::global().open_panel(SWITCHER_URL, width as i32, height as i32);
                        }
                    }),
                    SWITCHER_DELAY_MS,
                );
            }
            // The list follows the round.
            Some(None) => self.show_state(engine),
            None => {}
        }
    }

    /// Look every `SWITCHER_WATCH_MS` whether Ctrl (the modifiers of the
    /// shortcut's keys, should the user have changed them) is still held,
    /// and end the round of `Ctrl+Tab` once it is not. Where
    /// the system does not say, the round ends at once: every press
    /// switches to the tab used before.
    fn watch_tab_switch() {
        Engine::post_delayed(
            Box::new(|| {
                let Some(app) = global() else { return };
                let engine = Engine::global();
                if app.with_state(|s| s.switcher.is_none()) {
                    return;
                }
                if engine.tab_switch_keys_held() == Some(true) {
                    Self::watch_tab_switch();
                } else {
                    app.end_tab_switch(engine);
                }
            }),
            SWITCHER_WATCH_MS,
        );
    }

    /// Ctrl was let go: the tab the round of `Ctrl+Tab` stands on is chosen.
    fn end_tab_switch(&self, engine: &Engine) {
        let Some(chosen) = self.with_state(|s| s.switcher.is_some().then(|| s.end_tab_switch()))
        else {
            return;
        };
        engine.close_overlay();
        if let Some(tab) = chosen {
            self.activate_tab(engine, &tab);
        }
    }

    /// Make the content windows match the state: the current window shows
    /// its panes, and a window that showed one of them until now (a tab is
    /// on screen in one window at most) shows what it has left.
    pub(super) fn present(&self, engine: &Engine) {
        self.dev_drop_stale_phones(engine);
        let before = self.on_screen();
        let robbed = self.with_state(AppState::claim_panes);
        // The current window first: a page that moves leaves the other
        // window by being shown here.
        self.present_window(engine);
        for win in robbed {
            engine.in_window(win, || self.present_window(engine));
        }
        let shown = self.on_screen();
        // A page that leaves the screen was in use until now: its unload and
        // auto-archive periods start here, not when it was selected.
        let left: Vec<ContentId> = (before.iter())
            .filter(|c| !shown.contains(c))
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
        self.follow_pictures(engine, &before, &shown);
        // A capture never moves to another tab: drop it if its page left the screen.
        let stale = {
            let state = lock(&self.capture);
            state
                .session
                .as_ref()
                .is_some_and(|(c, _)| !shown.contains(c))
        };
        if stale {
            engine.close_overlay();
            lock(&self.capture).session = None;
        }
        self.push_state(engine);
    }

    /// The page window of tab `tab_id` for the current window, made if the
    /// tab has none.
    pub(super) fn page_of(&self, engine: &Engine, tab_id: &str) -> ContentId {
        let win = engine.current_window();
        // A page in another window comes over as it is where the engine
        // can move it; elsewhere it is closed there and made again here.
        let kept = self.content_of(tab_id).filter(|c| {
            let movable = engine.content_window(*c) == win || engine.can_move_pages();
            if !movable {
                self.with_state(|s| s.detach_content(tab_id));
                engine.close_content(*c);
            }
            movable
        });
        if let Some(c) = kept {
            return c;
        }
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
            s.content_to_tab.insert(c, tab_id.to_string());
            let rt = s.runtime.entry(tab_id.to_string()).or_default();
            rt.content = Some(c);
            rt.loaded = true;
        });
        engine.set_content_lock(c, home.as_deref().and_then(host_of));
        c
    }

    /// The current window's part of `present`: create windows for its
    /// panes, lay them out, hide its other pages.
    fn present_window(&self, engine: &Engine) {
        let win = engine.current_window();
        let panes = self.with_state(|s| s.visible_panes());
        let mut contents = Vec::new();
        for tab_id in &panes {
            contents.push(self.page_of(engine, tab_id));
        }
        // The pages of the other windows are theirs to show or hide, and
        // the web panel stays over whatever is shown.
        let over = engine.web_panel_content();
        // The phone of a responsive split shows with its tab.
        let phones: Vec<ContentId> = self.with_state(|s| {
            (s.phones.iter())
                .filter(|p| panes.contains(&p.tab))
                .map(|p| p.page.content)
                .collect()
        });
        let others: Vec<ContentId> = self.with_state(|s| {
            s.runtime
                .values()
                .filter_map(|r| r.content)
                .chain(s.phones.iter().map(|p| p.page.content))
                .filter(|c| !contents.contains(c) && !phones.contains(c) && Some(*c) != over)
                .filter(|c| engine.content_window(*c) == win)
                .collect()
        });
        for c in others {
            engine.hide_content(c);
        }
        match (
            self.split_on_screen(engine, &panes, &contents),
            contents.as_slice(),
        ) {
            (_, []) => {}
            (Some((tree, focus)), _) => engine.show_split(&tree, focus),
            (None, [one, ..]) => engine.show_content(*one),
        }
        contents.extend(phones);
        lock(&self.visible).insert(win, contents);
    }

    /// Pop-out: a video playing in a tab that just got hidden goes
    /// picture-in-picture; coming back to its tab ends picture-in-picture.
    /// `previous` and `shown` are the pages on screen before and now, in
    /// any window: a page that moves to another window stays on screen.
    pub(super) fn follow_pictures(
        &self,
        engine: &Engine,
        previous: &[ContentId],
        shown: &[ContentId],
    ) {
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
            // pages of a split and a peek included. A round of `Ctrl+Tab`
            // is over, as when a tab is chosen.
            Some(None) => {
                if self.with_state(|s| s.switcher.take().is_some()) {
                    engine.close_overlay();
                }
                self.close_peek(engine);
                self.present(engine);
            }
            None => {}
        }
    }

    /// Open the private space, or go to it if it is open.
    pub(super) fn open_private_space(&self, engine: &Engine) {
        let id = self.with_state(AppState::open_private_space);
        self.activate_space(engine, &id);
    }

    /// A link from another app is not private browsing: with the private
    /// space in front, the space beside it comes to the front for it.
    pub(super) fn leave_private_space(&self, engine: &Engine) {
        let beside = self.with_state(|s| {
            (s.is_private_space(&s.active_space)).then(|| s.lasting_space(&s.active_space))
        });
        if let Some(space) = beside {
            self.activate_space(engine, &space);
        }
    }

    /// Hide the private space: nothing lists it any more, its pages are
    /// paused and taken back from picture-in-picture, and every window
    /// that showed it shows the space it goes on in. It stays open;
    /// `open_private_space` shows it again.
    pub(super) fn hide_private_space(&self, engine: &Engine) {
        let others = self.with_state(|s| s.other_windows_in_private_space());
        let Some(pages) = self.with_state(AppState::hide_private_space) else {
            return;
        };
        for c in pages {
            engine.content_media(c, magnetite_engine::MediaAction::Pause);
            engine.content_picture_in_picture(c, false);
        }
        let show = || {
            let space = self.with_state(|s| s.active_space.clone());
            self.activate_space(engine, &space);
        };
        for win in others {
            engine.in_window(win, show);
        }
        show();
    }

    /// Close the private space with everything opened from it. Every
    /// window that showed it shows the space it goes on in.
    pub(super) fn close_private_space(&self, engine: &Engine) {
        let others = self.with_state(|s| s.other_windows_in_private_space());
        for c in self.with_state(AppState::close_private_space) {
            engine.close_content(c);
        }
        let show = || {
            let space = self.with_state(|s| s.active_space.clone());
            self.activate_space(engine, &space);
        };
        for win in others {
            engine.in_window(win, show);
        }
        show();
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
    /// the space's cookies. A private opener's tab goes to the private space
    /// instead, and no other one into it (`open_tab_from`). Addresses the
    /// policy refuses are not opened.
    /// Returns the tab's id; internal pages are singletons and return the
    /// existing tab.
    pub(super) fn open_url_from(
        &self,
        engine: &Engine,
        url: &str,
        mut background: bool,
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
        } else if url.starts_with(DOWNLOADS_URL) {
            "Downloads".to_string()
        } else {
            url.clone()
        };
        // An internal page that is open already, asked for at another place.
        let mut revisit = false;
        let id = self.with_state(|s| {
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
                return Some(existing);
            }
            let opener = context.and_then(|c| s.profile_for_context(c));
            let id = s.open_tab_from(opener.as_deref(), &url, &title)?;
            // What a page of the hidden private space opens stays out of
            // sight with it: a tab in the background.
            let (space, _) = s.find_tab(&id)?;
            background |= space.is_some_and(|sp| s.is_hidden_space(&sp.id));
            if let Some(t) = s.find_tab_mut(&id) {
                t.unread = background;
            }
            Some(id)
        })?;
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

    /// Open `url`, a link from another app, as a background Today tab of
    /// space `space` (the one a link rule names), which stays where it is:
    /// the tab is marked unread and runs in that space's profile.
    pub(super) fn open_background_in_space(&self, engine: &Engine, url: &str, space: &str) {
        let url = url.trim();
        if !url_policy::allowed(url, url_policy::Source::OtherApp) {
            trace(|| "not opening an address from another app with this scheme".to_string());
            return;
        }
        self.with_state(|s| {
            let id = s.new_today_tab(space, url, url);
            if let Some(t) = s.find_tab_mut(&id) {
                t.unread = true;
            }
        });
        self.push_state(engine);
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

    /// Close tab `tab_id`. A Today tab goes to the archive; a favorite or a
    /// pinned tab is unloaded and stays. A kept split (`kept_split`) closes
    /// as the one item it is: all its pages are unloaded, and it stays.
    pub(super) fn close_tab(&self, engine: &Engine, tab_id: &str) {
        let (contents, next, was_active, on_screen) = self.with_state(|s| {
            let kept = s.kept_split(tab_id);
            let closing = kept.clone().unwrap_or_else(|| vec![tab_id.to_owned()]);
            let was_active = s
                .active_tab_id()
                .is_some_and(|t| closing.iter().any(|c| c == t));
            let panes = s.visible_panes();
            let on_screen = panes.iter().any(|t| t == tab_id);
            let order = s.visible_tab_ids();
            let stays = |t: &&String| !closing.contains(t);
            let next = match order.iter().position(|t| t == tab_id) {
                Some(p) => {
                    (order[p..].iter().find(stays)).or_else(|| order[..p].iter().rev().find(stays))
                }
                // No row in this space (a favorite, or a page of another
                // space in the split): another page on screen, else the
                // space's first tab.
                None => (panes.iter().find(stays)).or_else(|| order.iter().find(stays)),
            }
            .cloned();
            // By where the tab is, not by its home page: a favorite, or
            // anything of a space that is not in Today.
            let is_pinned = s.find_tab(tab_id).is_some_and(|(space, _)| {
                space.is_none_or(|sp| !sp.today.iter().any(|t| t.id == tab_id))
            });
            let contents: Vec<ContentId> = if is_pinned {
                // Pinned items and favorites stay; closing unloads them and
                // sends them back to their pinned page, which they open at
                // next time. One that shares a split with a Today tab leaves
                // that split.
                if kept.is_none() {
                    s.split_remove(tab_id);
                }
                let unload = |s: &mut AppState, id: &String| {
                    if let Some(t) = s.find_tab_mut(id)
                        && let Some(home) = t.home_url.clone()
                    {
                        t.url = home;
                    }
                    s.detach_content(id)
                };
                closing.iter().filter_map(|id| unload(s, id)).collect()
            } else {
                s.archive_tab(tab_id).into_iter().collect()
            };
            // Nothing to move on to: no tab is in front (an unloaded pinned
            // tab must not stay "active" and load again by itself).
            if was_active && next.is_none() {
                let space = s.active_space.clone();
                s.active_tab.remove(&space);
            }
            (contents, next, was_active, on_screen)
        });
        for c in contents {
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
    /// to Today; the pages of its split that stand with it go along
    /// (`split_group`). Favorites belong to no space and stay as they are.
    pub(super) fn set_pinned(&self, engine: &Engine, id: &str, pinned: bool) {
        let ids = self.with_state(|s| {
            let ids = s.split_group(id);
            let space = s.find_tab(id).and_then(|(sp, _)| sp.map(|x| x.id.clone()));
            // The private space has only Today tabs.
            let space = space.filter(|sid| !s.is_private_space(sid));
            if let Some(sp) = space.and_then(|sid| s.space_mut(&sid)) {
                // Each goes to the top: the last one first keeps their order.
                for id in ids.iter().rev() {
                    if pinned {
                        sp.pin(id);
                    } else if sp.unpin(id)
                        && let Some(t) = sp.today.first_mut()
                    {
                        // Its time in Today starts now: it must not be archived
                        // for the hours it was pinned and not looked at.
                        t.last_active_at = Some(now());
                        // A Today tab opens as no web panel.
                        t.end_panel();
                    }
                }
            }
            ids
        });
        for id in &ids {
            self.apply_lock(engine, id);
        }
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

    /// The system is short of memory: unload pages out of sight now, not
    /// when their unload time is up.
    pub(super) fn unload_under_pressure(&self, engine: &Engine, level: MemoryPressure) {
        let to_close: Vec<ContentId> = self.with_state(|s| {
            let ids = s.tabs_to_unload_under_pressure(level, now(), &s.pages_in_use());
            ids.iter().filter_map(|id| s.detach_content(id)).collect()
        });
        if to_close.is_empty() {
            return;
        }
        for c in to_close {
            engine.close_content(c);
        }
        self.push_state(engine);
    }

    /// Once a minute: archive stale Today tabs, prune the archive and the
    /// history (and save the history), unload long-hidden content windows,
    /// keep locks in sync with pin state.
    pub(super) fn housekeeping(&self, engine: &Engine) {
        let (to_close, changed, locks) = self.with_state(|s| {
            let now = now();
            let active = s.pages_in_use();
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
            self.save_soon();
        }
    }
}

impl MagnetiteApp {
    /// Another window on the same sidebar: the same space in front, no tab
    /// yet. It is the current window from here on.
    pub(super) fn new_window(&self, engine: &Engine) {
        let win = engine.create_frame();
        self.with_state(|s| {
            s.open_window(win);
            s.persist_later();
        });
    }

    /// At start: the windows of the last run besides the first one open
    /// again, each showing what it showed, if several windows are on
    /// (Settings); else they are forgotten.
    pub(super) fn reopen_windows(&self, engine: &Engine) {
        let (reopen, on) =
            self.with_state(|s| (std::mem::take(&mut s.reopen), s.settings.several_windows));
        if !on {
            return;
        }
        for saved in reopen {
            let win = engine.create_frame();
            self.with_state(|s| s.reopen_window(win, saved));
        }
    }

    /// Window `win` closed, one of several: its view goes. Its pages went
    /// to another window or closed with it (`Engine::close_window`).
    pub(super) fn window_closed(&self, engine: &Engine, win: magnetite_engine::WindowId) {
        // Quitting closes every window; each one stays in the state, so any
        // save from here on keeps them all for the next start.
        if engine.is_quitting() {
            return;
        }
        let next = engine.current_window();
        self.with_state(|s| {
            s.close_window(win, next);
            s.persist_later();
        });
        lock(&self.visible).remove(&win);
        self.push_state(engine);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::{state, tab};
    use magnetite_core::split::Side;

    fn ids(tabs: &[magnetite_core::TabItem]) -> Vec<&str> {
        tabs.iter().map(|t| t.id.as_str()).collect()
    }

    #[test]
    fn a_group_of_tabs_lands_in_order_at_the_place_counted_without_it() {
        let mut s = state();
        let space = s.active_space.clone();
        s.sidebar.spaces[0].today.push(tab("d"));
        // `a` and `c` stand above the place and would shift it one by one.
        let to = Destination {
            space: &space,
            section: Section::Today,
            index: 1,
            folder: None,
            profile: None,
        };
        move_tabs(&mut s, &["a".into(), "c".into()], &to);
        assert_eq!(ids(&s.sidebar.spaces[0].today), ["b", "a", "c", "d"]);
    }

    #[test]
    fn a_split_added_to_the_favorites_is_kept_with_its_pages_side_by_side() {
        let mut s = state();
        let space = s.active_space.clone();
        assert!(s.split_insert(Some("a"), "c", Side::Right));
        let to = Destination {
            space: &space,
            section: Section::Favorites,
            index: 0,
            folder: None,
            profile: None,
        };
        let split = s.split_group("a");
        move_tabs(&mut s, &split, &to);
        assert_eq!(ids(&s.sidebar.favorites[0].items), ["a", "c"]);
        assert_eq!(s.kept_split("c"), Some(vec!["a".into(), "c".into()]));
    }
}
