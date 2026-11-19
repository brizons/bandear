//! The iOS client's former `MixReducerParityTests`: the Android client's `MonitorReducerTest` tests
//! of the mix (they left that file with this port), ported case for case, unit by unit as the mix
//! is built. Every state here is reached through events, as Android's were. Where the Swift tests
//! checked a strip's label, the port checks the name the label is made from: `None` where the
//! screen names the strip by its number ("CH 5").

use super::{
    answer, band, choose, choose_none, config, config_with, configure, connected_with, controls,
    group, info, keys_right, push, recall, report, report_of, running, set_cap, set_group_level,
    set_level, set_master, set_master_mute, set_mute, set_pan, set_solo, toggle, unlocked,
    untouched, vox, with_band, with_channels, Detail, Events, WithControls,
};
use crate::control::mix_bank::MixSnapshot;
use crate::control::server_message::{ChannelKind, MixReport, SourceChannel};
use crate::mix::more_of_me::lifted_levels;
use crate::mix::{
    ChannelStrip, EngineerControls, GroupFader, GroupTag, MixEvent, MixSession, MixState, MixSync,
    MoreOfMeNotice, NamedChannel, RecallRefusal,
};
use std::collections::BTreeSet;

// T5c fixtures, as the Android client's `MonitorReducerTest` named them.

/// The desk's whole mix, as `mix_state` carries it: full 32-channel and 16-group tables.
fn server_mix() -> MixReport {
    MixReport {
        channel_gains: (0..32)
            .map(|c| if c < 2 { 80 - 10 * c } else { 100 })
            .collect(),
        pans: (0..32).map(|c| if c == 0 { 25 } else { 100 }).collect(),
        channel_mutes: (0..32).map(|c| c == 0).collect(),
        channel_solos: (0..32).map(|c| c == 1).collect(),
        group_levels: (0..16).map(|g| if g == 0 { 60 } else { 100 }).collect(),
        volume_percent: 50,
        max_level_percent: 90,
        muted: true,
        ..report()
    }
}

/// The desk's whole mix at the neutral values a fresh three-channel session already shows, with
/// `gains` in place of the neutral levels.
fn neutral_mix(gains: Vec<i64>) -> MixReport {
    MixReport {
        channel_gains: gains,
        pans: (0..32).map(|c| if c % 2 == 0 { 0 } else { 100 }).collect(),
        channel_mutes: vec![false; 32],
        channel_solos: vec![false; 32],
        group_levels: vec![100; 16],
        ..report()
    }
}

/// The former Swift fixture `neutralMix()`: every channel neutral.
fn neutral() -> MixReport {
    neutral_mix(vec![100; 32])
}

/// The leader's whole mix, every channel at `level`.
fn leaders_mix(level: i64) -> MixReport {
    neutral_mix(vec![level; 32])
}

/// The engineer's controls: a limit of `limit` and `locked` channels.
fn engineer(limit: i64, locked: &[i64]) -> EngineerControls {
    controls(limit, locked, None)
}

/// The engineer's controls with the musician following `name`'s mix, or nobody's.
fn following(name: Option<&str>, locked: &[i64]) -> EngineerControls {
    controls(100, locked, name)
}

/// A session of three channels whose desk reported the engineer's `controls` with its mix.
fn locked_by(controls: EngineerControls, details: Option<Vec<SourceChannel>>) -> MixSession {
    running().after(&[
        configure(config_with(3, Vec::new(), details)),
        push(neutral().with(&controls)),
    ])
}

// T5d fixtures, as the Android client's `MonitorReducerTest` named them.

/// A keys pair on channels 0 and 1 and a voice on channel 2.
fn keys_pair() -> Vec<SourceChannel> {
    vec![
        info("Keys", ChannelKind::Keys).linked(),
        untouched(),
        info("Vox", ChannelKind::Other),
    ]
}

/// The `mix_state` the engineer's link of channels 1 and 2 sends while More of me lifted channel
/// 2: the pair's second side took the first's (lifted) level, and its pans moved apart.
fn linked_mix() -> MixReport {
    MixReport {
        channel_gains: vec![40, 30, 30, 35, 45],
        pans: vec![0, 0, 100, 100, 0],
        channel_mutes: vec![false; 5],
        channel_solos: vec![false; 5],
        ..report()
    }
}

/// The band's `config`: five channels, no groups, the band's details.
fn band_config() -> MixEvent {
    configure(config_with(5, Vec::new(), Some(band())))
}

/// The strips' names, which the screen's labels are made from.
fn names(mix: &MixState) -> Vec<Option<String>> {
    mix.strips().into_iter().map(|strip| strip.name).collect()
}

fn named(index: i64, name: Option<&str>) -> NamedChannel {
    NamedChannel {
        index,
        name: name.map(str::to_owned),
    }
}

fn locks(mix: &MixState) -> Vec<bool> {
    mix.strips().iter().map(|strip| strip.is_locked).collect()
}

fn ana() -> Option<String> {
    Some("Ana".to_owned())
}

// T5a, the channel layout

#[test]
fn announced_details_name_the_strips_show_their_kinds_and_join_pairs() {
    let strips = with_band(Vec::new()).mix.strips();
    assert_eq!(strips.len(), 3);

    assert_eq!(
        strips.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
        [Some("Vox".to_owned()), Some("Keys".to_owned()), None]
    );
    assert_eq!(
        strips.iter().map(|s| s.kind).collect::<Vec<_>>(),
        [
            Some(ChannelKind::Voice),
            Some(ChannelKind::Keys),
            Some(ChannelKind::Other)
        ]
    );
    assert_eq!(
        strips.iter().map(|s| s.index).collect::<Vec<_>>(),
        [0, 1, 4]
    );
    assert_eq!(
        strips.iter().map(|s| s.paired_with).collect::<Vec<_>>(),
        [None, Some(2), None]
    );
    assert!(strips[1].is_stereo());
    assert!(!strips[0].is_stereo());
}

#[test]
fn without_details_every_channel_shows_by_number_with_no_kind_as_before() {
    let mix = with_channels(3, vec![group("Band", &[0, 1])]).mix;

    assert_eq!(mix.channel_details, None);
    assert_eq!(names(&mix), [None, None, None]);
    assert_eq!(
        mix.strips().iter().map(|s| s.kind).collect::<Vec<_>>(),
        [None, None, None]
    );
    assert_eq!(
        mix.strips()
            .iter()
            .map(|s| s.paired_with)
            .collect::<Vec<_>>(),
        [None, None, None]
    );
    assert_eq!(
        mix.group_faders()
            .into_iter()
            .map(|fader| fader.members)
            .collect::<Vec<_>>(),
        [vec![named(0, None), named(1, None)]]
    );
}

