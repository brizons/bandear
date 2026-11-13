//! The console's readouts and sentences, word for word as the web console wrote them
//! (`format.js`): U+2212 for minus, ` · ` between parts, one full stop after a reason.

use crate::values::{clamp_level, js_round};
use crate::wire::{CaptureFormat, Musician};

/// The minus sign of every readout (U+2212), never a hyphen.
pub const MINUS: &str = "\u{2212}";

/// What separates the parts of a readout.
pub const SEPARATOR: &str = " \u{b7} ";

/// `lead.` or `lead: reason.`: the reason trimmed, its trailing full stops dropped, so a sentence
/// ends with one whatever the reason ends with; an empty reason gives the lead alone.
pub fn sentence_with_reason(lead: &str, reason: Option<&str>) -> String {
    let text = reason.unwrap_or("").trim().trim_end_matches('.');
    if text.is_empty() {
        format!("{lead}.")
    } else {
        format!("{lead}: {text}.")
    }
}

/// What a `set_groups` answer means for the phones.
pub fn format_groups_set(applied: bool, saved: bool, save_error: Option<&str>) -> String {
    if !applied {
        return "Groups saved; they apply at the next start.".to_owned();
    }
    if saved {
        "Groups applied to every phone.".to_owned()
    } else {
        sentence_with_reason("Groups applied to every phone, but not saved", save_error)
    }
}

/// What a `set_channels` answer means for the phones.
pub fn format_channels_set(applied: bool, saved: bool, save_error: Option<&str>) -> String {
    if !applied {
        return "Channel details saved; they apply at the next start.".to_owned();
    }
    if saved {
        "Channel details applied to every phone.".to_owned()
    } else {
        sentence_with_reason(
            "Channel details applied to every phone, but not saved",
            save_error,
        )
    }
}

/// What a `set_engineer_controls` answer means, for the musician called `name` when given.
pub fn format_engineer_controls_set(
    applied: bool,
    saved: bool,
    save_error: Option<&str>,
    name: Option<&str>,
) -> String {
    let subject = match name {
        Some(name) if !name.is_empty() => format!("{name}'s limit and locks"),
        _ => "Limit and locks".to_owned(),
    };
    if !applied {
        return format!("{subject} saved; they apply at the next start.");
    }
    if saved {
        format!("{subject} apply now and are saved.")
    } else {
        sentence_with_reason(
            &format!("{subject} apply now, but were not saved"),
            save_error,
        )
    }
}

/// A level as a percent (`85%`).
pub fn format_percent(level: f64) -> String {
    format!("{}%", clamp_level(level))
}

/// Decibels below full scale, whole (`−6 dBFS`, `0 dBFS`, `−∞ dBFS`).
pub fn format_db(db: f64) -> String {
    if db == f64::NEG_INFINITY {
        return format!("{MINUS}\u{221e} dBFS");
    }
    let rounded = js_round(db);
    if rounded < 0.0 {
        format!("{MINUS}{} dBFS", -rounded)
    } else {
        // `abs` turns a rounded -0 into 0.
        format!("{} dBFS", rounded.abs())
    }
}

/// A sample rate in kHz with at most one decimal (`48 kHz`, `44.1 kHz`).
pub fn format_sample_rate(hertz: f64) -> String {
    // `parseFloat(x.toFixed(1))`: one decimal, and none when it is a zero.
    let fixed = to_fixed(hertz / 1000.0, 1);
    let trimmed = fixed.strip_suffix(".0").unwrap_or(&fixed);
    format!("{trimmed} kHz")
}

/// A sample format in words (`I24` is `24-bit`, `F32` is `32-bit float`); one the console does not
/// know is shown as it came.
pub fn format_sample_format(format: &str) -> String {
    let mut chars = format.chars();
    let kind = chars.next();
    let bits = chars.as_str();
    if bits.is_empty() || !bits.bytes().all(|byte| byte.is_ascii_digit()) {
        return format.to_owned();
    }
    match kind {
        Some('I') => format!("{bits}-bit"),
        Some('U') => format!("{bits}-bit unsigned"),
        Some('F') => format!("{bits}-bit float"),
        _ => format.to_owned(),
    }
}

/// A buffer size (`Fixed(256)` is `256`, `device default` is `default buffer`).
pub fn format_buffer(buffer_size: &str) -> String {
    if let Some(frames) = buffer_size
        .strip_prefix("Fixed(")
        .and_then(|rest| rest.strip_suffix(')'))
        .filter(|frames| !frames.is_empty() && frames.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return frames.to_owned();
    }
    if buffer_size == "device default" {
        "default buffer".to_owned()
    } else {
        buffer_size.to_owned()
    }
}

