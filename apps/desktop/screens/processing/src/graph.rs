//! The equaliser graph's pure logic (`processing-view.js`): its scales, grid and curve, the
//! handles and what a drag, the wheel or a key does to them, and its text alternative.
//!
//! The graph is drawn in a box of `width` x `height` units: frequency on a logarithmic scale from
//! 20 Hz (left) to 20 kHz (right), gain from +18 dB (top) to −18 dB (bottom).

use bandear_desktop_console::format::MINUS;
use bandear_desktop_console::types::{EqPointDto, EqSettings};

use crate::strip::{EQ_HIGH_HZ, EQ_LOW_HZ, Range, eq_response_of, ranges};
use crate::words::{format_gain_db, format_hz, format_q};

/// The gain the graph shows either side of 0 dB: the bands' own range.
pub const EQ_SPAN_DB: f64 = ranges::BAND_GAIN_DB.1;

/// The graph's box, as the web console drew it (`600 x 200`, stretched to the width).
pub const GRAPH_WIDTH: f64 = 600.0;
pub const GRAPH_HEIGHT: f64 = 200.0;

/// The equaliser's bands by position, as the editor names them.
pub const BAND_NAMES: [&str; 4] = ["Low shelf", "Low peak", "High peak", "High shelf"];

/// How much one wheel notch or Page Up multiplies Q by.
pub const Q_STEP: f64 = 1.12;

fn clamp(value: f64, (low, high): Range) -> f64 {
    value.max(low).min(high)
}

fn log_span() -> f64 {
    (EQ_HIGH_HZ / EQ_LOW_HZ).ln()
}

/// The x of `freq_hz` on a graph `width` wide.
pub fn freq_to_x(freq_hz: f64, width: f64) -> f64 {
    (clamp(freq_hz, (EQ_LOW_HZ, EQ_HIGH_HZ)) / EQ_LOW_HZ).ln() / log_span() * width
}

/// The frequency at `x` on a graph `width` wide, held to 20 Hz-20 kHz.
pub fn x_to_freq(x: f64, width: f64) -> f64 {
    let fraction = clamp(x / width, (0.0, 1.0));
    if fraction == 0.0 {
        return EQ_LOW_HZ;
    }
    if fraction == 1.0 {
        return EQ_HIGH_HZ;
    }
    EQ_LOW_HZ * (fraction * log_span()).exp()
}

/// The y of `db` on a graph `height` tall, held to the scale's edges.
pub fn gain_to_y(db: f64, height: f64) -> f64 {
    let held = clamp(db, (-EQ_SPAN_DB, EQ_SPAN_DB));
    (EQ_SPAN_DB - held) / (2.0 * EQ_SPAN_DB) * height
}

/// The gain at `y` on a graph `height` tall, held to ±18 dB.
pub fn y_to_gain(y: f64, height: f64) -> f64 {
    clamp(
        EQ_SPAN_DB - y / height * 2.0 * EQ_SPAN_DB,
        (-EQ_SPAN_DB, EQ_SPAN_DB),
    )
}

/// A handle of the graph: the high-pass filter, or a band by position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    HighPass,
    Band(usize),
}

impl Target {
    /// Every handle, in their order (and the keyboard's): H, 1, 2, 3, 4.
    pub const ALL: [Target; 5] = [
        Target::HighPass,
        Target::Band(0),
        Target::Band(1),
        Target::Band(2),
        Target::Band(3),
    ];

    /// "High-pass", "Low peak".
    pub fn name(self) -> &'static str {
        match self {
            Self::HighPass => "High-pass",
            Self::Band(index) => BAND_NAMES[index],
        }
    }

    /// The handle's mark: "H", or the band's number.
    pub fn mark(self) -> String {
        match self {
            Self::HighPass => "H".to_owned(),
            Self::Band(index) => (index + 1).to_string(),
        }
    }

    fn frequency_range(self) -> Range {
        match self {
            Self::HighPass => ranges::HIGH_PASS_FREQUENCY_HZ,
            Self::Band(_) => ranges::BAND_FREQUENCY_HZ,
        }
    }

    fn q_range(self) -> Range {
        match self {
            Self::HighPass => ranges::HIGH_PASS_Q,
            Self::Band(_) => ranges::BAND_Q,
        }
    }
}

