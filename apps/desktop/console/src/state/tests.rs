//! `console-state.test.js`, test for test. Two of its checks hold by construction here: the status
//! the state keeps has no musicians field, and a musician's mix is only ever nested.

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use super::*;
use crate::mix::{EditValue, edits};
use crate::testing::{sample_status, with_fields};

const ANA: &str = "192.168.1.3:50000";
const BO: &str = "192.168.1.12:50000";
const CY: &str = "192.168.1.20:50000";

fn default_mix_json() -> Value {
    serde_json::to_value(Mix::engine_default()).expect("a mix")
}

fn row(address: &str, extra: Value) -> Value {
    let base = with_fields(
        json!({
            "address": address,
            "clientName": null,
            "name": null,
            "nameIsLocal": false,
            "controlConnected": true,
        }),
        default_mix_json(),
    );
    with_fields(
        with_fields(
            base,
            json!({
                "packetsSent": 0,
                "packetsDiscarded": 0,
                "linkHealth": "good",
                "packetsPerSecond": 200,
                "underrunsPerSecond": 0,
                "lossPercent": 0,
                "receiveTimeoutsPerSecond": 0,
            }),
        ),
        extra,
    )
}

fn status(musicians: Vec<Value>, extra: Value) -> Action {
    let report = with_fields(
        json!({
            "run": 1,
            "deviceName": "Volt 4",
            "captureFormat": { "channels": 4, "sampleRate": 48000, "sampleFormat": "I24", "bufferSize": "Fixed(256)" },
            "controlPort": 50001,
            "sampleRate": 48000,
            "sourceChannels": 4,
            "groups": [],
            "channels": [],
            "stopped": false,
            "samplesReceived": 0,
            "packetsSent": 0,
            "packetsDiscarded": 0,
            "musicians": musicians,
        }),
        extra,
    );
    Action::Status(Box::new(serde_json::from_value(report).expect("a status")))
}

fn status_of(report: StatusReport) -> Action {
    Action::Status(Box::new(report))
}

fn reduced(actions: Vec<Action>) -> ConsoleState {
    let mut state = ConsoleState::new();
    reduce_on(&mut state, actions);
    state
}

fn reduce_on(state: &mut ConsoleState, actions: Vec<Action>) {
    for action in actions {
        state.reduce(&action);
    }
}

fn after(state: &ConsoleState, actions: Vec<Action>) -> ConsoleState {
    let mut next = state.clone();
    reduce_on(&mut next, actions);
    next
}

/// The status rows' mixes are revision 0; every mix made here is newer than all made before it,
/// unless the test names its revision.
static LAST_REVISION: AtomicU64 = AtomicU64::new(10);

fn mix_with(changes: Value) -> Mix {
    let revision = LAST_REVISION.fetch_add(1, Ordering::SeqCst) + 1;
    let mix = with_fields(
        with_fields(default_mix_json(), json!({ "revision": revision })),
        changes,
    );
    serde_json::from_value(mix).expect("a mix")
}

fn mix_changed(run: u64, address: &str, origin: &str, mix: Mix) -> Action {
    Action::MixChanged(Box::new(MixChanged {
        run: Some(run),
        address: Some(address.to_owned()),
        id: None,
        origin: origin.to_owned(),
        mix,
    }))
}

fn change(address: &str, origin: &str, mix: Mix) -> Action {
    mix_changed(1, address, origin, mix)
}

fn engine(phase: Phase) -> Action {
    Action::Engine { phase, error: None }
}

fn mix_edit(address: &str, edits: Edits) -> Action {
    Action::MixEdit {
        address: address.to_owned(),
        edits,
    }
}

fn mix_applied(address: &str, sent: Edits, mix: Option<Mix>) -> Action {
    Action::MixApplied {
        address: address.to_owned(),
        sent,
        mix,
    }
}

fn one(key: &str, value: impl Into<EditValue>) -> Edits {
    edits([(key, value.into())]).expect("an edit")
}

fn select(address: &str) -> Action {
    Action::Select(address.to_owned())
}

fn two_musicians() -> Action {
    status(
        vec![row(ANA, json!({ "name": "Ana" })), row(BO, json!({}))],
        json!({}),
    )
}

