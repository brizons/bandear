//! `soundcheck-view.test.js`: the playable takes and Play's target, the card in every state, the
//! SOUNDCHECK chip, the transport during a soundcheck and the takes' Soundcheck buttons.

use super::*;
use crate::fixtures::{self, soundcheck_on, status_with, take};
use crate::record_view::{TransportLocal, record_messages, take_rows, transport_view};
use crate::soundcheck::{RECORDING_BLOCKS_SOUNDCHECK, SOUNDCHECK_BLOCKS_RECORDING};
use bandear_desktop_console::chips::ChipKind;
use bandear_desktop_console::router::Section;
use bandear_desktop_console::state::{Action, Phase};
use bandear_desktop_console::wire::{Recording, Soundcheck, StatusReport, TakeSummary};

fn show() -> TakeSummaryDto {
    take("Show")
}

fn line() -> TakeSummaryDto {
    let mut line = take("Line check");
    line.seconds = 62.0;
    line.frames = 2_976_000;
    line
}

fn live() -> TakeSummaryDto {
    let mut live = take("Encore");
    live.ended = "recording".to_owned();
    live
}

/// A running engine's state with `status`, the takes listed and the phase running.
fn running_with(status: StatusReport, takes: Vec<TakeSummaryDto>) -> ConsoleState {
    fixtures::reduce([
        Action::Status(Box::new(status)),
        Action::Engine {
            phase: Phase::Running,
            error: None,
        },
        Action::Takes(Box::new(fixtures::listing(takes))),
    ])
}

fn running() -> ConsoleState {
    running_with(status_with(|_| {}), vec![show(), line()])
}

fn with_soundcheck(soundcheck: Soundcheck) -> ConsoleState {
    running_with(
        status_with(|report| report.status.soundcheck = soundcheck),
        vec![show(), line()],
    )
}

fn recording_encore() -> Recording {
    Recording {
        state: "recording".to_owned(),
        take_folder: Some(live().folder),
        ..Recording::default()
    }
}

fn nowhere() -> TransportLocal {
    TransportLocal::default()
}

#[test]
fn the_takes_a_soundcheck_can_play_leave_out_the_take_being_recorded_and_play_has_a_target() {
    let state = running_with(status_with(|_| {}), vec![live(), show(), line()]);

    assert_eq!(
        playable_takes(&state),
        [
            PlayableTake {
                folder: show().folder,
                name: "Show".to_owned(),
                label: "Show \u{b7} 1:40".to_owned(),
            },
            PlayableTake {
                folder: line().folder,
                name: "Line check".to_owned(),
                label: "Line check \u{b7} 1:02".to_owned(),
            },
        ]
    );
    assert_eq!(
        soundcheck_target(&state, Some(&line().folder)),
        Some(line().folder),
        "the take chosen"
    );
    assert_eq!(
        soundcheck_target(&state, Some(&live().folder)),
        Some(show().folder),
        "never the take recording"
    );
    let after_line = running_with(
        status_with(|report| {
            report.status.last_take = Some(TakeSummary {
                folder: line().folder,
                ..TakeSummary::default()
            })
        }),
        vec![show(), line()],
    );
    assert_eq!(
        soundcheck_target(&after_line, None),
        Some(line().folder),
        "else the last take"
    );
    assert_eq!(
        soundcheck_target(&state, None),
        Some(show().folder),
        "else the newest"
    );
    assert_eq!(
        soundcheck_target(&running_with(status_with(|_| {}), Vec::new()), None),
        None,
        "none without takes"
    );
    let mut empty = take("Empty");
    empty.frames = 0;
    assert!(
        playable_takes(&running_with(status_with(|_| {}), vec![empty])).is_empty(),
        "a take with no frames plays nothing"
    );
}

