//! Developer features, the parts that need no browser: which pages count as
//! local, what a page is shown as (`Emulation`) and the `DevTools` commands
//! that make it so, and which program serves a local port.

use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// What the sidebar shows about a page for developers (`TabRuntime::dev`).
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DevState {
    /// The page is served from this computer, or from a host the user named
    /// (Settings → Developer): the developer bar shows for it.
    pub local: bool,
    /// Errors in the page's console since it began to load.
    pub errors: u32,
    /// Nothing listens at the page's address (`host:port`): the page loads
    /// by itself once something does.
    pub waiting: Option<String>,
    /// The folder of the program that serves the page, by its name.
    pub project: Option<String>,
    /// This computer serves the page: "reset" clears its site data too.
    pub clears: bool,
    /// Which wait for a server `waiting` is: a watcher that outlived its
    /// own wait leaves a later one alone.
    #[serde(skip)]
    pub wait: u64,
    /// The page's window reports failed requests (`Engine::watch_errors`).
    #[serde(skip)]
    pub watched: bool,
    /// The `host:port` `project` was looked up for.
    #[serde(skip)]
    pub served_by: Option<String>,
    pub emulation: Emulation,
    /// The page has a phone beside it (a responsive split): the same
    /// address shows in both, whichever of them navigates.
    pub responsive: bool,
    /// This page is that phone: narrow, with touch and a mobile layout.
    pub phone: bool,
    /// The page was told that it is a phone.
    #[serde(skip)]
    pub phone_told: bool,
    /// The address this page is loading came from its other half, and does
    /// not go back there.
    #[serde(skip)]
    pub following: bool,
}

/// The most errors counted for a page; the badge shows "99+" long before.
pub const MAX_ERRORS: u32 = 999;

impl DevState {
    /// One more error in the page's console.
    pub fn note_error(&mut self) {
        self.errors = (self.errors + 1).min(MAX_ERRORS);
    }
}

/// What a page is shown as instead of this computer's own settings. All
/// off: the page as it is.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Emulation {
    /// `prefers-color-scheme`, when set.
    pub color_scheme: Option<ColorScheme>,
    /// `prefers-reduced-motion: reduce`.
    pub reduced_motion: bool,
    /// The print style sheet.
    pub print: bool,
    pub network: Network,
    /// The processor four times slower.
    pub slow_cpu: bool,
    /// Every request goes to the server: nothing is read from the cache.
    pub no_cache: bool,
    /// The time zone the page sees (`Asia/Tokyo`), when set.
    pub timezone: Option<String>,
}

/// Whether `zone` reads like the name of a time zone (`Europe/Berlin`,
/// `UTC`, `Etc/GMT+5`). The browser knows which ones exist.
pub fn is_timezone(zone: &str) -> bool {
    (1..=64).contains(&zone.len())
        && zone
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/_+-".contains(c))
}

