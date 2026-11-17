//! The agent bridge's tools (`crate::agent` is the protocol and the socket):
//! what a coding agent may read, and of which pages. Only pages this
//! computer serves, never one of the private space, and nothing is changed
//! in any page.

use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use magnetite_engine::{ContentId, Engine};
use serde_json::{Value, json};

use crate::agent::{self, Content, ToolResult};
use crate::{capture, dev, lock};

use super::{MagnetiteApp, global};

/// How long an agent waits for a tool before it is told the page did not
/// answer.
const TOOL_TIMEOUT: Duration = Duration::from_secs(30);

/// The most device pixels a full-page screenshot for an agent may hold.
const AGENT_PIXELS: f64 = 40e6;

/// Where a tool's answer goes: back to the connection's thread.
type Done = Box<dyn FnOnce(ToolResult) + Send>;

/// A page an agent may read.
struct AgentPage {
    id: String,
    content: ContentId,
    url: String,
    title: String,
}

impl MagnetiteApp {
    /// Turn the bridge on or off as Settings → Developer says: at start,
    /// and whenever the setting changes.
    pub(super) fn apply_agent_bridge(&self) {
        let on = self.with_state(|s| s.settings.developer.agent_bridge);
        let mut bridge = lock(&self.agent);
        if !on {
            *bridge = AgentBridge::default();
            return;
        }
        if bridge.server.is_some() {
            return;
        }
        let tools: Arc<agent::Tools> = Arc::new(|name, args| {
            let (send, receive) = mpsc::channel();
            let (name, args) = (name.to_string(), args.clone());
            Engine::post(Box::new(move || {
                let done: Done = Box::new(move |result| {
                    let _ = send.send(result);
                });
                match global() {
                    Some(app) => app.agent_tool(Engine::global(), &name, &args, done),
                    None => done(Err("Magnetite is quitting".into())),
                }
            }));
            receive
                .recv_timeout(TOOL_TIMEOUT)
                .unwrap_or_else(|_| Err("the page did not answer in time".into()))
        });
        // An agent came or went: the developer bar shows it.
        let changed = Arc::new(|| {
            Engine::post(Box::new(|| {
                if let Some(app) = global() {
                    app.show_state_paced(Engine::global());
                }
            }));
        });
        match agent::serve(self.data_dir.join(agent::SOCKET), tools, changed) {
            Ok(server) => {
                *bridge = AgentBridge {
                    server: Some(server),
                    error: None,
                }
            }
            Err(e) => bridge.error = Some(e.to_string()),
        }
    }

    /// Close the bridge's socket: Magnetite quits.
    pub(super) fn close_agent_bridge(&self) {
        *lock(&self.agent) = AgentBridge::default();
    }

    /// For Settings: whether the bridge runs, how many agents are connected,
    /// why it could not start, and the command an agent is given.
    pub(super) fn agent_status(&self) -> Value {
        let bridge = lock(&self.agent);
        let program = std::env::current_exe().unwrap_or_default();
        let mut command = vec![program.display().to_string(), "mcp".into()];
        // Another data folder than the usual one: the agent must name it.
        if self.data_dir != crate::default_data_dir() {
            command.extend(["--data-dir".into(), self.data_dir.display().to_string()]);
        }
        json!({
            "running": bridge.server.is_some(),
            "clients": bridge.server.as_ref().map_or(0, agent::Server::clients),
            "error": bridge.error,
            "socket": bridge.server.as_ref().map(|s| s.path().display().to_string()),
            "command": command,
        })
    }

    /// How many agents are connected (the developer bar's mark).
    pub(super) fn agent_clients(&self) -> usize {
        lock(&self.agent)
            .server
            .as_ref()
            .map_or(0, agent::Server::clients)
    }

