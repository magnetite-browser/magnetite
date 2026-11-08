//! Chromium session files ("SNSS"), written command by command here: the
//! replay must find the tabs that were open, in order, with the page each
//! showed, its pinned state and its tab group.

use std::fs;
use std::path::PathBuf;

use magnetite_importer::ImportError;
use magnetite_importer::chromium::session::{self, Session, SessionTab, TabGroup};

/// A `base::Pickle`: values padded to 4 bytes, behind a u32 payload size.
#[derive(Default)]
struct Pickle(Vec<u8>);

impl Pickle {
    fn int(mut self, v: i32) -> Self {
        self.0.extend(v.to_le_bytes());
        self
    }

    fn u64(mut self, v: u64) -> Self {
        self.0.extend(v.to_le_bytes());
        self
    }

    fn bytes(mut self, bytes: &[u8], len: usize) -> Self {
        self = self.int(len.try_into().unwrap());
        self.0.extend(bytes);
        self.0.resize(self.0.len().next_multiple_of(4), 0);
        self
    }

    fn string(self, s: &str) -> Self {
        self.bytes(s.as_bytes(), s.len())
    }

    fn string16(self, s: &str) -> Self {
        let units: Vec<u16> = s.encode_utf16().collect();
        let bytes: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
        self.bytes(&bytes, units.len())
    }

    fn finish(self) -> Vec<u8> {
        let size = u32::try_from(self.0.len()).unwrap();
        [size.to_le_bytes().as_slice(), &self.0].concat()
    }
}

