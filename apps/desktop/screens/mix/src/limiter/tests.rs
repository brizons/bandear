//! The pure parts of `limiter.test.js`, and the master strip's returns (inventory M21).

use bandear_desktop_console::state::{Action, LOCAL_MIX};
use bandear_desktop_console::types::dto::{FxReturnSetDto, LimiterSetDto};
use bandear_desktop_console::types::{Effect, EffectLevels, LimiterSettings};
use bandear_desktop_console::wire::{Meters, OutputLevels};

use super::*;
use crate::testing::{BASS, LEAD, running, with_status};

fn limiter(enabled: bool, ceiling_db: f32, release_ms: f32) -> LimiterSettings {
    LimiterSettings {
        enabled,
        ceiling_db,
        release_ms,
    }
}

#[test]
fn a_limiters_settings_are_held_in_range_anything_unreadable_taking_the_default() {
    assert_eq!(default_limiter(), limiter(false, 0.0, 50.0));
    assert_eq!(
        clamp_limiter(&limiter(true, -30.0, 5000.0)),
        limiter(true, -24.0, 1000.0)
    );
    assert_eq!(
        clamp_limiter(&limiter(true, 3.0, 0.0)),
        limiter(true, 0.0, 1.0)
    );
    assert_eq!(
        clamp_limiter(&limiter(false, f32::NAN, f32::INFINITY)),
        default_limiter()
    );
    assert_eq!(limiter(true, -3.0, 80.0), limiter(true, -3.0, 80.0));
    assert_ne!(limiter(true, -3.0, 80.0), limiter(false, -3.0, 80.0));
}

#[test]
fn a_ceiling_is_said_in_dbfs_with_a_true_minus() {
    assert_eq!(format_ceiling(0.0), "0 dBFS");
    assert_eq!(format_ceiling(-6.0), "\u{2212}6 dBFS");
    assert_eq!(format_ceiling(-6.5), "\u{2212}6.5 dBFS");
    assert_eq!(format_ceiling(-0.04), "0 dBFS");
}

#[test]
fn a_release_is_said_as_the_processing_screen_says_a_time() {
    assert_eq!(format_ms(0.5), "0.5 ms");
    assert_eq!(format_ms(150.0), "150 ms");
    assert_eq!(format_ms(1000.0), "1 s");
    assert_eq!(format_ms(1500.0), "1.5 s");
}

#[test]
fn the_strips_view_says_the_state_the_ceilings_that_win_and_the_reduction_only_while_on() {
    let on = limiter_view(&limiter(true, -3.0, 120.0), 4.25, Some(90));
    assert_eq!(on.state, "Limiter on");
    assert_eq!(on.ceiling, "\u{2212}3 dBFS");
    assert_eq!(on.release, "120 ms");
    assert_eq!(
        on.note,
        "Peaks are held at \u{2212}3 dBFS or the mix's ceiling (90%), whichever is lower. No added latency."
    );
    assert!((on.reduction.0 - 4.25 / LIMITER_REDUCTION_SCALE_DB).abs() < 1e-9);
    assert_eq!(on.reduction.1, "\u{2212}4.3 dB");
    assert_eq!(
        on.ceiling_position,
        limiter_position(LimiterField::Ceiling, -3.0)
    );
    assert_eq!(
        on.release_position,
        limiter_position(LimiterField::Release, 120.0)
    );

    let local = limiter_view(&limiter(true, 0.0, 50.0), 0.0, None);
    assert!(
        local.note.contains("the mix's full scale"),
        "{}",
        local.note
    );

    let off = limiter_view(&limiter(false, -3.0, 120.0), 6.0, Some(90));
    assert_eq!(off.state, "Limiter off");
    assert_eq!(off.note, "Off: the mix is only clipped at its ceiling.");
    assert_eq!(off.reduction, (0.0, "0.0 dB".to_owned()));
}

#[test]
fn a_controls_change_gives_the_whole_limiter_and_a_slider_always_moves_at_least_one_step() {
    let settings = limiter(false, -6.0, 50.0);
    let drag = |settings: &LimiterSettings, field, position: i32| {
        limiter_change(
            settings,
            LimiterControl::Slider(field, f64::from(position)),
            false,
        )
    };
    let key = |settings: &LimiterSettings, field, position: i32| {
        limiter_change(
            settings,
            LimiterControl::Slider(field, f64::from(position)),
            true,
        )
    };
    assert_eq!(
        limiter_change(&settings, LimiterControl::Enabled(true), false),
        limiter(true, -6.0, 50.0)
    );
    // A drag lands on the value at its position, rounded as the strip shows it.
    assert_eq!(
        drag(&settings, LimiterField::Ceiling, 500),
        limiter(false, -12.0, 50.0)
    );
    assert_eq!(
        drag(&settings, LimiterField::Release, 1000),
        limiter(false, -6.0, 1000.0)
    );
    assert_eq!(
        drag(&settings, LimiterField::Release, 0),
        limiter(false, -6.0, 1.0)
    );
    // An arrow moves the position by one of 1,000 steps: less than the rounding, so it moves the
    // value by one unit (0.5 dB, 1 ms) its way instead of staying put.
    let ceiling_at = limiter_position(LimiterField::Ceiling, -6.0);
    assert_eq!(
        key(&settings, LimiterField::Ceiling, ceiling_at + 1).ceiling_db,
        -5.5
    );
    assert_eq!(
        key(&settings, LimiterField::Ceiling, ceiling_at - 1).ceiling_db,
        -6.5
    );
    let short = limiter(false, -6.0, 1.0);
    let release_at = limiter_position(LimiterField::Release, 1.0);
    assert_eq!(
        key(&short, LimiterField::Release, release_at + 1).release_ms,
        2.0
    );
    assert_eq!(
        key(&short, LimiterField::Release, release_at - 1).release_ms,
        1.0,
        "held at the end"
    );
    assert_eq!(
        key(&limiter(false, 0.0, 50.0), LimiterField::Ceiling, 1000).ceiling_db,
        0.0
    );
    assert_eq!(
        drag(&settings, LimiterField::Ceiling, ceiling_at),
        settings,
        "the same position changes nothing"
    );
}

