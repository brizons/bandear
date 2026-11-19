//! What the link health panel shows, worked out from a musician's link history
//! (`link-health-view.js`). Pure: the follower that reads it lives in [super::link_follow], the
//! chart's geometry in [super::link_chart].
//!
//! - The history is followed by its cursor: the newest `seq` held is the next read's `sinceSeq`,
//!   and only the samples after it come back, appended to at most the ring's size. A history id not
//!   seen before is a new history (the engine restarted): it is read whole (N21, N22).
//! - The chart draws on the monotonic clock (`monoMs`), never on the wall clock (`atMs`, shown as
//!   local time): the window is the chosen range up to the newest sample (N25).
//! - Loss and round trip, then the phone's jitter and cushion when it reports them, are small
//!   multiples with their own scales on one time axis (N26, N27). A line breaks at a missing value,
//!   a reconnect and a missing second; a second standing alone is a dot (N28). Underruns and
//!   timeouts are event marks in their own lanes (N29); a reconnect is a dashed marker.

use bandear_desktop_console::format::{LinkLevel, link_state, to_fixed};
use bandear_desktop_console::values::js_round;
use bandear_desktop_console::wire::{LinkSample, Musician};

use super::clock::{Zone, format_clock};
use super::link_chart::round1;

/// A range the panel offers: its span, its button's label and its words in the title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkRange {
    pub ms: u64,
    pub label: &'static str,
    pub long: &'static str,
}

/// The ranges, 1, 5 and 10 minutes.
pub const LINK_RANGES: [LinkRange; 3] = [
    LinkRange {
        ms: 60_000,
        label: "1 min",
        long: "last minute",
    },
    LinkRange {
        ms: 300_000,
        label: "5 min",
        long: "last 5 minutes",
    },
    LinkRange {
        ms: 600_000,
        label: "10 min",
        long: "last 10 minutes",
    },
];

/// The range the panel opens with: the whole ring.
pub const DEFAULT_RANGE_MS: u64 = 600_000;

/// How many samples a history keeps when the answer does not say.
const RING_SAMPLES: usize = 600;

/// The time one sample stands for.
pub(crate) const PERIOD_MS: f64 = 1000.0;

const DASH: &str = "\u{2014}";

/// The history held for a musician: its id, and its samples, the oldest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    pub history_id: u64,
    pub samples: Vec<LinkSample>,
}

/// One `link_history` answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HistoryRead {
    pub history_id: u64,
    pub samples: Vec<LinkSample>,
    /// How many samples the history keeps; 0 when the answer does not say.
    pub max_samples: usize,
}

/// The cursor to ask with for what is new: the newest `seq` held, or `None` to read the whole.
pub fn cursor_of(buffer: Option<&History>) -> Option<u64> {
    buffer?.samples.last().map(|sample| sample.seq)
}

/// What an answer does to the history held.
#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    pub buffer: Option<History>,
    pub changed: bool,
    /// Another history answered a read made with a cursor: read it whole, keeping `buffer` until
    /// then.
    pub refetch: bool,
}

/// What `read`, the answer to a read made with the cursor `asked_since`, does to `buffer`.
pub fn history_update(
    buffer: Option<&History>,
    read: &HistoryRead,
    asked_since: Option<u64>,
) -> Update {
    let same = buffer.is_some_and(|buffer| buffer.history_id == read.history_id);
    if !same && asked_since.is_some() {
        return Update {
            buffer: buffer.cloned(),
            changed: false,
            refetch: true,
        };
    }
    let newest = if same { cursor_of(buffer) } else { None };
    let fresh: Vec<LinkSample> = read
        .samples
        .iter()
        .filter(|sample| newest.is_none_or(|newest| sample.seq > newest))
        .cloned()
        .collect();
    let ring = if read.max_samples > 0 {
        read.max_samples
    } else {
        RING_SAMPLES
    };
    let changed = !same || !fresh.is_empty();
    let mut samples = match (same, buffer) {
        (true, Some(buffer)) => buffer.samples.clone(),
        _ => Vec::new(),
    };
    samples.extend(fresh);
    if samples.len() > ring {
        samples.drain(..samples.len() - ring);
    }
    Update {
        buffer: Some(History {
            history_id: read.history_id,
            samples,
        }),
        changed,
        refetch: false,
    }
}

/// The samples of a range up to the newest one, on the monotonic clock; the times `None` with none.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Window {
    pub samples: Vec<LinkSample>,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
}

