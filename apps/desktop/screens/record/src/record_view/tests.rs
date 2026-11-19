//! `record-view.test.js`: the track rows, the takes table, the REC chip, the disk estimate and its
//! warnings, the transport, the messages, the settings and the bounded peak history.

use super::*;
use crate::fixtures::{self, ROOT, recording_now, running, settings, status_with};
use crate::recording::settings_request;
use bandear_desktop_console::chips::ChipKind;
use bandear_desktop_console::router::Section;
use bandear_desktop_console::state::{Action, RecordingAction, RecordingOutcome};
use bandear_desktop_console::types::dto::{DiskEstimateDto, RecordingSettingsDto};
use bandear_desktop_console::wire::{Recording, TakeMarker, TakeSummary};

fn sample() -> ConsoleState {
    running(fixtures::status_with(|_| {}), settings())
}

fn recording_state(recording: Recording) -> ConsoleState {
    running(
        status_with(|report| report.status.recording = recording),
        settings(),
    )
}

fn with_settings(change: impl FnOnce(&mut RecordingSettingsDto)) -> ConsoleState {
    let mut kept = settings();
    change(&mut kept);
    running(status_with(|_| {}), kept)
}

fn at_start(at_start: bool) -> TransportLocal {
    TransportLocal {
        at_start,
        chosen_take: None,
    }
}

