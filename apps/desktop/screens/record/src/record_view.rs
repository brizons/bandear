//! What the Record screen shows, worked out from the console state (`record-view.js`). Pure
//! functions, no window: the view only draws these plain values.
//!
//! - Track rows: one per recorded unit of the running device, as the engine lays out the tracks
//!   (a linked pair is one stereo track, either side arms it; hidden inputs record too), then the
//!   stereo local mix, which the settings' `include_local` arms.
//! - Takes: one row per take the recordings folder holds, newest first; the take being recorded
//!   shows the length the status reports.
//! - The REC chip: shown on every screen while a take records, is being finished or has failed.
//! - The disk: free space and minutes left, the recorder's while a take records, else the
//!   estimate for the kept settings, with a warning below [LOW_MINUTES] and a stronger one below
//!   [CRITICAL_MINUTES].
//! - The waveform lanes are drawn from a bounded peak history per track ([PeakHistory]), fed by
//!   the meters while a take records: no engine call, nothing that grows.

use bandear_desktop_console::chips::{Chip, ChipKind};
use bandear_desktop_console::format::{SEPARATOR, sentence_with_reason, to_fixed};
use bandear_desktop_console::router::Section;
use bandear_desktop_console::state::ConsoleState;
use bandear_desktop_console::types::dto::TakeSummaryDto;
use bandear_desktop_console::wire::{Channel, Meters, TakeMarker};

pub use crate::peaks::PeakHistory;
use crate::recording::{
    Arm, RECORD_FORMATS, SettingsChange, format_duration, format_take_end, is_armed, plural,
    record_units,
};
use crate::soundcheck::{RECORDING_BLOCKS_SOUNDCHECK, SOUNDCHECK_BLOCKS_RECORDING};
use crate::soundcheck_view::{Play, RowSoundcheck, soundcheck_play, take_soundcheck};

/// Below this many minutes of recording space the disk is said to be low.
pub const LOW_MINUTES: u64 = 30;

/// Below this many minutes the warning says to act before the take fills the disk.
pub const CRITICAL_MINUTES: u64 = 10;

/// Why arming and the settings cannot change during a take.
pub const LOCKED_WHILE_RECORDING: &str =
    "Arming and the settings stay as they are while a take records; they apply from the next take.";

/// The inputs of the running device as the engine records them, or none while no engine runs.
pub fn device_channels(state: &ConsoleState) -> &[Channel] {
    match &state.engine.status {
        Some(status) => &status.channels[..status.channels.len().min(status.source_channels)],
        None => &[],
    }
}

/// Whether each input of the running device is linked to the next.
pub fn device_links(state: &ConsoleState) -> Vec<bool> {
    device_channels(state)
        .iter()
        .map(|channel| channel.link)
        .collect()
}

/// The armed inputs the desk's settings keep, 1-based.
fn armed_inputs(state: &ConsoleState) -> Vec<u32> {
    state
        .recording_settings
        .as_ref()
        .map(|settings| {
            settings
                .armed
                .iter()
                .map(|&input| u32::from(input))
                .collect()
        })
        .unwrap_or_default()
}

/// One row of the tracks: a recorded unit of the device, or the local mix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackRow {
    /// `in-<first input>`, or `local`.
    pub key: String,
    /// The first input, 1-based (what arming takes); 0 for the local mix.
    pub input: u32,
    /// The inputs, 1-based.
    pub inputs: Vec<u32>,
    /// The inputs, 0-based: what the meters are indexed by.
    pub sides: Vec<usize>,
    pub stereo: bool,
    pub name: String,
    pub source: String,
    pub armed: bool,
    pub local: bool,
}

