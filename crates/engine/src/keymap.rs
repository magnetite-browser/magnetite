//! Which keys run which command: the defaults, and what the user changed
//! (Settings → Shortcuts). Free of CEF: `keys.rs` turns key events into
//! `Combo`s and asks the `Keymap`.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::de::IntoDeserializer as _;
use serde::{Deserialize, Serialize};

use crate::events::Shortcut;

const MAC: bool = cfg!(target_os = "macos");

/// A command whose keys the user can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    NewTab,
    FocusAddress,
    CloseTab,
    ReopenClosed,
    NextTab,
    PrevTab,
    TogglePin,
    NewSplit,
    NewWindow,
    CloseWindow,
    NewSpace,
    PrivateSpace,
    ClosePrivateSpace,
    NextSpace,
    PrevSpace,
    ToggleSidebar,
    SitePanel,
    CopyLink,
    Capture,
    Find,
    FindNext,
    FindPrevious,
    Print,
    History,
    Downloads,
    Settings,
    ToggleDevTools,
}

impl Action {
    /// Every action, in the order Settings lists them. Of two actions on
    /// the same keys the earlier one gets them.
    pub const ALL: [Self; 27] = [
        Self::NewTab,
        Self::FocusAddress,
        Self::CloseTab,
        Self::ReopenClosed,
        Self::NextTab,
        Self::PrevTab,
        Self::TogglePin,
        Self::NewSplit,
        Self::NewWindow,
        Self::CloseWindow,
        Self::NewSpace,
        Self::PrivateSpace,
        Self::ClosePrivateSpace,
        Self::NextSpace,
        Self::PrevSpace,
        Self::ToggleSidebar,
        Self::SitePanel,
        Self::CopyLink,
        Self::Capture,
        Self::Find,
        Self::FindNext,
        Self::FindPrevious,
        Self::Print,
        Self::History,
        Self::Downloads,
        Self::Settings,
        Self::ToggleDevTools,
    ];

    /// The command the action runs.
    pub fn shortcut(self) -> Shortcut {
        match self {
            Self::NewTab => Shortcut::NewTab,
            Self::FocusAddress => Shortcut::FocusAddress,
            Self::CloseTab => Shortcut::CloseTab,
            Self::ReopenClosed => Shortcut::ReopenClosed,
            Self::NextTab => Shortcut::NextTab,
            Self::PrevTab => Shortcut::PrevTab,
            Self::TogglePin => Shortcut::TogglePin,
            Self::NewSplit => Shortcut::NewSplit,
            Self::NewWindow => Shortcut::NewWindow,
            Self::CloseWindow => Shortcut::CloseWindow,
            Self::NewSpace => Shortcut::NewSpace,
            Self::PrivateSpace => Shortcut::PrivateSpace,
            Self::ClosePrivateSpace => Shortcut::ClosePrivateSpace,
            Self::NextSpace => Shortcut::NextSpace,
            Self::PrevSpace => Shortcut::PrevSpace,
            Self::ToggleSidebar => Shortcut::ToggleSidebar,
            Self::SitePanel => Shortcut::SitePanel,
            Self::CopyLink => Shortcut::CopyLink,
            Self::Capture => Shortcut::Capture,
            Self::Find => Shortcut::Find,
            Self::FindNext => Shortcut::FindNext,
            Self::FindPrevious => Shortcut::FindPrevious,
            Self::Print => Shortcut::Print,
            Self::History => Shortcut::History,
            Self::Downloads => Shortcut::Downloads,
            Self::Settings => Shortcut::Settings,
            Self::ToggleDevTools => Shortcut::ToggleDevTools,
        }
    }

