//! The once-a-second link report: what the phone experienced over one interval, and the
//! accumulator that collects the receive loop's counts into it.
//!
//! Ported from the iOS client's `LinkStatsAccumulator.swift`, itself a port of the Android client's
//! `LinkStats.kt`. This is the phone's own measurement, not the wire message: the `stats` message
//! that carries it to the desk checks and rounds these values on the way out.

/// What the phone experienced over one report interval: the counts the desk classifies the link
/// from. `packets_received` counts the packets played, so a dropped duplicate or late packet is
/// not received twice, and received plus lost is the stretch of the stream the interval covered.
///
/// The extras the phone announces with `stats.v2`, in milliseconds, are `None` when not measured
/// (Android's `NaN`):
/// - `jitter_ms`: how much the packets' arrival varied over the interval, the mean distance of each
///   packet's spacing from the smoothed spacing. Not measured when no two packets arrived in the
///   interval.
/// - `cushion_ms`: the playback cushion the phone aims for at the interval's end, not measured
///   before the first packet opened the player.
/// - `backlog_ms`: the audio queued for playback at the interval's end, as last read from the
///   player.
///
/// `recovered` (announced with `redundancy.recovered`) counts the packets over the interval that
/// were played after filling a gap: each arrived for the first time after a newer one, while the
/// receiver held that newer one back for the desk's delayed copies
/// ([`crate::reorder_hold::ReorderHold::recovered`]), which only a delayed redundant copy or Wi-Fi
/// reordering does. They are in `packets_received`, not in `packets_lost`. A late packet that could
/// not be played (no hold, or past it) is not counted, nor is an immediate copy, which cannot be
/// told from its original.
///
/// Plain values only, so building one allocates nothing on the receive thread. The default is an
/// empty report with every extra unmeasured.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LinkStatsReport {
    pub interval_ms: i64,
    pub packets_received: i64,
    pub packets_lost: i64,
    pub underruns: i64,
    pub receive_timeouts: i64,
    pub jitter_ms: Option<f64>,
    pub cushion_ms: Option<f64>,
    pub backlog_ms: Option<f64>,
    pub recovered: i64,
}

/// Collects the receive loop's counts into one [`LinkStatsReport`] about once a second.
///
/// A line-for-line port, so both phones report the same Wi-Fi the same way to the engineer's
/// screen.
///
/// Time is passed in by the caller, in milliseconds of a monotonic clock, so the accumulator is
/// pure and its timing is tested without waiting. The player's underrun count is cumulative, so
/// the accumulator keeps the last value it saw and reports the difference; the playback cushion
/// keeps its own last value and is not affected.
///
/// Reports are held back until the stream starts (the first packet is played) or
/// [`LinkStatsAccumulator::STARTUP_GRACE_MS`] has passed since the accumulator was created,
/// whichever comes first; from then on one is due every [`LinkStatsAccumulator::INTERVAL_MS`]:
/// - The stream starts within the grace: the first interval starts at the first packet, so the
///   first report comes one interval later and describes the stream alone. The silence before it
///   was startup, not the link, and is not reported.
/// - The grace passes with no packet: the first report covers everything since the start (the
///   grace, or longer when the poll is late, clamped like any interval). It carries no packets, no
///   timeouts and no underruns, which is the finding: the phone heard nothing for that long.
/// - A stream that starts after the grace joins the interval already running.
///
/// Single-threaded: it lives on the receive thread, and between reports it only bumps counters.
/// Plain fields, so it allocates nothing.
#[derive(Debug, Clone)]
pub struct LinkStatsAccumulator {
    started_at_ms: i64,
    interval_started_at_ms: i64,
    packets_received: i64,
    packets_lost: i64,
    receive_timeouts: i64,
    last_underruns: i64,
    last_recovered: i64,
    stream_started: bool,
    arrival_deviation_sum_ms: f64,
    arrival_deviations: i64,
}

impl LinkStatsAccumulator {
    /// How often the phone reports its link to the desk: once a second while the receiver runs.
    pub const INTERVAL_MS: i64 = 1_000;

    /// The longest interval one report may cover; the desk refuses a report that claims more.
    pub const MAXIMUM_INTERVAL_MS: i64 = 60_000;

