//! Downloads: Chromium does the work (where files go, the save dialog); we
//! report each download's progress to the app, which lists them in the
//! sidebar (Chromium's own download bubble sits in the toolbar we keep out
//! of sight).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use cef::*;

use crate::engine::Engine;
use crate::events::{DownloadReport, DownloadState, EngineEvent};

/// Where downloads go, as the app's settings keep it. Chromium saves the
/// files; these are its preferences, in every profile alike.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct DownloadPrefs {
    /// The folder downloads are saved to, as an absolute path. Empty:
    /// Chromium's own choice, the system's Downloads folder.
    pub folder: String,
    /// Ask where to save each file.
    pub ask: bool,
}

/// Chromium's preference for the folder downloads go to. The folder "Save
/// as" starts in is another one, which Chromium keeps as the user last
/// left it: Magnetite does not touch it.
const FOLDER: &str = "download.default_directory";
const ASK: &str = "download.prompt_for_download";

impl DownloadPrefs {
    /// Set them on `ctx`. False if Chromium refused one (a context that is
    /// not initialised yet refuses all; it gets them in `prepare_context`).
    pub(crate) fn apply(&self, ctx: &RequestContext) -> bool {
        let folder = (!self.folder.is_empty()).then_some(self.folder.as_str());
        crate::autofill::set_bool_pref(ctx, ASK, self.ask)
            & crate::autofill::set_string_pref(ctx, FOLDER, folder)
    }
}

/// What hears which folder the user chose, once.
type Chosen = std::sync::Arc<std::sync::Mutex<Option<Box<dyn FnOnce(Option<PathBuf>) + Send>>>>;

// The folder the user chose in the system's dialog, or none.
wrap_run_file_dialog_callback! {
    struct FolderChosen {
        done: Chosen,
    }
    impl RunFileDialogCallback {
        fn on_file_dialog_dismissed(&self, file_paths: Option<&mut CefStringList>) {
            let chosen = file_paths.and_then(crate::client::first_string).map(PathBuf::from);
            if let Some(done) = self.done.lock().ok().and_then(|mut d| d.take()) {
                done(chosen);
            }
        }
    }
}

impl Engine {
    /// Use `prefs` from now on: in every profile that exists, and in those
    /// made later.
    pub fn set_downloads(&self, prefs: &DownloadPrefs) {
        let contexts: Vec<RequestContext> = {
            let mut inner = self.lock();
            inner.downloads.clone_from(prefs);
            inner.contexts.values().cloned().collect()
        };
        for ctx in request_context_get_global_context()
            .into_iter()
            .chain(contexts)
        {
            prefs.apply(&ctx);
        }
    }

    /// Whether a dialog of the system's can open now: there is a window
    /// to open it over.
    pub fn can_open_dialog(&self) -> bool {
        self.dialog_host().is_some()
    }

    /// Let the user choose a folder in the system's dialog, starting at
    /// `start`. `done` gets it on the UI thread, or `None` when the dialog
    /// was closed without a choice or could not open.
    pub fn choose_folder(
        &self,
        title: &str,
        start: Option<&std::path::Path>,
        done: Box<dyn FnOnce(Option<PathBuf>) + Send>,
    ) {
        let Some(host) = self.dialog_host() else {
            return done(None);
        };
        let start = start.map(|p| CefString::from(p.to_string_lossy().as_ref()));
        let mut callback =
            FolderChosen::new(std::sync::Arc::new(std::sync::Mutex::new(Some(done))));
        // No filters is an empty list, not none: CEF reads the list's size
        // without a look at whether there is one, and the app ends there.
        let mut filters = CefStringList::new();
        host.run_file_dialog(
            FileDialogMode::OPEN_FOLDER,
            Some(&CefString::from(title)),
            start.as_ref(),
            Some(&mut filters),
            Some(&mut callback),
        );
    }
}

