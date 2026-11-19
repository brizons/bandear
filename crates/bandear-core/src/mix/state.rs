//! The musician's mix: the per-channel lists and the master values, plus what the desk announced
//! about its channels and what the engineer set, and the strips and faders derived from them.

use super::channel_layout::{self, element, StereoPans, StripChannels};
use crate::control::mix_bank::MixSnapshot;
use crate::control::server_message::{ChannelGroup, ChannelKind, SourceChannel};
use std::collections::BTreeSet;

/// What the engineer set for this musician: a limit, locked channels, and whose mix this one
/// follows. The desk alone sets it; the phone never sends or saves it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EngineerControls {
    /// Caps what the musician hears, 0 to 100; 100 when the engineer set no limit.
    pub limit_percent: i64,
    pub locked_channels: BTreeSet<i64>,
    /// The musician whose mix this one mirrors, or `None`.
    pub following: Option<String>,
}

impl Default for EngineerControls {
    /// No limit, nothing locked, following nobody: what an older desk, which reports none, means.
    fn default() -> Self {
        EngineerControls {
            limit_percent: 100,
            locked_channels: BTreeSet::new(),
            following: None,
        }
    }
}

impl EngineerControls {
    /// True when the engineer set a limit below full scale.
    pub fn is_limited(&self) -> bool {
        self.limit_percent < 100
    }

    /// True when the engineer locked `channel`.
    pub fn is_locked(&self, channel: i64) -> bool {
        self.locked_channels.contains(&channel)
    }
}

/// A group as a strip's chip names it; `index` picks the chip's tone.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GroupTag {
    pub index: i64,
    pub name: String,
}

/// One channel strip as the musician sees it: a single channel, or a stereo pair read from its
/// first channel with its pan shown as a balance.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ChannelStrip {
    /// The channel the strip is known by: a pair's first channel.
    pub index: i64,
    /// The engineer's name for it, without the whitespace around it, or `None`: the screen then
    /// labels the strip by its number ("CH n"), in its own words.
    pub name: Option<String>,
    /// The first announced group that lists the channel, or `None`.
    pub group: Option<GroupTag>,
    pub level: i64,
    /// 0 hard left, 50 centre, 100 hard right; a pair's balance, read the same way.
    pub pan: i64,
    pub is_muted: bool,
    pub is_soloed: bool,
    /// What the channel carries; `None` when the desk announced no details, so no icon shows.
    pub kind: Option<ChannelKind>,
    /// The pair's second channel, or `None` for a single channel.
    pub paired_with: Option<i64>,
    /// The engineer locked the strip (a pair when either side is): it cannot be changed here.
    pub is_locked: bool,
    /// The musician this mix follows, or `None`: while set, the strip is the leader's.
    pub following: Option<String>,
}

impl ChannelStrip {
    /// True for a stereo pair's strip.
    pub fn is_stereo(&self) -> bool {
        self.paired_with.is_some()
    }
}

/// A channel by the index its strip is known by, with the engineer's name for it: the channels
/// the musician can call theirs, and a group fader's members. `None` when it has no name, and the
/// screen then names it by its number ("CH n"). Swift's `ChannelChoice` and a fader's member
/// labels, without the words.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NamedChannel {
    pub index: i64,
    pub name: Option<String>,
}

/// One group fader. `channels` are the desk's 0-based channels in the group; `members` names its
/// strips as they are shown, a hidden channel left out and a stereo pair named once.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GroupFader {
    pub index: i64,
    pub name: String,
    pub channels: Vec<i64>,
    pub level: i64,
    pub members: Vec<NamedChannel>,
    /// The musician this mix follows, or `None`: while set, the level is the leader's.
    pub following: Option<String>,
}

