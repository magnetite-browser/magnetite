//! Firefox import from synthetic profiles written per test, and from a real
//! one (`fixtures/firefox`: Firefox 159 run headless in a throwaway profile
//! on local test pages; it keeps Firefox's default bookmarks).

mod common;

use std::collections::HashSet;
use std::fs::File;
use std::path::Path;
use std::time::{Duration, SystemTime};

use common::{data_folder, mozlz4, outline, profile, tab, titles, write};
use magnetite_core::SidebarNode;
use magnetite_importer::{ImportError, firefox::import};
use serde_json::json;

#[test]
fn maps_pinned_tabs_groups_bookmarks_and_today() {
    let root = data_folder("firefox-mapping");
    let p = profile(&root);
    let session = json!({
        "windows": [
            {
                "tabs": [
                    tab("https://a.test/pinned", "Pinned", json!({"pinned": true})),
                    tab("about:newtab", "New Tab", json!({})),
                    tab("https://a.test/g1", "Group 1", json!({"groupId": "g"})),
                    tab("https://a.test/t1", "Today 1", json!({})),
                    tab("https://a.test/g2", "Group 2", json!({"groupId": "g"})),
                    tab("https://a.test/stray", "Stray", json!({"groupId": "gone"})),
                    tab("about:preferences", "Settings", json!({"pinned": true})),
                    tab("about:blank", "", json!({"groupId": "blank"})),
                    tab("https://a.test/untitled", "", json!({})),
                    {"entries": [
                        {"url": "https://a.test/back", "title": "Back"},
                        {"url": "https://a.test/forward", "title": "Forward"},
                    ], "index": 1},
                ],
                "groups": [
                    {"id": "g", "name": "Research"},
                    {"id": "blank", "name": "Blank"},
                ],
            },
            {"isPrivate": true, "tabs": [tab("https://a.test/private", "Private", json!({}))]},
            {"tabs": [tab("https://a.test/w2", "Window 2", json!({}))]},
        ],
        "savedGroups": [
            {"name": "Later", "tabs": [{"state": tab("https://a.test/saved", "Saved", json!({}))}]},
            {"name": "Nothing", "tabs": [{"state": tab("about:blank", "", json!({}))}]},
        ],
    });
    write(&p.join("sessionstore.jsonlz4"), &mozlz4(&session));
    let bookmarks = json!({"root": "placesRoot", "typeCode": 2, "children": [
        {"root": "bookmarksMenuFolder", "typeCode": 2, "children": [
            {"typeCode": 3},
            {"title": "Menu", "uri": "https://a.test/menu", "typeCode": 1, "dateAdded": 1_794_047_941_264_000_u64},
            {"title": "Most visited", "uri": "place:sort=8&maxResults=10", "typeCode": 1},
        ]},
        {"root": "toolbarFolder", "typeCode": 2, "children": [
            {"title": "Dev", "typeCode": 2, "children": [
                {"title": "", "uri": "https://a.test/docs", "typeCode": 1},
                {"title": "Nested", "typeCode": 2, "children": [
                    {"title": "Deep", "uri": "https://a.test/deep", "typeCode": 1},
                ]},
            ]},
            {"title": "Top", "uri": "https://a.test/top", "typeCode": 1},
        ]},
        {"root": "unfiledBookmarksFolder", "typeCode": 2, "children": []},
        {"root": "mobileFolder", "typeCode": 2, "children": [
            {"title": "Phone", "uri": "https://a.test/phone", "typeCode": 1},
        ]},
    ]});
    let backups = p.join("bookmarkbackups");
    write(
        &backups.join("bookmarks-2026-11-07_6_abc.json"),
        bookmarks.to_string().as_bytes(),
    );
    let old = json!({"children": [{"root": "toolbarFolder", "typeCode": 2, "children": [
        {"title": "Old", "uri": "https://a.test/old", "typeCode": 1},
    ]}]});
    write(
        &backups.join("bookmarks-2026-10-30_1_def.jsonlz4"),
        &mozlz4(&old),
    );

    let snap = import(&root, "Firefox", false).unwrap();
    assert_eq!(snap.profiles.len(), 1);
    assert!(snap.profiles[0].is_default);
    assert_eq!(snap.favorites[0].profile_id, snap.profiles[0].id);
    assert_eq!(snap.favorites[0].items.len(), 0);
    let [space] = snap.spaces.as_slice() else {
        panic!("one space")
    };
    assert_eq!(space.title, "Firefox");
    assert_eq!(space.profile_id, snap.profiles[0].id);
    assert_eq!(
        outline(&space.pinned),
        "Pinned, Research[Group 1, Group 2], Later[Saved]"
    );
    assert_eq!(
        outline(&space.bookmarks),
        "Bookmarks toolbar[Dev[https://a.test/docs, Nested[Deep]], Top], \
         Bookmarks menu[Menu], Mobile bookmarks[Phone]"
    );
    assert_eq!(
        titles(&space.today),
        "Today 1, Stray, https://a.test/untitled, Back, Window 2"
    );

    let tabs: Vec<_> = snap.all_tabs().collect();
    let pinned = tabs.iter().find(|t| t.title == "Pinned").unwrap();
    assert_eq!(pinned.home_url.as_deref(), Some("https://a.test/pinned"));
    assert_eq!(pinned.last_active_at, Some(1_794_047_937));
    assert!(space.today.iter().all(|t| t.home_url.is_none()));
    // Bookmarks are no tabs, and have no home page.
    assert!(tabs.iter().all(|t| t.title != "Menu"));
    let saved: Vec<_> = snap.all_bookmarks().collect();
    let menu = saved.iter().find(|t| t.title == "Menu").unwrap();
    assert!(menu.custom_title && menu.home_url.is_none());
    assert_eq!(menu.created_at, Some(1_794_047_941));
    assert!(
        saved
            .iter()
            .any(|t| t.url == "https://a.test/docs" && !t.custom_title)
    );

    let mut ids = HashSet::new();
    fn collect<'a>(nodes: &'a [SidebarNode], ids: &mut HashSet<&'a str>) {
        for n in nodes {
            assert!(ids.insert(n.id()), "duplicate id {}", n.id());
            if let SidebarNode::Folder(f) = n {
                collect(&f.children, ids);
            }
        }
    }
    collect(&space.pinned, &mut ids);
    assert!(space.today.iter().all(|t| ids.insert(&t.id)));
    assert!(ids.insert(&space.id));
}

