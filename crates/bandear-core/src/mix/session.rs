//! The phone's side of the mix for one session, and the pure reducer over it.

use super::channel_layout::{self, element};
use super::more_of_me::{lifted_levels, MoreOfMeNotice, MoreOfMeState};
use super::state::{EngineerControls, MixState};
use super::sync::MixSync;
use crate::control::mix_bank::MixSnapshot;
use crate::control::server_message::{DeskConfiguration, MixReport};

/// Something that changes the mix: the musician's edits, and what the desk announces.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MixEvent {
    /// The desk's `config`: its channels, groups and channel details.
    ConfigReceived(DeskConfiguration),
    ChannelLevelChanged {
        channel: i64,
        level: i64,
    },
    ChannelPanChanged {
        channel: i64,
        pan: i64,
    },
    ChannelMuteChanged {
        channel: i64,
        muted: bool,
    },
    ChannelSoloChanged {
        channel: i64,
        soloed: bool,
    },
    GroupLevelChanged {
        group: i64,
        level: i64,
    },
    MasterVolumeChanged {
        level: i64,
    },
    MasterMuteChanged {
        muted: bool,
    },
    LevelCapChanged {
        level: i64,
    },
    /// A session with a desk started. What the engineer set belongs to the last one, so it goes,
    /// and so do levels a link left to put back; the mix's numbers stay (see [`MixSync`]).
    SessionStarted,
    /// The session ended: a report that arrives after it changes nothing.
    SessionEnded,
    /// The desk pushed the musician's whole mix (`mix_state`), because it changed elsewhere.
    MixPushed(MixReport),
    /// The desk answered the phone's mix number `sequence`: with the whole mix it now plays, or
    /// with `None` when it refused that mix or its answer could not be read.
    MixAnswered {
        sequence: u64,
        report: Option<MixReport>,
    },
    /// The musician turned More of me on or off.
    MoreOfMeToggled {
        on: bool,
    },
    /// The musician called a channel theirs, by the channel its strip is known by, or none.
    MusicianChannelChosen {
        channel: Option<i64>,
    },
    /// The musician recalled a saved mix: it applies as if every control had been moved by hand,
    /// unless [`MixSession::recall_refusal`] says why it cannot.
    MixRecalled(MixSnapshot),
}

/// Why a saved mix cannot be recalled now. The screen words it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RecallRefusal {
    /// The mix follows `leader`'s, whose channels and groups the desk keeps, so a saved mix is
    /// not recalled at all, not even its master. `leader` may be empty: the desk names nobody.
    Following { leader: String },
}

/// The phone's side of the mix for one session, and the pure reducer over it.
///
/// Ported from the iOS client's former `MixSession`, itself from the mix branches of the Android
/// client's `MonitorReducer`, which reduced the whole app's state (it keeps only the UI half now);
/// only the mix lives here. Every change is a new value from the old one and an event, so each rule
/// is tested by the events that reach it, exactly as Android's suite did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MixSession {
    pub mix: MixState,
    /// How the desk's answers are matched to the mixes this phone sent.
    pub sync: MixSync,
    /// True from [`MixEvent::SessionStarted`] until [`MixEvent::SessionEnded`]. A report belongs
    /// to a session, so one that arrives outside a session changes neither the mix nor what the
    /// engineer set.
    pub is_session_running: bool,
    /// More of me and the musician's channel. The lift is never stored: `mix` always holds the
    /// musician's own levels, and the lifted ones are derived when the mix is played or sent.
    pub more_of_me: MoreOfMeState,
    /// The musician's own levels, kept when a link turned More of me off, to put back over the
    /// lift the desk still plays once its next report arrives; `None` when nothing waits. Not
    /// shown.
    pub own_levels_to_restore: Option<Vec<i64>>,
}

impl MixSession {
    /// The channels More of me lifts: the musician's channel and, when its strip is a stereo pair,
    /// the other side too; empty when no channel is chosen.
    pub fn more_of_me_channels(&self) -> Vec<i64> {
        self.more_of_me
            .musician_channel
            .map_or_else(Vec::new, |channel| self.mix.strip_sides(channel))
    }

