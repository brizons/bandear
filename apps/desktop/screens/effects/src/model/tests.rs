//! The effect lane over the simulated desk (`effects.test.js`: the console shows an effect at once
//! and settles it on the desk's answer; `effects-screen.js`: the change merged into what the panel
//! shows, the outcome and the tempo field).

use super::*;
use crate::effects::effects_view;
use bandear_desktop_console::command::{Answer, Owner};
use bandear_desktop_console::console::Options;
use bandear_desktop_console::router::Section;
use bandear_desktop_console::testing::{FakeDesk, Harness};
use bandear_desktop_console::types::EffectSettings;

/// The desk's `set_effect`, as `commands/effects.rs` answers it: the effect set, both effects in
/// the answer, and its words.
fn scripted() -> FakeDesk {
    let mut desk = FakeDesk::running();
    desk.script("set_effect", |desk, command| {
        let Command::SetEffect { effect, settings } = command else {
            return Err("not an effect".to_owned());
        };
        let effects = &mut desk.report.status.effects;
        let message = match effect {
            Effect::Reverb => {
                effects.reverb = serde_json::from_value::<ReverbSettings>(settings.clone())
                    .map_err(|error| error.to_string())?
                    .clamped();
                let reverb = effects.reverb;
                format!(
                    "The reverb is set: size {}%, damping {}%, pre-delay {} ms, width {}%.",
                    reverb.size_percent.round(),
                    reverb.damping_percent.round(),
                    reverb.pre_delay_ms.round(),
                    reverb.width_percent.round()
                )
            }
            Effect::Delay => {
                effects.delay = serde_json::from_value::<DelaySettings>(settings.clone())
                    .map_err(|error| error.to_string())?
                    .clamped();
                format!(
                    "The delay is set: {} ms, feedback {}%{}.",
                    effects.delay.time_ms_in_force().round(),
                    effects.delay.feedback_percent.round(),
                    if effects.delay.ping_pong {
                        ", ping-pong"
                    } else {
                        ""
                    }
                )
            }
        };
        Ok(Answer::EffectSet(EffectSetDto {
            outcome: "set",
            effect: *effect,
            effects: *effects,
            saved: true,
            save_error: None,
            message,
        }))
    });
    desk
}

fn rig() -> (Harness, Model) {
    let harness = Harness::connected(scripted(), Options::default());
    let model = Model::new(harness.console.for_owner(Owner::Screen(Section::Effects)));
    (harness, model)
}

/// Starts `flow`; its outcome lands in the cell once it ends.
fn start<T: 'static>(
    harness: &mut Harness,
    flow: impl Future<Output = T> + 'static,
) -> Rc<RefCell<Option<T>>> {
    let kept = Rc::new(RefCell::new(None));
    let into = Rc::clone(&kept);
    harness.console.spawn(async move {
        *into.borrow_mut() = Some(flow.await);
    });
    harness.settle();
    kept
}