/// The musician's mix: the per-channel lists and the master values, plus what the desk announced
/// about its channels and what the engineer set.
///
/// Ported from the iOS client's `MixState`, itself from the Android client's `MixUiState`
/// (`MonitorUiState.kt:125-256`). The stored values are plain lists, index-aligned with the desk's
/// channels; every strip the musician sees is derived from them and the desk's details, so a pair
/// or a hidden channel never has a stored shape of its own to fall out of step.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MixState {
    /// Source channels in the desk's last `config`; 0 before the first one.
    pub source_channels: i64,
    pub levels: Vec<i64>,
    pub pans: Vec<i64>,
    pub mutes: Vec<bool>,
    pub solos: Vec<bool>,
    /// The groups the desk announced, in its order; a group's index is the index of its level.
    pub groups: Vec<ChannelGroup>,
    pub group_levels: Vec<i64>,
    pub master_volume: i64,
    pub master_muted: bool,
    /// The ceiling the phone never plays above, in percent.
    pub level_cap: i64,
    /// One per source channel, or `None` when the desk's `config` carried none (an older desk).
    pub channel_details: Option<Vec<SourceChannel>>,
    pub engineer: EngineerControls,
}

impl Default for MixState {
    /// The mix before the first `config`: no channels or groups, the master and the cap neutral,
    /// unmuted, nothing the engineer set.
    fn default() -> Self {
        MixState {
            source_channels: 0,
            levels: Vec::new(),
            pans: Vec::new(),
            mutes: Vec::new(),
            solos: Vec::new(),
            groups: Vec::new(),
            group_levels: Vec::new(),
            master_volume: MixState::NEUTRAL_LEVEL,
            master_muted: false,
            level_cap: MixState::NEUTRAL_LEVEL,
            channel_details: None,
            engineer: EngineerControls::default(),
        }
    }
}

impl MixState {
    /// The level a new channel or group starts at, and the one a missing level reads as.
    pub const NEUTRAL_LEVEL: i64 = 100;

    /// The loudest the musician hears: the lower of the phone's cap and the engineer's limit.
    pub fn effective_cap(&self) -> i64 {
        self.level_cap.min(self.engineer.limit_percent)
    }

    /// Whose mix this one follows, or `None`. While set, the channels and groups are the leader's
    /// and only the master, the cap and the mute are the phone's.
    pub fn following(&self) -> Option<&str> {
        self.engineer.following.as_deref()
    }

    /// Which channels each strip controls, left to right.
    pub fn layout(&self) -> Vec<StripChannels> {
        channel_layout::strips(self.levels.len() as i64, self.channel_details.as_deref())
    }

    /// One strip per shown channel or stereo pair, left to right.
    pub fn strips(&self) -> Vec<ChannelStrip> {
        self.layout()
            .into_iter()
            .map(|channels| self.strip(channels))
            .collect()
    }

    /// One fader per announced group; a group with no level yet reads neutral. Without details
    /// from the desk its members are its channels by number, as they always were.
    pub fn group_faders(&self) -> Vec<GroupFader> {
        let layout = self.layout();
        self.groups
            .iter()
            .enumerate()
            .map(|(index, group)| {
                let members = if self.channel_details.is_none() {
                    group
                        .channels
                        .iter()
                        .map(|&channel| NamedChannel {
                            index: channel,
                            name: None,
                        })
                        .collect()
                } else {
                    layout
                        .iter()
                        .filter(|strip| {
                            strip
                                .sides()
                                .iter()
                                .any(|side| group.channels.contains(side))
                        })
                        .map(|strip| self.named(strip.first))
                        .collect()
                };
                GroupFader {
                    index: index as i64,
                    name: group.name.clone(),
                    channels: group.channels.clone(),
                    level: self
                        .group_levels
                        .get(index)
                        .copied()
                        .unwrap_or(Self::NEUTRAL_LEVEL),
                    members,
                    following: self.engineer.following.clone(),
                }
            })
            .collect()
    }

    /// The channels the musician can call theirs: one per strip of the *announced* channels.
    /// Before the first `config` there are none, whatever the lists hold.
    pub fn musician_choices(&self) -> Vec<NamedChannel> {
        channel_layout::strips(self.source_channels, self.channel_details.as_deref())
            .into_iter()
            .map(|strip| self.named(strip.first))
            .collect()
    }

