//! `MixBankParityTests.swift`: the Android client's tests of its saved mixes, ported case for
//! case: the saved-mix tests of `MonitorReducerTest.kt`, the bank's half of the saved-mix tests of
//! `MonitorViewModelTest.kt`, and `GroupSnapshotTest.theDefaultSnapshotStartsWithNoGroups`.
//!
//! Android's reducer takes the list its store hands over (`MixesLoaded`, `MixSaved` and the rest)
//! and words what happened in the same step. Here [`MixBank`] makes each change itself and returns
//! what it did, and the words are the screen's, so where Android checks a notice a test checks the
//! outcome. Android's `MixesLoaded(names)` is a bank holding those names ([`bank`]).

use super::{set_level, with_channels, Events};
use crate::control::mix_bank::{MixSnapshot, SavedMix};
use crate::mix::{MixBank, MixBankOutcome};

/// Android's `MixesLoaded(names)`: a bank holding a default mix under each of `names`, none
/// active.
fn bank(names: &[&str]) -> MixBank {
    MixBank::new(saved(names))
}

/// A default mix saved under each of `names`.
fn saved(names: &[&str]) -> Vec<SavedMix> {
    names
        .iter()
        .map(|name| SavedMix {
            name: (*name).to_owned(),
            snapshot: MixSnapshot::default(),
        })
        .collect()
}

/// The bank after `change`: Android's `after` helper for the saved mixes.
fn after(bank: MixBank, change: impl FnOnce(&mut MixBank)) -> MixBank {
    let mut next = bank;
    change(&mut next);
    next
}

fn saved_as(name: &str) -> MixBankOutcome {
    MixBankOutcome::Saved {
        name: name.to_owned(),
    }
}

// MonitorReducerTest.kt

#[test]
fn loaded_mixes_are_listed_in_stored_order() {
    let state = bank(&["Verse", "Chorus"]);

    assert_eq!(state.names(), ["Verse", "Chorus"]);
    assert_eq!(state.active_name(), None);
    assert!(!state.is_empty());
}

#[test]
fn the_empty_state_shows_when_nothing_is_saved() {
    assert!(bank(&[]).is_empty());
}

/// Android's `MixNotice("Enter a name for the mix")` is what a refused save says
/// (`MonitorViewModel.kt:434-437`), so the refused save stands for it; its `MixMissing("Gone",
/// ["Verse"])` is the list read again once "Gone" was not found, so the reload stands for it.
#[test]
fn the_empty_state_never_shows_while_one_mix_is_saved() {
    let one_saved = [
        bank(&["Verse"]),
        after(bank(&[]), |b| {
            b.save(MixSnapshot::default(), "Verse");
        }),
        after(bank(&["Verse", "Chorus"]), |b| {
            b.delete("Chorus");
        }),
        after(bank(&["Verse"]), |b| {
            b.save(MixSnapshot::default(), "  ");
        }),
        after(bank(&["Old"]), |b| {
            b.rename("Old", "Verse");
        }),
        after(bank(&["Gone", "Verse"]), |b| b.reload(saved(&["Verse"]))),
        after(bank(&["Verse"]), |b| b.activate("Verse")),
    ];

    for state in one_saved {
        assert!(
            !state.is_empty(),
            "the empty state shows next to a saved mix"
        );
        assert_eq!(state.names(), ["Verse"]);
    }
}

#[test]
fn deleting_the_last_mix_shows_the_empty_state() {
    let mut state = bank(&["Verse"]);

    let outcome = state.delete("Verse");

    assert!(state.is_empty());
    assert_eq!(
        outcome,
        MixBankOutcome::Deleted {
            name: "Verse".to_owned()
        }
    );
}

#[test]
fn a_saved_mix_becomes_the_active_one() {
    let mut state = bank(&["Chorus"]);

    let outcome = state.save(MixSnapshot::default(), "Verse");

    assert_eq!(state.names(), ["Chorus", "Verse"]);
    assert_eq!(state.active_name(), Some("Verse"));
    assert_eq!(outcome, saved_as("Verse"));
}

/// Android's `MixRecalled("Verse", …)` makes the recalled mix the active one; here the phone does
/// that once the session took the mix ([`MixBank::activate`]).
#[test]
fn deleting_the_active_mix_leaves_none_active() {
    let mut state = after(bank(&["Verse", "Chorus"]), |b| b.activate("Verse"));
    // Not in Android's test: without an active mix first, there would be nothing to lose.
    assert_eq!(state.active_name(), Some("Verse"));

    state.delete("Verse");

    assert_eq!(state.active_name(), None);
    assert_eq!(state.names(), ["Chorus"]);
}

#[test]
fn renaming_the_active_mix_keeps_it_active() {
    let mut state = after(bank(&["Verse", "Chorus"]), |b| {
        b.save(MixSnapshot::default(), "Verse");
    });
    // Android's `MixSaved("Verse", ["Chorus", "Verse"])`: saving again moves the mix to the end.
    assert_eq!(state.names(), ["Chorus", "Verse"]);

    let outcome = state.rename("Verse", "Bridge");

    assert_eq!(state.names(), ["Chorus", "Bridge"]);
    assert_eq!(state.active_name(), Some("Bridge"));
    assert_eq!(
        outcome,
        MixBankOutcome::Renamed {
            from: "Verse".to_owned(),
            to: "Bridge".to_owned()
        }
    );
}

