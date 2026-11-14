//! What the Record screen's virtual soundcheck card, its transport's Play and its takes table show
//! of the soundcheck, and the shell's SOUNDCHECK chip (`soundcheck-view.js`). Pure, no window.
//!
//! - The card: with the inputs live, the takes a soundcheck can play (never the take being
//!   recorded) with the one Play would start, a loop choice for the start, and why it cannot
//!   start; while a take plays, its name, position and length with a seek slider, pause or play,
//!   the loop, "back to live inputs", what paces it, the dropouts and a failure.
//! - Play has one target: the take chosen in the card, else the last take recorded, else the
//!   newest listed. There is no separate "listen only" playback: Play is a soundcheck start.

use bandear_desktop_console::chips::{Chip, ChipKind};
use bandear_desktop_console::format::{SEPARATOR, sentence_with_reason};
use bandear_desktop_console::router::Section;
use bandear_desktop_console::state::{ConsoleState, Phase};
use bandear_desktop_console::types::dto::TakeSummaryDto;
use bandear_desktop_console::values::js_round;
use bandear_desktop_console::wire::Soundcheck;

use crate::recording::format_duration;
use crate::soundcheck::{RECORDING_BLOCKS_SOUNDCHECK, SOUNDCHECK_RATE};

const NO_TAKES: &str = "Record a take first: its tracks then play back here in place of the \
                        inputs, so every mix and the phones can be set with nobody playing.";

const NO_ENGINE: &str = "Start the engine to play a take in place of the inputs.";

fn engine_runs(state: &ConsoleState) -> bool {
    state.engine.phase == Phase::Running
}

/// A take a soundcheck can play.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayableTake {
    pub folder: String,
    pub name: String,
    /// `name · m:ss`.
    pub label: String,
}

/// The takes a soundcheck can play, as listed: never the one recording, never an empty one.
pub fn playable_takes(state: &ConsoleState) -> Vec<PlayableTake> {
    state
        .takes
        .list
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|take| take.ended != "recording" && take.frames > 0)
        .map(|take| PlayableTake {
            folder: take.folder.clone(),
            name: take.name.clone(),
            label: format!("{}{SEPARATOR}{}", take.name, format_duration(take.seconds)),
        })
        .collect()
}

/// The take folder Play starts: `chosen` when it can be played, else the last take recorded,
/// else the newest listed; `None` without one.
pub fn soundcheck_target(state: &ConsoleState, chosen: Option<&str>) -> Option<String> {
    let playable = playable_takes(state);
    let can =
        |folder: &&str| !folder.is_empty() && playable.iter().any(|take| take.folder == *folder);
    if let Some(chosen) = chosen.filter(can) {
        return Some(chosen.to_owned());
    }
    let last = state.last_take().map(|take| take.folder.as_str());
    if let Some(last) = last.filter(can) {
        return Some(last.to_owned());
    }
    playable.first().map(|take| take.folder.clone())
}

/// Why a soundcheck cannot start now, or `None` when it can.
fn start_hint(state: &ConsoleState, target: Option<&str>) -> Option<&'static str> {
    if !engine_runs(state) {
        return Some(NO_ENGINE);
    }
    if state.is_recording_now() {
        return Some(RECORDING_BLOCKS_SOUNDCHECK);
    }
    match target {
        Some(_) => None,
        None => Some(NO_TAKES),
    }
}

/// A take stopped at its end without looping: paused there.
fn at_end(soundcheck: &Soundcheck) -> bool {
    soundcheck.state == "paused"
        && soundcheck.length_frames > 0
        && soundcheck.position_frames >= soundcheck.length_frames
}

fn underrun_words(frames: u64) -> String {
    if frames == 0 {
        return "No dropouts.".to_owned();
    }
    let ms = js_round(frames as f64 * 1000.0 / f64::from(SOUNDCHECK_RATE));
    format!("{ms} ms of silence where the disk fell behind.")
}

/// How the card's badge looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeKind {
    Off,
    Ok,
    Warn,
    Error,
}

/// The seek slider: its end, where it is, and whether it moves.
#[derive(Clone, Debug, PartialEq)]
pub struct Seek {
    pub max: f64,
    pub value: f64,
    pub disabled: bool,
}

/// The virtual soundcheck card.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundcheckCard {
    pub on: bool,
    /// The soundcheck's state.
    pub kind: String,
    pub badge: String,
    pub badge_kind: BadgeKind,
    pub take_name: Option<String>,
    pub position: String,
    pub length: String,
    pub slider: Seek,
    pub can_pause: bool,
    pub can_resume: bool,
    pub resume_word: &'static str,
    pub can_stop: bool,
    pub looping: bool,
    pub can_loop: bool,
    pub clock: &'static str,
    pub underruns: String,
    pub failure: Option<String>,
    pub choices: Vec<PlayableTake>,
    pub chosen: Option<String>,
    pub can_start: bool,
    pub start_hint: Option<&'static str>,
}

