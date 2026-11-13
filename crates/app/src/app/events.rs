//! What the engine reports: page windows opening and changing, keys,
//! permission prompts, downloads, and commands from Magnetite's own pages.

use magnetite_engine::{
    DevToolsAction, DividerAction, Engine, EngineDelegate, EngineEvent, ExtensionMenuAction,
    Shortcut, UrlSource,
};
use serde_json::Value;

use crate::state::{ArcImport, now};
use crate::{lock, trace, url_policy};

use super::commands::UiCommand;
use super::{EXTENSION_STORE, LinkVariant, MagnetiteApp, WELCOME_URL};

impl EngineDelegate for MagnetiteApp {
    fn on_event(&self, engine: &Engine, event: EngineEvent) {
        trace(|| format!("event {}", event.name()));
        match event {
            EngineEvent::Ready => {
                let (toolbar, dock, width, autofill, quick) = self.with_state(|s| {
                    let a = s.settings.appearance.clone().clamped();
                    (
                        s.settings.chrome_toolbar,
                        s.settings.devtools_dock,
                        a.sidebar_width,
                        s.settings.autofill,
                        s.settings.quick_search,
                    )
                });
                engine.set_quick_search_key(quick);
                // Before any profile exists: each gets these when it is made.
                engine.set_autofill(autofill);
                engine.set_sidebar_width(width);
                if toolbar {
                    engine.set_chrome_toolbar(true);
                }
                engine.set_devtools_dock(dock);
                if self.import == ArcImport::Now {
                    eprintln!(
                        "magnetite: state imported from Arc ({})",
                        self.data_dir.display()
                    );
                }
                engine.create_frame();
            }
            EngineEvent::FrameCreated
            | EngineEvent::FrameBoundsChanged
            | EngineEvent::ContentCreated { .. } => {}
            EngineEvent::SidebarLoaded => self.push_state(engine),
            EngineEvent::ContentClosed { id } => {
                self.with_prompts(|p| p.open.retain(|q| q.content != id));
                self.show_prompts(engine);
                // Closed by the app, the window is unknown here by now; one
                // that closed itself (`window.close()`, a mini window's close
                // button) still has its peek, mini window or tab, and the
                // sidebar must hear that it is gone.
                let by_itself = self.with_state(|s| {
                    let was_peek = s.peek.as_ref().is_some_and(|p| p.content == id);
                    if was_peek {
                        s.peek = None;
                    }
                    s.minis.retain(|m| m.content != id);
                    let tab = s.content_to_tab.remove(&id);
                    if let Some(rt) = tab.as_ref().and_then(|tab| s.runtime.get_mut(tab)) {
                        rt.unload();
                    }
                    was_peek || tab.is_some()
                });
                if by_itself {
                    self.push_state(engine);
                }
            }
            EngineEvent::TitleChanged { id, title } => {
                self.with_state(|s| {
                    if let Some(p) = s.page_outside_mut(id) {
                        p.title.clone_from(&title);
                    }
                    s.record_title(id, &title);
                    if let Some(tab) = s.content_to_tab.get(&id).cloned()
                        && let Some(t) = s.find_tab_mut(&tab)
                        && !t.custom_title
                        && !title.is_empty()
                    {
                        t.title = title;
                    }
                });
                self.push_state(engine);
            }
            EngineEvent::UrlChanged { id, url } => {
                self.with_state(|s| {
                    if let Some(p) = s.page_outside_mut(id) {
                        p.url.clone_from(&url);
                    }
                    s.record_visit(id, &url);
                    if let Some(tab) = s.content_to_tab.get(&id).cloned()
                        && let Some(t) = s.find_tab_mut(&tab)
                    {
                        t.url = url;
                    }
                });
                self.push_state(engine);
            }
            EngineEvent::FaviconChanged { id, url } => {
                self.with_state(|s| {
                    if let Some(rt) = s.page_runtime(id) {
                        rt.favicon = url;
                    }
                });
                self.push_state(engine);
            }
            EngineEvent::MediaChanged { id, media } => {
                self.with_state(|s| {
                    if let Some(rt) = s.page_runtime(id) {
                        rt.media = media;
                    }
                });
                self.show_state_paced(engine);
            }
            EngineEvent::Download(report) => {
                let changed = self.with_state(|s| {
                    let private = s.is_private_context(report.context.as_deref());
                    let before = s.downloads.report(&report, private, now());
                    let changed = before != Some(report.state.into());
                    if changed {
                        s.persist();
                    }
                    changed
                });
                // Started, finished or failed: the notification panel tells
                // (`Notifications.tsx`); progress only updates it.
                if changed {
                    engine.ensure_notifications();
                }
                self.show_downloads(engine);
            }
            EngineEvent::PermissionPrompt(prompt) => {
                self.with_prompts(|p| p.open.push(prompt));
                self.show_prompts(engine);
            }
            EngineEvent::PermissionPromptClosed { id } => {
                self.with_prompts(|p| p.open.retain(|q| q.id != id));
                self.show_prompts(engine);
            }
            EngineEvent::ExtensionPinsChanged => self.show_state(engine),
            EngineEvent::ExtensionMenu(action) => match action {
                ExtensionMenuAction::Open(id) => self.open_extension(engine, &id),
                ExtensionMenuAction::TogglePin(id) => {
                    let pinned = self.with_state(|s| s.settings.pinned_extensions.contains(&id));
                    self.pin_extension(engine, &id, !pinned);
                }
                ExtensionMenuAction::Manage => {
                    self.open_url(engine, "chrome://extensions", false);
                }
                ExtensionMenuAction::Store => {
                    self.open_url(engine, EXTENSION_STORE, false);
                }
            },
            EngineEvent::BackToTab { id } => {
                if let Some(tab) = self.with_state(|s| s.content_to_tab.get(&id).cloned()) {
                    self.activate_tab(engine, &tab);
                }
                // The window comes forward, also from another app.
                engine.focus_content(id);
            }
            EngineEvent::LoadingChanged {
                id,
                loading,
                can_go_back,
                can_go_forward,
            } => {
                self.with_state(|s| {
                    if let Some(rt) = s.page_runtime(id) {
                        rt.loading = loading;
                        rt.can_go_back = can_go_back;
                        rt.can_go_forward = can_go_forward;
                    }
                });
                self.show_state_paced(engine);
            }
            EngineEvent::OpenUrlRequested {
                url,
                background,
                new_window,
                own_window,
                locked,
                from,
                source,
                context,
            } => {
                let source = match source {
                    UrlSource::Page => url_policy::Source::Page,
                    UrlSource::Ui => url_policy::Source::User,
                    UrlSource::OtherApp => url_policy::Source::OtherApp,
                    UrlSource::Chromium => url_policy::Source::Chromium,
                };
                let (target, from_mini) = self.with_state(|s| {
                    let from_peek = from.is_some() && from == s.peek.as_ref().map(|p| p.content);
                    let ask = Ask {
                        source,
                        locked,
                        new_window,
                        own_window,
                        background,
                        from_peek,
                    };
                    let from_mini = from.is_some_and(|c| s.minis.iter().any(|m| m.content == c));
                    (ask.target(&s.settings.external_links), from_mini)
                });
                match target {
                    Target::Peek => self.open_peek(engine, &url, source, context.as_deref()),
                    Target::Mini => self.open_mini(engine, &url, source, context.as_deref()),
                    Target::Tab => {
                        self.open_url_from(engine, &url, background, source, context.as_deref());
                    }
                }
                // A link from another app, or what a mini window's page
                // opened, is in the main window: it comes to the front.
                let outside = source == url_policy::Source::OtherApp || from_mini;
                if outside && target != Target::Mini && !background {
                    engine.activate_frame();
                }
            }
            // Keys and menu bar items from a mini window act on its page.
            EngineEvent::Shortcut {
                from: Some(id),
                shortcut,
            } if self.is_mini(id) => self.mini_shortcut(engine, id, shortcut),
            EngineEvent::MiniToTab { id, space } => self.mini_to_tab(engine, id, space.as_deref()),
            // Keys from a page window act on that page (peek, split pane).
            EngineEvent::Shortcut {
                shortcut: Shortcut::CopyLink,
                from,
            } => {
                let result = self.copy_link(engine, from, LinkVariant::Clean);
                Self::ui_call(engine, "copied", &result);
            }
            EngineEvent::Shortcut {
                shortcut: Shortcut::Capture,
                from,
            } => self.capture_start(engine, from),
            EngineEvent::Shortcut {
                shortcut: Shortcut::ToggleDevTools,
                from,
            } => self.toggle_devtools(engine, from),
            EngineEvent::Shortcut {
                shortcut: Shortcut::Print,
                from,
            } => {
                if let Some(c) = self.focused_content(from) {
                    engine.content_print(c);
                }
            }
            EngineEvent::Shortcut {
                shortcut: Shortcut::OpenDevTools,
                from,
            } => {
                if let Some(c) = self.focused_content(from) {
                    engine.content_show_devtools(c);
                }
            }
            EngineEvent::Shortcut { shortcut, .. } => self.handle_shortcut(engine, shortcut),
            EngineEvent::SiteChanged { id, state } => {
                self.with_state(|s| {
                    if let Some(rt) = s.page_runtime(id) {
                        rt.site = Some(state);
                    }
                });
                self.show_state_paced(engine);
            }
            EngineEvent::DevToolsMenu { id, action } => match action {
                DevToolsAction::Close => engine.content_close_devtools(id),
                DevToolsAction::Dock(dock) => {
                    // The choice becomes the setting, as in Settings → Developer.
                    self.with_state(|s| s.settings.devtools_dock = dock);
                    Self::apply_devtools_dock(engine, dock);
                    self.push_state(engine);
                }
            },
            EngineEvent::SplitDivider {
                path,
                index,
                action,
            } => match action {
                DividerAction::Move(pos) => self.split_resize(engine, &path, index, pos),
                // Sizes are saved once, when the drag ends.
                DividerAction::Release => self.push_state(engine),
                DividerAction::Equalize => self.split_equalize(engine, &path),
            },
            EngineEvent::ContentFocused { id } => {
                // Clicking into a split page makes it the active tab, so the
                // address row and sidebar follow the page you work in.
                let changed = self.with_state(|s| {
                    let tab = s.content_to_tab.get(&id).cloned()?;
                    let in_split = s.visible_split().is_some_and(|t| t.contains(&tab));
                    let space = s.active_space.clone();
                    (in_split && s.active_tab.get(&space) != Some(&tab))
                        .then(|| s.active_tab.insert(space, tab))
                });
                if changed.is_some() {
                    self.push_state(engine);
                }
            }
            EngineEvent::MediaAccessChanged {
                id,
                camera,
                microphone,
            } => {
                self.with_state(|s| {
                    if let Some(rt) = s.page_runtime(id) {
                        rt.camera = camera;
                        rt.microphone = microphone;
                    }
                });
                self.show_state_paced(engine);
            }
            EngineEvent::QuitRequested => {
                self.with_state(|s| {
                    s.persist();
                    s.history.save();
                });
                engine.quit();
            }
            EngineEvent::MemoryPressure(level) => self.unload_under_pressure(engine, level),
            EngineEvent::Tick => {
                // Installed or removed extensions show up in the sidebar.
                let extensions = self.front_extensions(engine);
                let shown = lock(&self.shown_extensions).clone();
                if extensions != shown {
                    self.show_state(engine);
                }
                self.housekeeping(engine);
                self.smart_refresh_due(engine);
                self.update_tick(engine);
                static TICKS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                if TICKS
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    .is_multiple_of(5)
                {
                    self.sync_now();
                }
            }
        }
    }