/// Android's `MixMissing("Verse", [])` comes from its view model, which looked for the mix in the
/// store and did not find it (`MonitorViewModel.kt:449-453`): here that is the list read again and
/// [`MixBank::mix_named`] finding nothing.
#[test]
fn a_missing_mix_is_reported_and_the_list_refreshed() {
    let mut state = after(bank(&["Verse"]), |b| b.activate("Verse"));
    assert_eq!(state.active_name(), Some("Verse"));

    state.reload(Vec::new());

    assert_eq!(state.mix_named("Verse"), None);
    assert_eq!(state.active_name(), None);
    assert!(state.is_empty());
}

#[test]
fn a_reload_that_loses_the_active_mix_leaves_none_active() {
    let mut state = after(bank(&["Verse"]), |b| {
        b.save(MixSnapshot::default(), "Verse");
    });
    assert_eq!(state.active_name(), Some("Verse"));

    state.reload(saved(&["Chorus"]));

    assert_eq!(state.active_name(), None);
}

/// Android dispatches the refused save's notice by hand; here the save is refused, and its
/// notice, "Enter a name for the mix", is the screen's.
#[test]
fn a_refused_save_says_why() {
    let mut state = bank(&["Verse"]);

    let outcome = state.save(MixSnapshot::default(), "  ");

    assert_eq!(outcome, MixBankOutcome::NameNeeded);
    assert_eq!(state.names(), ["Verse"]);
}

// GroupSnapshotTest.kt

/// Passes before the mix module exists: the snapshot is the control path's (T7a).
#[test]
fn the_default_snapshot_starts_with_no_groups() {
    assert!(MixSnapshot::default().group_levels.is_empty());
}

// MonitorViewModelTest.kt, the bank's half

/// The bank's half of `MonitorViewModelTest.kt:1027-1045`: the name is saved trimmed and becomes
/// the active one, holding the mix as the musician left it.
#[test]
fn saving_and_recalling_a_mix() {
    let session = with_channels(2, Vec::new()).after(&[set_level(0, 30)]);
    let mut state = bank(&[]);

    let outcome = state.save(session.mix.snapshot(), " Verse ");

    assert_eq!(outcome, saved_as("Verse"));
    assert_eq!(state.names(), ["Verse"]);
    assert_eq!(state.active_name(), Some("Verse"));
    assert_eq!(
        state
            .mix_named("Verse")
            .map(|mix| mix.snapshot.channel_gains.clone()),
        Some(vec![30, 100])
    );
}

/// The bank's half of `MonitorViewModelTest.kt:1047-1059`.
#[test]
fn saving_under_an_existing_name_replaces_that_mix() {
    let session = with_channels(2, Vec::new()).after(&[set_level(0, 30)]);
    let mut state = after(bank(&[]), |b| {
        b.save(session.mix.snapshot(), "Verse");
    });
    let changed = session.after(&[set_level(0, 70)]);

    // As on the old screen, saving under a name already used updates that mix; it is not refused.
    let outcome = state.save(changed.mix.snapshot(), "Verse");

    assert_eq!(outcome, saved_as("Verse"));
    assert_eq!(state.names(), ["Verse"]);
    assert_eq!(
        state
            .mixes()
            .iter()
            .map(|mix| mix.snapshot.channel_gains.clone())
            .collect::<Vec<_>>(),
        [vec![70, 100]]
    );
}

/// The bank's half of `MonitorViewModelTest.kt:1061-1068`: Android's `saveMix` answers false and
/// says "Enter a name for the mix".
#[test]
fn saving_needs_a_name() {
    let mut state = bank(&[]);

    let outcome = state.save(MixSnapshot::default(), "   ");

    assert_eq!(outcome, MixBankOutcome::NameNeeded);
    assert!(state.is_empty());
}

/// The bank's half of `MonitorViewModelTest.kt:1082-1101`. Android checks its store's list and
/// its screen's, which are one list here; its notice, "A mix named \"Chorus\" already exists", is
/// the screen's.
#[test]
fn renaming_and_deleting_a_mix() {
    let mut state = MixBank::new(vec![
        SavedMix {
            name: "Verse".to_owned(),
            snapshot: MixSnapshot {
                volume_percent: 70,
                ..MixSnapshot::default()
            },
        },
        SavedMix {
            name: "Chorus".to_owned(),
            snapshot: MixSnapshot::default(),
        },
    ]);

    let taken = state.rename("Verse", "Chorus");

    assert_eq!(
        taken,
        MixBankOutcome::NameTaken {
            name: "Chorus".to_owned()
        }
    );
    assert_eq!(state.names(), ["Verse", "Chorus"]);

    let renamed = state.rename("Verse", " Bridge ");

    assert_eq!(
        renamed,
        MixBankOutcome::Renamed {
            from: "Verse".to_owned(),
            to: "Bridge".to_owned()
        }
    );
    assert_eq!(state.names(), ["Chorus", "Bridge"]);
    assert_eq!(
        state
            .mixes()
            .iter()
            .filter(|mix| mix.name == "Bridge")
            .map(|mix| mix.snapshot.volume_percent)
            .collect::<Vec<_>>(),
        [70]
    );

    state.delete("Chorus");

    assert_eq!(state.names(), ["Bridge"]);
}
