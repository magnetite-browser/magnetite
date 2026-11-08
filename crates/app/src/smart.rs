//! Smart folders, app side: connected accounts and fetching what a folder's
//! source lists. Requests go straight from this computer to the service with
//! the user's token; no Magnetite server is involved. Tokens live in
//! `smart-accounts.json` (owner-only) and are never synced.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use magnetite_core::smart::{ReviewShow, Service, SmartItem, SmartSource, normalise_host};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A connected account as the UI sees it (never the token).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub service: Service,
    /// GitHub host or YouTrack URL, as entered.
    pub host: String,
    pub login: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    #[serde(flatten)]
    account: Account,
    token: String,
}

/// This device's accounts with their tokens.
pub struct Accounts {
    path: PathBuf,
    stored: Vec<Stored>,
}

impl Accounts {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("smart-accounts.json");
        let stored = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Self { path, stored }
    }

    pub fn list(&self) -> Vec<Account> {
        self.stored.iter().map(|s| s.account.clone()).collect()
    }

    /// The account (and token) to read `source` with: the one named `login`,
    /// else the first for the source's service and host.
    pub fn for_source(
        &self,
        source: &SmartSource,
        login: Option<&str>,
    ) -> Option<(Account, String)> {
        let host = normalise_host(source.host());
        let mut matching = self.stored.iter().filter(|s| {
            s.account.service == source.service() && normalise_host(&s.account.host) == host
        });
        let pick = match login {
            Some(l) => matching.find(|s| s.account.login == l),
            None => matching.next(),
        };
        pick.map(|s| (s.account.clone(), s.token.clone()))
    }

    /// Store an account, replacing the same login on the same server.
    pub fn add(&mut self, account: Account, token: String) -> std::io::Result<()> {
        let host = normalise_host(&account.host);
        self.stored.retain(|s| {
            !(s.account.service == account.service
                && normalise_host(&s.account.host) == host
                && s.account.login == account.login)
        });
        self.stored.push(Stored { account, token });
        self.save()
    }

    pub fn remove(&mut self, id: &str) -> std::io::Result<()> {
        self.stored.retain(|s| s.account.id != id);
        self.save()
    }

    fn save(&self) -> std::io::Result<()> {
        crate::files::write_private(&self.path, &serde_json::to_vec_pretty(&self.stored)?)
    }
}

