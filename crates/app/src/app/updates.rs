//! Release updates as the app runs them: when to check, the answers for
//! Settings and the notification panel, and the install. Checking a release
//! and replacing the app are `update.rs`.

use magnetite_engine::{Engine, ReplyTicket};
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
    /// Installed from an rpm or deb: the package manager updates it.
    system: Option<update::SystemBinary>,
    /// Where a system package's user installs its updates, if Magnetite
    /// knows the desktop's software centre.
    centre: Option<update::SoftwareCentre>,
}

impl Updater {
    pub(super) fn new() -> Self {
        let system = update::SystemBinary::detect();
        let centre = system.as_ref().and_then(|_| update::SoftwareCentre::find());
        Self {
            status: if system.is_some() {
                update::Status::System
            } else {
                update::Status::Idle
            },
            found: None,
            started: now(),
            last_check: 0,
            announced: None,
            waiting: Vec::new(),
            system,
            centre,
        }
    }

    /// A check must not change the status: an install is under way, or the
    /// package manager installed the update and its restart is offered.
    fn keeps_status(&self) -> bool {
        matches!(self.status, update::Status::Installing { .. })
            || (self.system.is_some() && self.found.is_some())
    }

    /// Offer a system package's restart again after it failed (the version
    /// the package manager installed is still there); true if it did.
    fn offer_restart_again(&mut self) -> bool {
        let (Some(_), Some(m), update::Status::Failed { .. }) =
            (&self.system, &self.found, &self.status)
        else {
            return false;
        };
        self.status = update::Status::Available {
            version: m.version.clone(),
            notes_url: m.notes_url.clone(),
            installable: true,
        };
        true
    }
}

impl MagnetiteApp {
    fn with_updater<R>(&self, f: impl FnOnce(&mut Updater) -> R) -> R {
        f(&mut lock(&self.updater))
    }