fn addresses(state: &ConsoleState) -> Vec<&str> {
    state
        .musicians
        .iter()
        .map(|musician| musician.address.as_str())
        .collect()
}

#[test]
fn the_console_starts_idle_with_no_musicians_and_nobody_selected() {
    let state = ConsoleState::new();
    assert_eq!(state.engine.phase, Phase::Idle);
    assert!(state.engine.status.is_none());
    assert!(state.musicians.is_empty());
    assert_eq!(state.selected, None);
    assert!(state.selected_musician().is_none());
}

#[test]
fn a_status_fills_in_the_engine_and_the_musicians_in_its_order_and_selects_the_first() {
    let state = reduced(vec![two_musicians()]);

    assert_eq!(state.engine.phase, Phase::Running);
    assert_eq!(state.engine.status.as_ref().unwrap().device_name, "Volt 4");
    assert_eq!(addresses(&state), [ANA, BO]);
    assert_eq!(state.selected.as_deref(), Some(ANA));
    assert_eq!(
        state.selected_musician().unwrap().name.as_deref(),
        Some("Ana")
    );
    assert_eq!(state.musician_by_address(BO).unwrap().link_health, "good");
    assert_eq!(
        state.musician_by_address(BO).unwrap().mix,
        Mix::engine_default()
    );
}

#[test]
fn the_selection_stays_with_its_musician_and_falls_back_to_the_first_when_it_leaves() {
    let picked = reduced(vec![two_musicians(), select(BO)]);
    assert_eq!(picked.selected.as_deref(), Some(BO));

    let still_there = after(
        &picked,
        vec![status(
            vec![row(ANA, json!({})), row(BO, json!({}))],
            json!({}),
        )],
    );
    assert_eq!(still_there.selected.as_deref(), Some(BO));

    let left = after(&picked, vec![status(vec![row(ANA, json!({}))], json!({}))]);
    assert_eq!(left.selected.as_deref(), Some(ANA));

    let nobody = after(&picked, vec![status(vec![], json!({}))]);
    assert_eq!(nobody.selected, None);
}

#[test]
fn selecting_a_musician_that_is_not_listed_changes_nothing() {
    let mut state = reduced(vec![two_musicians()]);

    assert!(!state.reduce(&select(CY)));
    assert!(!state.reduce(&select(ANA)), "already selected");
}

#[test]
fn a_stopped_engines_status_says_so() {
    let state = reduced(vec![status(vec![], json!({ "stopped": true }))]);

    assert_eq!(state.engine.phase, Phase::Stopped);
}

#[test]
fn the_engine_phase_follows_start_and_stop_and_keeps_the_error_text() {
    let starting = reduced(vec![engine(Phase::Starting)]);
    assert_eq!(starting.engine.phase, Phase::Starting);
    assert_eq!(starting.engine.error, None);

    let failed = after(
        &starting,
        vec![Action::Engine {
            phase: Phase::Error,
            error: Some("no input device".to_owned()),
        }],
    );
    assert_eq!(failed.engine.phase, Phase::Error);
    assert_eq!(failed.engine.error.as_deref(), Some("no input device"));

    let running = after(&failed, vec![two_musicians()]);
    assert_eq!(running.engine.phase, Phase::Running);
    assert_eq!(
        running.engine.error, None,
        "a running engine has no error to show"
    );

    let unknown = Phase::from_name("dancing").expect_err("an unknown phase");
    assert!(unknown.contains("phase"), "{unknown}");
}

#[test]
fn a_stopping_engine_stays_stopping_while_its_last_statuses_arrive() {
    let state = reduced(vec![
        two_musicians(),
        engine(Phase::Stopping),
        two_musicians(),
    ]);

    assert_eq!(state.engine.phase, Phase::Stopping);
}

#[test]
fn an_idle_engine_forgets_its_snapshot_musicians_and_edits_but_not_whose_mix_was_open() {
    let busy = reduced(vec![
        two_musicians(),
        select(BO),
        mix_edit(BO, one("volumePercent", 80)),
        engine(Phase::Idle),
    ]);

    assert_eq!(busy.engine.phase, Phase::Idle);
    assert!(busy.engine.status.is_none());
    assert!(busy.musicians.is_empty());
    assert!(busy.pending.is_empty());
    assert_eq!(busy.selected.as_deref(), Some(BO));
    assert!(busy.selected_musician().is_none());

    let back = after(
        &busy,
        vec![
            engine(Phase::Starting),
            status(
                vec![row(ANA, json!({})), row(BO, json!({}))],
                json!({ "run": 2 }),
            ),
        ],
    );
    assert_eq!(
        back.selected.as_deref(),
        Some(BO),
        "the same musician opens again after a restart"
    );
}

