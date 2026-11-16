//! `MixStateTests.swift`: the derived views of a mix that the Android suite only reaches through
//! its reducer, pinned directly here.

use super::{band, controls, first_config, group, vox};
use crate::control::server_message::ChannelKind;
use crate::mix::channel_layout;
use crate::mix::{GroupTag, MixState};
use std::collections::BTreeSet;

/// Pans even channels left and odd ones right until told otherwise. Swift's test also checks the
/// default name "CH 1", which is the screen's words and stays with the platforms.
#[test]
fn pans_even_channels_left_and_odd_ones_right_until_told_otherwise() {
    assert_eq!(
        [0, 1, 2, 31].map(channel_layout::default_pan),
        [0, 100, 0, 100]
    );
}

/// Android matches a strip by its first channel only, so an edit reported for a pair's second
/// side writes that side alone. The screens only ever report the first.
#[test]
fn writes_both_sides_for_a_pairs_first_channel_and_one_side_otherwise() {
    let mix = first_config(5, Vec::new(), Some(band()));

    assert_eq!(mix.strip_sides(1), [1, 2]);
    assert_eq!(mix.strip_sides(2), [2]);
    assert_eq!(mix.strip_sides(0), [0]);
    assert_eq!(mix.strip_sides(3), [3]);
    assert_eq!(mix.strip_sides(99), [99]);
}

#[test]
fn reads_a_pairs_pan_as_the_mean_of_its_two_and_a_missing_pan_as_the_default() {
    let mut mix = first_config(5, Vec::new(), Some(band()));
    mix.pans = vec![30, 10, 50];

    assert_eq!(
        mix.strips()
            .iter()
            .map(|strip| strip.pan)
            .collect::<Vec<_>>(),
        [30, 30, 0]
    );
}

#[test]
fn reads_each_strips_level_mute_and_solo_from_its_first_channel() {
    let mut mix = first_config(5, Vec::new(), Some(band()));
    mix.levels = vec![10, 20, 30, 40, 50];
    mix.mutes = vec![false, true, false, false, true];
    mix.solos = vec![true, false, false, false, false];
    let strips = mix.strips();

    assert_eq!(
        strips.iter().map(|s| s.level).collect::<Vec<_>>(),
        [10, 20, 50]
    );
    assert_eq!(
        strips.iter().map(|s| s.is_muted).collect::<Vec<_>>(),
        [false, true, true]
    );
    assert_eq!(
        strips.iter().map(|s| s.is_soloed).collect::<Vec<_>>(),
        [true, false, false]
    );
}

#[test]
fn tags_a_strip_with_the_first_group_that_lists_its_channel() {
    let mix = first_config(3, vec![group("A", &[2]), group("B", &[0, 2])], None);

    assert_eq!(
        mix.strips()
            .into_iter()
            .map(|strip| strip.group)
            .collect::<Vec<_>>(),
        [
            Some(GroupTag {
                index: 1,
                name: "B".to_owned()
            }),
            None,
            Some(GroupTag {
                index: 0,
                name: "A".to_owned()
            }),
        ]
    );
}

#[test]
fn reads_a_group_with_no_level_yet_as_neutral() {
    let mut mix = first_config(2, vec![group("A", &[0]), group("B", &[1])], None);
    mix.group_levels = vec![40];

    assert_eq!(
        mix.group_faders()
            .iter()
            .map(|fader| fader.level)
            .collect::<Vec<_>>(),
        [40, 100]
    );
}

#[test]
fn offers_no_channel_to_call_ones_own_before_the_desk_announced_any() {
    let mix = MixState {
        levels: vec![100, 100],
        ..MixState::default()
    };

    assert!(mix.musician_choices().is_empty());
}

#[test]
fn names_a_channel_past_the_details_as_untouched_and_shows_no_kind_without_details() {
    assert_eq!(
        first_config(3, Vec::new(), Some(vec![vox()]))
            .strips()
            .iter()
            .map(|strip| strip.kind)
            .collect::<Vec<_>>(),
        [
            Some(ChannelKind::Voice),
            Some(ChannelKind::Other),
            Some(ChannelKind::Other)
        ]
    );
    assert_eq!(
        first_config(2, Vec::new(), None)
            .strips()
            .iter()
            .map(|strip| strip.kind)
            .collect::<Vec<_>>(),
        [None, None]
    );
}

#[test]
fn locks_a_pair_as_one_strip_when_either_side_is_locked() {
    let mut mix = first_config(5, Vec::new(), Some(band()));
    mix.engineer = controls(100, &[2], None);

    assert!(mix.is_strip_locked(1));
    assert!(mix.is_strip_locked(2));
    assert!(!mix.is_strip_locked(0));
    assert_eq!(mix.locked_sides(), BTreeSet::from([1, 2]));
    assert_eq!(
        mix.strips().iter().map(|s| s.is_locked).collect::<Vec<_>>(),
        [false, true, false]
    );
}

#[test]
fn marks_every_strip_and_group_fader_while_the_mix_follows_someone() {
    let mut mix = first_config(2, vec![group("A", &[0])], None);
    mix.engineer = controls(100, &[], Some("Ana"));

    assert_eq!(mix.following(), Some("Ana"));
    assert_eq!(
        mix.strips()
            .into_iter()
            .map(|strip| strip.following)
            .collect::<Vec<_>>(),
        [Some("Ana".to_owned()), Some("Ana".to_owned())]
    );
    assert_eq!(
        mix.group_faders()
            .into_iter()
            .map(|fader| fader.following)
            .collect::<Vec<_>>(),
        [Some("Ana".to_owned())]
    );
}

#[test]
fn caps_what_the_musician_hears_at_the_lower_of_their_cap_and_the_engineers_limit() {
    let mut mix = MixState {
        level_cap: 80,
        engineer: controls(60, &[], None),
        ..MixState::default()
    };
    assert_eq!(mix.effective_cap(), 60);

    mix.level_cap = 90;
    mix.engineer = controls(100, &[], None);
    assert_eq!(mix.effective_cap(), 90);
}
