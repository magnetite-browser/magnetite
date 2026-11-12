//! A small state for the tests of this module's parts.

use std::collections::HashMap;

use magnetite_core::{SidebarNode, TabItem};

use super::AppState;
use super::persist::default_sidebar;
use super::settings::Settings;

pub(super) fn tab(id: &str) -> TabItem {
    TabItem {
        created_at: Some(1),
        last_active_at: Some(1),
        ..TabItem::new(id, format!("https://example.org/{id}"), id)
    }
}

/// Three Today tabs (`b` active) and a pinned one. Its files would go
/// to a folder of its own, which exists only once something is saved.
pub(super) fn state() -> AppState {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = crate::files::test_dir(&format!("state-{n}"));
    let mut snap = default_sidebar();
    let sid = snap.spaces[0].id.clone();
    snap.spaces[0].today = vec![tab("a"), tab("b"), tab("c")];
    snap.spaces[0].pinned = vec![SidebarNode::Tab(TabItem {
        home_url: Some("https://example.org/p".into()),
        ..tab("p")
    })];
    AppState {
        sidebar: snap,
        active_space: sid.clone(),
        active_tab: HashMap::from([(sid, "b".to_string())]),
        runtime: HashMap::new(),
        content_to_tab: HashMap::new(),
        archive: vec![],
        history: super::History::load(&dir),
        splits: Vec::new(),
        peek: None,
        minis: Vec::new(),
        deleted_profiles: vec![],
        unsynced_spaces: std::collections::BTreeSet::new(),
        first_start: false,
        synced_sidebar: None,
        settings: Settings::default(),
        sidebar_visible: true,
        accounts: crate::smart::Accounts::load(&dir),
        smart_status: HashMap::new(),
        downloads: crate::downloads::Downloads::default(),
        saver: super::persist::Saver::new(dir.join("state.json")),
        path: dir.join("state.json"),
    }
}