/// A handle as the graph draws it: where, whether its part is on, and its accessible name.
#[derive(Clone, Debug, PartialEq)]
pub struct Handle {
    pub target: Target,
    pub name: &'static str,
    pub x: f64,
    pub y: f64,
    pub enabled: bool,
    /// "High-pass, on, 80 Hz" or "Low peak, on, 400 Hz, +6.0 dB, Q 1.00".
    pub label: String,
}

/// The graph's handles: the high-pass filter's on the 0 dB line at its frequency, then each band's
/// at its frequency and gain.
pub fn band_handles(eq: &EqSettings, width: f64, height: f64) -> Vec<Handle> {
    let high_pass = eq.high_pass;
    let on = |enabled: bool| if enabled { "on" } else { "off" };
    let mut handles = vec![Handle {
        target: Target::HighPass,
        name: "High-pass",
        x: freq_to_x(f64::from(high_pass.frequency_hz), width),
        y: height / 2.0,
        enabled: high_pass.enabled,
        label: format!(
            "High-pass, {}, {}",
            on(high_pass.enabled),
            format_hz(f64::from(high_pass.frequency_hz))
        ),
    }];
    for (index, band) in eq.bands.iter().enumerate() {
        handles.push(Handle {
            target: Target::Band(index),
            name: BAND_NAMES[index],
            x: freq_to_x(f64::from(band.frequency_hz), width),
            y: gain_to_y(f64::from(band.gain_db), height),
            enabled: band.enabled,
            label: format!(
                "{}, {}, {}, {}, Q {}",
                BAND_NAMES[index],
                on(band.enabled),
                format_hz(f64::from(band.frequency_hz)),
                format_gain_db(f64::from(band.gain_db)),
                format_q(f64::from(band.q))
            ),
        });
    }
    handles
}

/// The part's frequency and Q, read as numbers.
fn part_of(eq: &EqSettings, target: Target) -> (bool, f64, f64, f64) {
    match target {
        Target::HighPass => (
            eq.high_pass.enabled,
            f64::from(eq.high_pass.frequency_hz),
            0.0,
            f64::from(eq.high_pass.q),
        ),
        Target::Band(index) => {
            let band = eq.bands[index];
            (
                band.enabled,
                f64::from(band.frequency_hz),
                f64::from(band.gain_db),
                f64::from(band.q),
            )
        }
    }
}

/// A change of one part of the equaliser; `None` leaves that value as it is.
#[derive(Clone, Copy, Debug, Default)]
struct Change {
    enabled: Option<bool>,
    frequency_hz: Option<f64>,
    gain_db: Option<f64>,
    q: Option<f64>,
}

/// `eq` with `change` applied to the part `target` names (a copy; `eq` is left alone).
fn changed(eq: &EqSettings, target: Target, change: Change) -> EqSettings {
    let mut next = *eq;
    match target {
        Target::HighPass => {
            let part = &mut next.high_pass;
            if let Some(enabled) = change.enabled {
                part.enabled = enabled;
            }
            if let Some(frequency) = change.frequency_hz {
                part.frequency_hz = frequency as f32;
            }
            if let Some(q) = change.q {
                part.q = q as f32;
            }
        }
        Target::Band(index) => {
            let part = &mut next.bands[index];
            if let Some(enabled) = change.enabled {
                part.enabled = enabled;
            }
            if let Some(frequency) = change.frequency_hz {
                part.frequency_hz = frequency as f32;
            }
            if let Some(gain) = change.gain_db {
                part.gain_db = gain as f32;
            }
            if let Some(q) = change.q {
                part.q = q as f32;
            }
        }
    }
    next
}

