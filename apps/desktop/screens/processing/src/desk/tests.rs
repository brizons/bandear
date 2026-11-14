//! The processing commands over the console's simulated desk (`processing.test.js`'s console
//! tests).

use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;

use bandear_desktop_console::command::{Command, Owner};
use bandear_desktop_console::console::Options;
use bandear_desktop_console::router::Section;
use bandear_desktop_console::state::{Action, ShownStrip};
use bandear_desktop_console::testing::{FakeDesk, Harness};
use bandear_desktop_console::types::SnapshotSummary;
use bandear_desktop_console::types::dto::SessionLoadedDto;

use super::*;
use crate::strip::STRIP_CHANNELS;
use crate::strip::tests::{boosted, compressed};
use crate::testdesk::{Shared as Kept, factory_presets, processing_desk};

pub(crate) struct Desk {
    pub harness: Harness,
    pub strips: Strips,
}

/// The desk running the sample interface with CH 3-4 linked when `link`, its strips as `strips`
/// changes them before the console connects.
pub(crate) fn desk_with(link: bool, change: impl FnOnce(&mut FakeDesk, &Kept)) -> Desk {
    let mut fake = FakeDesk::running();
    if link {
        fake.report.status.channels[2].link = true;
    }
    let kept = processing_desk(&mut fake);
    change(&mut fake, &kept);
    let harness = Harness::connected(fake, Options::default());
    let strips = Strips::new(
        &harness
            .console
            .for_owner(Owner::Screen(Section::Processing)),
    );
    Desk { harness, strips }
}

pub(crate) fn desk() -> Desk {
    desk_with(false, |_, _| {})
}

/// Runs `flow` and hands back where its outcome lands once it ends.
pub(crate) fn spawned<T: 'static>(
    harness: &mut Harness,
    flow: impl Future<Output = T> + 'static,
) -> Rc<RefCell<Option<T>>> {
    let outcome = Rc::new(RefCell::new(None));
    let kept = Rc::clone(&outcome);
    harness.run(async move {
        let value = flow.await;
        *kept.borrow_mut() = Some(value);
    });
    outcome
}

/// A `load_session` answer.
pub(crate) fn loaded() -> SessionLoadedDto {
    SessionLoadedDto {
        session: SnapshotSummary {
            id: "show".to_owned(),
            name: "Show".to_owned(),
            saved_at: None,
            mixes: 0,
            problem: None,
        },
        applied: true,
        saved: true,
        save_error: None,
        mixes: Vec::new(),
        skipped: Vec::new(),
        refused: Vec::new(),
        damaged: Vec::new(),
        audio: None,
        audio_loaded: false,
        audio_skipped: None,
        presets: Vec::new(),
        local_loaded: false,
        local_mix: None,
        local_output_notice: None,
    }
}

impl Desk {
    pub fn run<T: 'static>(&mut self, flow: impl Future<Output = T> + 'static) -> T {
        self.harness.run(flow).expect("the flow ends")
    }

    pub fn strip(&self, channel: usize) -> ShownStrip {
        self.harness.console.state().channel_strip(channel)
    }

    pub fn sent(&self, name: &str) -> Vec<Command> {
        self.harness.calls_to(name).into_iter().cloned().collect()
    }
}

#[test]
fn the_console_reads_the_strips_and_presets_once_connected_and_every_command_keeps_its_answer() {
    let mut desk = desk_with(false, |_, kept| {
        kept.borrow_mut().strips[1] = compressed(-18.0);
    });
    {
        let state = desk.harness.console.state();
        assert_eq!(
            state.processing.as_ref().map(Vec::len),
            Some(STRIP_CHANNELS)
        );
        assert_eq!(state.strip_presets.as_ref().map(Vec::len), Some(5));
    }
    assert_eq!(
        desk.strip(2),
        ShownStrip {
            channel: 2,
            settings: compressed(-18.0),
            active: true,
            pending: false,
        }
    );

    let reset = desk.strips.reset(2);
    assert_eq!(desk.run(reset), Said::ok("CH 2 is back to no processing."));
    assert!(!desk.strip(2).active);

    let apply = desk.strips.apply_preset("Kick".to_owned(), 1);
    assert_eq!(desk.run(apply), Said::ok("\"Kick\" is on CH 1 now."));
    assert_eq!(desk.strip(1).settings, factory_presets()[1].1);
    let copy = desk.strips.copy(1, vec![6]);
    assert_eq!(
        desk.run(copy),
        Said::ok("CH 1's processing is on CH 6 now.")
    );
    assert!(desk.strip(6).active);
    let missing = desk.strips.apply_preset("Tuba".to_owned(), 1);
    let missing = desk.run(missing);
    assert!(!missing.ok);
    assert!(missing.message.contains("Tuba"), "{}", missing.message);

    let save = desk.strips.save_preset("Mine".to_owned(), boosted(3.0));
    assert_eq!(desk.run(save), Said::ok("Saved the strip preset \"Mine\"."));
    assert_eq!(
        desk.harness
            .console
            .state()
            .strip_presets
            .as_ref()
            .map(Vec::len),
        Some(6)
    );
    let refused = desk.strips.delete_preset("Vocal".to_owned());
    let refused = desk.run(refused);
    assert!(!refused.ok);
    assert!(
        refused.message.contains("cannot be deleted"),
        "{}",
        refused.message
    );
    let delete = desk.strips.delete_preset("Mine".to_owned());
    assert!(desk.run(delete).ok);
    assert_eq!(
        desk.harness
            .console
            .state()
            .strip_presets
            .as_ref()
            .map(Vec::len),
        Some(5)
    );

    let curve = desk.strips.load_eq_response(1, boosted(6.0).eq);
    let points = desk.run(curve).expect("a curve");
    assert_eq!(points.len(), 128);
    assert_eq!(
        desk.harness.console.state().eq_curves.get(&1),
        Some(&points)
    );

    desk.harness.desk.running = false;
    let stopped = desk.strips.reset(1);
    assert_eq!(
        desk.run(stopped),
        Said::refused("CH 1's processing was not reset: the engine is not running.")
    );
    desk.harness
        .desk
        .fail_next("save_strip_preset", "the disk is full", 1);
    let save = desk.strips.save_preset("Mine".to_owned(), boosted(3.0));
    assert_eq!(
        desk.run(save),
        Said::refused("The strip preset was not saved: the disk is full.")
    );
}

