//! History: the pages visited on this device, kept in `history.json` next
//! to `state.json`. Never synced, nothing from private profiles, only web
//! pages, and only as long as Settings → Tabs & archive says.

use std::path::{Path, PathBuf};

use magnetite_core::{Id, UnixSeconds};
use serde::{Deserialize, Serialize};

use super::{AppState, new_id};

/// The most visits kept; the oldest go first.
pub const MAX_VISITS: usize = 10_000;

/// A second visit of the same page within this time updates the first one
/// instead of adding another (reloads, a page that changes its address).
const SAME_VISIT: UnixSeconds = 3600;

/// One visit of a page (its main frame).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Visit {
    pub id: Id,
    pub url: String,
    pub title: String,
    pub visited_at: UnixSeconds,
}

/// The visits, oldest first, and where they are saved.
#[derive(Debug, Default)]
pub struct History {
    visits: Vec<Visit>,
    path: PathBuf,
    /// Changed since the last save.
    dirty: bool,
}

/// Whether a page at `url` goes into the history: web pages only, not
/// Magnetite's own, Chromium's or local ones.
fn recordable(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// The page an address names: without the place in it (`#…`) and a
/// trailing slash, so `a/#top` and `a` count as one page.
pub fn page_of(url: &str) -> &str {
    let page = url.split_once('#').map_or(url, |(page, _)| page);
    page.strip_suffix('/').unwrap_or(page)
}

impl History {
    /// The history saved in `dir`, or none. A file that cannot be read is
    /// started over: the next save replaces it.
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("history.json");
        let visits = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Vec<Visit>>(&bytes).ok())
            .unwrap_or_default();
        Self {
            visits,
            path,
            dirty: false,
        }
    }

    pub fn len(&self) -> usize {
        self.visits.len()
    }

    /// Newest first.
    pub fn newest(&self) -> impl Iterator<Item = &Visit> {
        self.visits.iter().rev()
    }

    /// Note a visit of `url` at `now`. A visit of the same page within the
    /// last hour is updated and becomes the newest instead. Returns whether
    /// anything changed.
    pub fn record(&mut self, url: &str, title: &str, now: UnixSeconds) -> bool {
        if !recordable(url) {
            return false;
        }
        let page = page_of(url);
        let recent = self
            .visits
            .iter()
            .rposition(|v| page_of(&v.url) == page && now - v.visited_at < SAME_VISIT);
        let mut visit = match recent {
            Some(i) => self.visits.remove(i),
            None => Visit {
                id: new_id(),
                url: String::new(),
                title: String::new(),
                visited_at: now,
            },
        };
        url.clone_into(&mut visit.url);
        visit.visited_at = now;
        // A page's address arrives before its title: keep the one known.
        if !title.is_empty() && title != url {
            title.clone_into(&mut visit.title);
        }
        self.visits.push(visit);
        let over = self.visits.len().saturating_sub(MAX_VISITS);
        self.visits.drain(..over);
        self.dirty = true;
        true
    }

    /// The page at `url` got its title: the newest visit of it takes it.
    pub fn retitle(&mut self, url: &str, title: &str) {
        if title.is_empty() || title == url || !recordable(url) {
            return;
        }
        let page = page_of(url);
        if let Some(v) = self
            .visits
            .iter_mut()
            .rev()
            .find(|v| page_of(&v.url) == page)
            && v.title != title
        {
            title.clone_into(&mut v.title);
            self.dirty = true;
        }
    }

    /// Forget visits older than `days` (0: all of them). Returns whether any went.
    pub fn prune(&mut self, days: u32, now: UnixSeconds) -> bool {
        let cutoff = now - i64::from(days) * 86_400;
        let before = self.visits.len();
        self.visits.retain(|v| days > 0 && v.visited_at >= cutoff);
        let changed = self.visits.len() != before;
        self.dirty |= changed;
        changed
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.visits.len();
        self.visits.retain(|v| v.id != id);
        let changed = self.visits.len() != before;
        self.dirty |= changed;
        changed
    }

    pub fn clear(&mut self) {
        self.dirty |= !self.visits.is_empty();
        self.visits.clear();
    }

    /// Write the file if anything changed: whole and owner-only, like
    /// `state.json`. No visits: no file.
    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        let result = if self.visits.is_empty() {
            match std::fs::remove_file(&self.path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        } else {
            serde_json::to_vec(&self.visits)
                .map_err(std::io::Error::other)
                .and_then(|bytes| crate::files::write_private(&self.path, &bytes))
        };
        match result {
            Ok(()) => self.dirty = false,
            Err(e) => eprintln!("magnetite: cannot save history: {e}"),
        }
    }
}

impl AppState {
    /// Page window `content` shows `url` now: a visit, unless history is off
    /// or the page runs in a private profile. Only tabs, the peek and mini
    /// windows count.
    pub fn record_visit(&mut self, content: magnetite_engine::ContentId, url: &str) {
        if self.settings.history_days == 0 || self.page_is_private(content) != Some(false) {
            return;
        }
        // No title yet: the one the page has now is the last page's.
        self.history.record(url, "", super::now());
    }

    /// Page window `content` got the title `title`.
    pub fn record_title(&mut self, content: magnetite_engine::ContentId, title: &str) {
        if self.settings.history_days == 0 || self.page_is_private(content) != Some(false) {
            return;
        }
        if let Some(url) = self.page_url(content) {
            self.history.retitle(&url, title);
        }
    }

