//! The numbers behind the console's controls: levels, pans, meters and group tones, as the web
//! console worked them out (`values.js`). Pure functions, no window.
//!
//! Levels are whole numbers from 0 to 100. Every rounding here is JavaScript's `Math.round`
//! ([js_round]: halves go toward positive infinity), never [f64::round], so a readout shows what the
//! web console showed.

/// The steps of a slider: its ends, its key steps, where a double-click puts it, and for a pan its
/// centre and the detent a drag snaps to it within.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub min: i32,
    pub max: i32,
    /// What an arrow key moves.
    pub step: i32,
    /// What Page Up and Page Down move.
    pub page: i32,
    /// Where a double-click puts the slider.
    pub reset: i32,
    /// A centre a key move never crosses in one go (a pan's), if any.
    pub centre: Option<i32>,
    /// How near the centre a dragged value snaps to it.
    pub detent: i32,
}

/// A fader: 0-100, unity at the top, a double-click back to unity.
pub const FADER: Range = Range {
    min: 0,
    max: 100,
    step: 1,
    page: 10,
    reset: 100,
    centre: None,
    detent: 0,
};

/// A pan: 0 hard left, 50 the centre, 100 hard right; a drag within 2 of the centre lands on it.
pub const PAN: Range = Range {
    min: 0,
    max: 100,
    step: 1,
    page: 10,
    reset: 50,
    centre: Some(50),
    detent: 2,
};

/// The quietest level a meter shows, in dBFS: a meter spans 60 dB.
pub const METER_FLOOR_DB: f64 = -60.0;

/// From here up to full scale a meter is hot.
pub const HOT_DB: f64 = -6.0;

/// The tone of a group's chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupTone {
    Teal,
    Blue,
    Violet,
    Orange,
    /// No group.
    None,
}

/// The tones groups take in turn.
pub const GROUP_TONES: [GroupTone; 4] = [
    GroupTone::Teal,
    GroupTone::Blue,
    GroupTone::Violet,
    GroupTone::Orange,
];

impl GroupTone {
    /// The tone's name, as the web console's CSS named it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Teal => "teal",
            Self::Blue => "blue",
            Self::Violet => "violet",
            Self::Orange => "orange",
            Self::None => "none",
        }
    }
}

/// JavaScript's `Math.round`: the nearest whole number, halves toward positive infinity
/// (`-6.5` gives `-6`, where [f64::round] gives `-7`).
pub fn js_round(value: f64) -> f64 {
    // `value - floor` is exact for every float with a fractional part, so a half is seen as one.
    let floor = value.floor();
    if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// A level as the whole number from `min` to `max` it stands for; not a number is `min`.
pub fn clamp_in(value: f64, min: i32, max: i32) -> i32 {
    if !value.is_finite() {
        return min;
    }
    // Clamped before the cast, so a huge level cannot overflow it.
    js_round(value).clamp(f64::from(min), f64::from(max)) as i32
}

/// A level as the whole number from 0 to 100 it stands for; not a number is 0.
pub fn clamp_level(value: f64) -> u8 {
    clamp_in(value, 0, 100) as u8
}

/// Where `percent` sits along a fader, from 0 to 1.
pub fn level_fraction(percent: f64) -> f64 {
    f64::from(clamp_level(percent)) / 100.0
}

/// The level at `fraction` along a fader.
pub fn level_from_fraction(fraction: f64) -> u8 {
    clamp_level(fraction * 100.0)
}

/// Where a pointer at `position` sits along a track starting at `start`, `length` long, from 0 to
/// 1; `reverse` counts from the end, as a vertical fader counts from its bottom. A track with no
/// length has no position (0).
pub fn pointer_fraction(position: f64, start: f64, length: f64, reverse: bool) -> f64 {
    // NaN is no length either.
    if !(length > 0.0) {
        return 0.0;
    }
    let fraction = ((position - start) / length).clamp(0.0, 1.0);
    if reverse { 1.0 - fraction } else { fraction }
}

/// A key a slider answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SliderKey {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
}

