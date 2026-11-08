//! Sync client (protocol v2, zero knowledge): pushes the sidebar and
//! settings to a `magnetite-sync` server as signed, encrypted blobs and pulls
//! newer versions. Network work runs on a worker thread; results come back to
//! the UI thread through `Engine::post`.
//!
//! The only secret is the sync code (32 random bytes, 24 words). It lives in
//! `<data-dir>/sync.key`, created with mode 0600, never in `state.json`;
//! everything else (signing key, encryption key, collection ids) is derived
//! from it.
//!
//! Versions only go up. A write counts once the server acknowledges exactly
//! the version it was encrypted for, and a server copy older than what this
//! device holds is refused, so a server cannot roll a device back to an older
//! (authentic) ciphertext.

use std::collections::HashMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use magnetite_core::SidebarSnapshot;
use magnetite_engine::Engine;
use magnetite_sync_proto::{
    ChainKeys, CollectionResponse, CreateChainRequest, PutCollectionRequest, PutCollectionResponse,
    SyncCode,
};
use serde::{Deserialize, Serialize};

use crate::lock;
use crate::state::{Settings, SyncSettings};

pub const SIDEBAR: &str = "sidebar";
pub const SETTINGS: &str = "settings";

/// Time to set up a connection (TLS included), and for a whole request.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// Largest answer read from a sync server: a collection is at most 16 MiB of
/// ciphertext, as JSON a bit more.
const MAX_REPLY: u64 = 32 * 1024 * 1024;

/// Secrets file content (`sync.key`, and `sync.delete` for a chain this
/// device left that the server has not confirmed deleting yet).
#[derive(Serialize, Deserialize, Clone)]
struct Secrets {
    /// Base64 seed of the sync code.
    seed: String,
    /// The server, for a pending deletion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    server: Option<String>,
}

fn secrets_path(data_dir: &Path) -> PathBuf {
    data_dir.join("sync.key")
}

fn pending_delete_path(data_dir: &Path) -> PathBuf {
    data_dir.join("sync.delete")
}

fn load_code(data_dir: &Path) -> Option<SyncCode> {
    let text = std::fs::read_to_string(secrets_path(data_dir)).ok()?;
    let secrets: Secrets = serde_json::from_str(&text).ok()?;
    SyncCode::from_base64(&secrets.seed).ok()
}

/// Write `sync.key` (see `files::write_private`).
fn save_code(data_dir: &Path, code: &SyncCode) -> std::io::Result<()> {
    let secrets = Secrets {
        seed: code.to_base64(),
        server: None,
    };
    crate::files::write_private(&secrets_path(data_dir), &serde_json::to_vec(&secrets)?)
}

/// Remember to delete `code`'s chain on `server` until the server confirms.
fn save_pending_delete(data_dir: &Path, server: &str, code: &SyncCode) -> std::io::Result<()> {
    let secrets = Secrets {
        seed: code.to_base64(),
        server: Some(server.to_string()),
    };
    crate::files::write_private(
        &pending_delete_path(data_dir),
        &serde_json::to_vec(&secrets)?,
    )
}

fn load_pending_delete(data_dir: &Path) -> Option<(String, SyncCode)> {
    let text = std::fs::read_to_string(pending_delete_path(data_dir)).ok()?;
    let secrets: Secrets = serde_json::from_str(&text).ok()?;
    Some((secrets.server?, SyncCode::from_base64(&secrets.seed).ok()?))
}

fn forget_code(data_dir: &Path) {
    std::fs::remove_file(secrets_path(data_dir)).ok();
}

/// The 24 words of this device's sync code, to show the user.
pub fn code_words(data_dir: &Path) -> Option<String> {
    load_code(data_dir).map(|c| c.words())
}

/// The address requests go to (no trailing slash). The server must use
/// `https://`; plain `http://` is accepted only for this machine (`localhost`,
/// `127.0.0.1`, `::1`). A path prefix is fine; user info and queries are not.
fn server_base(url: &str) -> Result<String, String> {
    let url = url.trim().trim_end_matches('/');
    let bad = || format!("not a sync server address: {url}");
    let uri: ureq::http::Uri = url.parse().map_err(|_| bad())?;
    let authority = uri.authority().ok_or_else(bad)?;
    if authority.as_str().contains('@') {
        return Err("the sync server address must not contain a user name".into());
    }
    if uri.query().is_some() {
        return Err(bad());
    }
    let loopback = matches!(authority.host(), "localhost" | "127.0.0.1" | "[::1]");
    match uri.scheme_str() {
        Some("https") => Ok(url.to_string()),
        Some("http") if loopback => Ok(url.to_string()),
        _ => Err(format!(
            "the sync server needs an https:// address (plain http only for this computer): {url}"
        )),
    }
}

/// Settings minus device-specific fields; what the `settings` collection
/// holds. The search engine is not among them: another device holding the
/// words could otherwise send every search here to an address of its choice.
#[derive(Serialize, Deserialize)]
struct SyncedSettings {
    auto_archive_hours: u32,
    unload_minutes: u32,
    external_links: String,
}

impl SyncedSettings {
    fn from(s: &Settings) -> Self {
        Self {
            auto_archive_hours: s.auto_archive_hours,
            unload_minutes: s.unload_minutes,
            external_links: s.external_links.clone(),
        }
    }
    fn apply(self, s: &mut Settings) {
        s.auto_archive_hours = self.auto_archive_hours;
        s.unload_minutes = self.unload_minutes;
        s.external_links = self.external_links;
    }
}

/// Work items for the sync thread.
pub enum Job {
    /// Start a new sync chain on `server` (a fresh sync code).
    Create {
        server: String,
        invite: Option<String>,
    },
    /// Join an existing chain with its 24 words.
    Join { server: String, words: String },
    /// Pull remote changes; write local data the server does not have yet.
    Sync {
        snapshot: Box<SidebarSnapshot>,
        settings: Box<Settings>,
        /// The sidebar is as this device last took it from sync (apart from
        /// what a device always keeps of its own): there is nothing to write,
        /// even where it differs from the server's.
        sidebar_unchanged: bool,
    },
    /// Stop syncing on this device; the chain stays on the server.
    Leave,
    /// Delete the chain and all its data from `server`, then leave.
    DeleteChain { server: String },
}

