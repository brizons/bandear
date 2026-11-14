//! The virtual soundcheck: the pure part of the console's soundcheck (`soundcheck.js`), which the
//! Record screen and the shell's chip show. No window.
//!
//! A soundcheck plays a recorded take back into the mixer in place of the live inputs
//! (`commands/soundcheck.rs`): the phones, the local output and the meters hear the take as they
//! would hear the band. There is no separate "listen only" playback: a take's Play is a soundcheck
//! start. "paused" is also a take that stopped at its end without looping (Play starts it again
//! from its start); "failed" is a read error, after which only going back to the live inputs is
//! left. While a soundcheck plays nothing records and the input device stays as it is; while a
//! take records no soundcheck starts.

use bandear_desktop_console::values::js_round;
use bandear_desktop_console::wire::Soundcheck;

use crate::recording::format_duration;

/// Every take plays at the engine's rate.
pub const SOUNDCHECK_RATE: u32 = 48_000;

/// Why nothing records while a soundcheck plays.
pub const SOUNDCHECK_BLOCKS_RECORDING: &str =
    "Go back to the live inputs before recording: a take never records a soundcheck's playback.";

/// Why no soundcheck starts while a take records.
pub const RECORDING_BLOCKS_SOUNDCHECK: &str =
    "Stop the recording before the soundcheck: a take records the live inputs.";

/// A soundcheck command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SoundcheckAction {
    Start,
    Pause,
    Resume,
    Seek,
    Loop,
    Stop,
}

impl SoundcheckAction {
    /// The lead of the command's words when the desk refused it.
    pub fn refused_lead(self) -> &'static str {
        match self {
            Self::Start => "The soundcheck did not start",
            Self::Pause => "The soundcheck was not paused",
            Self::Resume => "The soundcheck did not play on",
            Self::Seek => "The soundcheck did not move",
            Self::Loop => "The soundcheck's loop was not changed",
            Self::Stop => "The soundcheck did not stop",
        }
    }
}

/// The frame `seconds` into a take of `length_frames` frames: whole, and within the take.
pub fn frame_at(seconds: f64, length_frames: u64) -> u64 {
    let seconds = if seconds.is_finite() { seconds } else { 0.0 };
    let frame = js_round(seconds * f64::from(SOUNDCHECK_RATE)).max(0.0);
    // Held to the take before the cast, so a frame past it cannot overflow.
    frame.min(length_frames as f64) as u64
}

/// What a soundcheck command did, in words: `action` and its answer `soundcheck`.
pub fn format_soundcheck_done(action: SoundcheckAction, soundcheck: &Soundcheck) -> String {
    let at = format_duration(soundcheck.position_seconds);
    let name = soundcheck.take_name.as_deref().unwrap_or("the take");
    match action {
        SoundcheckAction::Start => {
            let looping = if soundcheck.looping { ", looping" } else { "" };
            format!("Soundcheck: \"{name}\" plays in place of the live inputs from {at}{looping}.")
        }
        SoundcheckAction::Pause => format!("Soundcheck paused at {at}."),
        SoundcheckAction::Resume => format!("Soundcheck playing from {at}."),
        SoundcheckAction::Seek => format!("Soundcheck moved to {at}."),
        SoundcheckAction::Loop if soundcheck.looping => {
            "The soundcheck starts the take again at its end.".to_owned()
        }
        SoundcheckAction::Loop => "The soundcheck stops at the end of the take.".to_owned(),
        SoundcheckAction::Stop => "Back to the live inputs.".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_soundchecks_frames_and_words() {
        assert_eq!(frame_at(1.5, 144_000), 72_000);
        assert_eq!(frame_at(9.0, 144_000), 144_000, "never past the end");
        assert_eq!(frame_at(-1.0, 144_000), 0);
        assert_eq!(frame_at(f64::NAN, 144_000), 0);
        let playing = Soundcheck {
            state: "playing".to_owned(),
            take_name: Some("Show".to_owned()),
            position_seconds: 62.0,
            looping: true,
            ..Soundcheck::default()
        };
        assert_eq!(
            format_soundcheck_done(SoundcheckAction::Start, &playing),
            "Soundcheck: \"Show\" plays in place of the live inputs from 1:02, looping."
        );
        let once = Soundcheck {
            looping: false,
            take_name: None,
            ..playing.clone()
        };
        assert_eq!(
            format_soundcheck_done(SoundcheckAction::Start, &once),
            "Soundcheck: \"the take\" plays in place of the live inputs from 1:02."
        );
        assert_eq!(
            format_soundcheck_done(SoundcheckAction::Pause, &playing),
            "Soundcheck paused at 1:02."
        );
        assert_eq!(
            format_soundcheck_done(SoundcheckAction::Resume, &playing),
            "Soundcheck playing from 1:02."
        );
        assert_eq!(
            format_soundcheck_done(SoundcheckAction::Seek, &playing),
            "Soundcheck moved to 1:02."
        );
        assert_eq!(
            format_soundcheck_done(SoundcheckAction::Loop, &playing),
            "The soundcheck starts the take again at its end."
        );
        assert_eq!(
            format_soundcheck_done(SoundcheckAction::Loop, &once),
            "The soundcheck stops at the end of the take."
        );
        assert_eq!(
            format_soundcheck_done(SoundcheckAction::Stop, &Soundcheck::default()),
            "Back to the live inputs."
        );
    }

    #[test]
    fn each_refused_command_has_its_lead() {
        let leads: Vec<_> = [
            SoundcheckAction::Start,
            SoundcheckAction::Pause,
            SoundcheckAction::Resume,
            SoundcheckAction::Seek,
            SoundcheckAction::Loop,
            SoundcheckAction::Stop,
        ]
        .into_iter()
        .map(SoundcheckAction::refused_lead)
        .collect();
        assert_eq!(
            leads,
            [
                "The soundcheck did not start",
                "The soundcheck was not paused",
                "The soundcheck did not play on",
                "The soundcheck did not move",
                "The soundcheck's loop was not changed",
                "The soundcheck did not stop",
            ]
        );
    }

    #[test]
    fn the_live_inputs_are_no_soundcheck_and_any_other_state_is_one() {
        assert!(!Soundcheck::default().is_soundchecking());
        for state in ["playing", "paused", "failed"] {
            let soundcheck = Soundcheck {
                state: state.to_owned(),
                ..Soundcheck::default()
            };
            assert!(soundcheck.is_soundchecking(), "{state}");
        }
    }
}
