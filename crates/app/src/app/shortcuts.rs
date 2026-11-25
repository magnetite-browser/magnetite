//! Keys and menu bar items, and the command palette they open.

use magnetite_core::search::urlencode;
use magnetite_core::split::Side;
use magnetite_engine::{
    Action, Combo, ContentId, Engine, First, KeysError, Overrides, Priorities, Shortcut,
};
use serde_json::{Value, json};

use super::{LinkVariant, MagnetiteApp, PaletteMode};
use crate::state::typed_site;

impl MagnetiteApp {
    /// Hand the engine the keys the shortcuts have now and who gets them
    /// first, by shortcut and by site (Settings → Shortcuts, the site
    /// panel): at start, and whenever they change here or through sync.
    pub(super) fn apply_shortcut_keys(&self, engine: &Engine) {
        let (keys, first, sites) = self.with_state(|s| {
            let settings = &s.settings;
            (
                settings.shortcuts.clone(),
                settings.shortcut_priority.clone(),
                settings.shortcut_sites.clone(),
            )
        });
        engine.set_shortcut_keys(&keys, &first);
        magnetite_engine::set_page_first_sites(sites);
    }

    /// The site (registrable domain) of the page at `url`, if a web page,
    /// and whether its pages get the keys of every shortcut first.
    pub(super) fn site_shortcuts(&self, url: &str) -> Value {
        let site = typed_site(url);
        let first = (site.as_ref())
            .is_some_and(|site| self.with_state(|s| s.settings.shortcut_sites.contains(site)));
        json!({ "site": site, "page_first": first })
    }

    /// From now on the pages of the site `url` names (a page's address,
    /// or a name typed in Settings) get the keys of every shortcut first
    /// (`page_first`), or each shortcut's own choice counts there again.
    /// What names no site changes nothing.
    pub(super) fn set_site_shortcuts(&self, engine: &Engine, url: &str, page_first: bool) {
        let Some(site) = typed_site(url) else {
            return;
        };
        self.with_state(|s| {
            let sites = &mut s.settings.shortcut_sites;
            sites.retain(|s| *s != site);
            if page_first {
                sites.push(site);
                sites.sort();
            }
        });
        self.apply_shortcut_keys(engine);
        self.push_state(engine);
    }

    /// From now on a key of `action` pressed in a page goes to `first`
    /// before the other.
    pub(super) fn set_shortcut_first(&self, engine: &Engine, action: Action, first: First) {
        self.with_state(|s| s.settings.shortcut_priority.set(action, first));
        self.apply_shortcut_keys(engine);
        self.push_state(engine);
    }

    /// Give `action` the keys `keys` (`None`: no keys), or tell why not.
    /// Other keys take effect at once, in the menu bar too.
    pub(super) fn set_shortcut(
        &self,
        engine: &Engine,
        action: Action,
        keys: Option<&str>,
    ) -> Result<(), KeysError> {
        let combo: Option<Combo> = keys.map(str::parse).transpose()?;
        self.with_state(|s| {
            s.settings.shortcuts.set(action, combo)?;
            s.drop_shadowed_space_keys();
            Ok(())
        })?;
        self.apply_shortcut_keys(engine);
        self.push_state(engine);
        Ok(())
    }

    /// Give space `id` the keys `keys` (`None`: no keys), or tell why not.
    /// A shortcut or space that had them loses them.
    pub(super) fn set_space_shortcut(
        &self,
        engine: &Engine,
        id: &str,
        keys: Option<&str>,
    ) -> Result<(), KeysError> {
        let combo: Option<Combo> = keys.map(str::parse).transpose()?;
        self.with_state(|s| s.set_space_shortcut(id, combo))?;
        // The keys may have been an action's.
        self.apply_shortcut_keys(engine);
        self.push_state(engine);
        Ok(())
    }

    /// Give `action` its default keys back and let it go first to whom it
    /// does by default, or every action (`None`); then no space has keys
    /// of its own either, and no site gets the keys first.
    pub(super) fn reset_shortcut(&self, engine: &Engine, action: Option<Action>) {
        self.with_state(|s| {
            if let Some(action) = action {
                s.settings.shortcuts.reset(action);
                (s.settings.shortcut_priority).set(action, action.default_first());
            } else {
                s.settings.shortcuts = Overrides::default();
                s.settings.shortcut_priority = Priorities::default();
                s.settings.shortcut_sites.clear();
                for space in &mut s.sidebar.spaces {
                    space.shortcut = None;
                }
            }
            s.drop_shadowed_space_keys();
        });
        self.apply_shortcut_keys(engine);
        self.push_state(engine);
    }

