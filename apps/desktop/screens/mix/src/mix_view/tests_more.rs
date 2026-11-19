//! The view parts of `engineer.test.js` and `local-mix.test.js`, the strips' `followingState`
//! (`mix-tools-logic.test.js`) and their processing links and balance.

use bandear_desktop_console::mix::{MAX_CHANNELS, Mix, MixField, edits};
use bandear_desktop_console::state::{Action, ConsoleState, LOCAL_MIX, Phase};
use bandear_desktop_console::testing::sample_status;
use bandear_desktop_console::types::dto::EngineerControlsSetDto;
use bandear_desktop_console::wire::{LocalOutput, Meters, OutputLevels, StatusReport};

use super::tests::{engine, mix_edit};
use super::*;
use crate::testing::{LEAD, reduced, running, status_of, with_status};

// --- The engineer's limit and locks on screen (`engineer.test.js`) ------------------------------

fn locks(channels: &[usize]) -> Vec<bool> {
    let mut table = vec![false; MAX_CHANNELS];
    for channel in channels {
        table[*channel] = true;
    }
    table
}

fn with_controls(
    limit: u8,
    locked: &[usize],
    change: impl FnOnce(&mut StatusReport),
) -> ConsoleState {
    let mut status = sample_status();
    status.musicians[0].mix.engineer_limit_percent = limit;
    status.musicians[0].mix.locked_channels = locks(locked);
    change(&mut status);
    reduced(vec![status_of(status)])
}

#[test]
fn a_channel_strip_says_whether_it_is_locked_in_words() {
    let strips = channel_strips(&with_controls(100, &[1], |_| {}), LEAD);
    assert!(!strips[0].locked);
    assert_eq!(strips[0].lock_text, "Unlocked");
    assert!(strips[1].locked);
    assert_eq!(strips[1].lock_text, "Locked");
}

#[test]
fn a_pair_shows_locked_when_either_side_is_and_its_lock_goes_to_both_inputs() {
    let state = with_controls(100, &[2], |status| status.status.channels[1].link = true);
    let pair = &channel_strips(&state, LEAD)[1];
    assert_eq!(pair.sides, [1, 2]);
    assert!(pair.locked);
    assert_eq!(
        channel_edits(
            &pair.sides,
            pair.stereo,
            MixField::LockedChannels,
            false.into()
        ),
        edits([
            ("lockedChannels.1", false.into()),
            ("lockedChannels.2", false.into())
        ])
        .unwrap()
    );
}

#[test]
fn the_master_shows_the_engineers_limit_apart_from_the_phones_cap_and_says_the_lower_wins() {
    let limited = master_strip(&with_controls(60, &[], |_| {}), LEAD).unwrap();
    assert_eq!(limited.cap, Some(90));
    assert_eq!(limited.limit, Some(60));
    assert_eq!(limited.readout, "85% \u{b7} cap 90% \u{b7} limit 60%");
    assert_eq!(
        limited.ceiling_note.as_deref(),
        Some("The lower of the phone's cap and your limit wins: at most 60%.")
    );
    let capped = master_strip(&with_controls(95, &[], |_| {}), LEAD).unwrap();
    assert_eq!(
        capped.ceiling_note.as_deref(),
        Some("The lower of the phone's cap and your limit wins: at most 90%.")
    );
}

#[test]
fn the_engineers_pending_edits_show_at_once_as_a_faders_do() {
    let state = with_status(
        with_controls(100, &[], |_| {}),
        vec![mix_edit(
            LEAD,
            &[
                ("engineerLimitPercent", 50.into()),
                ("lockedChannels.0", true.into()),
            ],
        )],
    );
    assert_eq!(master_strip(&state, LEAD).unwrap().limit, Some(50));
    assert!(channel_strips(&state, LEAD)[0].locked);
}

#[test]
fn a_lock_waiting_while_an_input_joins_a_pair_is_dropped_as_its_level_would_be() {
    let before = with_status(
        with_controls(100, &[], |_| {}),
        vec![mix_edit(
            LEAD,
            &[
                ("lockedChannels.1", true.into()),
                ("engineerLimitPercent", 40.into()),
            ],
        )],
    );
    let mut paired = sample_status();
    paired.status.channels[1].link = true;
    let after = with_status(before, vec![status_of(paired)]);
    assert_eq!(
        after.pending_edits(LEAD),
        edits([("engineerLimitPercent", 40.into())]).unwrap()
    );
}

