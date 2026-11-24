//! Recording a page as a video. The engine sends the page's pictures as it
//! changes; the UI page that started the recording asks for them one by one
//! (`record_frame`), draws them and lets the browser's own encoder make the
//! video, which comes back in pieces (`record_data`) and is written to the
//! Downloads folder. No sound, and no pointer: a page's picture has neither.

use std::fs::File;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use magnetite_engine::{ContentId, Engine, ReplyTicket};
use magnetite_ui::t;
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
    /// How many recordings were started: each one's id.
    started: u64,
}

struct Recording {
    /// Told to the page that makes the video, which names it with every
    /// command: what a recording that is over still sends reaches no other.
    id: u64,
    content: ContentId,
    /// The tab recorded, for the UI; a page outside the tabs has none.
    tab: Option<String>,
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
    /// video (`recorder.ts`) and gets the recording's `id`.
    pub(super) fn record_start(&self, engine: &Engine) -> Result<Value, String> {
        let c = self.focused_content(None).ok_or("no page to record")?;
        let tab = self.with_state(|s| s.content_to_tab.get(&c).cloned());
        let id = {
            let mut recorder = lock(&self.record);
            if recorder.now.is_some() {
                return Err("a recording is running".into());
            }
            recorder.started += 1;
            let since = SystemTime::now().duration_since(UNIX_EPOCH);
            recorder.now = Some(Recording {
                id: recorder.started,
                content: c,
                tab,
                since: since.map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0)),
                frame: None,
                waiting: None,
                stopping: false,
                file: None,
            });
            recorder.started
        };
        let stop = move || {
            if let Some(app) = global() {
                app.record_stop(Engine::global(), Some(id));
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
        Ok(json!({ "id": id }))
    }

    /// The page making recording `id` asks for the next picture:
    /// `{ frame }` (a JPEG, base64) at once or as soon as the page changes,
    /// `{ done: true }` when that recording ended.
    pub(super) fn record_frame(&self, engine: &Engine, id: u64) -> Value {
        let mut recorder = lock(&self.record);
        let now = recorder.now.as_mut().filter(|r| r.id == id && !r.stopping);
        let Some(r) = now else {
            return json!({ "done": true });
        };
        if let Some(frame) = r.frame.take() {
            return json!({ "frame": frame });
        }
        // One question at a time: an older one is over.
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

    /// A piece of the video of recording `id` (`data`, base64), in order;
    /// `mp4` says which kind of file it is, else `WebM`. The first piece
    /// makes the file.
    pub(super) fn record_data(&self, id: u64, data: &str, mp4: bool) -> Result<(), String> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| e.to_string())?;
        let mut recorder = lock(&self.record);
        let now = recorder.now.as_mut().filter(|r| r.id == id);
        let r = now.ok_or("that recording is over")?;
        if r.file.is_none() {
            let folder = downloads_folder().ok_or("there is no Downloads folder")?;
            let name = capture::recording_name(if mp4 { "mp4" } else { "webm" });
            let path = folder.join(name);
            let file = File::options().write(true).create_new(true).open(&path);
            r.file = Some((path, file.map_err(|e| e.to_string())?));
        }
        let (_, file) = r.file.as_mut().ok_or("no file")?;
        file.write_all(&bytes).map_err(|e| e.to_string())
    }

    /// End the recording: no more pictures; the recording page hands in the
    /// video's end and calls `record_end`. With `only`, the recording of
    /// that id (a timer's, which may have outlived its own).
    pub(super) fn record_stop(&self, engine: &Engine, only: Option<u64>) {
        let (content, id) = {
            let mut recorder = lock(&self.record);
            let Some(r) = recorder.now.as_mut().filter(|r| !r.stopping) else {
                return;
            };
            if only.is_some_and(|id| id != r.id) {
                return;
            }
            r.stopping = true;
            r.frame = None;
            if let Some(ticket) = r.waiting.take() {
                engine.finish_reply(ticket, Ok(json!({ "done": true }).to_string()));
            }
            (r.content, r.id)
        };
        engine.screencast(content, false, Box::new(|_| {}));
        self.push_state(engine);
        // A recording page that is gone, or too slow, never hands in the
        // video's end: what is in the file by then is not the whole video.
        std::thread::spawn(move || {
            std::thread::sleep(END_WITHIN);
            Engine::post(Box::new(move || {
                if let Some(app) = global() {
                    app.record_end(Engine::global(), id, true);
                }
            }));
        });
    }

    /// Recording `id` is over: its video is complete, or (`failed`) the
    /// page making it could not go on. Tell the sidebar where the file is,
    /// or that it is not whole. A recording that was not stopped before
    /// (its page gave up) stops sending pictures here.
    pub(super) fn record_end(&self, engine: &Engine, id: u64, failed: bool) {
        let ended = {
            let mut recorder = lock(&self.record);
            if recorder.now.as_ref().is_none_or(|r| r.id != id) {
                return;
            }
            let ended = recorder.now.take();
            recorder.last = (ended.as_ref())
                .and_then(|r| r.file.as_ref())
                .map(|(path, _)| path.clone());
            ended
        };
        let Some(ended) = ended else { return };
        if let Some(ticket) = ended.waiting {
            engine.finish_reply(ticket, Ok(json!({ "done": true }).to_string()));
        }
        if !ended.stopping {
            engine.screencast(ended.content, false, Box::new(|_| {}));
        }
        let name = (ended.file.as_ref())
            .and_then(|(path, _)| path.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        match name {
            Some(name) if !failed => {
                Self::ui_call(engine, "recordingReady", &json!({ "name": name }));
            }
            Some(name) => {
                let error = magnetite_ui::t!("app.capture.video_cut", name = name);
                Self::ui_call(engine, "captureFailed", &json!({ "error": error }));
            }
            None => {
                let error = magnetite_ui::t!("app.capture.nothing_recorded");
                Self::ui_call(engine, "captureFailed", &json!({ "error": error }));
            }
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
            None => t!("app.notices.file_gone"),
            Some(f) if reveal => match downloads::open(&f, true) {
                Ok(()) => "",
                Err(_) => t!("app.notices.show_file_failed"),
            },
            Some(f) if engine.copy_file(&f) => t!("app.notices.file_copied"),
            Some(_) => t!("app.notices.copy_file_failed"),
        }
    }
}