    /// The action that runs `shortcut`, if its keys can be changed.
    pub fn of(shortcut: Shortcut) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.shortcut() == shortcut)
    }

    /// Taken before the page sees the key, so a web app cannot have it. The
    /// others reach the page first and run only if it leaves them alone
    /// (docs/spec/site-controls.md).
    pub fn reserved(self) -> bool {
        matches!(
            self,
            Self::NewTab
                | Self::FocusAddress
                | Self::CloseTab
                | Self::ReopenClosed
                | Self::NextTab
                | Self::PrevTab
                | Self::NewSplit
                | Self::NewWindow
                | Self::CloseWindow
                | Self::NewSpace
                | Self::PrivateSpace
                | Self::ClosePrivateSpace
                | Self::NextSpace
                | Self::PrevSpace
        )
    }

    /// The keys the action has until the user changes them, on macOS (`mac`)
    /// or elsewhere, as saved text (`Combo`'s format). The keys Chromium has
    /// for the same command where the two differ by system.
    fn default_keys(self, mac: bool) -> Option<&'static str> {
        Some(match self {
            Self::NewTab => "primary+T",
            Self::FocusAddress => "primary+L",
            Self::CloseTab => "primary+W",
            Self::ReopenClosed => "primary+shift+T",
            Self::NextTab => "ctrl+Tab",
            Self::PrevTab => "ctrl+shift+Tab",
            Self::TogglePin => "primary+D",
            Self::NewSplit => "primary+shift+=",
            Self::NewWindow => "primary+N",
            Self::CloseWindow => "primary+shift+W",
            Self::NewSpace => "primary+shift+N",
            Self::PrivateSpace => "primary+shift+P",
            Self::ClosePrivateSpace => "primary+shift+Backspace",
            Self::NextSpace => "primary+alt+Right",
            Self::PrevSpace => "primary+alt+Left",
            Self::ToggleSidebar => "primary+S",
            Self::SitePanel => "primary+shift+O",
            Self::CopyLink => "primary+shift+C",
            Self::Capture => "primary+shift+2",
            Self::Find => "primary+F",
            Self::FindNext => "primary+G",
            Self::FindPrevious => "primary+shift+G",
            Self::Print => "primary+P",
            Self::History if mac => "primary+Y",
            Self::History => "primary+H",
            Self::Downloads if mac => "primary+alt+L",
            Self::Downloads => "primary+J",
            Self::Settings if mac => "primary+,",
            Self::Settings => return None,
            Self::ToggleDevTools if mac => "primary+alt+I",
            Self::ToggleDevTools => "primary+shift+I",
        })
    }

    /// The default keys, as they count on this system.
    pub fn default_combo(self) -> Option<Combo> {
        self.default_on(MAC)
    }

    /// The default keys of macOS (`mac`) or of the other systems, read as
    /// this system reads keys.
    fn default_on(self, mac: bool) -> Option<Combo> {
        let combo: Combo = self.default_keys(mac)?.parse().ok()?;
        Some(combo.here())
    }
}

/// The keys by name and Windows virtual-key code, which Chromium gives for
/// a key on every system (a page reads it as `KeyboardEvent.keyCode`).
/// Letters and digits are their own names.
const NAMED_KEYS: &[(&str, i32)] = &[
    ("Backspace", 0x08),
    ("Tab", 0x09),
    ("Enter", 0x0D),
    ("Space", 0x20),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("End", 0x23),
    ("Home", 0x24),
    ("Left", 0x25),
    ("Up", 0x26),
    ("Right", 0x27),
    ("Down", 0x28),
    ("Delete", 0x2E),
    (";", 0xBA),
    ("=", 0xBB),
    (",", 0xBC),
    ("-", 0xBD),
    (".", 0xBE),
    ("/", 0xBF),
    ("`", 0xC0),
    ("[", 0xDB),
    ("\\", 0xDC),
    ("]", 0xDD),
    ("'", 0xDE),
];
const F1: i32 = 0x70;
const F12: i32 = 0x7B;

fn key_code(name: &str) -> Option<i32> {
    if let [c] = name.as_bytes()
        && (c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return Some(i32::from(*c));
    }
    if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<i32>().ok())
        && (1..=12).contains(&n)
    {
        return Some(F1 + n - 1);
    }
    NAMED_KEYS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

