//! The capture tool: pictures of the page in front.

use magnetite_engine::{ContentId, Engine};
use serde_json::json;

use crate::{capture, dev, lock};

/// Run in a page before its picture is taken at a new size: resolves once
/// the page had time to lay itself out again.
const SETTLE: &str = "new Promise((done) => setTimeout(done, 400))";

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

    /// Crop the frozen frame to the element at `rect` and put the element's
    /// description on the clipboard as text: for an issue, or for a coding
    /// agent, which reads the picture from the file the text names. That
    /// file is in the data folder, written over by the next one and removed
    /// when Magnetite quits.
    pub(super) fn capture_context(&self, engine: &Engine, rect: [f64; 4]) {
        engine.close_overlay();
        let Some((c, frame)) = lock(&self.capture).session.take() else {
            return;
        };
        let picture = self.context_picture();
        let (x, y) = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
        let params = json!({
            "expression": format!("({})({x}, {y})", dev::ELEMENT_AT),
            "returnByValue": true,
        });
        engine.devtools(
            c,
            "Runtime.evaluate",
            &params,
            Box::new(move |reply| {
                let element = reply
                    .ok()
                    .map(|r| r["result"]["value"].clone())
                    .filter(serde_json::Value::is_object);
                std::thread::spawn(move || {
                    let result = capture::finish_region(&frame.png, frame.css_width, rect);
                    let saved = (result.as_ref().ok())
                        .filter(|cap| crate::files::write_private(&picture, &cap.png).is_ok())
                        .map(|_| picture.as_path());
                    let text = element.map(|e| dev::element_context(&e, saved));
                    Engine::post(Box::new(move || {
                        let copied = text.is_some_and(|t| Engine::global().copy_text(&t));
                        Self::capture_done_with(result, copied);
                    }));
                });
            }),
        );
    }

    /// Where the picture of an element copied as context is kept.
    pub(super) fn context_picture(&self) -> std::path::PathBuf {
        self.data_dir.join("capture-context.png")
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

    /// A capture set: the page in front in light and dark, each at a
    /// phone's, a tablet's and a computer's size, from the top of the page,
    /// as one picture. The page is shown as each for a moment, then as it
    /// was.
    pub(super) fn capture_set(&self, engine: &Engine) {
        let Some(c) = self.focused_content(None) else {
            return;
        };
        if std::mem::replace(&mut lock(&self.capture).set_running, true) {
            return;
        }
        let what = "light and dark at three sizes";
        Self::ui_call(engine, "captureBusy", &json!({ "what": what }));
        self.capture_set_next(engine, c, Vec::new());
    }

    /// Take the capture set's next picture of page `c`, or finish with the
    /// ones `taken`.
    fn capture_set_next(&self, engine: &Engine, c: ContentId, mut taken: Vec<Vec<u8>>) {
        let (schemes, sizes) = (dev::CAPTURE_SCHEMES, dev::CAPTURE_SIZES);
        let scheme = schemes.get(taken.len() / sizes.len());
        let Some((scheme, (width, height))) = scheme.zip(sizes.get(taken.len() % sizes.len()))
        else {
            return self.capture_set_done(engine, c, Ok(taken));
        };
        let (width, height) = (*width, *height);
        let (shown, _) = self.dev_shown(c);
        let ignore = || Box::new(|_| {});
        let media = shown.media(Some(*scheme));
        engine.devtools(c, "Emulation.setEmulatedMedia", &media, ignore());
        let metrics = json!({
            "width": width, "height": height, "deviceScaleFactor": 1, "mobile": width < 600,
        });
        engine.devtools(c, "Emulation.setDeviceMetricsOverride", &metrics, ignore());
        // The page lays itself out again, then its picture is taken.
        let settle = json!({ "expression": SETTLE, "awaitPromise": true });
        let again = move |taken: Result<Vec<Vec<u8>>, String>| {
            let (Some(app), engine) = (global(), Engine::global()) else {
                return;
            };
            match taken {
                Ok(taken) => app.capture_set_next(engine, c, taken),
                Err(e) => app.capture_set_done(engine, c, Err(e)),
            }
        };
        engine.devtools(
            c,
            "Runtime.evaluate",
            &settle,
            Box::new(move |_| {
                let shot = json!({
                    "format": "png",
                    "captureBeyondViewport": true,
                    "clip": { "x": 0, "y": 0, "width": width, "height": height, "scale": 1 },
                });
                Engine::global().devtools(
                    c,
                    "Page.captureScreenshot",
                    &shot,
                    Box::new(move |shot| {
                        use base64::Engine as _;
                        let png = shot.and_then(|s| {
                            base64::engine::general_purpose::STANDARD
                                .decode(s["data"].as_str().unwrap_or_default())
                                .map_err(|e| e.to_string())
                        });
                        again(png.map(|png| {
                            taken.push(png);
                            taken
                        }));
                    }),
                );
            }),
        );
    }

    /// The capture set of page `c` is taken, or failed: show the page as it
    /// was shown before, and put the pictures on one sheet.
    fn capture_set_done(&self, engine: &Engine, c: ContentId, taken: Result<Vec<Vec<u8>>, String>) {
        let (shown, phone) = self.dev_shown(c);
        let ignore = || Box::new(|_| {});
        let media = shown.media(shown.color_scheme);
        engine.devtools(c, "Emulation.setEmulatedMedia", &media, ignore());
        let clear = json!({});
        engine.devtools(c, "Emulation.clearDeviceMetricsOverride", &clear, ignore());
        if phone {
            for (method, params) in dev::phone_commands(true) {
                engine.devtools(c, method, &params, ignore());
            }
        }
        engine.restore_device(c);
        lock(&self.capture).set_running = false;
        match taken {
            Ok(taken) => {
                std::thread::spawn(move || {
                    let result = capture::finish_sheet(&taken, dev::CAPTURE_SIZES.len());
                    Engine::post(Box::new(move || Self::capture_done(result)));
                });
            }
            Err(e) => Self::capture_done(Err(e)),
        }
    }

    /// What page `c` is shown as, and whether it is the phone of a
    /// responsive split.
    fn dev_shown(&self, c: ContentId) -> (dev::Emulation, bool) {
        self.with_state(|s| {
            s.page_runtime(c)
                .map(|rt| (rt.dev.emulation.clone(), rt.dev.phone))
                .unwrap_or_default()
        })
    }

    /// On the UI thread: keep the new capture (replacing the last) and tell the sidebar.
    pub(super) fn capture_done(result: Result<capture::Capture, String>) {
        Self::capture_done_with(result, false);
    }

    /// `capture_done`; `context`: the element's description was copied too.
    fn capture_done_with(result: Result<capture::Capture, String>, context: bool) {
        let engine = Engine::global();
        match result {
            Ok(cap) => {
                let info = json!({ "width": cap.width, "height": cap.height, "thumb": cap.thumb, "cut_at": cap.cut_at, "context": context });
                if let Some(app) = global() {
                    lock(&app.capture).result = Some(cap);
                }
                Self::ui_call(engine, "captureReady", &info);
            }
            Err(e) => Self::ui_call(engine, "captureFailed", &json!({ "error": e })),
        }
    }
}
