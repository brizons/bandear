//! The musician's mix across the binding: one [`MixModel`] per phone, holding the session's mix
//! and the saved mixes, from `bandear_core::mix`.
//!
//! The phone hands every change in (the musician's edits, the desk's `config` and reports, the
//! session starting and ending) and gets back, from the same call, the compact state to show, the
//! `mix` to send if the change numbered one, and whether the musician's channel changed. Called
//! once per change, never per packet: a fader reports only whole-number moves, so at most about
//! sixty calls a second, which UniFFI carries easily on both platforms.
//!
//! The model lives behind a lock, and each call changes it and reads it under one hold: a change
//! and the number it goes out under land together, so no thread can see a state whose number and
//! mix disagree, and whoever calls from several threads gets the states in the order the calls
//! took the lock. The core's plain types cross as UniFFI remote types; the state the screens read
//! is a record of its own ([`MixModelState`]), numbers and enums plus the names that are data
//! (the engineer's channel and group names, the leader's, the saved mixes'). The words ("CH 1",
//! the notices) stay with each platform. Nothing here panics: the core does not on any input.

use bandear_core::control::client_message::MixCommand;
use bandear_core::control::mix_bank::{MixSnapshot, SavedMix};
use bandear_core::control::server_message::{
    ChannelKind, DeskConfiguration, MixReport, ServerMessage,
};
use bandear_core::mix::{
    self, ChannelStrip, GroupFader, GroupTag, MixBankOutcome, MixEvent, MixSync, MoreOfMeNotice,
    MoreOfMeState, NamedChannel, RecallOutcome, RecallRefusal,
};
use std::sync::{Mutex, MutexGuard, PoisonError};

/// The musician's mix on one phone: the session's mix and the saved mixes, changed one call at a
/// time. One per phone, which keeps it for as long as the app runs; a new session with a desk is
/// [`MixEvent::SessionStarted`], not a new model, so the mix's numbers keep growing across
/// reconnects.
#[derive(Debug, uniffi::Object)]
pub struct MixModel {
    inner: Mutex<mix::MixModel>,
}

#[uniffi::export]
impl MixModel {
    /// The model at launch: the musician's channel an earlier launch remembered (a `config` then
    /// keeps it, moves it to its pair's first side, or forgets it), and the mixes the phone keeps,
    /// none active.
    #[uniffi::constructor]
    pub fn new(musician_channel: Option<i64>, saved_mixes: Vec<SavedMix>) -> Self {
        Self {
            inner: Mutex::new(mix::MixModel::new(musician_channel, saved_mixes)),
        }
    }

    /// The mix after `event`: the state to show, the `mix` to send when the event numbered one,
    /// and whether the musician's channel changed (the phone then remembers it).
    pub fn apply(&self, event: MixEvent) -> MixUpdate {
        let mut model = self.lock();
        let update = model.apply(&event);
        MixUpdate::new(update, &model)
    }

    /// The `mix` every connection opens with: the mix as it stands, under its newest number,
    /// numbering nothing new.
    pub fn opening_mix(&self) -> MixCommand {
        self.lock().opening_mix()
    }

    /// The state to show, as it stands.
    pub fn state(&self) -> MixModelState {
        MixModelState::of(&self.lock())
    }

    /// The saved mixes, in the order they were saved: what the phone keeps across launches.
    pub fn saved_mixes(&self) -> Vec<SavedMix> {
        self.lock().bank().mixes().to_vec()
    }

    /// The saved mixes as the phone's store now holds them; the active mix stays active only while
    /// it is still saved.
    pub fn reload_saved_mixes(&self, mixes: Vec<SavedMix>) -> MixModelState {
        let mut model = self.lock();
        model.reload_saved_mixes(mixes);
        MixModelState::of(&model)
    }

    /// Saves the mix as it stands (the musician's own levels, never the More of me lift) under
    /// `name` without the whitespace around it, replacing a mix of that name, and makes it the
    /// active one. A blank name is refused.
    pub fn save_mix(&self, name: String) -> MixBankChange {
        let mut model = self.lock();
        let change = model.save_mix(&name);
        MixBankChange::new(change, &model)
    }

