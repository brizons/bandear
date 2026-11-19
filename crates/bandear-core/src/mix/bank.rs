//! The mixes the musician saved on this phone, and the rules for saving, renaming and deleting
//! them. Their JSON is [`crate::control::mix_bank`]'s; where it is kept stays with each platform.

use crate::control::client_message::trimmed_name;
use crate::control::mix_bank::{MixSnapshot, SavedMix};

/// What a change to the saved mixes did, or why it did nothing. Each one is one of the notices
/// Android's former reducer and view model raised; the words are the screen's, so only the reasons
/// live here.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MixBankOutcome {
    /// Saved under `name`, the name as typed without the whitespace around it.
    Saved {
        name: String,
    },
    Renamed {
        from: String,
        to: String,
    },
    /// The mix called `name` is gone, as asked, whether or not one was saved. `name` is as given.
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
    /// Another mix already has `name`, so a rename to it was refused rather than overwrite that
    /// mix.
    NameTaken {
        name: String,
    },
}

/// The mixes the musician saved on this phone, in the order they were saved, and the active one.
///
/// Ported from the iOS client's former `MixBank`. Android split these rules three ways: its store
/// saved under a trimmed name, replacing a mix of that name, and deleted; its view model refused a
/// blank name and checked a rename; its reducer kept the active one. Here they are one value, so
/// the list and the active mix never disagree. Keeping the list across launches is the platform's,
/// applying a recalled mix is [`MixSession`](super::MixSession)'s, and saying what happened is the
/// screen's.
///
/// A name is trimmed as iOS trimmed it (`whitespacesAndNewlines`: Unicode's White_Space plus
/// U+200B); Android's `trim()` kept U+200B, and Android follows iOS here since it adopted this.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MixBank {
    mixes: Vec<SavedMix>,
    active_name: Option<String>,
}

impl MixBank {
    /// The bank holding `mixes`, as the store hands them over, with none active.
    pub fn new(mixes: Vec<SavedMix>) -> MixBank {
        MixBank {
            mixes,
            active_name: None,
        }
    }

    /// The saved mixes, in the order they were saved: a mix saved again under its name, or
    /// renamed, moves to the end.
    pub fn mixes(&self) -> &[SavedMix] {
        &self.mixes
    }

    /// The mix last saved or recalled, while it is still saved; `None` otherwise. Like Android's
    /// former one, it is not remembered across launches: the store keeps only the list.
    pub fn active_name(&self) -> Option<&str> {
        self.active_name.as_deref()
    }

    /// The saved mixes' names, in order.
    pub fn names(&self) -> Vec<String> {
        self.mixes.iter().map(|mix| mix.name.clone()).collect()
    }

    /// True when nothing is saved: the screen shows its empty state.
    pub fn is_empty(&self) -> bool {
        self.mixes.is_empty()
    }

    /// The mix saved as exactly `name`, or `None`.
    pub fn mix_named(&self, name: &str) -> Option<&SavedMix> {
        self.mixes.iter().find(|mix| mix.name == name)
    }

    /// Saves `snapshot` under `name` without the whitespace around it, replacing the mix already
    /// saved under that name, and makes it the active one. A blank name is refused and changes
    /// nothing.
    pub fn save(&mut self, snapshot: MixSnapshot, name: &str) -> MixBankOutcome {
        let trimmed = trimmed_name(name);
        if trimmed.is_empty() {
            return MixBankOutcome::NameNeeded;
        }
        self.mixes.retain(|mix| mix.name != trimmed);
        self.mixes.push(SavedMix {
            name: trimmed.to_owned(),
            snapshot,
        });
        self.active_name = Some(trimmed.to_owned());
        MixBankOutcome::Saved {
            name: trimmed.to_owned(),
        }
    }

    /// Renames the mix saved as `name` to `new_name` without the whitespace around it. In Android's
    /// former order: a blank name is refused, then a mix that is gone, then a name that is already
    /// this mix's changes nothing, then a name another mix has is refused, so no mix is ever
    /// overwritten. A renamed mix moves to the end, as Android's former store saved it again under
    /// its new name, and stays active if it was.
    pub fn rename(&mut self, name: &str, new_name: &str) -> MixBankOutcome {
        let target = trimmed_name(new_name);
        if target.is_empty() {
            return MixBankOutcome::NameNeeded;
        }
        let Some(snapshot) = self.mix_named(name).map(|mix| mix.snapshot.clone()) else {
            return MixBankOutcome::Missing {
                name: name.to_owned(),
            };
        };
        if target == name {
            return MixBankOutcome::Unchanged;
        }
        if self.mix_named(target).is_some() {
            return MixBankOutcome::NameTaken {
                name: target.to_owned(),
            };
        }
        self.mixes.retain(|mix| mix.name != name);
        self.mixes.push(SavedMix {
            name: target.to_owned(),
            snapshot,
        });
        if self.active_name.as_deref() == Some(name) {
            self.active_name = Some(target.to_owned());
        }
        MixBankOutcome::Renamed {
            from: name.to_owned(),
            to: target.to_owned(),
        }
    }

    /// Deletes the mix saved as `name` without the whitespace around it. The active mix stays
    /// active only if it was another one.
    pub fn delete(&mut self, name: &str) -> MixBankOutcome {
        let trimmed = trimmed_name(name);
        self.mixes.retain(|mix| mix.name != trimmed);
        self.keep_active_only_while_saved();
        MixBankOutcome::Deleted {
            name: name.to_owned(),
        }
    }

    /// Makes the mix saved as `name` the active one, as a recall does once the mix took it; a name
    /// no mix has leaves none active.
    pub fn activate(&mut self, name: &str) {
        self.active_name = Some(name.to_owned());
        self.keep_active_only_while_saved();
    }

    /// The list as the store now holds it: Android's former `MixesLoaded`. The active mix stays
    /// active only while it is still saved.
    pub fn reload(&mut self, mixes: Vec<SavedMix>) {
        self.mixes = mixes;
        self.keep_active_only_while_saved();
    }

    /// Android's former `listing`.
    fn keep_active_only_while_saved(&mut self) {
        if let Some(active) = &self.active_name {
            if self.mix_named(active).is_none() {
                self.active_name = None;
            }
        }
    }
}