/// The window of `range_ms` up to the newest of `samples`.
pub fn window_of(samples: &[LinkSample], range_ms: u64) -> Window {
    let Some(newest) = samples.last() else {
        return Window::default();
    };
    let end = newest.mono_ms as i64;
    let start = end - range_ms as i64;
    let mut first = samples.len() - 1;
    while first > 0 && samples[first - 1].mono_ms as i64 > start {
        first -= 1;
    }
    Window {
        samples: samples[first..].to_vec(),
        start_ms: Some(start),
        end_ms: Some(end),
    }
}

/// A number a sample may carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Loss,
    Rtt,
    Jitter,
    Cushion,
    Underruns,
    Timeouts,
    Recovered,
    PacketsLost,
    PacketsReceived,
}

/// `field` of `sample`, whatever a gap says.
fn raw(sample: &LinkSample, field: Field) -> Option<f64> {
    let value = match field {
        Field::Loss => sample.loss_percent,
        Field::Rtt => sample.rtt_ms,
        Field::Jitter => sample.jitter_ms,
        Field::Cushion => sample.cushion_ms,
        Field::Underruns => sample.underruns.map(|count| count as f64),
        Field::Timeouts => sample.receive_timeouts.map(|count| count as f64),
        Field::Recovered => sample.recovered.map(|count| count as f64),
        Field::PacketsLost => sample.packets_lost.map(|count| count as f64),
        Field::PacketsReceived => sample.packets_received.map(|count| count as f64),
    };
    value.filter(|value| value.is_finite())
}

/// The value a panel reads from `sample`; a gap has none at all.
pub fn value_of(sample: &LinkSample, field: Field) -> Option<f64> {
    if sample.gap { None } else { raw(sample, field) }
}

/// Which of the phone's own numbers it reports (and so have panels or columns).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Extras {
    pub jitter: bool,
    pub cushion: bool,
    pub recovered: bool,
}

/// Which of the phone's own numbers `musician` reports: announced, or seen in a sample.
pub fn extras_reported(musician: &Musician, samples: &[LinkSample]) -> Extras {
    let announced = |capability: &str| {
        musician
            .capabilities
            .iter()
            .any(|listed| listed == capability)
    };
    let some = |field| {
        samples
            .iter()
            .any(|sample| value_of(sample, field).is_some())
    };
    Extras {
        jitter: announced("stats.v2") || some(Field::Jitter),
        cushion: announced("stats.v2") || some(Field::Cushion),
        recovered: announced("redundancy.recovered") || some(Field::Recovered),
    }
}

fn format_ms(value: Option<f64>) -> String {
    value.map_or_else(
        || DASH.to_owned(),
        |value| format!("{} ms", to_fixed(round1(value), 1)),
    )
}

fn format_loss(value: Option<f64>) -> String {
    value.map_or_else(
        || DASH.to_owned(),
        |value| format!("{}%", to_fixed(value, 2)),
    )
}

fn format_count(value: Option<u64>) -> String {
    value.map_or_else(|| DASH.to_owned(), |value| value.to_string())
}

fn plural(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The range `range_ms` in words: its label and its long form.
pub fn range_words(range_ms: u64) -> (String, String) {
    match LINK_RANGES.iter().find(|range| range.ms == range_ms) {
        Some(range) => (range.label.to_owned(), range.long.to_owned()),
        None => {
            let minutes = js_round(range_ms as f64 / 60_000.0);
            (format!("{minutes} min"), format!("last {minutes} minutes"))
        }
    }
}

/// The line above the chart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub level: LinkLevel,
    pub label: String,
    pub parts: Vec<String>,
    pub text: String,
}

