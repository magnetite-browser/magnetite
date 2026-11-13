//! The ad blocker: the filter lists the user subscribed to, compiled by
//! adblock-rust, checked for every request a web page makes. Only requests
//! (network rules): hiding page elements and scriptlets are not built yet.
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
use cef::*;

use crate::client::host_of;
use crate::sitedata::registrable_domain;

/// Compiled filter lists, ready for `set_filters`.
pub struct Filters(FilterEngine);

impl Filters {
    /// Compile the text of filter lists (Adblock Plus syntax). Rules the
    /// blocker cannot use are skipped, so is anything that is no rule.
    pub fn compile(lists: impl IntoIterator<Item = String>) -> Self {
        let mut set = FilterSet::new(false);
        let options = ParseOptions {
            rule_types: RuleTypes::NetworkOnly,
            ..ParseOptions::default()
        };
        for text in lists {
            set.add_filter_list(text, options);
        }
        Self(FilterEngine::new_with_filter_set(set))
    }

    /// What to do with a request for `url` of type `kind` (adblock-rust's
    /// names: "script", "image", …) made by a page at `source`.
    fn verdict(&self, url: &str, source: &str, kind: &str) -> Verdict {
        let Ok(request) = FilterRequest::new(url, source, kind, "GET") else {
            return Verdict::Load;
        };
        let result = self.0.check_network_request(&request);
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

/// What to do with a request for `url` of type `kind` from a page at
/// `source`, with the filters and allowed sites in use. Only web pages'
/// requests for web addresses are checked: not Magnetite's own pages,
/// extensions or DevTools.
fn decide(url: &str, source: &str, kind: ResourceType) -> Verdict {
    let web = |u: &str| u.starts_with("https://") || u.starts_with("http://");
    let Some(kind) = kind_name(kind).filter(|_| web(url) && web(source)) else {
        return Verdict::Load;
    };
    let Some(filters) = FILTERS.read().ok().and_then(|f| f.clone()) else {
        return Verdict::Load;
    };
    let allowed = ALLOWED.read().is_ok_and(|sites| {
        let site = host_of(source).map(|h| registrable_domain(&h));
        sites
            .as_ref()
            .zip(site)
            .is_some_and(|(sites, site)| sites.contains(&site))
    });
    if allowed {
        return Verdict::Load;
    }
    filters.verdict(url, source, kind)
}

/// The handler for a request a browser (`browser`, if any: service workers
/// have none) is about to make, when the blocker stops or changes it. The
/// page counts as the source, so a site's own exceptions and the user's
/// allowed sites hold for everything in it, also its frames; without a
/// page, the request's initiator.
pub(crate) fn resource_request_handler(
    browser: Option<&Browser>,
    request: Option<&Request>,
    initiator: Option<&CefString>,
) -> Option<ResourceRequestHandler> {
    // Nothing to ask while the blocker is off: the common case stays cheap.
    if FILTERS.read().ok().is_none_or(|f| f.is_none()) {
        return None;
    }
    let request = request?;
    let url = CefString::from(&request.url()).to_string();
    let page = browser
        .and_then(ImplBrowser::main_frame)
        .map(|f| CefString::from(&f.url()).to_string())
        .filter(|u| !u.is_empty());
    let source = page
        .or_else(|| initiator.map(CefString::to_string))
        .unwrap_or_default();
    match decide(&url, &source, request.resource_type()) {
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
";

    #[test]
    fn requests_are_blocked_by_the_lists_rules() {
        let filters = Filters::compile([LIST.to_string()]);
        let check = |url: &str, kind: &str| filters.verdict(url, "https://news.example/", kind);
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
        );
        assert_eq!(own, Verdict::Load);
        // Parameters go from what a page fetches (by default not from scripts).
        assert_eq!(
            check(
                "https://cdn.news.example/api?utm_source=x&id=1",
                "xmlhttprequest"
            ),
            Verdict::Rewrite("https://cdn.news.example/api?id=1".into())
        );
    }

    #[test]
    fn only_web_pages_requests_count_and_allowed_sites_load_everything() {
        set_filters(Some(Filters::compile([LIST.to_string()])));
        let ad = "https://ads.example/banner.js";
        assert_eq!(
            decide(ad, "https://www.news.example/a", ResourceType::SCRIPT),
            Verdict::Block
        );
        // The page itself, and pages that are no web pages.
        assert_eq!(
            decide(ad, "https://news.example/", ResourceType::MAIN_FRAME),
            Verdict::Load
        );
        assert_eq!(
            decide(ad, "magnetite://ui/sidebar", ResourceType::SCRIPT),
            Verdict::Load
        );
        assert_eq!(
            decide(
                ad,
                "chrome-extension://abc/popup.html",
                ResourceType::SCRIPT
            ),
            Verdict::Load
        );
        // A site the user allowed, with any of its hosts.
        set_allowed_sites(["news.example".to_string()]);
        assert_eq!(
            decide(ad, "https://www.news.example/a", ResourceType::SCRIPT),
            Verdict::Load
        );
        assert_eq!(
            decide(ad, "https://other.example/", ResourceType::SCRIPT),
            Verdict::Block
        );
        set_filters(None);
        assert_eq!(
            decide(ad, "https://other.example/", ResourceType::SCRIPT),
            Verdict::Load
        );
    }
}
