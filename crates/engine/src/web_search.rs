//! The search item of a page's menu. Chromium's own ("Search Google for
//! “…”") names and searches with Chromium's search engine, which stays
//! Google whatever Settings → Search says. The app gives its engine here
//! (`set_web_search`); the item then names it and searches with it
//! (`client.rs`), and the selection never reaches Google unless that is the
//! engine chosen.

use std::sync::{PoisonError, RwLock};

/// A search engine as a page's menu uses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSearch {
    /// Its name, as in "Search Kagi for “…”".
    pub name: String,
    /// Its search address, with `%s` where the query goes.
    pub template: String,
}

/// The engine a page's menu searches with; none until the app gives one.
static SEARCH: RwLock<Option<WebSearch>> = RwLock::new(None);

/// Search with `search` from the next page menu on.
pub fn set_web_search(search: WebSearch) {
    *SEARCH.write().unwrap_or_else(PoisonError::into_inner) = Some(search);
}

fn current() -> Option<WebSearch> {
    SEARCH
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// The most characters of the selection the label shows, as in Chromium.
const LABEL_TEXT_MAX: usize = 50;

/// The selected text as it is searched, as Chromium's item has it: without
/// white space at the ends, line breaks and tabs as spaces. `None` for a
/// selection of white space alone.
fn query(selection: &str) -> Option<String> {
    let text = selection
        .trim()
        .replace(['\n', '\r', '\t', '\u{2028}', '\u{2029}'], " ");
    (!text.is_empty()).then_some(text)
}

/// The label for searching `query` with the engine called `name`: a long
/// query is cut after a word, as Chromium cuts it. `&` is doubled, since a
/// menu takes a single one for the key of a mnemonic.
fn label(name: &str, query: &str) -> String {
    let text = if query.chars().count() <= LABEL_TEXT_MAX {
        query.to_string()
    } else {
        let cut: String = query.chars().take(LABEL_TEXT_MAX - 1).collect();
        let word = cut
            .rfind(' ')
            .filter(|&at| at > 0)
            .map_or(cut.as_str(), |at| &cut[..at]);
        format!("{}…", word.trim_end())
    };
    magnetite_ui::t!("app.page_menu.search_for", engine = name, text = text).replace('&', "&&")
}

/// The item's label for `selection`. `None` while the app has given no
/// engine, or for an empty selection: Chromium's item stays as it is.
pub(crate) fn menu_label(selection: &str) -> Option<String> {
    let search = current()?;
    Some(label(&search.name, &query(selection)?))
}

/// The address the item opens for `selection`, `None` as for `menu_label`.
pub(crate) fn menu_address(selection: &str) -> Option<String> {
    let search = current()?;
    Some(magnetite_core::search::search_address(
        &search.template,
        &query(selection)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::{label, query};

    #[test]
    fn the_selection_is_searched_on_one_line() {
        assert_eq!(query("  rust\ntraits\t "), Some("rust traits".into()));
        assert_eq!(query(" \n "), None);
    }

    #[test]
    fn the_label_names_the_engine_and_a_short_selection() {
        assert_eq!(
            label("Brave Search", "rust traits"),
            "Search Brave Search for “rust traits”"
        );
        // A menu would read a single `&` as a mnemonic.
        assert_eq!(label("Kagi", "a & b"), "Search Kagi for “a && b”");
        let long = "a selection that is much longer than the label has room for";
        assert_eq!(
            label("Kagi", long),
            "Search Kagi for “a selection that is much longer than the label…”"
        );
        // Without a space to cut at, it is cut inside the word.
        let word = "x".repeat(60);
        assert_eq!(
            label("Kagi", &word),
            format!("Search Kagi for “{}…”", "x".repeat(49))
        );
    }
}
