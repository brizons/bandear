//! Each input's channel processing as the desk answers it (`processing.js`): the strip's ranges,
//! its defaults and neutral test (the engine's own, see [StripSettings]), the equaliser's response
//! worked out here as the desk works it out, and the desk's answers in words.
//!
//! Channels are numbered from 1, as the desk numbers them. A linked pair is one strip: setting
//! either side sets both, and the answer names both.

use bandear_desktop_console::state::pair_units;
use bandear_desktop_console::types::{
    EqPointDto, EqSettings, ProcessingSetDto, StripPresetsSetDto, StripSettings,
};
use bandear_desktop_console::wire::Channel;

/// The mixer's channels, each with its strip.
pub const STRIP_CHANNELS: usize = 32;

/// The points of an equaliser response when none is asked for, and its frequency span.
pub const EQ_RESPONSE_POINTS: usize = 128;
pub const EQ_LOW_HZ: f64 = 20.0;
pub const EQ_HIGH_HZ: f64 = 20_000.0;

/// A number's range, `(low, high)`, as the desk holds it.
pub type Range = (f64, f64);

/// Every number's range, as the desk holds it (the engine's `dsp::eq` and `dsp::dynamics`).
pub mod ranges {
    use super::Range;

    pub const HIGH_PASS_FREQUENCY_HZ: Range = (20.0, 2_000.0);
    pub const HIGH_PASS_Q: Range = (0.5, 2.0);
    pub const BAND_FREQUENCY_HZ: Range = (20.0, 20_000.0);
    pub const BAND_GAIN_DB: Range = (-18.0, 18.0);
    pub const BAND_Q: Range = (0.3, 10.0);
    pub const GATE_THRESHOLD_DB: Range = (-80.0, 0.0);
    pub const GATE_RANGE_DB: Range = (0.0, 80.0);
    pub const GATE_ATTACK_MS: Range = (0.05, 50.0);
    pub const GATE_HOLD_MS: Range = (0.0, 2_000.0);
    pub const GATE_RELEASE_MS: Range = (5.0, 4_000.0);
    pub const COMPRESSOR_THRESHOLD_DB: Range = (-60.0, 0.0);
    pub const COMPRESSOR_RATIO: Range = (1.0, 20.0);
    pub const COMPRESSOR_KNEE_DB: Range = (0.0, 24.0);
    pub const COMPRESSOR_ATTACK_MS: Range = (0.1, 200.0);
    pub const COMPRESSOR_RELEASE_MS: Range = (10.0, 2_000.0);
    pub const COMPRESSOR_MAKEUP_DB: Range = (0.0, 24.0);
}

/// A strip that does nothing, with the desk's defaults.
pub fn default_strip() -> StripSettings {
    StripSettings::default()
}

/// `settings` as the desk keeps them: every number held in its range, one that is not a number
/// taking its default. Every edit is sent so.
pub fn clamp_strip(settings: &StripSettings) -> StripSettings {
    settings.clamped()
}

/// True when the strip changes nothing, as the desk decides it: the audio then skips it. The
/// sends are not part of it.
pub fn is_neutral_strip(settings: &StripSettings) -> bool {
    settings.is_neutral()
}

/// True when the equaliser changes nothing: the high-pass off and every band off or at 0 dB.
pub fn is_neutral_eq(eq: &EqSettings) -> bool {
    eq.is_neutral()
}

/// The gain of the equaliser `eq` at `freq_hz` in dB, the high-pass filter included, as the desk
/// works it out (RBJ biquads at 48 kHz).
pub fn eq_magnitude_db(eq: &EqSettings, freq_hz: f64) -> f64 {
    eq.clamped().magnitude_db(freq_hz)
}

/// The equaliser's response: `points` gains (held to 2-1,024) spaced evenly on a log scale from
/// 20 Hz to 20 kHz, both ends included, as `eq_response` answers.
pub fn eq_response_of(eq: &EqSettings, points: usize) -> Vec<EqPointDto> {
    let count = points.clamp(2, 1024);
    let span = EQ_HIGH_HZ / EQ_LOW_HZ;
    let eq = eq.clamped();
    (0..count)
        .map(|index| {
            let freq_hz = EQ_LOW_HZ * span.powf(index as f64 / (count - 1) as f64);
            EqPointDto {
                freq_hz,
                db: eq.magnitude_db(freq_hz),
            }
        })
        .collect()
}

/// "CH 3", "CH 3 and CH 4", "CH 1, CH 2 and CH 5".
pub fn channel_list(channels: &[usize]) -> String {
    let names: Vec<String> = channels
        .iter()
        .map(|channel| format!("CH {channel}"))
        .collect();
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// What a command did, in words: the status line's when it worked, the alert's when not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub ok: bool,
    pub message: String,
}

impl Said {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: message.into(),
        }
    }

    pub fn refused(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
        }
    }
}

