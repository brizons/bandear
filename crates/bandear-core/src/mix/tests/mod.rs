//! The mix's tests, ported one to one from the iOS client's former `BandEarMixTests` target, one
//! module per Swift file and one `#[test]` per Swift test, with the same inputs and the same
//! expected values; the Swift suite was deleted with this port. Its parity suites ported the
//! Android client's tests case for case, so they still name the Android test each case comes from.
//! Android keeps its view model's tests (`MonitorViewModelTest`); most of the others left their
//! files with this port, and `MixSnapshotTest`, `GroupSnapshotTest` and `ChannelLayoutTest` are
//! gone.
//!
//! These are the fixtures of the former Swift suite's `Fixtures`, under the same names where Rust
//! allows. What only checks display copy (the "CH n" label, a default name) stays with the
//! platforms, which word the screen: where a Swift test checked a label, the port checks the name
//! the strip carries (`None` when the screen names it by its number). `model` is new, not ported:
//! the [`MixModel`](super::MixModel) wiring has no Swift counterpart of its own.

mod channel_layout_parity;
mod desk_reports;
mod mix_bank;
mod mix_bank_parity;
mod mix_reducer_parity;
mod mix_session;
mod mix_state;
mod model;
mod more_of_me;
mod more_of_me_parity;
mod outgoing_mix;
mod outgoing_mix_parity;

use super::more_of_me::lifted_levels;
use super::{EngineerControls, MixEvent, MixSession, MixState};
use crate::control::client_message::{ClientMessage, MixCommand};
use crate::control::mix_bank::MixSnapshot;
use crate::control::server_message::{
    ChannelGroup, ChannelKind, DeskConfiguration, MixReport, ServerMessage, SourceChannel,
};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

// The desk's channels and groups.

/// A channel's details as Android's former `ChannelInfo(name, kind)`: shown, not linked.
pub(super) fn info(name: &str, kind: ChannelKind) -> SourceChannel {
    SourceChannel {
        name: name.to_owned(),
        kind,
        is_hidden: false,
        is_linked_to_next: false,
    }
}

/// A channel with Android's former `ChannelInfo` defaults: no name, kind `other`, shown, not
/// linked.
pub(super) fn untouched() -> SourceChannel {
    SourceChannel::untouched()
}

/// The former Swift fixtures' `info(hidden:link:)` flags, set on a channel's details.
pub(super) trait Detail {
    fn hidden(self) -> Self;
    fn linked(self) -> Self;
}

impl Detail for SourceChannel {
    fn hidden(self) -> Self {
        SourceChannel {
            is_hidden: true,
            ..self
        }
    }

    fn linked(self) -> Self {
        SourceChannel {
            is_linked_to_next: true,
            ..self
        }
    }
}

/// A group as Android's former `GroupInfo(name, channels)`; it had no id there, so the name stands
/// in.
pub(super) fn group(name: &str, channels: &[i64]) -> ChannelGroup {
    ChannelGroup {
        id: Some(name.to_owned()),
        name: name.to_owned(),
        channels: channels.to_vec(),
    }
}

pub(super) fn vox() -> SourceChannel {
    info("Vox", ChannelKind::Voice)
}

pub(super) fn keys() -> SourceChannel {
    info("Keys", ChannelKind::Keys).linked()
}

pub(super) fn keys_right() -> SourceChannel {
    info("Keys R", ChannelKind::Keys)
}

pub(super) fn spare() -> SourceChannel {
    info("Spare", ChannelKind::Other).hidden()
}

/// Five channels: a voice, a keys pair on channels 1 and 2, a hidden spare and an untouched input.
/// The `band` fixture of the Android client's former `MonitorReducerTest` mix tests.
pub(super) fn band() -> Vec<SourceChannel> {
    vec![vox(), keys(), keys_right(), spare(), untouched()]
}

