//! A private `DevTools` connection to our own browser process.
//!
//! Some Chromium features have no CEF API, only a `DevTools` command that needs
//! a trusted, browser-wide session (opening an extension's popup:
//! `Extensions.triggerAction`). Chromium serves such a session on a pipe
//! (`--remote-debugging-pipe`): it reads commands from fd 3 and writes
//! replies to fd 4. Both pipes are made inside our process before CEF
//! starts, so nothing outside it can connect: there is no port. All their
//! descriptors are close-on-exec, so programs we start do not inherit them.
//! Messages are JSON, each ended by a NUL byte.
//!
//! Windows hands Chromium such pipes as inherited handles instead; that is
//! not done yet, so there `open` makes nothing and extension popups do not
//! open from the sidebar.
#![cfg_attr(not(unix), allow(dead_code))]

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::{FromRawFd, RawFd};
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

/// No pipe on Windows yet (see above).
#[cfg(not(unix))]
pub(crate) fn open() -> bool {
    false
}

/// Make the pipe; call once in the browser process before CEF starts.
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
