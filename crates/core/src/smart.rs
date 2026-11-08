//! Smart folders: folders that fill themselves from a source, such as open
//! pull requests on GitHub or tickets in YouTrack. Fetching lives in the app;
//! this module holds the model and how fetched items update a folder.

use serde::{Deserialize, Serialize};

use crate::model::{Folder, Id, SidebarNode, TabItem, UnixSeconds};

/// What makes a folder smart. Its items are pinned-style tabs: a refresh adds
/// what the source lists and drops what it no longer lists. Tabs the user
/// added by hand stay untouched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartFolder {
    pub source: SmartSource,
    /// Login of the account to ask with (a service can have several);
    /// `None` uses the first account connected for the service and host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// Most items shown.
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Minutes between refreshes.
    #[serde(default = "default_refresh_minutes")]
    pub refresh_minutes: u32,
    /// URLs the source listed on the last refresh: the folder's own items.
    #[serde(default)]
    pub items: Vec<String>,
    /// URLs the user hid; not added again while the source lists them.
    #[serde(default)]
    pub hidden: Vec<String>,
    /// The source was asked at least once. Items that come later are new
    /// (shown as unread), also in a folder that was empty until then.
    #[serde(default)]
    pub filled: bool,
}

fn default_limit() -> u32 {
    20
}

fn default_refresh_minutes() -> u32 {
    15
}

impl SmartFolder {
    pub fn new(source: SmartSource) -> Self {
        Self {
            source,
            account: None,
            limit: default_limit(),
            refresh_minutes: default_refresh_minutes(),
            items: Vec::new(),
            hidden: Vec::new(),
            filled: false,
        }
    }
}

/// Where a smart folder's items come from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "service", rename_all = "snake_case")]
pub enum SmartSource {
    /// Pull requests or issues on GitHub (`github.com`) or a GitHub
    /// Enterprise server at `host`.
    #[serde(rename = "github")]
    GitHub {
        host: String,
        show: ReviewShow,
        /// Extra search qualifiers, e.g. `org:acme -label:wip`.
        #[serde(default)]
        filter: String,
    },
    /// Merge requests or issues on GitLab (`gitlab.com` or self-hosted at
    /// `host`), optionally only within one group (`acme/platform`).
    #[serde(rename = "gitlab")]
    GitLab {
        host: String,
        show: ReviewShow,
        #[serde(default)]
        group: String,
    },
    /// Issues on Jira (Cloud or Data Center at `url`) matching a JQL query,
    /// e.g. `assignee = currentUser() AND resolution = Unresolved`.
    #[serde(rename = "jira")]
    Jira { url: String, jql: String },
    /// Issues on the YouTrack instance at `url` matching a YouTrack query,
    /// e.g. `for: me #Unresolved`.
    #[serde(rename = "youtrack")]
    YouTrack { url: String, query: String },
}

/// The services smart folders can read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Service {
    #[serde(rename = "github")]
    GitHub,
    #[serde(rename = "gitlab")]
    GitLab,
    #[serde(rename = "jira")]
    Jira,
    #[serde(rename = "youtrack")]
    YouTrack,
}

impl SmartSource {
    pub fn service(&self) -> Service {
        match self {
            Self::GitHub { .. } => Service::GitHub,
            Self::GitLab { .. } => Service::GitLab,
            Self::Jira { .. } => Service::Jira,
            Self::YouTrack { .. } => Service::YouTrack,
        }
    }

    /// Server the source lives on: its host or URL.
    pub fn host(&self) -> &str {
        match self {
            Self::GitHub { host, .. } | Self::GitLab { host, .. } => host,
            Self::Jira { url, .. } | Self::YouTrack { url, .. } => url,
        }
    }
}

/// Which items a GitHub or GitLab folder shows ("pull requests" are merge
/// requests on GitLab).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewShow {
    /// Open pull requests waiting for my review.
    ReviewRequests,
    /// Open pull requests I opened.
    MyPullRequests,
    /// Both of the above.
    PullRequests,
    /// Open issues assigned to me.
    AssignedIssues,
}

/// One entry a source lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartItem {
    pub url: String,
    pub title: String,
}

/// The URL that identifies a tab as a source item: where it was opened from.
pub fn item_key(tab: &TabItem) -> &str {
    tab.home_url.as_deref().unwrap_or(&tab.url)
}

/// `host` without scheme, `www.` and trailing slashes, lowercase; how
/// accounts and sources are matched (`https://GitHub.com/` = `github.com`).
pub fn normalise_host(host: &str) -> String {
    let h = host.trim().trim_end_matches('/');
    let h = h.split_once("://").map_or(h, |(_, rest)| rest);
    h.strip_prefix("www.").unwrap_or(h).to_ascii_lowercase()
}

