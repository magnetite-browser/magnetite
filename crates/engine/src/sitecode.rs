//! The user's own CSS and JavaScript for a site (Settings → User scripts
//! and styles): put into every page of that site as it loads, in tabs,
//! peeks and mini windows. The CSS goes in when the document begins, as a
//! style sheet made in script, which a page's content security policy does
//! not stop; the JavaScript runs once the page has loaded. Both run as part
//! of the page, with the page's own rights and nothing of Magnetite's.

use std::sync::RwLock;

use cef::*;
use serde::{Deserialize, Serialize};

use crate::client::host_of;
use crate::engine::Engine;

/// What the user added to the pages of `site`: a host without `www.`, which
/// stands for its subdomains too.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SiteCode {
    pub site: String,
    pub css: String,
    pub js: String,
    /// Off: kept, but not put into pages.
    pub enabled: bool,
}

/// What the app last handed over (`Engine::set_site_code`).
static CODE: RwLock<Vec<SiteCode>> = RwLock::new(Vec::new());

/// The code for the page at `url`, if any is on: that of the longest site
/// the page's host is on.
fn code_for<'a>(code: &'a [SiteCode], url: &str) -> Option<&'a SiteCode> {
    let host = host_of(url)?;
    let on = |site: &str| host == site || host.ends_with(&format!(".{site}"));
    (code.iter())
        .filter(|c| c.enabled && !c.site.is_empty() && on(&c.site))
        .max_by_key(|c| c.site.len())
}

/// A script that makes `css` the document's sheet of the user's own styles:
/// added on first use, exchanged after that, emptied by an empty `css`.
fn style_script(css: &str) -> String {
    let css = serde_json::to_string(css).unwrap_or_else(|_| "\"\"".into());
    format!(
        "(() => {{ const css = {css}; const mark = '__magnetiteSiteStyle'; \
         let sheet = document.adoptedStyleSheets.find((s) => s[mark]); \
         if (!sheet) {{ if (!css) return; sheet = new CSSStyleSheet(); sheet[mark] = true; \
         document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet]; }} \
         sheet.replaceSync(css); }})()"
    )
}

fn run(frame: &Frame, script: &str) {
    frame.execute_java_script(Some(&CefString::from(script)), None, 0);
}

fn frame_url(frame: &Frame) -> String {
    CefString::from(&frame.url()).to_string()
}

impl Engine {
    /// Use `code` from now on. Pages that are open get the CSS at once (or
    /// lose it); JavaScript runs from the next load of a page on.
    pub fn set_site_code(&self, code: Vec<SiteCode>) {
        if let Ok(mut current) = CODE.write() {
            *current = code;
        }
        let browsers: Vec<Browser> = (self.lock().contents.values())
            .filter_map(|c| c.browser.clone())
            .collect();
        for frame in browsers.iter().filter_map(ImplBrowser::main_frame) {
            self.put_site_style(&frame);
        }
    }

    /// Give the document in `frame` (a page's main frame) the user's CSS for
    /// its site, or take away what an earlier setting gave it.
    #[allow(clippy::unused_self)] // an Engine method like its siblings
    pub(crate) fn put_site_style(&self, frame: &Frame) {
        let Ok(code) = CODE.read() else { return };
        let url = frame_url(frame);
        // Only web pages: never Magnetite's own or Chromium's.
        if host_of(&url).is_none() {
            return;
        }
        let css = code_for(&code, &url).map_or("", |c| c.css.as_str());
        run(frame, &style_script(css));
    }

    /// The page in `frame` (a main frame) has loaded: run the user's
    /// JavaScript for its site.
    #[allow(clippy::unused_self)] // an Engine method like its siblings
    pub(crate) fn run_site_script(&self, frame: &Frame) {
        let Ok(code) = CODE.read() else { return };
        if let Some(code) = code_for(&code, &frame_url(frame))
            && !code.js.trim().is_empty()
        {
            run(frame, &code.js);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(site: &str, css: &str, enabled: bool) -> SiteCode {
        SiteCode {
            site: site.into(),
            css: css.into(),
            js: String::new(),
            enabled,
        }
    }

    #[test]
    fn a_page_gets_the_code_of_the_closest_site_that_is_on() {
        let all = [
            code("example.org", "a", true),
            code("docs.example.org", "b", true),
            code("example.net", "c", false),
        ];
        let css = |url: &str| code_for(&all, url).map(|c| c.css.clone());
        assert_eq!(css("https://example.org/page").as_deref(), Some("a"));
        assert_eq!(css("https://www.example.org/").as_deref(), Some("a"));
        assert_eq!(css("https://docs.example.org/x").as_deref(), Some("b"));
        assert_eq!(css("https://api.docs.example.org/x").as_deref(), Some("b"));
        assert_eq!(css("https://notexample.org/"), None);
        assert_eq!(css("https://example.net/"), None, "switched off");
        assert_eq!(css("magnetite://ui/settings"), None);
    }

    #[test]
    fn css_is_handed_to_the_page_as_a_text_not_as_code() {
        let script = style_script("a::after { content: \"</script>'\\\n\"; }");
        assert!(script.contains(r#"const css = "a::after { content: \"</script>'\\\n\"; }";"#));
    }
}
