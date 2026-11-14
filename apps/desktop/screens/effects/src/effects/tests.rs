//! `effects.test.js`, the parts of this screen: the settings held in range, the delay's time at a
//! tempo, the tempo field, the panel's view of what is on its way over what the desk keeps, and
//! the desk's answers in words. A channel's sends and a mix's returns are the Processing and Mix
//! screens'.

use super::*;
use bandear_desktop_console::state::{Action, EffectChange, Phase};
use bandear_desktop_console::testing::sample_status;
use bandear_desktop_console::types::{Effect, EffectSettings};

fn running_with(change: impl FnOnce(&mut EffectSettings)) -> ConsoleState {
    let mut report = sample_status();
    change(&mut report.status.effects);
    let mut state = ConsoleState::new();
    state.reduce(&Action::Status(Box::new(report)));
    state
}

#[test]
fn the_effects_settings_are_held_in_range_anything_unreadable_taking_the_default() {
    let defaults = effects_view(&ConsoleState::new());
    assert_eq!(defaults.reverb.settings, ReverbSettings::default());
    assert_eq!(defaults.delay.settings, DelaySettings::default());
    assert_eq!(
        (
            defaults.reverb.settings.size_percent,
            defaults.reverb.settings.damping_percent,
            defaults.reverb.settings.pre_delay_ms,
            defaults.reverb.settings.width_percent
        ),
        (50.0, 40.0, 10.0, 100.0)
    );
    let delay = DelaySettings::default();
    assert_eq!(
        (
            delay.time_ms,
            delay.tempo_bpm,
            delay.note,
            delay.feedback_percent,
            delay.high_cut_hz,
            delay.ping_pong
        ),
        (375.0, 0.0, DelayNote::Quarter, 35.0, 6000.0, false)
    );

    let state = running_with(|effects| {
        effects.reverb = ReverbSettings {
            size_percent: 400.0,
            damping_percent: f32::NAN,
            pre_delay_ms: -5.0,
            width_percent: 20.0,
        };
        effects.delay = DelaySettings {
            time_ms: 9000.0,
            tempo_bpm: 12.0,
            note: DelayNote::Quarter,
            feedback_percent: 99.0,
            high_cut_hz: f32::NAN,
            ping_pong: true,
        };
    });
    let view = effects_view(&state);
    assert_eq!(
        view.reverb.settings,
        ReverbSettings {
            size_percent: 100.0,
            damping_percent: 40.0,
            pre_delay_ms: 0.0,
            width_percent: 20.0,
        }
    );
    assert_eq!(
        view.delay.settings,
        DelaySettings {
            time_ms: 2000.0,
            tempo_bpm: 0.0,
            note: DelayNote::Quarter,
            feedback_percent: 90.0,
            high_cut_hz: 6000.0,
            ping_pong: true,
        }
    );
    let fast = running_with(|effects| effects.delay.tempo_bpm = 500.0);
    assert_eq!(effects_view(&fast).delay.settings.tempo_bpm, 300.0);
    assert_eq!(
        note_of("whole bar"),
        DelayNote::Quarter,
        "an unknown note is a quarter"
    );
    assert_eq!(note_of("dottedEighth"), DelayNote::DottedEighth);
}

#[test]
fn the_delay_follows_a_note_at_a_tempo_held_to_two_seconds() {
    let at = |tempo_bpm, note| {
        delay_time_ms(&DelaySettings {
            tempo_bpm,
            note,
            ..DelaySettings::default()
        })
    };
    assert_eq!(
        delay_time_ms(&DelaySettings {
            time_ms: 420.0,
            ..DelaySettings::default()
        }),
        420.0
    );
    assert_eq!(at(120.0, DelayNote::Quarter), 500.0);
    assert_eq!(at(120.0, DelayNote::DottedEighth), 375.0);
    assert_eq!(at(30.0, DelayNote::Half), 2000.0);
    assert!((at(90.0, DelayNote::EighthTriplet) - 222.22).abs() < 0.01);
}

#[test]
fn the_tempo_field_takes_a_tempo_in_range_or_nothing() {
    assert_eq!(read_tempo(""), Ok(0.0));
    assert_eq!(read_tempo(" 128,5 "), Ok(128.5));
    assert_eq!(read_tempo("128.46"), Ok(128.5), "to a tenth of a bpm");
    assert_eq!(read_tempo("300"), Ok(300.0));
    let refusal = Err("A tempo is 30 to 300 bpm, or empty for none.".to_owned());
    assert_eq!(read_tempo("12"), refusal);
    assert_eq!(read_tempo("fast"), refusal);
    assert_eq!(read_tempo("301"), refusal);
    assert_eq!(format_tempo(128.5), "128.5");
    assert_eq!(format_tempo(120.0), "120");
}

