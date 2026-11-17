//! Developer features: what a local page gets (its console errors counted,
//! loading by itself once its server is up, the project it belongs to), and
//! the commands for any page (`DevTools` on a panel, emulation, reset).

use std::time::{Duration, Instant};

use magnetite_core::split::{MAX_PANES, Side, SplitNode};
use magnetite_engine::{ContentId, DevPanel, Engine, SPLIT_GAP};
use serde_json::{Value, json};

use crate::dev::{self, Emulation};
use crate::state::{Peek, Phone, TabRuntime};

use super::{MagnetiteApp, global};

/// The least width a page of a split keeps, in points.
const PANE_MIN: i32 = 200;

/// How often a page's address is tried while nothing listens there, and for
/// how long before the page is left as it is.
const TRY_EVERY: Duration = Duration::from_millis(500);
const TRY_FOR: Duration = Duration::from_secs(600);

impl MagnetiteApp {
    /// Whether the developer bar shows above the pages: the page in front
    /// is a local one, is shown as something else or is half of a
    /// responsive split, or a recording runs (which ends there). Not under
    /// a peek, which lies over the page.
    pub(super) fn dev_bar_wanted(&self) -> bool {
        let recording = !crate::lock(&self.record).for_ui().is_null();
        self.with_state(|s| {
            let dev = (s.active_tab_id())
                .filter(|_| s.peek.is_none())
                .and_then(|tab| s.runtime.get(tab))
                .map(|rt| &rt.dev);
            dev.is_some_and(|d| {
                (d.local && s.settings.developer.enabled)
                    || d.emulation != Emulation::default()
                    || d.responsive
                    || recording
            })
        })
    }

    /// Whether the page at `url` counts as a local one (Settings → Developer).
    fn is_dev_page(&self, url: &str) -> bool {
        self.with_state(|s| {
            let d = &s.settings.developer;
            d.enabled && dev::is_local(url, &d.hosts)
        })
    }

    /// Page `id` is at `url` now: note whether it is a local one, and whose.
    pub(super) fn dev_url_changed(&self, engine: &Engine, id: ContentId, url: &str) {
        let local = self.is_dev_page(url);
        let address = dev::loopback_address(url).filter(|_| local);
        let (watch, look_up) = self.with_state(|s| {
            let Some(rt) = s.page_runtime(id) else {
                return (false, None);
            };
            // The first local page in this window: its failed requests count
            // from now on.
            rt.dev.clears = dev::loopback_address(url).is_some();
            let watch = local && !rt.dev.watched;
            rt.dev.watched |= local;
            rt.dev.local = local;
            // Another address: another server, another wait.
            if rt.dev.waiting.is_some() && rt.dev.waiting != address {
                rt.dev.waiting = None;
            }
            if rt.dev.served_by == address {
                return (watch, None);
            }
            rt.dev.project = None;
            rt.dev.served_by.clone_from(&address);
            (watch, address)
        });
        if watch {
            engine.watch_errors(id);
        }
        self.dev_mirror(engine, id, url);
        if let Some(address) = look_up {
            Self::dev_find_project(id, address);
        }
    }

    /// Settings → Developer changed (which hosts count as local, or the
    /// tools turned off): every loaded page is looked at again.
    pub(super) fn dev_settings_changed(&self, engine: &Engine) {
        let pages: Vec<(ContentId, String)> = self.with_state(|s| {
            let tabs = (s.content_to_tab.iter())
                .filter_map(|(c, tab)| Some((*c, s.find_tab(tab)?.1.url.clone())));
            let phones = s
                .phones
                .iter()
                .map(|p| (p.page.content, p.page.url.clone()));
            // A peek and the mini windows count errors like any page.
            let outside = (s.peek.iter().chain(s.minis.iter())).map(|p| (p.content, p.url.clone()));
            tabs.chain(phones).chain(outside).collect()
        });
        for (c, url) in pages {
            self.dev_url_changed(engine, c, &url);
        }
    }

