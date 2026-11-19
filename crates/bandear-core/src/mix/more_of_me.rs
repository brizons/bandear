//! More of me: the musician's channel lifted to full level and every other channel halved, the
//! rules that turn it off, and why it is not doing what the musician last asked.
//!
//! Ported from the iOS client's `MoreOfMe.swift`. The notices' words are the screen's, so only
//! the reasons live here.

use super::channel_layout::{self, element};
use super::state::MixState;
use std::collections::BTreeSet;

/// Why More of me is not doing what the musician last asked; the toggle's caption says it.
///
/// Ported case for case from the Android client's `MoreOfMeNotice` (`MonitorUiState.kt:335-352`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MoreOfMeNotice {
    /// Asked to turn on before a channel was chosen.
    NeedsChannel,
    /// The musician chose no channel while More of me was on, so it went off.
    NoChannelSelected,
    /// The musician's channel left the mix (past the announced channels, or hidden), so it went
    /// off.
    ChannelOutOfRange,
    /// The desk reported a mix changed elsewhere; the phone shows it as played, so nothing is
    /// lifted.
    MixChangedElsewhere,
    /// The engineer locked the musician's channel, so the phone cannot lift it.
    ChannelLocked,
    /// The mix follows another musician's, whose channels the desk keeps, so nothing is lifted.
    Following,
    /// The engineer made the musician's channel part of a stereo pair; the lift the desk plays is
    /// one side's.
    ChannelPaired,
}

/// More of me and the channel it lifts.
///
/// The musician's channel says who the musician is rather than how the mix sounds, so it lives
/// apart from the mix; remembering it across launches is the app's. Ported from the Android
/// client's `MoreOfMeUiState` (`MonitorUiState.kt:354-361`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MoreOfMeState {
    pub is_enabled: bool,
    /// The channel the musician calls theirs, by the channel its strip is known by; `None` when
    /// none is chosen.
    pub musician_channel: Option<i64>,
    /// Why More of me is not doing what the musician last asked, or `None`.
    pub notice: Option<MoreOfMeNotice>,
}

/// The level the musician's channel is lifted to.
const LIFTED_LEVEL: i64 = 100;

/// `levels` with the musician's channels lifted to 100 and every other channel halved, except
/// the `locked_channels`, which stay as they are: the desk keeps a locked channel as the engineer
/// left it, so it is played and sent as stored.
///
/// The lift applies to the mix the phone plays and sends, never to the stored mix, so turning More
/// of me off gives back the mix exactly as the musician left it, with nothing to save or replay.
///
/// `musician_channels` is the musician's strip: one channel, or both sides of a stereo pair,
/// lifted alike so that the desk, which keeps a pair's sides equal, plays exactly what was sent.
/// The first one decides: when the levels have no such channel nothing is lifted and the levels
/// come back as they are. Android returns null there instead, and every caller of it then plays
/// and sends the stored mix (`ControlChannel.kt:423`, `MonitorReducer.kt:519`), which is what
/// this returns. A second side or a locked channel the levels do not have is ignored.
///
/// Ported from `MixSnapshot.moreOfMe` and `moreOfMeFor` (`MixSnapshot.kt:44-54`,
/// `ClientMixState.kt:77-83`).
pub fn lifted_levels(
    levels: &[i64],
    musician_channels: &[i64],
    locked_channels: &BTreeSet<i64>,
) -> Vec<i64> {
    match musician_channels.first() {
        Some(&first) if element(levels, first).is_some() => levels
            .iter()
            .enumerate()
            .map(|(channel, &level)| {
                let channel = channel as i64;
                if locked_channels.contains(&channel) {
                    level
                } else if musician_channels.contains(&channel) {
                    LIFTED_LEVEL
                } else {
                    halved(level)
                }
            })
            .collect(),
        _ => levels.to_vec(),
    }
}

/// Half of `level`, a half rounded up.
///
/// Android computes `(gain * 0.5).roundToInt()`, and Kotlin rounds a half toward positive
/// infinity. Half a whole number is whole or a half, so rounding it up is Kotlin's rule exactly,
/// for any level, negative ones included. Done in whole numbers (Swift goes through a `Double`),
/// so it is exact for every `i64`.
fn halved(level: i64) -> i64 {
    level.div_euclid(2) + level.rem_euclid(2)
}

// The rules that turn More of me off, and what the musician asks of it.

impl MoreOfMeState {
    /// The musician turned More of me on or off. It turns on only with a channel to lift: asking
    /// without one, while the mix follows another's, or while the engineer locked the channel,
    /// leaves it off and says why (`MonitorReducer.kt:365-378`).
    pub(crate) fn toggled(self, on: bool, mix: &MixState) -> MoreOfMeState {
        if !on {
            return self.switched(false, None);
        }
        let Some(channel) = self.musician_channel else {
            return self.switched(false, Some(MoreOfMeNotice::NeedsChannel));
        };
        if mix.following().is_some() {
            return self.switched(false, Some(MoreOfMeNotice::Following));
        }
        if mix.is_strip_locked(channel) {
            return self.switched(false, Some(MoreOfMeNotice::ChannelLocked));
        }
        self.switched(true, None)
    }

