//! The control path across the binding: reading the desk's messages, writing the phone's, the
//! reconnect policy, the client-id rule and the saved mixes' JSON, all from
//! `bandear_core::control`.
//!
//! Called once per control message or user action, never per packet, so UniFFI's cost per call is
//! no concern here, on Android included. The text crosses as `String` both ways; the platform's
//! WebSocket and storage send and keep it.
//!
//! The core's plain types cross as they are, declared here as UniFFI remote types (the compiler
//! refuses a declaration that stops matching the core's). Two cross as objects instead, because
//! their values are checked when made and must not be built around that check: [`Registration`]
//! (immutable) and [`ReconnectPolicy`] (a counter behind a lock). Nothing here panics: the core
//! does not on any input, and every function returns what the core returns.

use bandear_core::control::client_identity::{self, ClientIdChoice};
use bandear_core::control::client_message::{
    self, ClientCapability, ClientMessage, ClientMessageError, LinkStats, MixCommand,
};
use bandear_core::control::mix_bank::{self, MixSnapshot, SavedMix};
use bandear_core::control::reconnect_policy;
use bandear_core::control::server_message::{
    ChannelGroup, ChannelKind, DeskConfiguration, DeskError, MixReport, ServerMessage,
    ServerMessageError, SourceChannel,
};
use bandear_core::cushion::PlaybackBufferSetting;
use bandear_core::link_stats::LinkStatsReport;
use std::sync::{Mutex, MutexGuard, PoisonError};

// Reading the desk's messages.

/// The desk's message `json` holds, read as both apps read it: unknown types are kept as
/// `Unrecognized`, fields added after a message shipped are read leniently, and anything else
/// unreadable is the error that says why.
#[uniffi::export]
pub fn parse_server_message(json: String) -> Result<ServerMessage, ServerMessageError> {
    ServerMessage::parse(&json)
}

/// A control message from the desk.
#[uniffi::remote(Enum)]
pub enum ServerMessage {
    /// What the desk is capturing, sent as soon as a socket opens and again whenever the device or
    /// the channels change.
    Configuration(DeskConfiguration),
    /// The answer to the phone's own `mix`, carrying that mix's number.
    MixAck(MixReport),
    /// A `mix_ack` numbered `sequence` whose mix could not be read: it still answers that number,
    /// with nothing to show.
    UnreadableMixAck { sequence: u64 },
    /// The mix, pushed when it changed from elsewhere; never numbered.
    MixState(MixReport),
    /// The cushion the desk set for this musician.
    PlaybackBuffer(PlaybackBufferSetting),
    /// How many packets to hold back for the desk's delayed copies, 0 to 4.
    RedundancyHold { blocks: u8 },
    /// An `error` reply.
    Error(DeskError),
    /// A message of a type this build does not know, which the phone ignores.
    Unrecognized { message_type: String },
}

/// Why a message from the desk could not be read.
#[uniffi::remote(Error)]
pub enum ServerMessageError {
    /// Not a JSON object this client can route on, or a known message with a field of the wrong
    /// type.
    NotJson,
    /// A JSON object with no `type`.
    MissingType,
    /// A known message type missing a field it cannot be read without, or holding `null` there.
    MissingField { field: String, message_type: String },
    /// A known message type whose field holds a value this client cannot use: the message is
    /// ignored and what it would have changed stays as it was.
    UnusableField { field: String, message_type: String },
}

/// The `config` message.
#[uniffi::remote(Record)]
pub struct DeskConfiguration {
    pub source_channels: u8,
    /// The rate the desk named, or `None` when the `config` carried none.
    pub sample_rate: Option<u32>,
    pub groups: Vec<ChannelGroup>,
    /// Exactly one per source channel, or `None` when the desk sent no details (an older desk).
    pub channels: Option<Vec<SourceChannel>>,
}

/// One group in the desk's `config`.
#[uniffi::remote(Record)]
pub struct ChannelGroup {
    /// The desk's stable id for the group, or `None` from a desk that predates group ids.
    pub id: Option<String>,
    pub name: String,
    /// The member channels, 0-based, never negative.
    pub channels: Vec<i64>,
}