/// What a phone is to a page whose window already has a phone's width:
/// touch instead of a mouse, and a mobile layout (the viewport tag counts,
/// scrollbars lie over the page). `on` false: a computer again.
pub fn phone_commands(on: bool) -> [(&'static str, Value); 3] {
    [
        (
            "Emulation.setDeviceMetricsOverride",
            // No size of its own: the window's.
            json!({ "width": 0, "height": 0, "deviceScaleFactor": 0, "mobile": on }),
        ),
        (
            "Emulation.setTouchEmulationEnabled",
            json!({ "enabled": on, "maxTouchPoints": 5 }),
        ),
        (
            "Emulation.setEmitTouchEventsForMouse",
            json!({ "enabled": on, "configuration": "mobile" }),
        ),
    ]
}

/// The width of the phone half of a responsive split, in points.
pub const PHONE_WIDTH: i32 = 390;

/// The pictures of a capture set, in the order they are taken: each row of
/// the sheet is one colour scheme, each column a window size (CSS pixels).
pub const CAPTURE_SCHEMES: [ColorScheme; 2] = [ColorScheme::Light, ColorScheme::Dark];
pub const CAPTURE_SIZES: [(u32, u32); 3] = [(390, 844), (768, 1024), (1280, 800)];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorScheme {
    Light,
    Dark,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Network {
    #[default]
    Online,
    /// About a slow mobile connection (`DevTools`' "Slow 4G").
    Slow,
    Offline,
}

impl Emulation {
    /// `Emulation.setEmulatedMedia` for this, with `scheme` as the colour
    /// scheme (a capture set shows both, whatever the page is shown as).
    pub fn media(&self, scheme: Option<ColorScheme>) -> Value {
        let scheme = match scheme {
            Some(ColorScheme::Light) => "light",
            Some(ColorScheme::Dark) => "dark",
            None => "",
        };
        let motion = if self.reduced_motion { "reduce" } else { "" };
        json!({
            "media": if self.print { "print" } else { "" },
            "features": [
                { "name": "prefers-color-scheme", "value": scheme },
                { "name": "prefers-reduced-motion", "value": motion },
            ],
        })
    }

    /// The `DevTools` commands that show a page as this, coming from `was`:
    /// only what differs, so a page nothing was changed for is never
    /// touched.
    pub fn commands(&self, was: &Self) -> Vec<(&'static str, Value)> {
        let mut out = Vec::new();
        let media = |e: &Self| (e.color_scheme, e.reduced_motion, e.print);
        if media(self) != media(was) {
            out.push(("Emulation.setEmulatedMedia", self.media(self.color_scheme)));
        }
        if self.timezone != was.timezone {
            // Empty: this computer's own again.
            out.push((
                "Emulation.setTimezoneOverride",
                json!({ "timezoneId": self.timezone.as_deref().unwrap_or_default() }),
            ));
        }
        if (self.network, self.no_cache) != (was.network, was.no_cache) {
            out.push(("Network.enable", json!({})));
        }
        if self.network != was.network {
            // Bytes per second and milliseconds; -1: no limit.
            let (offline, latency, down, up) = match self.network {
                Network::Online => (false, 0.0, -1.0, -1.0),
                Network::Slow => (false, 562.5, 180_000.0, 84_375.0),
                Network::Offline => (true, 0.0, -1.0, -1.0),
            };
            out.push((
                "Network.emulateNetworkConditions",
                json!({
                    "offline": offline,
                    "latency": latency,
                    "downloadThroughput": down,
                    "uploadThroughput": up,
                }),
            ));
        }
        if self.no_cache != was.no_cache {
            out.push((
                "Network.setCacheDisabled",
                json!({ "cacheDisabled": self.no_cache }),
            ));
        }
        if self.slow_cpu != was.slow_cpu {
            out.push((
                "Emulation.setCPUThrottlingRate",
                json!({ "rate": if self.slow_cpu { 4 } else { 1 } }),
            ));
        }
        out
    }
}

/// Describes the element at a point of the page (`(x, y)` in the visible
/// area's CSS pixels): run in the page as `(ELEMENT_AT)(x, y)`.
pub const ELEMENT_AT: &str = include_str!("element_at.js");

/// An element as text for an issue or a coding agent: where it is, its
/// markup, the styles that shape it, and the picture's file if there is one.
/// `element` is what `ELEMENT_AT` returned.
pub fn element_context(element: &Value, picture: Option<&Path>) -> String {
    use std::fmt::Write as _;
    let text = |key: &str| element[key].as_str().unwrap_or_default();
    let mut out = format!("Element `{}` on {}", text("selector"), text("url"));
    if !text("title").is_empty() {
        let _ = write!(out, " ({})", text("title"));
    }
    out.push('\n');
    let n = |key: &str, i: usize| element[key][i].as_f64().unwrap_or(0.0).round();
    let _ = writeln!(
        out,
        "Size: {} × {} px at ({}, {}), viewport {} × {} px",
        n("rect", 2),
        n("rect", 3),
        n("rect", 0),
        n("rect", 1),
        n("viewport", 0),
        n("viewport", 1),
    );
    if let Some(picture) = picture {
        let _ = writeln!(out, "Screenshot: {}", picture.display());
    }
    if !text("text").is_empty() {
        let _ = writeln!(out, "Text: {}", text("text"));
    }
    let _ = write!(out, "\nHTML:\n```html\n{}\n```\n", text("html"));
    if let Some(styles) = element["styles"].as_object().filter(|s| !s.is_empty()) {
        out.push_str("\nComputed styles:\n```css\n");
        for (name, value) in styles {
            let _ = writeln!(out, "{name}: {};", value.as_str().unwrap_or_default());
        }
        out.push_str("```\n");
    }
    out
}

/// Developer features (Settings → Developer). This device's own, never
/// synced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeveloperSettings {
    /// Local pages get the developer bar and the error count.
    pub enabled: bool,
    /// Hosts that count as local besides this computer: `staging.example.org`,
    /// or `*.test` for every name under one.
    pub hosts: Vec<String>,
}

impl Default for DeveloperSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            hosts: Vec::new(),
        }
    }
}