#[test]
fn a_late_status_from_an_engine_that_stopped_does_not_bring_it_back() {
    let mut stopped = reduced(vec![
        two_musicians(),
        engine(Phase::Stopping),
        engine(Phase::Idle),
    ]);

    assert!(!stopped.reduce(&two_musicians()));
    assert_eq!(stopped.engine.phase, Phase::Idle);
}

#[test]
fn a_late_status_from_the_engine_a_failed_start_replaced_does_not_bring_it_back() {
    let mut failed = reduced(vec![
        two_musicians(),
        engine(Phase::Starting),
        Action::Engine {
            phase: Phase::Error,
            error: Some("No input device is available.".to_owned()),
        },
    ]);

    assert!(!failed.reduce(&two_musicians()));
    assert_eq!(failed.engine.phase, Phase::Error);
    assert_eq!(
        failed.engine.error.as_deref(),
        Some("No input device is available.")
    );

    let next = after(
        &failed,
        vec![status(vec![row(ANA, json!({}))], json!({ "run": 2 }))],
    );
    assert_eq!(
        next.engine.phase,
        Phase::Running,
        "a status of the next run is taken"
    );
    assert_eq!(next.engine.run, Some(2));
}

#[test]
fn a_status_of_the_new_run_is_taken_before_the_starts_answer_and_the_old_runs_are_not() {
    let restarting = reduced(vec![two_musicians(), engine(Phase::Starting)]);

    let mut late = restarting.clone();
    assert!(
        !late.reduce(&two_musicians()),
        "run 1 ended when the restart began"
    );

    let mut fresh = after(
        &restarting,
        vec![status(vec![row(BO, json!({}))], json!({ "run": 2 }))],
    );
    assert_eq!(fresh.engine.phase, Phase::Running);
    assert_eq!(addresses(&fresh), [BO]);
    assert!(!fresh.reduce(&two_musicians()), "an older run never wins");
}

#[test]
fn a_late_mix_change_from_an_ended_run_is_ignored() {
    let stopped = reduced(vec![two_musicians(), engine(Phase::Starting)]);
    let mut restarted = after(
        &stopped,
        vec![status(vec![row(BO, json!({}))], json!({ "run": 2 }))],
    );

    let late = mix_changed(1, BO, "phone", mix_with(json!({ "volumePercent": 5 })));

    assert!(!restarted.reduce(&late));
}

#[test]
fn meters_are_live_only_for_the_current_run_or_for_a_newer_one() {
    let running = reduced(vec![two_musicians()]);
    let restarting = after(&running, vec![engine(Phase::Starting)]);

    assert!(running.is_live_run(Some(1)));
    assert!(!running.is_live_run(Some(0)));
    assert!(
        !running.is_live_run(None),
        "a payload without a run is not trusted"
    );
    assert!(!restarting.is_live_run(Some(1)));
    assert!(restarting.is_live_run(Some(2)));
}

#[test]
fn a_mix_change_replaces_that_musicians_mix_from_the_phone_or_from_this_desk() {
    let state = reduced(vec![
        two_musicians(),
        change(BO, "phone", mix_with(json!({ "volumePercent": 30 }))),
    ]);

    assert_eq!(state.view_mix(BO).unwrap().volume_percent, 30);
    assert_eq!(state.view_mix(ANA).unwrap().volume_percent, 100);

    let from_desk = after(
        &state,
        vec![change(
            BO,
            "desktop",
            mix_with(json!({ "volumePercent": 55 })),
        )],
    );
    assert_eq!(from_desk.view_mix(BO).unwrap().volume_percent, 55);
}

#[test]
fn a_mix_change_for_a_musician_not_listed_changes_nothing() {
    let mut state = reduced(vec![two_musicians()]);

    assert!(!state.reduce(&change(CY, "phone", Mix::engine_default())));
    assert!(state.view_mix(CY).is_none());
}

