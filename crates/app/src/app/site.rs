//! The page in front as a site: permission prompts, the reload after
//! clearing site data, Copy Link, and where DevTools open.

use magnetite_engine::{ContentId, Engine, host_of};
use serde_json::{Value, json};

use crate::lock;

use super::{LinkVariant, MagnetiteApp, Prompts};

/// The site a permission prompt names: the full host with a port other than
/// the scheme's (international names in punycode, so a lookalike cannot
/// pass for another site); other origins as given.
pub(super) fn site_name(origin: &str) -> String {
    match url::Url::parse(origin) {
        Ok(u) if matches!(u.scheme(), "http" | "https") && u.host_str().is_some() => {
            let host = u.host_str().unwrap_or_default();
            match u.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_string(),
            }
        }
        _ => origin.trim_end_matches('/').to_string(),
    }
}

impl MagnetiteApp {
    pub(super) fn with_prompts<R>(&self, f: impl FnOnce(&mut Prompts) -> R) -> R {
        f(&mut lock(&self.prompts))
    }

    /// The prompts of the pages on screen (tabs, split panes, the peek) for
    /// the notification panel, newest first.
    pub(super) fn prompts_json(&self) -> Value {
        let mut on_screen = lock(&self.visible).clone();
        on_screen.extend(self.with_state(|s| s.peek.as_ref().map(|p| p.content)));
        let items: Vec<Value> = self.with_prompts(|p| {
            p.open
                .iter()
                .rev()
                .filter(|q| on_screen.contains(&q.content))
                .map(|q| {
                    json!({
                        // A string: prompt ids are u64, past what JSON numbers keep exact.
                        "id": q.id.to_string(),
                        "site": site_name(&q.origin),
                        "kinds": q.kinds,
                    })
                })
                .collect()
        });
        json!({ "items": items })
    }

    /// Send the prompts of the pages on screen to the notification panel
    /// when they changed, opening the panel for a new one.
    pub(super) fn show_prompts(&self, engine: &Engine) {
        let list = self.prompts_json();
        let text = list.to_string();
        let changed = self.with_prompts(|p| {
            let changed = p.shown != text;
            if changed {
                p.shown.clone_from(&text);
                p.changed_at = Some(std::time::Instant::now());
            }
            changed
        });
        if !changed {
            return;
        }
        if list["items"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
        {
            engine.ensure_notifications();
        }
        engine.notifications_eval(&format!(
            "window.__magnetite && window.__magnetite.prompts({text})"
        ));
    }

    /// Open or close `DevTools` for the page a key came from (`from`), else
    /// the page in focus.
    pub(super) fn toggle_devtools(&self, engine: &Engine, from: Option<ContentId>) {
        if let Some(c) = self.focused_content(from) {
            if engine.content_has_devtools(c) {
                engine.content_close_devtools(c);
            } else {
                engine.content_show_devtools(c);
            }
        }
    }

    /// Dock `DevTools` per `dock` from now on. Open ones in the other kind of
    /// window (docked vs separate) close and reopen in the new kind, once
    /// Chromium has let go of them.
    pub(super) fn apply_devtools_dock(engine: &Engine, dock: magnetite_engine::DevToolsDock) {
        let reopen = engine.devtools_to_reopen(dock);
        engine.set_devtools_dock(dock);
        for id in reopen {
            engine.content_close_devtools(id);
            Engine::post_delayed(
                Box::new(move || Engine::global().content_show_devtools(id)),
                300,
            );
        }
    }

    /// Reload every open page of `domain` that shares cookies with `profile`
    /// (no profile and the default profile are the same engine context).
    pub(super) fn reload_site(&self, engine: &Engine, domain: &str, profile: Option<&str>) {
        let contents: Vec<ContentId> = self.with_state(|s| {
            let context = |p: Option<&str>| s.context(p).map(|c| c.id);
            let cleared = context(profile);
            s.runtime
                .iter()
                .filter_map(|(tab, r)| Some((tab, r.content?)))
                .filter(|(tab, _)| context(s.profile_for_tab(tab).as_deref()) == cleared)
                .filter(|(tab, _)| {
                    s.find_tab(tab)
                        .and_then(|(_, t)| host_of(&t.url))
                        .is_some_and(|h| magnetite_engine::in_domain(&h, domain))
                })
                .map(|(_, c)| c)
                .collect()
        });
        for c in contents {
            engine.content_reload(c);
        }
    }

    /// Copy the link of the focused page (see `focused_content`).
    pub(super) fn copy_link(
        &self,
        engine: &Engine,
        from: Option<ContentId>,
        variant: LinkVariant,
    ) -> Value {
        let target = self.with_state(|s| {
            let from = from.or_else(|| s.peek.as_ref().map(|p| p.content));
            if let Some(p) = from.and_then(|c| s.page_outside(c)) {
                return Some((p.url.clone(), p.title.clone(), s.settings.strip_trackers));
            }
            let tab = from
                .and_then(|c| s.content_to_tab.get(&c).cloned())
                .or_else(|| s.active_tab_id().map(str::to_owned))?;
            s.find_tab(&tab)
                .map(|(_, t)| (t.url.clone(), t.title.clone(), s.settings.strip_trackers))
        });
        let Some((url, title, strip)) = target else {
            return json!({"copied": false});
        };
        let (clean, removed) = if strip {
            magnetite_core::links::clean_link(&url)
        } else {
            (url.clone(), 0)
        };
        let (text, removed) = match variant {
            LinkVariant::Clean => (clean, removed),
            LinkVariant::Original => (url, 0),
            LinkVariant::Markdown => (
                magnetite_core::links::markdown_link(&title, &clean),
                removed,
            ),
        };
        let copied = engine.copy_text(&text);
        json!({"copied": copied, "removed": removed, "variant": variant})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_name_the_whole_site() {
        assert_eq!(site_name("https://www.youtube.com/"), "www.youtube.com");
        assert_eq!(site_name("http://localhost:8000/"), "localhost:8000");
        // The default port is not shown, an international name is punycode.
        assert_eq!(site_name("https://example.org:443/"), "example.org");
        assert_eq!(site_name("https://xn--pple-43d.com/"), "xn--pple-43d.com");
        assert_eq!(site_name("https://аpple.com/"), "xn--pple-43d.com");
        assert_eq!(
            site_name("chrome-extension://abc/"),
            "chrome-extension://abc"
        );
    }
}
