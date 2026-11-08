//! The peek: a link opened over the page, and what becomes of it.

use magnetite_core::split::Side;
use magnetite_engine::Engine;

use crate::state::{AppState, Peek, TabRuntime};
use crate::url_policy;

use super::MagnetiteApp;

impl MagnetiteApp {
    /// Show `url` (from `source`) as a peek over the page, in the opener's
    /// `context` when there is one, else the active space's profile.
    pub(super) fn open_peek(
        &self,
        engine: &Engine,
        url: &str,
        source: url_policy::Source,
        context: Option<&str>,
    ) {
        if !url_policy::allowed(url, source) {
            return;
        }
        self.close_peek(engine);
        let (profile, engine_profile) = self.with_state(|s| {
            let profile = context
                .and_then(|c| s.profile_for_context(c))
                .unwrap_or_else(|| s.active_space().profile_id.clone());
            let engine_profile = s.context(Some(&profile));
            (profile, engine_profile)
        });
        let c = engine.create_content(url, engine_profile.as_ref());
        self.with_state(|s| {
            s.peek = Some(Peek {
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
        engine.show_peek(c);
        self.push_state(engine);
    }

    pub(super) fn close_peek(&self, engine: &Engine) {
        let c = self.with_state(|s| s.peek.take().map(|p| p.content));
        if let Some(c) = c {
            engine.close_content(c);
            self.push_state(engine);
        }
    }

    /// Keep the peeked page: it becomes a Today tab that reuses the window.
    pub(super) fn peek_to_tab(&self, engine: &Engine) {
        if let Some(id) = self.keep_peek() {
            self.activate_tab(engine, &id);
        }
    }

    /// Keep the peeked page as a tab beside the current one, in split view
    /// (on its own if the split is full or no tab is open).
    pub(super) fn peek_to_split(&self, engine: &Engine) {
        let beside = self.with_state(|s| s.active_tab_id().map(str::to_owned));
        let Some(id) = self.keep_peek() else {
            return;
        };
        let split =
            beside.is_some_and(|t| self.with_state(|s| s.split_insert(Some(&t), &id, Side::Right)));
        if split {
            self.present(engine);
        } else {
            self.activate_tab(engine, &id);
        }
    }

    /// Turn the peek into a Today tab (`AppState::keep_peek`).
    pub(super) fn keep_peek(&self) -> Option<String> {
        self.with_state(AppState::keep_peek)
    }
}