#[test]
fn each_recorded_unit_of_the_device_is_one_track_row_a_linked_pair_one_stereo_row() {
    let status = status_with(|report| {
        report.status.channels[2].link = true;
        report.status.channels[5].name = String::new();
    });
    let mut kept = settings();
    kept.armed = vec![4, 5];
    let rows = track_rows(&running(status, kept));

    let summary: Vec<_> = rows
        .iter()
        .map(|row| {
            (
                row.key.as_str(),
                row.inputs.clone(),
                row.stereo,
                row.name.as_str(),
                row.source.as_str(),
                row.armed,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("in-1", vec![1], false, "Kick", "IN 1", false),
            ("in-2", vec![2], false, "Snare", "IN 2", false),
            (
                "in-3",
                vec![3, 4],
                true,
                "Overheads",
                "IN 3\u{2013}4 \u{b7} stereo",
                true
            ),
            ("in-5", vec![5], false, "Lead vox", "IN 5", true),
            ("in-6", vec![6], false, "CH 6", "IN 6", false),
        ]
    );
    assert_eq!(
        rows[2].sides,
        [2, 3],
        "a row's sides are the 0-based inputs its meter reads"
    );
}

#[test]
fn an_unnamed_pair_is_called_by_both_its_inputs_and_a_hidden_input_still_records() {
    let status = status_with(|report| {
        report.status.channels[0].name = String::new();
        report.status.channels[0].link = true;
        report.status.channels[5].hidden = true;
    });
    let rows = track_rows(&running(status, settings()));

    assert_eq!(rows[0].name, "CH 1-2");
    assert_eq!(
        rows.last().map(|row| row.source.as_str()),
        Some("IN 6 \u{b7} hidden")
    );
    assert_eq!(rows.len(), 5);
}

#[test]
fn there_are_no_track_rows_while_no_engine_runs_the_inputs_depend_on_the_device() {
    let state = fixtures::reduce([Action::RecordingSettings(Some(settings()))]);
    assert!(track_rows(&state).is_empty());
    assert!(
        shown_rows(&state).is_empty(),
        "nor the local mix without an input row"
    );
}

#[test]
fn arming_a_row_changes_the_armed_inputs_a_pair_as_one_and_the_local_row_the_local_mix() {
    let status = status_with(|report| report.status.channels[2].link = true);
    let state = running(status, settings());
    let rows = track_rows(&state);

    // The change is applied when its request is built, on the settings answered last.
    let armed_after_press = |row: &TrackRow, kept: &RecordingSettingsDto| {
        settings_request(Some(kept), &arm_change(&state, row, row.armed)).armed
    };
    let kept = state.recording_settings.clone().expect("settings");
    assert_eq!(armed_after_press(&rows[1], &kept), [1, 2, 4], "Snare armed");
    assert_eq!(armed_after_press(&rows[0], &kept), [4], "Kick disarmed");
    assert_eq!(
        armed_after_press(&rows[2], &kept),
        [1],
        "the pair disarmed whole"
    );
    let local = local_track_row(&state).expect("the local row");
    assert_eq!(
        arm_change(&state, &local, local.armed),
        SettingsChange {
            include_local: Some(true),
            ..SettingsChange::default()
        }
    );
    let later = RecordingSettingsDto {
        armed: vec![1, 4, 6],
        ..kept
    };
    assert_eq!(
        armed_after_press(&rows[1], &later),
        [1, 2, 4, 6],
        "a press made before an earlier one was answered keeps the earlier one"
    );
}

#[test]
fn the_local_mix_row_records_when_the_settings_say_so() {
    let off = local_track_row(&sample()).expect("a row");
    assert_eq!(off.key, "local");
    assert!(off.local);
    assert_eq!(off.name, "Local mix");
    assert_eq!(off.source, "This computer's mix \u{b7} stereo");
    assert!(!off.armed);

    let on = with_settings(|kept| kept.include_local = true);
    assert!(local_track_row(&on).expect("a row").armed);
    assert_eq!(
        local_track_row(&ConsoleState::new()),
        None,
        "no row while the settings are unknown"
    );
    assert_eq!(
        shown_rows(&sample()).last().map(|row| row.key.as_str()),
        Some("local"),
        "after the inputs"
    );
}

#[test]
fn a_takes_start_reads_in_the_windows_local_time() {
    assert_eq!(
        format_take_started("2026-10-16T19:04:05Z", 120),
        "2026-10-16 21:04"
    );
    assert_eq!(
        format_take_started("2026-10-16T23:30:00Z", 60),
        "2026-10-17 00:30"
    );
    assert_eq!(
        format_take_started("2026-10-16T01:00:00Z", -300),
        "2026-10-15 20:00"
    );
    assert_eq!(format_take_started("not a date", 0), "Unknown");
}

#[test]
fn a_take_row_says_its_name_start_length_tracks_markers_and_how_it_ended() {
    let mut take = fixtures::take("Soundcheck");
    take.folder = format!("{ROOT}\\2026-10-16_210405_Soundcheck");
    take.started = "2026-10-16T19:04:05Z".to_owned();
    take.seconds = 3725.4;
    take.tracks = vec![
        fixtures::track("input", "Kick"),
        fixtures::track("pair", "Overheads"),
        fixtures::track("local", "Local mix"),
    ];
    take.markers = vec![fixtures::marker(30.0, "Song 1")];

    assert_eq!(
        take_row(&take, 120),
        TakeRow {
            key: take.folder.clone(),
            folder: take.folder.clone(),
            name: "Soundcheck".to_owned(),
            started: "2026-10-16 21:04".to_owned(),
            length: "1:02:05".to_owned(),
            tracks: "2 tracks + local mix".to_owned(),
            markers: "1 marker".to_owned(),
            end: TakeEnd {
                text: "Stopped",
                kind: EndKind::Ok,
                detail: "stopped".to_owned(),
            },
            dropped: None,
            recording: false,
        }
    );

    let mut local = take.clone();
    local.tracks = vec![fixtures::track("local", "Local mix")];
    local.markers = Vec::new();
    let local = take_row(&local, 0);
    assert_eq!(local.tracks, "Local mix only");
    assert_eq!(local.markers, "No markers");
    let mut lossy = take.clone();
    lossy.dropped_frames = 480;
    let lossy = take_row(&lossy, 0);
    assert_eq!(lossy.dropped.as_deref(), Some("480 frames missing"));
    assert_eq!(
        lossy.detail(),
        "2 tracks + local mix \u{b7} 1 marker \u{b7} 480 frames missing"
    );
}

#[test]
fn every_way_a_take_ends_has_a_short_word_and_its_whole_sentence() {
    assert_eq!(
        take_end_label("stopped", None),
        TakeEnd {
            text: "Stopped",
            kind: EndKind::Ok,
            detail: "stopped".to_owned(),
        }
    );
    assert_eq!(
        take_end_label("engineStopped", None),
        TakeEnd {
            text: "Engine stopped",
            kind: EndKind::Warn,
            detail: "ended when the engine stopped".to_owned(),
        }
    );
    assert_eq!(
        take_end_label("diskError", Some("the disk is full")),
        TakeEnd {
            text: "Disk error",
            kind: EndKind::Error,
            detail: "stopped by a disk error: the disk is full".to_owned(),
        }
    );
    let interrupted = take_end_label("interrupted", None);
    assert_eq!(
        (interrupted.text, interrupted.kind),
        ("Interrupted", EndKind::Warn)
    );
    assert_eq!(
        take_end_label("recording", None),
        TakeEnd {
            text: "Recording",
            kind: EndKind::Live,
            detail: "recording now".to_owned(),
        }
    );
    let other = take_end_label("other", None);
    assert_eq!((other.text, other.kind), ("Ended", EndKind::Ok));
}

#[test]
fn the_take_being_recorded_shows_the_length_the_status_reports_not_the_listed_one() {
    let mut listed = fixtures::take("Soundcheck");
    listed.folder = format!("{ROOT}\\2026-10-16_210405_Soundcheck");
    listed.seconds = 3.0;
    listed.ended = "recording".to_owned();
    let mut state = recording_state(recording_now());
    state.reduce(&Action::Takes(Box::new(fixtures::listing(vec![listed]))));

    let rows = take_rows(&state, 0);
    assert_eq!(rows[0].0.length, "12:34");
    assert!(rows[0].0.recording);
    assert!(
        take_rows(&ConsoleState::new(), 0).is_empty(),
        "nothing listed yet"
    );
}

#[test]
fn the_rec_chip_shows_while_a_take_records_with_its_time_and_tracks_and_leads_to_record() {
    assert_eq!(rec_chip(&sample()), None, "nothing while no take records");

    assert_eq!(
        rec_chip(&recording_state(recording_now())),
        Some(Chip {
            kind: ChipKind::Recording,
            word: "REC".to_owned(),
            time: "12:34".to_owned(),
            detail: "2 tracks".to_owned(),
            label: "Recording, 12 minutes 34 seconds, 2 tracks. Open Record".to_owned(),
            target: Section::Record,
        })
    );

    let stopping = rec_chip(&recording_state(Recording {
        state: "stopping".to_owned(),
        ..recording_now()
    }))
    .expect("a chip");
    assert_eq!(stopping.kind, ChipKind::Stopping);
    assert_eq!(stopping.word, "Finishing");
    assert_eq!(
        stopping.label,
        "Finishing the take's files after 12 minutes 34 seconds. Open Record"
    );

    let failed = rec_chip(&recording_state(Recording {
        state: "failed".to_owned(),
        reason: Some("the disk is full".to_owned()),
        ..recording_now()
    }))
    .expect("a chip");
    assert_eq!(failed.kind, ChipKind::Failed);
    assert_eq!(failed.word, "REC stopped");
    assert_eq!(failed.time, "12:34");
    assert_eq!(failed.detail, "2 tracks");
    assert_eq!(
        failed.label,
        "The recording stopped: the disk is full. Open Record"
    );
    assert_eq!(failed.target, Section::Record);
}

#[test]
fn a_duration_is_spoken_in_words() {
    assert_eq!(spoken_duration(0.0), "0 seconds");
    assert_eq!(spoken_duration(1.0), "1 second");
    assert_eq!(spoken_duration(754.0), "12 minutes 34 seconds");
    assert_eq!(spoken_duration(3600.0), "1 hour");
    assert_eq!(spoken_duration(3725.9), "1 hour 2 minutes 5 seconds");
}

#[test]
fn sizes_read_in_the_units_a_file_manager_uses() {
    assert_eq!(format_bytes(214_000_000_000), "214 GB");
    assert_eq!(format_bytes(1_340_000_000), "1.3 GB");
    assert_eq!(format_bytes(705_000_000), "705 MB");
    assert_eq!(format_bytes(2_500_000_000_000), "2.5 TB");
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(9_000_000_000), "9.0 GB");
}