    /// Renames the mix saved as `name` to `new_name` without the whitespace around it; refused for
    /// a blank name, a mix that is gone, or a name another mix has.
    pub fn rename_mix(&self, name: String, new_name: String) -> MixBankChange {
        let mut model = self.lock();
        let change = model.rename_mix(&name, &new_name);
        MixBankChange::new(change, &model)
    }

    /// Deletes the mix saved as `name` without the whitespace around it.
    pub fn delete_mix(&self, name: String) -> MixBankChange {
        let mut model = self.lock();
        let change = model.delete_mix(&name);
        MixBankChange::new(change, &model)
    }

    /// Recalls the mix saved as `name`: applied as if every control had been moved by hand, sent,
    /// and made the active one; refused while the mix follows another musician's, and missing when
    /// no mix has that name.
    pub fn recall_mix(&self, name: String) -> MixRecall {
        let mut model = self.lock();
        let recall = model.recall_mix(&name);
        MixRecall {
            outcome: recall.outcome,
            update: MixUpdate::new(recall.update, &model),
        }
    }
}

impl MixModel {
    /// The model, even after a call panicked while holding it: each call changes it in one
    /// assignment, so it is whole between calls, and no call panics.
    fn lock(&self) -> MutexGuard<'_, mix::MixModel> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What a message from the desk reports about the mix, or `None` when it reports nothing about
/// it: a `mix_state` is a push, a numbered `mix_ack` or `error` answers the phone's mix of that
/// number (an unreadable ack or an error with nothing to show), and anything else, an unnumbered
/// ack or error included, is not a report on the mix. A `config` changes the mix as
/// [`MixEvent::ConfigReceived`], which the phone hands over itself.
#[uniffi::export]
pub fn mix_event_reported_by(message: ServerMessage) -> Option<MixEvent> {
    MixEvent::reported_by(&message)
}

/// What one change to the mix asks of the phone, with the state it left, read under the same
/// lock.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MixUpdate {
    /// The state to show after the change.
    pub state: MixModelState,
    /// The `mix` to send, when the change numbered one; its `sequence` is `state.sync.latest`.
    pub command: Option<MixCommand>,
    /// The musician's channel changed: the phone remembers `state.more_of_me.musician_channel`.
    pub musician_channel_changed: bool,
}

impl MixUpdate {
    fn new(update: mix::MixUpdate, model: &mix::MixModel) -> Self {
        Self {
            state: MixModelState::of(model),
            command: update.command,
            musician_channel_changed: update.musician_channel_changed,
        }
    }
}

/// A change to the saved mixes: what it did, whether the list the phone keeps changed (the phone
/// writes it only then), and the state it left.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MixBankChange {
    pub outcome: MixBankOutcome,
    pub saved_mixes_changed: bool,
    pub state: MixModelState,
}

impl MixBankChange {
    fn new(change: mix::MixBankChange, model: &mix::MixModel) -> Self {
        Self {
            outcome: change.outcome,
            saved_mixes_changed: change.saved_mixes_changed,
            state: MixModelState::of(model),
        }
    }
}

/// A recall: what it did, and what it asks of the phone.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MixRecall {
    pub outcome: RecallOutcome,
    pub update: MixUpdate,
}

/// The mix as the screens show it: the strips and group faders, the master section, More of me,
/// the saved mixes' names and how the phone's mixes stand with the desk. Numbers and enums, plus
/// the names that are data; every word is the screen's.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MixModelState {
    /// From the session's start until its end; a report outside one changes nothing.
    pub is_session_running: bool,
    /// Source channels in the desk's last `config`; 0 before the first one.
    pub source_channels: i64,
    /// One strip per shown channel or stereo pair, left to right.
    pub strips: Vec<ChannelStrip>,
    /// One fader per announced group.
    pub group_faders: Vec<GroupFader>,
    /// The channels the musician can call theirs, by the channel each strip is known by.
    pub musician_choices: Vec<NamedChannel>,
    pub master_volume: i64,
    pub master_muted: bool,
    /// The phone's own cap, what the musician set.
    pub level_cap: i64,
    /// The loudest the musician hears: the lower of the cap and the engineer's limit. What the
    /// phone plays at.
    pub effective_cap: i64,
    /// The engineer's limit, 100 when none is set.
    pub engineer_limit_percent: i64,
    /// The engineer set a limit below full scale.
    pub is_limited: bool,
    /// The musician whose mix this one follows, or `None`: while set, only the master, the cap and
    /// the mute are the musician's.
    pub following: Option<String>,
    pub more_of_me: MoreOfMeState,
    /// Why a saved mix cannot be recalled now, or `None` when it can.
    pub recall_refusal: Option<RecallRefusal>,
    /// The newest number the phone sent and the newest the desk answered.
    pub sync: MixSync,
    /// The desk answered the newest mix: nothing of the phone's is on its way.
    pub is_settled: bool,
    /// The saved mixes' names, in the order they were saved.
    pub saved_mix_names: Vec<String>,
    /// The mix last saved or recalled, while it is still saved.
    pub active_mix_name: Option<String>,
}

