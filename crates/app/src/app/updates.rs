//! Release updates as the app runs them: when to check, the answers for
//! Settings and the notification panel, and the install. Checking a release
//! and replacing the app are `update.rs`.

use magnetite_engine::{ContentId, Engine, ReplyTicket};
use serde_json::{Value, json};

use super::{MagnetiteApp, global};
use crate::state::now;
use crate::{lock, update, url_policy};

/// Where the update check stands (`update.rs`).
pub(super) struct Updater {
    status: update::Status,
    /// The newer release found by the last check.
    found: Option<update::Manifest>,
    /// When Magnetite started and last checked (Unix seconds).
    started: i64,
    last_check: i64,
    /// The version the notification panel already announced.
    announced: Option<String>,
    /// "Check now" calls waiting for the result.
    waiting: Vec<ReplyTicket>,
}

impl Updater {
    pub(super) fn new() -> Self {
        Self {
            status: update::Status::Idle,
            found: None,
            started: now(),
            last_check: 0,
            announced: None,
            waiting: Vec::new(),
        }
    }
}

impl MagnetiteApp {
    fn with_updater<R>(&self, f: impl FnOnce(&mut Updater) -> R) -> R {
        f(&mut lock(&self.updater))
    }

    /// Start an automatic update check when one is due.
    pub(super) fn update_tick(&self, engine: &Engine) {
        if !self.with_state(|s| s.settings.check_updates) {
            return;
        }
        let due = self.with_updater(|u| {
            let now = now();
            let busy = matches!(
                u.status,
                update::Status::Checking | update::Status::Installing { .. }
            );
            !busy
                && if u.last_check == 0 {
                    now - u.started >= update::FIRST_CHECK_AFTER_SECS
                } else {
                    now - u.last_check >= update::CHECK_EVERY_SECS
                }
        });
        if due {
            self.check_updates(engine, None);
        }
    }

    /// Look for a new release; `ticket` gets the status when done.
    pub(super) fn check_updates(&self, engine: &Engine, ticket: Option<ReplyTicket>) {
        // An install under way keeps its status (and the release it found).
        if self.with_updater(|u| matches!(u.status, update::Status::Installing { .. })) {
            if let Some(t) = ticket {
                engine.finish_reply(t, Ok(self.update_json().to_string()));
            }
            return;
        }
        let Some(source) = update::Source::configured() else {
            self.with_updater(|u| u.status = update::Status::Unavailable);
            if let Some(t) = ticket {
                engine.finish_reply(t, Ok(self.update_json().to_string()));
            }
            return;
        };
        let start = self.with_updater(|u| {
            u.waiting.extend(ticket);
            let idle = !matches!(u.status, update::Status::Checking);
            if idle {
                u.status = update::Status::Checking;
            }
            idle
        });
        if !start {
            return;
        }
        self.show_update(engine);
        std::thread::spawn(move || {
            let result = update::check(&source);
            Engine::post(Box::new(move || {
                if let Some(app) = global() {
                    app.update_checked(Engine::global(), result);
                }
            }));
        });
    }

    fn update_checked(&self, engine: &Engine, result: Result<update::Manifest, String>) {
        let (announce, waiting) = self.with_updater(|u| {
            // A check that ends during an install must not offer the update
            // again: a second install would run over the first.
            if matches!(u.status, update::Status::Installing { .. }) {
                return (false, std::mem::take(&mut u.waiting));
            }
            u.last_check = now();
            u.found = None;
            u.status = match result {
                Ok(m) if update::newer(&m.version, env!("CARGO_PKG_VERSION")) => {
                    let installable = update::replaceable_bundle().is_some()
                        && m.assets.contains_key(&update::platform());
                    let status = update::Status::Available {
                        version: m.version.clone(),
                        notes_url: m.notes_url.clone(),
                        installable,
                    };
                    u.found = Some(m);
                    status
                }
                Ok(_) => update::Status::UpToDate {
                    checked_at: u.last_check,
                },
                Err(error) => update::Status::Failed { error },
            };
            let announce = match &u.status {
                update::Status::Available { version, .. }
                    if u.announced.as_ref() != Some(version) =>
                {
                    u.announced = Some(version.clone());
                    true
                }
                _ => false,
            };
            (announce, std::mem::take(&mut u.waiting))
        });
        let json = self.update_json().to_string();
        for t in waiting {
            engine.finish_reply(t, Ok(json.clone()));
        }
        if announce {
            engine.ensure_notifications();
        }
        self.show_update(engine);
    }

    /// Replace the app with the release found and restart (macOS), else
    /// open the release page.
    pub(super) fn install_update(&self, engine: &Engine) {
        // One install at a time: a second would share its folder and could
        // delete the app it is swapping in.
        if self.with_updater(|u| matches!(u.status, update::Status::Installing { .. })) {
            return;
        }
        let found = self.with_updater(|u| u.found.clone());
        let Some(m) = found else { return };
        let asset = m.assets.get(&update::platform()).cloned();
        let (Some(asset), Some(bundle)) = (asset, update::replaceable_bundle()) else {
            self.open_update_page(engine);
            return;
        };
        self.with_updater(|u| {
            u.status = update::Status::Installing {
                version: m.version.clone(),
            };
        });
        self.show_update(engine);
        std::thread::spawn(move || {
            let result = update::install(&asset, &bundle)
                .and_then(|cleanup| update::relaunch_after_exit(&bundle, &cleanup));
            Engine::post(Box::new(move || {
                let Some(app) = global() else { return };
                let engine = Engine::global();
                match result {
                    Ok(()) => {
                        app.with_state(|s| s.persist());
                        engine.quit();
                    }
                    Err(error) => {
                        app.with_updater(|u| u.status = update::Status::Failed { error });
                        engine.ensure_notifications();
                        app.show_update(engine);
                    }
                }
            }));
        });
    }

    /// The release notes of the update found, in a new tab.
    pub(super) fn open_update_page(&self, engine: &Engine) {
        let url = self.with_updater(|u| u.found.as_ref().map(|m| m.notes_url.clone()));
        if let Some(url) = url {
            self.open_url_from(engine, &url, false, url_policy::Source::Stored, None);
        }
    }

    pub(super) fn update_json(&self) -> Value {
        let status = self.with_updater(|u| u.status.clone());
        json!({ "status": status, "version": env!("CARGO_PKG_VERSION") })
    }

    /// Send the update status to the notification panel and to open
    /// Settings tabs: About shows it and must not go stale.
    fn show_update(&self, engine: &Engine) {
        let js = format!(
            "window.__magnetite && window.__magnetite.update({})",
            self.update_json()
        );
        engine.notifications_eval(&js);
        let settings_tabs: Vec<ContentId> = self.with_state(|s| {
            s.sidebar
                .all_tabs()
                .filter(|t| t.url.starts_with("magnetite://ui/settings"))
                .filter_map(|t| s.runtime.get(&t.id)?.content)
                .collect()
        });
        for page in settings_tabs {
            engine.content_ui_eval(page, &js);
        }
    }
}
