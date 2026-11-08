//! Chromium's own password manager and autofill: which parts are on. They
//! come with the engine; Magnetite only sets their switches, as Chromium
//! preferences of every profile.

use cef::*;
use serde::{Deserialize, Serialize};

use crate::engine::Engine;

/// The switches, as the app's settings keep them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutofillPrefs {
    /// Offer to save passwords, and fill them in.
    pub passwords: bool,
    /// Save addresses and fill them in.
    pub addresses: bool,
    /// Save payment cards and fill them in.
    pub cards: bool,
    /// After a sign-in, ask Google's service whether that password is known
    /// from a data breach. The one switch here that makes the browser
    /// contact a server, so it starts off.
    pub breach_check: bool,
}

impl Default for AutofillPrefs {
    fn default() -> Self {
        Self {
            passwords: true,
            addresses: true,
            cards: true,
            breach_check: false,
        }
    }
}

impl AutofillPrefs {
    /// The Chromium preferences these switches stand for.
    fn chromium(self) -> [(&'static str, bool); 5] {
        [
            ("credentials_enable_service", self.passwords),
            // Signing in without a click makes sense only with saved passwords.
            ("credentials_enable_autosignin", self.passwords),
            ("autofill.profile_enabled", self.addresses),
            ("autofill.credit_card_enabled", self.cards),
            ("profile.password_manager_leak_detection", self.breach_check),
        ]
    }

    /// Set them on `ctx`. False if Chromium refused one (a context that is
    /// not initialised yet refuses all; it gets them in `prepare_context`).
    pub(crate) fn apply(self, ctx: &RequestContext) -> bool {
        self.chromium()
            .iter()
            .all(|(name, on)| set_bool_pref(ctx, name, *on))
    }
}

/// Set Chromium preference `name` of `ctx` to `on`.
pub(crate) fn set_bool_pref(ctx: &RequestContext, name: &str, on: bool) -> bool {
    let Some(mut value) = value_create() else {
        return false;
    };
    value.set_bool(i32::from(on));
    // Not empty: cef-rs passes an empty string as a null pointer, and CEF
    // refuses to set anything without a place for its error message.
    let mut error = CefString::from(" ");
    let name = CefString::from(name);
    ctx.set_preference(Some(&name), Some(&mut value), Some(&mut error)) != 0
}

impl Engine {
    /// Use `prefs` from now on: in every profile that exists, and in those
    /// made later.
    pub fn set_autofill(&self, prefs: AutofillPrefs) {
        let contexts: Vec<RequestContext> = {
            let mut inner = self.lock();
            inner.autofill = prefs;
            inner.contexts.values().cloned().collect()
        };
        for ctx in request_context_get_global_context()
            .into_iter()
            .chain(contexts)
        {
            prefs.apply(&ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AutofillPrefs;

    #[test]
    fn nothing_here_contacts_a_server_unless_asked_to() {
        let prefs = AutofillPrefs::default();
        assert!(prefs.passwords && prefs.addresses && prefs.cards);
        assert!(!prefs.breach_check);
        let set = prefs.chromium();
        assert!(set.contains(&("profile.password_manager_leak_detection", false)));
        // Without saved passwords there is no signing in by itself either.
        let off = AutofillPrefs {
            passwords: false,
            ..prefs
        };
        assert!(
            off.chromium()
                .contains(&("credentials_enable_autosignin", false))
        );
    }
}
