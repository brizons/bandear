//! The link history chart's geometry (`healthLayout` in `link-health-view.js`; inventory E
//! section 8): the small multiples on one time axis, each with its own scale (N26), the lines that
//! break at a gap, a missing value or a reconnect, the dots of seconds standing alone (N28), the
//! event lanes (N29), the shaded gaps, the reconnect markers and the time axis; and the second
//! nearest the pointer (N30). Pure: the view paints what this answers.

use bandear_desktop_console::values::js_round;
use bandear_desktop_console::wire::LinkSample;

use super::clock::{Zone, format_clock};
use super::link_view::{Extras, Field, PERIOD_MS, Window, value_of};

const MINUS: &str = "\u{2212}";

const NICE_STEPS: [f64; 5] = [1.0, 2.0, 2.5, 5.0, 10.0];

/// The top of a scale for values up to `value`: a round number, never below `floor`.
pub fn nice_max(value: f64, floor: f64) -> f64 {
    let top = value.max(floor);
    let magnitude = 10f64.powf(top.log10().floor());
    let step = NICE_STEPS
        .into_iter()
        .find(|nice| nice * magnitude >= top - 1e-9)
        .unwrap_or(10.0);
    step * magnitude
}

/// `Math.round(value * 10) / 10`.
pub fn round1(value: f64) -> f64 {
    js_round(value * 10.0) / 10.0
}

// The chart's anatomy, in pixels.
const PAD_LEFT: f64 = 72.0;
const PAD_RIGHT: f64 = 12.0;
const MARKER_ROW: f64 = 16.0;
const TITLE_ROW: f64 = 18.0;
const PANEL_HEIGHT: f64 = 56.0;
const PANEL_GAP: f64 = 6.0;
const LANE_HEIGHT: f64 = 18.0;
const AXIS_HEIGHT: f64 = 22.0;
/// How far apart two reconnect labels must be, and how close to the edge one turns leftwards.
const LABEL_ROOM: f64 = 90.0;
/// How close to the right edge a relative time tick may come before the newest time's label.
const END_LABEL_ROOM: f64 = 64.0;

struct PanelSpec {
    id: &'static str,
    field: Field,
    title: &'static str,
    floor: f64,
    extra: Option<fn(&Extras) -> bool>,
}

const PANELS: [PanelSpec; 4] = [
    PanelSpec {
        id: "loss",
        field: Field::Loss,
        title: "Loss (%)",
        floor: 1.0,
        extra: None,
    },
    PanelSpec {
        id: "rtt",
        field: Field::Rtt,
        title: "Round trip (ms)",
        floor: 10.0,
        extra: None,
    },
    PanelSpec {
        id: "jitter",
        field: Field::Jitter,
        title: "Jitter (ms)",
        floor: 5.0,
        extra: Some(|extras| extras.jitter),
    },
    PanelSpec {
        id: "cushion",
        field: Field::Cushion,
        title: "Cushion (ms)",
        floor: 50.0,
        extra: Some(|extras| extras.cushion),
    },
];

/// A point of the chart, in pixels from its top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A label on a horizontal line of a panel.
#[derive(Clone, Debug, PartialEq)]
pub struct Tick {
    pub y: f64,
    pub label: String,
}

/// One small multiple.
#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub id: &'static str,
    pub title: &'static str,
    pub title_y: f64,
    pub top: f64,
    pub height: f64,
    pub max: f64,
    /// The baseline (0) and the grid at the top of the scale.
    pub ticks: [Tick; 2],
    /// Every run of two or more seconds, as a polyline.
    pub lines: Vec<Vec<Point>>,
    /// The seconds standing alone.
    pub dots: Vec<Point>,
}

/// An event mark in a lane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub x: f64,
    pub count: u64,
}

/// The underruns' or the timeouts' lane.
#[derive(Clone, Debug, PartialEq)]
pub struct Lane {
    pub id: &'static str,
    pub label: &'static str,
    pub top: f64,
    pub height: f64,
    pub marks: Vec<Mark>,
}

/// Seconds without reports, shaded across every panel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub x: f64,
    pub width: f64,
}

/// Where a text sits against its x.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// A reconnect marker; `label` `None` where an earlier one is too close to be labelled again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reconnect {
    pub x: f64,
    pub label: Option<&'static str>,
    pub anchor: Anchor,
}

