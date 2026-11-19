//! The strip editor's sliders (`processing-view.js`): each part's fields with their ranges, scales
//! and words, the 0-1,000 slider positions, the strip changed by part and field, and the
//! compressor's static curve.

use bandear_desktop_console::types::{CompressorSettings, StripSettings};

use crate::graph::BAND_NAMES;
use crate::strip::{Range, ranges};
use crate::words::{format_db, format_gain_db, format_hz, format_ms, format_q, format_ratio};

/// A part of the strip with an On switch and sliders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    HighPass,
    /// An equaliser band by position.
    Band(usize),
    Gate,
    Compressor,
}

impl Part {
    /// Every part, in the editor's order.
    pub const ALL: [Part; 7] = [
        Part::HighPass,
        Part::Band(0),
        Part::Band(1),
        Part::Band(2),
        Part::Band(3),
        Part::Gate,
        Part::Compressor,
    ];

    /// "High-pass", "Low peak", "Gate", "Compressor": the start of every control's name.
    pub fn name(self) -> &'static str {
        match self {
            Self::HighPass => "High-pass",
            Self::Band(index) => BAND_NAMES[index],
            Self::Gate => "Gate",
            Self::Compressor => "Compressor",
        }
    }

    /// The part's sliders.
    pub fn fields(self) -> &'static [Field] {
        match self {
            Self::HighPass => &HIGH_PASS_FIELDS,
            Self::Band(_) => &BAND_FIELDS,
            Self::Gate => &GATE_FIELDS,
            Self::Compressor => &COMPRESSOR_FIELDS,
        }
    }

    /// True for an equaliser part: changing it asks for the curve.
    pub fn is_eq(self) -> bool {
        matches!(self, Self::HighPass | Self::Band(_))
    }
}

/// A number of a strip part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    FrequencyHz,
    Q,
    GainDb,
    ThresholdDb,
    RangeDb,
    AttackMs,
    HoldMs,
    ReleaseMs,
    Ratio,
    KneeDb,
    MakeupDb,
}

/// How a slider's position maps to its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Linear,
    Log,
}

/// One slider of the editor.
#[derive(Clone, Copy, Debug)]
pub struct Field {
    pub key: Key,
    /// "Frequency", "Q", "Threshold"...
    pub label: &'static str,
    pub range: Range,
    pub scale: Scale,
    pub format: fn(f64) -> String,
}

const fn field(
    key: Key,
    label: &'static str,
    range: Range,
    scale: Scale,
    format: fn(f64) -> String,
) -> Field {
    Field {
        key,
        label,
        range,
        scale,
        format,
    }
}

/// The high-pass filter's sliders.
pub const HIGH_PASS_FIELDS: [Field; 2] = [
    field(
        Key::FrequencyHz,
        "Frequency",
        ranges::HIGH_PASS_FREQUENCY_HZ,
        Scale::Log,
        format_hz,
    ),
    field(Key::Q, "Q", ranges::HIGH_PASS_Q, Scale::Log, format_q),
];

/// Each band's sliders.
pub const BAND_FIELDS: [Field; 3] = [
    field(
        Key::FrequencyHz,
        "Frequency",
        ranges::BAND_FREQUENCY_HZ,
        Scale::Log,
        format_hz,
    ),
    field(
        Key::GainDb,
        "Gain",
        ranges::BAND_GAIN_DB,
        Scale::Linear,
        format_gain_db,
    ),
    field(Key::Q, "Q", ranges::BAND_Q, Scale::Log, format_q),
];

