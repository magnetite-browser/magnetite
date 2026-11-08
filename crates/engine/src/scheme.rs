//! `magnetite://ui/` serves the built sidebar UI from `EngineConfig::ui_dir`,
//! cached favicons (`favicons/…`) and extension icons (`ext-icons/<id>`).

use std::sync::{Arc, Mutex};

use cef::sys;
use cef::*;

pub const SCHEME: &str = "magnetite";
pub const UI_HOST: &str = "ui";

/// `STANDARD | SECURE | CORS_ENABLED | FETCH_ENABLED | DISPLAY_ISOLATED`.
/// Display-isolated: only `magnetite://` pages may frame, link to or load
/// these URLs, so web pages can neither embed our UI nor probe its icons.
pub fn scheme_options() -> i32 {
    sys::cef_scheme_options_t::CEF_SCHEME_OPTION_STANDARD as i32
        | sys::cef_scheme_options_t::CEF_SCHEME_OPTION_SECURE as i32
        | sys::cef_scheme_options_t::CEF_SCHEME_OPTION_CORS_ENABLED as i32
        | sys::cef_scheme_options_t::CEF_SCHEME_OPTION_FETCH_ENABLED as i32
        | sys::cef_scheme_options_t::CEF_SCHEME_OPTION_DISPLAY_ISOLATED as i32
}

/// Our pages run only their own scripts, and nothing may frame them.
/// Images: our caches, `data:` and favicons of web pages.
const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; \
    style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self' data:; \
    connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; \
    frame-ancestors 'none'";

pub fn register_schemes(registrar: &mut SchemeRegistrar) {
    registrar.add_custom_scheme(Some(&CefString::from(SCHEME)), scheme_options());
}

pub fn install_factory(ui_dir: std::path::PathBuf, favicon_dir: std::path::PathBuf) {
    let mut factory = UiSchemeFactory::new(Arc::new(ui_dir), Arc::new(favicon_dir));
    register_scheme_handler_factory(
        Some(&CefString::from(SCHEME)),
        Some(&CefString::from(UI_HOST)),
        Some(&mut factory),
    );
}

wrap_scheme_handler_factory! {
    struct UiSchemeFactory {
        ui_dir: Arc<std::path::PathBuf>,
        favicon_dir: Arc<std::path::PathBuf>,
    }
    impl SchemeHandlerFactory {
        fn create(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _scheme_name: Option<&CefString>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let url = CefString::from(&request?.url()).to_string();
            let path = url_path(&url);
            let (bytes, mime) = match path.strip_prefix("favicons/") {
                // Cached icons only; a miss is a plain failure so <img> falls back.
                Some(name) if !name.contains(['/', '\\']) && !name.starts_with('.') => {
                    (std::fs::read(self.favicon_dir.join(name)).ok()?, "image/png".to_string())
                }
                Some(_) => return None,
                None => match path.strip_prefix("ext-icons/") {
                    // Icons of installed extensions, by id (`extensions.rs`).
                    Some(id) => {
                        let file = crate::engine::Engine::global().extension_icon(id)?;
                        (crate::extensions::icon_bytes(&file)?, "image/png".to_string())
                    }
                    None => load_asset(&self.ui_dir, &path)?,
                },
            };
            Some(BytesResourceHandler::new(Arc::new(bytes), mime, Arc::new(Mutex::new(0))))
        }
    }
}

/// `magnetite://ui/a/b?x#y` -> `a/b`
fn url_path(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let rest = rest.split_once('/').map_or("", |(_, p)| p);
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    rest.trim_matches('/').to_string()
}

/// Read a file below `ui_dir`; unknown paths fall back to `index.html` so a
/// single-page app can own its routes. Path traversal is rejected.
fn load_asset(ui_dir: &std::path::Path, path: &str) -> Option<(Vec<u8>, String)> {
    if path.split('/').any(|seg| seg == "..") {
        return None;
    }
    let candidate = if path.is_empty() {
        ui_dir.join("index.html")
    } else {
        ui_dir.join(path)
    };
    let file = if candidate.is_file() {
        candidate
    } else if !path.contains('.') {
        ui_dir.join("index.html")
    } else {
        return None;
    };
    let bytes = std::fs::read(&file).ok()?;
    let mime = match file.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html",
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    };
    Some((bytes, mime.to_string()))
}

wrap_resource_handler! {
    struct BytesResourceHandler {
        data: Arc<Vec<u8>>,
        mime: String,
        offset: Arc<Mutex<usize>>,
    }
    impl ResourceHandler {
        fn open(
            &self,
            _request: Option<&mut Request>,
            handle_request: Option<&mut ::std::os::raw::c_int>,
            _callback: Option<&mut Callback>,
        ) -> ::std::os::raw::c_int {
            if let Some(h) = handle_request { *h = 1; }
            1
        }

        fn response_headers(
            &self,
            response: Option<&mut Response>,
            response_length: Option<&mut i64>,
            _redirect_url: Option<&mut CefString>,
        ) {
            if let Some(r) = response {
                r.set_status(200);
                r.set_mime_type(Some(&CefString::from(self.mime.as_str())));
                r.set_header_by_name(Some(&CefString::from("Cache-Control")), Some(&CefString::from("no-cache")), 1);
                r.set_header_by_name(Some(&CefString::from("Content-Security-Policy")), Some(&CefString::from(CONTENT_SECURITY_POLICY)), 1);
                r.set_header_by_name(Some(&CefString::from("X-Content-Type-Options")), Some(&CefString::from("nosniff")), 1);
            }
            if let Some(len) = response_length { *len = i64::try_from(self.data.len()).unwrap_or(i64::MAX); }
        }

        fn read(
            &self,
            data_out: *mut u8,
            bytes_to_read: ::std::os::raw::c_int,
            bytes_read: Option<&mut ::std::os::raw::c_int>,
            _callback: Option<&mut ResourceReadCallback>,
        ) -> ::std::os::raw::c_int {
            let mut offset = crate::engine::lock(&self.offset);
            let remaining = self.data.len().saturating_sub(*offset);
            let n = remaining.min(usize::try_from(bytes_to_read).unwrap_or(0));
            if n == 0 {
                if let Some(b) = bytes_read { *b = 0; }
                return 0;
            }
            // SAFETY: CEF guarantees `data_out` holds at least `bytes_to_read` bytes.
            unsafe { std::ptr::copy_nonoverlapping(self.data.as_ptr().add(*offset), data_out, n); }
            *offset += n;
            if let Some(b) = bytes_read { *b = i32::try_from(n).unwrap_or(i32::MAX); }
            1
        }
    }
}