fn key_name(code: i32) -> Option<String> {
    match u8::try_from(code) {
        Ok(c) if c.is_ascii_uppercase() || c.is_ascii_digit() => Some(char::from(c).to_string()),
        _ if (F1..=F12).contains(&code) => Some(format!("F{}", code - F1 + 1)),
        _ => (NAMED_KEYS.iter())
            .find(|(_, c)| *c == code)
            .map(|(n, _)| (*n).to_string()),
    }
}

/// A key with its modifiers. Saved as text, modifiers first:
/// `primary+shift+T`. `primary` is Cmd on macOS and Ctrl elsewhere, so keys
/// chosen on one system fit the other; `ctrl` is the Control key of a Mac.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Combo {
    pub primary: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Windows virtual-key code; always one `key_name` knows.
    key: i32,
}

impl Combo {
    /// Key `key` (a Windows virtual-key code) with no modifier held, for the
    /// caller to add them; `None` for a key shortcuts cannot use (a
    /// modifier alone, a key without a name here).
    pub fn of_key(key: i32) -> Option<Self> {
        key_name(key)?;
        Some(Self {
            primary: false,
            ctrl: false,
            alt: false,
            shift: false,
            key,
        })
    }

    /// The combo as it counts on this system: outside macOS there is one
    /// Control key, which is `primary`.
    #[must_use]
    pub fn here(self) -> Self {
        if MAC {
            self
        } else {
            Self {
                primary: self.primary || self.ctrl,
                ctrl: false,
                ..self
            }
        }
    }

    /// The key's name: `T`, `Tab`, `F6`, `=`.
    pub fn key_name(self) -> String {
        key_name(self.key).unwrap_or_default()
    }

    fn is_function_key(self) -> bool {
        (F1..=F12).contains(&self.key)
    }
}

impl fmt::Display for Combo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (held, name) in [
            (self.primary, "primary+"),
            (self.ctrl, "ctrl+"),
            (self.alt, "alt+"),
            (self.shift, "shift+"),
        ] {
            if held {
                f.write_str(name)?;
            }
        }
        f.write_str(&self.key_name())
    }
}

impl From<Combo> for String {
    fn from(combo: Combo) -> Self {
        combo.to_string()
    }
}

/// Why keys cannot be a shortcut. Settings shows the text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeysError {
    /// Not in `Combo`'s format, or a key without a name here.
    #[error("not a key combination: {0}")]
    NotKeys(String),
    /// No Cmd, Ctrl or Alt: it would be typing.
    #[error("A shortcut needs {}, or is a function key.", if MAC { "⌘, ⌃ or ⌥" } else { "Ctrl or Alt" })]
    NeedsModifier,
    /// The keys have a fixed meaning; whose, in a few words.
    #[error("These keys are fixed: {0}.")]
    Fixed(&'static str),
}

impl FromStr for Combo {
    type Err = KeysError;

    fn from_str(text: &str) -> Result<Self, KeysError> {
        let bad = || KeysError::NotKeys(text.to_string());
        // The key comes last and is never "+" (that key is named "=").
        let (mods, key) = text.rsplit_once('+').unwrap_or(("", text));
        let mut combo = Self {
            primary: false,
            ctrl: false,
            alt: false,
            shift: false,
            key: key_code(key).ok_or_else(bad)?,
        };
        for name in mods.split('+').filter(|m| !m.is_empty()) {
            let held = match name {
                "primary" => &mut combo.primary,
                "ctrl" => &mut combo.ctrl,
                "alt" => &mut combo.alt,
                "shift" => &mut combo.shift,
                _ => return Err(bad()),
            };
            *held = true;
        }
        Ok(combo)
    }
}

impl TryFrom<String> for Combo {
    type Error = KeysError;

    fn try_from(text: String) -> Result<Self, KeysError> {
        text.parse()
    }
}

