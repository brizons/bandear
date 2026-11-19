//! The mix the phone sends, and when it sends one.

use super::more_of_me::MoreOfMeNotice;
use super::session::{MixEvent, MixSession};
use crate::control::client_message::MixCommand;

impl MixSession {
    /// The `mix` the phone sends: the whole mix as it plays it, under the number of its newest
    /// change.
    ///
    /// Ported from the iOS client's `outgoingMix()`, itself from the Android client's
    /// `outgoingMixMessage` and `buildMixMessage` (`ControlChannel.kt:415-425,470-504`):
    /// - The number is [`MixSync::latest`](super::MixSync::latest). Android reads it before the
    ///   mix, so the mix is never older than the number it goes out under; here both come from one
    ///   value and cannot disagree. Every mix is numbered, the first one too, so every answer from
    ///   the desk can be matched to one.
    /// - The master volume, the cap and the mute are the musician's own: the cap is
    ///   [`MixState::level_cap`](super::MixState::level_cap), never the engineer's limit, since the
    ///   desk applies the lower of the two itself.
    /// - The per-channel lists and the group levels are the mix as played: with More of me on, the
    ///   musician's strip is lifted and every other channel halved here, and only here, the locked
    ///   sides left as stored. The stored mix never carries the lift.
    /// - Every list goes out whole on every `mix`, as Android sends them. Before the first
    ///   `config` the lists are empty and go out empty: a list sets the desk's channels from the
    ///   first and an empty one sets none, so the desk keeps its own, and its answer gives the
    ///   phone the mix it holds (`ControlChannel.kt:370-380`, `apps/server/src/mix.rs:1003-1027`).
    /// - Nothing the engineer set goes out: [`MixCommand`] has no field for it, and the desk alone
    ///   sets it.
    pub fn outgoing_mix(&self) -> MixCommand {
        let played = self.mix_as_played();
        MixCommand {
            sequence: Some(self.sync.latest),
            volume_percent: played.master_volume,
            max_level_percent: played.level_cap,
            muted: played.master_muted,
            channel_gains: Some(played.levels),
            pans: Some(played.pans),
            channel_mutes: Some(played.mutes),
            channel_solos: Some(played.solos),
            group_levels: Some(played.group_levels),
        }
    }

    /// True when `event`, which took the session from `before` to `after`, goes to the desk as a
    /// `mix`. Ported from the rules the Android client hands its `reduceChange`
    /// (`MonitorViewModel.kt:318-321,346-348,414-416,419-425`), which its view model keeps as
    /// lambdas:
    /// - An edit is sent when it changed the mix. One that changes nothing (a value already set, a
    ///   channel that does not exist, a locked strip, any strip while the mix follows another's)
    ///   sends nothing, as a slider that did not move never did.
    /// - A `config` is sent only when it turned More of me off, so the desk stops lifting; not
    ///   when it did so because the musician's channel became part of a pair. The link's own
    ///   `mix_state` follows with the pans it moved, a send now would put the phone's old pans back
    ///   over them, and the musician's own levels go back once it arrives
    ///   (`own_levels_to_restore`).
    /// - Toggling More of me is sent when it actually turned on or off.
    /// - Choosing a channel is sent when the choice was taken while More of me was on: the mix
    ///   then lifts another channel, or none.
    /// - A recalled mix is sent unless the mix was following another musician's
    ///   ([`MixSession::recall_refusal`]), even when it changed nothing (`MonitorViewModel.kt:454`).
    ///
    /// The desk's reports and the start and end of a session are never sent: the desk already
    /// plays what it reports. The one report the phone answers with a mix, the first after a link
    /// turned More of me off, is numbered by the reducer itself ([`MixSession::reduced`]).
    pub fn sends(event: &MixEvent, before: &MixSession, after: &MixSession) -> bool {
        match event {
            MixEvent::ChannelLevelChanged { .. }
            | MixEvent::ChannelPanChanged { .. }
            | MixEvent::ChannelMuteChanged { .. }
            | MixEvent::ChannelSoloChanged { .. }
            | MixEvent::GroupLevelChanged { .. }
            | MixEvent::MasterVolumeChanged { .. }
            | MixEvent::MasterMuteChanged { .. }
            | MixEvent::LevelCapChanged { .. } => before.mix != after.mix,
            MixEvent::ConfigReceived(_) => {
                before.more_of_me.is_enabled
                    && !after.more_of_me.is_enabled
                    && after.more_of_me.notice != Some(MoreOfMeNotice::ChannelPaired)
            }
            MixEvent::MoreOfMeToggled { .. } => {
                before.more_of_me.is_enabled != after.more_of_me.is_enabled
            }
            MixEvent::MusicianChannelChosen { channel } => {
                before.more_of_me.is_enabled && after.more_of_me.musician_channel == *channel
            }
            MixEvent::MixRecalled(_) => before.recall_refusal().is_none(),
            MixEvent::SessionStarted
            | MixEvent::SessionEnded
            | MixEvent::MixPushed(_)
            | MixEvent::MixAnswered { .. } => false,
        }
    }

    /// The session after `event`, numbered as the phone's newest mix when [`MixSession::sends`]
    /// says it goes to the desk: [`MixSession::reduced_sending`] with that rule.
    ///
    /// Every event can go through it. The phone sends [`MixSession::outgoing_mix`] whenever the
    /// result's `sync.latest` grew, which covers both a change the rule sends and the report the
    /// reducer numbers itself (`MonitorViewModel.kt:346-348,390-394`).
    pub fn reduced_numbering_sends(&self, event: &MixEvent) -> MixSession {
        self.reduced_sending(event, |before, after| Self::sends(event, before, after))
    }
}
