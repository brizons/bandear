//! `ChannelLayoutParityTests.swift`: the Android client's `ChannelLayoutTest.kt`, case for case
//! (`apps/android/app/src/test/java/com/bandear/android/ui/state/`).

use super::{info, untouched, Detail};
use crate::control::server_message::{ChannelKind, SourceChannel};
use crate::mix::channel_layout::{self, StereoPans, StripChannels};

fn shown() -> SourceChannel {
    untouched()
}

fn hidden() -> SourceChannel {
    untouched().hidden()
}

fn linked() -> SourceChannel {
    untouched().linked()
}

fn strip(first: i64) -> StripChannels {
    StripChannels::new(first, None)
}

fn pair(first: i64, second: i64) -> StripChannels {
    StripChannels::new(first, Some(second))
}

fn strips(count: i64, details: Option<&[SourceChannel]>) -> Vec<StripChannels> {
    channel_layout::strips(count, details)
}

fn pans(first: i64, second: i64) -> StereoPans {
    StereoPans { first, second }
}

#[test]
fn without_details_every_channel_is_a_strip_of_its_own() {
    assert_eq!(strips(3, None), [strip(0), strip(1), strip(2)]);
    assert_eq!(strips(0, None), []);
}

#[test]
fn hidden_channels_have_no_strip() {
    assert_eq!(
        strips(4, Some(&[shown(), hidden(), shown(), hidden()])),
        [strip(0), strip(2)]
    );
}

#[test]
fn a_linked_channel_and_the_next_are_one_strip_known_by_the_first() {
    let layout = strips(5, Some(&[shown(), linked(), shown(), linked(), shown()]));

    assert_eq!(layout, [strip(0), pair(1, 2), pair(3, 4)]);
    assert_eq!(layout[1].sides(), [1, 2]);
    assert_eq!(layout[0].sides(), [0]);
}

#[test]
fn a_hidden_pair_has_no_strip() {
    let details = [untouched().hidden().linked(), hidden(), shown()];

    assert_eq!(strips(3, Some(&details)), [strip(2)]);
}

#[test]
fn a_link_that_cannot_make_a_pair_leaves_each_channel_as_it_is() {
    // The server never sends these; the phone still shows what it can. A link on the last
    // channel, or into a hidden channel, makes no pair; a link on a pair's second side is part of
    // the pair.
    assert_eq!(strips(2, Some(&[shown(), linked()])), [strip(0), strip(1)]);
    assert_eq!(
        strips(3, Some(&[linked(), hidden(), shown()])),
        [strip(0), strip(2)]
    );
    assert_eq!(
        strips(3, Some(&[linked(), linked(), shown()])),
        [pair(0, 1), strip(2)]
    );
}

#[test]
fn channels_past_the_details_are_channels_the_engineer_never_touched() {
    assert_eq!(strips(3, Some(&[linked()])), [pair(0, 1), strip(2)]);
    // Details past the count have no channel to describe.
    assert_eq!(strips(1, Some(&[linked(), shown()])), [strip(0)]);
}

/// The name half of `aStripIsNamedByTheEngineerOrByItsNumber`: the engineer's name without the
/// spaces around it, or none. "CH 2" and "CH 3", the screen's words for a strip with no name, stay
/// with the platforms.
#[test]
fn a_strip_is_named_by_the_engineer_or_by_its_number() {
    assert_eq!(
        channel_layout::name(Some(&info(" Vox ", ChannelKind::Voice))).as_deref(),
        Some("Vox")
    );
    assert_eq!(
        channel_layout::name(Some(&info("  ", ChannelKind::Other))),
        None
    );
    assert_eq!(channel_layout::name(None), None);
}

#[test]
fn a_balance_is_written_as_both_pans() {
    assert_eq!(channel_layout::pans(0), pans(0, 0));
    assert_eq!(channel_layout::pans(25), pans(0, 50));
    assert_eq!(channel_layout::pans(50), pans(0, 100));
    assert_eq!(channel_layout::pans(75), pans(50, 100));
    assert_eq!(channel_layout::pans(100), pans(100, 100));
    // Out of range reads as the nearest end.
    assert_eq!(channel_layout::pans(-10), pans(0, 0));
    assert_eq!(channel_layout::pans(140), pans(100, 100));
}

#[test]
fn two_pans_read_back_as_their_mean() {
    assert_eq!(channel_layout::balance(pans(0, 100)), 50);
    assert_eq!(channel_layout::balance(pans(10, 50)), 30);
    // Halves round up, as the desktop rounds them.
    assert_eq!(channel_layout::balance(pans(0, 1)), 1);
    for balance in 0..=100 {
        assert_eq!(
            channel_layout::balance(channel_layout::pans(balance)),
            balance,
            "balance {balance}"
        );
    }
}
