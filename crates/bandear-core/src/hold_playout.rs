//! The receive loop's use of the [`ReorderHold`] and the [`LinkStatsAccumulator`], wake-up by
//! wake-up, kept apart from the socket and the audio output so the wiring is tested without either.
//!
//! A port of the iOS client's former `HoldPlayout`, itself a port of the Android client's former
//! `HoldPlayout`; this module replaced both. Per wake-up the loop calls, in order:
//! [`HoldPlayout::on_timeout`] when no packet came, [`HoldPlayout::wake`] with the hold the desk
//! asks for now, [`HoldPlayout::offer`] for a packet that came (skipping it when refused), then
//! [`HoldPlayout::next_to_play`] until it answers `None`, playing each packet it answers.
//! [`HoldPlayout::poll`] ends a report interval with the hold's recovered count, so a packet a late
//! copy brought counts in `packets_received` (it is played) and in `recovered`, never in
//! `packets_lost` (see [`LinkStatsReport`]).
//!
//! On Android the former player held the same accumulator, and asked it directly whether a report
//! was due and what each packet's arrival deviation was. Here, as in the former iOS playout, the
//! playout owns the accumulator, so nothing else can hold it: [`HoldPlayout::link_stats`] reads it,
//! and [`HoldPlayout::on_arrival_deviation`] feeds it.

use crate::link_stats::{LinkStatsAccumulator, LinkStatsReport};
use crate::reorder_hold::ReorderHold;

/// The reorder hold and the link report of one receive loop, over packets of type `P`.
///
/// Single-threaded, on the receive thread; nothing is allocated per packet.
pub struct HoldPlayout<P> {
    hold: ReorderHold<P>,
    link_stats: LinkStatsAccumulator,
}

impl<P> HoldPlayout<P> {
    /// A playout of `hold` that reports into `link_stats`.
    pub fn new(hold: ReorderHold<P>, link_stats: LinkStatsAccumulator) -> Self {
        Self { hold, link_stats }
    }

    /// The link report's counts so far, for asking whether a report is due
    /// ([`LinkStatsAccumulator::is_due`]) before reading the player's underruns.
    pub fn link_stats(&self) -> &LinkStatsAccumulator {
        &self.link_stats
    }

    /// The hold now in force, in packets, read from the desk's setting once per wake-up.
    pub fn hold_packets(&self) -> i64 {
        self.hold.hold_packets()
    }

    /// Every packet the stream skipped so far, each counted once (the receiver's own metrics).
    pub fn lost(&self) -> i64 {
        self.hold.lost()
    }

    /// No packet came within the receive timeout: counted, and nothing held is waited for longer.
    pub fn on_timeout(&mut self) {
        self.link_stats.on_receive_timeout();
        self.hold.release();
    }

    /// A wake-up, with the hold the desk asks for now (a shorter one plays what is now due).
    pub fn wake(&mut self, hold_packets: i64) {
        self.hold.set_hold_packets(hold_packets);
    }

    /// Offers a packet that arrived; false when it is dropped (a duplicate, or too late to play).
    pub fn offer(&mut self, sequence: u64, packet: P) -> bool {
        self.hold.offer(sequence, packet)
    }

    /// The next packet to play, in sequence order, counted as played at `now_ms` with the packets
    /// the stream skipped right before it; `None` when none is due.
    pub fn next_to_play(&mut self, now_ms: i64) -> Option<P> {
        let packet = self.hold.next_due()?;
        self.link_stats.on_packet(self.hold.lost_before(), now_ms);
        Some(packet)
    }

    /// How far a packet's spacing from the one before it was from the smoothed spacing, as it
    /// arrived, whatever the hold then does with it (see
    /// [`LinkStatsAccumulator::on_arrival_deviation`]).
    pub fn on_arrival_deviation(&mut self, deviation_ms: f64) {
        self.link_stats.on_arrival_deviation(deviation_ms);
    }