impl SliderKey {
    /// The key a DOM `KeyboardEvent.key` names, or `None` for one a slider leaves to the page.
    pub fn from_dom(key: &str) -> Option<Self> {
        Some(match key {
            "ArrowUp" => Self::Up,
            "ArrowDown" => Self::Down,
            "ArrowLeft" => Self::Left,
            "ArrowRight" => Self::Right,
            "PageUp" => Self::PageUp,
            "PageDown" => Self::PageDown,
            "Home" => Self::Home,
            "End" => Self::End,
            _ => return None,
        })
    }

    /// The key a GPUI keystroke names (`up`, `pageup`, ...), or `None` for one a slider leaves
    /// alone.
    pub fn from_gpui(key: &str) -> Option<Self> {
        Some(match key {
            "up" => Self::Up,
            "down" => Self::Down,
            "left" => Self::Left,
            "right" => Self::Right,
            "pageup" => Self::PageUp,
            "pagedown" => Self::PageDown,
            "home" => Self::Home,
            "end" => Self::End,
            _ => return None,
        })
    }
}

/// Where `key` moves a slider at `value` within `range`: a step, a page or either end, never past
/// an end, and for a range with a centre never across it in one move (it stops there).
pub fn key_value(key: SliderKey, value: i32, range: &Range) -> i32 {
    let target = match key {
        SliderKey::Up | SliderKey::Right => value + range.step,
        SliderKey::Down | SliderKey::Left => value - range.step,
        SliderKey::PageUp => value + range.page,
        SliderKey::PageDown => value - range.page,
        SliderKey::Home => return range.min,
        SliderKey::End => return range.max,
    }
    .clamp(range.min, range.max);
    match range.centre {
        // A move from one side of the centre to the other stops on it.
        Some(centre) if (value - centre) * (target - centre) < 0 => centre,
        _ => target,
    }
}

/// `value` on `range`'s centre when it is within the detent of it, else as it is.
pub fn snap_to_centre(value: i32, range: &Range) -> i32 {
    match range.centre {
        Some(centre) if (value - centre).abs() <= range.detent => centre,
        _ => value,
    }
}

/// The side of a pan (-1 left, 0 centre, 1 right) and how far, in percent of that side.
fn pan_side(pan: f64) -> (i32, i32) {
    let centre = PAN.centre.unwrap_or(50);
    let value = i32::from(clamp_level(pan));
    ((value - centre).signum(), (value - centre).abs() * 2)
}

/// A pan as the top bar writes it: `C`, or `L`/`R` and how far (`L20`, `R100`).
pub fn pan_label(pan: f64) -> String {
    match pan_side(pan) {
        (0, _) => "C".to_owned(),
        (side, amount) => format!("{}{amount}", if side < 0 { "L" } else { "R" }),
    }
}

/// A pan as a screen reader says it: `Centre`, or how far to which side (`20% left`).
pub fn pan_value_text(pan: f64) -> String {
    match pan_side(pan) {
        (0, _) => "Centre".to_owned(),
        (side, amount) => format!("{amount}% {}", if side < 0 { "left" } else { "right" }),
    }
}

/// A meter level (0-100, linear) in dBFS; silence is negative infinity.
pub fn level_to_db(level: f64) -> f64 {
    // `Number(level) || 0`: not a number is silence.
    let value = if level.is_nan() {
        0.0
    } else {
        level.min(100.0)
    };
    if value <= 0.0 {
        return f64::NEG_INFINITY;
    }
    20.0 * (value / 100.0).log10()
}

/// How full a meter's bar is for `level`, from 0 to 1, over the 60 dB it spans.
pub fn meter_fraction(level: f64) -> f64 {
    meter_fraction_from(level, METER_FLOOR_DB)
}

/// How full a meter's bar is for `level` over the span from `floor_db` to full scale.
pub fn meter_fraction_from(level: f64, floor_db: f64) -> f64 {
    let db = level_to_db(level);
    if db == f64::NEG_INFINITY {
        return 0.0;
    }
    ((db - floor_db) / -floor_db).clamp(0.0, 1.0)
}

