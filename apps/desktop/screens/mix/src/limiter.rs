//! The master limiter and the effect returns of every mix, as the master strip shows and changes
//! them (`limiter.js` and the returns of `effects.js`). No window.
//!
//! Each mix (a musician's, or the engineer's local mix) ends with a peak limiter after its sum and
//! before the clamp at the mix's ceiling (the lower of the phone's level cap and the engineer's
//! limit). Off by default; its ceiling is -24 to 0 dBFS (0 by default: it limits right at the mix's
//! own ceiling), its release 1 to 1,000 ms (50 by default); it adds no latency. Each mix also
//! returns the shared reverb and delay at 0-100 %. Both are the engineer's alone: no phone sets
//! them.

use bandear_desktop_console::format::{MINUS, reduction_view, sentence_with_reason, to_fixed};
use bandear_desktop_console::state::{ConsoleState, LOCAL_MIX};
use bandear_desktop_console::types::dto::{FxReturnSetDto, LimiterSetDto};
use bandear_desktop_console::types::{Effect, EffectLevels, LimiterSettings};
use bandear_desktop_console::values::{SliderKey, js_round};
use bandear_desktop_console::wire::{Meters, effect_name};

use crate::Said;
use crate::mix_view::master_strip;

/// The ceiling's range, in dBFS.
pub const CEILING_RANGE: (f64, f64) = (-24.0, 0.0);
/// The release's range, in ms.
pub const RELEASE_RANGE: (f64, f64) = (1.0, 1000.0);
/// The gain reduction the master strip's meter shows at full scale, in dB.
pub const LIMITER_REDUCTION_SCALE_DB: f64 = 12.0;
/// The positions of a limiter slider, as a range input's 0-1,000.
pub const SLIDER_STEPS: i32 = 1000;

/// The limiter every mix starts with: off, its ceiling at 0 dBFS, a 50 ms release.
pub fn default_limiter() -> LimiterSettings {
    LimiterSettings {
        enabled: false,
        ceiling_db: 0.0,
        release_ms: 50.0,
    }
}

fn number(value: f32, fallback: f32) -> f64 {
    f64::from(if value.is_finite() { value } else { fallback })
}

/// `settings` held in range, an unreadable number taking the default.
pub fn clamp_limiter(settings: &LimiterSettings) -> LimiterSettings {
    let fallback = default_limiter();
    LimiterSettings {
        enabled: settings.enabled,
        ceiling_db: number(settings.ceiling_db, fallback.ceiling_db)
            .clamp(CEILING_RANGE.0, CEILING_RANGE.1) as f32,
        release_ms: number(settings.release_ms, fallback.release_ms)
            .clamp(RELEASE_RANGE.0, RELEASE_RANGE.1) as f32,
    }
}

/// One of the limiter's two sliders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimiterField {
    /// 0.5 dB steps from -24 to 0 dBFS, on a straight scale.
    Ceiling,
    /// 1 to 1,000 ms on a log scale.
    Release,
}

impl LimiterField {
    fn range(self) -> (f64, f64) {
        match self {
            Self::Ceiling => CEILING_RANGE,
            Self::Release => RELEASE_RANGE,
        }
    }

    /// The smallest change the slider makes, as the strip rounds it.
    fn unit(self) -> f64 {
        match self {
            Self::Ceiling => 0.5,
            Self::Release => 1.0,
        }
    }

    fn of(self, settings: &LimiterSettings) -> f64 {
        f64::from(match self {
            Self::Ceiling => settings.ceiling_db,
            Self::Release => settings.release_ms,
        })
    }
}

/// A log scale needs positive ends: a range from 0 is shifted by 1 first.
fn log_offset(low: f64) -> f64 {
    if low <= 0.0 { 1.0 - low } else { 0.0 }
}

/// The slider position (0-1,000) of `field` at `value`.
pub fn limiter_position(field: LimiterField, value: f64) -> i32 {
    let (low, high) = field.range();
    let held = if value.is_finite() { value } else { 0.0 }.clamp(low, high);
    let fraction = match field {
        LimiterField::Release => {
            let offset = log_offset(low);
            ((held + offset) / (low + offset)).ln() / ((high + offset) / (low + offset)).ln()
        }
        LimiterField::Ceiling => (held - low) / (high - low),
    };
    js_round(fraction * f64::from(SLIDER_STEPS)) as i32
}

