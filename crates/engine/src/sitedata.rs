//! Site data controls for the site panel: permissions (Chromium content
//! settings in the page's own profile) and clearing a site's cookies and
//! storage. Origins and permission requests are remembered in memory for the
//! session only.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use cef::*;
use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::events::ContentId;
use crate::prompts::PromptKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    Camera,
    Microphone,
    Location,
    Notifications,
    Popups,
    Clipboard,
}

impl PermissionKind {
    const ALL: [Self; 6] = [
        Self::Camera,
        Self::Microphone,
        Self::Location,
        Self::Notifications,
        Self::Popups,
        Self::Clipboard,
    ];

    fn setting(self) -> ContentSettingTypes {
        match self {
            Self::Camera => ContentSettingTypes::MEDIASTREAM_CAMERA,
            Self::Microphone => ContentSettingTypes::MEDIASTREAM_MIC,
            Self::Location => ContentSettingTypes::GEOLOCATION,
            Self::Notifications => ContentSettingTypes::NOTIFICATIONS,
            Self::Popups => ContentSettingTypes::POPUPS,
            Self::Clipboard => ContentSettingTypes::CLIPBOARD_READ_WRITE,
        }
    }

    /// The site panel's row for what a prompt asks, if it has one.
    fn from_prompt_kind(kind: PromptKind) -> Option<Self> {
        match kind {
            PromptKind::Camera => Some(Self::Camera),
            PromptKind::Clipboard => Some(Self::Clipboard),
            PromptKind::Location => Some(Self::Location),
            PromptKind::Microphone => Some(Self::Microphone),
            PromptKind::Notifications => Some(Self::Notifications),
            _ => None,
        }
    }

    /// Kinds in a permission-prompt bitmask (`cef_permission_request_types_t`).
    fn from_prompt_bits(bits: u32) -> Vec<Self> {
        PromptKind::from_bits(bits)
            .into_iter()
            .filter_map(Self::from_prompt_kind)
            .collect()
    }

    /// Kinds in a media-access bitmask (`cef_media_access_permission_types_t`).
    fn from_media_bits(bits: u32) -> Vec<Self> {
        let map = [(1, Self::Microphone), (2, Self::Camera)];
        map.iter()
            .filter(|(b, _)| bits & b != 0)
            .map(|(_, k)| *k)
            .collect()
    }
}

/// What a site may do. `Ask` means Chromium's default (pop-ups: blocked).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionValue {
    Allow,
    Ask,
    Block,
}

#[derive(Debug, Clone, Serialize)]
pub struct PermissionRow {
    pub kind: PermissionKind,
    /// The origin that asked (differs from the page for embedded frames).
    pub origin: String,
    pub value: PermissionValue,
    /// Asked for while this tab was open.
    pub requested: bool,
}

/// Outcome of "Clear cookies and site storage".
#[derive(Debug, Clone, Serialize)]
pub struct ClearReport {
    pub domain: String,
    /// Cookies removed; `None` when the cookie store could not be read.
    pub cookies: Option<usize>,
    pub origins: Vec<String>,
    pub failed_origins: Vec<String>,
    /// Nothing was touched: the page is no longer on the site that was
    /// confirmed.
    pub moved: bool,
}

/// `scheme://host[:port]` of a URL, for http(s) only.
pub fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let host = rest.split(['/', '?', '#']).next()?.rsplit('@').next()?;
    (!host.is_empty()).then(|| format!("{scheme}://{}", host.to_ascii_lowercase()))
}

/// Host of an origin, without the port; an IPv6 address without its brackets.
fn host_of_origin(origin: &str) -> &str {
    let host = origin.split_once("://").map_or(origin, |(_, h)| h);
    match host.strip_prefix('[').and_then(|h| h.split_once(']')) {
        Some((address, _)) => address,
        None => host.split(':').next().unwrap_or(host),
    }
}

/// The registrable domain ("site") of `host`, from the Public Suffix List
/// including private suffixes: `a.b.co.uk` → `b.co.uk`, and the tenants of
/// shared hosts stay apart (`alice.github.io` is its own site). IP addresses
/// and hosts without a known suffix stay as they are.
pub fn registrable_domain(host: &str) -> String {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.parse::<std::net::IpAddr>().is_ok() {
        return host;
    }
    psl::domain_str(&host).map_or_else(|| host.clone(), str::to_string)
}