#[test]
fn with_the_inputs_live_the_card_offers_the_takes_and_says_when_a_soundcheck_cannot_start() {
    let view = soundcheck_view(&running(), Some(&line().folder), None, false);
    assert!(!view.on);
    assert_eq!(view.badge, "Off");
    assert_eq!(view.badge_kind, BadgeKind::Off);
    assert_eq!(view.chosen, Some(line().folder));
    assert_eq!(view.choices.len(), 2);
    assert!(view.can_start);
    assert_eq!(view.start_hint, None);
    assert!(!view.looping);
    assert!(view.can_loop, "looping can be chosen before the start");
    assert!(soundcheck_view(&running(), None, None, true).looping);

    let idle = soundcheck_view(
        &fixtures::reduce([Action::Takes(Box::new(fixtures::listing(vec![show()])))]),
        None,
        None,
        false,
    );
    assert!(!idle.can_start);
    assert_eq!(
        idle.start_hint,
        Some("Start the engine to play a take in place of the inputs.")
    );
    let recording = soundcheck_view(
        &running_with(
            status_with(|report| report.status.recording = recording_encore()),
            vec![show(), line()],
        ),
        None,
        None,
        false,
    );
    assert!(!recording.can_start);
    assert_eq!(recording.start_hint, Some(RECORDING_BLOCKS_SOUNDCHECK));
    let empty = soundcheck_view(
        &running_with(status_with(|_| {}), Vec::new()),
        None,
        None,
        false,
    );
    assert!(!empty.can_start);
    assert_eq!(
        empty.start_hint,
        Some(
            "Record a take first: its tracks then play back here in place of the inputs, so \
             every mix and the phones can be set with nobody playing."
        )
    );
}

#[test]
fn a_take_playing_shows_where_it_is_what_paces_it_and_the_dropouts() {
    let state = with_soundcheck(Soundcheck {
        looping: true,
        clock: "output".to_owned(),
        ..soundcheck_on()
    });

    let view = soundcheck_view(&state, None, None, false);
    assert!(view.on);
    assert_eq!(view.badge, "Playing");
    assert_eq!(view.badge_kind, BadgeKind::Ok);
    assert_eq!(view.take_name.as_deref(), Some("Show"));
    assert_eq!(view.position, "0:12");
    assert_eq!(view.length, "1:40");
    assert_eq!(
        view.slider,
        Seek {
            max: 100.0,
            value: 12.0,
            disabled: false,
        }
    );
    assert!(view.can_pause);
    assert!(!view.can_resume);
    assert!(view.can_stop);
    assert!(view.looping);
    assert_eq!(view.clock, "Paced by the local output.");
    assert_eq!(view.underruns, "No dropouts.");
    assert_eq!(view.failure, None);
    assert!(!view.can_start, "one take at a time");

    let dragged = soundcheck_view(&state, None, Some(47.5), false);
    assert_eq!(
        dragged.position, "0:47",
        "the slider shows where it is being dragged to"
    );
    assert_eq!(dragged.slider.value, 47.5);

    let timed = soundcheck_view(
        &with_soundcheck(Soundcheck {
            clock: "timer".to_owned(),
            underrun_frames: 9600,
            ..soundcheck_on()
        }),
        None,
        None,
        false,
    );
    assert_eq!(timed.clock, "Paced by a timer: no local output plays.");
    assert_eq!(
        timed.underruns,
        "200 ms of silence where the disk fell behind."
    );
}

