//! A bookmarks file: the HTML that Safari, Chrome, Firefox and most other
//! browsers write with "Export Bookmarks" (the Netscape bookmark format,
//! which `export.rs` writes too). It is a list of `<DT><A HREF=…>title</A>`
//! lines; a folder is `<DT><H3>name</H3>` followed by its own `<DL>` list.
//! Browsers write it loosely, so this reads tags rather than parsing HTML.

use magnetite_core::{Folder, SidebarNode, TabItem};

use crate::{Ids, ImportError, MAX_DEPTH, importable};

/// The bookmarks and folders of the file `html`, nested as they are there.
/// Folders that end up empty and addresses that are no web pages are left
/// out. A file without any bookmark is `ImportError::Empty`.
pub fn bookmarks(html: &str) -> Result<Vec<SidebarNode>, ImportError> {
    let mut ids = Ids::new("file");
    // The lists that are open, innermost last, each with its folder's name
    // (the first is the file's own list).
    let mut open: Vec<(String, Vec<SidebarNode>)> = vec![(String::new(), Vec::new())];
    // The name of a folder whose list has not started yet.
    let mut named: Option<String> = None;
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        rest = &rest[at + 1..];
        let end = rest.find('>').unwrap_or(rest.len());
        let (tag, after) = (&rest[..end], rest.get(end + 1..).unwrap_or(""));
        let name = tag
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match name.as_str() {
            "h3" => named = Some(text_until(after, "</h3")),
            "dl" => {
                if open.len() > MAX_DEPTH {
                    return Err(ImportError::TooDeep(MAX_DEPTH));
                }
                // The file's own list has no folder line before it.
                if let Some(title) = named.take() {
                    open.push((title, Vec::new()));
                }
            }
            "/dl" if open.len() > 1 => {
                let (title, children) = open.pop().unwrap_or_default();
                if !children.is_empty() {
                    let folder = Folder {
                        id: ids.next()?,
                        title: if title.is_empty() {
                            "Untitled".into()
                        } else {
                            title
                        },
                        icon: None,
                        children,
                        smart: None,
                    };
                    last(&mut open).push(SidebarNode::Folder(folder));
                }
            }
            "a" => {
                let title = text_until(after, "</a");
                if let Some(url) = attribute(tag, "href").filter(|u| importable(u)) {
                    let added = attribute(tag, "add_date").and_then(|d| d.parse().ok());
                    last(&mut open).push(SidebarNode::Tab(TabItem {
                        custom_title: !title.is_empty(),
                        created_at: added.filter(|&t: &i64| t > 0),
                        ..TabItem::new(
                            ids.next()?,
                            &url,
                            if title.is_empty() { &url } else { &title },
                        )
                    }));
                }
            }
            _ => {}
        }
        rest = after;
    }
    // A file that ends inside folders: what they hold goes up a level.
    while open.len() > 1 {
        let (_, children) = open.pop().unwrap_or_default();
        last(&mut open).extend(children);
    }
    let found = open.pop().map(|(_, list)| list).unwrap_or_default();
    if found.is_empty() {
        return Err(ImportError::Empty("The file".into()));
    }
    Ok(found)
}

/// The innermost open list.
fn last(open: &mut [(String, Vec<SidebarNode>)]) -> &mut Vec<SidebarNode> {
    let at = open.len() - 1;
    &mut open[at].1
}

/// The text from the start of `html` up to the closing tag `close`
/// (lower case, without its `>`), without tags inside it and with its
/// character references resolved.
fn text_until(html: &str, close: &str) -> String {
    let end = html.to_ascii_lowercase().find(close).unwrap_or(html.len());
    let mut text = String::new();
    let mut in_tag = false;
    for c in html[..end].chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    unescape(text.trim())
}

/// The value of attribute `name` (lower case) in the inside of a tag.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    loop {
        let at = from + lower[from..].find(name)?;
        from = at + name.len();
        // A whole attribute name, not the end of another one.
        let starts = tag[..at].ends_with(char::is_whitespace);
        let value = tag[from..].trim_start();
        let Some(value) = value.strip_prefix('=').filter(|_| starts) else {
            continue;
        };
        let value = value.trim_start();
        let quoted = value.chars().next().filter(|q| matches!(q, '"' | '\''));
        return Some(unescape(match quoted {
            Some(q) => value[1..].split(q).next().unwrap_or(""),
            None => value.split_whitespace().next().unwrap_or(""),
        }));
    }
}

/// `text` with the character references bookmark files use resolved.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let known = rest[1..].find(';').filter(|&end| end <= 8).and_then(|end| {
            let name = &rest[1..=end];
            let c = match name {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                "nbsp" => ' ',
                _ => {
                    let number = name.strip_prefix('#')?;
                    let code = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => number.parse().ok()?,
                    };
                    char::from_u32(code)?
                }
            };
            Some((c, end + 2))
        });
        let (c, len) = known.unwrap_or(('&', 1));
        out.push(c);
        rest = &rest[len..];
    }
    out.push_str(rest);
    out
}
