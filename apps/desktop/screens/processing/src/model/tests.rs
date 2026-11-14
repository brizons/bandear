//! The editor over the console's simulated desk (`processing-editor.test.js`).

use std::cell::Cell;
use std::rc::Rc;

use bandear_desktop_console::command::{Command, Owner};
use bandear_desktop_console::router::Section;

use super::*;
use crate::desk::tests::{Desk, desk_with, spawned};
use crate::strip::default_strip;
use crate::testdesk::factory_presets;

struct Editor {
    desk: Desk,
    model: Model,
    changes: Rc<Cell<usize>>,
}

/// The editor shown on `channel`, over a desk with CH 3-4 linked when `link`.
fn editor(channel: Option<u8>, link: bool) -> Editor {
    let desk = desk_with(link, |_, _| {});
    let model = Model::new(
        desk.harness
            .console
            .for_owner(Owner::Screen(Section::Processing)),
    );
    let changes = Rc::new(Cell::new(0));
    let counted = Rc::clone(&changes);
    model.on_change(move || counted.set(counted.get() + 1));
    model.shown(channel);
    let mut editor = Editor {
        desk,
        model,
        changes,
    };
    editor.desk.harness.settle();
    editor
}

impl Editor {
    fn run<T: 'static>(&mut self, flow: impl std::future::Future<Output = T> + 'static) -> T {
        self.desk.run(flow)
    }
}

#[test]
fn the_editor_opens_the_channel_asked_for_a_pair_on_its_first_side_and_steps_between_strips() {
    let editor = editor(Some(4), true);
    let view = editor.model.view();
    assert!(view.known);
    assert_eq!(
        view.channel, 3,
        "CH 4 is the second side of the pair CH 3-4"
    );
    assert!(
        view.label().starts_with("CH 3\u{2013}4"),
        "{}",
        view.label()
    );
    assert_eq!(view.sides, [3, 4]);
    assert_eq!(editor.model.step(1), 5);
    assert_eq!(editor.model.step(-1), 3);
    assert_eq!(editor.model.step(-1), 2);
    assert_eq!(
        editor.model.open(99),
        2,
        "a channel the device lacks keeps the one open"
    );
    let view = editor.model.view();
    assert_eq!(view.settings, default_strip());
    assert!(!view.active);
}

#[test]
fn a_sliders_change_goes_to_the_desk_as_the_whole_strip_held_in_range_and_is_shown_at_once() {
    let mut editor = editor(Some(1), false);
    editor.desk.harness.hold("set_channel_processing");
    let editing = spawned(
        &mut editor.desk.harness,
        editor.model.edit_enabled(Part::Compressor, true),
    );
    assert!(
        editor.model.view().settings.compressor.enabled,
        "shown before the answer"
    );
    assert!(editor.model.view().pending);
    editor.desk.harness.release();
    let outcome = editing.borrow().clone().expect("answered");
    assert!(outcome.ok, "{}", outcome.message);
    let before = editor.changes.get();
    let edit = spawned(
        &mut editor.desk.harness,
        editor
            .model
            .edit_number(Part::Compressor, Key::ThresholdDb, -500.0),
    );
    editor.desk.harness.release();
    assert!(edit.borrow().as_ref().is_some_and(|said| said.ok));
    let strip = editor.desk.strip(1);
    assert_eq!(
        strip.settings.compressor.threshold_db, -60.0,
        "held to the range"
    );
    assert!(strip.active);
    match editor.desk.sent("set_channel_processing").last() {
        Some(Command::SetChannelProcessing { channel, settings }) => {
            assert_eq!(*channel, 1);
            assert_eq!(settings.compressor.threshold_db, -60.0, "sent clamped");
        }
        other => panic!("no strip edit: {other:?}"),
    }
    assert_eq!(
        editor.changes.get(),
        before,
        "a sent edit is drawn from the state, never said"
    );
    assert_eq!(editor.model.view().outcome, None);
}

