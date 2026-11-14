//! Media playing in web pages, for the sidebar's media controls.
//!
//! Renderer: `media_hook.js` runs in each web page's main frame before the
//! page's scripts. It reports the playing element and the page's Media
//! Session metadata through a native `report` function
//! (`REPORT_MESSAGE`), and returns a `control` function that the renderer
//! keeps per browser and calls on `CONTROL_MESSAGE`.
//!
//! Browser: reports become `EngineEvent::MediaChanged`. Entering
//! picture-in-picture needs a user gesture, so it goes through the `DevTools`
//! protocol (`Runtime.evaluate` with `userGesture`) instead of the page script.

use std::cell::RefCell;
use std::collections::HashMap;

use cef::*;
use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::events::{ContentId, EngineEvent};

pub const REPORT_MESSAGE: &str = "magnetite.media";
pub const CONTROL_MESSAGE: &str = "magnetite.media.control";
const HOOK_JS: &str = include_str!("media_hook.js");
/// Longest title/artist we pass on; page-controlled text.
const MAX_TEXT: usize = 200;
/// Longest report taken from a page: the fields above with room to spare.
const MAX_REPORT: usize = 4096;

/// What a page is playing, as reported by its main frame.
#[allow(clippy::struct_excessive_bools)] // flat wire format shared with the UI
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaState {
    pub playing: bool,
    pub title: String,
    pub artist: String,
    /// The element is a video with a picture (picture-in-picture possible).
    pub video: bool,
    /// The page currently shows picture-in-picture.
    pub pip: bool,
    /// A stream without an end (live video): nothing to seek in.
    pub live: bool,
    pub can_next: bool,
    pub can_prev: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaAction {
    Play,
    Pause,
    Next,
    Previous,
    /// 15 seconds forward or back in the playing element.
    SeekForward,
    SeekBackward,
}

impl MediaAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Play => "play",
            Self::Pause => "pause",
            Self::Next => "next",
            Self::Previous => "previous",
            Self::SeekForward => "seekforward",
            Self::SeekBackward => "seekbackward",
        }
    }
}

/// Parse a report from the page script. `null` means nothing plays any more.
pub fn parse_report(json: &str) -> Result<Option<MediaState>, serde_json::Error> {
    let mut state: Option<MediaState> = serde_json::from_str(json)?;
    if let Some(s) = state.as_mut() {
        s.title = s.title.chars().take(MAX_TEXT).collect();
        s.artist = s.artist.chars().take(MAX_TEXT).collect();
    }
    Ok(state)
}

impl Engine {
    /// A page's media report (untrusted: the page can shape it).
    pub(crate) fn on_media_report(&self, browser: Option<&Browser>, json: &str) {
        if json.len() > MAX_REPORT {
            return;
        }
        let Some(id) = self.content_id_for(browser) else {
            return;
        };
        if let Ok(media) = parse_report(json) {
            #[cfg(target_os = "macos")]
            crate::pip::follow(id, media.as_ref());
            // Chromium's video window has a taskbar button of its own.
            #[cfg(target_os = "windows")]
            if media.as_ref().is_some_and(|m| m.pip) {
                crate::win::brand_video_window();
            }
            self.emit(EngineEvent::MediaChanged { id, media });
        }
    }

    /// Play, pause, skip or seek in content `id` (through the page's own
    /// handlers where it has them).
    pub fn content_media(&self, id: ContentId, action: MediaAction) {
        let Some(frame) = self.content_main_frame(id) else {
            return;
        };
        let Some(mut msg) = process_message_create(Some(&CefString::from(CONTROL_MESSAGE))) else {
            return;
        };
        if let Some(args) = msg.argument_list() {
            args.set_string(0, Some(&CefString::from(action.as_str())));
        }
        frame.send_process_message(ProcessId::RENDERER, Some(&mut msg));
    }

    /// Mute page `id` or let it sound again. A page that has no browser yet
    /// (just asked for, or waiting for its profile) is muted once it has one.
    pub fn content_set_muted(&self, id: ContentId, muted: bool) {
        {
            let mut inner = self.lock();
            if muted {
                inner.muted.insert(id);
            } else {
                inner.muted.remove(&id);
            }
        }
        if let Some(host) = self.content_browser_host(id) {
            host.set_audio_muted(i32::from(muted));
        }
    }

    /// Enter or leave picture-in-picture for the page's playing video.
    pub fn content_picture_in_picture(&self, id: ContentId, enter: bool) {
        const ENTER: &str = "(() => { const vs = [...document.querySelectorAll('video')]; \
            const v = vs.find(v => !v.paused && v.readyState > 1) || vs.find(v => v.readyState > 1); \
            if (!v || v.disablePictureInPicture || document.pictureInPictureElement) return false; \
            return v.requestPictureInPicture().then(() => true, () => false); })()";
        const LEAVE: &str = "document.pictureInPictureElement ? document.exitPictureInPicture().then(() => true, () => false) : false";
        #[cfg(target_os = "macos")]
        if enter {
            crate::pip::requested(id);
        }
        // Only entering needs the user's gesture; leaving must not hand the
        // page one.
        self.evaluate_in_page(id, if enter { ENTER } else { LEAVE }, enter);
    }

    /// Leave picture-in-picture and pause the video (Close on its controls).
    #[cfg(target_os = "macos")]
    pub(crate) fn content_close_picture_in_picture(&self, id: ContentId) {
        const CLOSE: &str = "(() => { const v = document.pictureInPictureElement; if (!v) return false; \
            return document.exitPictureInPicture().then(() => { v.pause(); return true; }, () => false); })()";
        self.evaluate_in_page(id, CLOSE, false);
    }

