//! End-to-end import of an anonymised real-world sidebar file (version 1,
//! Arc for macOS, autumn 2026). Numbers come from the fixture.

use magnetite_core::SidebarNode;

const FIXTURE: &str = include_str!("fixtures/sidebar-v1.json");

#[test]
fn imports_spaces_favorites_pinned_and_today() {
    let snap = magnetite_importer::import_str(FIXTURE).expect("parses");
    assert_eq!(snap.format, "magnetite-sidebar");
    assert_eq!(snap.spaces.len(), 2);
    assert_eq!(snap.favorites.len(), 1);
    assert_eq!(snap.favorites[0].items.len(), 5);

    let s1 = &snap.spaces[0];
    assert_eq!(s1.title, "Space 89");
    assert_eq!(s1.icon.as_deref(), Some("flash"));
    assert!(s1.accent.is_some(), "accent colour extracted from theme");
    assert_eq!(
        s1.pinned.len(),
        5,
        "direct pinned children (4 tabs + 1 folder)"
    );
    assert_eq!(s1.pinned.iter().flat_map(SidebarNode::tabs).count(), 6);
    assert_eq!(s1.today.len(), 11);

    let s2 = &snap.spaces[1];
    assert_eq!(s2.icon.as_deref(), Some("medical"));
    assert_eq!(s2.pinned.iter().flat_map(SidebarNode::tabs).count(), 6);
    assert_eq!(s2.today.len(), 12);

    // 5 favorites + 2 x (6 pinned + today)
    assert_eq!(snap.all_tabs().count(), 5 + 6 + 11 + 6 + 12);
}

#[test]
fn folders_keep_children_and_icons() {
    let snap = magnetite_importer::import_str(FIXTURE).unwrap();
    let folders: Vec<_> = snap
        .spaces
        .iter()
        .flat_map(|s| s.pinned.iter())
        .filter_map(|n| match n {
            SidebarNode::Folder(f) => Some(f),
            SidebarNode::Tab(_) => None,
        })
        .collect();
    assert_eq!(folders.len(), 2);
    for f in folders {
        assert_eq!(f.children.len(), 2);
        assert!(f.title.starts_with("Example title"));
    }
}

#[test]
fn pinned_tabs_get_home_urls_and_timestamps_are_unix() {
    let snap = magnetite_importer::import_str(FIXTURE).unwrap();
    let pinned = snap.spaces[0]
        .pinned
        .iter()
        .flat_map(SidebarNode::tabs)
        .next()
        .unwrap();
    assert_eq!(pinned.home_url.as_deref(), Some(pinned.url.as_str()));
    let today = &snap.spaces[0].today[0];
    assert!(today.home_url.is_none());
    // Apple epoch values (~8e8) must have been shifted to Unix (~1.79e9).
    assert!(today.created_at.unwrap() > 1_700_000_000);
    assert!(today.last_active_at.unwrap() > 1_700_000_000);
}

#[test]
fn tolerates_unknown_fields_and_missing_sections() {
    let minimal = r#"{"version": 1, "sidebar": {"containers": [{"global": {}}, {
        "items": ["X", {"id": "c1", "childrenIds": ["t1"], "data": {"itemContainer": {"containerType": {"spaceItems": {"_0": "s1"}}}}, "future": 1},
                  "Y", {"id": "t1", "parentID": "c1", "childrenIds": [], "title": "Renamed", "data": {"tab": {"savedURL": "https://example.org/", "savedTitle": "Example", "newThing": {}}}}],
        "spaces": ["Z", {"id": "s1", "title": "Only", "containerIDs": ["pinned", "c1"]}],
        "topAppsContainerIDs": []
    }]}}"#;
    let snap = magnetite_importer::import_str(minimal).unwrap();
    assert_eq!(snap.spaces.len(), 1);
    assert_eq!(snap.favorites[0].items, vec![]);
    let tab = snap.spaces[0]
        .pinned
        .iter()
        .flat_map(SidebarNode::tabs)
        .next()
        .unwrap();
    assert_eq!(tab.title, "Renamed");
    assert!(tab.custom_title);
    assert_eq!(snap.spaces[0].today, vec![]);
}

#[test]
fn bookmarks_html_contains_every_url() {
    let snap = magnetite_importer::import_str(FIXTURE).unwrap();
    let html = magnetite_importer::export::bookmarks_html(&snap);
    assert!(html.starts_with("<!DOCTYPE NETSCAPE-Bookmark-file-1>"));
    for tab in snap.all_tabs() {
        assert!(
            html.contains(&format!("HREF=\"{}\"", tab.url)),
            "{}",
            tab.url
        );
    }
    assert!(html.contains("<H3>Favorites</H3>"));
    assert!(html.contains("<H3>Space 89</H3>"));
}

#[test]
fn github_live_folders_become_smart_folders() {
    use magnetite_core::smart::{ReviewShow, SmartSource};
    let snap = magnetite_importer::import_str(FIXTURE).unwrap();
    let smart: Vec<_> = snap
        .spaces
        .iter()
        .flat_map(|s| &s.pinned)
        .filter_map(|n| match n {
            SidebarNode::Folder(f) => f.smart.as_ref().map(|sf| (f, sf)),
            SidebarNode::Tab(_) => None,
        })
        .collect();
    assert_eq!(smart.len(), 2, "both GitHub live folders");
    for (folder, sf) in smart {
        assert!(
            matches!(&sf.source, SmartSource::GitHub { host, show: ReviewShow::PullRequests, .. } if host == "github.com")
        );
        assert_eq!(
            sf.items.len(),
            folder.children.len(),
            "its tabs are its items"
        );
    }
}
