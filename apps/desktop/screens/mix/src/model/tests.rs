//! The Mix lanes of `console-app.test.js`, the console parts of `engineer.test.js` and
//! `local-mix.test.js`, `master-controls.test.js`, and the screen's page and life.

use std::cell::RefCell;
use std::rc::Rc;

use bandear_desktop_console::command::{Answer, Command};
use bandear_desktop_console::mix::{EditValue, MixField, edits};
use bandear_desktop_console::slider::{SliderEvent, SliderGesture};
use bandear_desktop_console::state::LOCAL_MIX;
use bandear_desktop_console::testing::{Harness, sample_status};
use bandear_desktop_console::values::{FADER, SliderKey};
use bandear_desktop_console::wire::{EngineEvent, MixChanged};

use super::*;
use crate::mix_view::{MixNotice, mix_messages};
use crate::testing::{BASS, LEAD, connected, dto, status_of};

/// The `set_mix` requests sent, in words (the desk's request types compare no other way).
fn sent_mixes(harness: &Harness, name: &str) -> Vec<String> {
    harness
        .calls_to(name)
        .into_iter()
        .map(|command| match command {
            Command::SetMix { mix, .. } | Command::SetLocalMix { mix } => format!("{mix:?}"),
            other => format!("{other:?}"),
        })
        .collect()
}

fn request(
    change: impl FnOnce(&mut bandear_desktop_console::types::dto::MixUpdateRequest),
) -> String {
    let mut request = bandear_desktop_console::types::dto::MixUpdateRequest::default();
    change(&mut request);
    format!("{request:?}")
}

fn gains(model: &Model, address: &str) -> Vec<u8> {
    model
        .console()
        .state()
        .view_mix(address)
        .unwrap()
        .channel_gains[..2]
        .to_vec()
}

#[test]
fn fader_moves_reach_set_mix_one_at_a_time_and_the_latest_value_always_goes_out() {
    let (mut harness, model) = connected();
    harness.hold("set_mix");

    model.edit(LEAD, edits([("channelGains.0", 10.into())]).unwrap());
    model.edit(LEAD, edits([("channelGains.0", 20.into())]).unwrap());
    model.edit(LEAD, edits([("channelGains.0", 30.into())]).unwrap());
    model.edit(LEAD, edits([("channelGains.1", 40.into())]).unwrap());
    harness.settle();
    assert_eq!(
        harness.calls_to("set_mix").len(),
        1,
        "one request in flight per musician"
    );

    harness.release();
    assert_eq!(
        sent_mixes(&harness, "set_mix"),
        [
            request(|request| request.channel_gains = Some(vec![10])),
            request(|request| request.channel_gains = Some(vec![30, 40])),
        ]
    );
    harness.release();
    let state = model.console().state();
    assert!(state.pending_edits(LEAD).is_empty());
    assert_eq!(state.view_mix(LEAD).unwrap().channel_gains[..2], [30, 40]);
    assert_eq!(
        state.musician_by_address(LEAD).unwrap().mix.channel_gains[..2],
        [30, 40]
    );
}

#[test]
fn when_the_answer_comes_before_its_event_the_fader_never_shows_an_older_value() {
    let (mut harness, model) = connected();
    let shown = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&shown);
    harness.console.subscribe(move |console, _| {
        let gain = console
            .state()
            .view_mix(LEAD)
            .map(|mix| mix.channel_gains[1]);
        seen.borrow_mut().extend(gain);
    });

    model.edit(LEAD, edits([("channelGains.1", 40.into())]).unwrap());
    harness.settle();

    let shown = shown.borrow();
    assert!(
        shown.len() >= 2,
        "the edit and the answer reached the state: {shown:?}"
    );
    assert!(shown.iter().all(|gain| *gain == 40), "shown: {shown:?}");
    let state = model.console().state();
    assert!(state.pending_edits(LEAD).is_empty());
    assert_eq!(
        state.musician_by_address(LEAD).unwrap().mix.channel_gains[1],
        40
    );
    assert_eq!(harness.calls_to("set_mix").len(), 1);
}

#[test]
fn a_phone_change_between_the_desks_request_and_its_answer_stays_on_screen() {
    let (mut harness, model) = connected();
    harness.hold("set_mix");
    model.edit(LEAD, edits([("channelGains.1", 40.into())]).unwrap());
    harness.settle();

    // The desk stored the request as revision 2; the phone's change came after it as revision 3,
    // and reaches the console before the request's answer.
    let mut answered = sample_status().musicians[0].mix.clone();
    answered.channel_gains[1] = 40;
    answered.revision = 2;
    let mut phone = answered.clone();
    phone.channel_gains[1] = 90;
    phone.revision = 3;
    harness.desk.report.musicians[0].mix = phone.clone();
    harness
        .desk
        .events
        .push_back(EngineEvent::MixChanged(Box::new(MixChanged {
            run: Some(1),
            address: Some(LEAD.to_owned()),
            id: None,
            origin: "phone".to_owned(),
            mix: phone,
        })));
    harness.emit();
    harness
        .desk
        .script("set_mix", move |_, _| Ok(Answer::Mix(dto(&answered))));
    harness.release();

    assert_eq!(gains(&model, LEAD)[1], 90);
    assert!(model.console().state().pending_edits(LEAD).is_empty());
}