/// The zone a meter level is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeterZone {
    Safe,
    Hot,
    Clip,
}

/// The zone of `level`: clipping at full scale, hot from 6 dB below it, else safe.
pub fn meter_zone(level: f64) -> MeterZone {
    if level >= 100.0 {
        MeterZone::Clip
    } else if level_to_db(level) >= HOT_DB {
        MeterZone::Hot
    } else {
        MeterZone::Safe
    }
}

/// The tone of the group at `index` (in the status's order); no group is [GroupTone::None].
pub fn group_tone(index: Option<i64>) -> GroupTone {
    match index {
        Some(index) if index >= 0 => GROUP_TONES[(index as usize) % GROUP_TONES.len()],
        _ => GroupTone::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_whole_numbers_from_0_to_100() {
        assert_eq!(clamp_level(62.4), 62);
        assert_eq!(clamp_level(62.5), 63);
        assert_eq!(clamp_level(-3.0), 0);
        assert_eq!(clamp_level(140.0), 100);
        assert_eq!(clamp_level(f64::NAN), 0);
        assert_eq!(
            clamp_level(f64::INFINITY),
            0,
            "not a finite number is the minimum"
        );
        assert_eq!(js_round(-6.5), -6.0, "JavaScript rounds halves up");
        assert_eq!(js_round(0.499_999_999_999_999_94), 0.0);
    }

    #[test]
    fn a_faders_position_and_its_percent_map_onto_each_other_in_a_straight_line() {
        for percent in 0..=100u8 {
            assert_eq!(
                level_from_fraction(level_fraction(f64::from(percent))),
                percent
            );
        }
        assert_eq!(level_fraction(62.0), 0.62);
        assert_eq!(level_from_fraction(0.624), 62);
        assert_eq!(level_from_fraction(1.2), 100);
        assert_eq!(level_from_fraction(-0.1), 0);
    }

    #[test]
    fn a_vertical_pointer_position_counts_from_the_bottom_of_the_track() {
        assert_eq!(pointer_fraction(300.0, 100.0, 200.0, true), 0.0);
        assert_eq!(pointer_fraction(100.0, 100.0, 200.0, true), 1.0);
        assert_eq!(pointer_fraction(200.0, 100.0, 200.0, true), 0.5);
        assert_eq!(
            pointer_fraction(20.0, 100.0, 200.0, true),
            1.0,
            "above the track is the top"
        );
        assert_eq!(
            pointer_fraction(150.0, 100.0, 200.0, false),
            0.25,
            "horizontal counts from the start"
        );
        assert_eq!(
            pointer_fraction(150.0, 100.0, 0.0, false),
            0.0,
            "a track with no length has no position"
        );
    }

    fn at(key: &str, value: i32, range: &Range) -> Option<i32> {
        SliderKey::from_dom(key).map(|key| key_value(key, value, range))
    }

    #[test]
    fn the_keyboard_moves_a_fader_by_a_step_a_page_or_to_either_end() {
        assert_eq!(at("ArrowUp", 62, &FADER), Some(63));
        assert_eq!(at("ArrowRight", 62, &FADER), Some(63));
        assert_eq!(at("ArrowDown", 62, &FADER), Some(61));
        assert_eq!(at("ArrowLeft", 62, &FADER), Some(61));
        assert_eq!(at("PageUp", 62, &FADER), Some(72));
        assert_eq!(at("PageDown", 62, &FADER), Some(52));
        assert_eq!(at("Home", 62, &FADER), Some(0));
        assert_eq!(at("End", 62, &FADER), Some(100));
        assert_eq!(at("ArrowUp", 100, &FADER), Some(100));
        assert_eq!(at("PageDown", 4, &FADER), Some(0));
        assert_eq!(
            at("Enter", 62, &FADER),
            None,
            "keys a fader does not use are left alone"
        );
        assert_eq!(at("a", 62, &FADER), None);
        assert_eq!(SliderKey::from_gpui("pageup"), Some(SliderKey::PageUp));
        assert_eq!(SliderKey::from_gpui("enter"), None);
    }

    #[test]
    fn the_keyboard_stops_a_pan_at_the_centre_on_the_way_across() {
        assert_eq!(at("PageUp", 45, &PAN), Some(50));
        assert_eq!(at("PageUp", 50, &PAN), Some(60));
        assert_eq!(at("PageDown", 55, &PAN), Some(50));
        assert_eq!(at("ArrowRight", 49, &PAN), Some(50));
        assert_eq!(at("ArrowLeft", 50, &PAN), Some(49));
        assert_eq!(at("Home", 70, &PAN), Some(0));
        assert_eq!(at("End", 30, &PAN), Some(100));
    }

    #[test]
    fn a_dragged_pan_near_the_centre_snaps_to_it() {
        assert_eq!(snap_to_centre(52, &PAN), 50);
        assert_eq!(snap_to_centre(48, &PAN), 50);
        assert_eq!(snap_to_centre(53, &PAN), 53);
        assert_eq!(snap_to_centre(47, &PAN), 47);
        assert_eq!(snap_to_centre(0, &PAN), 0);
    }

    #[test]
    fn a_pan_reads_as_l_c_or_r_and_how_far() {
        assert_eq!(pan_label(50.0), "C");
        assert_eq!(pan_label(40.0), "L20");
        assert_eq!(pan_label(65.0), "R30");
        assert_eq!(pan_label(0.0), "L100");
        assert_eq!(pan_label(100.0), "R100");
        assert_eq!(pan_label(51.0), "R2");
    }

    #[test]
    fn a_pans_spoken_value_names_the_side_in_words() {
        assert_eq!(pan_value_text(50.0), "Centre");
        assert_eq!(pan_value_text(40.0), "20% left");
        assert_eq!(pan_value_text(65.0), "30% right");
    }

    #[test]
    fn a_meter_level_converts_to_decibels_below_full_scale() {
        assert_eq!(level_to_db(100.0), 0.0);
        assert!((level_to_db(50.0) - -6.0206).abs() < 1e-3);
        assert_eq!(level_to_db(0.0), f64::NEG_INFINITY);
        assert_eq!(level_to_db(-4.0), f64::NEG_INFINITY);
    }

    #[test]
    fn a_meter_bar_spans_60_db_so_quiet_signals_still_show() {
        assert_eq!(meter_fraction(0.0), 0.0);
        assert_eq!(meter_fraction(100.0), 1.0);
        assert_eq!(meter_fraction(150.0), 1.0);
        assert!((meter_fraction(50.0) - 0.8997).abs() < 1e-3);
        assert!((meter_fraction(1.0) - 1.0 / 3.0).abs() < 1e-3);
        assert_eq!(
            meter_fraction(0.05),
            0.0,
            "below the 60 dB floor the bar is empty"
        );
    }

    #[test]
    fn a_meter_is_hot_from_6_db_below_full_scale_and_clips_at_full_scale() {
        assert_eq!(meter_zone(0.0), MeterZone::Safe);
        assert_eq!(meter_zone(50.0), MeterZone::Safe);
        assert_eq!(meter_zone(51.0), MeterZone::Hot);
        assert_eq!(meter_zone(99.0), MeterZone::Hot);
        assert_eq!(meter_zone(100.0), MeterZone::Clip);
    }

    #[test]
    fn groups_take_the_chip_tones_in_turn_and_no_group_is_neutral() {
        let tones: Vec<_> = (0..5).map(|index| group_tone(Some(index)).name()).collect();
        assert_eq!(tones, ["teal", "blue", "violet", "orange", "teal"]);
        assert_eq!(group_tone(None), GroupTone::None);
        assert_eq!(group_tone(Some(-1)), GroupTone::None);
        assert_eq!(GroupTone::None.name(), "none");
    }
}