/// One source channel in the desk's `config`, in channel order: entry 0 is CH 1.
#[uniffi::remote(Record)]
pub struct SourceChannel {
    /// What the engineer called the input; empty when it has no name.
    pub name: String,
    pub kind: ChannelKind,
    /// The engineer hid the input: it is in nobody's mix and the client does not show it.
    pub is_hidden: bool,
    /// This channel and the next are one stereo pair; only the pair's first channel carries it.
    pub is_linked_to_next: bool,
}

/// What a source channel carries; a kind this build does not know reads as `Other`.
#[uniffi::remote(Enum)]
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

/// A whole mix as the desk reports it (`mix_ack`, `mix_state`).
#[uniffi::remote(Record)]
pub struct MixReport {
    /// The number of the phone's mix a `mix_ack` answers; `None` on a push.
    pub sequence: Option<u64>,
    pub volume_percent: i64,
    pub max_level_percent: i64,
    pub muted: bool,
    pub channel_gains: Vec<i64>,
    pub pans: Vec<i64>,
    pub channel_mutes: Vec<bool>,
    pub channel_solos: Vec<bool>,
    pub group_levels: Vec<i64>,
    /// Caps what the musician hears on top of their own `max_level_percent`; 100 is no limit.
    pub engineer_limit_percent: i64,
    /// One entry per channel: a locked channel's controls should read disabled.
    pub locked: Vec<bool>,
    /// The musician whose mix this one mirrors, or `None`.
    pub following: Option<String>,
}

/// An `error` reply, with the number of the `mix` it refused when that mix had one.
#[uniffi::remote(Record)]
pub struct DeskError {
    pub message: String,
    pub sequence: Option<u64>,
}

// Writing the phone's messages.

/// The `register` message: who is listening, and on which UDP port. Made checked, so it holds
/// what the desk stores: the name trimmed and capped at 64 Unicode scalars, a client id the desk
/// would discard dropped, each capability once.
#[derive(Debug, PartialEq, Eq, Hash, uniffi::Object)]
#[uniffi::export(Debug, Eq, Hash)]
pub struct Registration {
    inner: client_message::Registration,
}

#[uniffi::export]
impl Registration {
    /// The registration of `name` listening on `udp_port`, with `client_id` when the desk keeps
    /// it and each of `capabilities` once, in the order given. Refuses a `udp_port` of 0, which
    /// the desk refuses.
    #[uniffi::constructor]
    pub fn new(
        name: String,
        udp_port: u16,
        client_id: Option<String>,
        capabilities: Vec<ClientCapability>,
    ) -> Result<Self, ClientMessageError> {
        client_message::Registration::new(&name, udp_port, client_id.as_deref(), &capabilities)
            .map(|inner| Self { inner })
    }

    /// The name as the desk stores it.
    pub fn name(&self) -> String {
        self.inner.name().to_owned()
    }

    pub fn udp_port(&self) -> u16 {
        self.inner.udp_port()
    }

    /// The client id, or `None` when none was given or the desk would not keep the one given.
    pub fn client_id(&self) -> Option<String> {
        self.inner.client_id().map(str::to_owned)
    }

    /// The capabilities announced, each once, in the order first given.
    pub fn capabilities(&self) -> Vec<ClientCapability> {
        self.inner.capabilities().to_vec()
    }

    /// The `register` message's text.
    pub fn encoded_json(&self) -> String {
        ClientMessage::Register(self.inner.clone()).to_json()
    }
}

/// The `mix` message's text for `command`: a list left `None` is left out, which the desk reads
/// as unchanged.
#[uniffi::export]
pub fn mix_message(command: MixCommand) -> String {
    ClientMessage::Mix(command).to_json()
}

/// The `stats` message's text for one of the phone's link reports: every count (`recovered`
/// included), a count below zero as 0, the interval held to 1-60 000 ms, and each measured extra
/// rounded to a hundredth or left out when the desk would not read it.
#[uniffi::export]
pub fn stats_message(report: LinkStatsReport) -> String {
    ClientMessage::Stats(LinkStats::from_report(&report)).to_json()
}

