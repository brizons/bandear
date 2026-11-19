//! The meters: a level meter over the 60 dB below full scale (`meter.js`), stereo as two of them,
//! and a gain reduction meter whose spoken value follows at most twice a second
//! (`reduction-meter.js`).

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use bandear_desktop_console::format::{format_db, reduction_view};
use bandear_desktop_console::scheduling::{Push, TrailingThrottle};
use bandear_desktop_console::values::{MeterZone, level_to_db, meter_fraction, meter_zone};
use gpui::{
    App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce, Role, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px, relative,
};

use crate::slider::Axis;
use crate::theme::{FONT_MONO, color, colors, text};

/// The color of a meter's bar for `level`.
fn zone_color(level: f64) -> u32 {
    match meter_zone(level) {
        MeterZone::Safe => colors::METER_SAFE,
        MeterZone::Hot => colors::METER_HOT,
        MeterZone::Clip => colors::METER_CLIP,
    }
}

/// A level meter: a bar over the 60 dB below full scale, green, then the accent from 6 dB below
/// full scale, red at full scale. 8 px wide (8 px tall when horizontal), as long as its slot. Its
/// value is spoken in dBFS.
#[derive(IntoElement)]
pub struct LevelMeter {
    id: ElementId,
    level: f64,
    label: SharedString,
    axis: Axis,
}

impl LevelMeter {
    /// A vertical meter at `level` (0-100, linear), named `label` for assistive technology.
    pub fn new(id: impl Into<ElementId>, level: f64, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            level,
            label: label.into(),
            axis: Axis::Vertical,
        }
    }

    /// The meter lying on its side.
    pub fn horizontal(mut self) -> Self {
        self.axis = Axis::Horizontal;
        self
    }
}

impl RenderOnce for LevelMeter {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let fraction = meter_fraction(self.level) as f32;
        let bar = div().absolute().bg(color(zone_color(self.level)));
        let bar = match self.axis {
            Axis::Vertical => bar.left_0().right_0().bottom_0().h(relative(fraction)),
            Axis::Horizontal => bar.top_0().bottom_0().left_0().w(relative(fraction)),
        };
        div()
            .id(self.id)
            .role(Role::Meter)
            .aria_label(self.label)
            .aria_numeric_value(self.level.clamp(0.0, 100.0))
            .aria_min_numeric_value(0.0)
            .aria_max_numeric_value(100.0)
            .aria_value(format_db(level_to_db(self.level)))
            .relative()
            .overflow_hidden()
            .border_1()
            .border_color(color(colors::BORDER_SUBTLE))
            .rounded(px(3.0))
            .bg(color(colors::METER_GROUND))
            .map(|meter| match self.axis {
                Axis::Vertical => meter.w(px(8.0)).h_full().min_h(px(40.0)),
                Axis::Horizontal => meter.h(px(8.0)).w_full().min_w(px(40.0)),
            })
            .child(bar)
    }
}

/// A stereo pair of meters, left and right, 2 px apart; each is named after `label`.
pub fn stereo_meter(id: &str, left: f64, right: f64, label: &str) -> impl IntoElement {
    div()
        .flex()
        .h_full()
        .gap(px(2.0))
        .child(LevelMeter::new(
            SharedString::from(format!("{id}-left")),
            left,
            format!("{label}, left"),
        ))
        .child(LevelMeter::new(
            SharedString::from(format!("{id}-right")),
            right,
            format!("{label}, right"),
        ))
}

/// A clock for the throttle: time since the first look at it.
fn now() -> Duration {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

/// What a reduction meter has said to assistive technology, and when it may say more.
struct Spoken {
    throttle: TrailingThrottle,
    said: SharedString,
    latest: SharedString,
}

/// How often a reduction meter speaks its value at most.
pub const REDUCTION_SPOKEN_INTERVAL: Duration = Duration::from_millis(500);

/// A gain reduction meter (a gate's, a compressor's, a limiter's): a blue bar growing from the
/// right over `scale_db`, and the reduction in words beside it (`−3.2 dB`). The value it speaks
/// follows through a 500 ms trailing throttle, so a screen reader is not flooded at 30 updates a
/// second and always hears the last value.
#[derive(IntoElement)]
pub struct ReductionMeter {
    id: ElementId,
    db: f64,
    scale_db: f64,
    label: SharedString,
}

impl ReductionMeter {
    /// A meter of `db` of reduction (positive), full at `scale_db`, named `label`.
    pub fn new(
        id: impl Into<ElementId>,
        db: f64,
        scale_db: f64,
        label: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            db,
            scale_db,
            label: label.into(),
        }
    }
}

impl RenderOnce for ReductionMeter {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (fraction, words) = reduction_view(self.db, self.scale_db);
        let words = SharedString::from(words);
        let spoken = window.use_keyed_state(self.id.clone(), cx, |_, _| Spoken {
            throttle: TrailingThrottle::new(REDUCTION_SPOKEN_INTERVAL),
            said: words.clone(),
            latest: words.clone(),
        });
        let push = spoken.update(cx, |spoken, _| {
            if spoken.latest == words {
                return None;
            }
            spoken.latest = words.clone();
            match spoken.throttle.push(now()) {
                Push::Run => {
                    spoken.said = words.clone();
                    None
                }
                Push::RunAfter { after, timer } => Some((after, timer)),
                Push::Waiting => None,
            }
        });
        if let Some((after, timer)) = push {
            let spoken = spoken.clone();
            cx.spawn(async move |cx| {
                cx.background_executor().timer(after).await;
                spoken.update(cx, |spoken, cx| {
                    if spoken.throttle.fire(timer, now()) {
                        spoken.said = spoken.latest.clone();
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        let said = spoken.read(cx).said.clone();
        let value = self.db.clamp(0.0, self.scale_db.max(0.0));
        div()
            .id(self.id)
            .role(Role::Meter)
            .aria_label(self.label)
            .aria_min_numeric_value(0.0)
            .aria_max_numeric_value(self.scale_db)
            .aria_numeric_value(if value.is_finite() { value } else { 0.0 })
            .aria_value(said)
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .relative()
                    .h(px(8.0))
                    .overflow_hidden()
                    .rounded(px(4.0))
                    .bg(color(colors::TRACK))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .right_0()
                            .w(relative(fraction as f32))
                            .bg(color(colors::BLUE)),
                    ),
            )
            .child(
                div()
                    .w(px(64.0))
                    .flex_none()
                    .text_right()
                    .whitespace_nowrap()
                    .font_family(FONT_MONO)
                    .text_size(px(text::SM))
                    .child(crate::a11y::seen(words)),
            )
    }
}