impl MixModelState {
    fn of(model: &mix::MixModel) -> Self {
        let session = model.session();
        let mix = &session.mix;
        Self {
            is_session_running: session.is_session_running,
            source_channels: mix.source_channels,
            strips: mix.strips(),
            group_faders: mix.group_faders(),
            musician_choices: mix.musician_choices(),
            master_volume: mix.master_volume,
            master_muted: mix.master_muted,
            level_cap: mix.level_cap,
            effective_cap: mix.effective_cap(),
            engineer_limit_percent: mix.engineer.limit_percent,
            is_limited: mix.engineer.is_limited(),
            following: mix.engineer.following.clone(),
            more_of_me: session.more_of_me,
            recall_refusal: session.recall_refusal(),
            sync: session.sync,
            is_settled: session.sync.is_settled(),
            saved_mix_names: model.bank().names(),
            active_mix_name: model.bank().active_name().map(str::to_owned),
        }
    }
}

/// Something that changes the mix: the musician's edits, and what the desk announces.
#[uniffi::remote(Enum)]
pub enum MixEvent {
    /// The desk's `config`: its channels, groups and channel details.
    ConfigReceived(DeskConfiguration),
    /// A strip's level, by the channel the strip is known by; held to 0-100.
    ChannelLevelChanged {
        channel: i64,
        level: i64,
    },
    /// A strip's pan (a pair's balance), 0 left, 50 centre, 100 right; held to 0-100.
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
    /// A group's level, by its position in the `config`; held to 0-100.
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
    /// The phone's own cap; held to 0-100.
    LevelCapChanged {
        level: i64,
    },
    /// A session with a desk started: what the engineer set belongs to the last one, so it goes.
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
    /// A saved mix applied as if every control had been moved by hand. `MixModel::recall_mix`
    /// recalls one by name and makes it the active one.
    MixRecalled(MixSnapshot),
}

/// Why More of me is not doing what the musician last asked; the screen words it.
#[uniffi::remote(Enum)]
pub enum MoreOfMeNotice {
    /// Asked to turn on before a channel was chosen.
    NeedsChannel,
    /// The musician chose no channel while More of me was on, so it went off.
    NoChannelSelected,
    /// The musician's channel left the mix (past the announced channels, or hidden), so it went
    /// off.
    ChannelOutOfRange,
    /// The desk reported a mix changed elsewhere, so nothing is lifted.
    MixChangedElsewhere,
    /// The engineer locked the musician's channel, so the phone cannot lift it.
    ChannelLocked,
    /// The mix follows another musician's, so nothing is lifted.
    Following,
    /// The engineer made the musician's channel part of a stereo pair.
    ChannelPaired,
}

/// More of me and the channel it lifts.
#[uniffi::remote(Record)]
pub struct MoreOfMeState {
    pub is_enabled: bool,
    /// The channel the musician calls theirs, by the channel its strip is known by.
    pub musician_channel: Option<i64>,
    /// Why More of me is not doing what the musician last asked, or `None`.
    pub notice: Option<MoreOfMeNotice>,
}

/// Why a saved mix cannot be recalled now; the screen words it.
#[uniffi::remote(Enum)]
pub enum RecallRefusal {
    /// The mix follows `leader`'s (who may be unnamed, an empty string).
    Following { leader: String },
}

/// What a recall did; the screen words it.
#[uniffi::remote(Enum)]
pub enum RecallOutcome {
    Recalled {
        name: String,
    },
    Refused {
        name: String,
        refusal: RecallRefusal,
    },
    /// No mix is saved as `name`.
    Missing {
        name: String,
    },
}