    /// [`reduced`](Self::reduced) for a change the musician made. When `sends` says the change goes
    /// to the desk (it is handed the session before and after the change), the same step numbers
    /// it as the phone's newest mix ([`MixSync::changed`]): the change and its number land in one
    /// value, so a send still queued already counts, and no answer from the desk can be matched to
    /// a mix older than the one on screen.
    ///
    /// A change that is sent also cancels levels still waiting to be put back after a link
    /// (`own_levels_to_restore`): the mix holds the musician's own unlifted levels, this change
    /// included, so the send already puts them on the desk, and restoring the older ones when its
    /// answer arrives would undo the change.
    ///
    /// Ported from Android's former `reduceChange`. Which changes are sent is
    /// [`MixSession::sends`], and [`MixSession::reduced_numbering_sends`] applies it.
    pub fn reduced_sending(
        &self,
        event: &MixEvent,
        sends: impl FnOnce(&MixSession, &MixSession) -> bool,
    ) -> MixSession {
        let mut next = self.reduced(event);
        if sends(self, &next) {
            next.sync = next.sync.changed();
            next.own_levels_to_restore = None;
        }
        next
    }

    /// The session after `event`.
    pub fn reduced(&self, event: &MixEvent) -> MixSession {
        let mut next = self.clone();
        match event {
            MixEvent::ConfigReceived(config) => next.apply_config(config),
            &MixEvent::ChannelLevelChanged { channel, level } => {
                next.edit_strip(channel, |mix, sides| {
                    replace(&mut mix.levels, sides, percent(level));
                });
            }
            &MixEvent::ChannelPanChanged { channel, pan } => {
                next.edit_strip(channel, |mix, sides| {
                    write_pan(&mut mix.pans, sides, percent(pan));
                });
            }
            &MixEvent::ChannelMuteChanged { channel, muted } => {
                next.edit_strip(channel, |mix, sides| replace(&mut mix.mutes, sides, muted));
            }
            &MixEvent::ChannelSoloChanged { channel, soloed } => {
                next.edit_strip(channel, |mix, sides| replace(&mut mix.solos, sides, soloed));
            }
            &MixEvent::GroupLevelChanged { group, level } => {
                next.set_group_level(group, percent(level));
            }
            // The master, the mute and the cap stay the musician's whatever the engineer set: a
            // lock or a mix being followed never reaches them.
            &MixEvent::MasterVolumeChanged { level } => next.mix.master_volume = percent(level),
            &MixEvent::MasterMuteChanged { muted } => next.mix.master_muted = muted,
            &MixEvent::LevelCapChanged { level } => next.mix.level_cap = percent(level),
            MixEvent::SessionStarted => {
                // This desk reports its own controls (an older one never does, and leaves all
                // open). A new session sends the phone's own mix when it opens, so no lift waits to
                // be dropped.
                next.is_session_running = true;
                next.mix.engineer = EngineerControls::default();
                next.own_levels_to_restore = None;
            }
            MixEvent::SessionEnded => next.is_session_running = false,
            MixEvent::MixPushed(report) => {
                // A push while a change of the phone's is on its way is overtaken by that change's
                // answer; the engineer's controls it carries hold at once.
                next.take_engineer_controls(report.engineer_controls());
                if next.sync.is_settled() {
                    next.apply_desk_mix(report);
                }
            }
            MixEvent::MixAnswered { sequence, report } => next.answer(*sequence, report.as_ref()),
            &MixEvent::MoreOfMeToggled { on } => {
                next.more_of_me = self.more_of_me.toggled(on, &self.mix);
            }
            &MixEvent::MusicianChannelChosen { channel } => {
                next.more_of_me = self.more_of_me.choosing(channel, &self.mix);
            }
            MixEvent::MixRecalled(snapshot) => {
                if self.recall_refusal().is_none() {
                    next.mix = self.mix.recalling(snapshot);
                }
            }
        }
        next
    }

    /// Why a saved mix cannot be recalled now, or `None` when it can: a recall is refused while the
    /// mix follows another musician's. The reducer refuses [`MixEvent::MixRecalled`] by it,
    /// [`MixSession::sends`] sends nothing by it, and the phone reads it to say why.
    pub fn recall_refusal(&self) -> Option<RecallRefusal> {
        self.mix.following().map(|leader| RecallRefusal::Following {
            leader: leader.to_owned(),
        })
    }

    /// The mix as the phone plays and sends it ([`MixSession::outgoing_mix`]), which is what the
    /// desk reports back when nothing changed elsewhere: with More of me on, the musician's strip
    /// lifted and every other channel halved, except the channels the engineer locked (a pair whole
    /// when either side is), which stay as stored.
    pub(crate) fn mix_as_played(&self) -> MixState {
        let mut played = self.mix.clone();
        if self.more_of_me.is_enabled {
            played.levels = lifted_levels(
                &self.mix.levels,
                &self.more_of_me_channels(),
                &self.mix.locked_sides(),
            );
        }
        played
    }

