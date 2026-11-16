//! Chromium's own password manager and autofill: which parts are on. They
//! come with the engine; Magnetite only sets their switches, as Chromium
//! preferences of every profile.

use std::sync::atomic::{AtomicBool, Ordering};

use cef::*;
use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::events::EngineEvent;

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

/// Chromium's preference behind "Offer to save passwords".
const PASSWORDS: &str = "credentials_enable_service";
/// Signing in without a click, with a saved password.
const AUTO_SIGN_IN: &str = "credentials_enable_autosignin";
const ADDRESSES: &str = "autofill.profile_enabled";
const CARDS: &str = "autofill.credit_card_enabled";
const BREACH_CHECK: &str = "profile.password_manager_leak_detection";

impl AutofillPrefs {
    /// The Chromium preferences these switches stand for.
    fn chromium(self) -> [(&'static str, bool); 4] {
        [
            (PASSWORDS, self.passwords),
            (ADDRESSES, self.addresses),
            (CARDS, self.cards),
            (BREACH_CHECK, self.breach_check),
        ]
    }

    /// Set them on `ctx`. False if Chromium refused one (a context that is
    /// not initialised yet refuses all; it gets them in `prepare_context`).
    pub(crate) fn apply(self, ctx: &RequestContext) -> bool {
        APPLYING.store(true, Ordering::Relaxed);
        let mut set = (self.chromium().iter()).all(|(name, on)| set_bool_pref(ctx, name, *on));
        // Signing in without a click makes sense only with saved passwords.
        // With them it stays as chosen on Chromium's page.
        if !self.passwords {
            set &= set_bool_pref(ctx, AUTO_SIGN_IN, false);
        }
        APPLYING.store(false, Ordering::Relaxed);
        set
    }

    /// The switches as `ctx` has them now. `None` before Chromium has
    /// finished making the context.
    fn live(ctx: &RequestContext) -> Option<Self> {
        let on = |name: &str| Some(ctx.preference(Some(&CefString::from(name)))?.bool() != 0);
        Some(Self {
            passwords: on(PASSWORDS)?,
            addresses: on(ADDRESSES)?,
            cards: on(CARDS)?,
            breach_check: on(BREACH_CHECK)?,
        })
    }
}

/// True while Magnetite itself sets the preferences: `SwitchWatch` then sees
/// them half set and must not take that for the user's choice.
static APPLYING: AtomicBool = AtomicBool::new(false);

// Chromium's own pages have these switches too (`chrome://settings`,
// `chrome://password-manager/settings`). A change made there is told to the
// app, which keeps it; otherwise the next start would undo it.
wrap_preference_observer! {
    struct SwitchWatch {
        ctx: RequestContext,
    }
    impl PreferenceObserver {
        fn on_preference_changed(&self, _name: Option<&CefString>) {
            if APPLYING.load(Ordering::Relaxed) {
                return;
            }
            let engine = Engine::global();
            let ours = engine.lock().autofill;
            if let Some(live) = AutofillPrefs::live(&self.ctx).filter(|live| *live != ours) {
                engine.emit(EngineEvent::AutofillChanged(live));
            }
        }
    }
}

/// Tell the app whenever one of the switches of `ctx` changes on a page of
/// Chromium's.
pub(crate) fn watch(ctx: &RequestContext) {
    for (name, _) in AutofillPrefs::default().chromium() {
        let mut watch = SwitchWatch::new(ctx.clone());
        match ctx.add_preference_observer(Some(&CefString::from(name)), Some(&mut watch)) {
            Some(watch) => Engine::global().lock().pref_watches.push(watch),
            None => crate::trace("cannot watch Chromium's password and autofill switches"),
        }
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
        let (contexts, passwords_back): (Vec<RequestContext>, bool) = {
            let mut inner = self.lock();
            let back = prefs.passwords && !inner.autofill.passwords;
            inner.autofill = prefs;
            (inner.contexts.values().cloned().collect(), back)
        };
        for ctx in request_context_get_global_context()
            .into_iter()
            .chain(contexts)
        {
            prefs.apply(&ctx);
            // Turning passwords off turned this off with them.
            if passwords_back {
                set_bool_pref(&ctx, AUTO_SIGN_IN, true);
            }
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
        // Auto sign-in is not among them: with passwords on it stays as
        // chosen on Chromium's page (`apply` turns it off with passwords).
        assert!(set.iter().all(|(name, _)| *name != super::AUTO_SIGN_IN));
    }
}
