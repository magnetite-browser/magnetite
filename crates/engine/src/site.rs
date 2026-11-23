//! Per-page site state for the address row and site panel (connection, zoom)
//! and native clipboard writes. Camera and microphone use arrive separately as
//! `EngineEvent::MediaAccessChanged`.

use cef::*;
use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::events::{ContentId, EngineEvent};

/// Chromium's own zoom steps, in percent.
pub const ZOOM_STEPS: &[u32] = &[
    25, 33, 50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300, 400, 500,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SiteState {
    /// `Some(true)` for an encrypted connection, `Some(false)` for plain
    /// http, `None` for pages without a network origin (magnetite:, file:, …).
    pub secure: Option<bool>,
    /// The certificate has an error (expired, wrong host, untrusted, …).
    pub cert_error: bool,
    /// Page zoom in percent.
    pub zoom: u32,
    /// The zoom, in percent, the page's site has without one of its own:
    /// 100 unless the user chose another (`Engine::set_default_zoom`).
    #[serde(default)]
    pub base_zoom: u32,
    /// The system's own engine shows the page, not Chromium (`system.rs`).
    #[serde(default)]
    pub system_engine: bool,
}

/// The zoom step after `current` percent, in (`up`) or out; `current` when
/// there is none.
pub fn zoom_step(current: u32, up: bool) -> u32 {
    let next = if up {
        ZOOM_STEPS.iter().find(|&&z| z > current)
    } else {
        ZOOM_STEPS.iter().rev().find(|&&z| z < current)
    };
    next.copied().unwrap_or(current)
}

/// Chromium zoom level ↔ percent: factor = 1.2^level.
fn level_to_percent(level: f64) -> u32 {
    let pct = (100.0 * 1.2f64.powf(level)).round();
    // Clamped to Chromium's range, so the cast cannot truncate or wrap.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let pct = pct.clamp(1.0, 1000.0) as u32;
    pct
}

fn percent_to_level(pct: u32) -> f64 {
    (f64::from(pct) / 100.0).ln() / 1.2f64.ln()
}

/// Chromium's preference for the zoom a site has until it gets one of its
/// own: a level per storage partition of the profile.
const DEFAULT_ZOOM: &str = "partition.default_zoom_level";
/// The partition a profile's own pages are in, as that preference names it.
const DEFAULT_PARTITION: &str = "x";

/// Make `pct` percent the zoom the sites of `ctx` start with. False if
/// Chromium refused (a context that is not initialised yet does; it gets
/// the zoom in `prepare_context`).
pub(crate) fn apply_default_zoom(ctx: &RequestContext, pct: u32) -> bool {
    let (Some(mut value), Some(mut levels)) = (value_create(), dictionary_value_create()) else {
        return false;
    };
    let level = if pct == 100 {
        0.0
    } else {
        percent_to_level(pct)
    };
    levels.set_double(Some(&CefString::from(DEFAULT_PARTITION)), level);
    value.set_dictionary(Some(&mut levels));
    // Not empty: see `autofill::set_bool_pref`.
    let mut error = CefString::from(" ");
    let name = CefString::from(DEFAULT_ZOOM);
    ctx.set_preference(Some(&name), Some(&mut value), Some(&mut error)) != 0
}

/// The level of a site zoomed to 100 percent on its own: level 0 itself
/// would give it the default zoom.
const OWN_100: f64 = 1e-6;

/// Errors are the low 16 bits and bits 24–31 of net's cert status
/// (`CERT_STATUS_ALL_ERRORS`); bits 16–23 are informational.
fn has_cert_error(status: CertStatus) -> bool {
    let raw: sys::cef_cert_status_t = status.into();
    (raw as u32) & 0xFF00_FFFF != 0
}

impl Engine {
    /// Current site state of content `id` (the visible navigation entry).
    pub fn content_site_state(&self, id: ContentId) -> Option<SiteState> {
        if self.is_system_page(id) {
            return crate::system::site_state(id);
        }
        let host = self.content_browser_host(id)?;
        let zoom = level_to_percent(host.zoom_level());
        let base_zoom = level_to_percent(host.default_zoom_level());
        let entry = host.visible_navigation_entry();
        let url = entry
            .as_ref()
            .map(|e| CefString::from(&e.url()).to_string())
            .unwrap_or_default();
        let ssl = entry.and_then(|e| e.sslstatus());
        let cert_error = ssl
            .as_ref()
            .is_some_and(|s| has_cert_error(s.cert_status()));
        let secure = if url.starts_with("https://") {
            Some(ssl.is_some_and(|s| s.is_secure_connection() != 0) && !cert_error)
        } else if url.starts_with("http://") {
            Some(false)
        } else {
            None
        };
        Some(SiteState {
            secure,
            cert_error,
            zoom,
            base_zoom,
            system_engine: false,
        })
    }

    pub(crate) fn emit_site_state(&self, id: ContentId) {
        if let Some(state) = self.content_site_state(id) {
            self.emit(EngineEvent::SiteChanged { id, state });
        }
    }

    /// Re-read the site state shortly after Chromium handled a zoom key.
    #[allow(clippy::unused_self)] // an Engine method like its siblings
    pub(crate) fn emit_site_state_soon(&self, id: ContentId) {
        Self::post_delayed(Box::new(move || Engine::global().emit_site_state(id)), 50);
    }

    /// Use `pct` percent as the zoom every site starts with: in every
    /// profile that exists, and in those made later. A site zoomed on its
    /// own keeps its zoom. Chromium reads the level when it
    /// starts, so what is set here counts from the next start; it has to
    /// be set before that start, when the setting changes.
    pub fn set_default_zoom(&self, pct: u32) {
        let contexts: Vec<RequestContext> = {
            let mut inner = self.lock();
            inner.default_zoom = pct;
            inner.contexts.values().cloned().collect()
        };
        for ctx in request_context_get_global_context()
            .into_iter()
            .chain(contexts)
        {
            apply_default_zoom(&ctx, pct);
        }
    }

    /// A page of Magnetite's own starts to load in `browser`: the zoom web
    /// pages start with is not for it. The sidebar and Magnetite's pages
    /// have a text size of their own (Settings → Appearance), so they stay
    /// at 100 percent whatever that zoom is.
    pub(crate) fn keep_own_page_unzoomed(browser: &Browser, url: &str) {
        if !url.starts_with("magnetite://") {
            return;
        }
        let Some(host) = browser.host() else { return };
        let zoomed = host.default_zoom_level().abs() > OWN_100;
        if zoomed && (host.zoom_level() - OWN_100).abs() > f64::EPSILON {
            host.set_zoom_level(OWN_100);
        }
    }

    /// Set page zoom to `pct` percent; `None` gives the site the zoom every
    /// site starts with again.
    pub fn set_content_zoom(&self, id: ContentId, pct: Option<u32>) {
        if self.is_system_page(id) {
            self.system_set_zoom(id, pct.unwrap_or(100));
        } else if let Some(host) = self.content_browser_host(id) {
            let base = level_to_percent(host.default_zoom_level());
            // A page of Magnetite's own has no part in that default
            // (`keep_own_page_unzoomed`): its zoom "again" is 100 percent.
            let own = (host.browser())
                .and_then(|b| b.main_frame())
                .is_some_and(|f| {
                    CefString::from(&f.url())
                        .to_string()
                        .starts_with("magnetite://")
                });
            host.set_zoom_level(match pct {
                None if own && base != 100 => OWN_100,
                // Level 0 is Chromium's word for "the default again".
                None => 0.0,
                Some(pct) if pct == base => 0.0,
                // So 100 percent, where that is not the default, is a
                // level next to 0.
                Some(100) => OWN_100,
                Some(pct) => percent_to_level(pct),
            });
            self.emit_site_state_soon(id);
        }
    }

    /// Put `text` on the system clipboard. Dev and test runs can set
    /// `MAGNETITE_CLIPBOARD_FILE` to write a file instead and leave the user's
    /// clipboard alone.
    pub fn copy_text(&self, text: &str) -> bool {
        if let Some(path) = std::env::var_os("MAGNETITE_CLIPBOARD_FILE") {
            return std::fs::write(path, text).is_ok();
        }
        #[cfg(target_os = "macos")]
        return crate::mac::copy_text(text);
        #[cfg(target_os = "windows")]
        return (self.frame_window())
            .is_some_and(|frame| crate::win::copy_text(frame.window_handle(), text));
        #[cfg(target_os = "linux")]
        return crate::clipboard::copy_text(text);
    }

    /// Put the file at `path` on the system clipboard, to paste it in the
    /// file manager or another app. `MAGNETITE_CLIPBOARD_FILE` gets the path
    /// instead, as for `copy_text`.
    pub fn copy_file(&self, path: &std::path::Path) -> bool {
        if let Some(to) = std::env::var_os("MAGNETITE_CLIPBOARD_FILE") {
            return std::fs::write(to, path.as_os_str().as_encoded_bytes()).is_ok();
        }
        #[cfg(target_os = "macos")]
        return crate::mac::copy_file(path);
        #[cfg(not(target_os = "macos"))]
        {
            let _ = path;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_levels_roundtrip_through_percent() {
        assert_eq!(level_to_percent(0.0), 100);
        for &p in ZOOM_STEPS {
            let back = level_to_percent(percent_to_level(p));
            assert!(back.abs_diff(p) <= 1, "{p} -> {back}");
        }
    }

    #[test]
    fn zoom_steps_move_to_the_next_step_and_stop_at_the_ends() {
        assert_eq!(zoom_step(100, true), 110);
        assert_eq!(zoom_step(100, false), 90);
        // Between steps (zoomed by the page or a trackpad): to the next one.
        assert_eq!(zoom_step(105, true), 110);
        assert_eq!(zoom_step(105, false), 100);
        assert_eq!(zoom_step(500, true), 500);
        assert_eq!(zoom_step(25, false), 25);
    }
}
