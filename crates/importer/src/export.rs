//! Output formats other than our own JSON.

use std::fmt::Write;

use magnetite_core::{SidebarNode, SidebarSnapshot, TabItem};

/// Netscape bookmarks HTML, importable by every browser. Favorites become a
/// top-level folder, each space a folder with "Pinned", "Today" and
/// "Bookmarks" below.
pub fn bookmarks_html(snapshot: &SidebarSnapshot) -> String {
    let mut out = String::from(
        "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n<TITLE>Bookmarks</TITLE>\n<H1>Bookmarks</H1>\n<DL><p>\n",
    );
    for favs in &snapshot.favorites {
        if favs.items.is_empty() {
            continue;
        }
        folder_open(&mut out, 1, "Favorites");
        for t in &favs.items {
            link(&mut out, 2, t);
        }
        folder_close(&mut out, 1);
    }
    for space in &snapshot.spaces {
        folder_open(&mut out, 1, &space.title);
        if !space.pinned.is_empty() {
            folder_open(&mut out, 2, "Pinned");
            nodes(&mut out, 3, &space.pinned);
            folder_close(&mut out, 2);
        }
        if !space.today.is_empty() {
            folder_open(&mut out, 2, "Today");
            for t in &space.today {
                link(&mut out, 3, t);
            }
            folder_close(&mut out, 2);
        }
        if !space.bookmarks.is_empty() {
            folder_open(&mut out, 2, "Bookmarks");
            nodes(&mut out, 3, &space.bookmarks);
            folder_close(&mut out, 2);
        }
        folder_close(&mut out, 1);
    }
    out.push_str("</DL><p>\n");
    out
}

fn nodes(out: &mut String, depth: usize, list: &[SidebarNode]) {
    for node in list {
        match node {
            SidebarNode::Tab(t) => link(out, depth, t),
            SidebarNode::Folder(f) => {
                folder_open(out, depth, &f.title);
                nodes(out, depth + 1, &f.children);
                folder_close(out, depth);
            }
        }
    }
}

fn folder_open(out: &mut String, depth: usize, title: &str) {
    let pad = "    ".repeat(depth);
    let _ = writeln!(out, "{pad}<DT><H3>{}</H3>", escape(title));
    let _ = writeln!(out, "{pad}<DL><p>");
}

fn folder_close(out: &mut String, depth: usize) {
    let _ = writeln!(out, "{}</DL><p>", "    ".repeat(depth));
}

fn link(out: &mut String, depth: usize, tab: &TabItem) {
    // Only web addresses, as the in-app import keeps them: no `javascript:`
    // or `file:` link in a file meant for another browser.
    if !crate::importable(&tab.url) {
        return;
    }
    let pad = "    ".repeat(depth);
    let added = tab
        .created_at
        .map(|t| format!(" ADD_DATE=\"{t}\""))
        .unwrap_or_default();
    let _ = writeln!(
        out,
        "{pad}<DT><A HREF=\"{}\"{added}>{}</A>",
        escape(&tab.url),
        escape(&tab.title)
    );
}

/// HTML-escaped, without control characters (the export may go to a
/// terminal).
fn escape(s: &str) -> String {
    s.replace(|c: char| c.is_control(), "")
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
