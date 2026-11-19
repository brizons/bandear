//! How long the session waits before each attempt to open a lost connection again.
//!
//! Ported from the iOS client's `ReconnectPolicy.swift`, itself a port of the Android client's
//! `ReconnectPolicy.kt`, with their waits in milliseconds as Android counts them.

/// The waits between attempts to open a lost connection again.
///
/// The first retries come quickly, so a short Wi-Fi blip or a desk restart costs the musician well
/// under a few seconds; then the wait grows and settles at [`ReconnectPolicy::MAXIMUM_DELAY_MS`],
/// so a desk that stays away is retried every five seconds for as long as the session runs,
/// without hammering the network meanwhile. A connection that proves itself, by receiving its
/// first message from the desk, calls [`ReconnectPolicy::reset`], so the next loss starts from the
/// short end again; a desk that accepts and drops at once does not (`ReconnectPolicy.kt:5-14`).
///
/// A value: the session owns one and nothing else touches it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ReconnectPolicy {
    /// How many attempts of the schedule have been handed out since the last reset.
    attempt: usize,
}

impl ReconnectPolicy {
    /// The waits of the first attempts, in order, in milliseconds (`ReconnectPolicy.kt:35`).
    /// Every attempt after them waits [`ReconnectPolicy::MAXIMUM_DELAY_MS`].
    pub const SCHEDULE_MS: [i64; 4] = [500, 1_000, 2_000, 4_000];

    /// The longest wait between two attempts, in milliseconds (`ReconnectPolicy.kt:38`).
    pub const MAXIMUM_DELAY_MS: i64 = 5_000;

    /// A policy at the start of its schedule.
    pub fn new() -> ReconnectPolicy {
        ReconnectPolicy::default()
    }

    /// The wait before the next attempt, in milliseconds; each call moves one step along the
    /// schedule.
    pub fn next_delay_ms(&mut self) -> i64 {
        let delay = Self::SCHEDULE_MS
            .get(self.attempt)
            .copied()
            .unwrap_or(Self::MAXIMUM_DELAY_MS);
        // The counter stops once the schedule is exhausted, so it can never overflow.
        if self.attempt < Self::SCHEDULE_MS.len() {
            self.attempt += 1;
        }
        delay
    }

    /// Starts the schedule again, after a connection heard from the desk.
    pub fn reset(&mut self) {
        self.attempt = 0;
    }

    /// The wait before attempt `attempt`, in milliseconds, counted from 0 for the first retry
    /// after a loss. A negative attempt reads as the first, and any attempt past the schedule
    /// waits [`ReconnectPolicy::MAXIMUM_DELAY_MS`].
    pub fn delay_for_attempt_ms(attempt: i64) -> i64 {
        usize::try_from(attempt.max(0))
            .ok()
            .and_then(|index| Self::SCHEDULE_MS.get(index).copied())
            .unwrap_or(Self::MAXIMUM_DELAY_MS)
    }

    /// Whether a connection that failed or closed is opened again: only while the session still
    /// runs and nobody stopped it (`ReconnectPolicy.kt:44-50`).
    pub fn should_reconnect(session_running: bool, stopped: bool) -> bool {
        session_running && !stopped
    }
}

#[cfg(test)]
mod android_parity {
    //! `ReconnectPolicyParityTests.swift`, the Android client's `ReconnectPolicyTest`: one test per
    //! Swift test. The same Android file's `ConnectionGenerationsTest` stays with each app's
    //! session, which numbers its own connections.

    use super::*;

    #[test]
    fn first_attempts_back_off_from_half_a_second() {
        let mut policy = ReconnectPolicy::new();

        assert_eq!(
            (0..4).map(|_| policy.next_delay_ms()).collect::<Vec<_>>(),
            [500, 1_000, 2_000, 4_000]
        );
    }

    #[test]
    fn later_attempts_wait_the_cap_forever() {
        let mut policy = ReconnectPolicy::new();
        for _ in 0..4 {
            policy.next_delay_ms();
        }

        assert_eq!(ReconnectPolicy::MAXIMUM_DELAY_MS, 5_000);
        assert!((0..1_000).all(|_| policy.next_delay_ms() == ReconnectPolicy::MAXIMUM_DELAY_MS));
    }

    #[test]
    fn a_successful_open_starts_the_schedule_again() {
        let mut policy = ReconnectPolicy::new();
        for _ in 0..6 {
            policy.next_delay_ms();
        }

        policy.reset();

        assert_eq!(policy.next_delay_ms(), 500);
        assert_eq!(policy.next_delay_ms(), 1_000);
    }

    #[test]
    fn delay_for_attempt_follows_the_schedule_and_is_safe_at_the_edges() {
        assert_eq!(ReconnectPolicy::delay_for_attempt_ms(0), 500);
        assert_eq!(ReconnectPolicy::delay_for_attempt_ms(2), 2_000);
        assert_eq!(ReconnectPolicy::delay_for_attempt_ms(3), 4_000);
        assert_eq!(ReconnectPolicy::delay_for_attempt_ms(4), 5_000);
        assert_eq!(ReconnectPolicy::delay_for_attempt_ms(i64::MAX), 5_000);
        assert_eq!(ReconnectPolicy::delay_for_attempt_ms(-1), 500);
        // Not in the Swift test: the most negative attempt reads as the first too.
        assert_eq!(ReconnectPolicy::delay_for_attempt_ms(i64::MIN), 500);
    }

    #[test]
    fn reconnects_only_while_the_session_runs_and_was_not_stopped() {
        assert!(ReconnectPolicy::should_reconnect(true, false));
        assert!(!ReconnectPolicy::should_reconnect(true, true));
        assert!(!ReconnectPolicy::should_reconnect(false, false));
        assert!(!ReconnectPolicy::should_reconnect(false, true));
    }
}