fn estimate(
    free_bytes: Option<u64>,
    minutes_left: Option<u64>,
    problem: Option<&str>,
) -> DiskEstimateDto {
    DiskEstimateDto {
        folder: ROOT.to_owned(),
        bytes_per_second: problem.is_none().then_some(288_000),
        free_bytes,
        minutes_left,
        problem: problem.map(str::to_owned),
    }
}

#[test]
fn the_disk_estimate_reads_the_free_space_and_the_minutes_left_before_a_take() {
    let mut state = sample();
    state.reduce(&Action::Estimate(Some(estimate(
        Some(214_000_000_000),
        Some(12_384),
        None,
    ))));

    assert_eq!(
        disk_view(&state),
        DiskView {
            free: "214 GB free".to_owned(),
            left: Some("about 206 h left".to_owned()),
            problem: None,
            warning: None,
        }
    );
}

#[test]
fn while_a_take_records_the_disk_reads_what_the_recorder_reports() {
    let view = disk_view(&recording_state(Recording {
        free_bytes: Some(9_000_000_000),
        minutes_left: Some(95),
        ..recording_now()
    }));
    assert_eq!(view.free, "9.0 GB free");
    assert_eq!(view.left.as_deref(), Some("about 95 min left"));
}

#[test]
fn the_disk_warns_below_the_low_and_critical_thresholds_of_minutes_left() {
    assert_eq!(LOW_MINUTES, 30);
    assert_eq!(CRITICAL_MINUTES, 10);
    let at = |minutes_left| {
        disk_view(&recording_state(Recording {
            minutes_left,
            ..recording_now()
        }))
        .warning
    };

    assert_eq!(at(Some(30)), None, "30 minutes is enough");
    assert_eq!(
        at(Some(29)),
        Some(DiskWarning {
            level: WarningLevel::Low,
            text: "About 29 minutes of recording space are left on this disk.".to_owned(),
        })
    );
    assert_eq!(
        at(Some(10)).map(|warning| warning.level),
        Some(WarningLevel::Low)
    );
    assert_eq!(
        at(Some(9)),
        Some(DiskWarning {
            level: WarningLevel::Critical,
            text: "Only about 9 minutes of recording space are left: free some disk space or \
                   choose another folder before the take fills it."
                .to_owned(),
        })
    );
    assert!(
        at(Some(1))
            .expect("a warning")
            .text
            .starts_with("Only about 1 minute of recording space is left")
    );
    assert_eq!(
        at(Some(0)),
        Some(DiskWarning {
            level: WarningLevel::Critical,
            text: "No recording space is left: free some disk space or choose another folder \
                   before the take fills it."
                .to_owned(),
        })
    );
    assert_eq!(at(None), None, "an unknown rate warns of nothing");
}

