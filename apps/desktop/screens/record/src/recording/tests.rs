//! `recording.test.js`: pairs and arming, the whole settings request, and the words of starts,
//! stops and unreadable takes; and what the console keeps past a run's end.

use super::*;
use crate::fixtures::{self, ROOT, recorder_dto, recording_now, status_with};
use bandear_desktop_console::state::{Action, Phase, RecordingAction, RecordingOutcome};
use bandear_desktop_console::types::dto::DiskEstimateDto;
use bandear_desktop_console::wire::TakeSummary;

/// CH 2 linked to CH 3, of four inputs.
const LINKED: [bool; 4] = [false, true, false, false];

#[test]
fn either_side_of_a_linked_pair_arms_the_pair_and_disarming_a_side_disarms_both() {
    assert_eq!(record_units(&LINKED), vec![vec![0], vec![1, 2], vec![3]]);
    assert!(is_armed(&[3], 2, &LINKED), "CH 3 arms the pair CH 2-3");
    assert!(!is_armed(&[3], 4, &LINKED));
    assert_eq!(
        armed_after(&[1, 3], 2, false, &LINKED),
        [1],
        "both sides go"
    );
    assert_eq!(armed_after(&[4], 1, true, &LINKED), [1, 4]);
    assert_eq!(armed_after(&[4], 4, true, &LINKED), [4], "once each");
    assert_eq!(
        armed_after(&[], 2, true, &[]),
        [2],
        "an input the status does not list"
    );
}

#[test]
fn a_linked_last_input_has_no_partner_and_records_alone() {
    assert_eq!(record_units(&[false, true]), vec![vec![0], vec![1]]);
}

#[test]
fn a_settings_request_is_the_whole_settings_with_the_change_on_top_the_default_folder_none() {
    assert_eq!(
        settings_request(None, &SettingsChange::default()),
        RecordingSettingsRequest {
            folder: None,
            format: Some("int24".to_owned()),
            include_local: false,
            armed: Vec::new(),
        }
    );
    let kept = RecordingSettingsDto {
        folder: "D:\\Takes".to_owned(),
        default_folder: fixtures::DEFAULT_FOLDER.to_owned(),
        folder_is_default: false,
        format: "float32",
        include_local: true,
        armed: vec![1, 2],
        notice: None,
    };
    assert_eq!(
        settings_request(
            Some(&kept),
            &SettingsChange {
                armed: Some(vec![5]),
                ..SettingsChange::default()
            }
        ),
        RecordingSettingsRequest {
            folder: Some("D:\\Takes".to_owned()),
            format: Some("float32".to_owned()),
            include_local: true,
            armed: vec![5],
        }
    );
    let blank = SettingsChange {
        folder: Some(Some("  ".to_owned())),
        ..SettingsChange::default()
    };
    assert_eq!(
        settings_request(Some(&kept), &blank).folder,
        None,
        "blank is the default"
    );
    let by_default = RecordingSettingsDto {
        folder_is_default: true,
        ..kept.clone()
    };
    assert_eq!(
        settings_request(Some(&by_default), &SettingsChange::default()).folder,
        None
    );
    let chosen = SettingsChange {
        folder: Some(None),
        ..SettingsChange::default()
    };
    assert_eq!(
        settings_request(Some(&kept), &chosen).folder,
        None,
        "the default folder asked for"
    );
}

#[test]
fn an_arm_change_in_a_settings_request_applies_to_the_armed_inputs_it_is_built_on() {
    let kept = RecordingSettingsDto {
        armed: vec![1, 4],
        ..fixtures::settings()
    };
    let arm = |input, on| SettingsChange {
        arm: Some(Arm {
            input,
            on,
            links: LINKED.to_vec(),
        }),
        ..SettingsChange::default()
    };

    assert_eq!(
        settings_request(Some(&kept), &arm(3, true)).armed,
        [1, 3, 4]
    );
    let pair = RecordingSettingsDto {
        armed: vec![1, 2],
        ..kept.clone()
    };
    assert_eq!(
        settings_request(Some(&pair), &arm(3, false)).armed,
        [1],
        "a pair"
    );
    assert_eq!(settings_request(None, &arm(2, true)).armed, [2]);
}

