//! What tests drive the console with: the sample engine's status and helpers to build payloads.
//! On with the `testing` feature (and in this crate's own tests), never in a build.

use serde_json::{Value, json};

use crate::mix::Mix;
use crate::wire::StatusReport;

/// `base` with every field of `extra` set on it (both JSON objects).
pub fn with_fields(base: Value, extra: Value) -> Value {
    let mut base = base;
    if let (Some(base), Some(extra)) = (base.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
    base
}

/// A musician's row as the sample engine lists it: `identity` and `link` on top of the defaults,
/// and its mix the engine's default with `mix` changes.
fn musician_row(address: &str, identity: Value, link: Value, mix: Value) -> Value {
    let capabilities: Vec<String> = identity
        .get("capabilities")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let has = |capability: &str| capabilities.iter().any(|listed| listed == capability);
    let mut row = json!({
        "address": address,
        "clientName": null,
        "clientId": null,
        "name": null,
        "nameIsLocal": false,
        "capabilities": [],
        "rttMs": null,
        "rttLastMs": null,
        "linkLatest": null,
        "buffer": { "mode": "auto" },
        "redundancy": { "on": false },
        "duplicatesSent": 0,
        "limiter": { "enabled": false, "ceilingDb": 0.0, "releaseMs": 50.0 },
        "fxReturns": { "reverb": 0, "delay": 0 },
        "recovered": 0,
    });
    row = with_fields(row, identity);
    row = with_fields(
        row,
        json!({
            "bufferSupported": has("playback.buffer"),
            "redundancySupported": has("redundancy.recovered"),
            "redundancyHoldSupported": has("redundancy.hold"),
        }),
    );
    let default = serde_json::to_value(Mix::engine_default()).expect("the default mix");
    let mut mixed = with_fields(default, json!({ "revision": 1 }));
    // A table change replaces the table from its first entry and keeps the rest, as the engine
    // merges a mix update.
    if let Some(changes) = mix.as_object() {
        for (field, value) in changes {
            match (mixed.get_mut(field), value.as_array()) {
                (Some(Value::Array(table)), Some(entries)) => {
                    for (index, entry) in entries.iter().enumerate() {
                        table[index] = entry.clone();
                    }
                }
                _ => {
                    mixed[field] = value.clone();
                }
            }
        }
    }
    row = with_fields(row, mixed);
    row = with_fields(row, json!({ "packetsSent": 0, "packetsDiscarded": 0 }));
    row = with_fields(row, link);
    with_fields(row, json!({ "mirror": null }))
}

fn good() -> Value {
    json!({
        "controlConnected": true,
        "linkHealth": "good",
        "packetsPerSecond": 200,
        "underrunsPerSecond": 0,
        "lossPercent": 0.01,
        "receiveTimeoutsPerSecond": 0,
    })
}

fn warning() -> Value {
    json!({
        "controlConnected": true,
        "linkHealth": "warning",
        "packetsPerSecond": 196,
        "underrunsPerSecond": 2,
        "lossPercent": 0.82,
        "receiveTimeoutsPerSecond": 0.5,
    })
}

fn lost() -> Value {
    json!({
        "controlConnected": true,
        "linkHealth": "bad",
        "packetsPerSecond": null,
        "underrunsPerSecond": null,
        "lossPercent": null,
        "receiveTimeoutsPerSecond": null,
    })
}

/// The sample engine's status as JSON (the web console's `sampleStatus`): six inputs on "Demo
/// interface", two groups, four phones (Lead vocal, Bass, Keys, and Drums with its link lost) and
/// the local mix.
pub fn sample_status_json() -> Value {
    let names = [
        ("Kick", "drums"),
        ("Snare", "drums"),
        ("Overheads", "drums"),
        ("Bass", "bass"),
        ("Lead vox", "voice"),
        ("Backing vox", "voice"),
    ];
    let group_of = |index: usize| match index {
        0..=2 => (json!("Drums"), json!("drums")),
        4 | 5 => (json!("Vocals"), json!("vocals")),
        _ => (Value::Null, Value::Null),
    };
    let channels: Vec<Value> = names
        .iter()
        .enumerate()
        .map(|(index, (name, kind))| {
            let (group, group_id) = group_of(index);
            json!({
                "number": index + 1,
                "name": name,
                "kind": kind,
                "hidden": false,
                "link": false,
                "group": group,
                "groupId": group_id,
                "level": 0,
            })
        })
        .collect();
    let no_reports = with_fields(lost(), json!({ "linkHealth": "unknown" }));
    json!({
        "run": 1,
        "deviceName": "Demo interface",
        "captureFormat": { "channels": 6, "sampleRate": 48000, "sampleFormat": "I24", "bufferSize": "Fixed(256)" },
        "controlPort": 50001,
        "sampleRate": 48000,
        "sourceChannels": 6,
        "groups": [
            { "id": "drums", "name": "Drums", "channels": [0, 1, 2], "invalid": false },
            { "id": "vocals", "name": "Vocals", "channels": [4, 5], "invalid": false },
        ],
        "channels": channels,
        "stopped": false,
        "samplesReceived": 0,
        "packetsSent": 0,
        "packetsDiscarded": 0,
        "musicians": [
            musician_row(
                "192.168.1.3:50000",
                json!({
                    "clientName": "Lead vocal",
                    "clientId": "5f0c2a8e-1b7d-4c3a-9e6f-2d8b4a7c1e90",
                    "name": "Lead vocal",
                    "capabilities": ["stats.v2", "playback.buffer"],
                }),
                good(),
                json!({
                    "channelGains": [62, 55, 40, 58, 88, 52],
                    "pans": [50, 50, 50, 50, 50, 40],
                    "volumePercent": 85,
                    "maxLevelPercent": 90,
                }),
            ),
            musician_row(
                "192.168.1.12:50000",
                json!({
                    "clientName": "Bass",
                    "clientId": "a3d9e7b1-6c2f-4e8a-b5d0-7f1c9e3a2b64",
                    "name": "Bass",
                    "capabilities": ["stats.v2", "redundancy.recovered", "redundancy.hold"],
                }),
                warning(),
                json!({}),
            ),
            musician_row(
                "192.168.1.17:50000",
                json!({ "clientName": "Keys", "clientId": "c81e4f2a-9b3d-4a7e-8c6f-0d2b5e9a1f37", "name": "Keys" }),
                no_reports,
                json!({ "pans": [50, 50, 50, 50, 30, 70] }),
            ),
            musician_row(
                "192.168.1.20:50000",
                json!({ "clientId": "e4b7c1d9-2a6f-4d3e-9b8c-5f0a7e2d6c18", "name": "Drums", "nameIsLocal": true }),
                lost(),
                json!({ "channelMuted": [false, false, false, true] }),
            ),
        ],
        "localMix": with_fields(serde_json::to_value(Mix::engine_default()).expect("a mix"), json!({ "revision": 1 })),
        "localOutput": {
            "device": null,
            "firstChannel": null,
            "state": "off",
            "reason": null,
            "underrunFrames": 0,
            "droppedFrames": 0,
            "notice": null,
        },
        "lastTake": null,
        "demo": false,
        "load": { "percent": 9.4, "averagePercent": 8.2, "peakPercent": 14.6, "lateBlocks": 0 },
        "localLimiter": { "enabled": false, "ceilingDb": 0.0, "releaseMs": 50.0 },
        "localFxReturns": { "reverb": 0, "delay": 0 },
    })
}

/// The sample engine's status (see [sample_status_json]).
pub fn sample_status() -> StatusReport {
    serde_json::from_value(sample_status_json()).expect("the sample status reads")
}
