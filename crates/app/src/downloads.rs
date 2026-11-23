//! Magnetite's download list, newest first. Chromium keeps its own full
//! history (`chrome://downloads`); this list is what the Downloads button
//! and the "All downloads" page show, and the only files Magnetite opens,
//! reveals or copies for the UI: the UI names a list entry, never a path.
//!
//! Saved in `state.json`: at most `KEEP` entries, with file name, place,
//! size, status and start time (no addresses). Downloads of private profiles
//! are listed while Magnetite runs and never saved.
//!
//! A failed download can be tried again, and any download's link copied,
//! while Magnetite still runs: its address is kept in memory for that, and
//! is gone with the next start.

use std::path::{Path, PathBuf};

use magnetite_core::{Id, UnixSeconds};
use magnetite_engine::{DownloadReport, DownloadState};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Entries kept; older ones drop off.
const KEEP: usize = 100;

/// Extensions of files that run code or hand off to other programs when
/// opened. A page names its download as it likes (`Invoice.pdf.command`),
/// and opening is one click away, so these are only ever shown in the file
/// manager, never opened by Magnetite.
const RISKY: &[&str] = &[
    "action",
    "app",
    "applescript",
    "appimage",
    "appx",
    "bash",
    "bat",
    "cmd",
    "com",
    "command",
    "csh",
    "deb",
    "desktop",
    "exe",
    "fileloc",
    "hta",
    "inetloc",
    "jar",
    "js",
    "jse",
    "ksh",
    "lnk",
    "mpkg",
    "msi",
    "msix",
    "pkg",
    "pl",
    "ps1",
    "py",
    "rb",
    "reg",
    "rpm",
    "run",
    "scpt",
    "scptd",
    "scr",
    "sh",
    "terminal",
    "tool",
    "url",
    "vbe",
    "vbs",
    "webloc",
    "workflow",
    "wsf",
    "zsh",
];