/// What the card shows. `chosen` is the take picked in the card, `drag` the seconds the slider is
/// being dragged to, `loop_next` the loop asked for the next start.
pub fn soundcheck_view(
    state: &ConsoleState,
    chosen: Option<&str>,
    drag: Option<f64>,
    loop_next: bool,
) -> SoundcheckCard {
    let soundcheck = state.soundcheck_status();
    let on = soundcheck.is_soundchecking();
    let failed = soundcheck.state == "failed";
    let target = soundcheck_target(state, chosen);
    let seconds = drag.unwrap_or(soundcheck.position_seconds);
    let hint = if on {
        None
    } else {
        start_hint(state, target.as_deref())
    };
    let badge = if at_end(&soundcheck) {
        "At the end".to_owned()
    } else {
        match soundcheck.state.as_str() {
            "off" => "Off",
            "playing" => "Playing",
            "paused" => "Paused",
            "failed" => "Failed",
            other => other,
        }
        .to_owned()
    };
    SoundcheckCard {
        on,
        badge,
        badge_kind: match soundcheck.state.as_str() {
            "off" => BadgeKind::Off,
            "paused" => BadgeKind::Warn,
            "failed" => BadgeKind::Error,
            _ => BadgeKind::Ok,
        },
        take_name: soundcheck.take_name.clone(),
        position: format_duration(seconds),
        length: format_duration(soundcheck.length_seconds),
        slider: Seek {
            max: soundcheck.length_seconds,
            value: seconds,
            disabled: failed,
        },
        can_pause: soundcheck.state == "playing",
        can_resume: soundcheck.state == "paused",
        resume_word: if at_end(&soundcheck) {
            "Play again"
        } else {
            "Play"
        },
        can_stop: on,
        looping: if on { soundcheck.looping } else { loop_next },
        can_loop: !failed,
        clock: if soundcheck.clock == "output" {
            "Paced by the local output."
        } else {
            "Paced by a timer: no local output plays."
        },
        underruns: underrun_words(soundcheck.underrun_frames),
        failure: failed.then(|| {
            sentence_with_reason(
                "The soundcheck stopped playing",
                soundcheck.reason.as_deref(),
            )
        }),
        choices: playable_takes(state),
        chosen: target,
        can_start: !on && hint.is_none(),
        start_hint: hint,
        kind: soundcheck.state,
    }
}

/// What Play does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayAction {
    /// Starts the target take as a soundcheck.
    Start,
    Pause,
    Resume,
}

/// The transport's Play: whether it can be pressed, what it does (on which take), and its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Play {
    pub can: bool,
    pub action: Option<PlayAction>,
    pub folder: Option<String>,
    pub label: String,
}

/// What the transport's Play does now.
pub fn soundcheck_play(state: &ConsoleState, chosen: Option<&str>) -> Play {
    let soundcheck = state.soundcheck_status();
    let fixed = |can, action, label: &str| Play {
        can,
        action,
        folder: None,
        label: label.to_owned(),
    };
    match soundcheck.state.as_str() {
        "playing" => return fixed(true, Some(PlayAction::Pause), "Pause the soundcheck"),
        "paused" => return fixed(true, Some(PlayAction::Resume), "Play the soundcheck on"),
        "failed" => return fixed(false, None, "The soundcheck stopped on an error"),
        _ => {}
    }
    let Some(folder) = soundcheck_target(state, chosen) else {
        return fixed(
            false,
            Some(PlayAction::Start),
            "Record a take to play it back",
        );
    };
    let name = playable_takes(state)
        .into_iter()
        .find(|take| take.folder == folder)
        .map_or_else(|| "the take".to_owned(), |take| take.name);
    Play {
        can: engine_runs(state) && !state.is_recording_now(),
        action: Some(PlayAction::Start),
        folder: Some(folder),
        label: format!("Play \"{name}\" in place of the live inputs"),
    }
}

/// A take row's Soundcheck button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowSoundcheck {
    /// The take plays in place of the inputs now ("Playing").
    pub playing: bool,
    pub disabled: bool,
    pub title: String,
}

/// A take row's Soundcheck button: only one take plays at a time, never the take being recorded,
/// never while one records or without an engine.
pub fn take_soundcheck(state: &ConsoleState, take: &TakeSummaryDto) -> RowSoundcheck {
    let soundcheck = state.soundcheck_status();
    if soundcheck.is_soundchecking()
        && soundcheck.take_folder.as_deref() == Some(take.folder.as_str())
    {
        return RowSoundcheck {
            playing: true,
            disabled: true,
            title: "This take plays in place of the live inputs now".to_owned(),
        };
    }
    let recording = take.ended == "recording";
    let blocked = recording
        || !engine_runs(state)
        || state.is_recording_now()
        || soundcheck.is_soundchecking()
        || take.frames == 0;
    RowSoundcheck {
        playing: false,
        disabled: blocked,
        title: if recording {
            "This take is being recorded".to_owned()
        } else {
            format!("Play \"{}\" in place of the live inputs", take.name)
        },
    }
}

/// The SOUNDCHECK chip, or `None` while the inputs are live.
pub fn soundcheck_chip(state: &ConsoleState) -> Option<Chip> {
    let soundcheck = state.soundcheck_status();
    if !soundcheck.is_soundchecking() {
        return None;
    }
    let name = soundcheck.take_name.as_deref().unwrap_or("the take");
    let time = format_duration(soundcheck.position_seconds);
    let whole = if soundcheck.position_seconds.is_finite() {
        soundcheck.position_seconds.floor().max(0.0) as u64
    } else {
        0
    };
    let at = format!("{whole} second{}", if whole == 1 { "" } else { "s" });
    let (kind, word, detail, label) = match soundcheck.state.as_str() {
        "failed" => (
            ChipKind::SoundcheckFailed,
            "SOUNDCHECK stopped",
            name.to_owned(),
            format!(
                "{} Open Record",
                sentence_with_reason(
                    "The soundcheck stopped playing",
                    soundcheck.reason.as_deref()
                )
            ),
        ),
        "paused" => (
            ChipKind::SoundcheckPaused,
            "SOUNDCHECK",
            format!("{name}{SEPARATOR}paused"),
            format!(
                "Soundcheck: \"{name}\" is paused in place of the live inputs, at {at}. Open Record"
            ),
        ),
        _ => (
            ChipKind::Soundcheck,
            "SOUNDCHECK",
            name.to_owned(),
            format!(
                "Soundcheck: \"{name}\" plays in place of the live inputs, at {at}. Open Record"
            ),
        ),
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

#[cfg(test)]
mod tests;