#[test]
fn a_refused_change_says_so_and_the_strip_falls_back_to_the_engines_value() {
    let (mut harness, model) = connected();
    harness.desk.running = false;

    model.edit(BASS, edits([("volumePercent", 10.into())]).unwrap());
    harness.settle();

    let state = model.console().state();
    let error = state.mix_error.as_ref().expect("the refusal is kept");
    assert_eq!(
        (error.address.as_str(), error.message.as_str()),
        (BASS, "the engine is not running")
    );
    assert_eq!(state.view_mix(BASS).unwrap().volume_percent, 100);
}

fn engineer_calls(harness: &Harness) -> Vec<(Option<i32>, Option<Vec<bool>>, Option<String>)> {
    harness
        .calls_to("set_engineer_controls")
        .into_iter()
        .map(|command| match command {
            Command::SetEngineerControls {
                address,
                client_id,
                controls,
            } => {
                assert_eq!(address, LEAD);
                (
                    controls.limit_percent,
                    controls.locked_channels.clone(),
                    client_id.clone(),
                )
            }
            _ => unreachable!(),
        })
        .collect()
}

#[test]
fn the_engineers_edits_go_to_set_engineer_controls_one_at_a_time_never_to_set_mix() {
    let (mut harness, model) = connected();
    harness.hold("set_engineer_controls");

    model.edit(LEAD, edits([("engineerLimitPercent", 70.into())]).unwrap());
    model.edit(
        LEAD,
        edits([
            ("engineerLimitPercent", 60.into()),
            ("lockedChannels.1", true.into()),
        ])
        .unwrap(),
    );
    harness.settle();
    assert_eq!(engineer_calls(&harness).len(), 1, "one request in flight");
    harness.release();
    harness.release();

    let phone = Some("5f0c2a8e-1b7d-4c3a-9e6f-2d8b4a7c1e90".to_owned());
    assert_eq!(
        engineer_calls(&harness),
        [
            (Some(70), None, phone.clone()),
            (Some(60), Some(vec![false, true]), phone),
        ],
        "each carries the row's client id, so a desk with no engine keeps it for the phone"
    );
    assert!(harness.calls_to("set_mix").is_empty());
    let state = model.console().state();
    assert!(state.pending_edits(LEAD).is_empty());
    let mix = state.view_mix(LEAD).unwrap();
    assert_eq!(mix.engineer_limit_percent, 60);
    assert!(mix.locked_channels[1]);
}

#[test]
fn a_fader_move_and_a_lock_at_once_go_out_on_their_own_commands() {
    let (mut harness, model) = connected();

    model.edit(
        LEAD,
        edits([
            ("channelGains.0", 30.into()),
            ("lockedChannels.0", true.into()),
        ])
        .unwrap(),
    );
    harness.settle();

    assert_eq!(
        sent_mixes(&harness, "set_mix"),
        [request(|request| request.channel_gains = Some(vec![30]))]
    );
    assert_eq!(engineer_calls(&harness)[0].1, Some(vec![true]));
    assert!(model.console().state().pending_edits(LEAD).is_empty());
}

#[test]
fn a_refused_change_of_the_engineers_controls_is_said_above_the_mix_and_the_edit_goes() {
    let (mut harness, model) = connected();
    harness
        .desk
        .fail_next("set_engineer_controls", "the engine is stopping", 1);

    model.edit(LEAD, edits([("engineerLimitPercent", 20.into())]).unwrap());
    harness.settle();

    let state = model.console().state();
    assert_eq!(
        mix_messages(&state).error.as_deref(),
        Some("A change to Lead vocal's mix was not applied: the engine is stopping.")
    );
    assert!(state.pending_edits(LEAD).is_empty());
}

