//! A query put into a search address. The command bar and the search item
//! of a page's menu search this way.

/// The address that searches `query` with `template`, an address with
/// `%s` where the query goes (`Settings.search_template`, a site search).
#[must_use]
pub fn search_address(template: &str, query: &str) -> String {
    template.replace("%s", &urlencode(query))
}

/// `s` as a value in an address's query: letters, digits and `-_.~` as
/// they are, spaces as `+`, every other byte percent-encoded.
#[must_use]
pub fn urlencode(s: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::search_address;

    #[test]
    fn the_query_goes_where_the_template_has_its_place() {
        let open = |query| search_address("https://search.example/?q=%s&ia=web", query);
        assert_eq!(
            open("rust traits"),
            "https://search.example/?q=rust+traits&ia=web"
        );
        // Characters that would end the value or the address are encoded.
        assert_eq!(
            open("a&b=c#d"),
            "https://search.example/?q=a%26b%3Dc%23d&ia=web"
        );
        assert_eq!(
            open("Straße"),
            "https://search.example/?q=Stra%C3%9Fe&ia=web"
        );
    }
}