#[test]
fn an_answer_that_took_the_controls_but_could_not_keep_them_says_so_above_the_mix() {
    let sent = edits([("engineerLimitPercent", 60.into())]).unwrap();
    let outcome = |saved: bool, save_error: Option<&str>| EngineerControlsSetDto {
        address: LEAD.to_owned(),
        limit_percent: 60,
        locked_channels: locks(&[]),
        kept_for: "phone",
        applied: true,
        saved,
        save_error: save_error.map(str::to_owned),
        mix: None,
    };
    let state = with_status(
        with_controls(100, &[], |_| {}),
        vec![
            Action::MixEdit {
                address: LEAD.to_owned(),
                edits: sent.clone(),
            },
            Action::EngineerApplied {
                address: LEAD.to_owned(),
                sent,
                outcome: Box::new(outcome(false, Some("the disk is full."))),
            },
        ],
    );
    assert!(state.pending_edits(LEAD).is_empty());
    assert_eq!(
        mix_messages(&state).controls.as_deref(),
        Some("Lead vocal's limit and locks apply now, but were not saved: the disk is full.")
    );
    let cleared = with_status(
        state,
        vec![Action::EngineerApplied {
            address: LEAD.to_owned(),
            sent: Default::default(),
            outcome: Box::new(outcome(true, None)),
        }],
    );
    assert_eq!(mix_messages(&cleared).controls, None);
}

// --- The local mix on screen (`local-mix.test.js`) ---------------------------------------------

fn with_output(output: LocalOutput) -> ConsoleState {
    let mut status = sample_status();
    status.status.local_output = Some(output);
    reduced(vec![status_of(status)])
}

fn playing_demo() -> LocalOutput {
    LocalOutput {
        device: Some("Demo interface".to_owned()),
        first_channel: Some(2),
        state: "running".to_owned(),
        ..LocalOutput::default()
    }
}

#[test]
fn the_local_mix_tab_comes_last_and_says_where_the_local_mix_plays() {
    let tabs = mix_tabs(&with_output(playing_demo()));
    assert_eq!(tabs.len(), sample_status().musicians.len() + 1);
    assert_eq!(
        tabs.last(),
        Some(&MixTab {
            address: LOCAL_MIX.to_owned(),
            label: "Local mix".to_owned(),
            link: TabLink {
                level: TabLevel::Local,
                label: "This computer \u{b7} Demo interface \u{b7} outputs 3/4".to_owned(),
                detail: "The local mix plays on this computer".to_owned(),
            },
            selected: false,
        })
    );
    let off = local_tab(&with_output(LocalOutput {
        state: "off".to_owned(),
        ..LocalOutput::default()
    }))
    .unwrap();
    assert_eq!(off.link.label, "This computer \u{b7} no output");
    assert_eq!(
        off.link.detail,
        "Choose where it plays in Network and system"
    );
    let failed = local_tab(&with_output(LocalOutput {
        state: "failed".to_owned(),
        reason: Some("unplugged".to_owned()),
        ..playing_demo()
    }))
    .unwrap();
    assert_eq!(failed.link.level, TabLevel::Warning);
    assert_eq!(failed.link.label, "This computer \u{b7} output stopped");
    assert_eq!(failed.link.detail, "unplugged");
    assert_eq!(
        local_tab(&ConsoleState::new()),
        None,
        "no engine, no local mix"
    );
    assert!(mix_tabs(&ConsoleState::new()).is_empty());
}

#[test]
fn the_local_mix_is_picked_like_a_musician_and_its_tab_says_it_is_open() {
    let picked = with_status(running(), vec![Action::Select(LOCAL_MIX.to_owned())]);
    assert!(local_tab(&picked).unwrap().selected);
    let again = with_status(picked, vec![status_of(sample_status())]);
    assert_eq!(
        again.selected.as_deref(),
        Some(LOCAL_MIX),
        "a status keeps it open"
    );
    let ended = with_status(again, vec![engine(Phase::Idle, None)]);
    assert_eq!(local_tab(&ended), None);
}

