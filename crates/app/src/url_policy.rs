//! Which addresses Magnetite opens in a tab, by where they come from.
//!
//! A page asking for a new tab turns into a load Magnetite starts itself, and
//! Chromium checks those less strictly than links: it would open `data:`
//! pages, our own `magnetite://ui/` pages or files. So every way in has a
//! list: web addresses always, internal and local pages only when the user
//! (or Magnetite's own UI) asks, script and data addresses never.

/// Where an address comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Typed or chosen in Magnetite's UI (palette, settings, menus).
    User,
    /// A web page: a popup, a link to a new tab.
    Page,
    /// A window Chromium opened itself (an extension's new tab).
    Chromium,
    /// Another app, with Magnetite as the default browser.
    OtherApp,
    /// Data from elsewhere: sync, an import, smart-folder results.
    Stored,
}

/// Whether Magnetite opens `url` coming from `source`.
#[allow(clippy::match_same_arms)] // one arm per scheme reads as the table
pub fn allowed(url: &str, source: Source) -> bool {
    use Source::{Chromium, OtherApp, Page, Stored, User};
    let Some((scheme, rest)) = url.split_once(':') else {
        return false;
    };
    let valid = scheme
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !valid {
        return false;
    }
    match (scheme.to_ascii_lowercase().as_str(), source) {
        ("http" | "https", _) => rest.starts_with("//"),
        ("about", _) => rest == "blank",
        ("blob", User | Page) => true,
        ("magnetite", User) => rest.starts_with("//ui/"),
        ("chrome" | "chrome-extension", User | Chromium) => rest.starts_with("//"),
        ("file", User | OtherApp) => rest.starts_with("//"),
        ("view-source", User) => true,
        (_, User | Page | Chromium | OtherApp | Stored) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::Source::{Chromium, OtherApp, Page, Stored, User};
    use super::allowed;

    #[test]
    fn web_addresses_open_from_anywhere_script_and_data_from_nowhere() {
        for source in [User, Page, Chromium, OtherApp, Stored] {
            assert!(allowed("https://example.org/a", source));
            assert!(allowed("HTTP://example.org", source));
            assert!(allowed("about:blank", source));
            assert!(!allowed("javascript:alert(1)//https://example.org", source));
            assert!(!allowed("data:text/html,<script>x</script>", source));
            assert!(!allowed("vbscript:x", source));
            assert!(!allowed("no scheme at all", source));
        }
    }

    #[test]
    fn internal_and_local_pages_only_from_trusted_sources() {
        assert!(allowed("magnetite://ui/settings", User));
        assert!(!allowed("magnetite://ui/settings", Page));
        assert!(!allowed("magnetite://ui/settings", Stored));
        assert!(allowed("chrome://extensions/", User));
        assert!(allowed("chrome://extensions/?id=x", Chromium));
        assert!(!allowed("chrome://settings", Page));
        assert!(!allowed("chrome-extension://abc/popup.html", Page));
        assert!(allowed("file:///Users/me/a.html", OtherApp));
        assert!(!allowed("file:///etc/passwd", Page));
        assert!(!allowed("file:///etc/passwd", Stored));
        assert!(allowed("blob:https://example.org/uuid", Page));
        assert!(!allowed("blob:https://example.org/uuid", Stored));
    }
}