/// One row per recorded unit of the running device, left to right, called as the engine names
/// its track: the input's name, else "CH 3" or "CH 3-4" for a pair.
pub fn track_rows(state: &ConsoleState) -> Vec<TrackRow> {
    let channels = device_channels(state);
    let links = device_links(state);
    let armed = armed_inputs(state);
    record_units(&links)
        .into_iter()
        .map(|sides| {
            let inputs: Vec<u32> = sides.iter().map(|&side| side as u32 + 1).collect();
            let stereo = inputs.len() == 2;
            let numbers = if stereo {
                format!("{}-{}", inputs[0], inputs[1])
            } else {
                inputs[0].to_string()
            };
            let named = channels[sides[0]].name.trim();
            let name = if named.is_empty() {
                format!("CH {numbers}")
            } else {
                named.to_owned()
            };
            let mut source = if stereo {
                format!("IN {}\u{2013}{}{SEPARATOR}stereo", inputs[0], inputs[1])
            } else {
                format!("IN {}", inputs[0])
            };
            if sides.iter().all(|&side| channels[side].hidden) {
                source.push_str(SEPARATOR);
                source.push_str("hidden");
            }
            TrackRow {
                key: format!("in-{}", inputs[0]),
                input: inputs[0],
                armed: is_armed(&armed, inputs[0], &links),
                inputs,
                sides,
                stereo,
                name,
                source,
                local: false,
            }
        })
        .collect()
}

/// A row's meter level from `meters`: the louder of an input row's sides, or of the local mix's
/// two sides.
pub fn track_level(row: &TrackRow, meters: &Meters) -> f64 {
    if row.local {
        return f64::from(meters.local.left.max(meters.local.right));
    }
    row.sides
        .iter()
        .map(|&side| meters.channels.get(side).copied().unwrap_or(0))
        .max()
        .map_or(0.0, f64::from)
}

/// The stereo local mix's row, armed by the settings' `include_local`; `None` while the settings
/// are unknown.
pub fn local_track_row(state: &ConsoleState) -> Option<TrackRow> {
    let settings = state.recording_settings.as_ref()?;
    Some(TrackRow {
        key: "local".to_owned(),
        input: 0,
        inputs: Vec::new(),
        sides: Vec::new(),
        stereo: true,
        name: "Local mix".to_owned(),
        source: format!("This computer's mix{SEPARATOR}stereo"),
        armed: settings.include_local,
        local: true,
    })
}

/// The rows the screen shows: the device's, then the local mix's when there is at least one.
pub fn shown_rows(state: &ConsoleState) -> Vec<TrackRow> {
    let mut rows = track_rows(state);
    if !rows.is_empty()
        && let Some(local) = local_track_row(state)
    {
        rows.push(local);
    }
    rows
}

/// The settings change pressing a row's arm button asks for: the row armed or disarmed (a pair as
/// one), or the local mix on or off. `armed_now` is what the press toggles: the row as shown, or
/// what a press still on its way asked.
pub fn arm_change(state: &ConsoleState, row: &TrackRow, armed_now: bool) -> SettingsChange {
    if row.local {
        return SettingsChange {
            include_local: Some(!armed_now),
            ..SettingsChange::default()
        };
    }
    SettingsChange {
        arm: Some(Arm {
            input: row.input,
            on: !armed_now,
            links: device_links(state),
        }),
        ..SettingsChange::default()
    }
}

/// When a take started (an RFC 3339 time) in the window's local time, `offset_minutes` east of
/// UTC: "2026-10-16 21:04", or "Unknown".
pub fn format_take_started(started: &str, offset_minutes: i32) -> String {
    let Ok(time) = chrono::DateTime::parse_from_rfc3339(started.trim()) else {
        return "Unknown".to_owned();
    };
    let local = time.naive_utc() + chrono::Duration::minutes(i64::from(offset_minutes));
    local.format("%Y-%m-%d %H:%M").to_string()
}

/// How a take's end looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndKind {
    Ok,
    Warn,
    Error,
    Live,
}

/// How a take ended, short and whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TakeEnd {
    pub text: &'static str,
    pub kind: EndKind,
    /// The sentence [crate::recording::format_take_end] says.
    pub detail: String,
}

/// How a take ended (`ended`, with a disk error's `reason`), short and whole.
pub fn take_end_label(ended: &str, reason: Option<&str>) -> TakeEnd {
    let (text, kind) = match ended {
        "stopped" => ("Stopped", EndKind::Ok),
        "engineStopped" => ("Engine stopped", EndKind::Warn),
        "diskError" => ("Disk error", EndKind::Error),
        "interrupted" => ("Interrupted", EndKind::Warn),
        "recording" => ("Recording", EndKind::Live),
        _ => ("Ended", EndKind::Ok),
    };
    TakeEnd {
        text,
        kind,
        detail: format_take_end(ended, reason),
    }
}