    /// Whether page window `content` runs in a private profile; `None` for a
    /// window that is no tab, not the peek and no mini window.
    fn page_is_private(&self, content: magnetite_engine::ContentId) -> Option<bool> {
        if let Some(p) = self.page_outside(content) {
            let profiles = &self.sidebar.profiles;
            return Some(
                profiles
                    .iter()
                    .find(|x| x.id == p.profile)
                    .is_none_or(|x| x.ephemeral),
            );
        }
        let tab = self.content_to_tab.get(&content)?;
        Some(self.is_private_tab(tab))
    }

    fn page_url(&self, content: magnetite_engine::ContentId) -> Option<String> {
        if let Some(p) = self.page_outside(content) {
            return Some(p.url.clone());
        }
        let tab = self.content_to_tab.get(&content)?;
        self.find_tab(tab).map(|(_, t)| t.url.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::state;
    use magnetite_core::Profile;

    fn history() -> History {
        History::default()
    }

    #[test]
    fn only_web_pages_are_recorded() {
        let mut h = history();
        for url in [
            "magnetite://ui/settings",
            "chrome://extensions",
            "file:///etc/hosts",
            "about:blank",
        ] {
            assert!(!h.record(url, "x", 100), "{url}");
        }
        assert!(h.record("https://example.org/", "Example", 100));
        assert!(h.record("http://localhost:3000/", "Dev", 100));
        assert_eq!(h.len(), 2);
    }

    #[test]
    fn a_page_seen_again_within_the_hour_is_one_visit() {
        let mut h = history();
        h.record("https://example.org/a", "A", 1000);
        h.record("https://example.org/b", "B", 1100);
        // A reload, then the same page with a place in it.
        h.record("https://example.org/a", "", 1200);
        h.record("https://example.org/a/#part", "", 1300);
        let urls: Vec<_> = h
            .newest()
            .map(|v| (v.url.as_str(), v.title.as_str(), v.visited_at))
            .collect();
        assert_eq!(
            urls,
            [
                ("https://example.org/a/#part", "A", 1300),
                ("https://example.org/b", "B", 1100)
            ]
        );
        // An hour later it is a visit of its own.
        h.record("https://example.org/a", "A", 1300 + SAME_VISIT);
        assert_eq!(h.len(), 3);
    }

    #[test]
    fn the_title_follows_the_address() {
        let mut h = history();
        // Chromium shows the address as the title until the page names itself.
        h.record("https://example.org/", "https://example.org/", 10);
        assert_eq!(h.newest().next().unwrap().title, "");
        h.retitle("https://example.org/", "Example");
        assert_eq!(h.newest().next().unwrap().title, "Example");
    }

    #[test]
    fn old_visits_go_and_the_list_has_a_cap() {
        let mut h = history();
        let day = 86_400;
        h.record("https://example.org/old", "", 0);
        h.record("https://example.org/new", "", 10 * day);
        assert!(h.prune(7, 10 * day));
        assert_eq!(
            h.newest().map(|v| v.url.as_str()).collect::<Vec<_>>(),
            ["https://example.org/new"]
        );
        assert!(!h.prune(7, 10 * day));
        // "Don't keep history" forgets everything.
        assert!(h.prune(0, 10 * day));
        assert_eq!(h.len(), 0);
        for i in 0..=MAX_VISITS {
            h.record(&format!("https://example.org/{i}"), "", 0);
        }
        assert_eq!(h.len(), MAX_VISITS);
        assert_eq!(h.visits[0].url, "https://example.org/1", "the oldest went");
    }

    #[test]
    fn saved_owner_only_and_removed_when_empty() {
        let dir = crate::files::test_dir("history");
        crate::files::private_dir(&dir).unwrap();
        let mut h = History::load(&dir);
        h.record("https://example.org/", "Example", 5);
        h.save();
        let loaded = History::load(&dir);
        assert_eq!(
            loaded.newest().next().map(|v| v.title.as_str()),
            Some("Example")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("history.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let id = h.newest().next().unwrap().id.clone();
        assert!(h.remove(&id));
        h.save();
        assert!(!dir.join("history.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn private_profiles_and_a_history_turned_off_leave_nothing() {
        let mut s = state();
        s.content_to_tab.insert(1, "a".into());
        s.content_to_tab.insert(2, "b".into());
        s.sidebar.profiles.push(Profile {
            id: "secret".into(),
            name: "Secret".into(),
            is_default: false,
            ephemeral: true,
        });
        s.find_tab_mut("b").unwrap().profile_id = Some("secret".into());
        s.record_visit(1, "https://example.org/a");
        s.record_visit(2, "https://example.org/b");
        // A window that is no tab: not recorded either.
        s.record_visit(3, "https://example.org/c");
        let urls: Vec<_> = s.history.newest().map(|v| v.url.clone()).collect();
        assert_eq!(urls, ["https://example.org/a"]);
        s.record_title(1, "Page A");
        assert_eq!(s.history.newest().next().unwrap().title, "Page A");
        // A mini window's page counts like a tab's, by its profile.
        s.minis.push(crate::state::Peek {
            content: 4,
            url: "https://example.org/m".into(),
            title: String::new(),
            runtime: crate::state::TabRuntime::default(),
            profile: s.active_space().profile_id.clone(),
        });
        s.record_visit(4, "https://example.org/m");
        assert_eq!(s.history.len(), 2);
        s.settings.history_days = 0;
        s.record_visit(1, "https://example.org/z");
        assert_eq!(s.history.len(), 2);
    }
}