/// `eq` with the handle `target` dragged to (`x`, `y`) on a graph `width` x `height`: a band takes
/// that frequency and gain, the high-pass filter only the frequency; either is turned on.
pub fn drag_handle(
    eq: &EqSettings,
    target: Target,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> EqSettings {
    let frequency_hz = clamp(x_to_freq(x, width), target.frequency_range());
    changed(
        eq,
        target,
        Change {
            enabled: Some(true),
            frequency_hz: Some(frequency_hz),
            gain_db: matches!(target, Target::Band(_)).then(|| y_to_gain(y, height)),
            q: None,
        },
    )
}

/// `eq` with the Q of `target` multiplied by 1.12 per step (`steps` may be negative), in range.
pub fn nudge_q(eq: &EqSettings, target: Target, steps: i32) -> EqSettings {
    let (_, _, _, q) = part_of(eq, target);
    changed(
        eq,
        target,
        Change {
            q: Some(clamp(q * Q_STEP.powi(steps), target.q_range())),
            ..Change::default()
        },
    )
}

/// A key a focused handle answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleKey {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    /// Enter or Space: the part on or off.
    Toggle,
}

impl HandleKey {
    /// The key GPUI names `key` (`left`, `pageup`, `enter`, `space`...).
    pub fn from_gpui(key: &str) -> Option<Self> {
        Some(match key {
            "left" => Self::Left,
            "right" => Self::Right,
            "up" => Self::Up,
            "down" => Self::Down,
            "pageup" => Self::PageUp,
            "pagedown" => Self::PageDown,
            "home" => Self::Home,
            "enter" | "space" => Self::Toggle,
            _ => return None,
        })
    }
}

/// `eq` after `key` on the handle `target`, or `None` when the key does nothing there. Left and
/// Right move the frequency a semitone (an octave with Shift), Up and Down the gain 0.5 dB (3 dB
/// with Shift), Page Up and Page Down Q, Home puts the gain back to 0 dB, Enter and Space turn the
/// part on or off. Keys never turn a part on otherwise.
pub fn handle_key(
    eq: &EqSettings,
    target: Target,
    key: HandleKey,
    shift: bool,
) -> Option<EqSettings> {
    let is_band = matches!(target, Target::Band(_));
    let (enabled, frequency, gain, _) = part_of(eq, target);
    match key {
        HandleKey::Left | HandleKey::Right => {
            let step: f64 = if shift { 2.0 } else { 2f64.powf(1.0 / 12.0) };
            let factor = if key == HandleKey::Right {
                step
            } else {
                1.0 / step
            };
            Some(changed(
                eq,
                target,
                Change {
                    frequency_hz: Some(clamp(frequency * factor, target.frequency_range())),
                    ..Change::default()
                },
            ))
        }
        HandleKey::Up | HandleKey::Down => {
            if !is_band {
                return None;
            }
            let step =
                if shift { 3.0 } else { 0.5 } * if key == HandleKey::Up { 1.0 } else { -1.0 };
            Some(changed(
                eq,
                target,
                Change {
                    gain_db: Some(clamp(gain + step, ranges::BAND_GAIN_DB)),
                    ..Change::default()
                },
            ))
        }
        HandleKey::PageUp | HandleKey::PageDown => {
            let steps = if shift { 4 } else { 1 } * if key == HandleKey::PageUp { 1 } else { -1 };
            Some(nudge_q(eq, target, steps))
        }
        HandleKey::Home => is_band.then(|| {
            changed(
                eq,
                target,
                Change {
                    gain_db: Some(0.0),
                    ..Change::default()
                },
            )
        }),
        HandleKey::Toggle => Some(changed(
            eq,
            target,
            Change {
                enabled: Some(!enabled),
                ..Change::default()
            },
        )),
    }
}

/// The curve's points in the graph's box, left to right.
pub fn curve_points(points: &[EqPointDto], width: f64, height: f64) -> Vec<(f64, f64)> {
    points
        .iter()
        .map(|point| (freq_to_x(point.freq_hz, width), gain_to_y(point.db, height)))
        .collect()
}

/// The SVG path through `points` on a graph `width` x `height`, one decimal per number.
pub fn curve_path(points: &[EqPointDto], width: f64, height: f64) -> String {
    curve_points(points, width, height)
        .iter()
        .enumerate()
        .map(|(index, (x, y))| {
            format!(
                "{}{} {}",
                if index == 0 { "M" } else { "L" },
                bandear_desktop_console::format::to_fixed(*x, 1),
                bandear_desktop_console::format::to_fixed(*y, 1)
            )
        })
        .collect()
}

/// A grid line of the graph and its label.
#[derive(Clone, Debug, PartialEq)]
pub struct GridLine {
    /// The x of a frequency line, or the y of a gain line.
    pub at: f64,
    pub label: String,
}