/// The gate's sliders.
pub const GATE_FIELDS: [Field; 5] = [
    field(
        Key::ThresholdDb,
        "Threshold",
        ranges::GATE_THRESHOLD_DB,
        Scale::Linear,
        format_db,
    ),
    field(
        Key::RangeDb,
        "Range",
        ranges::GATE_RANGE_DB,
        Scale::Linear,
        format_db,
    ),
    field(
        Key::AttackMs,
        "Attack",
        ranges::GATE_ATTACK_MS,
        Scale::Log,
        format_ms,
    ),
    field(
        Key::HoldMs,
        "Hold",
        ranges::GATE_HOLD_MS,
        Scale::Log,
        format_ms,
    ),
    field(
        Key::ReleaseMs,
        "Release",
        ranges::GATE_RELEASE_MS,
        Scale::Log,
        format_ms,
    ),
];

/// The compressor's sliders.
pub const COMPRESSOR_FIELDS: [Field; 6] = [
    field(
        Key::ThresholdDb,
        "Threshold",
        ranges::COMPRESSOR_THRESHOLD_DB,
        Scale::Linear,
        format_db,
    ),
    field(
        Key::Ratio,
        "Ratio",
        ranges::COMPRESSOR_RATIO,
        Scale::Log,
        format_ratio,
    ),
    field(
        Key::KneeDb,
        "Knee",
        ranges::COMPRESSOR_KNEE_DB,
        Scale::Linear,
        format_db,
    ),
    field(
        Key::AttackMs,
        "Attack",
        ranges::COMPRESSOR_ATTACK_MS,
        Scale::Log,
        format_ms,
    ),
    field(
        Key::ReleaseMs,
        "Release",
        ranges::COMPRESSOR_RELEASE_MS,
        Scale::Log,
        format_ms,
    ),
    field(
        Key::MakeupDb,
        "Makeup",
        ranges::COMPRESSOR_MAKEUP_DB,
        Scale::Linear,
        format_db,
    ),
];

/// The scales of the gain-reduction meters, in dB: the gate takes off up to its range.
pub const GATE_REDUCTION_SCALE_DB: f64 = 40.0;
pub const COMPRESSOR_REDUCTION_SCALE_DB: f64 = 20.0;

/// The steps of a slider, from 0 to this.
pub const SLIDER_STEPS: i32 = 1000;

fn clamp(value: f64, (low, high): Range) -> f64 {
    value.max(low).min(high)
}

/// A log scale needs positive ends: a range from 0 is shifted by 1 first.
fn log_offset((low, _): Range) -> f64 {
    if low <= 0.0 { 1.0 - low } else { 0.0 }
}

/// The slider position (0-1,000) of `value` for `field`, held to the range.
pub fn field_position(field: &Field, value: f64) -> i32 {
    let (low, high) = field.range;
    let held = clamp(if value.is_nan() { 0.0 } else { value }, field.range);
    let fraction = match field.scale {
        Scale::Log => {
            let offset = log_offset(field.range);
            ((held + offset) / (low + offset)).ln() / ((high + offset) / (low + offset)).ln()
        }
        Scale::Linear => (held - low) / (high - low),
    };
    bandear_desktop_console::values::js_round(fraction * f64::from(SLIDER_STEPS)) as i32
}

/// The value of `field` at slider `position` (0-1,000); the ends are exact.
pub fn field_value(field: &Field, position: i32) -> f64 {
    let (low, high) = field.range;
    let fraction = f64::from(position.clamp(0, SLIDER_STEPS)) / f64::from(SLIDER_STEPS);
    match field.scale {
        Scale::Log => {
            let offset = log_offset(field.range);
            clamp(
                (low + offset) * ((high + offset) / (low + offset)).powf(fraction) - offset,
                field.range,
            )
        }
        Scale::Linear => low + fraction * (high - low),
    }
}

