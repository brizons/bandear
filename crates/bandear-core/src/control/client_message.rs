//! The control messages the phone sends the desk: `register`, `mix` and `stats`.
//!
//! Ported from the iOS client's `ClientMessage.swift` and `LinkStatsMessage.swift`. Field names
//! are the desk's, which spells every one of them in snake_case (`apps/server/src/protocol.rs`).
//! The text is written as the iOS client's `JSONEncoder` wrote it (see [`ClientMessage::to_json`]).

use super::json::{ObjectWriter, Slashes};
use crate::link_stats::LinkStatsReport;
use std::fmt;

/// What this client tells the desk it understands. The desk keeps any name it is sent and gates
/// nothing on it (`apps/server/src/protocol.rs:127-146`), but the phone only ever announces what
/// it implements, so a closed set is the honest shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClientCapability {
    /// `stats` carry `jitter_ms`, `cushion_ms` and `backlog_ms`.
    StatsV2,
    /// The desk may set the playback buffer with a `playback` message.
    PlaybackBuffer,
    /// `stats` carry `recovered`: packets played after a delayed copy filled their gap.
    RedundancyRecovered,
    /// Packets are held back for the blocks a `redundancy` message names.
    RedundancyHold,
}

impl ClientCapability {
    /// Every capability this build implements, in the order the apps announce them.
    pub const ALL: [ClientCapability; 4] = [
        ClientCapability::StatsV2,
        ClientCapability::PlaybackBuffer,
        ClientCapability::RedundancyRecovered,
        ClientCapability::RedundancyHold,
    ];

    /// The capability's name on the wire.
    pub fn wire_name(self) -> &'static str {
        match self {
            ClientCapability::StatsV2 => "stats.v2",
            ClientCapability::PlaybackBuffer => "playback.buffer",
            ClientCapability::RedundancyRecovered => "redundancy.recovered",
            ClientCapability::RedundancyHold => "redundancy.hold",
        }
    }
}

/// The `register` message: who is listening, and on which UDP port.
///
/// There is deliberately no address field. The desk reads the address off the socket, so a client
/// cannot claim to be somebody else's (`apps/server/src/protocol.rs:107-111`).
///
/// [`Registration::new`] normalises exactly as the desk does, so what the phone shows is what the
/// desk stores: the name is trimmed and capped, and a client id the desk would discard is dropped
/// here instead of being sent and silently ignored. A name with nothing in it is sent empty, as
/// Android sends it (`buildRegisterMessage`, `ControlChannel.kt:545-557`): the desk registers the
/// musician with no name and still sends them audio (`registered_name`,
/// `apps/server/src/protocol.rs:330`). A zero UDP port is refused outright, because the desk
/// refuses the whole registration for it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Registration {
    name: String,
    udp_port: u16,
    client_id: Option<String>,
    capabilities: Vec<ClientCapability>,
}

impl Registration {
    /// The message's `type`.
    pub const MESSAGE_TYPE: &'static str = "register";
    /// `MAX_MUSICIAN_NAME_LENGTH` (`apps/server/src/protocol.rs:101`), in Unicode scalars.
    pub const MAXIMUM_NAME_LENGTH: usize = 64;
    /// `MAX_CLIENT_ID_LENGTH` (`apps/server/src/protocol.rs:104`).
    pub const MAXIMUM_CLIENT_ID_LENGTH: usize = 64;

    /// The registration of `name` listening on `udp_port`, with `client_id` when the desk keeps
    /// it and each of `capabilities` once, in the order given. Refuses a `udp_port` of 0.
    pub fn new(
        name: &str,
        udp_port: u16,
        client_id: Option<&str>,
        capabilities: &[ClientCapability],
    ) -> Result<Registration, ClientMessageError> {
        if udp_port == 0 {
            return Err(ClientMessageError::UdpPortMustNotBeZero);
        }

        let mut kept = Vec::with_capacity(capabilities.len());
        for &capability in capabilities {
            if !kept.contains(&capability) {
                kept.push(capability);
            }
        }
        Ok(Registration {
            name: capped(trimmed_name(name)),
            udp_port,
            client_id: client_id
                .filter(|raw| is_kept_client_id(raw))
                .map(str::to_owned),
            capabilities: kept,
        })
    }

    /// The name as the desk stores it: without the whitespace around it (see [`trimmed_name`]), and cut to
    /// [`Registration::MAXIMUM_NAME_LENGTH`] Unicode scalars.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn udp_port(&self) -> u16 {
        self.udp_port
    }

    /// The client id, or `None` when none was given or the desk would not keep the one given.
    pub fn client_id(&self) -> Option<&str> {
        self.client_id.as_deref()
    }

    /// The capabilities announced, each once, in the order first given.
    pub fn capabilities(&self) -> &[ClientCapability] {
        &self.capabilities
    }
}

