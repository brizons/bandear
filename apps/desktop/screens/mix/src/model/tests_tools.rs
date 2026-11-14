//! The console's limiter, returns and Mix tools commands (`limiter.test.js`, `mix-tools.test.js`)
//! and the screen's wiring of their answers (`mix-tools-logic.test.js`).

use bandear_desktop_console::command::{Answer, Command};
use bandear_desktop_console::state::LOCAL_MIX;
use bandear_desktop_console::types::{Effect, LimiterSettings, SessionPreset};
use bandear_desktop_console::wire::Mirror;

use super::*;
use crate::limiter::{LimiterControl, master_limiter, returns_view};
use crate::mix_view::{channel_strips, group_strips};
use crate::testing::{BASS, DRUMS, KEYS, LEAD, Presets, connected, mix_desk, screen};
use crate::tools::ToolRequest;

fn limiter(enabled: bool, ceiling_db: f32, release_ms: f32) -> LimiterSettings {
    LimiterSettings {
        enabled,
        ceiling_db,
        release_ms,
    }
}

fn shown(model: &Model, address: &str) -> (bool, bool, f32, f32) {
    let view = master_limiter(&model.console().state(), address).expect("a mix");
    (
        view.pending,
        view.settings.enabled,
        view.settings.ceiling_db,
        view.settings.release_ms,
    )
}