/// Ask the service who `token` belongs to: `(login, display name)`.
pub fn whoami(service: Service, host: &str, token: &str) -> Result<(String, String), String> {
    let (url, logins, name): (String, &[&str], &str) = match service {
        Service::GitHub => (format!("{}/user", github_api(host)?), &["login"], "name"),
        Service::GitLab => (
            format!("{}/api/v4/user", base_url(host)?),
            &["username"],
            "name",
        ),
        // Data Center names users; Cloud has an email (if visible) and an id.
        Service::Jira => (
            format!("{}/rest/api/2/myself", base_url(host)?),
            &["name", "emailAddress", "accountId"],
            "displayName",
        ),
        Service::YouTrack => (
            format!("{}/api/users/me?fields=login,fullName", base_url(host)?),
            &["login"],
            "fullName",
        ),
    };
    let me = get_json(&url, &authorization(service, token))?;
    let text = |k: &str| {
        me.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let login = logins.iter().map(|k| text(k)).find(|l| !l.is_empty());
    let login = login.ok_or("The server did not say who the token belongs to.")?;
    Ok((login, text(name)))
}

/// What `source` lists now, best first, at most `limit` items. `login` is
/// the account's user name (GitLab review requests are asked by name).
pub fn fetch(
    source: &SmartSource,
    token: &str,
    login: &str,
    limit: u32,
) -> Result<Vec<SmartItem>, String> {
    let n = limit.clamp(1, 100);
    let auth = authorization(source.service(), token);
    let items = match source {
        SmartSource::GitHub { host, show, filter } => {
            let api = github_api(host)?;
            let mut found = Vec::new();
            for q in github_queries(*show, filter) {
                let url = format!(
                    "{api}/search/issues?q={}&sort=updated&order=desc&per_page={n}",
                    encode(&q)
                );
                found.extend(parse_github(&get_json(&url, &auth)?));
            }
            newest_first(found)
        }
        SmartSource::GitLab { host, show, group } => {
            let mut found = Vec::new();
            for url in gitlab_requests(&base_url(host)?, *show, group, login, n) {
                found.extend(parse_gitlab(&get_json(&url, &auth)?));
            }
            newest_first(found)
        }
        SmartSource::Jira { url, jql } => {
            let base = base_url(url)?;
            // Jira Cloud retired /search for /search/jql; Data Center keeps it.
            let path = if normalise_host(url).ends_with(".atlassian.net") {
                "rest/api/3/search/jql"
            } else {
                "rest/api/2/search"
            };
            let request = format!(
                "{base}/{path}?jql={}&fields=summary&maxResults={n}",
                encode(jql)
            );
            parse_jira(&get_json(&request, &auth)?, &base)
        }
        SmartSource::YouTrack { url, query } => {
            let base = base_url(url)?;
            let request = format!(
                "{base}/api/issues?query={}&fields=idReadable,summary&$top={n}",
                encode(query)
            );
            parse_youtrack(&get_json(&request, &auth)?, &base)
        }
    };
    Ok(items
        .into_iter()
        .take(limit as usize)
        .map(|mut item| {
            cut_title(&mut item.title);
            item
        })
        .collect())
}

/// Longest title kept for an item, in characters: the sidebar shows a line
/// of it, and what a server sends is saved and synced.
const MAX_TITLE: usize = 300;

fn cut_title(title: &mut String) {
    if let Some((at, _)) = title.char_indices().nth(MAX_TITLE) {
        title.truncate(at);
        title.push('…');
    }
}

/// Results of several searches (mine + review requests) as one list: newest
/// activity first, each item once.
fn newest_first(mut found: Vec<(String, SmartItem)>) -> Vec<SmartItem> {
    found.sort_by(|a, b| b.0.cmp(&a.0));
    let mut items: Vec<SmartItem> = Vec::new();
    for (_, item) in found {
        if !items.iter().any(|i| i.url == item.url) {
            items.push(item);
        }
    }
    items
}

/// The `Authorization` header for `token`. Jira Cloud wants Basic auth with
/// `email:api-token`; everything else (Jira Data Center too) takes a Bearer token.
fn authorization(service: Service, token: &str) -> String {
    use base64::Engine as _;
    if service == Service::Jira && token.contains(':') {
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(token)
        )
    } else {
        format!("Bearer {token}")
    }
}

/// GitLab API requests for `show`: merge requests (or issues) of the whole
/// server, or of one group.
fn gitlab_requests(base: &str, show: ReviewShow, group: &str, login: &str, n: u32) -> Vec<String> {
    let root = match group.trim() {
        "" => format!("{base}/api/v4"),
        g => format!("{base}/api/v4/groups/{}", encode(g)),
    };
    let order = format!("state=opened&order_by=updated_at&sort=desc&per_page={n}");
    let mine = format!("{root}/merge_requests?scope=created_by_me&{order}");
    let review = format!(
        "{root}/merge_requests?scope=all&reviewer_username={}&{order}",
        encode(login)
    );
    match show {
        ReviewShow::ReviewRequests => vec![review],
        ReviewShow::MyPullRequests => vec![mine],
        ReviewShow::PullRequests => vec![mine, review],
        ReviewShow::AssignedIssues => vec![format!("{root}/issues?scope=assigned_to_me&{order}")],
    }
}

/// GitHub search queries for `show`, plus the user's own qualifiers.
fn github_queries(show: ReviewShow, filter: &str) -> Vec<String> {
    let kinds: &[&str] = match show {
        ReviewShow::ReviewRequests => &["is:pr review-requested:@me"],
        ReviewShow::MyPullRequests => &["is:pr author:@me"],
        ReviewShow::PullRequests => &["is:pr author:@me", "is:pr review-requested:@me"],
        ReviewShow::AssignedIssues => &["is:issue assignee:@me"],
    };
    kinds
        .iter()
        .map(|k| {
            format!("is:open archived:false {k} {}", filter.trim())
                .trim()
                .to_string()
        })
        .collect()
}

/// GitLab merge requests or issues as `(updated_at, item)`; titles read
/// "Title · project!12" (merge request) or "Title · project#12" (issue).
fn parse_gitlab(json: &Value) -> Vec<(String, SmartItem)> {
    let list = json.as_array().cloned().unwrap_or_default();
    list.iter()
        .filter_map(|i| {
            let url = i.get("web_url")?.as_str()?.to_string();
            let title = i.get("title")?.as_str()?.trim();
            let reference = i
                .pointer("/references/full")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let short = reference.rsplit('/').next().unwrap_or(reference);
            let updated = i
                .get("updated_at")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Some((
                updated,
                SmartItem {
                    url,
                    title: format!("{title} · {short}"),
                },
            ))
        })
        .collect()
}