/// Keys with a fixed meaning on this system and what has them, so a
/// shortcut cannot take them: the system's and Chromium's editing and page
/// keys, and Magnetite's keys that cannot be changed.
fn taken() -> impl Iterator<Item = (Combo, &'static str)> {
    const EVERYWHERE: &[(&str, &str)] = &[
        ("primary+C", "Copy"),
        ("primary+X", "Cut"),
        ("primary+V", "Paste"),
        ("primary+A", "Select All"),
        ("primary+Z", "Undo"),
        ("primary+shift+Z", "Redo"),
        ("primary+R", "Reload Page"),
        ("primary+shift+R", "Reload Page"),
        ("primary+=", "Zoom In"),
        ("primary+-", "Zoom Out"),
        ("primary+0", "Actual Size"),
        ("primary+O", "Keep a peek as a tab"),
        ("F12", "Developer Tools"),
    ];
    const MAC_ONLY: &[(&str, &str)] = &[
        ("primary+Q", "Quit"),
        ("primary+H", "Hide"),
        ("primary+alt+H", "Hide Others"),
        ("primary+M", "Minimize"),
        ("primary+alt+shift+V", "Paste and Match Style"),
        ("primary+[", "Back"),
        ("primary+]", "Forward"),
        ("primary+alt+N", "Quick search from anywhere"),
        ("primary+alt+J", "Developer Tools"),
        ("primary+alt+C", "Developer Tools"),
        ("primary+Tab", "the system's app switcher"),
        ("primary+Space", "the system's search"),
    ];
    const ELSEWHERE: &[(&str, &str)] = &[
        ("primary+Y", "Redo"),
        ("alt+Left", "Back"),
        ("alt+Right", "Forward"),
        ("F5", "Reload Page"),
        ("primary+shift+J", "Developer Tools"),
        ("primary+shift+V", "Paste and Match Style"),
        ("alt+F4", "closing the window"),
    ];
    let own = if MAC { MAC_ONLY } else { ELSEWHERE };
    // The digits choose a tab (Cmd or Ctrl) or a space (Ctrl; Alt outside macOS).
    let space_digits = if MAC { "ctrl" } else { "alt" };
    let digits = (1..=9).flat_map(move |n| {
        [
            (format!("primary+{n}"), "choosing a tab by its place"),
            (
                format!("{space_digits}+{n}"),
                "choosing a space by its place",
            ),
        ]
    });
    (EVERYWHERE.iter().chain(own))
        .map(|(keys, by)| ((*keys).to_string(), *by))
        .chain(digits)
        .filter_map(|(keys, by)| Some((keys.parse::<Combo>().ok()?.here(), by)))
}

/// Why `combo` cannot be a shortcut, if it cannot: it would be typing
/// (no Cmd, Ctrl or Alt), or its keys have a fixed meaning.
pub fn refusal(combo: Combo) -> Option<KeysError> {
    let combo = combo.here();
    if !(combo.primary || combo.ctrl || combo.alt || combo.is_function_key()) {
        return Some(KeysError::NeedsModifier);
    }
    taken()
        .find(|(taken, _)| *taken == combo)
        .map(|(_, by)| KeysError::Fixed(by))
}

/// What the user changed: an action's own keys, or `None` for no keys.
/// Actions on their default keys are not in it, so it stays small in
/// `state.json` and in sync. Reading it skips what this version does not
/// know (an action or key of a newer one) instead of failing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Overrides(BTreeMap<Action, Option<Combo>>);

impl<'de> Deserialize<'de> for Overrides {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        type Error = serde::de::value::Error;
        let raw = BTreeMap::<String, Option<String>>::deserialize(deserializer)?;
        let known = raw.into_iter().filter_map(|(action, keys)| {
            let action: Result<Action, Error> = Action::deserialize(action.into_deserializer());
            let keys = keys.map(|k| k.parse::<Combo>()).transpose();
            Some((action.ok()?, keys.ok()?))
        });
        Ok(Self(known.collect()).sanitised())
    }
}

