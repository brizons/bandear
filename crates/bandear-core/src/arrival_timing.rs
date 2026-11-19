//! How evenly the desk's packets arrive: the smoothed spacing between arrivals, the smoothed jitter
//! around it, and each arrival's deviation from that spacing, which the link report averages.
//!
//! Ported from the iOS client's former `ArrivalTiming`, itself ported from the Android client's
//! former receive loop in `PmonPlayer`, which kept the same three values in local variables; both
//! clients now take them from this module.
//! For every packet the reorder hold accepts, measured as it arrives rather than as it plays, so
//! the hold cannot hide the Wi-Fi's own timing:
//! 1. The interval since the previous arrival moves the spacing a tenth of the way towards it. The
//!    first interval seeds the spacing instead.
//! 2. The interval's distance from the spacing *just moved* moves the jitter a tenth of the way.
//! 3. That same signed deviation, interval minus the moved spacing, is handed out for the link
//!    report.
//!
//! The weight is Android's former 0.1, not RFC 3550's 1/16, so both clients kept reporting the same
//! Wi-Fi alike.

/// The arrival spacing and jitter of one stream.
///
/// Time is a monotonic nanosecond reading, as Android's `System.nanoTime()`, so a 5.33 ms packet
/// spacing is measured as 5.33 ms; a millisecond clock would read it as 5, 5, 6 and report its own
/// rounding as jitter. The interval is the signed difference over 1 000 000, Android's `Long`
/// arithmetic, so a reading that went backwards is a negative interval rather than a panic on the
/// receive thread. Nothing here reads a clock. A value on the receive path: plain fields, no
/// allocation.
///
/// As on iOS before, and unlike Android's former loop, which re-seeded while the spacing was
/// exactly 0.0, the spacing is `None` until the first interval seeds it, once.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ArrivalTiming {
    inter_arrival_ms: Option<f64>,
    jitter_ms: Option<f64>,
    previous_arrival_ns: Option<u64>,
}

impl ArrivalTiming {
    /// The weight each new interval gets in the spacing, and each new deviation in the jitter.
    pub const SMOOTHING: f64 = 0.1;

    /// Timing with no packet yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The smoothed spacing between arrivals, in milliseconds; `None` until a second packet
    /// arrived.
    pub fn inter_arrival_ms(&self) -> Option<f64> {
        self.inter_arrival_ms
    }

    /// The smoothed distance of each interval from the spacing, in milliseconds; `None` until a
    /// second packet arrived.
    pub fn jitter_ms(&self) -> Option<f64> {
        self.jitter_ms
    }