/// Jira issues; titles read "ABC-12 Summary".
fn parse_jira(json: &Value, base: &str) -> Vec<SmartItem> {
    let issues = json
        .get("issues")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    issues
        .iter()
        .filter_map(|i| {
            let key = i.get("key")?.as_str()?;
            let summary = i
                .pointer("/fields/summary")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            Some(SmartItem {
                url: format!("{base}/browse/{key}"),
                title: format!("{key} {summary}"),
            })
        })
        .collect()
}

/// Search results as `(updated_at, item)`; titles read "Title · repo#12".
fn parse_github(json: &Value) -> Vec<(String, SmartItem)> {
    let items = json
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    items
        .iter()
        .filter_map(|i| {
            let url = i.get("html_url")?.as_str()?.to_string();
            let title = i.get("title")?.as_str()?.trim();
            let number = i.get("number").and_then(Value::as_u64).unwrap_or_default();
            let repo = i
                .get("repository_url")
                .and_then(Value::as_str)
                .and_then(|r| r.rsplit('/').next())
                .unwrap_or_default();
            let updated = i
                .get("updated_at")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Some((
                updated,
                SmartItem {
                    url,
                    title: format!("{title} · {repo}#{number}"),
                },
            ))
        })
        .collect()
}

/// YouTrack issues; titles read "ABC-12 Summary".
fn parse_youtrack(json: &Value, base: &str) -> Vec<SmartItem> {
    json.as_array()
        .map(|issues| {
            issues
                .iter()
                .filter_map(|i| {
                    let id = i.get("idReadable")?.as_str()?;
                    let summary = i
                        .get("summary")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .trim();
                    Some(SmartItem {
                        url: format!("{base}/issue/{id}"),
                        title: format!("{id} {summary}"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `https://` unless a scheme is given; plain `http` only for this computer
/// (`localhost`, `127.0.0.1`, `::1`, checked on the parsed host, so
/// `localhost.example.org` is not "this computer"). Addresses with a user
/// name or password in them are refused: tokens go in their own field.
pub fn base_url(host: &str) -> Result<String, String> {
    let h = host.trim().trim_end_matches('/');
    if h.is_empty() {
        return Err("Enter the server address.".into());
    }
    let raw = if h.contains("://") {
        h.to_string()
    } else {
        format!("https://{h}")
    };
    let parsed = url::Url::parse(&raw).map_err(|_| "That is not a server address.".to_string())?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("Leave the user name out of the address.".into());
    }
    let this_computer = match parsed.host() {
        Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    match parsed.scheme() {
        "https" => Ok(raw),
        "http" if this_computer => Ok(raw),
        _ => Err("Use an https:// address.".into()),
    }
}

/// REST base of GitHub at `host`: api.github.com, or Enterprise's /api/v3.
fn github_api(host: &str) -> Result<String, String> {
    if normalise_host(host) == "github.com" {
        Ok("https://api.github.com".into())
    } else {
        Ok(format!("{}/api/v3", base_url(host)?))
    }
}

/// Whether `url` lives on the source's server (for adopting a folder's tabs).
pub fn on_source_host(url: &str, source: &SmartSource) -> bool {
    let host = normalise_host(source.host());
    let page = normalise_host(url);
    page == host || page.starts_with(&format!("{host}/"))
}

/// Largest answer read from a service: far more than a page of items.
const MAX_ANSWER: u64 = 10 * 1024 * 1024;

/// GET `url` as JSON with `token`, turning failures into sentences.
fn get_json(url: &str, authorization: &str) -> Result<Value, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(20)))
        .user_agent("Magnetite")
        .build()
        .into();
    let mut res = agent
        .get(url)
        .header("Authorization", authorization)
        .header("Accept", "application/json")
        .call()
        .map_err(|e| {
            format!(
                "Cannot reach {}: {e}",
                normalise_host(url).split('/').next().unwrap_or_default()
            )
        })?;
    match res.status().as_u16() {
        200..=299 => {
            // Capped as unpacked: the limit of the HTTP client counts the
            // compressed bytes, and a small answer can unpack to gigabytes.
            let mut body = Vec::new();
            res.body_mut()
                .with_config()
                .limit(MAX_ANSWER)
                .reader()
                .take(MAX_ANSWER + 1)
                .read_to_end(&mut body)
                .map_err(|e| format!("Unexpected answer: {e}"))?;
            if body.len() as u64 > MAX_ANSWER {
                return Err("The server sent more than a list of items can be.".into());
            }
            serde_json::from_slice(&body).map_err(|e| format!("Unexpected answer: {e}"))
        }
        401 => Err("The token was rejected. Create a new one and connect again.".into()),
        403 | 429 => Err("Access denied or too many requests. Try again later, or check the token's permissions.".into()),
        404 => Err("Not found. Check the server address.".into()),
        code => Err(format!("The server answered {code}.")),
    }
}

/// Percent-encode a query value.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};

    #[test]
    fn github_results_and_queries() {
        let json = serde_json::json!({ "items": [{
            "html_url": "https://github.com/acme/app/pull/7", "title": " Fix login ", "number": 7,
            "repository_url": "https://api.github.com/repos/acme/app", "updated_at": "2026-11-01T10:00:00Z"
        }]});
        let items = parse_github(&json);
        assert_eq!(items[0].1.title, "Fix login · app#7");
        assert_eq!(
            github_queries(ReviewShow::PullRequests, " org:acme "),
            vec![
                "is:open archived:false is:pr author:@me org:acme",
                "is:open archived:false is:pr review-requested:@me org:acme"
            ]
        );
    }

    #[test]
    fn youtrack_results_and_addresses() {
        let json = serde_json::json!([{ "idReadable": "FRG-12", "summary": "Crash on start" }]);
        let items = parse_youtrack(&json, "https://acme.youtrack.cloud");
        assert_eq!(items[0].url, "https://acme.youtrack.cloud/issue/FRG-12");
        assert_eq!(items[0].title, "FRG-12 Crash on start");
        assert_eq!(
            base_url("acme.youtrack.cloud/").unwrap(),
            "https://acme.youtrack.cloud"
        );
        assert!(
            base_url("http://tracker.example.com").is_err(),
            "no plain http to other machines"
        );
        assert!(base_url("http://127.0.0.1:8080").is_ok());
        // "This computer" is the parsed host, not a prefix of the address.
        assert!(base_url("http://localhost.attacker.example").is_err());
        assert!(base_url("http://127.0.0.1.attacker.example").is_err());
        assert!(base_url("http://[::1]:8080").is_ok());
        assert!(base_url("https://me:secret@git.example.org").is_err());
        assert_eq!(
            github_api("https://GitHub.com").unwrap(),
            "https://api.github.com"
        );
        assert_eq!(
            github_api("git.acme.dev").unwrap(),
            "https://git.acme.dev/api/v3"
        );
    }

    #[test]
    fn a_title_longer_than_a_line_is_cut_at_a_character() {
        let mut short = "Fix login".to_string();
        cut_title(&mut short);
        assert_eq!(short, "Fix login");
        let mut long = "ä".repeat(MAX_TITLE + 50);
        cut_title(&mut long);
        assert_eq!(long.chars().count(), MAX_TITLE + 1);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn gitlab_requests_results_and_titles() {
        let urls = gitlab_requests(
            "https://gitlab.acme.eu",
            ReviewShow::PullRequests,
            "acme/platform",
            "alex",
            20,
        );
        assert!(urls[0].starts_with("https://gitlab.acme.eu/api/v4/groups/acme%2Fplatform/merge_requests?scope=created_by_me&state=opened"));
        assert!(urls[1].contains("scope=all&reviewer_username=alex&"));
        let issues = gitlab_requests(
            "https://gitlab.com",
            ReviewShow::AssignedIssues,
            "",
            "alex",
            5,
        );
        assert_eq!(
            issues,
            vec![
                "https://gitlab.com/api/v4/issues?scope=assigned_to_me&state=opened&order_by=updated_at&sort=desc&per_page=5"
            ]
        );
        let json = serde_json::json!([{ "web_url": "https://gitlab.com/acme/app/-/merge_requests/4", "title": "Add EU region", "references": { "full": "acme/app!4" }, "updated_at": "2026-11-01" }]);
        assert_eq!(parse_gitlab(&json)[0].1.title, "Add EU region · app!4");
    }

    #[test]
    fn jira_results_and_cloud_auth() {
        let json = serde_json::json!({ "issues": [{ "key": "OPS-7", "fields": { "summary": "Rotate certificates" } }] });
        let items = parse_jira(&json, "https://acme.atlassian.net");
        assert_eq!(items[0].url, "https://acme.atlassian.net/browse/OPS-7");
        assert_eq!(items[0].title, "OPS-7 Rotate certificates");
        // Cloud: email:api-token as Basic; Data Center personal tokens as Bearer.
        assert_eq!(
            authorization(Service::Jira, "me@acme.eu:tok"),
            "Basic bWVAYWNtZS5ldTp0b2s="
        );
        assert_eq!(authorization(Service::Jira, "pat"), "Bearer pat");
        assert_eq!(authorization(Service::GitLab, "a:b"), "Bearer a:b");
    }

    #[test]
    fn a_folder_turned_smart_adopts_tabs_on_the_source_server() {
        let gh = SmartSource::GitHub {
            host: "github.com".into(),
            show: ReviewShow::PullRequests,
            filter: String::new(),
        };
        assert!(on_source_host("https://github.com/acme/app/pull/1", &gh));
        assert!(on_source_host(
            "https://www.github.com/acme/app/pull/1",
            &gh
        ));
        assert!(!on_source_host("https://gitlab.com/acme/app", &gh));
        let yt = SmartSource::YouTrack {
            url: "https://acme.example/youtrack".into(),
            query: String::new(),
        };
        assert!(on_source_host(
            "https://acme.example/youtrack/issue/FRG-1",
            &yt
        ));
        assert!(!on_source_host("https://acme.example/wiki", &yt));
    }

    #[test]
    fn tokens_stay_owner_only_and_accounts_match_by_server() {
        let dir = crate::files::test_dir("accounts");
        std::fs::create_dir_all(&dir).unwrap();
        let mut a = Accounts::load(&dir);
        let gh = |login: &str| Account {
            id: login.into(),
            service: Service::GitHub,
            host: "github.com".into(),
            login: login.into(),
            name: String::new(),
        };
        a.add(gh("work"), "t1".into()).unwrap();
        a.add(gh("home"), "t2".into()).unwrap();
        let source = SmartSource::GitHub {
            host: "https://www.github.com/".into(),
            show: ReviewShow::ReviewRequests,
            filter: String::new(),
        };
        assert_eq!(a.for_source(&source, None).unwrap().1, "t1");
        assert_eq!(a.for_source(&source, Some("home")).unwrap().1, "t2");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join("smart-accounts.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        a.remove("work").unwrap();
        assert_eq!(Accounts::load(&dir).list(), vec![gh("home")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// GitHub Enterprise at a local address: both searches run with the token
    /// and their results merge, newest first, without duplicates.
    #[test]
    fn fetch_runs_both_searches_against_a_server() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for stream in listener.incoming().take(2) {
                let mut stream = stream.unwrap();
                let mut head = Vec::new();
                for line in BufReader::new(&stream).lines() {
                    let line = line.unwrap();
                    if line.is_empty() {
                        break;
                    }
                    head.push(line);
                }
                let mine = head[0].contains("author%3A%40me");
                assert!(
                    head.iter()
                        .any(|h| h.eq_ignore_ascii_case("authorization: Bearer tok"))
                );
                let body = if mine {
                    r#"{"items":[{"html_url":"https://git/a/x/pull/1","title":"Mine","number":1,"repository_url":"r/x","updated_at":"2026-01-01"}]}"#
                } else {
                    r#"{"items":[{"html_url":"https://git/a/x/pull/2","title":"Review","number":2,"repository_url":"r/x","updated_at":"2026-02-01"},{"html_url":"https://git/a/x/pull/1","title":"Mine","number":1,"repository_url":"r/x","updated_at":"2026-01-01"}]}"#
                };
                write!(stream, "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).unwrap();
                seen.push(head[0].clone());
            }
            seen
        });
        let source = SmartSource::GitHub {
            host: format!("http://127.0.0.1:{port}"),
            show: ReviewShow::PullRequests,
            filter: String::new(),
        };
        let items = fetch(&source, "tok", "me", 10).unwrap();
        let titles: Vec<_> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, vec!["Review · x#2", "Mine · x#1"]);
        assert!(
            server
                .join()
                .unwrap()
                .iter()
                .all(|l| l.starts_with("GET /api/v3/search/issues?q="))
        );
    }
}
