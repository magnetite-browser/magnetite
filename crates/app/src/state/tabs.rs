//! Tabs in the state: finding them, taking them out, the archive and its
//! retention, and deleting a space.

use std::borrow::Cow;
use std::collections::HashSet;

use magnetite_core::split::SplitNode;
use magnetite_core::unread::unread_count;
use magnetite_core::{Color, Id, SidebarNode, Space, TabItem, UnixSeconds};
use magnetite_engine::{ContentId, MediaState, MemoryPressure};

use super::history::page_of;
use super::settings::{MAX_ARCHIVE_DAYS, NEVER_ARCHIVE};
use super::{
    AppState, ArchivedTab, ChosenProfile, NewTabPlace, Peek, PeekOver, TabRuntime, new_id, now,
};

/// The page of `split` whose row shows the split in space `sp`: the first
/// of its pages, in reading order, with a row there, as the sidebar draws
/// it (`App.tsx`). None when no page of it is in the space.
fn split_row(sp: &Space, split: &SplitNode<Id>) -> Option<Id> {
    split.ids().into_iter().find(|p| sp.find_tab(p).is_some())
}

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

/// How many tabs `recent_tabs` remembers.
const RECENT_TABS: usize = 200;
/// The most tabs one round of `Ctrl+Tab` offers: what its list shows
/// without scrolling.
const SWITCH_TABS: usize = 9;

