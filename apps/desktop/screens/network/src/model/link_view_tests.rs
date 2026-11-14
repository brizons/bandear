//! `link-health-view.test.js`, ported (the follower's tests are with the follower).

use bandear_desktop_console::wire::Redundancy;

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

fn read(history_id: u64, samples: Vec<LinkSample>) -> HistoryRead {
    HistoryRead {
        history_id,
        samples,
        max_samples: 600,
    }
}

fn musician() -> Musician {
    Musician {
        address: "192.168.1.12:50000".to_owned(),
        client_id: Some("a".to_owned()),
        name: Some("Bass".to_owned()),
        control_connected: true,
        link_health: "warning".to_owned(),
        loss_percent: Some(0.8),
        underruns_per_second: Some(0.2),
        receive_timeouts_per_second: Some(0.0),
        packets_per_second: Some(200.0),
        capabilities: vec!["stats.v2".to_owned()],
        ..Musician::default()
    }
}

fn both() -> Extras {
    Extras {
        jitter: true,
        cushion: true,
        recovered: false,
    }
}

#[test]
fn the_ranges_are_1_5_and_10_minutes_ten_by_default() {
    let ranges: Vec<_> = LINK_RANGES.iter().map(|range| range.ms).collect();
    assert_eq!(ranges, [60_000, 300_000, 600_000]);
    assert_eq!(DEFAULT_RANGE_MS, 600_000);
}

#[test]
fn new_samples_are_appended_after_the_cursor_bounded_to_the_ring() {
    let first = history_update(None, &read(7, seconds(1, 600, sample)), None);
    assert!(!first.refetch);
    let held = first.buffer.expect("a history");
    assert_eq!(held.samples.len(), 600);
    assert_eq!(cursor_of(Some(&held)), Some(600));
    assert_eq!(cursor_of(None), None);

    let next = history_update(Some(&held), &read(7, seconds(601, 603, sample)), Some(600));
    let after = next.buffer.expect("a history");
    assert_eq!(after.samples.len(), 600, "never more than the ring");
    assert_eq!(after.samples[0].seq, 4, "the oldest went");
    assert_eq!(cursor_of(Some(&after)), Some(603));
    assert!(next.changed);

    let repeated = history_update(Some(&after), &read(7, seconds(602, 603, sample)), Some(601));
    assert_eq!(
        repeated.buffer.expect("a history").samples.len(),
        600,
        "a sample already held is not added twice"
    );
    assert!(!repeated.changed);
}

#[test]
fn another_history_id_an_engine_restart_asks_for_the_whole_history_again() {
    let held = history_update(None, &read(7, seconds(1, 50, sample)), None)
        .buffer
        .expect("a history");

    let partial = history_update(Some(&held), &read(8, seconds(3, 4, sample)), Some(50));
    assert!(
        partial.refetch,
        "after another history's cursor is not the whole of it"
    );
    assert_eq!(
        partial.buffer.as_ref(),
        Some(&held),
        "kept until the whole one arrives"
    );

    let whole = history_update(Some(&held), &read(8, seconds(1, 4, sample)), None);
    assert!(!whole.refetch);
    let whole = whole.buffer.expect("a history");
    let seqs: Vec<_> = whole.samples.iter().map(|sample| sample.seq).collect();
    assert_eq!(seqs, [1, 2, 3, 4]);
    assert_eq!(whole.history_id, 8);
}

#[test]
fn the_window_is_the_range_up_to_the_newest_sample_on_the_monotonic_clock() {
    let window = window_of(&seconds(1, 600, sample), 60_000);
    assert_eq!(window.end_ms, Some(600_000));
    assert_eq!(window.start_ms, Some(540_000));
    assert_eq!(window.samples.len(), 60, "the newest minute");
    assert_eq!(window.samples[0].seq, 541);
    assert_eq!(window_of(&[], 60_000), Window::default());
}

#[test]
fn the_summary_gives_the_level_the_windows_loss_the_round_trip_and_the_underruns() {
    let mut samples = seconds(1, 8, |seq| {
        with(seq, |sample| {
            sample.packets_received = Some(198);
            sample.packets_lost = Some(2);
            sample.rtt_ms = Some(10.0);
        })
    });
    samples.push(with(9, |sample| {
        sample.packets_received = Some(196);
        sample.packets_lost = Some(4);
        sample.underruns = Some(1);
        sample.rtt_ms = Some(20.0);
    }));
    samples.extend(seconds(10, 11, gap));
    samples.push(with(12, |sample| {
        sample.reconnected = true;
        sample.packets_received = Some(200);
        sample.packets_lost = Some(0);
        sample.underruns = Some(2);
        sample.receive_timeouts = Some(1);
        sample.rtt_ms = Some(30.0);
    }));
    let summary = link_summary(&window_of(&samples, 300_000), 300_000, &musician());

    assert_eq!(summary.level, LinkLevel::Warning);
    assert_eq!(summary.label, "Warning");
    assert_eq!(
        summary.parts,
        [
            "Loss 1.00% over 5 min",
            "Round trip 30.0 ms now, 13.0 ms on average",
            "3 underruns in 5 min",
            "1 timeout",
            "2 s without reports",
            "1 reconnect",
        ]
    );
    assert_eq!(summary.text, summary.parts.join(" \u{b7} "));
}

