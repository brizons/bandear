//! The chart's tests of `link-health-view.test.js`, ported.

use bandear_desktop_console::wire::LinkSample;

use super::super::link_view::{Extras, Window, window_of};
use super::*;

const UTC: Zone = Zone::Fixed(0);
/// 2026-10-18 21:30:00 UTC.
const T0: u64 = 1_792_359_000_000;

/// A clean second `seq` (its monotonic time `seq` seconds).
fn sample(seq: u64) -> LinkSample {
    LinkSample {
        seq,
        mono_ms: seq * 1000,
        at_ms: T0 + seq * 1000,
        gap: false,
        reconnected: false,
        health: "good".to_owned(),
        interval_ms: Some(1000),
        packets_received: Some(199),
        packets_lost: Some(1),
        loss_percent: Some(0.5),
        underruns: Some(0),
        receive_timeouts: Some(0),
        rtt_ms: Some(10.0),
        jitter_ms: Some(2.0),
        cushion_ms: Some(40.0),
        backlog_ms: Some(35.0),
        recovered: None,
    }
}

fn with(seq: u64, change: impl FnOnce(&mut LinkSample)) -> LinkSample {
    let mut sample = sample(seq);
    change(&mut sample);
    sample
}

fn gap(seq: u64) -> LinkSample {
    LinkSample {
        gap: true,
        health: "unknown".to_owned(),
        interval_ms: None,
        packets_received: None,
        packets_lost: None,
        loss_percent: None,
        underruns: None,
        receive_timeouts: None,
        jitter_ms: None,
        cushion_ms: None,
        backlog_ms: None,
        ..sample(seq)
    }
}

fn seconds(from: u64, to: u64, make: impl Fn(u64) -> LinkSample) -> Vec<LinkSample> {
    (from..=to).map(make).collect()
}

fn both() -> Extras {
    Extras {
        jitter: true,
        cushion: true,
        recovered: false,
    }
}

#[test]
fn a_nice_scale_rounds_up_and_never_goes_below_its_floor() {
    assert_eq!(nice_max(0.0, 1.0), 1.0);
    assert_eq!(nice_max(0.3, 1.0), 1.0);
    assert_eq!(nice_max(1.3, 1.0), 2.0);
    assert_eq!(nice_max(3.1, 1.0), 5.0);
    assert_eq!(nice_max(23.0, 10.0), 25.0);
    assert_eq!(nice_max(61.0, 10.0), 100.0);
}

fn panel<'a>(layout: &'a Layout, id: &str) -> &'a Panel {
    layout
        .panels
        .iter()
        .find(|panel| panel.id == id)
        .expect("the panel")
}

#[test]
fn the_lines_break_at_gaps_and_reconnects_never_drawn_across_them() {
    let mut samples = seconds(1, 10, sample);
    samples.extend(seconds(11, 13, gap));
    samples.extend(seconds(14, 20, sample));
    samples.push(with(40, |sample| sample.reconnected = true));
    samples.extend(seconds(41, 45, sample));
    let layout = health_layout(&window_of(&samples, 60_000), 672.0, both(), 60_000, UTC);

    assert_eq!(
        panel(&layout, "loss").lines.len(),
        3,
        "three runs: before the gap, after it, after the reconnect"
    );
    assert_eq!(layout.gaps.len(), 1, "one band of seconds without reports");
    assert!(layout.gaps[0].width > 0.0);
    assert_eq!(layout.reconnects.len(), 1);
    assert_eq!(layout.reconnects[0].label, Some("Reconnected"));
    let plot = layout.plot;
    assert_eq!(
        plot.left + plot.width,
        672.0 - 12.0,
        "the plot spans the range"
    );
    let expected = plot.left + plot.width * ((40_000.0 - (45_000.0 - 60_000.0)) / 60_000.0);
    assert!((layout.reconnects[0].x - expected).abs() < 0.01);
}

#[test]
fn an_isolated_second_between_gaps_is_a_dot_and_a_missing_value_breaks_the_line() {
    let mut samples = seconds(1, 3, gap);
    samples.push(sample(4));
    samples.extend(seconds(5, 7, gap));
    samples.push(sample(8));
    samples.push(with(9, |sample| sample.rtt_ms = None));
    samples.push(sample(10));
    let layout = health_layout(&window_of(&samples, 60_000), 600.0, both(), 60_000, UTC);

    assert_eq!(panel(&layout, "loss").dots.len(), 1, "the second alone");
    let rtt = panel(&layout, "rtt");
    assert_eq!(rtt.dots.len(), 3, "4, 8 and 10 stand alone");
    assert!(rtt.lines.is_empty(), "no two neighbours to join");
}