/// The value of `field` at slider `position`, rounded as the strip shows it (0.5 dB, 1 ms).
pub fn limiter_value(field: LimiterField, position: f64) -> f64 {
    let (low, high) = field.range();
    let fraction = position.clamp(0.0, f64::from(SLIDER_STEPS)) / f64::from(SLIDER_STEPS);
    match field {
        LimiterField::Release => {
            let offset = log_offset(low);
            let value = ((low + offset) * ((high + offset) / (low + offset)).powf(fraction)
                - offset)
                .clamp(low, high);
            js_round(value).max(1.0)
        }
        LimiterField::Ceiling => js_round((low + fraction * (high - low)) * 2.0) / 2.0,
    }
}

/// A change of the master strip's limiter controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LimiterControl {
    /// The Limiter toggle.
    Enabled(bool),
    /// A slider at a position (0-1,000).
    Slider(LimiterField, f64),
}

/// The whole limiter after a control changed (`limiterChange`). When `key` says a key moved the
/// slider (one step of 1,000), a position that rounds back to the same value moves it by one unit
/// (0.5 dB, 1 ms) its way, so the keyboard always gets somewhere; a drag takes the value at its
/// position as it is, so a slow drag never flips between two neighbours. Held in range.
pub fn limiter_change(
    settings: &LimiterSettings,
    control: LimiterControl,
    key: bool,
) -> LimiterSettings {
    let held = clamp_limiter(settings);
    let (field, position) = match control {
        LimiterControl::Enabled(enabled) => return LimiterSettings { enabled, ..held },
        LimiterControl::Slider(field, position) => (field, position),
    };
    let now = field.of(&held);
    let from = f64::from(limiter_position(field, now));
    let mut next = limiter_value(field, position);
    if key && next == now && position != from {
        next = now + (position - from).signum() * field.unit();
    }
    let changed = match field {
        LimiterField::Ceiling => LimiterSettings {
            ceiling_db: next as f32,
            ..held
        },
        LimiterField::Release => LimiterSettings {
            release_ms: next as f32,
            ..held
        },
    };
    clamp_limiter(&changed)
}

/// A ceiling in words: "0 dBFS", "−6.5 dBFS".
pub fn format_ceiling(db: f64) -> String {
    let value = js_round(if db.is_finite() { db } else { 0.0 } * 10.0) / 10.0;
    if value == 0.0 {
        return "0 dBFS".to_owned();
    }
    let text = if value.fract() == 0.0 {
        format!("{}", value.abs())
    } else {
        to_fixed(value.abs(), 1)
    };
    let sign = if value < 0.0 { MINUS } else { "" };
    format!("{sign}{text} dBFS")
}

/// `value` with `digits` decimals at most, trailing zeros dropped (`String(Number(x.toFixed(2)))`).
fn trimmed(value: f64, digits: usize) -> String {
    let fixed: f64 = to_fixed(value, digits).parse().unwrap_or(0.0);
    format!("{fixed}")
}

/// A time in words, as the processing screen says it: "0.5 ms", "150 ms", "1.5 s".
pub fn format_ms(ms: f64) -> String {
    let value = if ms.is_finite() { ms } else { 0.0 };
    if value >= 1000.0 {
        return format!("{} s", trimmed(value / 1000.0, 2));
    }
    if value < 10.0 {
        format!("{} ms", trimmed(value, 2))
    } else {
        format!("{} ms", js_round(value))
    }
}

/// What the master strip's limiter shows (`limiterView`, `masterLimiter`).
#[derive(Clone, Debug, PartialEq)]
pub struct LimiterView {
    pub settings: LimiterSettings,
    pub ceiling: String,
    pub release: String,
    pub state: &'static str,
    pub note: String,
    pub reduction: (f64, String),
    pub ceiling_position: i32,
    pub release_position: i32,
    /// A change is on its way to the desk.
    pub pending: bool,
}

