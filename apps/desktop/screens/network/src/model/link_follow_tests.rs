//! `link-health-controller.test.js` and the follower's tests of `link-health-view.test.js`, with
//! the console's harness for a clock: a poll waits on a console timer, fired by the test.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use bandear_desktop_console::command::{Answer, Reply};
use bandear_desktop_console::console::Options;
use bandear_desktop_console::testing::{FakeDesk, Harness};
use bandear_desktop_console::types::dto::LinkSampleDto;

use super::*;

fn dto_sample(seq: u64) -> LinkSampleDto {
    LinkSampleDto {
        seq,
        mono_ms: seq * 1000,
        at_ms: seq * 1000,
        gap: false,
        reconnected: false,
        health: "good",
        interval_ms: None,
        packets_received: None,
        packets_lost: None,
        loss_percent: None,
        underruns: None,
        receive_timeouts: None,
        rtt_ms: None,
        jitter_ms: None,
        cushion_ms: None,
        backlog_ms: None,
        recovered: None,
    }
}

fn answer(history_id: u64, from: u64, to: u64) -> Reply {
    Ok(Answer::LinkHistory(LinkHistoryDto {
        musician_key: "client:a".to_owned(),
        history_id,
        samples: (from..=to).map(dto_sample).collect(),
        max_samples: 600,
        period_ms: 1000,
    }))
}

fn person(address: &str, client_id: Option<&str>, name: &str) -> Musician {
    Musician {
        address: address.to_owned(),
        client_id: client_id.map(str::to_owned),
        name: Some(name.to_owned()).filter(|name| !name.is_empty()),
        ..Musician::default()
    }
}

fn bass() -> Musician {
    person("192.168.1.12:50000", Some("a"), "Bass")
}

fn keys() -> Musician {
    person("192.168.1.13:50000", Some("b"), "Keys")
}

fn by_address() -> Musician {
    person("192.168.1.20:50000", None, "")
}

/// The harness, its link history scripted by `reply(key, since)`.
fn harness(reply: impl Fn(&str, Option<u64>) -> Reply + 'static) -> Harness {
    let mut desk = FakeDesk::running();
    desk.script("link_history", move |_, command| match command {
        Command::LinkHistory {
            musician_key,
            since_seq,
        } => reply(musician_key, *since_seq),
        _ => Err("not a history read".to_owned()),
    });
    Harness::new(desk, Options::default())
}

/// The reads the harness saw, as `(key, sinceSeq)`.
fn reads(harness: &Harness) -> Vec<(String, Option<u64>)> {
    harness
        .calls_to("link_history")
        .into_iter()
        .filter_map(|command| match command {
            Command::LinkHistory {
                musician_key,
                since_seq,
            } => Some((musician_key.clone(), *since_seq)),
            _ => None,
        })
        .collect()
}

fn controller(harness: &Harness) -> (LinkHealth, Rc<Cell<usize>>) {
    let changes = Rc::new(Cell::new(0));
    let counted = Rc::clone(&changes);
    let health = LinkHealth::new(
        &harness.console,
        Rc::new(move || counted.set(counted.get() + 1)),
    );
    (health, changes)
}

const UTC: Zone = Zone::Fixed(0);

#[test]
fn the_follower_reads_once_then_only_what_is_new_every_interval_and_stops() {
    let answers = Rc::new(RefCell::new(VecDeque::from([
        answer(1, 1, 3),
        answer(1, 4, 4),
        answer(2, 9, 9),
        answer(2, 1, 9),
    ])));
    let replies = Rc::clone(&answers);
    let mut harness = harness(move |_, _| replies.borrow_mut().pop_front().expect("an answer"));
    let changes = Rc::new(RefCell::new(Vec::new()));
    let heard = Rc::clone(&changes);
    let follower = Follower::new(
        harness.console.clone(),
        Rc::new(move |buffer: Option<History>, error| {
            heard
                .borrow_mut()
                .push((buffer.map_or(0, |buffer| buffer.samples.len()), error));
        }),
    );

    follower.follow(Some("client:a".to_owned()));
    harness.settle();
    assert!(harness.timers_waiting(), "the next read waits a second");
    harness.fire_timers();
    harness.fire_timers();

    assert_eq!(
        reads(&harness),
        [
            ("client:a".to_owned(), None),
            ("client:a".to_owned(), Some(3)),
            ("client:a".to_owned(), Some(4)),
            ("client:a".to_owned(), None),
        ]
    );
    assert_eq!(*changes.borrow(), [(3, None), (4, None), (9, None)]);

    follower.stop();
    harness.run_timers();
    assert_eq!(reads(&harness).len(), 4, "no more reads");
}