#[test]
fn a_slow_drag_goes_through_the_values_in_order_and_ends_on_the_one_it_was_let_go_at() {
    let start = limiter_position(LimiterField::Ceiling, -12.0);
    let mut shown = limiter(true, -12.0, 50.0);
    let mut seen = Vec::new();
    for position in start..=start + 50 {
        shown = limiter_change(
            &shown,
            LimiterControl::Slider(LimiterField::Ceiling, f64::from(position)),
            false,
        );
        seen.push(shown.ceiling_db);
    }
    assert!(
        seen.windows(2).all(|pair| pair[1] >= pair[0]),
        "flipped back: {seen:?}"
    );
    assert_eq!(
        shown.ceiling_db,
        limiter_change(
            &default_limiter(),
            LimiterControl::Slider(LimiterField::Ceiling, f64::from(start + 50)),
            false
        )
        .ceiling_db
    );
    let from = limiter_position(LimiterField::Release, 20.0);
    let mut release = limiter(true, 0.0, 20.0);
    let mut releases = Vec::new();
    for position in from..=from + 30 {
        release = limiter_change(
            &release,
            LimiterControl::Slider(LimiterField::Release, f64::from(position)),
            false,
        );
        releases.push(release.release_ms);
    }
    assert!(
        releases.windows(2).all(|pair| pair[1] >= pair[0]),
        "flipped back: {releases:?}"
    );
}

#[test]
fn the_gain_reduction_comes_from_the_meters_of_the_open_mix() {
    let levels = |reduction: f32| OutputLevels {
        left: 40,
        right: 41,
        limiter_reduction_db: reduction,
    };
    let mut meters = Meters {
        local: levels(1.5),
        ..Meters::default()
    };
    meters.musicians.insert(LEAD.to_owned(), levels(3.2));
    meters.musicians.insert(BASS.to_owned(), levels(0.0));
    assert!((limiter_reduction(Some(&meters), LEAD) - 3.2).abs() < 1e-6);
    assert_eq!(
        limiter_reduction(Some(&meters), BASS),
        0.0,
        "a mix that reports none"
    );
    assert_eq!(limiter_reduction(Some(&meters), LOCAL_MIX), 1.5);
    assert_eq!(limiter_reduction(Some(&meters), "10.0.0.9:50000"), 0.0);
    meters.local.limiter_reduction_db = -2.0;
    assert_eq!(limiter_reduction(Some(&meters), LOCAL_MIX), 0.0);
    assert_eq!(limiter_reduction(None, LEAD), 0.0);
}

fn limiter_set(settings: LimiterSettings, save_error: Option<&str>) -> LimiterSetDto {
    LimiterSetDto {
        musician_key: Some("client:5f0c2a8e-1b7d-4c3a-9e6f-2d8b4a7c1e90".to_owned()),
        outcome: "set",
        settings,
        saved: save_error.is_none(),
        save_error: save_error.map(str::to_owned),
        message: "This musician's limiter is on.".to_owned(),
    }
}

#[test]
fn the_outcome_is_said_with_the_settings_in_force_and_a_save_that_failed() {
    assert_eq!(
        format_limiter_set(
            Some(&limiter_set(limiter(true, -1.5, 80.0), None)),
            "Lead vocal"
        ),
        Said::done(
            "Lead vocal's limiter is on: peaks are held at \u{2212}1.5 dBFS, released over 80 ms."
        )
    );
    assert_eq!(
        format_limiter_set(
            Some(&limiter_set(default_limiter(), Some("disk full"))),
            "The local mix"
        ),
        Said::done(
            "The local mix's limiter is off. It was not saved for the next start: disk full."
        )
    );
    let other = LimiterSetDto {
        outcome: "other",
        message: "No.".to_owned(),
        ..limiter_set(default_limiter(), None)
    };
    assert_eq!(
        format_limiter_set(Some(&other), "Bass"),
        Said::refused("No.")
    );
    assert_eq!(
        format_limiter_set(None, "Bass"),
        Said::refused("Bass's limiter was not set.")
    );
}