#[test]
fn a_paused_take_one_at_its_end_and_one_that_failed_say_what_is_left_to_do() {
    let paused = soundcheck_view(
        &with_soundcheck(Soundcheck {
            state: "paused".to_owned(),
            ..soundcheck_on()
        }),
        None,
        None,
        false,
    );
    assert_eq!(paused.badge, "Paused");
    assert_eq!(paused.badge_kind, BadgeKind::Warn);
    assert!(!paused.can_pause);
    assert!(paused.can_resume);
    assert_eq!(paused.resume_word, "Play");

    let at_end = soundcheck_view(
        &with_soundcheck(Soundcheck {
            state: "paused".to_owned(),
            position_frames: 100 * 48_000,
            position_seconds: 100.0,
            ..soundcheck_on()
        }),
        None,
        None,
        false,
    );
    assert_eq!(at_end.badge, "At the end");
    assert_eq!(at_end.resume_word, "Play again");

    let failed = soundcheck_view(
        &with_soundcheck(Soundcheck {
            state: "failed".to_owned(),
            reason: Some("reading \"01 Kick.wav\" failed".to_owned()),
            ..soundcheck_on()
        }),
        None,
        None,
        false,
    );
    assert_eq!(failed.badge, "Failed");
    assert_eq!(failed.badge_kind, BadgeKind::Error);
    assert_eq!(
        failed.failure.as_deref(),
        Some("The soundcheck stopped playing: reading \"01 Kick.wav\" failed.")
    );
    assert!(!failed.can_pause);
    assert!(!failed.can_resume);
    assert!(!failed.can_loop);
    assert!(failed.slider.disabled);
    assert!(failed.can_stop, "going back to the live inputs is left");
}

#[test]
fn the_soundcheck_chip_shows_on_every_screen_while_a_take_plays_in_place_of_the_inputs() {
    assert_eq!(soundcheck_chip(&running()), None, "the inputs are live");
    assert_eq!(
        soundcheck_chip(&with_soundcheck(soundcheck_on())),
        Some(Chip {
            kind: ChipKind::Soundcheck,
            word: "SOUNDCHECK".to_owned(),
            time: "0:12".to_owned(),
            detail: "Show".to_owned(),
            label: "Soundcheck: \"Show\" plays in place of the live inputs, at 12 seconds. Open \
                    Record"
                .to_owned(),
            target: Section::Record,
        })
    );
    let paused = soundcheck_chip(&with_soundcheck(Soundcheck {
        state: "paused".to_owned(),
        position_seconds: 1.4,
        ..soundcheck_on()
    }))
    .expect("a chip");
    assert_eq!(paused.kind, ChipKind::SoundcheckPaused);
    assert_eq!(paused.word, "SOUNDCHECK");
    assert_eq!(paused.detail, "Show \u{b7} paused");
    assert_eq!(
        paused.label,
        "Soundcheck: \"Show\" is paused in place of the live inputs, at 1 second. Open Record"
    );
    let failed = soundcheck_chip(&with_soundcheck(Soundcheck {
        state: "failed".to_owned(),
        reason: Some("the disk is gone".to_owned()),
        ..soundcheck_on()
    }))
    .expect("a chip");
    assert_eq!(failed.kind, ChipKind::SoundcheckFailed);
    assert_eq!(failed.word, "SOUNDCHECK stopped");
    assert_eq!(failed.detail, "Show");
    assert_eq!(
        failed.label,
        "The soundcheck stopped playing: the disk is gone. Open Record"
    );
}