    /// The desk answered the phone's change number `sequence`, with the whole mix it now plays or
    /// with nothing (it refused the mix, or its answer could not be read). The desk answers in
    /// order, so every older change is answered too, even outside a session: the numbers only
    /// grow.
    ///
    /// Only the answer to the newest change is shown: an older one's mix is overtaken by a change
    /// still on its way, whose own answer follows. The engineer's controls of any readable answer
    /// hold at once; a refusal carries none and leaves them as they were.
    fn answer(&mut self, sequence: u64, report: Option<&MixReport>) {
        let answers_newest = sequence == self.sync.latest;
        self.sync = self.sync.answered_up_to(sequence);
        let Some(report) = report else {
            return;
        };
        self.take_engineer_controls(report.engineer_controls());
        if answers_newest {
            self.apply_desk_mix(report);
        }
    }

    /// The engineer's `controls` as the desk reported them with a mix. They hold at once, even when
    /// the mix they came with is overtaken by a change of the phone's still on its way: the phone
    /// never sends them, so nothing of its own can overtake them, and the desk reports them in
    /// order. Outside a session they change nothing.
    ///
    /// More of me goes off when the musician's channel is now locked or the mix now follows
    /// another's. When following stops, its notice goes; More of me stays off until the musician
    /// turns it on.
    fn take_engineer_controls(&mut self, controls: EngineerControls) {
        if !self.is_session_running || self.mix.engineer == controls {
            return;
        }
        let was_following = self.mix.following().is_some();
        self.mix.engineer = controls;
        if was_following
            && self.mix.following().is_none()
            && self.more_of_me.notice == Some(MoreOfMeNotice::Following)
        {
            self.more_of_me.notice = None;
        }
        self.more_of_me = self.more_of_me.off_if_held(&self.mix);
    }

    /// The desk's whole mix, pushed because it changed elsewhere or answering the phone's newest
    /// change: it becomes the phone's mix, exactly as the desk plays it.
    ///
    /// Outside a session it changes nothing. The first report after a link turned More of me off
    /// puts the musician's own levels back ([`MixSession::restore_own_levels`]). A report of the
    /// mix the phone already plays (an answer to its own mix, or the whole mix the desk re-sends
    /// after it missed events) changes nothing: that is the echo check. Any other report turns
    /// More of me off and says why: its levels are what the desk plays, so lifting them again on
    /// the next send would lift them twice.
    fn apply_desk_mix(&mut self, report: &MixReport) {
        if !self.is_session_running {
            return;
        }
        let reported = self.mix.taking_desk_mix(report);
        if let Some(own) = self.own_levels_to_restore.take() {
            self.restore_own_levels(&own, reported);
            return;
        }
        if reported == self.mix_as_played() {
            return;
        }
        self.mix = reported;
        if self.more_of_me.is_enabled {
            self.more_of_me.is_enabled = false;
            self.more_of_me.notice = Some(MoreOfMeNotice::MixChangedElsewhere);
        }
    }

    /// The first report after a link turned More of me off ([`MoreOfMeNotice::ChannelPaired`]).
    ///
    /// The `config` turned it off, kept the musician's `own` levels and sent nothing; the link's
    /// `mix_state` (or the answer to a change made meanwhile) arrives with the pans the link moved
    /// and the levels the desk plays, still lifted by the mix More of me sent. The phone takes that
    /// report with its own levels put back on every channel but a locked one, which the desk keeps
    /// (a pair whole when either side is), and with a pair's second side at its first side's level,
    /// as the link sets it. When that differs from what the desk plays, it is numbered as the
    /// phone's newest mix, so it is sent once through the usual numbering. While the mix follows
    /// another's the desk keeps every channel, so the report is taken as it is. Either way nothing
    /// waits any more (the caller already took `own_levels_to_restore`).
    fn restore_own_levels(&mut self, own: &[i64], reported: MixState) {
        if reported.following().is_some() {
            self.mix = reported;
            return;
        }
        let locked = reported.locked_sides();
        let mut levels: Vec<i64> = reported
            .levels
            .iter()
            .enumerate()
            .map(|(channel, &level)| {
                let own_level = own.get(channel).copied();
                match own_level {
                    Some(own_level) if !locked.contains(&(channel as i64)) => own_level,
                    _ => level,
                }
            })
            .collect();
        for strip in reported.layout() {
            if let Some(second) = strip.second.filter(|second| !locked.contains(second)) {
                // The layout is made from the levels, so both sides have one.
                if let Some(&first) = element(&levels, strip.first) {
                    replace(&mut levels, &[second], first);
                }
            }
        }
        let changed = levels != reported.levels;
        self.mix = reported;
        self.mix.levels = levels;
        if changed {
            self.sync = self.sync.changed();
        }
    }