#[test]
fn the_console_sends_local_mix_edits_with_set_local_mix_one_at_a_time_and_takes_the_answer() {
    let (mut harness, model) = connected();
    harness.hold("set_local_mix");

    model.select(LOCAL_MIX);
    model.edit(LOCAL_MIX, edits([("channelGains.1", 30.into())]).unwrap());
    model.edit(LOCAL_MIX, edits([("channelGains.1", 35.into())]).unwrap());
    model.edit(LOCAL_MIX, edits([("volumePercent", 70.into())]).unwrap());
    harness.settle();

    assert_eq!(
        sent_mixes(&harness, "set_local_mix"),
        [request(
            |request| request.channel_gains = Some(vec![100, 30])
        )],
        "one request in flight"
    );
    assert_eq!(gains(&model, LOCAL_MIX)[1], 35);
    harness.release();
    harness.release();

    assert_eq!(
        sent_mixes(&harness, "set_local_mix")[1],
        request(|request| {
            request.volume_percent = Some(70);
            request.channel_gains = Some(vec![100, 35]);
        })
    );
    let state = model.console().state();
    assert!(state.pending_edits(LOCAL_MIX).is_empty());
    assert_eq!(state.local_mix.as_ref().unwrap().volume_percent, 70);
    assert!(
        harness.calls_to("set_mix").is_empty(),
        "never a musician's command"
    );
}

#[test]
fn the_local_mixs_cap_limit_and_locks_never_go_out() {
    let (mut harness, model) = connected();
    model.edit(
        LOCAL_MIX,
        edits([
            ("maxLevelPercent", 50.into()),
            ("engineerLimitPercent", 40.into()),
            ("lockedChannels.0", true.into()),
        ])
        .unwrap(),
    );
    harness.settle();
    assert!(harness.calls_to("set_local_mix").is_empty());
    assert!(harness.calls_to("set_engineer_controls").is_empty());
    assert!(model.console().state().pending_edits(LOCAL_MIX).is_empty());
}

#[test]
fn while_a_saved_session_loads_mix_edits_wait_and_go_out_once_a_refused_load_lets_them() {
    let (mut harness, model) = connected();
    model.console().hold_sends();

    model.edit(LEAD, edits([("channelGains.0", 30.into())]).unwrap());
    model.edit(LEAD, edits([("lockedChannels.0", true.into())]).unwrap());
    harness.settle();
    assert!(
        harness.calls_to("set_mix").is_empty(),
        "held while it loads"
    );
    assert!(harness.calls_to("set_engineer_controls").is_empty());
    assert_eq!(model.console().state().pending_edits(LEAD).len(), 2);

    model.console().release_sends();
    harness.settle();
    assert_eq!(
        sent_mixes(&harness, "set_mix"),
        [request(|request| request.channel_gains = Some(vec![30]))]
    );
    assert_eq!(engineer_calls(&harness)[0].1, Some(vec![true]));
    assert!(model.console().state().pending_edits(LEAD).is_empty());
}

#[test]
fn a_channel_group_or_master_move_edits_the_open_mix() {
    let (mut harness, model) = connected();
    model.select(BASS);
    model.edit_channel(&[1, 2], true, MixField::Pans, 70.into());
    model.edit_group(1, 55);
    model.edit_master(MixField::Muted, true.into());
    let pending = model.console().state().pending_edits(BASS);
    assert_eq!(
        pending,
        edits([
            ("pans.1", 40.into()),
            ("pans.2", 100.into()),
            ("groupLevels.1", 55.into()),
            ("muted", true.into()),
        ])
        .unwrap()
    );
    harness.settle();
    assert!(model.console().state().pending_edits(BASS).is_empty());
}

// --- The master strip's controls (`master-controls.test.js`) -----------------------------------

/// The edits `events` of a gesture make on `control`.
fn edits_of(control: MasterControl, events: Vec<SliderEvent>) -> Vec<(MixField, EditValue)> {
    events
        .into_iter()
        .filter_map(|event| master_edit(control, event))
        .collect()
}

#[test]
fn a_finished_gesture_on_the_limit_sends_one_edit_of_the_engineers_limit_never_one_per_move() {
    let mut limit = SliderGesture::new(FADER, 100.0);
    let mut events = limit.press(1.0, false);
    events.extend(limit.move_to(0.8));
    events.extend(limit.move_to(0.6));
    assert!(
        edits_of(MasterControl::Limit, events).is_empty(),
        "nothing goes out while the limit moves"
    );
    assert_eq!(
        edits_of(MasterControl::Limit, limit.release()),
        [(MixField::EngineerLimitPercent, EditValue::Level(60))]
    );
}

#[test]
fn a_status_in_the_middle_of_a_drag_of_the_limit_does_not_move_its_thumb() {
    let mut limit = SliderGesture::new(FADER, 100.0);
    limit.press(1.0, false);
    limit.move_to(0.6);
    // A status arrives mid-drag with the limit the engine still has.
    limit.set(100.0);
    assert_eq!(limit.value(), 60, "the pointer wins while it drags");
    assert_eq!(
        edits_of(MasterControl::Limit, limit.release()),
        [(MixField::EngineerLimitPercent, EditValue::Level(60))],
        "the dragged value is what goes out"
    );
}

