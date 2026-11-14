//! The JSON the saved mixes are kept in on the phone, from one launch to the next.
//!
//! Ported from the iOS client's `SavedMixesJSON` (`MixBankStore.swift`), which writes them as the
//! Android client does (`MixBankStore.kt:74-114`, `MixSnapshot.kt:26-87`): a JSON list of `name`
//! and `snapshot` objects, each snapshot a string holding its own JSON object, under Android's
//! keys and with Android's defaults, so a mix moves between the two clients' code unchanged. Where
//! the text is kept (`UserDefaults`, `SharedPreferences`) stays with each platform.
//!
//! Reading never fails: a list that is missing or cannot be read is no saved mix, and an entry
//! that cannot be read is skipped with every other entry kept, because a saved mix can sit on a
//! phone for weeks and may have been written by an older version of the app.

use super::json::{self, ObjectWriter, Slashes};
use serde_json::{Map, Value};

/// A whole mix as the phone saves it: the four per-channel lists, the group levels, and the master
/// volume, cap and mute.
///
/// Android's `MixSnapshot` (`MixSnapshot.kt:16-25`), with its defaults: no lists, the master and
/// the cap at 100, unmuted. It holds the musician's own levels, never the More of me lift, and
/// nothing the desk announced or the engineer set: a saved mix is a balance, and the channels and
/// groups it is played on are the desk's.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MixSnapshot {
    pub channel_gains: Vec<i64>,
    /// 0 hard left, 50 centre, 100 hard right, one per channel.
    pub channel_pans: Vec<i64>,
    pub channel_mutes: Vec<bool>,
    pub channel_solos: Vec<bool>,
    /// One per group, in the desk's order.
    pub group_levels: Vec<i64>,
    pub volume_percent: i64,
    /// The phone's own cap, never the engineer's limit.
    pub max_level_percent: i64,
    pub muted: bool,
}

impl Default for MixSnapshot {
    fn default() -> Self {
        MixSnapshot {
            channel_gains: Vec::new(),
            channel_pans: Vec::new(),
            channel_mutes: Vec::new(),
            channel_solos: Vec::new(),
            group_levels: Vec::new(),
            volume_percent: 100,
            max_level_percent: 100,
            muted: false,
        }
    }
}

/// A mix the musician saved, under the name they gave it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SavedMix {
    pub name: String,
    pub snapshot: MixSnapshot,
}

/// `mixes` as the phone keeps them (Android's `serializeBanks`).
///
/// Written as the iOS client's `JSONEncoder` wrote them, keys sorted and `/` as itself
/// (`.sortedKeys, .withoutEscapingSlashes`). Android's `org.json` writes the same values with the
/// keys in the order it put them and `/` as `\/`; each client reads the other's.
pub fn encode_saved_mixes(mixes: &[SavedMix]) -> String {
    json::list(mixes.iter().map(|mix| {
        let mut object = ObjectWriter::new(Slashes::Plain);
        object.string("name", &mix.name);
        object.string("snapshot", &encode_snapshot(&mix.snapshot));
        object.finish()
    }))
}

/// The saved mixes `json` holds (Android's `parseBanks`): never fails. A missing or unreadable
/// list is no saved mix, and an entry that is no object, has no text name, or has no text snapshot
/// that reads as one is skipped.
pub fn decode_saved_mixes(json: Option<&str>) -> Vec<SavedMix> {
    let Some(Value::Array(entries)) = json.and_then(json::parse) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let fields = entry.as_object()?;
            let name = fields.get("name")?.as_str()?;
            let snapshot = decode_snapshot(fields.get("snapshot")?.as_str()?)?;
            Some(SavedMix {
                name: name.to_owned(),
                snapshot,
            })
        })
        .collect()
}