    /// A `config`: resizes the per-channel lists when the channel count changed, takes the
    /// announced groups and details (which rebuild the strips), keeps group levels by position, and
    /// fits the musician's channel to the new strips.
    ///
    /// A channel that still exists keeps what the musician set, a new one starts neutral with the
    /// default pan layout, and a repeated count changes nothing. Nothing is sent: a pair the
    /// engineer just linked was panned apart on the desk, and the `mix_state` that follows says so.
    /// When that link turns More of me off, the stored levels are the musician's own (the lift is
    /// never stored), so they are kept to be put back once that `mix_state` arrives
    /// ([`MixSession::apply_desk_mix`]).
    ///
    /// A `config` for no channels, or for more than the mix holds, is ignored, as Android's former
    /// parser ignored it before its reducer ever saw it.
    fn apply_config(&mut self, config: &DeskConfiguration) {
        let count = usize::from(config.source_channels);
        if !(1..=DeskConfiguration::MAXIMUM_MIX_CHANNELS).contains(&count) {
            return;
        }
        let before = self.mix.clone();
        let mix = &mut self.mix;

        if !(mix.source_channels == count as i64 && mix.levels.len() == count) {
            mix.source_channels = count as i64;
            mix.levels = resized(&mix.levels, count, |_| MixState::NEUTRAL_LEVEL);
            mix.pans = resized(&mix.pans, count, |channel| {
                channel_layout::default_pan(channel as i64)
            });
            mix.mutes = resized(&mix.mutes, count, |_| false);
            mix.solos = resized(&mix.solos, count, |_| false);
        }
        mix.group_levels = resized(&mix.group_levels, config.groups.len(), |_| {
            MixState::NEUTRAL_LEVEL
        });
        mix.groups = config.groups.clone();
        mix.channel_details = config.channels.clone();

        let fitted = self.more_of_me.fitted(&self.mix).off_if_newly_paired(
            &before,
            self.more_of_me.musician_channel,
            &self.mix,
        );
        if self.more_of_me.is_enabled
            && !fitted.is_enabled
            && fitted.notice == Some(MoreOfMeNotice::ChannelPaired)
        {
            self.own_levels_to_restore = Some(self.mix.levels.clone());
        }
        self.more_of_me = fitted.off_if_held(&self.mix);
    }

    /// Applies `change` with the channels an edit of `channel` writes: both sides of a pair's
    /// strip, otherwise the channel alone. A channel with no entry changes nothing, and neither
    /// does a strip the engineer locked (a pair when either side is) or any strip while the mix
    /// follows another's: the desk would keep those values anyway, so the phone's mix never
    /// carries one it keeps.
    fn edit_strip(&mut self, channel: i64, change: impl FnOnce(&mut MixState, &[i64])) {
        if element(&self.mix.levels, channel).is_none()
            || self.mix.following().is_some()
            || self.mix.is_strip_locked(channel)
        {
            return;
        }
        let sides = self.mix.strip_sides(channel);
        change(&mut self.mix, &sides);
    }

    /// Sets one group's level, rebuilding the list to one entry per announced group with the
    /// others as they read on screen. While the mix follows another's, the desk keeps every group
    /// level from the leader's, so nothing changes.
    fn set_group_level(&mut self, group: i64, level: i64) {
        let Some(group) = usize::try_from(group)
            .ok()
            .filter(|&group| group < self.mix.groups.len())
        else {
            return;
        };
        if self.mix.following().is_some() {
            return;
        }
        let mut levels = resized(&self.mix.group_levels, self.mix.groups.len(), |_| {
            MixState::NEUTRAL_LEVEL
        });
        levels[group] = level;
        self.mix.group_levels = levels;
    }
}

