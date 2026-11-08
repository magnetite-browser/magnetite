//! What the command bar finds for typed text: open tabs, bookmarks, the
//! history and archived tabs, in that order.

use std::collections::HashSet;

use magnetite_core::{Id, SidebarNode};
use serde::Serialize;

use super::AppState;
use super::history::page_of;

/// The most results the command bar gets.
const MAX_RESULTS: usize = 30;
/// The most history results among them.
const MAX_HISTORY: usize = 8;
/// How many of the newest archived tabs are searched.
const ARCHIVE_SEARCHED: usize = 200;

/// One result row of the command bar.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SearchResult {
    pub kind: ResultKind,
    pub id: Id,
    pub title: String,
    pub url: String,
    /// The space it is in ("Favorites" for a favorite).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favicon: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResultKind {
    Tab,
    Bookmark,
    History,
    Archived,
}

impl AppState {
    /// Tabs, bookmarks, visits and archived tabs whose title or address
    /// holds `query` (ignoring case), tabs first. A visited page that is
    /// open as a tab or listed as a bookmark already is left out, and each
    /// page shows once, at its newest visit.
    pub fn search(&self, query: &str) -> Vec<SearchResult> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let hit = |title: &str, url: &str| {
            title.to_lowercase().contains(&q) || url.to_lowercase().contains(&q)
        };
        let row = |kind, id: &str, title: &str, url: &str, space: Option<&str>| SearchResult {
            kind,
            id: id.to_string(),
            title: title.to_string(),
            url: url.to_string(),
            space: space.map(str::to_string),
            favicon: None,
        };
        let mut results = Vec::new();
        for space in &self.sidebar.spaces {
            let tabs = space
                .pinned
                .iter()
                .flat_map(SidebarNode::tabs)
                .chain(space.today.iter());
            for t in tabs.filter(|t| hit(&t.title, &t.url)) {
                results.push(SearchResult {
                    favicon: self.runtime.get(&t.id).and_then(|r| r.favicon.clone()),
                    ..row(ResultKind::Tab, &t.id, &t.title, &t.url, Some(&space.title))
                });
            }
        }
        for t in self.sidebar.favorites.iter().flat_map(|f| f.items.iter()) {
            if hit(&t.title, &t.url) {
                results.push(row(
                    ResultKind::Tab,
                    &t.id,
                    &t.title,
                    &t.url,
                    Some("Favorites"),
                ));
            }
        }
        for space in &self.sidebar.spaces {
            for b in space.bookmarks.iter().flat_map(SidebarNode::tabs) {
                if hit(&b.title, &b.url) {
                    results.push(row(
                        ResultKind::Bookmark,
                        &b.id,
                        &b.title,
                        &b.url,
                        Some(&space.title),
                    ));
                }
            }
        }
        let mut shown: HashSet<String> = results
            .iter()
            .map(|r| page_of(&r.url).to_string())
            .collect();
        let visits = self
            .history
            .newest()
            .filter(|v| hit(&v.title, &v.url))
            .filter(|v| shown.insert(page_of(&v.url).to_string()))
            .take(MAX_HISTORY)
            .map(|v| row(ResultKind::History, &v.id, &v.title, &v.url, None))
            .collect::<Vec<_>>();
        results.extend(visits);
        for a in self.archive.iter().rev().take(ARCHIVE_SEARCHED) {
            if hit(&a.tab.title, &a.tab.url) {
                results.push(row(
                    ResultKind::Archived,
                    &a.tab.id,
                    &a.tab.title,
                    &a.tab.url,
                    None,
                ));
            }
        }
        results.truncate(MAX_RESULTS);
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::state;

    #[test]
    fn visits_rank_below_tabs_and_bookmarks_and_do_not_repeat_them() {
        let mut s = state();
        let space = s.active_space.clone();
        s.space_mut(&space)
            .unwrap()
            .bookmarks
            .push(SidebarNode::Tab(magnetite_core::TabItem::new(
                "bm",
                "https://example.org/saved",
                "Saved",
            )));
        // Open as tab "a", saved as a bookmark, visited twice, visited only.
        for (url, at) in [
            ("https://example.org/a", 1),
            ("https://example.org/saved/", 2),
            ("https://example.org/old", 3),
            ("https://example.org/old#x", 5000),
            ("https://example.org/only", 5001),
        ] {
            s.history.record(url, "", at);
        }
        let found: Vec<(ResultKind, String)> = s
            .search("EXAMPLE.org")
            .into_iter()
            .map(|r| (r.kind, r.url))
            .collect();
        let kinds: Vec<ResultKind> = found.iter().map(|(k, _)| *k).collect();
        assert_eq!(kinds[..4], [ResultKind::Tab; 4], "tabs a, b, c and p first");
        assert_eq!(kinds[4], ResultKind::Bookmark);
        let visits: Vec<&str> = (found.iter())
            .filter(|(k, _)| *k == ResultKind::History)
            .map(|(_, u)| u.as_str())
            .collect();
        assert_eq!(
            visits,
            ["https://example.org/only", "https://example.org/old#x"]
        );
        assert_eq!(s.search("  "), []);
    }

    #[test]
    fn at_most_a_few_visits_show() {
        let mut s = state();
        for i in 0..20 {
            s.history
                .record(&format!("https://visited.example/{i}"), "", i);
        }
        let found = s.search("visited");
        assert_eq!(found.len(), MAX_HISTORY);
        assert_eq!(found[0].url, "https://visited.example/19", "newest first");
    }
}
