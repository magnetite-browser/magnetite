//! The ad blocker as the app runs it: the settings handed to the engine,
//! the lists downloaded and compiled on a thread of their own, and what
//! Settings and the site panel show. The lists are `adblock.rs`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::SystemTime;

use magnetite_engine::{Engine, Filters, host_of, registrable_domain};
use serde_json::{Value, json};

use super::MagnetiteApp;
use crate::state::AdblockSettings;
use crate::{adblock, lock};

/// Where the lists stand, shared with the threads that fetch them.
#[derive(Clone)]
pub(super) struct Blocker {
    /// The generation of the settings, bumped per change. Held while
    /// filters are handed over: a thread hands over its filters only if no
    /// newer change came, and turning blocking off cannot slip between its
    /// check and the hand-over.
    generation: Arc<Mutex<u64>>,
    /// Threads at work, for Settings to show "Downloading…".
    working: Arc<AtomicUsize>,
    /// Why the last download of a list failed, per list id.
    errors: Arc<Mutex<HashMap<String, String>>>,
    /// The lists from before compiled while the engine starts
    /// (`Blocker::start`), awaited before the first page is made.
    starting: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// The file with the scriptlets' code, where the package has one
    /// (`adblock.rs`).
    scriptlets: Arc<Option<PathBuf>>,
    /// A first start that has not opened a website yet: no list is
    /// fetched, so the welcome page alone contacts nobody
    /// (`website_opened`).
    held: Arc<AtomicBool>,
}

/// How often a tick (one a minute) looks for lists to fetch again
/// (`adblock::REFRESH_AFTER`): hourly.
const LOOK_EVERY_TICKS: u32 = 60;

impl Blocker {
    /// Compile the lists downloaded before, on a thread, while the engine
    /// starts: pages restored at start are blocked from their first request.
    /// `first_start`: downloads wait for the first website.
    pub(super) fn start(
        data_dir: &Path,
        scriptlets: Option<PathBuf>,
        settings: &AdblockSettings,
        first_start: bool,
    ) -> Self {
        let blocker = Self {
            held: Arc::new(AtomicBool::new(first_start)),
            scriptlets: Arc::new(scriptlets),
            generation: Arc::new(Mutex::new(0)),
            working: Arc::new(AtomicUsize::new(0)),
            errors: Arc::new(Mutex::new(HashMap::new())),
            starting: Arc::new(Mutex::new(None)),
        };
        if settings.enabled {
            let (b, dir, lists) = (
                blocker.clone(),
                adblock::dir(data_dir),
                settings.lists.clone(),
            );
            let compile = std::thread::spawn(move || b.compile_and_hand_over(0, &dir, &lists));
            *lock(&blocker.starting) = Some(compile);
        }
        blocker
    }

    /// Wait for the compile `start` began, if any.
    pub(super) fn started(&self) {
        if let Some(compile) = lock(&self.starting).take() {
            let _ = compile.join();
        }
    }

    fn is_current(&self, generation: u64) -> bool {
        *lock(&self.generation) == generation
    }

    /// Compile `lists` and block with them, unless the settings changed
    /// since `generation`.
    fn compile_and_hand_over(&self, generation: u64, dir: &Path, lists: &[String]) {
        // Without the scriptlets' file (a run from the checkout) the
        // lists' scriptlet rules stay unused.
        let scriptlets = (self.scriptlets.as_deref()).and_then(|p| std::fs::read_to_string(p).ok());
        let filters = Filters::compile(adblock::load(dir, lists), scriptlets.as_deref());
        let current = lock(&self.generation);
        if *current == generation {
            magnetite_engine::set_filters(Some(filters));
        }
    }
}

impl MagnetiteApp {
    /// Hand the ad blocker settings to the engine: the allowed sites at
    /// once; on a thread, the lists kept here compiled (`changed`: the
    /// settings changed, so those in use may be others), then the lists
    /// missing or old fetched and compiled again. Off: nothing is blocked
    /// from now on, and nothing is fetched. Nothing is fetched either
    /// before the first website of a first start (`website_opened`).
    pub(super) fn apply_adblock(&self, changed: bool) {
        let settings = self.with_state(|s| s.settings.adblock.clone());
        magnetite_engine::set_allowed_sites(settings.allowed_sites);
        let blocker = self.blocker.clone();
        let dir = adblock::dir(&self.data_dir);
        // Lists no longer chosen go, also while blocking is off.
        adblock::remove_others(&dir, &settings.lists);
        let generation = {
            let mut generation = lock(&blocker.generation);
            *generation += 1;
            if !settings.enabled {
                magnetite_engine::set_filters(None);
                return;
            }
            *generation
        };
        blocker.working.fetch_add(1, Ordering::SeqCst);
        std::thread::spawn(move || {
            let lists = &settings.lists;
            if changed {
                blocker.compile_and_hand_over(generation, &dir, lists);
            }
            let mut fetched = false;
            let held = blocker.held.load(Ordering::SeqCst);
            for id in lists.iter().filter(|_| !held) {
                // A newer change has its own thread: no download for this one.
                if !blocker.is_current(generation) {
                    break;
                }
                let Some(list) = adblock::list(id) else {
                    continue;
                };
                if !adblock::due(&dir, id, SystemTime::now()) {
                    continue;
                }
                let result = adblock::fetch(&dir, list);
                fetched |= result.is_ok();
                let mut errors = lock(&blocker.errors);
                match result {
                    Ok(()) => errors.remove(id),
                    Err(e) => errors.insert(id.clone(), e),
                };
            }
            if fetched {
                blocker.compile_and_hand_over(generation, &dir, lists);
            }
            blocker.working.fetch_sub(1, Ordering::SeqCst);
        });
    }