#[test]
fn a_dragged_strip_edit_shows_at_once_and_sends_only_its_latest_value_after_the_one_in_flight() {
    let mut desk = desk();
    desk.harness.hold("set_channel_processing");

    let first = spawned(
        &mut desk.harness,
        desk.strips.edit_processing(1, boosted(1.0)),
    );
    let middle = spawned(
        &mut desk.harness,
        desk.strips.edit_processing(1, boosted(2.0)),
    );
    let last = spawned(
        &mut desk.harness,
        desk.strips.edit_processing(1, boosted(3.0)),
    );
    assert_eq!(
        desk.strip(1),
        ShownStrip {
            channel: 1,
            settings: boosted(3.0),
            active: true,
            pending: true,
        }
    );
    assert_eq!(
        desk.sent("set_channel_processing").len(),
        1,
        "one in flight"
    );

    desk.harness.release();
    let gains: Vec<f32> = desk
        .sent("set_channel_processing")
        .iter()
        .map(|command| match command {
            Command::SetChannelProcessing { settings, .. } => settings.eq.bands[1].gain_db,
            _ => f32::NAN,
        })
        .collect();
    assert_eq!(gains, [1.0, 3.0], "the edit in between is never sent");
    assert!(desk.strip(1).pending, "the latest is still on its way");
    desk.harness.release();
    let set = Said::ok("CH 1 is set to the new processing.");
    assert_eq!(last.borrow().clone(), Some(set.clone()));
    assert_eq!(first.borrow().clone(), Some(set.clone()));
    assert_eq!(middle.borrow().clone(), Some(set));
    assert_eq!(
        desk.strip(1),
        ShownStrip {
            channel: 1,
            settings: boosted(3.0),
            active: true,
            pending: false,
        }
    );
}

#[test]
fn a_refused_strip_edit_is_said_settled_and_kept_as_the_strips_error() {
    let mut desk = desk();
    let outside = desk.strips.edit_processing(40, boosted(1.0));
    let outside = desk.run(outside);
    assert!(!outside.ok);
    assert!(outside.message.contains("CH 1-32"), "{}", outside.message);
    assert_eq!(
        desk.harness
            .console
            .state()
            .processing_error
            .as_ref()
            .and_then(|error| error.channel),
        Some(40)
    );

    desk.harness.desk.running = false;
    let stopped = desk.strips.edit_processing(2, boosted(1.0));
    assert_eq!(
        desk.run(stopped),
        Said::refused("CH 2's processing was not changed: the engine is not running.")
    );
    assert!(!desk.strip(2).pending, "a refusal settles the edit");
    assert_eq!(
        desk.harness
            .console
            .state()
            .processing_error
            .as_ref()
            .and_then(|error| error.channel),
        Some(2)
    );
}

#[test]
fn a_strip_edit_made_while_a_session_loads_is_said_to_be_dropped_or_sent_once_the_load_is_refused()
{
    let mut desk = desk();
    let console = desk.harness.console.clone();

    // A session loads (the Network screen's flow): every send is held back, then the load lands.
    console.hold_sends();
    let dropped = spawned(
        &mut desk.harness,
        desk.strips.edit_processing(1, boosted(2.0)),
    );
    assert!(desk.sent("set_channel_processing").is_empty(), "held back");
    assert_eq!(
        console.unsaved_edits(),
        1,
        "the waiting strip is an unsaved edit"
    );
    console.dispatch(Action::SessionLoaded(Box::new(loaded())));
    console.release_sends();
    desk.harness.settle();
    assert_eq!(
        dropped.borrow().clone(),
        Some(Said::refused(
            "CH 1's processing was not changed: a session was loaded meanwhile."
        )),
        "an edit the load dropped was never sent"
    );
    assert!(desk.sent("set_channel_processing").is_empty());
    assert!(!desk.strip(1).pending);

    // A refused load changed nothing, so the edit goes out once the hold ends.
    console.hold_sends();
    let kept = spawned(
        &mut desk.harness,
        desk.strips.edit_processing(1, boosted(3.0)),
    );
    assert!(kept.borrow().is_none(), "it waits for the load");
    console.release_sends();
    desk.harness.settle();
    assert_eq!(
        kept.borrow().clone(),
        Some(Said::ok("CH 1 is set to the new processing."))
    );
    assert_eq!(desk.sent("set_channel_processing").len(), 1);
}