/// A round of `Ctrl+Tab` by last use, while Ctrl is held: the tabs it goes
/// through (the active one first, then by last use) and where it stands.
/// Letting go of Ctrl chooses that tab.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TabSwitch {
    pub tabs: Vec<Id>,
    pub at: usize,
    /// Which round this is, counted while Magnetite runs: what was set off
    /// for one round (showing its list) must not act on the next.
    #[serde(skip)]
    pub round: u64,
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

    /// The page outside the sidebar in window `id`: a peek (on screen or
    /// not), a mini window's page, or the phone of a responsive split.
    pub fn page_outside(&self, id: ContentId) -> Option<&Peek> {
        (self.peeks.values())
            .chain(self.minis.iter())
            .chain(self.phones.iter().map(|p| &p.page))
            .find(|p| p.content == id)
    }

    pub fn page_outside_mut(&mut self, id: ContentId) -> Option<&mut Peek> {
        (self.peeks.values_mut())
            .chain(self.minis.iter_mut())
            .chain(self.phones.iter_mut().map(|p| &mut p.page))
            .find(|p| p.content == id)
    }

    /// What a peek opened now in the current window lies over.
    pub fn peek_over(&self) -> PeekOver {
        PeekOver::front(self.active_tab.get(&self.active_space), self.window)
    }

    /// The peek on screen in the current window: the one over the tab in
    /// front, or over the window while no tab is.
    pub fn peek(&self) -> Option<&Peek> {
        self.peeks.get(&self.peek_over())
    }

    /// Take the peek on screen in the current window out of the state.
    pub fn take_peek(&mut self) -> Option<Peek> {
        let over = self.peek_over();
        self.peeks.remove(&over)
    }

    /// The peeks on screen, at most one per window.
    pub fn peeks_on_screen(&self) -> impl Iterator<Item = &Peek> {
        let others = (self.views.iter()).map(|(win, view)| view.peek_over(*win));
        (std::iter::once(self.peek_over()).chain(others)).filter_map(|over| self.peeks.get(&over))
    }

    /// Take out the peeks that lost their place: a tab whose page is gone
    /// (closed, archived or unloaded: its peek goes with it), or a window
    /// that is gone or has a tab in front now. Returns their page windows.
    pub fn drop_stale_peeks(&mut self) -> Vec<ContentId> {
        let has_place = |over: &PeekOver| match over {
            PeekOver::Tab(tab) => {
                self.find_tab(tab).is_some()
                    && (self.runtime.get(tab)).is_some_and(|rt| rt.content.is_some())
            }
            PeekOver::Window(win) if *win == self.window => self.active_tab_id().is_none(),
            PeekOver::Window(win) => (self.views.get(win))
                .is_some_and(|view| !view.active_tab.contains_key(&view.active_space)),
        };
        let stale: Vec<PeekOver> = (self.peeks.keys())
            .filter(|over| !has_place(over))
            .cloned()
            .collect();
        (stale.iter())
            .filter_map(|over| self.peeks.remove(over))
            .map(|p| p.content)
            .collect()
    }

    /// The phone in window `id` is gone: its tab has none beside it.
    pub fn drop_phone(&mut self, id: ContentId) {
        let Some(at) = self.phones.iter().position(|p| p.page.content == id) else {
            return;
        };
        let phone = self.phones.remove(at);
        if let Some(rt) = self.runtime.get_mut(&phone.tab) {
            rt.dev.responsive = false;
        }
    }

    /// The page in window `id` could not load `url` and shows an error page
    /// there: that is its address now, without the last page's icon. No
    /// visit, as nothing was seen.
    pub fn load_failed(&mut self, id: ContentId, url: &str) {
        if let Some(p) = self.page_outside_mut(id) {
            url.clone_into(&mut p.url);
        }
        if let Some(tab) = self.content_to_tab.get(&id).cloned()
            && let Some(t) = self.find_tab_mut(&tab)
        {
            url.clone_into(&mut t.url);
        }
        if let Some(rt) = self.page_runtime(id) {
            rt.favicon = None;
        }
    }

    pub fn is_phone(&self, id: ContentId) -> bool {
        self.phones.iter().any(|p| p.page.content == id)
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

    /// Record page `id`'s new title (`EngineEvent::TitleChanged`). An
    /// unread count in it (`(3) WhatsApp`) goes to the tab's runtime, for
    /// the bubble on its row or tile, and stays out of the tab's title and
    /// the history: those are saved and synced, and a count there would be
    /// stale. A renamed tab keeps its name and still gets the count. The
    /// peek and the mini windows show their page's title as it is.
    pub fn set_page_title(&mut self, id: ContentId, title: &str) {
        if let Some(p) = self.page_outside_mut(id) {
            title.clone_into(&mut p.title);
        }
        let (count, title) = match unread_count(title) {
            Some((count, rest)) => (Some(count), Cow::Owned(rest)),
            None => (None, Cow::Borrowed(title)),
        };
        self.record_title(id, &title);
        let Some(tab) = self.content_to_tab.get(&id).cloned() else {
            return;
        };
        self.runtime.entry(tab.clone()).or_default().unread_count = count;
        if let Some(t) = self.find_tab_mut(&tab)
            && !t.custom_title
            && !title.is_empty()
        {
            t.title = title.into_owned();
        }
    }

    /// Record what page `id` plays now (`EngineEvent::MediaChanged`), and
    /// tell whether a sidebar shows the change. A sidebar shows a page's
    /// media in two places: the speaker on its tab's row while it plays
    /// sound (`Rows.tsx`), and the media bar, which lists the tabs that are
    /// not on screen in its window (`App.tsx`). A muted video that starts
    /// and stops in the tab on screen, as a feed's videos do while it
    /// scrolls, shows in neither: the next state the sidebar gets carries
    /// it. Neither does the media of a page outside the sidebar (the peek,
    /// a mini window, a phone).
    pub fn set_media(&mut self, id: ContentId, media: Option<MediaState>) -> bool {
        let sound = |m: &Option<MediaState>| m.as_ref().is_some_and(|m| m.playing && !m.silent);
        let outside = self.page_outside(id).is_some();
        let Some(rt) = self.page_runtime(id) else {
            return false;
        };
        let before = std::mem::replace(&mut rt.media, media);
        if outside || before == rt.media {
            return false;
        }
        if sound(&before) != sound(&rt.media) {
            return true;
        }
        // With another window, its media bar lists the tab.
        let on_screen =
            (self.content_to_tab.get(&id)).is_some_and(|tab| self.visible_panes().contains(tab));
        !on_screen || !self.views.is_empty()
    }

    /// The tabs a page that shares a tab could share instead, as with
    /// Chromium's "Share this tab instead": the loaded tabs of that page's
    /// profile (the share dialog offers no others), except the shared one
    /// and the sharing one, which would show the meeting in the meeting.
    pub fn share_targets(&self) -> Vec<&str> {
        // `None` for a page that is no tab (the peek, a mini window).
        let profile = |id: &str| self.find_tab(id).map(|_| self.profile_for_tab(id));
        let sharing: Vec<Option<String>> = (self.runtime.iter())
            .filter(|(_, rt)| rt.sharing)
            .filter_map(|(id, _)| profile(id))
            .collect();
        if sharing.is_empty() {
            return Vec::new();
        }
        let mut targets: Vec<&str> = (self.runtime.iter())
            .filter(|(_, rt)| rt.loaded && !rt.shared && !rt.sharing)
            .filter(|(id, _)| profile(id).is_some_and(|p| sharing.contains(&p)))
            .map(|(id, _)| id.as_str())
            .collect();
        targets.sort_unstable();
        targets
    }

    /// Turn the peek into a Today tab of the active space, reusing its
    /// window and what is known about its page. Returns the tab's id.
    pub fn keep_peek(&mut self) -> Option<Id> {
        let p = self.take_peek()?;
        let space = self.keep_space(&p.profile, &self.active_space);
        Some(self.keep_page(p, &space))
    }

    /// Turn the mini window's page in window `content` into a Today tab of
    /// space `space` (the active space if there is no such space), like a
    /// kept peek. Returns the tab's id.
    pub fn keep_mini(&mut self, content: ContentId, space: &str) -> Option<Id> {
        let pos = self.minis.iter().position(|m| m.content == content)?;
        let page = self.minis.remove(pos);
        let space = if self.space(space).is_some() {
            space
        } else {
            &self.active_space
        };
        let space = self.keep_space(&page.profile, space);
        Some(self.keep_page(page, &space))
    }

    /// A new Today tab for `url`, opened by a page of profile `opener`
    /// (none: by the user or another app) while the active space is in
    /// front. The tab runs in the opener's profile and stays on its side of
    /// the private space's border (`keep_space`): a private page's tab goes
    /// to the private space, also when another space is in front. None for
    /// a private page whose space is closed: its pages are closing, and
    /// nothing of it opens elsewhere.
    pub fn open_tab_from(&mut self, opener: Option<&str>, url: &str, title: &str) -> Option<Id> {
        let private = opener.is_some_and(|p| self.private_profiles.contains(p));
        if private && self.private_space.is_none() {
            return None;
        }
        let space = opener.map_or_else(
            || self.active_space.clone(),
            |p| self.keep_space(p, &self.active_space),
        );
        let id = self.new_today_tab(&space, url, title);
        // An override only when it differs from the space's profile. A tab
        // of the private space otherwise keeps the profile it was made with
        // (`new_today_tab`).
        let space_profile = self.space(&space).map(|sp| sp.profile_id.clone());
        if let Some(p) = opener.filter(|p| Some(*p) != space_profile.as_deref())
            && let Some(t) = self.find_tab_mut(&id)
        {
            t.profile_id = Some(p.to_string());
        }
        Some(id)
    }

    /// The tab whose row shows tab `tab` in space `sp`: the tab itself, or
    /// for a page of a split, the split's row there (`split_row`).
    pub fn row_of(&self, sp: &Space, tab: &str) -> Option<Id> {
        let tab = tab.to_string();
        match self.splits.iter().find(|t| t.contains(&tab)) {
            Some(split) => split_row(sp, split),
            None => Some(tab),
        }
    }

    /// Tab `id`, just opened at the top of Today by the page of tab
    /// `opener`, goes below that tab's row if Settings → Links says so and
    /// both are in the Today list of one space (a pinned tab's or a
    /// favorite's new tab stays at the top, as does one that went to
    /// another space). A split the opener is in is one row, at its first
    /// page with a row in the space (`split_row`): the tab goes below it,
    /// or stays at the top when that row is a pinned one. The tabs the
    /// row's pages opened before, and the tabs those opened, stay between,
    /// so links opened one after the other read from the top down.
    pub fn place_below_opener(&mut self, id: &str, opener: &str) {
        if self.settings.new_tab_place != NewTabPlace::Below || id == opener {
            return;
        }
        let has = |sp: &Space, tab: &str| sp.today.iter().any(|t| t.id == tab);
        let Some(space) =
            (self.sidebar.spaces.iter()).position(|sp| has(sp, id) && has(sp, opener))
        else {
            return;
        };
        let sp = &self.sidebar.spaces[space];
        let split = (self.splits.iter()).find(|t| t.contains(&opener.to_string()));
        let pages = split.map_or_else(|| vec![opener.to_string()], SplitNode::ids);
        let row = split.map_or_else(|| Some(opener.to_string()), |t| split_row(sp, t));
        let Some(at) = row.and_then(|r| sp.today.iter().position(|t| t.id == r)) else {
            return;
        };
        // Opened by one of the row's pages, or by a tab opened so. Bounded,
        // should the chain ever loop.
        let opened_from_row = |tab: &str| {
            let opener_of = |t: &str| self.runtime.get(t).and_then(|rt| rt.opener.as_deref());
            std::iter::successors(opener_of(tab), |o| opener_of(o))
                .take(self.runtime.len())
                .any(|o| pages.iter().any(|p| p == o))
        };
        // A split's other pages show no row, so they end nothing.
        let no_row = |tab: &Id| {
            (self.splits.iter()).any(|t| t.contains(tab) && split_row(sp, t).as_ref() != Some(tab))
        };
        let mut to = at + 1;
        while (sp.today.get(to)).is_some_and(|t| opened_from_row(&t.id) || no_row(&t.id)) {
            to += 1;
        }
        let Some(from) = sp.today.iter().position(|t| t.id == id) else {
            return;
        };
        let today = &mut self.sidebar.spaces[space].today;
        let tab = today.remove(from);
        today.insert(if from < to { to - 1 } else { to }, tab);
        self.runtime.entry(id.to_string()).or_default().opener = Some(opener.to_string());
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

    /// Ordered tabs of the active space as shown: pinned (depth first) then
    /// Today. Without the pinned tabs that open as web panels: the order
    /// is for going from tab to tab, and those are none to go to.
    pub fn visible_tab_ids(&self) -> Vec<Id> {
        let s = self.active_space();
        s.pinned
            .iter()
            .flat_map(SidebarNode::tabs)
            .filter(|t| !t.panel)
            .chain(s.today.iter())
            .map(|t| t.id.clone())
            .collect()
    }

    /// The tab `Ctrl+Tab` (`forward`) or `Ctrl+Shift+Tab` goes to: the next
    /// or previous row of `visible_tab_ids`, wrapping around, past the tabs
    /// of closed folders. From a tab with no row there (a favorite), the
    /// first or the last row.
    pub fn cycle_tab(&self, forward: bool) -> Option<Id> {
        let active = self.active_tab_id();
        let mut hidden = Vec::new();
        tabs_in_closed_folders(
            &self.active_space().pinned,
            &self.closed_folders,
            &mut hidden,
        );
        // The active tab keeps its row, also in a closed folder: the round goes on from there.
        let order: Vec<Id> = (self.visible_tab_ids().into_iter())
            .filter(|t| Some(t.as_str()) == active || !hidden.contains(&t.as_str()))
            .collect();
        let n = order.len();
        let pos = active.and_then(|a| order.iter().position(|t| t == a));
        let next = match (pos, forward) {
            (Some(p), true) => (p + 1) % n,
            (Some(p), false) => (p + n - 1) % n,
            (None, true) => 0,
            (None, false) => n.checked_sub(1)?,
        };
        order.get(next).cloned()
    }

    /// Tab `id` was chosen: it is the latest of `recent_tabs`.
    pub fn note_tab_use(&mut self, id: &str) {
        self.recent_tabs.retain(|t| t != id);
        self.recent_tabs.insert(0, id.to_string());
        self.recent_tabs.truncate(RECENT_TABS);
    }

    /// The favorites and the tabs of the active space by last use, the
    /// latest first and the active tab before all: by their place in
    /// `recent_tabs`, then (after a restart, say) by `last_active_at`.
    pub fn tabs_by_last_use(&self) -> Vec<Id> {
        let space = self.active_space();
        // A web panel is no tab to switch to: it opens over one.
        let favorites = (self.sidebar.favorites.iter())
            .flat_map(|f| f.items.iter())
            .filter(|t| self.sidebar.favorite_shows(t, &space.id));
        let pinned = space.pinned.iter().flat_map(SidebarNode::tabs);
        let kept = favorites.chain(pinned).filter(|t| !t.panel);
        let mut tabs: Vec<&TabItem> = kept.chain(&space.today).collect();
        let active = self.active_tab_id();
        tabs.sort_by_key(|t| {
            let place = self.recent_tabs.iter().position(|r| *r == t.id);
            (
                Some(t.id.as_str()) != active,
                place.unwrap_or(usize::MAX),
                std::cmp::Reverse(t.last_active_at),
            )
        });
        tabs.into_iter().map(|t| t.id.clone()).collect()
    }

    /// One press of `Ctrl+Tab` (`forward`) or `Ctrl+Shift+Tab` by last use:
    /// the first begins a round on the tab used before the active one (or,
    /// backwards, the one used longest ago), each further one goes a tab
    /// on, around the `SWITCH_TABS` used last. False with fewer than two
    /// tabs: there is nothing to switch to.
    pub fn switch_tab(&mut self, forward: bool) -> bool {
        if let Some(round) = &mut self.switcher {
            let n = round.tabs.len();
            round.at = (round.at + if forward { 1 } else { n - 1 }) % n;
            return true;
        }
        let mut tabs = self.tabs_by_last_use();
        tabs.truncate(SWITCH_TABS);
        // The round begins on the active tab, the first of the list: unless
        // that tab is not in the list (a favorite this space does not
        // show). Then it begins on the tab used before, and one tab is
        // enough to switch to.
        let active = self.active_tab_id();
        let unlisted =
            active.is_some_and(|a| self.find_tab(a).is_some() && !tabs.iter().any(|t| t == a));
        if tabs.len() < if unlisted { 1 } else { 2 } {
            return false;
        }
        let from_active = usize::from(active.is_some() && !unlisted);
        let at = if forward { from_active } else { tabs.len() - 1 };
        static ROUNDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let round = ROUNDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.switcher = Some(TabSwitch { tabs, at, round });
        true
    }

    /// End the round of `Ctrl+Tab`: the tab it stands on, if it is still
    /// there and a tab to go to (not made a web panel meanwhile, nor a
    /// favorite of other spaces only). `None`
    /// without a round.
    pub fn end_tab_switch(&mut self) -> Option<Id> {
        let round = self.switcher.take()?;
        let chosen = round.tabs.get(round.at)?;
        // Still there, and still one this space shows: a favorite may have
        // been given to other spaces while the round went on.
        let shown = self.find_tab(chosen).is_some_and(|(space, tab)| {
            space.is_some() || self.sidebar.favorite_shows(tab, &self.active_space)
        });
        let still = shown && !self.is_web_panel(chosen);
        still.then(|| chosen.clone())
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

    /// A new tab at the top of Today of space `space_id`. In the private
    /// space it gets a profile of its own (`private.rs`).
    pub fn new_today_tab(&mut self, space_id: &str, url: &str, title: &str) -> Id {
        let id = new_id();
        let profile_id =
            (self.private_space.as_deref() == Some(space_id)).then(|| self.new_private_profile());
        let tab = TabItem {
            created_at: Some(now()),
            last_active_at: Some(now()),
            profile_id,
            ..TabItem::new(id.clone(), url, title)
        };
        if let Some(s) = self.space_mut(space_id) {
            s.today.insert(0, tab);
        }
        id
    }

    /// Whether tab `id` is in use out of sight: it has media, playing or
    /// paused, or is being shared.
    /// Such a tab is never unloaded or archived in the background, which
    /// would stop the music or the sharing.
    fn in_use(&self, id: &str) -> bool {
        (self.runtime.get(id)).is_some_and(|r| r.media.is_some() || r.shared)
    }

    /// The Today tabs to archive now: out of sight for longer than the
    /// auto-archive period (counted from creation if never shown). Not the
    /// tabs in `active` (on screen, or the one a space returns to), not
    /// those with media or being shared, and not those kept active. None
    /// with auto-archive turned off (`NEVER_ARCHIVE`).
    pub fn stale_today_tabs(&self, now: UnixSeconds, active: &HashSet<Id>) -> Vec<Id> {
        if self.settings.auto_archive_hours == NEVER_ARCHIVE {
            return Vec::new();
        }
        let period = i64::from(self.settings.auto_archive_hours) * 3600;
        (self.sidebar.spaces.iter())
            .flat_map(|space| space.today.iter())
            .filter(|t| !t.keep_active && !active.contains(&t.id) && !self.in_use(&t.id))
            .filter(|t| {
                now.saturating_sub(t.last_active_at.or(t.created_at).unwrap_or(now)) > period
            })
            .map(|t| t.id.clone())
            .collect()
    }

    /// The tabs to load at start (Settings: `load_pinned_at_start`), in
    /// sidebar order.
    pub fn tabs_to_load_at_start(&self) -> Vec<Id> {
        self.start_loads().map(|t| t.id.clone()).collect()
    }

    /// Whether tab `id` is (still) one to load at start.
    pub fn loads_at_start(&self, id: &str) -> bool {
        self.start_loads().any(|t| t.id == id)
    }

    /// The favorites and the pinned tabs of every space, if Settings asks
    /// to load them at start. Not web panels: they open over the page in
    /// front, not as tabs. Only web addresses: those the policy takes from
    /// stored data (no files, no pages of ours), as nobody chose to open
    /// them this time.
    fn start_loads(&self) -> impl Iterator<Item = &TabItem> {
        let on = self.settings.load_pinned_at_start;
        let favorites = (self.sidebar.favorites.iter()).flat_map(|f| f.items.iter());
        let pinned = (self.sidebar.spaces.iter())
            .flat_map(|space| space.pinned.iter().flat_map(SidebarNode::tabs));
        favorites.chain(pinned).filter(move |t| {
            on && !t.panel && crate::url_policy::allowed(&t.url, crate::url_policy::Source::Stored)
        })
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
            .filter(|(id, r)| r.content.is_some() && !active.contains(*id) && !self.in_use(id))
            .filter_map(|(id, _)| {
                let tab = (self.sidebar.spaces.iter())
                    .flat_map(|space| space.today.iter())
                    .find(|t| t.id == *id)?;
                (!tab.keep_active).then(|| (id.clone(), tab.last_active_at.unwrap_or(now)))
            })
            .collect()
    }

    /// The pages never archived or unloaded by themselves, in every window:
    /// those on screen (all of a split's) and the tab each other space
    /// returns to.
    pub fn pages_in_use(&self) -> HashSet<Id> {
        let mut active: HashSet<Id> = self.active_tab.values().cloned().collect();
        active.extend(self.visible_panes());
        active.extend(self.web_panel.clone());
        for view in self.views.values() {
            active.extend(view.active_tab.values().cloned());
            active.extend(view.web_panel.clone());
            if let Some(front) = view.active_tab.get(&view.active_space) {
                let split = self.live_splits().find(|t| t.contains(front));
                active.extend(split.into_iter().flat_map(|t| t.ids()));
            }
        }
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
            tab.end_panel();
            self.archive.push(ArchivedTab {
                tab,
                space_id: space.unwrap_or_else(|| self.active_space.clone()),
                archived_at: now(),
            });
        }
        self.forget_tab(id)
    }

    /// Archive pinned tab `id` (`remove_pinned`) at the page it was pinned
    /// for, not at the one it was left on. Its own title names that page only
    /// while it is loaded there: closing a pinned tab takes it home and keeps
    /// the last page's title. Otherwise it takes the page's title from the
    /// history, or none. A title the user gave stays.
    pub fn archive_pinned(&mut self, id: &str) -> Option<ContentId> {
        let home = self.find_tab(id).and_then(|(_, t)| t.home_url.clone());
        if let Some(home) = home {
            let page = page_of(&home);
            let loaded = self.runtime.get(id).is_some_and(|r| r.loaded);
            let known = (self.history.newest())
                .find(|v| page_of(&v.url) == page && !v.title.is_empty())
                .map(|v| v.title.clone());
            if let Some(t) = self.find_tab_mut(id) {
                let own = loaded && page_of(&t.url) == page;
                t.url = home;
                if !t.custom_title && !own {
                    t.title = known.unwrap_or_default();
                }
            }
        }
        self.archive_tab(id)
    }

    /// Take the archive record at `pos` back into Today of its space (the
    /// active space if that one is gone, the space beside the private one if
    /// that is the active one). Returns the tab's id. A tab that
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
        // Never the private space: an archived tab was not one of it, and
        // would run there in a profile that is kept.
        let space = if self.space(&space_id).is_some() {
            space_id
        } else {
            self.lasting_space(&self.active_space)
        };
        self.space_mut(&space)?.today.insert(0, tab);
        Some(id)
    }

    /// Delete archived tabs older than the retention period
    /// (`Settings::archive_days`; 0: none, they are kept). True if any went.
    pub fn prune_archive(&mut self, now: UnixSeconds) -> bool {
        if self.settings.archive_days == 0 {
            return false;
        }
        let keep = i64::from(self.settings.archive_days.min(MAX_ARCHIVE_DAYS)) * 86_400;
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

    /// Add a space after the others (before the private space, which stays
    /// last while it is open), with the star icon (unless `icon` names
    /// another glyph) and a violet colour, running in `profile` (`None`: the
    /// default profile). Returns its id, or why it was not added: the
    /// profile chosen is gone, or is one of the private space's.
    pub fn new_space(
        &mut self,
        title: &str,
        icon: Option<String>,
        profile: Option<ChosenProfile>,
    ) -> Result<Id, &'static str> {
        let title = match title.trim() {
            "" => magnetite_ui::t!("app.names.new_space"),
            title => title,
        };
        // A private one is named like the one a space's page makes.
        let profile_id = self.chosen_profile(profile, title)?;
        let id = new_id();
        let space = Space {
            id: id.clone(),
            title: title.into(),
            icon: icon.or_else(|| Some("✦".into())),
            icon_image: None,
            accent: Some(Color {
                red: 0.55,
                green: 0.5,
                blue: 1.0,
            }),
            profile_id,
            shortcut: None,
            pinned: vec![],
            today: vec![],
            bookmarks: vec![],
        };
        let spaces = &self.sidebar.spaces;
        let at = spaces.iter().position(|sp| self.is_private_space(&sp.id));
        let at = at.unwrap_or(spaces.len());
        self.sidebar.spaces.insert(at, space);
        Ok(id)
    }

    /// Delete space `id` (never the last one). Its tabs go to the archive
    /// like closed ones (`archive_tab`, so a private profile's leave nothing
    /// behind). Returns the page windows to close.
    pub fn delete_space(&mut self, id: &str) -> Vec<ContentId> {
        // The private space does not count: it is gone at the next start.
        let private = usize::from(self.private_space.is_some());
        if self.sidebar.spaces.len() - private <= 1 && !self.is_private_space(id) {
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

/// The tabs in `nodes` that a closed folder (one of `closed`) hides, also
/// in folders within it, added to `out`.
fn tabs_in_closed_folders<'a>(
    nodes: &'a [SidebarNode],
    closed: &HashSet<Id>,
    out: &mut Vec<&'a str>,
) {
    for node in nodes {
        if let SidebarNode::Folder(f) = node {
            if closed.contains(&f.id) {
                out.extend(node.tabs().map(|t| t.id.as_str()));
            } else {
                tabs_in_closed_folders(&f.children, closed, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::{state, tab};

    #[test]
    fn favorites_and_pinned_tabs_load_at_start_when_asked_but_not_panels_or_files() {
        let mut s = state();
        let profile = s.sidebar.spaces[0].profile_id.clone();
        s.sidebar.favorites = vec![magnetite_core::FavoriteSet {
            profile_id: profile,
            items: vec![
                tab("f"),
                TabItem {
                    panel: true,
                    ..tab("w")
                },
                TabItem::new("l", "file:///home/me/notes.html", "l"),
            ],
        }];
        assert_eq!(s.tabs_to_load_at_start(), Vec::<Id>::new());
        s.settings.load_pinned_at_start = true;
        assert_eq!(s.tabs_to_load_at_start(), ["f", "p"]);
        assert!(s.loads_at_start("p") && !s.loads_at_start("w") && !s.loads_at_start("a"));
    }

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
    fn tab_cycling_skips_the_tabs_of_closed_folders() {
        let mut s = state();
        let folder = |id: &str, children| {
            SidebarNode::Folder(magnetite_core::Folder {
                id: id.into(),
                title: id.into(),
                icon: None,
                children,
                smart: None,
            })
        };
        // Rows: p, open o [x, closed n [y]], closed f [z, open g [w]], then a, b, c.
        let space = s.active_space.clone();
        s.space_mut(&space).unwrap().pinned = vec![
            SidebarNode::Tab(tab("p")),
            folder(
                "o",
                vec![
                    SidebarNode::Tab(tab("x")),
                    folder("n", vec![SidebarNode::Tab(tab("y"))]),
                ],
            ),
            folder(
                "f",
                vec![
                    SidebarNode::Tab(tab("z")),
                    folder("g", vec![SidebarNode::Tab(tab("w"))]),
                ],
            ),
        ];
        s.closed_folders = HashSet::from(["n".into(), "f".into()]);
        s.active_tab.insert(space.clone(), "x".into());
        assert_eq!(s.cycle_tab(true).as_deref(), Some("a"), "past y, z and w");
        assert_eq!(s.cycle_tab(false).as_deref(), Some("p"));
        s.active_tab.insert(space.clone(), "a".into());
        assert_eq!(
            s.cycle_tab(false).as_deref(),
            Some("x"),
            "an open folder's tab"
        );
        s.active_tab.insert(space, "z".into());
        assert_eq!(s.cycle_tab(true).as_deref(), Some("a"), "on from z, past w");
        assert_eq!(s.cycle_tab(false).as_deref(), Some("x"), "back from z");
    }

    #[test]
    fn a_pages_new_tab_goes_below_its_tab_when_the_setting_says_so() {
        let mut s = state();
        let space = s.active_space.clone();
        let today = |s: &AppState| -> Vec<String> {
            let tabs = s.active_space().today.iter();
            tabs.map(|t| t.title.clone()).collect()
        };
        let open = |s: &mut AppState, title: &str, opener: &str| {
            let id = s.new_today_tab(&space, "https://example.org/", title);
            s.place_below_opener(&id, opener);
            id
        };
        // As built: at the top, whichever tab opened it.
        open(&mut s, "top", "b");
        assert_eq!(today(&s), ["top", "a", "b", "c"]);
        s.settings.new_tab_place = crate::state::NewTabPlace::Below;
        // Below its tab, and what the same page opens next after it.
        let one = open(&mut s, "b1", "b");
        open(&mut s, "b2", "b");
        assert_eq!(today(&s), ["top", "a", "b", "b1", "b2", "c"]);
        // A tab opened from one of those is below that one, and the page's
        // next tab after both.
        open(&mut s, "b1x", &one);
        assert_eq!(today(&s), ["top", "a", "b", "b1", "b1x", "b2", "c"]);
        open(&mut s, "b3", "b");
        assert_eq!(today(&s), ["top", "a", "b", "b1", "b1x", "b2", "b3", "c"]);
        // The last tab's goes to the end, the first one's stays second.
        open(&mut s, "c1", "c");
        let first = s.active_space().today[0].id.clone();
        open(&mut s, "top1", &first);
        assert_eq!(
            today(&s),
            ["top", "top1", "a", "b", "b1", "b1x", "b2", "b3", "c", "c1"]
        );
        // A pinned tab is not in Today: its new tab is at the top.
        open(&mut s, "p1", "p");
        assert_eq!(today(&s)[0], "p1");
        // A tab that went to another space than its opener's (a private
        // page's, a link rule's) is at the top there.
        s.sidebar.spaces.push(Space {
            id: "two".into(),
            pinned: vec![],
            today: vec![tab("t")],
            ..s.sidebar.spaces[0].clone()
        });
        let elsewhere = s.new_today_tab("two", "https://example.org/", "far");
        s.place_below_opener(&elsewhere, "c");
        assert_eq!(s.space("two").unwrap().today[0].id, elsewhere);
    }

    #[test]
    fn a_pages_new_tab_goes_below_its_splits_row() {
        let mut s = state();
        let space = s.active_space.clone();
        s.settings.new_tab_place = crate::state::NewTabPlace::Below;
        let today = |s: &AppState| -> Vec<String> {
            let tabs = s.active_space().today.iter();
            tabs.map(|t| t.title.clone()).collect()
        };
        let open = |s: &mut AppState, title: &str, opener: &str| {
            let id = s.new_today_tab(&space, "https://example.org/", title);
            s.place_below_opener(&id, opener);
        };
        let split = |first: &str, second: &str| {
            let mut t = SplitNode::pane(first.to_string());
            t.insert(
                &first.to_string(),
                second.to_string(),
                magnetite_core::split::Side::Right,
            );
            t
        };
        // Split a | c with b between them: the sidebar shows the split in
        // a's place, and the tab goes right below that row, from either page.
        s.splits = vec![split("a", "c")];
        open(&mut s, "a1", "a");
        open(&mut s, "c1", "c");
        assert_eq!(today(&s), ["a", "a1", "c1", "b", "c"]);
        // Split b | c, its pages next to each other: c shows no row, so it
        // does not end what the row opened before.
        s.splits = vec![split("b", "c")];
        open(&mut s, "b1", "b");
        open(&mut s, "c2", "c");
        assert_eq!(today(&s), ["a", "a1", "c1", "b", "c", "b1", "c2"]);
        // A split shown with the pinned tabs: its Today page's tab is at
        // the top.
        s.splits = vec![split("p", "b")];
        open(&mut s, "b2", "b");
        assert_eq!(today(&s)[0], "b2");
    }

    #[test]
    fn a_failed_load_shows_its_address_and_drops_the_last_icon() {
        let mut s = state();
        s.content_to_tab.insert(7, "a".into());
        s.runtime.entry("a".into()).or_default().favicon =
            Some("https://example.org/favicon.ico".into());
        s.load_failed(7, "https://down.example/");
        assert_eq!(
            s.find_tab("a").map(|(_, t)| t.url.as_str()),
            Some("https://down.example/")
        );
        assert_eq!(s.runtime["a"].favicon, None);
    }

    #[test]
    fn an_unread_count_goes_from_the_title_to_the_runtime() {
        let mut s = state();
        s.content_to_tab.insert(1, "a".into());
        s.content_to_tab.insert(2, "p".into());
        s.find_tab_mut("p").unwrap().custom_title = true;
        s.set_page_title(1, "(3) Chat");
        s.set_page_title(2, "Inbox (12) - Mail");
        assert_eq!(s.find_tab("a").unwrap().1.title, "Chat");
        assert_eq!(s.runtime["a"].unread_count, Some(3));
        // A renamed tab keeps its name.
        assert_eq!(s.find_tab("p").unwrap().1.title, "p");
        assert_eq!(s.runtime["p"].unread_count, Some(12));
        // Read: the bubble goes.
        s.set_page_title(1, "Chat");
        assert_eq!(s.runtime["a"].unread_count, None);
        // A page that goes takes its count along.
        s.runtime.get_mut("p").unwrap().unload();
        assert_eq!(s.runtime["p"].unread_count, None);
    }

    #[test]
    fn media_changes_reach_the_sidebar_only_where_it_shows_them() {
        let mut s = state();
        s.content_to_tab.insert(1, "a".into());
        s.content_to_tab.insert(2, "b".into());
        let video = |playing: bool, silent: bool| {
            Some(MediaState {
                playing,
                video: true,
                silent,
                ..MediaState::default()
            })
        };
        // `b` is on screen: its muted video starting and stopping shows
        // nowhere, but is kept.
        assert!(!s.set_media(2, video(true, true)));
        assert!(!s.set_media(2, None));
        assert!(!s.set_media(2, video(false, true)));
        assert_eq!(s.runtime["b"].media, video(false, true));
        // Sound starting or stopping shows on its row.
        assert!(s.set_media(2, video(true, false)));
        assert!(s.set_media(2, video(true, true)));
        // `a` is not on screen: the media bar lists it.
        assert!(s.set_media(1, video(true, true)));
        assert!(!s.set_media(1, video(true, true)));
        // Another window's media bar lists `b`.
        s.open_window(2);
        s.enter(1);
        assert!(s.set_media(2, video(false, true)));
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
        let over = s.peek_over();
        s.peeks.insert(
            over,
            crate::state::Peek {
                content: 7,
                url: "https://example.org/video".into(),
                title: "Video".into(),
                runtime: TabRuntime {
                    content: Some(7),
                    loaded: true,
                    ..TabRuntime::default()
                },
                profile: s.active_space().profile_id.clone(),
            },
        );
        // Events of the peek's page go to the peek, not to a tab.
        let rt = s.page_runtime(7).expect("the peek");
        rt.can_go_back = true;
        rt.camera = true;
        assert!(s.runtime.values().all(|r| !r.can_go_back));
        assert!(s.page_runtime(8).is_none(), "no such page");

        let id = s.keep_peek().expect("a tab");
        assert!(s.peeks.is_empty());
        let kept = &s.runtime[&id];
        assert_eq!(kept.content, Some(7));
        assert!(kept.loaded && kept.can_go_back && kept.camera);
        // From now on the page's events go to the tab.
        s.page_runtime(7).expect("the tab").can_go_back = false;
        assert!(!s.runtime[&id].can_go_back);
    }

    #[test]
    fn a_peek_stays_with_its_tab_until_the_tabs_page_goes() {
        let mut s = state();
        let space = s.active_space.clone();
        let page = |content| Peek {
            content,
            url: format!("https://example.org/peek-{content}"),
            title: String::new(),
            runtime: TabRuntime::default(),
            profile: String::new(),
        };
        for (tab, content) in [("a", 1), ("b", 2), ("c", 3)] {
            s.runtime.entry(tab.into()).or_default().content = Some(content);
        }
        let front = |s: &AppState| s.peek().map(|p| p.content);
        // A peek over "b", then one over "a": each shows while its tab is
        // in front.
        s.peeks.insert(s.peek_over(), page(12));
        s.active_tab.insert(space.clone(), "a".into());
        assert_eq!(front(&s), None, "a has no peek yet");
        s.peeks.insert(s.peek_over(), page(11));
        assert_eq!(front(&s), Some(11));
        s.active_tab.insert(space.clone(), "b".into());
        assert_eq!(front(&s), Some(12), "back on b, its peek is there");
        assert!(
            s.page_outside(11).is_some(),
            "a hidden peek still hears its page"
        );
        assert_eq!(s.drop_stale_peeks(), [] as [ContentId; 0]);

        // Its tab unloaded or archived: the peek goes too.
        s.runtime.get_mut("a").unwrap().content = None;
        assert_eq!(s.drop_stale_peeks(), [11]);
        s.archive_tab("b");
        assert_eq!(s.drop_stale_peeks(), [12]);
        assert!(s.peeks.is_empty());

        // Over a space with no tab in front, a peek lasts until one is.
        s.active_tab.remove(&space);
        assert_eq!(s.peek_over(), PeekOver::Window(1));
        s.peeks.insert(s.peek_over(), page(9));
        assert_eq!(s.drop_stale_peeks(), [] as [ContentId; 0]);
        s.active_tab.insert(space, "c".into());
        assert_eq!(s.drop_stale_peeks(), [9]);
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
        // Turned off, no tab is archived however long it was left alone.
        s.settings.auto_archive_hours = NEVER_ARCHIVE;
        assert_eq!(s.stale_today_tabs(i64::MAX, &active), [] as [Id; 0]);
        s.settings.auto_archive_hours = 12;
        assert_eq!(s.tabs_to_unload(now(), &active), ["a"]);
        // Let go again, it goes the way of the others.
        s.find_tab_mut("c").unwrap().keep_active = false;
        let mut unload = s.tabs_to_unload(now(), &active);
        unload.sort();
        assert_eq!(unload, ["a", "c"]);
        // A tab that is being shared stays, like one that plays.
        s.runtime.get_mut("a").unwrap().shared = true;
        assert_eq!(s.tabs_to_unload(now(), &active), ["c"]);
        assert_eq!(s.stale_today_tabs(now(), &active), ["c"]);
        s.runtime.get_mut("a").unwrap().shared = false;
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
    fn a_removed_pinned_tab_is_archived_at_its_pinned_page_with_that_pages_title() {
        /// `p` of a fresh state at `url` titled `title`, loaded or not,
        /// archived after a visit of its pinned page titled `visit`.
        fn archived(url: &str, title: &str, loaded: bool, visit: Option<&str>) -> TabItem {
            let mut s = state();
            if let Some(title) = visit {
                s.history.record("https://example.org/p", title, now());
            }
            if loaded {
                let rt = TabRuntime {
                    loaded,
                    ..TabRuntime::default()
                };
                s.runtime.insert("p".into(), rt);
            }
            let p = s.find_tab_mut("p").unwrap();
            (p.url, p.title) = (url.into(), title.into());
            s.archive_pinned("p");
            assert!(s.find_tab("p").is_none());
            s.archive.pop().unwrap().tab
        }
        let (home, issue) = ("https://example.org/p", "https://example.org/p/issues/42");
        // Left on another page: archived at home, under the history's title.
        let t = archived(issue, "Bug #42", true, Some("Project P"));
        assert_eq!((t.url.as_str(), t.title.as_str()), (home, "Project P"));
        // Closed first (home again, the last title kept), with or without history.
        assert_eq!(
            archived(home, "Bug #42", false, Some("Project P")).title,
            "Project P"
        );
        assert_eq!(archived(home, "Bug #42", false, None).title, "");
        // Loaded on its page, only the place in it changed: its own title is
        // right, and better than the history's (another profile's, maybe).
        let t = archived("https://example.org/p#inbox", "Inbox", true, Some("Old"));
        assert_eq!(t.title, "Inbox");
    }

    #[test]
    fn a_title_the_user_gave_a_removed_pinned_tab_stays() {
        let mut s = state();
        s.history
            .record("https://example.org/p", "Project P", now());
        let p = s.find_tab_mut("p").unwrap();
        (p.url, p.title, p.custom_title) = ("https://example.org/p/x".into(), "Work".into(), true);
        s.archive_pinned("p");
        assert_eq!(s.archive.pop().unwrap().tab.title, "Work");
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
        // Kept forever: nothing goes, however old.
        s.settings.archive_days = 0;
        assert!(!s.prune_archive(100_000 * day));
        assert_eq!(s.archive.len(), 2);
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
    fn ctrl_tab_goes_to_the_tab_used_before_and_back() {
        // `state()`: Today a, b (active), c and the pinned p.
        let mut s = state();
        for used in ["a", "c", "b"] {
            s.note_tab_use(used);
        }
        assert_eq!(s.tabs_by_last_use(), ["b", "c", "a", "p"]);
        let space = s.active_space.clone();
        let press_and_let_go = |s: &mut AppState| {
            assert!(s.switch_tab(true));
            let to = s.end_tab_switch().unwrap();
            s.active_tab.insert(space.clone(), to.clone());
            s.note_tab_use(&to);
            to
        };
        assert_eq!(press_and_let_go(&mut s), "c");
        assert_eq!(press_and_let_go(&mut s), "b", "a second press goes back");
    }

    #[test]
    fn a_held_round_goes_further_back_and_around() {
        let mut s = state();
        for used in ["p", "a", "c", "b"] {
            s.note_tab_use(used);
        }
        let stands_on = |s: &AppState| {
            let round = s.switcher.as_ref().unwrap();
            round.tabs[round.at].clone()
        };
        s.switch_tab(true);
        s.switch_tab(true);
        assert_eq!(stands_on(&s), "a");
        s.switch_tab(true);
        s.switch_tab(true);
        assert_eq!(stands_on(&s), "b", "around to the tab it began on");
        s.switch_tab(false);
        assert_eq!(stands_on(&s), "p", "backwards: the one used longest ago");
        // The tab is gone when Ctrl is let go: nothing is chosen.
        s.take_tab("p");
        assert_eq!(s.end_tab_switch(), None);
        assert_eq!(s.end_tab_switch(), None, "no round, nothing to end");
    }

    #[test]
    fn one_tab_alone_has_nothing_to_switch_to() {
        let mut s = state();
        let space = s.active_space.clone();
        let sp = s.space_mut(&space).unwrap();
        sp.pinned.clear();
        sp.today.truncate(1);
        assert!(!s.switch_tab(true));
        assert!(s.switcher.is_none());
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

    #[test]
    fn a_new_space_runs_in_the_profile_chosen_for_it() {
        let mut s = state();
        let profile_of = |s: &AppState, id: &str| {
            let space = s.space(id).unwrap();
            (s.sidebar.profiles.iter())
                .find(|p| p.id == space.profile_id)
                .unwrap()
                .clone()
        };
        let main = s.new_space("  ", None, None).unwrap();
        assert_eq!(s.space(&main).unwrap().title, "New space");
        assert!(profile_of(&s, &main).is_default);

        let work = s
            .new_space("Work", None, Some(ChosenProfile::New("Work".into())))
            .unwrap();
        let lasting = profile_of(&s, &work);
        assert_eq!((lasting.name.as_str(), lasting.ephemeral), ("Work", false));
        let office = s
            .new_space("Office", None, Some(ChosenProfile::Id(lasting.id.clone())))
            .unwrap();
        assert_eq!(profile_of(&s, &office).id, lasting.id, "shared with Work");

        let secret = s
            .new_space("Secret", None, Some(ChosenProfile::Private))
            .unwrap();
        let private = profile_of(&s, &secret);
        assert_eq!(
            (private.name.as_str(), private.ephemeral),
            ("Secret (private)", true)
        );
        // The space's own, not one of the private space's: it stays while used.
        assert!(!s.private_profiles.contains(&private.id));
        s.prune_private_profiles();
        assert!(s.sidebar.profiles.iter().any(|p| p.id == private.id));
    }

    #[test]
    fn a_new_space_is_refused_a_profile_that_is_gone_or_the_private_spaces() {
        let mut s = state();
        let spaces = s.sidebar.spaces.len();
        let gone = ChosenProfile::Id("gone".into());
        assert!(s.new_space("Work", None, Some(gone)).is_err());
        let private_space = s.open_private_space(false);
        let theirs = s.space(&private_space).unwrap().profile_id.clone();
        assert!(
            s.new_space("Work", None, Some(ChosenProfile::Id(theirs)))
                .is_err()
        );
        assert_eq!(
            s.sidebar.spaces.len(),
            spaces + 1,
            "only the private space came"
        );
    }

    #[test]
    fn the_private_space_stays_last_when_a_space_is_added() {
        let mut s = state();
        let private_space = s.open_private_space(false);
        let work = s.new_space("Work", None, None).unwrap();
        let order: Vec<&str> = s.sidebar.spaces.iter().map(|sp| sp.id.as_str()).collect();
        assert_eq!(
            order[order.len() - 2..],
            [work.as_str(), private_space.as_str()]
        );
    }

    #[test]
    fn a_share_can_switch_to_the_loaded_tabs_of_its_profile() {
        let mut s = state();
        s.sidebar.profiles.push(magnetite_core::Profile {
            id: "work".into(),
            name: "Work".into(),
            is_default: false,
            ephemeral: false,
            color: None,
        });
        s.find_tab_mut("c").unwrap().profile_id = Some("work".into());
        let page = |shared, sharing| TabRuntime {
            loaded: true,
            shared,
            sharing,
            ..TabRuntime::default()
        };
        // `a` shares `b`; the pinned `p` runs in the same profile, `c` in
        // another one.
        let pages = [
            ("a", page(false, true)),
            ("b", page(true, false)),
            ("c", page(false, false)),
            ("p", page(false, false)),
        ];
        for (id, rt) in pages {
            s.runtime.insert(id.into(), rt);
        }
        assert_eq!(s.share_targets(), ["p"]);
        // A tab without a page has no tab for Chromium to switch to.
        s.runtime.get_mut("p").unwrap().loaded = false;
        assert_eq!(s.share_targets(), [] as [&str; 0]);
        // Nothing while no page shares a tab.
        s.runtime.get_mut("p").unwrap().loaded = true;
        s.runtime.get_mut("a").unwrap().sharing = false;
        assert_eq!(s.share_targets(), [] as [&str; 0]);
    }
}