#[test]
fn a_key_press_on_the_limit_is_a_finished_gesture() {
    let mut limit = SliderGesture::new(FADER, 100.0);
    assert_eq!(
        edits_of(MasterControl::Limit, limit.key(SliderKey::PageDown)),
        [(MixField::EngineerLimitPercent, EditValue::Level(90))]
    );
}

#[test]
fn the_level_and_the_phones_cap_go_out_as_they_move() {
    let mut level = SliderGesture::new(FADER, 85.0);
    let mut cap = SliderGesture::new(FADER, 90.0);
    let limit = SliderGesture::new(FADER, 100.0);
    let mut sent = edits_of(MasterControl::Level, level.press(0.5, false));
    sent.extend(edits_of(MasterControl::Cap, cap.press(0.3, false)));
    assert_eq!(
        sent,
        [
            (MixField::VolumePercent, EditValue::Level(50)),
            (MixField::MaxLevelPercent, EditValue::Level(30)),
        ]
    );
    assert_eq!(
        [level.value(), cap.value(), limit.value()],
        [50, 30, 100],
        "each control shows its own value"
    );
}

// --- The page and the screen's life ------------------------------------------------------------

#[test]
fn the_page_shows_the_open_musicians_mix_with_its_tools_and_meters() {
    let (mut harness, model) = connected();
    harness.desk.emit_meters();
    harness.emit();

    let page = model.page();
    assert!(page.tools);
    assert_eq!(page.notice, None);
    assert_eq!(page.tabs.len(), 5, "four phones and the local mix");
    let body = page.body.expect("Lead vocal's mix is open");
    assert_eq!(body.label, "Lead vocal's mix");
    assert!(!body.local);
    assert_eq!(body.strips.len(), 6);
    assert_eq!(body.strip_levels, [50; 6], "each input's meter");
    assert_eq!(body.master.volume, 85);
    assert_eq!(
        body.master_levels,
        (0, 0),
        "the meters say nothing of this phone yet"
    );
    assert_eq!(body.limiter.state, "Limiter off");
    assert_eq!(body.reduction_db, 0.0);
    assert_eq!(body.returns.rows.len(), 2);
    assert_eq!(body.groups.len(), 2);
}

#[test]
fn the_local_mix_has_no_tools_and_its_body_is_named_for_it() {
    let (_harness, model) = connected();
    model.select(LOCAL_MIX);
    let page = model.page();
    assert!(!page.tools);
    let body = page.body.expect("the local mix is open");
    assert_eq!(body.label, "Local mix");
    assert!(body.local);
    assert_eq!(body.master.cap, None);
}

#[test]
fn a_notice_takes_the_place_of_the_strips() {
    let (mut harness, model) = connected();
    let mut stopped = sample_status();
    stopped.status.stopped = true;
    model.console().dispatch(status_of(stopped));
    let page = model.page();
    assert_eq!(page.notice, Some(MixNotice::Stopped));
    assert_eq!(page.body, None);
    assert!(page.tools, "the tools stay while a musician is open");

    let mut nobody = sample_status();
    nobody.musicians.clear();
    harness.desk.report = nobody;
    harness.desk.emit_status();
    harness.emit();
    let page = model.page();
    assert_eq!(page.notice, Some(MixNotice::NoMusicians));
    assert!(!page.tools);
    assert_eq!(page.tabs.len(), 1, "the local mix alone");
}

#[test]
fn leaving_the_screen_drops_late_answers_and_the_tools_start_afresh_on_return() {
    let (mut harness, model) = connected();
    harness.hold("save_preset");
    model.tool(crate::tools::ToolRequest::Save {
        name: "Verse".to_owned(),
    });
    harness.settle();
    model.leave();
    harness.release();

    assert_eq!(
        model.outcome(),
        None,
        "no message drawn for a screen not shown"
    );
    assert_eq!(model.page().outcome, None);
    model.enter();
    assert_eq!(model.tools().open(), None);
    assert_eq!(
        model.tools().data().map(|data| data.address.as_str()),
        Some(LEAD),
        "the tools hold the open musician's data again"
    );
    assert_eq!(harness.calls_to("save_preset").len(), 1);
    assert_eq!(model.console().state().presets.as_ref().unwrap().len(), 1);
}

#[test]
fn a_stopped_engine_with_no_automatic_start_shows_the_stopped_notice() {
    let (_harness, model) = crate::testing::screen_with(
        bandear_desktop_console::testing::FakeDesk::stopped(),
        bandear_desktop_console::console::Options {
            auto_start: false,
            ..Default::default()
        },
    );
    let page = model.page();
    assert_eq!(page.notice, Some(MixNotice::Stopped));
    assert_eq!(
        page.notice.unwrap().action(),
        Some(("Start engine", crate::mix_view::NoticeAction::Start))
    );
    assert!(page.tabs.is_empty());
}
