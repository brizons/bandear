//! The desk's control messages, read as leniently as both apps read them.
//!
//! Ported from the iOS client's `ServerMessage.swift`, itself a port of the Android client's
//! `ControlChannel.kt` parsers. Unknown types are kept, never refused: the desk's own contract
//! repeats, message after message, that "a phone ignores any unknown type". Fields added after a
//! message shipped are read with the documented fallback rather than required, so an older desk
//! still drives this client.

use super::json;
use crate::cushion::PlaybackBufferSetting;
use serde_json::{Map, Value};
use std::fmt;

/// What a source channel carries. The desk names eleven (`apps/server/src/protocol.rs:420-424`)
/// and states the client's duty plainly: one that meets a kind it does not know shows it as
/// [`ChannelKind::Other`]. So reading never fails on an unknown kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChannelKind {
    Voice,
    Guitar,
    Bass,
    Keys,
    Drums,
    Percussion,
    Horns,
    Strings,
    Playback,
    Click,
    Other,
}

impl ChannelKind {
    /// Every kind, in the desk's order.
    pub const ALL: [ChannelKind; 11] = [
        ChannelKind::Voice,
        ChannelKind::Guitar,
        ChannelKind::Bass,
        ChannelKind::Keys,
        ChannelKind::Drums,
        ChannelKind::Percussion,
        ChannelKind::Horns,
        ChannelKind::Strings,
        ChannelKind::Playback,
        ChannelKind::Click,
        ChannelKind::Other,
    ];

    /// The kind's name on the wire: lowercase, one word.
    pub fn wire_name(self) -> &'static str {
        match self {
            ChannelKind::Voice => "voice",
            ChannelKind::Guitar => "guitar",
            ChannelKind::Bass => "bass",
            ChannelKind::Keys => "keys",
            ChannelKind::Drums => "drums",
            ChannelKind::Percussion => "percussion",
            ChannelKind::Horns => "horns",
            ChannelKind::Strings => "strings",
            ChannelKind::Playback => "playback",
            ChannelKind::Click => "click",
            ChannelKind::Other => "other",
        }
    }

    /// The kind named `name` on the wire, or `None` for a name this build does not know.
    pub fn from_wire_name(name: &str) -> Option<ChannelKind> {
        Self::ALL.into_iter().find(|kind| kind.wire_name() == name)
    }
}

/// One source channel in the desk's `config`, in channel order: entry 0 is CH 1.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SourceChannel {
    /// What the engineer called the input, at most 32 characters. Empty when it has no name, and
    /// the client then labels it by its number.
    pub name: String,
    pub kind: ChannelKind,
    /// The engineer hid the input: it is in nobody's mix and the client does not show it.
    pub is_hidden: bool,
    /// This channel and the next are one stereo pair. Only the pair's first channel carries it.
    pub is_linked_to_next: bool,
}

impl SourceChannel {
    /// A channel the engineer never touched: no name, kind `other`, shown, not linked.
    pub fn untouched() -> SourceChannel {
        SourceChannel {
            name: String::new(),
            kind: ChannelKind::Other,
            is_hidden: false,
            is_linked_to_next: false,
        }
    }
}

/// One group in the desk's `config`: a stable id, a name, and 0-based source channel indices.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChannelGroup {
    /// The desk's stable id for the group, or `None` from a desk that predates group ids. Groups
    /// are matched by position everywhere, as on Android.
    pub id: Option<String>,
    pub name: String,
    /// The member channels, 0-based, never negative.
    pub channels: Vec<i64>,
}

/// The `config` message: what the desk is capturing, sent as soon as a socket opens and again
/// whenever the device or the channels change (`apps/server/src/control.rs:672,678-690`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeskConfiguration {
    pub source_channels: u8,
    /// The rate the desk named, or `None` when the `config` carried none; every audio datagram
    /// carries the rate that matters.
    pub sample_rate: Option<u32>,
    pub groups: Vec<ChannelGroup>,
    /// Exactly one per source channel, or `None` when the desk sent no details (an older desk),
    /// which a screen shows differently from details that name nothing.
    pub channels: Option<Vec<SourceChannel>>,
}

impl DeskConfiguration {
    /// `MAX_MIX_CHANNELS` (`apps/server/src/mix.rs:9`).
    pub const MAXIMUM_MIX_CHANNELS: usize = 32;
    /// `MAX_GROUPS` (`apps/server/src/mix.rs:10`).
    pub const MAXIMUM_GROUPS: usize = 16;
}

/// A whole mix as the desk reports it. `mix_ack` answers the phone's own `mix` and carries that
/// mix's number; `mix_state` is pushed when the mix changed from elsewhere and never carries one
/// (`apps/server/src/protocol.rs:382-396`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MixReport {
    pub sequence: Option<u64>,
    pub volume_percent: i64,
    pub max_level_percent: i64,
    pub muted: bool,
    pub channel_gains: Vec<i64>,
    pub pans: Vec<i64>,
    pub channel_mutes: Vec<bool>,
    pub channel_solos: Vec<bool>,
    pub group_levels: Vec<i64>,
    /// Caps what the musician hears on top of their own `max_level_percent`; the desk applies the
    /// lower of the two, so a screen should show both and say the lower one wins.
    pub engineer_limit_percent: i64,
    /// One entry per channel, index-aligned with `channel_gains`. A locked channel keeps its
    /// level, pan, mute and solo whatever the phone sends, and its controls should read disabled.
    pub locked: Vec<bool>,
    /// The musician whose mix this one mirrors, or `None` when it follows nobody. While it is set,
    /// the channel and group controls should read disabled.
    pub following: Option<String>,
}

impl MixReport {
    /// What `engineer_limit_percent` holds when the engineer set no limit.
    pub const NO_ENGINEER_LIMIT_PERCENT: i64 = 100;
}

/// An `error` reply. It carries the number of the `mix` it refused, when that mix had one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DeskError {
    pub message: String,
    pub sequence: Option<u64>,
}

/// A control message from the desk.
///
/// Unknown types are kept, never refused: [`ServerMessage::Unrecognized`] makes the ignoring
/// visible, so a diagnostics screen can name what arrived instead of counting it as nothing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ServerMessage {
    Configuration(DeskConfiguration),
    MixAck(MixReport),
    /// A `mix_ack` numbered `sequence` whose mix could not be read. It still answers that number,
    /// as Android's does (`ControlChannel.kt:172-176`), so the phone's mix it answers is not left
    /// waiting for an answer that already came; there is just nothing to show.
    UnreadableMixAck {
        sequence: u64,
    },
    MixState(MixReport),
    PlaybackBuffer(PlaybackBufferSetting),
    RedundancyHold {
        blocks: u8,
    },
    Error(DeskError),
    /// A message of a type this build does not know, named by its `type`.
    Unrecognized {
        message_type: String,
    },
}

impl ServerMessage {
    /// `MAX_REDUNDANCY_DELAY_BLOCKS` (`apps/server/src/redundancy.rs:42`).
    pub const MAXIMUM_REDUNDANCY_HOLD_BLOCKS: u8 = 4;

