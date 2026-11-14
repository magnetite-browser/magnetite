//! Chrome extensions in our own UI.
//!
//! Listing: Chromium keeps a profile's extensions in `Secure Preferences`
//! (`extensions.settings`): where each one lives and whether it is enabled.
//! Name, icon and options page come from its manifest; names may be
//! localised through `_locales`. Only extensions with a toolbar action are
//! listed: the others have nothing to click.
//!
//! Opening: an extension's action (its popup, or its click handler) runs
//! through Chromium's `Extensions.triggerAction` on the private `DevTools` pipe
//! (`devpipe.rs`), for the tab target of the page in front. Chromium anchors
//! the popup to its own toolbar, which every page window has out of sight
//! (`content.rs`), so the popup opens at the page's top-left corner, next to
//! the sidebar, or with the top bar on at its top-right corner, below the top
//! bar's buttons.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;
use serde_json::{Value, json};

use cef::*;

use crate::engine::{ContextProfile, Engine};
use crate::events::{ContentId, EngineEvent};

/// An installed extension with a toolbar action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtensionInfo {
    /// Chromium's 32-letter id.
    pub id: String,
    pub name: String,
    /// The icon file (inside the extension's directory), if it has one.
    #[serde(skip)]
    pub icon: Option<PathBuf>,
    /// `chrome-extension://…` options page, if it has one.
    pub options_url: Option<String>,
    /// Pinned in Chromium's own UI ("Pin to toolbar" on its extensions page).
    pub chromium_pinned: bool,
}

/// Chromium's preference with the ids pinned in its own UI.
const PINS: &str = "extensions.pinned_extensions";

wrap_preference_observer! {
    struct PinWatch {}
    impl PreferenceObserver {
        fn on_preference_changed(&self, _name: Option<&CefString>) {
            Engine::global().emit(EngineEvent::ExtensionPinsChanged);
        }
    }
}

/// Tell the app whenever Chromium's pins of `ctx` change. Chromium writes
/// them to its preference file only seconds later, so the file alone
/// would show a pin late (`Engine::extensions` reads them live instead).
pub(crate) fn watch_pins(ctx: &RequestContext) {
    let name = CefString::from(PINS);
    match ctx.add_preference_observer(Some(&name), Some(&mut PinWatch::new())) {
        Some(watch) => Engine::global().lock().pin_watches.push(watch),
        None => crate::trace("cannot watch Chromium's extension pins"),
    }
}

/// Chromium's pins of `ctx` as they are now, not as last written to disk.
fn live_pins(ctx: &RequestContext) -> Option<Vec<String>> {
    let list = ctx.preference(Some(&CefString::from(PINS)))?.list()?;
    Some(
        (0..list.size())
            .map(|i| CefString::from(&list.string(i)).to_string())
            .collect(),
    )
}

/// Locations Chromium uses for its own built-in extensions (PDF viewer, …).
const COMPONENT: u64 = 5;
const EXTERNAL_COMPONENT: u64 = 10;
/// Icon size we look for (the sidebar shows them at 16 pt, 2× on Retina).
const ICON_SIZE: u64 = 32;

/// Enabled extensions with a toolbar action in the Chromium profile at
/// `profile_dir`, sorted by name. Unreadable entries are skipped.
pub fn installed(profile_dir: &Path) -> Vec<ExtensionInfo> {
    let mut settings = serde_json::Map::new();
    let mut pinned = Vec::new();
    // Older or unsigned builds keep some entries in plain Preferences, which
    // also holds Chromium's pins.
    for file in ["Preferences", "Secure Preferences"] {
        let Some(prefs) = std::fs::read(profile_dir.join(file))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        else {
            continue;
        };
        if let Some(Value::Object(map)) = prefs.pointer("/extensions/settings") {
            settings.extend(map.clone());
        }
        if let Some(Value::Array(ids)) = prefs.pointer("/extensions/pinned_extensions") {
            pinned.extend(ids.iter().filter_map(Value::as_str).map(str::to_string));
        }
    }
    let mut list: Vec<ExtensionInfo> = settings
        .iter()
        .filter_map(|(id, entry)| extension(profile_dir, id, entry))
        .map(|e| ExtensionInfo {
            chromium_pinned: pinned.contains(&e.id),
            ..e
        })
        .collect();
    list.sort_by_key(|e| e.name.to_lowercase());
    list
}