#[test]
fn an_estimate_that_cannot_be_made_says_why_and_nothing_unknown_is_guessed() {
    let mut state = sample();
    state.reduce(&Action::Estimate(Some(estimate(
        Some(214_000_000_000),
        None,
        Some("arm at least one input, or record the local mix"),
    ))));

    assert_eq!(
        disk_view(&state),
        DiskView {
            free: "214 GB free".to_owned(),
            left: None,
            problem: Some("Arm at least one input, or record the local mix.".to_owned()),
            warning: None,
        }
    );
    assert_eq!(
        disk_view(&ConsoleState::new()),
        DiskView {
            free: "Free space unknown".to_owned(),
            left: None,
            problem: None,
            warning: None,
        }
    );
}

#[test]
fn the_peak_history_keeps_the_newest_levels_up_to_its_capacity_and_never_grows() {
    let mut history = PeakHistory::new(4);
    assert_eq!(history.capacity(), 4);
    assert_eq!(history.len(), 0);
    assert!(history.is_empty());

    for level in 1..=1000 {
        history.push(level as f32);
        assert!(history.len() <= 4);
    }
    assert_eq!(history.len(), 4);
    let seen: Vec<_> = history.iter().collect();
    assert_eq!(seen, [997.0, 998.0, 999.0, 1000.0]);
    assert_eq!(history.at(0), 997.0, "the oldest first");
    assert_eq!(history.at(3), 1000.0);

    history.clear();
    assert_eq!(history.len(), 0);
    history.push(7.0);
    assert_eq!(history.at(0), 7.0);
}