#[test]
fn lengths_starts_stops_and_unreadable_takes_are_said_in_words() {
    assert_eq!(format_duration(7.9), "0:07");
    assert_eq!(format_duration(205.0), "3:25");
    assert_eq!(format_duration(3723.0), "1:02:03");
    assert_eq!(format_duration(-4.0), "0:00", "never negative");
    assert_eq!(format_duration(f64::NAN), "0:00");

    let started = RecordingStartedDto {
        recording: recorder_dto(&["Kick", "Local mix"]),
        folder: "D:\\Takes".to_owned(),
        armed: vec![1],
        skipped: vec![9, 10],
        include_local: true,
        format: "int24",
    };
    assert_eq!(
        format_started(&started, "Show"),
        "Recording \"Show\" on 2 tracks in D:\\Takes. CH 9, CH 10 are armed but not on this \
         device, so not recorded."
    );
    let one = RecordingStartedDto {
        recording: recorder_dto(&["Kick"]),
        skipped: vec![9],
        ..started.clone()
    };
    assert_eq!(
        format_started(&one, "Show"),
        "Recording \"Show\" on 1 track in D:\\Takes. CH 9 is armed but not on this device, so \
         not recorded."
    );
    let all = RecordingStartedDto {
        skipped: Vec::new(),
        ..started
    };
    assert_eq!(
        format_started(&all, "Show"),
        "Recording \"Show\" on 2 tracks in D:\\Takes."
    );

    let mut take = fixtures::take("Show");
    take.seconds = 205.0;
    assert_eq!(format_stopped(&take), "\"Show\" stopped: 3:25 on 1 track.");
    take.ended = "diskError".to_owned();
    take.reason = Some("the disk is full.".to_owned());
    take.seconds = 4.0;
    take.tracks = vec![
        fixtures::track("input", "Kick"),
        fixtures::track("input", "Snare"),
    ];
    take.dropped_frames = 256;
    assert_eq!(
        format_stopped(&take),
        "\"Show\" stopped by a disk error: the disk is full: 0:04 on 2 tracks. 256 frames the \
         disk could not keep up with are missing."
    );
    assert_eq!(format_unreadable(0), None);
    assert_eq!(
        format_unreadable(1).as_deref(),
        Some("1 take could not be read and is not listed.")
    );
    assert_eq!(
        format_unreadable(3).as_deref(),
        Some("3 takes could not be read and are not listed.")
    );
}

#[test]
fn every_way_a_take_ends_is_said_in_words() {
    assert_eq!(format_take_end("stopped", None), "stopped");
    assert_eq!(
        format_take_end("engineStopped", None),
        "ended when the engine stopped"
    );
    assert_eq!(
        format_take_end("diskError", Some("the disk is full")),
        "stopped by a disk error: the disk is full"
    );
    assert_eq!(
        format_take_end("diskError", None),
        "stopped by a disk error"
    );
    assert_eq!(format_take_end("recording", None), "recording now");
    assert_eq!(
        format_take_end("interrupted", None),
        "interrupted before it was finished; its files hold what was written"
    );
    assert_eq!(format_take_end("something new", None), "ended");
}

#[test]
fn the_console_keeps_the_recording_settings_and_takes_past_a_runs_end_the_estimate_not() {
    let settings = fixtures::settings();
    let status = status_with(|report| {
        report.status.recording = recording_now();
        report.status.last_take = Some(TakeSummary {
            name: "Earlier".to_owned(),
            ..TakeSummary::default()
        });
    });
    let mut state = fixtures::reduce([
        Action::Status(Box::new(status)),
        Action::RecordingSettings(Some(settings.clone())),
        Action::Takes(Box::new(fixtures::listing(vec![fixtures::take("Show")]))),
        Action::Estimate(Some(DiskEstimateDto {
            folder: ROOT.to_owned(),
            bytes_per_second: Some(144_000),
            free_bytes: None,
            minutes_left: None,
            problem: None,
        })),
        Action::RecordingOutcome(Some(RecordingOutcome {
            action: RecordingAction::Start,
            ok: true,
            message: "Recording.".to_owned(),
        })),
    ]);
    assert!(state.is_recording_now());
    assert_eq!(state.recording_status().seconds, 754.0);
    assert_eq!(
        state.last_take().map(|take| take.name.as_str()),
        Some("Earlier")
    );

    state.reduce(&Action::TakesFailed(
        "the folder could not be read".to_owned(),
    ));
    assert_eq!(
        state.takes.list.as_ref().map(Vec::len),
        Some(1),
        "the last list known stays"
    );
    assert_eq!(
        state.takes.error.as_deref(),
        Some("the folder could not be read")
    );

    state.reduce(&Action::Engine {
        phase: Phase::Idle,
        error: None,
    });
    assert_eq!(state.recording_settings, Some(settings));
    assert_eq!(state.takes.list.as_ref().map(Vec::len), Some(1));
    assert_eq!(state.estimate, None);
    assert_eq!(state.recording_status().state, "idle");
    assert!(!state.is_recording_now());
    assert!(state.last_take().is_none());
}