impl Overrides {
    /// The keys of `action` now: the user's, else the default.
    pub fn combo_of(&self, action: Action) -> Option<Combo> {
        match self.0.get(&action) {
            Some(own) => own.map(Combo::here),
            None => action.default_combo(),
        }
    }

    /// The action that has `combo` now.
    pub fn action_for(&self, combo: Combo) -> Option<Action> {
        let combo = combo.here();
        (Action::ALL.into_iter()).find(|a| self.combo_of(*a) == Some(combo))
    }

    /// Give `action` the keys `combo` (`None`: no keys). An action that had
    /// these keys loses them. Refused for keys with a fixed meaning
    /// (`refusal`).
    pub fn set(&mut self, action: Action, combo: Option<Combo>) -> Result<(), KeysError> {
        if let Some(combo) = combo {
            if let Some(why) = refusal(combo) {
                return Err(why);
            }
            self.take_from_others(action, combo);
        }
        if combo.map(Combo::here) == action.default_combo() {
            // The default keys of this system: nothing to save.
            self.0.remove(&action);
        } else {
            self.0.insert(action, combo);
        }
        Ok(())
    }

    /// Give `action` its default keys back; an action that has taken them
    /// since loses them. Nothing stays saved for `action`, so on every
    /// system it has that system's default keys again.
    pub fn reset(&mut self, action: Action) {
        if let Some(keys) = action.default_combo() {
            self.take_from_others(action, keys);
        }
        self.0.remove(&action);
    }

    /// Every other action that has `combo` gets no keys: all of them, also
    /// one listed after `action`, which only waited for the keys to be free
    /// (two actions can be on the same keys after a sync between systems).
    fn take_from_others(&mut self, action: Action, combo: Combo) {
        let combo = Some(combo.here());
        for other in Action::ALL {
            if other != action && self.combo_of(other) == combo {
                self.0.insert(other, None);
            }
        }
    }