/// The mix a first `config` builds: neutral levels, even channels panned left and odd ones right,
/// nothing muted or soloed, every group neutral. Built here by hand rather than through the code
/// under test, so a wrong default there is caught rather than copied.
pub(super) fn first_config(
    count: usize,
    groups: Vec<ChannelGroup>,
    details: Option<Vec<SourceChannel>>,
) -> MixState {
    MixState {
        source_channels: count as i64,
        levels: vec![100; count],
        pans: (0..count)
            .map(|channel| if channel % 2 == 0 { 0 } else { 100 })
            .collect(),
        mutes: vec![false; count],
        solos: vec![false; count],
        group_levels: vec![100; groups.len()],
        groups,
        channel_details: details,
        ..MixState::default()
    }
}

/// A `config` as the desk sends it: `count` channels, no groups, no details.
pub(super) fn config(count: u8) -> DeskConfiguration {
    config_with(count, Vec::new(), None)
}

/// A `config` as the desk sends it: `count` channels, its groups, and its details or none.
pub(super) fn config_with(
    count: u8,
    groups: Vec<ChannelGroup>,
    details: Option<Vec<SourceChannel>>,
) -> DeskConfiguration {
    DeskConfiguration {
        source_channels: count,
        sample_rate: None,
        groups,
        channels: details,
    }
}

// The events, under short names.

pub(super) fn configure(config: DeskConfiguration) -> MixEvent {
    MixEvent::ConfigReceived(config)
}

pub(super) fn set_level(channel: i64, level: i64) -> MixEvent {
    MixEvent::ChannelLevelChanged { channel, level }
}

pub(super) fn set_pan(channel: i64, pan: i64) -> MixEvent {
    MixEvent::ChannelPanChanged { channel, pan }
}

pub(super) fn set_mute(channel: i64, muted: bool) -> MixEvent {
    MixEvent::ChannelMuteChanged { channel, muted }
}

pub(super) fn set_solo(channel: i64, soloed: bool) -> MixEvent {
    MixEvent::ChannelSoloChanged { channel, soloed }
}

pub(super) fn set_group_level(group: i64, level: i64) -> MixEvent {
    MixEvent::GroupLevelChanged { group, level }
}

pub(super) fn set_master(level: i64) -> MixEvent {
    MixEvent::MasterVolumeChanged { level }
}

pub(super) fn set_master_mute(muted: bool) -> MixEvent {
    MixEvent::MasterMuteChanged { muted }
}

pub(super) fn set_cap(level: i64) -> MixEvent {
    MixEvent::LevelCapChanged { level }
}

pub(super) fn toggle(on: bool) -> MixEvent {
    MixEvent::MoreOfMeToggled { on }
}

pub(super) fn choose(channel: i64) -> MixEvent {
    MixEvent::MusicianChannelChosen {
        channel: Some(channel),
    }
}

pub(super) fn choose_none() -> MixEvent {
    MixEvent::MusicianChannelChosen { channel: None }
}

pub(super) fn push(report: MixReport) -> MixEvent {
    MixEvent::MixPushed(report)
}

pub(super) fn answer(sequence: u64, report: Option<MixReport>) -> MixEvent {
    MixEvent::MixAnswered { sequence, report }
}

pub(super) fn recall(snapshot: MixSnapshot) -> MixEvent {
    MixEvent::MixRecalled(snapshot)
}

// Sessions.

/// The former Swift suite's `MixSession` test extensions.
pub(super) trait Events {
    /// The session after `events`, in order: Android's `after` helper.
    fn after(&self, events: &[MixEvent]) -> MixSession;
    /// `event` as the musician's change that the phone sends, numbered as its newest mix: Android's
    /// `sent` helper over its former `reduceChange`.
    fn sent(&self, event: MixEvent) -> MixSession;
    /// The levels the desk plays while More of me is on, `locked` channels as stored: Android's
    /// former `mix.toSnapshot().moreOfMeFor(moreOfMeChannels, locked).channelGains`.
    fn played(&self, locked: &BTreeSet<i64>) -> Vec<i64>;
}

impl Events for MixSession {
    fn after(&self, events: &[MixEvent]) -> MixSession {
        events
            .iter()
            .fold(self.clone(), |session, event| session.reduced(event))
    }

