//! The iOS client's former `OutgoingMixTests`: the outgoing mix and the send rule beyond the
//! Android parity suite: the rules Android's tests only reached through its ViewModel and its
//! socket.

use super::{
    answer, choose, config, config_with, configure, connected_with, controls, group, info, push,
    report, running, set_cap, set_group_level, set_level, set_master, set_master_mute, set_mute,
    set_pan, set_solo, toggle, untouched, Detail, Events, WireMix, WithControls,
};
use crate::control::client_message::MixCommand;
use crate::control::server_message::{ChannelKind, MixReport};
use crate::mix::{MixEvent, MixSession, MoreOfMeNotice};
use std::collections::BTreeSet;

/// Every `mix` is numbered, the first one too (the desk's answers are matched by it), and the
/// number is the newest change's, not the newest the desk answered.
#[test]
fn goes_out_under_the_number_of_the_newest_change_from_the_first_mix_on() {
    assert_eq!(MixSession::default().outgoing_mix().sequence, Some(0));
    assert_eq!(
        WireMix::new(&MixSession::default().outgoing_mix()).int("seq"),
        Some(0)
    );

    let two_sent = connected_with(2, Vec::new())
        .reduced_numbering_sends(&set_level(0, 40))
        .reduced_numbering_sends(&set_level(0, 50));
    let first_answered = two_sent.reduced(&answer(1, None));

    assert_eq!(two_sent.outgoing_mix().sequence, Some(2));
    assert_eq!(first_answered.sync.answered, 1);
    assert_eq!(first_answered.outgoing_mix().sequence, Some(2));
}

#[test]
fn lifts_the_musicians_strip_only_in_what_it_sends_and_sends_everything_else_as_stored() {
    let on = connected_with(3, vec![group("A", &[0]), group("B", &[1, 2])]).after(&[
        set_level(0, 80),
        set_pan(1, 30),
        set_mute(2, true),
        set_solo(0, true),
        set_group_level(1, 60),
        set_master(70),
        set_cap(90),
        set_master_mute(true),
        choose(1),
        toggle(true),
    ]);

    assert_eq!(
        on.outgoing_mix(),
        MixCommand {
            sequence: Some(0),
            volume_percent: 70,
            max_level_percent: 90,
            muted: true,
            channel_gains: Some(vec![40, 100, 50]),
            pans: Some(vec![0, 30, 0]),
            channel_mutes: Some(vec![false, false, true]),
            channel_solos: Some(vec![true, false, false]),
            group_levels: Some(vec![100, 60]),
        }
    );
    assert_eq!(on.mix.levels, [80, 100, 100]);
    assert_eq!(
        on.after(&[toggle(false)]).outgoing_mix().channel_gains,
        Some(vec![80, 100, 100])
    );
}

/// The keys `buildMixMessage` writes, and only those: every list on every `mix`, an empty one as an
/// empty list, and nothing of what the engineer set, even while the engineer limits, locks and
/// makes the mix follow another's.
#[test]
fn writes_every_list_android_writes_an_empty_one_too_and_nothing_the_engineer_set() {
    let held = connected_with(2, Vec::new()).after(&[push(
        MixReport {
            channel_gains: vec![70, 70],
            pans: vec![0, 100],
            channel_mutes: vec![false, false],
            channel_solos: vec![false, false],
            ..report()
        }
        .with(&controls(60, &[0], Some("Ana"))),
    )]);
    assert!(held.mix.following() == Some("Ana") && held.mix.engineer.is_limited());

    let wire = WireMix::new(&held.outgoing_mix());

    assert_eq!(
        wire.object
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "type",
            "seq",
            "volume_percent",
            "max_level_percent",
            "muted",
            "channels",
            "pans",
            "mutes",
            "solos",
            "group_levels",
        ])
    );
    assert_eq!(wire.ints("channels"), Some(vec![70, 70]));
    assert_eq!(wire.ints("group_levels"), Some(vec![]));
    assert_eq!(wire.int("max_level_percent"), Some(100));
}

/// The desk already plays what it reports, so a report is never sent back, even one that changed
/// the mix; nor is the start or the end of a session.
#[test]
fn never_sends_for_the_desks_reports_or_the_start_and_end_of_a_session() {
    let settled = connected_with(2, Vec::new());
    let outstanding = settled.reduced_numbering_sends(&set_level(0, 40));
    let unmuted = vec![false, false];
    let pushed = push(MixReport {
        channel_gains: vec![80, 70],
        pans: vec![0, 100],
        channel_mutes: unmuted.clone(),
        channel_solos: unmuted.clone(),
        ..report()
    });
    let answered = answer(
        1,
        Some(MixReport {
            channel_gains: vec![30, 70],
            pans: vec![0, 100],
            channel_mutes: unmuted.clone(),
            channel_solos: unmuted,
            ..report()
        }),
    );
    let cases = [
        (settled.clone(), pushed),
        (outstanding.clone(), answered),
        (outstanding, answer(1, None)),
        (MixSession::default(), MixEvent::SessionStarted),
        (settled, MixEvent::SessionEnded),
    ];

    for (before, event) in cases {
        let after = before.reduced(&event);
        // Each one changes the session, so a rule that sent whatever changed would send it.
        assert_ne!(after, before, "{event:?}");

        assert!(!MixSession::sends(&event, &before, &after), "{event:?}");
        assert_eq!(before.reduced_numbering_sends(&event), after, "{event:?}");
    }
}