#[test]
fn a_b_keeps_the_strip_sends_a_neutral_one_and_puts_the_kept_one_back() {
    let mut editor = editor(Some(2), false);
    let edit = editor.model.edit_enabled(Part::Gate, true);
    assert!(editor.run(edit).ok);
    let working = editor.model.view().settings;
    assert!(editor.model.view().can_bypass);

    let bypass = editor.model.toggle_bypass();
    assert_eq!(
        editor.run(bypass),
        Said::ok("CH 2 is bypassed: you hear it with no processing.")
    );
    let view = editor.model.view();
    assert!(view.bypassed);
    assert_eq!(
        view.settings, working,
        "the kept strip is shown, to put back"
    );
    assert!(
        !editor.desk.strip(2).active,
        "the desk plays CH 2 untouched"
    );
    let refused = editor
        .model
        .edit_number(Part::Gate, Key::ThresholdDb, -10.0);
    assert_eq!(
        editor.run(refused),
        Said::refused("CH 2 is bypassed: turn the bypass off to change its processing.")
    );
    let reset = editor.model.reset();
    assert!(!editor.run(reset).ok, "nor a reset");

    let restore = editor.model.toggle_bypass();
    assert_eq!(
        editor.run(restore),
        Said::ok("CH 2's processing is back on.")
    );
    assert!(!editor.model.view().bypassed);
    assert_eq!(editor.desk.strip(2).settings, working);

    let bypass = editor.model.toggle_bypass();
    assert!(editor.run(bypass).ok);
    let leave = editor.model.leave();
    editor.run(leave);
    assert_eq!(
        editor.desk.strip(2).settings,
        working,
        "leaving the screen ends the bypass"
    );
    assert!(!editor.model.view().bypassed);
}

#[test]
fn a_strip_with_no_processing_cannot_be_bypassed() {
    let mut editor = editor(Some(2), false);
    assert!(!editor.model.view().can_bypass);
    let bypass = editor.model.toggle_bypass();
    assert_eq!(
        editor.run(bypass),
        Said::refused("CH 2 has no processing to bypass.")
    );
    assert!(editor.desk.sent("set_channel_processing").is_empty());
}

#[test]
fn a_restore_the_desk_refuses_keeps_the_strip_bypassed_and_kept_to_try_again() {
    let mut editor = editor(Some(2), false);
    let edit = editor.model.edit_enabled(Part::Gate, true);
    assert!(editor.run(edit).ok);
    let working = editor.model.view().settings;
    let bypass = editor.model.toggle_bypass();
    assert!(editor.run(bypass).ok);

    editor
        .desk
        .harness
        .desk
        .fail_next("set_channel_processing", "the desk is busy", 1);
    let refused = editor.model.toggle_bypass();
    assert!(!editor.run(refused).ok);
    let view = editor.model.view();
    assert!(view.bypassed, "still bypassed");
    assert_eq!(view.settings, working, "the kept strip is not lost");
    assert!(!editor.desk.strip(2).active);

    let restore = editor.model.toggle_bypass();
    assert!(editor.run(restore).ok);
    assert!(!editor.model.view().bypassed);
    assert_eq!(editor.desk.strip(2).settings, working);
}

#[test]
fn the_screen_comes_back_to_the_strip_it_had_open_last() {
    let editor = editor(Some(5), false);
    editor.model.shown(None);
    assert_eq!(
        editor.model.view().channel,
        5,
        "shown again from the sidebar: CH 5"
    );
    let fresh = Model::new(editor.desk.harness.console.clone());
    assert_eq!(
        fresh.view().channel,
        1,
        "a new window opens the first strip"
    );
}

#[test]
fn a_strip_changed_elsewhere_while_bypassed_ends_the_bypass_and_is_not_overwritten() {
    let mut editor = editor(Some(2), false);
    let edit = editor.model.edit_enabled(Part::Gate, true);
    assert!(editor.run(edit).ok);
    let bypass = editor.model.toggle_bypass();
    assert!(editor.run(bypass).ok);
    let elsewhere = editor.model.strips().apply_preset("Kick".to_owned(), 2);
    assert!(editor.run(elsewhere).ok);
    let view = editor.model.view();
    assert!(!view.bypassed);
    assert_eq!(view.settings, factory_presets()[1].1);
    let leave = editor.model.leave();
    editor.run(leave);
    assert_eq!(editor.desk.strip(2).settings, factory_presets()[1].1);
}

#[test]
fn the_curve_is_drawn_at_once_and_asked_of_the_desk_once_the_edits_pause() {
    let mut editor = editor(Some(1), false);
    editor.desk.harness.run_timers();
    let asked = editor.desk.sent("eq_response").len();
    assert_eq!(editor.model.view().curve.points.len(), 128);
    editor.model.request_curve();
    let edit = editor.model.edit_number(Part::Band(1), Key::GainDb, 6.0);
    assert!(editor.run(edit).ok);
    editor.model.request_curve();
    editor.model.request_curve();
    assert!(!editor.model.view().curve.from_desk);
    editor.desk.harness.run_timers();
    let sent = editor.desk.sent("eq_response");
    assert_eq!(sent.len(), asked + 1, "one ask once the edits paused");
    match sent.last() {
        Some(Command::EqResponse { settings, .. }) => assert_eq!(settings.bands[1].gain_db, 6.0),
        other => panic!("no curve ask: {other:?}"),
    }
    let view = editor.model.view();
    assert!(view.curve.from_desk);
    let peak = view
        .curve
        .points
        .iter()
        .map(|point| point.db)
        .fold(f64::MIN, f64::max);
    assert!(peak > 5.9 && peak < 6.1, "{peak}");
    let edit = editor.model.edit_number(Part::Band(1), Key::GainDb, -3.0);
    assert!(editor.run(edit).ok);
    assert!(
        !editor.model.view().curve.from_desk,
        "the desk's curve is for the old strip"
    );
}