/// "3 tracks + local mix", "1 track", "Local mix only", "Nothing armed".
fn tracks_summary(input_tracks: usize, local: bool) -> String {
    if input_tracks == 0 {
        return if local {
            "Local mix only"
        } else {
            "Nothing armed"
        }
        .to_owned();
    }
    let local = if local { " + local mix" } else { "" };
    format!("{}{local}", plural(input_tracks as u64, "track"))
}

/// One row of the takes table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TakeRow {
    pub key: String,
    pub folder: String,
    pub name: String,
    pub started: String,
    pub length: String,
    pub tracks: String,
    pub markers: String,
    pub end: TakeEnd,
    pub dropped: Option<String>,
    pub recording: bool,
}

impl TakeRow {
    /// The detail line under the name: the tracks, the markers and the frames missing.
    pub fn detail(&self) -> String {
        [
            Some(&self.tracks),
            Some(&self.markers),
            self.dropped.as_ref(),
        ]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(SEPARATOR)
    }
}

/// One row of the takes table for `take`, its start in the window's local time.
pub fn take_row(take: &TakeSummaryDto, offset_minutes: i32) -> TakeRow {
    let local = take.tracks.iter().any(|track| track.kind == "local");
    let markers = take.markers.len() as u64;
    TakeRow {
        key: take.folder.clone(),
        folder: take.folder.clone(),
        name: take.name.clone(),
        started: format_take_started(&take.started, offset_minutes),
        length: format_duration(take.seconds),
        tracks: tracks_summary(take.tracks.len() - usize::from(local), local),
        markers: if markers == 0 {
            "No markers".to_owned()
        } else {
            plural(markers, "marker")
        },
        end: take_end_label(&take.ended, take.reason.as_deref()),
        dropped: (take.dropped_frames > 0)
            .then(|| format!("{} missing", plural(take.dropped_frames, "frame"))),
        recording: take.ended == "recording",
    }
}

/// The takes table, newest first as listed (none until listed), each with its Soundcheck button.
/// The take being recorded now shows the length the status reports.
pub fn take_rows(state: &ConsoleState, offset_minutes: i32) -> Vec<(TakeRow, RowSoundcheck)> {
    let recording = state.recording_status();
    let takes = state.takes.list.as_deref().unwrap_or_default();
    takes
        .iter()
        .map(|take| {
            let live = recording.is_recording()
                && recording.take_folder.as_deref() == Some(take.folder.as_str());
            let mut row = take_row(take, offset_minutes);
            if live {
                row.length = format_duration(recording.seconds);
            }
            // Its Soundcheck button: a take's Play is a soundcheck start.
            (row, take_soundcheck(state, take))
        })
        .collect()
}

/// `seconds` in words: "12 minutes 34 seconds", "1 hour 2 minutes 5 seconds", "0 seconds".
pub fn spoken_duration(seconds: f64) -> String {
    let whole = if seconds.is_finite() {
        seconds.floor().max(0.0) as u64
    } else {
        0
    };
    let parts: Vec<String> = [
        (whole / 3600, "hour"),
        ((whole % 3600) / 60, "minute"),
        (whole % 60, "second"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, noun)| plural(count, noun))
    .collect();
    if parts.is_empty() {
        "0 seconds".to_owned()
    } else {
        parts.join(" ")
    }
}

/// A take's name from its folder: the folder's own name without its time stamp.
pub fn take_name_of(folder: Option<&str>) -> String {
    let base = folder
        .unwrap_or("")
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("");
    // `YYYY-MM-DD_HHMMSS_`, the stamp the desk names a take folder with.
    let stamped = base.len() > 18
        && base.is_char_boundary(18)
        && base[..18]
            .bytes()
            .enumerate()
            .all(|(index, byte)| match index {
                4 | 7 => byte == b'-',
                10 | 17 => byte == b'_',
                _ => byte.is_ascii_digit(),
            });
    if stamped { &base[18..] } else { base }.to_owned()
}