/// Whether the desk keeps `raw` as a client id: 1 to [`Registration::MAXIMUM_CLIENT_ID_LENGTH`]
/// ASCII letters, digits and hyphens (`valid_client_id`, `apps/server/src/protocol.rs:192`). The
/// one copy of the rule: [`crate::control::client_identity`] reads it from here.
pub(crate) fn is_kept_client_id(raw: &str) -> bool {
    (1..=Registration::MAXIMUM_CLIENT_ID_LENGTH).contains(&raw.len())
        && raw
            .bytes()
            .all(|byte| byte == b'-' || byte.is_ascii_alphanumeric())
}

/// `name` without the whitespace around it, as iOS trimmed it
/// (`trimmingCharacters(in: .whitespacesAndNewlines)`): Unicode's White_Space, which `str::trim`
/// strips, plus U+200B ZERO WIDTH SPACE, which Apple's set also holds. Across every Unicode scalar
/// that is the only difference between the two, so a pasted zero-width space around a name still
/// does not reach the desk.
pub(crate) fn trimmed_name(name: &str) -> &str {
    name.trim_matches(|c: char| c.is_whitespace() || c == '\u{200B}')
}

/// `name` cut to [`Registration::MAXIMUM_NAME_LENGTH`] Unicode scalars, which is what the desk
/// counts with `chars().take(…)`. Counting grapheme clusters (Swift's `Character`s) would keep more
/// scalars than the desk stores, and the phone would then display a name the desk does not have.
fn capped(name: &str) -> String {
    match name.char_indices().nth(Registration::MAXIMUM_NAME_LENGTH) {
        Some((end, _)) => name[..end].to_owned(),
        None => name.to_owned(),
    }
}

/// A measurement worth sending: finite, and inside the range the desk keeps, rounded to a
/// hundredth of a millisecond after the range check. Anything else becomes absent, which is what
/// the desk would have made of it.
fn reportable(milliseconds: Option<f64>) -> Option<f64> {
    milliseconds
        .filter(|value| value.is_finite() && (0.0..=LinkStats::MAXIMUM_REPORTED_MS).contains(value))
        .map(|value| (value * 100.0).round() / 100.0)
}

/// The `mix` message: the master values, always, and the per-channel and group lists.
///
/// A list left `None` is left out of the JSON rather than sent as null: the desk reads an absent
/// list as "unchanged" (`#[serde(default)] Option<Vec<…>>`, `apps/server/src/protocol.rs:21-37`).
/// Both apps nonetheless send every list on every `mix`, the whole mix as the phone plays it
/// (`ControlChannel.kt:470-504`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MixCommand {
    /// The number this mix goes out under, so its ack can be told from the acks of older mixes
    /// still in flight. `None` sends none, which the desk reads leniently.
    pub sequence: Option<u64>,
    pub volume_percent: i64,
    pub max_level_percent: i64,
    pub muted: bool,
    /// Per-channel level, 0 to 100. Goes out as `channels`.
    pub channel_gains: Option<Vec<i64>>,
    /// Per-channel pan, 0 left, 50 centre, 100 right.
    pub pans: Option<Vec<i64>>,
    /// Goes out as `mutes`.
    pub channel_mutes: Option<Vec<bool>>,
    /// Goes out as `solos`.
    pub channel_solos: Option<Vec<bool>>,
    pub group_levels: Option<Vec<i64>>,
}

impl MixCommand {
    /// The message's `type`.
    pub const MESSAGE_TYPE: &'static str = "mix";

    /// The master values alone: no number and no list.
    pub fn new(volume_percent: i64, max_level_percent: i64, muted: bool) -> MixCommand {
        MixCommand {
            sequence: None,
            volume_percent,
            max_level_percent,
            muted,
            channel_gains: None,
            pans: None,
            channel_mutes: None,
            channel_solos: None,
            group_levels: None,
        }
    }
}

/// What a `stats` message is made from, before [`LinkStats::new`] checks it. The default is an
/// interval of 0 (which [`LinkStats::new`] refuses) with nothing counted and nothing measured.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LinkStatsFields {
    pub interval_ms: u32,
    pub packets_received: u64,
    pub packets_lost: u64,
    pub underruns: u64,
    pub receive_timeouts: u64,
    pub jitter_ms: Option<f64>,
    pub cushion_ms: Option<f64>,
    pub backlog_ms: Option<f64>,
    pub recovered: Option<u64>,
}