    /// Ask (off the UI thread) which program serves `address`, for page `id`.
    fn dev_find_project(id: ContentId, address: String) {
        std::thread::spawn(move || {
            let project = dev::project_at(&address);
            Engine::post(Box::new(move || {
                let (Some(app), engine) = (global(), Engine::global()) else {
                    return;
                };
                let changed = app.with_state(|s| {
                    let rt = s.page_runtime(id)?;
                    (rt.dev.served_by.as_deref() == Some(&address) && rt.dev.project != project)
                        .then(|| rt.dev.project = project)
                });
                if changed.is_some() {
                    app.show_state_paced(engine);
                }
            }));
        });
    }

    /// Page `id` starts to load: its errors are the old page's.
    pub(super) fn dev_load_started(&self, id: ContentId) {
        self.with_state(|s| {
            if let Some(rt) = s.page_runtime(id) {
                rt.dev.errors = 0;
                rt.dev.log.clear();
            }
        });
    }

    /// Page `id` logged an error. Counted for local pages only.
    pub(super) fn dev_console_error(&self, engine: &Engine, id: ContentId, text: String) {
        let counted = self.with_state(|s| {
            let rt = s.page_runtime(id)?;
            if !rt.dev.local || rt.dev.waiting.is_some() {
                return None;
            }
            rt.dev.note_error(text);
            Some(())
        });
        if counted.is_some() {
            self.show_state_paced(engine);
        }
    }

    /// Page `id` could not load `url`. Nothing listening at a local address
    /// is a server that is not up yet: wait for it, then load the page.
    pub(super) fn dev_load_failed(&self, engine: &Engine, id: ContentId, url: &str, refused: bool) {
        if !refused || !self.is_dev_page(url) {
            return;
        }
        let Some(address) = dev::loopback_address(url) else {
            return;
        };
        let started = self.with_state(|s| {
            let rt = s.page_runtime(id)?;
            rt.dev.local = true;
            rt.dev.errors = 0;
            // One watcher per page and address.
            (rt.dev.waiting.as_deref() != Some(&address)).then(|| {
                rt.dev.waiting = Some(address.clone());
                rt.dev.wait += 1;
                rt.dev.wait
            })
        });
        let Some(wait) = started else {
            return;
        };
        self.show_state_paced(engine);
        std::thread::spawn(move || {
            let since = Instant::now();
            let mut up = false;
            while !up && since.elapsed() < TRY_FOR {
                std::thread::sleep(TRY_EVERY);
                up = dev::listens(&address);
            }
            Engine::post(Box::new(move || {
                let (Some(app), engine) = (global(), Engine::global()) else {
                    return;
                };
                // Still the page that waits for this address.
                let ours = app.with_state(|s| {
                    let rt = s.page_runtime(id)?;
                    (rt.dev.wait == wait && rt.dev.waiting.as_deref() == Some(&address))
                        .then(|| rt.dev.waiting = None)
                });
                if ours.is_none() {
                    return;
                }
                if up {
                    engine.content_reload(id);
                    Self::dev_find_project(id, address);
                }
                app.show_state_paced(engine);
            }));
        });
    }

    /// Open the `DevTools` of the page a key came from (`from`), else of the
    /// page in front, on `panel`.
    pub(super) fn dev_panel(&self, engine: &Engine, from: Option<ContentId>, panel: DevPanel) {
        if let Some(c) = self.focused_content(from) {
            engine.content_show_panel(c, panel);
        }
    }