/// The REC chip, or `None` while no take records, is finished or failed.
pub fn rec_chip(state: &ConsoleState) -> Option<Chip> {
    let recording = state.recording_status();
    let time = format_duration(recording.seconds);
    let detail = plural(recording.tracks.len() as u64, "track");
    let spoken = spoken_duration(recording.seconds);
    let (kind, word, label) = match recording.state.as_str() {
        "recording" => (
            ChipKind::Recording,
            "REC",
            format!("Recording, {spoken}, {detail}. Open Record"),
        ),
        "stopping" => (
            ChipKind::Stopping,
            "Finishing",
            format!("Finishing the take's files after {spoken}. Open Record"),
        ),
        "failed" => (
            ChipKind::Failed,
            "REC stopped",
            format!(
                "{} Open Record",
                sentence_with_reason("The recording stopped", recording.reason.as_deref())
            ),
        ),
        _ => return None,
    };
    Some(Chip {
        kind,
        word: word.to_owned(),
        time,
        detail,
        label,
        target: Section::Record,
    })
}

/// A size in the decimal units a file manager shows: "214 GB", "1.3 GB", "705 MB".
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    let digits = if unit == 0 || value >= 100.0 { 0 } else { 1 };
    format!("{} {}", to_fixed(value, digits), UNITS[unit])
}

/// "45 min" up to two hours, then whole hours: "34 h".
fn format_minutes(minutes: u64) -> String {
    if minutes < 120 {
        format!("{minutes} min")
    } else {
        format!("{} h", minutes / 60)
    }
}

/// The warning for `minutes` of recording space left, or `None` when there is enough (or the
/// minutes are unknown).
fn disk_warning(minutes: Option<u64>) -> Option<DiskWarning> {
    let minutes = minutes.filter(|minutes| *minutes < LOW_MINUTES)?;
    let verb = if minutes == 1 { "is" } else { "are" };
    if minutes < CRITICAL_MINUTES {
        let lead = if minutes == 0 {
            "No recording space is left".to_owned()
        } else {
            format!(
                "Only about {} of recording space {verb} left",
                plural(minutes, "minute")
            )
        };
        return Some(DiskWarning {
            level: WarningLevel::Critical,
            text: format!(
                "{lead}: free some disk space or choose another folder before the take fills it."
            ),
        });
    }
    Some(DiskWarning {
        level: WarningLevel::Low,
        text: format!(
            "About {} of recording space {verb} left on this disk.",
            plural(minutes, "minute")
        ),
    })
}

/// `text` as a sentence: trimmed, its first letter capitalised and one full stop; `None` when
/// there is nothing to say.
fn sentence(text: Option<&str>) -> Option<String> {
    let trimmed = text.unwrap_or("").trim().trim_end_matches('.');
    let mut letters = trimmed.chars();
    let first = letters.next()?;
    Some(format!("{}{}.", first.to_uppercase(), letters.as_str()))
}

/// How loud the disk warning is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarningLevel {
    Low,
    Critical,
}

/// The disk warning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiskWarning {
    pub level: WarningLevel,
    pub text: String,
}

/// The disk, in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiskView {
    pub free: String,
    pub left: Option<String>,
    pub problem: Option<String>,
    pub warning: Option<DiskWarning>,
}

/// The disk: while a take records, what the recorder reports; otherwise the estimate for the kept
/// settings, and why it cannot be made.
pub fn disk_view(state: &ConsoleState) -> DiskView {
    let recording = state.recording_status();
    let live = recording.is_recording();
    let estimate = state.estimate.as_ref();
    let (free, minutes) = if live {
        (recording.free_bytes, recording.minutes_left)
    } else {
        (
            estimate.and_then(|estimate| estimate.free_bytes),
            estimate.and_then(|estimate| estimate.minutes_left),
        )
    };
    DiskView {
        free: match free {
            Some(bytes) => format!("{} free", format_bytes(bytes)),
            None => "Free space unknown".to_owned(),
        },
        left: minutes.map(|minutes| format!("about {} left", format_minutes(minutes))),
        problem: if live {
            None
        } else {
            sentence(estimate.and_then(|estimate| estimate.problem.as_deref()))
        },
        warning: disk_warning(minutes),
    }
}