/// What a change to the saved mixes did, or why it did nothing; the screen words it.
#[uniffi::remote(Enum)]
pub enum MixBankOutcome {
    /// Saved under `name`, the name as typed without the whitespace around it.
    Saved {
        name: String,
    },
    Renamed {
        from: String,
        to: String,
    },
    /// The mix called `name` is gone, as asked, whether or not one was saved.
    Deleted {
        name: String,
    },
    /// A rename to the name the mix already has: nothing to do, and nothing to say.
    Unchanged,
    /// The name was blank, so nothing was saved or renamed.
    NameNeeded,
    /// No mix is saved as `name`.
    Missing {
        name: String,
    },
    /// Another mix already has `name`, so the rename was refused.
    NameTaken {
        name: String,
    },
}

/// How the desk's answers stand against the mixes the phone sent.
#[uniffi::remote(Record)]
pub struct MixSync {
    /// The number of the newest change the phone made to send; 0 before the first.
    pub latest: u64,
    /// The newest number the desk answered.
    pub answered: u64,
}

/// One channel strip: a single channel, or a stereo pair read from its first channel.
#[uniffi::remote(Record)]
pub struct ChannelStrip {
    /// The channel the strip is known by: a pair's first channel.
    pub index: i64,
    /// The engineer's name for it, or `None`: the screen then labels it by its number.
    pub name: Option<String>,
    /// The first announced group that lists the channel.
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
    /// The engineer locked the strip (a pair when either side is).
    pub is_locked: bool,
    /// The musician this mix follows, or `None`.
    pub following: Option<String>,
}

/// A group as a strip's chip names it; `index` picks the chip's tone.
#[uniffi::remote(Record)]
pub struct GroupTag {
    pub index: i64,
    pub name: String,
}

/// One group fader: its channels, its level, and the strips it holds as they are shown.
#[uniffi::remote(Record)]
pub struct GroupFader {
    pub index: i64,
    pub name: String,
    pub channels: Vec<i64>,
    pub level: i64,
    pub members: Vec<NamedChannel>,
    /// The musician this mix follows, or `None`.
    pub following: Option<String>,
}

/// A channel by the index its strip is known by, with the engineer's name for it, or `None`
/// when the screen names it by its number.
#[uniffi::remote(Record)]
pub struct NamedChannel {
    pub index: i64,
    pub name: Option<String>,
}

#[cfg(test)]
mod tests {
    //! The binding's own surface: the calls return what the core's model returns, under one lock.
    //! The rules are tested in `bandear_core::mix`.

    use super::*;
    use bandear_core::control::server_message::DeskError;
    use std::sync::Arc;

    fn config(channels: u8) -> MixEvent {
        MixEvent::ConfigReceived(DeskConfiguration {
            source_channels: channels,
            sample_rate: None,
            groups: Vec::new(),
            channels: None,
        })
    }

    fn connected(channels: u8) -> MixModel {
        let model = MixModel::new(None, Vec::new());
        model.apply(MixEvent::SessionStarted);
        model.apply(config(channels));
        model
    }

    #[test]
    fn a_change_that_sends_carries_the_state_it_left_and_its_number() {
        let model = connected(2);

        let update = model.apply(MixEvent::ChannelLevelChanged {
            channel: 1,
            level: 40,
        });

        let command = update.command.clone().expect("the change is sent");
        assert_eq!(command.sequence, Some(update.state.sync.latest));
        assert_eq!(command.channel_gains, Some(vec![100, 40]));
        assert_eq!(
            update
                .state
                .strips
                .iter()
                .map(|strip| strip.level)
                .collect::<Vec<_>>(),
            [100, 40]
        );
        assert!(!update.state.is_settled);
        assert_eq!(model.state(), update.state);
        assert_eq!(model.opening_mix(), command);
    }

