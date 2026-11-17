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
//! it and stops listing it; this device then drops the id. A tab with
//! `later` asks for that ("send to desktop"): it is taken without a click.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use magnetite_core::UnixSeconds;

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
    #[serde(flatten)]
    other: Map<String, Value>,
}

/// The collection as it is stored.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Devices {
    #[serde(default)]
    devices: Vec<Entry>,
    #[serde(flatten)]
    other: Map<String, Value>,
}

/// This device, and the tabs it took over that their devices may still list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Me {
    /// Made up once per install (`SyncSettings::device_id`).
    pub id: String,
    pub name: String,
    pub taken: Vec<String>,
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
    /// Its device sent it here; it is opened without a click.
    #[serde(skip)]
    pub later: bool,
}

/// What a round over the collection found.
#[derive(Debug, PartialEq)]
pub struct Round {
    /// The tabs open on other devices that this one has not taken.
    pub tabs: Vec<RemoteTab>,
    /// `Me::taken` without the tabs no device lists any more.
    pub taken: Vec<String>,
    /// This device's entry differs from the stored one: write `all`.
    pub write: bool,
}

/// Read the other devices' tabs out of `all` and bring this device's entry
/// up to date in it. The entry holds what this device took; a device that
/// took nothing has none.
pub fn round(all: &mut Devices, me: &Me, now: UnixSeconds) -> Round {
    let others = || all.devices.iter().filter(|d| d.id != me.id);
    let taken: Vec<String> = me
        .taken
        .iter()
        .filter(|id| others().any(|d| d.tabs.iter().any(|t| &t.id == *id)))
        .cloned()
        .collect();
    let tabs = others()
        .flat_map(|d| {
            d.tabs
                .iter()
                .filter(|t| !taken.contains(&t.id))
                .map(|t| RemoteTab {
                    device: d.name.clone(),
                    id: t.id.clone(),
                    url: t.url.clone(),
                    title: t.title.clone(),
                    space: t.space.clone(),
                    later: t.later,
                })
        })
        .collect();
    let stored = all
        .devices
        .iter()
        .find(|d| d.id == me.id)
        .map(|d| d.taken.clone())
        .unwrap_or_default();
    let write = stored != taken;
    if write {
        let mine = all.devices.iter().position(|d| d.id == me.id);
        let other = mine
            .map(|i| all.devices.remove(i).other)
            .unwrap_or_default();
        all.devices.retain(|d| now - d.updated_at < GONE_AFTER);
        if !taken.is_empty() {
            all.devices.push(Entry {
                id: me.id.clone(),
                name: me.name.clone(),
                updated_at: now,
                tabs: Vec::new(),
                taken: taken.clone(),
                other,
            });
        }
    }
    Round { tabs, taken, write }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: UnixSeconds = 2_000_000_000;

    fn stored(json: &str) -> Devices {
        serde_json::from_str(json).unwrap()
    }

    fn phone(taken_by_mac: &str) -> Devices {
        stored(&format!(
            r#"{{"devices":[
                {{"id":"phone","name":"iPhone","updated_at":{NOW},"battery":80,"tabs":[
                    {{"id":"a","url":"https://a.example/","title":"A","space":"s1","group":"x"}},
                    {{"id":"b","url":"https://b.example/","title":"B","later":true}}]}}
                {taken_by_mac}]}}"#
        ))
    }

    fn me(taken: &[&str]) -> Me {
        Me {
            id: "mac".into(),
            name: "Mac".into(),
            taken: taken.iter().map(ToString::to_string).collect(),
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
        assert!(round.tabs[1].later && !round.write);
        assert_eq!(all, phone(""));
    }

    #[test]
    fn a_taken_tab_is_written_and_no_longer_listed() {
        let mut all = phone("");
        let round = round(&mut all, &me(&["a"]), NOW);
        assert!(round.write);
        assert_eq!(
            round.tabs.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["b"]
        );
        let json = serde_json::to_value(&all).unwrap();
        assert_eq!(json["devices"][1]["taken"][0], "a");
        // What a newer version wrote in the phone's entry is still there.
        assert_eq!(json["devices"][0]["battery"], 80);
        assert_eq!(json["devices"][0]["tabs"][0]["group"], "x");
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
