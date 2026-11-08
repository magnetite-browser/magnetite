//! Chromium's session files (`Sessions/Session_*`): the tabs that were open,
//! which of them were pinned, and their tab groups.
//!
//! A session file is a log of commands ("SNSS") that Chromium replays on
//! startup to rebuild its windows; we replay it the same way. Command ids and
//! layouts follow Chromium at d078d38f8aa2 (2026-11-07):
//! `components/sessions/core/session_service_commands.cc` (the commands and
//! how they are replayed), `session_command.cc` and `command_storage_backend.cc`
//! (file framing), `serialized_navigation_entry.cc` (navigations) and
//! `base/pickle.cc` (the encoding of most payloads). Chromium writes native
//! endianness and alignment: little-endian, 64-bit fields 8-byte aligned, on
//! every platform it ships on.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fs;
use std::hash::Hash;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::{ImportError, MAX_FILE_BYTES};

/// What a session file holds, reduced to what the sidebar needs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Session {
    /// Open tabs, window by window in tab order.
    pub tabs: Vec<SessionTab>,
    /// Tab groups the tabs refer to.
    pub groups: Vec<TabGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTab {
    /// The page the tab shows (its current navigation entry).
    pub url: String,
    pub title: String,
    pub pinned: bool,
    /// Index into [`Session::groups`].
    pub group: Option<usize>,
    /// Unix seconds, when known.
    pub last_active: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabGroup {
    pub title: String,
}

/// Limits for untrusted files. Tabs, windows, groups and navigations beyond
/// them are ignored; Chromium itself keeps at most 50 navigations per tab.
const MAX_TABS: usize = 10_000;
const MAX_WINDOWS: usize = 1_000;
const MAX_NAVIGATIONS: usize = 256;

/// The newest session of the profile in `profile_dir`, or `None` if it has
/// none. Like Chromium, skips files whose initial state was never completely
/// written, and files it cannot read when an older one is fine (on Windows a
/// running browser locks its current file). Errors only when session files
/// exist but none can be read.
pub fn read_latest(profile_dir: &Path) -> Result<Option<Session>, ImportError> {
    let mut first_error = None;
    for path in session_files(profile_dir)? {
        match replay_file(&path) {
            Ok(replay) if replay.complete => return Ok(Some(replay.finish())),
            Ok(_) => {}
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    first_error.map_or(Ok(None), Err)
}

/// Parse the bytes of one session file.
pub fn parse(bytes: &[u8]) -> Result<Session, ImportError> {
    replay(bytes, "Chromium session file").map(Replay::finish)
}

/// Microseconds between 1601-01-01 (Chromium's time epoch) and 1970-01-01.
const EPOCH_1601_MICROS: i64 = 11_644_473_600_000_000;

/// The profile's session files, newest first: `Sessions/Session_<time>`, where
/// `<time>` is the file's creation in microseconds since 1601 (the
/// modification time stands in when the name has none). Profiles of browsers
/// from before mid-2021 have a single `Current Session` instead; once a
/// browser uses the `Sessions` folders, that file is stale.
fn session_files(profile_dir: &Path) -> Result<Vec<PathBuf>, ImportError> {
    let dir = profile_dir.join("Sessions");
    let mut files: Vec<(i64, PathBuf)> = match fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|entry| {
                let name = entry.file_name();
                let suffix = name.to_str()?.strip_prefix("Session_")?;
                let time = suffix.parse::<i64>().ok().or_else(|| {
                    let modified = entry.metadata().ok()?.modified().ok()?;
                    let micros = modified.duration_since(UNIX_EPOCH).ok()?.as_micros();
                    i64::try_from(micros).ok()?.checked_add(EPOCH_1601_MICROS)
                });
                Some((time.unwrap_or(0), entry.path()))
            })
            .collect(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let legacy = profile_dir.join("Current Session");
            let stale = profile_dir.join("Sessions_Encrypted").exists();
            return Ok(if legacy.is_file() && !stale {
                vec![legacy]
            } else {
                Vec::new()
            });
        }
        Err(source) => return Err(ImportError::Read { path: dir, source }),
    };
    files.sort_by(|a, b| b.cmp(a));
    Ok(files.into_iter().map(|(_, path)| path).collect())
}

fn replay_file(path: &Path) -> Result<Replay, ImportError> {
    let bytes = crate::read_file(path, MAX_FILE_BYTES)?;
    replay(&bytes, &path.display().to_string())
}

/// Replay the commands of a session file. `what` names it in errors.
fn replay(bytes: &[u8], what: &str) -> Result<Replay, ImportError> {
    let corrupt = |detail: String| ImportError::Corrupt {
        what: what.to_string(),
        detail,
    };
    let mut header = Reader { rest: bytes };
    if header.take(4) != Some(b"SNSS".as_slice()) {
        return Err(corrupt("not a session file".into()));
    }
    let version = header
        .i32()
        .ok_or_else(|| corrupt("too short for a header".into()))?;
    // 1 predates the marker, 3 is current; 2, 4 and 5 are encrypted.
    match version {
        1 | 3 => {}
        2 | 4 | 5 => return Err(corrupt(format!("encrypted (format version {version})"))),
        _ => return Err(corrupt(format!("unknown format version {version}"))),
    }

    let mut rest = header.rest;
    let mut state = Replay {
        complete: version == 1,
        ..Replay::default()
    };
    // Each command: u16 size (of id and payload), u8 id, payload. A command
    // cut short is where the browser stopped writing; keep what came before.
    while let Some((size, after)) = rest.split_first_chunk::<2>() {
        let size = usize::from(u16::from_le_bytes(*size));
        let Some((command, after)) = after.split_at_checked(size) else {
            break;
        };
        let Some((&id, payload)) = command.split_first() else {
            break;
        };
        if let Some(command) = Command::decode(id, payload) {
            state.apply(command);
        }
        rest = after;
    }
    Ok(state)
}

/// The commands that matter for open tabs. Everything else (bounds, user
/// agents, workspaces, split views, ...) is skipped, as are commands we do not
/// know; Chromium would stop replaying at those.
#[derive(Debug)]
enum Command {
    Tab(i32, TabCommand),
    TabClosed(i32),
    WindowType {
        window: i32,
        kind: i32,
    },
    WindowClosed(i32),
    GroupTitle {
        group: u128,
        title: String,
    },
    /// Written after the initial state of a file (format version 3).
    Marker,
}

#[derive(Debug)]
enum TabCommand {
    Window(i32),
    Index(i32),
    Navigation(Navigation),
    SelectedNavigation(i32),
    /// Navigations `index..index + count` were removed.
    Prune {
        index: i32,
        count: i32,
    },
    /// Navigations from `index` on were removed.
    PruneFromBack(i32),
    Pinned(bool),
    /// Microseconds since 1601.
    LastActive(i64),
    Group(Option<u128>),
}

#[derive(Debug)]
struct Navigation {
    /// Position in the tab's history (not in our list of navigations).
    index: i32,
    url: String,
    title: String,
}

impl Command {
    /// Decode a payload; `None` for commands we skip and malformed payloads.
    fn decode(id: u8, payload: &[u8]) -> Option<Self> {
        // Most payloads are C structs, checked for their exact size like
        // Chromium does; `IDAndIndexPayload` is {int32 id; int32 index}.
        let fixed = |size: usize| (payload.len() == size).then_some(Reader { rest: payload });
        let id_and_index = || {
            let mut r = fixed(8)?;
            Some((r.i32()?, r.i32()?))
        };
        let tab = |tab_id, command| Some(Self::Tab(tab_id, command));
        match id {
            // kCommandSetTabWindow: int32[2] {window_id, tab_id}.
            0 => {
                let (window, tab_id) = id_and_index()?;
                tab(tab_id, TabCommand::Window(window))
            }
            // kCommandSetTabIndexInWindow
            2 => {
                let (tab_id, index) = id_and_index()?;
                tab(tab_id, TabCommand::Index(index))
            }
            // kCommandTabNavigationPathPrunedFromBack (obsolete)
            5 => {
                let (tab_id, index) = id_and_index()?;
                tab(tab_id, TabCommand::PruneFromBack(index))
            }
            // kCommandUpdateTabNavigation: pickle of the tab id and
            // SerializedNavigationEntry, whose fields we need come first.
            6 => {
                let mut r = Reader::pickle(payload)?;
                let tab_id = r.i32()?;
                let index = r.i32()?;
                let url = r.string()?;
                let title = r.string16()?;
                // Page state and transition type: required by Chromium too.
                r.data()?;
                r.i32()?;
                let navigation = Navigation { index, url, title };
                tab(tab_id, TabCommand::Navigation(navigation))
            }
            // kCommandSetSelectedNavigationIndex
            7 => {
                let (tab_id, index) = id_and_index()?;
                tab(tab_id, TabCommand::SelectedNavigation(index))
            }
            // kCommandSetWindowType
            9 => {
                let (window, kind) = id_and_index()?;
                Some(Self::WindowType { window, kind })
            }
            // kCommandTabNavigationPathPrunedFromFront (obsolete): the index
            // is the count removed from the front.
            11 => match id_and_index()? {
                (tab_id, count) if count > 0 => tab(tab_id, TabCommand::Prune { index: 0, count }),
                _ => None,
            },
            // kCommandSetPinnedState: {int32 tab_id; bool pinned}.
            12 => {
                let mut r = fixed(8)?;
                let tab_id = r.i32()?;
                tab(tab_id, TabCommand::Pinned(r.bool()?))
            }
            // kCommandTabClosed, kCommandWindowClosed:
            // {int32 id; int64 close_time}.
            16 | 17 => {
                let closed = fixed(16)?.i32()?;
                Some(if id == 16 {
                    Self::TabClosed(closed)
                } else {
                    Self::WindowClosed(closed)
                })
            }
            // kCommandLastActiveTime: {int32 tab_id; int64 time}.
            21 => {
                let mut r = fixed(16)?;
                let tab_id = r.i32()?;
                r.take(4)?; // padding
                tab(tab_id, TabCommand::LastActive(r.i64()?))
            }
            // kCommandTabNavigationPathPruned: {int32 id, index, count}.
            24 => {
                let mut r = fixed(12)?;
                let (tab_id, index, count) = (r.i32()?, r.i32()?, r.i32()?);
                (index >= 0 && count > 0)
                    .then_some(Self::Tab(tab_id, TabCommand::Prune { index, count }))
            }
            // kCommandSetTabGroup:
            // {int32 tab_id; {uint64 high, low} group; bool has_group}.
            25 => {
                let mut r = fixed(32)?;
                let tab_id = r.i32()?;
                r.take(4)?; // padding
                let group = r.token()?;
                let has_group = r.bool()?;
                tab(tab_id, TabCommand::Group(has_group.then_some(group)))
            }
            // kCommandSetTabGroupMetadata2: pickle of the group token, title
            // and color (later fields are optional).
            27 => {
                let mut r = Reader::pickle(payload)?;
                let group = r.token()?;
                let title = r.string16()?;
                r.u32()?;
                Some(Self::GroupTitle { group, title })
            }
            // CommandStorageBackend's kInitialStateMarkerCommandId.
            255 => Some(Self::Marker),
            _ => None,
        }
    }
}

/// Reads `base::Pickle` data: little-endian values, each padded to 4 bytes.
/// Struct payloads read the same way, given the padding before 64-bit fields.
struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    /// The payload of a pickle: after a header that starts with its size.
    fn pickle(data: &'a [u8]) -> Option<Self> {
        let size = usize::try_from(Reader { rest: data }.u32()?).ok()?;
        let header = data.len().checked_sub(size)?;
        (header >= 4 && header % 4 == 0).then(|| Reader {
            rest: data.get(header..).unwrap_or_default(),
        })
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (data, rest) = self.rest.split_at_checked(n)?;
        let padding = n.next_multiple_of(4) - n;
        self.rest = rest.get(padding..).unwrap_or_default();
        Some(data)
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    fn i32(&mut self) -> Option<i32> {
        self.array().map(i32::from_le_bytes)
    }

    fn u32(&mut self) -> Option<u32> {
        self.array().map(u32::from_le_bytes)
    }

    fn i64(&mut self) -> Option<i64> {
        self.array().map(i64::from_le_bytes)
    }

    fn bool(&mut self) -> Option<bool> {
        self.take(1)?.first().map(|&b| b != 0)
    }

    /// A `base::Token` (tab group id): high then low half.
    fn token(&mut self) -> Option<u128> {
        let high = u64::from_le_bytes(self.array()?);
        let low = u64::from_le_bytes(self.array()?);
        Some((u128::from(high) << 64) | u128::from(low))
    }

    /// Bytes behind an int32 length (negative lengths are invalid).
    fn data(&mut self) -> Option<&'a [u8]> {
        let len = usize::try_from(self.i32()?).ok()?;
        self.take(len)
    }

    fn string(&mut self) -> Option<String> {
        self.data()
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
    }

    /// UTF-16 behind an int32 length in code units.
    fn string16(&mut self) -> Option<String> {
        let len = usize::try_from(self.i32()?).ok()?;
        let bytes = self.take(len.checked_mul(2)?)?;
        let units = bytes.as_chunks().0.iter().map(|&u| u16::from_le_bytes(u));
        Some(
            char::decode_utf16(units)
                .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
                .collect(),
        )
    }
}