/// The `stats` message: what the link did over the last interval.
///
/// The counts are deltas for that interval, never running totals, so a lost report never skews the
/// next one (`apps/server/src/protocol.rs:237-241`).
///
/// The optional measurements are checked on the way in rather than on the way out: one that is not
/// finite, or outside the range the desk keeps, is left out, which is what the desk would have
/// made of it, and JSON has no non-finite number to send. `recovered` is held to
/// `packets_received` because the desk holds it there too, so the phone's own diagnostics agree
/// with the engineer's screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkStats {
    interval_ms: u32,
    packets_received: u64,
    packets_lost: u64,
    underruns: u64,
    receive_timeouts: u64,
    jitter_ms: Option<f64>,
    cushion_ms: Option<f64>,
    backlog_ms: Option<f64>,
    recovered: Option<u64>,
}

impl LinkStats {
    /// The message's `type`.
    pub const MESSAGE_TYPE: &'static str = "stats";
    /// `MAX_STATS_INTERVAL_MS` (`apps/server/src/protocol.rs:234`).
    pub const MAXIMUM_INTERVAL_MS: u32 = 60_000;
    /// `MAX_STATS_MILLISECONDS` (`apps/server/src/protocol.rs:278`).
    pub const MAXIMUM_REPORTED_MS: f64 = 60_000.0;

    /// The message for `fields`, refusing an interval outside 1 to
    /// [`LinkStats::MAXIMUM_INTERVAL_MS`]: the desk cannot normalise rates by it. A measurement is
    /// kept when it is finite and from 0 to [`LinkStats::MAXIMUM_REPORTED_MS`], rounded to a
    /// hundredth of a millisecond after that check, as Android does (`ControlChannel.kt:586`).
    pub fn new(fields: LinkStatsFields) -> Result<LinkStats, ClientMessageError> {
        let interval_ms = fields.interval_ms;
        if !(1..=LinkStats::MAXIMUM_INTERVAL_MS).contains(&interval_ms) {
            return Err(ClientMessageError::StatsIntervalOutOfRange { interval_ms });
        }
        Ok(LinkStats::checked(fields))
    }

    /// `fields` with their measurements checked and `recovered` held, the interval as given.
    fn checked(fields: LinkStatsFields) -> LinkStats {
        let LinkStatsFields {
            interval_ms,
            packets_received,
            packets_lost,
            underruns,
            receive_timeouts,
            jitter_ms,
            cushion_ms,
            backlog_ms,
            recovered,
        } = fields;
        LinkStats {
            interval_ms,
            packets_received,
            packets_lost,
            underruns,
            receive_timeouts,
            jitter_ms: reportable(jitter_ms),
            cushion_ms: reportable(cushion_ms),
            backlog_ms: reportable(backlog_ms),
            recovered: recovered.map(|recovered| recovered.min(packets_received)),
        }
    }

    /// The `stats` message for one of the phone's link reports: Android's `buildStatsMessage`
    /// (`ControlChannel.kt:571-591`).
    ///
    /// - `recovered` is always carried, 0 included, as Android always sends it: the phone
    ///   announces `redundancy.recovered`, so a missing count would read as a phone that cannot
    ///   count.
    /// - The extras go through [`LinkStats::new`]'s checks: one not measured, or outside what the
    ///   desk reads, is left out, and the rest are rounded to a hundredth.
    /// - The counts are never below zero in a report the link stats accumulator makes; a report
    ///   built by hand with a negative count sends 0 for it.
    ///
    /// It never fails: the interval is held to the one range [`LinkStats::new`] refuses outside of,
    /// which changes nothing for the accumulator's reports, already clamped to it.
    pub fn from_report(report: &LinkStatsReport) -> LinkStats {
        let LinkStatsReport {
            interval_ms,
            packets_received,
            packets_lost,
            underruns,
            receive_timeouts,
            jitter_ms,
            cushion_ms,
            backlog_ms,
            recovered,
        } = *report;
        // Held to the one range `new` refuses outside of, so the message needs no refusal.
        let interval_ms = interval_ms.clamp(1, i64::from(LinkStats::MAXIMUM_INTERVAL_MS));
        // A count below zero is sent as 0; every other `i64` fits a `u64`.
        let count = |count: i64| u64::try_from(count).unwrap_or(0);
        LinkStats::checked(LinkStatsFields {
            interval_ms: u32::try_from(interval_ms).unwrap_or(LinkStats::MAXIMUM_INTERVAL_MS),
            packets_received: count(packets_received),
            packets_lost: count(packets_lost),
            underruns: count(underruns),
            receive_timeouts: count(receive_timeouts),
            jitter_ms,
            cushion_ms,
            backlog_ms,
            recovered: Some(count(recovered)),
        })
    }

    pub fn interval_ms(&self) -> u32 {
        self.interval_ms
    }

    pub fn packets_received(&self) -> u64 {
        self.packets_received
    }

    pub fn packets_lost(&self) -> u64 {
        self.packets_lost
    }

    pub fn underruns(&self) -> u64 {
        self.underruns
    }

    pub fn receive_timeouts(&self) -> u64 {
        self.receive_timeouts
    }