    /// The link report due at `now_ms`, with the hold's recovered count, or `None` when none is
    /// due. `cushion_ms` and `backlog_ms` are the playback values at `now_ms`, `None` when not
    /// measured (see [`LinkStatsAccumulator::poll`]).
    pub fn poll(
        &mut self,
        now_ms: i64,
        cumulative_underruns: i64,
        cushion_ms: Option<f64>,
        backlog_ms: Option<f64>,
    ) -> Option<LinkStatsReport> {
        self.link_stats.poll(
            now_ms,
            cumulative_underruns,
            cushion_ms,
            backlog_ms,
            self.hold.recovered(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each packet carries its own sequence, so what is played reads as plainly as the sequence
    /// order.
    type Playout = HoldPlayout<u64>;

    const NOTHING: [u64; 0] = [];

    fn playout() -> Playout {
        HoldPlayout::new(ReorderHold::new(), LinkStatsAccumulator::new(0))
    }

    /// One wake-up with a packet, as the loop runs it: the hold, the offer, then every packet due.
    fn arrive(playout: &mut Playout, sequence: u64, now_ms: i64, hold_packets: i64) -> Vec<u64> {
        playout.wake(hold_packets);
        if !playout.offer(sequence, sequence) {
            return Vec::new();
        }
        drain(playout, now_ms)
    }

    /// One wake-up with no packet: the timeout, the hold, then every packet now due.
    fn time_out(playout: &mut Playout, now_ms: i64, hold_packets: i64) -> Vec<u64> {
        playout.on_timeout();
        playout.wake(hold_packets);
        drain(playout, now_ms)
    }

    fn drain(playout: &mut Playout, now_ms: i64) -> Vec<u64> {
        let mut played = Vec::new();
        while let Some(packet) = playout.next_to_play(now_ms) {
            played.push(packet);
        }
        played
    }

    /// The report a poll answered, or the test stops there.
    #[track_caller]
    fn due(report: Option<LinkStatsReport>) -> LinkStatsReport {
        report.expect("no report was due")
    }

    // What only the former iOS playout had, beside the Android cases below. On Android the former
    // player held the same accumulator the playout did, and asked it directly whether a report was
    // due and what each arrival's deviation was. Here the playout owns the accumulator, so it
    // carries those two calls itself; these tests pin that they reach the report. They also pin the
    // arguments Android's former playout tests always left at their defaults: the underrun count
    // and the playback extras.

    /// A playout with no hold, whose stream starts with packet 0 played at 100 ms.
    fn streaming() -> Playout {
        let mut playout = playout();
        arrive(&mut playout, 0, 100, 0);
        playout
    }

    #[test]
    fn report_is_due() {
        let mut playout = streaming();

        assert!(!playout.link_stats().is_due(1_099));
        assert!(playout.link_stats().is_due(1_100));
        assert_eq!(
            playout
                .poll(1_100, 0, None, None)
                .map(|r| r.packets_received),
            Some(1)
        );
        assert!(!playout.link_stats().is_due(1_100));
    }

    #[test]
    fn arrival_deviation() {
        let mut playout = streaming();
        playout.on_arrival_deviation(4.0);
        playout.on_arrival_deviation(-2.0);

        let report = due(playout.poll(1_100, 0, None, None));
        assert_eq!(report.jitter_ms, Some(3.0));
    }

    #[test]
    fn poll_arguments() {
        let mut playout = streaming();

        let first = due(playout.poll(1_100, 2, Some(20.0), Some(33.25)));
        assert_eq!(first.underruns, 2);
        assert_eq!(first.cushion_ms, Some(20.0));
        assert_eq!(first.backlog_ms, Some(33.25));

        let second = due(playout.poll(2_100, 5, None, None));
        assert_eq!(second.underruns, 3);
        assert_eq!(second.cushion_ms, None);
        assert_eq!(second.backlog_ms, None);
        assert_eq!(second.jitter_ms, None);
    }

    /// The Android client's own tests for the receive loop's wiring of the hold and the link
    /// report, ported case for case and in order from its former `HoldPlayoutTest` through the iOS
    /// client's former `HoldPlayoutParityTests`, both deleted with this port; each test keeps the
    /// Android test's name in snake case. Wake-up by wake-up as the player drives it, a packet a
    /// delayed copy brought is played, counted in `packets_received` and in `recovered`, never in
    /// `packets_lost`. Same wake-ups, same plays, same reports.
    mod android_parity {
        use super::{arrive, due, playout, time_out, NOTHING};
        use crate::control::client_message::{ClientMessage, LinkStats};

        /// Android's and iOS's first case also turned the report into the `stats` message and
        /// compared it with the JSON the server's own protocol test reads
        /// (`protocol::tests::the_stats_a_phone_sends_after_a_copy_filled_a_gap`), byte for byte
        /// here.
        #[test]
        fn a_delayed_copy_is_played_in_its_place_and_reported_as_received_and_recovered_not_lost() {
            let mut playout = playout();
            assert_eq!(arrive(&mut playout, 10, 100, 1), NOTHING);
            assert_eq!(arrive(&mut playout, 11, 110, 1), [10]);
            // 12 is lost on the way; 13 comes, and 12's copy after it, still in time.
            assert_eq!(arrive(&mut playout, 13, 120, 1), [11]);
            assert_eq!(arrive(&mut playout, 12, 130, 1), [12]);
            // Nothing else comes: the timeout plays what is held.
            assert_eq!(time_out(&mut playout, 1_000, 1), [13]);

            let report = due(playout.poll(1_110, 0, None, None));
            assert_eq!(report.packets_received, 4);
            assert_eq!(report.packets_lost, 0);
            assert_eq!(report.recovered, 1);
            assert_eq!(report.receive_timeouts, 1);
            assert_eq!(playout.lost(), 0);
            assert_eq!(
                ClientMessage::Stats(LinkStats::from_report(&report)).to_json(),
                r#"{"type":"stats","interval_ms":1000,"packets_received":4,"packets_lost":0,"underruns":0,"receive_timeouts":1,"recovered":1}"#
            );
        }

        #[test]
        fn a_copy_too_late_for_the_hold_is_dropped_and_its_original_counted_lost_once() {
            let mut playout = playout();
            arrive(&mut playout, 0, 100, 1);
            assert_eq!(arrive(&mut playout, 2, 110, 1), [0]);
            // 3 makes 1 past its deadline: given up on, and 2 plays with the gap before it.
            assert_eq!(arrive(&mut playout, 3, 120, 1), [2]);
            assert_eq!(arrive(&mut playout, 1, 130, 1), NOTHING);

            let report = due(playout.poll(1_110, 0, None, None));
            assert_eq!(report.packets_received, 2);
            assert_eq!(report.packets_lost, 1);
            assert_eq!(
                report.recovered, 0,
                "a copy past the hold is not played, so not recovered"
            );
            assert_eq!(playout.lost(), 1);
        }

        #[test]
        fn the_hold_the_desk_asks_for_applies_from_the_next_wake_up() {
            let mut playout = playout();
            assert_eq!(arrive(&mut playout, 0, 100, 2), NOTHING);
            assert_eq!(arrive(&mut playout, 1, 110, 2), NOTHING);
            assert_eq!(playout.hold_packets(), 2);
            // The desk turns the delay off: what is held is now due, and the next packet plays at
            // once.
            assert_eq!(arrive(&mut playout, 2, 120, 0), [0, 1, 2]);
            assert_eq!(playout.hold_packets(), 0);
            assert_eq!(arrive(&mut playout, 3, 130, 0), [3]);
        }

        #[test]
        fn recovered_is_reported_per_interval_and_never_more_than_the_packets_played() {
            let mut playout = playout();
            arrive(&mut playout, 0, 100, 1);
            arrive(&mut playout, 2, 110, 1);
            arrive(&mut playout, 1, 120, 1);
            arrive(&mut playout, 3, 130, 1);
            let first = due(playout.poll(1_110, 0, None, None));
            assert_eq!(first.recovered, 1);
            assert!(first.recovered <= first.packets_received);

            arrive(&mut playout, 4, 1_200, 1);
            let second = due(playout.poll(2_110, 0, None, None));
            assert_eq!(second.recovered, 0, "the difference since the last report");
            assert_eq!(playout.poll(2_120, 0, None, None), None);
            assert!(second.packets_lost <= 0);
        }
    }
}