/// Every capability this build implements, in the order the apps announce them.
#[uniffi::export]
pub fn all_client_capabilities() -> Vec<ClientCapability> {
    ClientCapability::ALL.to_vec()
}

/// The capability's name on the wire (`stats.v2`, …).
#[uniffi::export]
pub fn client_capability_name(capability: ClientCapability) -> String {
    capability.wire_name().to_owned()
}

/// What this client tells the desk it understands.
#[uniffi::remote(Enum)]
pub enum ClientCapability {
    /// `stats` carry `jitter_ms`, `cushion_ms` and `backlog_ms`.
    StatsV2,
    /// The desk may set the playback buffer with a `playback` message.
    PlaybackBuffer,
    /// `stats` carry `recovered`.
    RedundancyRecovered,
    /// Packets are held back for the blocks a `redundancy` message names.
    RedundancyHold,
}

/// The `mix` message: the master values, always, and the per-channel and group lists, each left
/// out when `None`.
#[uniffi::remote(Record)]
pub struct MixCommand {
    /// The number this mix goes out under, so its ack can be told from older ones.
    #[uniffi(default)]
    pub sequence: Option<u64>,
    pub volume_percent: i64,
    pub max_level_percent: i64,
    pub muted: bool,
    /// Per-channel level, 0 to 100. Goes out as `channels`.
    #[uniffi(default)]
    pub channel_gains: Option<Vec<i64>>,
    /// Per-channel pan, 0 left, 50 centre, 100 right.
    #[uniffi(default)]
    pub pans: Option<Vec<i64>>,
    /// Goes out as `mutes`.
    #[uniffi(default)]
    pub channel_mutes: Option<Vec<bool>>,
    /// Goes out as `solos`.
    #[uniffi(default)]
    pub channel_solos: Option<Vec<bool>>,
    #[uniffi(default)]
    pub group_levels: Option<Vec<i64>>,
}

/// Why a message could not be built: something the desk would refuse.
#[uniffi::remote(Error)]
pub enum ClientMessageError {
    /// The desk refuses a registration without a port to send audio to.
    UdpPortMustNotBeZero,
    /// Outside 1 to 60 000 ms; the desk cannot normalise rates by it.
    StatsIntervalOutOfRange { interval_ms: u32 },
}

// The reconnect policy.

/// The waits between attempts to open a lost connection again: 0.5, 1, 2 and 4 seconds, then 5
/// seconds for as long as the session runs, until a connection that heard from the desk resets
/// it. One per session, which owns it.
#[derive(Debug, Default, uniffi::Object)]
pub struct ReconnectPolicy {
    inner: Mutex<reconnect_policy::ReconnectPolicy>,
}

#[uniffi::export]
impl ReconnectPolicy {
    /// A policy at the start of its schedule.
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self::default()
    }

    /// The wait before the next attempt, in milliseconds; each call moves one step along the
    /// schedule.
    pub fn next_delay_ms(&self) -> i64 {
        self.lock().next_delay_ms()
    }

    /// Starts the schedule again, after a connection heard from the desk.
    pub fn reset(&self) {
        self.lock().reset();
    }
}

