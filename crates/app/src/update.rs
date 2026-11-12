//! Updates from the project's releases. Each release carries `latest.json`
//! (version, release notes, one package per platform with its SHA-256 and
//! size) and `latest.json.sig`, an Ed25519 signature of those exact bytes
//! made with the release key. Magnetite believes a manifest only with a
//! valid signature from `PUBLIC_KEY`, and a package only with the
//! manifest's hash and size. The release side lives in `scripts/release/`.
//!
//! What leaves the computer: per check two HTTPS requests to the release
//! host (at start and once a day while running, unless turned off in
//! Settings), with a generic user agent and nothing that identifies the
//! user beyond the connection itself.
//!
//! Installing: on macOS the app bundle is replaced in place when Magnetite
//! may write next to it and Gatekeeper accepts the new bundle (Developer ID
//! signed and notarized); then it restarts. Otherwise, and on Linux and
//! Windows, the release page opens for a manual download.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The release key's public half (base64 of the raw 32 bytes, printed by
/// `scripts/release/new-update-key.sh`). Empty: this build has no updates.
const PUBLIC_KEY: &str = "d51FsDQ59WN1P85cj0UOGVCZuhCEPSJtSyUlV7DcwZg=";

/// Where releases are published (the workspace `repository`); the manifest
/// is a fixed asset of the latest release.
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// Hours between automatic checks while Magnetite runs.
pub const CHECK_EVERY_SECS: i64 = 24 * 3600;
/// Seconds after start before the first automatic check.
pub const FIRST_CHECK_AFTER_SECS: i64 = 30;

/// The app being replaced, inside the staging folder next to it.
const OLD_APP: &str = "old.app";

/// Largest manifest or signature accepted.
const MAX_MANIFEST: u64 = 64 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub notes_url: String,
    pub assets: HashMap<String, Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// What the UI shows (Settings → About, the notification panel).
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Status {
    /// Not checked yet.
    #[default]
    Idle,
    /// This build has no release key or no repository: no updates.
    Unavailable,
    Checking,
    UpToDate {
        checked_at: i64,
    },
    Available {
        version: String,
        notes_url: String,
        /// Magnetite can replace itself (macOS, writable location).
        installable: bool,
    },
    Installing {
        version: String,
    },
    Failed {
        error: String,
    },
}

/// Where the feed and key come from. Development builds may point both
/// elsewhere (`MAGNETITE_UPDATE_FEED`, `MAGNETITE_UPDATE_KEY`) for tests.
pub struct Source {
    pub feed: String,
    pub key: VerifyingKey,
}

/// A development feed on this computer may use plain http.
fn local_feed(feed: &str) -> bool {
    cfg!(debug_assertions)
        && ["http://127.0.0.1:", "http://localhost:"]
            .iter()
            .any(|p| feed.starts_with(p))
}

impl Source {
    pub fn configured() -> Option<Self> {
        let env = |name: &str| std::env::var(name).ok().filter(|_| cfg!(debug_assertions));
        let key = env("MAGNETITE_UPDATE_KEY").unwrap_or_else(|| PUBLIC_KEY.to_string());
        let feed = env("MAGNETITE_UPDATE_FEED").or_else(|| {
            (!REPOSITORY.is_empty()).then(|| format!("{REPOSITORY}/releases/latest/download"))
        })?;
        let bytes: [u8; 32] = base64::engine::general_purpose::STANDARD
            .decode(key.trim())
            .ok()?
            .try_into()
            .ok()?;
        Some(Self {
            feed: feed.trim_end_matches('/').to_string(),
            key: VerifyingKey::from_bytes(&bytes).ok()?,
        })
    }
}

/// This platform's key in the manifest: `macos-aarch64`, `linux-x86_64`, …
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// The manifest in `body`, if `signature` (base64) signs it with `key`.
pub fn verify(body: &[u8], signature: &str, key: &VerifyingKey) -> Result<Manifest, String> {
    let sig: [u8; 64] = base64::engine::general_purpose::STANDARD
        .decode(signature.trim())
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or("The update signature is malformed.")?;
    key.verify(body, &Signature::from_bytes(&sig))
        .map_err(|_| "The update is not signed with Magnetite's release key.")?;
    let manifest: Manifest =
        serde_json::from_slice(body).map_err(|_| "The update description is malformed.")?;
    if !manifest.notes_url.starts_with("https://") {
        return Err("The update description is malformed.".into());
    }
    Ok(manifest)
}

/// Whether version `candidate` is newer than `current` by semantic versioning
/// (`1.2.3`, an optional leading `v`; `1.0.0-beta.10` is newer than
/// `1.0.0-beta.9`, a pre-release older than its release). A version that does
/// not parse is never newer: an update is not worth a guess.
pub fn newer(candidate: &str, current: &str) -> bool {
    let parse = |v: &str| semver::Version::parse(v.trim().trim_start_matches('v')).ok();
    match (parse(candidate), parse(current)) {
        (Some(a), Some(b)) => a.cmp_precedence(&b).is_gt(),
        _ => false,
    }
}