/// The capture format as the top bar reads it: `48 kHz · 24-bit · 256`.
pub fn format_capture_format(format: &CaptureFormat) -> String {
    [
        format_sample_rate(f64::from(format.sample_rate)),
        format_sample_format(&format.sample_format),
        format_buffer(&format.buffer_size),
    ]
    .join(SEPARATOR)
}

/// How a phone's link is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkLevel {
    Good,
    Warning,
    Bad,
    Unknown,
}

impl LinkLevel {
    /// The level's name (`good`, `warning`, `bad`, `unknown`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Warning => "warning",
            Self::Bad => "bad",
            Self::Unknown => "unknown",
        }
    }
}

/// A phone's link in words: its level, a short label and the detail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkState {
    pub level: LinkLevel,
    pub label: String,
    pub detail: String,
}

/// The link of `musician` in words, from its health and its reported rates.
pub fn link_state(musician: &Musician) -> LinkState {
    let level = match musician.link_health.as_str() {
        "good" => LinkLevel::Good,
        "warning" => LinkLevel::Warning,
        "bad" => LinkLevel::Bad,
        _ => LinkLevel::Unknown,
    };
    let state = |level, label: &str, detail: &str| LinkState {
        level,
        label: label.to_owned(),
        detail: detail.to_owned(),
    };
    let (Some(glitches), Some(loss), Some(timeouts)) = (
        musician.underruns_per_second,
        musician.loss_percent,
        musician.receive_timeouts_per_second,
    ) else {
        if level == LinkLevel::Bad {
            return state(level, "Link lost", "Reports from this phone stopped");
        }
        if !musician.control_connected {
            return state(
                LinkLevel::Unknown,
                "Not connected",
                "The app on this phone is not connected",
            );
        }
        return state(
            LinkLevel::Unknown,
            "No reports",
            "No reports from this phone yet",
        );
    };
    if level == LinkLevel::Bad && musician.packets_per_second == Some(0.0) {
        return state(level, "No audio", "This phone receives no audio");
    }
    let label = match level {
        LinkLevel::Good => "Good",
        LinkLevel::Warning => "Warning",
        LinkLevel::Bad => "Poor",
        LinkLevel::Unknown => "Unknown",
    };
    let detail = [
        format!("Loss {}%", to_fixed(loss, 2)),
        format!("Glitches {}/s", to_fixed(glitches, 1)),
        format!("Timeouts {}/s", to_fixed(timeouts, 1)),
    ]
    .join(SEPARATOR);
    state(level, label, &detail)
}

/// True when `musician`'s link is a warning or bad.
pub fn needs_attention(musician: &Musician) -> bool {
    matches!(
        link_state(musician).level,
        LinkLevel::Warning | LinkLevel::Bad
    )
}

/// What the console calls a musician: their name, else the phone's IP without its port.
pub fn musician_label(name: Option<&str>, address: &str) -> String {
    if let Some(name) = name.filter(|name| !name.is_empty()) {
        return name.to_owned();
    }
    if let Some(bracketed) = address.strip_prefix('[') {
        return bracketed.split(']').next().unwrap_or(bracketed).to_owned();
    }
    match address.rfind(':') {
        Some(colon) => address[..colon].to_owned(),
        None => address.to_owned(),
    }
}