#[test]
fn a_strip_edit_before_the_strips_were_ever_read_reads_them_all_as_the_desk_keeps_them() {
    let mut desk = desk_with(false, |fake, kept| {
        fake.fail_next("processing", "the desk is busy", 1);
        kept.borrow_mut().strips[4] = compressed(-12.0);
    });
    assert!(desk.harness.console.state().processing.is_none());

    let edit = desk.strips.edit_processing(1, boosted(2.0));
    assert!(desk.run(edit).ok);
    let state = desk.harness.console.state();
    assert_eq!(
        state.processing.as_ref().map(Vec::len),
        Some(STRIP_CHANNELS)
    );
    assert_eq!(
        state.channel_strip(5).settings,
        compressed(-12.0),
        "as the desk keeps it"
    );
    assert_eq!(state.channel_strip(1).settings, boosted(2.0));
}

#[test]
fn a_reset_a_copy_or_a_preset_waits_for_the_strip_edit_still_going_to_a_linked_partner() {
    let mut desk = desk_with(true, |_, _| {});
    desk.harness.hold("set_channel_processing");
    let order = |desk: &Desk| -> Vec<&'static str> {
        desk.harness
            .calls
            .iter()
            .map(Command::name)
            .filter(|name| name.ends_with("_processing") || *name == "apply_strip_preset")
            .collect()
    };

    let edit = spawned(
        &mut desk.harness,
        desk.strips.edit_processing(4, compressed(-30.0)),
    );
    let reset = spawned(&mut desk.harness, desk.strips.reset(3));
    let preset = spawned(
        &mut desk.harness,
        desk.strips.apply_preset("Kick".to_owned(), 3),
    );
    let copy = spawned(&mut desk.harness, desk.strips.copy(1, vec![3]));
    assert_eq!(
        order(&desk),
        ["set_channel_processing"],
        "CH 4's edit goes first"
    );

    desk.harness.release();
    for said in [&reset, &preset, &copy] {
        assert!(said.borrow().as_ref().is_some_and(|said| said.ok));
    }
    assert!(edit.borrow().as_ref().is_some_and(|said| said.ok));
    assert_eq!(
        order(&desk),
        [
            "set_channel_processing",
            "reset_channel_processing",
            "apply_strip_preset",
            "copy_channel_processing"
        ]
    );
    assert!(!desk.strip(4).active, "CH 1's neutral strip, as CH 3's");
}

#[test]
fn a_send_shows_at_once_goes_out_on_its_own_lane_and_says_a_refusal() {
    let mut desk = desk();
    desk.harness.hold("set_channel_send");
    let first = spawned(
        &mut desk.harness,
        desk.strips.edit_send(5, Effect::Reverb, 10.0),
    );
    let last = spawned(
        &mut desk.harness,
        desk.strips.edit_send(5, Effect::Reverb, 140.4),
    );
    let delay = spawned(
        &mut desk.harness,
        desk.strips.edit_send(5, Effect::Delay, 20.6),
    );
    {
        let state = desk.harness.console.state();
        let sends = channel_sends(&state, 5);
        assert_eq!(
            (sends.reverb, sends.delay),
            (100, 21),
            "shown at once, clamped"
        );
    }
    assert_eq!(
        desk.sent("set_channel_send").len(),
        2,
        "one lane per channel and effect"
    );
    desk.harness.release();
    desk.harness.release();
    assert_eq!(
        last.borrow().clone(),
        Some(Said::ok("CH 5 is sending 100% to the reverb."))
    );
    assert_eq!(first.borrow().clone(), last.borrow().clone());
    assert_eq!(
        delay.borrow().clone(),
        Some(Said::ok("CH 5 is sending 21% to the delay."))
    );
    {
        let state = desk.harness.console.state();
        assert!(state.sends_pending.is_empty(), "settled");
        let sends = channel_sends(&state, 5);
        assert_eq!(
            (sends.reverb, sends.delay),
            (100, 21),
            "as the desk keeps them"
        );
    }

    desk.harness.desk.running = false;
    let refused = spawned(
        &mut desk.harness,
        desk.strips.edit_send(5, Effect::Reverb, 30.0),
    );
    desk.harness.release();
    assert_eq!(
        refused.borrow().clone(),
        Some(Said::refused(
            "CH 5's reverb send was not changed: the engine is not running."
        ))
    );
    assert!(desk.harness.console.state().sends_pending.is_empty());
    assert_eq!(channel_sends(&desk.harness.console.state(), 5).reverb, 100);
}
