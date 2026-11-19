//! The iOS client's former `MixBankTests`: the saved mixes' rules that the Android suite never
//! reached: its view model checked a rename and its store deleted before either reached a test, and
//! its reducer kept the active mix one that is saved. Here all three are [`MixBank`]'s.

use crate::control::mix_bank::{MixSnapshot, SavedMix};
use crate::mix::{MixBank, MixBankOutcome};

fn saved(names: &[&str]) -> Vec<SavedMix> {
    names
        .iter()
        .map(|name| SavedMix {
            name: (*name).to_owned(),
            snapshot: MixSnapshot::default(),
        })
        .collect()
}

/// Android's former view model checked a blank name first, then a mix that is gone, then the same
/// name; only a taken name was in its suite (`renamingAndDeletingAMix`).
#[test]
fn a_rename_to_a_blank_or_the_same_name_or_of_a_mix_not_saved_changes_nothing() {
    let mut state = MixBank::new(saved(&["Chorus", "Verse"]));
    state.activate("Verse");
    assert_eq!(state.active_name(), Some("Verse"));
    let before = state.clone();

    assert_eq!(state.rename("Verse", " \n "), MixBankOutcome::NameNeeded);
    assert_eq!(state.rename("Bridge", " "), MixBankOutcome::NameNeeded);
    assert_eq!(
        state.rename("Bridge", "Intro"),
        MixBankOutcome::Missing {
            name: "Bridge".to_owned()
        }
    );
    assert_eq!(state.rename("Verse", " Verse "), MixBankOutcome::Unchanged);
    assert_eq!(state, before);
}

/// Android's former store trimmed the name it deleted by, and its view model said "Deleted" whether
/// or not a mix had that name.
#[test]
fn deletes_by_the_name_trimmed_and_says_so_whether_or_not_a_mix_had_it() {
    let mut state = MixBank::new(saved(&["Verse", "Chorus"]));

    assert_eq!(
        state.delete(" Verse "),
        MixBankOutcome::Deleted {
            name: " Verse ".to_owned()
        }
    );
    assert_eq!(state.names(), ["Chorus"]);
    assert_eq!(
        state.delete("Bridge"),
        MixBankOutcome::Deleted {
            name: "Bridge".to_owned()
        }
    );
    assert_eq!(state.names(), ["Chorus"]);
}

#[test]
fn makes_only_a_saved_mix_the_active_one() {
    let mut state = MixBank::new(saved(&["Verse"]));
    state.activate("Verse");
    assert_eq!(state.active_name(), Some("Verse"));

    state.activate("Gone");

    assert_eq!(state.active_name(), None);
}
