//! Opens the screen alone over the simulated desk, with its engine running on the sample
//! interface and musicians: no engine starts, no input opens and nothing goes on the network.
//!
//! The desk answers the recording settings, the takes and the disk estimate from a few takes it
//! keeps. `RECORD_PREVIEW` picks what the engine is doing: `takes` (the default: idle, with takes),
//! `recording` (a take records, so the lanes draw) or `soundcheck` (a take plays in place of the
//! inputs).

use std::cell::RefCell;
use std::rc::Rc;

use bandear_desktop_console::command::{Answer, Command};
use bandear_desktop_console::testing::FakeDesk;
use bandear_desktop_console::types::dto::{
    DiskEstimateDto, RecordingSettingsDto, TakeMarkerDto, TakeSummaryDto, TakeTrackDto, TakesDto,
};
use bandear_desktop_console::wire::{Recording, Soundcheck, TakeMarker, TakeSummary};

const FOLDER: &str = "C:\\Users\\Demo\\Music\\BandEar\\Recordings";

fn track(kind: &str, name: &str) -> TakeTrackDto {
    TakeTrackDto {
        file: format!("{name}.wav"),
        kind: kind.to_owned(),
        channels: Vec::new(),
        name: name.to_owned(),
        instrument: None,
        frames: 0,
    }
}

fn take(
    stamp: &str,
    name: &str,
    seconds: f64,
    ended: &str,
    reason: Option<&str>,
) -> TakeSummaryDto {
    TakeSummaryDto {
        folder: format!("{FOLDER}\\{stamp}_{name}"),
        name: name.to_owned(),
        started: "2026-10-16T19:04:05Z".to_owned(),
        sample_rate: 48_000,
        format: "int24".to_owned(),
        device: "Demo interface".to_owned(),
        tracks: vec![
            track("input", "Kick"),
            track("input", "Snare"),
            track("local", "Local mix"),
        ],
        frames: (seconds * 48_000.0) as u64,
        seconds,
        markers: vec![TakeMarkerDto {
            frame: 30 * 48_000,
            seconds: 30.0,
            name: "Chorus".to_owned(),
        }],
        dropped_frames: if ended == "diskError" { 480 } else { 0 },
        ended: ended.to_owned(),
        reason: reason.map(str::to_owned),
    }
}

fn main() {
    let scenario = std::env::var("RECORD_PREVIEW").unwrap_or_else(|_| "takes".to_owned());
    let mut desk = FakeDesk::running();
    let takes = vec![
        take("2026-10-16_210405", "Show", 3725.0, "stopped", None),
        take("2026-10-15_200000", "Rehearsal", 95.0, "interrupted", None),
        take(
            "2026-10-14_190000",
            "Line check",
            62.0,
            "diskError",
            Some("the disk is full"),
        ),
    ];
    let settings = Rc::new(RefCell::new(RecordingSettingsDto {
        folder: FOLDER.to_owned(),
        default_folder: FOLDER.to_owned(),
        folder_is_default: true,
        format: "int24",
        include_local: true,
        armed: vec![1, 2, 5],
        notice: None,
    }));
    let status = &mut desk.report.status;
    status.last_take = Some(TakeSummary {
        folder: takes[0].folder.clone(),
        name: "Show".to_owned(),
        seconds: 3725.0,
        markers: vec![TakeMarker {
            frame: 30 * 48_000,
            seconds: 30.0,
            name: "Chorus".to_owned(),
        }],
        ended: "stopped".to_owned(),
        ..TakeSummary::default()
    });
    match scenario.as_str() {
        "recording" => {
            status.recording = Recording {
                state: "recording".to_owned(),
                take_folder: Some(format!("{FOLDER}\\2026-10-16_220000_Encore")),
                frames: 754 * 48_000,
                seconds: 754.0,
                tracks: vec![
                    "Kick".to_owned(),
                    "Snare".to_owned(),
                    "Local mix".to_owned(),
                ],
                markers: vec![TakeMarker {
                    frame: 65 * 48_000,
                    seconds: 65.0,
                    name: "Song 1".to_owned(),
                }],
                free_bytes: Some(9_000_000_000),
                minutes_left: Some(25),
                ..Recording::default()
            };
        }
        "soundcheck" => {
            status.soundcheck = Soundcheck {
                state: "playing".to_owned(),
                take_folder: Some(takes[1].folder.clone()),
                take_name: Some("Rehearsal".to_owned()),
                position_frames: 12 * 48_000,
                position_seconds: 12.0,
                length_frames: 95 * 48_000,
                length_seconds: 95.0,
                looping: true,
                clock: "output".to_owned(),
                ..Soundcheck::default()
            };
        }
        _ => {}
    }

    let kept = Rc::clone(&settings);
    desk.script("recording_settings", move |_, _| {
        Ok(Answer::RecordingSettings(kept.borrow().clone()))
    });
    let kept = Rc::clone(&settings);
    desk.script("set_recording_settings", move |_, command| {
        let Command::SetRecordingSettings { settings: request } = command else {
            return Err("not a settings change".to_owned());
        };
        let mut settings = kept.borrow_mut();
        settings.include_local = request.include_local;
        settings.armed = request.armed.iter().map(|&input| input as u16).collect();
        settings.format = if request.format.as_deref() == Some("float32") {
            "float32"
        } else {
            "int24"
        };
        Ok(Answer::RecordingSettings(settings.clone()))
    });
    desk.script("record_estimate", |_, _| {
        Ok(Answer::Estimate(DiskEstimateDto {
            folder: FOLDER.to_owned(),
            bytes_per_second: Some(864_000),
            free_bytes: Some(214_000_000_000),
            minutes_left: Some(4128),
            problem: None,
        }))
    });
    desk.script("list_takes", move |_, _| {
        Ok(Answer::Takes(TakesDto {
            folder: FOLDER.to_owned(),
            takes: takes.clone(),
            unreadable: 1,
            missing: false,
        }))
    });
    desk.script("open_take_folder", |_, _| Ok(Answer::Done));
    bandear_desktop_preview::run::<bandear_desktop_record::view::RecordScreen>(desk);
}
