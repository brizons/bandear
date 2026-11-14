//! A removal that asks first (N7, N39): the first press arms it, the confirming press works only
//! once it has waited [CONFIRM_DELAY] (so a double click never confirms), and it disarms by itself
//! after [ARMED_FOR]. The model keeps the timers (console sleeps, so tests fire them); this keeps
//! the state: what is armed, whether it is ready, and a generation that tells a late timer from the
//! current arming.

use std::time::Duration;

pub use bandear_desktop_kit::buttons::{CONFIRM_DELAY, CONFIRM_TIMEOUT as ARMED_FOR};

/// What is armed, and whether its confirmation works yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arming<K> {
    armed: Option<(K, bool)>,
    generation: u64,
}

impl<K> Default for Arming<K> {
    fn default() -> Self {
        Self {
            armed: None,
            generation: 0,
        }
    }
}

impl<K: PartialEq + Clone> Arming<K> {
    /// Arms `key` (disarming anything else); answers the arming's generation for its timers.
    pub fn arm(&mut self, key: K) -> u64 {
        self.generation += 1;
        self.armed = Some((key, false));
        self.generation
    }

    /// The pause of arming `generation` is over: its confirmation works now.
    pub fn ready(&mut self, generation: u64) -> bool {
        match &mut self.armed {
            Some((_, ready)) if self.generation == generation && !*ready => {
                *ready = true;
                true
            }
            _ => false,
        }
    }

    /// Arming `generation` timed out: it disarms, unless another arming came since.
    pub fn expire(&mut self, generation: u64) -> bool {
        if self.generation == generation && self.armed.is_some() {
            self.armed = None;
            return true;
        }
        false
    }

    /// Keep, Escape, or another choice: nothing stays armed.
    pub fn disarm(&mut self) {
        self.generation += 1;
        self.armed = None;
    }

    /// What is armed.
    pub fn armed(&self) -> Option<&K> {
        self.armed.as_ref().map(|(key, _)| key)
    }

    pub fn is_armed(&self, key: &K) -> bool {
        self.armed.as_ref().is_some_and(|(armed, _)| armed == key)
    }

    pub fn is_ready(&self, key: &K) -> bool {
        self.armed
            .as_ref()
            .is_some_and(|(armed, ready)| armed == key && *ready)
    }

    /// The confirming press for `key`: true (and disarmed) only once it is ready.
    pub fn confirm(&mut self, key: &K) -> bool {
        if self.is_ready(key) {
            self.disarm();
            return true;
        }
        false
    }
}

/// How long after the pause the arming still waits before it disarms itself.
pub fn rest_of_arming() -> Duration {
    ARMED_FOR.saturating_sub(CONFIRM_DELAY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_confirmation_works_only_after_the_pause_and_the_arming_ends_by_itself() {
        assert_eq!(CONFIRM_DELAY, Duration::from_millis(600));
        assert_eq!(ARMED_FOR, Duration::from_secs(10));
        let mut arming = Arming::default();
        let first = arming.arm("bass");
        assert!(arming.is_armed(&"bass"));
        assert!(!arming.confirm(&"bass"), "a double click keeps it");
        assert!(arming.ready(first));
        assert!(!arming.confirm(&"keys"), "only what is armed");
        assert!(arming.confirm(&"bass"));
        assert!(!arming.is_armed(&"bass"));

        let second = arming.arm("bass");
        let third = arming.arm("keys");
        assert!(
            !arming.ready(second),
            "a late timer of an earlier arming does nothing"
        );
        assert!(!arming.expire(second));
        assert!(arming.is_armed(&"keys"), "arming another disarms the first");
        assert!(arming.expire(third));
        assert_eq!(arming.armed(), None);
        let fourth = arming.arm("keys");
        arming.disarm();
        assert!(!arming.ready(fourth));
    }
}