/// A part's number, by key; `None` for a key the part does not have.
fn number_mut(settings: &mut StripSettings, part: Part, key: Key) -> Option<&mut f32> {
    Some(match (part, key) {
        (Part::HighPass, Key::FrequencyHz) => &mut settings.eq.high_pass.frequency_hz,
        (Part::HighPass, Key::Q) => &mut settings.eq.high_pass.q,
        (Part::Band(index), Key::FrequencyHz) => &mut settings.eq.bands[index].frequency_hz,
        (Part::Band(index), Key::GainDb) => &mut settings.eq.bands[index].gain_db,
        (Part::Band(index), Key::Q) => &mut settings.eq.bands[index].q,
        (Part::Gate, Key::ThresholdDb) => &mut settings.gate.threshold_db,
        (Part::Gate, Key::RangeDb) => &mut settings.gate.range_db,
        (Part::Gate, Key::AttackMs) => &mut settings.gate.attack_ms,
        (Part::Gate, Key::HoldMs) => &mut settings.gate.hold_ms,
        (Part::Gate, Key::ReleaseMs) => &mut settings.gate.release_ms,
        (Part::Compressor, Key::ThresholdDb) => &mut settings.compressor.threshold_db,
        (Part::Compressor, Key::Ratio) => &mut settings.compressor.ratio,
        (Part::Compressor, Key::KneeDb) => &mut settings.compressor.knee_db,
        (Part::Compressor, Key::AttackMs) => &mut settings.compressor.attack_ms,
        (Part::Compressor, Key::ReleaseMs) => &mut settings.compressor.release_ms,
        (Part::Compressor, Key::MakeupDb) => &mut settings.compressor.makeup_db,
        _ => return None,
    })
}

/// The value of `part`'s `key` in `settings` (0 for a key the part does not have).
pub fn value_of(settings: &StripSettings, part: Part, key: Key) -> f64 {
    let mut copy = *settings;
    number_mut(&mut copy, part, key).map_or(0.0, |value| f64::from(*value))
}

/// `settings` with `part`'s `key` set to `value` (a copy).
pub fn with_number(settings: &StripSettings, part: Part, key: Key, value: f64) -> StripSettings {
    let mut next = *settings;
    if let Some(number) = number_mut(&mut next, part, key) {
        *number = value as f32;
    }
    next
}

/// Whether `part` is on in `settings`.
pub fn enabled_of(settings: &StripSettings, part: Part) -> bool {
    match part {
        Part::HighPass => settings.eq.high_pass.enabled,
        Part::Band(index) => settings.eq.bands[index].enabled,
        Part::Gate => settings.gate.enabled,
        Part::Compressor => settings.compressor.enabled,
    }
}

/// `settings` with `part` turned on or off (a copy).
pub fn with_enabled(settings: &StripSettings, part: Part, on: bool) -> StripSettings {
    let mut next = *settings;
    match part {
        Part::HighPass => next.eq.high_pass.enabled = on,
        Part::Band(index) => next.eq.bands[index].enabled = on,
        Part::Gate => next.gate.enabled = on,
        Part::Compressor => next.compressor.enabled = on,
    }
    next
}

/// The compressor's static curve: the output level (dB) for an input level `input_db`, with a soft
/// knee around the threshold (as the desk's compressor), plus the makeup gain.
pub fn compressor_output_db(compressor: &CompressorSettings, input_db: f64) -> f64 {
    let threshold = f64::from(compressor.threshold_db);
    let ratio = f64::from(compressor.ratio);
    let knee = f64::from(compressor.knee_db);
    let over = input_db - threshold;
    let output = if 2.0 * over < -knee {
        input_db
    } else if knee > 0.0 && 2.0 * over.abs() <= knee {
        input_db + (1.0 / ratio - 1.0) * (over + knee / 2.0).powi(2) / (2.0 * knee)
    } else {
        threshold + over / ratio
    };
    output + f64::from(compressor.makeup_db)
}

/// The lowest level the transfer curve shows, on both axes.
const TRANSFER_FLOOR_DB: f64 = -60.0;