    /// The message `json` holds, as the iOS client reads one; see each message's reader below for
    /// what is required and what is read leniently. Never panics, whatever the text.
    pub fn parse(json: &str) -> Result<ServerMessage, ServerMessageError> {
        let Some(Value::Object(fields)) = json::parse(json) else {
            return Err(ServerMessageError::NotJson);
        };
        let message_type = match fields.get("type") {
            None | Some(Value::Null) => return Err(ServerMessageError::MissingType),
            Some(Value::String(message_type)) => message_type.as_str(),
            Some(_) => return Err(ServerMessageError::NotJson),
        };

        let message = match message_type {
            "config" => configuration(&fields).map(ServerMessage::Configuration),
            "mix_ack" => mix_ack(&fields),
            // The desk never numbers a push; a number on one answers nothing this phone sent.
            "mix_state" => mix_report(&fields, None).map(ServerMessage::MixState),
            "playback" => playback_buffer(&fields).map(ServerMessage::PlaybackBuffer),
            "redundancy" => {
                redundancy_hold(&fields).map(|blocks| ServerMessage::RedundancyHold { blocks })
            }
            "error" => desk_error(&fields).map(ServerMessage::Error),
            other => Ok(ServerMessage::Unrecognized {
                message_type: other.to_owned(),
            }),
        };
        message.map_err(|fault| fault.in_message(message_type))
    }
}

/// A JSON object's fields.
type Fields = Map<String, Value>;

/// Why a known message could not be read, before it is named by its type.
#[derive(Debug)]
enum Fault {
    /// A field it cannot do without is absent or `null`.
    Missing(&'static str),
    /// A field holds a value this client cannot use.
    Unusable(&'static str),
    /// A field holds a value of the wrong type: `JSONDecoder`'s type mismatches and numbers that do
    /// not fit, which the iOS client reported as not JSON.
    NotJson,
}

impl Fault {
    fn in_message(self, message_type: &str) -> ServerMessageError {
        match self {
            Fault::Missing(field) => ServerMessageError::MissingField {
                field: field.to_owned(),
                message_type: message_type.to_owned(),
            },
            Fault::Unusable(field) => ServerMessageError::UnusableField {
                field: field.to_owned(),
                message_type: message_type.to_owned(),
            },
            Fault::NotJson => ServerMessageError::NotJson,
        }
    }
}

/// A field the message cannot be read without (`container.decode`): absent or `null` is missing,
/// and a value `read` cannot read is the wrong type.
fn required<'a, T>(
    fields: &'a Fields,
    key: &'static str,
    read: impl FnOnce(&'a Value) -> Option<T>,
) -> Result<T, Fault> {
    match fields.get(key) {
        None | Some(Value::Null) => Err(Fault::Missing(key)),
        Some(value) => read(value).ok_or(Fault::NotJson),
    }
}

/// A field that may be left out (`try container.decodeIfPresent`): absent or `null` is none, and a
/// value `read` cannot read is still the wrong type.
fn optional<'a, T>(
    fields: &'a Fields,
    key: &'static str,
    read: impl FnOnce(&'a Value) -> Option<T>,
) -> Result<Option<T>, Fault> {
    match fields.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => read(value).map(Some).ok_or(Fault::NotJson),
    }
}

/// A field read leniently (`try? container.decodeIfPresent`): anything `read` cannot read is
/// none, and costs nothing else.
fn lenient<'a, T>(
    fields: &'a Fields,
    key: &str,
    read: impl FnOnce(&'a Value) -> Option<T>,
) -> Option<T> {
    fields.get(key).and_then(read)
}

/// A `config`, read as leniently as the Android client reads it (`ControlChannel.kt:158-167`,
/// `parseGroups` at `:642-658`, `parseChannelDetails` at `:669-680`). The desk added fields over
/// time (the channel details after the groups, a group's `id` after its name and channels), so a
/// phone that insisted on every field would lose the whole configuration to an older desk.
///
/// - `source_channels` is the one field it cannot do without.
/// - `sample_rate` is read when present; Android never reads it, and every audio datagram carries
///   the rate that matters.
/// - `groups`: anything but a list is none; an entry that is not an object or has no name is
///   skipped; `id` is optional; a member channel that does not read as a whole number, or is
///   negative, is dropped. Android's `optString` and `optInt` coerce where this drops (a name of
///   `7` becomes "7", a channel of `2.5` or "3" becomes 2 or 3); the desk only ever sends names and
///   whole numbers, so this is the one place the two read garbage differently.
/// - `channels`: anything but a list is no details at all (an older desk), which is not the same
///   as empty details; a list becomes exactly `source_channels` entries, an entry that is not an
///   object or a field of the wrong type reading as the default, an unknown kind as `other`.
fn configuration(fields: &Fields) -> Result<DeskConfiguration, Fault> {
    let count = required(fields, "source_channels", json::byte)?;
    let groups = lenient(fields, "groups", Value::as_array)
        .map(|entries| entries.iter().filter_map(group).collect())
        .unwrap_or_default();
    let channels = lenient(fields, "channels", Value::as_array).map(|entries| {
        (0..usize::from(count))
            .map(|index| {
                entries
                    .get(index)
                    .and_then(source_channel)
                    .unwrap_or_else(SourceChannel::untouched)
            })
            .collect()
    });
    Ok(DeskConfiguration {
        source_channels: count,
        sample_rate: lenient(fields, "sample_rate", json::unsigned_32),
        groups,
        channels,
    })
}

/// One `groups` entry, or `None` for one that is not an object or has no name.
fn group(entry: &Value) -> Option<ChannelGroup> {
    let fields = entry.as_object()?;
    let name = lenient(fields, "name", Value::as_str).filter(|name| !name.is_empty())?;
    let channels = lenient(fields, "channels", Value::as_array)
        .map(|members| {
            members
                .iter()
                .filter_map(json::integer)
                .filter(|&channel| channel >= 0)
                .collect()
        })
        .unwrap_or_default();
    Some(ChannelGroup {
        id: lenient(fields, "id", Value::as_str).map(str::to_owned),
        name: name.to_owned(),
        channels,
    })
}

/// One `channels` entry, or `None` for one that is not an object (read as untouched).
fn source_channel(entry: &Value) -> Option<SourceChannel> {
    let fields = entry.as_object()?;
    Some(SourceChannel {
        name: lenient(fields, "name", Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        // An unknown kind is shown as `other`, which is the desk's own contract.
        kind: lenient(fields, "kind", Value::as_str)
            .and_then(ChannelKind::from_wire_name)
            .unwrap_or(ChannelKind::Other),
        is_hidden: lenient(fields, "hidden", Value::as_bool) == Some(true),
        is_linked_to_next: lenient(fields, "link", Value::as_bool) == Some(true),
    })
}

/// A `mix_ack`, read as Android reads one (`ControlChannel.kt:169-177`). A numbered ack whose mix
/// cannot be read still answers its number, as [`ServerMessage::UnreadableMixAck`].
///
/// An ack with no number whose mix cannot be read is refused like any unreadable report. Android
/// never reads the mix of an unnumbered ack and only counts it as acknowledged; this client has no
/// message for "acknowledged, nothing to read", and the desk numbers the answer to every numbered
/// `mix` (`apps/server/src/control.rs:2530-2570`), so that difference is named here rather than
/// modelled.
///
/// The number (`seq`) is read as Android's `answeredSequence` reads it (`ControlChannel.kt:631`):
/// a number that is not negative names one, and anything else names none without costing the
/// message. Android cuts a fraction off (`1.5` names 1) where this names none; the desk only ever
/// echoes the whole number the phone sent.
fn mix_ack(fields: &Fields) -> Result<ServerMessage, Fault> {
    let sequence = lenient(fields, "seq", json::unsigned);
    match (mix_report(fields, sequence), sequence) {
        (Ok(report), _) => Ok(ServerMessage::MixAck(report)),
        (Err(_), Some(sequence)) => Ok(ServerMessage::UnreadableMixAck { sequence }),
        (Err(fault), None) => Err(fault),
    }
}

/// A whole mix report (`mix_ack`, `mix_state`) numbered `sequence`, read as Android's
/// `parseMixReport` and `parseEngineerControls` read one (`ControlChannel.kt:601-628`).
///
/// - The mix is required whole: the master volume, the level cap, the mute and all five lists, read
///   in that order. The desk always sends every field, so a report missing one, or with an entry of
///   the wrong type, is unreadable: a push is refused and never applied, and a numbered ack answers
///   its number with nothing to show. Android's `getInt` and `getBoolean` coerce where this refuses
///   (a level of `2.5` or `"3"`, a mute of `"true"`); the desk never sends either.
/// - The engineer's controls came later and are read leniently, so a desk that predates them still
///   drives this client. `engineer_limit_percent` is any number, held to 0-100 and cut to its whole
///   part, and anything else is no limit; `locked` locks the entries that are `true` and no other,
///   and anything but a list locks nothing; `following` is a string, an empty one still a leader,
///   and anything else is nobody.
fn mix_report(fields: &Fields, sequence: Option<u64>) -> Result<MixReport, Fault> {
    let volume_percent = required(fields, "volume_percent", json::integer)?;
    let max_level_percent = required(fields, "max_level_percent", json::integer)?;
    let muted = required(fields, "muted", Value::as_bool)?;
    let channel_gains = required(fields, "channels", json::integers)?;
    let pans = required(fields, "pans", json::integers)?;
    let channel_mutes = required(fields, "mutes", json::booleans)?;
    let channel_solos = required(fields, "solos", json::booleans)?;
    let group_levels = required(fields, "group_levels", json::integers)?;

    // Any number, as Kotlin's `Number.toInt()` takes any number, a fraction too; held to the range
    // before it becomes a whole number (JSON has no NaN).
    let engineer_limit_percent = lenient(fields, "engineer_limit_percent", json::number)
        .map(|limit| limit.clamp(0.0, 100.0) as i64)
        .unwrap_or(MixReport::NO_ENGINEER_LIMIT_PERCENT);
    let locked = lenient(fields, "locked", Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .map(|entry| entry.as_bool() == Some(true))
                .collect()
        })
        // A desk that predates the locks sends none: nothing locked.
        .unwrap_or_else(|| vec![false; channel_gains.len()]);
    let following = lenient(fields, "following", Value::as_str).map(str::to_owned);

    Ok(MixReport {
        sequence,
        volume_percent,
        max_level_percent,
        muted,
        channel_gains,
        pans,
        channel_mutes,
        channel_solos,
        group_levels,
        engineer_limit_percent,
        locked,
        following,
    })
}

/// A `playback` message's buffer. One this phone cannot honour is refused, so the session keeps
/// the one it had, as Android does (`parsePlaybackBuffer`, `ControlChannel.kt:689`). The buffer's
/// `mode` is required and read before `ms`, and an `ms` of the wrong type is refused whatever the
/// mode, as the iOS client's decoder did.
fn playback_buffer(fields: &Fields) -> Result<PlaybackBufferSetting, Fault> {
    let buffer = required(fields, "buffer", Value::as_object)?;
    let mode = required(buffer, "mode", Value::as_str)?;
    let milliseconds = optional(buffer, "ms", json::integer)?;

    match mode {
        "auto" => Ok(PlaybackBufferSetting::Auto),
        "fixed" => {
            let milliseconds = milliseconds.ok_or(Fault::Missing("ms"))?;
            let allowed = i64::from(PlaybackBufferSetting::MINIMUM_FIXED_MILLISECONDS)
                ..=i64::from(PlaybackBufferSetting::MAXIMUM_FIXED_MILLISECONDS);
            u16::try_from(milliseconds)
                .ok()
                .filter(|_| allowed.contains(&milliseconds))
                .map(|milliseconds| PlaybackBufferSetting::Fixed { milliseconds })
                .ok_or(Fault::Unusable("ms"))
        }
        _ => Err(Fault::Unusable("mode")),
    }
}

/// A `redundancy` message's hold. A delay this phone cannot hold is refused, so the session keeps
/// the hold it had, as Android does (`parseRedundancyHold`, `ControlChannel.kt:708`).
fn redundancy_hold(fields: &Fields) -> Result<u8, Fault> {
    let blocks = required(fields, "delay_blocks", json::integer)?;
    u8::try_from(blocks)
        .ok()
        .filter(|&blocks| blocks <= ServerMessage::MAXIMUM_REDUNDANCY_HOLD_BLOCKS)
        .ok_or(Fault::Unusable("delay_blocks"))
}

/// An `error` reply: its message is required, its number optional but never of the wrong type.
fn desk_error(fields: &Fields) -> Result<DeskError, Fault> {
    Ok(DeskError {
        message: required(fields, "message", Value::as_str)?.to_owned(),
        sequence: optional(fields, "seq", json::unsigned)?,
    })
}

/// Why a message from the desk could not be read. An unknown `type` is not here: it is
/// [`ServerMessage::Unrecognized`], a message this build ignores, not a failure.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ServerMessageError {
    /// Not a JSON object this client can route on, or a known message with a field of the wrong
    /// type.
    NotJson,
    /// A JSON object with no `type`, so there is nothing to route it by.
    MissingType,
    /// A known message type missing a field it cannot be read without, or holding `null` there.
    MissingField { field: String, message_type: String },
    /// A known message type whose field holds a value this client cannot use, so the message is
    /// ignored and what it would have changed stays as it was.
    UnusableField { field: String, message_type: String },
}

