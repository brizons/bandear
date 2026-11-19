//! The iOS client's former `MixSessionTests`: the reducer's edits and `config` beyond the Android
//! parity suite.

use super::{
    answer, band, choose, config, config_with, configure, connected_with, controls, first_config,
    group, push, report, report_of, running, set_cap, set_group_level, set_level, set_master,
    set_master_mute, set_mute, set_pan, set_solo, toggle, unlocked, with_band, with_channels,
    Events, WithControls,
};
use crate::control::server_message::{DeskConfiguration, MixReport};
use crate::mix::{EngineerControls, MixEvent, MixSession, MixSync};

/// A running five-channel session whose musician set their own levels and lifted channel 2 with
/// More of me, before the engineer linked channels 1 and 2 into the band's keys pair. Checked to
/// hold the levels to put back, so a test built on it cannot pass for want of any.
fn linked_while_lifted() -> MixSession {
    let paired = running().after(&[
        configure(config(5)),
        set_level(0, 80),
        set_level(1, 60),
        set_level(2, 40),
        set_level(3, 70),
        set_level(4, 90),
        choose(2),
        toggle(true),
        configure(config_with(5, Vec::new(), Some(band()))),
    ]);
    assert_eq!(paired.own_levels_to_restore, Some(vec![80, 60, 40, 70, 90]));
    paired
}

/// The `mix_state` the link sends: the pair's pans moved apart, the levels `gains`.
fn link_report(gains: &[i64]) -> MixReport {
    MixReport {
        channel_gains: gains.to_vec(),
        pans: vec![0, 0, 100, 100, 0],
        channel_mutes: vec![false; 5],
        channel_solos: vec![false; 5],
        ..report()
    }
}

/// The bridge the hand-built fixtures rely on: the states built by hand are the ones the reducer
/// builds.
#[test]
fn builds_from_a_first_config_exactly_the_mix_the_layout_tests_start_from() {
    assert_eq!(
        with_band(Vec::new()).mix,
        first_config(5, Vec::new(), Some(band()))
    );
    let groups = vec![group("A", &[0]), group("B", &[1, 2])];
    assert_eq!(
        with_channels(3, groups.clone()).mix,
        first_config(3, groups, None)
    );
}

/// Android's former parser dropped such a `config` before its reducer saw it; the mix holds 32
/// channels, so the rule lives with the mix here. The former Swift parameterized test, its three
/// arguments in one loop.
#[test]
fn ignores_a_config_for_no_channels_or_more_than_the_mix_holds() {
    for count in [0, 33, 255] {
        let state = with_channels(2, Vec::new());

        assert_eq!(state.after(&[configure(config(count))]), state, "{count}");
    }
}

#[test]
fn takes_a_config_for_all_32_channels_the_mix_holds() {
    assert_eq!(
        with_channels(DeskConfiguration::MAXIMUM_MIX_CHANNELS as u8, Vec::new())
            .mix
            .levels
            .len(),
        32
    );
}

#[test]
fn leaves_a_locked_strip_as_it_is_and_a_pair_when_either_side_is_locked() {
    let mut state = with_band(Vec::new());
    state.mix.engineer = controls(100, &[0, 2], None);

    let edited = state.after(&[
        set_level(0, 10),
        set_level(1, 10),
        set_pan(1, 10),
        set_mute(1, true),
        set_solo(0, true),
    ]);

    assert_eq!(edited, state);
    assert_eq!(
        edited.after(&[set_level(4, 10)]).mix.levels,
        [100, 100, 100, 100, 10]
    );
}

#[test]
fn leaves_channels_and_groups_to_the_leader_while_following_and_the_master_to_the_musician() {
    let mut state = with_channels(2, vec![group("A", &[0])]);
    state.mix.engineer = controls(100, &[], Some("Ana"));

    assert_eq!(
        state.after(&[
            set_level(0, 10),
            set_pan(1, 10),
            set_mute(0, true),
            set_group_level(0, 10),
        ]),
        state
    );

    let master = state.after(&[set_master(70), set_master_mute(true), set_cap(80)]);
    assert_eq!(master.mix.master_volume, 70);
    assert!(master.mix.master_muted);
    assert_eq!(master.mix.level_cap, 80);
}

#[test]
fn fills_the_other_groups_when_a_group_level_is_set_while_the_levels_list_is_short() {
    let mut state = with_channels(2, vec![group("A", &[0]), group("B", &[1])]);
    state.mix.group_levels = Vec::new();

    assert_eq!(
        state.after(&[set_group_level(1, 30)]).mix.group_levels,
        [100, 30]
    );
}