    /// The arrival jitter, rounded to a hundredth; `None` when not measured or not sendable.
    pub fn jitter_ms(&self) -> Option<f64> {
        self.jitter_ms
    }

    /// The playback cushion, rounded to a hundredth; `None` when not measured or not sendable.
    pub fn cushion_ms(&self) -> Option<f64> {
        self.cushion_ms
    }

    /// The audio queued for playback, rounded to a hundredth; `None` when not measured or not
    /// sendable.
    pub fn backlog_ms(&self) -> Option<f64> {
        self.backlog_ms
    }

    /// The packets a delayed copy brought, never more than `packets_received`.
    pub fn recovered(&self) -> Option<u64> {
        self.recovered
    }
}

/// A control message the phone sends the desk. One `type` per case, named by the payload itself so
/// the tag lives in exactly one place.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientMessage {
    Register(Registration),
    Mix(MixCommand),
    Stats(LinkStats),
}

impl ClientMessage {
    /// The message's `type` on the wire.
    pub fn message_type(&self) -> &'static str {
        match self {
            ClientMessage::Register(_) => Registration::MESSAGE_TYPE,
            ClientMessage::Mix(_) => MixCommand::MESSAGE_TYPE,
            ClientMessage::Stats(_) => LinkStats::MESSAGE_TYPE,
        }
    }

    /// The message as text, written as the iOS client's `JSONEncoder` wrote it: `type` first, then
    /// the fields in the order the desk documents them, a field with nothing to say left out
    /// rather than sent as null, a whole measurement without a fraction (`40`), and `/` in a
    /// string as `\/`. Never fails: every value a message holds can be written.
    pub fn to_json(&self) -> String {
        let mut object = ObjectWriter::new(Slashes::Escaped);
        object.string("type", self.message_type());
        match self {
            ClientMessage::Register(registration) => {
                object.string("name", &registration.name);
                object.unsigned("udp_port", u64::from(registration.udp_port));
                if let Some(client_id) = &registration.client_id {
                    object.string("client_id", client_id);
                }
                if !registration.capabilities.is_empty() {
                    object.strings(
                        "capabilities",
                        registration
                            .capabilities
                            .iter()
                            .map(|capability| capability.wire_name()),
                    );
                }
            }
            ClientMessage::Mix(mix) => {
                if let Some(sequence) = mix.sequence {
                    object.unsigned("seq", sequence);
                }
                object.integer("volume_percent", mix.volume_percent);
                object.integer("max_level_percent", mix.max_level_percent);
                object.boolean("muted", mix.muted);
                if let Some(channel_gains) = &mix.channel_gains {
                    object.integers("channels", channel_gains);
                }
                if let Some(pans) = &mix.pans {
                    object.integers("pans", pans);
                }
                if let Some(channel_mutes) = &mix.channel_mutes {
                    object.booleans("mutes", channel_mutes);
                }
                if let Some(channel_solos) = &mix.channel_solos {
                    object.booleans("solos", channel_solos);
                }
                if let Some(group_levels) = &mix.group_levels {
                    object.integers("group_levels", group_levels);
                }
            }
            ClientMessage::Stats(stats) => {
                object.unsigned("interval_ms", u64::from(stats.interval_ms));
                object.unsigned("packets_received", stats.packets_received);
                object.unsigned("packets_lost", stats.packets_lost);
                object.unsigned("underruns", stats.underruns);
                object.unsigned("receive_timeouts", stats.receive_timeouts);
                for (key, value) in [
                    ("jitter_ms", stats.jitter_ms),
                    ("cushion_ms", stats.cushion_ms),
                    ("backlog_ms", stats.backlog_ms),
                ] {
                    if let Some(value) = value {
                        object.number(key, value);
                    }
                }
                if let Some(recovered) = stats.recovered {
                    object.unsigned("recovered", recovered);
                }
            }
        }
        object.finish()
    }
}

/// Why a message could not be built. Each case is something the desk would refuse, caught here so
/// it never costs a round trip.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ClientMessageError {
    /// The desk refuses a registration without a port to send audio to.
    UdpPortMustNotBeZero,
    /// Outside 1 to [`LinkStats::MAXIMUM_INTERVAL_MS`]; the desk cannot normalise rates by it.
    StatsIntervalOutOfRange { interval_ms: u32 },
}

impl fmt::Display for ClientMessageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientMessageError::UdpPortMustNotBeZero => {
                write!(formatter, "a registration needs a UDP port other than 0")
            }
            ClientMessageError::StatsIntervalOutOfRange { interval_ms } => write!(
                formatter,
                "a stats interval must be from 1 to {} ms, not {interval_ms} ms",
                LinkStats::MAXIMUM_INTERVAL_MS
            ),
        }
    }
}

impl std::error::Error for ClientMessageError {}

