//! `MoreOfMeTests.swift`: the lift's rules beyond the Android parity suite: its rounding, and the
//! pair and lock rules of Android's `moreOfMeFor` (`ClientMixState.kt:77-83`), which Android only
//! reaches through its reducer tests.

use super::unlocked;
use crate::mix::more_of_me::lifted_levels;
use std::collections::BTreeSet;

/// Android halves with `(gain * 0.5).roundToInt()`, and Kotlin rounds a half toward positive
/// infinity, so half of an odd level is its upper half.
#[test]
fn halves_every_level_a_mix_can_hold_0_to_100_a_half_rounded_up() {
    let levels: Vec<i64> = (0..=100).chain([0]).collect();

    let lifted = lifted_levels(&levels, &[101], &unlocked());
    assert_eq!(lifted.len(), 102);

    let kotlin: Vec<i64> = (0..=100).map(|level| (level + 1) / 2).collect();
    assert_eq!(lifted[0..=100], kotlin[..]);
    // Rust's `round` takes a half away from zero, as Swift's `rounded()` does; on every level a
    // mix can hold it agrees with Kotlin's rule too.
    assert_eq!(
        (0..=100)
            .map(|level| (level as f64 * 0.5).round() as i64)
            .collect::<Vec<_>>(),
        kotlin
    );
    assert_eq!(
        [lifted[1], lifted[3], lifted[99], lifted[100]],
        [1, 2, 50, 50]
    );
    assert_eq!(lifted[101], 100);
}

/// No level in a mix is negative, but the lift takes any list, and on a negative half Swift's
/// own rounding (-0.5 to -1) and Kotlin's (-0.5 to 0) disagree. This holds the lift to Kotlin's.
#[test]
fn rounds_a_negative_half_up_as_kotlin_does() {
    assert_eq!(
        lifted_levels(&[-1, -3, -4, 0], &[3], &unlocked()),
        [0, -1, -2, 100]
    );
}

#[test]
fn lifts_both_sides_of_a_pair_alike_and_halves_every_other_channel() {
    assert_eq!(
        lifted_levels(&[80, 60, 40, 20], &[1, 2], &unlocked()),
        [40, 100, 100, 10]
    );
}

#[test]
fn leaves_a_locked_channel_as_stored_the_musicians_own_included() {
    assert_eq!(
        lifted_levels(&[80, 60, 40, 20], &[1, 2], &BTreeSet::from([0, 2])),
        [80, 100, 40, 10]
    );
}

#[test]
fn ignores_a_second_side_or_a_locked_channel_the_levels_do_not_have() {
    assert_eq!(
        lifted_levels(&[80, 60], &[1, 2], &BTreeSet::from([5, -1])),
        [40, 100]
    );
    assert_eq!(lifted_levels(&[], &[0], &unlocked()), Vec::<i64>::new());
}

/// Android's `moreOfMeFor` lifts by the first channel and gives up when it has none in the mix,
/// whatever the second side says.
#[test]
fn lifts_nothing_when_the_first_channel_is_outside_the_levels() {
    assert_eq!(lifted_levels(&[80, 60], &[5, 1], &unlocked()), [80, 60]);
}