#[test]
fn the_master_strip_shows_the_mixs_limiter_the_change_on_its_way_first() {
    let (mut harness, model) = connected();

    let on = harness.run(model.edit_mix_limiter(LEAD, limiter(true, -3.0, 50.0)));
    assert_eq!(
        on,
        Some(Said::done(
            "Lead vocal's limiter is on: peaks are held at \u{2212}3 dBFS, released over 50 ms."
        ))
    );
    let note = master_limiter(&model.console().state(), LEAD).unwrap().note;
    assert!(note.contains("the mix's ceiling (90%)"), "{note}");

    // Shown at once, before the desk answers; the latest change is the one that stays.
    harness.hold("set_musician_limiter");
    let first = model.edit_mix_limiter(BASS, limiter(true, -6.0, 50.0));
    let second = model.edit_mix_limiter(BASS, limiter(true, -9.0, 300.0));
    harness.settle();
    assert_eq!(shown(&model, BASS), (true, true, -9.0, 300.0));
    assert_eq!(
        harness.calls_to("set_musician_limiter").len(),
        2,
        "one in flight"
    );
    harness.release();
    harness.release();
    harness.run(first);
    assert_eq!(
        harness.run(second),
        Some(Said::done(
            "Bass's limiter is on: peaks are held at \u{2212}9 dBFS, released over 300 ms."
        ))
    );
    assert_eq!(
        shown(&model, BASS),
        (false, true, -9.0, 300.0),
        "the status read after it"
    );
    let keys: Vec<_> = harness
        .calls_to("set_musician_limiter")
        .into_iter()
        .map(|command| match command {
            Command::SetMusicianLimiter { musician_key, .. } => musician_key.as_str(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        keys,
        [
            "client:5f0c2a8e-1b7d-4c3a-9e6f-2d8b4a7c1e90",
            "client:a3d9e7b1-6c2f-4e8a-b5d0-7f1c9e3a2b64",
            "client:a3d9e7b1-6c2f-4e8a-b5d0-7f1c9e3a2b64",
        ],
        "each mix's limiter goes to its phone's key"
    );

    let local = harness.run(model.edit_mix_limiter(LOCAL_MIX, limiter(true, -40.0, 50.0)));
    assert_eq!(
        local,
        Some(Said::done(
            "The local mix's limiter is on: peaks are held at \u{2212}24 dBFS, released over 50 ms."
        ))
    );
    assert_eq!(shown(&model, LOCAL_MIX).2, -24.0);

    let gone = harness
        .run(model.edit_mix_limiter("10.0.0.9:50000", limiter(false, 0.0, 50.0)))
        .unwrap();
    assert!(!gone.ok);
    assert_eq!(
        gone.message,
        "10.0.0.9:50000's limiter was not set: 10.0.0.9:50000 is not here."
    );
}

#[test]
fn the_console_says_a_limiter_the_desk_refused_and_shows_the_statuss_again() {
    let (mut harness, model) = connected();
    harness
        .desk
        .fail_next("set_musician_limiter", "the engine is stopping", 1);
    let said = harness.run(model.edit_mix_limiter(BASS, limiter(true, -6.0, 50.0)));
    assert_eq!(
        said,
        Some(Said::refused(
            "Bass's limiter was not set: the engine is stopping."
        ))
    );
    let (pending, enabled, _, _) = shown(&model, BASS);
    assert!(!pending);
    assert!(!enabled, "the desk's own again");
}

#[test]
fn a_limiter_the_desk_set_stays_set_and_shown_when_the_status_cannot_be_read_after_it() {
    let (mut harness, model) = connected();
    harness
        .desk
        .fail_next("engine_status", "the window lost the desk", 1);
    let said = harness.run(model.edit_mix_limiter(BASS, limiter(true, -6.0, 80.0)));
    assert_eq!(
        said,
        Some(Said::done(
            "Bass's limiter is on: peaks are held at \u{2212}6 dBFS, released over 80 ms."
        ))
    );
    assert_eq!(
        shown(&model, BASS),
        (false, true, -6.0, 80.0),
        "the desk's answer, not the old status"
    );
    harness
        .desk
        .fail_next("engine_status", "the window lost the desk", 1);
    let local = harness.run(model.edit_mix_limiter(LOCAL_MIX, limiter(true, -3.0, 50.0)));
    assert!(local.unwrap().ok);
    assert_eq!(shown(&model, LOCAL_MIX).2, -3.0);
}

#[test]
fn a_limiter_change_is_settled_even_when_the_desks_answer_cannot_be_read() {
    let (mut harness, model) = connected();
    harness
        .desk
        .script("set_musician_limiter", |_, _| Ok(Answer::Done));
    let said = harness
        .run(model.edit_mix_limiter(BASS, limiter(true, -6.0, 80.0)))
        .unwrap();
    assert!(!said.ok);
    assert!(
        said.message.starts_with("Bass's limiter was not set: "),
        "{}",
        said.message
    );
    assert!(!shown(&model, BASS).0, "no change is left on its way");
}

#[test]
fn a_limiter_control_of_the_open_mix_changes_the_shown_limiter_and_only_a_refusal_is_said() {
    let (mut harness, model) = connected();
    model.select(BASS);
    model.limiter_edit(LimiterControl::Enabled(true));
    harness.settle();
    assert_eq!(shown(&model, BASS), (false, true, 0.0, 50.0));
    assert_eq!(
        model.page().outcome,
        None,
        "a limiter that was set says nothing"
    );

    // An arrow held on the ceiling moves it at least one step; the pointer at the same place does not.
    let from = f64::from(crate::limiter::limiter_position(
        crate::limiter::LimiterField::Ceiling,
        0.0,
    ));
    model.limiter_key_down(crate::limiter::LimiterField::Ceiling, "down");
    model.limiter_edit(LimiterControl::Slider(
        crate::limiter::LimiterField::Ceiling,
        from - 1.0,
    ));
    model.limiter_key_up();
    harness.settle();
    assert_eq!(shown(&model, BASS).2, -0.5);

    harness
        .desk
        .fail_next("set_musician_limiter", "the engine is stopping", 1);
    model.limiter_edit(LimiterControl::Enabled(false));
    harness.settle();
    assert_eq!(
        model.page().outcome,
        Some(Said::refused(
            "Bass's limiter was not set: the engine is stopping."
        ))
    );
}

#[test]
fn a_return_goes_out_one_at_a_time_per_mix_and_effect_and_its_answer_is_kept() {
    let (mut harness, model) = connected();
    harness.hold("set_musician_fx_return");
    let first = model.edit_mix_return(LEAD, Effect::Reverb, 10);
    drop(model.edit_mix_return(LEAD, Effect::Reverb, 20));
    let last = model.edit_mix_return(LEAD, Effect::Reverb, 30);
    let delay = model.edit_mix_return(LEAD, Effect::Delay, 5);
    harness.settle();
    assert_eq!(
        harness.calls_to("set_musician_fx_return").len(),
        2,
        "one in flight per mix and effect"
    );
    let view = returns_view(&model.console().state(), LEAD);
    assert!(view.pending);
    assert_eq!(view.rows[0].text, "30%");
    harness.release();
    harness.release();
    harness.run(first);
    harness.run(delay);
    assert_eq!(
        harness.run(last),
        Some(Said::done("Lead vocal's mix returns the reverb at 30%."))
    );
    let levels: Vec<_> = harness
        .calls_to("set_musician_fx_return")
        .into_iter()
        .filter_map(|command| match command {
            Command::SetMusicianFxReturn {
                effect: Effect::Reverb,
                level,
                ..
            } => Some(*level),
            _ => None,
        })
        .collect();
    assert_eq!(
        levels,
        [10, 30],
        "the latest wins, the one between never goes"
    );
    let view = returns_view(&model.console().state(), LEAD);
    assert!(!view.pending);
    assert_eq!(
        (view.rows[0].level, view.rows[1].level),
        (30, 5),
        "kept as answered"
    );
}

#[test]
fn a_refused_return_says_why_and_falls_back_and_the_local_mix_has_its_own() {
    let (mut harness, model) = connected();
    harness
        .desk
        .fail_next("set_musician_fx_return", "the engine is stopping", 1);
    assert_eq!(
        harness.run(model.edit_mix_return(BASS, Effect::Reverb, 40)),
        Some(Said::refused(
            "Bass's reverb return was not set: the engine is stopping."
        ))
    );
    let view = returns_view(&model.console().state(), BASS);
    assert!(!view.pending);
    assert_eq!(view.rows[0].level, 0);

    assert_eq!(
        harness.run(model.edit_mix_return("10.0.0.9:50000", Effect::Delay, 20)),
        Some(Said::refused(
            "10.0.0.9:50000's delay return was not set: 10.0.0.9:50000 is not here."
        ))
    );
    assert_eq!(
        harness.run(model.edit_mix_return(LOCAL_MIX, Effect::Delay, 25)),
        Some(Said::done("The local mix returns the delay at 25%."))
    );
    assert_eq!(
        returns_view(&model.console().state(), LOCAL_MIX).rows[1].level,
        25
    );

    model.select(BASS);
    harness
        .desk
        .fail_next("set_musician_fx_return", "the engine is stopping", 1);
    model.return_edit(Effect::Delay, 60);
    harness.settle();
    assert_eq!(
        model.page().outcome,
        Some(Said::refused(
            "Bass's delay return was not set: the engine is stopping."
        )),
        "a refused return of the open mix is said above the strips"
    );
}

#[test]
fn the_mix_tools_answer_in_words_and_a_followers_strips_say_whose_mix_they_follow() {
    let (mut harness, model) = connected();
    assert_eq!(
        model.console().state().presets.as_deref(),
        Some(&[][..]),
        "read when the console connects"
    );
    assert_eq!(
        harness.run(model.save_preset(LEAD, "Verse")),
        Some(Said::done("Saved Lead vocal's mix as \"Verse\"."))
    );
    let verse = model.console().state().presets.as_ref().unwrap()[0]
        .id
        .clone();
    assert_eq!(
        harness.run(model.apply_preset(&verse, vec![BASS.to_owned(), KEYS.to_owned()])),
        Some(Said::done("Applied \"Verse\" to Bass and Keys."))
    );
    assert_eq!(
        harness.run(model.rename_preset(&verse, "Chorus")),
        Some(Said::done("Renamed \"Verse\" to \"Chorus\"."))
    );
    assert_eq!(
        harness.run(model.copy_mix(LEAD, vec![DRUMS.to_owned()])),
        Some(Said::done(
            "Copied Lead vocal's mix to Drums: channels, pans, mutes, solos and group levels."
        ))
    );
    assert_eq!(
        harness.run(model.set_mirror(BASS, Some(LEAD))),
        Some(Said::done("Bass follows Lead vocal's mix now."))
    );
    {
        let state = model.console().state();
        let bass = state.musician_by_address(BASS).unwrap();
        assert_eq!(bass.mirror.as_ref().unwrap().name, "Lead vocal");
        assert_eq!(
            channel_strips(&state, BASS)[0].following.as_deref(),
            Some("Lead vocal")
        );
        assert_eq!(
            group_strips(&state, BASS)[0].following.as_deref(),
            Some("Lead vocal")
        );
        assert_eq!(channel_strips(&state, LEAD)[0].following, None);
    }
    let circle = harness.run(model.set_mirror(LEAD, Some(BASS))).unwrap();
    assert!(!circle.ok);
    assert!(
        circle.message.starts_with("The mirror was not changed: ")
            && circle.message.ends_with("circle."),
        "{}",
        circle.message
    );
    assert_eq!(
        harness.run(model.delete_preset(&verse)),
        Some(Said::done("Deleted the preset \"Chorus\"."))
    );
    assert_eq!(model.console().state().presets.as_deref(), Some(&[][..]));
    assert!(matches!(
        harness.calls_to("set_mirror")[0],
        Command::SetMirror { client_id: Some(phone), leader: Some(leader), .. }
            if phone == "a3d9e7b1-6c2f-4e8a-b5d0-7f1c9e3a2b64" && leader.name == "Lead vocal"
                && leader.client_id.as_deref() == Some("5f0c2a8e-1b7d-4c3a-9e6f-2d8b4a7c1e90")
    ));
}

#[test]
fn a_mirror_to_a_musician_not_listed_is_refused_before_it_reaches_the_desk() {
    let (mut harness, model) = connected();
    assert_eq!(
        harness.run(model.set_mirror(BASS, Some("10.0.0.9:50000"))),
        Some(Said::refused(
            "The mirror was not changed: no musician is listed at 10.0.0.9:50000."
        ))
    );
    assert!(harness.calls_to("set_mirror").is_empty());
}

#[test]
fn the_screens_wiring_hands_the_mirror_answers_to_the_tools_done_or_refused() {
    let (mut harness, model) = connected();
    let request = model.with_tools(|tools| tools.choose_mirror(KEYS)).unwrap();
    assert_eq!(model.tools().mirror_value(), KEYS, "on its way");
    let done = harness.run(model.run_tool(request)).flatten().unwrap();
    assert!(done.ok, "{}", done.message);
    assert_eq!(
        model.tools().mirror_value(),
        KEYS,
        "the new mirror at once, never the old one in between"
    );
    assert_eq!(model.page().outcome, Some(done));

    // Bass starts following Lead by another route while the tools still offer Bass to Lead.
    harness.desk.report.musicians[1].mirror = Some(Mirror {
        leader: format!("address:{LEAD}"),
        leader_address: Some(LEAD.to_owned()),
        name: "Lead".to_owned(),
    });
    let request = model.with_tools(|tools| tools.choose_mirror(BASS)).unwrap();
    assert_eq!(model.tools().mirror_value(), BASS, "on its way");
    let refused = harness.run(model.run_tool(request)).flatten().unwrap();
    assert!(!refused.ok);
    assert!(refused.message.contains("circle"), "{}", refused.message);
    assert_eq!(
        model.tools().mirror_value(),
        KEYS,
        "the refusal reached the tools: the desk's mirror shows"
    );
    assert_eq!(model.page().outcome, Some(refused));
}

#[test]
fn a_mirror_command_that_fails_or_finds_no_musician_open_still_frees_the_mirror_control() {
    let (mut harness, model) = connected();
    harness
        .desk
        .fail_next("set_mirror", "the bridge went away", 1);
    model.tool(model.with_tools(|tools| tools.choose_mirror(BASS)).unwrap());
    harness.settle();
    assert_eq!(
        model.tools().mirror_value(),
        "",
        "no choice stays on show unanswered"
    );

    let mut nobody = harness.desk.status();
    nobody.musicians.clear();
    harness.desk.report = nobody;
    harness.desk.emit_status();
    harness.emit();
    let calls = harness.calls_to("set_mirror").len();
    let request = model.with_tools(|tools| tools.choose_mirror(KEYS)).unwrap();
    assert_eq!(harness.run(model.run_tool(request)), Some(None));
    assert_eq!(
        harness.calls_to("set_mirror").len(),
        calls,
        "nothing is asked for without a musician open"
    );
    assert_eq!(model.tools().mirror_value(), "");
}

#[test]
fn a_done_save_empties_its_field_and_a_done_copy_unticks_its_musicians() {
    let (mut harness, model) = connected();
    let save = model.with_tools(|tools| tools.save("Verse")).unwrap();
    harness.run(model.run_tool(save));
    assert!(model.take_clears().save);
    assert_eq!(model.take_clears(), Default::default(), "once");

    model.with_tools(|tools| tools.tick(crate::tools::TickList::Copy, BASS, true));
    let copy = model.with_tools(|tools| tools.copy()).unwrap();
    harness.run(model.run_tool(copy));
    assert!(!model.tools().is_ticked(crate::tools::TickList::Copy, BASS));
}

#[test]
fn a_refused_preset_says_why_and_presets_that_could_not_be_read_are_read_again_on_asking() {
    let presets = Presets::default();
    presets.borrow_mut().push(SessionPreset {
        id: "p9".to_owned(),
        name: "Verse".to_owned(),
        mix: Default::default(),
    });
    let mut desk = mix_desk(presets);
    desk.fail_next("presets", "the session is busy", 1);
    let (mut harness, model) = screen(desk);
    assert_eq!(model.console().state().presets, None, "not known yet");
    assert_eq!(
        model.tools().data().unwrap().presets,
        None,
        "the tools know they are unread"
    );

    model.tool(ToolRequest::LoadPresets);
    harness.settle();
    let names: Vec<_> = model
        .console()
        .state()
        .presets
        .iter()
        .flatten()
        .map(|preset| preset.name.clone())
        .collect();
    assert_eq!(names, ["Verse"]);
    assert_eq!(
        harness.run(model.save_preset(LEAD, "verse")),
        Some(Said::refused(
            "The preset was not saved: a preset named \"verse\" already exists."
        ))
    );
}