    /// Load the page anew, past the cache. A page this computer serves
    /// starts from nothing: its origin's cookies, storage, caches and
    /// service workers go first. Any other page keeps them, a host the user
    /// named as local too: a key must not sign the user out of a site.
    pub(super) fn dev_reset(&self, engine: &Engine, from: Option<ContentId>) {
        let Some(c) = self.focused_content(from) else {
            return;
        };
        let reload = move || {
            let params = json!({ "ignoreCache": true });
            Engine::global().devtools(c, "Page.reload", &params, Box::new(|_| {}));
        };
        let origin = self
            .page_url(c)
            .filter(|url| dev::loopback_address(url).is_some())
            .and_then(|url| url::Url::parse(&url).ok())
            .map(|url| url.origin().ascii_serialization());
        match origin {
            Some(origin) => engine.devtools(
                c,
                "Storage.clearDataForOrigin",
                &json!({ "origin": origin, "storageTypes": "all" }),
                Box::new(move |_| reload()),
            ),
            None => reload(),
        }
    }

    /// The address of the page in window `c`.
    fn page_url(&self, c: ContentId) -> Option<String> {
        self.with_state(|s| {
            if let Some(p) = s.page_outside(c) {
                return Some(p.url.clone());
            }
            let tab = s.content_to_tab.get(&c)?;
            s.find_tab(tab).map(|(_, t)| t.url.clone())
        })
    }

    /// Show the page in front as `patch` says (fields of `Emulation`; what
    /// it leaves out stays). Returns what the page is shown as now.
    pub(super) fn dev_emulate(&self, engine: &Engine, patch: Value) -> Result<Value, String> {
        let c = self.focused_content(None).ok_or("no page")?;
        let (was, now) = self
            .with_state(|s| {
                let rt = s.page_runtime(c)?;
                let was = rt.dev.emulation.clone();
                let mut merged = json!(was);
                if let (Some(merged), Value::Object(patch)) = (merged.as_object_mut(), patch) {
                    merged.extend(patch);
                }
                let now: Emulation = match serde_json::from_value(merged) {
                    Ok(now) => now,
                    Err(e) => return Some(Err(e.to_string())),
                };
                if now
                    .timezone
                    .as_deref()
                    .is_some_and(|z| !dev::is_timezone(z))
                {
                    return Some(Err("not a time zone".into()));
                }
                rt.dev.emulation = now.clone();
                Some(Ok((was, now)))
            })
            .ok_or("no page")??;
        for (method, params) in now.commands(&was) {
            engine.devtools(c, method, &params, Box::new(|_| {}));
        }
        self.show_state_paced(engine);
        Ok(json!(now))
    }

    /// Open the active tab's page again, beside it, in profile `profile`:
    /// the same site with another user's logins. Not from the private
    /// space, nor into a private profile.
    pub(super) fn dev_split_as(&self, engine: &Engine, profile: &str) {
        let url = self.with_state(|s| {
            let known = (s.sidebar.profiles.iter()).any(|p| p.id == profile);
            let tab = s.active_tab_id()?.to_owned();
            // Another profile than the page's own, the default one included.
            let own = s.profile_for_tab(&tab).as_deref() == Some(profile);
            if !known || own || s.private_profiles.contains(profile) || s.in_private_space(&tab) {
                return None;
            }
            s.find_tab(&tab).map(|(_, t)| t.url.clone())
        });
        // In the background: the page is made when the split shows it, in
        // the profile it has by then.
        let Some(tab) = url.and_then(|url| self.open_url(engine, &url, true)) else {
            return;
        };
        self.with_state(|s| {
            if let Some(t) = s.find_tab_mut(&tab) {
                t.profile_id = Some(profile.to_owned());
            }
        });
        self.split_add(engine, None, &tab, Side::Right);
    }