#[test]
fn local_edits_show_over_the_servers_mix_until_they_are_answered() {
    let state = reduced(vec![
        two_musicians(),
        mix_edit(BO, one("channelGains.1", 40)),
    ]);

    assert_eq!(state.view_mix(BO).unwrap().channel_gains[1], 40);
    assert_eq!(
        state.musician_by_address(BO).unwrap().mix.channel_gains[1],
        100,
        "the server's mix is kept"
    );
    assert_eq!(state.pending_edits(BO), one("channelGains.1", 40));
    assert!(state.pending_edits(ANA).is_empty());
}

#[test]
fn a_later_phone_change_does_not_hide_an_edit_still_waiting_for_its_answer() {
    let mut phone_mix = mix_with(json!({ "volumePercent": 30 }));
    phone_mix.channel_gains[1] = 90;

    let state = reduced(vec![
        two_musicians(),
        mix_edit(BO, one("channelGains.1", 40)),
        change(BO, "phone", phone_mix),
    ]);

    assert_eq!(state.view_mix(BO).unwrap().channel_gains[1], 40);
    assert_eq!(
        state.view_mix(BO).unwrap().volume_percent,
        30,
        "the phone's other changes show"
    );
}

#[test]
fn an_answer_settles_the_edits_it_carried_and_an_edit_made_since_keeps_showing() {
    let state = reduced(vec![
        two_musicians(),
        mix_edit(BO, one("channelGains.1", 40)),
        mix_edit(BO, one("channelGains.1", 45)),
        mix_applied(BO, one("channelGains.1", 40), None),
    ]);

    assert_eq!(state.view_mix(BO).unwrap().channel_gains[1], 45);
    assert_eq!(state.pending_edits(BO), one("channelGains.1", 45));

    let mut engine_mix = mix_with(json!({}));
    engine_mix.channel_gains[1] = 45;
    let settled = after(
        &state,
        vec![
            mix_changed(1, BO, "desktop", engine_mix),
            mix_applied(BO, one("channelGains.1", 45), None),
        ],
    );
    assert!(!settled.pending.contains_key(BO));
    assert_eq!(settled.view_mix(BO).unwrap().channel_gains[1], 45);
}

#[test]
fn an_older_answer_never_rolls_back_the_mix_so_a_phone_change_that_came_after_it_stays() {
    let mut answer_mix = mix_with(json!({ "revision": 5 }));
    answer_mix.channel_gains[1] = 40;
    let mut phone_mix = mix_with(json!({ "revision": 6, "volumePercent": 30 }));
    phone_mix.channel_gains[1] = 90;

    let state = reduced(vec![
        two_musicians(),
        mix_edit(BO, one("channelGains.1", 40)),
        // The engine applied the desk's change, then the phone's; the events come in that order...
        mix_changed(1, BO, "desktop", answer_mix.clone()),
        mix_changed(1, BO, "phone", phone_mix),
        // ...and the answer to the desk's request, holding the mix as it was then, comes last.
        mix_applied(BO, one("channelGains.1", 40), Some(answer_mix)),
    ]);

    assert_eq!(
        state.view_mix(BO).unwrap().channel_gains[1],
        90,
        "the phone's later change stays"
    );
    assert_eq!(state.view_mix(BO).unwrap().volume_percent, 30);
    assert!(!state.pending.contains_key(BO));
}

#[test]
fn an_answer_that_comes_before_its_event_shows_its_mix_at_once_and_the_event_changes_nothing() {
    let mut answer_mix = mix_with(json!({ "revision": 5 }));
    answer_mix.channel_gains[1] = 40;
    let sent = one("channelGains.1", 40);

    let mut answered = reduced(vec![
        two_musicians(),
        mix_edit(BO, sent.clone()),
        mix_applied(BO, sent, Some(answer_mix.clone())),
    ]);

    assert_eq!(
        answered.view_mix(BO).unwrap().channel_gains[1],
        40,
        "never the older value"
    );
    assert!(!answered.pending.contains_key(BO));
    assert_eq!(answered.musician_by_address(BO).unwrap().mix.revision, 5);
    assert!(!answered.reduce(&mix_changed(1, BO, "desktop", answer_mix)));
}

