//! Recording takes: the pure part of the console's recording (`recording.js`), which the Record
//! screen shows. No window.
//!
//! The desk records the armed inputs of the running device into a take folder, a file per input
//! and one stereo file per linked pair, plus the local mix when asked (`commands/recording.rs`).
//! How it records is the desk's own settings ([RecordingSettingsDto]); the status's `recording`
//! says whether a take records, and its `lastTake` is the last take that ended.

use bandear_desktop_console::format::sentence_with_reason;
use bandear_desktop_console::types::dto::{
    RecordingSettingsDto, RecordingSettingsRequest, RecordingStartedDto, TakeSummaryDto,
};

/// The formats a take is recorded in, the default first.
pub const RECORD_FORMATS: [&str; 2] = ["int24", "float32"];

/// `count noun`, with the noun's plural for any count but one.
pub fn plural(count: u64, noun: &str) -> String {
    if count == 1 {
        format!("{count} {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// The recorded units of a device's inputs, as the engine lays out the tracks: `[first, second]`
/// (0-based) for an input linked to the next one (`links[i]`), else `[index]`. Hidden inputs
/// record too.
pub fn record_units(links: &[bool]) -> Vec<Vec<usize>> {
    let mut units = Vec::new();
    let mut index = 0;
    while index < links.len() {
        if links[index] && index + 1 < links.len() {
            units.push(vec![index, index + 1]);
            index += 2;
        } else {
            units.push(vec![index]);
            index += 1;
        }
    }
    units
}

/// The inputs (1-based) recorded with `input`: its pair, or itself.
fn unit_of(input: u32, links: &[bool]) -> Vec<u32> {
    let index = (input as usize).wrapping_sub(1);
    record_units(links)
        .into_iter()
        .find(|unit| unit.contains(&index))
        .map(|unit| unit.into_iter().map(|side| side as u32 + 1).collect())
        .unwrap_or_else(|| vec![input])
}

/// True when `input` (1-based) records with `armed`: it is armed, or the other side of its linked
/// pair is, since either side arms the pair.
pub fn is_armed(armed: &[u32], input: u32, links: &[bool]) -> bool {
    unit_of(input, links)
        .iter()
        .any(|side| armed.contains(side))
}

/// The armed inputs after arming (`on`) or disarming `input` (1-based), ascending and each once.
/// Disarming one side of a linked pair disarms the pair: either side alone would still record it.
pub fn armed_after(armed: &[u32], input: u32, on: bool, links: &[bool]) -> Vec<u32> {
    let mut next: std::collections::BTreeSet<u32> = armed.iter().copied().collect();
    if on {
        next.insert(input);
    } else {
        for side in unit_of(input, links) {
            next.remove(&side);
        }
    }
    next.into_iter().collect()
}

/// One input armed or disarmed, on the armed inputs a request is built on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arm {
    pub input: u32,
    pub on: bool,
    /// The device's inputs, each linked to the next or not.
    pub links: Vec<bool>,
}

/// A change of the recording settings; a field left `None` keeps what the desk has.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsChange {
    /// The folder (`Some(None)`, or blank text, for the default folder).
    pub folder: Option<Option<String>>,
    pub format: Option<String>,
    pub include_local: Option<bool>,
    pub armed: Option<Vec<u32>>,
    /// One input armed or disarmed when the request is built, so quick presses compose.
    pub arm: Option<Arm>,
}

/// The whole `set_recording_settings` request for `settings` (the desk's last answer, or `None`
/// for the defaults) with `change` on top; the folder `None` for the default folder. An arm press
/// applies to the armed inputs the request is built on, so presses made before an earlier one was
/// answered compose instead of replacing each other.
pub fn settings_request(
    settings: Option<&RecordingSettingsDto>,
    change: &SettingsChange,
) -> RecordingSettingsRequest {
    let mut folder = settings
        .filter(|settings| !settings.folder_is_default)
        .map(|settings| settings.folder.clone());
    let mut format = settings
        .map(|settings| settings.format.to_owned())
        .unwrap_or_else(|| RECORD_FORMATS[0].to_owned());
    let mut include_local = settings.is_some_and(|settings| settings.include_local);
    let mut armed: Vec<u32> = settings
        .map(|settings| {
            settings
                .armed
                .iter()
                .map(|&input| u32::from(input))
                .collect()
        })
        .unwrap_or_default();
    if let Some(chosen) = &change.folder {
        folder = chosen.clone();
    }
    if let Some(chosen) = &change.format {
        format = chosen.clone();
    }
    if let Some(chosen) = change.include_local {
        include_local = chosen;
    }
    if let Some(chosen) = &change.armed {
        armed = chosen.clone();
    }
    if let Some(arm) = &change.arm {
        armed = armed_after(&armed, arm.input, arm.on, &arm.links);
    }
    if folder.as_deref().is_some_and(|text| text.trim().is_empty()) {
        folder = None;
    }
    RecordingSettingsRequest {
        folder,
        format: Some(format),
        include_local,
        armed,
    }
}

/// `seconds` as a clock: "0:07", "3:25", "1:02:03".
pub fn format_duration(seconds: f64) -> String {
    let whole = if seconds.is_finite() {
        seconds.floor().max(0.0) as u64
    } else {
        0
    };
    let (hours, minutes, rest) = (whole / 3600, (whole % 3600) / 60, whole % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{rest:02}")
    } else {
        format!("{minutes}:{rest:02}")
    }
}

fn channel_list(inputs: &[u16]) -> String {
    inputs
        .iter()
        .map(|input| format!("CH {input}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a start did, from the `start_recording` answer, for a take called `name`.
pub fn format_started(started: &RecordingStartedDto, name: &str) -> String {
    let tracks = started.recording.tracks.len() as u64;
    let mut words = format!(
        "Recording \"{name}\" on {} in {}.",
        plural(tracks, "track"),
        started.folder
    );
    if !started.skipped.is_empty() {
        let verb = if started.skipped.len() == 1 {
            "is"
        } else {
            "are"
        };
        words.push_str(&format!(
            " {} {verb} armed but not on this device, so not recorded.",
            channel_list(&started.skipped)
        ));
    }
    words
}

/// How a take ended (`ended`, with the disk error's `reason`), in words.
pub fn format_take_end(ended: &str, reason: Option<&str>) -> String {
    match ended {
        "stopped" => "stopped".to_owned(),
        "engineStopped" => "ended when the engine stopped".to_owned(),
        "diskError" => {
            let sentence = sentence_with_reason("stopped by a disk error", reason);
            sentence
                .strip_suffix('.')
                .map(str::to_owned)
                .unwrap_or(sentence)
        }
        "recording" => "recording now".to_owned(),
        "interrupted" => {
            "interrupted before it was finished; its files hold what was written".to_owned()
        }
        _ => "ended".to_owned(),
    }
}

/// What a finished stop did: the take, its length, its tracks and anything lost.
pub fn format_stopped(take: &TakeSummaryDto) -> String {
    let mut words = format!(
        "\"{}\" {}: {} on {}.",
        take.name,
        format_take_end(&take.ended, take.reason.as_deref()),
        format_duration(take.seconds),
        plural(take.tracks.len() as u64, "track")
    );
    if take.dropped_frames > 0 {
        words.push_str(&format!(
            " {} the disk could not keep up with are missing.",
            plural(take.dropped_frames, "frame")
        ));
    }
    words
}

/// What `list_takes` left out, in words, or `None` when it read every take.
pub fn format_unreadable(unreadable: usize) -> Option<String> {
    if unreadable == 0 {
        return None;
    }
    let verb = if unreadable == 1 { "is" } else { "are" };
    Some(format!(
        "{} could not be read and {verb} not listed.",
        plural(unreadable as u64, "take")
    ))
}

#[cfg(test)]
mod tests;
