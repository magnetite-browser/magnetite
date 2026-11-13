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

use crate::events::Shortcut;

const VK_LEFT: i32 = 0x25;
const VK_RIGHT: i32 = 0x27;
const VK_TAB: i32 = 0x09;

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

/// Map a raw key-down event to a *reserved* shortcut: matched before the page
/// sees the key, so web apps cannot take it (see docs/spec/site-controls.md).
pub fn shortcut_for_key(event: &KeyEvent) -> Option<Shortcut> {
    if event.type_ != KeyEventType::RAWKEYDOWN && event.type_ != KeyEventType::KEYDOWN {
        return None;
    }
    let mods = event.modifiers & (CMD_FLAG | SHIFT | ALT | CTRL);
    let key = event.windows_key_code;
    let upper = |c: char| c as i32;
    match (mods, key) {
        (m, k) if m == CMD && k == upper('T') => Some(Shortcut::NewTab),
        (m, k) if m == CMD && k == upper('W') => Some(Shortcut::CloseTab),
        (m, k) if m == CMD && k == upper('L') => Some(Shortcut::FocusAddress),
        (m, k) if m == (CMD | SHIFT) && k == upper('T') => Some(Shortcut::ReopenClosed),
        (m, k) if m == (CMD | SHIFT) && k == upper('N') => Some(Shortcut::NewSpace),
        // VK_OEM_PLUS: the "=" / "+" key.
        (m, 0xBB) if m == (CMD | SHIFT) => Some(Shortcut::NewSplit),
        (m, k) if m == (CMD | ALT) && k == VK_RIGHT => Some(Shortcut::NextSpace),
        (m, k) if m == (CMD | ALT) && k == VK_LEFT => Some(Shortcut::PrevSpace),
        (m, k) if m == CTRL && k == VK_TAB => Some(Shortcut::NextTab),
        (m, k) if m == (CTRL | SHIFT) && k == VK_TAB => Some(Shortcut::PrevTab),
        (m, k) if m == CMD && (upper('1')..=upper('9')).contains(&k) => {
            u8::try_from(k - upper('0')).ok().map(Shortcut::SelectTab)
        }
        (m, k) if m == SPACE_DIGITS && (upper('1')..=upper('9')).contains(&k) => {
            u8::try_from(k - upper('0')).ok().map(Shortcut::SelectSpace)
        }
        _ => None,
    }
}

/// Magnetite shortcuts a page may claim for itself: they are matched only after
/// the page had the key and did not handle it (`preventDefault`).
pub fn yieldable_shortcut(event: &KeyEvent) -> Option<Shortcut> {
    if event.type_ != KeyEventType::RAWKEYDOWN && event.type_ != KeyEventType::KEYDOWN {
        return None;
    }
    let mods = event.modifiers & (CMD_FLAG | SHIFT | ALT | CTRL);
    match (mods, event.windows_key_code) {
        (m, k) if m == CMD && k == 'S' as i32 => Some(Shortcut::ToggleSidebar),
        (m, k) if m == CMD && k == 'D' as i32 => Some(Shortcut::TogglePin),
        (m, k) if m == (CMD | SHIFT) && k == 'C' as i32 => Some(Shortcut::CopyLink),
        (m, k) if m == (CMD | SHIFT) && k == 'O' as i32 => Some(Shortcut::SitePanel),
        (m, k) if m == (CMD | SHIFT) && k == '2' as i32 => Some(Shortcut::Capture),
        _ => None,
    }
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

/// Map a Chrome command (by IDC name) to a shortcut.
pub fn shortcut_for_chrome_command(command_id: i32) -> Option<Shortcut> {
    const TABLE: &[(&str, Shortcut)] = &[
        ("IDC_NEW_TAB", Shortcut::NewTab),
        ("IDC_NEW_WINDOW", Shortcut::NewTab),
        ("IDC_CLOSE_TAB", Shortcut::CloseTab),
        ("IDC_CLOSE_WINDOW", Shortcut::CloseTab),
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
    TABLE
        .iter()
        .find(|(name, _)| command_id_for(name) == Some(command_id))
        .map(|(_, s)| *s)
}

pub(crate) fn command_id_for(name: &str) -> Option<i32> {
    let c = std::ffi::CString::new(name).ok()?;
    // SAFETY: valid NUL-terminated string; CEF only reads it.
    let id = unsafe { sys::cef_id_for_command_id_name(c.as_ptr()) };
    (id > 0).then_some(id)
}