/// `snapshot` as its own JSON object (Android's `MixSnapshot.toJson`), keys sorted.
pub fn encode_snapshot(snapshot: &MixSnapshot) -> String {
    // In `JSONEncoder`'s `.sortedKeys` order.
    let mut object = ObjectWriter::new(Slashes::Plain);
    object.integers("channelGains", &snapshot.channel_gains);
    object.booleans("channelMutes", &snapshot.channel_mutes);
    object.integers("channelPans", &snapshot.channel_pans);
    object.booleans("channelSolos", &snapshot.channel_solos);
    object.integers("groupLevels", &snapshot.group_levels);
    object.integer("maxLevelPercent", snapshot.max_level_percent);
    object.boolean("muted", snapshot.muted);
    object.integer("volumePercent", snapshot.volume_percent);
    object.finish()
}

/// The snapshot `json` holds (Android's `MixSnapshot.fromJson`, which throws where this is `None`:
/// when `json` is no object).
///
/// Every value is read as Android's `optInt` and `optBoolean` read it. A list that is missing or
/// not a list is empty; in a list, a number is read as its whole part, a `true` or `false` as
/// itself, and anything else as 0 or false, so a list keeps its length and every channel its
/// place. The master and the cap read as 100 and the mute as false unless they are a number and a
/// `true` or `false`. Android also reads a number or a `true` written as text; this reads text as
/// the default, as the `config` reader does, since neither client ever writes one.
pub fn decode_snapshot(json: &str) -> Option<MixSnapshot> {
    let Some(Value::Object(fields)) = json::parse(json) else {
        return None;
    };
    let defaults = MixSnapshot::default();
    Some(MixSnapshot {
        channel_gains: list(&fields, "channelGains", |value| {
            whole_part(value).unwrap_or(0)
        }),
        channel_pans: list(&fields, "channelPans", |value| {
            whole_part(value).unwrap_or(0)
        }),
        channel_mutes: list(&fields, "channelMutes", |value| {
            value.as_bool().unwrap_or(false)
        }),
        channel_solos: list(&fields, "channelSolos", |value| {
            value.as_bool().unwrap_or(false)
        }),
        group_levels: list(&fields, "groupLevels", |value| {
            whole_part(value).unwrap_or(0)
        }),
        volume_percent: fields
            .get("volumePercent")
            .and_then(whole_part)
            .unwrap_or(defaults.volume_percent),
        max_level_percent: fields
            .get("maxLevelPercent")
            .and_then(whole_part)
            .unwrap_or(defaults.max_level_percent),
        muted: fields
            .get("muted")
            .and_then(Value::as_bool)
            .unwrap_or(defaults.muted),
    })
}

/// The list under `key`, every entry read with `read`; empty when there is none or it is no list.
fn list<T>(fields: &Map<String, Value>, key: &str, read: impl Fn(&Value) -> T) -> Vec<T> {
    fields
        .get(key)
        .and_then(Value::as_array)
        .map(|entries| entries.iter().map(read).collect())
        .unwrap_or_default()
}

/// A number's whole part, or `None` for anything that is no number, a `true` or `false` included.
/// Read through a `Double`, as the iOS client's `JSONSerialization` hands it over, so one whose
/// whole part is not below 2⁶³ (`i64::MAX` itself among them) or below -2⁶³ is `None` too.
fn whole_part(value: &Value) -> Option<i64> {
    let whole = json::number(value)?.trunc();
    let range = (i64::MIN as f64)..(i64::MAX as f64);
    range.contains(&whole).then_some(whole as i64)
}

#[cfg(test)]
mod android_parity {
    //! `MixBankStoreParityTests.swift`: the JSON tests of Android's `MixSnapshotTest.kt` (its
    //! `MixSnapshotTest` and `MixBankStoreJsonTest`) and `GroupSnapshotTest.kt`, one test per Swift
    //! test. Android's `toJson`, `fromJson`, `serializeBanks` and `parseBanks` are
    //! [`encode_snapshot`], [`decode_snapshot`], [`encode_saved_mixes`] and
    //! [`decode_saved_mixes`].

    use super::*;