#[test]
fn an_event_that_comes_before_the_answer_shows_the_change_and_the_answer_settles_it() {
    let mut engine_mix = mix_with(json!({ "revision": 5 }));
    engine_mix.channel_gains[1] = 40;
    let sent = one("channelGains.1", 40);

    let state = reduced(vec![
        two_musicians(),
        mix_edit(BO, sent.clone()),
        mix_changed(1, BO, "desktop", engine_mix.clone()),
    ]);
    assert_eq!(state.view_mix(BO).unwrap().channel_gains[1], 40);

    let settled = after(&state, vec![mix_applied(BO, sent, Some(engine_mix))]);
    assert_eq!(settled.view_mix(BO).unwrap().channel_gains[1], 40);
    assert!(!settled.pending.contains_key(BO));
}

#[test]
fn a_status_with_an_older_revision_never_rolls_the_mix_back_and_a_newer_one_is_taken() {
    let mut engine_mix = mix_with(json!({ "revision": 5 }));
    engine_mix.channel_gains[1] = 40;
    let changed = reduced(vec![
        two_musicians(),
        mix_changed(1, BO, "phone", engine_mix),
    ]);

    let older = after(
        &changed,
        vec![status(
            vec![row(ANA, json!({})), row(BO, json!({ "revision": 4 }))],
            json!({}),
        )],
    );
    assert_eq!(
        older.view_mix(BO).unwrap().channel_gains[1],
        40,
        "the status was read before the change"
    );

    let mut gains = Mix::engine_default().channel_gains;
    gains[1] = 70;
    let newer_row = row(BO, json!({ "revision": 6, "channelGains": gains }));
    let newer = after(
        &older,
        vec![status(vec![row(ANA, json!({})), newer_row], json!({}))],
    );
    assert_eq!(newer.view_mix(BO).unwrap().channel_gains[1], 70);
}

#[test]
fn a_refused_change_falls_back_to_the_servers_mix_and_keeps_the_error_for_that_musician() {
    let refused = reduced(vec![
        two_musicians(),
        mix_edit(BO, one("volumePercent", 80)),
        Action::MixFailed {
            address: BO.to_owned(),
            sent: one("volumePercent", 80),
            error: "the engine is not running".to_owned(),
        },
    ]);

    assert_eq!(refused.view_mix(BO).unwrap().volume_percent, 100);
    assert_eq!(
        refused.mix_error,
        Some(MixError {
            address: BO.to_owned(),
            message: "the engine is not running".to_owned(),
        })
    );

    let recovered = after(
        &refused,
        vec![
            mix_edit(BO, one("volumePercent", 70)),
            mix_applied(BO, one("volumePercent", 70), None),
        ],
    );
    assert_eq!(recovered.mix_error, None);
}

#[test]
fn edits_for_a_musician_who_is_not_listed_are_ignored_and_a_bad_edit_fails_loudly() {
    let mut state = reduced(vec![two_musicians()]);

    assert!(!state.reduce(&mix_edit(CY, one("volumePercent", 80))));
    assert!(!state.reduce(&mix_edit(BO, Edits::new())));
    let error = EditKey::parse("volume").expect_err("not a mix field");
    assert!(error.contains("mix edit"), "{error}");
}

#[test]
fn a_status_keeps_the_edits_of_musicians_still_listed_and_drops_those_of_musicians_who_left() {
    let state = reduced(vec![
        two_musicians(),
        mix_edit(ANA, one("muted", true)),
        mix_edit(BO, one("muted", true)),
        status(vec![row(ANA, json!({}))], json!({})),
    ]);

    assert_eq!(state.pending_edits(ANA), one("muted", true));
    assert!(!state.pending.contains_key(BO));
    assert!(state.view_mix(ANA).unwrap().muted);
}

#[test]
fn musicians_whose_link_is_warning_or_bad_need_attention() {
    let state = reduced(vec![status(
        vec![
            row(ANA, json!({})),
            row(BO, json!({ "linkHealth": "warning" })),
            row(
                CY,
                json!({
                    "linkHealth": "bad",
                    "packetsPerSecond": null,
                    "underrunsPerSecond": null,
                    "lossPercent": null,
                    "receiveTimeoutsPerSecond": null,
                }),
            ),
        ],
        json!({}),
    )]);

    let needing: Vec<_> = state
        .musicians_needing_attention()
        .iter()
        .map(|musician| musician.address.as_str())
        .collect();
    assert_eq!(needing, [BO, CY]);
}

