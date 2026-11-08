//! Sidebar files that no Arc wrote: cycles, duplicates, absurd nesting and
//! profile ids that are not safe as folder names. Each must import (or fail)
//! cleanly instead of crashing or mixing up data.

use magnetite_core::SidebarNode;
use magnetite_importer::{ImportError, import_str};
use serde_json::{Value, json};

/// A sidebar file from `items` and `spaces` (Arc's alternating id/object
/// lists are built here).
fn sidebar(items: &[Value], spaces: &[Value], top_apps: &[&str]) -> String {
    let alternate = |list: &[Value]| -> Vec<Value> {
        list.iter()
            .flat_map(|v| [v["id"].clone(), v.clone()])
            .collect()
    };
    let mut top: Vec<Value> = vec![json!({"default": true})];
    top.extend(top_apps.iter().map(|id| json!(id)));
    json!({"version": 1, "sidebar": {"containers": [{"global": {}}, {
        "items": alternate(items),
        "spaces": alternate(spaces),
        "topAppsContainerIDs": top,
    }]}})
    .to_string()
}

fn container(id: &str, children: &[&str]) -> Value {
    json!({"id": id, "childrenIds": children, "data": {}})
}

fn folder(id: &str, children: &[&str]) -> Value {
    json!({"id": id, "childrenIds": children, "title": id, "data": {"list": {}}})
}

fn tab(id: &str) -> Value {
    json!({"id": id, "childrenIds": [], "data": {"tab": {"savedURL": format!("https://example.org/{id}")}}})
}

fn space(id: &str, profile: &Value) -> Value {
    json!({"id": id, "title": id, "containerIDs": ["pinned", format!("{id}-pinned"), "unpinned", format!("{id}-today")], "profile": profile})
}

#[test]
fn cycles_and_items_listed_twice_are_imported_once() {
    let file = sidebar(
        &[
            // The pinned container lists itself; the folder lists itself and
            // its parent; `t1` is also listed under Today and the favorites.
            container("s1-pinned", &["s1-pinned", "f1", "t1"]),
            folder("f1", &["f1", "s1-pinned", "t1", "t2"]),
            container("s1-today", &["t1", "t3", "t3"]),
            container("top", &["top", "t1", "t4"]),
            tab("t1"),
            tab("t2"),
            tab("t3"),
            tab("t4"),
        ],
        &[space("s1", &json!({"default": true}))],
        &["top"],
    );
    let snap = import_str(&file).expect("imports");
    // Favorites are read first, so `t1` stays there.
    let ids: Vec<&str> = snap.all_tabs().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["t1", "t4", "t2", "t3"], "each tab once");
    let [SidebarNode::Folder(f)] = snap.spaces[0].pinned.as_slice() else {
        panic!("only the folder is pinned")
    };
    assert_eq!(f.children.len(), 1);
}

#[test]
fn absurdly_deep_folders_are_refused() {
    let depth = 100;
    let mut items: Vec<Value> = (0..depth)
        .map(|i| folder(&format!("f{i}"), &[&format!("f{}", i + 1)]))
        .collect();
    items.push(container("s1-pinned", &["f0"]));
    let file = sidebar(&items, &[space("s1", &json!({"default": true}))], &[]);
    assert!(matches!(import_str(&file), Err(ImportError::TooDeep(_))));
}

#[test]
fn profile_ids_are_safe_folder_names_and_never_shared() {
    let custom = |id: &str| json!({"custom": {"_0": id}});
    let spaces = [
        space("a", &custom("work")),
        space("b", &custom("w/ork")),
        space("c", &custom("WORK")),
        space("d", &custom("default")),
        space("e", &custom("w/ork")),
        space("f", &json!({"default": true})),
    ];
    let snap = import_str(&sidebar(&[container("x", &[])], &spaces, &[])).unwrap();
    let profile_of: Vec<&str> = snap.spaces.iter().map(|s| s.profile_id.as_str()).collect();
    assert_eq!(
        profile_of,
        [
            "work",
            "arc-profile-1",
            "arc-profile-2",
            "arc-profile-3",
            "arc-profile-1",
            "default"
        ]
    );
    let ids: Vec<&str> = snap.profiles.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "default",
            "work",
            "arc-profile-1",
            "arc-profile-2",
            "arc-profile-3"
        ]
    );
    assert_eq!(snap.profiles.iter().filter(|p| p.is_default).count(), 1);
}

/// A custom profile named by an object instead of an id string keeps its
/// spaces apart from the default profile's.
#[test]
fn a_profile_named_by_its_folder_is_a_profile_of_its_own() {
    let folder =
        |name: &str| json!({"custom": {"_0": {"directoryBasename": name, "machineID": "m"}}});
    let spaces = [
        space("a", &folder("Profile 1")),
        space("b", &folder("Profile 2")),
        space("c", &folder("Profile 1")),
        space("d", &json!({"default": true})),
    ];
    let snap = import_str(&sidebar(&[container("x", &[])], &spaces, &[])).unwrap();
    let profile_of: Vec<&str> = snap.spaces.iter().map(|s| s.profile_id.as_str()).collect();
    assert_eq!(profile_of[0], profile_of[2]);
    assert_ne!(profile_of[0], profile_of[1]);
    assert_eq!(profile_of[3], "default");
    assert!(!profile_of[..3].contains(&"default"));
}

/// Spaces count like tabs and folders: a file cannot bring more of them (and
/// of their profiles) than an import may hold.
#[test]
fn more_spaces_than_the_item_limit_are_refused() {
    let spaces: Vec<Value> = (0..=100_000)
        .map(|i| json!({"id": format!("s{i}")}))
        .collect();
    let file = sidebar(&[container("x", &[])], &spaces, &[]);
    assert!(matches!(
        import_str(&file),
        Err(ImportError::TooManyItems(100_000))
    ));
}

/// Every profile id that is not safe as a folder name gets the next free
/// number. Looking for it from 1 each time took minutes for a file like this.
#[test]
fn many_unsafe_profile_ids_do_not_stall_the_import() {
    let spaces: Vec<Value> = (1..=30_000)
        .map(|i| {
            space(
                &format!("s{i}"),
                &json!({"custom": {"_0": format!("p/{i}")}}),
            )
        })
        .collect();
    let file = sidebar(&[container("x", &[])], &spaces, &[]);
    let (done, result) = std::sync::mpsc::channel();
    std::thread::spawn(move || done.send(import_str(&file)));
    let snap = result
        .recv_timeout(std::time::Duration::from_secs(20))
        .expect("the import finishes in time")
        .unwrap();
    assert_eq!(snap.spaces[29_999].profile_id, "arc-profile-30000");
}
