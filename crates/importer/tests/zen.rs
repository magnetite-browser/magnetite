//! Zen import from synthetic sidebar files (`zen-sessions.jsonlz4`, Zen 1.18
//! and later) and from a real one (`fixtures/zen`: Zen 1.24.1b run headless
//! in a throwaway profile on local test pages).

mod common;

use std::path::Path;

use common::{data_folder, mozlz4, outline, profile, tab, titles, write};
use magnetite_core::Color;
use magnetite_importer::firefox::import;
use serde_json::json;

const WORK: &str = "{0b68fba4-0301-4374-9cb2-fcb34720f71e}";
const CODE: &str = "{a09e4a90-a907-41ce-9ba7-a5df57ce8105}";

#[test]
fn maps_spaces_essentials_folders_and_today() {
    let root = data_folder("zen-mapping");
    let p = profile(&root);
    let pinned = |space: &str, group: Option<&str>| json!({"pinned": true, "zenWorkspace": space, "groupId": group});
    let placeholder = |space: &str, group: &str| {
        tab(
            "about:blank",
            "",
            json!({"pinned": true, "zenIsEmpty": true, "zenWorkspace": space, "groupId": group}),
        )
    };
    let sidebar = json!({
        "spaces": [
            {"uuid": WORK, "name": "Work", "icon": "💼", "theme": {"gradientColors": [
                {"c": [0, 0, 255]},
                {"c": [255, 0, 0], "isPrimary": true},
            ]}},
            {"uuid": CODE, "name": "Code", "icon": "chrome://browser/skin/zen-icons/selectable/code.svg"},
        ],
        "folders": [
            {"id": "docs", "name": "Docs", "parentId": null, "userIcon": ""},
            {"id": "inner", "name": "Inner", "parentId": "docs"},
            {"id": "empty", "name": "Empty"},
        ],
        "tabs": [
            tab("https://mail.test/", "Mail", json!({"pinned": true, "zenEssential": true, "zenWorkspace": null})),
            // Zen before 1.18 wrote the flag as a string.
            tab("https://cal.test/", "Calendar", json!({"pinned": true, "zenEssential": "true"})),
            {
                "entries": [
                    {"url": "https://work.test/pin", "title": "Pinned page"},
                    {"url": "https://work.test/now", "title": "Now"},
                ],
                "index": 2,
                "pinned": true,
                "zenWorkspace": WORK,
                "_zenPinnedInitialState": {"entry": {"url": "https://work.test/pin", "title": "Pinned page"}},
            },
            tab("https://work.test/renamed", "Page title", json!({"pinned": true, "zenWorkspace": WORK, "zenStaticLabel": "My label"})),
            placeholder(WORK, "docs"),
            tab("https://work.test/doc1", "Doc 1", pinned(WORK, Some("docs"))),
            placeholder(WORK, "inner"),
            tab("https://work.test/deep", "Deep", pinned(WORK, Some("inner"))),
            tab("https://work.test/doc2", "Doc 2", pinned(WORK, Some("docs"))),
            tab("https://code.test/repo", "Repo", pinned(CODE, None)),
            placeholder(CODE, "empty"),
            tab("https://code.test/peek", "Glance", json!({"zenIsGlance": true, "zenWorkspace": CODE})),
            tab("https://work.test/today", "Today W", json!({"zenWorkspace": WORK})),
            tab("https://code.test/today", "Today C", json!({"pinned": false, "zenWorkspace": CODE})),
            tab("https://lost.test/", "Lost", json!({"zenWorkspace": "{deleted}"})),
            tab("about:newtab", "New Tab", json!({"zenWorkspace": CODE})),
        ],
    });
    write(&p.join("zen-sessions.jsonlz4"), &mozlz4(&sidebar));
    let bookmarks = json!({"children": [{"root": "toolbarFolder", "children": [
        {"title": "Zen bookmark", "uri": "https://example.org/zen", "typeCode": 1},
    ]}]});
    write(
        &p.join("bookmarkbackups/bookmarks-2026-11-07_1_abc.jsonlz4"),
        &mozlz4(&bookmarks),
    );

    let snap = import(&root, "Zen", true).unwrap();
    assert_eq!(titles(&snap.favorites[0].items), "Mail, Calendar");
    assert_eq!(
        snap.favorites[0].items[0].home_url.as_deref(),
        Some("https://mail.test/")
    );
    let [work, code] = snap.spaces.as_slice() else {
        panic!("two spaces")
    };
    assert_eq!(
        (work.title.as_str(), work.icon.as_deref()),
        ("Work", Some("💼"))
    );
    assert_eq!((code.title.as_str(), code.icon.as_deref()), ("Code", None));
    assert_eq!(
        work.accent,
        Some(Color {
            red: 1.0,
            green: 0.0,
            blue: 0.0
        })
    );
    assert_eq!(code.accent, None);
    assert_eq!(
        outline(&work.pinned),
        "Now, My label, Docs[Doc 1, Inner[Deep], Doc 2]"
    );
    assert_eq!(outline(&work.bookmarks), "Bookmarks toolbar[Zen bookmark]");
    assert_eq!(titles(&work.today), "Today W, Lost");
    assert_eq!(outline(&code.pinned), "Repo, Empty[]");
    assert_eq!(titles(&code.today), "Today C");

    let tabs: Vec<_> = snap.all_tabs().collect();
    let moved = tabs.iter().find(|t| t.title == "Now").unwrap();
    assert_eq!(moved.url, "https://work.test/now");
    assert_eq!(moved.home_url.as_deref(), Some("https://work.test/pin"));
    let renamed = tabs.iter().find(|t| t.title == "My label").unwrap();
    assert!(renamed.custom_title);
}