/// Android's former reducer wrote into every list by index and would throw on one shorter than the
/// levels. The lists are always resized together, so only a state built by hand gets here, but a
/// reducer behind a screen must not be able to crash the app over it.
#[test]
fn writes_only_what_a_list_has_room_for_when_one_is_shorter_than_the_levels() {
    let mut state = with_channels(3, Vec::new());
    state.mix.mutes = vec![false];

    let edited = state.after(&[set_mute(2, true)]);

    assert_eq!(edited.mix.mutes, [false]);
    assert_eq!(edited.mix.levels, state.mix.levels);
}

// Reconciliation with the desk

#[test]
fn numbers_each_change_as_the_newest_and_is_settled_once_the_desk_answered_the_newest() {
    let fresh = MixSync::default();
    assert_eq!(
        fresh,
        MixSync {
            latest: 0,
            answered: 0
        }
    );
    assert!(fresh.is_settled());

    let two_changes = fresh.changed().changed();
    assert_eq!(
        two_changes,
        MixSync {
            latest: 2,
            answered: 0
        }
    );
    assert!(!two_changes.is_settled());
    assert!(!two_changes.answered_up_to(1).is_settled());
    assert!(two_changes.answered_up_to(2).is_settled());
}

#[test]
fn never_takes_an_answered_number_back_the_desk_answers_in_order() {
    let sync = MixSync {
        latest: 5,
        answered: 4,
    };
    assert_eq!(sync.answered_up_to(2), sync);
    assert_eq!(
        sync.answered_up_to(5),
        MixSync {
            latest: 5,
            answered: 5
        }
    );
}

#[test]
fn keeps_its_numbers_across_a_reconnect_and_forgets_only_what_the_engineer_set() {
    let mut state = connected_with(2, Vec::new()).sent(set_level(0, 40));
    state = state.after(&[push(report().with(&controls(50, &[], None)))]);

    let reconnected = state.after(&[MixEvent::SessionEnded, MixEvent::SessionStarted]);

    assert_eq!(
        reconnected.sync,
        MixSync {
            latest: 1,
            answered: 0
        }
    );
    assert_eq!(reconnected.mix.engineer, EngineerControls::default());
    assert_eq!(reconnected.mix.levels, [40, 100]);
    assert!(reconnected.is_session_running);
}

/// Android's former `answerMix` moved the answered number before it asked whether a session was
/// running: the numbers only grow, across reconnects too.
#[test]
fn counts_a_late_answer_as_answered_and_changes_nothing_else_outside_a_session() {
    let ended = connected_with(2, Vec::new())
        .sent(set_level(0, 40))
        .after(&[MixEvent::SessionEnded]);
    let late = MixReport {
        channel_gains: vec![10, 10],
        ..report()
    }
    .with(&controls(30, &[], None));

    let answered = ended.after(&[answer(1, Some(late))]);

    assert_eq!(
        answered.sync,
        MixSync {
            latest: 1,
            answered: 1
        }
    );
    assert_eq!(answered.mix, ended.mix);
    assert!(!answered.is_session_running);
}

#[test]
fn hands_the_predicate_the_session_before_and_after_the_change_and_numbers_nothing_it_refuses() {
    let state = connected_with(2, Vec::new());
    let mut seen: Vec<Vec<i64>> = Vec::new();

    let next = state.reduced_sending(&set_level(1, 30), |before, after| {
        seen.extend([before.mix.levels.clone(), after.mix.levels.clone()]);
        false
    });

    assert_eq!(seen, [vec![100, 100], vec![100, 30]]);
    assert_eq!(next.sync, MixSync::default());
    assert_eq!(next.mix.levels, [100, 30]);
}

#[test]
fn starts_a_session_running_and_ends_it() {
    assert!(!MixSession::default().is_session_running);
    assert!(running().is_session_running);
    assert!(
        !running()
            .after(&[MixEvent::SessionEnded])
            .is_session_running
    );
}

#[test]
fn shows_the_answer_to_the_newest_mix_even_when_an_older_answer_never_came() {
    let two_in_flight = connected_with(2, Vec::new())
        .sent(set_level(0, 40))
        .sent(set_level(0, 50));

    let answered = two_in_flight.after(&[answer(
        2,
        Some(MixReport {
            channel_gains: vec![60, 70],
            ..report()
        }),
    )]);

    assert_eq!(answered.mix.levels, [60, 70]);
    assert_eq!(
        answered.sync,
        MixSync {
            latest: 2,
            answered: 2
        }
    );
}

// More of me