#[test]
fn when_the_open_musician_is_removed_the_first_one_opens_and_the_console_remembers_who_left() {
    let state = reduced(vec![
        two_musicians(),
        select(BO),
        status(vec![row(ANA, json!({ "name": "Ana" }))], json!({})),
    ]);

    assert_eq!(state.selected.as_deref(), Some(ANA));
    assert_eq!(
        state.departed,
        Some(Departed {
            address: BO.to_owned(),
            label: "192.168.1.12".to_owned(),
        })
    );

    let dismissed = after(&state, vec![select(ANA)]);
    assert_eq!(
        dismissed.departed, None,
        "picking a musician, even the open one, clears it"
    );

    let back = after(
        &state,
        vec![status(
            vec![row(ANA, json!({})), row(BO, json!({}))],
            json!({}),
        )],
    );
    assert_eq!(
        back.departed, None,
        "it clears when that musician is listed again"
    );

    let ended = after(&state, vec![engine(Phase::Idle)]);
    assert_eq!(ended.departed, None, "and when the run ends");
}

#[test]
fn a_musician_who_was_not_open_leaving_says_nothing() {
    let state = reduced(vec![
        two_musicians(),
        status(vec![row(ANA, json!({}))], json!({})),
    ]);

    assert_eq!(state.selected.as_deref(), Some(ANA));
    assert_eq!(state.departed, None);
}

#[test]
fn the_audio_choices_start_on_the_systems_own_driver_at_256_frames_and_asio_only_when_chosen() {
    assert_eq!(ConsoleState::new().audio, default_audio());

    let asio = reduced(vec![Action::Audio(AudioChange {
        host: Some("asio".to_owned()),
        asio_driver: Some(Some("Volt ASIO".to_owned())),
        ..AudioChange::default()
    })]);
    assert_eq!(
        asio.audio,
        Audio {
            host: Host::Asio,
            device: None,
            buffer_frames: Some(256),
            asio_driver: Some("Volt ASIO".to_owned()),
        }
    );

    let odd = reduced(vec![Action::Audio(AudioChange {
        host: Some("kernel".to_owned()),
        buffer_frames: Some(Some(0.0)),
        ..AudioChange::default()
    })]);
    assert_eq!(
        odd.audio,
        Audio {
            host: Host::Auto,
            device: None,
            buffer_frames: None,
            asio_driver: None,
        }
    );
}

#[test]
fn a_word_about_the_audio_input_lasts_for_its_run() {
    let fallback = AudioNotice {
        kind: AudioNoticeKind::Fallback,
        message: "The saved input Volt was not found.".to_owned(),
    };
    let said = reduced(vec![
        two_musicians(),
        Action::AudioNotice(Some(fallback.clone())),
    ]);

    assert_eq!(said.audio_notice, Some(fallback.clone()));
    assert_eq!(
        after(&said, vec![status(vec![row(ANA, json!({}))], json!({}))]).audio_notice,
        Some(fallback)
    );
    for phase in [Phase::Starting, Phase::Idle, Phase::Error] {
        assert_eq!(
            after(&said, vec![engine(phase)]).audio_notice,
            None,
            "{phase:?}"
        );
    }
    assert_eq!(
        after(&said, vec![Action::AudioNotice(None)]).audio_notice,
        None
    );
    assert_eq!(ConsoleState::new().audio_notice, None);
}