    /// Every channel the phone must leave as it is: each one the engineer locked, and both sides
    /// of a pair when either side is, so a pair is never split by an edit, a recall or More of me.
    pub fn locked_sides(&self) -> BTreeSet<i64> {
        let mut locked = self.engineer.locked_channels.clone();
        for strip in self.layout() {
            let sides = strip.sides();
            if sides.iter().any(|&side| self.engineer.is_locked(side)) {
                locked.extend(sides);
            }
        }
        locked
    }

    /// The engineer's name for `channel`'s strip, without the whitespace around it, or `None`
    /// when it has none: Swift's `label(of:)` without the "CH n" words, which are the screen's.
    pub fn name_of(&self, channel: i64) -> Option<String> {
        channel_layout::name(self.detail(channel))
    }

    /// The channels an edit of `channel` writes: both sides when it is the first of a pair's
    /// strip, otherwise `channel` alone. Matched by the first channel only, exactly as Android
    /// matches it.
    pub fn strip_sides(&self, channel: i64) -> Vec<i64> {
        self.layout()
            .into_iter()
            .find(|strip| strip.first == channel)
            .map_or_else(|| vec![channel], |strip| strip.sides())
    }

    /// True when the engineer locked `channel`'s strip: a pair is locked when either side is.
    pub fn is_strip_locked(&self, channel: i64) -> bool {
        self.strip_sides(channel)
            .into_iter()
            .any(|side| self.engineer.is_locked(side))
    }

    /// This mix as the phone saves it: Android's `MixUiState.toSnapshot()`
    /// (`MonitorUiState.kt:246-255`). The stored mix holds the musician's own levels, so a mix
    /// saved while More of me is on is saved without the lift (`MonitorViewModel.kt:427`).
    pub fn snapshot(&self) -> MixSnapshot {
        MixSnapshot {
            channel_gains: self.levels.clone(),
            channel_pans: self.pans.clone(),
            channel_mutes: self.mutes.clone(),
            channel_solos: self.solos.clone(),
            group_levels: self.group_levels.clone(),
            volume_percent: self.master_volume,
            max_level_percent: self.level_cap,
            muted: self.master_muted,
        }
    }

    fn strip(&self, channels: StripChannels) -> ChannelStrip {
        let index = channels.first;
        let pan = match channels.second {
            Some(second) => channel_layout::balance(StereoPans {
                first: self.pan_of(index),
                second: self.pan_of(second),
            }),
            None => self.pan_of(index),
        };
        ChannelStrip {
            index,
            name: self.name_of(index),
            group: self.group_tag(index),
            // The layout is made from the levels, so every strip has its level.
            level: element(&self.levels, index)
                .copied()
                .unwrap_or(Self::NEUTRAL_LEVEL),
            pan,
            is_muted: element(&self.mutes, index).copied().unwrap_or(false),
            is_soloed: element(&self.solos, index).copied().unwrap_or(false),
            kind: self.channel_details.as_ref().map(|_| {
                self.detail(index)
                    .map_or(ChannelKind::Other, |detail| detail.kind)
            }),
            paired_with: channels.second,
            is_locked: channels
                .sides()
                .into_iter()
                .any(|side| self.engineer.is_locked(side)),
            following: self.engineer.following.clone(),
        }
    }

    fn named(&self, channel: i64) -> NamedChannel {
        NamedChannel {
            index: channel,
            name: self.name_of(channel),
        }
    }

    fn pan_of(&self, channel: i64) -> i64 {
        element(&self.pans, channel)
            .copied()
            .unwrap_or_else(|| channel_layout::default_pan(channel))
    }

    fn detail(&self, channel: i64) -> Option<&SourceChannel> {
        element(self.channel_details.as_deref()?, channel)
    }

    /// The first announced group that lists `channel`: the one its strip's chip names.
    fn group_tag(&self, channel: i64) -> Option<GroupTag> {
        self.groups
            .iter()
            .position(|group| group.channels.contains(&channel))
            .map(|index| GroupTag {
                index: index as i64,
                name: self.groups[index].name.clone(),
            })
    }
}