/// The summary: the link level the table shows, the window's loss, the round trip now and on
/// average, the underruns and, when there were any, the timeouts, the seconds without reports
/// and the reconnects; for a phone that reports recoveries, what a copy recovered.
pub fn link_summary(window: &Window, range_ms: u64, musician: &Musician) -> Summary {
    let state = link_state(musician);
    let (level, label) = (state.level, state.label);
    if window.samples.is_empty() {
        let text = "No link reports yet".to_owned();
        return Summary {
            level,
            label,
            parts: vec![text.clone()],
            text,
        };
    }
    let words = range_words(range_ms).0;
    let reports: Vec<&LinkSample> = window.samples.iter().filter(|sample| !sample.gap).collect();
    let sum = |field| -> f64 { reports.iter().filter_map(|sample| raw(sample, field)).sum() };
    let lost = sum(Field::PacketsLost);
    let expected = lost + sum(Field::PacketsReceived);
    let mut parts = vec![if expected > 0.0 {
        format!(
            "Loss {} over {words}",
            format_loss(Some(lost * 100.0 / expected))
        )
    } else {
        format!("Loss {DASH} over {words}")
    }];
    let trips: Vec<f64> = reports.iter().filter_map(|sample| sample.rtt_ms).collect();
    let now = window.samples.iter().rev().find_map(|sample| sample.rtt_ms);
    match now {
        Some(now) if !trips.is_empty() => {
            let average = trips.iter().sum::<f64>() / trips.len() as f64;
            parts.push(format!(
                "Round trip {} now, {} on average",
                format_ms(Some(now)),
                format_ms(Some(average))
            ));
        }
        _ => parts.push("Round trip not measured yet".to_owned()),
    }
    let underruns = sum(Field::Underruns) as u64;
    parts.push(if underruns == 0 {
        format!("No underruns in {words}")
    } else {
        format!("{} in {words}", plural(underruns, "underrun", "underruns"))
    });
    let timeouts = sum(Field::Timeouts) as u64;
    if timeouts > 0 {
        parts.push(plural(timeouts, "timeout", "timeouts"));
    }
    let gaps = window.samples.len() - reports.len();
    if gaps > 0 {
        parts.push(format!("{gaps} s without reports"));
    }
    let reconnects = window
        .samples
        .iter()
        .filter(|sample| sample.reconnected)
        .count() as u64;
    if reconnects > 0 {
        parts.push(plural(reconnects, "reconnect", "reconnects"));
    }
    if reports.iter().any(|sample| sample.recovered.is_some()) {
        let recovered = sum(Field::Recovered) as u64;
        if recovered > 0 {
            parts.push(format!(
                "{} recovered by redundancy in {words}",
                plural(recovered, "packet", "packets")
            ));
        } else if musician.redundancy.on {
            parts.push(format!("No packets recovered in {words}"));
        }
    }
    let text = parts.join(" \u{b7} ");
    Summary {
        level,
        label,
        parts,
        text,
    }
}

/// A sample's health in the readout's and the table's words.
fn level_word(health: &str) -> &'static str {
    match health {
        "good" => "Good",
        "warning" => "Warning",
        "bad" => "Poor",
        _ => "No report",
    }
}

/// The hover readout of a second.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tooltip {
    pub time: String,
    pub title: &'static str,
    pub lines: Vec<(&'static str, String)>,
    pub note: Option<&'static str>,
}

/// The readout of `sample`: the values the phone has (the extras only where shown), its local
/// time, and a note for a gap or a reconnect.
pub fn sample_tooltip(sample: &LinkSample, extras: Extras, zone: Zone) -> Tooltip {
    let time = format_clock(sample.at_ms, zone);
    if sample.gap {
        return Tooltip {
            time,
            title: "No report",
            lines: sample
                .rtt_ms
                .map(|rtt| vec![("Round trip", format_ms(Some(rtt)))])
                .unwrap_or_default(),
            note: Some("No report arrived for this second"),
        };
    }
    let mut lines = vec![
        ("Loss", format_loss(sample.loss_percent)),
        ("Round trip", format_ms(sample.rtt_ms)),
    ];
    if extras.jitter {
        lines.push(("Jitter", format_ms(sample.jitter_ms)));
    }
    if extras.cushion {
        lines.push(("Cushion", format_ms(sample.cushion_ms)));
    }
    lines.push(("Underruns", format_count(sample.underruns)));
    lines.push(("Timeouts", format_count(sample.receive_timeouts)));
    if extras.recovered {
        lines.push(("Recovered", format_count(sample.recovered)));
    }
    Tooltip {
        time,
        title: level_word(&sample.health),
        lines,
        note: sample
            .reconnected
            .then_some("Reconnected: the time before it went unmeasured"),
    }
}

/// A second as a table row, every value as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableRow {
    pub key: u64,
    pub time: String,
    pub level: &'static str,
    pub loss: String,
    pub rtt: String,
    pub jitter: String,
    pub cushion: String,
    pub underruns: String,
    pub timeouts: String,
    pub recovered: String,
    pub note: &'static str,
}

/// A column of the table: its header, and the extra it needs (`None` for always).
pub const TABLE_COLUMNS: [(&str, Option<fn(&Extras) -> bool>); 10] = [
    ("Time", None),
    ("Level", None),
    ("Loss", None),
    ("Round trip", None),
    ("Jitter", Some(|extras| extras.jitter)),
    ("Cushion", Some(|extras| extras.cushion)),
    ("Underruns", None),
    ("Timeouts", None),
    ("Recovered", Some(|extras| extras.recovered)),
    ("Note", None),
];