#[test]
fn takes_an_answer_carrying_the_lift_as_the_mix_the_phone_plays_and_never_stores_the_lift() {
    let on = connected_with(3, Vec::new())
        .after(&[choose(1), toggle(true)])
        .sent(set_level(0, 80));
    assert!(on.more_of_me.is_enabled);

    let answered = on.after(&[answer(
        1,
        Some(report_of(&on.mix, Some(on.played(&unlocked())))),
    )]);

    assert_eq!(answered.mix, on.mix);
    assert_eq!(answered.mix.levels, [80, 100, 100]);
    assert_eq!(answered.more_of_me, on.more_of_me);
    assert!(answered.sync.is_settled());
    assert_eq!(answered.after(&[toggle(false)]).mix.levels, [80, 100, 100]);
}

#[test]
fn lifts_a_pairs_two_sides_from_its_first_channel_one_channel_otherwise_none_without_one() {
    let mut state = with_band(Vec::new());
    assert_eq!(state.more_of_me_channels(), Vec::<i64>::new());

    state.more_of_me.musician_channel = Some(1);
    assert_eq!(state.more_of_me_channels(), [1, 2]);
    state.more_of_me.musician_channel = Some(0);
    assert_eq!(state.more_of_me_channels(), [0]);
    // Only a channel set directly (the app's remembered one) can sit on a pair's second side: the
    // picker offers a pair by its first channel, and a `config` moves it there. A strip is matched
    // by its first channel only, as Android's former reducer matched it.
    state.more_of_me.musician_channel = Some(2);
    assert_eq!(state.more_of_me_channels(), [2]);
}

#[test]
fn keeps_the_musicians_own_levels_only_when_a_link_turns_more_of_me_off() {
    let chosen_only = running().after(&[
        configure(config(5)),
        choose(2),
        configure(config_with(5, Vec::new(), Some(band()))),
    ]);
    let pair_elsewhere = running().after(&[
        configure(config(5)),
        choose(0),
        toggle(true),
        configure(config_with(5, Vec::new(), Some(band()))),
    ]);

    assert_eq!(
        linked_while_lifted().own_levels_to_restore,
        Some(vec![80, 60, 40, 70, 90])
    );
    assert_eq!(chosen_only.own_levels_to_restore, None);
    assert_eq!(pair_elsewhere.own_levels_to_restore, None);
}

/// A new session sends the phone's own mix when it opens, so no lift waits to be dropped.
#[test]
fn forgets_the_levels_a_link_left_to_restore_when_a_new_session_starts() {
    let reconnected =
        linked_while_lifted().after(&[MixEvent::SessionEnded, MixEvent::SessionStarted]);

    assert_eq!(reconnected.own_levels_to_restore, None);
    assert_eq!(
        reconnected
            .after(&[push(link_report(&[40, 30, 30, 35, 45]))])
            .mix
            .levels,
        [40, 30, 30, 35, 45]
    );
}

/// While the mix follows another's the desk keeps every channel from the leader's, so nothing of
/// the phone's can go back.
#[test]
fn takes_the_links_report_as_it_is_while_the_mix_follows_anothers_and_stops_waiting() {
    let paired = linked_while_lifted();

    let followed = paired.after(&[push(link_report(&[40, 30, 30, 35, 45]).with(&controls(
        100,
        &[],
        Some("Ana"),
    )))]);

    assert_eq!(followed.mix.levels, [40, 30, 30, 35, 45]);
    assert_eq!(followed.own_levels_to_restore, None);
    assert_eq!(followed.sync, paired.sync);
}

#[test]
fn leaves_the_desks_level_on_a_locked_channel_a_pair_whole_when_the_own_levels_go_back() {
    let locked = linked_while_lifted().after(&[push(
        link_report(&[40, 30, 30, 35, 45]).with(&controls(100, &[2, 4], None)),
    )]);

    assert_eq!(locked.mix.levels, [80, 30, 30, 70, 45]);
    assert_eq!(locked.own_levels_to_restore, None);
}

#[test]
fn numbers_no_change_when_the_own_levels_put_back_are_what_the_desk_already_plays() {
    let paired = linked_while_lifted();

    let restored = paired.after(&[push(link_report(&[80, 60, 60, 70, 90]))]);

    assert_eq!(restored.mix.levels, [80, 60, 60, 70, 90]);
    assert_eq!(restored.sync, paired.sync);
    assert_eq!(restored.own_levels_to_restore, None);
}

#[test]
fn keeps_the_levels_to_restore_through_a_change_that_is_not_sent() {
    let paired = linked_while_lifted();

    let unsent = paired.reduced_sending(&set_master(70), |_, _| false);

    assert_eq!(unsent.own_levels_to_restore, paired.own_levels_to_restore);
    assert_eq!(paired.sent(set_master(70)).own_levels_to_restore, None);
}