#[cfg(test)]
mod tests {
    //! `ClientMessageTests.swift`, one test per Swift test; a test the Swift file runs once per
    //! argument runs its arguments in a loop here. The Swift tests compare what goes on the wire
    //! as a parsed object; these compare the text itself, which is now fixed.

    use super::*;
    use serde_json::Value;

    fn stats(fields: LinkStatsFields) -> LinkStats {
        LinkStats::new(fields).expect("a stats message the desk reads")
    }

    fn counts(interval_ms: u32) -> LinkStatsFields {
        LinkStatsFields {
            interval_ms,
            ..LinkStatsFields::default()
        }
    }

    /// The message's text parsed, to read one field.
    fn wire_object(message: &ClientMessage) -> serde_json::Map<String, Value> {
        match serde_json::from_str(&message.to_json()) {
            Ok(Value::Object(object)) => object,
            other => panic!("expected an object, got {other:?}"),
        }
    }

    fn registration(name: &str) -> Registration {
        Registration::new(name, 50_000, None, &[]).expect("a registration")
    }

    // register

    #[test]
    fn names_its_own_type() {
        let registration = registration("Ana");
        let mix = MixCommand::new(80, 100, false);
        let stats = stats(counts(1_000));

        assert_eq!(
            ClientMessage::Register(registration).message_type(),
            "register"
        );
        assert_eq!(ClientMessage::Mix(mix).message_type(), "mix");
        assert_eq!(ClientMessage::Stats(stats).message_type(), "stats");
    }

    #[test]
    fn sends_the_shape_an_older_phone_sends_when_it_has_no_id_and_no_capabilities() {
        assert_eq!(
            ClientMessage::Register(registration("Ana")).to_json(),
            r#"{"type":"register","name":"Ana","udp_port":50000}"#
        );
    }

    #[test]
    fn sends_the_id_and_the_capabilities_it_has_in_the_order_given() {
        let registration = Registration::new(
            "Ana",
            50_000,
            Some("3F2504E0-4F89-11D3-9A0C-0305E82C3301"),
            &[
                ClientCapability::StatsV2,
                ClientCapability::PlaybackBuffer,
                ClientCapability::RedundancyHold,
            ],
        )
        .unwrap();

        assert_eq!(
            ClientMessage::Register(registration).to_json(),
            r#"{"type":"register","name":"Ana","udp_port":50000,"client_id":"3F2504E0-4F89-11D3-9A0C-0305E82C3301","capabilities":["stats.v2","playback.buffer","redundancy.hold"]}"#
        );
    }

    #[test]
    fn trims_the_name_the_musician_typed() {
        assert_eq!(registration("  Ana  ").name(), "Ana");
    }

    /// iOS's trim also strips U+200B, which Rust's `str::trim` keeps (the T7b verification):
    /// a zero-width space pasted around a name never reached the desk, and still does not.
    #[test]
    fn trims_a_zero_width_space_around_the_name_as_ios_did() {
        assert_eq!(registration("\u{200B}Ana\u{200B}").name(), "Ana");
        assert_eq!(registration(" \u{200B} Ana \u{200B} ").name(), "Ana");
        assert_eq!(registration("\u{200B}\u{200B}").name(), "");
        assert_eq!(
            registration("An\u{200B}a").name(),
            "An\u{200B}a",
            "only the edges are trimmed"
        );
    }

    /// Android's `RegistrationMessageTest.trimsTheNameButKeepsAnEmptyOne`: a name with nothing in
    /// it still registers, with an empty name, which the desk stores as no name
    /// (`registered_name`, `apps/server/src/protocol.rs:330-336`) and still sends audio to.
    #[test]
    fn keeps_a_name_with_nothing_in_it_sent_empty() {
        for name in ["", "   ", "\t\n"] {
            let registration = Registration::new(name, 50_001, None, &[]).unwrap();

            assert_eq!(registration.name(), "", "{name:?}");
            assert_eq!(
                ClientMessage::Register(registration).to_json(),
                r#"{"type":"register","name":"","udp_port":50001}"#
            );
        }
    }

    #[test]
    fn caps_the_name_at_the_sixty_four_the_desk_keeps() {
        let registration = registration(&"a".repeat(100));

        assert_eq!(Registration::MAXIMUM_NAME_LENGTH, 64);
        assert_eq!(registration.name().chars().count(), 64);
    }

    /// The desk truncates with Rust's `chars().take(64)`, which counts Unicode scalars, so this
    /// does too. Counting Swift `Character`s instead would keep more scalars than the desk stores
    /// and the phone would show a name the desk does not have.
    #[test]
    fn counts_the_name_in_scalars_the_way_the_desk_counts_it() {
        // Three scalars joined by two zero-width joiners: 5 scalars, 1 Character.
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        let name = family.repeat(20);

        let registration = registration(&name);

        assert_eq!(name.chars().count(), 100);
        assert_eq!(registration.name().chars().count(), 64);
    }

