//! The application: reacts to engine events and sidebar commands, keeps
//! `AppState` consistent, and drives content windows.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, Weak};

use magnetite_engine::{ContentId, Engine, WindowId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::state::{AppState, ArcImport};
use crate::sync::SyncClient;
use crate::{capture, lock};

mod adblocking;
mod addresses;
mod commands;
mod dev;
mod engines;
mod events;
mod extensions;
mod mini;
mod panels;
mod peek;
mod record;
mod screenshots;
mod shortcuts;
mod site;
mod smart_folders;
mod split_view;
mod syncing;
mod tabs;

mod updates;

pub(crate) use events::launch_url;

/// What UI pages get of the state: without bookkeeping they never need (sync
/// versions, Chromium pins seen, smart folders allowed here), so less of it
/// can leak from any page.
fn for_ui(mut state: Value) -> Value {
    if let Some(settings) = state.get_mut("settings").and_then(Value::as_object_mut) {
        settings.remove("chromium_pins_seen");
        settings.remove("smart_allowed");
        if let Some(sync) = settings.get_mut("sync").and_then(Value::as_object_mut) {
            sync.remove("versions");
        }
    }
    state
}

/// The state as the sidebar gets it, with the recording that runs
/// (`recording`, `Recorder::for_ui`).
fn ui_json(
    state: &AppState,
    extensions: &[magnetite_engine::ExtensionInfo],
    recording: Value,
) -> Value {
    let mut ui = for_ui(json!(state.ui_state(extensions)));
    ui["recording"] = recording;
    ui
}

/// Where "Chrome Web Store…" goes: its extensions section.
const EXTENSION_STORE: &str = "https://chromewebstore.google.com/category/extensions";

pub struct MagnetiteApp {
    state: Mutex<AppState>,
    data_dir: PathBuf,
    import: ArcImport,
    open_on_start: Option<String>,
    /// Addresses from this start's command line, opened as links from
    /// another app once the sidebar is up.
    launch_urls: Vec<String>,
    sync: OnceLock<SyncClient>,
    /// A sync chain is being started or joined: a second request would make
    /// a second chain and replace the stored sync code.
    sync_connecting: AtomicBool,
    /// Content windows shown by the last `present`, per window, to detect
    /// tab switches.
    visible: Mutex<HashMap<WindowId, Vec<ContentId>>>,
    capture: Mutex<CaptureState>,
    /// The extensions the sidebar was last shown (refreshed on tick).
    shown_extensions: Mutex<Vec<magnetite_engine::ExtensionInfo>>,
    split_drag: Mutex<SplitDrag>,
    updater: Mutex<updates::Updater>,
    prompts: Mutex<Prompts>,
    /// A save is on its way (`save_soon`).
    save_queued: AtomicBool,
    show_pace: Mutex<ShowPace>,
    blocker: adblocking::Blocker,
    /// The recording of a page that runs (`record.rs`).
    record: Mutex<record::Recorder>,
    /// The spaces' own keys the engine was last told (`apply_space_keys`).
    space_keys: Mutex<Vec<magnetite_engine::Combo>>,
    /// Sites that moved to the system's engine from a private space
    /// (`engines.rs`): for this run only, never written anywhere.
    private_system_sites: Mutex<Vec<String>>,
}

/// When the sidebar was last sent the state, and whether another sending
/// is on its way (`show_state_paced`).
#[derive(Default)]
struct ShowPace {
    shown: Option<std::time::Instant>,
    queued: bool,
}

/// The longest a change waits to be saved.
const SAVE_EVERY_MS: i64 = 1000;

/// The shortest time between two sendings of the state that follow page
/// events: short enough to look immediate.
const SHOW_EVERY: std::time::Duration = std::time::Duration::from_millis(100);

/// Permission prompts waiting for an answer. The notification panel shows
/// those of the pages on screen; the others wait until their page is.
#[derive(Default)]
struct Prompts {
    open: Vec<magnetite_engine::PermissionPrompt>,
    /// The list the panel was last sent (JSON), so only changes go out.
    shown: String,
    /// When that list last changed: cards appeared or moved.
    changed_at: Option<std::time::Instant>,
}

/// How long the cards must stay as they are before Allow counts. The panel
/// holds its buttons as long, so a page that asks just before a click it
/// expects (on its own content under the card) cannot have it grant itself
/// a permission.
const PROMPT_SETTLE: std::time::Duration = std::time::Duration::from_secs(1);

/// The page of the first start: sync, import, a few keys (`Welcome.tsx`).
const WELCOME_URL: &str = "magnetite://ui/welcome";
/// The page behind "All downloads" (`DownloadsPage.tsx`).
const DOWNLOADS_URL: &str = "magnetite://ui/downloads";

/// A sidebar tab being dragged over the page area (drop-zone overlay).
#[derive(Default)]
struct SplitDrag {
    /// Bumped per drag, so screenshots of an ended drag are ignored.
    generation: u64,
    tab: Option<String>,
    /// Visible pages: tab id, bounds in the page area, screenshot.
    panes: Vec<(String, [i32; 4], Option<String>)>,
    overlay_open: bool,
    /// The tab of a drag the sidebar just called ended, and when: its drop
    /// is reported by another page and can arrive a moment later.
    ended: Option<(String, std::time::Instant)>,
}

/// How long after "drag ended" a drop of that drag still counts.
const LATE_DROP: std::time::Duration = std::time::Duration::from_secs(1);

/// The running capture session (frozen frame of one page) and the last result.
#[derive(Default)]
struct CaptureState {
    session: Option<(ContentId, magnetite_engine::FrozenFrame)>,
    /// A capture set is being taken: the page changes size and colours.
    set_running: bool,
    result: Option<capture::Capture>,
}

static GLOBAL: OnceLock<Weak<MagnetiteApp>> = OnceLock::new();

/// Make the app reachable from background workers (sync outcomes).
pub fn install_global(app: &std::sync::Arc<MagnetiteApp>) {
    let _ = GLOBAL.set(std::sync::Arc::downgrade(app));
}

fn global() -> Option<std::sync::Arc<MagnetiteApp>> {
    GLOBAL.get().and_then(Weak::upgrade)
}

/// What "copy link" puts on the clipboard.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum LinkVariant {
    /// Trackers removed (if the setting is on).
    #[default]
    Clean,
    /// Exactly the page's URL.
    Original,
    /// `[title](clean url)`.
    Markdown,
}