#[test]
fn picks_the_profile_firefox_opens() {
    let root = data_folder("firefox-profiles");
    for name in ["a", "b", "c"] {
        let session = json!({"windows": [{"tabs": [tab(&format!("https://{name}.test/"), name, json!({}))]}]});
        write(
            &root.join(format!("Profiles/{name}/sessionstore.jsonlz4")),
            &mozlz4(&session),
        );
    }
    let a = "[Profile0]\nName=a\nIsRelative=1\nPath=Profiles/a\n";
    let b = "[Profile1]\nName=b\nIsRelative=1\nPath=Profiles/b\n";
    let cases = [
        // The install's profile wins over the one marked default.
        (
            format!(
                "[Install4F96D1932A9F858E]\nDefault=Profiles/b\nLocked=1\n\n{a}Default=1\n\n{b}"
            ),
            "b",
        ),
        (format!("{a}\n{b}Default=1\n"), "b"),
        (format!("[General]\nVersion=2\n\n{b}\n{a}"), "b"),
        (
            format!(
                "[Profile0]\nIsRelative=0\nPath={}\n",
                root.join("Profiles/c").display()
            ),
            "c",
        ),
    ];
    for (ini, want) in cases {
        write(&root.join("profiles.ini"), ini.as_bytes());
        let snap = import(&root, "Firefox", false).unwrap();
        assert_eq!(titles(&snap.spaces[0].today), want, "{ini}");
    }
}

#[test]
fn reads_the_newer_session_file() {
    let root = data_folder("firefox-session-files");
    let p = profile(&root);
    let quit = p.join("sessionstore.jsonlz4");
    let running = p.join("sessionstore-backups/recovery.jsonlz4");
    let session = |title: &str| {
        mozlz4(&json!({"windows": [{"tabs": [tab("https://a.test/", title, json!({}))]}]}))
    };
    write(&quit, &session("Quit"));
    write(&running, &session("Running"));
    let age = |path: &Path, secs| {
        let time = SystemTime::now() - Duration::from_secs(secs);
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(time)
            .unwrap();
    };
    let today = || titles(&import(&root, "Firefox", false).unwrap().spaces[0].today);

    age(&quit, 3600);
    assert_eq!(today(), "Running");
    age(&running, 7200);
    assert_eq!(today(), "Quit");
    // A broken newest file falls back to the other one.
    write(&running, b"mozLz40\0garbage");
    assert_eq!(today(), "Quit");
}

#[test]
fn broken_or_empty_profiles_fail_cleanly() {
    let root = data_folder("firefox-broken");
    let p = profile(&root);
    let run = || import(&root, "Firefox", false);
    assert!(matches!(run(), Err(ImportError::Empty(name)) if name == "Firefox"));

    write(
        &p.join("sessionstore.jsonlz4"),
        b"mozLz40\0\x10\0\0\0\xff\xff",
    );
    assert!(matches!(run(), Err(ImportError::Corrupt { .. })));
    std::fs::remove_file(p.join("sessionstore.jsonlz4")).unwrap();

    let mut deep = json!({"title": "Deep", "uri": "https://a.test/", "typeCode": 1});
    for _ in 0..40 {
        deep = json!({"title": "f", "typeCode": 2, "children": [deep]});
    }
    let tree = json!({"children": [{"root": "toolbarFolder", "children": [deep]}]});
    write(
        &p.join("bookmarkbackups/bookmarks-2026-11-07.json"),
        tree.to_string().as_bytes(),
    );
    assert!(matches!(run(), Err(ImportError::TooDeep(_))));
    // A backup folder that cannot be listed is not taken for no bookmarks.
    std::fs::remove_dir_all(p.join("bookmarkbackups")).unwrap();
    write(&p.join("bookmarkbackups"), b"");
    assert!(matches!(run(), Err(ImportError::Read { .. })));

    write(&root.join("profiles.ini"), b"[General]\nVersion=2\n");
    assert!(matches!(run(), Err(ImportError::Empty(_))));
}

#[test]
fn imports_a_real_profile() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/firefox");
    let snap = import(&root, "Firefox", false).unwrap();
    let space = &snap.spaces[0];
    assert_eq!(
        outline(&space.pinned),
        "Page pinned-a, Research[Page group-1, Page group-2], Later[Page saved-1]"
    );
    assert_eq!(
        outline(&space.bookmarks),
        "Bookmarks toolbar[Toolbar bookmark, Dev[Rust docs, Nested[Deep]]], \
         Bookmarks menu[Mozilla Firefox[Get Help, Customize Firefox, Get Involved, About Us], \
         Menu bookmark], Other bookmarks[https://example.net/other]"
    );
    assert_eq!(titles(&space.today), "Page today-1, Page today-2");
}