impl ReconnectPolicy {
    /// The policy, even after a call panicked while holding it: its counter is whole between
    /// calls, and no call panics.
    fn lock(&self) -> MutexGuard<'_, reconnect_policy::ReconnectPolicy> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The wait before attempt `attempt`, in milliseconds, counted from 0 for the first retry after a
/// loss: a negative attempt reads as the first, any attempt past the schedule waits the most.
#[uniffi::export]
pub fn reconnect_delay_for_attempt_ms(attempt: i64) -> i64 {
    reconnect_policy::ReconnectPolicy::delay_for_attempt_ms(attempt)
}

/// The longest wait between two attempts, in milliseconds.
#[uniffi::export]
pub fn reconnect_maximum_delay_ms() -> i64 {
    reconnect_policy::ReconnectPolicy::MAXIMUM_DELAY_MS
}

/// Whether a connection that failed or closed is opened again: only while the session still runs
/// and nobody stopped it.
#[uniffi::export]
pub fn should_reconnect(session_running: bool, stopped: bool) -> bool {
    reconnect_policy::ReconnectPolicy::should_reconnect(session_running, stopped)
}

// The client id.

/// `raw` when the desk keeps it as a client id (1 to 64 ASCII letters, digits and hyphens),
/// otherwise `None`, which the desk reads as no id at all. A new id stays the platform's to make
/// (a lowercase random UUID) and to keep.
#[uniffi::export]
pub fn valid_client_id(raw: Option<String>) -> Option<String> {
    client_identity::valid_client_id(raw.as_deref()).map(str::to_owned)
}

/// The client id to announce: the `stored` one when the desk keeps it, with nothing to save;
/// otherwise `generated`, a new id the platform made (a lowercase random UUID), which the phone
/// must save. Making the id and keeping it stay the platform's.
#[uniffi::export]
pub fn choose_client_id(stored: Option<String>, generated: String) -> ClientIdChoice {
    client_identity::choose_client_id(stored.as_deref(), generated)
}

/// The client id a phone announces, and whether it must store it.
#[uniffi::remote(Record)]
pub struct ClientIdChoice {
    pub client_id: String,
    /// Whether `client_id` is new, so the phone stores it from now on.
    pub should_save: bool,
}

// The saved mixes' JSON.

/// `mixes` as the phone keeps them: a JSON list of `name` and `snapshot` objects, each snapshot a
/// string of its own JSON object, as both apps write them.
#[uniffi::export]
pub fn encode_saved_mixes(mixes: Vec<SavedMix>) -> String {
    mix_bank::encode_saved_mixes(&mixes)
}

/// The saved mixes `json` holds. Never fails: no text or unreadable text is no saved mix, and an
/// entry that cannot be read is skipped with every other entry kept.
#[uniffi::export]
pub fn decode_saved_mixes(json: Option<String>) -> Vec<SavedMix> {
    mix_bank::decode_saved_mixes(json.as_deref())
}

/// `snapshot` as its own JSON object.
#[uniffi::export]
pub fn encode_mix_snapshot(snapshot: MixSnapshot) -> String {
    mix_bank::encode_snapshot(&snapshot)
}

/// The snapshot `json` holds, or `None` when it holds no JSON object; a damaged value inside reads
/// as its default.
#[uniffi::export]
pub fn decode_mix_snapshot(json: String) -> Option<MixSnapshot> {
    mix_bank::decode_snapshot(&json)
}

/// A mix the musician saved, under the name they gave it.
#[uniffi::remote(Record)]
pub struct SavedMix {
    pub name: String,
    pub snapshot: MixSnapshot,
}

/// A whole mix as the phone saves it: the musician's own levels, never the More of me lift.
#[uniffi::remote(Record)]
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

#[cfg(test)]
mod tests {
    //! The binding's own surface: each function returns what the core returns. The rules are
    //! tested in `bandear_core::control`.

    use super::*;
    use std::sync::Arc;