/// A label of the time axis.
#[derive(Clone, Debug, PartialEq)]
pub struct TimeTick {
    pub x: f64,
    pub label: String,
    pub anchor: Anchor,
}

/// The area every panel and lane shares in x.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plot {
    pub left: f64,
    pub width: f64,
    pub top: f64,
    pub bottom: f64,
    pub axis_y: f64,
}

/// The chart's geometry (`healthLayout`).
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub width: f64,
    pub height: f64,
    pub plot: Plot,
    /// The domain's start and span, on the monotonic clock.
    pub start_ms: f64,
    pub span_ms: f64,
    pub panels: Vec<Panel>,
    pub lanes: Vec<Lane>,
    pub gaps: Vec<Band>,
    pub reconnects: Vec<Reconnect>,
    pub time_ticks: Vec<TimeTick>,
}

impl Layout {
    /// The x of monotonic time `mono_ms`, not rounded (the crosshair's).
    pub fn x_of(&self, mono_ms: f64) -> f64 {
        self.plot.left + (mono_ms - self.start_ms) / self.span_ms * self.plot.width
    }
}

fn tick_label(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value}")
    } else {
        format!("{}", round1(value))
    }
}

/// The runs a line joins: broken at a missing value, a reconnect and a missing second.
fn runs_of(samples: &[LinkSample], field: Field) -> Vec<Vec<(&LinkSample, f64)>> {
    let mut runs = Vec::new();
    let mut run: Vec<(&LinkSample, f64)> = Vec::new();
    let mut previous: Option<&LinkSample> = None;
    for sample in samples {
        let value = value_of(sample, field);
        let broken = value.is_none()
            || sample.reconnected
            || previous.is_some_and(|previous| {
                sample.mono_ms as f64 - previous.mono_ms as f64 > 1.5 * PERIOD_MS
            });
        if broken && !run.is_empty() {
            runs.push(std::mem::take(&mut run));
        }
        if let Some(value) = value {
            run.push((sample, value));
        }
        previous = Some(sample);
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs
}

/// The chart's geometry for `window` at `width` pixels, over `range_ms`, with panels for the
/// phone's `extras`; the axis's last label is the newest second's time in `zone`.
pub fn health_layout(
    window: &Window,
    width: f64,
    extras: Extras,
    range_ms: u64,
    zone: Zone,
) -> Layout {
    let span = range_ms as f64;
    let plot_width = (width - PAD_LEFT - PAD_RIGHT).max(1.0);
    let start_ms = window.end_ms.map_or(0.0, |end| end as f64 - span);
    let x_of = |mono_ms: f64| round1(PAD_LEFT + (mono_ms - start_ms) / span * plot_width);
    let samples = &window.samples;

    let mut y = MARKER_ROW;
    let mut panels = Vec::new();
    for spec in PANELS.iter() {
        if spec.extra.is_some_and(|reported| !reported(&extras)) {
            continue;
        }
        let top = y + TITLE_ROW;
        let highest = samples
            .iter()
            .filter_map(|sample| value_of(sample, spec.field))
            .fold(None, |most: Option<f64>, value| {
                Some(most.map_or(value, |most| most.max(value)))
            });
        let max = nice_max(highest.unwrap_or(0.0), spec.floor);
        let y_of = |value: f64| round1(top + PANEL_HEIGHT - value.min(max) / max * PANEL_HEIGHT);
        let runs = runs_of(samples, spec.field);
        let point = |(sample, value): &(&LinkSample, f64)| Point {
            x: x_of(sample.mono_ms as f64),
            y: y_of(*value),
        };
        panels.push(Panel {
            id: spec.id,
            title: spec.title,
            title_y: top - 6.0,
            top,
            height: PANEL_HEIGHT,
            max,
            ticks: [
                Tick {
                    y: y_of(0.0),
                    label: "0".to_owned(),
                },
                Tick {
                    y: y_of(max),
                    label: tick_label(max),
                },
            ],
            lines: runs
                .iter()
                .filter(|run| run.len() > 1)
                .map(|run| run.iter().map(point).collect())
                .collect(),
            dots: runs
                .iter()
                .filter(|run| run.len() == 1)
                .map(|run| point(&run[0]))
                .collect(),
        });
        y = top + PANEL_HEIGHT + PANEL_GAP;
    }

    let lanes: Vec<Lane> = [
        ("underruns", Field::Underruns, "Underruns"),
        ("timeouts", Field::Timeouts, "Timeouts"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (id, field, label))| Lane {
        id,
        label,
        top: y + index as f64 * LANE_HEIGHT,
        height: LANE_HEIGHT,
        marks: samples
            .iter()
            .filter_map(|sample| {
                value_of(sample, field)
                    .filter(|count| *count > 0.0)
                    .map(|count| Mark {
                        x: x_of(sample.mono_ms as f64),
                        count: count as u64,
                    })
            })
            .collect(),
    })
    .collect();
    let bottom = y + lanes.len() as f64 * LANE_HEIGHT;

    // Consecutive gap seconds (their seqs in a row) form one band.
    let mut bands: Vec<(&LinkSample, &LinkSample)> = Vec::new();
    let mut open = false;
    for sample in samples {
        match bands.last_mut() {
            Some((_, last)) if open && sample.gap && sample.seq == last.seq + 1 => *last = sample,
            _ if sample.gap => {
                bands.push((sample, sample));
                open = true;
            }
            _ => open = false,
        }
    }
    let gaps = bands
        .into_iter()
        .map(|(first, last)| {
            let x0 = PAD_LEFT.max(x_of(first.mono_ms as f64 - PERIOD_MS));
            Band {
                x: x0,
                width: round1((x_of(last.mono_ms as f64) - x0).max(1.0)),
            }
        })
        .collect();

    let mut reconnects = Vec::new();
    let mut labelled = f64::NEG_INFINITY;
    for sample in samples.iter().filter(|sample| sample.reconnected) {
        let x = x_of(sample.mono_ms as f64);
        let room = x - labelled >= LABEL_ROOM;
        if room {
            labelled = x;
        }
        reconnects.push(Reconnect {
            x,
            label: room.then_some("Reconnected"),
            anchor: if x > PAD_LEFT + plot_width - LABEL_ROOM {
                Anchor::End
            } else {
                Anchor::Start
            },
        });
    }

    let mut time_ticks = Vec::new();
    if let (Some(end), Some(newest)) = (window.end_ms, samples.last()) {
        let step = if span <= 60_000.0 {
            15_000.0
        } else if span <= 300_000.0 {
            60_000.0
        } else {
            120_000.0
        };
        let count = (span / step).floor() as i64;
        let right = PAD_LEFT + plot_width;
        for k in (1..=count).rev() {
            let back = k as f64 * step;
            let x = x_of(end as f64 - back);
            if right - x < END_LABEL_ROOM {
                continue;
            }
            let label = if span <= 60_000.0 {
                format!("{MINUS}{} s", back / 1000.0)
            } else {
                format!("{MINUS}{} min", back / 60_000.0)
            };
            time_ticks.push(TimeTick {
                x,
                label,
                anchor: if x <= PAD_LEFT + 0.5 {
                    Anchor::Start
                } else {
                    Anchor::Middle
                },
            });
        }
        time_ticks.push(TimeTick {
            x: right,
            label: format_clock(newest.at_ms, zone),
            anchor: Anchor::End,
        });
    }

    Layout {
        width,
        height: bottom + AXIS_HEIGHT,
        plot: Plot {
            left: PAD_LEFT,
            width: plot_width,
            top: MARKER_ROW,
            bottom,
            axis_y: bottom + 15.0,
        },
        start_ms,
        span_ms: span,
        panels,
        lanes,
        gaps,
        reconnects,
        time_ticks,
    }
}

/// The index in `window.samples` of the second nearest the pointer at `x`; `None` off the plot
/// (8 px of slack on each side).
pub fn nearest_index(window: &Window, layout: &Layout, x: f64) -> Option<usize> {
    let samples = &window.samples;
    let Plot { left, width, .. } = layout.plot;
    if samples.is_empty() || x < left - 8.0 || x > left + width + 8.0 {
        return None;
    }
    let fraction = ((x - left) / width).clamp(0.0, 1.0);
    let mono = layout.start_ms + fraction * layout.span_ms;
    let at = samples.partition_point(|sample| (sample.mono_ms as f64) < mono);
    let low = at.min(samples.len() - 1);
    if low > 0 && mono - (samples[low - 1].mono_ms as f64) < samples[low].mono_ms as f64 - mono {
        return Some(low - 1);
    }
    Some(low)
}

#[cfg(test)]
#[path = "link_chart_tests.rs"]
mod tests;