/// The one report the phone answers with a send is the first after a link turned More of me off,
/// and the reducer numbers that one itself: through this rule it is numbered once, not twice.
#[test]
fn numbers_the_restore_after_a_link_once_as_the_reducer_does() {
    let paired = running().after(&[
        configure(config(3)),
        set_level(0, 80),
        set_level(1, 60),
        choose(1),
        toggle(true),
        configure(config_with(
            3,
            Vec::new(),
            Some(vec![
                info("Keys", ChannelKind::Keys).linked(),
                untouched(),
                untouched(),
            ]),
        )),
    ]);
    assert!(paired.own_levels_to_restore.is_some());
    // The link's `mix_state`: the second side took the first side's lifted level, pans apart.
    let unmuted = vec![false, false, false];
    let link = push(MixReport {
        channel_gains: vec![40, 40, 50],
        pans: vec![0, 100, 0],
        channel_mutes: unmuted.clone(),
        channel_solos: unmuted,
        ..report()
    });

    let restored = paired.reduced_numbering_sends(&link);

    assert_eq!(restored, paired.reduced(&link));
    assert_eq!(restored.sync.latest, paired.sync.latest + 1);
    assert_eq!(
        restored.outgoing_mix().channel_gains,
        Some(vec![80, 80, 100])
    );
    assert_eq!(restored.outgoing_mix().pans, Some(vec![0, 100, 0]));
}

/// Android's former view model sent a `config` only when it turned More of me off, so the desk
/// stops lifting; its parity suite only reaches the link, which is not sent.
#[test]
fn sends_a_config_that_turned_more_of_me_off_and_one_that_left_it_alone_not_at_all() {
    let on = connected_with(5, Vec::new()).after(&[choose(3), toggle(true)]);

    let dropped = on.reduced_numbering_sends(&configure(config(2)));
    let kept = on.reduced_numbering_sends(&configure(config(5)));

    assert_eq!(
        dropped.more_of_me.notice,
        Some(MoreOfMeNotice::ChannelOutOfRange)
    );
    assert_eq!(dropped.sync.latest, on.sync.latest + 1);
    assert_eq!(dropped.outgoing_mix().channel_gains, Some(vec![100, 100]));
    assert!(kept.more_of_me.is_enabled);
    assert_eq!(kept.sync, on.sync);
}

/// Android's former view model sent a choice only when it was taken while More of me was on: the
/// mix sent then lifts another channel, or none. Choosing the channel already chosen sends the same
/// mix again, as Android's rule did.
#[test]
fn sends_a_channel_choice_only_when_it_was_taken_while_more_of_me_was_on() {
    let off = connected_with(3, Vec::new());
    let on = off.after(&[choose(0), toggle(true)]);

    let chosen_while_off = off.reduced_numbering_sends(&choose(1));
    let moved = on.reduced_numbering_sends(&choose(1));
    let refused = on.reduced_numbering_sends(&choose(9));
    let again = on.reduced_numbering_sends(&choose(0));

    assert_eq!(chosen_while_off.more_of_me.musician_channel, Some(1));
    assert_eq!(chosen_while_off.sync, off.sync);
    assert_eq!(moved.sync.latest, on.sync.latest + 1);
    assert_eq!(moved.outgoing_mix().channel_gains, Some(vec![50, 100, 50]));
    assert_eq!(refused, on);
    assert_eq!(again.sync.latest, on.sync.latest + 1);
}

/// Android's former view model sent a toggle only when More of me actually turned on or off.
#[test]
fn sends_a_toggle_only_when_more_of_me_actually_turned_on_or_off() {
    let on = connected_with(2, Vec::new()).after(&[choose(0), toggle(true)]);

    let on_again = on.reduced_numbering_sends(&toggle(true));
    let turned_off = on.reduced_numbering_sends(&toggle(false));

    assert_eq!(on_again.sync, on.sync);
    assert_eq!(turned_off.sync.latest, on.sync.latest + 1);
    assert_eq!(
        turned_off.outgoing_mix().channel_gains,
        Some(vec![100, 100])
    );
}
