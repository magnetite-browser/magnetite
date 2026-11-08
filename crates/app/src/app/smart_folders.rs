//! Smart folders: connecting an account, saving a folder and fetching its
//! items.

use magnetite_core::smart::{Service, SmartFolder, SmartItem, SmartSource, item_key};
use magnetite_engine::Engine;
use serde_json::json;

use crate::state::{new_id, now};
use crate::{smart, url_policy};

use super::{MagnetiteApp, global};

/// How a service is named in messages.
pub(super) fn service_name(service: Service) -> &'static str {
    match service {
        Service::GitHub => "GitHub",
        Service::GitLab => "GitLab",
        Service::Jira => "Jira",
        Service::YouTrack => "YouTrack",
    }
}

impl MagnetiteApp {
    /// Check `token` with the service on a worker thread, then keep the
    /// account. Answers the UI query when done.
    pub(super) fn smart_connect(engine: &Engine, service: Service, host: String, token: String) {
        let Some(ticket) = engine.defer_reply() else {
            return;
        };
        std::thread::spawn(move || {
            let (host, token) = (host.trim().to_string(), token.trim().to_string());
            let checked = smart::whoami(service, &host, &token);
            Engine::post(Box::new(move || {
                let engine = Engine::global();
                let reply = match (checked, global()) {
                    (Ok((login, name)), Some(app)) => {
                        let account = smart::Account {
                            id: new_id(),
                            service,
                            host,
                            login,
                            name,
                        };
                        let stored = app.with_state(|s| s.accounts.add(account.clone(), token));
                        stored
                            .map_err(|e| format!("Cannot store the account: {e}"))
                            .map(|()| {
                                app.show_state(engine);
                                // Folders waiting for this account fill now.
                                app.smart_refresh_due(engine);
                                json!(account).to_string()
                            })
                    }
                    (Err(e), _) => Err(e),
                    (Ok(_), None) => Err("Magnetite is closing.".into()),
                };
                engine.finish_reply(ticket, reply);
            }));
        });
    }

    /// What `source` lists now, fetched on a worker thread (answers later).
    pub(super) fn smart_preview(
        &self,
        engine: &Engine,
        source: SmartSource,
        account: Option<&str>,
    ) -> Result<(), String> {
        let (account, token) = self
            .with_state(|s| s.accounts.for_source(&source, account))
            .ok_or_else(|| {
                format!(
                    "Connect a {} account first.",
                    service_name(source.service())
                )
            })?;
        let Some(ticket) = engine.defer_reply() else {
            return Ok(());
        };
        std::thread::spawn(move || {
            let result = smart::fetch(&source, &token, &account.login, 6)
                .map(|items| json!(items).to_string());
            Engine::post(Box::new(move || {
                Engine::global().finish_reply(ticket, result);
            }));
        });
        Ok(())
    }

    /// Create a smart folder at the top of the active space, or turn folder
    /// `id` into one / change it, then fill it. Tabs already in the folder
    /// that live on the source's server count as its items, so the refresh
    /// drops the ones the source no longer lists.
    pub(super) fn smart_save(
        &self,
        engine: &Engine,
        id: Option<String>,
        title: &str,
        mut smart: SmartFolder,
    ) -> Result<String, String> {
        let title = match title.trim() {
            "" => match smart.source.service() {
                Service::GitHub => "Pull requests",
                Service::GitLab => "Merge requests",
                Service::Jira | Service::YouTrack => "Issues",
            },
            t => t,
        }
        .to_string();
        let folder_id = self.with_state(|s| {
            if let Some(id) = id {
                let f = s.folder_mut(&id)?;
                f.title = title;
                let old = f.smart.take();
                let on_host = f
                    .children
                    .iter()
                    .flat_map(magnetite_core::SidebarNode::tabs)
                    .map(|t| item_key(t).to_string())
                    .filter(|u| smart::on_source_host(u, &smart.source));
                smart.items = old.as_ref().map(|o| o.items.clone()).unwrap_or_default();
                smart.items.extend(
                    on_host
                        .filter(|u| !smart.items.contains(u))
                        .collect::<Vec<_>>(),
                );
                if let Some(o) = old.filter(|o| o.source == smart.source) {
                    smart.hidden = o.hidden;
                    smart.filled = o.filled;
                }
                f.smart = Some(smart);
                Some(id)
            } else {
                let space = s.active_space.clone();
                let id = new_id();
                s.space_mut(&space)?.pinned.insert(
                    0,
                    magnetite_core::SidebarNode::Folder(magnetite_core::Folder {
                        id: id.clone(),
                        title,
                        icon: None,
                        children: vec![],
                        smart: Some(smart),
                    }),
                );
                Some(id)
            }
        });
        let folder_id = folder_id.ok_or("That folder no longer exists.")?;
        // Made or edited here: this device's accounts may fill it.
        self.with_state(|s| {
            s.smart_status.remove(&folder_id);
            s.allow_smart(&folder_id);
        });
        self.push_state(engine);
        self.smart_fetch(engine, &folder_id);
        Ok(folder_id)
    }