/// The compressor's static curve in a `size` x `size` box, −60 to 0 dB each way: `steps + 1`
/// points, left to right.
pub fn transfer_points(
    compressor: &CompressorSettings,
    size: f64,
    steps: usize,
) -> Vec<(f64, f64)> {
    let to_y = |db: f64| clamp(db, (TRANSFER_FLOOR_DB, 0.0)) / TRANSFER_FLOOR_DB * size;
    (0..=steps)
        .map(|index| {
            let along = index as f64 / steps as f64;
            let input_db = TRANSFER_FLOOR_DB * (1.0 - along);
            (
                along * size,
                to_y(compressor_output_db(compressor, input_db)),
            )
        })
        .collect()
}

/// The transfer curve as an SVG path, one decimal per number.
pub fn transfer_path(compressor: &CompressorSettings, size: f64, steps: usize) -> String {
    use bandear_desktop_console::format::to_fixed;
    transfer_points(compressor, size, steps)
        .iter()
        .enumerate()
        .map(|(index, (x, y))| {
            format!(
                "{}{} {}",
                if index == 0 { "M" } else { "L" },
                to_fixed(*x, 1),
                to_fixed(*y, 1)
            )
        })
        .collect()
}

/// The transfer curve's accessible name.
pub fn transfer_label(compressor: &CompressorSettings) -> String {
    format!(
        "Compressor curve: threshold {}, ratio {}, knee {}, makeup {}",
        format_db(f64::from(compressor.threshold_db)),
        format_ratio(f64::from(compressor.ratio)),
        format_db(f64::from(compressor.knee_db)),
        format_db(f64::from(compressor.makeup_db))
    )
}

#[cfg(test)]
mod tests {
    use bandear_desktop_console::format::reduction_view;

    use super::*;
    use crate::strip::default_strip;

