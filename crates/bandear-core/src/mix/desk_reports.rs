//! What the desk's messages report about the mix.

use super::session::MixEvent;
use super::state::EngineerControls;
use crate::control::server_message::{MixReport, ServerMessage};

impl MixReport {
    /// What the engineer set for this musician, as the report carries it: the limit, every
    /// channel whose `locked` entry is true, and whose mix this one follows.
    ///
    /// The decoder already read them as leniently as Android's former `parseEngineerControls`; this
    /// only turns the per-channel list into the channels it locks. A report always carries them, an
    /// older desk's as the defaults (no limit, nothing locked, following nobody), so a report
    /// always says what the engineer set.
    pub fn engineer_controls(&self) -> EngineerControls {
        EngineerControls {
            limit_percent: self.engineer_limit_percent,
            locked_channels: self
                .locked
                .iter()
                .enumerate()
                .filter(|(_, &locked)| locked)
                .map(|(channel, _)| channel as i64)
                .collect(),
            following: self.following.clone(),
        }
    }
}

impl MixEvent {
    /// What a message from the desk reports about the mix, or `None` when it reports nothing
    /// about it.
    ///
    /// Ported from what Android's control channel handed its listener before this port:
    /// - a `mix_state` is the desk's whole mix, pushed because it changed elsewhere;
    /// - a numbered `mix_ack` answers the phone's mix of that number with the mix the desk now
    ///   plays, and one whose mix could not be read answers it with nothing to show;
    /// - a numbered `error` refused the phone's mix of that number, so it answers it with nothing;
    /// - an ack or an error without a number cannot be matched to any mix: the ack only
    ///   acknowledges, and the error answered something else (a registration, a stats report).
    ///
    /// Nothing else is a report on the mix. A `config` changes the mix too, as
    /// [`MixEvent::ConfigReceived`], but it is the desk announcing its channels, not reporting a
    /// mix, so it is not mapped here.
    pub fn reported_by(message: &ServerMessage) -> Option<MixEvent> {
        match message {
            ServerMessage::MixState(report) => Some(MixEvent::MixPushed(report.clone())),
            ServerMessage::MixAck(report) => Some(MixEvent::MixAnswered {
                sequence: report.sequence?,
                report: Some(report.clone()),
            }),
            &ServerMessage::UnreadableMixAck { sequence } => Some(MixEvent::MixAnswered {
                sequence,
                report: None,
            }),
            ServerMessage::Error(error) => Some(MixEvent::MixAnswered {
                sequence: error.sequence?,
                report: None,
            }),
            ServerMessage::Configuration(_)
            | ServerMessage::PlaybackBuffer(_)
            | ServerMessage::RedundancyHold { .. }
            | ServerMessage::Unrecognized { .. } => None,
        }
    }
}