    /// Refresh smart folders whose interval has passed, and those waiting
    /// for an account (cheap: no request goes out without one).
    pub(super) fn smart_refresh_due(&self, engine: &Engine) {
        let now = now();
        let due: Vec<String> = self.with_state(|s| {
            s.smart_folders()
                .into_iter()
                .filter(|(id, sf)| {
                    let st = s.smart_status.get(id);
                    if !s.smart_is_allowed(id, sf) {
                        // Once: `smart_fetch` marks it as waiting to be
                        // turned on here (no request goes out), so the row
                        // can say so and offer it.
                        return !st.is_some_and(|st| st.needs_approval);
                    }
                    let waiting = st.is_some_and(|st| st.needs_account);
                    let busy = st.is_some_and(|st| st.fetching);
                    let checked = st.and_then(|st| st.checked_at);
                    !busy
                        && (waiting
                            || checked
                                .is_none_or(|t| now - t >= i64::from(sf.refresh_minutes) * 60))
                })
                .map(|(id, _)| id)
                .collect()
        });
        for id in due {
            self.smart_fetch(engine, &id);
        }
    }

    /// Fetch folder `id`'s items on a worker thread; `smart_fetched` applies them.
    pub(super) fn smart_fetch(&self, engine: &Engine, id: &str) {
        let job = self.with_state(|s| {
            let (_, sf) = s.smart_folders().into_iter().find(|(f, _)| f == id)?;
            let allowed = s.smart_is_allowed(id, &sf);
            let account = s.accounts.for_source(&sf.source, sf.account.as_deref());
            let st = s.smart_status.entry(id.to_string()).or_default();
            st.needs_approval = !allowed;
            if !allowed {
                st.error = Some(
                    "Added or changed on another device. Turn it on here to fill it with this device's accounts."
                        .into(),
                );
                return Some(None);
            }
            st.checked_at = Some(now());
            match account {
                // Still no account: nothing new to show.
                None if st.needs_account => None,
                None => {
                    st.needs_account = true;
                    st.error = Some(format!(
                        "Connect a {} account to fill this folder.",
                        service_name(sf.source.service())
                    ));
                    Some(None)
                }
                Some((account, token)) => {
                    st.needs_account = false;
                    st.fetching = true;
                    Some(Some((sf, account.login, token)))
                }
            }
        });
        let Some(job) = job else { return };
        self.show_state(engine);
        let Some((sf, login, token)) = job else {
            return;
        };
        let id = id.to_string();
        std::thread::spawn(move || {
            let result = smart::fetch(&sf.source, &token, &login, sf.limit);
            Engine::post(Box::new(move || {
                if let Some(app) = global() {
                    app.smart_fetched(Engine::global(), &id, result);
                }
            }));
        });
    }

    /// On the UI thread: put fetched items into folder `id`. Open tabs the
    /// source dropped stay until closed; failures keep the items shown.
    pub(super) fn smart_fetched(
        &self,
        engine: &Engine,
        id: &str,
        result: Result<Vec<SmartItem>, String>,
    ) {
        let changed = self.with_state(|s| {
            let st = s.smart_status.entry(id.to_string()).or_default();
            st.fetching = false;
            let items = match result {
                Ok(mut items) => {
                    st.error = None;
                    st.updated_at = Some(now());
                    // Results come from a server: web addresses only.
                    items.retain(|i| url_policy::allowed(&i.url, url_policy::Source::Stored));
                    items
                }
                Err(e) => {
                    st.error = Some(e);
                    return false;
                }
            };
            let open: Vec<String> = s
                .runtime
                .iter()
                .filter(|(_, r)| r.content.is_some())
                .map(|(t, _)| t.clone())
                .collect();
            s.folder_mut(id)
                .is_some_and(|f| f.apply_smart(&items, |t| open.contains(&t.id), new_id, now()))
        });
        if changed {
            self.push_state(engine);
        } else {
            self.show_state(engine);
        }
    }

    /// Take tab `id` out of its smart folder and keep it out (unloading it
    /// first, which also moves on if it is the active tab).
    pub(super) fn smart_hide(&self, engine: &Engine, id: &str) {
        self.close_tab(engine, id);
        self.with_state(|s| {
            if let Some(f) = s.parent_folder_mut(id) {
                let key = f.children.iter().find_map(|n| match n {
                    magnetite_core::SidebarNode::Tab(t) if t.id == id => {
                        Some(item_key(t).to_string())
                    }
                    _ => None,
                });
                if let (Some(smart), Some(key)) = (f.smart.as_mut(), key) {
                    smart.items.retain(|k| *k != key);
                    smart.hidden.push(key);
                }
                f.children
                    .retain(|n| !matches!(n, magnetite_core::SidebarNode::Tab(t) if t.id == id));
            }
            s.runtime.remove(id);
        });
        self.push_state(engine);
    }
}