    /// Open the command palette centred over the page area; the quick
    /// search in a window of its own, on the screen with the pointer.
    pub(super) fn open_palette(
        engine: &Engine,
        mode: PaletteMode,
        side: Option<Side>,
        tab: Option<&str>,
    ) {
        let mut url = format!("magnetite://ui/command?mode={}", mode.as_str());
        if mode == PaletteMode::Split {
            url.push_str("&side=");
            url.push_str(side.unwrap_or(Side::Right).as_str());
        }
        if let Some(tab) = tab {
            url.push_str("&tab=");
            url.push_str(&urlencode(tab));
        }
        if mode == PaletteMode::Quick {
            engine.open_quick_search(&url, 640, 56);
        } else {
            engine.open_palette(&url, 640, 56);
        }
    }

    /// Reload, history, zoom and find for the page in front (menu bar items).
    pub(super) fn page_command(&self, engine: &Engine, shortcut: Shortcut) {
        if let Some(c) = self.focused_content(None) {
            Self::page_command_on(engine, c, shortcut);
        }
    }

    /// Reload, history, zoom and find for page `c`.
    pub(super) fn page_command_on(engine: &Engine, c: ContentId, shortcut: Shortcut) {
        let zoom = || engine.content_site_state(c).map_or(100, |s| s.zoom);
        match shortcut {
            Shortcut::Reload => engine.content_reload(c),
            Shortcut::HardReload => engine.content_hard_reload(c),
            Shortcut::Back => engine.content_go_back(c),
            Shortcut::Forward => engine.content_go_forward(c),
            Shortcut::ZoomIn | Shortcut::ZoomOut => {
                let up = shortcut == Shortcut::ZoomIn;
                engine.set_content_zoom(c, Some(magnetite_engine::zoom_step(zoom(), up)));
            }
            Shortcut::ZoomReset => engine.set_content_zoom(c, None),
            Shortcut::Find | Shortcut::FindNext | Shortcut::FindPrevious => {
                engine.content_find(c, shortcut);
            }
            _ => {}
        }
    }