/// Results delivered on the UI thread.
pub enum Outcome {
    Connected {
        server: String,
        /// This device started the chain (it did not join one).
        created: bool,
    },
    /// Starting or joining a chain did not work.
    ConnectFailed(String),
    /// Remote had newer data; the app should replace its own and re-present.
    Pulled {
        sidebar: Option<SidebarSnapshot>,
        settings: Option<SyncedSettingsPublic>,
        versions: Vec<(String, u64)>,
    },
    /// Pushed successfully (or nothing to push); remember the new versions.
    Pushed {
        versions: Vec<(String, u64)>,
    },
    Error(String),
    Disconnected,
}

/// `SyncedSettings` re-exported without exposing the private type.
pub struct SyncedSettingsPublic(SyncedSettings);
impl SyncedSettingsPublic {
    /// Take another device's settings, checked like the Settings page's.
    pub fn apply(self, s: &mut Settings) {
        self.0.apply(s);
        *s = std::mem::take(s).sanitised();
    }
}

pub type Deliver = Arc<dyn Fn(Outcome) + Send + Sync>;
/// Takes outcomes on whichever thread has them (`Deliver` runs on the UI thread).
type Notify = Arc<dyn Fn(Outcome) + Send + Sync>;

/// What the sync thread is woken up for.
enum Msg {
    Create {
        server: String,
        invite: Option<String>,
    },
    Join {
        server: String,
        words: String,
    },
    /// Run the sync waiting in `Shared::pending`.
    Sync,
    /// Erase the chain on `server`; this device has left it already.
    DeleteRemote {
        server: String,
        code: SyncCode,
    },
}

/// A queued `Job::Sync`.
struct SyncRequest {
    snapshot: Box<SidebarSnapshot>,
    settings: Box<Settings>,
    sidebar_unchanged: bool,
}

/// Shared by the UI thread and the sync thread.
struct Shared {
    /// Bumped when this device leaves its chain. The worker hands out results
    /// only while it holds this lock and the epoch is the one its work started
    /// in, so nothing from the old chain arrives after `Disconnected`.
    epoch: Mutex<u64>,
    /// The newest sync request. Each carries the full state, so a newer one
    /// replaces a queued one and at most one waits, however long the server
    /// takes.
    pending: Mutex<Option<SyncRequest>>,
}

pub struct SyncClient {
    tx: Sender<Msg>,
    shared: Arc<Shared>,
    data_dir: PathBuf,
    notify: Notify,
}

impl SyncClient {
    /// Start the worker. `deliver` is called on the UI thread for each outcome.
    pub fn start(data_dir: PathBuf, sync_settings: SyncSettings, deliver: Deliver) -> Self {
        Self::spawn(
            data_dir,
            sync_settings,
            Arc::new(move |o| {
                let d = deliver.clone();
                Engine::post(Box::new(move || d(o)));
            }),
        )
    }

    fn spawn(data_dir: PathBuf, sync_settings: SyncSettings, notify: Notify) -> Self {
        let (tx, rx) = channel::<Msg>();
        let shared = Arc::new(Shared {
            epoch: Mutex::new(0),
            pending: Mutex::new(None),
        });
        let worker = Worker::new(data_dir.clone(), sync_settings);
        // A deletion the server never confirmed is tried again at start.
        if let Some((server, code)) = load_pending_delete(&data_dir) {
            tx.send(Msg::DeleteRemote { server, code }).ok();
        }
        let (s, n) = (shared.clone(), notify.clone());
        thread::Builder::new()
            .name("magnetite-sync".into())
            .spawn(move || run(worker, &rx, &s, &n))
            .expect("spawn sync thread");
        Self {
            tx,
            shared,
            data_dir,
            notify,
        }
    }

    /// Hand `job` to the sync thread. Leaving (and deleting the chain) takes
    /// effect here and now, without waiting for network work in progress;
    /// the server is told afterwards, best effort.
    pub fn submit(&self, job: Job) {
        let msg = match job {
            Job::Create { server, invite } => Msg::Create { server, invite },
            Job::Join { server, words } => Msg::Join { server, words },
            Job::Sync {
                snapshot,
                settings,
                sidebar_unchanged,
            } => {
                let request = SyncRequest {
                    snapshot,
                    settings,
                    sidebar_unchanged,
                };
                if lock(&self.shared.pending).replace(request).is_some() {
                    return; // the wake-up queued for the older one runs this
                }
                Msg::Sync
            }
            Job::Leave => {
                self.leave();
                return;
            }
            Job::DeleteChain { server } => {
                let code = load_code(&self.data_dir);
                // Kept until the server confirms, so a failed deletion can be
                // tried again even though the device leaves the chain now.
                if let Some(code) = &code
                    && let Err(e) = save_pending_delete(&self.data_dir, &server, code)
                {
                    (self.notify)(Outcome::Error(format!("cannot remember the deletion: {e}")));
                }
                self.leave();
                let Some(code) = code else { return };
                Msg::DeleteRemote { server, code }
            }
        };
        self.tx.send(msg).ok();
    }

    fn leave(&self) {
        let mut epoch = lock(&self.shared.epoch);
        *epoch += 1;
        lock(&self.shared.pending).take();
        forget_code(&self.data_dir);
        (self.notify)(Outcome::Disconnected);
    }
}

/// The sync thread: one message at a time, results to `notify`.
fn run(mut w: Worker, rx: &Receiver<Msg>, shared: &Shared, notify: &Notify) {
    let mut seen = 0;
    for msg in rx {
        let epoch = *lock(&shared.epoch);
        if epoch != seen {
            // The device left its chain (and may have joined another) since.
            w.use_code(load_code(&w.data_dir));
            seen = epoch;
        }
        let outcomes = match msg {
            Msg::Create { server, invite } => connect_outcomes(w.create(&server, invite)),
            Msg::Join { server, words } => connect_outcomes(w.join(&server, &words)),
            Msg::Sync => {
                // Not inside the `match`: its lock would last the whole sync.
                let request = lock(&shared.pending).take();
                request.map_or_else(Vec::new, |r| {
                    w.sync(&r.snapshot, &r.settings, r.sidebar_unchanged)
                })
            }
            Msg::DeleteRemote { server, code } => w.delete_remote(&server, &code),
        };
        let current = lock(&shared.epoch);
        if *current != epoch {
            // Left while this ran: its results no longer apply, and a code
            // it just adopted is forgotten again.
            forget_code(&w.data_dir);
            continue;
        }
        for o in outcomes {
            notify(o);
        }
    }
}

/// What a start or join ended with, told apart from the errors of a round.
fn connect_outcomes(outcomes: Vec<Outcome>) -> Vec<Outcome> {
    outcomes
        .into_iter()
        .map(|o| match o {
            Outcome::Error(e) => Outcome::ConnectFailed(e),
            o => o,
        })
        .collect()
}