#[test]
fn the_transport_plays_the_chosen_take_as_a_soundcheck_then_pauses_resumes_and_goes_back_live() {
    let chosen = TransportLocal {
        at_start: false,
        chosen_take: Some(line().folder),
    };
    let live_inputs = transport_view(&running(), &chosen);
    assert_eq!(
        live_inputs.play,
        Play {
            can: true,
            action: Some(PlayAction::Start),
            folder: Some(line().folder),
            label: "Play \"Line check\" in place of the live inputs".to_owned(),
        }
    );
    assert_eq!(live_inputs.stop_label, "Stop recording");

    let playing = transport_view(&with_soundcheck(soundcheck_on()), &nowhere());
    assert!(playing.soundcheck);
    assert!(!playing.can_record, "nothing records during a soundcheck");
    assert!(playing.can_stop);
    assert_eq!(playing.stop_label, "Back to live inputs");
    assert_eq!(playing.position, "0:12");
    assert_eq!(
        playing.state_word,
        "Soundcheck \u{b7} \"Show\" plays in place of the inputs"
    );
    assert_eq!(playing.state_kind, "soundcheck");
    assert_eq!(
        playing.play,
        Play {
            can: true,
            action: Some(PlayAction::Pause),
            folder: None,
            label: "Pause the soundcheck".to_owned(),
        }
    );
    assert!(
        playing.can_rewind,
        "Back to start moves the take to its start"
    );
    assert!(!playing.can_mark);
    assert!(playing.markers.is_empty(), "no markers during a soundcheck");
    assert_eq!(
        record_messages(&with_soundcheck(soundcheck_on())).locked,
        Some(SOUNDCHECK_BLOCKS_RECORDING),
        "the screen says why nothing records"
    );
    assert_eq!(record_messages(&running()).locked, None);

    let paused = transport_view(
        &with_soundcheck(Soundcheck {
            state: "paused".to_owned(),
            ..soundcheck_on()
        }),
        &nowhere(),
    );
    assert_eq!(paused.play.action, Some(PlayAction::Resume));
    assert_eq!(paused.play.label, "Play the soundcheck on");
    assert_eq!(paused.state_word, "Soundcheck \u{b7} \"Show\" is paused");

    let failed = transport_view(
        &with_soundcheck(Soundcheck {
            state: "failed".to_owned(),
            reason: Some("gone".to_owned()),
            ..soundcheck_on()
        }),
        &nowhere(),
    );
    assert!(!failed.play.can);
    assert_eq!(failed.play.label, "The soundcheck stopped on an error");
    assert!(failed.can_stop);
    assert!(!failed.can_rewind);
    assert_eq!(failed.state_word, "The soundcheck stopped on an error");

    let nothing = transport_view(&running_with(status_with(|_| {}), Vec::new()), &nowhere());
    assert!(!nothing.play.can);
    assert_eq!(nothing.play.label, "Record a take to play it back");
    let recording = transport_view(
        &running_with(
            status_with(|report| report.status.recording = recording_encore()),
            vec![show(), line()],
        ),
        &nowhere(),
    );
    assert!(!recording.play.can, "no soundcheck while a take records");
    assert_eq!(recording.play.label, RECORDING_BLOCKS_SOUNDCHECK);
    let idle = transport_view(
        &fixtures::reduce([Action::Takes(Box::new(fixtures::listing(vec![show()])))]),
        &nowhere(),
    );
    assert!(!idle.play.can, "nothing plays without an engine");
    assert_eq!(idle.play.label, "Play \"Show\" in place of the live inputs");
}

#[test]
fn each_take_row_can_start_its_soundcheck_and_the_one_playing_says_so() {
    let rows = take_rows(&with_soundcheck(soundcheck_on()), 0);
    assert_eq!(
        rows[0].1,
        RowSoundcheck {
            playing: true,
            disabled: true,
            title: "This take plays in place of the live inputs now".to_owned(),
        }
    );
    assert!(rows[1].1.disabled, "one take at a time");

    let live_rows = take_rows(
        &running_with(
            status_with(|report| report.status.recording = recording_encore()),
            vec![live(), show()],
        ),
        0,
    );
    assert!(live_rows[0].1.disabled, "not the take being recorded");
    assert_eq!(live_rows[0].1.title, "This take is being recorded");
    assert!(live_rows[1].1.disabled, "nor any take while one records");
    let idle = take_rows(&running(), 0);
    assert_eq!(
        idle[1].1,
        RowSoundcheck {
            playing: false,
            disabled: false,
            title: "Play \"Line check\" in place of the live inputs".to_owned(),
        }
    );
    let stopped = take_rows(
        &fixtures::reduce([Action::Takes(Box::new(fixtures::listing(vec![show()])))]),
        0,
    );
    assert!(stopped[0].1.disabled, "nothing plays without an engine");
}
