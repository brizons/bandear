//! [`MixModel`]: new tests, not ported. The model has no Swift counterpart of its own: it is the
//! wiring iOS's `ControlSession` (`reduce`, the opening mix) and `SavedMixes` (save, rename,
//! delete, recall) did around the former Swift `MixSession` and `MixBank`, moved here so both
//! phones share it.

use super::{
    answer, choose, config, config_with, configure, controls, info, push, report, report_of,
    set_level, set_master, toggle, unlocked, untouched, Detail, Events, WithControls,
};
use crate::control::mix_bank::{MixSnapshot, SavedMix};
use crate::control::server_message::{ChannelKind, MixReport};
use crate::mix::{
    MixBankChange, MixBankOutcome, MixEvent, MixModel, MixRecall, MixUpdate, RecallOutcome,
    RecallRefusal,
};

fn saved(name: &str, gains: &[i64]) -> SavedMix {
    SavedMix {
        name: name.to_owned(),
        snapshot: MixSnapshot {
            channel_gains: gains.to_vec(),
            ..MixSnapshot::default()
        },
    }
}

/// A model whose session started and whose desk announced `channels` channels.
fn connected(channels: u8) -> MixModel {
    let mut model = MixModel::new(None, Vec::new());
    model.apply(&MixEvent::SessionStarted);
    model.apply(&configure(config(channels)));
    model
}

#[test]
fn starts_with_the_remembered_channel_and_the_saved_mixes_none_active() {
    let model = MixModel::new(Some(3), vec![saved("Verse", &[30])]);

    assert_eq!(model.session().more_of_me.musician_channel, Some(3));
    assert_eq!(model.bank().names(), ["Verse"]);
    assert_eq!(model.bank().active_name(), None);
}

#[test]
fn applies_an_event_and_hands_back_the_mix_to_send_when_it_numbered_one() {
    let mut model = connected(2);

    let sent = model.apply(&set_level(0, 40));
    let unchanged = model.apply(&set_level(0, 40));

    let command = sent.command.expect("a change that changed the mix is sent");
    assert_eq!(command.sequence, Some(1));
    assert_eq!(command.channel_gains, Some(vec![40, 100]));
    assert!(!sent.musician_channel_changed);
    assert_eq!(
        unchanged,
        MixUpdate {
            command: None,
            musician_channel_changed: false
        }
    );
    assert_eq!(model.session().sync.latest, 1);
    // The command is the session's outgoing mix, in the same step as its number.
    assert_eq!(Some(model.session().outgoing_mix()), Some(command));
}

#[test]
fn hands_back_the_restore_the_reducer_numbers_itself() {
    // Channel 2 lifted, then the engineer linked channels 1 and 2, which turned More of me off.
    let mut model = connected(3);
    model.apply(&set_level(0, 80));
    model.apply(&choose(2));
    model.apply(&toggle(true));
    // The desk answers the lifted mix with the mix it now plays.
    let played = report_of(
        &model.session().mix,
        Some(model.session().played(&unlocked())),
    );
    model.apply(&answer(model.session().sync.latest, Some(played)));
    let link = model.apply(&configure(config_with(
        3,
        Vec::new(),
        Some(vec![
            untouched(),
            info("Keys", ChannelKind::Keys).linked(),
            untouched(),
        ]),
    )));
    assert_eq!(link.command, None);
    let latest = model.session().sync.latest;

    // The link's `mix_state`: the lift the desk plays, the pair's second side at its first's.
    let update = model.apply(&push(MixReport {
        channel_gains: vec![40, 50, 50],
        pans: vec![0, 0, 100],
        channel_mutes: vec![false; 3],
        channel_solos: vec![false; 3],
        ..report()
    }));

    assert_eq!(model.session().sync.latest, latest + 1);
    assert_eq!(
        update.command.and_then(|command| command.channel_gains),
        Some(vec![80, 100, 100])
    );
}

#[test]
fn says_when_the_musicians_channel_changed() {
    let mut model = MixModel::new(Some(1), Vec::new());

    let chosen_before_config = model.apply(&choose(0));
    let kept = model.apply(&configure(config(4)));
    let chosen = model.apply(&choose(2));
    let dropped = model.apply(&configure(config(2)));

    // Nothing is announced yet, so the choice changes nothing.
    assert!(!chosen_before_config.musician_channel_changed);
    assert!(!kept.musician_channel_changed);
    assert!(chosen.musician_channel_changed);
    assert!(dropped.musician_channel_changed);
    assert_eq!(model.session().more_of_me.musician_channel, None);
}