/// What the centred command palette (`magnetite://ui/command`) is opened for.
/// Every command-bar prompt uses it, so they all appear in the same place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PaletteMode {
    /// Open something in a new tab.
    New,
    /// Open something next to the current tab (`side`).
    Split,
    /// Edit the current tab's address.
    Navigate,
    /// Name a new space.
    NewSpace,
    /// Name a new profile for the active space, or for a favorite (`tab`).
    NewProfile,
    /// The quick search, from anywhere: the pick opens in a mini window.
    Quick,
    /// The spaces by name, to go to one.
    Spaces,
}

impl PaletteMode {
    /// The `mode` query value the palette page reads.
    fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Split => "split",
            Self::Navigate => "navigate",
            Self::NewSpace => "new_space",
            Self::NewProfile => "new_profile",
            Self::Quick => "quick",
            Self::Spaces => "spaces",
        }
    }
}

impl MagnetiteApp {
    /// The languages websites are asked for, as the engine takes them at
    /// its start.
    pub fn accept_languages(&self) -> String {
        self.with_state(|s| s.settings.languages.accept_languages())
    }

    pub fn new(
        data_dir: PathBuf,
        scriptlets: Option<PathBuf>,
        import: ArcImport,
        open_on_start: Option<String>,
        launch_urls: Vec<String>,
    ) -> Self {
        let state = AppState::load_or_default(&data_dir, import);
        let blocker = adblocking::Blocker::start(
            &data_dir,
            scriptlets,
            &state.settings.adblock,
            state.first_start,
        );
        Self {
            state: Mutex::new(state),
            data_dir,
            import,
            open_on_start,
            launch_urls,
            sync: OnceLock::new(),
            sync_connecting: AtomicBool::new(false),
            visible: Mutex::new(HashMap::new()),
            capture: Mutex::new(CaptureState::default()),
            shown_extensions: Mutex::new(Vec::new()),
            split_drag: Mutex::new(SplitDrag::default()),
            updater: Mutex::new(updates::Updater::new()),
            prompts: Mutex::new(Prompts::default()),
            save_queued: AtomicBool::new(false),
            show_pace: Mutex::new(ShowPace::default()),
            blocker,
            record: Mutex::new(record::Recorder::default()),
            space_keys: Mutex::new(Vec::new()),
            private_system_sites: Mutex::new(Vec::new()),
        }
    }