#[test]
fn the_peak_history_keeps_what_it_is_given_in_order_until_it_is_full() {
    let mut history = PeakHistory::new(3);
    history.push(10.0);
    history.push(20.0);
    assert_eq!(history.iter().collect::<Vec<_>>(), [10.0, 20.0]);
    assert_eq!(PeakHistory::new(0).capacity(), 1, "at least one level");
}

#[test]
fn a_takes_name_is_its_folders_name_without_the_time_stamp() {
    assert_eq!(
        take_name_of(Some("D:\\Recordings\\2026-10-16_210405_Soundcheck")),
        "Soundcheck"
    );
    assert_eq!(
        take_name_of(Some("D:\\Recordings\\2026-10-16_210405_Take (2)")),
        "Take (2)"
    );
    assert_eq!(
        take_name_of(Some("/home/demo/Recordings/2026-10-16_210405_Show")),
        "Show"
    );
    assert_eq!(take_name_of(Some("/home/demo/Recordings/Show")), "Show");
    assert_eq!(take_name_of(None), "");
}

#[test]
fn the_transport_records_while_an_engine_runs_and_stops_and_marks_while_a_take_records() {
    let idle = transport_view(&sample(), &at_start(false));
    assert!(idle.can_record);
    assert!(!idle.can_stop);
    assert!(!idle.can_mark);
    assert!(!idle.recording);
    assert_eq!(idle.position, "0:00");
    assert_eq!(idle.state_word, "Ready to record");
    assert_eq!(idle.armed, "2 tracks");
    assert_eq!(idle.format, "WAV \u{b7} 48 kHz \u{b7} 24-bit");
    assert_eq!(idle.stop_label, "Stop recording");

    let live = transport_view(&recording_state(recording_now()), &at_start(false));
    assert!(!live.can_record);
    assert!(live.can_stop);
    assert!(live.can_mark);
    assert!(!live.can_rewind);
    assert!(live.recording);
    assert_eq!(live.position, "12:34");
    assert_eq!(live.state_word, "Recording \u{b7} Soundcheck");
    assert_eq!(live.state_kind, "recording");
    assert_eq!(
        live.markers,
        [MarkerRow {
            key: "0".to_owned(),
            time: "1:05".to_owned(),
            name: "Song 1".to_owned(),
        }]
    );
    assert_eq!(live.markers_of.as_deref(), Some("Soundcheck"));

    let stopping = transport_view(
        &recording_state(Recording {
            state: "stopping".to_owned(),
            ..recording_now()
        }),
        &at_start(false),
    );
    assert_eq!(stopping.state_word, "Finishing the take's files\u{2026}");
    assert!(stopping.can_stop, "Stop can be pressed again");
    assert!(!stopping.can_mark);
    let failed = transport_view(
        &recording_state(Recording {
            state: "failed".to_owned(),
            ..recording_now()
        }),
        &at_start(false),
    );
    assert_eq!(failed.state_word, "The take stopped on an error");
    assert!(failed.can_stop, "a failed take is collected by Stop");
    assert!(
        failed.can_record,
        "only a take recording or stopping holds Record"
    );

    let off = transport_view(
        &fixtures::reduce([Action::RecordingSettings(Some(settings()))]),
        &at_start(false),
    );
    assert!(!off.can_record);
    assert_eq!(off.state_word, "Start the engine to record");
}