    #[test]
    fn refuses_a_udp_port_of_zero_which_the_desk_rejects() {
        assert_eq!(
            Registration::new("Ana", 0, None, &[]),
            Err(ClientMessageError::UdpPortMustNotBeZero)
        );
    }

    #[test]
    fn drops_a_client_id_the_desk_would_not_keep() {
        for client_id in ["has space", "has_underscore", "has.dot", "", "aa\u{00E9}"] {
            let registration = Registration::new("Ana", 50_000, Some(client_id), &[]).unwrap();

            assert_eq!(registration.client_id(), None, "{client_id:?}");
        }
    }

    #[test]
    fn drops_a_client_id_longer_than_the_desk_keeps() {
        let registration = Registration::new("Ana", 50_000, Some(&"a".repeat(65)), &[]).unwrap();

        assert_eq!(Registration::MAXIMUM_CLIENT_ID_LENGTH, 64);
        assert_eq!(registration.client_id(), None);
    }

    #[test]
    fn keeps_letters_digits_and_hyphens_in_a_client_id() {
        for client_id in ["a", "A1-b2", "3F25-04E0"] {
            let registration = Registration::new("Ana", 50_000, Some(client_id), &[]).unwrap();

            assert_eq!(registration.client_id(), Some(client_id));
        }
    }

    #[test]
    fn announces_each_capability_once_keeping_the_order_given() {
        let registration = Registration::new(
            "Ana",
            50_000,
            None,
            &[
                ClientCapability::RedundancyHold,
                ClientCapability::StatsV2,
                ClientCapability::RedundancyHold,
                ClientCapability::StatsV2,
            ],
        )
        .unwrap();

        assert_eq!(
            registration.capabilities(),
            [ClientCapability::RedundancyHold, ClientCapability::StatsV2]
        );
    }

    // mix

    #[test]
    fn sends_only_the_master_values_when_no_list_changed() {
        assert_eq!(
            ClientMessage::Mix(MixCommand::new(80, 95, true)).to_json(),
            r#"{"type":"mix","volume_percent":80,"max_level_percent":95,"muted":true}"#
        );
    }

    #[test]
    fn numbers_a_mix_when_it_was_given_a_number() {
        let mix = MixCommand {
            sequence: Some(7),
            ..MixCommand::new(80, 100, false)
        };

        assert_eq!(wire_object(&ClientMessage::Mix(mix))["seq"], 7);
    }

    #[test]
    fn maps_the_names_onto_the_keys_the_desk_reads() {
        let mix = MixCommand {
            channel_gains: Some(vec![10, 20]),
            pans: Some(vec![0, 50]),
            channel_mutes: Some(vec![true, false]),
            channel_solos: Some(vec![false, true]),
            group_levels: Some(vec![30]),
            ..MixCommand::new(80, 100, false)
        };

        assert_eq!(
            ClientMessage::Mix(mix).to_json(),
            r#"{"type":"mix","volume_percent":80,"max_level_percent":100,"muted":false,"channels":[10,20],"pans":[0,50],"mutes":[true,false],"solos":[false,true],"group_levels":[30]}"#
        );
    }

    #[test]
    fn leaves_out_a_list_it_was_not_given_rather_than_sending_null() {
        let mix = MixCommand {
            channel_gains: Some(vec![10]),
            ..MixCommand::new(80, 100, false)
        };
        let object = wire_object(&ClientMessage::Mix(mix));

        assert_eq!(object["channels"], serde_json::json!([10]));
        assert_eq!(object.len(), 5);
        for absent in ["pans", "mutes", "solos", "group_levels", "seq"] {
            assert!(!object.contains_key(absent), "{absent}");
        }
    }

    // stats

    #[test]
    fn sends_the_four_counts_an_older_phone_sends() {
        let stats = stats(LinkStatsFields {
            interval_ms: 1_000,
            packets_received: 480,
            packets_lost: 2,
            underruns: 1,
            receive_timeouts: 3,
            ..LinkStatsFields::default()
        });

        assert_eq!(
            ClientMessage::Stats(stats).to_json(),
            r#"{"type":"stats","interval_ms":1000,"packets_received":480,"packets_lost":2,"underruns":1,"receive_timeouts":3}"#
        );
    }

    #[test]
    fn sends_the_extras_a_stats_v2_phone_measures() {
        let stats = stats(LinkStatsFields {
            interval_ms: 1_000,
            packets_received: 480,
            jitter_ms: Some(1.25),
            cushion_ms: Some(40.0),
            backlog_ms: Some(52.5),
            recovered: Some(2),
            ..LinkStatsFields::default()
        });

        assert_eq!(
            ClientMessage::Stats(stats).to_json(),
            r#"{"type":"stats","interval_ms":1000,"packets_received":480,"packets_lost":0,"underruns":0,"receive_timeouts":0,"jitter_ms":1.25,"cushion_ms":40,"backlog_ms":52.5,"recovered":2}"#
        );
    }

