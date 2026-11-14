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
    /// Page zoom in percent (100 = default).
    pub zoom: u32,
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

/// Errors are the low 16 bits and bits 24–31 of net's cert status
/// (`CERT_STATUS_ALL_ERRORS`); bits 16–23 are informational.
fn has_cert_error(status: CertStatus) -> bool {
    let raw: sys::cef_cert_status_t = status.into();
    (raw as u32) & 0xFF00_FFFF != 0
}

impl Engine {
    /// Current site state of content `id` (the visible navigation entry).
    pub fn content_site_state(&self, id: ContentId) -> Option<SiteState> {
        let host = self.content_browser_host(id)?;
        let zoom = level_to_percent(host.zoom_level());
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

    /// Set page zoom to `pct` percent (100 resets).
    pub fn set_content_zoom(&self, id: ContentId, pct: u32) {
        if let Some(host) = self.content_browser_host(id) {
            host.set_zoom_level(if pct == 100 {
                0.0
            } else {
                percent_to_level(pct)
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
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = text;
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
