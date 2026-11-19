//! `MixBankTests.swift`: the saved mixes' rules that the Android suite never reaches: its view
//! model checks a rename and its store deletes before either reaches a test
//! (`MonitorViewModel.kt:458-472`, `MixBankStore.kt:36-44`), and its reducer keeps the active mix
//! one that is saved (`MonitorReducer.kt:523-525`). Here all three are [`MixBank`]'s.

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

/// Android checks a blank name first, then a mix that is gone, then the same name; only a taken
/// name is in its suite (`renamingAndDeletingAMix`).
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

/// Android's store trims the name it deletes by, and its view model says "Deleted" whether or
/// not a mix had that name (`MonitorViewModel.kt:475-478`).
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