    fn on_ui_query(&self, engine: &Engine, origin: &str, request: &str) -> Result<String, String> {
        // Only the command's name: requests carry tokens, sync words,
        // invites and the user's addresses, none of which belong in a log.
        trace(|| {
            let name = serde_json::from_str::<Value>(request)
                .ok()
                .and_then(|v| v.get("cmd").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_default();
            format!("query {name}")
        });
        let cmd: UiCommand =
            serde_json::from_str(request).map_err(|e| format!("bad command: {e}"))?;
        // The sync code and the chain's lifecycle belong to Settings and to
        // the welcome page, which shows the same controls at the first
        // start: a flaw in any other page of ours must not reach them.
        let settings_only = matches!(
            cmd,
            UiCommand::SyncCode
                | UiCommand::SyncCreate { .. }
                | UiCommand::SyncJoin { .. }
                | UiCommand::SyncLeave
                | UiCommand::SyncDeleteChain
        );
        let trusted = ["magnetite://ui/settings", WELCOME_URL];
        if settings_only && !trusted.iter().any(|page| origin.starts_with(page)) {
            return Err("only the Settings page may do that".into());
        }
        // Granting a site a permission is the prompt's job alone.
        if matches!(cmd, UiCommand::AnswerPermission { .. })
            && !origin.starts_with("magnetite://ui/notifications")
        {
            return Err("only the notification panel may do that".into());
        }
        let value = self.handle_command(engine, cmd)?;
        Ok(value.to_string())
    }
}

/// Where `OpenUrlRequested` opens an address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Tab,
    Peek,
    Mini,
}