    /// Android's `MixSnapshotTest.sample()`.
    fn sample() -> MixSnapshot {
        MixSnapshot {
            channel_gains: vec![80, 100, 40],
            channel_pans: vec![0, 50, 100],
            channel_mutes: vec![false, true, false],
            channel_solos: vec![true, false, false],
            volume_percent: 70,
            max_level_percent: 90,
            muted: true,
            ..MixSnapshot::default()
        }
    }

    /// Android's `MixBankStoreJsonTest.snapshot(gain)`.
    fn snapshot(gain: i64) -> MixSnapshot {
        MixSnapshot {
            channel_gains: vec![gain],
            volume_percent: 100,
            ..MixSnapshot::default()
        }
    }

    // MixSnapshotTest

    #[test]
    fn round_trips_through_json_with_every_field_preserved() {
        let original = sample();

        let restored = decode_snapshot(&encode_snapshot(&original)).expect("a snapshot");

        assert_eq!(restored.channel_gains, original.channel_gains);
        assert_eq!(restored.channel_pans, original.channel_pans);
        assert_eq!(restored.channel_mutes, original.channel_mutes);
        assert_eq!(restored.channel_solos, original.channel_solos);
        assert_eq!(restored.volume_percent, 70);
        assert_eq!(restored.max_level_percent, 90);
        assert!(restored.muted);
    }

    // MixBankStoreJsonTest

    #[test]
    fn round_trips_named_banks_through_json() {
        let banks = [
            SavedMix {
                name: "Rehearsal".to_owned(),
                snapshot: snapshot(60),
            },
            SavedMix {
                name: "Show \"A\"".to_owned(),
                snapshot: snapshot(90),
            },
        ];

        let restored = decode_saved_mixes(Some(&encode_saved_mixes(&banks)));

        assert_eq!(restored.len(), 2);
        assert_eq!(restored[0].name, "Rehearsal");
        assert_eq!(restored[0].snapshot.channel_gains.first(), Some(&60));
        assert_eq!(restored[1].name, "Show \"A\"");
        assert_eq!(restored[1].snapshot.channel_gains.first(), Some(&90));
    }

    #[test]
    fn falls_back_to_an_empty_list_for_malformed_json() {
        assert_eq!(decode_saved_mixes(Some("{not json")), []);
    }

    #[test]
    fn falls_back_to_an_empty_list_for_missing_json() {
        assert_eq!(decode_saved_mixes(None), []);
    }

