//! The `magnetite-import` command line tool, run as a program on sidebar
//! files written by the test.

mod common;

use std::process::Command;

use common::{temp_dir, write};
use serde_json::{Value, json};

fn tool() -> Command {
    Command::new(env!("CARGO_BIN_EXE_magnetite-import"))
}

/// An Arc sidebar file: one space titled `title` with `tabs` Today tabs.
fn sidebar(title: &str, tabs: usize) -> Vec<u8> {
    let ids: Vec<String> = (0..tabs).map(|i| format!("t{i}")).collect();
    let mut items: Vec<Value> = vec![json!("today"), json!({"id": "today", "childrenIds": ids})];
    for id in &ids {
        let url = format!("https://example.org/{id}");
        items.push(json!(id));
        items.push(json!({"id": id, "data": {"tab": {"savedURL": url}}}));
    }
    let space = json!({"id": "s", "title": title, "containerIDs": ["unpinned", "today"]});
    json!({"sidebar": {"containers": [{"items": items, "spaces": ["s", space]}]}})
        .to_string()
        .into_bytes()
}

/// A space title is text from the file; printed as it is, its escape
/// sequences would drive the terminal.
#[test]
fn the_summary_prints_titles_without_control_characters() {
    let dir = temp_dir("cli-summary");
    let input = dir.join("sidebar.json");
    write(&input, &sidebar("Work\u{1b}]0;title\u{7}\u{1b}[31m", 2));
    let out = tool()
        .arg("--input")
        .arg(&input)
        .arg("--summary")
        .arg("--output")
        .arg(dir.join("out.json"))
        .output()
        .unwrap();
    assert!(out.status.success());
    let summary = String::from_utf8(out.stderr).unwrap();
    assert!(
        !summary.chars().any(|c| c.is_control() && c != '\n'),
        "{summary:?}"
    );
    let space: Vec<&str> = summary.lines().nth(1).unwrap().split_whitespace().collect();
    assert_eq!(space, ["Work]0;title[31m", "pinned", "0", "today", "2"]);
}

/// `magnetite-import … | head`: a reader that has gone away is not a failure.
#[test]
fn a_reader_that_is_gone_is_not_an_error() {
    let dir = temp_dir("cli-pipe");
    let input = dir.join("sidebar.json");
    write(&input, &sidebar("Work", 2));
    // A pipe nobody reads, as `head` leaves it once it has its lines.
    let (reader, writer) = std::io::pipe().unwrap();
    drop(reader);
    let out = tool()
        .arg("--input")
        .arg(&input)
        .stdout(writer)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
