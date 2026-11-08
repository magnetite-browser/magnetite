//! Extensions in the sidebar: the list of the profile in front, opening
//! one, pinning.

use magnetite_engine::Engine;
use serde_json::json;

use super::MagnetiteApp;

impl MagnetiteApp {
    /// Installed extensions of the profile the page in front uses.
    pub(super) fn front_extensions(&self, engine: &Engine) -> Vec<magnetite_engine::ExtensionInfo> {
        let context = self.with_state(|s| s.front_context());
        engine.extensions(context.as_ref())
    }

    /// Open extension `id` for the page in front; with no page there is no
    /// tab for it to act on.
    pub(super) fn open_extension(&self, engine: &Engine, id: &str) {
        match self.focused_content(None) {
            Some(c) => engine.open_extension(c, id),
            None => Self::ui_call(
                engine,
                "notice",
                &json!({ "text": "Open a page first: extensions act on the page in front." }),
            ),
        }
    }

    pub(super) fn pin_extension(&self, engine: &Engine, id: &str, pinned: bool) {
        self.with_state(|s| {
            let list = &mut s.settings.pinned_extensions;
            list.retain(|x| x != id);
            if pinned {
                list.push(id.to_string());
            }
        });
        self.push_state(engine);
    }
}
