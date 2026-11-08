//! Links as people share them: tracking parameters removed, nothing else
//! touched (order, encoding, duplicates and the fragment stay as they were).

/// Query parameters that only identify campaigns or clicks. Ships with each
/// release; never fetched from anywhere.
const TRACKING_PARAMS: &[&str] = &[
    "fbclid",
    "gclid",
    "dclid",
    "gbraid",
    "wbraid",
    "msclkid",
    "yclid",
    "twclid",
    "ttclid",
    "li_fat_id",
    "igshid",
    "mc_cid",
    "mc_eid",
    "_hsenc",
    "_hsmi",
    "mkt_tok",
    "oly_anon_id",
    "oly_enc_id",
    "vero_id",
    "rb_clickid",
    "s_cid",
    "wickedid",
    "_openstat",
];

/// Prefixes treated like the names above (`utm_source`, `utm_medium`, …).
const TRACKING_PREFIXES: &[&str] = &["utm_", "pk_", "mtm_"];

fn is_tracking(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    TRACKING_PARAMS.contains(&name.as_str())
        || TRACKING_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Remove tracking parameters from `url`. Returns the cleaned URL and how many
/// parameters were removed; a URL without any comes back unchanged.
pub fn clean_link(url: &str) -> (String, usize) {
    let (before_fragment, fragment) = match url.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (url, None),
    };
    let Some((base, query)) = before_fragment.split_once('?') else {
        return (url.to_string(), 0);
    };
    let mut removed = 0;
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let name = pair.split_once('=').map_or(*pair, |(n, _)| n);
            let drop = !name.is_empty() && is_tracking(name);
            removed += usize::from(drop);
            !drop
        })
        .collect();
    if removed == 0 {
        return (url.to_string(), 0);
    }
    let mut out = base.to_string();
    let query = kept.join("&");
    if !query.is_empty() {
        out.push('?');
        out.push_str(&query);
    }
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    (out, removed)
}

/// `[title](url)`, with brackets in the title escaped.
pub fn markdown_link(title: &str, url: &str) -> String {
    let title = if title.trim().is_empty() {
        url
    } else {
        title.trim()
    };
    let title = title
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    // Both brackets: an open one would pair with the link's closing one.
    format!("[{title}]({})", url.replace('(', "%28").replace(')', "%29"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_trackers_and_keeps_everything_else() {
        assert_eq!(
            clean_link("https://a.org/p?utm_source=x&id=5#a"),
            ("https://a.org/p?id=5#a".into(), 1)
        );
        assert_eq!(
            clean_link("https://a.org/?q=%20x&UTM_Medium=m&q=2&fbclid=abc&x"),
            ("https://a.org/?q=%20x&q=2&x".into(), 2),
            "order, encoding, duplicates and bare keys stay"
        );
        assert_eq!(
            clean_link("https://a.org/?gclid=1#top"),
            ("https://a.org/#top".into(), 1)
        );
        assert_eq!(
            clean_link("https://a.org/?id=1"),
            ("https://a.org/?id=1".into(), 0)
        );
        assert_eq!(
            clean_link("https://a.org/#utm_source=x"),
            ("https://a.org/#utm_source=x".into(), 0),
            "fragment untouched"
        );
        assert_eq!(
            clean_link("https://a.org/?utmost=1"),
            ("https://a.org/?utmost=1".into(), 0),
            "prefix needs the underscore"
        );
    }

    #[test]
    fn markdown_escapes_title() {
        assert_eq!(
            markdown_link("A [b]", "https://x.org/a_(b)"),
            "[A \\[b\\]](https://x.org/a_%28b%29)"
        );
        assert_eq!(
            markdown_link(" ", "https://x.org"),
            "[https://x.org](https://x.org)"
        );
    }
}