#[test]
fn a_failed_stop_keeps_the_run_and_its_musicians_and_says_what_failed() {
    let state = reduced(vec![
        two_musicians(),
        engine(Phase::Stopping),
        Action::CommandFailed {
            action: CommandAction::Stop,
            message: "could not stop the capture".to_owned(),
        },
    ]);

    assert_eq!(
        state.engine.phase,
        Phase::Running,
        "the engine may well still run"
    );
    assert_eq!(state.engine.run, Some(1));
    assert_eq!(state.musicians.len(), 2);
    assert_eq!(
        state.command_error,
        Some(CommandError {
            action: CommandAction::Stop,
            message: "could not stop the capture".to_owned(),
        })
    );

    let later = after(
        &state,
        vec![change(BO, "phone", mix_with(json!({ "volumePercent": 9 })))],
    );
    assert_eq!(
        later.view_mix(BO).unwrap().volume_percent,
        9,
        "its events still count"
    );

    let retried = after(&state, vec![engine(Phase::Stopping)]);
    assert_eq!(
        retried.command_error, None,
        "the next start or stop clears it"
    );
}

#[test]
fn a_command_that_works_clears_the_refusal_of_the_same_command_and_only_that_one() {
    let refused = reduced(vec![
        two_musicians(),
        Action::CommandFailed {
            action: CommandAction::Switch,
            message: "the device is busy".to_owned(),
        },
    ]);

    assert_eq!(
        after(&refused, vec![Action::CommandDone(CommandAction::Switch)]).command_error,
        None
    );
    let mut other = refused.clone();
    assert!(!other.reduce(&Action::CommandDone(CommandAction::Stop)));
}

#[test]
fn the_console_is_connecting_until_it_hears_the_engine_and_says_when_it_cannot() {
    assert_eq!(
        ConsoleState::new().connection,
        Connection {
            status: ConnectionStatus::Connecting,
            error: None,
        }
    );

    let failed = reduced(vec![Action::ConnectionFailed(
        "the event channel is closed".to_owned(),
    )]);
    assert_eq!(
        failed.connection,
        Connection {
            status: ConnectionStatus::Failed,
            error: Some("the event channel is closed".to_owned()),
        }
    );

    let again = after(&failed, vec![Action::Connecting]);
    assert_eq!(again.connection.status, ConnectionStatus::Connecting);
    assert_eq!(again.connection.error, None);
    assert_eq!(
        after(&again, vec![Action::Connected]).connection,
        Connection {
            status: ConnectionStatus::Ready,
            error: None,
        }
    );
    assert_eq!(
        after(&again, vec![two_musicians()]).connection.status,
        ConnectionStatus::Ready,
        "a status proves it"
    );
}

#[test]
fn an_action_that_changes_nothing_leaves_the_state_as_it_was() {
    // The web reducer ignored an action it did not know; here every action is known, so the
    // nearest check is an action that names nothing the console has.
    let mut state = reduced(vec![two_musicians()]);

    assert!(!state.reduce(&Action::CommandDone(CommandAction::Start)));
}

fn lead() -> String {
    sample_status().musicians[0].address.clone()
}

fn sample_with(change: impl FnOnce(&mut StatusReport)) -> Action {
    let mut report = sample_status();
    change(&mut report);
    status_of(report)
}

#[test]
fn edits_to_group_levels_are_dropped_when_the_groups_move_and_kept_when_one_is_only_renamed() {
    let lead = lead();
    let levels = edits([
        ("groupLevels.0", 30.into()),
        ("groupLevels.1", 80.into()),
        ("channelGains.0", 50.into()),
    ])
    .unwrap();
    let edited = reduced(vec![
        status_of(sample_status()),
        mix_edit(&lead, levels.clone()),
    ]);

    let renamed = after(
        &edited,
        vec![sample_with(|status| {
            status.status.groups[0].name = "Kit".to_owned()
        })],
    );
    assert_eq!(renamed.pending_edits(&lead), levels);

    // Reordered: a level kept by position would land on the other group.
    let reordered = after(
        &edited,
        vec![sample_with(|status| status.status.groups.reverse())],
    );
    assert_eq!(reordered.pending_edits(&lead), one("channelGains.0", 50));
    let deleted = after(
        &edited,
        vec![sample_with(|status| {
            status.status.groups.pop();
        })],
    );
    assert_eq!(deleted.pending_edits(&lead), one("channelGains.0", 50));
}