#[test]
fn falls_back_to_the_session_window_without_a_sidebar_file() {
    let root = data_folder("zen-fallback");
    let space = json!([{"uuid": WORK, "name": "From session"}]);
    let session = json!({"windows": [
        {"isPopup": true, "spaces": [{"uuid": CODE, "name": "Popup"}], "tabs": []},
        {"spaces": space, "tabs": [tab("https://a.test/", "Kept", json!({"zenWorkspace": WORK}))]},
    ]});
    write(
        &profile(&root).join("sessionstore.jsonlz4"),
        &mozlz4(&session),
    );
    let snap = import(&root, "Zen", true).unwrap();
    assert_eq!(snap.spaces.len(), 1);
    assert_eq!(snap.spaces[0].title, "From session");
    assert_eq!(titles(&snap.spaces[0].today), "Kept");
}

#[test]
fn folder_cycles_and_unknown_parents_are_imported_once() {
    let root = data_folder("zen-cycles");
    let in_folder = |id: &str| json!({"pinned": true, "zenWorkspace": WORK, "groupId": id});
    let sidebar = json!({
        "spaces": [{"uuid": WORK, "name": "Work"}],
        "folders": [
            {"id": "a", "name": "A", "parentId": "b"},
            {"id": "b", "name": "B", "parentId": "a"},
            {"id": "c", "name": "C", "parentId": "missing"},
        ],
        "tabs": [
            tab("https://a.test/", "In A", in_folder("a")),
            tab("https://b.test/", "In B", in_folder("b")),
            tab("https://c.test/", "In C", in_folder("c")),
        ],
    });
    write(
        &profile(&root).join("zen-sessions.jsonlz4"),
        &mozlz4(&sidebar),
    );
    let snap = import(&root, "Zen", true).unwrap();
    assert_eq!(outline(&snap.spaces[0].pinned), "B[A[In A], In B], C[In C]");
}

#[test]
fn imports_a_real_profile() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/zen");
    let snap = import(&root, "Zen", true).unwrap();
    assert_eq!(titles(&snap.favorites[0].items), "Page ess-1, Page ess-2");
    let [work, code] = snap.spaces.as_slice() else {
        panic!("two spaces")
    };
    assert_eq!(
        (work.title.as_str(), work.icon.as_deref()),
        ("Work", Some("💼"))
    );
    assert_eq!(
        outline(&work.pinned),
        "Page pinned-b, My label, Docs[Page folder-1, Page folder-2, Inner[Page nested-1]]"
    );
    assert_eq!(
        outline(&work.bookmarks),
        "Bookmarks toolbar[Zen bookmark], \
         Bookmarks menu[Mozilla Firefox[Get Help, Customize Firefox, Get Involved, About Us]]"
    );
    assert_eq!(titles(&work.today), "Page work-today");
    assert_eq!((code.title.as_str(), code.icon.as_deref()), ("Code", None));
    assert_eq!(outline(&code.pinned), "Page code-pin");
    assert_eq!(titles(&code.today), "Page code-today");
    let moved = snap
        .all_tabs()
        .find(|t| t.title == "Page pinned-b")
        .unwrap();
    assert_eq!(
        moved.home_url.as_deref(),
        Some("http://127.0.0.1:28380/work-pin.html")
    );
}