    /// Start an automatic update check when one is due. A system package
    /// only looks whether the package manager replaced it.
    pub(super) fn update_tick(&self, engine: &Engine) {
        if self.with_updater(|u| u.system.is_some()) {
            self.system_update_tick(engine);
            return;
        }
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

    /// Offer a restart once the package manager put a newer version in
    /// place of the running one, and name the newest one if it does so
    /// again before the restart.
    fn system_update_tick(&self, engine: &Engine) {
        let version = self.with_updater(|u| {
            let version = u.system.as_mut()?.replaced()?;
            let notes_url = format!("{}/releases/tag/v{version}", env!("CARGO_PKG_REPOSITORY"));
            u.status = update::Status::Available {
                version: version.clone(),
                notes_url: notes_url.clone(),
                installable: true,
            };
            u.found = Some(update::Manifest {
                version: version.clone(),
                notes_url,
                assets: std::collections::HashMap::new(),
            });
            u.announced = Some(version.clone());
            Some(version)
        });
        if version.is_some() {
            engine.ensure_notifications();
            self.show_update(engine);
        }
    }

    /// Look for a new release; `ticket` gets the status when done. A system
    /// package asks only through here ("Check now"): its package manager
    /// installs what the check finds.
    pub(super) fn check_updates(&self, engine: &Engine, ticket: Option<ReplyTicket>) {
        // The package manager may have installed it already, also before a
        // restart that failed.
        if self.with_updater(|u| u.system.is_some()) {
            self.system_update_tick(engine);
            if self.with_updater(Updater::offer_restart_again) {
                self.show_update(engine);
            }
        }
        // An install under way keeps its status (and the release it found),
        // as does a system package waiting for its restart.
        if self.with_updater(|u| u.keeps_status()) {
            if let Some(t) = ticket {
                engine.finish_reply(t, Ok(self.update_json().to_string()));
            }
            return;
        }
        let system = self.with_updater(|u| u.system.is_some());
        let Some(source) = update::Source::configured(system) else {
            // A system package is still updated, by its package manager.
            if !system {
                self.with_updater(|u| u.status = update::Status::Unavailable);
            }
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
            // again: a second install would run over the first. Nor may it
            // take back the restart a system package offers.
            if u.keeps_status() {
                return (false, std::mem::take(&mut u.waiting));
            }
            u.last_check = now();
            u.found = None;
            u.status = match result {
                // Its package manager installs it. `found` stays empty: it
                // means the restart once that is done (`system_update_tick`).
                Ok(m)
                    if u.system.is_some()
                        && update::newer(&m.version, env!("CARGO_PKG_VERSION")) =>
                {
                    update::Status::SystemUpdate {
                        version: m.version,
                        notes_url: m.notes_url,
                    }
                }
                Ok(m) if update::newer(&m.version, env!("CARGO_PKG_VERSION")) => {
                    let package = update::platform(update::appimage().is_some());
                    let installable = update::can_install() && m.assets.contains_key(&package);
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
                Err(error) => update::Status::CheckFailed { error },
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

    /// Replace the app with the release found and restart (macOS), run the
    /// release's installer (Windows, a copy its installer put there),
    /// restart into what the package manager installed (Linux packages),
    /// replace the AppImage and restart (Linux), else open the release
    /// page.
    pub(super) fn install_update(&self, engine: &Engine) {
        let restarted = self.with_updater(|u| {
            let system = u.system.as_ref().filter(|_| u.found.is_some())?;
            Some(system.restart_after_exit())
        });
        match restarted {
            Some(Ok(())) => {
                self.with_state(|s| s.persist());
                engine.quit();
                return;
            }
            Some(Err(e)) => {
                let error = magnetite_ui::t!("app.update.restart_failed", error = e);
                self.with_updater(|u| u.status = update::Status::Failed { error });
                engine.ensure_notifications();
                self.show_update(engine);
                return;
            }
            None => {}
        }
        // One install at a time: a second would share its folder and could
        // delete the app it is swapping in.
        if self.with_updater(|u| matches!(u.status, update::Status::Installing { .. })) {
            return;
        }
        let found = self.with_updater(|u| u.found.clone());
        let Some(m) = found else { return };
        // Asked once: the package picked is the one for what gets replaced.
        let image = update::appimage();
        let asset = (m.assets.get(&update::platform(image.is_some()))).cloned();
        let installing = |app: &Self| {
            app.with_updater(|u| {
                u.status = update::Status::Installing {
                    version: m.version.clone(),
                };
            });
            app.show_update(engine);
        };
        // Windows: the installer does it, once Magnetite has quit.
        #[cfg(target_os = "windows")]
        if let Some(asset) = asset.clone().filter(|_| update::installed_by_setup()) {
            installing(self);
            std::thread::spawn(move || {
                let result = update::fetch_installer(&asset)
                    .and_then(|installer| update::run_installer(&installer));
                Engine::post(Box::new(move || Self::update_installed(result)));
            });
            return;
        }
        #[cfg(target_os = "linux")]
        if let (Some(asset), Some(image)) = (asset.clone(), image) {
            installing(self);
            std::thread::spawn(move || {
                let result = update::install_appimage(&asset, &image);
                Engine::post(Box::new(move || Self::update_installed(result)));
            });
            return;
        }
        let (Some(asset), Some(bundle)) = (asset, update::replaceable_bundle()) else {
            self.open_update_page(engine);
            return;
        };
        installing(self);
        std::thread::spawn(move || {
            let result = update::install(&asset, &bundle)
                .and_then(|cleanup| update::relaunch_after_exit(&bundle, &cleanup));
            Engine::post(Box::new(move || Self::update_installed(result)));
        });
    }

    /// Quit, and start again with this run's arguments once quit.
    pub(super) fn restart(&self, engine: &Engine) -> Result<(), String> {
        let program = std::env::current_exe().map_err(|e| e.to_string())?;
        update::restart_after_exit(&program).map_err(|e| e.to_string())?;
        self.with_state(|s| s.persist());
        engine.quit();
        Ok(())
    }

    /// The install is ready for Magnetite to quit (what starts it again is
    /// already waiting), or it failed and says why.
    fn update_installed(result: Result<(), String>) {
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
    }

    /// The release notes of the update found, in a new tab.
    pub(super) fn open_update_page(&self, engine: &Engine) {
        let url = self.with_updater(|u| match (&u.found, &u.status) {
            (Some(m), _) => Some(m.notes_url.clone()),
            (None, update::Status::SystemUpdate { notes_url, .. }) => Some(notes_url.clone()),
            _ => None,
        });
        if let Some(url) = url {
            self.open_url_from(engine, &url, false, url_policy::Source::Stored, None);
        }
    }

    /// A system package's software centre, on its list of updates.
    pub(super) fn open_software_centre(&self) -> Result<(), String> {
        let centre = self.with_updater(|u| u.centre.clone());
        let centre = centre.ok_or(magnetite_ui::t!("app.update.no_software_centre"))?;
        centre
            .open()
            .map_err(|e| magnetite_ui::t!("app.update.open_failed", name = centre.name, error = e))
    }

    pub(super) fn update_json(&self) -> Value {
        self.with_updater(|u| {
            // How a system package gets its updates, for Settings.
            let system = u.system.as_ref().map(|s| {
                json!({
                    "command": s.package.map(update::Package::command),
                    "centre": u.centre.as_ref().map(|c| c.name),
                })
            });
            json!({ "status": u.status, "system": system, "version": env!("CARGO_PKG_VERSION") })
        })
    }

    /// Send the update status to the notification panel and to open
    /// Settings tabs: About shows it and must not go stale.
    fn show_update(&self, engine: &Engine) {
        let js = format!(
            "window.__magnetite && window.__magnetite.update({})",
            self.update_json()
        );
        engine.notifications_eval(&js);
        for page in self.own_page_tabs("magnetite://ui/settings") {
            engine.content_ui_eval(page, &js);
        }
    }
}
