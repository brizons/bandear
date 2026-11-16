//! The mix as one phone holds it: the session's mix and the saved mixes, changed one call at a
//! time, each call saying what the phone must send, store or tell the musician.

use super::bank::{MixBank, MixBankOutcome};
use super::session::{MixEvent, MixSession, RecallRefusal};
use crate::control::client_message::MixCommand;
use crate::control::mix_bank::SavedMix;

/// What one change to the mix asks of the phone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixUpdate {
    /// The `mix` to send, when the change numbered one; `None` when nothing goes to the desk. It
    /// carries the number the change took, so the two never disagree.
    pub command: Option<MixCommand>,
    /// The musician's channel changed (chosen, moved to its pair's first side, or forgotten by a
    /// `config`): the phone remembers it across launches.
    pub musician_channel_changed: bool,
}

/// What a recall of a saved mix did. The screen words it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RecallOutcome {
    /// The mix saved as `name` is now the mix, and the active one.
    Recalled { name: String },
    /// The mix saved as `name` was not recalled, for `refusal`.
    Refused {
        name: String,
        refusal: RecallRefusal,
    },
    /// No mix is saved as `name`.
    Missing { name: String },
}

/// A recall: what it did, and what it asks of the phone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixRecall {
    pub outcome: RecallOutcome,
    pub update: MixUpdate,
}

/// A change to the saved mixes: what it did, and whether the list the phone keeps changed, so
/// the phone writes it only then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixBankChange {
    pub outcome: MixBankOutcome,
    pub saved_mixes_changed: bool,
}

/// The mix as one phone holds it: the [`MixSession`] and the [`MixBank`], changed only through
/// these calls.
///
/// Each call is the wiring the iOS client did around them (`ControlSession.reduce`, its opening
/// mix, `SavedMixes`), in one step: an event goes through
/// [`MixSession::reduced_numbering_sends`], and whenever that numbered a new mix, the
/// [`MixSession::outgoing_mix`] of the session it left is the `mix` to send. The change and its
/// number land together, so whoever holds the model behind a lock can never see a state whose
/// number and mix disagree. Sockets, storage, scheduling and the words stay with the platform.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MixModel {
    session: MixSession,
    bank: MixBank,
}

impl MixModel {
    /// The model at launch: the musician's channel an earlier launch remembered, set before any
    /// `config` (which then keeps it, moves it to its pair's first side, or forgets it, as it
    /// would a chosen one; `MonitorViewModel.kt:519-525`), and the mixes the phone keeps, none
    /// active.
    pub fn new(musician_channel: Option<i64>, saved_mixes: Vec<SavedMix>) -> MixModel {
        let mut session = MixSession::default();
        session.more_of_me.musician_channel = musician_channel;
        MixModel {
            session,
            bank: MixBank::new(saved_mixes),
        }
    }

    /// The session's mix as it stands.
    pub fn session(&self) -> &MixSession {
        &self.session
    }

    /// The saved mixes as they stand.
    pub fn bank(&self) -> &MixBank {
        &self.bank
    }

    /// The mix after `event`, and the `mix` to send when the event numbered one: a change the
    /// send rule sends ([`MixSession::sends`]), or the first report after a link, which the
    /// reducer numbers itself (`MonitorViewModel.kt:346-348,390-394`).
    ///
    /// The session changes in one assignment, so a model behind a lock is whole between calls.
    pub fn apply(&mut self, event: &MixEvent) -> MixUpdate {
        let next = self.session.reduced_numbering_sends(event);
        let update = MixUpdate {
            command: (next.sync.latest != self.session.sync.latest).then(|| next.outgoing_mix()),
            musician_channel_changed: next.more_of_me.musician_channel
                != self.session.more_of_me.musician_channel,
        };
        self.session = next;
        update
    }

    /// The `mix` every connection opens with: the mix as it stands, under its newest number,
    /// numbering nothing new (`ControlChannel.kt:370-389`).
    pub fn opening_mix(&self) -> MixCommand {
        self.session.outgoing_mix()
    }

    /// Saves the mix as it stands (the musician's own levels, never the More of me lift) under
    /// `name` without the whitespace around it, replacing a mix of that name, and makes it the
    /// active one (`MonitorViewModel.kt:427-442`).
    pub fn save_mix(&mut self, name: &str) -> MixBankChange {
        let snapshot = self.session.mix.snapshot();
        self.changing_bank(|bank| bank.save(snapshot, name))
    }

    /// Renames the mix saved as `name` to `new_name`, as [`MixBank::rename`] does.
    pub fn rename_mix(&mut self, name: &str, new_name: &str) -> MixBankChange {
        self.changing_bank(|bank| bank.rename(name, new_name))
    }

    /// Deletes the mix saved as `name`, as [`MixBank::delete`] does.
    pub fn delete_mix(&mut self, name: &str) -> MixBankChange {
        self.changing_bank(|bank| bank.delete(name))
    }

    /// Recalls the mix saved as `name`: applied as if every control had been moved by hand, sent,
    /// and made the active one. Refused while the mix follows another musician's, and missing when
    /// no mix has that name; either way nothing changes or is sent
    /// (`MonitorViewModel.kt:445-455`, `MonitorReducer.kt:136-149`). The phone reloads the list it
    /// keeps first ([`MixModel::reload_saved_mixes`]) when another screen may have changed it.
    pub fn recall_mix(&mut self, name: &str) -> MixRecall {
        let unchanged = MixUpdate {
            command: None,
            musician_channel_changed: false,
        };
        let Some(snapshot) = self.bank.mix_named(name).map(|mix| mix.snapshot.clone()) else {
            return MixRecall {
                outcome: RecallOutcome::Missing {
                    name: name.to_owned(),
                },
                update: unchanged,
            };
        };
        if let Some(refusal) = self.session.recall_refusal() {
            return MixRecall {
                outcome: RecallOutcome::Refused {
                    name: name.to_owned(),
                    refusal,
                },
                update: unchanged,
            };
        }
        let update = self.apply(&MixEvent::MixRecalled(snapshot));
        self.bank.activate(name);
        MixRecall {
            outcome: RecallOutcome::Recalled {
                name: name.to_owned(),
            },
            update,
        }
    }

    /// The saved mixes as the phone's store now holds them: Android's `MixesLoaded`. The active
    /// mix stays active only while it is still saved.
    pub fn reload_saved_mixes(&mut self, mixes: Vec<SavedMix>) {
        self.bank.reload(mixes);
    }

    /// `change` made to the bank, and whether it changed the list.
    fn changing_bank(
        &mut self,
        change: impl FnOnce(&mut MixBank) -> MixBankOutcome,
    ) -> MixBankChange {
        let before = self.bank.mixes().to_vec();
        let outcome = change(&mut self.bank);
        MixBankChange {
            outcome,
            saved_mixes_changed: self.bank.mixes() != before.as_slice(),
        }
    }
}