    /// How long the phone waits for its first packet before it reports a link that hears nothing.
    ///
    /// The receiver starts before the desk knows where to send audio, and the desk judges a report
    /// with no packets as bad, so a report sent before the audio arrives would show a healthy phone
    /// as red. Three seconds covers connecting, registering and the first packets, and matches how
    /// long the desk keeps a report fresh.
    pub const STARTUP_GRACE_MS: i64 = 3_000;

    /// An accumulator for a receiver started at `started_at_ms`, the clock every later call reads
    /// from.
    pub fn new(started_at_ms: i64) -> Self {
        Self {
            started_at_ms,
            interval_started_at_ms: started_at_ms,
            packets_received: 0,
            packets_lost: 0,
            receive_timeouts: 0,
            last_underruns: 0,
            last_recovered: 0,
            stream_started: false,
            arrival_deviation_sum_ms: 0.0,
            arrival_deviations: 0,
        }
    }

    /// One played packet, arriving at `now_ms`, with the `lost_before` packets the stream skipped
    /// right before it.
    pub fn on_packet(&mut self, lost_before: i64, now_ms: i64) {
        if !self.stream_started && !self.grace_passed(now_ms) {
            // Reports start with the stream: the first interval runs from this packet.
            self.interval_started_at_ms = now_ms;
        }
        self.stream_started = true;
        self.packets_received += 1;
        self.packets_lost += lost_before;
    }

    /// One receive timeout: a wait for the next packet that ended with nothing.
    ///
    /// Counted only once the stream has started (a packet was played). The receiver starts before
    /// the desk knows where to send audio, so a wait before the first packet says nothing about the
    /// link, and counting it would show a phone that connects cleanly as red for its first second.
    /// Once started, the stream stays started: a wait while a restarted desk comes back is a real
    /// gap in the audio and is counted.
    pub fn on_receive_timeout(&mut self) {
        if self.stream_started {
            self.receive_timeouts += 1;
        }
    }

    /// How far one packet's spacing from the one before it was from the smoothed spacing, in
    /// milliseconds, early or late alike: the interval's jitter is the mean of these.
    pub fn on_arrival_deviation(&mut self, deviation_ms: f64) {
        self.arrival_deviation_sum_ms += deviation_ms.abs();
        self.arrival_deviations += 1;
    }

    /// True once a report is due at `now_ms`, so the caller reads the underrun count only then.
    pub fn is_due(&self, now_ms: i64) -> bool {
        (self.stream_started || self.grace_passed(now_ms))
            && now_ms - self.interval_started_at_ms >= Self::INTERVAL_MS
    }

    fn grace_passed(&self, now_ms: i64) -> bool {
        now_ms - self.started_at_ms >= Self::STARTUP_GRACE_MS
    }

    /// The report for the interval ending at `now_ms`, or `None` when one is not due yet.
    ///
    /// The interval is the real time since the last report, clamped to what the desk accepts, so a
    /// late poll (the loop waits up to one receive timeout) is reported as the longer interval it
    /// was and the desk's rates stay honest. The interval counts start again from zero.
    ///
    /// `cushion_ms` and `backlog_ms` are the playback values at `now_ms`, `None` when not measured,
    /// and are carried as given; the jitter is the interval's own (see [`LinkStatsReport`]).
    /// `cumulative_underruns` is the player's count and `cumulative_recovered` the hold's
    /// ([`crate::reorder_hold::ReorderHold::recovered`]); the report carries each one's difference
    /// since the last report.
    ///
    /// Android's jitter is `NaN` when a deviation was not a number, and infinite when one was
    /// infinite; its message leaves either out (`ControlChannel.kt:586-591`). Here such an
    /// interval's jitter is `None`, so the report never holds a `NaN`.
    pub fn poll(
        &mut self,
        now_ms: i64,
        cumulative_underruns: i64,
        cushion_ms: Option<f64>,
        backlog_ms: Option<f64>,
        cumulative_recovered: i64,
    ) -> Option<LinkStatsReport> {
        if !self.is_due(now_ms) {
            return None;
        }
        // A count that went down belongs to a new track, which starts from zero.
        let underruns = (cumulative_underruns - self.last_underruns).max(0);
        self.last_underruns = cumulative_underruns;
        // Likewise a recovered count that went down belongs to a new hold.
        let recovered = if cumulative_recovered >= self.last_recovered {
            cumulative_recovered - self.last_recovered
        } else {
            cumulative_recovered
        };
        self.last_recovered = cumulative_recovered;
        let elapsed = now_ms - self.interval_started_at_ms;
        let report = LinkStatsReport {
            interval_ms: elapsed.clamp(1, Self::MAXIMUM_INTERVAL_MS),
            packets_received: self.packets_received,
            packets_lost: self.packets_lost,
            underruns,
            receive_timeouts: self.receive_timeouts,
            jitter_ms: self.mean_arrival_deviation(),
            cushion_ms,
            backlog_ms,
            recovered,
        };
        self.interval_started_at_ms = now_ms;
        self.packets_received = 0;
        self.packets_lost = 0;
        self.receive_timeouts = 0;
        self.arrival_deviation_sum_ms = 0.0;
        self.arrival_deviations = 0;
        Some(report)
    }

