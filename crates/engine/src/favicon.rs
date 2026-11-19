//! Local favicon cache. When a loaded page reports its icon, Chromium fetches
//! it (the page already did) and we keep a PNG under
//! `<data_dir>/favicons/<host>.png`. The sidebar shows it through
//! `magnetite://ui/favicons/<host>.png`, so unloaded tabs and favorites get real
//! icons without the sidebar ever contacting a site.

use std::path::{Path, PathBuf};

use cef::*;

use crate::engine::Engine;
use crate::events::ContentId;

/// Cache key for a page URL: lowercase host without `www.`, restricted to
/// characters that are safe in a file name. Matches `hostOf` in the UI.
pub fn cache_key(page_url: &str) -> Option<String> {
    let rest = page_url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    let host = host.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let ok = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
        && !host.contains("..");
    ok.then(|| host.replace(':', "_"))
}

pub fn cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("favicons")
}

impl Engine {
    /// Download `icon_url` for content `id` and store it under the page's host.
    pub(crate) fn cache_favicon(&self, id: ContentId, icon_url: &str) {
        // Private pages leave nothing on disk.
        if (self.lock().contents.get(&id)).is_none_or(|c| c.ephemeral) {
            return;
        }
        // A system page has no Chromium browser to fetch with.
        if self.is_system_page(id) {
            // Fetched with no cookies, so the request tells its site
            // nothing of any profile.
            let page = crate::system::backend::address_of(id);
            if let Some(key) = page.as_deref().and_then(cache_key) {
                let path = cache_dir(&self.config().data_dir).join(format!("{key}.png"));
                crate::system::backend::fetch_icon(icon_url, path);
            }
            return;
        }
        let Some(browser) = self.content_browser(id) else {
            return;
        };
        let page = browser
            .main_frame()
            .map(|f| CefString::from(&f.url()).to_string())
            .unwrap_or_default();
        if !page.starts_with("http") {
            return;
        }
        let Some(key) = cache_key(&page) else { return };
        let Some(host) = browser.host() else { return };
        let path = cache_dir(&self.config().data_dir).join(format!("{key}.png"));
        let mut cb = SaveFavicon::new(path);
        host.download_image(Some(&CefString::from(icon_url)), 1, 64, 0, Some(&mut cb));
    }
}

wrap_download_image_callback! {
    struct SaveFavicon {
        path: PathBuf,
    }
    impl DownloadImageCallback {
        fn on_download_image_finished(&self, _url: Option<&CefString>, status: ::std::os::raw::c_int, image: Option<&mut Image>) {
            if status != 200 && status != 0 {
                return;
            }
            // CEF rejects null out-parameters, so the pixel size must be requested.
            let (mut w, mut h) = (0, 0);
            let Some(png) = image.and_then(|i| i.as_png(2.0, 1, Some(&mut w), Some(&mut h))) else { return };
            let mut bytes = vec![0u8; png.size()];
            if bytes.is_empty() || png.data(Some(&mut bytes), 0) != bytes.len() {
                return;
            }
            store_icon(&self.path, &bytes);
        }
    }
}

/// Keep the PNG `bytes` as the icon at `path` (`cache_dir`).
pub(crate) fn store_icon(path: &Path, bytes: &[u8]) {
    let Some(dir) = path.parent() else { return };
    // Not the path: its name is the page's host.
    if let Err(e) = std::fs::create_dir_all(dir) {
        crate::trace(&format!("favicon cache: cannot create its folder: {e}"));
    }
    // A temp file of its own per call: two pages of one host can store the
    // same icon at once, from different threads, and each rename then puts
    // a whole file in place.
    static STORED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = STORED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("{}-{n}.tmp", std::process::id()));
    let stored = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path));
    if let Err(e) = stored {
        let _ = std::fs::remove_file(&tmp);
        crate::trace(&format!("favicon cache: cannot store an icon: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::cache_key;

    #[test]
    fn keys_are_hosts_safe_for_file_names() {
        assert_eq!(
            cache_key("https://www.GitHub.com/a?b").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            cache_key("http://localhost:8080/").as_deref(),
            Some("localhost_8080")
        );
        assert_eq!(
            cache_key("https://user@example.org/x").as_deref(),
            Some("example.org")
        );
        assert_eq!(cache_key("https://../etc"), None);
        assert_eq!(cache_key("https://a/b%2f"), Some("a".into()));
        assert_eq!(cache_key("about:blank"), None);
    }
}
