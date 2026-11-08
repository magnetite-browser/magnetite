//! Helpers for the import tests: folders in Cargo's temp directory (for
//! Firefox and Zen with a `profiles.ini`), JSON compressed the way Firefox
//! writes it, and trees printed as one line for compact assertions.

#![allow(dead_code)] // each test binary uses only some of these

use std::fs;
use std::path::{Path, PathBuf};

use magnetite_core::{SidebarNode, TabItem};
use serde_json::{Value, json};

/// A fresh, empty folder for `test`.
pub fn temp_dir(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Copy the folder `from` with everything in it to `to`.
pub fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A fresh Firefox or Zen data folder for `test` whose `profiles.ini` names
/// one profile, `Profiles/p` (see `profile`).
pub fn data_folder(test: &str) -> PathBuf {
    let root = temp_dir(test);
    write(
        &root.join("profiles.ini"),
        b"[Profile0]\nName=default\nIsRelative=1\nPath=Profiles/p\nDefault=1\n",
    );
    root
}

pub fn profile(root: &Path) -> PathBuf {
    root.join("Profiles/p")
}

/// Write `bytes` to `path`, creating its folders.
pub fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

/// `json` as a mozlz4 file.
pub fn mozlz4(json: &Value) -> Vec<u8> {
    let raw = json.to_string().into_bytes();
    let size = u32::try_from(raw.len()).unwrap().to_le_bytes();
    [b"mozLz40\0", &size[..], &lz4_flex::block::compress(&raw)].concat()
}

/// A session tab showing `url`, plus the fields of `extra`.
pub fn tab(url: &str, title: &str, extra: Value) -> Value {
    let mut tab = json!({
        "entries": [{"url": url, "title": title}],
        "index": 1,
        "lastAccessed": 1_794_047_937_124_u64,
    });
    if let (Some(tab), Value::Object(extra)) = (tab.as_object_mut(), extra) {
        tab.extend(extra);
    }
    tab
}

/// Titles in order, folders as `Name[children]`.
pub fn outline(nodes: &[SidebarNode]) -> String {
    nodes
        .iter()
        .map(|n| match n {
            SidebarNode::Tab(t) => t.title.clone(),
            SidebarNode::Folder(f) => format!("{}[{}]", f.title, outline(&f.children)),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn titles(tabs: &[TabItem]) -> String {
    tabs.iter()
        .map(|t| t.title.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
