//! Recording a page as a video. The engine sends the page's pictures as it
//! changes; the UI page that started the recording asks for them one by one
//! (`record_frame`), draws them and lets the browser's own encoder make the
//! video, which comes back in pieces (`record_data`) and is written to the
//! Downloads folder. No sound, and no pointer: a page's picture has neither.

use std::fs::File;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use magnetite_engine::{ContentId, Engine, ReplyTicket};
use serde_json::{Value, json};

use crate::{capture, downloads, lock};

use super::{MagnetiteApp, global};

/// The longest a recording runs before it ends by itself.
const LIMIT: Duration = Duration::from_secs(300);
/// How long the recording page has to hand in the video's end.
const END_WITHIN: Duration = Duration::from_secs(5);

/// The user's Downloads folder, where recordings go.
fn downloads_folder() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join("Downloads")).filter(|d| d.is_dir())
}

/// The recording that runs, and the file of the last one.
#[derive(Default)]
pub(super) struct Recorder {
    now: Option<Recording>,
    last: Option<PathBuf>,
}

struct Recording {
    content: ContentId,
    /// The tab recorded, for the UI; a page outside the tabs has none.
    tab: Option<String>,
    started: Instant,
    /// Milliseconds since 1970, for the UI's clock.
    since: u64,
    /// The newest picture nobody asked for yet.
    frame: Option<String>,
    /// The recording page's question for the next picture.
    waiting: Option<ReplyTicket>,
    /// No more pictures: the video's last pieces are on their way.
    stopping: bool,
    file: Option<(PathBuf, File)>,
}

impl Recorder {
    /// For the UI: which tab is recorded, since when.
    pub(super) fn for_ui(&self) -> Value {
        match self.now.as_ref().filter(|r| !r.stopping) {
            Some(r) => json!({ "tab": r.tab, "since": r.since }),
            None => Value::Null,
        }
    }
}

impl MagnetiteApp {
    /// End the recording that runs, or ask for one: the sidebar and the top
    /// bar hear it, and the first to answer (`record_start`) makes the video.
    pub(super) fn record_toggle(&self, engine: &Engine) {
        if lock(&self.record).now.is_some() {
            self.record_stop(engine, None);
        } else {
            Self::ui_call(engine, "record", &json!(null));
        }
    }

