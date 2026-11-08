//! From typed input to an address or a search.

use crate::state::SiteSearch;

use super::MagnetiteApp;

/// Turn command-bar input into a URL: keep URLs, search a site for input
/// that starts with its keyword (`site_search`), add https:// to host-like
/// input, otherwise search with `template`.
pub(super) fn normalise_url_with(input: &str, template: &str, sites: &[SiteSearch]) -> String {
    let s = input.trim();
    if s.contains("://")
        || s.starts_with("about:")
        || s.starts_with("chrome://")
        || s.starts_with("magnetite://")
    {
        return s.to_string();
    }
    if let Some((site, query)) = site_search(s, sites) {
        return site.url.replace("%s", &urlencode(query));
    }
    if !s.contains(' ') && is_loopback(s) {
        // Servers on this computer speak plain http.
        return format!("http://{s}");
    }
    let looks_like_host = !s.contains(' ') && s.contains('.') && !s.ends_with('.');
    if looks_like_host {
        return format!("https://{s}");
    }
    template.replace("%s", &urlencode(s))
}

/// The site search `input` asks for: its first word is a keyword (any
/// case) and a query follows.
pub(super) fn site_search<'a>(
    input: &'a str,
    sites: &'a [SiteSearch],
) -> Option<(&'a SiteSearch, &'a str)> {
    let (keyword, query) = input.trim().split_once(char::is_whitespace)?;
    let query = query.trim();
    let keyword = keyword.to_lowercase();
    let site = sites.iter().find(|s| s.keyword == keyword)?;
    (!query.is_empty()).then_some((site, query))
}

/// Whether `host[:port][/path]` names this computer: `localhost`, a name
/// under it, or a loopback address.
pub(super) fn is_loopback(input: &str) -> bool {
    let authority = input.split(['/', '?', '#']).next().unwrap_or_default();
    let host = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => authority,
    };
    host == "localhost"
        || host.ends_with(".localhost")
        || host == "[::1]"
        || host
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|ip| ip.is_loopback())
}

pub(super) fn urlencode(s: &str) -> String {
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

impl MagnetiteApp {
    pub(super) fn normalise_url(&self, input: &str) -> String {
        self.with_state(|s| {
            normalise_url_with(
                input,
                &s.settings.search_template,
                &s.settings.site_searches,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_input_becomes_an_address_or_a_search() {
        let open = |input| normalise_url_with(input, "https://search.example/?q=%s", &[]);
        assert_eq!(open("example.org/a"), "https://example.org/a");
        assert_eq!(open("about:blank"), "about:blank");
        // Servers on this computer speak plain http.
        assert_eq!(open("localhost:3000"), "http://localhost:3000");
        assert_eq!(open("app.localhost/x"), "http://app.localhost/x");
        assert_eq!(open("127.0.0.1:8080"), "http://127.0.0.1:8080");
        assert_eq!(open("[::1]:8080"), "http://[::1]:8080");
        assert_eq!(
            open("localhost is down"),
            "https://search.example/?q=localhost+is+down"
        );
        assert_eq!(open("rust traits"), "https://search.example/?q=rust+traits");
    }

    #[test]
    fn a_keyword_and_a_query_search_that_site() {
        let sites = [SiteSearch {
            keyword: "gh".into(),
            name: "GitHub".into(),
            url: "https://github.com/search?q=%s".into(),
        }];
        let open = |input| normalise_url_with(input, "https://search.example/?q=%s", &sites);
        assert_eq!(open("gh rust cef"), "https://github.com/search?q=rust+cef");
        assert_eq!(open("GH  a&b "), "https://github.com/search?q=a%26b");
        // A keyword alone, or inside other words, is a plain search.
        assert_eq!(open("gh"), "https://search.example/?q=gh");
        assert_eq!(open("ghost town"), "https://search.example/?q=ghost+town");
        // Addresses stay addresses.
        assert_eq!(open("https://gh.example/ x"), "https://gh.example/ x");
        assert_eq!(
            site_search("gh\tissue", &sites).map(|(s, q)| (s.name.as_str(), q)),
            Some(("GitHub", "issue"))
        );
    }
}