/// The most hosts the user can name.
const MAX_HOSTS: usize = 50;

impl DeveloperSettings {
    /// Hosts as lower-case names, each once; what is no host name is dropped.
    pub fn sanitised(mut self) -> Self {
        let mut hosts: Vec<String> = Vec::new();
        for host in std::mem::take(&mut self.hosts) {
            let mut host = host.trim().to_ascii_lowercase();
            // An IPv6 address as an address has it, in brackets, and in the
            // one spelling `is_local` compares with.
            let v6 = (host.strip_prefix('[').and_then(|h| h.strip_suffix(']')))
                .and_then(|ip| ip.parse::<std::net::Ipv6Addr>().ok());
            if let Some(ip) = v6 {
                host = format!("[{ip}]");
            }
            let name = host.strip_prefix("*.").unwrap_or(&host);
            let valid = v6.is_some()
                || !name.is_empty()
                    && name.len() <= 253
                    && (name.bytes()).all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
                    && !name.starts_with('.')
                    && !name.ends_with('.');
            if valid && !hosts.contains(&host) && hosts.len() < MAX_HOSTS {
                hosts.push(host);
            }
        }
        self.hosts = hosts;
        self
    }
}

/// Whether the page at `url` is one under development: served over http(s)
/// from this computer (`localhost`, a name under it, a loopback address), or
/// from one of `hosts`.
pub fn is_local(url: &str, hosts: &[String]) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    match url.host() {
        Some(url::Host::Domain(name)) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            name == "localhost"
                || name.ends_with(".localhost")
                || hosts.iter().any(|h| match h.strip_prefix("*.") {
                    Some(parent) => name
                        .strip_suffix(parent)
                        .is_some_and(|rest| rest.ends_with('.')),
                    None => *h == name,
                })
        }
        // An address named by the user counts too (a server in the local
        // network), written as it is in an address: `[::1]` with brackets.
        Some(url::Host::Ipv4(ip)) => ip.is_loopback() || hosts.contains(&ip.to_string()),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback() || hosts.contains(&format!("[{ip}]")),
        None => false,
    }
}

/// `host:port` of the page at `url` if this computer serves it: the address
/// a development server listens on.
pub fn loopback_address(url: &str) -> Option<String> {
    if !is_local(url, &[]) {
        return None;
    }
    let url = url::Url::parse(url).ok()?;
    Some(format!(
        "{}:{}",
        url.host_str()?,
        url.port_or_known_default()?
    ))
}

/// The port of `address` (`host:port`).
fn port_of(address: &str) -> Option<u16> {
    address.rsplit_once(':')?.1.parse().ok()
}

/// Whether something listens at `address` (`host:port`) right now. Blocks
/// for at most a moment: for a worker thread.
pub fn listens(address: &str) -> bool {
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
    let Some((host, port)) = address.rsplit_once(':') else {
        return false;
    };
    let name = host.trim_end_matches('.').to_ascii_lowercase();
    // `localhost` and the names under it are this computer by definition;
    // not every system's resolver knows that.
    let addresses: Vec<SocketAddr> = if name == "localhost" || name.ends_with(".localhost") {
        let Ok(port) = port.parse() else {
            return false;
        };
        vec![
            (Ipv4Addr::LOCALHOST, port).into(),
            (Ipv6Addr::LOCALHOST, port).into(),
        ]
    } else {
        address
            .to_socket_addrs()
            .map(Iterator::collect)
            .unwrap_or_default()
    };
    addresses
        .iter()
        // Loopback only: a name that resolves elsewhere is never connected to.
        .filter(|a| a.ip().is_loopback())
        .any(|a| TcpStream::connect_timeout(a, Duration::from_millis(300)).is_ok())
}