/// The desk's answer to a processing change (`set_channel_processing`, `reset_channel_processing`,
/// `copy_channel_processing`, `apply_strip_preset`, `set_channel_send`) in words. Only `set` is
/// done; `invalid` and `notFound` changed nothing.
pub fn format_processing_set(answer: &ProcessingSetDto) -> Said {
    said(
        answer.outcome == "set",
        &answer.message,
        "The processing was changed.",
        "The processing was not changed.",
    )
}

/// The desk's answer to a strip preset change (`save_strip_preset`, `delete_strip_preset`) in
/// words: `saved`, `replaced` and `deleted` are done; `invalid`, `factory` and `notFound` not.
pub fn format_strip_presets_set(answer: &StripPresetsSetDto) -> Said {
    said(
        matches!(answer.outcome, "saved" | "replaced" | "deleted"),
        &answer.message,
        "The strip presets were changed.",
        "The strip presets were not changed.",
    )
}

fn said(ok: bool, message: &str, done: &str, not_done: &str) -> Said {
    let message = if message.is_empty() {
        if ok { done } else { not_done }
    } else {
        message
    };
    Said {
        ok,
        message: message.to_owned(),
    }
}

/// `channels` (1-based) with the other side of each one's stereo pair among `table` (a status's
/// channels in channel order), each once, in the order met: the strips a change to them touches.
pub fn strip_unit_channels(table: &[Channel], channels: &[usize]) -> Vec<usize> {
    let units = pair_units(table);
    let mut touched = Vec::new();
    for channel in channels {
        if !touched.contains(channel) {
            touched.push(*channel);
        }
        let unit = units.iter().find(|unit| {
            channel
                .checked_sub(1)
                .is_some_and(|index| unit.contains(&index))
        });
        for index in unit.into_iter().flatten() {
            if !touched.contains(&(index + 1)) {
                touched.push(index + 1);
            }
        }
    }
    touched
}

#[cfg(test)]
pub(crate) mod tests {
    use bandear_desktop_console::types::EqBandSettings;

    use super::*;

    /// A strip with its compressor on at `threshold_db`.
    pub(crate) fn compressed(threshold_db: f32) -> StripSettings {
        let mut strip = default_strip();
        strip.compressor.enabled = true;
        strip.compressor.threshold_db = threshold_db;
        strip
    }

    /// A strip whose first peak band boosts by `gain_db`.
    pub(crate) fn boosted(gain_db: f32) -> StripSettings {
        let mut strip = default_strip();
        strip.eq.bands[1].gain_db = gain_db;
        strip
    }

    #[test]
    fn a_strip_takes_the_desks_defaults_and_holds_every_number_in_range() {
        let neutral = default_strip();
        assert!(is_neutral_strip(&neutral));
        let frequencies: Vec<f32> = neutral
            .eq
            .bands
            .iter()
            .map(|band| band.frequency_hz)
            .collect();
        assert_eq!(frequencies, [100.0, 400.0, 2_500.0, 8_000.0]);
        assert!(
            neutral
                .eq
                .bands
                .iter()
                .all(|band| band.enabled && band.gain_db == 0.0)
        );
        assert!(!neutral.eq.high_pass.enabled);
        assert_eq!(neutral.eq.high_pass.frequency_hz, 80.0);
        assert_eq!(
            (
                neutral.gate.threshold_db,
                neutral.gate.range_db,
                neutral.gate.attack_ms,
                neutral.gate.hold_ms,
                neutral.gate.release_ms
            ),
            (-50.0, 40.0, 0.5, 50.0, 150.0)
        );
        let compressor = neutral.compressor;
        assert_eq!(
            (
                compressor.threshold_db,
                compressor.ratio,
                compressor.knee_db,
                compressor.attack_ms,
                compressor.release_ms,
                compressor.makeup_db
            ),
            (-18.0, 3.0, 6.0, 10.0, 150.0, 0.0)
        );
        assert_eq!((neutral.sends.reverb, neutral.sends.delay), (0, 0));

        let mut wild = default_strip();
        wild.eq.high_pass.enabled = true;
        wild.eq.high_pass.frequency_hz = 5.0;
        wild.eq.bands[0].gain_db = 40.0;
        wild.eq.bands[1].q = f32::NAN;
        wild.gate.threshold_db = -200.0;
        wild.gate.range_db = 99.0;
        wild.compressor.enabled = true;
        wild.compressor.ratio = 50.0;
        wild.compressor.makeup_db = f32::NAN;
        let held = clamp_strip(&wild);
        assert_eq!(held.eq.high_pass.frequency_hz, 20.0);
        assert_eq!(held.eq.bands[0].gain_db, 18.0);
        assert_eq!(
            held.eq.bands[0].frequency_hz, 100.0,
            "the band keeps its frequency"
        );
        assert_eq!(held.eq.bands[1].q, 1.0, "not a number takes the default");
        assert_eq!(held.gate.threshold_db, -80.0);
        assert_eq!(held.gate.range_db, 80.0);
        assert_eq!(held.compressor.ratio, 20.0);
        assert_eq!(held.compressor.makeup_db, 0.0);

        assert!(!is_neutral_strip(&compressed(-20.0)));
        let mut unity = compressed(-20.0);
        unity.compressor.ratio = 1.0;
        assert!(
            is_neutral_strip(&unity),
            "a ratio of 1 with no makeup does nothing"
        );
        assert!(!is_neutral_strip(&boosted(3.0)));
        let mut gate = default_strip();
        gate.gate.enabled = true;
        gate.gate.range_db = 0.0;
        assert!(
            is_neutral_strip(&gate),
            "a gate closing by 0 dB does nothing"
        );
        assert!(is_neutral_eq(&default_strip().eq));
    }

