//! The shared reverb and delay, as the Effects screen shows and changes them (`effects.js`). No
//! window.
//!
//! The desk has one reverb and one delay on two buses. Every input sends to each at its own level
//! (kept with its strip, the Processing screen's), every mix returns each at its own level (the Mix
//! screen's master strip); this screen sets the effects themselves. They add no latency to the dry
//! mix. The ranges and the clamping are the engine's own ([ReverbSettings::clamped],
//! [DelaySettings::clamped]), so what the screen sends is what the engine keeps.

use bandear_desktop_console::format::sentence_with_reason;
use bandear_desktop_console::state::{ConsoleState, Phase};
use bandear_desktop_console::types::dto::EffectSetDto;
use bandear_desktop_console::types::{DelayNote, DelaySettings, ReverbSettings};
use bandear_desktop_console::values::js_round;

/// A note value the delay follows at a tempo: its value as the desk names it and its words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoteChoice {
    pub note: DelayNote,
    pub value: &'static str,
    pub label: &'static str,
}

/// The note values, in the order the Note select offers them.
pub const DELAY_NOTES: [NoteChoice; 7] = [
    NoteChoice {
        note: DelayNote::Half,
        value: "half",
        label: "1/2",
    },
    NoteChoice {
        note: DelayNote::Quarter,
        value: "quarter",
        label: "1/4",
    },
    NoteChoice {
        note: DelayNote::DottedEighth,
        value: "dottedEighth",
        label: "Dotted 1/8",
    },
    NoteChoice {
        note: DelayNote::QuarterTriplet,
        value: "quarterTriplet",
        label: "1/4 triplet",
    },
    NoteChoice {
        note: DelayNote::Eighth,
        value: "eighth",
        label: "1/8",
    },
    NoteChoice {
        note: DelayNote::EighthTriplet,
        value: "eighthTriplet",
        label: "1/8 triplet",
    },
    NoteChoice {
        note: DelayNote::Sixteenth,
        value: "sixteenth",
        label: "1/16",
    },
];

/// The lowest and highest tempo, in bpm.
pub const TEMPO_RANGE: (f32, f32) = (30.0, 300.0);

/// The choice of `note`.
pub fn note_choice(note: DelayNote) -> NoteChoice {
    DELAY_NOTES
        .into_iter()
        .find(|choice| choice.note == note)
        .unwrap_or(DELAY_NOTES[1])
}

/// The note the desk names `value`; an unknown one is a quarter.
pub fn note_of(value: &str) -> DelayNote {
    DELAY_NOTES
        .into_iter()
        .find(|choice| choice.value == value)
        .map_or(DelayNote::Quarter, |choice| choice.note)
}

/// The time the delay repeats at, in ms: the note at the tempo when one is set, held to 1-2,000.
pub fn delay_time_ms(delay: &DelaySettings) -> f64 {
    f64::from(delay.time_ms_in_force())
}

/// A time in words: "375 ms", "1.25 s".
pub fn format_time(ms: f64) -> String {
    let value = if ms.is_finite() { ms } else { 0.0 };
    if value >= 1000.0 {
        format!("{} s", js_round(value / 10.0) / 100.0)
    } else {
        format!("{} ms", js_round(value))
    }
}

/// A frequency in words: "6 kHz", "12.5 kHz".
pub fn format_khz(hz: f64) -> String {
    let value = if hz.is_finite() { hz } else { 0.0 };
    format!("{} kHz", js_round(value / 100.0) / 10.0)
}

/// A tempo as its field shows it: "128.5", "120".
pub fn format_tempo(bpm: f32) -> String {
    // An `f32` prints its shortest form, so a tempo kept to a tenth reads as typed.
    bpm.to_string()
}

/// The tempo typed in the delay's tempo field: the bpm (0 for an empty field: no tempo), or why
/// the text is not a tempo in range.
pub fn read_tempo(text: &str) -> Result<f32, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(0.0);
    }
    let (low, high) = TEMPO_RANGE;
    let refused = || format!("A tempo is {low} to {high} bpm, or empty for none.");
    let number: f64 = trimmed
        .replacen(',', ".", 1)
        .parse()
        .map_err(|_| refused())?;
    if !number.is_finite() || number < f64::from(low) || number > f64::from(high) {
        return Err(refused());
    }
    Ok((js_round(number * 10.0) / 10.0) as f32)
}

/// The reverb as the panel shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct ReverbView {
    pub settings: ReverbSettings,
    pub size: String,
    pub damping: String,
    pub pre_delay: String,
    pub width: String,
}

/// The delay as the panel shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct DelayView {
    pub settings: DelaySettings,
    /// The time slider's own value, in words, even while a tempo sets the time.
    pub time: String,
    /// The time in force, with the note and tempo when synced: "250 ms (1/8 at 120 bpm)".
    pub time_in_force: String,
    pub feedback: String,
    pub high_cut: String,
    /// A tempo sets the time.
    pub synced: bool,
    pub note_label: &'static str,
}

/// What the effects panel shows.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectsView {
    /// An engine runs: otherwise every control is disabled.
    pub running: bool,
    pub reverb: ReverbView,
    pub delay: DelayView,
}

/// What the effects panel shows: both effects (the change on its way over the status's), held in
/// range, with every value in words.
pub fn effects_view(state: &ConsoleState) -> EffectsView {
    let status = state.engine.status.as_ref();
    let kept = status.map(|status| status.effects).unwrap_or_default();
    let pending = &state.effects_pending;
    let reverb = pending
        .reverb
        .as_ref()
        .map_or(kept.reverb, |waiting| waiting.value)
        .clamped();
    let delay = pending
        .delay
        .as_ref()
        .map_or(kept.delay, |waiting| waiting.value)
        .clamped();
    let synced = delay.tempo_bpm > 0.0;
    let note = note_choice(delay.note);
    let in_force = format_time(delay_time_ms(&delay));
    let percent = |value: f32| format!("{}%", js_round(f64::from(value)));
    EffectsView {
        running: state.engine.phase == Phase::Running && status.is_some(),
        reverb: ReverbView {
            size: percent(reverb.size_percent),
            damping: percent(reverb.damping_percent),
            pre_delay: format_time(f64::from(reverb.pre_delay_ms)),
            width: percent(reverb.width_percent),
            settings: reverb,
        },
        delay: DelayView {
            time: format_time(f64::from(delay.time_ms)),
            time_in_force: if synced {
                format!(
                    "{in_force} ({} at {} bpm)",
                    note.label,
                    format_tempo(delay.tempo_bpm)
                )
            } else {
                in_force
            },
            feedback: percent(delay.feedback_percent),
            high_cut: format_khz(f64::from(delay.high_cut_hz)),
            synced,
            note_label: note.label,
            settings: delay,
        },
    }
}

/// A command's outcome in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub ok: bool,
    pub message: String,
}

/// The desk's answer to an effect change, in words.
pub fn format_effect_set(answer: &EffectSetDto) -> Said {
    if answer.outcome != "set" {
        let message = if answer.message.is_empty() {
            "The effect was not set.".to_owned()
        } else {
            answer.message.clone()
        };
        return Said { ok: false, message };
    }
    let mut message = if answer.message.is_empty() {
        format!("The {} is set.", answer.effect.name())
    } else {
        answer.message.clone()
    };
    if let Some(error) = answer.save_error.as_deref() {
        message.push(' ');
        message.push_str(&sentence_with_reason(
            "It was not saved for the next start",
            Some(error),
        ));
    }
    Said { ok: true, message }
}

#[cfg(test)]
mod tests;