#[test]
fn after_a_take_the_position_shows_its_length_until_back_to_start() {
    let take = TakeSummary {
        folder: format!("{ROOT}\\2026-10-16_210405_Soundcheck"),
        name: "Soundcheck".to_owned(),
        seconds: 95.0,
        markers: vec![TakeMarker {
            frame: 30 * 48_000,
            seconds: 30.0,
            name: "Chorus".to_owned(),
        }],
        ended: "stopped".to_owned(),
        ..TakeSummary::default()
    };
    let state = running(
        status_with(|report| report.status.last_take = Some(take)),
        settings(),
    );

    let after = transport_view(&state, &at_start(false));
    assert_eq!(after.position, "1:35");
    assert!(after.can_rewind);
    assert_eq!(
        after.markers,
        [MarkerRow {
            key: "0".to_owned(),
            time: "0:30".to_owned(),
            name: "Chorus".to_owned(),
        }]
    );
    assert_eq!(after.markers_of.as_deref(), Some("Soundcheck"));

    let rewound = transport_view(&state, &at_start(true));
    assert_eq!(rewound.position, "0:00");
    assert!(!rewound.can_rewind);
    assert!(rewound.markers.is_empty(), "the last take's markers hide");
}

#[test]
fn a_float_take_and_the_local_mix_read_in_the_transports_summary() {
    let view = transport_view(
        &with_settings(|kept| {
            kept.format = "float32";
            kept.include_local = true;
            kept.armed = Vec::new();
        }),
        &at_start(false),
    );
    assert_eq!(view.format, "WAV \u{b7} 48 kHz \u{b7} 32-bit float");
    assert_eq!(view.armed, "Local mix only");
    let both = transport_view(
        &with_settings(|kept| kept.include_local = true),
        &at_start(false),
    );
    assert_eq!(both.armed, "2 tracks + local mix");
    let none = transport_view(
        &with_settings(|kept| kept.armed = Vec::new()),
        &at_start(false),
    );
    assert_eq!(none.armed, "Nothing armed");
}

#[test]
fn the_screen_says_each_commands_outcome_a_failed_take_and_a_takes_listing_that_failed() {
    let mut said = recording_state(Recording {
        state: "failed".to_owned(),
        reason: Some("the disk is full".to_owned()),
        ..recording_now()
    });
    said.reduce(&Action::RecordingOutcome(Some(RecordingOutcome {
        action: RecordingAction::Start,
        ok: false,
        message: "The recording did not start: nothing is armed.".to_owned(),
    })));
    said.reduce(&Action::TakesFailed("the folder is gone".to_owned()));

    assert_eq!(
        record_messages(&said),
        Messages {
            outcome: Some(Said {
                ok: false,
                text: "The recording did not start: nothing is armed.".to_owned(),
            }),
            failure: Some("The take stopped recording: the disk is full.".to_owned()),
            takes: Some("The takes could not be listed: the folder is gone.".to_owned()),
            locked: None,
        }
    );

    let recording = record_messages(&recording_state(recording_now()));
    assert_eq!(recording.outcome, None);
    assert_eq!(recording.failure, None);
    assert_eq!(
        recording.locked,
        Some(
            "Arming and the settings stay as they are while a take records; they apply from the \
             next take."
        )
    );
}

#[test]
fn the_settings_show_the_folder_its_default_and_notice_the_format_and_the_local_mix() {
    let view = settings_view(&with_settings(|kept| {
        kept.notice = Some("The Music folder\u{2026}".to_owned());
    }));
    assert_eq!(
        view,
        SettingsView {
            known: true,
            folder: ROOT.to_owned(),
            default_folder: fixtures::DEFAULT_FOLDER.to_owned(),
            is_default: false,
            notice: Some("The Music folder\u{2026}".to_owned()),
            format: "int24".to_owned(),
            formats: vec![
                ("int24", "WAV 24-bit \u{b7} one file per track"),
                ("float32", "WAV 32-bit float \u{b7} one file per track"),
            ],
            include_local: false,
            locked: false,
        }
    );
    let unknown = settings_view(&ConsoleState::new());
    assert!(!unknown.known);
    assert!(unknown.is_default, "the default until read");
    assert!(settings_view(&recording_state(recording_now())).locked);
}