const GRID_FREQS: [f64; 8] = [
    50.0, 100.0, 200.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0,
];
const GRID_GAINS: [i32; 5] = [12, 6, 0, -6, -12];

/// The graph's grid: the frequency lines ("50 Hz" ... "10 kHz") and the gain lines ("+12" ...
/// "−12"; the 0 dB line is drawn on its own, stronger, and its label left out by the view).
pub fn grid_lines(width: f64, height: f64) -> (Vec<GridLine>, Vec<GridLine>) {
    let freqs = GRID_FREQS
        .iter()
        .map(|hz| GridLine {
            at: freq_to_x(*hz, width),
            label: format_hz(*hz),
        })
        .collect();
    let gains = GRID_GAINS
        .iter()
        .map(|db| GridLine {
            at: gain_to_y(f64::from(*db), height),
            label: if *db > 0 {
                format!("+{db}")
            } else if *db < 0 {
                format!("{MINUS}{}", -db)
            } else {
                "0".to_owned()
            },
        })
        .collect();
    (freqs, gains)
}

/// The equaliser in words, the graph's text alternative.
pub fn curve_description(eq: &EqSettings) -> String {
    let high_pass = eq.high_pass;
    let mut parts = vec![
        "Equaliser response.".to_owned(),
        if high_pass.enabled {
            format!(
                "High-pass on at {}.",
                format_hz(f64::from(high_pass.frequency_hz))
            )
        } else {
            "High-pass off.".to_owned()
        },
    ];
    for (index, band) in eq.bands.iter().enumerate() {
        parts.push(if band.enabled {
            format!(
                "{} on: {}, {}, Q {}.",
                BAND_NAMES[index],
                format_hz(f64::from(band.frequency_hz)),
                format_gain_db(f64::from(band.gain_db)),
                format_q(f64::from(band.q))
            )
        } else {
            format!("{} off.", BAND_NAMES[index])
        });
    }
    parts.join(" ")
}

/// The points to draw and where they came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub points: Vec<EqPointDto>,
    pub from_desk: bool,
}