impl fmt::Display for ServerMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerMessageError::NotJson => {
                write!(formatter, "not a control message this client reads")
            }
            ServerMessageError::MissingType => write!(formatter, "a control message with no type"),
            ServerMessageError::MissingField {
                field,
                message_type,
            } => write!(formatter, "a `{message_type}` message with no `{field}`"),
            ServerMessageError::UnusableField {
                field,
                message_type,
            } => write!(
                formatter,
                "a `{message_type}` message whose `{field}` cannot be used"
            ),
        }
    }
}

impl std::error::Error for ServerMessageError {}

#[cfg(test)]
mod tests {
    //! `ServerMessageTests.swift`, one test per Swift test; a test the Swift file runs once per
    //! argument runs its arguments in a loop here.

    use super::*;
    use serde_json::json;

    fn decode(json: &str) -> Result<ServerMessage, ServerMessageError> {
        ServerMessage::parse(json)
    }

    fn configuration(json: &str) -> DeskConfiguration {
        match decode(json) {
            Ok(ServerMessage::Configuration(configuration)) => configuration,
            other => panic!("expected a configuration, got {other:?}"),
        }
    }

    /// The groups a `config` carrying `groups` (as raw JSON) is read with.
    fn config_groups(groups: &str) -> Vec<ChannelGroup> {
        configuration(&format!(
            r#"{{"type":"config","source_channels":4,"groups":{groups}}}"#
        ))
        .groups
    }

    fn mix_ack(json: &str) -> MixReport {
        match decode(json) {
            Ok(ServerMessage::MixAck(report)) => report,
            other => panic!("expected a mix ack, got {other:?}"),
        }
    }