/// A marker as the transport lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkerRow {
    pub key: String,
    pub time: String,
    pub name: String,
}

/// What the screen keeps of its own that the transport reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TransportLocal {
    /// Back to start was pressed after a take.
    pub at_start: bool,
    /// The take the soundcheck card picks for Play.
    pub chosen_take: Option<String>,
}

/// The transport: what can be pressed, the position, the state in words, the markers and what a
/// take would record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transport {
    pub recording: bool,
    pub soundcheck: bool,
    /// The recorder's state, or "soundcheck".
    pub state_kind: String,
    pub state_word: String,
    pub can_record: bool,
    pub can_stop: bool,
    pub stop_label: &'static str,
    pub can_mark: bool,
    pub can_rewind: bool,
    pub position: String,
    pub markers: Vec<MarkerRow>,
    pub markers_of: Option<String>,
    pub armed: String,
    pub format: String,
    pub play: Play,
}

/// The transport. Until Back to start (`local.at_start`) the position shows the last take's
/// length and its markers. Play is the virtual soundcheck; while a take plays in place of the
/// inputs the position is the take's, Stop goes back to the live inputs and Back to start moves
/// the take to its start.
pub fn transport_view(state: &ConsoleState, local: &TransportLocal) -> Transport {
    let soundcheck = state.soundcheck_status();
    let mut transport = record_transport(state, local);
    if !soundcheck.is_soundchecking() {
        return transport;
    }
    let name = soundcheck.take_name.as_deref().unwrap_or("the take");
    transport.soundcheck = true;
    transport.recording = false;
    transport.state_kind = "soundcheck".to_owned();
    transport.state_word = match soundcheck.state.as_str() {
        "paused" => format!("Soundcheck{SEPARATOR}\"{name}\" is paused"),
        "failed" => "The soundcheck stopped on an error".to_owned(),
        _ => format!("Soundcheck{SEPARATOR}\"{name}\" plays in place of the inputs"),
    };
    transport.can_record = false;
    transport.can_stop = true;
    transport.stop_label = "Back to live inputs";
    transport.can_mark = false;
    transport.can_rewind = soundcheck.state != "failed" && soundcheck.position_frames > 0;
    transport.position = format_duration(soundcheck.position_seconds);
    transport.markers = Vec::new();
    transport.markers_of = None;
    transport.play = soundcheck_play(state, local.chosen_take.as_deref());
    transport
}

fn marker_rows(markers: &[TakeMarker]) -> Vec<MarkerRow> {
    markers
        .iter()
        .enumerate()
        .map(|(index, marker)| MarkerRow {
            key: index.to_string(),
            time: format_duration(marker.seconds),
            name: marker.name.clone(),
        })
        .collect()
}

/// The words of a take's format.
fn format_words(format: &str) -> &str {
    match format {
        "int24" => "24-bit",
        "float32" => "32-bit float",
        other => other,
    }
}