#[test]
fn a_failed_read_is_said_and_the_follower_keeps_trying_with_what_it_had() {
    let fail = Rc::new(Cell::new(false));
    let failing = Rc::clone(&fail);
    let mut harness = harness(move |_, _| {
        if failing.get() {
            Err("the engine is not running".to_owned())
        } else {
            answer(1, 1, 2)
        }
    });
    let changes = Rc::new(RefCell::new(Vec::new()));
    let heard = Rc::clone(&changes);
    let follower = Follower::new(
        harness.console.clone(),
        Rc::new(move |buffer: Option<History>, error| {
            heard
                .borrow_mut()
                .push((buffer.map_or(0, |buffer| buffer.samples.len()), error));
        }),
    );

    follower.follow(Some("client:a".to_owned()));
    harness.settle();
    fail.set(true);
    harness.fire_timers();
    assert_eq!(
        changes.borrow().last(),
        Some(&(2, Some("the engine is not running".to_owned())))
    );
    harness.fire_timers();
    assert_eq!(changes.borrow().len(), 2, "the same failure is said once");
    assert!(harness.timers_waiting(), "tries again");
}

#[test]
fn following_another_musician_drops_the_first_ones_history_and_ignores_a_late_answer() {
    let mut harness = harness(|key, _| {
        if key == "client:a" {
            answer(1, 1, 5)
        } else {
            answer(2, 1, 2)
        }
    });
    let changes = Rc::new(RefCell::new(Vec::new()));
    let heard = Rc::clone(&changes);
    let follower = Follower::new(
        harness.console.clone(),
        Rc::new(move |buffer: Option<History>, _| {
            heard
                .borrow_mut()
                .push(buffer.map(|buffer| buffer.history_id));
        }),
    );
    harness.hold("link_history");

    follower.follow(Some("client:a".to_owned()));
    follower.follow(Some("client:b".to_owned()));
    harness.settle();
    harness.release();

    assert_eq!(
        *changes.borrow(),
        [Some(2)],
        "only the musician followed now"
    );
    assert_eq!(follower.buffer().map(|buffer| buffer.history_id), Some(2));
}

#[test]
fn picking_a_musician_opens_their_panel_and_follows_their_history() {
    let mut harness = harness(|_, _| answer(1, 1, 3));
    let (health, changes) = controller(&harness);
    let listed = [bass(), keys()];

    assert_eq!(health.view(&listed, UTC), (None, None), "closed at first");
    health.select(&listed, &bass().address);
    harness.settle();

    let (selected, panel) = health.view(&listed, UTC);
    assert_eq!(selected.as_deref(), Some(bass().address.as_str()));
    let panel = panel.expect("the panel");
    assert_eq!(panel.title, "Bass \u{b7} last 10 minutes");
    assert_eq!(panel.window.samples.len(), 3);
    assert_eq!(reads(&harness), [("client:a".to_owned(), None)]);
    assert_eq!(changes.get(), 1, "the screen hears of the history");
    assert!(harness.timers_waiting(), "keeps polling");
}

#[test]
fn picking_the_open_musician_again_closes_the_panel_and_stops_the_follower() {
    let mut harness = harness(|_, _| answer(1, 1, 3));
    let (health, _) = controller(&harness);
    let listed = [bass(), keys()];
    health.select(&listed, &bass().address);
    harness.settle();

    health.select(&listed, &bass().address);

    assert_eq!(health.view(&listed, UTC), (None, None));
    harness.run_timers();
    assert_eq!(reads(&harness).len(), 1, "no more reads");
}

#[test]
fn picking_another_musician_follows_them_instead() {
    let mut harness = harness(|_, _| answer(1, 1, 3));
    let (health, _) = controller(&harness);
    let listed = [bass(), keys()];
    health.select(&listed, &bass().address);
    harness.settle();

    health.select(&listed, &keys().address);
    harness.settle();

    assert_eq!(
        health.view(&listed, UTC).0.as_deref(),
        Some(keys().address.as_str())
    );
    assert_eq!(
        reads(&harness).last(),
        Some(&("client:b".to_owned(), None)),
        "read whole for the new musician"
    );
}

#[test]
fn the_panel_closes_and_the_follower_stops_when_the_musician_leaves_the_list() {
    let mut harness = harness(|_, _| answer(1, 1, 3));
    let (health, _) = controller(&harness);
    health.select(&[bass(), keys()], &bass().address);
    harness.settle();

    assert_eq!(health.view(&[keys()], UTC), (None, None));
    harness.run_timers();
    assert_eq!(reads(&harness).len(), 1);
    assert_eq!(
        health.view(&[bass(), keys()], UTC),
        (None, None),
        "stays closed"
    );
}

#[test]
fn the_panel_closes_and_the_follower_stops_when_the_engine_stops_nobody_is_listed() {
    let mut harness = harness(|_, _| answer(1, 1, 3));
    let (health, _) = controller(&harness);
    health.select(&[bass()], &bass().address);
    harness.settle();

    health.follow_list(&[]);
    assert_eq!(health.view(&[], UTC).1, None);
    harness.run_timers();
    assert_eq!(reads(&harness).len(), 1);
}

