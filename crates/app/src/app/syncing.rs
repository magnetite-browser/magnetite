//! Sync as the app sees it: starting or joining a chain, each round, and
//! what its outcomes change.

use magnetite_engine::Engine;

use crate::devices::{DEVICES, Me, RemoteTab};
use crate::state::{SyncSettings, now};
use crate::sync::{self, Job, Outcome, SyncClient};
use crate::url_policy;

use super::{MagnetiteApp, global};

impl MagnetiteApp {
    pub(super) fn sync_client(&self) -> &SyncClient {
        self.sync.get_or_init(|| {
            let settings = self.with_state(|s| s.settings.sync.clone());
            SyncClient::start(
                self.data_dir.clone(),
                settings,
                std::sync::Arc::new(|outcome| {
                    if let Some(app) = global() {
                        app.on_sync_outcome(Engine::global(), outcome);
                    }
                }),
            )
        })
    }

    /// Start or join a sync chain with `job`. Refused while this device is
    /// in a chain or still connecting to one: either would replace the
    /// stored sync code.
    pub(super) fn sync_connect(&self, job: Job) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        if self.sync_connecting.swap(true, Ordering::SeqCst) {
            return Err("Still connecting to the sync server.".into());
        }
        if sync::signed_in(&self.data_dir) {
            if self.with_state(|s| s.settings.sync.enabled) {
                self.sync_connecting.store(false, Ordering::SeqCst);
                return Err("This device is in a sync chain already. Leave it first.".into());
            }
            // A sync code without sync being on (the state was started
            // fresh, or the app closed in the middle of joining): nothing
            // shows a way to leave that chain, so it is left now.
            self.sync_client().submit(Job::Leave);
            self.sync_connecting.store(true, Ordering::SeqCst);
        }
        self.with_state(|s| s.settings.sync.last_error = None);
        self.sync_client().submit(job);
        Ok(())
    }

    /// Push local data and pull remote changes, if sync is on.
    pub(super) fn sync_now(&self) {
        // A chain deletion the server did not confirm is tried again by the
        // sync thread when it starts; a device that left has no other
        // reason to start it.
        if sync::deletion_pending(&self.data_dir) {
            self.sync_client();
        }
        let job = self.with_state(|s| {
            if !s.settings.sync.enabled || !sync::signed_in(s.data_dir()) {
                return None;
            }
            let snapshot = s.sidebar_for_sync();
            if s.settings.sync.device_id.is_empty() {
                s.settings.sync.device_id = crate::state::new_id();
            }
            let me = Me {
                id: s.settings.sync.device_id.clone(),
                // The kind of device is all the others need to tell them apart.
                name: if cfg!(target_os = "macos") {
                    "Mac"
                } else {
                    "Linux PC"
                }
                .into(),
                taken: s.settings.sync.taken.clone(),
            };
            Some(Job::Sync {
                me,
                sidebar_unchanged: s.synced_sidebar.as_ref() == Some(&snapshot),
                snapshot: Box::new(snapshot),
                settings: Box::new(s.settings.clone()),
            })
        });
        if let Some(job) = job {
            self.sync_client().submit(job);
        }
    }

    pub(super) fn on_sync_outcome(&self, engine: &Engine, outcome: Outcome) {
        // The answer to a start or join ends that attempt; the errors of a
        // round, or of an earlier chain's deletion, do not.
        if matches!(
            outcome,
            Outcome::Connected { .. } | Outcome::ConnectFailed(_)
        ) {
            self.sync_connecting
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
        match outcome {
            Outcome::Connected { server, created } => {
                self.with_state(|s| {
                    let sy = &mut s.settings.sync;
                    sy.enabled = true;
                    sy.server_url = server;
                    sy.versions.clear();
                    sy.last_error = None;
                    sy.started_here = created;
                    s.synced_sidebar = None;
                });
                self.push_state(engine);
                self.sync_now();
            }
            Outcome::Pushed { versions, .. } => {
                self.with_state(|s| {
                    for (name, v) in versions {
                        // A stored version never goes down (rollback protection).
                        let known = s.settings.sync.versions.entry(name).or_insert(0);
                        *known = (*known).max(v);
                    }
                    s.settings.sync.last_sync_at = Some(now());
                    s.settings.sync.last_error = None;
                });
                self.push_state(engine);
            }
            Outcome::Pulled {
                sidebar,
                settings,
                versions,
            } => {
                let contents = self.with_state(|s| s.adopt_pull(sidebar, settings, versions));
                for c in contents {
                    engine.close_content(c);
                }
                // Another device may have changed a shortcut's keys.
                self.apply_shortcut_keys(engine);
                // Shows what is open already; nothing new loads (`adopt_pull`).
                self.present(engine);
                self.push_state(engine);
            }
            Outcome::Devices {
                tabs,
                sent,
                forgotten,
                version,
            } => {
                let sent = self.with_state(|s| {
                    let sync = &mut s.settings.sync;
                    // A stored version never goes down (rollback protection).
                    let known = sync.versions.entry(DEVICES.into()).or_insert(0);
                    *known = (*known).max(version);
                    sync.taken.retain(|id| !forgotten.contains(id));
                    // Not those taken while this round was under way: with a
                    // click, or opened after an earlier round's answer.
                    let new = |t: &RemoteTab| !sync.taken.contains(&t.id);
                    let sent: Vec<_> = sent.into_iter().filter(new).collect();
                    s.device_tabs = tabs.into_iter().filter(new).collect();
                    sent
                });
                // Tabs sent here open by themselves, in the background.
                let any = !sent.is_empty();
                for tab in sent {
                    self.take_device_tab(engine, &tab, true);
                }
                self.push_state(engine);
                if any {
                    self.sync_now();
                }
            }
            Outcome::Error(msg) | Outcome::ConnectFailed(msg) => {
                self.with_state(|s| s.settings.sync.last_error = Some(msg));
                self.push_state(engine);
            }
            Outcome::Disconnected => {
                self.with_state(|s| {
                    s.settings.sync = SyncSettings::default();
                    s.synced_sidebar = None;
                    s.device_tabs.clear();
                });
                self.push_state(engine);
            }
        }
    }

    /// Take the tab with `id` over from the device it is open on (a click
    /// on its row): it opens here in front, and its device closes it.
    pub(super) fn take_device_tab_by_id(&self, engine: &Engine, id: &str) {
        let tab = self.with_state(|s| {
            let at = s.device_tabs.iter().position(|t| t.id == id)?;
            Some(s.device_tabs.remove(at))
        });
        if let Some(tab) = tab {
            self.take_device_tab(engine, &tab, false);
            self.push_state(engine);
            self.sync_now();
        }
    }

    /// Open `tab` of another device as a Today tab of its space (the space
    /// in front if this device does not have that one) and note it as taken,
    /// so its device closes it. `background`: unread, behind the tab in front.
    fn take_device_tab(&self, engine: &Engine, tab: &RemoteTab, background: bool) {
        let opened = self.with_state(|s| {
            // The round lists none this device does not open (`devices`);
            // checked again here, where the address is opened.
            if !url_policy::allowed(&tab.url, url_policy::Source::Stored) {
                return None;
            }
            s.settings.sync.taken.push(tab.id.clone());
            let space = s.device_tab_space(tab.space.as_ref());
            let title = if tab.title.is_empty() {
                &tab.url
            } else {
                &tab.title
            };
            let id = s.new_today_tab(&space, &tab.url, title);
            if background && let Some(t) = s.find_tab_mut(&id) {
                t.unread = true;
            }
            Some((space, id))
        });
        if let Some((space, id)) = opened
            && !background
        {
            if self.with_state(|s| s.active_space != space) {
                self.activate_space(engine, &space);
            }
            self.activate_tab(engine, &id);
        }
    }
}