    #[test]
    fn skips_banks_whose_snapshot_is_malformed() {
        assert_eq!(
            decode_saved_mixes(Some(r#"[{"name":"Broken","snapshot":"nope"}]"#)),
            []
        );
    }

    // GroupSnapshotTest

    #[test]
    fn round_trips_group_levels_through_json() {
        let original = MixSnapshot {
            channel_gains: vec![100, 100],
            group_levels: vec![80, 40],
            ..MixSnapshot::default()
        };

        let restored = decode_snapshot(&encode_snapshot(&original)).expect("a snapshot");

        assert_eq!(restored.group_levels, [80, 40]);
    }

    #[test]
    fn a_snapshot_without_group_levels_serializes_an_empty_list() {
        let snapshot = MixSnapshot {
            channel_gains: vec![100],
            ..MixSnapshot::default()
        };

        let restored = decode_snapshot(&encode_snapshot(&snapshot)).expect("a snapshot");

        assert!(restored.group_levels.is_empty());
    }

    #[test]
    fn an_older_bank_with_no_group_levels_reads_as_empty() {
        let legacy = r#"{"channelGains":[100],"volumePercent":70}"#;

        let restored = decode_snapshot(legacy).expect("a snapshot");

        assert_eq!(restored.channel_gains, [100]);
        assert!(restored.group_levels.is_empty());
    }
}

#[cfg(test)]
mod stored_text {
    //! `MixBankStoreTests.swift` (`UserDefaultsMixBankStoreTests`), one test per Swift test, the
    //! JSON half of each: the Swift tests write and read the text through a throwaway
    //! `UserDefaults` suite, which stays the platform's; these pass the text the suite held.

    use super::*;
    use std::collections::BTreeSet;

    /// The JSON object or list `text` holds, or `None`.
    fn object(text: &str) -> Option<Value> {
        serde_json::from_str(text).ok()
    }

    fn keys(value: &Value) -> BTreeSet<&str> {
        value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect()
    }

    /// The text the Swift test leaves in the store is what `UserDefaults.string(forKey:)` reads
    /// back: nothing for a missing value, `Data` or a list, the number's digits for a number.
    #[test]
    fn holds_no_saved_mix_on_the_first_launch_nor_when_what_it_holds_is_no_list() {
        assert_eq!(decode_saved_mixes(None), []);

        for value in ["", "   ", r#"{"name":"Verse"}"#, "42"] {
            assert_eq!(
                decode_saved_mixes(Some(value)),
                [],
                "{value} is no list of mixes"
            );
        }

        assert_eq!(encode_saved_mixes(&[]), "[]");
        assert_eq!(decode_saved_mixes(Some(&encode_saved_mixes(&[]))), []);
    }

    #[test]
    fn what_one_launch_saves_the_next_one_reads_written_as_android_writes_it() {
        let mixes = [
            SavedMix {
                name: "Verse".to_owned(),
                snapshot: MixSnapshot {
                    channel_gains: vec![30, 60],
                    channel_pans: vec![0, 100],
                    channel_mutes: vec![true, false],
                    channel_solos: vec![false, true],
                    group_levels: vec![40],
                    volume_percent: 70,
                    max_level_percent: 90,
                    muted: true,
                },
            },
            SavedMix {
                name: "Show \"A\" / ñ".to_owned(),
                snapshot: MixSnapshot::default(),
            },
        ];

        let stored = encode_saved_mixes(&mixes);

        assert_eq!(decode_saved_mixes(Some(&stored)), mixes);
        // Android's shape (`MixBankStore.kt:74-88`, `MixSnapshot.kt:26-35`): a string holding a
        // list of `name` and `snapshot` objects, each snapshot a string of its own JSON object.
        let list = object(&stored).unwrap();
        let list = list.as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(keys(&list[0]), BTreeSet::from(["name", "snapshot"]));
        assert_eq!(list[0]["name"], "Verse");
        let fields = object(list[0]["snapshot"].as_str().unwrap()).unwrap();
        assert_eq!(
            keys(&fields),
            BTreeSet::from([
                "channelGains",
                "channelPans",
                "channelMutes",
                "channelSolos",
                "groupLevels",
                "volumePercent",
                "maxLevelPercent",
                "muted",
            ])
        );
        assert_eq!(fields["channelGains"], serde_json::json!([30, 60]));
        assert_eq!(fields["channelMutes"], serde_json::json!([true, false]));
        assert_eq!(fields["volumePercent"], 70);
        assert_eq!(fields["muted"], true);
        // Not in the Swift test: the text itself, as the iOS client's `JSONEncoder` wrote it.
        assert_eq!(
            stored,
            r#"[{"name":"Verse","snapshot":"{\"channelGains\":[30,60],\"channelMutes\":[true,false],\"channelPans\":[0,100],\"channelSolos\":[false,true],\"groupLevels\":[40],\"maxLevelPercent\":90,\"muted\":true,\"volumePercent\":70}"},{"name":"Show \"A\" / ñ","snapshot":"{\"channelGains\":[],\"channelMutes\":[],\"channelPans\":[],\"channelSolos\":[],\"groupLevels\":[],\"maxLevelPercent\":100,\"muted\":false,\"volumePercent\":100}"}]"#
        );
    }

    /// Android's `serializeBanks` output for one mix, as its `org.json` writes it.
    #[test]
    fn reads_the_saved_mixes_android_writes() {
        let android = r#"[{"name":"Rehearsal","snapshot":"{\"channelGains\":[60,40],\"channelPans\":[0,100],\"channelMutes\":[false,true],\"channelSolos\":[true,false],\"groupLevels\":[80],\"volumePercent\":70,\"maxLevelPercent\":90,\"muted\":true}"}]"#;

        assert_eq!(
            decode_saved_mixes(Some(android)),
            [SavedMix {
                name: "Rehearsal".to_owned(),
                snapshot: MixSnapshot {
                    channel_gains: vec![60, 40],
                    channel_pans: vec![0, 100],
                    channel_mutes: vec![false, true],
                    channel_solos: vec![true, false],
                    group_levels: vec![80],
                    volume_percent: 70,
                    max_level_percent: 90,
                    muted: true,
                },
            }]
        );
    }

    /// Android skips an entry that is no object, has no text name, or has no snapshot it can
    /// read, and keeps every other (`MixBankStore.kt:94-114`).
    #[test]
    fn a_damaged_entry_loses_only_itself() {
        let stored = r#"[{"name":"Verse","snapshot":"{\"channelGains\":[30,60]}"},
             7,
             null,
             {"name":5,"snapshot":"{}"},
             {"name":"No snapshot"},
             {"name":"Nested","snapshot":{"channelGains":[10]}},
             {"name":"Broken","snapshot":"nope"},
             {"name":"A list","snapshot":"[1,2]"},
             {"name":"Chorus","snapshot":"{\"volumePercent\":70}"}]"#;

        let mixes = decode_saved_mixes(Some(stored));

        assert_eq!(
            mixes
                .iter()
                .map(|mix| mix.name.as_str())
                .collect::<Vec<_>>(),
            ["Verse", "Chorus"]
        );
        assert_eq!(
            mixes[0].snapshot,
            MixSnapshot {
                channel_gains: vec![30, 60],
                ..MixSnapshot::default()
            }
        );
        assert_eq!(
            mixes[1].snapshot,
            MixSnapshot {
                volume_percent: 70,
                ..MixSnapshot::default()
            }
        );
    }

    /// Android reads each value with `optInt` and `optBoolean`: a number is read as its whole part
    /// and anything else as the default, so a list keeps its length and every channel its place.
    /// Android also reads a number or a `true` written as text; this reads text as the default, as
    /// the `config` reader does, since neither client writes one.
    #[test]
    fn reads_a_damaged_value_as_the_default_keeping_every_channel_in_its_place() {
        let stored = r#"[{"name":"Odd","snapshot":"{\"channelGains\":[2.7,-3.9,true,\"5\",null,[1]],\"channelPans\":7,\"channelMutes\":[true,1,\"true\",null],\"channelSolos\":null,\"groupLevels\":[40],\"volumePercent\":\"loud\",\"maxLevelPercent\":55.5,\"muted\":1}"}]"#;

        let mixes = decode_saved_mixes(Some(stored));

        assert_eq!(
            mixes[0].snapshot,
            MixSnapshot {
                channel_gains: vec![2, -3, 0, 0, 0, 0],
                channel_pans: Vec::new(),
                channel_mutes: vec![true, false, false, false],
                channel_solos: Vec::new(),
                group_levels: vec![40],
                volume_percent: 100,
                max_level_percent: 55,
                muted: false,
            }
        );
    }

    /// Not ported: a whole part too large for an `i64` is no number, as the iOS client reads one
    /// (through a `Double`, so `i64::MAX` itself rounds up past the range).
    #[test]
    fn a_number_past_the_range_of_a_whole_number_reads_as_the_default() {
        let snapshot = decode_snapshot(&format!(
            r#"{{"channelGains":[1e20,{},-9.3e18],"volumePercent":1e300}}"#,
            i64::MAX
        ))
        .unwrap();

        assert_eq!(snapshot.channel_gains, [0, 0, 0]);
        assert_eq!(snapshot.volume_percent, 100);
    }
}
