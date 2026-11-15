//! The ad blocker: the filter lists the user subscribed to, compiled by
//! adblock-rust, checked for every request a web page makes (network
//! rules), and the scriptlets the lists ask for in a page (`scriptlets.rs`
//! puts them there). Hiding page elements is not built yet.
//!
//! The app compiles the lists (`Filters::compile`, slow: off the main
//! thread) and hands them over (`set_filters`); CEF asks on its IO thread
//! (`resource_request_handler`), so the filters sit behind a lock of their
//! own, not the engine's.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use ::adblock::Engine as FilterEngine;
use ::adblock::lists::{FilterSet, ParseOptions, RuleTypes};
use ::adblock::request::Request as FilterRequest;
use ::adblock::resources::{PermissionMask, Resource};
use cef::*;

use crate::client::host_of;
use crate::sitedata::registrable_domain;

/// The text of a filter list (Adblock Plus syntax).
pub struct List {
    pub text: String,
    /// Whether its rules may use the scriptlets that need trust
    /// (`TRUSTED`): they can do a page anything, so only the lists of the
    /// scriptlets' own makers may.
    pub trusted: bool,
}

/// What a scriptlet marked as needing trust asks of the list using it, and
/// what a trusted list has: the first of adblock-rust's permission bits.
const TRUSTED: PermissionMask = PermissionMask::from_bits(1);

/// Compiled filter lists, ready for `set_filters`.
pub struct Filters {
    engine: FilterEngine,
    /// The lists' scriptlet rules are in use: `page_script` has something
    /// to say.
    scriptlets: bool,
}

impl Filters {
    /// Compile filter lists. Rules the blocker cannot use are skipped, so
    /// is anything that is no rule. `scriptlets` is the code the lists'
    /// scriptlet rules (`##+js(…)`) refer to: resources as adblock-rust
    /// reads them (JSON), a scriptlet that needs trust marked with
    /// permission 1. Without it, and when it cannot be read, the scriptlet
    /// rules are left out.
    pub fn compile(lists: impl IntoIterator<Item = List>, scriptlets: Option<&str>) -> Self {
        let resources =
            scriptlets.and_then(|json| serde_json::from_str::<Vec<Resource>>(json).ok());
        let mut set = FilterSet::new(false);
        for List { text, trusted } in lists {
            let permissions = if trusted {
                TRUSTED
            } else {
                PermissionMask::default()
            };
            if resources.is_some() {
                // Of the rules for a page's content only the scriptlets:
                // nothing here hides elements yet, and their rules are many.
                let rules: Vec<&str> = text.lines().filter(|l| l.contains("#+js(")).collect();
                let rules = rules.join("\n");
                let options = ParseOptions {
                    rule_types: RuleTypes::CosmeticOnly,
                    permissions,
                    ..ParseOptions::default()
                };
                set.add_filter_list(rules, options);
            }
            let options = ParseOptions {
                rule_types: RuleTypes::NetworkOnly,
                permissions,
                ..ParseOptions::default()
            };
            set.add_filter_list(text, options);
        }
        let mut engine = FilterEngine::new_with_filter_set(set);
        let scriptlets = resources.is_some();
        if let Some(resources) = resources {
            engine.use_resources(resources);
        }
        Self { engine, scriptlets }
    }

    /// The script a page at `url` is to run before its own: the scriptlets
    /// the lists have for it, in a scope of their own. Empty for none.
    fn page_script(&self, url: &str) -> String {
        if !self.scriptlets {
            return String::new();
        }
        let scriptlets = self.engine.url_cosmetic_resources(url).injected_script;
        if scriptlets.is_empty() {
            return scriptlets;
        }
        // `scriptletGlobals` is what uBlock Origin's scriptlets share state
        // through; it must exist.
        format!("(function() {{\nconst scriptletGlobals = {{}};\n{scriptlets}}})();")
    }