impl Engine {
    /// Download `url` in the profile with context id `context` (`""`: the
    /// default one), as a new download. False when no page of that profile
    /// is open: a download needs one to start from.
    pub fn start_download(&self, context: &str, url: &str) -> bool {
        let host = self
            .lock()
            .contents
            .values()
            .filter(|c| c.profile == context)
            .find_map(|c| c.browser.as_ref()?.host());
        let Some(host) = host else { return false };
        host.start_download(Some(&CefString::from(url)));
        true
    }
}

/// A download: its profile's context and Chromium's id.
type Key = (Option<String>, u32);

thread_local! {
    /// Last state and step told per download, so the app hears about each
    /// state change and about every percent, not every received chunk.
    static TOLD: RefCell<HashMap<Key, (DownloadState, i64)>> = RefCell::new(HashMap::new());
}

wrap_download_handler! {
    pub(crate) struct MagnetiteDownloadHandler {}
    impl DownloadHandler {
        // CEF's default here would cancel every download.
        fn can_download(&self, _browser: Option<&mut Browser>, _url: Option<&CefString>, _method: Option<&CefString>) -> ::std::os::raw::c_int {
            1
        }
        // Chromium's own handling (Downloads folder, its settings).
        fn on_before_download(
            &self,
            _browser: Option<&mut Browser>,
            _item: Option<&mut DownloadItem>,
            _suggested_name: Option<&CefString>,
            _callback: Option<&mut BeforeDownloadCallback>,
        ) -> ::std::os::raw::c_int {
            0
        }
        fn on_download_updated(&self, browser: Option<&mut Browser>, item: Option<&mut DownloadItem>, _callback: Option<&mut DownloadItemCallback>) {
            let Some(item) = item else { return };
            // An extension being installed from the Chrome Web Store is no
            // download to the user (Chrome does not list it either).
            if is_extension_install(&CefString::from(&item.original_url()).to_string()) {
                return;
            }
            let state = if item.is_complete() != 0 {
                DownloadState::Finished
            } else if item.is_canceled() != 0 || item.is_interrupted() != 0 {
                DownloadState::Failed
            } else if item.is_in_progress() != 0 {
                DownloadState::InProgress
            } else {
                return;
            };
            let engine = Engine::global();
            let context = browser
                .and_then(|b| b.host())
                .and_then(|h| h.request_context())
                .and_then(|c| engine.context_id_of(&c));
            let received = item.received_bytes();
            let total = Some(item.total_bytes()).filter(|t| *t > 0);
            // Percent when the size is known, else every MiB.
            let step = total.map_or(received >> 20, |t| received * 100 / t);
            let key = (context.clone(), item.id());
            let new = TOLD.with(|t| t.borrow_mut().insert(key, (state, step)) != Some((state, step)));
            if !new {
                return;
            }
            let full = CefString::from(&item.full_path()).to_string();
            // Before redirects: where they led may be good for one use only.
            let asked = CefString::from(&item.original_url()).to_string();
            // Until Chromium has picked the file, the server's name stands in.
            let suggested = [
                CefString::from(&item.suggested_file_name()).to_string(),
                disposition_name(&CefString::from(&item.content_disposition()).to_string()).unwrap_or_default(),
                url_name(&CefString::from(&item.url()).to_string()).unwrap_or_default(),
            ]
            .into_iter()
            .find(|n| !n.is_empty())
            .unwrap_or_default();
            engine.emit(EngineEvent::Download(DownloadReport {
                id: item.id(),
                context,
                name: display_name(&full, &suggested),
                path: (state == DownloadState::Finished && !full.is_empty()).then(|| PathBuf::from(&full)),
                received,
                total,
                state,
                url: retryable(&asked).then_some(asked),
            }));
        }
    }
}

/// Where the Chrome Web Store's installer fetches an extension's package
/// (`extension_urls::GetDefaultWebstoreUpdateUrl` in Chromium); the file
/// lands as `<extension id>.crx` and is deleted once installed.
const WEBSTORE_PACKAGES: &str = "https://clients2.google.com/service/update2/crx";