/// `SessionWindow::TYPE_NORMAL`; popups, apps and devtools are not imported.
const WINDOW_NORMAL: i32 = 0;

/// The state replayed so far, as in Chromium's `CreateTabsAndWindows`.
#[derive(Default)]
struct Replay {
    tabs: HashMap<i32, Tab>,
    /// Window types by window id. Chromium only restores windows whose type
    /// was set (others are "constrained") and that were not closed since.
    windows: HashMap<i32, i32>,
    group_titles: HashMap<u128, String>,
    /// Whether Chromium would restore from this file: its initial state was
    /// fully written (the marker), or its format predates the marker.
    complete: bool,
}

struct Tab {
    window: Option<i32>,
    /// Position in the window; there may be gaps.
    index: i32,
    /// The selected navigation's `Navigation::index`.
    current: i32,
    /// Ordered by `Navigation::index`.
    navigations: Vec<Navigation>,
    pinned: bool,
    group: Option<u128>,
    /// Microseconds since 1601.
    last_active: Option<i64>,
}

impl Default for Tab {
    fn default() -> Self {
        // Chromium's defaults: no position and no selected navigation yet.
        Self {
            window: None,
            index: -1,
            current: -1,
            navigations: Vec::new(),
            pinned: false,
            group: None,
            last_active: None,
        }
    }
}

