//! The chain's `devices` collection: per device the tabs open on it, and
//! the tabs of other devices it took over. Phones list their tabs there;
//! a desktop shows them and takes them over.
//!
//! Every device changes only its own entry and writes the collection as the
//! version after the one it read (`sync`), so devices cannot overwrite each
//! other, unlike with the sidebar.
//!
//! Taking a tab over: a phone lists a tab; this device opens it and puts
//! the tab's id into its own `taken`; the phone sees its tab there, closes
//! it and stops listing it; this device then drops the id. A tab another
//! device took is left to that one.
//!
//! A tab with `later` asks for that ("send to desktop"): it is taken without
//! a click, by one device only. The round puts it into `taken`, and it opens
//! once that write went through; a device that reads after sees the claim.
//!
//! A phone never writes the sidebar; it lists changes to it in `edits`
//! (magnetite-mobile-core's `edits` module). The round hands over those no
//! device has made; this device makes them, the sidebar syncs as after any
//! change, and the next round puts their ids into this device's `applied`.
//! The phone sees its edit there and stops listing it; this device then
//! drops the id. Unlike a sent tab, an edit is noted only once made: the
//! phone forgets a noted edit, so a note stored for one not made yet (a
//! write whose answer got lost) would lose it. Two devices that sync at the
//! same moment may then both make one, which changes nothing: an edit that
//! adds something carries the new item's id, and the others set a name or
//! remove.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use magnetite_core::UnixSeconds;

use crate::url_policy;

/// The collection's name (`ChainKeys::collection_id`).
pub const DEVICES: &str = "devices";
/// An entry not written for this long is of a device that is gone.
const GONE_AFTER: UnixSeconds = 90 * 24 * 60 * 60;

/// A tab open on a device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceTab {
    pub id: String,
    pub url: String,
    #[serde(default)]
    pub title: String,
    /// The space it is open in, if the chain's sidebar has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<String>,
    /// Sent to the other devices: whichever sees it first takes it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub later: bool,
    /// What a newer version wrote; kept when the collection is written.
    #[serde(flatten)]
    other: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Entry {
    id: String,
    #[serde(default)]
    name: String,
    /// When this entry was last written.
    #[serde(default)]
    updated_at: UnixSeconds,
    #[serde(default)]
    tabs: Vec<DeviceTab>,
    /// Ids of other devices' tabs this device took over.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    taken: Vec<String>,
    /// A phone's changes to the sidebar (`Edit`). Read one by one: a kind
    /// this version does not know is left alone, not an error.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    edits: Vec<Value>,
    /// Ids of other devices' edits this device made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    applied: Vec<String>,
    #[serde(flatten)]
    other: Map<String, Value>,
}

/// A change a phone asks for (magnetite-mobile-core's `edits::Edit`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Edit {
    /// The edit's own id; `item` in the change names what it is about.
    pub id: String,
    #[serde(flatten)]
    pub change: Change,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Change {
    /// A new pinned tab `item`, last in the space's pinned tree.
    Pin {
        space: String,
        item: String,
        url: String,
        #[serde(default)]
        title: String,
    },
    /// A new favorite `item`, last among those of the space's profile.
    Favorite {
        space: String,
        item: String,
        url: String,
        #[serde(default)]
        title: String,
    },
    /// A new space `item`, after the others.
    NewSpace {
        item: String,
        title: String,
        #[serde(default)]
        icon: Option<String>,
    },
    /// A new name for a pinned tab, folder, favorite or space.
    Rename { item: String, title: String },
    /// A pinned tab or a favorite goes to the archive.
    Remove { item: String },
}

/// The collection as it is stored.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Devices {
    #[serde(default)]
    devices: Vec<Entry>,
    #[serde(flatten)]
    other: Map<String, Value>,
}

/// This device, and the tabs it took over and edits it made that their
/// devices may still list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Me {
    /// Made up once per install (`SyncSettings::device_id`).
    pub id: String,
    pub name: String,
    pub taken: Vec<String>,
    pub applied: Vec<String>,
}