/// Whether a download that started at `url` is an extension install.
fn is_extension_install(url: &str) -> bool {
    url.strip_prefix(WEBSTORE_PACKAGES)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('?'))
}

/// Whether a download from `url` can be asked for again: a web address. A
/// `blob:` or `data:` one belongs to the page that made it.
fn retryable(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// The name to show for a download at `full` (Chromium's current path):
/// while it runs that is `<name>.crdownload`, or `Unconfirmed <n>.crdownload`
/// before Chromium has picked the file; then the `suggested` name.
fn display_name(full: &str, suggested: &str) -> String {
    let file = std::path::Path::new(full)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let file = file.strip_suffix(".crdownload").unwrap_or(&file);
    if !file.is_empty() && !file.starts_with("Unconfirmed ") {
        return file.to_string();
    }
    if suggested.is_empty() {
        "download".to_string()
    } else {
        suggested.to_string()
    }
}

/// The file name a `Content-Disposition` header suggests: `filename*`
/// (RFC 5987, UTF-8) first, else `filename`. Display only.
fn disposition_name(header: &str) -> Option<String> {
    let param = |key: &str| {
        header.split(';').find_map(|p| {
            let (k, v) = p.split_once('=')?;
            k.trim()
                .eq_ignore_ascii_case(key)
                .then(|| v.trim().trim_matches('"').to_string())
        })
    };
    let name = param("filename*")
        .and_then(|v| {
            v.split_once("''")
                .map(|(_, encoded)| percent_decode(encoded))
        })
        .or_else(|| param("filename"))?;
    last_segment(&name)
}

/// The last path segment of `url`, decoded.
fn url_name(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next()?.split_once("://")?.1;
    last_segment(&percent_decode(path.split_once('/')?.1))
}

fn last_segment(name: &str) -> Option<String> {
    Some(name.rsplit(['/', '\\']).next()?.trim().to_string()).filter(|n| !n.is_empty())
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{display_name, disposition_name, is_extension_install, url_name};

    #[test]
    fn web_store_installs_are_no_downloads() {
        assert!(is_extension_install(
            "https://clients2.google.com/service/update2/crx?response=redirect&x=id%3Dabc%26installsource%3Dondemand%26uc"
        ));
        assert!(!is_extension_install(
            "https://clients2.google.com/service/update2/crxfile.zip"
        ));
        assert!(!is_extension_install(
            "https://example.org/service/update2/crx?x=1"
        ));
        assert!(!is_extension_install("https://example.org/tool.crx"));
    }

    #[test]
    fn names_from_the_server_while_chromium_has_none() {
        assert_eq!(
            disposition_name(r#"attachment; filename="slow.bin""#).as_deref(),
            Some("slow.bin")
        );
        assert_eq!(
            disposition_name("attachment; filename=a.txt; filename*=UTF-8''na%C3%AFve%20file.txt")
                .as_deref(),
            Some("naïve file.txt")
        );
        assert_eq!(
            disposition_name(r#"attachment; filename="../../etc/x""#).as_deref(),
            Some("x")
        );
        assert_eq!(disposition_name("inline"), None);
        assert_eq!(
            url_name("https://example.org/files/My%20Doc.pdf?x=1").as_deref(),
            Some("My Doc.pdf")
        );
        assert_eq!(url_name("https://example.org/"), None);
    }

    #[test]
    fn names_come_from_the_path_without_chromiums_temporary_suffix() {
        assert_eq!(
            display_name("/d/report (1).pdf", "report.pdf"),
            "report (1).pdf"
        );
        assert_eq!(
            display_name("/d/report.pdf.crdownload", "x.pdf"),
            "report.pdf"
        );
        assert_eq!(
            display_name("/d/Unconfirmed 4711.crdownload", "a.zip"),
            "a.zip"
        );
        assert_eq!(display_name("", "a.zip"), "a.zip");
        assert_eq!(display_name("", ""), "download");
    }
}