    /// Start recording the page in front. The UI page that asks makes the
    /// video (`recorder.ts`).
    pub(super) fn record_start(&self, engine: &Engine) -> Result<Value, String> {
        let c = self.focused_content(None).ok_or("no page to record")?;
        let tab = self.with_state(|s| s.content_to_tab.get(&c).cloned());
        let started = Instant::now();
        {
            let mut recorder = lock(&self.record);
            if recorder.now.is_some() {
                return Err("a recording is running".into());
            }
            let since = SystemTime::now().duration_since(UNIX_EPOCH);
            recorder.now = Some(Recording {
                content: c,
                tab,
                started,
                since: since.map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0)),
                frame: None,
                waiting: None,
                stopping: false,
                file: None,
            });
        }
        let stop = move || {
            if let Some(app) = global() {
                app.record_stop(Engine::global(), Some(started));
            }
        };
        engine.screencast(
            c,
            true,
            Box::new(move |started| {
                if started.is_err() {
                    stop();
                }
            }),
        );
        std::thread::spawn(move || {
            std::thread::sleep(LIMIT);
            Engine::post(Box::new(stop));
        });
        self.push_state(engine);
        Ok(json!({}))
    }

    /// The recording page asks for the next picture: `{ frame }` (a JPEG,
    /// base64) at once or as soon as the page changes, `{ done: true }`
    /// when the recording ended.
    pub(super) fn record_frame(&self, engine: &Engine) -> Value {
        let mut recorder = lock(&self.record);
        let Some(r) = recorder.now.as_mut().filter(|r| !r.stopping) else {
            return json!({ "done": true });
        };
        if let Some(frame) = r.frame.take() {
            return json!({ "frame": frame });
        }
        // One page records: an older question is over.
        if let Some(old) = std::mem::replace(&mut r.waiting, engine.defer_reply()) {
            engine.finish_reply(old, Ok(json!({ "done": true }).to_string()));
        }
        Value::Null
    }

    /// A picture of page `id` arrived (`EngineEvent::ScreencastFrame`).
    pub(super) fn record_picture(&self, engine: &Engine, id: ContentId, jpeg: String) {
        let mut recorder = lock(&self.record);
        let Some(r) = recorder.now.as_mut() else {
            return;
        };
        if r.content != id || r.stopping {
            return;
        }
        match r.waiting.take() {
            Some(ticket) => engine.finish_reply(ticket, Ok(json!({ "frame": jpeg }).to_string())),
            None => r.frame = Some(jpeg),
        }
    }

    /// A piece of the video (`data`, base64), in order; `mp4` says which
    /// kind of file it is, else `WebM`. The first piece makes the file.
    pub(super) fn record_data(&self, data: &str, mp4: bool) -> Result<(), String> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| e.to_string())?;
        let mut recorder = lock(&self.record);
        let r = recorder.now.as_mut().ok_or("no recording is running")?;
        if r.file.is_none() {
            let folder = downloads_folder().ok_or("there is no Downloads folder")?;
            let name = capture::dated_name("recording", if mp4 { "mp4" } else { "webm" });
            let path = folder.join(name);
            let file = File::options().write(true).create_new(true).open(&path);
            r.file = Some((path, file.map_err(|e| e.to_string())?));
        }
        let (_, file) = r.file.as_mut().ok_or("no file")?;
        file.write_all(&bytes).map_err(|e| e.to_string())
    }

    /// End the recording: no more pictures; the recording page hands in the
    /// video's end and calls `record_end`. With `only`, the recording that
    /// began then (a timer's, which may have outlived its own).
    pub(super) fn record_stop(&self, engine: &Engine, only: Option<Instant>) {
        let (content, started) = {
            let mut recorder = lock(&self.record);
            let Some(r) = recorder.now.as_mut().filter(|r| !r.stopping) else {
                return;
            };
            if only.is_some_and(|at| at != r.started) {
                return;
            }
            r.stopping = true;
            r.frame = None;
            if let Some(ticket) = r.waiting.take() {
                engine.finish_reply(ticket, Ok(json!({ "done": true }).to_string()));
            }
            (r.content, r.started)
        };
        engine.screencast(content, false, Box::new(|_| {}));
        self.push_state(engine);
        // A recording page that is gone never answers.
        std::thread::spawn(move || {
            std::thread::sleep(END_WITHIN);
            Engine::post(Box::new(move || {
                if let Some(app) = global() {
                    app.record_end(Engine::global(), Some(started));
                }
            }));
        });
    }

    /// The video is complete (or its page gave up): tell the sidebar where
    /// it is. With `only`, as `record_stop`.
    pub(super) fn record_end(&self, engine: &Engine, only: Option<Instant>) {
        let file = {
            let mut recorder = lock(&self.record);
            if (recorder.now.as_ref()).is_none_or(|r| only.is_some_and(|at| at != r.started)) {
                return;
            }
            let file = recorder
                .now
                .take()
                .and_then(|r| r.file)
                .map(|(path, _)| path);
            recorder.last.clone_from(&file);
            file
        };
        if let Some(name) = file.as_ref().and_then(|f| f.file_name()) {
            let name = name.to_string_lossy();
            Self::ui_call(engine, "recordingReady", &json!({ "name": name }));
        } else {
            let error = "the page showed nothing to record";
            Self::ui_call(engine, "captureFailed", &json!({ "error": error }));
        }
        self.push_state(engine);
    }

    /// Page `id` closed: a recording of it ends.
    pub(super) fn record_page_closed(&self, engine: &Engine, id: ContentId) {
        let ours = (lock(&self.record).now.as_ref()).is_some_and(|r| r.content == id);
        if ours {
            self.record_stop(engine, None);
        }
    }

    /// Put the last recording's file on the clipboard, or show it in the
    /// file manager (`reveal`). Says what happened.
    pub(super) fn record_file(&self, engine: &Engine, reveal: bool) -> &'static str {
        let file = lock(&self.record).last.clone().filter(|f| f.exists());
        match file {
            None => "That file was moved or deleted.",
            Some(f) if reveal => match downloads::open(&f, true) {
                Ok(()) => "",
                Err(_) => "Could not show the file",
            },
            Some(f) if engine.copy_file(&f) => "File copied",
            Some(_) => "Could not copy the file",
        }
    }
}