    #[test]
    fn a_message_from_the_desk_crosses_as_the_core_reads_it() {
        assert_eq!(
            parse_server_message(r#"{"type":"redundancy","delay_blocks":2}"#.to_owned()),
            Ok(ServerMessage::RedundancyHold { blocks: 2 })
        );
        assert_eq!(
            parse_server_message(r#"{"type":"playback"}"#.to_owned()),
            Err(ServerMessageError::MissingField {
                field: "buffer".to_owned(),
                message_type: "playback".to_owned(),
            })
        );
        assert_eq!(
            parse_server_message("not json".to_owned()),
            Err(ServerMessageError::NotJson)
        );
    }

    #[test]
    fn a_registration_holds_what_the_desk_stores_and_writes_its_message() {
        let registration = Registration::new(
            "  Ana  ".to_owned(),
            50_000,
            Some("has space".to_owned()),
            vec![ClientCapability::StatsV2, ClientCapability::StatsV2],
        )
        .unwrap();

        assert_eq!(registration.name(), "Ana");
        assert_eq!(registration.udp_port(), 50_000);
        assert_eq!(registration.client_id(), None);
        assert_eq!(registration.capabilities(), [ClientCapability::StatsV2]);
        assert_eq!(
            registration.encoded_json(),
            r#"{"type":"register","name":"Ana","udp_port":50000,"capabilities":["stats.v2"]}"#
        );
        assert_eq!(
            Registration::new("Ana".to_owned(), 0, None, Vec::new()),
            Err(ClientMessageError::UdpPortMustNotBeZero)
        );
    }

    #[test]
    fn the_mix_and_stats_messages_are_the_cores() {
        let command = MixCommand {
            sequence: Some(3),
            ..MixCommand::new(80, 100, false)
        };
        assert_eq!(
            mix_message(command),
            r#"{"type":"mix","seq":3,"volume_percent":80,"max_level_percent":100,"muted":false}"#
        );
        assert_eq!(
            stats_message(LinkStatsReport {
                interval_ms: 1_000,
                packets_received: 4,
                receive_timeouts: 1,
                recovered: 1,
                ..LinkStatsReport::default()
            }),
            r#"{"type":"stats","interval_ms":1000,"packets_received":4,"packets_lost":0,"underruns":0,"receive_timeouts":1,"recovered":1}"#
        );
        assert_eq!(all_client_capabilities(), ClientCapability::ALL);
        assert_eq!(
            all_client_capabilities()
                .into_iter()
                .map(client_capability_name)
                .collect::<Vec<_>>(),
            [
                "stats.v2",
                "playback.buffer",
                "redundancy.recovered",
                "redundancy.hold"
            ]
        );
    }

    #[test]
    fn the_reconnect_policy_steps_along_its_schedule_and_resets() {
        let policy = ReconnectPolicy::new();

        let delays: Vec<i64> = (0..6).map(|_| policy.next_delay_ms()).collect();
        assert_eq!(delays, [500, 1_000, 2_000, 4_000, 5_000, 5_000]);
        policy.reset();
        assert_eq!(policy.next_delay_ms(), 500);

        assert_eq!(reconnect_delay_for_attempt_ms(-1), 500);
        assert_eq!(reconnect_delay_for_attempt_ms(i64::MAX), 5_000);
        assert_eq!(reconnect_maximum_delay_ms(), 5_000);
        assert!(should_reconnect(true, false));
        assert!(!should_reconnect(true, true));
    }

    #[test]
    fn a_poisoned_policy_goes_on_counting() {
        let policy = Arc::new(ReconnectPolicy::new());
        policy.next_delay_ms();
        let held = Arc::clone(&policy);
        let _ = std::thread::spawn(move || {
            let _guard = held.inner.lock().unwrap();
            panic!("poison the lock");
        })
        .join();

        assert!(policy.inner.is_poisoned());
        assert_eq!(policy.next_delay_ms(), 1_000);
    }

    #[test]
    fn the_client_id_rule_and_the_saved_mixes_are_the_cores() {
        assert_eq!(
            valid_client_id(Some("phone-1".to_owned())).as_deref(),
            Some("phone-1")
        );
        assert_eq!(valid_client_id(Some("not valid!".to_owned())), None);
        assert_eq!(valid_client_id(None), None);

        let mixes = vec![SavedMix {
            name: "Verse".to_owned(),
            snapshot: MixSnapshot {
                channel_gains: vec![30],
                ..MixSnapshot::default()
            },
        }];
        let json = encode_saved_mixes(mixes.clone());
        assert_eq!(decode_saved_mixes(Some(json)), mixes);
        assert_eq!(decode_saved_mixes(None), []);
        assert_eq!(
            decode_mix_snapshot(encode_mix_snapshot(mixes[0].snapshot.clone())),
            Some(mixes[0].snapshot.clone())
        );
        assert_eq!(decode_mix_snapshot("[1]".to_owned()), None);
    }

    #[test]
    fn the_client_id_choice_is_the_cores() {
        assert_eq!(
            choose_client_id(Some("phone-1".to_owned()), "fresh-id".to_owned()),
            ClientIdChoice {
                client_id: "phone-1".to_owned(),
                should_save: false,
            }
        );
        for stored in [None, Some("not valid!".to_owned())] {
            assert_eq!(
                choose_client_id(stored, "fresh-id".to_owned()),
                ClientIdChoice {
                    client_id: "fresh-id".to_owned(),
                    should_save: true,
                }
            );
        }
    }
}