    /// A page went to `url`. The first website of a first start lets the
    /// lists be fetched: until then only Magnetite's own pages showed.
    pub(super) fn website_opened(&self, url: &str) {
        let held = &self.blocker.held;
        if held.load(Ordering::SeqCst)
            && host_of(url).is_some()
            && held.swap(false, Ordering::SeqCst)
        {
            self.apply_adblock(false);
        }
    }

    /// Turn the ad blocker on or off (Settings, the welcome page).
    pub(super) fn set_adblock_enabled(&self, enabled: bool) {
        self.with_state(|s| s.settings.adblock.enabled = enabled);
        self.apply_adblock(true);
    }

    /// Subscribe to list `id`, or not. One list per call, so quick choices
    /// made before the page heard back do not undo each other.
    pub(super) fn set_adblock_list(&self, id: &str, subscribed: bool) {
        if adblock::list(id).is_none() {
            return;
        }
        self.with_state(|s| {
            let lists = &mut s.settings.adblock.lists;
            lists.retain(|l| l != id);
            if subscribed {
                lists.push(id.to_string());
            }
        });
        self.apply_adblock(true);
    }

    /// On a tick now and then: fetch the lists again that are due.
    pub(super) fn adblock_tick(&self) {
        static TICKS: AtomicU32 = AtomicU32::new(0);
        if !TICKS
            .fetch_add(1, Ordering::Relaxed)
            .is_multiple_of(LOOK_EVERY_TICKS)
        {
            return;
        }
        let (enabled, lists) =
            self.with_state(|s| (s.settings.adblock.enabled, s.settings.adblock.lists.clone()));
        let dir = adblock::dir(&self.data_dir);
        let now = SystemTime::now();
        let due = lists.iter().any(|id| adblock::due(&dir, id, now));
        if enabled && due && self.blocker.working.load(Ordering::SeqCst) == 0 {
            self.apply_adblock(false);
        }
    }

    /// The lists for Settings: each with when it was downloaded and why the
    /// last download failed, whether downloads are under way or wait for
    /// the first website, and which lists are the recommended ones.
    pub(super) fn adblock_json(&self) -> Value {
        let dir = adblock::dir(&self.data_dir);
        let errors = lock(&self.blocker.errors).clone();
        let lists: Vec<Value> = (adblock::LISTS.iter())
            .map(|l| {
                let fetched = adblock::fetched_at(&dir, l.id)
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs());
                json!({
                    "id": l.id,
                    "name": l.name,
                    "about": l.about,
                    "host": l.urls.first().and_then(|u| url::Url::parse(u).ok()).and_then(|u| u.host_str().map(str::to_string)),
                    "fetched_at": fetched,
                    "error": errors.get(l.id),
                })
            })
            .collect();
        json!({
            "lists": lists,
            "working": self.blocker.working.load(Ordering::SeqCst) > 0,
            "held": self.blocker.held.load(Ordering::SeqCst),
            "recommended": adblock::system_recommended(),
        })
    }

    /// The site (registrable domain) of the page at `url`, if a web page.
    fn site_of(url: &str) -> Option<String> {
        host_of(url).map(|h| registrable_domain(&h))
    }

    /// Whether ads are blocked on the site of the page at `url`, and that
    /// site.
    pub(super) fn site_ads(&self, url: &str) -> Value {
        let Some(site) = Self::site_of(url) else {
            return json!({ "site": null, "enabled": false, "blocking": false });
        };
        let (enabled, allowed) = self.with_state(|s| {
            let a = &s.settings.adblock;
            (a.enabled, a.allowed_sites.contains(&site))
        });
        json!({ "site": site, "enabled": enabled, "blocking": enabled && !allowed })
    }

    /// Block ads on the site of the page at `url` again, or let it load
    /// everything; the page in front loads again to show it.
    pub(super) fn set_site_ads(&self, engine: &Engine, url: &str, blocking: bool) {
        let Some(site) = Self::site_of(url) else {
            return;
        };
        self.with_state(|s| {
            let sites = &mut s.settings.adblock.allowed_sites;
            sites.retain(|s| *s != site);
            if !blocking {
                sites.push(site);
                sites.sort();
            }
        });
        self.apply_adblock_sites();
        self.save_soon();
        self.on_active_content(engine, Engine::content_reload);
    }

    /// Hand the engine the allowed sites alone (no lists to compile).
    pub(super) fn apply_adblock_sites(&self) {
        let sites = self.with_state(|s| s.settings.adblock.allowed_sites.clone());
        magnetite_engine::set_allowed_sites(sites);
    }
}