impl Folder {
    /// Bring a smart folder up to date with `items`, what its source lists
    /// now (best first). New items become tabs, unread unless this is the
    /// folder's first fill; items the source dropped go away unless `keep`
    /// says otherwise (the tab is open); hidden items stay away. Tabs the
    /// user added by hand keep their place, ahead of the source's items.
    /// Returns whether anything changed.
    pub fn apply_smart(
        &mut self,
        items: &[SmartItem],
        keep: impl Fn(&TabItem) -> bool,
        mut new_id: impl FnMut() -> Id,
        now: UnixSeconds,
    ) -> bool {
        let Some(smart) = self.smart.as_mut() else {
            return false;
        };
        // The first answer fills the folder; nothing in it is news yet.
        let first_fill = !smart.filled && smart.items.is_empty();
        smart.filled = true;
        let listed: Vec<&SmartItem> = items
            .iter()
            .filter(|i| !smart.hidden.contains(&i.url))
            .take(smart.limit as usize)
            .collect();
        let before = self.children.clone();

        let mut manual = Vec::new();
        let mut own: Vec<TabItem> = Vec::new();
        for node in std::mem::take(&mut self.children) {
            match node {
                SidebarNode::Tab(t) if smart.items.iter().any(|k| k == item_key(&t)) => own.push(t),
                SidebarNode::Tab(t) if listed.iter().any(|i| i.url == item_key(&t)) => own.push(t),
                other => manual.push(other),
            }
        }
        let mut children = manual;
        for item in &listed {
            let tab = match own.iter().position(|t| item_key(t) == item.url) {
                Some(i) => {
                    let mut t = own.remove(i);
                    if !t.custom_title {
                        t.title.clone_from(&item.title);
                    }
                    t
                }
                None => TabItem {
                    home_url: Some(item.url.clone()),
                    created_at: Some(now),
                    unread: !first_fill,
                    ..TabItem::new(new_id(), item.url.clone(), item.title.clone())
                },
            };
            children.push(SidebarNode::Tab(tab));
        }
        // Gone from the source but open right now: stays (still the
        // folder's own item) until a refresh finds it closed.
        let kept: Vec<TabItem> = own.into_iter().filter(|t| keep(t)).collect();
        smart.items = listed
            .iter()
            .map(|i| i.url.clone())
            .chain(kept.iter().map(|t| item_key(t).to_string()))
            .collect();
        children.extend(kept.into_iter().map(SidebarNode::Tab));
        // Forget hidden items once the source stops listing them.
        smart.hidden.retain(|h| items.iter().any(|i| &i.url == h));
        self.children = children;
        self.children != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(n: u32) -> SmartItem {
        SmartItem {
            url: format!("https://github.com/acme/app/pull/{n}"),
            title: format!("PR {n}"),
        }
    }

    fn folder() -> Folder {
        Folder {
            id: "f".into(),
            title: "Pull requests".into(),
            icon: None,
            children: vec![SidebarNode::Tab(TabItem {
                home_url: Some("https://example.org/notes".into()),
                ..TabItem::new("manual", "https://example.org/notes", "Notes")
            })],
            smart: Some(SmartFolder::new(SmartSource::GitHub {
                host: "github.com".into(),
                show: ReviewShow::PullRequests,
                filter: String::new(),
            })),
        }
    }

    fn tabs(f: &Folder) -> Vec<(String, bool)> {
        f.children
            .iter()
            .filter_map(|n| match n {
                SidebarNode::Tab(t) => Some((t.title.clone(), t.unread)),
                SidebarNode::Folder(_) => None,
            })
            .collect()
    }

    #[test]
    fn refresh_adds_new_items_drops_gone_ones_and_keeps_manual_tabs() {
        let mut f = folder();
        let mut n = 0;
        let mut ids = || {
            n += 1;
            format!("t{n}")
        };
        assert!(f.apply_smart(&[item(1), item(2)], |_| false, &mut ids, 0));
        // First fill: nothing is "new" yet; hand-added tabs come first.
        assert_eq!(
            tabs(&f),
            vec![
                ("Notes".into(), false),
                ("PR 1".into(), false),
                ("PR 2".into(), false)
            ]
        );
        // PR 1 merged, PR 3 opened: 3 arrives unread, 2 keeps its tab id.
        assert!(f.apply_smart(&[item(3), item(2)], |_| false, &mut ids, 0));
        assert_eq!(
            tabs(&f),
            vec![
                ("Notes".into(), false),
                ("PR 3".into(), true),
                ("PR 2".into(), false)
            ]
        );
        assert!(matches!(&f.children[2], SidebarNode::Tab(t) if t.id == "t2"));
        assert!(
            !f.apply_smart(&[item(3), item(2)], |_| false, &mut ids, 0),
            "no change"
        );
    }

    #[test]
    fn open_tabs_survive_and_hidden_items_stay_away_until_the_source_drops_them() {
        let mut f = folder();
        let mut ids = {
            let mut n = 0;
            move || {
                n += 1;
                format!("t{n}")
            }
        };
        f.apply_smart(&[item(1), item(2)], |_| false, &mut ids, 0);
        f.smart.as_mut().unwrap().hidden.push(item(2).url);
        // PR 1 is gone from the source but open: it stays for now.
        f.apply_smart(&[item(2)], |t| t.title == "PR 1", &mut ids, 0);
        assert_eq!(
            tabs(&f),
            vec![("Notes".into(), false), ("PR 1".into(), false)]
        );
        // Once closed, the next refresh drops it; the hidden mark ends with PR 2.
        f.apply_smart(&[], |_| false, &mut ids, 0);
        assert_eq!(tabs(&f), vec![("Notes".into(), false)]);
        assert_eq!(f.smart.as_ref().unwrap().hidden, vec![] as Vec<String>);
    }

    #[test]
    fn limit_caps_items_and_hosts_normalise() {
        let mut f = folder();
        f.smart.as_mut().unwrap().limit = 1;
        f.apply_smart(&[item(1), item(2)], |_| false, || "t".into(), 0);
        assert_eq!(tabs(&f).len(), 2, "Notes plus one item");
        assert_eq!(normalise_host("https://www.GitHub.com/"), "github.com");
        assert_eq!(normalise_host("acme.youtrack.cloud"), "acme.youtrack.cloud");
    }
}