fn agent(local: bool) -> ureq::Agent {
    ureq::Agent::config_builder()
        // Releases redirect to their storage host; never to plain http.
        .https_only(!local)
        .max_redirects(5)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(Duration::from_secs(600)))
        .user_agent("Magnetite")
        .build()
        .into()
}

fn get(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut res = agent
        .get(url)
        .call()
        .map_err(|e| format!("Cannot reach the update server: {e}"))?;
    let mut body = Vec::new();
    res.body_mut()
        .as_reader()
        .take(limit + 1)
        .read_to_end(&mut body)
        .map_err(|e| format!("The download broke off: {e}"))?;
    if body.len() as u64 > limit {
        return Err("The update server sent more than expected.".into());
    }
    Ok(body)
}

/// Fetch and verify the latest release's manifest (blocking).
pub fn check(source: &Source) -> Result<Manifest, String> {
    let agent = agent(local_feed(&source.feed));
    let body = get(
        &agent,
        &format!("{}/latest.json", source.feed),
        MAX_MANIFEST,
    )?;
    let sig = get(
        &agent,
        &format!("{}/latest.json.sig", source.feed),
        MAX_MANIFEST,
    )?;
    verify(&body, &String::from_utf8_lossy(&sig), &source.key)
}

/// The app bundle Magnetite runs from, if it may replace it: macOS, inside
/// a `.app`, in a folder it can write to.
pub fn replaceable_bundle() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let bundle = exe.ancestors().nth(3)?.to_path_buf();
    let parent = bundle.parent()?;
    let writable = tempfile_in(parent).is_some();
    (bundle.extension()? == "app" && writable).then_some(bundle)
}

/// Probe that `dir` takes new files.
fn tempfile_in(dir: &Path) -> Option<()> {
    let probe = dir.join(format!(".magnetite-write-test-{}", std::process::id()));
    std::fs::write(&probe, b"").ok()?;
    std::fs::remove_file(&probe).ok()
}

/// Download, check and put the new bundle in place of `bundle` (blocking).
/// Returns the folder to delete after the restart (holds the old bundle).
pub fn install(asset: &Asset, bundle: &Path) -> Result<PathBuf, String> {
    let parent = bundle.parent().ok_or("No folder for the update.")?;
    // Next to the app, so the swap is a rename on the same volume.
    let staging = parent.join(format!(".magnetite-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir(&staging).map_err(|e| format!("Cannot prepare the update: {e}"))?;
    let result = stage_and_swap(asset, bundle, &staging);
    // Not when the old app is stranded in there (see `stage_and_swap`).
    if result.is_err() && !staging.join(OLD_APP).exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result.map(|()| staging)
}

fn stage_and_swap(asset: &Asset, bundle: &Path, staging: &Path) -> Result<(), String> {
    let data = get(&agent(local_feed(&asset.url)), &asset.url, asset.size)?;
    let hash = hex(&Sha256::digest(&data));
    if data.len() as u64 != asset.size || !hash.eq_ignore_ascii_case(asset.sha256.trim()) {
        return Err("The download does not match the signed update.".into());
    }
    let zip = staging.join("update.zip");
    std::fs::write(&zip, &data).map_err(|e| format!("Cannot save the update: {e}"))?;
    let unpacked = staging.join("new");
    run("/usr/bin/ditto", &["-x", "-k"], &[&zip, &unpacked])?;
    let new_app = std::fs::read_dir(&unpacked)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "app"))
        .ok_or("The update holds no app.")?;
    if bundle_id(&new_app)? != bundle_id(bundle)? {
        return Err("The update is a different app.".into());
    }
    // Signed by the team that signed this Magnetite: a valid signature of
    // any other developer does not do. Then Gatekeeper decides, as it would
    // for a download: notarized.
    let team = team_id(bundle).ok_or(
        "This Magnetite is not signed by a developer team. Download the update from the release page.",
    )?;
    let requirement =
        format!("-R=anchor apple generic and certificate leaf[subject.OU] = \"{team}\"");
    run(
        "/usr/bin/codesign",
        &["--verify", "--deep", "--strict", &requirement],
        &[&new_app],
    )
    .map_err(|_| "The update is not signed by Magnetite's developers.".to_string())?;
    run(
        "/usr/sbin/spctl",
        &["--assess", "--type", "execute"],
        &[&new_app],
    )
    .map_err(|_| {
        "macOS does not accept this update (not notarized). Download it from the release page."
            .to_string()
    })?;
    let old = staging.join(OLD_APP);
    std::fs::rename(bundle, &old).map_err(|e| format!("Cannot move the old app aside: {e}"))?;
    if let Err(e) = std::fs::rename(&new_app, bundle) {
        return Err(match std::fs::rename(&old, bundle) {
            Ok(()) => format!("Cannot put the new app in place: {e}"),
            // Neither app is in place now: say where the old one is.
            Err(_) => format!(
                "Cannot put the new app in place: {e}. Your Magnetite is now at {}; move it back by hand.",
                old.display()
            ),
        });
    }
    Ok(())
}

