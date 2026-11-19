//! Puts received packets in sequence order before they are played, holding them back for
//! [`ReorderHold::hold_packets`] packets so a late copy can fill the gap of its lost original.
//!
//! A line-for-line port of the iOS client's `ReorderHold.swift`, itself a line-for-line port of the
//! Android client's `ReorderHold.kt`, so both clients treat the same Wi-Fi the same way and a bug
//! report from either one means the same thing.
//!
//! The desk can send each packet twice, the copy a few blocks after the original (the `redundancy`
//! message names the delay). A copy of a lost packet then arrives after newer ones; played on
//! arrival it would be out of order and dropped. Holding every packet that many packets late lets
//! the copy arrive in time and play in its place, at the cost of the hold in latency, and only
//! while the desk asks for one.
//!
//! The rules, packet by packet ([`ReorderHold::offer`], then [`ReorderHold::next_due`] until it
//! answers `None`):
//! - The first packet starts the stream, wherever it joins.
//! - A packet newer than any before is held; one that fills a gap among those held is held too.
//! - A packet already played or given up on, or a second copy of one held, is dropped
//!   ([`ReorderHold::dropped`]): it cannot rewind the stream or count a gap twice.
//! - A packet [`STREAM_RESTART_PACKETS`] or more behind the newest starts a new stream (the server
//!   restarted): what the old one left held is let go, and the jump is not counted as loss.
//! - The oldest packet held is due once the newest is [`ReorderHold::hold_packets`] ahead of it. A
//!   missing one is waited for until the newest is *more* than that ahead (its copy can no longer
//!   come in time), then skipped; the skipped packets count once, as
//!   [`ReorderHold::lost_before`] the next one played.
//! - A packet played after filling a gap is [`ReorderHold::recovered`].
//! - [`ReorderHold::release`] makes everything held due, for when no packet came for a while.
//!
//! With a hold of 0 every packet is due as it arrives, which is exactly how the phone always played.

/// How far behind the newest packet a sequence must be to mean the server restarted: about 2.5 s
/// at 100 packets/s. Wi-Fi reorders by a few positions, never by seconds.
pub const STREAM_RESTART_PACKETS: i64 = 256;

/// The longest hold there is room for: the longest delay the desk gives a redundant copy.
pub const MAXIMUM_HOLD_PACKETS: i64 = 4;

/// Room for the held packets, the one that makes the newest due, and one arriving meanwhile.
const HOLD_SLOTS: usize = MAXIMUM_HOLD_PACKETS as usize + 2;

/// [`HOLD_SLOTS`] as a sequence distance.
const HOLD_SLOTS_SEQUENCES: i64 = HOLD_SLOTS as i64;

/// What `oldest_held` answers when nothing is held.
const NONE_HELD: i64 = i64::MIN;

/// The reorder hold over packets of type `P`, which the hold owns from [`ReorderHold::offer`] until
/// [`ReorderHold::next_due`] hands them back.
///
/// Its slots are fixed arrays, so it allocates nothing, per packet or ever, and it is deliberately
/// not `Clone`: it lives on the receive path and is moved, never copied. Single-threaded: only the
/// receive loop touches it.
///
/// Nothing the network sends makes it panic: every sequence it accepts leaves room for `head` to
/// move past it, and its counters saturate at `i64::MAX` instead of overflowing.
pub struct ReorderHold<P> {
    packets: [Option<P>; HOLD_SLOTS],
    sequences: [i64; HOLD_SLOTS],
    arrived_late: [bool; HOLD_SLOTS],
    hold_packets: i64,
    lost: i64,
    dropped: i64,
    recovered: i64,
    lost_before: i64,
    last_recovered: bool,
    started: bool,
    /// The next sequence to play or give up on.
    head: i64,
    /// The newest sequence offered.
    newest: i64,
    /// Whether anything has played in this stream, for the gap before the next one.
    played: bool,
    last_played: i64,
    /// Everything held is due, until nothing is.
    releasing: bool,
    /// A packet that arrived too far ahead to hold with the others. Held once `head` is close
    /// enough, or once they have played, so `head` never passes it while it waits.
    waiting: Option<P>,
    waiting_sequence: i64,
}

impl<P> Default for ReorderHold<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P> ReorderHold<P> {
    /// An empty hold of 0 packets, before any stream started.
    pub fn new() -> Self {
        Self {
            packets: std::array::from_fn(|_| None),
            sequences: [0; HOLD_SLOTS],
            arrived_late: [false; HOLD_SLOTS],
            hold_packets: 0,
            lost: 0,
            dropped: 0,
            recovered: 0,
            lost_before: 0,
            last_recovered: false,
            started: false,
            head: 0,
            newest: 0,
            played: false,
            last_played: 0,
            releasing: false,
            waiting: None,
            waiting_sequence: 0,
        }
    }

    /// The packets each one is held back, 0 through [`MAXIMUM_HOLD_PACKETS`].
    pub fn hold_packets(&self) -> i64 {
        self.hold_packets
    }

    /// Sets the hold, clamped to 0 through [`MAXIMUM_HOLD_PACKETS`]. A shorter hold makes what is
    /// now due playable at once; a longer one holds the next packets.
    pub fn set_hold_packets(&mut self, packets: i64) {
        self.hold_packets = packets.clamp(0, MAXIMUM_HOLD_PACKETS);
    }

