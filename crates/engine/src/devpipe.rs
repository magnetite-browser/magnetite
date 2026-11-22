//! A private `DevTools` connection to our own browser process.
//!
//! Some Chromium features have no CEF API, only a `DevTools` command that needs
//! a trusted, browser-wide session (opening an extension's popup:
//! `Extensions.triggerAction`). Chromium serves such a session on a pipe
//! (`--remote-debugging-pipe`): on Unix it reads commands from fd 3 and
//! writes replies to fd 4; on Windows it takes the two pipe handles from
//! `--remote-debugging-io-pipes`. Both pipes are made inside our process
//! before CEF starts, so nothing outside it can connect: there is no port.
//! Programs we start do not inherit them (descriptors are close-on-exec,
//! handles are not inheritable). Messages are JSON, each ended by a NUL byte.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::{FromRawFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{FromRawHandle, IntoRawHandle, OwnedHandle};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::{Value, json};

use crate::engine::{Engine, lock};

type Reply = Box<dyn FnOnce(Result<Value, String>) + Send>;

struct Pipe {
    to_chrome: Mutex<File>,
    pending: Mutex<HashMap<u64, Reply>>,
    next_id: AtomicU64,
}

static PIPE: OnceLock<Pipe> = OnceLock::new();

/// The fds Chromium reads commands from and writes replies to.
#[cfg(unix)]
const CHROME_READS: RawFd = 3;
#[cfg(unix)]
const CHROME_WRITES: RawFd = 4;

/// Chromium's ends of the pipe, as `--remote-debugging-io-pipes` names them:
/// its read handle, then its write handle. Chromium owns them once it runs.
#[cfg(windows)]
static CHROME_HANDLES: OnceLock<String> = OnceLock::new();

/// The value of `--remote-debugging-io-pipes`, once the pipe is made.
#[cfg(windows)]
pub(crate) fn chrome_handles() -> Option<&'static str> {
    CHROME_HANDLES.get().map(String::as_str)
}

/// Make the pipe; call in the browser process before CEF starts. A second
/// call finds it made. Returns false (and the features needing it stay off)
/// when the system gives no pipe.
#[cfg(windows)]
pub(crate) fn open() -> bool {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Pipes::CreatePipe;

    if PIPE.get().is_some() {
        return true;
    }
    // An anonymous pipe, as (read end, write end). Not inheritable, as no
    // security attributes ask for it: Chromium runs in this process.
    let pipe = || {
        let (mut read, mut write) = (HANDLE::default(), HANDLE::default());
        // SAFETY: both out-pointers are valid; on success the handles are
        // new and owned by nothing else.
        unsafe {
            CreatePipe(&raw mut read, &raw mut write, None, 0).ok()?;
            Some((
                OwnedHandle::from_raw_handle(read.0),
                OwnedHandle::from_raw_handle(write.0),
            ))
        }
    };
    let (Some((chrome_reads, ours_write)), Some((ours_read, chrome_writes))) = (pipe(), pipe())
    else {
        return false;
    };
    // Handles fit 32 bits, also in a 64-bit process; Chromium parses them so.
    let number = |h: &OwnedHandle| {
        use std::os::windows::io::AsRawHandle;
        u32::try_from(h.as_raw_handle() as usize).ok()
    };
    let (Some(reads), Some(writes)) = (number(&chrome_reads), number(&chrome_writes)) else {
        return false;
    };
    if !start(File::from(ours_write), File::from(ours_read)) {
        return false;
    }
    // From here on they are Chromium's, which closes them.
    let _ = chrome_reads.into_raw_handle();
    let _ = chrome_writes.into_raw_handle();
    CHROME_HANDLES.set(format!("{reads},{writes}")).is_ok()
}

