//! The iOS client's former `MoreOfMeParityTests`: the Android client's tests of the More of me lift
//! (the `moreOfMe` tests of its former `MixSnapshotTest` and `GroupSnapshotTest`, from
//! `moreOfMePushesTheMusicianChannelToFullAndHalvesTheRest` on), ported case for case.
//!
//! Android lifted a whole `MixSnapshot` and returned a new one. The lift here takes and returns the
//! levels alone ([`lifted_levels`]), so where Android checks that the other values come through
//! untouched, the port checks it on the session: the desk reporting the lifted levels back with
//! every other value as stored is the mix the phone plays, and changes nothing.

use super::{
    choose, connected_with, group, push, report_of, set_cap, set_group_level, set_level,
    set_master, set_master_mute, set_mute, set_pan, set_solo, toggle, unlocked, Events,
};
use crate::mix::more_of_me::lifted_levels;
use crate::mix::MixSession;

/// Android's `sample()` gains.
const SAMPLE_GAINS: [i64; 3] = [80, 100, 40];

/// Android's `sample()`, reached through events on a running three-channel session: levels 80,
/// 100 and 40; pans 0, 50 and 100; the middle channel muted and the first soloed; the master at
/// 70, the cap at 90, muted.
fn sample() -> MixSession {
    connected_with(3, Vec::new()).after(&[
        set_level(0, 80),
        set_level(2, 40),
        set_pan(1, 50),
        set_pan(2, 100),
        set_mute(1, true),
        set_solo(0, true),
        set_master(70),
        set_cap(90),
        set_master_mute(true),
    ])
}

#[test]
fn more_of_me_pushes_the_musician_channel_to_full_and_halves_the_rest() {
    let derived = lifted_levels(&SAMPLE_GAINS, &[1], &unlocked());
    assert_eq!(derived.len(), 3);

    assert_eq!(derived[0], 40);
    assert_eq!(derived[1], 100);
    assert_eq!(derived[2], 20);
}

/// Android lifts the sample and then checks the sample. The stored mix here is the session's, so
/// this turns More of me on and checks the mix the session holds.
#[test]
fn more_of_me_leaves_every_stored_value_untouched() {
    let on = sample().after(&[choose(1), toggle(true)]);
    // Not in Android's test: without More of me on, nothing here could have been lifted.
    assert!(on.more_of_me.is_enabled);

    assert_eq!(on.mix.levels, [80, 100, 40]);
    assert_eq!(on.mix.pans, [0, 50, 100]);
    assert_eq!(on.mix.mutes, [false, true, false]);
    assert_eq!(on.mix.solos, [true, false, false]);
    assert_eq!(on.mix.master_volume, 70);
    assert_eq!(on.mix.level_cap, 90);
    assert!(on.mix.master_muted);
}

#[test]
fn more_of_me_keeps_the_master_controls_and_flags() {
    let on = sample().after(&[choose(0), toggle(true)]);
    let derived = on.played(&unlocked());

    assert_eq!(derived, [100, 50, 20]);
    // The pans, mutes, solos and masters the phone plays are the stored ones: reported back with
    // the lifted levels, the mix is the one the phone plays and nothing changes.
    assert_eq!(on.after(&[push(report_of(&on.mix, Some(derived)))]), on);
}

/// Android's `null`: no lift. Every Android caller then plays and sends the stored levels, which is
/// what the lift returns here.
#[test]
fn more_of_me_does_nothing_without_a_designated_channel() {
    assert_eq!(lifted_levels(&SAMPLE_GAINS, &[], &unlocked()), SAMPLE_GAINS);
}

#[test]
fn more_of_me_does_nothing_for_a_channel_outside_the_mix() {
    assert_eq!(
        lifted_levels(&SAMPLE_GAINS, &[9], &unlocked()),
        SAMPLE_GAINS
    );
}

// From the Android client's former `GroupSnapshotTest`.

#[test]
fn more_of_me_keeps_the_group_levels_untouched() {
    let on = connected_with(2, vec![group("A", &[0]), group("B", &[1])]).after(&[
        set_level(0, 80),
        set_group_level(0, 30),
        set_group_level(1, 60),
        choose(1),
        toggle(true),
    ]);
    let derived = on.played(&unlocked());

    assert_eq!(derived, [40, 100]);
    assert_eq!(on.mix.group_levels, [30, 60]);
    // Reported back with the stored group levels, the mix is the one the phone plays.
    assert_eq!(on.after(&[push(report_of(&on.mix, Some(derived)))]), on);
}