    /// The interval's jitter: `None` when no deviation came, or when one was not a finite number.
    fn mean_arrival_deviation(&self) -> Option<f64> {
        if self.arrival_deviations == 0 {
            return None;
        }
        let mean = self.arrival_deviation_sum_ms / self.arrival_deviations as f64;
        mean.is_finite().then_some(mean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Accumulator = LinkStatsAccumulator;

    const GRACE: i64 = Accumulator::STARTUP_GRACE_MS;

    /// The report a poll answered, or the test stops there.
    #[track_caller]
    fn due(report: Option<LinkStatsReport>) -> LinkStatsReport {
        report.expect("no report was due")
    }

    /// A poll with the arguments Android's tests leave at their defaults: no playback extras and
    /// no recovered packets.
    fn poll(
        stats: &mut Accumulator,
        now_ms: i64,
        cumulative_underruns: i64,
    ) -> Option<LinkStatsReport> {
        stats.poll(now_ms, cumulative_underruns, None, None, 0)
    }

    /// An accumulator whose stream started at `started_at`, with its first packet played then.
    fn streaming(started_at: i64) -> Accumulator {
        let mut stats = Accumulator::new(started_at);
        stats.on_packet(0, started_at);
        stats
    }

    // What only the Swift port had to answer, beside the Android cases below:
    // - A measurement not taken is `None`, never `NaN`, so a non-finite arrival deviation must not
    //   reach the report as a number.
    // - The interval clamp at both ends, which Android pins only at the top.
    // - A track's underrun count that went down, a rule `LinkStats.kt:153-155` states and no
    //   Android test exercises.

    /// The Swift test also checks the maximum against the wire message's own limit
    /// (`LinkStats.maximumIntervalMilliseconds`); that message is ported with the control codecs
    /// (T7), which will check it against this constant.
    #[test]
    fn timing() {
        assert_eq!(Accumulator::INTERVAL_MS, 1_000);
        assert_eq!(Accumulator::MAXIMUM_INTERVAL_MS, 60_000);
        assert_eq!(Accumulator::STARTUP_GRACE_MS, 3_000);
    }

    // `None`, never `NaN`.

    #[test]
    fn extras_pass_through() {
        let mut stats = streaming(0);

        let first = due(stats.poll(1_000, 0, Some(12.5), None, 0));
        assert_eq!(first.cushion_ms, Some(12.5));
        assert_eq!(first.backlog_ms, None);

        let second = due(stats.poll(2_000, 0, None, Some(40.0), 0));
        assert_eq!(
            second.cushion_ms, None,
            "the cushion is the value at the interval's end"
        );
        assert_eq!(second.backlog_ms, Some(40.0));
    }

    #[test]
    fn zero_jitter_is_measured() {
        let mut stats = streaming(0);
        stats.on_arrival_deviation(0.0);
        stats.on_arrival_deviation(-0.0);

        let report = due(poll(&mut stats, 1_000, 0));
        assert_eq!(report.jitter_ms, Some(0.0));
    }

    /// Android's mean turns `NaN` or infinite, and its message writer leaves it out
    /// (`ControlChannel.kt:586-591`). Here the report itself says unmeasured, for that interval
    /// only.
    #[test]
    fn non_finite_deviation() {
        for deviation in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut stats = streaming(0);
            stats.on_arrival_deviation(2.0);
            stats.on_arrival_deviation(deviation);

            let spoiled = due(poll(&mut stats, 1_000, 0));
            assert_eq!(spoiled.jitter_ms, None, "{deviation}");
            assert_eq!(
                spoiled.packets_received, 1,
                "the counts are reported all the same"
            );

            stats.on_arrival_deviation(1.5);
            let next = due(poll(&mut stats, 2_000, 0));
            assert_eq!(
                next.jitter_ms,
                Some(1.5),
                "the next interval measures afresh"
            );
        }
    }

    // The interval clamp, both ends.

    #[test]
    fn clamps_the_longest_interval() {
        let mut stats = streaming(0);

        let at_limit = due(poll(&mut stats, 60_000, 0));
        assert_eq!(at_limit.interval_ms, 60_000);

        let past_limit = due(poll(&mut stats, 120_001, 0));
        assert_eq!(past_limit.interval_ms, 60_000);

        // The clamp shapes the report, not the clock: the next interval runs from that poll.
        assert_eq!(poll(&mut stats, 121_000, 0), None);
        let next = due(poll(&mut stats, 121_001, 0));
        assert_eq!(next.interval_ms, 1_000);
    }

    /// Android also clamps from below, to 1 ms. No due report can reach that clamp, since a report
    /// is due only a whole interval on, so what is pinned is what the clamp exists for: no report
    /// ever claims less than an interval, and a clock that went backwards reports nothing.
    #[test]
    fn clamps_the_shortest_interval() {
        let mut stats = streaming(5_000);

        assert_eq!(poll(&mut stats, 5_999, 0), None);
        let first = due(poll(&mut stats, 6_000, 0));
        assert_eq!(first.interval_ms, Accumulator::INTERVAL_MS);

        assert!(!stats.is_due(5_000));
        assert_eq!(poll(&mut stats, 5_000, 0), None);
        let next = due(poll(&mut stats, 7_000, 0));
        assert_eq!(next.interval_ms, 1_000, "measured from the last report");
    }

    // Underruns.

    #[test]
    fn underruns_of_a_new_track() {
        let mut stats = streaming(0);

        assert_eq!(poll(&mut stats, 1_000, 5).map(|r| r.underruns), Some(5));
        // A count that went down belongs to a new track, which starts from zero.
        assert_eq!(poll(&mut stats, 2_000, 2).map(|r| r.underruns), Some(0));
        assert_eq!(poll(&mut stats, 3_000, 6).map(|r| r.underruns), Some(4));
    }

    /// The Android client's own tests for the once-a-second link report, ported case for case from
    /// `apps/android/app/src/test/java/com/bandear/android/LinkStatsTest.kt` through
    /// `LinkStatsParityTests.swift`: when a report is due, what interval it claims and which counts
    /// it carries. Same calls, same clock readings, same expected reports. Time is injected, so
    /// nothing here waits.
    ///
    /// One translation, deliberate: Android marks an extra it did not measure as `NaN`; here it is
    /// `None`. Android's `assertEquals` on two reports holds `NaN` equal to `NaN` (a data class
    /// compares its doubles with `Double.compare`), so its whole-report assertions read here as the
    /// same report with `None` extras.
    mod android_parity {
        use super::{due, poll, Accumulator, GRACE};
        use crate::link_stats::LinkStatsReport;

        /// A report with Android's four counts and nothing else measured.
        fn report(
            interval_ms: i64,
            packets_received: i64,
            packets_lost: i64,
            receive_timeouts: i64,
        ) -> LinkStatsReport {
            LinkStatsReport {
                interval_ms,
                packets_received,
                packets_lost,
                underruns: 0,
                receive_timeouts,
                ..LinkStatsReport::default()
            }
        }

        // LinkStatsTest.kt:15
        #[test]
        fn no_report_is_due_before_the_interval_has_passed() {
            let mut stats = Accumulator::new(10_000);
            stats.on_packet(0, 10_000);

            assert!(!stats.is_due(10_999));
            assert_eq!(poll(&mut stats, 10_999, 0), None);
        }

        // LinkStatsTest.kt:24
        #[test]
        fn reports_the_interval_counts_once_the_interval_has_passed() {
            let mut stats = Accumulator::new(10_000);
            for _ in 0..3 {
                stats.on_packet(0, 10_000);
            }
            stats.on_packet(2, 10_500);
            stats.on_receive_timeout();

            assert!(stats.is_due(11_000));
            assert_eq!(poll(&mut stats, 11_000, 0), Some(report(1_000, 4, 2, 1)));
        }

        // LinkStatsTest.kt:44
        #[test]
        fn no_report_before_the_stream_starts_within_the_startup_grace() {
            let mut stats = Accumulator::new(0);
            stats.on_receive_timeout();

            // No audio yet, and the desk is probably still adding this phone as a target: a report
            // now would carry no packets and show a healthy phone as red.
            assert!(!stats.is_due(1_000));
            assert!(!stats.is_due(GRACE - 1));
            assert_eq!(poll(&mut stats, GRACE - 1, 0), None);
        }

        // LinkStatsTest.kt:56
        #[test]
        fn the_first_report_comes_one_interval_after_the_stream_starts_and_covers_only_the_stream()
        {
            let mut stats = Accumulator::new(0);
            stats.on_packet(0, 400);

            // The silence before the first packet was startup, not the link, so it is not
            // reported.
            assert_eq!(poll(&mut stats, 1_000, 0), None);
            assert_eq!(poll(&mut stats, 1_400, 0), Some(report(1_000, 1, 0, 0)));
        }

        // LinkStatsTest.kt:75
        #[test]
        fn with_no_audio_the_first_report_comes_after_the_grace_and_covers_all_of_it() {
            let mut stats = Accumulator::new(0);
            for _ in 0..12 {
                stats.on_receive_timeout();
            }

            assert_eq!(poll(&mut stats, GRACE - 1, 0), None);
            assert_eq!(poll(&mut stats, GRACE, 0), Some(report(GRACE, 0, 0, 0)));
        }

        // LinkStatsTest.kt:93
        #[test]
        fn after_the_grace_reports_flow_every_interval_and_a_stream_that_starts_late_keeps_the_cadence(
        ) {
            let mut stats = Accumulator::new(0);
            let _ = poll(&mut stats, 3_000, 0);

            assert_eq!(poll(&mut stats, 3_999, 0), None);
            assert_eq!(
                poll(&mut stats, 4_000, 0).map(|r| r.packets_received),
                Some(0)
            );

            // The audio finally arrives: reports are already flowing, so the interval runs on.
            stats.on_packet(0, 4_500);
            let report = poll(&mut stats, 5_000, 0);
            assert_eq!(report.map(|r| r.interval_ms), Some(1_000));
            assert_eq!(report.map(|r| r.packets_received), Some(1));
        }

        // LinkStatsTest.kt:108
        #[test]
        fn a_timeout_before_the_first_packet_is_not_counted() {
            let mut stats = Accumulator::new(0);

            // The receiver starts before the desk knows where to send audio.
            stats.on_receive_timeout();
            stats.on_receive_timeout();

            assert_eq!(
                poll(&mut stats, GRACE, 0).map(|r| r.receive_timeouts),
                Some(0)
            );
        }

        // LinkStatsTest.kt:122
        #[test]
        fn a_timeout_after_the_first_packet_is_counted() {
            let mut stats = Accumulator::new(0);
            stats.on_receive_timeout();
            stats.on_packet(0, 200);

            stats.on_receive_timeout();

            let report = poll(&mut stats, 1_200, 0);
            assert_eq!(report.map(|r| r.receive_timeouts), Some(1));
            assert_eq!(report.map(|r| r.packets_received), Some(1));
        }

        // LinkStatsTest.kt:135
        #[test]
        fn the_stream_stays_started_across_reports() {
            let mut stats = Accumulator::new(0);
            stats.on_packet(0, 0);
            let _ = poll(&mut stats, 1_000, 0);

            // Audio stopped after the stream started: every wait now says something about the
            // link.
            stats.on_receive_timeout();

            assert_eq!(
                poll(&mut stats, 2_000, 0).map(|r| r.receive_timeouts),
                Some(1)
            );
        }

        // LinkStatsTest.kt:147
        #[test]
        fn a_late_poll_reports_the_real_elapsed_interval() {
            let mut stats = Accumulator::new(0);
            stats.on_packet(0, 0);

            // The loop can only poll between receives, up to one receive timeout late.
            let report = poll(&mut stats, 1_250, 0);
            assert_eq!(report.map(|r| r.interval_ms), Some(1_250));
        }

        // LinkStatsTest.kt:156
        #[test]
        fn the_counts_start_again_after_a_report() {
            let mut stats = Accumulator::new(0);
            for _ in 0..100 {
                stats.on_packet(0, 0);
            }
            stats.on_packet(5, 500);
            stats.on_receive_timeout();
            let _ = poll(&mut stats, 1_000, 0);

            stats.on_packet(0, 1_500);

            // The next interval runs from the last report, not from the start.
            assert_eq!(poll(&mut stats, 1_999, 0), None);
            assert_eq!(poll(&mut stats, 2_000, 0), Some(report(1_000, 1, 0, 0)));
        }

        // LinkStatsTest.kt:180
        #[test]
        fn underruns_are_the_difference_of_the_tracks_cumulative_count() {
            let mut stats = Accumulator::new(0);
            stats.on_packet(0, 0);

            assert_eq!(poll(&mut stats, 1_000, 3).map(|r| r.underruns), Some(3));
            assert_eq!(poll(&mut stats, 2_000, 3).map(|r| r.underruns), Some(0));
            assert_eq!(poll(&mut stats, 3_000, 7).map(|r| r.underruns), Some(4));
        }

        // LinkStatsTest.kt:190
        #[test]
        fn recovered_is_the_difference_of_the_trackers_cumulative_count() {
            let mut stats = Accumulator::new(0);
            stats.on_packet(0, 0);

            let recovered = |stats: &mut Accumulator, now_ms, cumulative_recovered| {
                stats
                    .poll(now_ms, 0, None, None, cumulative_recovered)
                    .map(|r| r.recovered)
            };
            assert_eq!(recovered(&mut stats, 1_000, 3), Some(3));
            assert_eq!(recovered(&mut stats, 2_000, 3), Some(0));
            assert_eq!(recovered(&mut stats, 3_000, 5), Some(2));
            // A count that went down belongs to a new tracker, which starts from zero.
            assert_eq!(recovered(&mut stats, 4_000, 1), Some(1));
        }

        // LinkStatsTest.kt:202
        #[test]
        fn the_jitter_is_the_mean_arrival_deviation_over_the_interval() {
            let mut stats = Accumulator::new(0);
            stats.on_packet(0, 0);
            stats.on_arrival_deviation(1.0);
            stats.on_arrival_deviation(-3.0);
            stats.on_arrival_deviation(2.0);

            // An early packet varies the arrival as much as a late one.
            let report = due(poll(&mut stats, 1_000, 0));
            let jitter = report.jitter_ms.expect("jitter measured");
            assert!((jitter - 2.0).abs() <= 1e-9, "{jitter}");
        }

        // LinkStatsTest.kt:214
        #[test]
        fn the_jitter_starts_again_after_a_report_and_is_not_measured_without_arrivals() {
            let mut stats = Accumulator::new(0);
            stats.on_packet(0, 0);
            stats.on_arrival_deviation(8.0);
            let _ = poll(&mut stats, 1_000, 0);

            // No two packets arrived in this interval: there is no spacing to measure.
            let quiet = due(poll(&mut stats, 2_000, 0));
            assert_eq!(quiet.jitter_ms, None);

            stats.on_arrival_deviation(0.5);
            let next = due(poll(&mut stats, 3_000, 0));
            let jitter = next.jitter_ms.expect("jitter measured");
            assert!((jitter - 0.5).abs() <= 1e-9, "{jitter}");
        }

        // LinkStatsTest.kt:228
        #[test]
        fn the_cushion_and_the_backlog_are_the_values_at_the_intervals_end() {
            let mut stats = Accumulator::new(0);
            stats.on_packet(0, 0);

            let report = due(stats.poll(1_000, 0, Some(10.0), Some(37.5), 0));

            assert_eq!(report.cushion_ms, Some(10.0));
            assert_eq!(report.backlog_ms, Some(37.5));
        }

        // LinkStatsTest.kt:239
        #[test]
        fn a_report_without_the_extras_marks_them_not_measured() {
            let mut stats = Accumulator::new(0);

            let report = due(poll(&mut stats, GRACE, 0));

            assert_eq!(report.jitter_ms, None);
            assert_eq!(report.cushion_ms, None);
            assert_eq!(report.backlog_ms, None);
        }

        // LinkStatsTest.kt:250
        #[test]
        fn the_interval_is_clamped_to_what_the_server_accepts() {
            let mut stats = Accumulator::new(0);

            // A phone that slept for two minutes still sends an interval the desk accepts.
            assert_eq!(
                poll(&mut stats, 120_000, 0).map(|r| r.interval_ms),
                Some(Accumulator::MAXIMUM_INTERVAL_MS)
            );
        }
    }
}