impl TableRow {
    /// The row's cells in [TABLE_COLUMNS]'s order.
    pub fn cells(&self) -> [&str; 10] {
        [
            &self.time,
            self.level,
            &self.loss,
            &self.rtt,
            &self.jitter,
            &self.cushion,
            &self.underruns,
            &self.timeouts,
            &self.recovered,
            self.note,
        ]
    }
}

/// The window's seconds as table rows, the newest first.
pub fn history_table(window: &Window, zone: Zone) -> Vec<TableRow> {
    window
        .samples
        .iter()
        .rev()
        .map(|sample| {
            let gap = sample.gap;
            let kept = |value: Option<f64>| if gap { None } else { value };
            let counted = |value: Option<u64>| if gap { None } else { value };
            TableRow {
                key: sample.seq,
                time: format_clock(sample.at_ms, zone),
                level: if gap {
                    "No report"
                } else {
                    level_word(&sample.health)
                },
                loss: format_loss(kept(sample.loss_percent)),
                rtt: format_ms(sample.rtt_ms),
                jitter: format_ms(kept(sample.jitter_ms)),
                cushion: format_ms(kept(sample.cushion_ms)),
                underruns: format_count(counted(sample.underruns)),
                timeouts: format_count(counted(sample.receive_timeouts)),
                recovered: format_count(counted(sample.recovered)),
                note: if gap {
                    "Gap"
                } else if sample.reconnected {
                    "Reconnected"
                } else {
                    ""
                },
            }
        })
        .collect()
}

/// A range button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeButton {
    pub ms: u64,
    pub label: &'static str,
    pub pressed: bool,
}

/// What the panel shows (`linkHealthView`).
#[derive(Clone, Debug, PartialEq)]
pub struct PanelView {
    /// "Bass · last 5 minutes".
    pub title: String,
    /// The musician's name or address, for the Close button's name.
    pub name: String,
    pub ranges: Vec<RangeButton>,
    pub range_ms: u64,
    pub summary: Summary,
    /// The chart's text alternative.
    pub description: String,
    pub window: Window,
    pub extras: Extras,
    pub extras_note: Option<String>,
    pub recovered_note: Option<&'static str>,
    /// The rows, only while the table shows.
    pub table: Option<Vec<TableRow>>,
    /// Why the last read failed.
    pub message: Option<String>,
    /// Why there is no chart yet.
    pub empty: Option<&'static str>,
    pub zone: Zone,
}

/// The panel for `musician` and the history held for them.
pub fn link_health_view(
    musician: &Musician,
    buffer: Option<&History>,
    range_ms: u64,
    error: Option<&str>,
    show_table: bool,
    zone: Zone,
) -> PanelView {
    let samples = buffer.map_or(&[][..], |buffer| &buffer.samples[..]);
    let window = window_of(samples, range_ms);
    let (_, long) = range_words(range_ms);
    let name = match musician.name.as_deref() {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => musician.address.clone(),
    };
    let extras = extras_reported(musician, samples);
    let extras_note = match (extras.jitter, extras.cushion) {
        (true, true) => None,
        (false, false) => Some("Jitter and cushion"),
        (false, true) => Some("Jitter"),
        (true, false) => Some("Cushion"),
    }
    .map(|missing| format!("{missing}: not reported by this phone (it needs a newer app)."));
    let recovered_note = (musician.redundancy.on && !extras.recovered)
        .then_some("Recovered packets: not reported by this phone (it needs a newer app).");
    let summary = link_summary(&window, range_ms, musician);
    PanelView {
        title: format!("{name} \u{b7} {long}"),
        description: format!(
            "Link health of {name} over the {long}: {}. {}.",
            summary.label, summary.text
        ),
        name,
        ranges: LINK_RANGES
            .iter()
            .map(|range| RangeButton {
                ms: range.ms,
                label: range.label,
                pressed: range.ms == range_ms,
            })
            .collect(),
        range_ms,
        summary,
        table: show_table.then(|| history_table(&window, zone)),
        empty: window.samples.is_empty().then_some(
            "No link reports from this phone yet: the chart starts with its first report.",
        ),
        window,
        extras,
        extras_note,
        recovered_note,
        message: error.map(|error| format!("The link history could not be read: {error}.")),
        zone,
    }
}

#[cfg(test)]
#[path = "link_view_tests.rs"]
mod tests;