/// Whether a file named `name` runs code or hands off when opened (`RISKY`).
pub fn risky(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| RISKY.contains(&e.to_ascii_lowercase().as_str()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    InProgress,
    Finished,
    Failed,
}

impl From<DownloadState> for Status {
    fn from(s: DownloadState) -> Self {
        match s {
            DownloadState::InProgress => Self::InProgress,
            DownloadState::Finished => Self::Finished,
            DownloadState::Failed | DownloadState::Canceled => Self::Failed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download {
    pub id: Id,
    pub name: String,
    /// Where the finished file is.
    #[serde(default)]
    pub path: Option<PathBuf>,
    pub status: Status,
    pub received: i64,
    #[serde(default)]
    pub total: Option<i64>,
    /// When it started.
    pub at: UnixSeconds,
    /// Failed because it was cancelled (in the list or the save dialog).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub canceled: bool,
    /// Paused while it runs: this run only.
    #[serde(skip)]
    paused: bool,
    /// Chromium's context and download id, while this run knows it.
    #[serde(skip)]
    key: Option<(Option<String>, u32)>,
    /// Where it was asked from, for `retry`: this run only, never saved.
    #[serde(skip)]
    url: Option<String>,
    /// From a private profile: listed, not saved.
    #[serde(skip)]
    private: bool,
    /// When its status last changed in this run (0: before it), so the
    /// notification panel shows only news.
    #[serde(skip)]
    changed_at: UnixSeconds,
}

#[derive(Debug, Default)]
pub struct Downloads {
    items: Vec<Download>,
}

impl Downloads {
    /// The saved list; a download still running when Magnetite quit did
    /// not finish.
    pub fn restore(mut saved: Vec<Download>) -> Self {
        for d in &mut saved {
            if d.status == Status::InProgress {
                d.status = Status::Failed;
            }
        }
        saved.truncate(KEEP);
        Self { items: saved }
    }

    /// Apply Chromium's report about a download. Returns the status the
    /// download had before (`None`: a new one).
    pub fn report(
        &mut self,
        r: &DownloadReport,
        private: bool,
        now: UnixSeconds,
    ) -> Option<Status> {
        let key = Some((r.context.clone(), r.id));
        let status = Status::from(r.state);
        if let Some(d) = self.items.iter_mut().find(|d| d.key == key) {
            let before = d.status;
            if before != status {
                d.changed_at = now;
            }
            d.name.clone_from(&r.name);
            d.path.clone_from(&r.path);
            d.status = status;
            d.canceled = r.state == DownloadState::Canceled;
            d.paused = r.paused;
            d.received = r.received;
            d.total = r.total;
            if r.url.is_some() {
                d.url.clone_from(&r.url);
            }
            return Some(before);
        }
        self.items.insert(
            0,
            Download {
                id: crate::state::new_id(),
                name: r.name.clone(),
                path: r.path.clone(),
                status,
                received: r.received,
                total: r.total,
                at: now,
                canceled: r.state == DownloadState::Canceled,
                paused: r.paused,
                key,
                url: r.url.clone(),
                private,
                changed_at: now,
            },
        );
        if self.items.len() > KEEP
            && let Some(oldest) = self
                .items
                .iter()
                .rposition(|d| d.status != Status::InProgress)
        {
            self.items.remove(oldest);
        }
        None
    }

    /// What `state.json` keeps: everything but private profiles' downloads.
    pub fn lasting(&self) -> Vec<Download> {
        self.items.iter().filter(|d| !d.private).cloned().collect()
    }

    /// Empty the list (running downloads stay).
    pub fn clear(&mut self) {
        self.items.retain(|d| d.status == Status::InProgress);
    }

    /// The profile context and address to fetch failed entry `id` from
    /// again, if this run knows them.
    pub fn retry(&self, id: &str) -> Option<(&str, &str)> {
        let d = self
            .items
            .iter()
            .find(|d| d.id == id && d.status == Status::Failed)?;
        Some((d.key.as_ref()?.0.as_deref()?, d.url.as_deref()?))
    }

    /// Take entry `id` off the list (the file stays): by hand, or as its
    /// retry is a download of its own. A running one stays.
    pub fn remove(&mut self, id: &str) {
        self.items
            .retain(|d| d.id != id || d.status == Status::InProgress);
    }

    /// What the engine needs to pause, resume or cancel entry `id`
    /// (`Engine::control_download`), while it runs in this run.
    pub fn running(&self, id: &str) -> Option<(Option<&str>, u32)> {
        let d = self
            .items
            .iter()
            .find(|d| d.id == id && d.status == Status::InProgress)?;
        let (context, chromium_id) = d.key.as_ref()?;
        Some((context.as_deref(), *chromium_id))
    }

    /// The web address entry `id` was asked from, while this run knows it.
    pub fn link(&self, id: &str) -> Option<&str> {
        self.items.iter().find(|d| d.id == id)?.url.as_deref()
    }

    /// Entry `id`'s file, if it finished.
    pub fn file(&self, id: &str) -> Option<&Path> {
        self.items
            .iter()
            .find(|d| d.id == id && d.status == Status::Finished)?
            .path
            .as_deref()
    }

    /// The list as the sidebar shows it (no paths or addresses; `missing`:
    /// the file was moved or deleted since; `risky`: it is only ever
    /// revealed; `retryable`: it failed and `retry` can fetch it again;
    /// `link`: `link` knows its address).
    pub fn for_ui(&self) -> Value {
        let items: Vec<Value> = self
            .items
            .iter()
            .map(|d| {
                let missing =
                    d.status == Status::Finished && !d.path.as_deref().is_some_and(Path::exists);
                json!({
                    "id": d.id,
                    "name": d.name,
                    "status": d.status,
                    "received": d.received,
                    "total": d.total,
                    "at": d.at,
                    "changed_at": d.changed_at,
                    "missing": missing,
                    "risky": risky(&d.name),
                    "retryable": self.retry(&d.id).is_some(),
                    "canceled": d.canceled,
                    "paused": d.paused,
                    "link": d.url.is_some(),
                })
            })
            .collect();
        json!({ "items": items })
    }
}

/// Open `path` with its app, or show it in the file manager (`reveal`, and
/// always for a `risky` file; a file that is gone shows its folder).
pub fn open(path: &Path, reveal: bool) -> std::io::Result<()> {
    use std::io::{Error, ErrorKind};
    use std::process::{Command, Stdio};
    if !path.is_absolute() {
        return Err(Error::from(ErrorKind::InvalidInput));
    }
    let reveal = reveal || path.file_name().and_then(|n| n.to_str()).is_some_and(risky);
    let exists = path.exists();
    if !exists && !reveal {
        return Err(Error::from(ErrorKind::NotFound));
    }
    let folder = path.parent().filter(|p| p.is_dir());
    let mut cmd;
    #[cfg(target_os = "macos")]
    {
        cmd = Command::new("/usr/bin/open");
        match (exists, reveal) {
            (true, true) => cmd.arg("-R").arg(path),
            (true, false) => cmd.arg(path),
            (false, _) => cmd.arg(folder.ok_or(ErrorKind::NotFound)?),
        };
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd = Command::new("explorer.exe");
        // Windows paths cannot hold `"`, so quoting them is safe.
        match (exists, reveal) {
            (true, true) => cmd.raw_arg(format!("/select,\"{}\"", path.display())),
            (true, false) => cmd.arg(path),
            (false, _) => cmd.arg(folder.ok_or(ErrorKind::NotFound)?),
        };
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        cmd = Command::new("xdg-open");
        if exists && !reveal {
            cmd.arg(path);
        } else {
            cmd.arg(folder.ok_or(ErrorKind::NotFound)?);
        }
    }
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(id: u32, state: DownloadState, path: Option<&str>) -> DownloadReport {
        DownloadReport {
            id,
            context: Some(String::new()),
            name: "a.pdf".into(),
            path: path.map(PathBuf::from),
            received: 5,
            total: Some(10),
            state,
            paused: false,
            url: Some("https://example.org/a.pdf".into()),
        }
    }

    #[test]
    fn files_that_run_code_are_risky_whatever_comes_before_the_extension() {
        for name in ["Invoice.pdf.command", "setup.PKG", "link.webloc", "x.sh"] {
            assert!(risky(name), "{name}");
        }
        for name in [
            "report.pdf",
            "photo.jpg",
            "archive.zip",
            "installer.dmg",
            "command",
            ".sh",
        ] {
            assert!(!risky(name), "{name}");
        }
    }

    #[test]
    fn reports_update_one_entry_and_tell_the_status_before() {
        let mut list = Downloads::default();
        assert_eq!(
            list.report(&report(1, DownloadState::InProgress, None), false, 7),
            None
        );
        let before = list.report(
            &report(1, DownloadState::Finished, Some("/d/a.pdf")),
            false,
            9,
        );
        assert_eq!(before, Some(Status::InProgress));
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].at, 7);
        let id = list.items[0].id.clone();
        assert_eq!(list.file(&id), Some(Path::new("/d/a.pdf")));
    }

    #[test]
    fn private_downloads_are_listed_not_kept() {
        let mut list = Downloads::default();
        list.report(&report(1, DownloadState::Finished, Some("/d/a")), true, 1);
        list.report(&report(2, DownloadState::Finished, Some("/d/b")), false, 2);
        assert_eq!(list.items.len(), 2);
        let kept = list.lasting();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].path.as_deref(), Some(Path::new("/d/b")));
    }

    #[test]
    fn restored_running_downloads_failed_and_the_list_stays_short() {
        let mut list = Downloads::default();
        for id in (0u32..).take(KEEP + 5) {
            list.report(&report(id, DownloadState::InProgress, None), false, 1);
        }
        // All running: none may drop off.
        assert_eq!(list.items.len(), KEEP + 5);
        let restored = Downloads::restore(list.lasting());
        assert_eq!(restored.items.len(), KEEP);
        assert!(restored.items.iter().all(|d| d.status == Status::Failed));
        // A file opens only once finished, and only through the list.
        assert_eq!(restored.file(&restored.items[0].id), None);
        assert_eq!(restored.file("/etc/passwd"), None);
    }

    #[test]
    fn only_a_download_that_failed_in_this_run_can_be_retried() {
        let mut list = Downloads::default();
        list.report(&report(1, DownloadState::InProgress, None), false, 1);
        let id = list.items[0].id.clone();
        assert_eq!(list.retry(&id), None);
        list.report(&report(1, DownloadState::Failed, None), false, 2);
        assert_eq!(list.retry(&id), Some(("", "https://example.org/a.pdf")));
        // The address is not saved, so after a restart there is none.
        let saved = serde_json::to_string(&list.lasting()).unwrap();
        assert!(!saved.contains("example.org"));
        let restored = Downloads::restore(serde_json::from_str(&saved).unwrap());
        assert_eq!(restored.retry(&id), None);
    }

    #[test]
    fn only_a_running_download_is_paused_resumed_or_cancelled_and_it_stays_listed() {
        let mut list = Downloads::default();
        list.report(&report(1, DownloadState::InProgress, None), false, 1);
        let id = list.items[0].id.clone();
        assert_eq!(list.running(&id), Some((Some(""), 1)));
        let paused = DownloadReport {
            paused: true,
            ..report(1, DownloadState::InProgress, None)
        };
        list.report(&paused, false, 2);
        assert_eq!(list.for_ui()["items"][0]["paused"], true);
        // Running: it stays in the list until it ends.
        list.remove(&id);
        assert_eq!(list.items.len(), 1);
        list.report(&report(1, DownloadState::Canceled, None), false, 3);
        assert_eq!(list.running(&id), None);
        assert_eq!(list.for_ui()["items"][0]["canceled"], true);
        // Cancelled and failed alike: it can be fetched again.
        assert!(list.retry(&id).is_some());
        // After a restart it is still cancelled, not just failed.
        let saved = serde_json::to_string(&list.lasting()).unwrap();
        assert!(Downloads::restore(serde_json::from_str(&saved).unwrap()).items[0].canceled);
        list.remove(&id);
        assert!(list.items.is_empty());
    }

    #[test]
    fn a_link_is_known_in_the_run_only() {
        let mut list = Downloads::default();
        list.report(&report(1, DownloadState::Finished, Some("/d/a")), false, 1);
        let id = list.items[0].id.clone();
        assert_eq!(list.link(&id), Some("https://example.org/a.pdf"));
        assert_eq!(list.for_ui()["items"][0]["link"], true);
        let saved = serde_json::to_string(&list.lasting()).unwrap();
        let restored = Downloads::restore(serde_json::from_str(&saved).unwrap());
        assert_eq!(restored.link(&id), None);
        assert_eq!(restored.for_ui()["items"][0]["link"], false);
    }

    #[test]
    fn clearing_keeps_running_downloads() {
        let mut list = Downloads::default();
        list.report(&report(1, DownloadState::Finished, Some("/d/a")), false, 1);
        list.report(&report(2, DownloadState::InProgress, None), false, 2);
        list.clear();
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].status, Status::InProgress);
    }
}
