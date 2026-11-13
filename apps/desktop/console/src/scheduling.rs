//! A throttle that always runs once more after the last push (`trailingThrottle` of
//! `scheduling.js`), with the clock and the timer left to its owner, so it is pure.
//!
//! A reduction meter speaks its value through one at 500 ms: the first push runs at once, pushes
//! within the interval fold into one run at its end, and a cancel drops that run.

use std::time::Duration;

/// What a push asks of the throttle's owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Push {
    /// Run now.
    Run,
    /// Start a timer for `after` and call [TrailingThrottle::fire] with `timer` when it ends.
    RunAfter { after: Duration, timer: u64 },
    /// A run is already waiting for its timer: nothing to do.
    Waiting,
}

/// The throttle's state. Times are what the owner's clock says, from any fixed start.
#[derive(Clone, Debug)]
pub struct TrailingThrottle {
    interval: Duration,
    last_run: Option<Duration>,
    waiting: Option<u64>,
    timers: u64,
}

impl TrailingThrottle {
    /// A throttle that runs at most once every `interval`, and always once after the last push.
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_run: None,
            waiting: None,
            timers: 0,
        }
    }

    /// A push at `now`.
    pub fn push(&mut self, now: Duration) -> Push {
        if self.waiting.is_some() {
            return Push::Waiting;
        }
        let due = self.last_run.map_or(now, |last| last + self.interval);
        if due <= now {
            self.last_run = Some(now);
            return Push::Run;
        }
        self.timers += 1;
        self.waiting = Some(self.timers);
        Push::RunAfter {
            after: due - now,
            timer: self.timers,
        }
    }

    /// The timer `timer` ended at `now`: true when the run it waited for is still due (it was
    /// not cancelled, nor replaced).
    pub fn fire(&mut self, timer: u64, now: Duration) -> bool {
        if self.waiting != Some(timer) {
            return false;
        }
        self.waiting = None;
        self.last_run = Some(now);
        true
    }

    /// Drops the run waiting for its timer, if any.
    pub fn cancel(&mut self) {
        self.waiting = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The throttle with a clock and timers the test runs by hand, and what it spoke.
    struct Speaker {
        throttle: TrailingThrottle,
        clock: Duration,
        timers: Vec<(u64, Duration)>,
        value: u32,
        spoken: Vec<u32>,
    }

    impl Speaker {
        fn push(&mut self, value: u32) {
            self.value = value;
            match self.throttle.push(self.clock) {
                Push::Run => self.spoken.push(self.value),
                Push::RunAfter { after, timer } => self.timers.push((timer, self.clock + after)),
                Push::Waiting => {}
            }
        }

        fn advance(&mut self, ms: u64) {
            self.clock += Duration::from_millis(ms);
            let due: Vec<_> = self
                .timers
                .iter()
                .copied()
                .filter(|(_, at)| *at <= self.clock)
                .collect();
            self.timers.retain(|(_, at)| *at > self.clock);
            for (timer, _) in due {
                if self.throttle.fire(timer, self.clock) {
                    self.spoken.push(self.value);
                }
            }
        }
    }

    #[test]
    fn a_trailing_throttle_runs_at_once_then_once_more_at_the_end_with_the_latest_value() {
        let mut speaker = Speaker {
            throttle: TrailingThrottle::new(Duration::from_millis(500)),
            clock: Duration::ZERO,
            timers: Vec::new(),
            value: 0,
            spoken: Vec::new(),
        };
        speaker.push(1);
        assert_eq!(speaker.spoken, [1], "the first push runs at once");
        speaker.advance(100);
        speaker.push(2);
        speaker.advance(100);
        speaker.push(3);
        assert_eq!(speaker.spoken, [1], "pushes inside the interval wait");
        speaker.advance(300);
        assert_eq!(
            speaker.spoken,
            [1, 3],
            "the last value is run when the interval ends"
        );
        speaker.advance(2000);
        assert_eq!(speaker.spoken, [1, 3], "nothing more once the pushes stop");
        speaker.push(4);
        assert_eq!(
            speaker.spoken,
            [1, 3, 4],
            "after a quiet interval a push runs at once"
        );
        speaker.advance(10);
        speaker.push(5);
        speaker.throttle.cancel();
        speaker.advance(1000);
        assert_eq!(speaker.spoken, [1, 3, 4], "a cancelled run never comes");
    }
}