struct Worker {
    data_dir: PathBuf,
    settings: SyncSettings,
    code: Option<SyncCode>,
    agent: ureq::Agent,
    /// Highest version per collection this device took from the server or
    /// had acknowledged, in this chain. A job may carry older versions (the
    /// UI thread applies outcomes later); the higher one counts.
    known: HashMap<String, u64>,
}

impl Worker {
    fn new(data_dir: PathBuf, settings: SyncSettings) -> Self {
        let agent = ureq::Agent::config_builder()
            // Status codes are handled here, not turned into errors by ureq.
            .http_status_as_error(false)
            // A redirect could lead to plain http or another host.
            .max_redirects(0)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build()
            .into();
        Self {
            code: load_code(&data_dir),
            data_dir,
            settings,
            agent,
            known: HashMap::new(),
        }
    }

    /// Switch to `code`'s chain (or none): versions start over.
    fn use_code(&mut self, code: Option<SyncCode>) {
        self.code = code;
        self.settings.versions.clear();
        self.known.clear();
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0))
}

/// A response: status and body bytes.
struct Reply {
    status: u16,
    body: Vec<u8>,
}

impl Reply {
    /// The body as `T`. A parse error would quote the body, so it is not
    /// shown: no text of the server's reaches Settings.
    fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, String> {
        serde_json::from_slice(&self.body)
            .map_err(|_| "the sync server sent an answer this version cannot read".to_string())
    }
    /// What went wrong, in our words: the server's own text is never shown,
    /// so a server cannot use Settings to ask for the sync code.
    fn error(&self) -> String {
        match self.status {
            400 => "the sync server did not understand the request".into(),
            401 => "the sync server did not accept this device's signature (is the clock right?)".into(),
            403 => "the sync server refused: it takes no new sync chains, the invite code is wrong, or the chain is full".into(),
            404 => "the sync server does not know this sync chain".into(),
            408 => "the request took too long to reach the sync server".into(),
            410 => "this sync chain was deleted a moment ago".into(),
            409 => "another device wrote first; trying again".into(),
            413 => "this sync chain has used up its storage on the server".into(),
            429 | 503 => "the sync server is busy; trying again later".into(),
            s if s >= 500 => "the sync server had an error; trying again later".into(),
            s => format!("the sync server answered {s}"),
        }
    }
}