/// Make the pipe; call in the browser process before CEF starts, and before
/// any thread that may open a file (`reserve_devtools_pipe`). A second call
/// finds it made.
/// Returns false (and the features needing it stay off) when fds 3 and 4
/// are already in use.
#[cfg(unix)]
pub(crate) fn open() -> bool {
    if PIPE.get().is_some() {
        return true;
    }
    // SAFETY: plain POSIX calls on descriptors created here; fds 3 and 4
    // are checked to be unused before anything is put there.
    let fds = unsafe {
        if libc::fcntl(CHROME_READS, libc::F_GETFD) != -1
            || libc::fcntl(CHROME_WRITES, libc::F_GETFD) != -1
        {
            return false;
        }
        let (mut to, mut from) = ([0; 2], [0; 2]);
        if libc::pipe(to.as_mut_ptr()) != 0 || libc::pipe(from.as_mut_ptr()) != 0 {
            return false;
        }
        // pipe() hands out the lowest free fds, likely 3 to 6: move every end
        // above 10 first, then put Chromium's two ends on 3 and 4.
        let mut high = [0; 4];
        for (h, fd) in high.iter_mut().zip([to[0], to[1], from[0], from[1]]) {
            *h = libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 10);
            libc::close(fd);
        }
        let [chrome_reads, ours_write, ours_read, chrome_writes] = high;
        if high.contains(&-1)
            || libc::dup2(chrome_reads, CHROME_READS) == -1
            || libc::dup2(chrome_writes, CHROME_WRITES) == -1
        {
            return false;
        }
        libc::close(chrome_reads);
        libc::close(chrome_writes);
        libc::fcntl(CHROME_READS, libc::F_SETFD, libc::FD_CLOEXEC);
        libc::fcntl(CHROME_WRITES, libc::F_SETFD, libc::FD_CLOEXEC);
        (ours_write, ours_read)
    };
    // SAFETY: both fds were just created and are owned by nothing else.
    let (to_chrome, from_chrome) = unsafe { (File::from_raw_fd(fds.0), File::from_raw_fd(fds.1)) };
    start(to_chrome, from_chrome)
}

/// Start reading replies and keep our two ends.
fn start(to_chrome: File, from_chrome: File) -> bool {
    let started = std::thread::Builder::new()
        .name("devtools-pipe".into())
        .spawn(move || read_replies(from_chrome))
        .is_ok();
    started
        && PIPE
            .set(Pipe {
                to_chrome: Mutex::new(to_chrome),
                pending: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(1),
            })
            .is_ok()
}

/// Whether the pipe exists (then `--remote-debugging-pipe` is passed on).
pub(crate) fn is_open() -> bool {
    PIPE.get().is_some()
}

/// Send `method` with `params`; `reply` runs on the UI thread with the
/// result, or the error Chromium (or a closed pipe) gave.
pub(crate) fn call(
    method: &str,
    params: &Value,
    reply: impl FnOnce(Result<Value, String>) + Send + 'static,
) {
    let Some(pipe) = PIPE.get() else {
        reply(Err("no DevTools pipe".into()));
        return;
    };
    let id = pipe.next_id.fetch_add(1, Ordering::Relaxed);
    lock(&pipe.pending).insert(id, Box::new(reply));
    let mut message = json!({ "id": id, "method": method, "params": params })
        .to_string()
        .into_bytes();
    message.push(0);
    if lock(&pipe.to_chrome).write_all(&message).is_err()
        && let Some(reply) = lock(&pipe.pending).remove(&id)
    {
        reply(Err("DevTools pipe closed".into()));
    }
}

/// Reader thread: hands each reply to its caller on the UI thread. Events
/// (messages without an id) are not used.
fn read_replies(mut from_chrome: File) {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    while let Ok(n) = from_chrome.read(&mut chunk) {
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..n]);
        while let Some(end) = buffer.iter().position(|b| *b == 0) {
            let message: Vec<u8> = buffer.drain(..=end).collect();
            deliver(&message[..end]);
        }
    }
}

fn deliver(message: &[u8]) {
    let Ok(value) = serde_json::from_slice::<Value>(message) else {
        return;
    };
    let Some(id) = value.get("id").and_then(Value::as_u64) else {
        return;
    };
    let Some(reply) = PIPE.get().and_then(|p| lock(&p.pending).remove(&id)) else {
        return;
    };
    let result = match value.get("error") {
        Some(e) => Err(e
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("DevTools error")
            .to_string()),
        None => Ok(value.get("result").cloned().unwrap_or(Value::Null)),
    };
    Engine::post(Box::new(move || reply(result)));
}