/// After Magnetite quits: start `bundle` again and delete `cleanup`, which
/// holds the old app. If the new one does not open, the old one stays.
pub fn relaunch_after_exit(bundle: &Path, cleanup: &Path) -> Result<(), String> {
    let script = r#"while /bin/kill -0 "$1" 2>/dev/null; do /bin/sleep 0.2; done; /usr/bin/open "$2" && /bin/rm -rf "$3""#;
    std::process::Command::new("/bin/sh")
        .args(["-c", script, "sh", &std::process::id().to_string()])
        .arg(bundle)
        .arg(cleanup)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("Cannot restart: {e}"))
}

/// The Apple developer team that signed `app` (`TeamIdentifier`, ten
/// letters and digits); `None` for ad-hoc or unsigned builds.
fn team_id(app: &Path) -> Option<String> {
    let out = std::process::Command::new("/usr/bin/codesign")
        .args(["-dv", "--verbose=2"])
        .arg(app)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .find_map(|l| l.strip_prefix("TeamIdentifier="))
        .filter(|t| {
            t.len() == 10
                && t.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        })
        .map(str::to_owned)
}

fn bundle_id(app: &Path) -> Result<String, String> {
    let plist = app.join("Contents/Info.plist");
    let out = std::process::Command::new("/usr/bin/plutil")
        .args(["-extract", "CFBundleIdentifier", "raw", "-o", "-"])
        .arg(&plist)
        .output()
        .map_err(|e| e.to_string())?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .ok_or_else(|| "The update's app has no identifier.".to_string())
}

fn run(program: &str, args: &[&str], paths: &[&Path]) -> Result<(), String> {
    let status = std::process::Command::new(program)
        .args(args)
        .args(paths)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| format!("{program} failed"))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn signed(body: &str) -> (VerifyingKey, String) {
        let key = SigningKey::from_bytes(&[7; 32]);
        let sig = key.sign(body.as_bytes());
        (
            key.verifying_key(),
            base64::engine::general_purpose::STANDARD.encode(sig.to_bytes()),
        )
    }

    const BODY: &str = r#"{"version":"0.2.0","notes_url":"https://example.org/r/v0.2.0","assets":{"macos-aarch64":{"url":"https://example.org/a.zip","sha256":"ab","size":3}}}"#;

    #[test]
    fn only_manifests_signed_by_the_release_key_count() {
        let (key, sig) = signed(BODY);
        let m = verify(BODY.as_bytes(), &sig, &key).unwrap();
        assert_eq!(m.version, "0.2.0");
        assert_eq!(m.assets["macos-aarch64"].size, 3);
        let tampered = BODY.replace("0.2.0", "9.9.9");
        assert!(verify(tampered.as_bytes(), &sig, &key).is_err());
        let other = SigningKey::from_bytes(&[8; 32]).verifying_key();
        assert!(verify(BODY.as_bytes(), &sig, &other).is_err());
        assert!(verify(BODY.as_bytes(), "not base64!", &key).is_err());
    }

    #[test]
    fn release_notes_must_be_https() {
        let body = BODY.replace("https://example.org/r", "javascript:alert(1)//");
        let (key, sig) = signed(&body);
        assert!(verify(body.as_bytes(), &sig, &key).is_err());
    }

    #[test]
    fn versions_compare_by_number_then_pre_release() {
        assert!(newer("0.2.0", "0.1.9"));
        assert!(newer("v1.10.0", "1.9.3"));
        assert!(newer("1.0.0", "1.0.0-beta.2"));
        assert!(newer("1.0.0-beta.2", "1.0.0-beta.1"));
        assert!(!newer("1.0.0", "1.0.0"));
        assert!(!newer("0.9.9", "1.0.0"));
        assert!(!newer("1.0.0-rc.1", "1.0.0"));
        // Numeric pre-release parts count as numbers: no going back to beta.9.
        assert!(newer("1.0.0-beta.10", "1.0.0-beta.9"));
        assert!(!newer("1.0.0-beta.9", "1.0.0-beta.10"));
        assert!(!newer("2", "1.0.0"), "not a version");
        assert!(!newer("1.0.1", "garbage"));
    }

    #[test]
    fn hashes_print_as_lowercase_hex() {
        assert_eq!(hex(&Sha256::digest(b"abc"))[..8], *"ba7816bf");
    }
}