    /// `channel` taken as the musician's, or none (`MonitorReducer.kt:380-395`). Only a channel
    /// the picker offers can be chosen: one the desk announced, shown, and known by its strip (a
    /// pair by its first channel); any other changes nothing. Choosing none while More of me is on
    /// turns it off, since nothing is left to lift; so does choosing a channel the engineer
    /// locked. The musician can still call a locked channel theirs.
    pub(crate) fn choosing(self, channel: Option<i64>, mix: &MixState) -> MoreOfMeState {
        if let Some(channel) = channel {
            if !mix
                .musician_choices()
                .iter()
                .any(|choice| choice.index == channel)
            {
                return self;
            }
        }
        if channel.is_none() && self.is_enabled {
            return MoreOfMeState {
                is_enabled: false,
                musician_channel: None,
                notice: Some(MoreOfMeNotice::NoChannelSelected),
            };
        }
        MoreOfMeState {
            is_enabled: self.is_enabled,
            musician_channel: channel,
            notice: None,
        }
        .off_if_held(mix)
    }

    /// The musician's channel fitted to the strips of the channels a `config` announced in `mix`
    /// (`MonitorReducer.kt:305-321`). A channel on a pair's second side becomes the pair's first,
    /// the channel its strip is known by, and More of me keeps lifting the pair. A channel with no
    /// strip (past the announced count, or hidden, and so in no mix) is forgotten: More of me, if
    /// it was on, goes off and says why; a channel that was only remembered is dropped quietly.
    ///
    /// Only a `config` calls this, and only once it announced at least one channel, so the
    /// announced strips are never the empty ones Android guards against.
    pub(crate) fn fitted(self, mix: &MixState) -> MoreOfMeState {
        let Some(channel) = self.musician_channel else {
            return self;
        };
        let strip = channel_layout::strips(mix.source_channels, mix.channel_details.as_deref())
            .into_iter()
            .find(|strip| strip.sides().contains(&channel));
        match strip {
            Some(strip) => MoreOfMeState {
                musician_channel: Some(strip.first),
                ..self
            },
            None => MoreOfMeState {
                is_enabled: false,
                musician_channel: None,
                notice: if self.is_enabled {
                    Some(MoreOfMeNotice::ChannelOutOfRange)
                } else {
                    self.notice
                },
            },
        }
    }

    /// Off, saying why, when it is on and a `config` just made the musician's channel part of a
    /// stereo pair, on either side: on the second, the channel has just moved to the first
    /// (`MonitorReducer.kt:257-273`). `previous_channel` was the musician's in the mix `before`.
    ///
    /// The desk plays the lift it was sent, one side's, and the link gives the second side the
    /// first side's level and pans the pair apart, so the pair's lift would differ from what the
    /// desk plays and the `mix_state` the link sends next would not match. Turned on again, More of
    /// me lifts both sides. A pair the musician's strip already was, or one elsewhere, changes
    /// nothing.
    pub(crate) fn off_if_newly_paired(
        self,
        before: &MixState,
        previous_channel: Option<i64>,
        after: &MixState,
    ) -> MoreOfMeState {
        let (true, Some(channel), Some(previous_channel)) =
            (self.is_enabled, self.musician_channel, previous_channel)
        else {
            return self;
        };
        let sides = after.strip_sides(channel);
        let newly_paired = sides.len() == 2 && sides != before.strip_sides(previous_channel);
        if newly_paired {
            self.switched(false, Some(MoreOfMeNotice::ChannelPaired))
        } else {
            self
        }
    }

    /// Off, saying why, when it is on and the desk would discard its lift
    /// (`MonitorReducer.kt:290-303`): the mix follows another musician's, whose channels the desk
    /// keeps, or the engineer locked the musician's channel, which the desk keeps as the engineer
    /// left it.
    pub(crate) fn off_if_held(self, mix: &MixState) -> MoreOfMeState {
        let (true, Some(channel)) = (self.is_enabled, self.musician_channel) else {
            return self;
        };
        if mix.following().is_some() {
            return self.switched(false, Some(MoreOfMeNotice::Following));
        }
        if mix.is_strip_locked(channel) {
            return self.switched(false, Some(MoreOfMeNotice::ChannelLocked));
        }
        self
    }

    /// This state turned on or off with `notice`, the musician's channel kept.
    fn switched(self, is_enabled: bool, notice: Option<MoreOfMeNotice>) -> MoreOfMeState {
        MoreOfMeState {
            is_enabled,
            musician_channel: self.musician_channel,
            notice,
        }
    }
}