/// What came with an `OpenUrlRequested`, as far as the choice of `Target`
/// goes.
struct Ask {
    source: url_policy::Source,
    locked: bool,
    new_window: bool,
    /// The user chose "Open Link in New Window".
    own_window: bool,
    background: bool,
    /// The peek asked: what it opens becomes a tab.
    from_peek: bool,
}

impl Ask {
    /// "Open Link in New Window" gets a mini window. A link from another app
    /// goes where Settings say (`external_links`:
    /// "tab", "peek" or "mini"). A page locked to its site, or one that asks
    /// for a new window, gets a peek, unless it is the peek itself or the
    /// link opens in the background. Everything else becomes a tab.
    fn target(&self, external_links: &str) -> Target {
        if self.own_window {
            return Target::Mini;
        }
        if self.source == url_policy::Source::OtherApp {
            match external_links {
                "peek" => return Target::Peek,
                "mini" => return Target::Mini,
                _ => {}
            }
        }
        if (self.locked || self.new_window) && !self.from_peek && !self.background {
            Target::Peek
        } else {
            Target::Tab
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Ask, Target};
    use crate::url_policy::Source;

    fn ask(source: Source) -> Ask {
        Ask {
            source,
            locked: false,
            new_window: false,
            own_window: false,
            background: false,
            from_peek: false,
        }
    }

    #[test]
    fn a_link_opened_in_a_new_window_gets_a_mini_window() {
        let chosen = Ask {
            own_window: true,
            locked: true,
            ..ask(Source::Page)
        };
        assert_eq!(chosen.target("tab"), Target::Mini);
    }

    #[test]
    fn links_from_other_apps_go_where_the_setting_says() {
        let link = ask(Source::OtherApp);
        assert_eq!(link.target("tab"), Target::Tab);
        assert_eq!(link.target("peek"), Target::Peek);
        assert_eq!(link.target("mini"), Target::Mini);
        // Only links from other apps: a page's or the UI's never.
        assert_eq!(ask(Source::Page).target("mini"), Target::Tab);
        assert_eq!(ask(Source::User).target("mini"), Target::Tab);
    }

    #[test]
    fn popups_and_locked_pages_peek_unless_the_peek_or_the_background_asks() {
        let popup = Ask {
            new_window: true,
            ..ask(Source::Page)
        };
        assert_eq!(popup.target("mini"), Target::Peek);
        let from_peek = Ask {
            from_peek: true,
            ..popup
        };
        assert_eq!(from_peek.target("tab"), Target::Tab);
        let locked_background = Ask {
            locked: true,
            background: true,
            ..ask(Source::Page)
        };
        assert_eq!(locked_background.target("tab"), Target::Tab);
    }
}