impl Worker {
    /// Signed request to `path` below `/v2` on this device's server.
    fn request(
        &self,
        keys: &ChainKeys,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<Reply, String> {
        self.request_to(&self.settings.server_url, keys, method, path, body)
    }

    /// Signed request to `path` below `/v2` on `server`. The signature covers
    /// the server's host, so no other server can replay it.
    fn request_to(
        &self,
        server: &str,
        keys: &ChainKeys,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
    ) -> Result<Reply, String> {
        let base = server_base(server)?;
        let url = format!("{base}/v2{path}");
        let host = base
            .parse::<ureq::http::Uri>()
            .ok()
            .and_then(|u| u.authority().map(|a| a.as_str().to_string()))
            .unwrap_or_default();
        let bytes = body.unwrap_or_default();
        let builder = ureq::http::Request::builder()
            .method(method)
            .uri(&url)
            .header(
                "authorization",
                keys.authorization(method, &host, path, &bytes, now_ms()),
            )
            // The body's hash ahead of the body: the server checks the
            // signature before it reads an upload.
            .header(
                magnetite_sync_proto::CONTENT_DIGEST,
                magnetite_sync_proto::content_digest(&bytes),
            )
            .header("content-type", "application/json");
        let result = if bytes.is_empty() {
            builder
                .body(())
                .map_err(|e| e.to_string())
                .and_then(|r| self.agent.run(r).map_err(|e| e.to_string()))
        } else {
            builder
                .body(bytes)
                .map_err(|e| e.to_string())
                .and_then(|r| self.agent.run(r).map_err(|e| e.to_string()))
        };
        let mut res = result.map_err(|e| format!("cannot reach the sync server: {e}"))?;
        let status = res.status().as_u16();
        // ureq's own limit counts the bytes on the wire; a compressed answer
        // can unpack to far more. Count what comes out of the decoder.
        let mut body = Vec::new();
        res.body_mut()
            .with_config()
            .limit(MAX_REPLY)
            .reader()
            .take(MAX_REPLY + 1)
            .read_to_end(&mut body)
            .map_err(|e| e.to_string())?;
        if body.len() as u64 > MAX_REPLY {
            return Err("the sync server sent more data than a collection can hold".into());
        }
        Ok(Reply { status, body })
    }

    fn adopt(&mut self, server: &str, code: SyncCode, created: bool) -> Vec<Outcome> {
        // A deletion of this very chain that the server never confirmed is
        // off: it would erase the chain the device is in again.
        let this = code.to_base64();
        if load_pending_delete(&self.data_dir).is_some_and(|(_, c)| c.to_base64() == this) {
            std::fs::remove_file(pending_delete_path(&self.data_dir)).ok();
        }
        if let Err(e) = save_code(&self.data_dir, &code) {
            return vec![Outcome::Error(format!("cannot store the sync code: {e}"))];
        }
        self.use_code(Some(code));
        self.settings.started_here = created;
        vec![Outcome::Connected {
            server: server.to_string(),
            created,
        }]
    }

    /// A second chain over the first would replace its sync code, and the
    /// first chain could then neither be used nor deleted.
    fn in_a_chain(&self) -> Option<Vec<Outcome>> {
        self.code.as_ref().map(|_| {
            vec![Outcome::Error(
                "this device is in a sync chain already; leave it first".into(),
            )]
        })
    }

    fn create(&mut self, server: &str, invite: Option<String>) -> Vec<Outcome> {
        if let Some(refused) = self.in_a_chain() {
            return refused;
        }
        self.settings.server_url = server.trim().to_string();
        let code = SyncCode::generate();
        let body = serde_json::to_vec(&CreateChainRequest {
            invite: invite.filter(|i| !i.trim().is_empty()),
        })
        .unwrap_or_default();
        match self.request(&code.keys(), "POST", "/chain", Some(body)) {
            Ok(r) if r.status == 201 || r.status == 204 => self.adopt(server.trim(), code, true),
            Ok(r) => vec![Outcome::Error(r.error())],
            Err(e) => vec![Outcome::Error(e)],
        }
    }

    fn join(&mut self, server: &str, words: &str) -> Vec<Outcome> {
        if let Some(refused) = self.in_a_chain() {
            return refused;
        }
        self.settings.server_url = server.trim().to_string();
        let code = match SyncCode::from_words(words) {
            Ok(c) => c,
            Err(e) => return vec![Outcome::Error(e.to_string())],
        };
        match self.request(&code.keys(), "GET", "/chain", None) {
            Ok(r) if r.status == 204 => self.adopt(server.trim(), code, false),
            // The server tells only the key holder that a chain does not exist.
            Ok(r) if r.status == 404 => vec![Outcome::Error(format!(
                "No sync chain with this code on {}",
                self.settings.server_url
            ))],
            Ok(r) => vec![Outcome::Error(r.error())],
            Err(e) => vec![Outcome::Error(e)],
        }
    }

    fn sync(
        &mut self,
        snapshot: &SidebarSnapshot,
        settings: &Settings,
        sidebar_unchanged: bool,
    ) -> Vec<Outcome> {
        let Some(code) = self.code.clone() else {
            return vec![Outcome::Error("this device is not in a sync chain".into())];
        };
        let keys = code.keys();
        self.settings = settings.sync.clone();
        // What went wrong per collection, reported after what went through.
        let mut errors: Vec<String> = Vec::new();
        let mut pulled_sidebar = None;
        let mut pulled_settings = None;
        let mut versions = Vec::new();
        let plain: [(&str, Vec<u8>); 2] = [
            (SIDEBAR, serde_json::to_vec(snapshot).unwrap_or_default()),
            (
                SETTINGS,
                serde_json::to_vec(&SyncedSettings::from(settings)).unwrap_or_default(),
            ),
        ];
        for (name, bytes) in &plain {
            let id = keys.collection_id(name);
            let path = format!("/collections/{id}");
            // The version this job's snapshot is built on, and the newest this
            // device has seen (a pull the UI thread may not have applied yet).
            let applied = self.settings.versions.get(*name).copied().unwrap_or(0);
            let seen = applied.max(self.known.get(*name).copied().unwrap_or(0));
            // Whether the server holds exactly what this device would write.
            let mut same = false;
            // Pull first: if the server is ahead, take it.
            match self.request(&keys, "GET", &path, None) {
                Ok(r) if r.status == 200 => {
                    let remote: CollectionResponse = match r.json() {
                        Ok(c) => c,
                        Err(e) => {
                            errors.push(e);
                            continue;
                        }
                    };
                    if remote.version < seen {
                        errors.push(format!(
                            "{name}: the server has version {} but this device has {seen}; not going back",
                            remote.version
                        ));
                        continue;
                    }
                    if remote.version > seen {
                        let read = keys
                            .open(&id, remote.version, &remote.sealed())
                            .map_err(|e| e.to_string())
                            .and_then(|data| {
                                // Data this version cannot read (a newer Magnetite
                                // wrote it) is not taken: counting it as applied
                                // would let this device overwrite it later.
                                let unreadable = "cannot read it; is Magnetite up to date?";
                                if *name == SIDEBAR {
                                    // One without spaces cannot be shown. Not
                                    // counted as seen: every round says so
                                    // until a usable one is there.
                                    match serde_json::from_slice::<SidebarSnapshot>(&data) {
                                        Ok(s) if s.spaces.is_empty() => {
                                            Err("it has no spaces".to_string())
                                        }
                                        Ok(s) => {
                                            pulled_sidebar = Some(s);
                                            Ok(())
                                        }
                                        Err(_) => Err(unreadable.to_string()),
                                    }
                                } else {
                                    serde_json::from_slice::<SyncedSettings>(&data)
                                        .map(|s| pulled_settings = Some(SyncedSettingsPublic(s)))
                                        .map_err(|_| unreadable.to_string())
                                }
                            });
                        match read {
                            Ok(()) => {
                                self.known.insert((*name).to_string(), remote.version);
                                versions.push(((*name).to_string(), remote.version));
                            }
                            Err(e) => {
                                errors.push(format!("{name} version {}: {e}", remote.version));
                            }
                        }
                        continue;
                    }
                    // Compared by content, not by a count of local changes:
                    // the app's state changes all the time (a page's title,
                    // a download) without the synced data changing, and a
                    // device that writes the same data again makes every
                    // other device take it over its own newer changes.
                    same = keys
                        .open(&id, remote.version, &remote.sealed())
                        .is_ok_and(|data| data == *bytes);
                }
                // Nothing there yet. Only the device that started the chain
                // writes first: a device that joined before that one's first
                // write went through would make its own sidebar the chain's.
                Ok(r) if r.status == 404 => {
                    if applied == 0 && !self.settings.started_here {
                        errors.push(format!(
                            "{name}: this sync chain holds no data yet; sync the device that started it first"
                        ));
                        continue;
                    }
                }
                Ok(r) => {
                    errors.push(r.error());
                    continue;
                }
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            }
            if seen > applied {
                // This snapshot predates a pull the UI thread is applying; it
                // queues a fresh one then. Writing this one would undo the pull.
                continue;
            }
            // A sidebar without spaces is not written: no device would take
            // it. A device has one when it keeps all its spaces to itself.
            let nothing_to_send = *name == SIDEBAR && snapshot.spaces.is_empty();
            if same || nothing_to_send || (*name == SIDEBAR && sidebar_unchanged) {
                continue;
            }
            match self.push(&keys, name, applied, bytes) {
                Ok(version) => {
                    self.known.insert((*name).to_string(), version);
                    versions.push(((*name).to_string(), version));
                }
                Err(e) => errors.push(e),
            }
        }
        let mut outcomes = Vec::new();
        if pulled_sidebar.is_some() || pulled_settings.is_some() {
            outcomes.push(Outcome::Pulled {
                sidebar: pulled_sidebar,
                settings: pulled_settings,
                versions,
            });
        } else if errors.is_empty() || !versions.is_empty() {
            outcomes.push(Outcome::Pushed { versions });
        }
        // Last: a collection that went through must not hide the other's
        // failure.
        outcomes.extend(errors.into_iter().map(Outcome::Error));
        outcomes
    }

    /// Write collection `name` as the version after `base`. Returns that
    /// version once the server acknowledged exactly it.
    fn push(&self, keys: &ChainKeys, name: &str, base: u64, bytes: &[u8]) -> Result<u64, String> {
        let id = keys.collection_id(name);
        let next = base
            .checked_add(1)
            .ok_or_else(|| format!("{name}: version overflow"))?;
        let sealed = keys.seal(&id, next, bytes).map_err(|e| e.to_string())?;
        let body = serde_json::to_vec(&PutCollectionRequest::new(base, sealed)).unwrap_or_default();
        let r = self.request(keys, "PUT", &format!("/collections/{id}"), Some(body))?;
        match r.status {
            200 => match r.json::<PutCollectionResponse>()?.version {
                v if v == next => Ok(next),
                v => Err(format!(
                    "{name}: the server acknowledged version {v} for a write of version {next}; ignored"
                )),
            },
            409 => Err(format!("{name}: another device wrote first; will retry")),
            _ => Err(r.error()),
        }
    }

    /// Erase `code`'s chain on `server`. This device left it already, so only
    /// a failure is reported; the pending record stays for another try.
    fn delete_remote(&self, server: &str, code: &SyncCode) -> Vec<Outcome> {
        let failed = |e: String| {
            vec![Outcome::Error(format!(
                "Left the sync chain, but the server has not deleted it yet ({e}). Magnetite tries again at its next start."
            ))]
        };
        // 404: the chain is gone already. Anything else, also a refused
        // signature (401, for example after a wrong clock), is no proof.
        match self.request_to(server, &code.keys(), "DELETE", "/chain", None) {
            Ok(r) if r.status == 204 || r.status == 404 => {
                std::fs::remove_file(pending_delete_path(&self.data_dir)).ok();
                Vec::new()
            }
            Ok(r) => failed(r.error()),
            Err(e) => failed(e),
        }
    }
}

/// A chain deletion the server has not confirmed yet waits to be tried again.
pub fn deletion_pending(data_dir: &Path) -> bool {
    pending_delete_path(data_dir).exists()
}

pub fn signed_in(data_dir: &Path) -> bool {
    load_code(data_dir).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnetite_core::{Profile, SidebarSnapshot, Space};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    fn snapshot(title: &str) -> SidebarSnapshot {
        let mut s = SidebarSnapshot::new();
        s.profiles.push(Profile {
            id: "default".into(),
            name: "Default".into(),
            is_default: true,
            ephemeral: false,
        });
        s.spaces.push(Space {
            id: "s1".into(),
            title: title.into(),
            icon: None,
            accent: None,
            profile_id: "default".into(),
            pinned: vec![],
            today: vec![],
            bookmarks: Vec::new(),
        });
        s
    }

    fn settings_with(sync: &SyncSettings) -> Settings {
        Settings {
            sync: sync.clone(),
            ..Settings::default()
        }
    }

    fn describe(o: &Outcome) -> String {
        match o {
            Outcome::Connected { .. } => "Connected".into(),
            Outcome::ConnectFailed(e) => format!("ConnectFailed({e})"),
            Outcome::Pulled { .. } => "Pulled".into(),
            Outcome::Pushed { .. } => "Pushed".into(),
            Outcome::Error(e) => format!("Error({e})"),
            Outcome::Disconnected => "Disconnected".into(),
        }
    }

    /// A fresh data directory for one test.
    fn data_dir(name: &str) -> PathBuf {
        let dir = crate::files::test_dir(&format!("sync-{name}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A misbehaving server: `answer(method, path)` gives status and JSON
    /// body for every request. Returns its `http://127.0.0.1:<port>` address.
    fn fake_server(answer: impl Fn(&str, &str) -> (u16, String) + Send + 'static) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut words = line.split_whitespace();
                let (method, path) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
                let mut length = 0;
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header.trim().is_empty() {
                        break;
                    }
                    if let Some(v) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                }
                reader.read_exact(&mut vec![0; length]).unwrap();
                let (status, body) = answer(method, path);
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        url
    }

    /// A worker in a stored sync chain, talking to `server`.
    fn worker_in_chain(name: &str, server: &str) -> (Worker, SyncCode) {
        let dir = data_dir(name);
        let code = SyncCode::generate();
        save_code(&dir, &code).unwrap();
        let settings = SyncSettings {
            server_url: server.into(),
            // The device that started the chain: it may write first.
            started_here: true,
            ..SyncSettings::default()
        };
        (Worker::new(dir, settings), code)
    }

    fn sync_settings(versions: u64) -> Settings {
        let mut s = Settings::default();
        s.sync.versions = [
            (SIDEBAR.to_string(), versions),
            (SETTINGS.to_string(), versions),
        ]
        .into();
        s
    }

    #[test]
    fn writes_count_only_when_acknowledged_at_the_version_written() {
        let ack = Arc::new(AtomicU64::new(0));
        let answer = ack.clone();
        let server = fake_server(move |method, _| match method {
            "GET" => (404, r#"{"error":"no such collection"}"#.into()),
            _ => (
                200,
                format!(r#"{{"version":{}}}"#, answer.load(Ordering::SeqCst)),
            ),
        });
        let (mut w, _) = worker_in_chain("ack", &server);
        let settings = settings_with(&w.settings);
        // Writes go out as version 1; lower, higher or absurd answers are refused.
        for wrong in [0, 2, u64::MAX] {
            ack.store(wrong, Ordering::SeqCst);
            let out = w.sync(&snapshot("Work"), &settings, false);
            assert!(
                out.iter()
                    .all(|o| matches!(o, Outcome::Error(e) if e.contains("acknowledged"))),
                "ack {wrong}: {:?}",
                out.iter().map(describe).collect::<Vec<_>>()
            );
        }
        ack.store(1, Ordering::SeqCst);
        let out = w.sync(&snapshot("Work"), &settings, false);
        let [Outcome::Pushed { versions }] = out.as_slice() else {
            panic!("{:?}", out.iter().map(describe).collect::<Vec<_>>())
        };
        assert_eq!(versions, &[(SIDEBAR.into(), 1), (SETTINGS.into(), 1)]);
    }

    #[test]
    fn older_server_versions_never_replace_newer_local_data() {
        // The server answers every GET with an authentic ciphertext of the
        // version in `serving`; PUTs are counted.
        let serving = Arc::new(AtomicU64::new(3));
        let puts = Arc::new(AtomicUsize::new(0));
        let (worker_code, server) = {
            let code = SyncCode::generate();
            let keys = code.keys();
            let (serving, puts) = (serving.clone(), puts.clone());
            let ids = [
                (keys.collection_id(SIDEBAR), true),
                (keys.collection_id(SETTINGS), false),
            ];
            let server = fake_server(move |method, path| {
                if method == "PUT" {
                    puts.fetch_add(1, Ordering::SeqCst);
                    return (409, r#"{"error":"version conflict"}"#.into());
                }
                let version = serving.load(Ordering::SeqCst);
                let (id, sidebar) = ids
                    .iter()
                    .find(|(id, _)| path.ends_with(id.as_str()))
                    .unwrap();
                let plain = if *sidebar {
                    serde_json::to_vec(&snapshot(&format!("v{version}"))).unwrap()
                } else {
                    serde_json::to_vec(&SyncedSettings::from(&Settings::default())).unwrap()
                };
                let sealed = keys.seal(id, version, &plain).unwrap();
                let body = CollectionResponse::new(version, sealed);
                (200, serde_json::to_string(&body).unwrap())
            });
            (code, server)
        };
        let dir = data_dir("rollback");
        save_code(&dir, &worker_code).unwrap();
        let mut w = Worker::new(dir, SyncSettings::default());
        let at = |versions: u64| {
            let mut s = sync_settings(versions);
            s.sync.server_url.clone_from(&server);
            s
        };
        let pulled_title = |out: &[Outcome]| match out {
            [
                Outcome::Pulled {
                    sidebar: Some(sb), ..
                },
            ] => Some(sb.spaces[0].title.clone()),
            _ => None,
        };

        // This device is at 5; the server offers 3: refused, nothing written.
        let out = w.sync(&snapshot("Local"), &at(5), false);
        assert!(
            out.iter()
                .all(|o| matches!(o, Outcome::Error(e) if e.contains("not going back"))),
            "{:?}",
            out.iter().map(describe).collect::<Vec<_>>()
        );
        assert_eq!(puts.load(Ordering::SeqCst), 0);

        // Version 6 is newer and taken.
        serving.store(6, Ordering::SeqCst);
        assert_eq!(
            pulled_title(&w.sync(&snapshot("Local"), &at(5), false)).as_deref(),
            Some("v6")
        );

        // A job built before that pull was applied (version 5) must not be
        // written over it as version 7: nothing is written.
        let out = w.sync(&snapshot("Stale"), &at(5), false);
        assert!(
            out.iter()
                .all(|o| matches!(o, Outcome::Pushed { versions } if versions.is_empty()))
        );
        assert_eq!(puts.load(Ordering::SeqCst), 0);

        // A job still carrying version 2 (the UI thread has not applied the
        // pull yet) must not take version 4: this device has seen 6.
        serving.store(4, Ordering::SeqCst);
        let out = w.sync(&snapshot("Local"), &at(2), false);
        assert_eq!(pulled_title(&out), None);
        assert!(
            out.iter()
                .any(|o| matches!(o, Outcome::Error(e) if e.contains("has 6")))
        );
    }

    #[test]
    fn what_the_server_already_has_is_not_written_again() {
        // The server holds version 3 of both collections: the sidebar "Same"
        // and the default settings. PUTs are counted and acknowledged.
        let puts = Arc::new(AtomicUsize::new(0));
        let code = SyncCode::generate();
        let server = {
            let keys = code.keys();
            let puts = puts.clone();
            let ids = [
                (keys.collection_id(SIDEBAR), true),
                (keys.collection_id(SETTINGS), false),
            ];
            fake_server(move |method, path| {
                if method == "PUT" {
                    puts.fetch_add(1, Ordering::SeqCst);
                    return (200, r#"{"version":4}"#.into());
                }
                let (id, sidebar) = ids
                    .iter()
                    .find(|(id, _)| path.ends_with(id.as_str()))
                    .unwrap();
                let plain = if *sidebar {
                    serde_json::to_vec(&snapshot("Same")).unwrap()
                } else {
                    serde_json::to_vec(&SyncedSettings::from(&Settings::default())).unwrap()
                };
                let body = CollectionResponse::new(3, keys.seal(id, 3, &plain).unwrap());
                (200, serde_json::to_string(&body).unwrap())
            })
        };
        let dir = data_dir("unchanged");
        save_code(&dir, &code).unwrap();
        let mut w = Worker::new(dir, SyncSettings::default());
        let mut settings = sync_settings(3);
        settings.sync.server_url = server;

        // Round after round with the same data: nothing is written.
        for _ in 0..3 {
            let out = w.sync(&snapshot("Same"), &settings, false);
            assert!(
                matches!(out.as_slice(), [Outcome::Pushed { versions }] if versions.is_empty()),
                "{:?}",
                out.iter().map(describe).collect::<Vec<_>>()
            );
        }
        assert_eq!(puts.load(Ordering::SeqCst), 0);

        // A sidebar that differs only by what this device keeps of its own
        // since it took the chain's (`sidebar_unchanged`) is not written
        // either: the other device would take it and answer in kind.
        let out = w.sync(&snapshot("Kept here"), &settings, true);
        assert!(matches!(out.as_slice(), [Outcome::Pushed { versions }] if versions.is_empty()));
        assert_eq!(puts.load(Ordering::SeqCst), 0);

        // Nor is a sidebar without spaces (all of them kept on this device):
        // no device would take it.
        let mut kept_here = snapshot("Kept here");
        kept_here.spaces.clear();
        let out = w.sync(&kept_here, &settings, false);
        assert!(matches!(out.as_slice(), [Outcome::Pushed { versions }] if versions.is_empty()));
        assert_eq!(puts.load(Ordering::SeqCst), 0);

        // A changed sidebar is written; the unchanged settings are not.
        let out = w.sync(&snapshot("Renamed"), &settings, false);
        let [Outcome::Pushed { versions }] = out.as_slice() else {
            panic!("{:?}", out.iter().map(describe).collect::<Vec<_>>())
        };
        assert_eq!(versions, &[(SIDEBAR.into(), 4)]);
        assert_eq!(puts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_device_that_joined_does_not_write_into_an_empty_chain() {
        let puts = Arc::new(AtomicUsize::new(0));
        let counted = puts.clone();
        let server = fake_server(move |method, _| {
            if method == "PUT" {
                counted.fetch_add(1, Ordering::SeqCst);
                return (200, r#"{"version":1}"#.into());
            }
            (404, r#"{"error":"no such collection"}"#.into())
        });
        let (mut w, _) = worker_in_chain("joined", &server);
        // It joined; the device that started the chain has not written yet.
        w.settings.started_here = false;
        let out = w.sync(&snapshot("Mine"), &settings_with(&w.settings), false);
        assert!(
            out.iter()
                .all(|o| matches!(o, Outcome::Error(e) if e.contains("holds no data yet"))),
            "{:?}",
            out.iter().map(describe).collect::<Vec<_>>()
        );
        assert_eq!(puts.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_sidebar_without_spaces_is_refused_on_every_round() {
        let code = SyncCode::generate();
        let server = {
            let keys = code.keys();
            let ids = [
                (keys.collection_id(SIDEBAR), true),
                (keys.collection_id(SETTINGS), false),
            ];
            fake_server(move |_, path| {
                let (id, sidebar) = ids
                    .iter()
                    .find(|(id, _)| path.ends_with(id.as_str()))
                    .unwrap();
                let plain = if *sidebar {
                    serde_json::to_vec(&SidebarSnapshot::new()).unwrap()
                } else {
                    serde_json::to_vec(&SyncedSettings::from(&Settings::default())).unwrap()
                };
                let body = CollectionResponse::new(3, keys.seal(id, 3, &plain).unwrap());
                (200, serde_json::to_string(&body).unwrap())
            })
        };
        let dir = data_dir("no-spaces");
        save_code(&dir, &code).unwrap();
        let mut w = Worker::new(dir, SyncSettings::default());
        let mut settings = sync_settings(2);
        settings.sync.server_url = server;
        for _ in 0..2 {
            let out = w.sync(&snapshot("Mine"), &settings, false);
            assert!(
                !out.iter().any(|o| matches!(
                    o,
                    Outcome::Pulled {
                        sidebar: Some(_),
                        ..
                    }
                )),
                "the sidebar is not taken"
            );
            // The reason comes last, after what went through, so it stays.
            assert!(
                matches!(out.last(), Some(Outcome::Error(e)) if e.contains("no spaces")),
                "{:?}",
                out.iter().map(describe).collect::<Vec<_>>()
            );
        }
        assert!(!w.known.contains_key(SIDEBAR), "and not counted as seen");
    }

    #[test]
    fn a_second_chain_is_not_started_over_the_first() {
        let server = fake_server(|_, _| (201, String::new()));
        let (mut w, code) = worker_in_chain("second", &server);
        let out = w.create(&server, None);
        assert!(
            matches!(out.as_slice(), [Outcome::Error(e)] if e.contains("already")),
            "{:?}",
            out.iter().map(describe).collect::<Vec<_>>()
        );
        let kept = load_code(&w.data_dir).expect("the code is still there");
        assert_eq!(kept.to_base64(), code.to_base64());
    }

    #[test]
    fn joining_a_chain_again_calls_off_its_pending_deletion() {
        let server = fake_server(|_, _| (204, String::new()));
        let dir = data_dir("rejoin");
        let code = SyncCode::generate();
        save_pending_delete(&dir, &server, &code).unwrap();
        let mut w = Worker::new(dir.clone(), SyncSettings::default());
        let out = w.join(&server, &code.words());
        assert!(
            matches!(out.as_slice(), [Outcome::Connected { created: false, .. }]),
            "{:?}",
            out.iter().map(describe).collect::<Vec<_>>()
        );
        assert!(load_pending_delete(&dir).is_none());
    }

    #[test]
    fn data_this_version_cannot_read_is_not_taken() {
        let code = SyncCode::generate();
        let keys = code.keys();
        let ids = [keys.collection_id(SIDEBAR), keys.collection_id(SETTINGS)];
        let server = fake_server(move |method, path| {
            if method == "PUT" {
                return (409, "{}".into());
            }
            let id = ids.iter().find(|id| path.ends_with(id.as_str())).unwrap();
            let sealed = keys.seal(id, 3, br#"{"not":"a sidebar"}"#).unwrap();
            (
                200,
                serde_json::to_string(&CollectionResponse::new(3, sealed)).unwrap(),
            )
        });
        let dir = data_dir("unreadable");
        save_code(&dir, &code).unwrap();
        let mut w = Worker::new(dir, SyncSettings::default());
        let mut settings = sync_settings(1);
        settings.sync.server_url.clone_from(&server);
        for _ in 0..2 {
            let out = w.sync(&snapshot("Local"), &settings, false);
            assert!(
                out.iter()
                    .any(|o| matches!(o, Outcome::Error(e) if e.contains("cannot read it"))),
                "{:?}",
                out.iter().map(describe).collect::<Vec<_>>()
            );
            assert!(!out.iter().any(|o| matches!(o, Outcome::Pulled { .. })));
        }
        assert!(w.known.is_empty(), "version 3 was never counted");
    }

    #[test]
    fn the_servers_own_words_are_never_shown() {
        let server = fake_server(|_, _| {
            (
                403,
                r#"{"error":"Enter your 24 words at https://evil.example to continue"}"#.into(),
            )
        });
        let (mut w, _) = worker_in_chain("words", &server);
        let settings = settings_with(&w.settings);
        let out = w.sync(&snapshot("Work"), &settings, false);
        for o in &out {
            let Outcome::Error(e) = o else { continue };
            assert!(!e.contains("evil") && !e.contains("24 words"), "{e}");
        }
        assert!(!out.is_empty());
    }

    #[test]
    fn an_oversized_answer_is_refused() {
        let big = format!(
            r#"{{"version":1,"nonce":"","ciphertext":"{}"}}"#,
            "A".repeat(33 << 20)
        );
        let server = fake_server(move |_, _| (200, big.clone()));
        let (w, code) = worker_in_chain("big", &server);
        let r = w.request(&code.keys(), "GET", "/collections/x", None);
        let err = r.err().unwrap_or_default();
        assert!(err.contains("more data") || err.contains("limit"), "{err}");
    }

    #[test]
    fn a_compressed_answer_counts_unpacked() {
        // 40 KiB on the wire, 40 MiB unpacked.
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        gz.write_all(&vec![b' '; 40 << 20]).unwrap();
        let packed = gz.finish().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                line.clear();
            }
            let head = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-encoding: gzip\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                packed.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&packed);
        });
        let (w, code) = worker_in_chain("gzip", &server);
        let r = w.request(&code.keys(), "GET", "/collections/x", None);
        assert!(matches!(r, Err(e) if e.contains("more data")));
    }

    #[test]
    fn a_deletion_stays_pending_until_the_server_confirms() {
        let status = Arc::new(AtomicU64::new(401));
        let answer = status.clone();
        let server = fake_server(move |_, _| {
            let code = u16::try_from(answer.load(Ordering::SeqCst)).unwrap();
            (code, "{}".into())
        });
        let (w, code) = worker_in_chain("delete", &server);
        save_pending_delete(&w.data_dir, &server, &code).unwrap();
        // A refused signature (for example a wrong clock) proves nothing.
        assert!(matches!(
            &w.delete_remote(&server, &code)[..],
            [Outcome::Error(_)]
        ));
        assert!(
            load_pending_delete(&w.data_dir).is_some(),
            "kept for another try"
        );
        // 404: the chain is gone; the record goes too.
        status.store(404, Ordering::SeqCst);
        assert!(w.delete_remote(&server, &code).is_empty());
        assert!(load_pending_delete(&w.data_dir).is_none());
    }

    #[test]
    fn server_urls_need_https_except_on_this_computer() {
        for ok in [
            "https://sync.example.org",
            "https://sync.example.org/magnetite/",
            "http://localhost:8787",
            "http://127.0.0.1:8787",
            "http://[::1]:8787",
        ] {
            assert_eq!(
                server_base(ok).as_deref(),
                Ok(ok.trim_end_matches('/')),
                "{ok}"
            );
        }
        for bad in [
            "http://sync.example.org",
            "http://localhost.example.org",
            "http://127.0.0.1.nip.io",
            "http://127.0.0.1@sync.example.org",
            "https://user:secret@sync.example.org",
            "https://sync.example.org/?redirect=x",
            "ftp://sync.example.org",
            "sync.example.org",
            "",
        ] {
            assert!(server_base(bad).is_err(), "{bad}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_sync_key_is_only_ever_readable_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let dir = data_dir("key-mode");
        // Even over an older, world-readable file.
        std::fs::write(secrets_path(&dir), "{}").unwrap();
        std::fs::set_permissions(secrets_path(&dir), std::fs::Permissions::from_mode(0o644))
            .unwrap();
        let code = SyncCode::generate();
        save_code(&dir, &code).unwrap();
        let mode = std::fs::metadata(secrets_path(&dir))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(load_code(&dir), Some(code));
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            1,
            "no temporary file left"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn leaving_does_not_wait_for_a_stalled_server() {
        // Takes the connection but never answers.
        let stalled = TcpListener::bind("127.0.0.1:0").unwrap();
        let server = format!("http://{}", stalled.local_addr().unwrap());
        let (w, _) = worker_in_chain("stalled", &server);
        let dir = w.data_dir.clone();
        let (tx, rx) = channel();
        let client = SyncClient::spawn(
            dir.clone(),
            w.settings.clone(),
            Arc::new(move |o| {
                tx.send(describe(&o)).ok();
            }),
        );
        let sync = || Job::Sync {
            snapshot: Box::new(snapshot("Work")),
            settings: Box::new(settings_with(&w.settings)),
            sidebar_unchanged: false,
        };
        client.submit(sync());
        let _connection = stalled.accept().unwrap(); // the worker is now stuck

        let started = std::time::Instant::now();
        for _ in 1..20 {
            client.submit(sync());
        }
        client.submit(Job::Leave);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(rx.try_recv().as_deref(), Ok("Disconnected"));
        assert!(!signed_in(&dir));
        assert!(
            lock(&client.shared.pending).is_none(),
            "queued sync dropped"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The server keeps its data in Postgres: this test needs one in
    /// `DATABASE_URL`, as magnetite-sync's own tests do (CI starts one on
    /// Linux and runs it with `--ignored`).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "needs a Postgres in DATABASE_URL"]
    async fn two_devices_share_a_chain_through_a_real_server() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL names a Postgres");
        let state = magnetite_sync::AppState {
            db: magnetite_sync::Db::connect(&url).await.unwrap(),
            registration: magnetite_sync::Registration::Open,
            retention_days: None,
            // The database may hold chains of earlier runs.
            limits: magnetite_sync::Limits {
                max_chains: u32::MAX,
                max_new_chains_per_day: 0,
                ..magnetite_sync::Limits::default()
            },
        };
        let (addr, _handle) = magnetite_sync::serve_ephemeral(state).await.unwrap();
        let server = format!("http://{addr}");
        let (d1, d2, d3) = (data_dir("a"), data_dir("b"), data_dir("c"));

        let result = tokio::task::spawn_blocking(move || {
            // Device A starts a chain and pushes.
            let mut a = Worker::new(d1.clone(), SyncSettings::default());
            let out = a.create(&server, None);
            assert!(
                matches!(out[0], Outcome::Connected { .. }),
                "create: {}",
                describe(&out[0])
            );
            let words = code_words(&d1).expect("code stored");
            let out = a.sync(&snapshot("Work"), &settings_with(&a.settings), false);
            let Outcome::Pushed { versions } = &out[0] else {
                panic!("expected push, got {}", describe(&out[0]))
            };
            assert_eq!(versions.len(), 2);
            a.settings.versions = versions.iter().cloned().collect();

            // Device B joins with the 24 words and pulls A's sidebar.
            let mut b = Worker::new(d2.clone(), SyncSettings::default());
            let out = b.join(&server, &words.to_uppercase());
            assert!(
                matches!(out[0], Outcome::Connected { .. }),
                "join: {}",
                describe(&out[0])
            );
            let out = b.sync(&snapshot("Local only"), &settings_with(&b.settings), false);
            let Outcome::Pulled {
                sidebar: Some(sb),
                versions,
                ..
            } = &out[0]
            else {
                panic!("expected pull, got {}", describe(&out[0]))
            };
            assert_eq!(sb.spaces[0].title, "Work");
            b.settings.versions = versions.iter().cloned().collect();

            // B changes and pushes; A pulls the change.
            let out = b.sync(
                &snapshot("Work renamed"),
                &settings_with(&b.settings),
                false,
            );
            assert!(
                matches!(out[0], Outcome::Pushed { .. }),
                "push from b: {}",
                describe(&out[0])
            );
            let out = a.sync(&snapshot("Work"), &settings_with(&a.settings), false);
            let Outcome::Pulled {
                sidebar: Some(sb), ..
            } = &out[0]
            else {
                panic!("expected pull on a, got {}", describe(&out[0]))
            };
            assert_eq!(sb.spaces[0].title, "Work renamed");

            // A code that is not on the server cannot join.
            let mut c = Worker::new(d3.clone(), SyncSettings::default());
            let out = c.join(&server, &SyncCode::generate().words());
            assert!(
                matches!(&out[0], Outcome::Error(e) if e.contains("No sync chain")),
                "{}",
                describe(&out[0])
            );

            // Deleting the chain removes it for everyone.
            assert!(
                a.delete_remote(&a.settings.server_url, &load_code(&d1).unwrap())
                    .is_empty()
            );
            let mut again = Worker::new(d2.clone(), SyncSettings::default());
            assert!(matches!(&again.join(&server, &words)[0], Outcome::Error(_)));
            for d in [d1, d2, d3] {
                let _ = std::fs::remove_dir_all(d);
            }
        })
        .await;
        result.unwrap();
    }
}