fn extension(profile_dir: &Path, id: &str, entry: &Value) -> Option<ExtensionInfo> {
    if !is_extension_id(id) {
        return None;
    }
    let location = entry.get("location").and_then(Value::as_u64)?;
    let disabled = entry
        .get("disable_reasons")
        .and_then(Value::as_array)
        .is_some_and(|r| !r.is_empty())
        || entry.get("state").and_then(Value::as_u64) == Some(0);
    if location == COMPONENT || location == EXTERNAL_COMPONENT || disabled {
        return None;
    }
    let path = entry.get("path").and_then(Value::as_str)?;
    // Packed extensions live below the profile's Extensions folder; unpacked
    // ones wherever the user loaded them from.
    let dir = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        inside(&profile_dir.join("Extensions"), path)?
    };
    // Everything below is read through the real folder: no file outside it.
    let dir = dir.canonicalize().ok()?;
    let manifest: Value =
        serde_json::from_slice(&read_small(&inside(&dir, "manifest.json")?, MAX_FILE)?).ok()?;
    let action = manifest
        .get("action")
        .or_else(|| manifest.get("browser_action"))?;
    let name = localised(
        &dir,
        &manifest,
        manifest.get("name").and_then(Value::as_str)?,
    );
    let icon = best_icon(action.get("default_icon"))
        .or_else(|| best_icon(manifest.get("icons")))
        .and_then(|rel| inside(&dir, &rel));
    let options = manifest
        .pointer("/options_ui/page")
        .or_else(|| manifest.get("options_page"))
        .and_then(Value::as_str);
    Some(ExtensionInfo {
        id: id.to_string(),
        name,
        icon,
        options_url: options
            .map(|p| format!("chrome-extension://{id}/{}", p.trim_start_matches('/'))),
        chromium_pinned: false,
    })
}

/// Chromium ids are 32 letters a–p.
pub fn is_extension_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b))
}

/// `__MSG_key__` names come from `_locales/<default_locale>/messages.json`.
fn localised(dir: &Path, manifest: &Value, name: &str) -> String {
    let Some(key) = name
        .strip_prefix("__MSG_")
        .and_then(|k| k.strip_suffix("__"))
    else {
        return name.to_string();
    };
    let locale = manifest
        .get("default_locale")
        .and_then(Value::as_str)
        .unwrap_or("en");
    inside(dir, &format!("_locales/{locale}/messages.json"))
        .and_then(|path| read_small(&path, MAX_FILE))
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|m| {
            let messages = m.as_object()?;
            // Message keys are case-insensitive.
            let (_, entry) = messages.iter().find(|(k, _)| k.eq_ignore_ascii_case(key))?;
            entry.get("message")?.as_str().map(str::to_string)
        })
        .unwrap_or_else(|| name.to_string())
}

/// The icon path closest to `ICON_SIZE` (the smallest one at least that big,
/// else the biggest) from a `{ "16": "a.png", … }` map or a single path.
fn best_icon(icons: Option<&Value>) -> Option<String> {
    match icons? {
        Value::String(path) => Some(path.clone()),
        Value::Object(map) => {
            let mut sized: Vec<(u64, &str)> = map
                .iter()
                .filter_map(|(size, path)| Some((size.parse().ok()?, path.as_str()?)))
                .collect();
            sized.sort_unstable();
            sized
                .iter()
                .find(|(size, _)| *size >= ICON_SIZE)
                .or(sized.last())
                .map(|(_, path)| (*path).to_string())
        }
        _ => None,
    }
}

/// The file `rel` below `dir`, refusing paths that climb out of it, also
/// through symbolic links: the real path must still be below `dir`'s real
/// path. `None` for files that do not exist.
fn inside(dir: &Path, rel: &str) -> Option<PathBuf> {
    let rel = Path::new(rel.trim_start_matches('/'));
    if !rel
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    let file = dir.join(rel).canonicalize().ok()?;
    file.starts_with(dir.canonicalize().ok()?).then_some(file)
}

/// Largest manifest, message file or icon we read (they are a few KiB).
const MAX_FILE: u64 = 1024 * 1024;

/// Read `path` if it is a file of at most `max` bytes.
pub(crate) fn read_small(path: &Path, max: u64) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    (meta.is_file() && meta.len() <= max).then(|| std::fs::read(path).ok())?
}

/// An extension icon's bytes (for `magnetite://ui/ext-icons/<id>`).
pub(crate) fn icon_bytes(path: &Path) -> Option<Vec<u8>> {
    read_small(path, MAX_FILE)
}

/// The installed list of one profile, re-read only when Chromium rewrote
/// one of its two preference files.
#[derive(Default)]
pub(crate) struct ExtensionCache {
    by_profile: HashMap<PathBuf, (Option<SystemTime>, Vec<ExtensionInfo>)>,
}

