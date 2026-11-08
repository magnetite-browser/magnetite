//! The capture tool: pictures of the page in front.

use magnetite_engine::{ContentId, Engine};
use serde_json::json;

use crate::{capture, lock};

use super::{MagnetiteApp, global};

impl MagnetiteApp {
    /// Freeze the focused page (or the page a key came from) and open the
    /// capture overlay over it.
    pub(super) fn capture_start(&self, engine: &Engine, from: Option<ContentId>) {
        let Some(c) = self.focused_content(from) else {
            return;
        };
        engine.capture_frame(
            c,
            Box::new(move |frame| {
                let (Some(app), engine) = (global(), Engine::global()) else {
                    return;
                };
                match frame {
                    Ok(frame) => {
                        lock(&app.capture).session = Some((c, frame));
                        if !engine.open_overlay(c, "magnetite://ui/capture") {
                            Self::ui_call(
                                engine,
                                "captureFailed",
                                &json!({ "error": "the page is not visible" }),
                            );
                        }
                    }
                    Err(e) => Self::ui_call(engine, "captureFailed", &json!({ "error": e })),
                }
            }),
        );
    }

    /// Crop the frozen frame to `rect` (CSS px), or keep all of it.
    pub(super) fn capture_region(&self, engine: &Engine, rect: Option<[f64; 4]>) {
        engine.close_overlay();
        let Some((_, frame)) = lock(&self.capture).session.take() else {
            return;
        };
        let rect = rect.unwrap_or([0.0, 0.0, frame.css_width, frame.css_height]);
        std::thread::spawn(move || {
            let result = capture::finish_region(&frame.png, frame.css_width, rect);
            Engine::post(Box::new(move || Self::capture_done(result)));
        });
    }

    pub(super) fn capture_full(&self, engine: &Engine) {
        engine.close_overlay();
        let Some((c, _)) = lock(&self.capture).session.take() else {
            return;
        };
        Self::ui_call(engine, "captureBusy", &json!({ "what": "full page" }));
        // 100 megapixels: a long page at Retina resolution, well inside memory.
        engine.capture_full_page(
            c,
            100e6,
            Box::new(|tiles| match tiles {
                Ok(t) => {
                    std::thread::spawn(move || {
                        let result = capture::finish_tiles(&t.tiles, t.cut_at);
                        Engine::post(Box::new(move || Self::capture_done(result)));
                    });
                }
                Err(e) => Self::capture_done(Err(e)),
            }),
        );
    }

    /// On the UI thread: keep the new capture (replacing the last) and tell the sidebar.
    pub(super) fn capture_done(result: Result<capture::Capture, String>) {
        let engine = Engine::global();
        match result {
            Ok(cap) => {
                let info = json!({ "width": cap.width, "height": cap.height, "thumb": cap.thumb, "cut_at": cap.cut_at });
                if let Some(app) = global() {
                    lock(&app.capture).result = Some(cap);
                }
                Self::ui_call(engine, "captureReady", &info);
            }
            Err(e) => Self::ui_call(engine, "captureFailed", &json!({ "error": e })),
        }
    }
}