#[test]
fn opens_each_connection_with_the_mix_as_it_stands_and_numbers_nothing_new() {
    let mut model = connected(2);
    model.apply(&set_master(70));

    let opening = model.opening_mix();

    assert_eq!(opening, model.session().outgoing_mix());
    assert_eq!(opening.sequence, Some(1));
    assert_eq!(model.session().sync.latest, 1);
}

#[test]
fn saves_the_musicians_own_mix_and_says_whether_the_list_changed() {
    let mut model = connected(2);
    model.apply(&set_level(0, 30));
    model.apply(&choose(1));
    model.apply(&toggle(true));

    let saved_change = model.save_mix(" Verse ");
    let refused = model.save_mix("  ");

    assert_eq!(
        saved_change,
        MixBankChange {
            outcome: MixBankOutcome::Saved {
                name: "Verse".to_owned()
            },
            saved_mixes_changed: true,
        }
    );
    // Saved without the lift.
    assert_eq!(model.bank().mixes()[0].snapshot.channel_gains, [30, 100]);
    assert_eq!(model.bank().active_name(), Some("Verse"));
    assert_eq!(
        refused,
        MixBankChange {
            outcome: MixBankOutcome::NameNeeded,
            saved_mixes_changed: false,
        }
    );
}

#[test]
fn renames_and_deletes_and_says_whether_the_list_changed() {
    let mut model = MixModel::new(None, vec![saved("Verse", &[]), saved("Chorus", &[])]);

    assert_eq!(
        model.rename_mix("Verse", "Chorus"),
        MixBankChange {
            outcome: MixBankOutcome::NameTaken {
                name: "Chorus".to_owned()
            },
            saved_mixes_changed: false,
        }
    );
    assert!(model.rename_mix("Verse", "Bridge").saved_mixes_changed);
    assert!(model.delete_mix("Chorus").saved_mixes_changed);
    assert_eq!(
        model.delete_mix("Gone"),
        MixBankChange {
            outcome: MixBankOutcome::Deleted {
                name: "Gone".to_owned()
            },
            saved_mixes_changed: false,
        }
    );
    assert_eq!(model.bank().names(), ["Bridge"]);
}

#[test]
fn recalls_a_saved_mix_sends_it_and_makes_it_the_active_one() {
    let mut model = connected(2);
    model.reload_saved_mixes(vec![saved("Verse", &[30, 60])]);

    let recall = model.recall_mix("Verse");

    assert_eq!(
        recall.outcome,
        RecallOutcome::Recalled {
            name: "Verse".to_owned()
        }
    );
    assert_eq!(
        recall
            .update
            .command
            .and_then(|command| command.channel_gains),
        Some(vec![30, 60])
    );
    assert_eq!(model.session().mix.levels, [30, 60]);
    assert_eq!(model.bank().active_name(), Some("Verse"));
}

#[test]
fn refuses_a_recall_while_following_and_says_why() {
    let mut model = connected(2);
    model.apply(&push(report().with(&controls(100, &[], Some("Ana")))));
    model.reload_saved_mixes(vec![saved("Verse", &[30, 60])]);
    let before = model.session().clone();

    let recall = model.recall_mix("Verse");

    assert_eq!(
        recall,
        MixRecall {
            outcome: RecallOutcome::Refused {
                name: "Verse".to_owned(),
                refusal: RecallRefusal::Following {
                    leader: "Ana".to_owned()
                },
            },
            update: MixUpdate {
                command: None,
                musician_channel_changed: false
            },
        }
    );
    assert_eq!(model.session(), &before);
    assert_eq!(model.bank().active_name(), None);
}

#[test]
fn says_a_recalled_mix_that_is_no_longer_saved_is_missing() {
    let mut model = connected(2);
    let before = model.session().clone();

    let recall = model.recall_mix("Gone");

    assert_eq!(
        recall.outcome,
        RecallOutcome::Missing {
            name: "Gone".to_owned()
        }
    );
    assert_eq!(recall.update.command, None);
    assert_eq!(model.session(), &before);
}

#[test]
fn a_reload_keeps_the_active_mix_only_while_it_is_saved() {
    let mut model = MixModel::new(None, vec![saved("Verse", &[])]);
    model.save_mix("Verse");

    model.reload_saved_mixes(vec![saved("Chorus", &[])]);

    assert_eq!(model.bank().names(), ["Chorus"]);
    assert_eq!(model.bank().active_name(), None);
}