#[test]
fn reset_copy_and_presets_say_what_they_did_a_built_in_preset_cannot_be_deleted() {
    let mut editor = editor(Some(1), false);
    let apply = editor.model.apply_preset("Vocal");
    let outcome = editor.run(apply);
    assert!(outcome.ok, "{}", outcome.message);
    assert_eq!(editor.model.view().settings, factory_presets()[0].1);
    assert_eq!(editor.model.view().outcome, Some(outcome.clone()));
    assert_eq!(editor.model.view().status_line(), outcome.message);

    let copy = editor.model.copy_to(vec![5, 6]);
    assert!(editor.run(copy).ok);
    assert_eq!(editor.desk.strip(6).settings, factory_presets()[0].1);
    let none = editor.model.copy_to(Vec::new());
    assert_eq!(
        editor.run(none),
        Said::refused("Pick the channels to copy CH 1's processing to.")
    );

    let blank = editor.model.save_preset("  ");
    assert_eq!(
        editor.run(blank),
        Said::refused("Name the preset to save it.")
    );
    let save = editor.model.save_preset("Mine");
    assert!(editor.run(save).ok);
    let mine = editor
        .model
        .view()
        .presets
        .into_iter()
        .find(|preset| preset.name == "Mine");
    assert_eq!(
        mine,
        Some(PresetOption {
            name: "Mine".to_owned(),
            label: "Mine".to_owned(),
            factory: false
        })
    );
    assert_eq!(editor.model.view().presets[0].label, "Vocal (built-in)");
    let refused = editor.model.delete_preset("Vocal");
    let refused = editor.run(refused);
    assert!(!refused.ok);
    assert_eq!(
        editor.model.view().failure,
        refused.message,
        "the alert says why"
    );
    let delete = editor.model.delete_preset("Mine");
    assert!(editor.run(delete).ok);

    let reset = editor.model.reset();
    assert!(editor.run(reset).ok);
    assert!(!editor.model.view().active);
}

#[test]
fn strips_not_read_yet_say_so_and_a_read_can_be_asked_for_again() {
    let desk = desk_with(false, |fake, _| {
        fake.fail_next("processing", "the desk is busy", 1)
    });
    let model = Model::new(desk.harness.console.clone());
    let mut desk = desk;
    model.shown(None);
    assert!(model.view().unread);
    assert_eq!(model.view().channel, 1);
    let read = model.read_again();
    assert_eq!(desk.run(read), None);
    assert!(!model.view().unread);

    let stop = desk.harness.console.stop();
    desk.run(stop);
    assert!(!model.view().known, "no engine, no inputs to open");
}

#[test]
fn the_sends_of_the_open_strip_show_at_once_and_a_refusal_is_the_alert_until_another_strip_opens() {
    let mut editor = editor(Some(5), false);
    let send = editor.model.edit_send(Effect::Reverb, 20.0);
    assert_eq!(
        editor.run(send),
        Said::ok("CH 5 is sending 20% to the reverb.")
    );
    assert_eq!(editor.model.view().sends.reverb, 20);
    assert_eq!(editor.model.view().failure, "");

    editor.desk.harness.desk.running = false;
    let refused = editor.model.edit_send(Effect::Delay, 30.0);
    let refused = editor.run(refused);
    assert_eq!(
        refused,
        Said::refused("CH 5's delay send was not changed: the engine is not running.")
    );
    assert_eq!(editor.model.view().failure, refused.message);
    editor.model.open(6);
    assert_eq!(
        editor.model.view().failure,
        "",
        "forgotten on another strip"
    );
}

#[test]
fn a_store_error_shows_on_the_strip_it_is_about_or_on_every_strip() {
    let mut editor = editor(Some(1), false);
    let outside = editor.model.strips().edit_processing(40, default_strip());
    editor.run(outside);
    assert_eq!(
        editor.model.view().error,
        None,
        "CH 40 is not the strip open"
    );
    editor.desk.harness.desk.running = false;
    let refused = editor.model.edit_number(Part::Gate, Key::RangeDb, 10.0);
    let refused = editor.run(refused);
    assert_eq!(
        editor.model.view().error.as_deref(),
        Some(refused.message.as_str())
    );
}