#[test]
fn only_a_slider_key_held_on_a_limiter_slider_makes_its_change_a_key_step() {
    let mut keys = SliderKeys::default();
    assert!(!keys.keyed(LimiterField::Ceiling), "nothing pressed");
    keys.key_down(LimiterField::Ceiling, "up");
    assert!(keys.keyed(LimiterField::Ceiling));
    assert!(
        !keys.keyed(LimiterField::Release),
        "another slider's input is not that key's"
    );
    // The arrow's one step of 1,000 rounds back to -12 dBFS: as a key step it moves 0.5 dB up.
    let settings = limiter(true, -12.0, 50.0);
    let from = f64::from(limiter_position(LimiterField::Ceiling, -12.0));
    let stepped = limiter_change(
        &settings,
        LimiterControl::Slider(LimiterField::Ceiling, from + 1.0),
        keys.keyed(LimiterField::Ceiling),
    );
    assert_eq!(stepped.ceiling_db, -11.5);
    keys.key_up();
    assert!(!keys.keyed(LimiterField::Ceiling), "released");
    let dragged = limiter_change(
        &settings,
        LimiterControl::Slider(LimiterField::Ceiling, from + 1.0),
        keys.keyed(LimiterField::Ceiling),
    );
    assert_eq!(
        dragged.ceiling_db, -12.0,
        "the same position from the pointer stays put"
    );

    for key in ["pagedown", "home", "end", "left"] {
        keys.key_down(LimiterField::Release, key);
        assert!(keys.keyed(LimiterField::Release), "{key}");
    }
    keys.key_down(LimiterField::Release, "tab");
    assert!(!keys.keyed(LimiterField::Release), "Tab moves no slider");
    keys.key_down(LimiterField::Release, "down");
    keys.pointer_down();
    assert!(!keys.keyed(LimiterField::Release), "a drag is never a key");
    let down = limiter_change(
        &settings,
        LimiterControl::Slider(
            LimiterField::Release,
            f64::from(limiter_position(LimiterField::Release, 50.0) - 1),
        ),
        true,
    );
    assert_eq!(down.release_ms, 49.0, "an arrow down moves one millisecond");
}

#[test]
fn the_master_strip_shows_the_mixs_limiter_the_change_on_its_way_first() {
    let state = running();
    let lead = master_limiter(&state, LEAD).expect("Lead vocal's limiter");
    assert!(!lead.pending);
    assert_eq!(lead.state, "Limiter off");
    assert_eq!(lead.settings.ceiling_db, 0.0);
    assert_eq!(
        master_limiter(&state, LOCAL_MIX).unwrap().note,
        "Off: the mix is only clipped at its ceiling."
    );
    assert_eq!(
        master_limiter(&state, "10.0.0.9:50000"),
        None,
        "no such mix"
    );

    let waiting = with_status(
        state,
        vec![Action::LimiterEdit {
            address: LEAD.to_owned(),
            settings: limiter(true, -3.0, 50.0),
        }],
    );
    let lead = master_limiter(&waiting, LEAD).unwrap();
    assert!(lead.pending);
    assert_eq!(
        lead.note,
        "Peaks are held at \u{2212}3 dBFS or the mix's ceiling (90%), whichever is lower. No added latency.",
        "Lead vocal's phone caps at 90%: the note names that ceiling"
    );
}

#[test]
fn a_mixs_returns_show_the_change_on_its_way_over_the_status_off_at_zero() {
    let state = running();
    let view = returns_view(&state, LEAD);
    assert!(!view.pending);
    let rows: Vec<_> = view
        .rows
        .iter()
        .map(|row| (row.name, row.label.as_str(), row.level, row.text.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            ("Reverb", "Reverb return", 0, "Off"),
            ("Delay", "Delay return", 0, "Off")
        ]
    );
    let waiting = with_status(
        state,
        vec![Action::ReturnEdit {
            address: LOCAL_MIX.to_owned(),
            effect: Effect::Delay,
            level: 40,
        }],
    );
    let local = returns_view(&waiting, LOCAL_MIX);
    assert!(local.pending);
    assert_eq!(local.rows[1].text, "40%");
    assert_eq!(local.rows[0].text, "Off");
    assert_eq!(format_level(100), "100%");
}

#[test]
fn a_returns_answer_is_said_in_words_and_a_refusal_in_the_desks() {
    let answer = FxReturnSetDto {
        musician_key: None,
        outcome: "set",
        effect: Effect::Reverb,
        returns: EffectLevels {
            reverb: 40,
            delay: 0,
        },
        saved: true,
        save_error: None,
        message: "set".to_owned(),
    };
    assert_eq!(
        format_return_set(Some(&answer), "Ana"),
        Said::done("Ana's mix returns the reverb at 40%.")
    );
    let none = FxReturnSetDto {
        effect: Effect::Delay,
        save_error: Some("disk full".to_owned()),
        ..answer.clone()
    };
    assert_eq!(
        format_return_set(Some(&none), "The local mix"),
        Said::done(
            "The local mix returns no delay. It was not saved for the next start: disk full."
        )
    );
    assert_eq!(
        format_return_set(None, "Ana"),
        Said::refused("Ana's return was not set.")
    );
}
