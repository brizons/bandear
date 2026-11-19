//! What the screen's tests build their states from: the desk's settings, takes and soundchecks,
//! and the sample engine's status with changes on top (`fake-bridge.js`'s `sampleStatus`).

use bandear_desktop_console::state::{Action, ConsoleState};
use bandear_desktop_console::testing::sample_status;
use bandear_desktop_console::types::dto::{
    RecordingDto, RecordingSettingsDto, TakeMarkerDto, TakeSummaryDto, TakeTrackDto, TakesDto,
};
use bandear_desktop_console::wire::{Recording, Soundcheck, StatusReport, TakeMarker};

/// The recordings folder of the tests.
pub const ROOT: &str = "D:\\Recordings";

/// The desk's default folder in the tests.
pub const DEFAULT_FOLDER: &str = "C:\\Users\\Demo\\Music\\BandEar\\Recordings";

/// The settings the record view tests start from: CH 1 and CH 4 armed in `D:\Recordings`.
pub fn settings() -> RecordingSettingsDto {
    RecordingSettingsDto {
        folder: ROOT.to_owned(),
        default_folder: DEFAULT_FOLDER.to_owned(),
        folder_is_default: false,
        format: "int24",
        include_local: false,
        armed: vec![1, 4],
        notice: None,
    }
}

/// A track of a take.
pub fn track(kind: &str, name: &str) -> TakeTrackDto {
    TakeTrackDto {
        file: format!("{name}.wav"),
        kind: kind.to_owned(),
        channels: Vec::new(),
        name: name.to_owned(),
        instrument: None,
        frames: 0,
    }
}

/// A take called `name`, as `list_takes` lists it: one input track, 100 seconds, stopped.
pub fn take(name: &str) -> TakeSummaryDto {
    TakeSummaryDto {
        folder: format!("{ROOT}\\2026-10-16_2100{}0_{name}", name.len()),
        name: name.to_owned(),
        started: "2026-10-16T19:00:00Z".to_owned(),
        sample_rate: 48_000,
        format: "int24".to_owned(),
        device: "Demo interface".to_owned(),
        tracks: vec![track("input", "Kick")],
        frames: 4_800_000,
        seconds: 100.0,
        markers: Vec::new(),
        dropped_frames: 0,
        ended: "stopped".to_owned(),
        reason: None,
    }
}

/// A marker at `seconds` called `name`.
pub fn marker(seconds: f64, name: &str) -> TakeMarkerDto {
    TakeMarkerDto {
        frame: (seconds * 48_000.0) as u64,
        seconds,
        name: name.to_owned(),
    }
}

/// The takes listing of `takes` in [ROOT].
pub fn listing(takes: Vec<TakeSummaryDto>) -> TakesDto {
    TakesDto {
        folder: ROOT.to_owned(),
        takes,
        unreadable: 0,
        missing: false,
    }
}

/// The recorder while the take "Soundcheck" records: 12:34 in, two tracks, one marker.
pub fn recording_now() -> Recording {
    Recording {
        state: "recording".to_owned(),
        reason: None,
        take_folder: Some(format!("{ROOT}\\2026-10-16_210405_Soundcheck")),
        frames: 754 * 48_000,
        seconds: 754.0,
        dropped_frames: 0,
        bytes_written: 0,
        tracks: vec!["Kick".to_owned(), "Bass".to_owned()],
        markers: vec![TakeMarker {
            frame: 65 * 48_000,
            seconds: 65.0,
            name: "Song 1".to_owned(),
        }],
        free_bytes: Some(214_000_000_000),
        minutes_left: Some(2040),
    }
}

/// The recorder `start_recording` answers with, for tests of its words.
pub fn recorder_dto(tracks: &[&str]) -> RecordingDto {
    RecordingDto {
        state: "recording",
        reason: None,
        take_folder: Some(format!("{ROOT}\\2026-10-16_210405_Show")),
        frames: 0,
        seconds: 0.0,
        dropped_frames: 0,
        bytes_written: 0,
        tracks: tracks.iter().map(|name| (*name).to_owned()).collect(),
        markers: Vec::new(),
        free_bytes: None,
        minutes_left: None,
    }
}

/// The soundcheck while "Show" plays, 12 seconds into its 100.
pub fn soundcheck_on() -> Soundcheck {
    Soundcheck {
        state: "playing".to_owned(),
        reason: None,
        take_folder: Some(take("Show").folder),
        take_name: Some("Show".to_owned()),
        position_frames: 12 * 48_000,
        position_seconds: 12.0,
        length_frames: 100 * 48_000,
        length_seconds: 100.0,
        looping: false,
        underrun_frames: 0,
        clock: "timer".to_owned(),
    }
}

/// The sample engine's status, run 1, with `change` made to it.
pub fn status_with(change: impl FnOnce(&mut StatusReport)) -> StatusReport {
    let mut report = sample_status();
    change(&mut report);
    report
}

/// The console after `actions`, in order.
pub fn reduce(actions: impl IntoIterator<Item = Action>) -> ConsoleState {
    let mut state = ConsoleState::new();
    for action in actions {
        state.reduce(&action);
    }
    state
}

/// A running engine's state with `status` and the desk's `settings` read.
pub fn running(status: StatusReport, settings: RecordingSettingsDto) -> ConsoleState {
    reduce([
        Action::Status(Box::new(status)),
        Action::RecordingSettings(Some(settings)),
    ])
}
