//! A Chromium profile's last session as a space: pinned tabs, tab groups as
//! folders, the other open tabs as Today.

mod common;

use std::path::Path;

use common::{copy_dir, outline, temp_dir, titles};
use magnetite_core::SidebarNode;

#[test]
fn a_real_session_becomes_pinned_tabs_folders_and_today() {
    let root = temp_dir("chromium-import");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/chromium/Default");
    copy_dir(&fixture, &root.join("Default"));
    std::fs::write(
        root.join("Local State"),
        r#"{"profile":{"profiles_order":["Default"]}}"#,
    )
    .unwrap();

    let snap = magnetite_importer::chromium::import(&root, "Google Chrome").unwrap();
    let page = |name: &str| format!("http://127.0.0.1:8765/{name}.html");
    let space = &snap.spaces[0];
    assert_eq!(space.title, "Google Chrome");

    let SidebarNode::Tab(pinned) = &space.pinned[0] else {
        panic!("pinned tab first")
    };
    assert_eq!(pinned.home_url.as_deref(), Some(page("pinned").as_str()));
    // Then the tab group; no bookmarks in this profile.
    assert_eq!(
        outline(&space.pinned),
        "Pinned page, Reading 📚[Grüße – 日本語 🦀, First]"
    );

    assert_eq!(titles(&space.today), "done, First", "about:blank left out");
    assert!(space.today.iter().all(|t| t.home_url.is_none()));
    assert_eq!(space.today[1].last_active_at, Some(1_794_048_413));
}
