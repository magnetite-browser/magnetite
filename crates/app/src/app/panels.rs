//! Web panels: a favorite's or pinned tab's page shown over the page area, at the
//! sidebar's edge, while the tab in front stays where it is.

use magnetite_engine::{ContentId, Engine};

use crate::state::now;

use super::MagnetiteApp;

impl MagnetiteApp {
    /// Show favorite `tab_id` as the window's web panel, loading its page
    /// if it has none. The panel open until now is hidden and stays loaded.
    pub(super) fn open_web_panel(&self, engine: &Engine, tab_id: &str) {
        let content = self.page_of(engine, tab_id);
        let (width, mobile) = self.with_state(|s| {
            s.web_panel = Some(tab_id.to_string());
            let Some(t) = s.find_tab_mut(tab_id) else {
                return (None, false);
            };
            t.last_active_at = Some(now());
            t.unread = false;
            (t.panel_width, t.panel_mobile)
        });
        engine.set_mobile_view(content, mobile);
        engine.show_web_panel(content, width);
        self.push_state(engine);
    }

    /// Hide the open web panel. Its page stays loaded, as a favorite's
    /// does, and the keyboard goes back to the page in front.
    pub(super) fn close_web_panel(&self, engine: &Engine) {
        if self.hide_web_panel(engine) {
            self.on_active_content(engine, Engine::focus_content);
        }
        self.push_state(engine);
    }

    /// Hide the open web panel and leave the keyboard where it is. False
    /// if none was shown.
    fn hide_web_panel(&self, engine: &Engine) -> bool {
        self.with_state(|s| s.web_panel = None);
        let shown = engine.web_panel_content();
        if let Some(c) = shown {
            engine.hide_content(c);
        }
        shown.is_some()
    }

    /// A click on a panel's favorite: open it, or close it if it is open.
    pub(super) fn toggle_web_panel(&self, engine: &Engine, tab_id: &str) {
        if self.with_state(|s| s.web_panel.as_deref() == Some(tab_id)) {
            self.close_web_panel(engine);
        } else if self.with_state(|s| s.is_web_panel(tab_id)) {
            self.open_web_panel(engine, tab_id);
        }
    }

    /// The key for the web panel: close the open one, or open the one
    /// used last.
    pub(super) fn web_panel_key(&self, engine: &Engine) {
        if self.with_state(|s| s.web_panel.is_some()) {
            self.close_web_panel(engine);
        } else if let Some(id) = self.with_state(|s| s.latest_web_panel()) {
            self.open_web_panel(engine, &id);
        }
    }

    /// Favorite `tab_id` opens as a web panel from now on (`panel`), and
    /// does so at once, or as a tab again.
    pub(super) fn set_web_panel(&self, engine: &Engine, tab_id: &str, panel: bool) {
        let on_screen = self.with_state(|s| s.visible_panes().iter().any(|t| t == tab_id));
        if !self.with_state(|s| s.set_web_panel(tab_id, panel)) {
            return;
        }
        if !panel {
            // `settle_web_panel` hides it with the next look at the state.
            self.push_state(engine);
            return;
        }
        // It was a page of the page area: that shows what is left.
        if on_screen {
            let space = self.with_state(|s| s.active_space.clone());
            self.activate_space(engine, &space);
        }
        self.open_web_panel(engine, tab_id);
    }

    /// Web panel `tab_id`'s page is shown as on a phone from now on
    /// (`mobile`), or as itself again.
    pub(super) fn set_panel_mobile(&self, engine: &Engine, tab_id: &str, mobile: bool) {
        let content = self.with_state(|s| {
            if !s.is_web_panel(tab_id) {
                return None;
            }
            s.find_tab_mut(tab_id)?.panel_mobile = mobile;
            // Without a page (never opened, or unloaded) it applies when
            // the panel opens next.
            Some(s.runtime.get(tab_id).and_then(|r| r.content))
        });
        let Some(content) = content else { return };
        if let Some(c) = content {
            engine.set_mobile_view(c, mobile);
        }
        self.push_state(engine);
    }

    /// The open web panel showing page `content` was dragged to `width`
    /// points (none: the usual width): its favorite keeps that.
    pub(super) fn web_panel_resized(
        &self,
        engine: &Engine,
        content: ContentId,
        width: Option<u32>,
    ) {
        let kept = self.with_state(|s| {
            let tab = s.content_to_tab.get(&content).cloned()?;
            s.find_tab_mut(&tab)?.panel_width = width;
            Some(())
        });
        if kept.is_some() {
            self.push_state(engine);
        }
    }

    /// A page got the keyboard. With "close by itself" on, an open web
    /// panel closes when it was a page behind it, not its own.
    pub(super) fn page_focused(&self, engine: &Engine, content: ContentId) {
        let behind = self.with_state(|s| {
            s.settings.panel_autoclose
                && s.web_panel.is_some()
                && s.content_to_tab.contains_key(&content)
        });
        // A panel that has the keyboard got it after this page did.
        let stale = engine.web_panel_focused();
        if behind && !stale && engine.web_panel_content().is_some_and(|p| p != content) {
            // The keyboard stays in the page that took it: in a split that
            // need not be the active tab's.
            self.hide_web_panel(engine);
            self.push_state(engine);
        }
    }

    /// Make the current window's record of its open web panel and the page
    /// the engine shows as one agree. They part when the panel's page
    /// closes or moves to another window, or its favorite stops being a
    /// panel (removed, moved out of the favorites, changed by sync): then
    /// the panel is closed. The same for the pages shown as on a phone.
    pub(super) fn settle_web_panel(&self, engine: &Engine) {
        let shown = engine.web_panel_content();
        let wanted = self.with_state(|s| {
            let tab = s.web_panel.as_deref().filter(|t| s.is_web_panel(t))?;
            s.runtime.get(tab)?.content
        });
        // A page whose favorite is no mobile web panel any more is itself
        // again.
        for c in engine.mobile_views() {
            let mobile = self.with_state(|s| {
                (s.content_to_tab.get(&c)).is_some_and(|t| {
                    s.is_web_panel(t) && s.find_tab(t).is_some_and(|(_, t)| t.panel_mobile)
                })
            });
            if !mobile {
                engine.set_mobile_view(c, false);
            }
        }
        if shown == wanted {
            return;
        }
        self.with_state(|s| s.web_panel = None);
        if let Some(c) = shown {
            engine.hide_content(c);
        }
    }
}
