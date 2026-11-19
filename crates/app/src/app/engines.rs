//! Which engine shows a site's pages, as Settings and the site panel see
//! it: Chromium, or the system's own (`magnetite_engine`'s `system.rs`).

use magnetite_engine::{ContextProfile, Engine, host_of, in_domain, registrable_domain};
use serde_json::{Value, json};

use super::MagnetiteApp;
use crate::state::PROTECTED_VIDEO_SITES;

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
            (
                engine.known
                    && PROTECTED_VIDEO_SITES
                        .iter()
                        .any(|site| in_domain(&host, site)),
                engine.sites.iter().any(|site| in_domain(&host, site)),
            )
        });
        json!({ "site": registrable_domain(&host), "system": known || named, "known": known })
    }

    /// Show the site of the page at `url` in the system's engine from now
    /// on, or in Chromium again; the page in front loads again, in the
    /// engine it has now.
    pub(super) fn set_site_engine(&self, engine: &Engine, url: &str, system: bool) {
        let Some(host) = host_of(url) else {
            return;
        };
        let sites = self.with_state(|s| {
            let own = &mut s.settings.system_engine.sites;
            own.retain(|site| !in_domain(&host, site));
            if system {
                own.push(registrable_domain(&host));
            }
            s.settings.system_engine.hosts()
        });
        engine.set_system_engine_sites(sites);
        self.save_soon();
        self.on_active_content(engine, |engine, id| engine.content_load_url(id, url));
    }

    /// Settings: delete everything the system's engine keeps (cookies,
    /// site storage, cache), in every profile.
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
        for profile in profiles {
            engine.clear_system_data(profile.as_ref(), None, Box::new(|_| {}));
        }
    }
}
