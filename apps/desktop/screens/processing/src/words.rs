//! The words every processing value is shown in (`processing-view.js`): frequencies, gains, levels,
//! Q, times and ratios, with a real minus sign and never "−0".

use bandear_desktop_console::format::{MINUS, to_fixed};
use bandear_desktop_console::values::js_round;

/// JavaScript's `String(Number(x))` for a finite number: the shortest text that reads back, and
/// "0" for minus zero.
fn number_text(value: f64) -> String {
    if value == 0.0 {
        "0".to_owned()
    } else {
        format!("{value}")
    }
}

/// A number with at most `digits` decimals, trailing zeros dropped (`String(Number(v.toFixed(d)))`).
fn trimmed(value: f64, digits: usize) -> String {
    number_text(to_fixed(value, digits).parse::<f64>().unwrap_or(0.0))
}

/// `text` with a real minus sign in place of a leading hyphen.
fn signed(text: &str) -> String {
    match text.strip_prefix('-') {
        Some(rest) => format!("{MINUS}{rest}"),
        None => text.to_owned(),
    }
}

/// A value that is not a number reads as 0, as `Number(x) || 0` does.
fn or_zero(value: f64) -> f64 {
    if value.is_nan() { 0.0 } else { value }
}

/// "80 Hz", "2.5 kHz", "1 kHz" (99.6 is "100 Hz").
pub fn format_hz(hz: f64) -> String {
    let value = or_zero(hz);
    let rounded = js_round(value);
    if rounded < 1_000.0 {
        return format!("{} Hz", number_text(rounded));
    }
    format!("{} kHz", trimmed(value / 1_000.0, 1))
}

/// An equaliser gain with one decimal and its sign: "+3.0 dB", "−2.0 dB", "0.0 dB".
pub fn format_gain_db(db: f64) -> String {
    let text = to_fixed(or_zero(db), 1);
    let value: f64 = text.parse().unwrap_or(0.0);
    if value == 0.0 {
        "0.0 dB".to_owned()
    } else if value > 0.0 {
        format!("+{text} dB")
    } else {
        format!("{} dB", signed(&text))
    }
}

/// A level or amount in dB with up to one decimal: "−18 dB", "−2.5 dB", "6 dB", "0 dB".
pub fn format_db(db: f64) -> String {
    let text = trimmed(or_zero(db), 1);
    format!("{} dB", signed(&text))
}

/// A filter's Q: "0.71", "10.0".
pub fn format_q(q: f64) -> String {
    let value = or_zero(q);
    if value >= 10.0 {
        to_fixed(value, 1)
    } else {
        to_fixed(value, 2)
    }
}

/// A time: "0.5 ms", "0.05 ms", "150 ms", "1.5 s".
pub fn format_ms(ms: f64) -> String {
    let value = or_zero(ms);
    if value >= 1_000.0 {
        return format!("{} s", trimmed(value / 1_000.0, 2));
    }
    if value < 10.0 {
        format!("{} ms", trimmed(value, 2))
    } else {
        format!("{} ms", number_text(js_round(value)))
    }
}

/// A compressor ratio: "3:1", "2.5:1".
pub fn format_ratio(ratio: f64) -> String {
    let value = if ratio.is_nan() || ratio == 0.0 {
        1.0
    } else {
        ratio
    };
    format!("{}:1", trimmed(value, 1))
}

/// An effect send: "Off" at 0, else "20%".
pub fn format_send(level: u8) -> String {
    if level == 0 {
        "Off".to_owned()
    } else {
        format!("{level}%")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_written_in_words_the_engineer_reads() {
        assert_eq!(format_hz(80.0), "80 Hz");
        assert_eq!(format_hz(99.6), "100 Hz");
        assert_eq!(format_hz(2_500.0), "2.5 kHz");
        assert_eq!(format_hz(1_000.0), "1 kHz");
        assert_eq!(format_hz(12_500.0), "12.5 kHz");
        assert_eq!(format_gain_db(3.0), "+3.0 dB");
        assert_eq!(format_gain_db(-2.0), "\u{2212}2.0 dB");
        assert_eq!(format_gain_db(0.0), "0.0 dB");
        assert_eq!(format_gain_db(-0.01), "0.0 dB", "no minus zero");
        assert_eq!(format_db(-18.0), "\u{2212}18 dB");
        assert_eq!(format_db(-2.5), "\u{2212}2.5 dB");
        assert_eq!(format_db(6.0), "6 dB");
        assert_eq!(format_db(-0.01), "0 dB");
        assert_eq!(format_q(0.7071), "0.71");
        assert_eq!(format_q(10.0), "10.0");
        assert_eq!(format_ms(0.5), "0.5 ms");
        assert_eq!(format_ms(0.05), "0.05 ms");
        assert_eq!(format_ms(150.0), "150 ms");
        assert_eq!(format_ms(1_500.0), "1.5 s");
        assert_eq!(format_ratio(3.0), "3:1");
        assert_eq!(format_ratio(2.5), "2.5:1");
        assert_eq!(format_send(0), "Off");
        assert_eq!(format_send(20), "20%");
    }

    #[test]
    fn the_strip_settings_read_in_single_precision_still_say_their_words() {
        assert_eq!(format_q(f64::from(std::f32::consts::FRAC_1_SQRT_2)), "0.71");
        assert_eq!(format_ms(f64::from(0.05_f32)), "0.05 ms");
        assert_eq!(format_hz(f64::from(99.6_f32)), "100 Hz");
        assert_eq!(format_gain_db(f64::from(-2.05_f32)), "\u{2212}2.0 dB");
    }
}