#[test]
fn the_level_is_the_one_the_musicians_table_shows() {
    let lost = Musician {
        link_health: "bad".to_owned(),
        loss_percent: None,
        underruns_per_second: None,
        ..musician()
    };
    let summary = link_summary(&window_of(&[sample(1)], 60_000), 60_000, &lost);
    assert_eq!(
        (summary.level, summary.label.as_str()),
        (LinkLevel::Bad, "Link lost")
    );
}

#[test]
fn a_quiet_window_says_so_in_the_summary() {
    let samples = seconds(1, 5, |seq| {
        with(seq, |sample| {
            sample.packets_lost = Some(0);
            sample.packets_received = Some(200);
            sample.rtt_ms = None;
        })
    });
    let good = Musician {
        link_health: "good".to_owned(),
        underruns_per_second: Some(0.0),
        ..musician()
    };
    assert_eq!(
        link_summary(&window_of(&samples, 60_000), 60_000, &good).parts,
        [
            "Loss 0.00% over 1 min",
            "Round trip not measured yet",
            "No underruns in 1 min"
        ]
    );
    assert_eq!(
        link_summary(&Window::default(), 60_000, &musician()).parts,
        ["No link reports yet"]
    );
}

#[test]
fn the_tooltip_leads_with_the_values_and_the_samples_local_time() {
    let tip = sample_tooltip(
        &with(5, |sample| {
            sample.underruns = Some(1);
            sample.reconnected = true;
        }),
        both(),
        UTC,
    );
    assert_eq!(tip.time, "21:30:05");
    assert_eq!(tip.title, "Good");
    let lines: Vec<_> = tip
        .lines
        .iter()
        .map(|(label, value)| (*label, value.as_str()))
        .collect();
    assert_eq!(
        lines,
        [
            ("Loss", "0.50%"),
            ("Round trip", "10.0 ms"),
            ("Jitter", "2.0 ms"),
            ("Cushion", "40.0 ms"),
            ("Underruns", "1"),
            ("Timeouts", "0"),
        ]
    );
    assert_eq!(
        tip.note,
        Some("Reconnected: the time before it went unmeasured")
    );

    let missing = sample_tooltip(&gap(6), Extras::default(), UTC);
    assert_eq!(missing.title, "No report");
    assert_eq!(missing.lines, [("Round trip", "10.0 ms".to_owned())]);
    assert_eq!(missing.note, Some("No report arrived for this second"));
}

#[test]
fn the_table_lists_the_windows_seconds_the_newest_first() {
    let rows = history_table(
        &window_of(
            &[sample(1), gap(2), with(3, |sample| sample.jitter_ms = None)],
            60_000,
        ),
        UTC,
    );
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows[0],
        TableRow {
            key: 3,
            time: "21:30:03".to_owned(),
            level: "Good",
            loss: "0.50%".to_owned(),
            rtt: "10.0 ms".to_owned(),
            jitter: "\u{2014}".to_owned(),
            cushion: "40.0 ms".to_owned(),
            underruns: "0".to_owned(),
            timeouts: "0".to_owned(),
            recovered: "\u{2014}".to_owned(),
            note: "",
        }
    );
    assert_eq!(rows[1].level, "No report");
    assert_eq!(rows[1].note, "Gap");
    assert_eq!(rows[1].rtt, "10.0 ms", "a gap keeps its round trip");
}

#[test]
fn jitter_and_cushion_are_shown_for_a_phone_that_announces_stats_v2_or_reports_them() {
    let phone = |capabilities: &[&str]| Musician {
        capabilities: capabilities
            .iter()
            .map(|capability| (*capability).to_owned())
            .collect(),
        ..musician()
    };
    assert_eq!(
        extras_reported(&phone(&["stats.v2"]), &[]),
        Extras {
            jitter: true,
            cushion: true,
            recovered: false
        }
    );
    let none = with(1, |sample| {
        sample.jitter_ms = None;
        sample.cushion_ms = None;
    });
    assert_eq!(extras_reported(&phone(&[]), &[none]), Extras::default());
    let jitter = with(1, |sample| sample.cushion_ms = None);
    assert_eq!(
        extras_reported(&phone(&[]), &[jitter]),
        Extras {
            jitter: true,
            cushion: false,
            recovered: false
        }
    );
}