fn sent(harness: &Harness) -> Vec<(Effect, serde_json::Value)> {
    harness
        .calls_to("set_effect")
        .into_iter()
        .filter_map(|command| match command {
            Command::SetEffect { effect, settings } => Some((*effect, settings.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn the_console_shows_an_effect_at_once_and_settles_it_on_the_desks_answer() {
    let (mut harness, model) = rig();
    harness.hold("set_effect");

    let first = start(
        &mut harness,
        model.edit_effect(EffectChange::Delay(DelaySettings {
            time_ms: 200.0,
            ..DelaySettings::default()
        })),
    );
    let second = start(
        &mut harness,
        model.edit_effect(EffectChange::Delay(DelaySettings {
            tempo_bpm: 100.0,
            note: DelayNote::Quarter,
            ..DelaySettings::default()
        })),
    );
    assert_eq!(
        effects_view(&harness.console.state()).delay.time_in_force,
        "600 ms (1/4 at 100 bpm)",
        "shown at once"
    );
    assert_eq!(sent(&harness).len(), 1, "one in flight per effect");
    harness.release();
    harness.release();

    assert!(first.borrow().is_some(), "the first change was answered");
    let said = second.borrow().clone().expect("answered");
    assert_eq!(
        said,
        Said {
            ok: true,
            message: "The delay is set: 600 ms, feedback 35%.".to_owned(),
        }
    );
    assert!(harness.console.state().effects_pending.delay.is_none());
    assert_eq!(
        effects_view(&harness.console.state())
            .delay
            .settings
            .tempo_bpm,
        100.0,
        "the latest change stays"
    );
    assert_eq!(sent(&harness).len(), 2);
}

#[test]
fn a_slider_dragged_sends_the_first_value_then_only_the_latest() {
    let (mut harness, model) = rig();
    harness.hold("set_effect");
    for size in [10.0, 20.0, 30.0, 40.0] {
        start(
            &mut harness,
            model.edit_reverb(move |reverb| reverb.size_percent = size),
        );
    }
    harness.release();
    harness.release();
    let sizes: Vec<_> = sent(&harness)
        .into_iter()
        .map(|(effect, settings)| (effect, settings["sizePercent"].as_f64()))
        .collect();
    assert_eq!(
        sizes,
        [(Effect::Reverb, Some(10.0)), (Effect::Reverb, Some(40.0))]
    );
    assert_eq!(
        model.outcome().map(|said| said.message),
        Some("The reverb is set: size 40%, damping 40%, pre-delay 10 ms, width 100%.".to_owned()),
        "every settled change is said"
    );
}

#[test]
fn a_change_merges_into_what_the_panel_shows_and_sends_the_whole_effect_held_in_range() {
    let (mut harness, model) = rig();
    harness.run(model.edit_delay(|delay| delay.feedback_percent = 60.0));
    harness.run(model.edit_delay(|delay| delay.ping_pong = true));
    harness.run(model.edit_delay(|delay| delay.high_cut_hz = 99_000.0));

    let (_, last) = sent(&harness).pop().expect("sent");
    assert_eq!(
        last["feedbackPercent"].as_f64(),
        Some(60.0),
        "the earlier change kept"
    );
    assert_eq!(last["pingPong"].as_bool(), Some(true));
    assert_eq!(last["highCutHz"].as_f64(), Some(20_000.0), "held in range");
    assert_eq!(last["timeMs"].as_f64(), Some(375.0), "the whole effect");
    assert_eq!(
        model.outcome().map(|said| said.message),
        Some("The delay is set: 375 ms, feedback 60%, ping-pong.".to_owned())
    );
}

#[test]
fn a_tempo_is_set_with_its_note_and_a_bad_one_is_said_and_never_sent() {
    let (mut harness, model) = rig();
    assert!(model.set_tempo("fast", DelayNote::Eighth).is_none());
    assert_eq!(
        model.outcome(),
        Some(Said {
            ok: false,
            message: "A tempo is 30 to 300 bpm, or empty for none.".to_owned(),
        })
    );
    assert!(sent(&harness).is_empty(), "nothing sent");

    let flow = model
        .set_tempo(" 120 ", DelayNote::Eighth)
        .expect("a tempo");
    harness.run(flow);
    let (_, delay) = sent(&harness).pop().expect("sent");
    assert_eq!(delay["tempoBpm"].as_f64(), Some(120.0));
    assert_eq!(delay["note"].as_str(), Some("eighth"));
    assert_eq!(
        effects_view(&harness.console.state()).delay.time_in_force,
        "250 ms (1/8 at 120 bpm)"
    );

    let flow = model.set_tempo("", DelayNote::Eighth).expect("no tempo");
    harness.run(flow);
    assert!(
        !effects_view(&harness.console.state()).delay.synced,
        "an empty field clears it"
    );
}

#[test]
fn a_change_the_desk_refuses_falls_back_and_says_why() {
    let (mut harness, model) = rig();
    harness
        .desk
        .fail_next("set_effect", "the engine is not running", 1);
    let said = harness
        .run(model.edit_reverb(|reverb| reverb.width_percent = 20.0))
        .expect("answered");
    assert_eq!(
        said,
        Said {
            ok: false,
            message: "The reverb was not set: the engine is not running.".to_owned(),
        }
    );
    assert_eq!(model.outcome(), Some(said));
    assert!(harness.console.state().effects_pending.reverb.is_none());
    assert_eq!(
        effects_view(&harness.console.state())
            .reverb
            .settings
            .width_percent,
        100.0,
        "back to what the desk keeps"
    );
}

#[test]
fn an_older_answer_for_one_effect_never_undoes_the_others_newer_change() {
    let (mut harness, model) = rig();
    harness.hold("set_effect");
    start(
        &mut harness,
        model.edit_reverb(|reverb| reverb.size_percent = 80.0),
    );
    start(
        &mut harness,
        model.edit_delay(|delay| delay.feedback_percent = 60.0),
    );
    assert_eq!(sent(&harness).len(), 2, "each effect has its own lane");
    harness.release();

    let view = effects_view(&harness.console.state());
    assert_eq!(view.reverb.settings.size_percent, 80.0);
    assert_eq!(view.delay.settings.feedback_percent, 60.0);
    assert_eq!(
        harness
            .console
            .state()
            .engine
            .status
            .as_ref()
            .map(|status| status.effects),
        Some(EffectSettings {
            reverb: ReverbSettings {
                size_percent: 80.0,
                ..ReverbSettings::default()
            },
            delay: DelaySettings {
                feedback_percent: 60.0,
                ..DelaySettings::default()
            },
        })
    );
}

#[test]
fn the_model_sends_its_commands_for_its_screen() {
    let (_, model) = rig();
    assert_eq!(model.console().owner(), Owner::Screen(Section::Effects));
}

#[test]
fn a_console_connected_to_a_running_engine_lets_every_control_be_used() {
    let harness = Harness::connected(
        FakeDesk::running(),
        Options {
            auto_start: false,
            ..Options::default()
        },
    );
    let state = harness.console.state();
    assert!(effects_view(&state).running);
}