/// `host` is `domain` or one of its subdomains (a leading dot is ignored).
pub fn in_domain(host: &str, domain: &str) -> bool {
    let host = host.trim_start_matches('.');
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// `host` (of an origin, or a cookie's domain: leading dot, IPv6 in
/// brackets) belongs to the site `domain`, a registrable domain. Unlike
/// `in_domain`, a page on a public suffix (`github.io`) does not take in the
/// suffix's tenants.
fn in_site(host: &str, domain: &str) -> bool {
    let host = host.trim_start_matches('.');
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    registrable_domain(host) == domain
}

impl Engine {
    /// Remember that page `id` loaded a frame from `url` (for clearing data).
    pub(crate) fn note_origin(&self, id: ContentId, url: &str) {
        let Some(origin) = origin_of(url) else { return };
        let mut inner = self.lock();
        let Some(profile) = inner.contents.get(&id).map(|c| c.profile.clone()) else {
            return;
        };
        inner.origins.entry(profile).or_default().insert(origin);
    }

    pub(crate) fn note_permission_prompt(
        &self,
        id: ContentId,
        origin: &str,
        bits: u32,
        media: bool,
    ) {
        let kinds = if media {
            PermissionKind::from_media_bits(bits)
        } else {
            PermissionKind::from_prompt_bits(bits)
        };
        let origin = origin_of(origin).unwrap_or_else(|| origin.trim_end_matches('/').to_string());
        // Its own statement: `page_url` locks the engine too.
        let page = self
            .page_url(id)
            .and_then(|url| origin_of(&url))
            .unwrap_or_default();
        let mut inner = self.lock();
        let set = inner.permission_requests.entry(id).or_default();
        for k in kinds {
            set.insert((page.clone(), origin.clone(), k));
        }
    }

    fn page_url(&self, id: ContentId) -> Option<String> {
        let frame = self
            .lock()
            .contents
            .get(&id)?
            .browser
            .clone()?
            .main_frame()?;
        Some(CefString::from(&frame.url()).to_string())
    }

    /// Permission rows for page `id`: settings that differ from the default,
    /// plus everything asked for since the tab came to this site.
    pub fn site_permissions(&self, id: ContentId) -> Vec<PermissionRow> {
        let (Some(url), Some(ctx)) = (
            self.page_url(id),
            self.content_browser_host(id)
                .and_then(|h| h.request_context()),
        ) else {
            return Vec::new();
        };
        let Some(top) = origin_of(&url) else {
            return Vec::new();
        };
        let requested: BTreeSet<(String, PermissionKind)> = self
            .lock()
            .permission_requests
            .get(&id)
            .into_iter()
            .flatten()
            .filter(|(page, ..)| *page == top)
            .map(|(_, origin, kind)| (origin.clone(), *kind))
            .collect();
        let mut candidates: BTreeSet<(String, PermissionKind)> = PermissionKind::ALL
            .iter()
            .map(|k| (top.clone(), *k))
            .collect();
        candidates.extend(requested.iter().cloned());
        candidates
            .into_iter()
            .filter_map(|(origin, kind)| {
                let raw = ctx.content_setting(
                    Some(&CefString::from(format!("{origin}/").as_str())),
                    Some(&CefString::from(url.as_str())),
                    kind.setting(),
                );
                let value = if raw == ContentSettingValues::ALLOW {
                    PermissionValue::Allow
                } else if raw == ContentSettingValues::BLOCK {
                    PermissionValue::Block
                } else {
                    PermissionValue::Ask
                };
                let was_requested = requested.contains(&(origin.clone(), kind));
                // Chromium reports the effective value; "changed" means it
                // differs from the profile's default for this permission.
                let default = ctx.content_setting(None, None, kind.setting());
                (raw != default || was_requested).then_some(PermissionRow {
                    kind,
                    origin,
                    value,
                    requested: was_requested,
                })
            })
            .collect()
    }

    /// Set what `origin` may do on page `id` (in the page's profile).
    pub fn set_site_permission(
        &self,
        id: ContentId,
        origin: &str,
        kind: PermissionKind,
        value: PermissionValue,
    ) {
        let (Some(url), Some(ctx)) = (
            self.page_url(id),
            self.content_browser_host(id)
                .and_then(|h| h.request_context()),
        ) else {
            return;
        };
        let setting = match value {
            PermissionValue::Allow => ContentSettingValues::ALLOW,
            PermissionValue::Block => ContentSettingValues::BLOCK,
            // Back to Chromium's default: ask (pop-ups: blocked).
            PermissionValue::Ask => ContentSettingValues::DEFAULT,
        };
        ctx.set_content_setting(
            Some(&CefString::from(
                format!("{}/", origin.trim_end_matches('/')).as_str(),
            )),
            Some(&CefString::from(url.as_str())),
            kind.setting(),
            setting,
        );
    }

    /// What clearing page `id`'s site data would touch: the registrable
    /// domain and the origins of it seen in this profile this session.
    pub fn site_data_scope(&self, id: ContentId) -> Option<(String, Vec<String>)> {
        let page_origin = origin_of(&self.page_url(id)?)?;
        let domain = registrable_domain(host_of_origin(&page_origin));
        let mut origins: BTreeSet<String> = {
            let inner = self.lock();
            let profile = inner
                .contents
                .get(&id)
                .map(|c| c.profile.clone())
                .unwrap_or_default();
            inner.origins.get(&profile).cloned().unwrap_or_default()
        };
        origins.retain(|o| in_site(host_of_origin(o), &domain));
        origins.insert(page_origin);
        Some((domain, origins.into_iter().collect()))
    }

    /// Delete cookies of the page's registrable domain (and subdomains) and
    /// the storage of every origin of that domain seen in this profile this
    /// session. `done` runs on the UI thread with what happened.
    ///
    /// `confirmed` is the domain the user was shown and agreed to. A page can
    /// go elsewhere while the question is on screen; then nothing is deleted
    /// and the report says so (`moved`).
    pub fn clear_site_data(
        &self,
        id: ContentId,
        confirmed: &str,
        done: Box<dyn FnOnce(ClearReport)>,
    ) {
        let scope = self.site_data_scope(id).filter(|(d, _)| d == confirmed);
        let Some((domain, origins)) = scope else {
            done(ClearReport {
                domain: confirmed.to_string(),
                cookies: None,
                origins: vec![],
                failed_origins: vec![],
                moved: true,
            });
            return;
        };
        let job = CLEAR_JOBS.with(|j| {
            let mut j = j.borrow_mut();
            let key = j.keys().max().map_or(1, |k| k + 1);
            j.insert(
                key,
                ClearJob {
                    id,
                    domain: domain.clone(),
                    origins,
                    done,
                },
            );
            key
        });
        let manager = self
            .content_browser_host(id)
            .and_then(|h| h.request_context())
            .and_then(|c| c.cookie_manager(None));
        let deleted = Arc::new(AtomicUsize::new(0));
        let guard = Arc::new(CookiesDone {
            job,
            deleted: deleted.clone(),
            ok: AtomicBool::new(true),
        });
        let started = manager.is_some_and(|m| {
            let mut visitor = CookieWiper::new(domain, deleted, guard.clone());
            m.visit_all_cookies(Some(&mut visitor)) != 0
        });
        if !started {
            // Nothing will visit; report the cookie step as failed.
            guard.ok.store(false, Ordering::SeqCst);
        }
        drop(guard);
    }
}

struct ClearJob {
    id: ContentId,
    domain: String,
    origins: Vec<String>,
    done: Box<dyn FnOnce(ClearReport)>,
}

thread_local! {
    static CLEAR_JOBS: RefCell<HashMap<u64, ClearJob>> = RefCell::new(HashMap::new());
}

/// Dropped when CEF has finished visiting cookies (and our last handle is
/// gone): continues with the storage step on the UI thread.
struct CookiesDone {
    job: u64,
    deleted: Arc<AtomicUsize>,
    /// False when the cookie store could not be visited.
    ok: AtomicBool,
}

impl Drop for CookiesDone {
    fn drop(&mut self) {
        let cookies = self
            .ok
            .load(Ordering::SeqCst)
            .then(|| self.deleted.load(Ordering::SeqCst));
        let job = self.job;
        Engine::post(Box::new(move || clear_storage(job, cookies)));
    }
}

wrap_cookie_visitor! {
    struct CookieWiper {
        domain: String,
        deleted: Arc<AtomicUsize>,
        _done: Arc<CookiesDone>,
    }
    impl CookieVisitor {
        fn visit(
            &self,
            cookie: Option<&Cookie>,
            _count: ::std::os::raw::c_int,
            _total: ::std::os::raw::c_int,
            delete_cookie: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            if let (Some(c), Some(del)) = (cookie, delete_cookie)
                && in_site(&c.domain.to_string(), &self.domain)
            {
                *del = 1;
                self.deleted.fetch_add(1, Ordering::SeqCst);
            }
            1
        }
    }
}

/// The storage step of clearing site data, on the UI thread: clear every
/// origin of job `job_id`. `cookies`: how many the cookie step removed
/// (`None` if it failed).
fn clear_storage(job_id: u64, cookies: Option<usize>) {
    let Some(job) = CLEAR_JOBS.with(|j| j.borrow_mut().remove(&job_id)) else {
        return;
    };
    let state = std::rc::Rc::new(RefCell::new((job.origins.len(), Vec::<String>::new())));
    let report = std::rc::Rc::new(RefCell::new(Some((
        job.domain.clone(),
        cookies,
        job.origins.clone(),
        job.done,
    ))));
    for origin in job.origins {
        let state = state.clone();
        let report = report.clone();
        let o = origin.clone();
        Engine::global().devtools(
            job.id,
            "Storage.clearDataForOrigin",
            &serde_json::json!({ "origin": origin, "storageTypes": "all" }),
            Box::new(move |result| {
                let mut s = state.borrow_mut();
                s.0 -= 1;
                if result.is_err() {
                    s.1.push(o);
                }
                if s.0 == 0
                    && let Some((domain, cookies, origins, done)) = report.borrow_mut().take()
                {
                    done(ClearReport {
                        domain,
                        cookies,
                        origins,
                        failed_origins: s.1.clone(),
                        moved: false,
                    });
                }
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrable_domains_and_origins() {
        assert_eq!(registrable_domain("www.shop.example.org"), "example.org");
        assert_eq!(registrable_domain("a.b.example.co.uk"), "example.co.uk");
        assert_eq!(registrable_domain("localhost"), "localhost");
        assert_eq!(registrable_domain("192.168.1.10"), "192.168.1.10");
        // Tenants of shared hosts are separate sites.
        assert_eq!(registrable_domain("alice.github.io"), "alice.github.io");
        assert_ne!(
            registrable_domain("alice.github.io"),
            registrable_domain("bob.github.io")
        );
        assert_eq!(registrable_domain("Docs.Example.COM."), "example.com");
        assert_eq!(
            origin_of("https://User@Docs.Example.org:8443/x?y#z").as_deref(),
            Some("https://docs.example.org:8443")
        );
        assert_eq!(origin_of("magnetite://ui/settings"), None);
        assert!(in_domain(".mail.example.org", "example.org"));
        assert!(!in_domain("badexample.org", "example.org"));
        assert_eq!(
            PermissionKind::from_prompt_bits(4 | 4096),
            vec![PermissionKind::Camera, PermissionKind::Microphone]
        );
    }

    #[test]
    fn clearing_a_site_leaves_other_sites_alone() {
        assert!(in_site("example.org", "example.org"));
        assert!(in_site(".mail.example.org", "example.org"));
        assert!(!in_site("badexample.org", "example.org"));
        // A page on a public suffix itself: its tenants are other sites.
        let suffix = registrable_domain("github.io");
        assert!(in_site("github.io", &suffix));
        assert!(!in_site("alice.github.io", &suffix));
        assert!(!in_site(".alice.github.io", &suffix));
    }

    #[test]
    fn ipv6_hosts_keep_their_address() {
        assert_eq!(host_of_origin("http://[::1]:5173"), "::1");
        assert_eq!(host_of_origin("http://[2001:db8::2]"), "2001:db8::2");
        assert_eq!(host_of_origin("https://example.org:8443"), "example.org");
        assert_eq!(host_of_origin("https://example.org"), "example.org");
        let site = registrable_domain(host_of_origin("http://[::1]:5173"));
        assert_eq!(site, "::1");
        // Cookie domains carry the brackets.
        assert!(in_site("[::1]", &site));
        assert!(!in_site("[::2]", &site));
    }
}