    /// The pages on screen, in any window.
    fn on_screen(&self) -> Vec<ContentId> {
        lock(&self.visible).values().flatten().copied().collect()
    }

    /// Live content window of a tab.
    fn content_of(&self, tab_id: &str) -> Option<ContentId> {
        self.with_state(|s| s.runtime.get(tab_id).and_then(|r| r.content))
    }

    /// Work on the state, as the engine's current window sees it: the
    /// state follows whenever the engine has moved on to another window.
    fn with_state<R>(&self, f: impl FnOnce(&mut AppState) -> R) -> R {
        let mut guard = lock(&self.state);
        if let Some(engine) = Engine::try_global() {
            guard.enter(engine.current_window());
        }
        f(&mut guard)
    }

    /// The state changed: show it in the sidebar and save it, both soon
    /// rather than at once. A loading page reports a dozen changes within
    /// a second, and each save waits for the disk.
    fn push_state(&self, engine: &Engine) {
        // A private tab that closed takes its profile along.
        let released = self.with_state(AppState::release_private_profiles);
        if !released.is_empty() {
            engine.forget_contexts(&released);
        }
        self.save_soon();
        self.show_state_paced(engine);
    }

    /// Something an open Settings page shows changed outside of it (a
    /// switch on one of Chromium's pages, the spaces in the sidebar): the
    /// page asks for its data again (`SettingsPage.tsx`).
    fn tell_settings_pages(&self, engine: &Engine) {
        let js = r#"window.dispatchEvent(new Event("magnetite-settings-changed"))"#;
        for page in self.own_page_tabs("magnetite://ui/settings") {
            engine.content_ui_eval(page, js);
        }
    }

    /// Save the state within `SAVE_EVERY`, on the saver's thread. Quitting
    /// saves at once (`EngineEvent::QuitRequested`).
    fn save_soon(&self) {
        if self.save_queued.swap(true, Ordering::Relaxed) {
            return;
        }
        Engine::post_delayed(
            Box::new(|| {
                let Some(app) = global() else { return };
                app.save_queued.store(false, Ordering::Relaxed);
                app.with_state(|s| s.persist_later());
            }),
            SAVE_EVERY_MS,
        );
    }

    /// `show_state`, but at most once per `SHOW_EVERY`: the first change
    /// shows at once, those that follow closely show together.
    fn show_state_paced(&self, engine: &Engine) {
        let wait = {
            let mut pace = lock(&self.show_pace);
            if pace.queued {
                return;
            }
            let wait = (pace.shown)
                .and_then(|at| SHOW_EVERY.checked_sub(at.elapsed()))
                .filter(|wait| !wait.is_zero());
            pace.queued = wait.is_some();
            wait
        };
        match wait {
            None => self.show_state(engine),
            Some(wait) => Engine::post_delayed(
                Box::new(|| {
                    // Shown in the meantime by someone else: nothing to do.
                    if let Some(app) = global()
                        && lock(&app.show_pace).queued
                    {
                        app.show_state(Engine::global());
                    }
                }),
                i64::try_from(wait.as_millis()).unwrap_or(0).max(1),
            ),
        }
    }

    /// Send the state to the sidebar without saving it (status only, such as
    /// a smart folder refreshing). It saves only when
    /// it follows extensions newly pinned or unpinned in Chromium's own UI
    /// (`AppState::follow_chromium_pins`).
    fn show_state(&self, engine: &Engine) {
        *lock(&self.show_pace) = ShowPace {
            shown: Some(std::time::Instant::now()),
            queued: false,
        };
        // Every window shows the sidebar its own way; the current one last,
        // for what there is one of (the extension menu, the mini windows'
        // list of spaces).
        let current = engine.current_window();
        let mut windows = engine.windows();
        windows.retain(|win| *win != current);
        windows.push(current);
        for win in windows {
            engine.in_window(win, || self.show_window_state(engine));
        }
        self.apply_space_keys(engine);
        // Another tab, split or peek on screen: other prompts may show.
        self.show_prompts(engine);
    }