    /// What to do with a `method` request for `url` of type `kind`
    /// (adblock-rust's names: "script", "image", …) made by a page at
    /// `source`.
    fn verdict(&self, url: &str, source: &str, kind: &str, method: &str) -> Verdict {
        let Ok(request) = FilterRequest::new(url, source, kind, method) else {
            return Verdict::Load;
        };
        let result = self.engine.check_network_request(&request);
        if result.should_block() {
            Verdict::Block
        } else if let Some(url) = result.rewritten_url {
            Verdict::Rewrite(url)
        } else {
            Verdict::Load
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Load,
    Block,
    /// Load this address instead (tracking parameters removed).
    Rewrite(String),
}

/// The filters in use; `None` while the blocker is off.
static FILTERS: RwLock<Option<Arc<Filters>>> = RwLock::new(None);
/// Sites (registrable domains) the user let load everything.
static ALLOWED: RwLock<Option<HashSet<String>>> = RwLock::new(None);

/// Block with `filters` from now on; `None` turns the blocker off.
pub fn set_filters(filters: Option<Filters>) {
    *FILTERS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = filters.map(Arc::new);
}

/// The sites (registrable domains) where nothing is blocked.
pub fn set_allowed_sites(sites: impl IntoIterator<Item = String>) {
    *ALLOWED
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sites.into_iter().collect());
}

/// adblock-rust's name for a CEF resource type; `None` for what is never
/// blocked: the page itself (the address the user opened).
fn kind_name(kind: ResourceType) -> Option<&'static str> {
    Some(match kind {
        ResourceType::MAIN_FRAME
        | ResourceType::NAVIGATION_PRELOAD_MAIN_FRAME
        | ResourceType::NAVIGATION_PRELOAD_SUB_FRAME => return None,
        ResourceType::SUB_FRAME => "sub_frame",
        ResourceType::STYLESHEET => "stylesheet",
        ResourceType::SCRIPT
        | ResourceType::WORKER
        | ResourceType::SHARED_WORKER
        | ResourceType::SERVICE_WORKER => "script",
        ResourceType::IMAGE | ResourceType::FAVICON => "image",
        ResourceType::FONT_RESOURCE => "font",
        ResourceType::OBJECT | ResourceType::PLUGIN_RESOURCE => "object",
        ResourceType::MEDIA => "media",
        ResourceType::XHR => "xmlhttprequest",
        ResourceType::PING => "ping",
        ResourceType::CSP_REPORT => "csp_report",
        _ => "other",
    })
}

fn is_web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// The filters in use, unless the user let the site of the page at `page`
/// load everything.
fn filters_for(page: &str) -> Option<Arc<Filters>> {
    let filters = FILTERS.read().ok().and_then(|f| f.clone())?;
    let allowed = ALLOWED.read().is_ok_and(|sites| {
        let site = host_of(page).map(|h| registrable_domain(&h));
        sites
            .as_ref()
            .zip(site)
            .is_some_and(|(sites, site)| sites.contains(&site))
    });
    (!allowed).then_some(filters)
}

/// What to do with a `method` request for `url` of type `kind` from a page
/// at `source`, with the filters and allowed sites in use. Only web pages'
/// requests for web addresses are checked: not Magnetite's own pages,
/// extensions or DevTools.
fn decide(url: &str, source: &str, kind: ResourceType, method: &str) -> Verdict {
    let Some(kind) = kind_name(kind).filter(|_| is_web(url) && is_web(source)) else {
        return Verdict::Load;
    };
    filters_for(source).map_or(Verdict::Load, |f| f.verdict(url, source, kind, method))
}

/// The script a page at `url` is to run before its own, with the filters
/// and allowed sites in use: the lists' scriptlets for it. Empty for none,
/// as for every page that is no web page.
fn page_script(url: &str) -> String {
    if !is_web(url) {
        return String::new();
    }
    filters_for(url).map_or_else(String::new, |f| f.page_script(url))
}