    pub(super) fn mix_state(json: &str) -> MixReport {
        match decode(json) {
            Ok(ServerMessage::MixState(report)) => report,
            other => panic!("expected a mix state, got {other:?}"),
        }
    }

    pub(super) fn missing(field: &str, message_type: &str) -> ServerMessageError {
        ServerMessageError::MissingField {
            field: field.to_owned(),
            message_type: message_type.to_owned(),
        }
    }

    fn unusable(field: &str, message_type: &str) -> ServerMessageError {
        ServerMessageError::UnusableField {
            field: field.to_owned(),
            message_type: message_type.to_owned(),
        }
    }

    fn channel(
        name: &str,
        kind: ChannelKind,
        is_hidden: bool,
        is_linked_to_next: bool,
    ) -> SourceChannel {
        SourceChannel {
            name: name.to_owned(),
            kind,
            is_hidden,
            is_linked_to_next,
        }
    }

    /// A whole `mix_ack` with two channels and one group, as the desk writes it
    /// (`apps/server/src/protocol.rs:382-396`).
    const MIX_ACK_JSON: &str = r#"{"type":"mix_ack","seq":7,"volume_percent":80,"max_level_percent":95,"muted":false,
     "channels":[10,20],"pans":[0,100],"mutes":[false,true],"solos":[true,false],
     "group_levels":[30],"engineer_limit_percent":90,"locked":[false,true],"following":null}"#;

    /// A whole one-channel `mix_state`, every field present, written on one line so a test can
    /// replace one field's text.
    const WHOLE_MIX_STATE: &str = r#"{"type":"mix_state","volume_percent":50,"max_level_percent":100,"muted":false,"channels":[5],"pans":[50],"mutes":[false],"solos":[false],"group_levels":[],"engineer_limit_percent":100,"locked":[false],"following":null}"#;

    // config

    #[test]
    fn reads_the_whole_config_the_desk_sends_on_connect() {
        let config = configuration(
            r#"{"type":"config","source_channels":2,"sample_rate":48000,
             "groups":[{"id":"g1","name":"Drums","channels":[0,1]}],
             "channels":[{"name":"Lead","kind":"voice","hidden":false,"link":true},
                         {"name":"","kind":"guitar","hidden":true,"link":false}]}"#,
        );

        assert_eq!(config.source_channels, 2);
        assert_eq!(config.sample_rate, Some(48_000));
        assert_eq!(
            config.groups,
            [ChannelGroup {
                id: Some("g1".to_owned()),
                name: "Drums".to_owned(),
                channels: vec![0, 1],
            }]
        );
        assert_eq!(
            config.channels,
            Some(vec![
                channel("Lead", ChannelKind::Voice, false, true),
                channel("", ChannelKind::Guitar, true, false),
            ])
        );
    }

    #[test]
    fn reads_every_kind_the_desk_can_name() {
        for kind in ChannelKind::ALL {
            let config = configuration(&format!(
                r#"{{"type":"config","source_channels":1,"sample_rate":48000,"groups":[],
                 "channels":[{{"name":"CH 1","kind":"{}","hidden":false,"link":false}}]}}"#,
                kind.wire_name()
            ));

            assert_eq!(config.channels.unwrap()[0].kind, kind);
        }
    }

    /// The desk's own contract: "one that meets a kind it does not know shows it as `other`"
    /// (`apps/server/src/protocol.rs:420-424`).
    #[test]
    fn shows_a_kind_it_does_not_know_as_other() {
        let config = configuration(
            r#"{"type":"config","source_channels":1,"sample_rate":48000,"groups":[],
             "channels":[{"name":"CH 1","kind":"theremin","hidden":false,"link":false}]}"#,
        );

        assert_eq!(config.channels.unwrap()[0].kind, ChannelKind::Other);
    }

    #[test]
    fn ignores_a_field_it_has_never_heard_of() {
        let config = configuration(
            r#"{"type":"config","source_channels":1,"sample_rate":48000,"groups":[],"channels":[],
             "something_added_later":{"nested":true}}"#,
        );

        assert_eq!(config.source_channels, 1);
    }

    /// An absent `channels` is an older desk's, and it is not the same as an empty list: with no
    /// details a strip shows no kind icon, with details every strip shows one, `other` at worst.
    #[test]
    fn reads_a_config_with_no_groups_as_none_and_with_no_channel_details_as_none_at_all() {
        let config = configuration(r#"{"type":"config","source_channels":0,"sample_rate":48000}"#);

        assert!(config.groups.is_empty());
        assert_eq!(config.channels, None);
    }

    /// The Android client never reads `sample_rate` from a `config`, and its own tests send
    /// configs without one. The rate that matters is in every audio datagram's header.
    #[test]
    fn reads_a_config_with_no_sample_rate() {
        let config = configuration(r#"{"type":"config","source_channels":2}"#);

        assert_eq!(config.sample_rate, None);
        assert_eq!(config.source_channels, 2);
    }

    #[test]
    fn refuses_a_config_that_does_not_say_how_many_channels_it_describes() {
        assert_eq!(
            decode(r#"{"type":"config","groups":[]}"#),
            Err(missing("source_channels", "config"))
        );
    }

    // config groups, read as leniently as the Android client's parseGroups

    /// Rules from `parseGroups` (`ControlChannel.kt:642-658`), which has no test of its own on
    /// Android: a group's `id` came after its name and channels on the desk, so an older desk
    /// sends none; an entry that is not an object, or has no name, is skipped; a channel that is
    /// not a whole number, or is negative, is dropped. One bad group never costs the musician the
    /// others.
    #[test]
    fn keeps_a_group_from_a_desk_that_predates_group_ids() {
        assert_eq!(
            config_groups(r#"[{"name":"Band","channels":[0,1]}]"#),
            [ChannelGroup {
                id: None,
                name: "Band".to_owned(),
                channels: vec![0, 1],
            }]
        );
    }

    #[test]
    fn skips_a_group_that_is_not_an_object_or_has_no_name() {
        for groups in [
            r#"["not a group"]"#,
            r#"[{"channels":[0]}]"#,
            r#"[{"name":"","channels":[0]}]"#,
            r#"[{"name":7}]"#,
        ] {
            assert!(config_groups(groups).is_empty(), "{groups}");
        }
    }

    #[test]
    fn keeps_the_groups_around_one_it_has_to_skip() {
        let groups =
            config_groups(r#"[{"name":"A","channels":[0]},"junk",{"name":"B","channels":[1]}]"#);

        assert_eq!(
            groups
                .iter()
                .map(|group| group.name.as_str())
                .collect::<Vec<_>>(),
            ["A", "B"]
        );
    }

    #[test]
    fn drops_a_group_channel_that_is_not_a_whole_number_or_is_negative() {
        let groups = config_groups(r#"[{"name":"A","channels":[0,"one",-1,2.5,3,null]}]"#);

        assert_eq!(
            groups
                .into_iter()
                .map(|group| group.channels)
                .collect::<Vec<_>>(),
            [vec![0, 3]]
        );
    }

    #[test]
    fn reads_a_group_with_no_channels_as_an_empty_one_and_groups_that_are_not_a_list_as_none() {
        assert_eq!(
            config_groups(r#"[{"name":"A"}]"#)
                .into_iter()
                .map(|group| group.channels)
                .collect::<Vec<_>>(),
            [Vec::<i64>::new()]
        );
        assert!(config_groups(r#""not a list""#).is_empty());
    }

    #[test]
    fn knows_the_mix_sizes_the_desk_works_in() {
        assert_eq!(DeskConfiguration::MAXIMUM_MIX_CHANNELS, 32);
        assert_eq!(DeskConfiguration::MAXIMUM_GROUPS, 16);
    }

    // mix_ack and mix_state

    #[test]
    fn reads_an_ack_as_the_answer_to_its_own_numbered_mix() {
        let report = mix_ack(MIX_ACK_JSON);

        assert_eq!(report.sequence, Some(7));
        assert_eq!(report.volume_percent, 80);
        assert_eq!(report.max_level_percent, 95);
        assert!(!report.muted);
        assert_eq!(report.channel_gains, [10, 20]);
        assert_eq!(report.pans, [0, 100]);
        assert_eq!(report.channel_mutes, [false, true]);
        assert_eq!(report.channel_solos, [true, false]);
        assert_eq!(report.group_levels, [30]);
        assert_eq!(report.engineer_limit_percent, 90);
        assert_eq!(report.locked, [false, true]);
        assert_eq!(report.following, None);
    }

    #[test]
    fn reads_a_pushed_state_which_never_carries_a_number() {
        let report = mix_state(
            r#"{"type":"mix_state","volume_percent":50,"max_level_percent":100,"muted":true,
             "channels":[5],"pans":[50],"mutes":[false],"solos":[false],"group_levels":[],
             "engineer_limit_percent":100,"locked":[false],"following":"Ana"}"#,
        );

        assert_eq!(report.sequence, None);
        assert!(report.muted);
        assert_eq!(report.following.as_deref(), Some("Ana"));
    }

    /// `engineer_limit_percent`, `locked` and `following` were all added after the rest, so a desk
    /// that predates one of them sends a report without it. 100 is the documented "no limit".
    #[test]
    fn reads_a_report_from_a_desk_that_predates_the_engineers_controls() {
        let report = mix_ack(
            r#"{"type":"mix_ack","volume_percent":80,"max_level_percent":100,"muted":false,
             "channels":[10,20,30],"pans":[50,50,50],"mutes":[false,false,false],
             "solos":[false,false,false],"group_levels":[]}"#,
        );

        assert_eq!(MixReport::NO_ENGINEER_LIMIT_PERCENT, 100);
        assert_eq!(report.engineer_limit_percent, 100);
        assert_eq!(report.locked, [false, false, false]);
        assert_eq!(report.following, None);
    }

    #[test]
    fn refuses_a_report_with_no_master_volume() {
        assert_eq!(
            decode(r#"{"type":"mix_ack","max_level_percent":100,"muted":false}"#),
            Err(missing("volume_percent", "mix_ack"))
        );
    }

    // mix reports, read as the Android client's parseMixReport and parseEngineerControls

    /// `parseMixReport` reads each of the five lists with `getJSONArray`, which throws when one is
    /// missing (`ControlChannel.kt:601-613`): the desk always sends the whole mix.
    #[test]
    fn refuses_a_pushed_mix_missing_any_of_its_lists() {
        for list in ["channels", "pans", "mutes", "solos", "group_levels"] {
            let mut object: Value = serde_json::from_str(WHOLE_MIX_STATE).unwrap();
            object.as_object_mut().unwrap().remove(list);

            assert!(decode(&object.to_string()).is_err(), "{list}");
        }
    }

    #[test]
    fn refuses_a_pushed_mix_with_an_entry_of_the_wrong_type_in_a_list() {
        assert!(
            decode(&WHOLE_MIX_STATE.replace(r#""mutes":[false]"#, r#""mutes":[null]"#)).is_err()
        );
    }

    /// Android: "An ack whose mix cannot be read still answers its number"
    /// (`ControlChannel.kt:174`).
    #[test]
    fn answers_the_number_of_an_ack_whose_mix_cannot_be_read() {
        for json in [
            r#"{"type":"mix_ack","seq":12,"channels":[100]}"#,
            r#"{"type":"mix_ack","seq":12,"volume_percent":"loud"}"#,
        ] {
            assert_eq!(
                decode(json),
                Ok(ServerMessage::UnreadableMixAck { sequence: 12 }),
                "{json}"
            );
        }
    }

    /// `answeredSequence` (`ControlChannel.kt:631`) names a mix only with a number that is not
    /// negative; anything else names none, and the ack still acknowledges.
    #[test]
    fn reads_an_ack_whose_number_is_not_a_whole_number_that_is_not_negative_as_one_with_none() {
        for seq in [
            r#""seq":-1,"#,
            r#""seq":"12","#,
            r#""seq":1.5,"#,
            r#""seq":null,"#,
        ] {
            let json = WHOLE_MIX_STATE.replace(
                r#""type":"mix_state","#,
                &format!(r#""type":"mix_ack",{seq}"#),
            );

            let report = mix_ack(&json);
            assert_eq!(report.sequence, None, "{seq}");
            assert_eq!(report.channel_gains, [5]);
        }
    }

    #[test]
    fn ignores_whatever_number_a_pushed_mix_carries() {
        for seq in [r#""seq":7,"#, r#""seq":"x","#] {
            let json = WHOLE_MIX_STATE.replace(
                r#""type":"mix_state","#,
                &format!(r#""type":"mix_state",{seq}"#),
            );

            assert_eq!(mix_state(&json).sequence, None, "{seq}");
        }
    }

    /// Kotlin's `Number.toInt()` cuts a fraction toward zero before `coerceIn(0, 100)`; a boolean
    /// is not a `Number`, so it is no limit.
    #[test]
    fn reads_a_limit_with_a_fraction_as_its_whole_part_and_a_limit_that_is_not_a_number_as_none() {
        for (limit, read) in [
            ("60.9", 60),
            ("-0.5", 0),
            ("100.5", 100),
            ("true", 100),
            ("null", 100),
        ] {
            let json = WHOLE_MIX_STATE.replace(
                r#""engineer_limit_percent":100"#,
                &format!(r#""engineer_limit_percent":{limit}"#),
            );

            assert_eq!(mix_state(&json).engineer_limit_percent, read, "{limit}");
        }
    }

    #[test]
    fn reads_a_locked_list_that_is_not_a_list_as_nothing_locked() {
        let json = WHOLE_MIX_STATE.replace(r#""locked":[false]"#, r#""locked":{"0":true}"#);

        assert_eq!(mix_state(&json).locked, [false]);
    }

    // playback

    #[test]
    fn reads_the_desk_handing_the_cushion_back_to_the_phone() {
        assert_eq!(
            decode(r#"{"type":"playback","buffer":{"mode":"auto"}}"#),
            Ok(ServerMessage::PlaybackBuffer(PlaybackBufferSetting::Auto))
        );
    }

    #[test]
    fn reads_a_cushion_the_desk_pinned() {
        assert_eq!(
            decode(r#"{"type":"playback","buffer":{"mode":"fixed","ms":40}}"#),
            Ok(ServerMessage::PlaybackBuffer(
                PlaybackBufferSetting::Fixed { milliseconds: 40 }
            ))
        );
    }

    #[test]
    fn reads_the_ends_of_the_cushion_the_desk_allows() {
        for milliseconds in [5_u16, 500] {
            assert_eq!(
                decode(&format!(
                    r#"{{"type":"playback","buffer":{{"mode":"fixed","ms":{milliseconds}}}}}"#
                )),
                Ok(ServerMessage::PlaybackBuffer(
                    PlaybackBufferSetting::Fixed { milliseconds }
                ))
            );
        }
        assert_eq!(PlaybackBufferSetting::MINIMUM_FIXED_MILLISECONDS, 5);
        assert_eq!(PlaybackBufferSetting::MAXIMUM_FIXED_MILLISECONDS, 500);
    }

    /// Android ignores a buffer it cannot honour and keeps the one it had (`parsePlaybackBuffer`,
    /// `ControlChannel.kt:689`); refusing the message is how this reader asks for the same.
    #[test]
    fn refuses_a_pinned_cushion_outside_the_range_the_desk_allows() {
        for milliseconds in [4, 501, -1, 0] {
            assert_eq!(
                decode(&format!(
                    r#"{{"type":"playback","buffer":{{"mode":"fixed","ms":{milliseconds}}}}}"#
                )),
                Err(unusable("ms", "playback")),
                "{milliseconds}"
            );
        }
    }

    #[test]
    fn refuses_a_pinned_cushion_with_no_length() {
        assert_eq!(
            decode(r#"{"type":"playback","buffer":{"mode":"fixed"}}"#),
            Err(missing("ms", "playback"))
        );
    }

    #[test]
    fn refuses_a_buffer_mode_this_build_does_not_know() {
        assert_eq!(
            decode(r#"{"type":"playback","buffer":{"mode":"elastic","ms":40}}"#),
            Err(unusable("mode", "playback"))
        );
    }

    #[test]
    fn refuses_a_playback_message_with_no_buffer_at_all() {
        assert_eq!(
            decode(r#"{"type":"playback"}"#),
            Err(missing("buffer", "playback"))
        );
    }

    // redundancy

    #[test]
    fn reads_how_long_to_hold_packets_back() {
        assert_eq!(
            decode(r#"{"type":"redundancy","delay_blocks":2}"#),
            Ok(ServerMessage::RedundancyHold { blocks: 2 })
        );
        assert_eq!(
            decode(r#"{"type":"redundancy","delay_blocks":0}"#),
            Ok(ServerMessage::RedundancyHold { blocks: 0 })
        );
    }

    #[test]
    fn reads_the_longest_delay_the_desk_can_ask_for() {
        assert_eq!(ServerMessage::MAXIMUM_REDUNDANCY_HOLD_BLOCKS, 4);
        assert_eq!(
            decode(r#"{"type":"redundancy","delay_blocks":4}"#),
            Ok(ServerMessage::RedundancyHold { blocks: 4 })
        );
    }

    /// Android ignores a delay it cannot hold and keeps the hold it had (`parseRedundancyHold`,
    /// `ControlChannel.kt:708`).
    #[test]
    fn refuses_a_delay_outside_the_blocks_the_desk_can_ask_for() {
        for blocks in [-1, 5, 9] {
            assert_eq!(
                decode(&format!(
                    r#"{{"type":"redundancy","delay_blocks":{blocks}}}"#
                )),
                Err(unusable("delay_blocks", "redundancy")),
                "{blocks}"
            );
        }
    }

    #[test]
    fn refuses_a_redundancy_message_with_no_delay() {
        assert_eq!(
            decode(r#"{"type":"redundancy"}"#),
            Err(missing("delay_blocks", "redundancy"))
        );
    }

    // error

    #[test]
    fn reads_an_error_that_answers_nothing_in_particular() {
        assert_eq!(
            decode(r#"{"type":"error","message":"no such channel"}"#),
            Ok(ServerMessage::Error(DeskError {
                message: "no such channel".to_owned(),
                sequence: None,
            }))
        );
    }

    #[test]
    fn reads_the_number_of_the_mix_an_error_refused() {
        assert_eq!(
            decode(r#"{"type":"error","message":"locked","seq":7}"#),
            Ok(ServerMessage::Error(DeskError {
                message: "locked".to_owned(),
                sequence: Some(7),
            }))
        );
    }

    // routing

    /// The desk's contract, stated for message after message: "a phone ignores any unknown type".
    /// Keeping the name rather than returning nothing means a diagnostics screen can say what
    /// arrived instead of silently counting it as nothing.
    #[test]
    fn keeps_the_name_of_a_type_it_does_not_know_instead_of_failing() {
        assert_eq!(
            decode(r#"{"type":"something_new","payload":{"a":1}}"#),
            Ok(ServerMessage::Unrecognized {
                message_type: "something_new".to_owned(),
            })
        );
    }

    #[test]
    fn refuses_something_that_is_not_json_at_all() {
        assert_eq!(decode("not json at all"), Err(ServerMessageError::NotJson));
    }

    #[test]
    fn refuses_a_message_with_no_type_to_route_on() {
        for json in [r#"{}"#, r#"{"message":"hi"}"#] {
            assert_eq!(decode(json), Err(ServerMessageError::MissingType), "{json}");
        }
    }

    // Not ported: the edges the iOS decoder was read off for this port (`JSONDecoder` on the Mac
    // the iOS client is built on), pinned so the port keeps answering them the same way.

    #[test]
    fn reads_every_edge_as_the_ios_decoder_reads_it() {
        let report = r#""volume_percent":1,"max_level_percent":1,"muted":false,"channels":[1],"pans":[1],"mutes":[false],"solos":[false],"group_levels":[]"#;
        let cases: Vec<(String, Result<ServerMessage, ServerMessageError>)> = vec![
            (r#"{"type":null}"#.into(), Err(ServerMessageError::MissingType)),
            (r#"{"type":5}"#.into(), Err(ServerMessageError::NotJson)),
            ("[1]".into(), Err(ServerMessageError::NotJson)),
            (r#""str""#.into(), Err(ServerMessageError::NotJson)),
            (
                r#"{"type":"config","source_channels":null}"#.into(),
                Err(missing("source_channels", "config")),
            ),
            (r#"{"type":"config","source_channels":256}"#.into(), Err(ServerMessageError::NotJson)),
            (r#"{"type":"config","source_channels":"2"}"#.into(), Err(ServerMessageError::NotJson)),
            (
                r#"{"type":"config","source_channels":2.0,"sample_rate":-1,"channels":null}"#.into(),
                Ok(ServerMessage::Configuration(DeskConfiguration {
                    source_channels: 2,
                    sample_rate: None,
                    groups: Vec::new(),
                    channels: None,
                })),
            ),
            (
                r#"{"type":"config","source_channels":1,"groups":[{"name":"A","id":5,"channels":"x"}]}"#.into(),
                Ok(ServerMessage::Configuration(DeskConfiguration {
                    source_channels: 1,
                    sample_rate: None,
                    groups: vec![ChannelGroup {
                        id: None,
                        name: "A".into(),
                        channels: Vec::new(),
                    }],
                    channels: None,
                })),
            ),
            (r#"{"type":"mix_state","volume_percent":null}"#.into(), Err(missing("volume_percent", "mix_state"))),
            (r#"{"type":"mix_state","volume_percent":"x"}"#.into(), Err(ServerMessageError::NotJson)),
            (
                format!(r#"{{"type":"mix_state",{}}}"#, report.replace(r#""group_levels":[]"#, r#""group_levels":null"#)),
                Err(missing("group_levels", "mix_state")),
            ),
            (
                format!(r#"{{"type":"mix_state",{report},"locked":[true,true,true],"engineer_limit_percent":"x"}}"#),
                Ok(ServerMessage::MixState(MixReport {
                    sequence: None,
                    volume_percent: 1,
                    max_level_percent: 1,
                    muted: false,
                    channel_gains: vec![1],
                    pans: vec![1],
                    channel_mutes: vec![false],
                    channel_solos: vec![false],
                    group_levels: Vec::new(),
                    engineer_limit_percent: 100,
                    locked: vec![true, true, true],
                    following: None,
                })),
            ),
            (r#"{"type":"mix_ack","seq":12}"#.into(), Ok(ServerMessage::UnreadableMixAck { sequence: 12 })),
            (r#"{"type":"mix_ack"}"#.into(), Err(missing("volume_percent", "mix_ack"))),
            (
                r#"{"type":"mix_ack","seq":1e2,"volume_percent":"x"}"#.into(),
                Ok(ServerMessage::UnreadableMixAck { sequence: 100 }),
            ),
            (r#"{"type":"mix_ack","seq":-1,"volume_percent":"x"}"#.into(), Err(ServerMessageError::NotJson)),
            (r#"{"type":"playback","buffer":null}"#.into(), Err(missing("buffer", "playback"))),
            (r#"{"type":"playback","buffer":"auto"}"#.into(), Err(ServerMessageError::NotJson)),
            (r#"{"type":"playback","buffer":{}}"#.into(), Err(missing("mode", "playback"))),
            (r#"{"type":"playback","buffer":{"mode":null}}"#.into(), Err(missing("mode", "playback"))),
            (r#"{"type":"playback","buffer":{"mode":5}}"#.into(), Err(ServerMessageError::NotJson)),
            (r#"{"type":"playback","buffer":{"mode":"auto","ms":"x"}}"#.into(), Err(ServerMessageError::NotJson)),
            (r#"{"type":"playback","buffer":{"mode":"auto","ms":2.5}}"#.into(), Err(ServerMessageError::NotJson)),
            (r#"{"type":"playback","buffer":{"mode":"fixed","ms":null}}"#.into(), Err(missing("ms", "playback"))),
            (
                r#"{"type":"playback","buffer":{"mode":"fixed","ms":40.0}}"#.into(),
                Ok(ServerMessage::PlaybackBuffer(PlaybackBufferSetting::Fixed { milliseconds: 40 })),
            ),
            (r#"{"type":"playback","buffer":{"mode":"fixed","ms":1e30}}"#.into(), Err(ServerMessageError::NotJson)),
            (r#"{"type":"redundancy","delay_blocks":null}"#.into(), Err(missing("delay_blocks", "redundancy"))),
            (r#"{"type":"redundancy","delay_blocks":2.0}"#.into(), Ok(ServerMessage::RedundancyHold { blocks: 2 })),
            (r#"{"type":"redundancy","delay_blocks":2.5}"#.into(), Err(ServerMessageError::NotJson)),
            (r#"{"type":"redundancy","delay_blocks":"2"}"#.into(), Err(ServerMessageError::NotJson)),
            (r#"{"type":"error"}"#.into(), Err(missing("message", "error"))),
            (r#"{"type":"error","message":null}"#.into(), Err(missing("message", "error"))),
            (r#"{"type":"error","message":5}"#.into(), Err(ServerMessageError::NotJson)),
            (r#"{"type":"error","message":"m","seq":"x"}"#.into(), Err(ServerMessageError::NotJson)),
            (r#"{"type":"error","message":"m","seq":-1}"#.into(), Err(ServerMessageError::NotJson)),
            (
                r#"{"type":"error","message":"m","seq":null}"#.into(),
                Ok(ServerMessage::Error(DeskError {
                    message: "m".into(),
                    sequence: None,
                })),
            ),
        ];

        for (json, expected) in cases {
            assert_eq!(decode(&json), expected, "{json}");
        }
    }

    #[test]
    fn never_panics_on_what_the_network_sends() {
        let shapes = [
            json!(null),
            json!(true),
            json!(-1),
            json!(1.5),
            json!("x"),
            json!([]),
            json!({}),
            json!([null, 1, "x", {}]),
            json!({"a": [1, {"b": null}]}),
            json!(u64::MAX),
            json!(i64::MIN),
            json!(1e300),
        ];
        let fields = [
            "type",
            "source_channels",
            "sample_rate",
            "groups",
            "channels",
            "seq",
            "volume_percent",
            "max_level_percent",
            "muted",
            "pans",
            "mutes",
            "solos",
            "group_levels",
            "engineer_limit_percent",
            "locked",
            "following",
            "buffer",
            "delay_blocks",
            "message",
        ];
        let types = [
            "config",
            "mix_ack",
            "mix_state",
            "playback",
            "redundancy",
            "error",
            "x",
        ];

        for message_type in types {
            for field in fields {
                for shape in &shapes {
                    let mut object = Map::new();
                    object.insert("type".into(), json!(message_type));
                    object.insert(field.into(), shape.clone());
                    let _ = decode(&Value::Object(object.clone()).to_string());
                    // The same field inside every place a nested object is read.
                    for nested in ["groups", "channels", "buffer"] {
                        object.insert(nested.into(), json!([{ field: shape.clone() }]));
                        let _ = decode(&Value::Object(object.clone()).to_string());
                        object.insert(nested.into(), json!({ field: shape.clone() }));
                        let _ = decode(&Value::Object(object.clone()).to_string());
                    }
                }
            }
        }
        for text in [
            "",
            "{",
            "{\"type\":",
            "\u{0}",
            "[[[[",
            "{\"type\":\"config\",\"source_channels\":1e400}",
        ] {
            let _ = decode(text);
        }
    }
}

#[cfg(test)]
mod android_parity {
    //! The Android client's tests of how a `config`, a `mix_state` and a `mix_ack` are read, from
    //! `ControlChannelTest.kt`, as `ConfigParityTests.swift` and `MixReportParityTests.swift` port
    //! them: one test per Swift test.

    use super::tests::mix_state;
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;

    fn details(json: &str) -> Option<Vec<SourceChannel>> {
        match ServerMessage::parse(json) {
            Ok(ServerMessage::Configuration(config)) => config.channels,
            other => panic!("expected a configuration, got {other:?}"),
        }
    }

    fn info(name: &str, kind: ChannelKind) -> SourceChannel {
        SourceChannel {
            name: name.to_owned(),
            kind,
            ..SourceChannel::untouched()
        }
    }

    // ConfigParityTests.swift

    #[test]
    fn a_config_hands_on_the_channel_details_in_channel_order() {
        let json = r#"{"type":"config","source_channels":4,"groups":[],"channels":[
                {"name":"Vox","kind":"voice","hidden":false,"link":false},
                {"name":"Keys","kind":"keys","hidden":false,"link":true},
                {"name":"","kind":"keys","hidden":false,"link":false},
                {"name":"Spare","kind":"other","hidden":true,"link":false}]}"#;

        assert_eq!(
            details(json),
            Some(vec![
                info("Vox", ChannelKind::Voice),
                SourceChannel {
                    is_linked_to_next: true,
                    ..info("Keys", ChannelKind::Keys)
                },
                info("", ChannelKind::Keys),
                SourceChannel {
                    is_hidden: true,
                    ..info("Spare", ChannelKind::Other)
                },
            ])
        );
    }

    #[test]
    fn a_config_from_an_older_server_hands_on_no_details() {
        // The opening config of Android's `openChannel`, which has no `channels`, as an older
        // desk's.
        assert_eq!(
            details(r#"{"type":"config","source_channels":2,"groups":[]}"#),
            None
        );
    }

    #[test]
    fn channel_details_are_read_leniently() {
        let json = r#"{"type":"config","source_channels":3,"channels":[
                {"name":"Gtr","kind":"theremin","hidden":true,"link":true},
                "not an entry",
                {"name":7,"kind":null},
                {"name":"Extra","kind":"bass"}]}"#;

        // An unknown kind reads as other; an entry that is not an object, or a field of the wrong
        // type, reads as the default; an entry past the announced count is dropped.
        assert_eq!(
            details(json),
            Some(vec![
                SourceChannel {
                    is_hidden: true,
                    is_linked_to_next: true,
                    ..info("Gtr", ChannelKind::Other)
                },
                SourceChannel::untouched(),
                SourceChannel::untouched(),
            ])
        );
        // Fewer entries than channels: the rest are channels the engineer never touched.
        assert_eq!(
            details(
                r#"{"type":"config","source_channels":2,"channels":[{"name":"Bass","kind":"bass"}]}"#
            ),
            Some(vec![
                info("Bass", ChannelKind::Bass),
                SourceChannel::untouched()
            ])
        );
        // Anything but an array is no details at all.
        assert_eq!(details(r#"{"type":"config","source_channels":2}"#), None);
        assert_eq!(
            details(r#"{"type":"config","source_channels":2,"channels":"voice"}"#),
            None
        );
    }

    #[test]
    fn every_kind_is_read_by_its_wire_name() {
        for kind in ChannelKind::ALL {
            assert_eq!(ChannelKind::from_wire_name(kind.wire_name()), Some(kind));
        }
        assert_eq!(
            ChannelKind::ALL.map(ChannelKind::wire_name),
            [
                "voice",
                "guitar",
                "bass",
                "keys",
                "drums",
                "percussion",
                "horns",
                "strings",
                "playback",
                "click",
                "other",
            ]
        );
    }

    // MixReportParityTests.swift

    /// A mix report as the desk writes it: its full 32-channel and 16-group tables, and `seq` if
    /// any. Android's `mixReport` helper in `ControlChannelTest.kt:47-59`.
    fn mix_report(message_type: &str, first_level: i64, seq: Option<i64>) -> Value {
        let mut object = json!({
            "type": message_type,
            "volume_percent": 80,
            "max_level_percent": 90,
            "muted": true,
            "channels": (0..32).map(|channel| first_level + channel).collect::<Vec<_>>(),
            "pans": (0..32).map(|channel| if channel == 1 { 25 } else { default_pan(channel) }).collect::<Vec<_>>(),
            "mutes": (0..32).map(|channel| channel == 2).collect::<Vec<_>>(),
            "solos": (0..32).map(|channel| channel == 3).collect::<Vec<_>>(),
            "group_levels": (0..16).map(|group| 50 + group).collect::<Vec<_>>(),
        });
        if let Some(seq) = seq {
            object["seq"] = json!(seq);
        }
        object
    }

    /// Android's `defaultPan`: even channels left, odd ones right.
    fn default_pan(channel: i64) -> i64 {
        if channel % 2 == 0 {
            0
        } else {
            100
        }
    }

    /// `object` with `key` set to `value`, or without `key` when `value` is `None`, which is what
    /// org.json's `put` does with a Kotlin null. `json!(null)` stands for Android's
    /// `JSONObject.NULL`.
    fn putting(key: &str, value: Option<Value>, mut object: Value) -> Value {
        let fields = object.as_object_mut().unwrap();
        match value {
            Some(value) => fields.insert(key.to_owned(), value),
            None => fields.remove(key),
        };
        object
    }

    /// What Android's `parseEngineerControls` returns, read off a report: the limit, the locked
    /// channels and the leader.
    fn controls(report: &MixReport) -> (i64, BTreeSet<usize>, Option<String>) {
        let locked = (0..report.locked.len())
            .filter(|&channel| report.locked[channel])
            .collect();
        (
            report.engineer_limit_percent,
            locked,
            report.following.clone(),
        )
    }

    /// Android also asserts that nothing is sent back and no error is reported. Nothing is sent
    /// back is the connection's; no error is the read succeeding.
    #[test]
    fn a_pushed_mix_is_read_whole_and_handed_on_with_nothing_sent_back() {
        let mix = mix_state(&mix_report("mix_state", 10, None).to_string());

        assert_eq!(
            mix.channel_gains,
            (0..32).map(|channel| 10 + channel).collect::<Vec<_>>()
        );
        assert_eq!(
            mix.pans,
            (0..32)
                .map(|channel| if channel == 1 {
                    25
                } else {
                    default_pan(channel)
                })
                .collect::<Vec<_>>()
        );
        assert_eq!(
            mix.channel_mutes,
            (0..32).map(|channel| channel == 2).collect::<Vec<_>>()
        );
        assert_eq!(
            mix.channel_solos,
            (0..32).map(|channel| channel == 3).collect::<Vec<_>>()
        );
        assert_eq!(
            mix.group_levels,
            (0..16).map(|group| 50 + group).collect::<Vec<_>>()
        );
        assert_eq!(mix.volume_percent, 80);
        assert_eq!(mix.max_level_percent, 90);
        assert!(mix.muted);
    }

    /// Android reports it as "Invalid control response" and hands nothing on; here the read
    /// fails, and a message that fails is never handed on.
    #[test]
    fn a_push_missing_part_of_the_mix_is_reported_and_not_handed_on() {
        assert!(ServerMessage::parse(
            r#"{"type":"mix_state","volume_percent":80,"channels":[100,100]}"#
        )
        .is_err());
    }

    #[test]
    fn the_engineers_controls_are_read_leniently() {
        let read = |limit: Option<Value>, locked: Option<Value>| {
            let object = putting(
                "locked",
                locked,
                putting(
                    "engineer_limit_percent",
                    limit,
                    mix_report("mix_state", 10, None),
                ),
            );
            controls(&mix_state(&object.to_string()))
        };
        let open = |limit: i64, locked: &[usize]| (limit, locked.iter().copied().collect(), None);

        // A short list leaves the rest open; an entry that is not a boolean is open.
        assert_eq!(
            read(None, Some(json!([true, false, true]))),
            open(100, &[0, 2])
        );
        assert_eq!(
            read(None, Some(json!([1, null, true, "true"]))),
            open(100, &[2])
        );
        // A limit that is not a number is no limit; one out of range is held to 0-100.
        assert_eq!(
            read(Some(json!("high")), Some(json!("all"))),
            open(100, &[])
        );
        assert_eq!(read(Some(json!(140)), None), open(100, &[]));
        assert_eq!(read(Some(json!(-5)), None), open(0, &[]));
    }

    #[test]
    fn following_is_read_leniently() {
        let read = |value: Option<Value>| {
            mix_state(&putting("following", value, mix_report("mix_state", 10, None)).to_string())
                .following
        };

        // Absent (an older desk), null, or anything but a string follows nobody.
        assert_eq!(read(None), None);
        assert_eq!(read(Some(json!(null))), None);
        assert_eq!(read(Some(json!(7))), None);
        assert_eq!(read(Some(json!(["Ana"]))), None);
        assert_eq!(read(Some(json!("Ana"))).as_deref(), Some("Ana"));
        // A leader the desk has no name for is still followed.
        assert_eq!(read(Some(json!(""))).as_deref(), Some(""));
    }
}