    /// Android rounds each extra to a hundredth, after checking its range on the measured value
    /// (`putMilliseconds`, `ControlChannel.kt:586`).
    #[test]
    fn rounds_a_measurement_to_a_hundredth_of_a_millisecond() {
        for (measured, sent) in [
            (1.2345, "1.23"),
            (0.006, "0.01"),
            (37.5, "37.5"),
            (59_999.996, "60000"),
        ] {
            let stats = stats(LinkStatsFields {
                jitter_ms: Some(measured),
                cushion_ms: Some(measured),
                backlog_ms: Some(measured),
                ..counts(1_000)
            });

            assert_eq!(
                ClientMessage::Stats(stats).to_json(),
                format!(
                    r#"{{"type":"stats","interval_ms":1000,"packets_received":0,"packets_lost":0,"underruns":0,"receive_timeouts":0,"jitter_ms":{sent},"cushion_ms":{sent},"backlog_ms":{sent}}}"#
                ),
                "{measured}"
            );
        }
    }

    #[test]
    fn refuses_an_interval_the_desk_rejects() {
        for interval_ms in [0, 60_001, 100_000] {
            assert_eq!(
                LinkStats::new(counts(interval_ms)),
                Err(ClientMessageError::StatsIntervalOutOfRange { interval_ms })
            );
        }
    }

    #[test]
    fn accepts_the_ends_of_the_interval_the_desk_allows() {
        for interval_ms in [1, 60_000] {
            assert_eq!(stats(counts(interval_ms)).interval_ms(), interval_ms);
        }
        assert_eq!(LinkStats::MAXIMUM_INTERVAL_MS, 60_000);
    }

    /// JSON has no non-finite number, so one would cost the whole report for a measurement the
    /// desk would have dropped anyway.
    #[test]
    fn drops_a_measurement_that_is_not_a_finite_number() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let stats = stats(LinkStatsFields {
                jitter_ms: Some(value),
                cushion_ms: Some(value),
                backlog_ms: Some(value),
                ..counts(1_000)
            });

            assert_eq!(stats.jitter_ms(), None);
            assert!(!wire_object(&ClientMessage::Stats(stats)).contains_key("jitter_ms"));
        }
    }

    #[test]
    fn drops_a_measurement_outside_the_range_the_desk_keeps_before_rounding_it() {
        for value in [-0.5, -0.004, 60_000.004, 60_000.5] {
            let stats = stats(LinkStatsFields {
                cushion_ms: Some(value),
                ..counts(1_000)
            });

            assert_eq!(LinkStats::MAXIMUM_REPORTED_MS, 60_000.0);
            assert_eq!(stats.cushion_ms(), None, "{value}");
        }
    }

    #[test]
    fn holds_recovered_to_the_packets_that_were_received_as_the_desk_does() {
        let stats = stats(LinkStatsFields {
            packets_received: 10,
            recovered: Some(25),
            ..counts(1_000)
        });

        assert_eq!(stats.recovered(), Some(10));
    }

    // Not ported: the bytes the iOS client's `JSONEncoder` wrote for what no Swift test pins.

    #[test]
    fn writes_a_name_and_an_extra_byte_for_byte_as_the_ios_client_did() {
        let registration =
            Registration::new(" AC/DC \u{1} ", 5, Some("ab"), &[ClientCapability::StatsV2])
                .unwrap();
        assert_eq!(
            ClientMessage::Register(registration).to_json(),
            r#"{"type":"register","name":"AC\/DC \u0001","udp_port":5,"client_id":"ab","capabilities":["stats.v2"]}"#
        );

        let stats = stats(LinkStatsFields {
            packets_received: 1,
            jitter_ms: Some(-0.0),
            cushion_ms: Some(0.004),
            backlog_ms: Some(40.0),
            recovered: Some(0),
            ..counts(1_000)
        });
        assert_eq!(
            ClientMessage::Stats(stats).to_json(),
            r#"{"type":"stats","interval_ms":1000,"packets_received":1,"packets_lost":0,"underruns":0,"receive_timeouts":0,"jitter_ms":-0,"cushion_ms":0,"backlog_ms":40,"recovered":0}"#
        );
    }
}

#[cfg(test)]
mod link_stats_message {
    //! `LinkStatsMessageTests.swift`: a link report as the `stats` message the desk reads, one
    //! test per Swift test.

    use super::*;
    use serde_json::Value;
    use std::collections::BTreeSet;