#[test]
fn the_panels_view_has_a_title_with_the_range_the_range_buttons_the_summary_and_a_note() {
    let samples = seconds(1, 30, |seq| {
        with(seq, |sample| {
            sample.jitter_ms = None;
            sample.cushion_ms = None;
        })
    });
    let buffer = History {
        history_id: 1,
        samples,
    };
    let older = Musician {
        capabilities: Vec::new(),
        ..musician()
    };
    let view = link_health_view(&older, Some(&buffer), 300_000, None, false, UTC);

    assert_eq!(view.title, "Bass \u{b7} last 5 minutes");
    let ranges: Vec<_> = view
        .ranges
        .iter()
        .map(|range| (range.label, range.pressed))
        .collect();
    assert_eq!(
        ranges,
        [("1 min", false), ("5 min", true), ("10 min", false)]
    );
    assert_eq!(view.summary.level, LinkLevel::Warning);
    assert_eq!(
        view.extras_note.as_deref(),
        Some("Jitter and cushion: not reported by this phone (it needs a newer app).")
    );
    assert_eq!(view.table, None, "only when asked for");
    assert_eq!(view.message, None);
    assert!(
        view.description
            .starts_with("Link health of Bass over the last 5 minutes: Warning. ")
    );

    let failing = link_health_view(
        &musician(),
        None,
        60_000,
        Some("the engine is not running"),
        true,
        UTC,
    );
    assert_eq!(
        failing.message.as_deref(),
        Some("The link history could not be read: the engine is not running.")
    );
    assert_eq!(failing.extras_note, None);
    assert_eq!(failing.table, Some(Vec::new()));
    assert_eq!(
        failing.empty,
        Some("No link reports from this phone yet: the chart starts with its first report.")
    );
    let cushion_only = link_health_view(
        &older,
        Some(&History {
            history_id: 1,
            samples: vec![with(1, |sample| sample.jitter_ms = None)],
        }),
        60_000,
        None,
        false,
        UTC,
    );
    assert_eq!(
        cushion_only.extras_note.as_deref(),
        Some("Jitter: not reported by this phone (it needs a newer app).")
    );
}

#[test]
fn a_clock_time_is_local_to_the_given_zone_to_the_second() {
    assert_eq!(format_clock(T0 + 65_000, UTC), "21:31:05");
}

#[test]
fn recovered_packets_are_shown_for_a_phone_that_reports_them_and_said_as_not_reported_otherwise() {
    let reporting = Musician {
        capabilities: vec!["stats.v2".to_owned(), "redundancy.recovered".to_owned()],
        redundancy: Redundancy {
            on: true,
            delay_blocks: Some(2),
        },
        ..musician()
    };
    assert!(extras_reported(&reporting, &[]).recovered, "announced");
    let none = Musician {
        capabilities: Vec::new(),
        ..musician()
    };
    assert!(
        extras_reported(&none, &[with(1, |sample| sample.recovered = Some(0))]).recovered,
        "reported"
    );

    let mut samples = seconds(1, 3, |seq| with(seq, |sample| sample.recovered = Some(2)));
    samples.push(gap(4));
    samples.push(with(5, |sample| sample.recovered = Some(1)));
    let window = window_of(&samples, 60_000);
    assert!(
        link_summary(&window, 60_000, &reporting)
            .parts
            .contains(&"7 packets recovered by redundancy in 1 min".to_owned())
    );
    let quiet = seconds(1, 3, |seq| with(seq, |sample| sample.recovered = Some(0)));
    assert!(
        link_summary(&window_of(&quiet, 60_000), 60_000, &reporting)
            .parts
            .contains(&"No packets recovered in 1 min".to_owned())
    );
    let off = Musician {
        redundancy: Redundancy::default(),
        ..reporting.clone()
    };
    assert!(
        !link_summary(&window_of(&quiet, 60_000), 60_000, &off)
            .parts
            .iter()
            .any(|part| part.contains("recovered")),
        "nothing to say with redundancy off and nothing recovered"
    );

    let extras = Extras {
        jitter: true,
        cushion: true,
        recovered: true,
    };
    let tip = sample_tooltip(&with(5, |sample| sample.recovered = Some(3)), extras, UTC);
    assert_eq!(tip.lines.last(), Some(&("Recovered", "3".to_owned())));
    assert_eq!(
        history_table(
            &window_of(&[with(1, |sample| sample.recovered = Some(3))], 60_000),
            UTC
        )[0]
        .recovered,
        "3"
    );

    let view = link_health_view(
        &reporting,
        Some(&History {
            history_id: 1,
            samples,
        }),
        60_000,
        None,
        false,
        UTC,
    );
    assert_eq!(view.recovered_note, None);
    let older = Musician {
        redundancy: Redundancy {
            on: true,
            delay_blocks: Some(2),
        },
        ..musician()
    };
    let three = History {
        history_id: 1,
        samples: seconds(1, 3, sample),
    };
    assert_eq!(
        link_health_view(&older, Some(&three), 60_000, None, false, UTC).recovered_note,
        Some("Recovered packets: not reported by this phone (it needs a newer app).")
    );
    assert_eq!(
        link_health_view(&musician(), Some(&three), 60_000, None, false, UTC).recovered_note,
        None,
        "redundancy off: nothing to report"
    );
}