impl Replay {
    fn apply(&mut self, command: Command) {
        match command {
            Command::Tab(id, command) => {
                if let Some(tab) = capped_entry(&mut self.tabs, id, MAX_TABS) {
                    tab.apply(command);
                }
            }
            Command::TabClosed(id) => {
                self.tabs.remove(&id);
            }
            Command::WindowType { window, kind } => {
                if let Some(slot) = capped_entry(&mut self.windows, window, MAX_WINDOWS) {
                    *slot = kind;
                }
            }
            Command::WindowClosed(id) => {
                self.windows.remove(&id);
            }
            // A group holds at least one tab.
            Command::GroupTitle { group, title } => {
                if let Some(slot) = capped_entry(&mut self.group_titles, group, MAX_TABS) {
                    *slot = title;
                }
            }
            Command::Marker => self.complete = true,
        }
    }

    /// The open tabs, as in Chromium's `AddTabsToWindows` and
    /// `SortTabsBasedOnVisualOrderAndClear`: tabs with navigations in normal
    /// windows, windows by id (creation order), tabs by index then id.
    fn finish(mut self) -> Session {
        let mut open: Vec<(i32, Tab)> = self
            .tabs
            .into_iter()
            .filter(|(_, tab)| {
                tab.window.is_some_and(|window| {
                    window != 0 && self.windows.get(&window) == Some(&WINDOW_NORMAL)
                }) && !tab.navigations.is_empty()
            })
            .collect();
        open.sort_by_key(|(id, tab)| (tab.window, tab.index, *id));

        let mut groups = Vec::new();
        let mut group_index = HashMap::new();
        let tabs = open
            .into_iter()
            .filter_map(|(_, tab)| {
                let group = tab.group.map(|token| {
                    *group_index.entry(token).or_insert_with(|| {
                        let title = self.group_titles.remove(&token).unwrap_or_default();
                        groups.push(TabGroup { title });
                        groups.len() - 1
                    })
                });
                // The first navigation at or after the selected one, else the
                // last: the selection can point past entries Chromium did not
                // store (like the new tab page).
                let selected = tab
                    .navigations
                    .iter()
                    .position(|n| n.index >= tab.current)
                    .unwrap_or(tab.navigations.len().saturating_sub(1));
                let navigation = tab.navigations.into_iter().nth(selected)?;
                Some(SessionTab {
                    url: navigation.url,
                    title: navigation.title,
                    pinned: tab.pinned,
                    group,
                    last_active: tab.last_active.and_then(unix_seconds),
                })
            })
            .collect();
        Session { tabs, groups }
    }
}