/// A program of the user's that listens on a local port.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Server {
    pub port: u16,
    /// The program's name (`node`).
    pub command: String,
    /// The name of the folder it runs in, unless that says nothing (the
    /// home folder, the root).
    pub project: Option<String>,
}

fn lsof(args: &[&str]) -> Option<String> {
    let out = Command::new("lsof").args(args).output().ok()?;
    // lsof exits with 1 when some files could not be listed; what it did
    // print still counts.
    String::from_utf8(out.stdout).ok()
}

/// The folder process `pid` runs in.
fn folder_of(pid: u32) -> Option<PathBuf> {
    let out = lsof(&["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])?;
    let dir = out.lines().find_map(|l| l.strip_prefix('n'))?;
    Some(PathBuf::from(dir))
}

/// The name of the project in `dir`: the folder's own; none for the root and
/// the home folder, which name no project.
fn project_name(dir: &Path, home: Option<&Path>) -> Option<String> {
    if home == Some(dir) {
        return None;
    }
    Some(dir.file_name()?.to_string_lossy().into_owned())
}

/// Whether a program running in `dir` is the user's own work rather than an
/// installed app or a service: below the home folder, outside its Library
/// and outside any app bundle.
fn is_workspace(dir: &Path, home: Option<&Path>) -> bool {
    let Some(inside) = home.and_then(|home| dir.strip_prefix(home).ok()) else {
        return false;
    };
    let mut parts = inside.components().map(|c| c.as_os_str().to_string_lossy());
    let first = parts.next();
    first.is_some_and(|first| {
        first != "Library"
            && !std::iter::once(first)
                .chain(parts)
                .any(|p| p.ends_with(".app"))
    })
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// What `lsof -F pcn` printed for listening sockets: process id, command
/// and port of each, a port once (the same socket shows per address family).
fn parse_listeners(out: &str) -> Vec<(u32, String, u16)> {
    let mut found: Vec<(u32, String, u16)> = Vec::new();
    let (mut pid, mut command) = (None, String::new());
    for line in out.lines() {
        let Some(kind) = line.chars().next() else {
            continue;
        };
        let value = &line[kind.len_utf8()..];
        match kind {
            'p' => pid = value.parse().ok(),
            'c' => command = value.to_string(),
            'n' => {
                // `*:5173`, `127.0.0.1:3000`, `[::1]:3000`.
                if let (Some(pid), Some(port)) = (pid, port_of(value))
                    && !found.iter().any(|(_, _, p)| *p == port)
                {
                    found.push((pid, command.clone(), port));
                }
            }
            _ => {}
        }
    }
    found
}

/// The name of the folder the program listening at `address` runs in.
/// Blocks while `lsof` runs: for a worker thread.
pub fn project_at(address: &str) -> Option<String> {
    let port = port_of(address)?;
    let out = lsof(&["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-Fpcn"])?;
    let (pid, _, _) = parse_listeners(&out).into_iter().next()?;
    project_name(&folder_of(pid)?, home().as_deref())
}

/// Ports below this belong to the system's services, not to projects.
const FIRST_USER_PORT: u16 = 1024;

/// The development servers that run: the user's programs listening on
/// local ports from a folder of their own work (`is_workspace`), lowest
/// port first. Blocks while `lsof` runs: for a worker thread.
pub fn servers() -> Vec<Server> {
    let Some(out) = lsof(&["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpcn"]) else {
        return Vec::new();
    };
    let home = home();
    let mut servers: Vec<Server> = parse_listeners(&out)
        .into_iter()
        .filter(|(_, _, port)| *port >= FIRST_USER_PORT)
        .filter_map(|(pid, command, port)| {
            let dir = folder_of(pid).filter(|dir| is_workspace(dir, home.as_deref()))?;
            Some(Server {
                port,
                command,
                project: project_name(&dir, home.as_deref()),
            })
        })
        .collect();
    servers.sort_by_key(|s| s.port);
    servers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_pages_are_this_computer_and_named_hosts() {
        let none: &[String] = &[];
        for url in [
            "http://localhost:5173/",
            "https://localhost/",
            "http://app.localhost:3000/x",
            "http://127.0.0.1:8080/",
            "http://127.1.2.3/",
            "http://[::1]:3000/",
            "http://LOCALHOST:3000/",
        ] {
            assert!(is_local(url, none), "{url}");
        }
        for url in [
            "https://localhost.example.org/",
            "https://example.org/localhost",
            "http://user@example.org:80@localhost.evil.example/",
            "file:///Users/me/index.html",
            "magnetite://ui/settings",
            "http://192.168.1.4:3000/",
            "not a url",
        ] {
            assert!(!is_local(url, none), "{url}");
        }
        let hosts = ["staging.example.org".to_string(), "*.test".to_string()];
        assert!(is_local("https://staging.example.org/a", &hosts));
        assert!(is_local("http://shop.test/", &hosts));
        assert!(is_local("http://a.b.test/", &hosts));
        // The name itself is not under itself, nor is a longer word.
        assert!(!is_local("http://test/", &hosts));
        assert!(!is_local("http://contest/", &hosts));
        assert!(!is_local("https://example.org/", &hosts));
        assert!(!is_local(
            "https://staging.example.org.evil.example/",
            &hosts
        ));
    }

    #[test]
    fn an_address_the_user_named_is_local_but_no_loopback() {
        let named = DeveloperSettings {
            hosts: vec![
                "192.168.1.4".into(),
                "[FD00:0:0:0:0:0:0:1]".into(),
                "[fd00::1]".into(),
                "[nope]".into(),
            ],
            ..DeveloperSettings::default()
        };
        let hosts = named.sanitised().hosts;
        assert_eq!(hosts, ["192.168.1.4", "[fd00::1]"]);
        assert!(is_local("http://192.168.1.4:3000/", &hosts));
        assert!(is_local("http://[fd00::1]:3000/", &hosts));
        assert!(!is_local("http://192.168.1.5:3000/", &hosts));
        assert!(!is_local("http://192.168.1.4:3000/", &[]));
        assert_eq!(loopback_address("http://192.168.1.4:3000/"), None);
    }

    #[test]
    fn a_name_under_localhost_is_tried_on_this_computer() {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = server.local_addr().unwrap().port();
        assert!(listens(&format!("shop.localhost:{port}")));
        assert!(listens(&format!("localhost:{port}")));
        drop(server);
        assert!(!listens(&format!("shop.localhost:{port}")));
        assert!(!listens("no port"));
    }

    #[test]
    fn a_loopback_address_has_its_port() {
        assert_eq!(
            loopback_address("http://localhost:5173/a?b").as_deref(),
            Some("localhost:5173")
        );
        assert_eq!(
            loopback_address("http://127.0.0.1/").as_deref(),
            Some("127.0.0.1:80")
        );
        assert_eq!(
            loopback_address("http://[::1]:3000/").as_deref(),
            Some("[::1]:3000")
        );
        assert_eq!(loopback_address("https://example.org/"), None);
    }

    #[test]
    fn hosts_are_cleaned() {
        let s = DeveloperSettings {
            hosts: vec![
                " Staging.Example.org ".into(),
                "staging.example.org".into(),
                "*.test".into(),
                "https://x.example".into(),
                "*.".into(),
                String::new(),
            ],
            ..DeveloperSettings::default()
        }
        .sanitised();
        assert_eq!(s.hosts, ["staging.example.org", "*.test"]);
    }

    #[test]
    fn a_time_zone_is_a_name_and_set_by_itself() {
        assert!(is_timezone("Europe/Berlin") && is_timezone("Etc/GMT+5") && is_timezone("UTC"));
        assert!(!is_timezone("") && !is_timezone("Europe/Berlin\"; x") && !is_timezone("a b"));
        let tokyo = Emulation {
            timezone: Some("Asia/Tokyo".into()),
            ..Emulation::default()
        };
        let there = tokyo.commands(&Emulation::default());
        assert_eq!(there.len(), 1);
        assert_eq!(there[0].1["timezoneId"], "Asia/Tokyo");
        // Back to this computer's own: an empty name.
        let back = Emulation::default().commands(&tokyo);
        assert_eq!(back[0].1["timezoneId"], "");
    }

    #[test]
    fn only_what_changed_is_sent() {
        let off = Emulation::default();
        assert_eq!(off.commands(&off).len(), 0);
        let dark = Emulation {
            color_scheme: Some(ColorScheme::Dark),
            ..off.clone()
        };
        let sent = dark.commands(&off);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "Emulation.setEmulatedMedia");
        assert_eq!(sent[0].1["features"][0]["value"], "dark");
        // Back to the page as it is: the same command, with nothing set.
        let back = off.commands(&dark);
        assert_eq!(back[0].1["features"][0]["value"], "");
        let offline = Emulation {
            network: Network::Offline,
            ..dark.clone()
        };
        let names: Vec<&str> = offline.commands(&dark).iter().map(|c| c.0).collect();
        assert_eq!(
            names,
            ["Network.enable", "Network.emulateNetworkConditions"]
        );
    }

    #[test]
    fn an_element_reads_as_text() {
        let element = json!({
            "selector": "main > button.buy",
            "url": "http://localhost:5173/cart",
            "title": "Cart",
            "rect": [40.4, 120.0, 52.0, 24.0],
            "viewport": [800, 600],
            "text": "Buy",
            "html": "<button class=\"buy\">Buy</button>",
            "styles": { "color": "rgb(0, 0, 0)", "display": "inline-block" },
        });
        let text = element_context(&element, Some(Path::new("/tmp/element.png")));
        assert!(
            text.starts_with("Element `main > button.buy` on http://localhost:5173/cart (Cart)\n")
        );
        assert!(text.contains("Size: 52 × 24 px at (40, 120), viewport 800 × 600 px\n"));
        assert!(text.contains("Screenshot: /tmp/element.png\n"));
        assert!(text.contains("```html\n<button class=\"buy\">Buy</button>\n```"));
        assert!(text.contains("color: rgb(0, 0, 0);\ndisplay: inline-block;\n"));
        // Without a picture or styles those parts are left out.
        let bare = element_context(
            &json!({ "selector": "p", "url": "http://localhost/", "html": "<p></p>" }),
            None,
        );
        assert!(!bare.contains("Screenshot") && !bare.contains("Computed styles"));
    }

    #[test]
    fn listeners_are_read_once_per_port() {
        let out =
            "p412\ncnode\nf23\nn*:5173\nf24\nn[::1]:5173\np77\ncpostgres\nf7\nn127.0.0.1:5432\n";
        assert_eq!(
            parse_listeners(out),
            [
                (412, "node".to_string(), 5173),
                (77, "postgres".to_string(), 5432)
            ]
        );
    }

    #[test]
    fn the_home_folder_names_no_project() {
        let home = Some(Path::new("/Users/me"));
        assert_eq!(
            project_name(Path::new("/Users/me/code/shop"), home).as_deref(),
            Some("shop")
        );
        assert_eq!(project_name(Path::new("/Users/me"), home), None);
        assert_eq!(project_name(Path::new("/"), home), None);
    }

    #[test]
    fn a_workspace_is_the_users_own_folder() {
        let home = Some(Path::new("/Users/me"));
        assert!(is_workspace(Path::new("/Users/me/code/shop"), home));
        for dir in [
            "/Users/me",
            "/Users/me/Library/Application Support/Tool",
            "/Users/me/Applications/Hub.app/Contents/MacOS",
            "/Applications/Hub.app/Contents/MacOS",
            "/opt/homebrew/var/postgresql@18",
            "/",
        ] {
            assert!(!is_workspace(Path::new(dir), home), "{dir}");
        }
        assert!(!is_workspace(Path::new("/Users/me/code"), None));
    }
}