    #[test]
    fn the_equalisers_response_is_worked_out_as_the_desk_works_it_out() {
        let flat = eq_response_of(&default_strip().eq, EQ_RESPONSE_POINTS);
        assert_eq!(flat.len(), 128);
        assert_eq!(flat[0].freq_hz, 20.0);
        assert!((flat[127].freq_hz - 20_000.0).abs() < 1e-6);
        assert!(flat.iter().all(|point| point.db == 0.0));

        let mut peak = default_strip().eq;
        peak.bands[2] = EqBandSettings {
            enabled: true,
            frequency_hz: 2_000.0,
            gain_db: 6.0,
            q: 1.0,
        };
        assert!((eq_magnitude_db(&peak, 2_000.0) - 6.0).abs() < 0.01);
        assert!(eq_magnitude_db(&peak, 20.0).abs() < 0.1);
        let points = eq_response_of(&peak, 4);
        assert!((points[2].freq_hz - 2_000.0).abs() < 1e-6);
        assert!((points[2].db - 6.0).abs() < 0.01);

        let mut high_pass = default_strip().eq;
        high_pass.high_pass.enabled = true;
        high_pass.high_pass.frequency_hz = 200.0;
        assert!(
            (eq_magnitude_db(&high_pass, 200.0) + 3.01).abs() < 0.05,
            "-3 dB at the corner"
        );
        assert!(eq_magnitude_db(&high_pass, 20.0) < -30.0);

        let mut shelf = default_strip().eq;
        shelf.bands[0].gain_db = 6.0;
        assert!((eq_magnitude_db(&shelf, 20.0) - 6.0).abs() < 0.5);
        assert!(eq_magnitude_db(&shelf, 20_000.0).abs() < 0.1);

        assert_eq!(eq_response_of(&peak, 1).len(), 2);
        assert_eq!(eq_response_of(&peak, 5_000).len(), 1024);
    }

    #[test]
    fn channels_and_the_desks_answers_are_said_in_words() {
        assert_eq!(channel_list(&[3]), "CH 3");
        assert_eq!(channel_list(&[3, 4]), "CH 3 and CH 4");
        assert_eq!(channel_list(&[1, 2, 5]), "CH 1, CH 2 and CH 5");
        let set = |outcome: &'static str, message: &str| ProcessingSetDto {
            outcome,
            channels: Vec::new(),
            message: message.to_owned(),
        };
        assert_eq!(
            format_processing_set(&set("set", "CH 1 is set.")),
            Said::ok("CH 1 is set.")
        );
        assert_eq!(
            format_processing_set(&set("invalid", "Not a channel.")),
            Said::refused("Not a channel.")
        );
        assert_eq!(
            format_processing_set(&set("notFound", "")),
            Said::refused("The processing was not changed.")
        );
        assert_eq!(
            format_processing_set(&set("set", "")),
            Said::ok("The processing was changed.")
        );
        let presets = |outcome: &'static str| StripPresetsSetDto {
            outcome,
            preset: None,
            presets: Vec::new(),
            message: "m".to_owned(),
        };
        for outcome in ["saved", "replaced", "deleted"] {
            assert!(format_strip_presets_set(&presets(outcome)).ok, "{outcome}");
        }
        for outcome in ["invalid", "factory", "notFound"] {
            assert!(!format_strip_presets_set(&presets(outcome)).ok, "{outcome}");
        }
    }

    #[test]
    fn a_change_touches_the_strips_of_its_channels_and_their_pair_partners() {
        let channel = |number: u8, link: bool| Channel {
            number,
            link,
            ..Channel::default()
        };
        let table = [
            channel(1, false),
            channel(2, false),
            channel(3, true),
            channel(4, false),
        ];
        assert_eq!(strip_unit_channels(&table, &[3]), [3, 4]);
        assert_eq!(strip_unit_channels(&table, &[4]), [4, 3]);
        assert_eq!(strip_unit_channels(&table, &[1, 4]), [1, 4, 3]);
        assert_eq!(
            strip_unit_channels(&[], &[9]),
            [9],
            "a channel past the device"
        );
    }
}
