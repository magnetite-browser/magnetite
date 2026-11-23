//! Moving a file to the system's trash, where the user can still get it
//! back: the downloads list's "Move to Trash".

use std::path::Path;

/// Move the file at `path` to the trash: the Trash on macOS, the Recycle
/// Bin on Windows, the freedesktop.org trash in the home folder on Linux.
pub fn move_to_trash(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    return crate::mac::trash(path);
    #[cfg(target_os = "windows")]
    return crate::win::trash(path);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return freedesktop::trash(path);
}

/// The trash of the freedesktop.org specification in the user's home
/// (`$XDG_DATA_HOME/Trash`), which file managers list and restore from. A
/// file on another disk than the home is refused, not copied: that would
/// need the trash of that disk.
#[cfg(all(unix, any(not(target_os = "macos"), test)))]
#[cfg_attr(target_os = "macos", allow(dead_code))]
mod freedesktop {
    use std::fs;
    use std::io::{self, Write};
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    pub(super) fn trash(path: &Path) -> io::Result<()> {
        // A relative one is invalid by the XDG specification.
        let data = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|d| d.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .ok_or(io::ErrorKind::NotFound)?;
        trash_into(&data.join("Trash"), path, &deletion_date())
    }

    /// Move `path` into the trash folder `trash`: the file to `files/`, a
    /// note of where it was and when (`deleted_at`) to `info/`. A name
    /// taken there gets a number.
    pub(super) fn trash_into(trash: &Path, path: &Path, deleted_at: &str) -> io::Result<()> {
        use std::os::unix::fs::DirBuilderExt;
        let path = std::path::absolute(path)?;
        let name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
        let (files, info) = (trash.join("files"), trash.join("info"));
        for dir in [&files, &info] {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)?;
        }
        for n in 1..1000 {
            let entry = numbered(name, n);
            let mut note_name = entry.clone();
            note_name.push(".trashinfo");
            let note = info.join(note_name);
            // The note first, made only if new: it reserves the name.
            let mut file = match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&note)
            {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                opened => opened?,
            };
            let target = files.join(&entry);
            if target.exists() {
                let _ = fs::remove_file(&note);
                continue;
            }
            let moved = write!(
                file,
                "[Trash Info]\nPath={}\nDeletionDate={deleted_at}\n",
                encode(path.as_os_str().as_bytes())
            )
            .and_then(|()| fs::rename(&path, &target));
            if moved.is_err() {
                let _ = fs::remove_file(&note);
            }
            return moved;
        }
        Err(io::ErrorKind::AlreadyExists.into())
    }

    /// `name` for the `n`th try: as it is, then `report 2.pdf`, `report 3.pdf`.
    fn numbered(name: &std::ffi::OsStr, n: u32) -> std::ffi::OsString {
        if n == 1 {
            return name.to_os_string();
        }
        let path = Path::new(name);
        let mut out = path.file_stem().unwrap_or(name).to_os_string();
        out.push(format!(" {n}"));
        if let Some(ext) = path.extension() {
            out.push(".");
            out.push(ext);
        }
        out
    }

    /// A path as the specification writes it: percent-encoded bytes, with
    /// `/` and the unreserved characters as they are.
    fn encode(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|&b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                    char::from(b).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect()
    }

    /// Now in local time, as `2026-11-23T14:05:09`.
    fn deletion_date() -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let now = libc::time_t::try_from(now).unwrap_or_default();
        // SAFETY: an all-zero `tm` is valid; both pointers live for the call.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&raw const now, &raw mut tm) };
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_trashed_file_moves_with_a_note_and_a_free_name() {
            let dir = std::env::temp_dir().join(format!("magnetite-trash-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            let downloads = dir.join("Down loads");
            fs::create_dir_all(&downloads).unwrap();
            let trash = dir.join("Trash");
            for round in 1..=2 {
                let file = downloads.join("report.pdf");
                fs::write(&file, format!("round {round}")).unwrap();
                trash_into(&trash, &file, "2026-11-23T14:05:09").unwrap();
                assert!(!file.exists());
            }
            let kept = fs::read_to_string(trash.join("files/report 2.pdf")).unwrap();
            assert_eq!(kept, "round 2");
            let note = fs::read_to_string(trash.join("info/report.pdf.trashinfo")).unwrap();
            let place = downloads.join("report.pdf");
            assert_eq!(
                note,
                format!(
                    "[Trash Info]\nPath={}\nDeletionDate=2026-11-23T14:05:09\n",
                    place.to_str().unwrap().replace(' ', "%20")
                )
            );
            // Nothing to move: no note is left behind.
            assert!(trash_into(&trash, &downloads.join("gone.pdf"), "x").is_err());
            assert!(!trash.join("info/gone.pdf.trashinfo").exists());
            fs::remove_dir_all(&dir).unwrap();
        }

        #[test]
        fn names_get_numbers_before_their_extension() {
            assert_eq!(numbered("a.tar.gz".as_ref(), 1), "a.tar.gz");
            assert_eq!(numbered("a.tar.gz".as_ref(), 3), "a.tar 3.gz");
            assert_eq!(numbered("README".as_ref(), 2), "README 2");
        }
    }
}