/// C struct payloads: little-endian fields, 64-bit ones 8-byte aligned.
fn ints(values: &[i32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

const GROUP: (u64, u64) = (0x0123_4567_89ab_cdef, 0x0fed_cba9_8765_4321);

/// A session file, as Chromium writes it (format version 3).
struct Snss(Vec<u8>);

impl Snss {
    fn new() -> Self {
        Self([b"SNSS".as_slice(), &3_i32.to_le_bytes()].concat())
    }

    fn command(mut self, id: u8, payload: &[u8]) -> Self {
        let size = u16::try_from(payload.len() + 1).unwrap();
        self.0.extend(size.to_le_bytes());
        self.0.push(id);
        self.0.extend(payload);
        self
    }

    /// The marker Chromium writes once the initial state is complete.
    fn marker(self) -> Self {
        self.command(255, &[])
    }

    fn window(self, window: i32) -> Self {
        self.command(9, &ints(&[window, 0]))
    }

    fn tab(self, window: i32, tab: i32, index: i32) -> Self {
        self.command(0, &ints(&[window, tab]))
            .command(2, &ints(&[tab, index]))
    }

    fn navigation(self, tab: i32, index: i32, url: &str, title: &str) -> Self {
        let pickle = Pickle::default()
            .int(tab)
            .int(index)
            .string(url)
            .string16(title)
            .string("") // page state
            .int(0) // transition type
            .finish();
        self.command(6, &pickle)
    }

    fn select(self, tab: i32, index: i32) -> Self {
        self.command(7, &ints(&[tab, index]))
    }

    fn pinned(self, tab: i32) -> Self {
        self.command(12, &ints(&[tab, 1]))
    }

    fn last_active(self, tab: i32, micros_since_1601: i64) -> Self {
        let payload = [ints(&[tab, 0]), micros_since_1601.to_le_bytes().to_vec()].concat();
        self.command(21, &payload)
    }

    fn group(self, tab: i32) -> Self {
        let mut payload = ints(&[tab, 0]);
        payload.extend(GROUP.0.to_le_bytes());
        payload.extend(GROUP.1.to_le_bytes());
        payload.extend([1, 0, 0, 0, 0, 0, 0, 0]);
        self.command(25, &payload)
    }

    fn group_title(self, title: &str) -> Self {
        let pickle = Pickle::default()
            .u64(GROUP.0)
            .u64(GROUP.1)
            .string16(title)
            .int(3) // color
            .finish();
        self.command(27, &pickle)
    }

    /// Tab (16) or window (17) closed: `{int32 id; int64 close_time}`.
    fn closed(self, id: u8, closed: i32) -> Self {
        self.command(id, &ints(&[closed, 0, 0, 0]))
    }
}

fn tab(url: &str, title: &str) -> SessionTab {
    SessionTab {
        url: url.into(),
        title: title.into(),
        pinned: false,
        group: None,
        last_active: None,
    }
}

#[test]
fn replays_open_tabs_window_by_window_in_tab_order() {
    let file = Snss::new()
        // Window 5 was created after window 2, so it comes second.
        .window(5)
        .window(2)
        .tab(5, 50, 0)
        .navigation(50, 0, "https://five.example/", "Five")
        .tab(2, 21, 1)
        .navigation(21, 0, "https://two-b.example/", "Zwei – 日本語 🦀")
        .tab(2, 20, 0)
        .navigation(20, 0, "https://two-a.example/", "Two A")
        .pinned(20)
        // 2026-01-01T00:00:00Z in microseconds since 1601.
        .last_active(20, 13_411_699_200_000_000)
        .group(21)
        .group_title("Reading 📚")
        .marker()
        // Closed tab and closed window: neither may appear.
        .tab(2, 22, 2)
        .navigation(22, 0, "https://closed-tab.example/", "Closed")
        .closed(16, 22)
        .window(9)
        .tab(9, 90, 0)
        .navigation(90, 0, "https://closed-window.example/", "Closed")
        .closed(17, 9)
        // Commands we do not know are skipped.
        .command(200, b"unknown")
        // A popup (window type 1) is not a tab to import.
        .command(9, &ints(&[7, 1]))
        .tab(7, 70, 0)
        .navigation(70, 0, "https://popup.example/", "Popup");

    let session = session::parse(&file.0).unwrap();
    assert_eq!(
        session,
        Session {
            tabs: vec![
                SessionTab {
                    pinned: true,
                    last_active: Some(1_767_225_600),
                    ..tab("https://two-a.example/", "Two A")
                },
                SessionTab {
                    group: Some(0),
                    ..tab("https://two-b.example/", "Zwei – 日本語 🦀")
                },
                tab("https://five.example/", "Five"),
            ],
            groups: vec![TabGroup {
                title: "Reading 📚".into()
            }],
        }
    );
}

/// `fixtures/chromium/Default` is a throwaway profile of headless Chrome for
/// Testing 153 (macOS, 2026-11-07), driven by a test extension over pages on
/// 127.0.0.1: it pinned a tab, grouped two, went back in one, closed one and
/// opened a second window. Window state blobs (screen UUIDs) are blanked.
#[test]
fn reads_a_real_chrome_session() {
    let profile = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/chromium/Default");
    let page = |name: &str| format!("http://127.0.0.1:8765/{name}.html");
    let session = session::read_latest(&profile).unwrap().unwrap();
    assert_eq!(
        session,
        Session {
            tabs: vec![
                SessionTab {
                    pinned: true,
                    last_active: Some(1_794_048_406),
                    ..tab(&page("pinned"), "Pinned page")
                },
                SessionTab {
                    last_active: Some(1_794_048_405),
                    ..tab("about:blank", "")
                },
                SessionTab {
                    group: Some(0),
                    last_active: Some(1_794_048_407),
                    ..tab(&page("unicode"), "Grüße – 日本語 🦀")
                },
                // Went back from second.html.
                SessionTab {
                    group: Some(0),
                    last_active: Some(1_794_048_413),
                    ..tab(&page("first"), "First")
                },
                tab(&page("done"), "done"),
                // The second window.
                SessionTab {
                    last_active: Some(1_794_048_413),
                    ..tab(&page("first"), "First")
                },
            ],
            groups: vec![TabGroup {
                title: "Reading 📚".into()
            }],
        }
    );
}

#[test]
fn the_selected_navigation_is_the_page_shown() {
    let file = Snss::new()
        .window(1)
        .tab(1, 10, 0)
        .navigation(10, 0, "https://a.example/", "A")
        .navigation(10, 1, "https://b.example/", "B")
        .navigation(10, 2, "https://c.example/", "C")
        // Went back once.
        .select(10, 1)
        // Selected past the stored entries (Chromium skips the new tab
        // page): the last stored one is shown.
        .tab(1, 11, 1)
        .navigation(11, 0, "https://d.example/", "D")
        .select(11, 4)
        .marker();

    let urls: Vec<String> = session::parse(&file.0)
        .unwrap()
        .tabs
        .into_iter()
        .map(|t| t.url)
        .collect();
    assert_eq!(urls, ["https://b.example/", "https://d.example/"]);
}

/// Pruning drops navigations and, as in Chromium, renumbers the later ones
/// and the selection: without either, another page would be shown.
#[test]
fn pruned_navigations_are_dropped_and_the_rest_renumbered() {
    // Tab `tab` (also its index in window 1) went through `pages` and shows
    // the third.
    let history = |mut file: Snss, tab: i32, pages: [&str; 4]| {
        file = file.tab(1, tab, tab);
        for (index, page) in (0..).zip(pages) {
            file = file.navigation(tab, index, &format!("https://{page}.example/"), page);
        }
        file.select(tab, 2)
    };
    let file = Snss::new().window(1);
    // kCommandTabNavigationPathPruned {id, index, count}: without entry 0,
    // the shown entry is number 1.
    let file = history(file, 1, ["a", "b", "c", "d"]).command(24, &ints(&[1, 0, 1]));
    // kCommandTabNavigationPathPrunedFromBack {id, index}: entries from 1 on
    // are gone, the shown one with them; the last one left is shown.
    let file = history(file, 2, ["e", "f", "g", "h"]).command(5, &ints(&[2, 1]));
    // kCommandTabNavigationPathPrunedFromFront {id, count}: like the first.
    let file = history(file, 3, ["i", "j", "k", "l"])
        .command(11, &ints(&[3, 1]))
        .marker();

    let titles: Vec<String> = session::parse(&file.0)
        .unwrap()
        .tabs
        .into_iter()
        .map(|t| t.title)
        .collect();
    assert_eq!(titles, ["c", "e", "k"]);
}

#[test]
fn a_truncated_file_keeps_the_commands_before_the_cut() {
    let file = Snss::new()
        .window(1)
        .tab(1, 10, 0)
        .navigation(10, 0, "https://kept.example/", "Kept")
        .tab(1, 11, 1)
        .navigation(11, 0, "https://cut.example/", "Cut")
        .0;
    let cut = &file[..file.len() - 5];
    let session = session::parse(cut).unwrap();
    assert_eq!(session.tabs, [tab("https://kept.example/", "Kept")]);

    // Cut anywhere, the file still parses (or fails cleanly in the header).
    for len in 0..file.len() {
        assert_eq!(
            session::parse(&file[..len]).is_ok(),
            len >= 8,
            "cut at {len}"
        );
    }
}

#[test]
fn rejects_files_it_cannot_read() {
    let corrupt = |bytes: &[u8]| matches!(session::parse(bytes), Err(ImportError::Corrupt { .. }));
    assert!(corrupt(b"{\"not\": \"a session\"}"));
    // Encrypted with the OS keychain's key (Sessions_Encrypted/).
    assert!(corrupt(
        &[b"SNSS".as_slice(), &5_i32.to_le_bytes()].concat()
    ));
}

#[test]
fn read_latest_takes_the_newest_complete_file() {
    let profile = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("chromium-session-latest");
    let _ = fs::remove_dir_all(&profile);
    fs::create_dir_all(&profile).unwrap();
    assert_eq!(session::read_latest(&profile).unwrap(), None);

    let one_tab = |url: &str| Snss::new().window(1).tab(1, 1, 0).navigation(1, 0, url, "");
    let write = |name: &str, file: Snss| fs::write(profile.join(name), file.0).unwrap();
    let url = |profile: &PathBuf| {
        session::read_latest(profile).unwrap().unwrap().tabs[0]
            .url
            .clone()
    };
    // Legacy files predate the marker (format version 1).
    let mut legacy = one_tab("https://legacy.example/");
    legacy.0[4] = 1;
    write("Current Session", legacy);
    assert_eq!(url(&profile), "https://legacy.example/");
    // Once the browser uses Sessions/, the legacy file is stale.
    fs::create_dir(profile.join("Sessions")).unwrap();
    assert_eq!(session::read_latest(&profile).unwrap(), None);

    write(
        "Sessions/Session_13400000000000000",
        one_tab("https://old.example/").marker(),
    );
    write(
        "Sessions/Session_13500000000000000",
        one_tab("https://new.example/").marker(),
    );
    // Newest, but its initial state was never completely written.
    write(
        "Sessions/Session_13600000000000000",
        one_tab("https://partial.example/"),
    );
    assert_eq!(url(&profile), "https://new.example/");
    fs::remove_dir_all(&profile).unwrap();
}