/// The points to draw for `eq`: the desk's (`desk`, worked out for the equaliser `asked`) while
/// that is still the equaliser shown, else `points` worked out here the same way, so a drag is
/// drawn at once and the desk's curve replaces it once it answers.
pub fn curve_source(
    asked: Option<&EqSettings>,
    eq: &EqSettings,
    desk: Option<&[EqPointDto]>,
    points: usize,
) -> Curve {
    match (desk, asked) {
        (Some(desk), Some(asked)) if asked == eq => Curve {
            points: desk.to_vec(),
            from_desk: true,
        },
        _ => Curve {
            points: eq_response_of(eq, points),
            from_desk: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use bandear_desktop_console::types::StripSettings;

    use super::*;

    const WIDTH: f64 = GRAPH_WIDTH;
    const HEIGHT: f64 = GRAPH_HEIGHT;

    fn near(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "{actual} is not {expected}"
        );
    }

    fn eq() -> EqSettings {
        StripSettings::default().eq
    }

    #[test]
    fn the_graphs_frequency_scale_is_logarithmic_from_20_hz_to_20_khz_and_reads_back() {
        near(freq_to_x(20.0, WIDTH), 0.0, 1e-9);
        near(freq_to_x(20_000.0, WIDTH), WIDTH, 1e-9);
        near(freq_to_x(632.4555, WIDTH), WIDTH / 2.0, 1e-3);
        near(x_to_freq(WIDTH / 2.0, WIDTH), 632.4555, 1e-3);
        near(x_to_freq(freq_to_x(2_500.0, WIDTH), WIDTH), 2_500.0, 1e-6);
        assert_eq!(x_to_freq(-50.0, WIDTH), 20.0, "held to the span");
        assert_eq!(x_to_freq(WIDTH + 50.0, WIDTH), 20_000.0);
    }

    #[test]
    fn the_graphs_gain_scale_spans_18_db_plus_18_at_the_top_and_reads_back() {
        near(gain_to_y(18.0, HEIGHT), 0.0, 1e-9);
        near(gain_to_y(0.0, HEIGHT), HEIGHT / 2.0, 1e-9);
        near(gain_to_y(-18.0, HEIGHT), HEIGHT, 1e-9);
        near(y_to_gain(gain_to_y(4.5, HEIGHT), HEIGHT), 4.5, 1e-9);
        assert_eq!(y_to_gain(-10.0, HEIGHT), 18.0);
        assert_eq!(y_to_gain(HEIGHT + 10.0, HEIGHT), -18.0);
        assert_eq!(
            gain_to_y(40.0, HEIGHT),
            0.0,
            "a curve beyond the scale at its edge"
        );
    }

    #[test]
    fn a_handle_sits_at_its_bands_frequency_and_gain_the_high_pass_one_on_the_0_db_line() {
        let mut eq = eq();
        eq.bands[1].gain_db = 6.0;
        let handles = band_handles(&eq, WIDTH, HEIGHT);
        let targets: Vec<Target> = handles.iter().map(|handle| handle.target).collect();
        assert_eq!(targets, Target::ALL);
        assert_eq!(
            BAND_NAMES,
            ["Low shelf", "Low peak", "High peak", "High shelf"]
        );
        near(handles[0].x, freq_to_x(80.0, WIDTH), 1e-9);
        near(handles[0].y, HEIGHT / 2.0, 1e-9);
        assert!(!handles[0].enabled);
        assert_eq!(handles[0].label, "High-pass, off, 80 Hz");
        near(handles[2].x, freq_to_x(400.0, WIDTH), 1e-9);
        near(handles[2].y, gain_to_y(6.0, HEIGHT), 1e-9);
        assert_eq!(handles[2].label, "Low peak, on, 400 Hz, +6.0 dB, Q 1.00");
        assert_eq!(
            (Target::HighPass.mark(), Target::Band(3).mark()),
            ("H".to_owned(), "4".to_owned())
        );
    }

    #[test]
    fn dragging_a_handle_moves_its_band_in_range_and_turns_it_on() {
        let mut eq = eq();
        eq.bands[2].enabled = false;
        let moved = drag_handle(
            &eq,
            Target::Band(2),
            freq_to_x(1_000.0, WIDTH),
            gain_to_y(-4.0, HEIGHT),
            WIDTH,
            HEIGHT,
        );
        near(f64::from(moved.bands[2].frequency_hz), 1_000.0, 1e-3);
        near(f64::from(moved.bands[2].gain_db), -4.0, 1e-5);
        assert!(moved.bands[2].enabled);
        assert!(!eq.bands[2].enabled, "the strip it came from is left alone");
        let far = drag_handle(&eq, Target::Band(0), -40.0, -40.0, WIDTH, HEIGHT);
        assert_eq!(far.bands[0].frequency_hz, 20.0);
        assert_eq!(far.bands[0].gain_db, 18.0);
        let high_pass = drag_handle(
            &eq,
            Target::HighPass,
            freq_to_x(5_000.0, WIDTH),
            0.0,
            WIDTH,
            HEIGHT,
        );
        assert_eq!(high_pass.high_pass.frequency_hz, 2_000.0, "held to 2 kHz");
        assert!(high_pass.high_pass.enabled);
        assert_eq!(
            high_pass.bands, eq.bands,
            "only frequency moves for the high-pass"
        );
    }

    #[test]
    fn the_wheel_and_the_keys_change_q_frequency_gain_and_on_off_held_in_range() {
        let eq = eq();
        let band = Target::Band(1);
        near(f64::from(nudge_q(&eq, band, 1).bands[1].q), 1.12, 1e-6);
        near(
            f64::from(nudge_q(&eq, band, -1).bands[1].q),
            1.0 / 1.12,
            1e-6,
        );
        assert_eq!(nudge_q(&eq, band, 100).bands[1].q, 10.0);
        assert_eq!(nudge_q(&eq, Target::HighPass, -100).high_pass.q, 0.5);

        let key = |eq: &EqSettings, key, shift| handle_key(eq, band, key, shift).expect("handled");
        near(
            f64::from(key(&eq, HandleKey::Right, false).bands[1].frequency_hz),
            400.0 * 2f64.powf(1.0 / 12.0),
            1e-3,
        );
        near(
            f64::from(key(&eq, HandleKey::Left, true).bands[1].frequency_hz),
            200.0,
            1e-3,
        );
        assert_eq!(key(&eq, HandleKey::Up, false).bands[1].gain_db, 0.5);
        assert_eq!(key(&eq, HandleKey::Down, true).bands[1].gain_db, -3.0);
        near(
            f64::from(key(&eq, HandleKey::PageUp, false).bands[1].q),
            1.12,
            1e-6,
        );
        near(
            f64::from(key(&eq, HandleKey::PageDown, true).bands[1].q),
            1.0 / 1.12f64.powi(4),
            1e-6,
        );
        let boosted = key(&eq, HandleKey::Up, false);
        assert_eq!(key(&boosted, HandleKey::Home, false).bands[1].gain_db, 0.0);
        assert!(!key(&eq, HandleKey::Toggle, false).bands[1].enabled);
        assert!(
            handle_key(&eq, Target::HighPass, HandleKey::Toggle, false)
                .unwrap()
                .high_pass
                .enabled
        );
        assert_eq!(
            handle_key(&eq, Target::HighPass, HandleKey::Up, false),
            None,
            "the high-pass has no gain"
        );
        assert_eq!(
            handle_key(&eq, Target::HighPass, HandleKey::Home, false),
            None
        );
        let mut off = eq;
        off.bands[1].enabled = false;
        assert!(
            !key(&off, HandleKey::Right, false).bands[1].enabled,
            "only a drag turns a part on, never a key"
        );
        assert_eq!(HandleKey::from_gpui("space"), Some(HandleKey::Toggle));
        assert_eq!(HandleKey::from_gpui("a"), None);
    }

    #[test]
    fn the_curve_is_a_path_through_every_point_drawn_inside_the_scale() {
        let points = [
            EqPointDto {
                freq_hz: 20.0,
                db: 0.0,
            },
            EqPointDto {
                freq_hz: 20_000.0,
                db: 30.0,
            },
        ];
        assert_eq!(curve_path(&points, WIDTH, HEIGHT), "M0.0 100.0L600.0 0.0");
        assert_eq!(curve_path(&[], WIDTH, HEIGHT), "");
    }

    #[test]
    fn the_graphs_grid_marks_decades_and_gains_with_their_words() {
        let (freqs, gains) = grid_lines(WIDTH, HEIGHT);
        let labels: Vec<&str> = freqs.iter().map(|line| line.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "50 Hz", "100 Hz", "200 Hz", "500 Hz", "1 kHz", "2 kHz", "5 kHz", "10 kHz"
            ]
        );
        near(freqs[1].at, freq_to_x(100.0, WIDTH), 1e-9);
        let labels: Vec<&str> = gains.iter().map(|line| line.label.as_str()).collect();
        assert_eq!(labels, ["+12", "+6", "0", "\u{2212}6", "\u{2212}12"]);
        near(gains[2].at, HEIGHT / 2.0, 1e-9);
    }

    #[test]
    fn the_curves_text_alternative_says_every_part_of_the_equaliser() {
        let mut eq = eq();
        eq.high_pass.enabled = true;
        eq.bands[1].gain_db = 3.0;
        eq.bands[3].enabled = false;
        assert_eq!(
            curve_description(&eq),
            "Equaliser response. High-pass on at 80 Hz. Low shelf on: 100 Hz, 0.0 dB, Q 0.71. \
             Low peak on: 400 Hz, +3.0 dB, Q 1.00. High peak on: 2.5 kHz, 0.0 dB, Q 1.00. \
             High shelf off."
        );
    }

    #[test]
    fn the_desks_curve_is_drawn_only_for_the_equaliser_it_was_asked_for_else_the_local_one() {
        let eq = eq();
        let desk = [EqPointDto {
            freq_hz: 20.0,
            db: 1.0,
        }];
        let source = curve_source(Some(&eq), &eq, Some(&desk), 128);
        assert_eq!(
            (source.points.as_slice(), source.from_desk),
            (&desk[..], true)
        );
        let mut changed = eq;
        changed.bands[0].gain_db = 2.0;
        let local = curve_source(Some(&eq), &changed, Some(&desk), 8);
        assert_eq!((local.points.len(), local.from_desk), (8, false));
        assert_eq!(curve_source(None, &eq, None, 4).points.len(), 4);
    }
}