#[test]
fn edits_to_an_input_are_dropped_when_it_joins_or_leaves_a_pair_or_is_hidden_others_are_kept() {
    let lead = lead();
    let edited = reduced(vec![
        status_of(sample_status()),
        mix_edit(
            &lead,
            edits([
                ("pans.1", 30.into()),
                ("channelGains.2", 40.into()),
                ("channelMuted.4", true.into()),
                ("channelSolo.5", true.into()),
                ("volumePercent", 70.into()),
            ])
            .unwrap(),
        ),
    ]);

    let linked = after(
        &edited,
        vec![sample_with(|status| status.status.channels[1].link = true)],
    );
    assert_eq!(
        linked.pending_edits(&lead),
        edits([
            ("channelMuted.4", true.into()),
            ("channelSolo.5", true.into()),
            ("volumePercent", 70.into()),
        ])
        .unwrap()
    );

    let hidden = after(
        &edited,
        vec![sample_with(|status| {
            status.status.channels[4].hidden = true
        })],
    );
    assert_eq!(
        hidden.pending_edits(&lead),
        edits([
            ("pans.1", 30.into()),
            ("channelGains.2", 40.into()),
            ("channelSolo.5", true.into()),
            ("volumePercent", 70.into()),
        ])
        .unwrap()
    );

    let renamed = after(
        &edited,
        vec![sample_with(|status| {
            status.status.channels[1].name = "Snare top".to_owned()
        })],
    );
    assert_eq!(
        renamed.pending_edits(&lead),
        edited.pending_edits(&lead),
        "a name moves nothing"
    );
}

#[test]
fn the_local_mix_is_opened_and_edited_like_a_musicians_but_keeps_no_cap_limit_or_lock() {
    let report = sample_status();
    let mut state = reduced(vec![status_of(report)]);
    assert!(
        state.mix_of(LOCAL_MIX).is_some(),
        "the local mix is open while an engine runs"
    );
    assert!(state.reduce(&select(LOCAL_MIX)));

    state.reduce(&mix_edit(
        LOCAL_MIX,
        edits([
            ("volumePercent", 60.into()),
            ("maxLevelPercent", 50.into()),
            ("engineerLimitPercent", 40.into()),
            ("lockedChannels.0", true.into()),
        ])
        .unwrap(),
    ));
    assert_eq!(state.pending_edits(LOCAL_MIX), one("volumePercent", 60));

    let local = Action::MixChanged(Box::new(MixChanged {
        run: Some(1),
        address: None,
        id: Some(LOCAL_MIX.to_owned()),
        origin: "desktop".to_owned(),
        mix: Mix {
            revision: 9,
            muted: true,
            ..Mix::engine_default()
        },
    }));
    assert!(state.reduce(&local));
    assert!(state.view_mix(LOCAL_MIX).unwrap().muted);
}

#[test]
fn whole_value_edits_settle_only_on_the_answer_to_the_one_sent() {
    let mut state = reduced(vec![two_musicians()]);
    let first = StripSettings::default();
    state.reduce(&Action::ProcessingEdit {
        channel: 3,
        settings: first,
    });
    let sent = state.processing_pending[&3].stamp;
    let mut second = StripSettings::default();
    second.gate.enabled = true;
    state.reduce(&Action::ProcessingEdit {
        channel: 3,
        settings: second,
    });

    state.reduce(&Action::ProcessingApplied {
        channel: Some(3),
        sent: Some(sent),
        channels: Vec::new(),
    });
    assert_eq!(
        state
            .processing_pending
            .get(&3)
            .map(|waiting| waiting.value),
        Some(second),
        "a later edit stays"
    );
    assert_eq!(
        state.processing_active(3),
        Some(true),
        "the edit on its way counts"
    );
    assert_eq!(state.processing_active(4), None, "never read yet");
    assert_eq!(state.channel_strip(4).settings, StripSettings::default());

    state.reduce(&Action::SendEdit {
        channel: 2,
        effect: Effect::Reverb,
        level: 30,
    });
    state.reduce(&Action::SendEdit {
        channel: 2,
        effect: Effect::Reverb,
        level: 40,
    });
    state.reduce(&Action::SendSettled {
        channel: 2,
        effect: Effect::Reverb,
        level: 30,
    });
    assert_eq!(
        state.sends_pending[&2].reverb,
        Some(40),
        "a later level stays"
    );
    state.reduce(&Action::SendSettled {
        channel: 2,
        effect: Effect::Reverb,
        level: 40,
    });
    assert!(state.sends_pending.is_empty());
}
