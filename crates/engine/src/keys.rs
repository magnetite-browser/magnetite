//! Keyboard shortcut mapping for content windows.
//!
//! Chrome-style browsers route their built-in accelerators (Cmd+T, Cmd+W, …;
//! Ctrl outside macOS) through `CefCommandHandler::OnChromeCommand`; we
//! intercept those by IDC name. Keys Chrome does not bind reach our
//! `KeyboardHandler` (`client.rs`: before the page sees them, and after it
//! left them unhandled; a content window's own `OnKeyEvent` gets the few
//! that reach the window) and are matched here by key code and modifiers.

use cef::sys;
use cef::*;

use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

use crate::events::Shortcut;
use crate::keymap::{Action, Combo, Overrides};

// The flags are `u32` on macOS and Linux and `i32` on Windows.
#[allow(clippy::unnecessary_cast, clippy::cast_sign_loss)]
const CMD_FLAG: u32 = sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN.0 as u32;
#[allow(clippy::unnecessary_cast, clippy::cast_sign_loss)]
const SHIFT: u32 = sys::cef_event_flags_t::EVENTFLAG_SHIFT_DOWN.0 as u32;
#[allow(clippy::unnecessary_cast, clippy::cast_sign_loss)]
const ALT: u32 = sys::cef_event_flags_t::EVENTFLAG_ALT_DOWN.0 as u32;
#[allow(clippy::unnecessary_cast, clippy::cast_sign_loss)]
const CTRL: u32 = sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN.0 as u32;
/// The shortcut modifier: Cmd on macOS, Ctrl elsewhere.
const CMD: u32 = if cfg!(target_os = "macos") {
    CMD_FLAG
} else {
    CTRL
};
/// The modifier of the space digits: Ctrl on macOS. Elsewhere Ctrl is the
/// shortcut modifier and its digits select tabs, so the spaces get Alt.
const SPACE_DIGITS: u32 = if cfg!(target_os = "macos") { CTRL } else { ALT };

/// The keys of the actions as the user has them (Settings → Shortcuts), and
/// until when shortcuts are off because Settings records new keys.
static KEYS: RwLock<Option<Overrides>> = RwLock::new(None);
static PAUSED_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);

/// Use these keys from now on.
fn set_overrides(overrides: &Overrides) {
    if let Ok(mut keys) = KEYS.write() {
        *keys = Some(overrides.clone());
    }
}

/// Stop matching shortcuts until `until` (`None`: match again), so the
/// keys pressed reach the Settings page that records them.
fn pause(until: Option<Instant>) {
    if let Ok(mut paused) = PAUSED_UNTIL.lock() {
        *paused = until;
    }
}

pub fn paused() -> bool {
    (PAUSED_UNTIL.lock().ok())
        .and_then(|until| *until)
        .is_some_and(|until| Instant::now() < until)
}

/// How long Settings may hold shortcuts off for one recording. The page
/// ends it itself; this is for a page that went away meanwhile.
const RECORDING: Duration = Duration::from_secs(20);

impl crate::Engine {
    /// Use the user's keys for the shortcuts from now on (Settings →
    /// Shortcuts), in the pages and in the menu bar.
    pub fn set_shortcut_keys(&self, overrides: &Overrides) {
        set_overrides(overrides);
        #[cfg(target_os = "macos")]
        crate::menubar::apply_keys();
    }

    /// While Settings records new keys (`recording`) no shortcut runs, so
    /// the keys reach the page. It ends by itself after `RECORDING`.
    pub fn record_shortcut_keys(&self, recording: bool) {
        let until = recording.then(|| Instant::now() + RECORDING);
        pause(until);
        #[cfg(target_os = "macos")]
        crate::menubar::apply_keys();
        if let Some(until) = until {
            let end = move || {
                // Still this recording, not a later one.
                if PAUSED_UNTIL.lock().is_ok_and(|p| *p == Some(until)) {
                    crate::Engine::global().record_shortcut_keys(false);
                }
            };
            let wait = i64::try_from(RECORDING.as_millis()).unwrap_or(i64::MAX);
            Self::post_delayed(Box::new(end), wait);
        }
    }
}

/// The keys of `action` now.
pub fn combo_of(action: Action) -> Option<Combo> {
    match KEYS.read().ok()?.as_ref() {
        Some(keys) => keys.combo_of(action),
        None => action.default_combo(),
    }
}

fn action_for(combo: Combo) -> Option<Action> {
    match KEYS.read().ok()?.as_ref() {
        Some(keys) => keys.action_for(combo),
        None => Overrides::default().action_for(combo),
    }
}

/// A key-down event's key and modifiers.
fn pressed(event: &KeyEvent) -> Option<Combo> {
    if event.type_ != KeyEventType::RAWKEYDOWN && event.type_ != KeyEventType::KEYDOWN {
        return None;
    }
    let held = |flag: u32| event.modifiers & flag != 0;
    let mut combo = Combo::of_key(event.windows_key_code)?;
    combo.primary = held(CMD);
    // Outside macOS Ctrl is the shortcut modifier, and the only Control key.
    combo.ctrl = cfg!(target_os = "macos") && held(CTRL);
    combo.alt = held(ALT);
    combo.shift = held(SHIFT);
    Some(combo)
}

/// Map a raw key-down event to a *reserved* shortcut: matched before the page
/// sees the key, so web apps cannot take it (see docs/spec/site-controls.md).
pub fn shortcut_for_key(event: &KeyEvent) -> Option<Shortcut> {
    let combo = pressed(event).filter(|_| !paused())?;
    let mods = event.modifiers & (CMD_FLAG | SHIFT | ALT | CTRL);
    let key = event.windows_key_code;
    // The digits are fixed: a tab or a space by its place.
    if (i32::from(b'1')..=i32::from(b'9')).contains(&key) && (mods == CMD || mods == SPACE_DIGITS) {
        let n = u8::try_from(key - i32::from(b'0')).ok()?;
        return Some(if mods == CMD {
            Shortcut::SelectTab(n)
        } else {
            Shortcut::SelectSpace(n)
        });
    }
    action_for(combo)
        .filter(|a| a.reserved())
        .map(Action::shortcut)
}