impl MixState {
    /// This mix with the desk's `report` applied to the channels and groups the desk announced. The
    /// report holds the desk's full tables (32 channels, 16 groups) and the rest is not shown.
    /// Before the first `config` the strips the phone shows keep their count. A value the report
    /// lacks keeps what the phone has, and levels and pans stay between 0 and 100. What the
    /// engineer set is not taken here: it holds on its own, before the mix is weighed at all.
    fn taking_desk_mix(&self, report: &MixReport) -> MixState {
        let announced = self.source_channels > 0;
        let channel_count = if announced {
            usize::try_from(self.source_channels).unwrap_or(0)
        } else {
            self.levels.len()
        };
        let group_count = if announced {
            self.groups.len()
        } else {
            self.group_levels.len()
        };
        let percents = |values: &[i64]| values.iter().copied().map(percent).collect::<Vec<_>>();

        MixState {
            levels: resized(&percents(&report.channel_gains), channel_count, |c| {
                self.levels.get(c).copied().unwrap_or(Self::NEUTRAL_LEVEL)
            }),
            pans: resized(&percents(&report.pans), channel_count, |c| {
                self.pans
                    .get(c)
                    .copied()
                    .unwrap_or_else(|| channel_layout::default_pan(c as i64))
            }),
            mutes: resized(&report.channel_mutes, channel_count, |c| {
                self.mutes.get(c).copied().unwrap_or(false)
            }),
            solos: resized(&report.channel_solos, channel_count, |c| {
                self.solos.get(c).copied().unwrap_or(false)
            }),
            group_levels: resized(&percents(&report.group_levels), group_count, |g| {
                self.group_levels
                    .get(g)
                    .copied()
                    .unwrap_or(Self::NEUTRAL_LEVEL)
            }),
            master_volume: percent(report.volume_percent),
            level_cap: percent(report.max_level_percent),
            master_muted: report.muted,
            ..self.clone()
        }
    }

    /// This mix with the saved `snapshot` applied, as if every control had been moved by hand. What
    /// the desk announced (the channel count, the groups, the details) and what the engineer set
    /// are kept; the strips take the saved mix's size until the desk announces again, and a saved
    /// list shorter than its levels reads as the default pans and nothing muted or soloed. A
    /// channel the engineer locked keeps the level, pan, mute and solo it has, as a moved control
    /// on it would, and a pair is locked whole ([`MixState::locked_sides`]). The values are taken
    /// as saved, as Android took them: the phone only saves values it holds.
    fn recalling(&self, snapshot: &MixSnapshot) -> MixState {
        let count = snapshot.channel_gains.len();
        let locked = self.locked_sides();
        let kept = |channel: usize, current: &[i64], saved: i64| -> i64 {
            match current.get(channel) {
                Some(&value) if locked.contains(&(channel as i64)) => value,
                _ => saved,
            }
        };
        let kept_flag = |channel: usize, current: &[bool], saved: bool| -> bool {
            match current.get(channel) {
                Some(&value) if locked.contains(&(channel as i64)) => value,
                _ => saved,
            }
        };

        MixState {
            levels: (0..count)
                .map(|c| kept(c, &self.levels, snapshot.channel_gains[c]))
                .collect(),
            pans: (0..count)
                .map(|c| {
                    let saved = snapshot
                        .channel_pans
                        .get(c)
                        .copied()
                        .unwrap_or_else(|| channel_layout::default_pan(c as i64));
                    kept(c, &self.pans, saved)
                })
                .collect(),
            mutes: (0..count)
                .map(|c| {
                    let saved = snapshot.channel_mutes.get(c).copied().unwrap_or(false);
                    kept_flag(c, &self.mutes, saved)
                })
                .collect(),
            solos: (0..count)
                .map(|c| {
                    let saved = snapshot.channel_solos.get(c).copied().unwrap_or(false);
                    kept_flag(c, &self.solos, saved)
                })
                .collect(),
            group_levels: snapshot.group_levels.clone(),
            master_volume: snapshot.volume_percent,
            level_cap: snapshot.max_level_percent,
            master_muted: snapshot.muted,
            ..self.clone()
        }
    }
}

/// `value` held to 0-100.
fn percent(value: i64) -> i64 {
    value.clamp(0, 100)
}

/// The first `count` elements of `list`, then `filling(index)` for each index past its end.
fn resized<T: Clone>(list: &[T], count: usize, filling: impl Fn(usize) -> T) -> Vec<T> {
    (0..count)
        .map(|index| list.get(index).cloned().unwrap_or_else(|| filling(index)))
        .collect()
}

/// Writes `value` at each of `indices` the list has.
///
/// Android's former reducer wrote by index unguarded and would throw on a list shorter than the
/// levels. The lists are always resized together, so only a hand-built state gets here, but a
/// reducer behind a screen must not be able to crash the app over it.
fn replace<T: Clone>(list: &mut [T], indices: &[i64], value: T) {
    for &index in indices {
        if let Some(slot) = usize::try_from(index).ok().and_then(|i| list.get_mut(i)) {
            *slot = value.clone();
        }
    }
}

/// The pans with `pan` written for `sides`: a single channel takes it as it is, a pair takes it as
/// its balance, written as both pans.
fn write_pan(pans: &mut [i64], sides: &[i64], pan: i64) {
    let &[first, second] = sides else {
        replace(pans, sides, pan);
        return;
    };
    let both = channel_layout::pans(pan);
    replace(pans, &[first], both.first);
    replace(pans, &[second], both.second);
}