    /// The responsive split: the page in front a second time to its right,
    /// as wide as a phone and shown as one, each of the two following the
    /// other's address. The phone is a second page of the tab, not a tab.
    /// Called again, it ends: the phone closes.
    pub(super) fn dev_responsive(&self, engine: &Engine) {
        enum Do {
            End(ContentId),
            Start(String, String),
            Full,
        }
        let what = self.with_state(|s| {
            let tab = s.active_tab_id()?.to_owned();
            if let Some(phone) = s.phones.iter().find(|p| p.tab == tab) {
                return Some(Do::End(phone.page.content));
            }
            if s.in_private_space(&tab) || s.is_web_panel(&tab) {
                return None;
            }
            let shown = s.visible_panes();
            let phones = s.phones.iter().filter(|p| shown.contains(&p.tab)).count();
            if shown.len() + phones >= MAX_PANES {
                return Some(Do::Full);
            }
            let url = s.find_tab(&tab).map(|(_, t)| t.url.clone())?;
            url.starts_with("http").then_some(Do::Start(url, tab))
        });
        match what {
            Some(Do::Start(url, tab)) => {
                // In the tab's profile: the same logins in both.
                let (profile, context) = self.with_state(|s| {
                    let profile = s.profile_for_tab(&tab);
                    let context = s.context(profile.as_deref());
                    (profile.unwrap_or_default(), context)
                });
                let c = engine.create_content(&url, context.as_ref());
                self.with_state(|s| {
                    let mut runtime = TabRuntime {
                        content: Some(c),
                        loaded: true,
                        loading: true,
                        ..TabRuntime::default()
                    };
                    runtime.dev.phone = true;
                    s.runtime.entry(tab.clone()).or_default().dev.responsive = true;
                    s.phones.push(Phone {
                        tab,
                        page: Peek {
                            content: c,
                            title: url.clone(),
                            url,
                            runtime,
                            profile,
                        },
                        width: dev::PHONE_WIDTH,
                    });
                });
                self.close_peek(engine);
            }
            Some(Do::End(phone)) => {
                self.with_state(|s| s.drop_phone(phone));
                engine.close_content(phone);
            }
            Some(Do::Full) => {
                let text = "Split view holds up to four pages";
                return Self::ui_call(engine, "notice", &json!({ "text": text }));
            }
            None => return,
        }
        self.present(engine);
    }

    /// Close the phones whose tab is gone or has no page any more.
    pub(super) fn dev_drop_stale_phones(&self, engine: &Engine) {
        let stale: Vec<ContentId> = self.with_state(|s| {
            let stale: Vec<ContentId> = (s.phones.iter())
                .filter(|p| s.runtime.get(&p.tab).is_none_or(|rt| rt.content.is_none()))
                .map(|p| p.page.content)
                .collect();
            for c in &stale {
                s.drop_phone(*c);
            }
            stale
        });
        for c in stale {
            engine.close_content(c);
        }
    }

    /// `tree` (the split on screen; none: `alone`, the one page there) with
    /// the phones of its pages beside them; none if one page shows alone.
    pub(super) fn dev_with_phones(
        &self,
        engine: &Engine,
        tree: Option<SplitNode<ContentId>>,
        alone: Option<ContentId>,
    ) -> Option<SplitNode<ContentId>> {
        let phones: Vec<(ContentId, ContentId, i32)> = self.with_state(|s| {
            (s.phones.iter())
                .filter_map(|p| Some((s.runtime.get(&p.tab)?.content?, p.page.content, p.width)))
                .collect()
        });
        let area = engine.split_area();
        let mut tree = tree;
        for (page, phone, width) in phones {
            let shown = match &tree {
                Some(tree) => tree.contains(&page),
                None => alone == Some(page),
            };
            if shown {
                let base = tree.unwrap_or_else(|| SplitNode::pane(page));
                tree = Some(base.with_beside(&page, phone, width, area, SPLIT_GAP, PANE_MIN));
            }
        }
        tree
    }

    /// The tab whose phone the divider at `path` of the tiling on screen
    /// belongs to: the one where the split itself has a page, not a divider.
    fn dev_phone_divider(&self, path: &[usize]) -> Option<String> {
        self.with_state(|s| {
            let tab = match s.visible_split() {
                Some(tree) => tree.pane_at(path).cloned(),
                None => s
                    .active_tab_id()
                    .filter(|_| path.is_empty())
                    .map(str::to_owned),
            }?;
            s.phones.iter().any(|p| p.tab == tab).then_some(tab)
        })
    }