#[test]
fn a_failed_read_reaches_the_panel_and_the_next_good_read_clears_it() {
    let fail = Rc::new(Cell::new(false));
    let failing = Rc::clone(&fail);
    let mut harness = harness(move |_, _| {
        if failing.get() {
            Err("the engine is not running".to_owned())
        } else {
            answer(1, 1, 3)
        }
    });
    let (health, _) = controller(&harness);
    health.select(&[bass()], &bass().address);
    harness.settle();
    fail.set(true);
    harness.fire_timers();

    let failed = health.view(&[bass()], UTC).1.expect("the panel");
    assert_eq!(
        failed.message.as_deref(),
        Some("The link history could not be read: the engine is not running.")
    );
    assert_eq!(failed.window.samples.len(), 3, "the history held is kept");

    fail.set(false);
    harness.fire_timers();
    assert_eq!(
        health.view(&[bass()], UTC).1.expect("the panel").message,
        None
    );
}

#[test]
fn the_range_and_the_table_are_the_panels_own_closing_hides_the_table_but_keeps_the_range() {
    let mut harness = harness(|_, _| answer(1, 1, 3));
    let (health, _) = controller(&harness);
    health.select(&[bass()], &bass().address);
    health.set_range(60_000);
    health.show_table(true);
    harness.settle();

    let view = health.view(&[bass()], UTC).1.expect("the panel");
    assert_eq!(view.range_ms, 60_000);
    assert!(view.table.is_some());

    health.close();
    harness.run_timers();
    assert_eq!(reads(&harness).len(), 1, "closing stops the reads");
    health.select(&[bass()], &bass().address);
    let reopened = health.view(&[bass()], UTC).1.expect("the panel");
    assert_eq!(reopened.range_ms, 60_000);
    assert_eq!(reopened.table, None);
}

#[test]
fn going_away_stops_the_follower_and_a_late_answer_changes_nothing() {
    let mut harness = harness(|_, _| answer(1, 1, 3));
    let (health, changes) = controller(&harness);
    harness.hold("link_history");
    health.select(&[bass()], &bass().address);
    harness.settle();

    health.close();
    harness.release();
    harness.run_timers();

    assert_eq!(reads(&harness).len(), 1, "no more reads");
    assert_eq!(changes.get(), 0, "the late answer is dropped");
}

#[test]
fn a_phone_listed_by_address_that_registers_keeps_its_panel_open_followed_by_its_new_key() {
    let mut harness = harness(|_, _| answer(1, 1, 3));
    let (health, _) = controller(&harness);
    health.select(&[by_address()], &by_address().address);
    harness.settle();
    assert_eq!(
        reads(&harness),
        [("address:192.168.1.20:50000".to_owned(), None)]
    );

    let registered = person("192.168.1.20:50000", Some("p"), "Drums");
    let (selected, panel) = health.view(std::slice::from_ref(&registered), UTC);
    assert_eq!(selected.as_deref(), Some(registered.address.as_str()));
    let panel = panel.expect("the panel");
    assert_eq!(panel.title, "Drums \u{b7} last 10 minutes");
    assert_eq!(
        panel.window.samples.len(),
        3,
        "the history held stays until the new read"
    );
    harness.settle();
    assert_eq!(
        reads(&harness).last(),
        Some(&("client:p".to_owned(), None)),
        "followed by the client id now"
    );

    health.select(std::slice::from_ref(&registered), &registered.address);
    assert_eq!(
        health.view(std::slice::from_ref(&registered), UTC).1,
        None,
        "picking them again still closes"
    );
}

#[test]
fn a_row_keyed_by_address_is_only_followed_to_a_registered_row_at_the_same_address() {
    let key = "address:192.168.1.20:50000";
    let address = Some("192.168.1.20:50000");
    let elsewhere = person("192.168.1.21:50000", Some("p"), "");
    assert_eq!(
        followed_musician(&[elsewhere], key, address),
        None,
        "another address"
    );
    assert_eq!(
        followed_musician(&[by_address()], key, address),
        Some(&by_address()),
        "still by address"
    );
    let registered = person("192.168.1.20:50000", Some("p"), "");
    assert_eq!(
        followed_musician(std::slice::from_ref(&registered), key, address),
        Some(&registered),
        "registered"
    );
    let moved = person("192.168.1.20:50000", Some("a"), "Bass");
    assert_eq!(
        followed_musician(&[moved], "client:z", address),
        None,
        "a phone with a client id is never followed to another one at its old address"
    );
}

#[test]
fn a_history_read_asks_the_desk_for_all_of_it_or_since_a_sample() {
    let mut harness = harness(|_, _| answer(1, 1, 3));
    let follower = Follower::new(harness.console.clone(), Rc::new(|_, _| {}));
    follower.follow(Some("client:a".to_owned()));
    harness.settle();
    harness.fire_timers();
    assert_eq!(
        reads(&harness),
        [
            ("client:a".to_owned(), None),
            ("client:a".to_owned(), Some(3))
        ]
    );
}