    /// Packets the stream skipped since the hold was created, each counted once.
    pub fn lost(&self) -> i64 {
        self.lost
    }

    /// Duplicate or late packets dropped since the hold was created.
    pub fn dropped(&self) -> i64 {
        self.dropped
    }

    /// Packets played after filling a gap: each arrived after a newer one and was still in time.
    /// What the phone reports as `recovered`; they count as received, never as lost.
    pub fn recovered(&self) -> i64 {
        self.recovered
    }

    /// How many packets the stream skipped right before the packet [`ReorderHold::next_due`] last
    /// answered.
    pub fn lost_before(&self) -> i64 {
        self.lost_before
    }

    /// Whether the packet [`ReorderHold::next_due`] last answered filled a gap.
    pub fn last_recovered(&self) -> bool {
        self.last_recovered
    }

    /// Offers the received packet `sequence`; false when it is dropped (a duplicate, too late to
    /// play, or a sequence no real stream can reach). Then [`ReorderHold::next_due`] answers what
    /// is due.
    ///
    /// The header carries an unsigned 64-bit sequence. Android compares it as a signed `Long`, and
    /// Kotlin's arithmetic wraps; iOS refuses a sequence above `i64::MAX` before it can touch the
    /// arithmetic. This port refuses `i64::MAX` too, which iOS accepts and then traps on, moving
    /// `head` past it once it plays. No stream reaches 2^63 blocks (that is billions of years), so
    /// every real sequence behaves exactly as it does on Android.
    pub fn offer(&mut self, sequence: u64, packet: P) -> bool {
        let sequence = match i64::try_from(sequence) {
            Ok(sequence) if sequence < i64::MAX => sequence,
            _ => {
                self.dropped = self.dropped.saturating_add(1);
                return false;
            }
        };

        if self.started && self.newest - sequence >= STREAM_RESTART_PACKETS {
            self.restart();
        }
        if !self.started {
            self.started = true;
            self.head = sequence;
            self.newest = sequence;
            self.store(sequence, packet, false);
            return true;
        }
        if sequence < self.head
            || self.is_held(sequence)
            || (self.waiting.is_some() && sequence == self.waiting_sequence)
        {
            self.dropped = self.dropped.saturating_add(1);
            return false;
        }
        if sequence > self.newest {
            self.newest = sequence;
            if sequence - self.head >= HOLD_SLOTS_SEQUENCES {
                // Far ahead: what is held now plays first, then this one is held (see `next_due`).
                if self.waiting.is_some() {
                    // A second jump before the first was settled: the first is given up on.
                    self.dropped = self.dropped.saturating_add(1);
                }
                self.waiting = Some(packet);
                self.waiting_sequence = sequence;
                return true;
            }
            self.store(sequence, packet, false);
        } else if sequence - self.head < HOLD_SLOTS_SEQUENCES {
            // Within what is held: it fills a gap, arriving after a newer one.
            self.store(sequence, packet, true);
        } else {
            // Behind a packet that arrived far ahead, and too far from what is held to hold.
            self.dropped = self.dropped.saturating_add(1);
            return false;
        }
        true
    }

    /// The next packet to play now, in sequence order, or `None` when none is due;
    /// [`ReorderHold::lost_before`] and [`ReorderHold::last_recovered`] then describe it. Call it
    /// until it answers `None` after every offer. (`next()` on iOS and Android.)
    pub fn next_due(&mut self) -> Option<P> {
        while self.started {
            if self.waiting_sequence - self.head < HOLD_SLOTS_SEQUENCES {
                if let Some(ahead) = self.waiting.take() {
                    // The packet that arrived far ahead is now close enough to hold with the
                    // others, so the stream plays it in its place: nothing skips past it or leaves
                    // it to rewind to.
                    self.store(self.waiting_sequence, ahead, false);
                }
            }

            let slot = slot_of(self.head);
            if self.packets[slot].is_some() && self.sequences[slot] == self.head {
                if !self.releasing && self.newest - self.head < self.hold_packets {
                    break;
                }
                let packet = self.packets[slot].take();
                // Never negative: head only moves forward past the last packet played.
                self.lost_before = if self.played {
                    (self.head - self.last_played - 1).max(0)
                } else {
                    0
                };
                self.lost = self.lost.saturating_add(self.lost_before);
                self.last_recovered = self.arrived_late[slot];
                if self.last_recovered {
                    self.recovered = self.recovered.saturating_add(1);
                }
                self.played = true;
                self.last_played = self.head;
                self.head += 1;
                return packet;
            }

            let held = self.oldest_held();
            if held == NONE_HELD {
                // Nothing held: a packet that arrived far ahead is held now, the gap before it
                // given up on (counted when it is played).
                let Some(far) = self.waiting.take() else {
                    break;
                };
                self.head = self.waiting_sequence;
                self.store(self.waiting_sequence, far, false);
                continue;
            }

            // Missing: given up on once its copy can no longer come in time, as is every sequence
            // up to the next one held.
            if !self.releasing && self.newest - self.head <= self.hold_packets {
                break;
            }
            self.head = if self.releasing {
                held
            } else {
                held.min(self.newest - self.hold_packets)
            };
        }
        self.releasing = false;
        None
    }