/// A tab open on another device, as the sidebar shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RemoteTab {
    /// The device's name: "iPhone".
    pub device: String,
    pub id: String,
    pub url: String,
    pub title: String,
    pub space: Option<String>,
}

/// What a round over the collection found.
#[derive(Debug, PartialEq)]
pub struct Round {
    /// The tabs open on other devices that no device has taken, of those
    /// this device opens (`url_policy`).
    pub tabs: Vec<RemoteTab>,
    /// The tabs sent to the other devices that this round takes: they are
    /// in `taken`, and open once `all` is written (or needs no write).
    pub sent: Vec<RemoteTab>,
    /// `Me::taken` without the tabs no device lists any more, with `sent`.
    pub taken: Vec<String>,
    /// The edits of other devices no device has made yet: to make now. A
    /// later round notes them in `applied`, from `Me::applied`.
    pub edits: Vec<Edit>,
    /// `Me::applied` without the edits no device lists any more.
    pub applied: Vec<String>,
    /// This device's entry differs from the stored one: write `all`.
    pub write: bool,
}

/// Read the other devices' tabs out of `all` and bring this device's entry
/// up to date in it. The entry holds what this device took and the edits it
/// made; a device with neither has none.
pub fn round(all: &mut Devices, me: &Me, now: UnixSeconds) -> Round {
    let others = || all.devices.iter().filter(|d| d.id != me.id);
    let mut taken: Vec<String> = me
        .taken
        .iter()
        .filter(|id| others().any(|d| d.tabs.iter().any(|t| &t.id == *id)))
        .cloned()
        .collect();
    let (mut tabs, mut sent) = (Vec::new(), Vec::new());
    for d in others() {
        for t in &d.tabs {
            let elsewhere = others().any(|o| o.taken.contains(&t.id));
            // One this device would not open stays with its device: taken,
            // it would be closed there and open nowhere.
            let opens = url_policy::allowed(&t.url, url_policy::Source::Stored);
            if elsewhere || !opens || taken.contains(&t.id) {
                continue;
            }
            let tab = RemoteTab {
                device: d.name.clone(),
                id: t.id.clone(),
                url: t.url.clone(),
                title: t.title.clone(),
                space: t.space.clone(),
            };
            if t.later {
                taken.push(t.id.clone());
                sent.push(tab);
            } else {
                tabs.push(tab);
            }
        }
    }
    let stored = all.devices.iter().find(|d| d.id == me.id);
    let lists = |id: &String| {
        others().any(|d| (d.edits.iter()).any(|e| e.get("id").and_then(Value::as_str) == Some(id)))
    };
    // Those the stored entry notes count too: a round can run with what this
    // device knew before it made them, and must not take the note back.
    let mut applied: Vec<String> = Vec::new();
    for id in (me.applied.iter()).chain(stored.into_iter().flat_map(|d| &d.applied)) {
        if lists(id) && !applied.contains(id) {
            applied.push(id.clone());
        }
    }
    let mut edits = Vec::new();
    for d in others() {
        for e in &d.edits {
            let Ok(edit) = serde_json::from_value::<Edit>(e.clone()) else {
                continue;
            };
            let elsewhere = others().any(|o| o.applied.contains(&edit.id));
            if !elsewhere && !applied.contains(&edit.id) {
                edits.push(edit);
            }
        }
    }
    let stored = stored.map(|d| (d.taken.clone(), d.applied.clone()));
    let write = stored.unwrap_or_default() != (taken.clone(), applied.clone());
    if write {
        let mine = all.devices.iter().position(|d| d.id == me.id);
        let other = mine
            .map(|i| all.devices.remove(i).other)
            .unwrap_or_default();
        all.devices.retain(|d| now - d.updated_at < GONE_AFTER);
        if !taken.is_empty() || !applied.is_empty() {
            all.devices.push(Entry {
                id: me.id.clone(),
                name: me.name.clone(),
                updated_at: now,
                tabs: Vec::new(),
                taken: taken.clone(),
                edits: Vec::new(),
                applied: applied.clone(),
                other,
            });
        }
    }
    Round {
        tabs,
        sent,
        taken,
        edits,
        applied,
        write,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: UnixSeconds = 2_000_000_000;

    fn stored(json: &str) -> Devices {
        serde_json::from_str(json).unwrap()
    }

    /// A phone with the tabs "a" and "b", and `more` entries after it.
    fn phone(more: &str) -> Devices {
        phone_with("", more)
    }

    /// `b`: more fields of the phone's tab "b".
    fn phone_with(b: &str, more: &str) -> Devices {
        stored(&format!(
            r#"{{"devices":[
                {{"id":"phone","name":"iPhone","updated_at":{NOW},"battery":80,"tabs":[
                    {{"id":"a","url":"https://a.example/","title":"A","space":"s1","group":"x"}},
                    {{"id":"b","url":"https://b.example/","title":"B"{b}}}]}}
                {more}]}}"#
        ))
    }

    fn ids(tabs: &[RemoteTab]) -> Vec<&str> {
        tabs.iter().map(|t| t.id.as_str()).collect()
    }

    fn me(taken: &[&str]) -> Me {
        Me {
            id: "mac".into(),
            name: "Mac".into(),
            taken: taken.iter().map(ToString::to_string).collect(),
            applied: Vec::new(),
        }
    }

    #[test]
    fn other_devices_tabs_are_listed_and_nothing_is_written_for_looking() {
        let mut all = phone("");
        let round = round(&mut all, &me(&[]), NOW);
        assert_eq!(round.tabs.len(), 2);
        assert_eq!(
            (
                round.tabs[0].device.as_str(),
                round.tabs[0].space.as_deref()
            ),
            ("iPhone", Some("s1"))
        );
        assert!(round.sent.is_empty() && !round.write);
        assert_eq!(all, phone(""));
    }

    #[test]
    fn a_taken_tab_is_written_and_no_longer_listed() {
        let mut all = phone("");
        let round = round(&mut all, &me(&["a"]), NOW);
        assert!(round.write);
        assert_eq!(ids(&round.tabs), ["b"]);
        let json = serde_json::to_value(&all).unwrap();
        assert_eq!(json["devices"][1]["taken"][0], "a");
        // What a newer version wrote in the phone's entry is still there.
        assert_eq!(json["devices"][0]["battery"], 80);
        assert_eq!(json["devices"][0]["tabs"][0]["group"], "x");
    }

    #[test]
    fn a_sent_tab_is_taken_by_the_round_itself() {
        let sent = r#","later":true"#;
        let mut all = phone_with(sent, "");
        let first = round(&mut all, &me(&[]), NOW);
        assert_eq!((ids(&first.tabs), ids(&first.sent)), (vec!["a"], vec!["b"]));
        assert!(first.write && first.taken == ["b"]);
        assert_eq!(
            serde_json::to_value(&all).unwrap()["devices"][1]["taken"][0],
            "b"
        );
        // Opened and noted: the next round leaves it alone.
        let next = round(&mut all, &me(&["b"]), NOW);
        assert!(next.sent.is_empty() && !next.write);
    }

    #[test]
    fn a_tab_this_device_would_not_open_is_neither_shown_nor_taken() {
        let local = format!(
            r#",{{"id":"tablet","name":"iPad","updated_at":{NOW},"tabs":[
                {{"id":"f","url":"file:///etc/passwd"}},
                {{"id":"j","url":"javascript:alert(1)","later":true}}]}}"#
        );
        let mut all = phone(&local);
        let round = round(&mut all, &me(&[]), NOW);
        assert_eq!(ids(&round.tabs), ["a", "b"]);
        assert!(round.sent.is_empty() && !round.write);
    }

    #[test]
    fn a_tab_another_device_took_is_left_to_it() {
        let other = format!(
            r#",{{"id":"pc","name":"Linux PC","updated_at":{NOW},"tabs":[],"taken":["a","b"]}}"#
        );
        let mut all = phone_with(r#","later":true"#, &other);
        let round = round(&mut all, &me(&[]), NOW);
        assert!(round.tabs.is_empty() && round.sent.is_empty() && !round.write);
    }

    #[test]
    fn once_the_phone_closed_the_tab_the_handover_is_forgotten() {
        // The phone lists neither "gone" nor anything this device took.
        let mine = format!(
            r#",{{"id":"mac","name":"Mac","updated_at":{NOW},"tabs":[],"taken":["gone"]}}"#
        );
        let mut all = phone(&mine);
        let round = round(&mut all, &me(&["gone"]), NOW);
        assert!(round.taken.is_empty() && round.write);
        assert_eq!(all, phone(""), "a device that took nothing has no entry");
    }

    /// A phone that asks for a pinned tab (edit "e1") and for a change of a
    /// kind this version does not know, and `more` entries after it.
    fn phone_editing(more: &str) -> Devices {
        stored(&format!(
            r#"{{"devices":[
                {{"id":"phone","name":"iPhone","updated_at":{NOW},"tabs":[],"edits":[
                    {{"id":"e1","made_at":1,"op":"pin","space":"s1","item":"t9","url":"https://p.example/","title":"P"}},
                    {{"id":"e2","op":"paint_it_blue","item":"s1"}}]}}
                {more}]}}"#
        ))
    }

    #[test]
    fn a_phones_edit_is_noted_once_made_and_its_unknown_kinds_are_left_alone() {
        let mut all = phone_editing("");
        // Handed over, not noted: should this device never make it, the
        // phone still lists it.
        let first = round(&mut all, &me(&[]), NOW);
        assert_eq!(first.edits.len(), 1);
        assert!(matches!(&first.edits[0].change, Change::Pin { item, .. } if item == "t9"));
        assert!(!first.write && first.applied.is_empty());

        // Made: the next round notes it and hands it over no more.
        let made = Me {
            applied: vec!["e1".into()],
            ..me(&[])
        };
        let next = round(&mut all, &made, NOW);
        assert!(next.edits.is_empty() && next.write && next.applied == ["e1"]);
        let json = serde_json::to_value(&all).unwrap();
        assert_eq!(json["devices"][1]["applied"], serde_json::json!(["e1"]));
        // The phone's entry, the edit of the unknown kind with it, is as it was.
        assert_eq!(json["devices"][0]["edits"][1]["op"], "paint_it_blue");

        // A round with what this device knew before it made it keeps the note.
        let early = round(&mut all, &me(&[]), NOW);
        assert!(early.edits.is_empty() && !early.write);
    }

    #[test]
    fn an_edit_another_device_made_is_left_to_it() {
        let other = format!(
            r#",{{"id":"pc","name":"Linux PC","updated_at":{NOW},"tabs":[],"applied":["e1"]}}"#
        );
        let mut all = phone_editing(&other);
        let round = round(&mut all, &me(&[]), NOW);
        assert!(round.edits.is_empty() && !round.write);
    }

    #[test]
    fn once_the_phone_stopped_listing_an_edit_it_is_forgotten() {
        let mine = format!(
            r#",{{"id":"mac","name":"Mac","updated_at":{NOW},"tabs":[],"applied":["done"]}}"#
        );
        let mut all = phone(&mine);
        let noted = Me {
            applied: vec!["done".into()],
            ..me(&[])
        };
        let round = round(&mut all, &noted, NOW);
        assert!(round.applied.is_empty() && round.write);
        assert_eq!(all, phone(""), "a device that made nothing has no entry");
    }

    #[test]
    fn entries_of_devices_long_gone_go_with_a_write() {
        let old = r#",{"id":"old","name":"Old","updated_at":1,"tabs":[{"id":"z","url":"https://z.example/"}]}"#;
        let mut all = phone(old);
        // Looking does not write, so the old entry stays and its tab shows.
        assert_eq!(round(&mut all, &me(&[]), NOW).tabs.len(), 3);
        round(&mut all, &me(&["a"]), NOW);
        assert!(!serde_json::to_string(&all).unwrap().contains("\"old\""));
    }
}
