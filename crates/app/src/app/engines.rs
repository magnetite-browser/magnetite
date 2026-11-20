//! Which engine shows a site's pages, as Settings and the site panel see
//! it: Chromium, or the system's own (`magnetite_engine`'s `system.rs`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use magnetite_engine::{ContentId, ContextProfile, Engine, host_of, in_domain, registrable_domain};
use serde_json::{Value, json};

use super::MagnetiteApp;
use crate::lock;

impl MagnetiteApp {
    /// Every site the system's engine shows: those of the settings, and
    /// those that moved there from a private space in this run.
    pub(super) fn system_engine_sites(&self) -> Vec<String> {
        let mut sites = self.with_state(|s| s.settings.system_engine.hosts());
        sites.extend(lock(&self.private_system_sites).iter().cloned());
        sites
    }

    /// Protected video did not play in Chromium page `id` at `url`, of
    /// profile `context`: its service refuses Magnetite's Chromium. Unless
    /// the user decided otherwise for its site, the site opens in the
    /// system's engine from now on, and the page loads again there.
    /// From a private space the site moves for this run only, and nothing
    /// is kept of it.
    pub(super) fn protected_playback_failed(
        &self,
        engine: &Engine,
        id: ContentId,
        url: &str,
        context: Option<&str>,
    ) {
        let Some(host) = host_of(url) else { return };
        let site = registrable_domain(&host);
        let private = self.with_state(|s| {
            let private = s.is_private_context(context);
            let own = &mut s.settings.system_engine;
            if !own.moves(&host) {
                return None;
            }
            // A full list keeps the page where it is.
            (private || own.set_site(&host, site.clone(), true).is_ok()).then_some(private)
        });
        match private {
            None => return,
            Some(true) => lock(&self.private_system_sites).push(site.clone()),
            Some(false) => self.save_soon(),
        }
        engine.set_system_engine_sites(self.system_engine_sites());
        engine.content_load_url(id, url);
        let name = Engine::system_engine_name().0;
        let text =
            format!("Protected video did not play on {site}: it opens in {name} from now on.");
        Self::ui_call(engine, "notice", &json!({ "text": text }));
    }

    /// Site panel: whether the system's engine shows the site of the page
    /// at `url`, and whether that is one of the known services (`known`),
    /// which the panel cannot turn off one by one.
    pub(super) fn site_engine(&self, url: &str) -> Value {
        let (name, about) = Engine::system_engine_name();
        let host = host_of(url).filter(|_| Engine::system_engine_available());
        let Some(host) = host else {
            return json!({ "site": null, "system": false, "known": false, "name": name, "about": about });
        };
        let (known, named) = self.with_state(|s| {
            let engine = &s.settings.system_engine;
            (engine.knows(&host), engine.names(&host))
        });
        let named = named || (lock(&self.private_system_sites).iter()).any(|s| in_domain(&host, s));
        json!({
            "site": registrable_domain(&host),
            "system": known || named,
            "known": known,
            "name": name,
            "about": about,
        })
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
        self.with_state(|s| {
            (s.settings.system_engine).set_site(&host, registrable_domain(&host), system)
        })?;
        lock(&self.private_system_sites).retain(|own| !in_domain(&host, own));
        engine.set_system_engine_sites(self.system_engine_sites());
        self.save_soon();
        self.on_active_content(engine, |engine, id| engine.content_load_url(id, url));
        Ok(())
    }

    /// Settings: delete everything the system's engine keeps (cookies,
    /// site storage, cache), in every profile. Answers the UI query when
    /// the last profile is done: with an error if one of them could not be
    /// cleared.
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
        let failed: Rc<RefCell<Option<String>>> = Rc::default();
        for profile in profiles {
            let (left, failed) = (left.clone(), failed.clone());
            let cleared = move |outcome: Result<Option<usize>, String>| {
                if let Err(why) = outcome {
                    failed.replace(Some(why));
                }
                left.set(left.get() - 1);
                if let (0, Some(ticket)) = (left.get(), ticket) {
                    let reply = failed.take().map_or_else(|| Ok("{}".into()), Err);
                    Engine::global().finish_reply(ticket, reply);
                }
            };
            engine.clear_system_data(profile.as_ref(), None, Box::new(cleared));
        }
    }
}