impl Tab {
    fn apply(&mut self, command: TabCommand) {
        match command {
            TabCommand::Window(window) => self.window = Some(window),
            TabCommand::Index(index) => self.index = index,
            TabCommand::Navigation(navigation) => {
                match self
                    .navigations
                    .binary_search_by_key(&navigation.index, |n| n.index)
                {
                    Ok(i) => self.navigations[i] = navigation,
                    Err(i) if self.navigations.len() < MAX_NAVIGATIONS => {
                        self.navigations.insert(i, navigation);
                    }
                    Err(_) => {}
                }
            }
            TabCommand::SelectedNavigation(index) => self.current = index,
            TabCommand::Prune { index, count } => {
                // As `ProcessTabNavigationPathPrunedCommand`; `index >= 0` and
                // `count > 0`, so nothing below overflows.
                let end = index.saturating_add(count);
                if self.current >= end {
                    self.current -= count;
                } else if self.current >= index {
                    self.current = index - 1;
                }
                self.navigations
                    .retain(|n| n.index < index || n.index >= end);
                for n in &mut self.navigations {
                    if n.index >= index {
                        n.index -= count;
                    }
                }
            }
            TabCommand::PruneFromBack(index) => self.navigations.retain(|n| n.index < index),
            TabCommand::Pinned(pinned) => self.pinned = pinned,
            TabCommand::LastActive(time) => self.last_active = Some(time),
            TabCommand::Group(group) => self.group = group,
        }
    }
}

/// The entry for `key`, inserted as default unless `map` already holds `max`.
fn capped_entry<K: Eq + Hash, V: Default>(
    map: &mut HashMap<K, V>,
    key: K,
    max: usize,
) -> Option<&mut V> {
    let full = map.len() >= max;
    match map.entry(key) {
        Entry::Occupied(entry) => Some(entry.into_mut()),
        Entry::Vacant(_) if full => None,
        Entry::Vacant(entry) => Some(entry.insert(V::default())),
    }
}

/// Chromium's microseconds since 1601 as Unix seconds; `None` for its null time.
pub(super) fn unix_seconds(micros: i64) -> Option<i64> {
    (micros > 0).then(|| (micros - EPOCH_1601_MICROS).div_euclid(1_000_000))
}