impl Engine {
    /// Installed extensions of `profile`'s context (none for in-memory ones).
    pub fn extensions(&self, profile: Option<&ContextProfile>) -> Vec<ExtensionInfo> {
        if profile.is_some_and(|p| p.ephemeral) {
            return Vec::new();
        }
        let dir = match profile.filter(|p| !p.id.is_empty()) {
            Some(p) => self.config.profile_dir(&p.id),
            None => self.config.default_profile_dir(),
        };
        // Installs land in Secure Preferences, Chromium's pins in Preferences.
        let stamp = ["Preferences", "Secure Preferences"]
            .iter()
            .filter_map(|f| {
                std::fs::metadata(dir.join(f))
                    .and_then(|m| m.modified())
                    .ok()
            })
            .max();
        let cached = (self.lock().extensions.by_profile.get(&dir))
            .filter(|(seen, _)| *seen == stamp)
            .map(|(_, list)| list.clone());
        let mut list = cached.unwrap_or_else(|| {
            let list = installed(&dir);
            (self.lock().extensions.by_profile).insert(dir, (stamp, list.clone()));
            list
        });
        let context = match profile.map(|p| p.id.as_str()).unwrap_or_default() {
            "" => request_context_get_global_context(),
            id => self.lock().contexts.get(id).cloned(),
        };
        if let Some(pins) = context.as_ref().and_then(live_pins) {
            for e in &mut list {
                e.chromium_pinned = pins.contains(&e.id);
            }
        }
        list
    }

    /// The icon file of extension `id` (from any profile listed so far).
    pub(crate) fn extension_icon(&self, id: &str) -> Option<PathBuf> {
        self.lock()
            .extensions
            .by_profile
            .values()
            .flat_map(|(_, list)| list)
            .find(|e| e.id == id)
            .and_then(|e| e.icon.clone())
    }

    /// What the menu bar's Extensions menu lists when it opens: the
    /// extensions of `profile` (read fresh then), `pinned` marked as in the
    /// sidebar.
    #[allow(clippy::unused_self)] // per-platform: only macOS has a menu bar
    pub fn set_extension_menu(&self, profile: Option<ContextProfile>, pinned: Vec<String>) {
        #[cfg(target_os = "macos")]
        crate::menubar::set_extensions(profile, pinned);
        #[cfg(not(target_os = "macos"))]
        let _ = (profile, pinned);
    }

    /// Run extension `extension`'s action (open its popup) for page `id`.
    pub fn open_extension(&self, id: ContentId, extension: &str) {
        if !is_extension_id(extension) || !crate::devpipe::is_open() {
            return;
        }
        let extension = extension.to_string();
        let asked = std::time::Instant::now();
        self.with_tab_target(id, move |tab| {
            crate::devpipe::call(
                "Extensions.triggerAction",
                &json!({ "id": extension, "targetId": tab }),
                move |r| {
                    let took = asked.elapsed().as_millis();
                    match r {
                        Ok(_) => crate::trace(&format!("extension action {extension}: {took} ms")),
                        Err(e) => crate::trace(&format!(
                            "extension action {extension} failed after {took} ms: {e}"
                        )),
                    }
                },
            );
        });
    }

    /// Call `f` with page `id`'s tab target (the `DevTools` id of the tab that
    /// holds the page), found once and remembered. Every page window is a
    /// browser window of its own, so the tab is the one in the page's window:
    /// the page names its own target, the browser-wide session gives that
    /// target's window and the tabs, and the tab with the same window wins
    /// (tabs showing the page's address are asked first).
    fn with_tab_target(&self, id: ContentId, f: impl FnOnce(String) + Send + 'static) {
        if let Some(tab) = self
            .lock()
            .contents
            .get(&id)
            .and_then(|c| c.tab_target.clone())
        {
            f(tab);
            return;
        }
        let found = move |tab: String| {
            if let Some(c) = Engine::global().lock().contents.get_mut(&id) {
                c.tab_target = Some(tab.clone());
            }
            f(tab);
        };
        let reply = Box::new(move |r: Result<Value, String>| {
            let page = r.ok().and_then(|v| {
                let info = v.get("targetInfo")?;
                let text = |k: &str| info.get(k).and_then(Value::as_str).map(str::to_string);
                Some((text("targetId")?, text("url").unwrap_or_default()))
            });
            let Some((page, url)) = page else { return };
            crate::devpipe::call(
                "Browser.getWindowForTarget",
                &json!({ "targetId": page }),
                move |r| {
                    let Some(window) = r.ok().and_then(|v| v.get("windowId")?.as_i64()) else {
                        return;
                    };
                    crate::devpipe::call(
                        "Target.getTargets",
                        &json!({ "filter": [{ "type": "tab" }] }),
                        move |r| {
                            let mut tabs: Vec<(bool, String)> = r
                                .ok()
                                .and_then(|v| v.get("targetInfos")?.as_array().cloned())
                                .unwrap_or_default()
                                .iter()
                                .filter_map(|t| {
                                    let tab = t.get("targetId")?.as_str()?.to_string();
                                    Some((
                                        t.get("url").and_then(Value::as_str) != Some(url.as_str()),
                                        tab,
                                    ))
                                })
                                .collect();
                            tabs.sort();
                            let candidates =
                                tabs.into_iter().map(|(_, tab)| tab).collect::<Vec<_>>();
                            tab_in_window(candidates.into_iter(), window, Box::new(found));
                        },
                    );
                },
            );
        });
        self.devtools(id, "Target.getTargetInfo", &json!({}), reply);
    }
}