    pub(super) fn handle_shortcut(&self, engine: &Engine, shortcut: Shortcut) {
        match shortcut {
            Shortcut::ToggleDevTools => self.toggle_devtools(engine, None),
            Shortcut::Settings => {
                self.open_url(engine, "magnetite://ui/settings", false);
            }
            Shortcut::Downloads | Shortcut::History => {
                if !engine.sidebar_visible() {
                    self.handle_shortcut(engine, Shortcut::ToggleSidebar);
                }
                engine.focus_sidebar();
                let view = if shortcut == Shortcut::History {
                    "openHistory"
                } else {
                    "openDownloads"
                };
                Self::ui_call(engine, view, &json!({}));
            }
            Shortcut::Print => {
                if let Some(c) = self.focused_content(None) {
                    engine.content_print(c);
                }
            }
            Shortcut::Reload
            | Shortcut::HardReload
            | Shortcut::Back
            | Shortcut::Forward
            | Shortcut::ZoomIn
            | Shortcut::ZoomOut
            | Shortcut::ZoomReset
            | Shortcut::Find
            | Shortcut::FindNext
            | Shortcut::FindPrevious => self.page_command(engine, shortcut),
            Shortcut::DevPanel(panel) => self.dev_panel(engine, None, panel),
            Shortcut::DevReset => self.dev_reset(engine, None),
            Shortcut::CopyLink => {
                let result = self.copy_link(engine, None, LinkVariant::Clean);
                Self::ui_call(engine, "copied", &result);
            }
            Shortcut::Capture => self.capture_start(engine, None),
            Shortcut::NewSplit => {
                Self::open_palette(engine, PaletteMode::Split, Some(Side::Right), None);
            }
            Shortcut::SitePanel => {
                // The panel is part of the sidebar: a hidden one comes back.
                if !engine.sidebar_visible() {
                    self.handle_shortcut(engine, Shortcut::ToggleSidebar);
                }
                engine.focus_sidebar();
                Self::ui_call(engine, "openSitePanel", &json!({}));
            }
            Shortcut::NewTab => Self::open_palette(engine, PaletteMode::New, None, None),
            // Without several windows (Settings) the key keeps what it did
            // before: a new tab.
            Shortcut::NewWindow => {
                if self.with_state(|s| s.settings.several_windows) {
                    self.new_window(engine);
                } else {
                    self.handle_shortcut(engine, Shortcut::NewTab);
                }
            }
            Shortcut::CloseWindow => {
                // With one window the key keeps what it did before there
                // were several: it closes the tab.
                if engine.windows().len() > 1 {
                    engine.close_window(engine.current_window());
                } else {
                    self.handle_shortcut(engine, Shortcut::CloseTab);
                }
            }
            Shortcut::FocusAddress => {
                let has_tab = self.with_state(|s| s.active_tab_id().is_some());
                let mode = if has_tab {
                    PaletteMode::Navigate
                } else {
                    PaletteMode::New
                };
                Self::open_palette(engine, mode, None, None);
            }
            Shortcut::CloseTab => {
                // A peek is in front: the key closes it, not the tab behind it.
                if self.with_state(|s| s.peek.is_some()) {
                    self.close_peek(engine);
                } else if engine.web_panel_focused() {
                    // The keyboard is in the web panel: the key is for it.
                    self.close_web_panel(engine);
                } else if let Some(id) = self.with_state(|s| s.active_tab_id().map(str::to_owned)) {
                    self.close_tab(engine, &id);
                }
            }
            Shortcut::ToggleSidebar => {
                let visible = !engine.sidebar_visible();
                engine.set_sidebar_visible(visible);
                self.with_state(|s| s.sidebar_visible = visible);
                // Keys must keep reaching Magnetite: a hidden sidebar gets none.
                if !visible {
                    self.on_active_content(engine, Engine::focus_content);
                }
                self.push_state(engine);
            }
            Shortcut::WebPanel => self.web_panel_key(engine),
            Shortcut::NextTab | Shortcut::PrevTab => {
                self.cycle_tab(engine, shortcut == Shortcut::NextTab);
            }
            Shortcut::SelectTab(n) => {
                // Tabs count from 1; a 0 from a page names none.
                let target = self.with_state(|s| {
                    let at = usize::from(n).checked_sub(1)?;
                    s.visible_tab_ids().get(at).cloned()
                });
                if let Some(t) = target {
                    self.activate_tab(engine, &t);
                }
            }
            Shortcut::SelectSpace(n) => {
                // Spaces count from 1, in the footer's order.
                let target = self.with_state(|s| {
                    let at = usize::from(n).checked_sub(1)?;
                    let space = s.listed_spaces().nth(at)?;
                    (space.id != s.active_space).then(|| space.id.clone())
                });
                if let Some(id) = target {
                    self.activate_space(engine, &id);
                }
            }
            Shortcut::ReopenClosed => self.reopen_closed(engine),
            Shortcut::TogglePin => {
                let active = self.with_state(|s| {
                    // A peek is in front: the key is not for the tab behind it.
                    if s.peek.is_some() {
                        return None;
                    }
                    let id = s.active_tab_id()?.to_owned();
                    // Pinned by where it is: in its space, but not in Today.
                    let pinned = s.find_tab(&id).is_some_and(|(space, _)| {
                        space.is_some_and(|sp| !sp.today.iter().any(|t| t.id == id))
                    });
                    Some((id, pinned))
                });
                if let Some((id, pinned)) = active {
                    self.set_pinned(engine, &id, !pinned);
                }
            }
            Shortcut::NewSpace => Self::open_palette(engine, PaletteMode::NewSpace, None, None),
            Shortcut::SpacePicker => Self::open_palette(engine, PaletteMode::Spaces, None, None),
            Shortcut::SpaceKeys(keys) => {
                let target = self.with_state(|s| {
                    let space = s.space_for_keys(keys)?;
                    (space.id != s.active_space).then(|| space.id.clone())
                });
                if let Some(id) = target {
                    self.activate_space(engine, &id);
                }
            }
            Shortcut::PrivateSpace => self.open_private_space(engine),
            Shortcut::ClosePrivateSpace => self.close_private_space(engine),
            Shortcut::HidePrivateSpace => self.hide_private_space(engine),
            Shortcut::NewMini => Self::open_palette(engine, PaletteMode::Quick, None, None),
            Shortcut::ClosePeek => self.close_peek(engine),
            Shortcut::PeekToTab => self.peek_to_tab(engine),
            Shortcut::PeekToSplit => self.peek_to_split(engine),
            Shortcut::NextSpace | Shortcut::PrevSpace => {
                let target = self.with_state(|s| {
                    // A hidden private space is no stop on the way.
                    let spaces: Vec<&str> = s.listed_spaces().map(|sp| sp.id.as_str()).collect();
                    let n = spaces.len();
                    let pos = spaces
                        .iter()
                        .position(|id| *id == s.active_space)
                        .unwrap_or(0);
                    let next = if shortcut == Shortcut::NextSpace {
                        (pos + 1) % n
                    } else {
                        (pos + n - 1) % n
                    };
                    spaces[next].to_string()
                });
                self.activate_space(engine, &target);
            }
        }
    }
}
