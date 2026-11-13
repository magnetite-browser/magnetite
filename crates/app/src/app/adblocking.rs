//! The ad blocker as the app runs it: the settings handed to the engine,
//! the lists downloaded and compiled on a thread of their own, and what
//! Settings and the site panel show. The lists are `adblock.rs`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::time::SystemTime;

use magnetite_engine::{Engine, Filters, registrable_domain};
use serde_json::{Value, json};

use super::MagnetiteApp;
use crate::{adblock, lock};

/// Where the lists stand, shared with the threads that fetch them.
#[derive(Default, Clone)]
pub(super) struct Blocker {
    /// Bumped per change of the settings: a thread that finishes after a
    /// newer one started does not hand over its (older) filters.
    generation: Arc<AtomicU64>,
    /// Threads at work, for Settings to show "Downloading…".
    working: Arc<AtomicUsize>,
    /// Why the last download of a list failed, per list id.
    errors: Arc<Mutex<HashMap<String, String>>>,
}

/// How often a tick (one a minute) looks for lists to fetch again
/// (`adblock::REFRESH_AFTER`): hourly.
const LOOK_EVERY_TICKS: u32 = 60;

impl MagnetiteApp {
    /// Hand the ad blocker settings to the engine: the allowed sites at
    /// once; the lists, fetched when missing or old, compiled on a thread.
    /// Off: nothing is blocked from now on, and nothing is fetched.
    pub(super) fn apply_adblock(&self) {
        let settings = self.with_state(|s| s.settings.adblock.clone());
        magnetite_engine::set_allowed_sites(settings.allowed_sites);
        let blocker = self.blocker.clone();
        let generation = blocker.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let dir = adblock::dir(&self.data_dir);
        if !settings.enabled {
            magnetite_engine::set_filters(None);
            return;
        }
        blocker.working.fetch_add(1, Ordering::SeqCst);
        std::thread::spawn(move || {
            adblock::remove_others(&dir, &settings.lists);
            for id in &settings.lists {
                let Some(list) = adblock::list(id) else {
                    continue;
                };
                if !adblock::due(&dir, id, SystemTime::now()) {
                    continue;
                }
                let result = adblock::fetch(&dir, list);
                let mut errors = lock(&blocker.errors);
                match result {
                    Ok(()) => errors.remove(id),
                    Err(e) => errors.insert(id.clone(), e),
                };
            }
            if blocker.generation.load(Ordering::SeqCst) == generation {
                let filters = Filters::compile(adblock::load(&dir, &settings.lists));
                // Checked again: compiling takes a moment.
                if blocker.generation.load(Ordering::SeqCst) == generation {
                    magnetite_engine::set_filters(Some(filters));
                }
            }
            blocker.working.fetch_sub(1, Ordering::SeqCst);
        });
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
            self.apply_adblock();
        }
    }

    /// The lists for Settings: each with when it was downloaded and why the
    /// last download failed, and whether downloads are under way.
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
                    "host": url::Url::parse(l.url).ok().and_then(|u| u.host_str().map(str::to_string)),
                    "fetched_at": fetched,
                    "error": errors.get(l.id),
                })
            })
            .collect();
        json!({ "lists": lists, "working": self.blocker.working.load(Ordering::SeqCst) > 0 })
    }

    /// Whether ads are blocked on the site of `host`, and that site.
    pub(super) fn site_ads(&self, host: &str) -> Value {
        let site = registrable_domain(host);
        let (enabled, allowed) = self.with_state(|s| {
            let a = &s.settings.adblock;
            (a.enabled, a.allowed_sites.contains(&site))
        });
        json!({ "site": site, "enabled": enabled, "blocking": enabled && !allowed })
    }

    /// Block ads on the site of `host` again, or let it load everything;
    /// the page in front loads again to show it.
    pub(super) fn set_site_ads(&self, engine: &Engine, host: &str, blocking: bool) {
        let site = registrable_domain(host);
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