    /// Makes everything held due: no packet came for a while, so nothing held is worth waiting
    /// for. [`ReorderHold::next_due`] then plays it in order.
    pub fn release(&mut self) {
        if self.started {
            self.releasing = true;
        }
    }

    fn restart(&mut self) {
        for packet in &mut self.packets {
            *packet = None;
        }
        self.waiting = None;
        self.started = false;
        self.played = false;
        self.releasing = false;
    }

    fn is_held(&self, sequence: i64) -> bool {
        let slot = slot_of(sequence);
        self.packets[slot].is_some() && self.sequences[slot] == sequence
    }

    fn store(&mut self, sequence: i64, packet: P, late: bool) {
        let slot = slot_of(sequence);
        self.packets[slot] = Some(packet);
        self.sequences[slot] = sequence;
        self.arrived_late[slot] = late;
    }

    /// The oldest sequence held from `head` on, or [`NONE_HELD`].
    fn oldest_held(&self) -> i64 {
        let mut oldest = i64::MAX;
        for (packet, &sequence) in self.packets.iter().zip(&self.sequences) {
            if packet.is_some() && sequence >= self.head && sequence < oldest {
                oldest = sequence;
            }
        }
        if oldest == i64::MAX {
            NONE_HELD
        } else {
            oldest
        }
    }
}