    fn sent(&self, event: MixEvent) -> MixSession {
        self.reduced_sending(&event, |_, _| true)
    }

    fn played(&self, locked: &BTreeSet<i64>) -> Vec<i64> {
        lifted_levels(&self.mix.levels, &self.more_of_me_channels(), locked)
    }
}

/// No channel locked.
pub(super) fn unlocked() -> BTreeSet<i64> {
    BTreeSet::new()
}

/// A session whose first `config` announced `count` channels and `groups`, with no details.
pub(super) fn with_channels(count: u8, groups: Vec<ChannelGroup>) -> MixSession {
    MixSession::default().after(&[configure(config_with(count, groups, None))])
}

/// A session whose first `config` announced the five-channel [`band`].
pub(super) fn with_band(groups: Vec<ChannelGroup>) -> MixSession {
    MixSession::default().after(&[configure(config_with(5, groups, Some(band())))])
}

/// A session that started and has heard nothing from its desk yet: Android's `running()`.
pub(super) fn running() -> MixSession {
    MixSession::default().after(&[MixEvent::SessionStarted])
}

/// A running session whose first `config` announced `count` channels and `groups`.
pub(super) fn connected_with(count: u8, groups: Vec<ChannelGroup>) -> MixSession {
    running().after(&[configure(config_with(count, groups, None))])
}

// The desk's reports.

/// What the engineer set: a limit, locked channels, whom the mix follows.
pub(super) fn controls(limit: i64, locked: &[i64], following: Option<&str>) -> EngineerControls {
    EngineerControls {
        limit_percent: limit,
        locked_channels: locked.iter().copied().collect(),
        following: following.map(str::to_owned),
    }
}

/// A whole mix as the desk reports it, with the defaults of Android's former `MixSnapshot`: no
/// lists, master and cap at 100, unmuted. The engineer set nothing, as an older desk reports. The
/// tests set its other values with struct update syntax, as the Swift tests' arguments did.
pub(super) fn report() -> MixReport {
    MixReport {
        sequence: None,
        volume_percent: 100,
        max_level_percent: 100,
        muted: false,
        channel_gains: Vec::new(),
        pans: Vec::new(),
        channel_mutes: Vec::new(),
        channel_solos: Vec::new(),
        group_levels: Vec::new(),
        // 100 is no limit: what an older desk's report reads as.
        engineer_limit_percent: 100,
        locked: Vec::new(),
        following: None,
    }
}

/// The mix as the phone holds it, reported back whole: Android's former `MixUiState.toSnapshot()`.
/// With `gains`, the report carries them in place of the stored levels, as the desk reports the
/// lift it plays while More of me is on.
pub(super) fn report_of(mix: &MixState, gains: Option<Vec<i64>>) -> MixReport {
    MixReport {
        channel_gains: gains.unwrap_or_else(|| mix.levels.clone()),
        pans: mix.pans.clone(),
        channel_mutes: mix.mutes.clone(),
        channel_solos: mix.solos.clone(),
        group_levels: mix.group_levels.clone(),
        volume_percent: mix.master_volume,
        max_level_percent: mix.level_cap,
        muted: mix.master_muted,
        ..report()
    }
}

/// The former Swift fixtures' `MixReport.with(_:)`.
pub(super) trait WithControls {
    /// This report carrying the engineer's `controls`, as the desk writes them: a limit, one
    /// `locked` entry per channel up to the last locked one, and whom the mix follows. Android
    /// hands the controls beside the mix (`MixPushed(snapshot, engineer)`); the desk sends them
    /// inside it.
    fn with(self, controls: &EngineerControls) -> MixReport;
}

impl WithControls for MixReport {
    fn with(self, controls: &EngineerControls) -> MixReport {
        let locked_count = controls
            .locked_channels
            .iter()
            .max()
            .map_or(0, |last| last + 1);
        MixReport {
            engineer_limit_percent: controls.limit_percent,
            locked: (0..locked_count)
                .map(|channel| controls.is_locked(channel))
                .collect(),
            following: controls.following.clone(),
            ..self
        }
    }
}