#[test]
fn the_panels_are_small_multiples_on_one_time_axis_loss_round_trip_then_the_extras() {
    let samples = seconds(1, 30, |seq| {
        with(seq, |sample| {
            sample.loss_percent = Some(if seq == 10 { 3.2 } else { 0.0 });
            sample.rtt_ms = Some(12.0);
        })
    });
    let window = window_of(&samples, 60_000);

    let extras = health_layout(&window, 600.0, both(), 60_000, UTC);
    let ids: Vec<_> = extras.panels.iter().map(|panel| panel.id).collect();
    assert_eq!(ids, ["loss", "rtt", "jitter", "cushion"]);
    let loss = &extras.panels[0];
    assert_eq!(loss.title, "Loss (%)");
    let labels: Vec<_> = loss.ticks.iter().map(|tick| tick.label.as_str()).collect();
    assert_eq!(labels, ["0", "5"]);
    for panel in &extras.panels[1..] {
        assert!(
            panel.top > extras.panels[0].top,
            "stacked, each with its own scale"
        );
    }

    let older = health_layout(&window, 600.0, Extras::default(), 60_000, UTC);
    let ids: Vec<_> = older.panels.iter().map(|panel| panel.id).collect();
    assert_eq!(ids, ["loss", "rtt"]);
    assert!(older.height < extras.height);
    assert_eq!(older.height, 234.0, "two panels");
    assert_eq!(extras.height, 394.0, "four panels");
}

#[test]
fn underruns_and_timeouts_are_event_marks_in_their_own_lanes() {
    let samples = seconds(1, 20, |seq| {
        with(seq, |sample| {
            sample.underruns = Some(if seq % 5 == 0 { 2 } else { 0 });
            sample.receive_timeouts = Some(u64::from(seq == 7));
        })
    });
    let layout = health_layout(
        &window_of(&samples, 60_000),
        600.0,
        Extras::default(),
        60_000,
        UTC,
    );
    let [underruns, timeouts] = &layout.lanes[..] else {
        panic!("two lanes");
    };
    assert_eq!(underruns.label, "Underruns");
    assert_eq!(underruns.marks.len(), 4);
    assert_eq!(underruns.marks[0].count, 2);
    assert_eq!(timeouts.label, "Timeouts");
    assert_eq!(timeouts.marks.len(), 1);
    assert!(timeouts.top > underruns.top);
}

#[test]
fn the_time_axis_counts_back_from_the_newest_second_and_ends_at_its_local_time() {
    let samples = seconds(1, 600, sample);
    let labels = |range: u64| -> Vec<String> {
        health_layout(
            &window_of(&samples, range),
            800.0,
            Extras::default(),
            range,
            UTC,
        )
        .time_ticks
        .into_iter()
        .map(|tick| tick.label)
        .collect()
    };
    assert_eq!(
        labels(600_000),
        [
            "\u{2212}10 min",
            "\u{2212}8 min",
            "\u{2212}6 min",
            "\u{2212}4 min",
            "\u{2212}2 min",
            "21:40:00"
        ]
    );
    assert_eq!(
        labels(60_000),
        [
            "\u{2212}60 s",
            "\u{2212}45 s",
            "\u{2212}30 s",
            "\u{2212}15 s",
            "21:40:00"
        ]
    );
    let one = health_layout(
        &window_of(&samples, 60_000),
        800.0,
        Extras::default(),
        60_000,
        UTC,
    );
    assert_eq!(
        one.time_ticks.last().map(|tick| tick.anchor),
        Some(Anchor::End),
        "the newest label stays inside the chart"
    );
    assert_eq!(
        one.time_ticks[0].anchor,
        Anchor::Start,
        "the first at the left edge"
    );
}

#[test]
fn a_reconnect_label_near_the_right_edge_reads_leftwards_and_close_ones_are_not_labelled_twice() {
    let mut samples = seconds(1, 50, sample);
    samples.push(with(58, |sample| sample.reconnected = true));
    samples.push(with(59, |sample| sample.reconnected = true));
    samples.push(sample(60));
    let layout = health_layout(
        &window_of(&samples, 60_000),
        600.0,
        Extras::default(),
        60_000,
        UTC,
    );

    assert_eq!(layout.reconnects.len(), 2, "both are marked");
    assert_eq!(layout.reconnects[0].anchor, Anchor::End);
    assert_eq!(
        layout.reconnects[1].label, None,
        "too close to the last label"
    );
}

#[test]
fn the_hover_finds_the_nearest_second_to_the_pointer() {
    let window = window_of(&seconds(1, 60, sample), 60_000);
    let layout = health_layout(&window, 612.0, Extras::default(), 60_000, UTC);
    let Plot { left, width, .. } = layout.plot;

    assert_eq!(
        nearest_index(&window, &layout, left + width),
        Some(59),
        "the newest"
    );
    assert_eq!(
        nearest_index(&window, &layout, left + width - width / 60.0 - 1.0),
        Some(58)
    );
    assert_eq!(
        nearest_index(&window, &layout, left - 40.0),
        None,
        "outside the plot"
    );
    assert_eq!(nearest_index(&Window::default(), &layout, left), None);
}