/// The handler for a request a browser (`browser`, if any: service workers
/// have none) is about to make, when the blocker stops or changes it. The
/// page counts as the source, so a site's own exceptions and the user's
/// allowed sites hold for everything in it, also its frames; without a
/// page, the request's initiator. The request for a page's document itself
/// is never stopped: it is where the page's scriptlets are seen to.
pub(crate) fn resource_request_handler(
    browser: Option<&Browser>,
    request: Option<&Request>,
    initiator: Option<&CefString>,
) -> Option<ResourceRequestHandler> {
    // Nothing to ask while the blocker is off: the common case stays cheap.
    let filters = FILTERS.read().ok().and_then(|f| f.clone());
    let request = request?;
    if request.resource_type() == ResourceType::MAIN_FRAME {
        // Also with the blocker off again: a page may still have a script.
        let scriptlets = filters.is_some_and(|f| f.scriptlets) || crate::scriptlets::in_use();
        return (scriptlets && browser.is_some()).then(|| crate::scriptlets::exchange(page_script));
    }
    filters?;
    let url = CefString::from(&request.url()).to_string();
    let page = browser
        .and_then(ImplBrowser::main_frame)
        .map(|f| CefString::from(&f.url()).to_string())
        .filter(|u| !u.is_empty());
    let source = page
        .or_else(|| initiator.map(CefString::to_string))
        .unwrap_or_default();
    let method = CefString::from(&request.method()).to_string();
    match decide(&url, &source, request.resource_type(), &method) {
        Verdict::Load => None,
        Verdict::Block => Some(Stop::new(None)),
        Verdict::Rewrite(to) => Some(Stop::new(Some(to))),
    }
}