/// Magnetite shortcuts a page may claim for itself: they are matched only after
/// the page had the key and did not handle it (`preventDefault`).
pub fn yieldable_shortcut(event: &KeyEvent) -> Option<Shortcut> {
    let combo = pressed(event).filter(|_| !paused())?;
    action_for(combo)
        .filter(|a| !a.reserved())
        .map(Action::shortcut)
}

/// Chromium's own zoom commands; we let Chromium zoom and then report it.
pub fn is_zoom_command(command_id: i32) -> bool {
    ["IDC_ZOOM_PLUS", "IDC_ZOOM_MINUS", "IDC_ZOOM_NORMAL"]
        .iter()
        .any(|name| command_id_for(name) == Some(command_id))
}

/// Keys that only mean something inside the peek window (and `Cmd+O` in a
/// mini window: "Open in").
pub fn peek_shortcut(event: &KeyEvent) -> Option<Shortcut> {
    if event.type_ != KeyEventType::RAWKEYDOWN && event.type_ != KeyEventType::KEYDOWN {
        return None;
    }
    let mods = event.modifiers & (CMD_FLAG | SHIFT | ALT | CTRL);
    match (mods, event.windows_key_code) {
        (0, 0x1B) => Some(Shortcut::ClosePeek),
        (m, k) if m == CMD && k == 'O' as i32 => Some(Shortcut::PeekToTab),
        _ => None,
    }
}

/// What to do with a command Chromium runs for one of its own keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeCommand {
    /// Not ours: Chromium goes on with it.
    Pass,
    /// The keys belong to no action any more: nothing happens.
    Drop,
    Run(Shortcut),
}

/// Map a Chrome command (by IDC name) to a shortcut. Chromium's keys are
/// fixed, ours are not: the command stands for the default keys of its
/// action, and what runs is the action that has those keys now.
pub fn shortcut_for_chrome_command(command_id: i32) -> ChromeCommand {
    const TABLE: &[(&str, Shortcut)] = &[
        ("IDC_NEW_TAB", Shortcut::NewTab),
        ("IDC_NEW_WINDOW", Shortcut::NewWindow),
        ("IDC_CLOSE_TAB", Shortcut::CloseTab),
        ("IDC_CLOSE_WINDOW", Shortcut::CloseWindow),
        ("IDC_FOCUS_LOCATION", Shortcut::FocusAddress),
        ("IDC_FOCUS_SEARCH", Shortcut::FocusAddress),
        ("IDC_SAVE_PAGE", Shortcut::ToggleSidebar),
        ("IDC_SHOW_DOWNLOADS", Shortcut::Downloads),
        ("IDC_BOOKMARK_THIS_TAB", Shortcut::TogglePin),
        ("IDC_RESTORE_TAB", Shortcut::ReopenClosed),
        ("IDC_NEW_INCOGNITO_WINDOW", Shortcut::NewSpace),
        ("IDC_SELECT_NEXT_TAB", Shortcut::NextTab),
        ("IDC_SELECT_PREVIOUS_TAB", Shortcut::PrevTab),
        ("IDC_SELECT_TAB_0", Shortcut::SelectTab(1)),
        ("IDC_SELECT_TAB_1", Shortcut::SelectTab(2)),
        ("IDC_SELECT_TAB_2", Shortcut::SelectTab(3)),
        ("IDC_SELECT_TAB_3", Shortcut::SelectTab(4)),
        ("IDC_SELECT_TAB_4", Shortcut::SelectTab(5)),
        ("IDC_SELECT_TAB_5", Shortcut::SelectTab(6)),
        ("IDC_SELECT_TAB_6", Shortcut::SelectTab(7)),
        ("IDC_SELECT_TAB_7", Shortcut::SelectTab(8)),
        ("IDC_SELECT_LAST_TAB", Shortcut::SelectTab(9)),
        ("IDC_COPY_URL", Shortcut::CopyLink),
        ("IDC_PRINT", Shortcut::Print),
        ("IDC_SHOW_HISTORY", Shortcut::History),
        // DevTools open through us, so they dock and get our menu items.
        ("IDC_DEV_TOOLS", Shortcut::ToggleDevTools),
        ("IDC_DEV_TOOLS_TOGGLE", Shortcut::ToggleDevTools),
        ("IDC_DEV_TOOLS_CONSOLE", Shortcut::OpenDevTools),
        ("IDC_DEV_TOOLS_INSPECT", Shortcut::OpenDevTools),
    ];
    let Some((_, shortcut)) = TABLE
        .iter()
        .find(|(name, _)| command_id_for(name) == Some(command_id))
    else {
        return ChromeCommand::Pass;
    };
    let Some(keys) = Action::of(*shortcut).and_then(Action::default_combo) else {
        return ChromeCommand::Run(*shortcut);
    };
    match action_for(keys).filter(|_| !paused()) {
        Some(action) => ChromeCommand::Run(action.shortcut()),
        None => ChromeCommand::Drop,
    }
}

pub(crate) fn command_id_for(name: &str) -> Option<i32> {
    let c = std::ffi::CString::new(name).ok()?;
    // SAFETY: valid NUL-terminated string; CEF only reads it.
    let id = unsafe { sys::cef_id_for_command_id_name(c.as_ptr()) };
    (id > 0).then_some(id)
}