    /// Without keys a shortcut cannot have, which may come from another
    /// device or system (such an action keeps its default keys), and
    /// without what says nothing on any system: an action on keys that are
    /// its default both on macOS and elsewhere. What differs from the
    /// default of one system stays, so "no keys for Settings", chosen on a
    /// Mac, survives a device where Settings has none anyway.
    #[must_use]
    pub fn sanitised(mut self) -> Self {
        self.0.retain(|action, own| {
            let fine = own.is_none_or(|combo| refusal(combo).is_none());
            let own = own.map(Combo::here);
            fine && !(own == action.default_on(true) && own == action.default_on(false))
        });
        self
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combo(keys: &str) -> Combo {
        keys.parse().unwrap()
    }

    #[test]
    fn combos_are_saved_as_text_and_read_back() {
        for keys in [
            "primary+shift+T",
            "ctrl+Tab",
            "primary+alt+Left",
            "F6",
            "primary+=",
            "alt+\\",
        ] {
            assert_eq!(combo(keys).to_string(), keys);
        }
        for bad in [
            "",
            "primary",
            "primary+",
            "cmd+T",
            "primary+t",
            "primary+F13",
            "primary++",
        ] {
            assert!(bad.parse::<Combo>().is_err(), "{bad}");
        }
    }

    #[test]
    fn every_action_is_listed_once_and_has_usable_default_keys() {
        let mut seen = Vec::new();
        for action in Action::ALL {
            assert_eq!(Action::of(action.shortcut()), Some(action));
            if let Some(keys) = action.default_combo() {
                assert_eq!(refusal(keys), None, "{action:?}");
                // The other systems' defaults must be usable there too.
                for mac in [true, false] {
                    assert!(
                        action
                            .default_keys(mac)
                            .is_none_or(|k| k.parse::<Combo>().is_ok())
                    );
                }
                assert!(!seen.contains(&keys), "{action:?} shares its default keys");
                seen.push(keys);
            }
        }
        assert_eq!(
            Overrides::default().action_for(combo("primary+T")),
            Some(Action::NewTab)
        );
    }

    #[test]
    fn new_keys_replace_the_default_and_are_all_that_is_saved() {
        let mut keys = Overrides::default();
        keys.set(Action::NewTab, Some(combo("primary+K"))).unwrap();
        assert_eq!(keys.action_for(combo("primary+K")), Some(Action::NewTab));
        assert_eq!(keys.action_for(combo("primary+T")), None);
        assert_eq!(
            serde_json::to_string(&keys).unwrap(),
            r#"{"new_tab":"primary+K"}"#
        );
        keys.set(Action::Print, None).unwrap();
        assert_eq!(keys.combo_of(Action::Print), None);
        keys.reset(Action::NewTab);
        keys.reset(Action::Print);
        assert!(keys.is_empty());
    }

    #[test]
    fn keys_in_use_move_to_the_action_they_are_given_to() {
        let mut keys = Overrides::default();
        keys.set(Action::Find, Some(combo("primary+T"))).unwrap();
        assert_eq!(keys.action_for(combo("primary+T")), Some(Action::Find));
        assert_eq!(keys.combo_of(Action::NewTab), None);
        // Back to the default: Find, which took them since, gives them up.
        keys.reset(Action::NewTab);
        assert_eq!(keys.action_for(combo("primary+T")), Some(Action::NewTab));
        assert_eq!(keys.combo_of(Action::Find), None);
    }

    #[test]
    fn a_reset_takes_its_keys_from_every_action_that_has_them() {
        // As after a sync between systems: Find and Print both on New Tab's
        // default keys, behind New Tab in the list.
        let mut keys: Overrides =
            serde_json::from_str(r#"{"find":"primary+T","print":"primary+T"}"#).unwrap();
        keys.reset(Action::NewTab);
        assert_eq!(keys.combo_of(Action::Find), None);
        assert_eq!(keys.combo_of(Action::Print), None);
        // New Tab on other keys later: nothing else wakes up on the old ones.
        keys.set(Action::NewTab, Some(combo("primary+K"))).unwrap();
        assert_eq!(keys.action_for(combo("primary+T")), None);
    }

    #[test]
    fn keys_with_a_fixed_meaning_and_plain_typing_are_refused() {
        let mut keys = Overrides::default();
        for fixed in ["primary+C", "primary+1", "primary+R", "T", "shift+T"] {
            assert!(
                keys.set(Action::NewTab, Some(combo(fixed))).is_err(),
                "{fixed}"
            );
        }
        assert!(keys.set(Action::NewTab, Some(combo("F6"))).is_ok());
    }

    #[test]
    fn what_another_version_or_device_sent_is_read_as_far_as_known() {
        let keys: Overrides = serde_json::from_str(
            r#"{"new_tab":"primary+K","teleport":"primary+U","print":"hyper+P","find":null,
                "history":"primary+C","close_tab":"primary+W"}"#,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_string(&keys).unwrap(),
            r#"{"new_tab":"primary+K","find":null}"#
        );
    }

    #[test]
    fn no_keys_for_an_action_survives_a_system_where_it_has_none_anyway() {
        // Settings has keys on macOS only. "None", chosen on a Mac, must not
        // be dropped by a Linux device, or the Mac would get its keys back.
        let keys: Overrides = serde_json::from_str(r#"{"settings":null}"#).unwrap();
        assert_eq!(
            serde_json::to_string(&keys).unwrap(),
            r#"{"settings":null}"#
        );
        assert_eq!(keys.combo_of(Action::Settings), None);
        // Reset saves nothing: each system has its own default again.
        let mut keys = keys;
        keys.reset(Action::Settings);
        assert!(keys.is_empty());
        assert_eq!(
            keys.combo_of(Action::Settings),
            Action::Settings.default_combo()
        );
    }

    #[test]
    fn a_mac_control_key_is_the_one_control_key_elsewhere() {
        let here = combo("ctrl+K").here();
        assert_eq!(here.primary, !MAC);
        assert_eq!(here.ctrl, MAC);
        assert_eq!(combo("primary+K").here(), combo("primary+K"));
    }
}