    /// The keys every report's message carries, `recovered` among them as on Android.
    const COUNT_KEYS: [&str; 7] = [
        "type",
        "interval_ms",
        "packets_received",
        "packets_lost",
        "underruns",
        "receive_timeouts",
        "recovered",
    ];

    fn report(interval_ms: i64) -> LinkStatsReport {
        LinkStatsReport {
            interval_ms,
            ..LinkStatsReport::default()
        }
    }

    /// `report`'s message as the desk reads it.
    fn wire(report: &LinkStatsReport) -> serde_json::Map<String, Value> {
        match serde_json::from_str(&ClientMessage::Stats(LinkStats::from_report(report)).to_json())
        {
            Ok(Value::Object(object)) => object,
            other => panic!("expected an object, got {other:?}"),
        }
    }

    fn keys(object: &serde_json::Map<String, Value>) -> BTreeSet<&str> {
        object.keys().map(String::as_str).collect()
    }

    #[test]
    fn carries_every_count_of_the_report_each_in_its_own_field() {
        let stats = LinkStats::from_report(&LinkStatsReport {
            interval_ms: 1_250,
            packets_received: 98,
            packets_lost: 2,
            underruns: 1,
            receive_timeouts: 3,
            recovered: 4,
            ..LinkStatsReport::default()
        });

        assert_eq!(
            stats,
            LinkStats::new(LinkStatsFields {
                interval_ms: 1_250,
                packets_received: 98,
                packets_lost: 2,
                underruns: 1,
                receive_timeouts: 3,
                recovered: Some(4),
                ..LinkStatsFields::default()
            })
            .unwrap()
        );
    }

    #[test]
    fn always_carries_recovered_as_android_does_even_when_nothing_was_recovered() {
        let message = wire(&LinkStatsReport {
            packets_received: 200,
            ..report(1_000)
        });

        assert_eq!(keys(&message), COUNT_KEYS.into_iter().collect());
        assert_eq!(message["recovered"], 0);
    }

    #[test]
    fn carries_the_extras_that_were_measured_to_a_hundredth_and_leaves_out_the_rest() {
        let message = wire(&LinkStatsReport {
            packets_received: 200,
            jitter_ms: Some(1.2345),
            cushion_ms: Some(10.0),
            backlog_ms: None,
            ..report(1_000)
        });

        let mut expected: BTreeSet<&str> = COUNT_KEYS.into_iter().collect();
        expected.extend(["jitter_ms", "cushion_ms"]);
        assert_eq!(keys(&message), expected);
        assert_eq!(message["jitter_ms"], 1.23);
        assert_eq!(message["cushion_ms"], 10);
    }

    #[test]
    fn sends_a_count_below_zero_as_zero_rather_than_stopping_the_app() {
        let stats = LinkStats::from_report(&LinkStatsReport {
            interval_ms: 1_000,
            packets_received: -1,
            packets_lost: -2,
            underruns: -3,
            receive_timeouts: -4,
            recovered: -5,
            ..LinkStatsReport::default()
        });

        assert_eq!(stats.packets_received(), 0);
        assert_eq!(stats.packets_lost(), 0);
        assert_eq!(stats.underruns(), 0);
        assert_eq!(stats.receive_timeouts(), 0);
        assert_eq!(stats.recovered(), Some(0));
    }

    #[test]
    fn holds_an_interval_the_desk_would_refuse_to_the_range_it_reads() {
        let interval =
            |interval_ms: i64| LinkStats::from_report(&report(interval_ms)).interval_ms();

        assert_eq!(interval(0), 1);
        assert_eq!(interval(-5), 1);
        assert_eq!(interval(1), 1);
        assert_eq!(interval(60_000), 60_000);
        assert_eq!(interval(60_001), 60_000);
        assert_eq!(interval(i64::MAX), 60_000);
    }

    /// Android's `STATS_AFTER_A_COPY_FILLED_A_GAP` (`HoldPlayoutTest.kt:113-114`): a one-second
    /// report, four packets played, one of them brought by a delayed copy. The hold makes this
    /// report (`hold_playout`'s first Android case); its message is the JSON the server's own
    /// protocol test reads (`protocol::tests::the_stats_a_phone_sends_after_a_copy_filled_a_gap`),
    /// now byte for byte.
    #[test]
    fn sends_the_report_of_a_copy_that_filled_a_gap_as_the_servers_protocol_test_reads_it() {
        let report = LinkStatsReport {
            interval_ms: 1_000,
            packets_received: 4,
            receive_timeouts: 1,
            recovered: 1,
            ..LinkStatsReport::default()
        };
        let server = r#"{"type":"stats","interval_ms":1000,"packets_received":4,"packets_lost":0,"underruns":0,"receive_timeouts":1,"recovered":1}"#;

        assert_eq!(
            ClientMessage::Stats(LinkStats::from_report(&report)).to_json(),
            server
        );
    }
}
