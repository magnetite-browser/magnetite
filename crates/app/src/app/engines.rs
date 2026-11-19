//! Which engine shows a site's pages, as Settings and the site panel see
//! it: Chromium, or the system's own (`magnetite_engine`'s `system.rs`).

use std::cell::Cell;
use std::rc::Rc;

use magnetite_engine::{ContextProfile, Engine, host_of, registrable_domain};
use serde_json::{Value, json};

use super::MagnetiteApp;

impl MagnetiteApp {
    /// Site panel: whether the system's engine shows the site of the page
    /// at `url`, and whether that is one of the known services (`known`),
    /// which the panel cannot turn off one by one.
    pub(super) fn site_engine(&self, url: &str) -> Value {
        let host = host_of(url).filter(|_| Engine::system_engine_available());
        let Some(host) = host else {
            return json!({ "site": null, "system": false, "known": false });
        };
        let (known, named) = self.with_state(|s| {
            let engine = &s.settings.system_engine;
            (engine.knows(&host), engine.names(&host))
        });
        json!({ "site": registrable_domain(&host), "system": known || named, "known": known })
    }

    /// Show the site of the page at `url` in the system's engine from now
    /// on, or in Chromium again; the page in front loads again, in the
    /// engine it has now. An error, and nothing changed, if the list of
    /// sites is full.
    pub(super) fn set_site_engine(
        &self,
        engine: &Engine,
        url: &str,
        system: bool,
    ) -> Result<(), String> {
        let Some(host) = host_of(url) else {
            return Ok(());
        };
        let sites = self.with_state(|s| {
            let own = &mut s.settings.system_engine;
            own.set_site(&host, registrable_domain(&host), system)?;
            Ok::<_, String>(own.hosts())
        })?;
        engine.set_system_engine_sites(sites);
        self.save_soon();
        self.on_active_content(engine, |engine, id| engine.content_load_url(id, url));
        Ok(())
    }

    /// Settings: delete everything the system's engine keeps (cookies,
    /// site storage, cache), in every profile. Answers the UI query when
    /// the last profile's data is gone.
    pub(super) fn clear_system_engine_data(&self, engine: &Engine) {
        let profiles: Vec<Option<ContextProfile>> = self.with_state(|s| {
            (s.sidebar.profiles.iter())
                .map(|p| {
                    (!p.is_default).then(|| ContextProfile {
                        id: p.id.clone(),
                        ephemeral: p.ephemeral,
                    })
                })
                .collect()
        });
        let ticket = engine.defer_reply();
        let left = Rc::new(Cell::new(profiles.len()));
        for profile in profiles {
            let left = left.clone();
            let cleared = move |_| {
                left.set(left.get() - 1);
                if let (0, Some(ticket)) = (left.get(), ticket) {
                    Engine::global().finish_reply(ticket, Ok("{}".into()));
                }
            };
            engine.clear_system_data(profile.as_ref(), None, Box::new(cleared));
        }
    }
}