/// A dynamics' gain reduction as its meter shows it (`reductionView`): how full the bar is over
/// `scale_db` (0 to 1), and the readout (`0.0 dB`, `−3.2 dB`); under 0.05 dB is none.
pub fn reduction_view(db: f64, scale_db: f64) -> (f64, String) {
    let value = if db.is_finite() && db >= 0.05 {
        db
    } else {
        0.0
    };
    let fraction = if scale_db > 0.0 {
        (value / scale_db).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let text = if value == 0.0 {
        "0.0 dB".to_owned()
    } else {
        format!("{MINUS}{} dB", to_fixed(value, 1))
    };
    (fraction, text)
}

/// JavaScript's `Number.prototype.toFixed`: `digits` decimals, a value exactly halfway rounded to
/// the larger one (Rust's formatting rounds such a value to even).
pub fn to_fixed(value: f64, digits: usize) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    // Rust prints the exact binary value correctly rounded, so 25 more digits show whether it is
    // exactly halfway; the last step rounds that half up, as JavaScript does.
    let wide = format!("{:.*}", digits + 25, value.abs());
    let (whole, fraction) = wide.split_once('.').unwrap_or((&wide, ""));
    let mut kept: Vec<u8> = whole.bytes().chain(fraction.bytes().take(digits)).collect();
    let rest = &fraction[digits.min(fraction.len())..];
    if rest.as_bytes().first().is_some_and(|digit| *digit >= b'5') {
        let mut index = kept.len();
        loop {
            if index == 0 {
                kept.insert(0, b'1');
                break;
            }
            index -= 1;
            if kept[index] == b'9' {
                kept[index] = b'0';
            } else {
                kept[index] += 1;
                break;
            }
        }
    }
    let digits_text = String::from_utf8(kept).unwrap_or_default();
    let split = digits_text.len() - digits;
    let (whole, fraction) = digits_text.split_at(split);
    // As JavaScript: any value below zero keeps its sign, even one that rounds to zero.
    let sign = if value < 0.0 { "-" } else { "" };
    if digits == 0 {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{fraction}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_group_edit_says_whether_the_phones_have_it_now_later_or_not_kept() {
        assert_eq!(
            format_groups_set(true, true, None),
            "Groups applied to every phone."
        );
        assert_eq!(
            format_groups_set(false, true, None),
            "Groups saved; they apply at the next start."
        );
        assert_eq!(
            format_groups_set(true, false, Some("the disk is full")),
            "Groups applied to every phone, but not saved: the disk is full."
        );
        assert_eq!(
            format_groups_set(true, false, Some("the disk is full..")),
            "Groups applied to every phone, but not saved: the disk is full.",
            "one full stop, whatever the reason ends with"
        );
        for save_error in [None, Some(""), Some("  ")] {
            assert_eq!(
                format_groups_set(true, false, save_error),
                "Groups applied to every phone, but not saved.",
                "no reason to give: {save_error:?}"
            );
        }
    }

    #[test]
    fn a_channel_edit_says_whether_the_phones_have_it_now_later_or_not_kept() {
        assert_eq!(
            format_channels_set(true, true, None),
            "Channel details applied to every phone."
        );
        assert_eq!(
            format_channels_set(false, true, None),
            "Channel details saved; they apply at the next start."
        );
        assert_eq!(
            format_channels_set(true, false, Some("the disk is full.")),
            "Channel details applied to every phone, but not saved: the disk is full."
        );
        for save_error in [None, Some(""), Some("  ")] {
            assert_eq!(
                format_channels_set(true, false, save_error),
                "Channel details applied to every phone, but not saved."
            );
        }
    }

    #[test]
    fn the_engineers_controls_say_whether_they_apply_and_were_kept() {
        assert_eq!(
            format_engineer_controls_set(true, true, None, Some("Bass")),
            "Bass's limit and locks apply now and are saved."
        );
        assert_eq!(
            format_engineer_controls_set(false, true, None, None),
            "Limit and locks saved; they apply at the next start."
        );
        assert_eq!(
            format_engineer_controls_set(true, false, Some("the disk is full"), None),
            "Limit and locks apply now, but were not saved: the disk is full."
        );
    }

    fn reporting(extra: serde_json::Value) -> Musician {
        let mut row = json!({
            "address": "192.168.1.3:50000",
            "controlConnected": true,
            "linkHealth": "good",
            "packetsPerSecond": 200,
            "underrunsPerSecond": 0,
            "lossPercent": 0,
            "receiveTimeoutsPerSecond": 0,
        });
        for (key, value) in extra.as_object().expect("an object") {
            row[key] = value.clone();
        }
        serde_json::from_value(row).expect("a musician")
    }

    fn no_report() -> serde_json::Value {
        json!({
            "packetsPerSecond": null,
            "underrunsPerSecond": null,
            "lossPercent": null,
            "receiveTimeoutsPerSecond": null,
        })
    }

    fn merged(a: serde_json::Value, b: serde_json::Value) -> serde_json::Value {
        let mut a = a;
        for (key, value) in b.as_object().expect("an object") {
            a[key] = value.clone();
        }
        a
    }

    #[test]
    fn percent_and_decibel_readouts() {
        assert_eq!(format_percent(85.0), "85%");
        assert_eq!(format_percent(62.4), "62%");
        assert_eq!(format_db(-6.02), format!("{MINUS}6 dBFS"));
        assert_eq!(format_db(0.0), "0 dBFS");
        assert_eq!(format_db(-0.3), "0 dBFS");
        assert_eq!(
            format_db(-6.5),
            format!("{MINUS}6 dBFS"),
            "halves round up, as Math.round"
        );
        assert_eq!(
            format_db(f64::NEG_INFINITY),
            format!("{MINUS}\u{221e} dBFS")
        );
    }

    #[test]
    fn the_capture_format_reads_like_the_designs_top_bar() {
        assert_eq!(format_sample_rate(48000.0), "48 kHz");
        assert_eq!(format_sample_rate(44100.0), "44.1 kHz");
        assert_eq!(format_sample_rate(96000.0), "96 kHz");
        assert_eq!(format_sample_format("I16"), "16-bit");
        assert_eq!(format_sample_format("I24"), "24-bit");
        assert_eq!(format_sample_format("F32"), "32-bit float");
        assert_eq!(format_sample_format("U8"), "8-bit unsigned");
        assert_eq!(
            format_sample_format("DSD"),
            "DSD",
            "an unknown format is shown as it came"
        );
        assert_eq!(format_buffer("Fixed(256)"), "256");
        assert_eq!(format_buffer("device default"), "default buffer");
        assert_eq!(
            format_capture_format(&CaptureFormat {
                channels: 4,
                sample_rate: 48000,
                sample_format: "I24".to_owned(),
                buffer_size: "Fixed(256)".to_owned(),
            }),
            "48 kHz \u{b7} 24-bit \u{b7} 256"
        );
    }

    #[test]
    fn a_reporting_phones_link_reads_good_warning_or_poor_with_its_numbers() {
        assert_eq!(
            link_state(&reporting(json!({}))),
            LinkState {
                level: LinkLevel::Good,
                label: "Good".to_owned(),
                detail: "Loss 0.00% \u{b7} Glitches 0.0/s \u{b7} Timeouts 0.0/s".to_owned(),
            }
        );
        let warning = link_state(&reporting(
            json!({ "linkHealth": "warning", "lossPercent": 0.82, "underrunsPerSecond": 2 }),
        ));
        assert_eq!(warning.level, LinkLevel::Warning);
        assert_eq!(warning.label, "Warning");
        assert_eq!(
            warning.detail,
            "Loss 0.82% \u{b7} Glitches 2.0/s \u{b7} Timeouts 0.0/s"
        );
        assert_eq!(
            link_state(&reporting(
                json!({ "linkHealth": "bad", "packetsPerSecond": 150 })
            ))
            .label,
            "Poor"
        );
    }

    #[test]
    fn a_bad_link_without_reports_is_lost_and_one_that_hears_nothing_has_no_audio() {
        let lost = link_state(&reporting(merged(
            json!({ "linkHealth": "bad" }),
            no_report(),
        )));
        assert_eq!(lost.level, LinkLevel::Bad);
        assert_eq!(lost.label, "Link lost");
        let silent = link_state(&reporting(
            json!({ "linkHealth": "bad", "packetsPerSecond": 0 }),
        ));
        assert_eq!(silent.level, LinkLevel::Bad);
        assert_eq!(silent.label, "No audio");
    }

    #[test]
    fn a_phone_with_no_reports_yet_says_whether_its_app_is_connected() {
        assert_eq!(
            link_state(&reporting(merged(
                json!({ "linkHealth": "unknown" }),
                no_report()
            )))
            .label,
            "No reports"
        );
        assert_eq!(
            link_state(&reporting(merged(
                json!({ "linkHealth": "unknown", "controlConnected": false }),
                no_report()
            )))
            .label,
            "Not connected"
        );
        assert_eq!(
            link_state(&reporting(merged(
                json!({ "linkHealth": "sideways" }),
                no_report()
            )))
            .level,
            LinkLevel::Unknown,
            "a health the window does not know reads as unknown"
        );
    }

    #[test]
    fn only_a_warning_or_bad_link_needs_attention() {
        assert!(!needs_attention(&reporting(json!({}))));
        assert!(needs_attention(&reporting(
            json!({ "linkHealth": "warning" })
        )));
        assert!(needs_attention(&reporting(merged(
            json!({ "linkHealth": "bad" }),
            no_report()
        ))));
        assert!(!needs_attention(&reporting(merged(
            json!({ "linkHealth": "unknown" }),
            no_report()
        ))));
    }

    #[test]
    fn a_musician_is_called_by_name_or_by_the_phones_ip_when_there_is_none() {
        assert_eq!(musician_label(Some("Bass"), "192.168.1.12:50000"), "Bass");
        assert_eq!(musician_label(None, "192.168.1.12:50000"), "192.168.1.12");
        assert_eq!(musician_label(Some(""), "[fe80::1]:50000"), "fe80::1");
    }

    #[test]
    fn a_reduction_reads_in_tenths_of_a_db_below_zero() {
        assert_eq!(reduction_view(0.0, 20.0), (0.0, "0.0 dB".to_owned()));
        assert_eq!(
            reduction_view(0.04, 20.0),
            (0.0, "0.0 dB".to_owned()),
            "under 0.05 dB is none"
        );
        assert_eq!(reduction_view(f64::NAN, 20.0), (0.0, "0.0 dB".to_owned()));
        let (fraction, text) = reduction_view(3.2, 20.0);
        assert!((fraction - 0.16).abs() < 1e-9);
        assert_eq!(text, format!("{MINUS}3.2 dB"));
        assert_eq!(reduction_view(40.0, 20.0).0, 1.0, "a full bar at most");
    }

    #[test]
    fn to_fixed_rounds_halves_up_as_javascript_does() {
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(2.0, 1), "2.0");
        assert_eq!(to_fixed(0.82, 2), "0.82");
        assert_eq!(to_fixed(44.1, 1), "44.1");
    }
}
