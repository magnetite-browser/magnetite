//! The ad blocker's filter lists (Settings → Ad blocking): the lists one can
//! subscribe to, their download straight from the publisher, and when to
//! fetch them again. Blocking with them is `magnetite_engine::Filters`.
//!
//! The lists are downloaded as they are and compiled on this computer;
//! nothing is sent but the download request itself.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// A filter list one can subscribe to.
pub struct FilterList {
    pub id: &'static str,
    pub name: &'static str,
    /// What it blocks, in a few words.
    pub about: &'static str,
    /// Where it is downloaded from, over https.
    pub url: &'static str,
}

/// The lists offered. Only lists in one file: lists that include others
/// (`!#include`, uBlock Origin's) would need those fetched as well.
pub const LISTS: &[FilterList] = &[
    FilterList {
        id: "easylist",
        name: "EasyList",
        about: "Ads on most international websites.",
        url: "https://easylist.to/easylist/easylist.txt",
    },
    FilterList {
        id: "easyprivacy",
        name: "EasyPrivacy",
        about: "Trackers and analytics.",
        url: "https://easylist.to/easylist/easyprivacy.txt",
    },
    FilterList {
        id: "easylist-germany",
        name: "EasyList Germany",
        about: "Ads on German-language websites. Use with EasyList.",
        url: "https://easylist.to/easylistgermany/easylistgermany.txt",
    },
    FilterList {
        id: "peter-lowe",
        name: "Peter Lowe's list",
        about: "Servers of ad and tracking companies.",
        url: "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=adblockplus&showintro=0&mimetype=plaintext",
    },
];

/// A list is fetched again after this long: what `EasyList` asks for.
pub const REFRESH_AFTER: Duration = Duration::from_secs(4 * 24 * 3600);

/// The largest list taken (`EasyList` is about 2 MB).
const MAX_LIST: u64 = 16 << 20;

/// The list with id `id`, if Magnetite offers it.
pub fn list(id: &str) -> Option<&'static FilterList> {
    LISTS.iter().find(|l| l.id == id)
}

/// Where the downloaded lists are kept.
pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("filter-lists")
}

fn path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.txt"))
}

/// When list `id` was last downloaded, if it is here.
pub fn fetched_at(dir: &Path, id: &str) -> Option<SystemTime> {
    std::fs::metadata(path(dir, id))
        .and_then(|m| m.modified())
        .ok()
}

/// Whether list `id` is missing or older than `REFRESH_AFTER` at `now`.
pub fn due(dir: &Path, id: &str, now: SystemTime) -> bool {
    // A list from the future (the clock was changed) is no older than now.
    fetched_at(dir, id).is_none_or(|at| now.duration_since(at).unwrap_or_default() >= REFRESH_AFTER)
}

/// Download `list` from its publisher and keep it (blocking). The copy
/// from before stays when this fails.
pub fn fetch(dir: &Path, list: &FilterList) -> Result<(), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(5)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(Duration::from_secs(120)))
        .user_agent("Magnetite")
        .build()
        .into();
    let mut res = agent
        .get(list.url)
        .call()
        .map_err(|e| format!("Cannot download it: {e}"))?;
    let html = (res.headers().get("content-type"))
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t.trim_start().to_ascii_lowercase().starts_with("text/html"));
    let mut body = Vec::new();
    res.body_mut()
        .as_reader()
        .take(MAX_LIST + 1)
        .read_to_end(&mut body)
        .map_err(|e| format!("The download broke off: {e}"))?;
    check(&body, html)?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // Written whole: a reader never sees half a list.
    crate::files::write_private(&path(dir, list.id), &body).map_err(|e| e.to_string())
}

/// Whether a download looks like a filter list: text, not too large, and
/// not a web page (`html`: served as one), such as the sign-in page of a
/// hotel's Wi-Fi. No list starts with `<`; every page does, after a byte
/// order mark and white space.
fn check(body: &[u8], html: bool) -> Result<(), String> {
    if body.len() as u64 > MAX_LIST {
        return Err("The list is larger than expected.".into());
    }
    let text = std::str::from_utf8(body).map_err(|_| "The download is no filter list.")?;
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if html || text.starts_with('<') || text.is_empty() {
        return Err("The download is no filter list.".into());
    }
    Ok(())
}

/// The text of the lists `ids` that are here.
pub fn load(dir: &Path, ids: &[String]) -> Vec<String> {
    (ids.iter())
        .filter_map(|id| std::fs::read_to_string(path(dir, id)).ok())
        .collect()
}

/// Delete the downloaded lists not in `keep`: unsubscribed ones.
pub fn remove_others(dir: &Path, keep: &[String]) {
    for list in LISTS.iter().filter(|l| !keep.iter().any(|k| k == l.id)) {
        let _ = std::fs::remove_file(path(dir, list.id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_text_that_is_no_web_page_counts_as_a_list() {
        let list = b"[Adblock Plus 2.0]\n||ads.example^\n";
        assert!(check(list, false).is_ok());
        assert!(check(list, true).is_err(), "served as a web page");
        assert!(check(b"  <!DOCTYPE html><html>Sign in to the Wi-Fi</html>", false).is_err());
        assert!(check(b"\n<!-- portal --><html>Sign in</html>", false).is_err());
        assert!(check("\u{feff}<html><body>".as_bytes(), false).is_err());
        assert!(check(b"\n \n", false).is_err());
        assert!(check(&[0xff, 0xfe, 0x00], false).is_err());
    }

    #[test]
    fn a_list_is_due_when_missing_or_old() {
        let dir = std::env::temp_dir().join(format!("magnetite-lists-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(due(&dir, "easylist", SystemTime::now()));
        std::fs::write(path(&dir, "easylist"), "||ads.example^").unwrap();
        let now = SystemTime::now();
        assert!(!due(&dir, "easylist", now));
        assert!(due(&dir, "easylist", now + REFRESH_AFTER));
        assert_eq!(
            load(&dir, &["easylist".into(), "easyprivacy".into()]),
            ["||ads.example^"]
        );
        remove_others(&dir, &["easyprivacy".into()]);
        assert!(fetched_at(&dir, "easylist").is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_list_is_fetched_over_https_and_has_its_own_id() {
        for (i, l) in LISTS.iter().enumerate() {
            assert!(l.url.starts_with("https://"), "{}", l.id);
            assert_eq!(list(l.id).map(|f| f.url), Some(l.url));
            assert!(LISTS[..i].iter().all(|o| o.id != l.id), "{}", l.id);
        }
    }
}