    #[test]
    fn the_desks_answer_settles_the_mix() {
        let model = connected(2);
        let sent = model
            .apply(MixEvent::ChannelLevelChanged {
                channel: 0,
                level: 30,
            })
            .command
            .expect("sent");
        let ack = ServerMessage::parse(&format!(
            r#"{{"type":"mix_ack","seq":{},"volume_percent":100,"max_level_percent":100,"muted":false,"channels":[30,100],"pans":[0,100],"mutes":[false,false],"solos":[false,false],"group_levels":[]}}"#,
            sent.sequence.unwrap_or_default()
        ))
        .expect("an ack");
        let event = mix_event_reported_by(ack).expect("an answer");

        let update = model.apply(event);

        assert_eq!(update.command, None);
        assert!(update.state.is_settled);
        assert_eq!(
            update.state.sync,
            MixSync {
                latest: 1,
                answered: 1
            }
        );
        assert_eq!(
            mix_event_reported_by(ServerMessage::Error(DeskError {
                message: "no UDP target".to_owned(),
                sequence: None,
            })),
            None
        );
    }

    #[test]
    fn the_saved_mixes_are_saved_and_recalled_through_the_model() {
        let model = connected(2);
        model.apply(MixEvent::ChannelLevelChanged {
            channel: 0,
            level: 30,
        });

        let saved = model.save_mix(" Verse ".to_owned());
        model.apply(MixEvent::ChannelLevelChanged {
            channel: 0,
            level: 90,
        });
        let recall = model.recall_mix("Verse".to_owned());

        assert_eq!(
            saved.outcome,
            MixBankOutcome::Saved {
                name: "Verse".to_owned()
            }
        );
        assert!(saved.saved_mixes_changed);
        assert_eq!(saved.state.saved_mix_names, ["Verse"]);
        assert_eq!(
            recall.outcome,
            RecallOutcome::Recalled {
                name: "Verse".to_owned()
            }
        );
        assert_eq!(
            recall
                .update
                .command
                .and_then(|command| command.channel_gains),
            Some(vec![30, 100])
        );
        assert_eq!(
            recall.update.state.active_mix_name.as_deref(),
            Some("Verse")
        );
        assert_eq!(model.saved_mixes()[0].snapshot.channel_gains, [30, 100]);
        assert!(!model.delete_mix("Gone".to_owned()).saved_mixes_changed);
        assert_eq!(
            model.rename_mix("Verse".to_owned(), " ".to_owned()).outcome,
            MixBankOutcome::NameNeeded
        );
        assert_eq!(model.reload_saved_mixes(Vec::new()).active_mix_name, None);
    }

    #[test]
    fn the_remembered_channel_crosses_and_a_change_to_it_is_reported() {
        let model = MixModel::new(Some(3), Vec::new());
        assert_eq!(model.state().more_of_me.musician_channel, Some(3));

        let update = model.apply(config(2));

        assert!(update.musician_channel_changed);
        assert_eq!(update.state.more_of_me.musician_channel, None);
    }

    #[test]
    fn a_poisoned_model_goes_on() {
        let model = Arc::new(connected(2));
        let held = Arc::clone(&model);
        let _ = std::thread::spawn(move || {
            let _guard = held.inner.lock().unwrap();
            panic!("poison the lock");
        })
        .join();

        assert!(model.inner.is_poisoned());
        let update = model.apply(MixEvent::MasterVolumeChanged { level: 70 });
        assert_eq!(update.state.master_volume, 70);
        assert_eq!(
            update.command.map(|command| command.sequence),
            Some(Some(1))
        );
    }

    #[test]
    fn changes_from_two_threads_each_land_with_their_own_number() {
        let model = Arc::new(connected(4));
        let threads: Vec<_> = (0..4)
            .map(|channel| {
                let model = Arc::clone(&model);
                std::thread::spawn(move || {
                    (1..=25)
                        .map(|level| {
                            let update =
                                model.apply(MixEvent::ChannelLevelChanged { channel, level });
                            let command = update.command.expect("every move changes the mix");
                            // The command and the state it left were read under the same hold.
                            assert_eq!(command.sequence, Some(update.state.sync.latest));
                            assert_eq!(
                                command
                                    .channel_gains
                                    .as_ref()
                                    .map(|gains| gains[channel as usize]),
                                Some(level)
                            );
                            update.state.sync.latest
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();

        let mut numbers: Vec<u64> = threads
            .into_iter()
            .flat_map(|thread| thread.join().expect("no thread panics"))
            .collect();
        numbers.sort_unstable();

        assert_eq!(numbers, (1..=100).collect::<Vec<u64>>());
        assert_eq!(model.state().sync.latest, 100);
    }
}
