//! A bookmarks file as browsers export it, read back into folders.

mod common;

use common::outline;
use magnetite_core::SidebarNode;
use magnetite_importer::bookmarks_file::bookmarks;
use magnetite_importer::{ImportError, export::bookmarks_html, firefox::import};
use std::path::Path;

/// The shape Safari writes: upper-case tags, `<p>` after lists, a Reading
/// List folder with an id, and no closing tags for `<DT>`.
const SAFARI: &str = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<HTML>
<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">
<Title>Bookmarks</Title>
<H1>Bookmarks</H1>
<DT><H3 FOLDED>Favorites</H3>
<DL><p>
    <DT><A HREF="https://example.org/a?x=1&amp;y=2">Tom &amp; Jerry&#39;s &lt;page&gt;</A>
    <DT><H3 FOLDED>Dev</H3>
    <DL><p>
        <DT><A HREF="https://example.org/docs" ADD_DATE="1794047941">Docs</A>
        <DT><a href='https://example.org/lower'>lower <b>case</b></a>
        <DT><H3>Empty</H3>
        <DL><p>
        </DL><p>
    </DL><p>
    <DT><A HREF="javascript:alert(1)">Bookmarklet</A>
    <DT><A HREF="file:///etc/hosts">Local</A>
    <DT><A DATA-HREF="https://wrong.example/" HREF="https://example.org/untitled"></A>
</DL><p>
<DT><H3 FOLDED id="com.apple.ReadingList">Reading List</H3>
<DL><p>
    <DT><A HREF="http://example.net/later">Later</A>
</DL><p>
</HTML>
"#;

#[test]
fn reads_folders_and_links_and_leaves_out_what_is_no_web_page() {
    let found = bookmarks(SAFARI).unwrap();
    assert_eq!(
        outline(&found),
        "Favorites[Tom & Jerry's <page>, Dev[Docs, lower case], https://example.org/untitled], \
         Reading List[Later]"
    );
    let all: Vec<_> = found.iter().flat_map(SidebarNode::tabs).collect();
    assert_eq!(all[0].url, "https://example.org/a?x=1&y=2");
    assert!(all[0].custom_title && all[0].home_url.is_none());
    assert_eq!(all[1].created_at, Some(1_794_047_941));
    // No title: the address stands in, and a rename is not claimed.
    assert!(!all[3].custom_title);
}

#[test]
fn reads_back_what_the_export_writes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/firefox");
    let snap = import(&root, "Firefox", false).unwrap();
    let found = bookmarks(&bookmarks_html(&snap)).unwrap();
    let saved: Vec<_> = found
        .iter()
        .flat_map(SidebarNode::tabs)
        .map(|t| t.url.clone())
        .collect();
    for bookmark in snap.all_bookmarks() {
        assert!(saved.contains(&bookmark.url), "{} is missing", bookmark.url);
    }
}

#[test]
fn files_without_bookmarks_or_nested_too_deep_are_refused() {
    assert!(matches!(
        bookmarks("<html><p>Hello</p>"),
        Err(ImportError::Empty(_))
    ));
    assert!(matches!(bookmarks(""), Err(ImportError::Empty(_))));
    let deep = "<DT><H3>f</H3><DL>".repeat(40) + "<DT><A HREF=\"https://example.org/\">x</A>";
    assert!(matches!(bookmarks(&deep), Err(ImportError::TooDeep(_))));
    // A file cut off inside a folder keeps what it had.
    let cut = "<DL><DT><H3>Open</H3><DL><DT><A HREF=\"https://example.org/\">Kept</A>";
    assert_eq!(outline(&bookmarks(cut).unwrap()), "Kept");
}