/// The slot `sequence` is held in. Sequences here are never negative (see
/// [`ReorderHold::offer`]); the Euclidean remainder is the floor modulus Android's
/// `Math.floorMod` computes either way.
fn slot_of(sequence: i64) -> usize {
    sequence.rem_euclid(HOLD_SLOTS_SEQUENCES) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    /// Each packet carries its own sequence, so what is played reads as plainly as the sequence
    /// order.
    type Hold = ReorderHold<u64>;

    const NOTHING: [u64; 0] = [];

    fn hold_of(packets: i64) -> Hold {
        let mut hold = Hold::new();
        hold.set_hold_packets(packets);
        hold
    }

    /// Everything due now, in the order it plays.
    fn drain(hold: &mut Hold) -> Vec<u64> {
        let mut played = Vec::new();
        while let Some(packet) = hold.next_due() {
            played.push(packet);
        }
        played
    }

    /// Offers `sequence` the way the receive loop does, then plays everything that became due.
    fn offer_and_drain(hold: &mut Hold, sequence: u64) -> Vec<u64> {
        let _ = hold.offer(sequence, sequence);
        drain(hold)
    }

    // Behaviour ported from `ReorderHoldTests.swift`. Every expectation there was traced by hand
    // through the Kotlin first, so the clients agree on the same input rather than this one
    // agreeing with a reading of the comments.

    #[test]
    fn bounds() {
        assert_eq!(STREAM_RESTART_PACKETS, 256);
        assert_eq!(MAXIMUM_HOLD_PACKETS, 4);
    }

    #[test]
    fn clamps_hold_packets() {
        for (requested, kept) in [(-1, 0), (0, 0), (3, 3), (4, 4), (9, 4)] {
            let hold = hold_of(requested);

            assert_eq!(hold.hold_packets(), kept, "requested {requested}");
        }
    }

    // No hold, as the phone always played.

    #[test]
    fn joins_anywhere() {
        assert_eq!(offer_and_drain(&mut Hold::new(), 1_000), [1_000]);
    }

    #[test]
    fn plays_in_order_at_once() {
        let mut hold = Hold::new();

        assert_eq!(offer_and_drain(&mut hold, 10), [10]);
        assert_eq!(offer_and_drain(&mut hold, 11), [11]);
        assert_eq!(offer_and_drain(&mut hold, 12), [12]);
        assert_eq!(hold.lost(), 0);
        assert_eq!(hold.recovered(), 0);
    }

    #[test]
    fn counts_gap_once() {
        let mut hold = Hold::new();
        offer_and_drain(&mut hold, 10);

        assert_eq!(offer_and_drain(&mut hold, 12), [12]);
        assert_eq!(hold.lost_before(), 1);
        assert_eq!(hold.lost(), 1);

        assert_eq!(offer_and_drain(&mut hold, 13), [13]);
        assert_eq!(hold.lost_before(), 0);
        assert_eq!(hold.lost(), 1);
    }

    #[test]
    fn drops_late_packet() {
        let mut hold = Hold::new();
        offer_and_drain(&mut hold, 10);
        offer_and_drain(&mut hold, 12);

        assert!(!hold.offer(11, 11));
        assert_eq!(drain(&mut hold), NOTHING);
        assert_eq!(hold.dropped(), 1);
    }

    #[test]
    fn drops_duplicate_of_played() {
        let mut hold = Hold::new();
        offer_and_drain(&mut hold, 10);

        assert!(!hold.offer(10, 10));
        assert_eq!(hold.dropped(), 1);
    }

    // Holding back for a delayed copy.

    #[test]
    fn holds_until_due() {
        let mut hold = hold_of(2);

        assert_eq!(offer_and_drain(&mut hold, 10), NOTHING);
        assert_eq!(offer_and_drain(&mut hold, 11), NOTHING);
        assert_eq!(offer_and_drain(&mut hold, 12), [10]);
        assert_eq!(offer_and_drain(&mut hold, 13), [11]);
    }

    #[test]
    fn recovers_late_copy() {
        let mut hold = hold_of(2);

        assert_eq!(offer_and_drain(&mut hold, 10), NOTHING);
        assert_eq!(offer_and_drain(&mut hold, 12), [10]);
        assert_eq!(offer_and_drain(&mut hold, 11), NOTHING);
        assert_eq!(offer_and_drain(&mut hold, 13), [11]);
        assert!(hold.last_recovered());
        assert_eq!(offer_and_drain(&mut hold, 14), [12]);
        assert!(!hold.last_recovered());

        assert_eq!(hold.recovered(), 1);
        assert_eq!(hold.lost(), 0);
    }

    #[test]
    fn gives_up_on_missing_packet() {
        let mut hold = hold_of(2);

        assert_eq!(offer_and_drain(&mut hold, 10), NOTHING);
        assert_eq!(offer_and_drain(&mut hold, 12), [10]);
        // Newest is two ahead of the gap at 11: its copy could still come.
        assert_eq!(offer_and_drain(&mut hold, 13), NOTHING);
        // Newest is three ahead: no copy sent two late can arrive in time now.
        assert_eq!(offer_and_drain(&mut hold, 14), [12]);

        assert_eq!(hold.lost_before(), 1);
        assert_eq!(hold.lost(), 1);
    }

    #[test]
    fn drops_duplicate_of_held() {
        let mut hold = hold_of(2);
        offer_and_drain(&mut hold, 10);
        offer_and_drain(&mut hold, 11);

        assert!(!hold.offer(11, 11));
        assert_eq!(hold.dropped(), 1);
    }

    // Release.

    #[test]
    fn release_plays_everything_held() {
        let mut hold = hold_of(2);
        offer_and_drain(&mut hold, 10);
        offer_and_drain(&mut hold, 11);

        hold.release();

        assert_eq!(drain(&mut hold), [10, 11]);
    }

    #[test]
    fn release_lasts_one_drain() {
        let mut hold = hold_of(2);
        offer_and_drain(&mut hold, 10);
        hold.release();
        drain(&mut hold);

        assert_eq!(offer_and_drain(&mut hold, 11), NOTHING);
    }

    #[test]
    fn release_skips_gap() {
        let mut hold = hold_of(2);
        offer_and_drain(&mut hold, 10);
        assert_eq!(offer_and_drain(&mut hold, 12), [10]);

        hold.release();

        assert_eq!(drain(&mut hold), [12]);
        assert_eq!(hold.lost(), 1);
    }

    // A server that restarted.

    #[test]
    fn restarts_stream() {
        let mut hold = Hold::new();
        offer_and_drain(&mut hold, 1_000);
        offer_and_drain(&mut hold, 1_001);

        assert_eq!(offer_and_drain(&mut hold, 5), [5]);
        assert_eq!(hold.lost(), 0);
        assert_eq!(hold.lost_before(), 0);
    }

    #[test]
    fn restart_forgets_held_packets() {
        let mut hold = hold_of(2);
        offer_and_drain(&mut hold, 1_000);
        offer_and_drain(&mut hold, 1_001);

        assert_eq!(offer_and_drain(&mut hold, 5), NOTHING);
        assert_eq!(offer_and_drain(&mut hold, 6), NOTHING);
        assert_eq!(offer_and_drain(&mut hold, 7), [5]);
        assert_eq!(hold.dropped(), 0);
        assert_eq!(hold.lost(), 0);
    }

    #[test]
    fn restart_boundary() {
        let mut hold = Hold::new();
        offer_and_drain(&mut hold, 1_000);

        // 255 behind: late, dropped.
        assert!(!hold.offer(745, 745));
        // 256 behind: the server restarted.
        assert_eq!(offer_and_drain(&mut hold, 744), [744]);
    }

    // A packet far ahead.

    #[test]
    fn plays_far_packet() {
        let mut hold = Hold::new();
        offer_and_drain(&mut hold, 10);

        assert_eq!(offer_and_drain(&mut hold, 20), [20]);
        assert_eq!(hold.lost_before(), 9);
        assert_eq!(hold.lost(), 9);
    }

    #[test]
    fn far_packet_waits_for_held() {
        let mut hold = hold_of(2);
        offer_and_drain(&mut hold, 10);
        offer_and_drain(&mut hold, 11);

        assert_eq!(offer_and_drain(&mut hold, 20), [10, 11]);
        assert_eq!(offer_and_drain(&mut hold, 21), NOTHING);
        assert_eq!(offer_and_drain(&mut hold, 22), [20]);
        assert_eq!(hold.lost_before(), 8);
    }

    #[test]
    fn second_far_jump_drops_first() {
        let mut hold = hold_of(2);
        let _ = hold.offer(10, 10);
        let _ = hold.offer(20, 20);
        let _ = hold.offer(30, 30);

        assert_eq!(hold.dropped(), 1);
        assert_eq!(drain(&mut hold), [10]);
    }

    #[test]
    fn drops_duplicate_of_waiting() {
        let mut hold = Hold::new();
        offer_and_drain(&mut hold, 10);
        let _ = hold.offer(20, 20);

        assert!(!hold.offer(20, 20));
        assert_eq!(hold.dropped(), 1);
    }

    #[test]
    fn drops_packet_between_held_and_far() {
        let mut hold = Hold::new();
        offer_and_drain(&mut hold, 10);
        let _ = hold.offer(30, 30);

        assert!(!hold.offer(25, 25));
        assert_eq!(hold.dropped(), 1);
    }

    // Beyond the Android client.

    /// Kotlin reads the eight sequence bytes as a signed `Long` and its arithmetic wraps. Refusing
    /// a sequence with the top bit set keeps the arithmetic in range, and no real stream reaches
    /// 2^63 blocks, so it changes nothing a desk can produce.
    #[test]
    fn refuses_out_of_range_sequence() {
        for sequence in [i64::MAX as u64 + 1, u64::MAX] {
            let mut hold = Hold::new();

            assert!(!hold.offer(sequence, sequence), "{sequence}");
            assert_eq!(hold.dropped(), 1);
            assert_eq!(offer_and_drain(&mut hold, 10), [10]);
        }
    }

    /// `i64::MAX` is refused as well (L11). Accepted, it would play and then move `head` one past
    /// the largest sequence there is: Swift traps there, and Rust panics in a debug build. The one
    /// below it plays, and the stream after it restarts as from any server restart.
    #[test]
    fn refuses_the_largest_sequence() {
        let largest = i64::MAX as u64;
        let mut hold = Hold::new();

        assert!(!hold.offer(largest, largest));
        assert_eq!(drain(&mut hold), NOTHING);
        assert_eq!(hold.dropped(), 1);

        assert_eq!(offer_and_drain(&mut hold, largest - 1), [largest - 1]);
        assert!(!hold.offer(largest, largest));
        assert_eq!(hold.dropped(), 2);
        assert_eq!(offer_and_drain(&mut hold, 10), [10]);
        assert_eq!(hold.lost(), 0, "a restart is not loss");
    }

    /// The counters saturate instead of overflowing (L11). The gap before a sequence close to
    /// `i64::MAX` is counted as lost, and a restart does not reset the count, so two such jumps
    /// would overflow it; `dropped` and `recovered` cannot get there in a test's lifetime, so they
    /// start close to the top.
    #[test]
    fn counters_saturate() {
        let far = i64::MAX as u64 - 1;
        let mut hold = Hold::new();
        for _ in 0..2 {
            // A stream at 0, a jump to the far end, and back to 0 as a restart.
            assert_eq!(offer_and_drain(&mut hold, 0), [0]);
            assert_eq!(offer_and_drain(&mut hold, far), [far]);
        }
        assert_eq!(hold.lost(), i64::MAX);

        hold.dropped = i64::MAX;
        assert!(!hold.offer(far, far));
        assert_eq!(hold.dropped(), i64::MAX);

        let mut late = hold_of(2);
        late.recovered = i64::MAX;
        for sequence in [10, 12, 11] {
            offer_and_drain(&mut late, sequence);
        }
        assert_eq!(offer_and_drain(&mut late, 13), [11]);
        assert!(late.last_recovered());
        assert_eq!(late.recovered(), i64::MAX);
    }

    #[test]
    fn survives_garbage_mid_stream() {
        let mut hold = Hold::new();
        offer_and_drain(&mut hold, 10);

        assert!(!hold.offer(u64::MAX, u64::MAX));
        assert_eq!(offer_and_drain(&mut hold, 11), [11]);
    }

    // Whatever Wi-Fi does.

    /// The cases above pin each rule; this pins how they combine. Every run mixes reordering,
    /// duplicates, loss, far jumps, hold changes and releases, and never steps back far enough to
    /// look like a restart, so within it the stream must only ever move forward.
    ///
    /// Order and uniqueness are largely structural (the hold never moves `head` back), so the
    /// failure this mostly guards against is the one that would be silent on a phone: the receive
    /// loop hanging. `next_due()` answering forever is caught by counting what it returns;
    /// `next_due()` never returning at all is caught by the watchdog in [`completes_within`].
    #[test]
    fn never_out_of_order_or_twice() {
        let seeds = 0..50_u64;
        let log = Arc::new(Mutex::new(BTreeMap::new()));

        let finished = {
            let log = Arc::clone(&log);
            let seeds = seeds.clone();
            completes_within(Duration::from_secs(20), move || {
                for seed in seeds {
                    let run = property_run(seed);
                    log.lock().unwrap().insert(seed, run);
                }
            })
        };

        let runs = log.lock().unwrap().clone();
        let stuck = seeds.clone().find(|seed| !runs.contains_key(seed));
        assert!(
            finished,
            "next_due() never returned, first in the run for seed {}",
            stuck.unwrap_or(0)
        );
        for (seed, run) in &runs {
            assert!(
                !run.runaway,
                "seed {seed}: next_due() kept answering after one offer"
            );
            let mut sorted = run.played.clone();
            sorted.sort_unstable();
            assert_eq!(run.played, sorted, "seed {seed}: played out of order");
            sorted.dedup();
            assert_eq!(
                sorted.len(),
                run.played.len(),
                "seed {seed}: played a packet twice"
            );
            assert!(!run.played.is_empty(), "seed {seed}: nothing played");
        }
    }

    /// What one seeded run played, and whether `next_due()` ran away in it.
    #[derive(Debug, Clone, Default)]
    struct PropertyRun {
        played: Vec<u64>,
        runaway: bool,
    }

    fn property_run(seed: u64) -> PropertyRun {
        let mut generator = SplitMix64 { state: seed };
        let mut hold = Hold::new();
        let mut run = PropertyRun::default();
        let mut newest: i64 = 1_000;

        for _ in 0..500 {
            match generator.below(10) {
                0 => hold.set_hold_packets(generator.in_closed_range(0, MAXIMUM_HOLD_PACKETS)),
                1 => hold.release(),
                _ => {
                    // Mostly forward, sometimes a skip (loss), sometimes a few back (reorder,
                    // copy).
                    let step = generator.in_closed_range(-3, 4);
                    let sequence = (newest + step).max(0);
                    newest = newest.max(sequence);
                    let _ = hold.offer(sequence as u64, sequence as u64);
                }
            }

            let mut drained = 0;
            while let Some(packet) = hold.next_due() {
                run.played.push(packet);
                drained += 1;
                if drained > 64 {
                    run.runaway = true;
                    return run;
                }
            }
        }
        run
    }

    /// SplitMix64: a seeded generator, so a failing property run reproduces from its seed alone.
    /// Draws as Swift's `random(in:using:)` does over the same generator, so a seed plays the same
    /// run here as in `ReorderHoldTests.swift`.
    struct SplitMix64 {
        state: u64,
    }

    impl SplitMix64 {
        fn next(&mut self) -> u64 {
            self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut mixed = self.state;
            mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            mixed ^ (mixed >> 31)
        }

        /// Swift's `next(upperBound:)`: Lemire's nearly divisionless method, a value in
        /// `0..upper_bound`.
        fn below(&mut self, upper_bound: u64) -> u64 {
            let mut product = u128::from(self.next()) * u128::from(upper_bound);
            if (product as u64) < upper_bound {
                let threshold = upper_bound.wrapping_neg() % upper_bound;
                while (product as u64) < threshold {
                    product = u128::from(self.next()) * u128::from(upper_bound);
                }
            }
            (product >> 64) as u64
        }

        /// Swift's `random(in: lower...upper)` for a range narrower than the whole type.
        fn in_closed_range(&mut self, lower: i64, upper: i64) -> i64 {
            let delta = upper.wrapping_sub(lower) as u64;
            (lower as u64).wrapping_add(self.below(delta + 1)) as i64
        }
    }

    /// Runs `body` on a thread of its own and waits at most `limit` for it, so a loop that never
    /// returns turns into a failed assertion instead of a hung test run; the stuck thread is left
    /// behind and dies when the test process exits.
    fn completes_within(limit: Duration, body: impl FnOnce() + Send + 'static) -> bool {
        let (done, finished) = mpsc::channel();
        thread::spawn(move || {
            body();
            let _ = done.send(());
        });
        finished.recv_timeout(limit).is_ok()
    }

    /// The Android client's own tests for `ReorderHold`, ported case for case from
    /// `apps/android/app/src/test/java/com/bandear/android/ReorderHoldTest.kt` through
    /// `ReorderHoldParityTests.swift`: same arrivals, same expected plays, same counters. This is
    /// the cross-client contract. A failure here means the phones no longer treat the same Wi-Fi
    /// the same way, whatever the rest of the suite says.
    mod android_parity {
        use super::{drain, hold_of, Hold, NOTHING};
        use crate::reorder_hold::{ReorderHold, MAXIMUM_HOLD_PACKETS, STREAM_RESTART_PACKETS};

        /// One arrival: offers `sequence` and answers the sequences played because of it, in
        /// order.
        fn arrive(hold: &mut Hold, sequence: u64) -> Vec<u64> {
            let _ = hold.offer(sequence, sequence);
            drain(hold)
        }

        // With no hold the phone plays as it always did.

        #[test]
        fn with_no_hold_packets_in_order_play_as_they_arrive() {
            let mut hold = hold_of(0);

            assert_eq!(arrive(&mut hold, 0), [0]);
            assert_eq!(arrive(&mut hold, 1), [1]);
            assert_eq!(arrive(&mut hold, 2), [2]);
            assert_eq!(hold.lost(), 0);
            assert_eq!(hold.dropped(), 0);
        }

        #[test]
        fn the_first_packet_starts_the_stream_wherever_it_joins() {
            let mut hold = hold_of(0);

            // A phone that joins a stream already running has lost nothing before its first
            // packet.
            assert_eq!(arrive(&mut hold, 500), [500]);
            assert_eq!(hold.lost_before(), 0);
            assert_eq!(arrive(&mut hold, 501), [501]);
            assert_eq!(hold.lost(), 0);
        }

        #[test]
        fn with_no_hold_a_gap_is_counted_once_and_the_packet_after_it_plays() {
            let mut hold = hold_of(0);
            arrive(&mut hold, 0);

            assert_eq!(arrive(&mut hold, 3), [3]);
            assert_eq!(hold.lost_before(), 2);
            assert_eq!(arrive(&mut hold, 4), [4]);
            assert_eq!(hold.lost_before(), 0);
            assert_eq!(hold.lost(), 2);
        }

        #[test]
        fn with_no_hold_a_duplicate_is_dropped() {
            let mut hold = hold_of(0);
            arrive(&mut hold, 0);
            arrive(&mut hold, 1);

            assert!(!hold.offer(1, 1));
            assert_eq!(arrive(&mut hold, 2), [2]);
            assert_eq!(hold.lost(), 0);
            assert_eq!(hold.dropped(), 1);
        }

        #[test]
        fn with_no_hold_a_late_packet_is_dropped_without_rewinding_the_stream() {
            let mut hold = hold_of(0);
            arrive(&mut hold, 0);
            arrive(&mut hold, 3);

            // 1 and 2 arrive after 3 was played: too late to play, and their gap is already
            // counted.
            assert_eq!(arrive(&mut hold, 1), NOTHING);
            assert_eq!(arrive(&mut hold, 2), NOTHING);

            assert_eq!(arrive(&mut hold, 4), [4]);
            assert_eq!(hold.lost(), 2);
            assert_eq!(hold.dropped(), 2);
            assert_eq!(
                hold.recovered(),
                0,
                "a packet never played is never recovered"
            );
        }

        #[test]
        fn a_server_restart_starts_a_new_stream_without_counting_loss() {
            let mut hold = hold_of(0);
            arrive(&mut hold, 5_000);

            // The server restarted and its sequence began again at 0.
            assert_eq!(arrive(&mut hold, 0), [0]);
            assert_eq!(arrive(&mut hold, 1), [1]);
            assert_eq!(hold.lost(), 0);
            assert_eq!(hold.dropped(), 0);
        }

        #[test]
        fn only_a_packet_at_least_the_restart_distance_behind_starts_a_new_stream() {
            let mut hold = hold_of(0);
            arrive(&mut hold, 1_000);
            let restart = STREAM_RESTART_PACKETS as u64;

            assert_eq!(arrive(&mut hold, 1_000 - restart + 1), NOTHING);
            assert_eq!(arrive(&mut hold, 1_000 - restart), [1_000 - restart]);
            assert_eq!(hold.lost(), 0);
            assert_eq!(hold.dropped(), 1);
        }

        #[test]
        fn with_no_hold_an_immediate_copy_fills_its_gap_in_order() {
            let mut hold = hold_of(0);
            // The server sends each block, then its copy right away. Original 2 is lost: its copy
            // arrives before 3, so it is simply played in its place, and the second copies are
            // dropped.
            let mut arrivals = Vec::new();
            for block in 0..=4_u64 {
                if block != 2 {
                    arrivals.push(block);
                }
                arrivals.push(block);
            }
            let played: Vec<u64> = arrivals
                .into_iter()
                .flat_map(|sequence| arrive(&mut hold, sequence))
                .collect();

            assert_eq!(played, [0, 1, 2, 3, 4]);
            assert_eq!(hold.lost(), 0);
            assert_eq!(hold.dropped(), 4);
            assert_eq!(hold.recovered(), 0, "it cannot be told from its original");
        }

        // With a hold the phone plays a few packets late, so a late copy can fill its gap.

        #[test]
        fn with_a_hold_packets_in_order_play_that_many_packets_late() {
            let mut hold = hold_of(2);

            assert_eq!(arrive(&mut hold, 0), NOTHING);
            assert_eq!(arrive(&mut hold, 1), NOTHING);
            assert_eq!(arrive(&mut hold, 2), [0]);
            assert_eq!(arrive(&mut hold, 3), [1]);
            assert_eq!(arrive(&mut hold, 4), [2]);
            assert_eq!(hold.lost(), 0);
        }

        #[test]
        fn a_delayed_copy_fills_the_gap_of_its_lost_original_and_is_counted_as_recovered() {
            let mut hold = hold_of(2);
            // The server sends each block, then the copy of the one two blocks before it.
            // Original 5 is lost; its copy arrives right after original 7.
            let mut arrivals = Vec::new();
            for block in 0..=10_u64 {
                if block != 5 {
                    arrivals.push(block);
                }
                if block >= 2 {
                    arrivals.push(block - 2);
                }
            }
            let mut played: Vec<u64> = arrivals
                .into_iter()
                .flat_map(|sequence| arrive(&mut hold, sequence))
                .collect();
            hold.release();
            played.extend(drain(&mut hold));

            assert_eq!(played, (0..=10).collect::<Vec<u64>>());
            assert_eq!(hold.lost(), 0);
            assert_eq!(hold.recovered(), 1);
            assert_eq!(
                hold.dropped(),
                8,
                "every other copy is a duplicate or too late"
            );
        }

        #[test]
        fn the_recovered_packet_is_marked_when_it_is_played() {
            let mut hold = hold_of(1);
            assert_eq!(arrive(&mut hold, 0), NOTHING);
            assert_eq!(arrive(&mut hold, 2), [0], "0 is due, 1 may still come");

            assert!(hold.offer(1, 1));
            assert_eq!(hold.next_due(), Some(1));
            assert!(hold.last_recovered());
            assert_eq!(hold.lost_before(), 0);
            assert_eq!(hold.next_due(), None);
            assert_eq!(arrive(&mut hold, 3), [2]);
            assert!(!hold.last_recovered());
        }

        #[test]
        fn a_gap_not_filled_before_its_deadline_is_skipped_and_counted_lost() {
            let mut hold = hold_of(2);
            for sequence in [0, 1, 2] {
                arrive(&mut hold, sequence);
            }

            // 3 is lost, and its copy too: once 6 arrives it can no longer come in time.
            assert_eq!(arrive(&mut hold, 4), [1, 2]);
            assert_eq!(arrive(&mut hold, 5), NOTHING, "3 may still come as a copy");
            assert_eq!(arrive(&mut hold, 6), [4]);
            assert_eq!(hold.lost_before(), 1);
            // Its copy, arriving now, is too late to play.
            assert_eq!(arrive(&mut hold, 3), NOTHING);
            assert_eq!(hold.lost(), 1);
            assert_eq!(hold.recovered(), 0);
        }

        #[test]
        fn a_duplicate_of_a_held_packet_is_dropped() {
            let mut hold = hold_of(2);
            arrive(&mut hold, 0);
            arrive(&mut hold, 1);

            assert!(!hold.offer(1, 1));
            assert!(!hold.offer(0, 0));
            assert_eq!(hold.dropped(), 2);
            assert_eq!(arrive(&mut hold, 2), [0]);
        }

        #[test]
        fn a_server_restart_drops_what_the_old_stream_left_held() {
            let mut hold = hold_of(2);
            arrive(&mut hold, 5_000);
            arrive(&mut hold, 5_001);

            // The server restarted: its new stream starts over, held as any other.
            assert_eq!(arrive(&mut hold, 0), NOTHING);
            assert_eq!(arrive(&mut hold, 1), NOTHING);
            assert_eq!(arrive(&mut hold, 2), [0]);
            assert_eq!(hold.lost_before(), 0);
            assert_eq!(hold.lost(), 0);
        }

        #[test]
        fn a_long_jump_forward_plays_what_was_held_then_counts_the_gap_once() {
            let mut hold = hold_of(2);
            arrive(&mut hold, 0);
            arrive(&mut hold, 1);

            assert_eq!(arrive(&mut hold, 1_000), [0, 1]);
            assert_eq!(arrive(&mut hold, 1_001), NOTHING);
            assert_eq!(arrive(&mut hold, 1_002), [1_000]);
            assert_eq!(hold.lost_before(), 998);
            assert_eq!(hold.lost(), 998);
        }

        /// Every packet due now, checking that no gap before one is ever negative.
        fn drain_checked(hold: &mut Hold, played: &mut Vec<u64>) {
            while let Some(packet) = hold.next_due() {
                played.push(packet);
                assert!(
                    hold.lost_before() >= 0,
                    "lost_before {}",
                    hold.lost_before()
                );
            }
        }

        #[test]
        fn a_packet_that_arrived_far_ahead_during_a_burst_is_played_in_its_place() {
            let mut hold = hold_of(4);
            // The server sends each block, then the copy of the one four blocks before it.
            // Originals 6 and 7 are lost, so 8 arrives too far ahead of what is held to be held
            // with it at once.
            let mut arrivals = Vec::new();
            for block in 0..=15_u64 {
                if block != 6 && block != 7 {
                    arrivals.push(block);
                }
                if block >= 4 {
                    arrivals.push(block - 4);
                }
            }
            let mut played = Vec::new();
            for sequence in arrivals {
                let _ = hold.offer(sequence, sequence);
                drain_checked(&mut hold, &mut played);
            }
            hold.release();
            drain_checked(&mut hold, &mut played);

            let every_packet: Vec<u64> = (0..=15).collect();
            assert_eq!(
                played, every_packet,
                "every packet once, in order, 8 included"
            );
            assert_eq!(hold.lost(), 0);
            assert_eq!(hold.recovered(), 2, "the copies of 6 and 7");

            // Nothing is left behind to rewind the stream later.
            hold.release();
            hold.set_hold_packets(0);
            drain_checked(&mut hold, &mut played);
            assert_eq!(arrive(&mut hold, 16), [16]);
            assert_eq!(played, every_packet);
            assert_eq!(hold.lost_before(), 0);
            assert_eq!(hold.lost(), 0);
        }

        #[test]
        fn releasing_plays_everything_held_in_order() {
            let mut hold = hold_of(2);
            for sequence in [0, 1, 2] {
                arrive(&mut hold, sequence);
            }

            // No packet for a while: what is held is played rather than kept stale.
            hold.release();
            assert_eq!(drain(&mut hold), [1, 2]);
            assert_eq!(arrive(&mut hold, 3), NOTHING);
            assert_eq!(arrive(&mut hold, 4), NOTHING);
            assert_eq!(arrive(&mut hold, 5), [3]);
        }

        #[test]
        fn a_shorter_hold_plays_what_is_now_due() {
            let mut hold = hold_of(3);
            for sequence in [0, 1, 2] {
                arrive(&mut hold, sequence);
            }

            hold.set_hold_packets(0);
            assert_eq!(drain(&mut hold), [0, 1, 2]);
            assert_eq!(arrive(&mut hold, 3), [3]);
        }

        #[test]
        fn the_hold_stays_within_what_the_phone_keeps_room_for() {
            let mut hold = ReorderHold::<u64>::new();
            assert_eq!(hold.hold_packets(), 0);
            hold.set_hold_packets(9);
            assert_eq!(hold.hold_packets(), MAXIMUM_HOLD_PACKETS);
            hold.set_hold_packets(-1);
            assert_eq!(hold.hold_packets(), 0);
        }
    }
}