/// The view of `settings` with `reduction_db` of gain reduction (shown only while on) for a mix
/// whose own ceiling is `ceiling_percent` (`None` for the local mix, which has no cap or limit).
pub fn limiter_view(
    settings: &LimiterSettings,
    reduction_db: f64,
    ceiling_percent: Option<u8>,
) -> LimiterView {
    let held = clamp_limiter(settings);
    let reduction = reduction_view(
        if held.enabled { reduction_db } else { 0.0 },
        LIMITER_REDUCTION_SCALE_DB,
    );
    let ceiling = format_ceiling(f64::from(held.ceiling_db));
    let mix_ceiling = match ceiling_percent {
        Some(percent) => format!("the mix's ceiling ({percent}%)"),
        None => "the mix's full scale".to_owned(),
    };
    LimiterView {
        settings: held,
        note: if held.enabled {
            format!(
                "Peaks are held at {ceiling} or {mix_ceiling}, whichever is lower. No added latency."
            )
        } else {
            "Off: the mix is only clipped at its ceiling.".to_owned()
        },
        ceiling,
        release: format_ms(f64::from(held.release_ms)),
        state: if held.enabled {
            "Limiter on"
        } else {
            "Limiter off"
        },
        reduction,
        ceiling_position: limiter_position(LimiterField::Ceiling, f64::from(held.ceiling_db)),
        release_position: limiter_position(LimiterField::Release, f64::from(held.release_ms)),
        pending: false,
    }
}

/// The limiter of the mix `address` names: the change on its way, else the status's, else the
/// default; and whether a change is on its way.
pub fn mix_limiter(state: &ConsoleState, address: &str) -> (LimiterSettings, bool) {
    if let Some(waiting) = state.limiter_pending.get(address) {
        return (waiting.value, true);
    }
    let reported = if address == LOCAL_MIX {
        state
            .engine
            .status
            .as_ref()
            .map(|status| status.local_limiter)
    } else {
        state
            .musician_by_address(address)
            .map(|musician| musician.limiter)
    };
    (
        clamp_limiter(&reported.unwrap_or_else(default_limiter)),
        false,
    )
}

/// The master strip's limiter for the mix `address`, or `None` when there is no such mix. The gain
/// reduction comes from the meters, apart ([limiter_reduction]).
pub fn master_limiter(state: &ConsoleState, address: &str) -> Option<LimiterView> {
    let strip = master_strip(state, address)?;
    let (settings, pending) = mix_limiter(state, address);
    let ceiling = match (strip.cap, strip.limit) {
        (Some(cap), Some(limit)) => Some(cap.min(limit)),
        _ => None,
    };
    Some(LimiterView {
        pending,
        ..limiter_view(&settings, 0.0, ceiling)
    })
}

/// How far the limiter of the mix `selected` turns it down now, in dB, from `meters`.
pub fn limiter_reduction(meters: Option<&Meters>, selected: &str) -> f64 {
    let output = meters.and_then(|meters| {
        if selected == LOCAL_MIX {
            Some(&meters.local)
        } else {
            meters.musicians.get(selected)
        }
    });
    let db = output.map_or(0.0, |output| f64::from(output.limiter_reduction_db));
    if db.is_finite() && db > 0.0 { db } else { 0.0 }
}

/// The desk's answer to a limiter change in words, for the mix called `name` ("Ana", or "The local
/// mix").
pub fn format_limiter_set(answer: Option<&LimiterSetDto>, name: &str) -> Said {
    let Some(answer) = answer.filter(|answer| answer.outcome == "set") else {
        return Said::refused(
            answer
                .map(|answer| answer.message.clone())
                .unwrap_or_else(|| format!("{name}'s limiter was not set.")),
        );
    };
    let settings = clamp_limiter(&answer.settings);
    let whose = if name == "The local mix" {
        "The local mix's".to_owned()
    } else {
        format!("{name}'s")
    };
    let mut message = if settings.enabled {
        format!(
            "{whose} limiter is on: peaks are held at {}, released over {}.",
            format_ceiling(f64::from(settings.ceiling_db)),
            format_ms(f64::from(settings.release_ms))
        )
    } else {
        format!("{whose} limiter is off.")
    };
    if let Some(error) = &answer.save_error {
        message.push(' ');
        message.push_str(&sentence_with_reason(
            "It was not saved for the next start",
            Some(error),
        ));
    }
    Said::done(message)
}