    /// Notes a packet accepted at `now_ns` on a monotonic clock and answers its deviation from the
    /// smoothed spacing, in milliseconds (negative for a packet early on it), or `None` for the
    /// first packet, which has no interval yet.
    pub fn on_arrival(&mut self, now_ns: u64) -> Option<f64> {
        let previous = self.previous_arrival_ns.replace(now_ns)?;
        // The bits of the wrapping difference read as signed: Android's `Long` subtraction.
        let interval = now_ns.wrapping_sub(previous) as i64 as f64 / 1_000_000.0;
        let spacing = self.inter_arrival_ms.map_or(interval, |spacing| {
            spacing + Self::SMOOTHING * (interval - spacing)
        });
        let deviation = interval - spacing;
        let jitter = self.jitter_ms.unwrap_or(0.0);
        self.inter_arrival_ms = Some(spacing);
        self.jitter_ms = Some(jitter + Self::SMOOTHING * (deviation.abs() - jitter));
        Some(deviation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Close to the hand-worked value: a tenth of a decimal is not exact in binary.
    #[track_caller]
    fn assert_ms(actual: Option<f64>, expected: f64) {
        let value = actual.expect("not measured");
        assert!(
            (value - expected).abs() <= 1e-9,
            "{value} is not {expected}"
        );
    }

    // The arrival spacing and jitter of the Android client's former receive loop in `PmonPlayer`,
    // which had no test of its own there. Every expected value below was worked out by hand from
    // that loop, so the order of its steps, its seed and its 0.1 weight are each pinned by a
    // number.

    #[test]
    fn first_packet() {
        let mut timing = ArrivalTiming::new();

        assert_eq!(timing.on_arrival(1_000_000_000), None);
        assert_eq!(timing.inter_arrival_ms(), None);
        assert_eq!(timing.jitter_ms(), None);
    }

    /// Unseeded, the spacing would start from 0 and move a tenth of the way to 10 ms: a spacing of
    /// 1 ms, a deviation of 9 ms and a jitter of 0.9 ms for a perfectly even stream.
    #[test]
    fn seed() {
        let mut timing = ArrivalTiming::new();
        let _ = timing.on_arrival(1_000_000_000);

        assert_eq!(timing.on_arrival(1_010_000_000), Some(0.0));
        assert_eq!(timing.inter_arrival_ms(), Some(10.0));
        assert_eq!(timing.jitter_ms(), Some(0.0));
    }

    /// Intervals of 10, 4 and 20 ms:
    /// - 4 ms: spacing 10 + 0.1 × (4 − 10) = 9.4, deviation 4 − 9.4 = −5.4, jitter 0.1 × 5.4 = 0.54.
    ///   Taken before the spacing moved, the deviation would be −6 and the jitter 0.6.
    /// - 20 ms: spacing 9.4 + 0.1 × (20 − 9.4) = 10.46, deviation 20 − 10.46 = 9.54,
    ///   jitter 0.54 + 0.1 × (9.54 − 0.54) = 1.44.
    #[test]
    fn order_and_weight() {
        let mut timing = ArrivalTiming::new();
        let _ = timing.on_arrival(0);
        let _ = timing.on_arrival(10_000_000);

        assert_ms(timing.on_arrival(14_000_000), -5.4);
        assert_ms(timing.inter_arrival_ms(), 9.4);
        assert_ms(timing.jitter_ms(), 0.54);

        assert_ms(timing.on_arrival(34_000_000), 9.54);
        assert_ms(timing.inter_arrival_ms(), 10.46);
        assert_ms(timing.jitter_ms(), 1.44);
    }

    /// 256-frame packets at 48 kHz are 5.333 ms apart. Read in whole milliseconds they would arrive
    /// 5, 5 and 6 ms apart, and that rounding alone would read as about 0.44 ms of jitter.
    #[test]
    fn sub_millisecond_spacing() {
        let mut timing = ArrivalTiming::new();
        let start: u64 = 7_000_000_000;
        assert_eq!(timing.on_arrival(start), None);

        for packet in 1..=20_u64 {
            let deviation = timing.on_arrival(start + packet * 5_333_333);
            assert_eq!(deviation, Some(0.0), "packet {packet}");
        }
        assert_ms(timing.inter_arrival_ms(), 5.333_333);
        assert_eq!(timing.jitter_ms(), Some(0.0));
    }

    /// Android's former loop marked "no spacing yet" with a spacing of exactly 0.0, so a first
    /// interval of 0 (two readings in the same nanosecond) would seed it again on the next packet.
    /// Here the seed is the first interval whatever it is: after 0 then 10 ms the spacing is 0.1 ×
    /// 10 = 1, the deviation 9 and the jitter 0.9, where re-seeding would give 10, 0 and 0.
    #[test]
    fn zero_first_interval() {
        let mut timing = ArrivalTiming::new();
        let _ = timing.on_arrival(5_000_000);

        assert_eq!(timing.on_arrival(5_000_000), Some(0.0));
        assert_eq!(timing.inter_arrival_ms(), Some(0.0));
        assert_eq!(timing.jitter_ms(), Some(0.0));

        assert_ms(timing.on_arrival(15_000_000), 9.0);
        assert_ms(timing.inter_arrival_ms(), 1.0);
        assert_ms(timing.jitter_ms(), 0.9);
    }

    /// The difference is signed, as Android's `Long` one is: a reading 5 ms behind the previous one
    /// is an interval of −5 ms, where an unsigned subtraction would overflow on the receive thread.
    #[test]
    fn backwards_reading() {
        let mut timing = ArrivalTiming::new();
        let _ = timing.on_arrival(20_000_000);

        assert_ms(timing.on_arrival(15_000_000), 0.0);
        assert_ms(timing.inter_arrival_ms(), -5.0);
        assert_ms(timing.jitter_ms(), 0.0);
    }
}