#[test]
fn times_and_frequencies_read_as_the_panel_writes_them() {
    assert_eq!(format_time(375.0), "375 ms");
    assert_eq!(format_time(1250.0), "1.25 s");
    assert_eq!(format_time(2000.0), "2 s");
    assert_eq!(format_time(10.4), "10 ms");
    assert_eq!(format_khz(6000.0), "6 kHz");
    assert_eq!(format_khz(12_500.0), "12.5 kHz");
    let labels: Vec<_> = DELAY_NOTES.iter().map(|choice| choice.label).collect();
    assert_eq!(
        labels,
        [
            "1/2",
            "1/4",
            "Dotted 1/8",
            "1/4 triplet",
            "1/8",
            "1/8 triplet",
            "1/16"
        ]
    );
    assert_eq!(note_choice(DelayNote::Eighth).label, "1/8");
}

#[test]
fn the_panel_shows_what_is_on_its_way_over_what_the_desk_keeps() {
    let mut state = running_with(|effects| {
        effects.reverb.size_percent = 70.0;
        effects.delay.tempo_bpm = 120.0;
        effects.delay.note = DelayNote::Eighth;
    });
    state.reduce(&Action::EffectEdit(EffectChange::Reverb(ReverbSettings {
        size_percent: 90.0,
        ..ReverbSettings::default()
    })));

    let view = effects_view(&state);
    assert!(view.running);
    assert_eq!(view.reverb.size, "90%", "the change on its way");
    assert_eq!(view.reverb.damping, "40%");
    assert_eq!(view.reverb.pre_delay, "10 ms");
    assert_eq!(view.reverb.width, "100%");
    assert_eq!(view.delay.time_in_force, "250 ms (1/8 at 120 bpm)");
    assert_eq!(
        view.delay.time, "375 ms",
        "the slider's own time while synced"
    );
    assert!(view.delay.synced);
    assert_eq!(view.delay.feedback, "35%");
    assert_eq!(view.delay.high_cut, "6 kHz");
    assert_eq!(view.delay.note_label, "1/8");

    let unsynced = effects_view(&running_with(|_| {}));
    assert!(!unsynced.delay.synced);
    assert_eq!(unsynced.delay.time_in_force, "375 ms");
}

#[test]
fn every_control_waits_for_a_running_engine() {
    assert!(!effects_view(&ConsoleState::new()).running, "no engine");
    let mut stopping = running_with(|_| {});
    stopping.reduce(&Action::Engine {
        phase: Phase::Stopping,
        error: None,
    });
    assert!(!effects_view(&stopping).running, "only a running engine");
}

fn answer(outcome: &'static str, message: &str, save_error: Option<&str>) -> EffectSetDto {
    EffectSetDto {
        outcome,
        effect: Effect::Delay,
        effects: EffectSettings::default(),
        saved: save_error.is_none(),
        save_error: save_error.map(str::to_owned),
        message: message.to_owned(),
    }
}

#[test]
fn the_desks_answers_are_said_in_words() {
    assert_eq!(
        format_effect_set(&answer("set", "The delay is set: 375 ms.", None)),
        Said {
            ok: true,
            message: "The delay is set: 375 ms.".to_owned(),
        }
    );
    assert_eq!(
        format_effect_set(&answer(
            "set",
            "The delay is set: 375 ms.",
            Some("disk full")
        )),
        Said {
            ok: true,
            message: "The delay is set: 375 ms. It was not saved for the next start: disk full."
                .to_owned(),
        }
    );
    assert_eq!(
        format_effect_set(&answer("invalid", "Nope.", None)),
        Said {
            ok: false,
            message: "Nope.".to_owned(),
        }
    );
    assert_eq!(
        format_effect_set(&answer("invalid", "", None)).message,
        "The effect was not set."
    );
    assert_eq!(
        format_effect_set(&answer("set", "", None)).message,
        "The delay is set."
    );
}

#[test]
fn an_older_answer_about_one_effect_never_undoes_the_others_newer_one() {
    let mut state = running_with(|_| {});
    let reverb = ReverbSettings {
        size_percent: 80.0,
        ..ReverbSettings::default()
    };
    let delay = DelaySettings {
        feedback_percent: 60.0,
        ..DelaySettings::default()
    };
    // The delay's answer comes first; the reverb's, sent before it, carries the delay as it was.
    state.reduce(&Action::EffectSettled {
        effect: Effect::Delay,
        sent: None,
        effects: Some(EffectSettings {
            reverb: ReverbSettings::default(),
            delay,
        }),
    });
    state.reduce(&Action::EffectSettled {
        effect: Effect::Reverb,
        sent: None,
        effects: Some(EffectSettings {
            reverb,
            delay: DelaySettings::default(),
        }),
    });
    let view = effects_view(&state);
    assert_eq!(
        view.reverb.settings, reverb,
        "each effect as its own answer left it"
    );
    assert_eq!(view.delay.settings, delay);
}
