//! Permission prompts ("example.org wants to show notifications") drawn by
//! the app in the notification panel at the page area's top right, instead
//! of Chromium's bubble, which would anchor to the hidden toolbar at the
//! page's top left. CEF hands each prompt over with a callback; the app
//! answers it with [`Engine::answer_permission_prompt`]. Chromium keeps the
//! answer in the page's profile as it would for its own bubble.

use std::cell::RefCell;
use std::collections::HashMap;

use cef::*;
use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::events::{ContentId, EngineEvent};

/// One thing a prompt asks for, named for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptKind {
    Camera,
    /// Move the camera (pan, tilt, zoom).
    CameraControl,
    Microphone,
    Location,
    Notifications,
    Clipboard,
    /// Download several files at once.
    Downloads,
    Midi,
    Fonts,
    /// Know when the user is idle.
    Idle,
    /// Place windows on all displays.
    Windows,
    LocalNetwork,
    Sensors,
    /// Use its cookies while embedded in another site.
    StorageAccess,
    /// Open links of a kind (a protocol handler).
    Links,
    KeyboardLock,
    PointerLock,
    /// Anything else Chromium asks for.
    Other,
}

/// `cef_permission_request_types_t` bits and what they mean here.
const KINDS: [(u32, PromptKind); 19] = [
    (1 << 1, PromptKind::CameraControl),
    (1 << 2, PromptKind::Camera),
    (1 << 4, PromptKind::Clipboard),
    (1 << 5, PromptKind::StorageAccess),
    (1 << 7, PromptKind::Fonts),
    (1 << 8, PromptKind::Location),
    (1 << 11, PromptKind::Idle),
    (1 << 12, PromptKind::Microphone),
    (1 << 13, PromptKind::Midi),
    (1 << 14, PromptKind::Downloads),
    (1 << 15, PromptKind::Notifications),
    (1 << 16, PromptKind::KeyboardLock),
    (1 << 17, PromptKind::PointerLock),
    (1 << 19, PromptKind::Links),
    (1 << 20, PromptKind::StorageAccess),
    (1 << 23, PromptKind::Windows),
    (1 << 25, PromptKind::LocalNetwork),
    (1 << 26, PromptKind::LocalNetwork),
    (1 << 28, PromptKind::Sensors),
];

impl PromptKind {
    /// The kinds in a prompt's bitmask, each once, in the order above; bits
    /// without a name here add one `Other`.
    pub fn from_bits(bits: u32) -> Vec<Self> {
        let mut kinds: Vec<Self> = Vec::new();
        let mut known = 0;
        for (bit, kind) in KINDS {
            known |= bit;
            if bits & bit != 0 && !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
        if bits & !known != 0 {
            kinds.push(Self::Other);
        }
        kinds
    }
}

/// A prompt waiting for the user (`EngineEvent::PermissionPrompt`).
#[derive(Debug, Clone)]
pub struct PermissionPrompt {
    /// CEF's id for the prompt; answers and closes name it.
    pub id: u64,
    /// The page that asked.
    pub content: ContentId,
    /// The origin asking, as CEF gives it (`https://example.org/`).
    pub origin: String,
    pub kinds: Vec<PromptKind>,
}

/// The user's answer to a prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptAnswer {
    Allow,
    Block,
    /// Closed without deciding; the site may ask again.
    Dismiss,
}

thread_local! {
    /// Callbacks of the prompts on display, by prompt id (UI thread only).
    static OPEN: RefCell<HashMap<u64, PermissionPromptCallback>> = RefCell::new(HashMap::new());
}

impl Engine {
    /// Take over the prompt CEF is about to show for page `content`; false
    /// leaves it to Chromium's own bubble.
    pub(crate) fn take_permission_prompt(
        &self,
        content: ContentId,
        id: u64,
        origin: &str,
        bits: u32,
        callback: Option<&mut PermissionPromptCallback>,
    ) -> bool {
        let Some(callback) = callback else {
            return false;
        };
        OPEN.with(|open| open.borrow_mut().insert(id, callback.clone()));
        self.emit(EngineEvent::PermissionPrompt(PermissionPrompt {
            id,
            content,
            origin: origin.to_string(),
            kinds: PromptKind::from_bits(bits),
        }));
        true
    }

    /// Answer prompt `id`; nothing happens if it closed meanwhile.
    pub fn answer_permission_prompt(&self, id: u64, answer: PromptAnswer) {
        let Some(callback) = OPEN.with(|open| open.borrow_mut().remove(&id)) else {
            return;
        };
        callback.cont(match answer {
            PromptAnswer::Allow => PermissionRequestResult::ACCEPT,
            PromptAnswer::Block => PermissionRequestResult::DENY,
            PromptAnswer::Dismiss => PermissionRequestResult::DISMISS,
        });
    }

    /// Prompt `id` is over: answered, or gone with a navigation or its page.
    pub(crate) fn permission_prompt_closed(&self, id: u64) {
        OPEN.with(|open| open.borrow_mut().remove(&id));
        self.emit(EngineEvent::PermissionPromptClosed { id });
    }
}

#[cfg(test)]
mod tests {
    use super::PromptKind;

    #[test]
    fn kinds_come_from_the_bits_each_once() {
        // Camera and microphone (a video call), notifications alone.
        assert_eq!(
            PromptKind::from_bits((1 << 2) | (1 << 12)),
            [PromptKind::Camera, PromptKind::Microphone]
        );
        assert_eq!(PromptKind::from_bits(1 << 15), [PromptKind::Notifications]);
        // Two bits for the same kind give it once.
        assert_eq!(
            PromptKind::from_bits((1 << 25) | (1 << 26)),
            [PromptKind::LocalNetwork]
        );
        // Bits without a name here (an AR session) still say something is asked.
        assert_eq!(
            PromptKind::from_bits((1 << 0) | (1 << 15)),
            [PromptKind::Notifications, PromptKind::Other]
        );
    }
}