    /// Tell the engine the keys that spaces have of their own, when they
    /// changed: here, by a space that came or went, or through sync.
    fn apply_space_keys(&self, engine: &Engine) {
        let keys: Vec<_> =
            self.with_state(|s| s.space_keys().into_iter().map(|(_, keys)| keys).collect());
        let mut told = lock(&self.space_keys);
        if *told != keys {
            engine.set_space_keys(&keys);
            *told = keys;
        }
    }

    /// The current window's part of `show_state`.
    fn show_window_state(&self, engine: &Engine) {
        self.settle_web_panel(engine);
        let context = self.with_state(|s| s.front_context());
        let extensions = engine.extensions(context.as_ref());
        let chromium: Vec<String> = extensions
            .iter()
            .filter(|e| e.chromium_pinned)
            .map(|e| e.id.clone())
            .collect();
        let profile = context.as_ref().map(|c| c.id.clone()).unwrap_or_default();
        self.with_state(|s| {
            if s.follow_chromium_pins(&profile, chromium) {
                s.persist();
            }
        });
        let recording = lock(&self.record).for_ui();
        let (json, chrome, accent, pinned) = self.with_state(|s| {
            (
                ui_json(s, &extensions, recording).to_string(),
                s.chrome_rgb(),
                s.accent_rgb(),
                s.pins(&profile).to_vec(),
            )
        });
        engine.set_extension_menu(context, pinned);
        engine.set_dev_bar_visible(self.dev_bar_wanted());
        // "Open in" of mini windows: the active space first.
        let spaces = self.with_state(|s| {
            // Not the private space: a page from elsewhere would not be
            // private there.
            let mut spaces: Vec<(String, String)> = (s.sidebar.spaces.iter())
                .filter(|sp| !s.is_private_space(&sp.id))
                .map(|sp| (sp.id.clone(), sp.title.clone()))
                .collect();
            if let Some(i) = spaces.iter().position(|(id, _)| *id == s.active_space) {
                spaces[..=i].rotate_right(1);
            }
            spaces
        });
        engine.set_mini_spaces(spaces);
        *lock(&self.shown_extensions) = extensions;
        engine.set_chrome_color(0xFF00_0000 | chrome);
        engine.set_accent_color(0xFF00_0000 | accent);
        engine.ui_eval_all(&format!(
            "window.__magnetite && window.__magnetite.setState({json})"
        ));
    }

    /// Send the download list to both sidebars, the notification panel and
    /// open "All downloads" tabs.
    fn show_downloads(&self, engine: &Engine) {
        let list = self.with_state(|s| s.downloads.for_ui());
        let js = format!("window.__magnetite && window.__magnetite.downloads({list})");
        engine.ui_eval_all(&js);
        engine.notifications_eval(&js);
        for page in self.own_page_tabs(DOWNLOADS_URL) {
            engine.content_ui_eval(page, &js);
        }
    }

    /// The loaded tabs showing Magnetite's own page `url`: news such a page
    /// shows but does not ask for goes to them.
    fn own_page_tabs(&self, url: &str) -> Vec<ContentId> {
        self.with_state(|s| {
            s.sidebar
                .all_tabs()
                .filter(|t| t.url.starts_with(url))
                .filter_map(|t| s.runtime.get(&t.id)?.content)
                .collect()
        })
    }

    fn ui_call(engine: &Engine, method: &str, arg: &Value) {
        engine.ui_eval(&format!(
            "window.__magnetite && window.__magnetite.{method}({arg})"
        ));
    }

    /// The page a command acts on: the page window a key came from, else the
    /// open peek, else the web panel while the keyboard is in it, else the
    /// active tab.
    fn focused_content(&self, from: Option<ContentId>) -> Option<ContentId> {
        from.or_else(|| {
            let panel = Engine::try_global()
                .filter(|e| e.web_panel_focused())
                .and_then(Engine::web_panel_content);
            self.with_state(|s| {
                (s.peek.as_ref().map(|p| p.content)).or(panel).or_else(|| {
                    s.active_tab_id()
                        .and_then(|t| s.runtime.get(t))
                        .and_then(|r| r.content)
                })
            })
        })
    }

    /// Run `f` on the page in front: an open peek, a web panel with the
    /// keyboard, else the active tab.
    fn on_active_content(&self, engine: &Engine, f: impl FnOnce(&Engine, ContentId)) {
        if let Some(c) = self.focused_content(None) {
            f(engine, c);
        }
    }
}