/// Ask the `candidates` (tab targets) one by one for their window and hand
/// the first in `window` to `found`.
fn tab_in_window(
    mut candidates: std::vec::IntoIter<String>,
    window: i64,
    found: Box<dyn FnOnce(String) + Send>,
) {
    let Some(tab) = candidates.next() else {
        crate::trace("extension action: no tab for the page");
        return;
    };
    crate::devpipe::call(
        "Browser.getWindowForTarget",
        &json!({ "targetId": tab }),
        move |r| {
            if r.ok().and_then(|v| v.get("windowId")?.as_i64()) == Some(window) {
                found(tab);
            } else {
                tab_in_window(candidates, window, found);
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn lists_enabled_extensions_with_actions_and_their_localised_names() {
        let tmp = std::env::temp_dir().join(format!("magnetite-ext-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let store = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let unpacked = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let disabled = "cccccccccccccccccccccccccccccccc";
        let no_action = "dddddddddddddddddddddddddddddddd";
        let component = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
        let dev = tmp.join("dev");
        write(
            &tmp.join(format!("Extensions/{store}/1.0/manifest.json")),
            r#"{"name":"__MSG_extName__","default_locale":"en","action":{"default_icon":{"16":"i16.png","38":"i38.png","19":"i19.png"}},"options_ui":{"page":"options.html"}}"#,
        );
        write(
            &tmp.join(format!("Extensions/{store}/1.0/_locales/en/messages.json")),
            r#"{"extname":{"message":"Vault"}}"#,
        );
        write(
            &dev.join("manifest.json"),
            r#"{"name":"Ad Blocker","browser_action":{},"icons":{"128":"big.png"}}"#,
        );
        write(
            &tmp.join(format!("Extensions/{disabled}/1.0/manifest.json")),
            r#"{"name":"Off","action":{}}"#,
        );
        write(
            &tmp.join(format!("Extensions/{no_action}/1.0/manifest.json")),
            r#"{"name":"Background only"}"#,
        );
        let prefs = json!({ "extensions": { "settings": {
            store: { "location": 1, "path": format!("{store}/1.0"), "disable_reasons": [] },
            unpacked: { "location": 4, "path": dev.to_string_lossy() },
            disabled: { "location": 1, "path": format!("{disabled}/1.0"), "disable_reasons": [1] },
            no_action: { "location": 1, "path": format!("{no_action}/1.0") },
            component: { "location": 5, "path": "/somewhere/pdf" },
        }}});
        write(&tmp.join(format!("Extensions/{store}/1.0/i38.png")), "png");
        write(&dev.join("big.png"), "png");
        write(&tmp.join("Secure Preferences"), &prefs.to_string());
        let pins = json!({ "extensions": { "pinned_extensions": [store] } });
        write(&tmp.join("Preferences"), &pins.to_string());

        let list = installed(&tmp);
        let names: Vec<&str> = list.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Ad Blocker", "Vault"]);
        let vault = &list[1];
        assert!(vault.chromium_pinned && !list[0].chromium_pinned);
        let real = |p: PathBuf| p.canonicalize().unwrap();
        assert_eq!(
            vault.icon,
            Some(real(tmp.join(format!("Extensions/{store}/1.0/i38.png"))))
        );
        assert_eq!(
            vault.options_url.as_deref(),
            Some(format!("chrome-extension://{store}/options.html").as_str())
        );
        assert_eq!(list[0].icon, Some(real(dev.join("big.png"))));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn files_outside_the_extension_are_never_read() {
        let tmp = std::env::temp_dir().join(format!("magnetite-ext-inside-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let ext = tmp.join("ext");
        write(&ext.join("icons/a.png"), "png");
        write(&tmp.join("secret.txt"), "secret");
        let real = ext.canonicalize().unwrap();
        assert_eq!(inside(&ext, "icons/a.png"), Some(real.join("icons/a.png")));
        assert_eq!(inside(&ext, "/icons/a.png"), Some(real.join("icons/a.png")));
        assert!(inside(&ext, "../secret.txt").is_none());
        assert!(inside(&ext, "missing.png").is_none());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(tmp.join("secret.txt"), ext.join("link.png")).unwrap();
            assert!(
                inside(&ext, "link.png").is_none(),
                "a link out of the folder"
            );
        }
        assert!(is_extension_id("nngceckbapebfimnlniiiahkandclblb"));
        assert!(!is_extension_id("../../etc"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