// Cancels a request, or sends it to another address (`to`).
wrap_resource_request_handler! {
    struct Stop {
        to: Option<String>,
    }
    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            match (&self.to, request) {
                // A changed address is a redirect to it.
                (Some(to), Some(request)) => {
                    request.set_url(Some(&CefString::from(to.as_str())));
                    ReturnValue::CONTINUE
                }
                _ => ReturnValue::CANCEL,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "\
||ads.example^
||tracker.example^$third-party
@@||ads.example/allowed.js
||cdn.news.example^$removeparam=utm_source
||beacon.example^$method=post
";

    fn list(text: &str) -> List {
        List {
            text: text.to_string(),
            trusted: false,
        }
    }

    #[test]
    fn requests_are_blocked_by_the_lists_rules() {
        let filters = Filters::compile([list(LIST)], None);
        let check =
            |url: &str, kind: &str| filters.verdict(url, "https://news.example/", kind, "GET");
        assert_eq!(
            check("https://ads.example/banner.js", "script"),
            Verdict::Block
        );
        assert_eq!(
            check("https://ads.example/allowed.js", "script"),
            Verdict::Load
        );
        assert_eq!(
            check("https://cdn.news.example/app.js", "script"),
            Verdict::Load
        );
        // Third-party only: the tracker's own site may use itself.
        assert_eq!(
            check("https://tracker.example/t.gif", "image"),
            Verdict::Block
        );
        let own = filters.verdict(
            "https://tracker.example/t.gif",
            "https://tracker.example/",
            "image",
            "GET",
        );
        assert_eq!(own, Verdict::Load);
        // A rule for one method: CEF's method name counts.
        let beacon = |method: &str| {
            filters.verdict(
                "https://beacon.example/r",
                "https://news.example/",
                "ping",
                method,
            )
        };
        assert_eq!(beacon("POST"), Verdict::Block);
        assert_eq!(beacon("GET"), Verdict::Load);
        // Parameters go from what a page fetches (by default not from scripts).
        assert_eq!(
            check(
                "https://cdn.news.example/api?utm_source=x&id=1",
                "xmlhttprequest"
            ),
            Verdict::Rewrite("https://cdn.news.example/api?id=1".into())
        );
    }

    /// Scriptlets as adblock-rust reads them: `hello` for any list, and
    /// `trusted-hello` for trusted lists only.
    fn scriptlets() -> String {
        use base64::Engine as _;
        let code = |js: &str| base64::engine::general_purpose::STANDARD.encode(js);
        serde_json::json!([
            {
                "name": "hello.js",
                "aliases": ["hi.js"],
                "kind": { "mime": "application/javascript" },
                "content": code("function hello(a) { window.hello = a; }"),
            },
            {
                "name": "trusted-hello.js",
                "aliases": [],
                "kind": { "mime": "application/javascript" },
                "content": code("function trustedHello(a) { window.trusted = a; }"),
                "permission": 1,
            },
        ])
        .to_string()
    }

    const SCRIPTLET_RULES: &str = "\
||ads.example^
news.example##+js(hi, 1)
news.example##+js(trusted-hello, 2)
news.example##.ad-banner
";

    #[test]
    fn a_page_gets_the_scriptlets_the_lists_have_for_it() {
        let rules = |trusted| List {
            text: SCRIPTLET_RULES.to_string(),
            trusted,
        };
        let filters = Filters::compile([rules(false)], Some(&scriptlets()));
        let script = filters.page_script("https://www.news.example/article");
        // In a scope of its own, with what uBlock Origin's scriptlets expect.
        assert!(script.starts_with("(function() {\nconst scriptletGlobals = {};\n"));
        assert!(script.ends_with("})();"));
        assert!(script.contains("function hello(a)"));
        assert!(script.contains("hello(\"1\")"));
        // A scriptlet that needs trust, asked for by a list that has none.
        assert!(!script.contains("trustedHello"));
        assert_eq!(filters.page_script("https://other.example/"), "");
        // The network rules of the same list still hold.
        assert_eq!(
            filters.verdict(
                "https://ads.example/a.js",
                "https://news.example/",
                "script",
                "GET"
            ),
            Verdict::Block
        );

        let trusted = Filters::compile([rules(true)], Some(&scriptlets()));
        let script = trusted.page_script("https://news.example/");
        assert!(script.contains("hello(\"1\")") && script.contains("trustedHello(\"2\")"));

        // Without the scriptlets' code, or with code that cannot be read,
        // no page gets a script.
        for code in [None, Some("no json")] {
            let none = Filters::compile([rules(true)], code);
            assert_eq!(none.page_script("https://news.example/"), "");
        }
    }

    #[test]
    fn only_web_pages_requests_count_and_allowed_sites_load_everything() {
        let lists = [
            list(LIST),
            list("news.example,other.example##+js(hello, 1)"),
        ];
        set_filters(Some(Filters::compile(lists, Some(&scriptlets()))));
        let ad = "https://ads.example/banner.js";
        assert_ne!(page_script("https://news.example/"), "");
        assert_eq!(page_script("magnetite://ui/sidebar"), "");
        assert_eq!(
            decide(
                ad,
                "https://www.news.example/a",
                ResourceType::SCRIPT,
                "GET"
            ),
            Verdict::Block
        );
        // The page itself, and pages that are no web pages.
        assert_eq!(
            decide(ad, "https://news.example/", ResourceType::MAIN_FRAME, "GET"),
            Verdict::Load
        );
        assert_eq!(
            decide(ad, "magnetite://ui/sidebar", ResourceType::SCRIPT, "GET"),
            Verdict::Load
        );
        assert_eq!(
            decide(
                ad,
                "chrome-extension://abc/popup.html",
                ResourceType::SCRIPT,
                "GET"
            ),
            Verdict::Load
        );
        // A site the user allowed, with any of its hosts.
        set_allowed_sites(["news.example".to_string()]);
        assert_eq!(
            decide(
                ad,
                "https://www.news.example/a",
                ResourceType::SCRIPT,
                "GET"
            ),
            Verdict::Load
        );
        assert_eq!(
            decide(ad, "https://other.example/", ResourceType::SCRIPT, "GET"),
            Verdict::Block
        );
        // An allowed site gets no scriptlets either.
        assert_eq!(page_script("https://www.news.example/a"), "");
        assert_ne!(page_script("https://other.example/"), "");
        set_filters(None);
        assert_eq!(page_script("https://other.example/"), "");
        assert_eq!(
            decide(ad, "https://other.example/", ResourceType::SCRIPT, "GET"),
            Verdict::Load
        );
    }
}