/// Which limiter slider a key is moving (`createSliderKeys`): a slider's change is a key step only
/// while a slider key is held down on that slider. A press of the pointer is never a key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SliderKeys {
    keyed: Option<LimiterField>,
}

impl SliderKeys {
    /// `key` (a GPUI key name) went down on `slider`.
    pub fn key_down(&mut self, slider: LimiterField, key: &str) {
        self.keyed = SliderKey::from_gpui(key).map(|_| slider);
    }

    pub fn key_up(&mut self) {
        self.keyed = None;
    }

    pub fn pointer_down(&mut self) {
        self.keyed = None;
    }

    /// True while a slider key is held down on `slider`.
    pub fn keyed(&self, slider: LimiterField) -> bool {
        self.keyed == Some(slider)
    }
}

/// Both shared effects, in the strip's order.
pub const EFFECTS: [Effect; 2] = [Effect::Reverb, Effect::Delay];

/// An effect's name on screen.
pub fn effect_label(effect: Effect) -> &'static str {
    match effect {
        Effect::Reverb => "Reverb",
        Effect::Delay => "Delay",
    }
}

/// A return level in words: "40%", "Off" at 0.
pub fn format_level(level: u8) -> String {
    match level.min(100) {
        0 => "Off".to_owned(),
        level => format!("{level}%"),
    }
}

/// The returns of the mix `address`: the change on its way over the status's, and whether one is
/// on its way.
pub fn mix_returns(state: &ConsoleState, address: &str) -> (EffectLevels, bool) {
    let reported = if address == LOCAL_MIX {
        state
            .engine
            .status
            .as_ref()
            .map(|status| status.local_fx_returns)
    } else {
        state
            .musician_by_address(address)
            .map(|musician| musician.fx_returns)
    }
    .unwrap_or_default();
    let waiting = state.returns_pending.get(address);
    let level = |effect: Effect| {
        waiting
            .and_then(|levels| levels.get(effect))
            .unwrap_or_else(|| reported.get(effect))
            .min(100)
    };
    (
        EffectLevels {
            reverb: level(Effect::Reverb),
            delay: level(Effect::Delay),
        },
        waiting.is_some(),
    )
}

/// One return row of the master strip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReturnRow {
    pub effect: Effect,
    /// The row's word ("Reverb") and the slider's name ("Reverb return").
    pub name: &'static str,
    pub label: String,
    pub level: u8,
    pub text: String,
}

/// What the master strip shows of its mix's returns: one row per effect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReturnsView {
    pub pending: bool,
    pub rows: Vec<ReturnRow>,
}

pub fn returns_view(state: &ConsoleState, address: &str) -> ReturnsView {
    let (levels, pending) = mix_returns(state, address);
    ReturnsView {
        pending,
        rows: EFFECTS
            .iter()
            .map(|effect| ReturnRow {
                effect: *effect,
                name: effect_label(*effect),
                label: format!("{} return", effect_label(*effect)),
                level: levels.get(*effect),
                text: format_level(levels.get(*effect)),
            })
            .collect(),
    }
}

/// The desk's answer to a return change in words, for the mix called `name`.
pub fn format_return_set(answer: Option<&FxReturnSetDto>, name: &str) -> Said {
    let Some(answer) = answer.filter(|answer| answer.outcome == "set") else {
        return Said::refused(
            answer
                .map(|answer| answer.message.clone())
                .unwrap_or_else(|| format!("{name}'s return was not set.")),
        );
    };
    let level = answer.returns.get(answer.effect).min(100);
    let effect = effect_name(answer.effect);
    let whose = if name == "The local mix" {
        "The local mix".to_owned()
    } else {
        format!("{name}'s mix")
    };
    let mut message = if level == 0 {
        format!("{whose} returns no {effect}.")
    } else {
        format!("{whose} returns the {effect} at {level}%.")
    };
    if let Some(error) = &answer.save_error {
        message.push(' ');
        message.push_str(&sentence_with_reason(
            "It was not saved for the next start",
            Some(error),
        ));
    }
    Said::done(message)
}

#[cfg(test)]
mod tests;