    /// The loaded pages an agent may read: served by this computer, and not
    /// in the private space.
    fn agent_pages(&self) -> Vec<AgentPage> {
        self.with_state(|s| {
            s.sidebar
                .all_tabs()
                .filter(|t| dev::loopback_address(&t.url).is_some())
                .filter(|t| {
                    !s.find_tab(&t.id)
                        .and_then(|(space, _)| space)
                        .is_some_and(|sp| s.is_private_space(&sp.id))
                })
                .filter_map(|t| {
                    Some(AgentPage {
                        id: t.id.clone(),
                        content: s.runtime.get(&t.id)?.content?,
                        url: t.url.clone(),
                        title: t.title.clone(),
                    })
                })
                .collect()
        })
    }

    /// The page a tool was asked about: the one named, else the local page
    /// in front, else the only one there is.
    fn agent_page(&self, args: &Value) -> Result<AgentPage, String> {
        let mut pages = self.agent_pages();
        let pick = |pages: &mut Vec<AgentPage>, at: usize| Ok(pages.swap_remove(at));
        if let Some(id) = args["page"].as_str() {
            return match pages.iter().position(|p| p.id == id) {
                Some(at) => pick(&mut pages, at),
                None => Err(format!("no local page with id {id}; see list_pages")),
            };
        }
        let front = self.with_state(|s| s.active_tab_id().map(str::to_owned));
        if let Some(at) = pages.iter().position(|p| Some(&p.id) == front.as_ref()) {
            return pick(&mut pages, at);
        }
        match pages.len() {
            0 => Err("no local page is open in Magnetite".into()),
            1 => pick(&mut pages, 0),
            _ => Err("several local pages are open: name one by its id from list_pages".into()),
        }
    }

    /// Run tool `name` for an agent; `done` gets the answer. UI thread.
    fn agent_tool(&self, engine: &Engine, name: &str, args: &Value, done: Done) {
        // Turned off while the request was on its way.
        if lock(&self.agent).server.is_none() {
            return done(Err("the agent bridge is off".into()));
        }
        if name == "list_pages" {
            let front = self.with_state(|s| s.active_tab_id().map(str::to_owned));
            let pages: Vec<Value> = (self.agent_pages().iter())
                .map(|p| {
                    let (project, errors) = self.with_state(|s| {
                        (s.runtime.get(&p.id))
                            .map(|rt| (rt.dev.project.clone(), rt.dev.errors))
                            .unwrap_or_default()
                    });
                    json!({
                        "id": p.id,
                        "url": p.url,
                        "title": p.title,
                        "project": project,
                        "console_errors": errors,
                        "in_front": Some(&p.id) == front.as_ref(),
                    })
                })
                .collect();
            return done(Ok(vec![Content::Text(json!(pages).to_string())]));
        }
        let page = match self.agent_page(args) {
            Ok(page) => page,
            Err(why) => return done(Err(why)),
        };
        // A picture is handed out only if the page is still at the address
        // it was chosen by when the picture is there.
        let done: Done = {
            let (id, url) = (page.id.clone(), page.url.clone());
            Box::new(move |result| {
                let same = global().is_some_and(|app| {
                    app.agent_pages().iter().any(|p| p.id == id && p.url == url)
                });
                done(if same {
                    result
                } else {
                    Err("the page went to another site".into())
                });
            })
        };
        match name {
            "console_errors" => {
                let log: Vec<String> = self.with_state(|s| {
                    (s.runtime.get(&page.id))
                        .map(|rt| rt.dev.log.iter().cloned().collect())
                        .unwrap_or_default()
                });
                let text = if log.is_empty() {
                    format!("No errors in the console of {}.", page.url)
                } else {
                    log.join("\n")
                };
                done(Ok(vec![Content::Text(text)]));
            }
            "screenshot" if args["full_page"] == true => engine.capture_full_page(
                page.content,
                AGENT_PIXELS,
                Box::new(move |tiles| match tiles {
                    Ok(t) => {
                        std::thread::spawn(move || {
                            done(
                                capture::finish_tiles(&t.tiles, t.cut_at)
                                    .map(|cap| vec![Content::Png(cap.png)]),
                            );
                        });
                    }
                    Err(e) => done(Err(e)),
                }),
            ),
            "screenshot" => engine.devtools(
                page.content,
                "Page.captureScreenshot",
                &json!({ "format": "png" }),
                Box::new(move |shot| {
                    use base64::Engine as _;
                    let png = shot.and_then(|s| {
                        base64::engine::general_purpose::STANDARD
                            .decode(s["data"].as_str().unwrap_or_default())
                            .map_err(|e| e.to_string())
                    });
                    done(png.map(|png| vec![Content::Png(png)]));
                }),
            ),
            "read_page" => {
                let most = args["max_chars"].as_u64().unwrap_or(20_000).min(200_000);
                // The arguments go in as data (JSON), never as code.
                let expression = format!(
                    "({READ_PAGE})({}, {}, {most})",
                    json!(args["selector"].as_str()),
                    json!(args["format"].as_str() == Some("text")),
                );
                Self::agent_evaluate(engine, page.content, &expression, done, |value| {
                    value.as_str().map(str::to_owned)
                });
            }
            "describe_element" => {
                let Some(selector) = args["selector"].as_str() else {
                    return done(Err("describe_element needs a selector".into()));
                };
                let expression = format!("({})(0, 0, {})", dev::ELEMENT_AT, json!(selector));
                Self::agent_evaluate(engine, page.content, &expression, done, |value| {
                    value.is_object().then(|| dev::element_context(value, None))
                });
            }
            _ => done(Err(format!("unknown tool: {name}"))),
        }
    }