    /// Evaluate `expression` in page `id`; with `gesture`, as if the user
    /// had just clicked (entering picture-in-picture needs that). Goes
    /// through the `DevTools` protocol in-process (`Runtime.evaluate`, its
    /// `userGesture` set from `gesture`).
    fn evaluate_in_page(&self, id: ContentId, expression: &str, gesture: bool) {
        let Some(host) = self.content_browser_host(id) else {
            return;
        };
        let Some(mut params) = dictionary_value_create() else {
            return;
        };
        params.set_string(
            Some(&CefString::from("expression")),
            Some(&CefString::from(expression)),
        );
        params.set_bool(Some(&CefString::from("userGesture")), i32::from(gesture));
        host.execute_dev_tools_method(
            0,
            Some(&CefString::from("Runtime.evaluate")),
            Some(&mut params),
        );
    }

    fn content_main_frame(&self, id: ContentId) -> Option<Frame> {
        self.lock().contents.get(&id)?.browser.clone()?.main_frame()
    }

    pub(crate) fn content_browser_host(&self, id: ContentId) -> Option<BrowserHost> {
        self.lock().contents.get(&id)?.browser.clone()?.host()
    }
}

// ---- renderer process ----

thread_local! {
    /// Per browser: the main frame's context and its `control` function.
    static CONTROLS: RefCell<HashMap<i32, (V8Context, V8Value)>> = RefCell::new(HashMap::new());
}

fn is_web_url(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://") || url.starts_with("file://")
}

/// Install the page script in a web page's main frame. Called from
/// `on_context_created`, before any page script runs.
pub(crate) fn install(browser: &Browser, frame: &Frame, context: &V8Context) {
    if frame.is_main() == 0 || !is_web_url(&CefString::from(&frame.url()).to_string()) {
        return;
    }
    // CEF calls this for every script world of the frame, also the separate
    // ones extensions run their content scripts in. The hook belongs in the
    // page's own: installed again elsewhere it would take over `CONTROLS`
    // with a copy that knows none of the page's handlers.
    let own_world = frame
        .v8_context()
        .is_some_and(|main| main.is_same(Some(&mut context.clone())) != 0);
    if !own_world {
        return;
    }
    let mut handler = MediaReport::new();
    let Some(report) =
        v8_value_create_function(Some(&CefString::from("report")), Some(&mut handler))
    else {
        return;
    };
    if context.enter() == 0 {
        return;
    }
    let mut factory = None;
    let mut exception = None;
    let ok = context.eval(
        Some(&CefString::from(HOOK_JS)),
        None,
        1,
        Some(&mut factory),
        Some(&mut exception),
    ) != 0;
    let control = factory
        .filter(|_| ok)
        .and_then(|f| f.execute_function(None, Some(&[Some(report)])));
    context.exit();
    if let Some(control) = control {
        CONTROLS.with(|c| {
            c.borrow_mut()
                .insert(browser.identifier(), (context.clone(), control))
        });
    }
}

pub(crate) fn release(browser: &Browser, context: &V8Context) {
    CONTROLS.with(|c| {
        let mut map = c.borrow_mut();
        let id = browser.identifier();
        if map
            .get(&id)
            .is_some_and(|(ctx, _)| ctx.is_same(Some(&mut context.clone())) != 0)
        {
            map.remove(&id);
        }
    });
}

/// Handle `CONTROL_MESSAGE` in the renderer. Returns true when consumed.
pub(crate) fn on_message(browser: &Browser, message: &ProcessMessage) -> bool {
    if CefString::from(&message.name()).to_string() != CONTROL_MESSAGE {
        return false;
    }
    let action = message
        .argument_list()
        .map(|a| CefString::from(&a.string(0)).to_string())
        .unwrap_or_default();
    // Taken out of `CONTROLS` first: the page's handler can remove a frame,
    // and `release` then needs the map while the handler still runs.
    let entry = CONTROLS.with(|c| c.borrow().get(&browser.identifier()).cloned());
    if let Some((context, control)) = entry
        && context.is_valid() != 0
        && context.enter() != 0
    {
        let arg = v8_value_create_string(Some(&CefString::from(action.as_str())));
        control.execute_function(None, Some(&[arg]));
        context.exit();
    }
    true
}

wrap_v8_handler! {
    struct MediaReport {}
    impl V8Handler {
        fn execute(
            &self,
            _name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            _retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> ::std::os::raw::c_int {
            let json = arguments
                .and_then(|a| a.first().cloned().flatten())
                .filter(|v| v.is_string() != 0)
                .map(|v| CefString::from(&v.string_value()).to_string());
            let Some(json) = json.filter(|j| j.len() <= MAX_REPORT) else { return 1 };
            let Some(frame) = v8_context_get_current_context().and_then(|c| c.frame()) else { return 1 };
            let Some(mut msg) = process_message_create(Some(&CefString::from(REPORT_MESSAGE))) else { return 1 };
            if let Some(args) = msg.argument_list() {
                args.set_string(0, Some(&CefString::from(json.as_str())));
            }
            frame.send_process_message(ProcessId::BROWSER, Some(&mut msg));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_parse_and_are_capped() {
        assert!(parse_report("null").unwrap().is_none());
        let long = "x".repeat(500);
        let json = format!(
            r#"{{"playing":true,"title":"{long}","artist":"A","video":false,"pip":false,"live":false,"can_next":true,"can_prev":false}}"#
        );
        let state = parse_report(&json).unwrap().unwrap();
        assert!(state.playing && state.can_next);
        assert_eq!(state.title.len(), MAX_TEXT);
        assert!(parse_report("{\"nonsense\":1}").is_err());
    }
}