// What the phone sends.

/// A `mix` as the desk reads it: the JSON the core's `ClientMessage::Mix` writes, parsed back, so a
/// test checks keys and values and never their order. Android's tests read the sent `JSONObject`
/// the same way.
pub(super) struct WireMix {
    pub object: Map<String, Value>,
}

impl WireMix {
    pub fn new(command: &MixCommand) -> WireMix {
        let text = ClientMessage::Mix(command.clone()).to_json();
        match serde_json::from_str(&text) {
            Ok(Value::Object(object)) => WireMix { object },
            other => panic!("a mix is a JSON object, not {other:?}"),
        }
    }

    /// The whole numbers under `key`, or `None` when there is no such list: Android's `ints(key)`.
    pub fn ints(&self, key: &str) -> Option<Vec<i64>> {
        self.object
            .get(key)?
            .as_array()?
            .iter()
            .map(Value::as_i64)
            .collect()
    }

    pub fn bools(&self, key: &str) -> Option<Vec<bool>> {
        self.object
            .get(key)?
            .as_array()?
            .iter()
            .map(Value::as_bool)
            .collect()
    }

    pub fn int(&self, key: &str) -> Option<i64> {
        self.object.get(key)?.as_i64()
    }

    pub fn has(&self, key: &str) -> bool {
        self.object.contains_key(key)
    }
}

/// The phone as the Android client's `MonitorViewModelTest` drove it, with the socket left out.
///
/// Every event goes through [`MixSession::reduced_numbering_sends`], and whenever that numbered a
/// new mix the phone sends [`MixSession::outgoing_mix`] as the session then stands. Android's fake
/// scheduler ran each send at once, so `sent` is what its fake socket records as
/// `sentOfType("mix")`.
#[derive(Default)]
pub(super) struct Phone {
    pub session: MixSession,
    /// Every `mix` sent, in order: the opening one, then one per numbered change.
    pub sent: Vec<MixCommand>,
    /// How many of `sent` the desk has answered.
    answered: usize,
}

impl Phone {
    /// Android's `connected(channels)`: the session starts, the socket opens and sends the opening
    /// mix, and then the desk's `config` announces its channels.
    pub fn connected(config: DeskConfiguration) -> Phone {
        let mut phone = Phone::default();
        phone.apply(&[MixEvent::SessionStarted]);
        phone.opened();
        phone.apply(&[configure(config)]);
        phone
    }

    /// The socket opened: the phone sends its mix as it stands, under its newest number, and
    /// numbers nothing new.
    pub fn opened(&mut self) {
        self.sent.push(self.session.outgoing_mix());
    }

    pub fn apply(&mut self, events: &[MixEvent]) {
        for event in events {
            let before = self.session.sync.latest;
            self.session = self.session.reduced_numbering_sends(event);
            if self.session.sync.latest != before {
                self.sent.push(self.session.outgoing_mix());
            }
        }
    }

    /// The desk's message `object`, read as the phone reads it. One that reports nothing about the
    /// mix changes nothing.
    pub fn receive(&mut self, object: Value) {
        let message = ServerMessage::parse(&object.to_string()).expect("the desk's message reads");
        if let Some(event) = MixEvent::reported_by(&message) {
            self.apply(&[event]);
        }
    }

    /// Android's `answerMixes`: the desk answers every mix not answered yet, in order, each with
    /// the mix as sent under its own number.
    pub fn answer_mixes(&mut self) {
        let unanswered = self.sent[self.answered..].to_vec();
        self.answered = self.sent.len();
        for mix in unanswered {
            let mut answer = WireMix::new(&mix).object;
            answer.insert("type".to_owned(), Value::from("mix_ack"));
            self.receive(Value::Object(answer));
        }
    }

    pub fn last_mix(&self) -> WireMix {
        WireMix::new(self.sent.last().expect("the phone sent a mix"))
    }
}
