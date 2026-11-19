//! Per-key send lanes (`coalesce.js`): one send in flight per key, and while it is, pushes replace
//! the single value waiting, so the latest always goes out next and the ones in between never do.
//!
//! A fader dragged across its track sends its first value, then only the latest after each answer.
//! A failed send still releases the lane (and is reported to `on_error`); [Lanes::idle] resolves
//! once nothing is in flight or waiting for a key. The sends run as flows of a [Console].

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use crate::console::Console;
use crate::exec::Slot;

/// A lane's send: the key and the value to send, resolving to `Err` with the reason it failed.
pub type Send<K, V> = Rc<dyn Fn(K, V) -> Pin<Box<dyn Future<Output = Result<(), String>>>>>;

/// What a failed send reports: the reason, the key and the value it sent.
pub type OnError<K, V> = Rc<dyn Fn(&str, &K, &V)>;

struct Lane<V> {
    waiting: Option<V>,
    idle: Vec<Slot<()>>,
}

struct Shared<K, V> {
    console: Console,
    send: Send<K, V>,
    on_error: RefCell<Option<OnError<K, V>>>,
    lanes: RefCell<BTreeMap<K, Lane<V>>>,
}

/// Lanes keyed by `K` carrying values `V`; clones share the lanes.
pub struct Lanes<K, V>(Rc<Shared<K, V>>);

impl<K, V> Clone for Lanes<K, V> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<K: Ord + Clone + 'static, V: Clone + 'static> Lanes<K, V> {
    /// Lanes whose sends run as `console`'s flows.
    pub fn new(console: &Console, send: Send<K, V>) -> Self {
        Self(Rc::new(Shared {
            console: console.clone(),
            send,
            on_error: RefCell::new(None),
            lanes: RefCell::new(BTreeMap::new()),
        }))
    }

    /// The same lanes, reporting each failed send from now on to `on_error`: the lanes' clones
    /// share the reporter, and a send already in flight reports to it when it fails.
    pub fn on_error(self, on_error: OnError<K, V>) -> Self {
        *self.0.on_error.borrow_mut() = Some(on_error);
        self
    }

    /// Sends `value` for `key` now, or after the send in flight for `key` (replacing any value
    /// already waiting there).
    pub fn push(&self, key: K, value: V) {
        {
            let mut lanes = self.0.lanes.borrow_mut();
            if let Some(lane) = lanes.get_mut(&key) {
                lane.waiting = Some(value);
                return;
            }
            lanes.insert(
                key.clone(),
                Lane {
                    waiting: None,
                    idle: Vec::new(),
                },
            );
        }
        let shared = Rc::clone(&self.0);
        self.0.console.spawn(async move {
            let mut value = value;
            loop {
                let sent = (shared.send)(key.clone(), value.clone()).await;
                // Out of the cell before it runs, so a reporter may set another one.
                let on_error = shared.on_error.borrow().clone();
                if let (Err(error), Some(on_error)) = (&sent, on_error) {
                    on_error(error, &key, &value);
                }
                let next = shared
                    .lanes
                    .borrow_mut()
                    .get_mut(&key)
                    .and_then(|lane| lane.waiting.take());
                match next {
                    Some(waiting) => value = waiting,
                    None => {
                        let lane = shared.lanes.borrow_mut().remove(&key);
                        for idle in lane.map(|lane| lane.idle).unwrap_or_default() {
                            idle.fill(());
                        }
                        break;
                    }
                }
            }
        });
    }

    /// True while a send for `key` is in flight or waiting.
    pub fn busy(&self, key: &K) -> bool {
        self.0.lanes.borrow().contains_key(key)
    }

    /// Resolves once nothing is in flight or waiting for `key`.
    pub fn idle(&self, key: &K) -> impl Future<Output = ()> + 'static {
        let slot = Slot::new();
        match self.0.lanes.borrow_mut().get_mut(key) {
            Some(lane) => lane.idle.push(slot.clone()),
            None => slot.fill(()),
        }
        slot.wait()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::console::Options;

    /// A send that never settles on its own, so each test decides when an answer arrives.
    struct Held {
        calls: Rc<RefCell<Vec<(String, u32, Slot<Result<(), String>>)>>>,
    }

    impl Held {
        fn new() -> Self {
            Self {
                calls: Rc::new(RefCell::new(Vec::new())),
            }
        }

        fn send(&self) -> Send<String, u32> {
            let calls = Rc::clone(&self.calls);
            Rc::new(move |key, value| {
                let answer = Slot::new();
                calls.borrow_mut().push((key, value, answer.clone()));
                Box::pin(answer.wait())
            })
        }

        fn sent(&self) -> Vec<(String, u32)> {
            self.calls
                .borrow()
                .iter()
                .map(|(key, value, _)| (key.clone(), *value))
                .collect()
        }

        fn answer(&self, console: &Console, call: usize, result: Result<(), String>) {
            let slot = self.calls.borrow()[call].2.clone();
            slot.fill(result);
            console.drive();
        }
    }

    fn console() -> Console {
        Console::new(Options::default())
    }

    #[test]
    fn the_first_value_for_a_key_is_sent_at_once() {
        let console = console();
        let held = Held::new();
        let lanes = Lanes::new(&console, held.send());

        lanes.push("bass".to_owned(), 1);

        assert_eq!(held.sent(), [("bass".to_owned(), 1)]);
        assert!(lanes.busy(&"bass".to_owned()));
    }