#[test]
fn only_shown_strips_can_be_chosen_and_a_pair_is_chosen_by_its_first_channel() {
    let state = with_band(Vec::new());

    assert_eq!(
        state
            .mix
            .musician_choices()
            .iter()
            .map(|choice| choice.index)
            .collect::<Vec<_>>(),
        [0, 1, 4]
    );
    assert_eq!(
        state.mix.musician_choices(),
        [
            named(0, Some("Vox")),
            named(1, Some("Keys")),
            named(4, None)
        ]
    );
    // The hidden spare and the pair's second side are not offered, so they cannot be chosen.
    assert_eq!(state.after(&[choose(3), choose(2)]), state);
    assert_eq!(
        state.after(&[choose(1)]).more_of_me.musician_channel,
        Some(1)
    );
}

#[test]
fn a_group_names_its_shown_strips_a_pair_once() {
    let mix = with_band(vec![group("Band", &[0, 1, 2, 3])]).mix;

    assert_eq!(
        mix.group_faders()
            .into_iter()
            .map(|fader| fader.members)
            .collect::<Vec<_>>(),
        [vec![named(0, Some("Vox")), named(1, Some("Keys"))]]
    );
}

// T5b, edits and config

#[test]
fn the_first_config_builds_neutral_strips_with_the_captured_pan_layout() {
    let mix = with_channels(3, Vec::new()).mix;

    assert_eq!(mix.source_channels, 3);
    assert_eq!(mix.levels, [100, 100, 100]);
    assert_eq!(mix.pans, [0, 100, 0]);
    assert_eq!(mix.mutes, [false, false, false]);
    assert_eq!(mix.solos, [false, false, false]);
    assert_eq!(names(&mix), [None, None, None]);
    assert_eq!(
        mix.musician_choices()
            .iter()
            .map(|choice| choice.index)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn a_config_with_a_new_count_keeps_the_channels_that_still_exist() {
    let set =
        with_channels(3, Vec::new()).after(&[set_level(0, 40), set_pan(1, 30), set_mute(2, true)]);

    let grown = set.after(&[configure(config(4))]);
    let shrunk = set.after(&[configure(config(2))]);

    assert_eq!(grown.mix.levels, [40, 100, 100, 100]);
    assert_eq!(grown.mix.pans, [0, 30, 0, 100]);
    assert_eq!(grown.mix.mutes, [false, false, true, false]);
    assert_eq!(shrunk.mix.levels, [40, 100]);
    assert_eq!(shrunk.mix.pans, [0, 30]);
    assert_eq!(shrunk.mix.source_channels, 2);
}

#[test]
fn a_repeated_config_leaves_the_strips_alone() {
    let set = with_channels(2, Vec::new()).after(&[set_level(1, 70), set_solo(0, true)]);

    assert_eq!(set.after(&[configure(config(2))]).mix, set.mix);
}

#[test]
fn announced_groups_tag_their_channels_and_keep_their_levels() {
    let groups = vec![group("Drums", &[0, 1]), group("Keys", &[2])];
    let state = with_channels(4, groups.clone()).after(&[set_group_level(1, 60)]);
    let tag = |index: i64, name: &str| {
        Some(GroupTag {
            index,
            name: name.to_owned(),
        })
    };

    assert_eq!(
        state
            .mix
            .strips()
            .into_iter()
            .map(|strip| strip.group)
            .collect::<Vec<_>>(),
        [tag(0, "Drums"), tag(0, "Drums"), tag(1, "Keys"), None]
    );
    assert_eq!(
        state.mix.group_faders(),
        [
            GroupFader {
                index: 0,
                name: "Drums".to_owned(),
                channels: vec![0, 1],
                level: 100,
                members: vec![named(0, None), named(1, None)],
                following: None,
            },
            GroupFader {
                index: 1,
                name: "Keys".to_owned(),
                channels: vec![2],
                level: 60,
                members: vec![named(2, None)],
                following: None,
            },
        ]
    );

    let mut more = groups;
    more.push(group("Vox", &[3]));
    let reannounced = state.after(&[configure(config_with(4, more, None))]);

    assert_eq!(reannounced.mix.group_levels, [100, 60, 100]);
    assert_eq!(reannounced.mix.strips().len(), 4);
    assert_eq!(reannounced.mix.strips()[3].group, tag(2, "Vox"));
}

#[test]
fn a_pairs_level_mute_and_solo_write_both_channels() {
    let state =
        with_band(Vec::new()).after(&[set_level(1, 40), set_mute(1, true), set_solo(1, true)]);

    assert_eq!(state.mix.levels, [100, 40, 40, 100, 100]);
    assert_eq!(state.mix.mutes, [false, true, true, false, false]);
    assert_eq!(state.mix.solos, [false, true, true, false, false]);
    let strips = state.mix.strips();
    assert_eq!(strips.len(), 3);
    let pair = &strips[1];
    assert_eq!(pair.level, 40);
    assert!(pair.is_muted);
    assert!(pair.is_soloed);
}

#[test]
fn a_single_strips_edit_writes_only_its_channel() {
    let state = with_band(Vec::new()).after(&[set_level(0, 30), set_pan(4, 70), set_mute(4, true)]);

    assert_eq!(state.mix.levels, [30, 100, 100, 100, 100]);
    assert_eq!(state.mix.pans, [0, 100, 0, 100, 70]);
    assert_eq!(state.mix.mutes, [false, false, false, false, true]);
}

#[test]
fn a_pairs_pan_is_a_balance_written_as_both_pans_and_read_back_as_their_mean() {
    let left = with_band(Vec::new()).after(&[set_pan(1, 25)]);
    let right = left.after(&[set_pan(1, 80)]);

    assert_eq!(left.mix.pans, [0, 0, 50, 100, 0]);
    assert_eq!(right.mix.pans, [0, 60, 100, 100, 0]);
    assert!(left.mix.strips().len() == 3 && right.mix.strips().len() == 3);
    assert_eq!(left.mix.strips()[1].pan, 25);
    assert_eq!(right.mix.strips()[1].pan, 80);
    // Pans set elsewhere read as their mean.
    let mut elsewhere = with_band(Vec::new()).mix;
    elsewhere.pans = vec![0, 10, 50, 100, 0];
    assert_eq!(elsewhere.strips()[1].pan, 30);
}

#[test]
fn a_new_config_mid_session_rebuilds_the_strips_and_keeps_the_mix() {
    let set = with_band(Vec::new()).after(&[set_level(4, 60), set_level(1, 40)]);
    let unlinked = vec![
        vox(),
        info("Keys L", ChannelKind::Keys),
        keys_right(),
        info("Spare", ChannelKind::Other),
        info("Click", ChannelKind::Click).hidden(),
    ];

    let rebuilt = set.after(&[configure(config_with(5, Vec::new(), Some(unlinked)))]);

    assert_eq!(
        names(&rebuilt.mix),
        ["Vox", "Keys L", "Keys R", "Spare"].map(|name| Some(name.to_owned()))
    );
    assert_eq!(
        rebuilt
            .mix
            .strips()
            .iter()
            .map(|s| s.paired_with)
            .collect::<Vec<_>>(),
        [None, None, None, None]
    );
    assert_eq!(rebuilt.mix.levels, set.mix.levels);
    assert_eq!(rebuilt.mix.pans, set.mix.pans);
}

#[test]
fn channel_controls_change_only_their_channel() {
    let state = with_channels(3, Vec::new()).after(&[
        set_level(1, 25),
        set_pan(1, 50),
        set_mute(1, true),
        set_solo(2, true),
    ]);

    let strips = state.mix.strips();
    assert_eq!(strips.len(), 3);
    assert_eq!(
        strips[1],
        ChannelStrip {
            index: 1,
            name: None,
            group: None,
            level: 25,
            pan: 50,
            is_muted: true,
            is_soloed: false,
            kind: None,
            paired_with: None,
            is_locked: false,
            following: None,
        }
    );
    assert_eq!(
        strips[0],
        ChannelStrip {
            index: 0,
            name: None,
            group: None,
            level: 100,
            pan: 0,
            is_muted: false,
            is_soloed: false,
            kind: None,
            paired_with: None,
            is_locked: false,
            following: None,
        }
    );
    assert!(strips[2].is_soloed);
    assert_eq!(state.mix.levels, [100, 25, 100]);
}

#[test]
fn levels_and_pans_stay_between_zero_and_a_hundred() {
    let state = with_channels(1, vec![group("Band", &[0])]).after(&[
        set_level(0, 140),
        set_pan(0, -5),
        set_group_level(0, 101),
        set_master(120),
        set_cap(-1),
    ]);

    assert_eq!(state.mix.levels, [100]);
    assert_eq!(state.mix.pans, [0]);
    assert_eq!(state.mix.group_levels, [100]);
    assert_eq!(state.mix.master_volume, 100);
    assert_eq!(state.mix.level_cap, 0);
}

#[test]
fn a_control_for_a_channel_or_group_that_does_not_exist_changes_nothing() {
    let state = with_channels(2, Vec::new());

    assert_eq!(
        state.after(&[
            set_level(5, 10),
            set_mute(-1, true),
            set_pan(2, 10),
            set_solo(9, true),
            set_group_level(0, 50),
        ]),
        state
    );
}

#[test]
fn a_group_level_fills_the_other_groups_as_they_read() {
    let groups = vec![group("A", &[0]), group("B", &[1]), group("C", &[1])];
    let recalled_short = with_channels(2, Vec::new()).after(&[recall(MixSnapshot {
        channel_gains: vec![100, 100],
        group_levels: vec![20],
        ..MixSnapshot::default()
    })]);
    let announced = recalled_short.after(&[configure(config_with(2, groups, None))]);

    let state = announced.after(&[set_group_level(2, 70)]);

    assert_eq!(state.mix.group_levels, [20, 100, 70]);
}

#[test]
fn master_controls_change_the_master() {
    let state =
        with_channels(2, Vec::new()).after(&[set_master(80), set_master_mute(true), set_cap(90)]);

    assert_eq!(state.mix.master_volume, 80);
    assert!(state.mix.master_muted);
    assert_eq!(state.mix.level_cap, 90);
}

/// Android's snapshot is the stored mix; `outgoing_mix` is the mix as played, which is the same
/// while More of me is off, as it is here.
#[test]
fn the_mix_becomes_the_snapshot_the_control_channel_sends() {
    let state = with_channels(2, vec![group("Band", &[0, 1])]).after(&[
        set_level(0, 30),
        set_solo(1, true),
        set_group_level(0, 70),
        set_master(60),
        set_cap(85),
        set_master_mute(true),
    ]);

    let snapshot = state.outgoing_mix();

    assert_eq!(snapshot.channel_gains, Some(vec![30, 100]));
    assert_eq!(snapshot.pans, Some(vec![0, 100]));
    assert_eq!(snapshot.channel_mutes, Some(vec![false, false]));
    assert_eq!(snapshot.channel_solos, Some(vec![false, true]));
    assert_eq!(snapshot.group_levels, Some(vec![70]));
    assert_eq!(snapshot.volume_percent, 60);
    assert_eq!(snapshot.max_level_percent, 85);
    assert!(snapshot.muted);
}

// T5c, reconciliation with the desk: server reports

#[test]
fn a_pushed_mix_takes_the_servers_values_for_the_announced_channels_and_groups() {
    let groups = vec![group("Band", &[0, 1])];
    let state = connected_with(2, groups.clone());

    let pushed = state.after(&[push(server_mix())]);

    assert_eq!(pushed.mix.source_channels, 2);
    assert_eq!(pushed.mix.levels, [80, 70]);
    assert_eq!(pushed.mix.pans, [25, 100]);
    assert_eq!(pushed.mix.mutes, [true, false]);
    assert_eq!(pushed.mix.solos, [false, true]);
    assert_eq!(pushed.mix.groups, groups);
    assert_eq!(pushed.mix.group_levels, [60]);
    assert_eq!(pushed.mix.master_volume, 50);
    assert_eq!(pushed.mix.level_cap, 90);
    assert!(pushed.mix.master_muted);
}

#[test]
fn a_pushed_mix_is_clamped_and_keeps_what_the_server_did_not_send() {
    let state = connected_with(2, vec![group("Band", &[0, 1])]).after(&[set_level(1, 40)]);

    let pushed = state.after(&[push(MixReport {
        channel_gains: vec![140],
        pans: vec![-5],
        volume_percent: 120,
        max_level_percent: -1,
        ..report()
    })]);

    assert_eq!(pushed.mix.levels, [100, 40]);
    assert_eq!(pushed.mix.pans, [0, 100]);
    assert_eq!(pushed.mix.group_levels, [100]);
    assert_eq!(pushed.mix.master_volume, 100);
    assert_eq!(pushed.mix.level_cap, 0);
}

#[test]
fn a_pushed_mix_before_the_first_config_keeps_the_strips_the_phone_shows() {
    let recalled = running().after(&[recall(MixSnapshot {
        channel_gains: vec![30, 30, 30],
        group_levels: vec![20],
        ..MixSnapshot::default()
    })]);

    let pushed = recalled.after(&[push(server_mix())]);

    assert_eq!(pushed.mix.levels, [80, 70, 100]);
    assert_eq!(pushed.mix.group_levels, [60]);
}

#[test]
fn a_pushed_mix_changes_nothing_outside_a_session() {
    let idle_with_channels = with_channels(2, Vec::new());
    let ended = connected_with(2, Vec::new()).after(&[MixEvent::SessionEnded]);

    assert_eq!(
        idle_with_channels.after(&[push(server_mix())]),
        idle_with_channels
    );
    assert_eq!(ended.after(&[push(server_mix())]), ended);
}

#[test]
fn a_pushed_mix_turns_more_of_me_off_and_keeps_the_mix_as_the_server_plays_it() {
    let on = connected_with(2, Vec::new()).after(&[choose(1), toggle(true)]);

    let pushed = on.after(&[push(server_mix())]);

    assert!(!pushed.more_of_me.is_enabled);
    assert_eq!(pushed.more_of_me.musician_channel, Some(1));
    assert_eq!(
        pushed.more_of_me.notice,
        Some(MoreOfMeNotice::MixChangedElsewhere)
    );
    assert_eq!(pushed.mix.levels, [80, 70]);
}

#[test]
fn a_push_of_the_mix_the_phone_already_plays_changes_nothing() {
    let plain = connected_with(2, Vec::new()).after(&[set_level(0, 80)]);
    let lifted = plain.after(&[choose(1), toggle(true)]);
    // Not in Android's test: without it, a reducer that ignored More of me would pass vacuously.
    assert!(lifted.more_of_me.is_enabled);

    assert_eq!(plain.after(&[push(report_of(&plain.mix, None))]), plain);
    // With More of me on the desk plays the lifted mix; hearing it back keeps More of me on.
    let played = lifted_levels(&lifted.mix.levels, &[1], &unlocked());
    assert_eq!(
        lifted.after(&[push(report_of(&lifted.mix, Some(played)))]),
        lifted
    );
}

// Seq: the phone's mixes, numbered, and the desk's answers to them.

#[test]
fn a_change_that_is_sent_is_numbered_as_the_newest_mix_and_one_that_is_not_keeps_the_number() {
    let state = connected_with(2, Vec::new());

    let sent = state.sent(set_level(0, 40));
    let kept = sent.reduced_sending(&set_level(0, 40), |before, after| before.mix != after.mix);

    assert_eq!(
        sent.sync,
        MixSync {
            latest: 1,
            answered: 0
        }
    );
    assert_eq!(sent.mix.levels, [40, 100]);
    assert_eq!(kept, sent);
    assert!(!sent.sync.is_settled());
}

#[test]
fn a_push_while_a_mix_of_the_phone_is_unanswered_is_dropped() {
    let outstanding = connected_with(2, Vec::new()).sent(set_level(0, 40));

    assert_eq!(outstanding.after(&[push(server_mix())]), outstanding);
}

#[test]
fn only_the_answer_to_the_newest_mix_is_shown() {
    let two_in_flight = connected_with(2, Vec::new())
        .sent(set_level(0, 40))
        .sent(set_level(0, 50));

    let older_answered = two_in_flight.after(&[answer(1, Some(server_mix()))]);

    assert_eq!(older_answered.mix.levels, [50, 100]);
    assert_eq!(
        older_answered.sync,
        MixSync {
            latest: 2,
            answered: 1
        }
    );
    assert_eq!(older_answered.after(&[push(server_mix())]), older_answered);

    let newest_answered = older_answered.after(&[answer(2, Some(server_mix()))]);

    assert_eq!(newest_answered.mix.levels, [80, 70]);
    assert!(newest_answered.sync.is_settled());
}

#[test]
fn a_refused_newest_mix_settles_it_so_the_next_push_is_shown() {
    let refused = connected_with(2, Vec::new())
        .sent(set_level(0, 40))
        .after(&[answer(1, None)]);

    assert_eq!(refused.mix.levels, [40, 100]);
    assert!(refused.sync.is_settled());
    assert_eq!(refused.after(&[push(server_mix())]).mix.levels, [80, 70]);
}

// Engineer: the engineer's limit and locks.

#[test]
fn the_reported_limit_and_locks_are_kept_and_show_on_the_strips() {
    let state = locked_by(engineer(60, &[1]), None);

    assert_eq!(state.mix.engineer, engineer(60, &[1]));
    assert_eq!(locks(&state.mix), [false, true, false]);
    assert!(state.mix.is_strip_locked(1));
    assert!(!state.mix.is_strip_locked(0));
}

#[test]
fn an_older_server_leaves_everything_open_and_unlimited() {
    let state = connected_with(3, Vec::new()).after(&[push(server_mix())]);

    assert_eq!(state.mix.engineer, EngineerControls::default());
    assert_eq!(locks(&state.mix), [false, false, false]);
    assert_eq!(state.mix.effective_cap(), state.mix.level_cap);
}

#[test]
fn the_musician_hears_the_lower_of_their_cap_and_the_engineers_limit() {
    let limited = locked_by(engineer(60, &[]), None);

    assert_eq!(limited.mix.level_cap, 100);
    assert_eq!(limited.mix.effective_cap(), 60);
    // The phone's own cap still moves, and wins when it is lower.
    let lower_cap = limited.after(&[set_cap(50)]);
    assert_eq!(lower_cap.mix.level_cap, 50);
    assert_eq!(lower_cap.mix.effective_cap(), 50);
    assert_eq!(lower_cap.outgoing_mix().max_level_percent, 50);
}

#[test]
fn a_locked_channel_cannot_be_changed_from_the_phone() {
    let state = locked_by(engineer(100, &[1]), None);

    let tried = state.after(&[
        set_level(1, 40),
        set_pan(1, 50),
        set_mute(1, true),
        set_solo(1, true),
    ]);

    assert_eq!(tried, state);
    // The other channels still move.
    assert_eq!(tried.after(&[set_level(0, 30)]).mix.levels, [30, 100, 100]);
}

#[test]
fn a_pair_is_locked_as_one_strip_when_either_side_is() {
    let pair = vec![
        info("Keys", ChannelKind::Keys).linked(),
        untouched(),
        info("Vox", ChannelKind::Other),
    ];
    let state = locked_by(engineer(100, &[1]), Some(pair));

    assert_eq!(locks(&state.mix), [true, false]);
    assert!(state.mix.is_strip_locked(0));
    assert_eq!(
        state.after(&[set_level(0, 40), set_pan(0, 25), set_mute(0, true)]),
        state
    );
}

#[test]
fn a_lock_arriving_while_a_mix_is_outstanding_holds_at_once_and_settles_with_the_answer() {
    let outstanding = connected_with(3, Vec::new()).sent(set_level(0, 40));
    let kept = neutral_mix((0..32).map(|c| if c == 0 { 70 } else { 100 }).collect());

    // The push is overtaken by the phone's mix still on its way, but the lock holds at once.
    let pushed = outstanding.after(&[push(kept.clone().with(&engineer(100, &[0])))]);
    assert_eq!(pushed.mix.levels, [40, 100, 100]);
    assert!(pushed.mix.is_strip_locked(0));
    assert_eq!(pushed.after(&[set_level(0, 20)]), pushed);

    // The answer to that mix carries the value the desk kept, and settles it.
    let answered = pushed.after(&[answer(1, Some(kept.with(&engineer(100, &[0]))))]);
    assert_eq!(answered.mix.levels, [70, 100, 100]);
    assert!(answered.sync.is_settled());
    assert!(answered.mix.is_strip_locked(0));
}

#[test]
fn a_refused_mix_leaves_the_engineers_controls_as_they_were() {
    let state = locked_by(engineer(60, &[1]), None).sent(set_level(0, 40));

    assert_eq!(
        state.after(&[answer(1, None)]).mix.engineer,
        engineer(60, &[1])
    );
}

#[test]
fn a_new_session_starts_unlocked_and_reports_outside_one_change_nothing() {
    let ended = locked_by(engineer(60, &[1]), None).after(&[MixEvent::SessionEnded]);

    assert_eq!(
        ended.after(&[push(server_mix().with(&engineer(40, &[0])))]),
        ended
    );
    assert_eq!(
        ended.after(&[MixEvent::SessionStarted]).mix.engineer,
        EngineerControls::default()
    );
}

// Following another musician's mix (a mirror the engineer set).

#[test]
fn a_reported_following_holds_and_an_older_server_follows_nobody() {
    let state = locked_by(following(Some("Ana"), &[]), None);

    assert_eq!(state.mix.following(), Some("Ana"));
    assert_eq!(
        connected_with(3, Vec::new())
            .after(&[push(server_mix())])
            .mix
            .following(),
        None
    );
    assert_eq!(
        state
            .after(&[push(neutral().with(&following(None, &[])))])
            .mix
            .following(),
        None
    );
}

#[test]
fn following_that_starts_while_a_mix_is_outstanding_holds_at_once_and_the_answer_brings_the_leaders_mix(
) {
    let outstanding = connected_with(3, Vec::new()).sent(set_level(0, 40));

    let pushed = outstanding.after(&[push(leaders_mix(60).with(&following(Some("Ana"), &[])))]);

    // The push is overtaken by the phone's mix still on its way; whom it follows holds at once.
    assert_eq!(pushed.mix.following(), Some("Ana"));
    assert_eq!(pushed.mix.levels, [40, 100, 100]);

    let answered = pushed.after(&[answer(
        1,
        Some(leaders_mix(60).with(&following(Some("Ana"), &[]))),
    )]);

    assert_eq!(answered.mix.levels, [60, 60, 60]);
    assert!(answered.sync.is_settled());
}

#[test]
fn while_following_channel_and_group_edits_change_nothing() {
    let state = running().after(&[
        configure(config_with(3, vec![group("Band", &[0, 1])], None)),
        push(neutral().with(&following(Some("Ana"), &[]))),
    ]);

    let tried = state.after(&[
        set_level(0, 40),
        set_pan(1, 20),
        set_mute(2, true),
        set_solo(0, true),
        set_group_level(0, 50),
    ]);

    assert_eq!(tried, state);
    assert_eq!(
        state
            .mix
            .strips()
            .into_iter()
            .map(|strip| strip.following)
            .collect::<Vec<_>>(),
        [ana(), ana(), ana()]
    );
    assert_eq!(
        state
            .mix
            .group_faders()
            .into_iter()
            .map(|fader| fader.following)
            .collect::<Vec<_>>(),
        [ana()]
    );
}

#[test]
fn while_following_the_master_the_cap_and_the_mute_are_still_the_musicians() {
    let state = locked_by(following(Some("Ana"), &[]), None).after(&[
        set_master(70),
        set_cap(80),
        set_master_mute(true),
    ]);

    assert_eq!(state.mix.master_volume, 70);
    assert_eq!(state.mix.level_cap, 80);
    assert!(state.mix.master_muted);
}

#[test]
fn an_edit_after_following_starts_with_a_mix_outstanding_changes_nothing() {
    let pushed = connected_with(3, Vec::new())
        .sent(set_level(0, 40))
        .after(&[push(leaders_mix(60).with(&following(Some("Ana"), &[])))]);

    assert_eq!(pushed.after(&[set_level(1, 20), set_mute(0, true)]), pushed);
}

#[test]
fn when_following_stops_the_controls_are_the_musicians_again_and_the_mix_stays_as_it_was() {
    let followed = connected_with(3, vec![group("Band", &[0])]).after(&[
        choose(1),
        toggle(true),
        push(leaders_mix(60).with(&following(Some("Ana"), &[]))),
    ]);

    let free = followed.after(&[push(leaders_mix(60).with(&following(None, &[])))]);

    assert_eq!(free.mix.following(), None);
    assert_eq!(free.mix.levels, [60, 60, 60]);
    assert_eq!(free.more_of_me.notice, None);
    assert!(!free.more_of_me.is_enabled);
    assert_eq!(free.after(&[set_level(0, 30)]).mix.levels, [30, 60, 60]);
    assert_eq!(free.after(&[set_group_level(0, 50)]).mix.group_levels, [50]);
    assert!(free.after(&[toggle(true)]).more_of_me.is_enabled);
    assert_eq!(
        free.mix
            .strips()
            .into_iter()
            .map(|strip| strip.following)
            .collect::<Vec<_>>(),
        [None, None, None]
    );
}

#[test]
fn a_locked_channel_of_a_follower_is_both_locked_and_following() {
    let strips = locked_by(following(Some("Ana"), &[1]), None).mix.strips();
    assert_eq!(strips.len(), 3);

    assert!(strips[1].is_locked);
    assert_eq!(strips[1].following, ana());
}

#[test]
fn a_new_session_follows_nobody_until_its_server_says() {
    let ended = locked_by(following(Some("Ana"), &[]), None).after(&[MixEvent::SessionEnded]);

    assert_eq!(ended.mix.following(), Some("Ana"));
    assert_eq!(
        ended.after(&[MixEvent::SessionStarted]).mix.following(),
        None
    );
}

// T5d, More of me

#[test]
fn more_of_me_needs_a_channel_before_it_turns_on() {
    let state = with_channels(2, Vec::new()).after(&[toggle(true)]);

    assert!(!state.more_of_me.is_enabled);
    assert_eq!(state.more_of_me.notice, Some(MoreOfMeNotice::NeedsChannel));
}

/// The Swift test checked the chosen channel's label, "CH 2": the channel it names by number is 1,
/// and it has no name of its own.
#[test]
fn more_of_me_turns_on_and_off_once_a_channel_is_chosen() {
    let on = with_channels(2, Vec::new()).after(&[toggle(true), choose(1), toggle(true)]);

    assert!(on.more_of_me.is_enabled);
    assert_eq!(on.more_of_me.notice, None);
    let channel = on.more_of_me.musician_channel.expect("a channel is chosen");
    assert_eq!(channel, 1);
    assert_eq!(on.mix.name_of(channel), None);
    assert!(!on.after(&[toggle(false)]).more_of_me.is_enabled);
}

#[test]
fn more_of_me_is_never_written_into_the_mix() {
    let on = with_channels(2, Vec::new()).after(&[set_level(0, 80), choose(1), toggle(true)]);

    assert_eq!(on.mix.levels, [80, 100]);
}

#[test]
fn choosing_no_channel_turns_more_of_me_off() {
    let off = with_channels(2, Vec::new()).after(&[choose(0), toggle(true), choose_none()]);

    assert_eq!(off.more_of_me.musician_channel, None);
    assert!(!off.more_of_me.is_enabled);
    assert_eq!(
        off.more_of_me.notice,
        Some(MoreOfMeNotice::NoChannelSelected)
    );
}

#[test]
fn choosing_another_channel_keeps_more_of_me_on() {
    let moved = with_channels(3, Vec::new()).after(&[choose(0), toggle(true), choose(2)]);

    assert!(moved.more_of_me.is_enabled);
    assert_eq!(moved.more_of_me.musician_channel, Some(2));
}

#[test]
fn only_an_announced_channel_can_be_chosen() {
    let state = with_channels(2, Vec::new()).after(&[choose(0)]);
    // Not in Android's test: without it, a reducer that ignored every choice would pass.
    assert_eq!(state.more_of_me.musician_channel, Some(0));

    assert_eq!(state.after(&[choose(2), choose(-1)]), state);
    assert_eq!(
        MixSession::default().after(&[choose(0)]),
        MixSession::default()
    );
}

#[test]
fn more_of_me_lifts_both_sides_of_a_pair() {
    let lifted = running().after(&[band_config(), choose(1), toggle(true)]);
    let played = lifted.played(&unlocked());

    assert_eq!(played, [50, 100, 100, 50, 50]);
    // The desk plays both sides lifted; hearing that back keeps More of me on.
    assert_eq!(
        lifted.after(&[push(report_of(&lifted.mix, Some(played)))]),
        lifted
    );
}

// A `config`: the musician's channel fitted to the announced strips, and pairs the engineer
// links.

#[test]
fn a_config_that_drops_the_musicians_channel_turns_more_of_me_off() {
    let on = with_channels(4, Vec::new()).after(&[choose(3), toggle(true)]);

    let dropped = on.after(&[configure(config(2))]);

    assert_eq!(dropped.more_of_me.musician_channel, None);
    assert!(!dropped.more_of_me.is_enabled);
    assert_eq!(
        dropped.more_of_me.notice,
        Some(MoreOfMeNotice::ChannelOutOfRange)
    );
}

/// Android builds the remembered channel into the state directly, as the app does with the
/// channel it kept from the last launch; so does this.
#[test]
fn a_remembered_channel_outside_the_first_config_is_forgotten_quietly() {
    let mut remembered = MixSession::default();
    remembered.more_of_me.musician_channel = Some(6);

    let state = remembered.after(&[configure(config(4))]);

    assert_eq!(state.more_of_me.musician_channel, None);
    assert_eq!(state.more_of_me.notice, None);
}

#[test]
fn a_remembered_channel_inside_the_config_is_kept() {
    let mut remembered = MixSession::default();
    remembered.more_of_me.musician_channel = Some(3);

    assert_eq!(
        remembered
            .after(&[configure(config(4))])
            .more_of_me
            .musician_channel,
        Some(3)
    );
}

#[test]
fn the_musicians_channel_on_a_pairs_second_side_moves_to_the_pair() {
    let remembered = with_channels(5, Vec::new()).after(&[choose(2)]);

    let paired = remembered.after(&[band_config()]);

    assert_eq!(paired.more_of_me.musician_channel, Some(1));
    assert_eq!(paired.more_of_me.notice, None);
    assert_eq!(paired.more_of_me_channels(), [1, 2]);
}

#[test]
fn a_config_that_pairs_the_musicians_channel_turns_more_of_me_off_and_says_why() {
    // The musician's channel on what becomes the pair's second side: it moves to the first, and
    // the lift the desk plays (one side) no longer matches the strip's (both sides).
    let second = with_channels(5, Vec::new())
        .after(&[choose(2), toggle(true)])
        .after(&[band_config()]);

    assert_eq!(second.more_of_me.musician_channel, Some(1));
    assert!(!second.more_of_me.is_enabled);
    assert_eq!(
        second.more_of_me.notice,
        Some(MoreOfMeNotice::ChannelPaired)
    );

    // The musician's channel on what becomes the pair's first side: the same.
    let first = with_channels(5, Vec::new())
        .after(&[choose(1), toggle(true)])
        .after(&[band_config()]);

    assert_eq!(first.more_of_me.musician_channel, Some(1));
    assert!(!first.more_of_me.is_enabled);
    assert_eq!(first.more_of_me.notice, Some(MoreOfMeNotice::ChannelPaired));

    // Turned on again it lifts the pair, and a config that keeps the pair as it is leaves it on.
    let again = second.after(&[toggle(true), band_config()]);

    assert!(again.more_of_me.is_enabled);
    assert_eq!(again.more_of_me_channels(), [1, 2]);
    // A pair elsewhere leaves it alone.
    let elsewhere = with_channels(5, Vec::new())
        .after(&[choose(0), toggle(true)])
        .after(&[band_config()]);

    assert!(elsewhere.more_of_me.is_enabled);
    assert_eq!(elsewhere.more_of_me.notice, None);
}

#[test]
fn after_a_link_turns_more_of_me_off_the_musicians_own_levels_go_back_with_the_moved_pans() {
    let on = running().after(&[
        configure(config(5)),
        set_level(0, 80),
        set_level(1, 60),
        set_level(2, 40),
        set_level(3, 70),
        set_level(4, 90),
        choose(2),
        toggle(true),
    ]);
    // The desk plays the lift the phone sent: CH 3 at 100, the rest halved.
    assert_eq!(
        lifted_levels(&on.mix.levels, &[2], &unlocked()),
        [40, 30, 100, 35, 45]
    );

    let paired = on.after(&[band_config()]);

    assert!(!paired.more_of_me.is_enabled);
    assert_eq!(paired.sync, on.sync);

    // The link's `mix_state`: the pair's second side took the first's (lifted) level, and its
    // pans moved apart. The phone keeps those pans and puts its own levels back, once.
    let restored = paired.after(&[push(linked_mix())]);

    assert_eq!(restored.mix.levels, [80, 60, 60, 70, 90]);
    assert_eq!(restored.mix.pans, [0, 0, 100, 100, 0]);
    assert!(!restored.more_of_me.is_enabled);
    assert_eq!(
        restored.more_of_me.notice,
        Some(MoreOfMeNotice::ChannelPaired)
    );
    assert_eq!(restored.sync.latest, paired.sync.latest + 1);

    // Its answer changes nothing more, and More of me turned on again lifts once.
    let answered = restored.after(&[answer(
        restored.sync.latest,
        Some(report_of(&restored.mix, None)),
    )]);
    assert_eq!(answered.mix, restored.mix);
    assert!(answered.sync.is_settled());
    let again = answered.after(&[toggle(true)]);
    assert_eq!(
        again.played(&again.mix.locked_sides()),
        [40, 100, 100, 35, 45]
    );
}

#[test]
fn an_edit_made_before_the_links_mix_arrives_stands_and_no_older_levels_are_restored() {
    let paired = running().after(&[
        configure(config(5)),
        set_level(0, 80),
        set_level(1, 60),
        set_level(2, 40),
        set_level(3, 70),
        set_level(4, 90),
        choose(2),
        toggle(true),
        band_config(),
    ]);
    // Not in Android's test: without levels waiting, "none restored" would hold vacuously.
    assert!(paired.own_levels_to_restore.is_some());

    // The musician moves a fader before the link's `mix_state` arrives: that send carries their
    // own levels (More of me is off), so nothing waits to be put back any more.
    let edited = paired.sent(set_level(0, 55));
    assert_eq!(edited.own_levels_to_restore, None);

    // The desk took that whole mix (the pair's sides made equal, the first winning).
    let desk = MixReport {
        channel_gains: vec![55, 60, 60, 70, 90],
        pans: vec![0, 100, 0, 100, 0],
        channel_mutes: vec![false; 5],
        channel_solos: vec![false; 5],
        ..report()
    };
    let answered = edited.after(&[push(linked_mix()), answer(edited.sync.latest, Some(desk))]);

    assert_eq!(answered.mix.levels, [55, 60, 60, 70, 90]);
    assert_eq!(answered.sync.latest, edited.sync.latest);
    assert!(answered.sync.is_settled());
}

#[test]
fn a_hidden_musician_channel_is_forgotten_and_more_of_me_says_why() {
    let on = with_channels(5, Vec::new()).after(&[choose(3), toggle(true)]);
    let remembered = with_channels(5, Vec::new()).after(&[choose(3)]);

    let hidden_on = on.after(&[band_config()]);
    let hidden_remembered = remembered.after(&[band_config()]);

    assert_eq!(hidden_on.more_of_me.musician_channel, None);
    assert!(!hidden_on.more_of_me.is_enabled);
    assert_eq!(
        hidden_on.more_of_me.notice,
        Some(MoreOfMeNotice::ChannelOutOfRange)
    );
    assert_eq!(hidden_remembered.more_of_me.musician_channel, None);
    assert_eq!(hidden_remembered.more_of_me.notice, None);
}

// What the engineer set: locks and following.

#[test]
fn more_of_me_leaves_a_pair_whole_when_only_one_side_is_locked() {
    let on = locked_by(engineer(100, &[1]), Some(keys_pair())).after(&[choose(2), toggle(true)]);
    let played = on.played(&on.mix.locked_sides());

    assert!(on.more_of_me.is_enabled);
    // Neither side of the locked pair is lowered; the voice is lifted.
    assert_eq!(played, [100, 100, 100]);
    // The desk keeps the pair as it was; hearing that back keeps More of me on.
    assert_eq!(
        on.after(&[push(
            report_of(&on.mix, Some(played)).with(&on.mix.engineer)
        )]),
        on
    );
}

#[test]
fn more_of_me_neither_lifts_nor_lowers_a_locked_channel() {
    let on =
        locked_by(engineer(100, &[2]), None).after(&[set_level(0, 80), choose(1), toggle(true)]);
    let played = on.played(&on.mix.engineer.locked_channels);

    assert!(on.more_of_me.is_enabled);
    assert_eq!(played, [40, 100, 100]);
    // The desk plays exactly that; hearing it back keeps More of me on.
    assert_eq!(
        on.after(&[push(
            report_of(&on.mix, Some(played)).with(&on.mix.engineer)
        )]),
        on
    );
}

#[test]
fn more_of_me_is_off_while_the_engineer_locks_the_musicians_channel() {
    let on = connected_with(3, Vec::new()).after(&[choose(1), toggle(true)]);
    let lifted = lifted_levels(&on.mix.levels, &[1], &unlocked());

    let locked = on.after(&[push(
        report_of(&on.mix, Some(lifted)).with(&engineer(100, &[1])),
    )]);

    assert!(!locked.more_of_me.is_enabled);
    assert_eq!(locked.more_of_me.musician_channel, Some(1));
    assert_eq!(
        locked.more_of_me.notice,
        Some(MoreOfMeNotice::ChannelLocked)
    );
    // Turning it on again is refused while the channel is locked, and says why.
    let asked = locked.after(&[toggle(false), toggle(true)]);
    assert!(!asked.more_of_me.is_enabled);
    assert_eq!(asked.more_of_me.notice, Some(MoreOfMeNotice::ChannelLocked));
    // Choosing a locked channel while More of me is on turns it off too.
    let moved = locked_by(engineer(100, &[2]), None).after(&[choose(0), toggle(true), choose(2)]);
    assert!(!moved.more_of_me.is_enabled);
    assert_eq!(moved.more_of_me.musician_channel, Some(2));
    assert_eq!(moved.more_of_me.notice, Some(MoreOfMeNotice::ChannelLocked));
}

#[test]
fn more_of_me_is_off_while_following_and_says_why() {
    let on = connected_with(3, Vec::new()).after(&[choose(1), toggle(true)]);

    let followed = on.after(&[push(leaders_mix(60).with(&following(Some("Ana"), &[])))]);

    assert!(!followed.more_of_me.is_enabled);
    assert_eq!(followed.more_of_me.notice, Some(MoreOfMeNotice::Following));
    assert_eq!(followed.mix.levels, [60, 60, 60]);
    // Turning it on is refused while it lasts; the musician's channel can still be chosen.
    let asked = followed.after(&[toggle(true)]);
    assert!(!asked.more_of_me.is_enabled);
    assert_eq!(asked.more_of_me.notice, Some(MoreOfMeNotice::Following));
    assert_eq!(
        asked.after(&[choose(2)]).more_of_me.musician_channel,
        Some(2)
    );
}

// T9a, recalling a saved mix
//
// Android's `MixRecalled(name, snapshot)` also makes the mix the active one and words a notice.
// Here the session takes only the mix; the active mix is `MixBank`'s (`mix_bank_parity`), and the
// notices are the screen's, so where Android checks one a test checks the recall refusal.
// `aPushedMixLeavesTheSavedMixesAsTheyWere` is not ported in Swift either: the saved mixes are not
// part of the session, so a push cannot reach them.

/// The saved mix both lock tests recall.
fn saved_everything_set() -> MixSnapshot {
    MixSnapshot {
        channel_gains: vec![10, 20, 30],
        channel_pans: vec![50, 50, 50],
        channel_mutes: vec![true, true, true],
        channel_solos: vec![true, true, true],
        ..MixSnapshot::default()
    }
}

#[test]
fn recalling_a_saved_mix_leaves_locked_channels_as_they_are() {
    let state = locked_by(engineer(100, &[1]), None);

    let recalled = state.after(&[recall(saved_everything_set())]).mix;

    assert_eq!(recalled.levels, [10, 100, 30]);
    assert_eq!(recalled.pans, [50, 100, 50]);
    assert_eq!(recalled.mutes, [true, false, true]);
    assert_eq!(recalled.solos, [true, false, true]);
}

#[test]
fn recall_keeps_a_pair_whole_when_only_one_side_is_locked() {
    let state = locked_by(engineer(100, &[1]), Some(keys_pair()));

    let recalled = state.after(&[recall(saved_everything_set())]).mix;

    // The pair is locked as one strip: neither side takes the saved values, so they stay equal.
    assert_eq!(recalled.levels, [100, 100, 30]);
    assert_eq!(recalled.pans, [0, 100, 50]);
    assert_eq!(recalled.mutes, [false, false, true]);
    assert_eq!(recalled.solos, [false, false, true]);
    assert_eq!(recalled.locked_sides(), BTreeSet::from([0, 1]));
}

/// Android's notice, "\"Verse\" was not recalled: your mix follows Ana's", and its unchanged active
/// mix are the screen's and the bank's; the refusal they come from is checked here.
#[test]
fn recalling_a_saved_mix_while_following_is_refused_and_says_why() {
    let state = locked_by(following(Some("Ana"), &[]), None);

    let refused = state.after(&[recall(MixSnapshot {
        channel_gains: vec![10, 20, 30],
        volume_percent: 40,
        ..MixSnapshot::default()
    })]);

    assert_eq!(refused.mix, state.mix);
    assert_eq!(
        state.recall_refusal(),
        Some(RecallRefusal::Following {
            leader: "Ana".to_owned()
        })
    );
    // A leader with no name is still a refusal; the screen names them in words.
    let nameless = locked_by(following(Some(""), &[]), None);
    assert_eq!(
        nameless.after(&[recall(MixSnapshot::default())]).mix,
        nameless.mix
    );
    assert_eq!(
        nameless.recall_refusal(),
        Some(RecallRefusal::Following {
            leader: String::new()
        })
    );
}

/// Android also checks that the recalled mix is the active one and its notice: `MixBank`'s
/// `activate` and the screen's.
#[test]
fn recalling_a_mix_applies_its_values_and_keeps_what_the_server_announced() {
    let groups = vec![group("Band", &[0, 1])];
    let live = with_channels(2, groups.clone()).after(&[choose(1)]);
    let snapshot = MixSnapshot {
        channel_gains: vec![30, 60],
        channel_pans: vec![20, 80],
        channel_mutes: vec![true, false],
        channel_solos: vec![false, true],
        group_levels: vec![40],
        volume_percent: 70,
        max_level_percent: 90,
        muted: true,
    };

    let recalled = live.after(&[recall(snapshot)]);

    assert_eq!(recalled.mix.levels, [30, 60]);
    assert_eq!(recalled.mix.pans, [20, 80]);
    assert_eq!(recalled.mix.mutes, [true, false]);
    assert_eq!(recalled.mix.solos, [false, true]);
    assert_eq!(recalled.mix.group_levels, [40]);
    assert_eq!(recalled.mix.master_volume, 70);
    assert_eq!(recalled.mix.level_cap, 90);
    assert!(recalled.mix.master_muted);
    assert_eq!(recalled.mix.source_channels, 2);
    assert_eq!(recalled.mix.groups, groups);
    assert_eq!(recalled.more_of_me.musician_channel, Some(1));
}

#[test]
fn a_mix_recalled_before_the_server_announces_keeps_its_group_levels() {
    let recalled = MixSession::default().after(&[recall(MixSnapshot {
        channel_gains: vec![50, 50],
        group_levels: vec![30, 40],
        ..MixSnapshot::default()
    })]);

    let announced = recalled.after(&[configure(config_with(
        2,
        vec![group("A", &[0]), group("B", &[1])],
        None,
    ))]);

    assert_eq!(
        announced
            .mix
            .group_faders()
            .iter()
            .map(|fader| fader.level)
            .collect::<Vec<_>>(),
        [30, 40]
    );
    assert_eq!(announced.mix.levels, [50, 50]);
}

#[test]
fn a_recalled_mix_with_another_channel_count_shows_its_strips_until_the_server_announces_again() {
    let recalled = with_channels(2, Vec::new()).after(&[recall(MixSnapshot {
        channel_gains: vec![10, 20, 30, 40],
        ..MixSnapshot::default()
    })]);

    assert_eq!(recalled.mix.levels, [10, 20, 30, 40]);
    assert_eq!(recalled.mix.pans, [0, 100, 0, 100]);

    let reannounced = recalled.after(&[configure(config(2))]);

    assert_eq!(reannounced.mix.levels, [10, 20]);
}