#[test]
fn the_local_mixs_strips_have_no_lock_and_its_master_no_cap_or_limit() {
    let state = with_status(running(), vec![Action::Select(LOCAL_MIX.to_owned())]);
    let strips = channel_strips(&state, LOCAL_MIX);
    assert!(!strips.is_empty());
    assert!(
        strips
            .iter()
            .all(|strip| !strip.lockable && strip.following.is_none())
    );
    assert!(
        channel_strips(&state, LEAD)
            .iter()
            .all(|strip| strip.lockable)
    );
    assert_eq!(
        master_strip(&state, LOCAL_MIX),
        Some(MasterStrip {
            volume: 100,
            cap: None,
            limit: None,
            muted: false,
            readout: "100%".to_owned(),
            ceiling_note: None,
        })
    );
}

#[test]
fn with_no_phones_the_local_mix_can_still_be_opened_and_shows_its_strips() {
    let mut status = sample_status();
    status.musicians.clear();
    let empty = reduced(vec![status_of(status)]);
    assert_eq!(mix_notice(&empty), Some(MixNotice::NoMusicians));
    assert_eq!(mix_tabs(&empty).len(), 1, "the Local mix tab alone");
    let local = with_status(empty, vec![Action::Select(LOCAL_MIX.to_owned())]);
    assert_eq!(mix_notice(&local), None);
    assert!(!channel_strips(&local, LOCAL_MIX).is_empty());
}

#[test]
fn the_master_meter_shows_the_local_mixs_peak_on_the_local_mix_tab_and_a_musicians_on_theirs() {
    let mut meters = Meters {
        channels: vec![10, 20],
        local: OutputLevels {
            left: 7,
            right: 9,
            limiter_reduction_db: 0.0,
        },
        ..Meters::default()
    };
    meters.musicians.insert(
        LEAD.to_owned(),
        OutputLevels {
            left: 40,
            right: 50,
            limiter_reduction_db: 0.0,
        },
    );
    assert_eq!(master_levels(Some(&meters), Some(LOCAL_MIX)), (7, 9));
    assert_eq!(master_levels(Some(&meters), Some(LEAD)), (40, 50));
    assert_eq!(
        master_levels(Some(&meters), Some("192.168.1.99:50000")),
        (0, 0)
    );
    assert_eq!(master_levels(None, Some(LOCAL_MIX)), (0, 0));
}

#[test]
fn a_strip_of_a_mix_that_follows_another_says_so_and_takes_no_move() {
    assert_eq!(
        following_state(Some("Lead vocal"), "Kick"),
        Following {
            disabled: true,
            text: "Follows Lead vocal".to_owned(),
            label: "Kick, follows Lead vocal's mix".to_owned(),
        }
    );
    assert_eq!(
        following_state(None, "Kick"),
        Following {
            disabled: false,
            text: String::new(),
            label: "Kick".to_owned(),
        }
    );
}

#[test]
fn a_strips_processing_link_opens_its_first_input_and_says_whether_it_is_on() {
    assert_eq!(
        processing_link(&[1, 2], "Keys L", Some(true)),
        ProcessingLink {
            channel: 2,
            state: ProcessingState::On,
            text: "Processing on",
            label: "Processing of Keys L, on".to_owned(),
        }
    );
    assert_eq!(
        processing_link(&[0], "Kick", Some(false)).label,
        "Processing of Kick, off"
    );
    let unread = processing_link(&[0], "Kick", None);
    assert_eq!(unread.text, "Processing");
    assert_eq!(unread.label, "Processing of Kick, not read yet");
}

#[test]
fn a_pairs_balance_and_its_two_pans_read_each_other_back() {
    assert_eq!(balance_to_pans(50), (0, 100));
    assert_eq!(balance_to_pans(70), (40, 100));
    assert_eq!(balance_to_pans(0), (0, 0));
    assert_eq!(pans_to_balance(40, 100), 70);
    assert_eq!(pans_to_balance(1, 2), 2, "halves round up, as Math.round");
    assert_eq!(Mix::engine_default().pans[..2], [0, 100]);
}
