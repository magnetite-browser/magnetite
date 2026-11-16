//! Texts of Chromium's own dialogs that Magnetite words differently. Only
//! the screen-share dialog so far: it calls the browser "Chromium", and its
//! tab list leaves out most of the sidebar without saying so.
//!
//! Chromium asks `Strings` for every text it shows, on any thread and in
//! every process, so all but the few texts listed here are turned down at
//! once.

use std::cell::Cell;
use std::ffi::CStr;
use std::os::raw::c_int;
use std::sync::OnceLock;

use cef::*;

/// What Chromium's builds without Google's branding call the browser.
const THEIR_NAME: &str = "Chromium";
const OUR_NAME: &str = "Magnetite";

/// Texts of the screen-share dialog that name the browser: the label of the
/// tab list, and what it says when macOS has not allowed screen recording.
/// Names from CEF's `cef_pack_strings.h`.
const NAMES_THE_BROWSER: [&CStr; 7] = [
    c"IDS_DESKTOP_MEDIA_PICKER_SOURCE_TYPE_TAB",
    c"IDS_DESKTOP_MEDIA_PICKER_SCREEN_PERMISSION_TEXT_MAC",
    c"IDS_DESKTOP_MEDIA_PICKER_SCREEN_PERMISSION_TEXT_PERIOD_MAC",
    c"IDS_DESKTOP_MEDIA_PICKER_WINDOW_PERMISSION_TEXT_MAC",
    c"IDS_DESKTOP_MEDIA_PICKER_WINDOW_PERMISSION_TEXT_PERIOD_MAC",
    c"IDS_DESKTOP_MEDIA_PICKER_PERMISSION_RESTART_TEXT_MAC",
    c"IDS_DESKTOP_MEDIA_PICKER_SYSTEM_AUDIO_PERMISSION_TEXT_MAC",
];

/// The text beside the tab list while no tab is picked.
const TAB_LIST_PROMPT: &CStr = c"IDS_DESKTOP_MEDIA_PICKER_EMPTY_PREVIEW";

/// Added to `TAB_LIST_PROMPT`. The list holds the tabs Chromium has a page
/// for, in the profile of the tab that asks: a tab the sidebar shows but
/// that is not loaded is missing, and the list can be empty.
const TAB_LIST_HINT: &str = "Only loaded tabs of this profile are listed.";

/// The English of `TAB_LIST_PROMPT`. The hint is English, so it is added to
/// this text only and other languages keep Chromium's.
const TAB_LIST_PROMPT_ENGLISH: &str = "Select a tab to share";

/// How a text is changed.
#[derive(Clone, Copy)]
enum Change {
    Rename,
    AddTabHint,
}

/// The numbers the CEF in use gives the texts above (they differ between
/// CEF versions).
fn changes() -> &'static [(c_int, Change)] {
    static CHANGES: OnceLock<Vec<(c_int, Change)>> = OnceLock::new();
    CHANGES.get_or_init(|| {
        // SAFETY: the names are NUL-terminated, and CEF only reads them.
        let id = |name: &CStr| unsafe { sys::cef_id_for_pack_string_name(name.as_ptr()) };
        NAMES_THE_BROWSER
            .iter()
            .map(|name| (id(name), Change::Rename))
            .chain([(id(TAB_LIST_PROMPT), Change::AddTabHint)])
            .collect()
    })
}

/// `text` as Magnetite words it; `None` leaves Chromium's.
fn reword(change: Change, text: &str) -> Option<String> {
    match change {
        Change::Rename => text
            .contains(THEIR_NAME)
            .then(|| text.replace(THEIR_NAME, OUR_NAME)),
        Change::AddTabHint => {
            (text == TAB_LIST_PROMPT_ENGLISH).then(|| format!("{text}. {TAB_LIST_HINT}"))
        }
    }
}

thread_local! {
    /// Set while `Strings` asks Chromium for its own text, which comes back
    /// through `Strings`.
    static ASKING: Cell<bool> = const { Cell::new(false) };
}

/// Chromium's own text for `id`, in the language of its UI.
fn chromium_text(id: c_int) -> Option<String> {
    ASKING.set(true);
    let text =
        resource_bundle_get_global().map(|bundle| CefString::from(&bundle.localized_string(id)));
    ASKING.set(false);
    text.map(|t| t.to_string()).filter(|t| !t.is_empty())
}

wrap_resource_bundle_handler! {
    pub(crate) struct Strings {}

    impl ResourceBundleHandler {
        fn localized_string(&self, string_id: c_int, string: Option<&mut CefString>) -> c_int {
            let Some(&(_, change)) = changes().iter().find(|(id, _)| *id == string_id) else {
                return 0;
            };
            if ASKING.get() {
                return 0;
            }
            let (Some(out), Some(text)) = (string, chromium_text(string_id)) else {
                return 0;
            };
            reword(change, &text).is_some_and(|ours| out.try_set(&ours)).into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browser_is_called_magnetite() {
        assert_eq!(
            reword(Change::Rename, "Chromium Tab").as_deref(),
            Some("Magnetite Tab")
        );
        assert_eq!(reword(Change::Rename, "Window"), None);
    }

    #[test]
    fn the_hint_follows_the_english_prompt_only() {
        assert_eq!(
            reword(Change::AddTabHint, "Select a tab to share").as_deref(),
            Some("Select a tab to share. Only loaded tabs of this profile are listed.")
        );
        assert_eq!(reword(Change::AddTabHint, "Tab zum Teilen auswählen"), None);
    }
}
