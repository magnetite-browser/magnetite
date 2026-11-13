//! Files with personal data: readable by the owner only from the moment they
//! exist, and written whole (a temp file renamed into place), so a crash or a
//! reader never sees half a file.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Write `bytes` to `path` with owner-only permissions (0600 on Unix).
/// Each write has a temp file of its own, so two threads writing the same
/// file at once leave one whole version of it. A failed write removes its
/// temp file; those of a process that crashed go with the next write.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let n = WRITES.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    remove_stale_temps(path, pid);
    let tmp = path.with_extension(format!("{pid}-{n}.tmp"));
    let written = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

/// Remove the temp files of `path` that another process (`pid`: this one)
/// left: `<stem>.<pid>-<n>.tmp` (`write_private`). This process's own may
/// be in use by another thread.
fn remove_stale_temps(path: &Path, pid: u32) {
    let (Some(dir), Some(stem)) = (path.parent(), path.file_stem().and_then(|s| s.to_str())) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let owner = (name.strip_prefix(stem))
            .and_then(|rest| rest.strip_prefix('.'))
            .and_then(|rest| rest.strip_suffix(".tmp"))
            .and_then(|rest| rest.split_once('-'))
            .filter(|(_, n)| n.bytes().all(|b| b.is_ascii_digit()) && !n.is_empty())
            .and_then(|(p, _)| p.parse::<u32>().ok());
        if owner.is_some_and(|owner| owner != pid) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Create `dir` (and its parents) if needed and make it the owner's only
/// (0700 on Unix): it holds the sidebar, tokens and Chromium's profiles.
pub fn private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// A folder of test `name`'s own, unique to this test run (the process),
/// in the system's temp folder: unit tests get no `CARGO_TARGET_TMPDIR`.
/// Not created; anything an earlier run left there is removed.
#[cfg(test)]
pub fn test_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("magnetite-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn private_files_and_dirs_are_the_owners_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = super::test_dir("files");
        super::private_dir(&dir).unwrap();
        let file = dir.join("secret.json");
        super::write_private(&file, b"{}").unwrap();
        super::write_private(&file, b"{\"a\":1}").unwrap();
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&file), 0o600);
        assert_eq!(std::fs::read(&file).unwrap(), b"{\"a\":1}");
        // A crashed process's temp file goes; another file's stays.
        let stale = dir.join("secret.1-0.tmp");
        let other = dir.join("secrets.1-0.tmp");
        std::fs::write(&stale, b"half").unwrap();
        std::fs::write(&other, b"half").unwrap();
        super::write_private(&file, b"{}").unwrap();
        assert!(!stale.exists() && other.exists());
        // A write that fails (a folder is in the way) leaves no temp file.
        let blocked = dir.join("blocked.json");
        std::fs::create_dir(&blocked).unwrap();
        assert!(super::write_private(&blocked, b"{}").is_err());
        let temp = |e: &std::fs::DirEntry| e.file_name().to_string_lossy().ends_with(".tmp");
        let temps: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(temp)
            .collect();
        assert_eq!(temps.len(), 1, "only the other file's");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