    /// The divider at `path` was dragged to screen position `pos`, or
    /// (`None`) double-clicked. If it is a phone's, the phone takes the
    /// width that gives it (390 points again); true then.
    pub(super) fn dev_phone_resized(
        &self,
        engine: &Engine,
        path: &[usize],
        pos: Option<i32>,
    ) -> bool {
        let Some(tab) = self.dev_phone_divider(path) else {
            return false;
        };
        let area = engine.split_area();
        self.with_state(|s| {
            // The tab's place: the phone's right edge is its right edge.
            let place = match s.visible_split() {
                Some(tree) => (tree.layout(area, SPLIT_GAP).into_iter())
                    .find(|(t, _)| *t == tab)
                    .map(|(_, b)| b),
                None => Some(area),
            };
            let width = match (pos, place) {
                (Some(pos), Some(b)) => (b.x + b.width - pos - SPLIT_GAP / 2)
                    .clamp(PANE_MIN, (b.width - SPLIT_GAP - PANE_MIN).max(PANE_MIN)),
                _ => dev::PHONE_WIDTH,
            };
            if let Some(phone) = s.phones.iter_mut().find(|p| p.tab == tab) {
                phone.width = width;
            }
        });
        true
    }

    /// Page `id` of a responsive split went to `url`: its other half goes
    /// there too. An address that came from the other half stays here.
    fn dev_mirror(&self, engine: &Engine, id: ContentId, url: &str) {
        let (phone, to) = self.with_state(|s| {
            // The pair `id` is half of: the tab's page and the phone's.
            let pair = s.phones.iter().find_map(|p| {
                let page = s.runtime.get(&p.tab)?.content?;
                (id == page || id == p.page.content).then(|| (page, p.page.content, p.tab.clone()))
            });
            let Some((page, phone, tab)) = pair else {
                return (false, None);
            };
            let Some(rt) = s.page_runtime(id) else {
                return (false, None);
            };
            // A phone hears that it is one when its page exists: now.
            let tell = rt.dev.phone && !rt.dev.phone_told;
            rt.dev.phone_told |= tell;
            if rt.dev.following {
                // Within the same document nothing loads: this was all.
                rt.dev.following = rt.loading;
                return (tell, None);
            }
            let (other, there) = if id == phone {
                (page, s.find_tab(&tab).map(|(_, t)| t.url.clone()))
            } else {
                (phone, s.page_outside(phone).map(|p| p.url.clone()))
            };
            if there.as_deref() == Some(url) || !url.starts_with("http") {
                return (tell, None);
            }
            if let Some(rt) = s.page_runtime(other) {
                rt.dev.following = true;
            }
            (tell, Some(other))
        });
        if phone {
            for (method, params) in dev::phone_commands(true) {
                engine.devtools(id, method, &params, Box::new(|_| {}));
            }
        }
        if let Some(other) = to {
            engine.content_load_url(other, url);
        }
    }

    /// Page `id` finished loading: where it goes from here is its own.
    pub(super) fn dev_load_ended(&self, id: ContentId) {
        self.with_state(|s| {
            if let Some(rt) = s.page_runtime(id) {
                rt.dev.following = false;
            }
        });
    }

    /// The programs listening on local ports, for the command bar: asked
    /// off the UI thread, answered when known.
    pub(super) fn dev_servers(engine: &Engine) {
        let Some(ticket) = engine.defer_reply() else {
            return;
        };
        std::thread::spawn(move || {
            let reply = json!({ "servers": dev::servers() }).to_string();
            Engine::post(Box::new(move || {
                Engine::global().finish_reply(ticket, Ok(reply));
            }));
        });
    }
}