fn record_transport(state: &ConsoleState, local: &TransportLocal) -> Transport {
    let recording = state.recording_status();
    let live = recording.is_recording();
    let failed = recording.state == "failed";
    let engine_runs = state.engine.phase == bandear_desktop_console::state::Phase::Running;
    let mut seconds = 0.0;
    let mut markers: &[TakeMarker] = &[];
    let mut markers_of = None;
    if live || failed {
        seconds = recording.seconds;
        markers = &recording.markers;
        markers_of = Some(take_name_of(recording.take_folder.as_deref()));
    } else if let Some(last) = state.last_take().filter(|_| !local.at_start) {
        seconds = last.seconds;
        markers = &last.markers;
        markers_of = Some(last.name.clone());
    }
    let state_word = match recording.state.as_str() {
        "recording" => format!(
            "Recording{SEPARATOR}{}",
            take_name_of(recording.take_folder.as_deref())
        ),
        "stopping" => "Finishing the take's files\u{2026}".to_owned(),
        "failed" => "The take stopped on an error".to_owned(),
        _ if !engine_runs => "Start the engine to record".to_owned(),
        _ => "Ready to record".to_owned(),
    };
    let armed_rows = track_rows(state).iter().filter(|row| row.armed).count();
    let settings = state.recording_settings.as_ref();
    let format = settings.map_or(RECORD_FORMATS[0], |settings| settings.format);
    Transport {
        recording: live,
        soundcheck: false,
        state_kind: recording.state.clone(),
        state_word,
        can_record: engine_runs && !live,
        can_stop: live || failed,
        stop_label: "Stop recording",
        can_mark: recording.state == "recording",
        can_rewind: !live && !failed && seconds > 0.0,
        position: format_duration(seconds),
        markers: marker_rows(markers),
        markers_of,
        armed: tracks_summary(
            armed_rows,
            settings.is_some_and(|settings| settings.include_local),
        ),
        format: format!("WAV{SEPARATOR}48 kHz{SEPARATOR}{}", format_words(format)),
        play: if live {
            Play {
                can: false,
                action: None,
                folder: None,
                label: RECORDING_BLOCKS_SOUNDCHECK.to_owned(),
            }
        } else {
            soundcheck_play(state, local.chosen_take.as_deref())
        },
    }
}

/// The last command's outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub ok: bool,
    pub text: String,
}

/// The screen's messages, each `None` when there is nothing to say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Messages {
    pub outcome: Option<Said>,
    pub failure: Option<String>,
    pub takes: Option<String>,
    pub locked: Option<&'static str>,
}

/// The screen's messages: the last command's outcome, a take that failed, a takes listing that
/// failed, and why arming is locked while a take records (or why nothing records while a
/// soundcheck plays).
pub fn record_messages(state: &ConsoleState) -> Messages {
    let recording = state.recording_status();
    let locked = if state.is_recording_now() {
        Some(LOCKED_WHILE_RECORDING)
    } else if state.is_soundcheck_now() {
        Some(SOUNDCHECK_BLOCKS_RECORDING)
    } else {
        None
    };
    Messages {
        outcome: state.recording_outcome.as_ref().map(|outcome| Said {
            ok: outcome.ok,
            text: outcome.message.clone(),
        }),
        failure: (recording.state == "failed").then(|| {
            sentence_with_reason("The take stopped recording", recording.reason.as_deref())
        }),
        takes: state
            .takes
            .error
            .as_deref()
            .map(|error| sentence_with_reason("The takes could not be listed", Some(error))),
        locked,
    }
}

/// The settings panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsView {
    /// False until the desk's settings were read.
    pub known: bool,
    pub folder: String,
    pub default_folder: String,
    pub is_default: bool,
    pub notice: Option<String>,
    pub format: String,
    /// The formats, `(value, label)`.
    pub formats: Vec<(&'static str, &'static str)>,
    pub include_local: bool,
    /// True while a take records.
    pub locked: bool,
}

/// The settings panel.
pub fn settings_view(state: &ConsoleState) -> SettingsView {
    let settings = state.recording_settings.as_ref();
    SettingsView {
        known: settings.is_some(),
        folder: settings
            .map(|settings| settings.folder.clone())
            .unwrap_or_default(),
        default_folder: settings
            .map(|settings| settings.default_folder.clone())
            .unwrap_or_default(),
        is_default: settings.is_none_or(|settings| settings.folder_is_default),
        notice: settings.and_then(|settings| settings.notice.clone()),
        format: settings
            .map_or(RECORD_FORMATS[0], |settings| settings.format)
            .to_owned(),
        formats: RECORD_FORMATS
            .iter()
            .map(|&value| (value, format_label(value)))
            .collect(),
        include_local: settings.is_some_and(|settings| settings.include_local),
        locked: state.is_recording_now(),
    }
}

/// The Files choice's words for a format.
fn format_label(format: &'static str) -> &'static str {
    match format {
        "int24" => "WAV 24-bit \u{b7} one file per track",
        "float32" => "WAV 32-bit float \u{b7} one file per track",
        other => other,
    }
}

#[cfg(test)]
mod tests;
