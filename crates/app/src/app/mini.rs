//! Mini windows: a page in a small window of its own, outside the sidebar,
//! for links from other apps and the quick search, and what becomes of it.

use magnetite_engine::{ContentId, Engine, Shortcut};

use crate::state::{Peek, TabRuntime};
use crate::url_policy;

use super::{LinkVariant, MagnetiteApp};

impl MagnetiteApp {
    /// Open `url` (from `source`) in a new mini window: in the opener's
    /// `context` when a page asked, else in the active space's profile, as
    /// a peek would.
    pub(super) fn open_mini(
        &self,
        engine: &Engine,
        url: &str,
        source: url_policy::Source,
        context: Option<&str>,
    ) {
        if !url_policy::allowed(url, source) {
            return;
        }
        let (profile, engine_profile) = self.with_state(|s| {
            let profile = context
                .and_then(|c| s.profile_for_context(c))
                .unwrap_or_else(|| s.active_space().profile_id.clone());
            let engine_profile = s.context(Some(&profile));
            (profile, engine_profile)
        });
        let c = engine.create_mini(url, engine_profile.as_ref());
        self.with_state(|s| {
            s.minis.push(Peek {
                content: c,
                url: url.to_string(),
                title: url.to_string(),
                runtime: TabRuntime {
                    content: Some(c),
                    loaded: true,
                    loading: true,
                    ..TabRuntime::default()
                },
                profile,
            });
        });
    }

    /// Whether page window `id` is a mini window's.
    pub(super) fn is_mini(&self, id: ContentId) -> bool {
        self.with_state(|s| s.minis.iter().any(|m| m.content == id))
    }

    /// Close mini window `id`: its page is dropped, nothing is archived.
    pub(super) fn close_mini(&self, engine: &Engine, id: ContentId) {
        self.with_state(|s| s.minis.retain(|m| m.content != id));
        engine.close_content(id);
    }

    /// "Open in": mini window `id`'s page becomes a Today tab of `space`
    /// (the active space when `None`), reusing its window, and the main
    /// window comes to the front with it.
    pub(super) fn mini_to_tab(&self, engine: &Engine, id: ContentId, space: Option<&str>) {
        let tab = self.with_state(|s| {
            let space = space.map_or_else(|| s.active_space.clone(), str::to_owned);
            s.keep_mini(id, &space)
        });
        let Some(tab) = tab else {
            return;
        };
        engine.mini_to_tab(id);
        self.activate_tab(engine, &tab);
        engine.activate_frame();
        engine.focus_content(id);
    }

    /// A key or menu bar item from mini window `id`: those for a page act
    /// on its page, `Cmd+O` opens it in the current space, and the others
    /// are for the main window, which comes to the front.
    pub(super) fn mini_shortcut(&self, engine: &Engine, id: ContentId, shortcut: Shortcut) {
        match shortcut {
            Shortcut::CloseTab => self.close_mini(engine, id),
            Shortcut::PeekToTab => self.mini_to_tab(engine, id, None),
            Shortcut::Reload
            | Shortcut::Back
            | Shortcut::Forward
            | Shortcut::ZoomIn
            | Shortcut::ZoomOut
            | Shortcut::ZoomReset => Self::page_command_on(engine, id, shortcut),
            Shortcut::CopyLink => {
                let result = self.copy_link(engine, Some(id), LinkVariant::Clean);
                Self::ui_call(engine, "copied", &result);
            }
            Shortcut::Print => engine.content_print(id),
            Shortcut::ToggleDevTools => self.toggle_devtools(engine, Some(id)),
            Shortcut::OpenDevTools => engine.content_show_devtools(id),
            // Nothing the peek's own keys or the quick search key mean here.
            Shortcut::ClosePeek | Shortcut::PeekToSplit => {}
            Shortcut::NewMini => self.handle_shortcut(engine, shortcut),
            _ => {
                engine.activate_frame();
                self.handle_shortcut(engine, shortcut);
            }
        }
    }
}