    fn near(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "{actual} is not {expected}"
        );
    }

    #[test]
    fn a_slider_moves_along_a_fields_scale_linear_or_logarithmic_for_times() {
        let threshold = COMPRESSOR_FIELDS[0];
        assert_eq!(field_position(&threshold, -30.0), 500);
        assert_eq!(field_value(&threshold, 250), -45.0);
        let attack = COMPRESSOR_FIELDS[3];
        assert_eq!(field_position(&attack, 0.1), 0);
        assert_eq!(field_position(&attack, 200.0), 1000);
        near(
            field_value(&attack, field_position(&attack, 10.0)),
            10.0,
            0.1,
        );
        assert_eq!(field_position(&attack, 1e6), 1000, "held to the scale");
        let hold = GATE_FIELDS[3];
        assert_eq!(
            field_value(&hold, 0),
            0.0,
            "a scale that starts at 0 still reaches it"
        );
        near(field_value(&hold, field_position(&hold, 50.0)), 50.0, 0.5);
    }

    #[test]
    fn a_setting_is_changed_on_a_copy_of_the_strip_by_its_part_and_key() {
        let strip = default_strip();
        let next = with_number(&strip, Part::Band(1), Key::GainDb, 4.0);
        assert_eq!(next.eq.bands[1].gain_db, 4.0);
        assert_eq!(strip.eq.bands[1].gain_db, 0.0);
        assert!(
            with_enabled(&strip, Part::Compressor, true)
                .compressor
                .enabled
        );
        assert_eq!(value_of(&next, Part::Band(1), Key::GainDb), 4.0);
        assert_eq!(
            value_of(&strip, Part::Gate, Key::Ratio),
            0.0,
            "the gate has no ratio"
        );
        assert!(!enabled_of(&strip, Part::HighPass));
    }

    #[test]
    fn the_compressors_static_curve_straight_below_the_knee_the_ratio_above_makeup_on_top() {
        let compressor = CompressorSettings {
            threshold_db: -20.0,
            ratio: 4.0,
            knee_db: 0.0,
            makeup_db: 0.0,
            ..CompressorSettings::default()
        };
        assert_eq!(compressor_output_db(&compressor, -40.0), -40.0);
        assert_eq!(compressor_output_db(&compressor, 0.0), -15.0);
        let makeup = CompressorSettings {
            makeup_db: 5.0,
            ..compressor
        };
        assert_eq!(compressor_output_db(&makeup, 0.0), -10.0);
        let soft = CompressorSettings {
            knee_db: 10.0,
            ..compressor
        };
        assert_eq!(compressor_output_db(&soft, -26.0), -26.0, "below the knee");
        near(
            compressor_output_db(&soft, -20.0),
            -20.0 + (0.25 - 1.0) * 25.0 / 20.0,
            1e-9,
        );
        assert_eq!(compressor_output_db(&soft, -10.0), -17.5, "above the knee");
        let path = transfer_path(&compressor, 60.0, 60);
        assert!(path.starts_with("M0.0 60.0"), "{path}");
        assert!(path.ends_with("L60.0 15.0"), "{path}");
        assert_eq!(
            transfer_label(&default_strip().compressor),
            "Compressor curve: threshold \u{2212}18 dB, ratio 3:1, knee 6 dB, makeup 0 dB"
        );
    }

    #[test]
    fn a_gain_reduction_meter_fills_in_proportion_to_what_is_taken_off() {
        assert_eq!(
            reduction_view(6.0, 24.0),
            (0.25, "\u{2212}6.0 dB".to_owned())
        );
        assert_eq!(reduction_view(0.01, 24.0), (0.0, "0.0 dB".to_owned()));
        assert_eq!(
            reduction_view(90.0, 24.0),
            (1.0, "\u{2212}90.0 dB".to_owned())
        );
        assert_eq!(reduction_view(f64::NAN, 24.0), (0.0, "0.0 dB".to_owned()));
    }

    #[test]
    fn every_slider_of_the_editor_covers_its_parts_whole_range_and_says_its_value() {
        let strip = default_strip();
        let expected = [
            (
                Part::HighPass,
                vec![ranges::HIGH_PASS_FREQUENCY_HZ, ranges::HIGH_PASS_Q],
            ),
            (
                Part::Band(1),
                vec![
                    ranges::BAND_FREQUENCY_HZ,
                    ranges::BAND_GAIN_DB,
                    ranges::BAND_Q,
                ],
            ),
            (
                Part::Gate,
                vec![
                    ranges::GATE_THRESHOLD_DB,
                    ranges::GATE_RANGE_DB,
                    ranges::GATE_ATTACK_MS,
                    ranges::GATE_HOLD_MS,
                    ranges::GATE_RELEASE_MS,
                ],
            ),
            (
                Part::Compressor,
                vec![
                    ranges::COMPRESSOR_THRESHOLD_DB,
                    ranges::COMPRESSOR_RATIO,
                    ranges::COMPRESSOR_KNEE_DB,
                    ranges::COMPRESSOR_ATTACK_MS,
                    ranges::COMPRESSOR_RELEASE_MS,
                    ranges::COMPRESSOR_MAKEUP_DB,
                ],
            ),
        ];
        for (part, ranges) in expected {
            let fields = part.fields();
            assert_eq!(fields.len(), ranges.len(), "{}", part.name());
            for (field, range) in fields.iter().zip(ranges) {
                assert_eq!(field.range, range, "{} {}", part.name(), field.label);
                assert_eq!(field_value(field, 0), range.0);
                near(field_value(field, 1000), range.1, 1e-9);
                let value = value_of(&strip, part, field.key);
                assert!(
                    (range.0..=range.1).contains(&value),
                    "{} {} is a number of the part",
                    part.name(),
                    field.label
                );
                assert!(!(field.format)(value).is_empty());
            }
        }
        assert_eq!((COMPRESSOR_FIELDS[1].format)(4.0), "4:1");
        let labels: Vec<&str> = Part::ALL.iter().map(|part| part.name()).collect();
        assert_eq!(
            labels,
            [
                "High-pass",
                "Low shelf",
                "Low peak",
                "High peak",
                "High shelf",
                "Gate",
                "Compressor"
            ]
        );
    }
}
