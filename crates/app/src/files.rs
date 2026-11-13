//! Files with personal data: readable by the owner only from the moment they
//! exist, and written whole (a temp file renamed into place), so a crash or a
//! reader never sees half a file.

use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Write `bytes` to `path` with owner-only permissions (0600 on Unix).
/// Each write has a temp file of its own, so two threads writing the same
/// file at once leave one whole version of it.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let n = WRITES.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("{}-{n}.tmp", std::process::id()));
    // A leftover from a crash would keep its mode; `create_new` sets ours.
    let _ = std::fs::remove_file(&tmp);
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
        let _ = std::fs::remove_dir_all(&dir);
    }
}