    /// Evaluate `expression` in page `c` and answer with what `read` makes
    /// of its value; no value: nothing matched. The document that runs it
    /// checks itself that this computer serves it (`LOCAL_ONLY`): a page
    /// that went elsewhere since it was chosen answers nothing.
    fn agent_evaluate(
        engine: &Engine,
        c: ContentId,
        expression: &str,
        done: Done,
        read: impl FnOnce(&Value) -> Option<String> + 'static,
    ) {
        let guarded = format!("({LOCAL_ONLY})(() => ({expression}))");
        let params = json!({ "expression": guarded, "returnByValue": true });
        engine.devtools(
            c,
            "Runtime.evaluate",
            &params,
            Box::new(move |reply| {
                done(match reply {
                    Ok(reply) if reply.get("exceptionDetails").is_some() => {
                        Err("not a valid CSS selector".into())
                    }
                    Ok(reply) if reply["result"]["value"]["local"] != true => {
                        Err("the page went to another site".into())
                    }
                    Ok(reply) => read(&reply["result"]["value"]["value"])
                        .map(|text| vec![Content::Text(text)])
                        .ok_or_else(|| "nothing on the page matches".to_string()),
                    Err(e) => Err(e),
                });
            }),
        );
    }
}

/// Runs `read` in the page only if this computer serves the document it
/// runs in, as `dev::loopback_address` says for an address: the check and
/// the read are one script in one document. `location` cannot be replaced
/// by a page.
const LOCAL_ONLY: &str = r#"(read) => {
  const host = location.hostname.replace(/\.$/, "");
  const local = /^https?:$/.test(location.protocol)
    && (host === "localhost" || host.endsWith(".localhost") || /^127(\.\d{1,3}){3}$/.test(host) || host === "[::1]");
  return local ? { local, value: read() } : { local };
}"#;

/// The markup or the visible text of the document, or of the first element
/// matching `selector`, cut to `most` characters. Null: nothing matches.
const READ_PAGE: &str = r#"(selector, text, most) => {
  const el = selector ? document.querySelector(selector) : document.documentElement;
  if (!el) return null;
  const all = text ? (el.innerText ?? el.textContent ?? "") : el.outerHTML;
  return all.length > most ? all.slice(0, most) + `\n… (cut at ${most} of ${all.length} characters)` : all;
}"#;

/// The bridge, while Settings has it on.
#[derive(Default)]
pub(super) struct AgentBridge {
    server: Option<agent::Server>,
    /// Why it could not start.
    error: Option<String>,
}