    #[test]
    fn values_pushed_while_a_send_is_in_flight_wait_and_only_the_latest_goes_out_after_it() {
        let console = console();
        let held = Held::new();
        let lanes = Lanes::new(&console, held.send());

        lanes.push("bass".to_owned(), 1);
        lanes.push("bass".to_owned(), 2);
        lanes.push("bass".to_owned(), 3);
        assert_eq!(held.sent().len(), 1, "one send in flight per key");

        held.answer(&console, 0, Ok(()));
        let values: Vec<_> = held.sent().into_iter().map(|(_, value)| value).collect();
        assert_eq!(
            values,
            [1, 3],
            "the latest value wins and the one in between is dropped"
        );

        held.answer(&console, 1, Ok(()));
        assert_eq!(held.sent().len(), 2);
        assert!(!lanes.busy(&"bass".to_owned()));
    }

    #[test]
    fn a_failed_send_still_lets_the_waiting_value_go_out_and_reports_the_failure() {
        let console = console();
        let held = Held::new();
        let failures = Rc::new(RefCell::new(Vec::new()));
        let reported = Rc::clone(&failures);
        let lanes =
            Lanes::new(&console, held.send()).on_error(Rc::new(move |error, key, value| {
                reported
                    .borrow_mut()
                    .push((error.to_owned(), key.clone(), *value));
            }));

        lanes.push("bass".to_owned(), 1);
        lanes.push("bass".to_owned(), 2);
        held.answer(&console, 0, Err("refused".to_owned()));

        assert_eq!(
            *failures.borrow(),
            [("refused".to_owned(), "bass".to_owned(), 1)]
        );
        let values: Vec<_> = held.sent().into_iter().map(|(_, value)| value).collect();
        assert_eq!(values, [1, 2], "the trailing send is guaranteed");
    }

    #[test]
    fn a_reporter_set_while_the_lanes_are_shared_or_sending_hears_every_later_failure() {
        let console = console();
        let held = Held::new();
        let lanes = Lanes::new(&console, held.send());
        let clone = lanes.clone();
        lanes.push("bass".to_owned(), 1);

        let failures = Rc::new(RefCell::new(Vec::new()));
        let reported = Rc::clone(&failures);
        let lanes = lanes.on_error(Rc::new(move |error, key, value| {
            reported
                .borrow_mut()
                .push((error.to_owned(), key.clone(), *value));
        }));
        held.answer(&console, 0, Err("refused".to_owned()));
        clone.push("drums".to_owned(), 2);
        held.answer(&console, 1, Err("offline".to_owned()));

        assert_eq!(
            *failures.borrow(),
            [
                ("refused".to_owned(), "bass".to_owned(), 1),
                ("offline".to_owned(), "drums".to_owned(), 2),
            ],
            "the send in flight and a clone's send both report"
        );
        assert!(!lanes.busy(&"bass".to_owned()));
    }

    #[test]
    fn a_send_that_fails_at_once_counts_as_finished() {
        let console = console();
        let attempts = Rc::new(RefCell::new(0));
        let counted = Rc::clone(&attempts);
        let lanes: Lanes<String, u32> = Lanes::new(
            &console,
            Rc::new(move |_, _| {
                *counted.borrow_mut() += 1;
                Box::pin(async { Err("no desk".to_owned()) })
            }),
        );

        lanes.push("bass".to_owned(), 1);
        lanes.push("bass".to_owned(), 2);

        assert_eq!(*attempts.borrow(), 2);
        assert!(!lanes.busy(&"bass".to_owned()));
    }

    #[test]
    fn each_key_has_its_own_lane() {
        let console = console();
        let held = Held::new();
        let lanes = Lanes::new(&console, held.send());

        lanes.push("bass".to_owned(), 1);
        lanes.push("drums".to_owned(), 2);

        let keys: Vec<_> = held.sent().into_iter().map(|(key, _)| key).collect();
        assert_eq!(keys, ["bass", "drums"]);
    }

    #[test]
    fn idle_resolves_once_nothing_is_in_flight_or_waiting_for_that_key() {
        let console = console();
        let held = Held::new();
        let lanes = Lanes::new(&console, held.send());
        let idle = Rc::new(RefCell::new(Vec::new()));

        let seen = Rc::clone(&idle);
        let quiet = lanes.idle(&"bass".to_owned());
        console.spawn(async move {
            quiet.await;
            seen.borrow_mut().push("before");
        });
        assert_eq!(*idle.borrow(), ["before"], "an idle lane is idle at once");

        lanes.push("bass".to_owned(), 1);
        lanes.push("bass".to_owned(), 2);
        let seen = Rc::clone(&idle);
        let busy = lanes.idle(&"bass".to_owned());
        console.spawn(async move {
            busy.await;
            seen.borrow_mut().push("after");
        });
        held.answer(&console, 0, Ok(()));
        assert_eq!(
            *idle.borrow(),
            ["before"],
            "the trailing send is still going"
        );
        held.answer(&console, 1, Ok(()));
        assert_eq!(*idle.borrow(), ["before", "after"]);
    }
}
