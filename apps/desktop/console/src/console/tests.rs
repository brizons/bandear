//! The engine-lifecycle tests of `console-app.test.js` (connect, start, stop, audio choices,
//! devices, title, select, dispose), driven by the fake desk. The fake bridge's simulated engine is
//! not ported (D7); [FakeDesk] answers what these tests need. The mix, group, channel and screen
//! flows' tests go with their screens.

use std::cell::RefCell;
use std::rc::Rc;

use bandear_desktop_core::session::SessionAudio;

use super::*;
use crate::engine_config::{AudioChange, default_audio};
use crate::state::{CommandError, Connection, ConnectionStatus};
use crate::testing::{ASIO_DRIVER, FakeDesk, Harness, INTERFACE, USB, sample_status};

const LEAD: &str = "192.168.1.3:50000";
const BASS: &str = "192.168.1.12:50000";
const VOLT: &str = "INPUT 1/2 (Volt 4)";

fn connected() -> Harness {
    Harness::connected(FakeDesk::running(), Options::default())
}

fn opened(saved: SessionAudio) -> Harness {
    let mut desk = FakeDesk::stopped();
    desk.saved_audio = Some(saved);
    Harness::connected(desk, Options::default())
}

fn saved(
    host: &str,
    device: Option<&str>,
    buffer: Option<u32>,
    asio: Option<&str>,
) -> SessionAudio {
    SessionAudio {
        host: host.to_owned(),
        device: device.map(str::to_owned),
        buffer_frames: buffer,
        asio_driver: asio.map(str::to_owned),
    }
}

fn audio(host: Host, device: Option<&str>, buffer: Option<u32>, asio: Option<&str>) -> Audio {
    Audio {
        host,
        device: device.map(str::to_owned),
        buffer_frames: buffer,
        asio_driver: asio.map(str::to_owned),
    }
}

fn host(name: &str) -> AudioChange {
    AudioChange {
        host: Some(name.to_owned()),
        ..AudioChange::default()
    }
}

fn buffer(frames: f64) -> AudioChange {
    AudioChange {
        buffer_frames: Some(Some(frames)),
        ..AudioChange::default()
    }
}

fn start(harness: &mut Harness) -> Option<Outcome> {
    let flow = harness.console.start();
    harness.run(flow)
}

fn stop(harness: &mut Harness) {
    let flow = harness.console.stop();
    harness.run(flow);
}

fn apply(
    harness: &mut Harness,
    change: AudioChange,
    device: Option<&str>,
    keep_device: bool,
) -> Option<Outcome> {
    let flow = harness
        .console
        .apply_audio(change, device.map(str::to_owned), keep_device);
    harness.run(flow)
}

fn switch(harness: &mut Harness, device: &str) {
    let flow = harness.console.switch_device(device.to_owned());
    harness.run(flow);
}

fn reconnect(harness: &mut Harness) {
    let flow = harness.console.connect();
    harness.run(flow);
}

fn starts(harness: &Harness) -> Vec<(String, Option<String>)> {
    harness.starts()
}

fn on(host: &str, device: Option<&str>) -> (String, Option<String>) {
    (host.to_owned(), device.map(str::to_owned))
}

fn last_start(harness: &Harness) -> (String, Option<String>) {
    harness.starts().last().cloned().expect("a start")
}

fn audio_saves(harness: &Harness) -> usize {
    harness.calls_to("set_audio_settings").len()
}

fn phase(harness: &Harness) -> Phase {
    harness.console.state().engine.phase
}

fn device_name(harness: &Harness) -> String {
    harness
        .console
        .state()
        .engine
        .status
        .as_ref()
        .map(|status| status.device_name.clone())
        .unwrap_or_default()
}

fn volume(harness: &Harness, address: &str) -> Option<u8> {
    harness
        .console
        .state()
        .view_mix(address)
        .map(|mix| mix.volume_percent)
}

#[test]
fn connecting_to_a_running_engine_shows_it_and_starts_nothing() {
    let harness = connected();

    let state = harness.console.state();
    assert_eq!(state.engine.phase, Phase::Running);
    assert_eq!(state.musicians.len(), sample_status().musicians.len());
    assert_eq!(state.selected.as_deref(), Some(LEAD));
    assert!(harness.starts().is_empty());
}

#[test]
fn connecting_to_a_stopped_engine_starts_it_as_the_old_window_did_never_on_asio() {
    let harness = Harness::connected(FakeDesk::stopped(), Options::default());

    let requests: Vec<_> = harness
        .calls_to("start_engine")
        .into_iter()
        .map(|command| match command {
            Command::StartEngine(request) => (
                request.device_filter.clone(),
                request.targets.clone(),
                request.control_port,
                request.groups.len(),
                request.host.clone(),
                request.buffer_frames,
            ),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        requests,
        [(
            None,
            vec![],
            Some(50001),
            0,
            Some("auto".to_owned()),
            Some(256)
        )]
    );
    assert_eq!(phase(&harness), Phase::Running);
}

#[test]
fn without_auto_start_a_stopped_engine_waits_for_the_engineer() {
    let options = Options {
        auto_start: false,
        ..Options::default()
    };
    let mut harness = Harness::connected(FakeDesk::stopped(), options);

    assert_eq!(phase(&harness), Phase::Idle);
    assert!(harness.starts().is_empty());

    start(&mut harness);
    assert_eq!(phase(&harness), Phase::Running);
}

#[test]
fn a_failed_start_shows_its_error_and_a_late_status_of_the_engine_it_replaced_is_ignored() {
    let mut harness = connected();
    let before = harness.desk.status();
    harness.desk.fail_start = Some("No input device is available.".to_owned());

    start(&mut harness);
    harness.console.event(EngineEvent::Status(Box::new(before)));

    let state = harness.console.state();
    assert_eq!(state.engine.phase, Phase::Error);
    assert_eq!(
        state.engine.error.as_deref(),
        Some("No input device is available.")
    );
    assert!(state.musicians.is_empty());
}

#[test]
fn after_a_stop_late_events_of_the_stopped_engine_are_ignored() {
    let mut harness = connected();
    let before = harness.desk.status();
    let meters = Rc::new(RefCell::new(0));
    let heard = Rc::clone(&meters);
    harness.console.on_meters(move |_| *heard.borrow_mut() += 1);

    stop(&mut harness);
    harness.console.event(EngineEvent::Status(Box::new(before)));
    harness.desk.change_from_phone(LEAD, 5);
    harness.desk.emit_meters();
    harness.desk.run = 1;
    harness.emit();

    assert_eq!(phase(&harness), Phase::Idle);
    assert!(harness.console.state().musicians.is_empty());
    assert_eq!(*meters.borrow(), 0);
}

#[test]
fn meters_of_the_running_engine_reach_every_listener_until_it_unsubscribes() {
    let mut harness = connected();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let heard = Rc::clone(&seen);
    let listener = harness
        .console
        .on_meters(move |meters| heard.borrow_mut().push(meters.channels.len()));

    harness.desk.emit_meters();
    harness.emit();
    harness.console.off_meters(listener);
    harness.desk.emit_meters();
    harness.emit();

    assert_eq!(*seen.borrow(), [sample_status().status.source_channels]);
    assert_eq!(
        harness.console.meters().map(|meters| meters.run),
        Some(Some(1)),
        "the latest meters stay for the views"
    );
}

#[test]
fn a_failed_stop_keeps_the_engines_run_shows_the_error_and_its_events_still_count() {
    let mut harness = connected();
    harness
        .desk
        .fail_next("stop_engine", "could not stop the capture", 1);

    stop(&mut harness);
    harness.desk.change_from_phone(LEAD, 5);
    harness.emit();

    let state = harness.console.state();
    assert_eq!(
        state.engine.phase,
        Phase::Running,
        "engine_status says it still runs"
    );
    assert_eq!(state.musicians.len(), sample_status().musicians.len());
    assert_eq!(
        state.command_error,
        Some(CommandError {
            action: CommandAction::Stop,
            message: "could not stop the capture".to_owned(),
        })
    );
    assert_eq!(state.view_mix(LEAD).map(|mix| mix.volume_percent), Some(5));
}

#[test]
fn a_stop_that_reports_a_failure_but_did_stop_the_engine_ends_the_run() {
    let mut harness = connected();
    harness
        .desk
        .fail_after_effect("stop_engine", "stopped, but late");

    stop(&mut harness);

    assert_eq!(phase(&harness), Phase::Idle);
}

#[test]
fn when_a_start_works_but_reading_its_status_fails_the_new_run_stays_live() {
    let mut harness = connected();
    harness
        .desk
        .fail_next("engine_status", "the status could not be read", 1);

    start(&mut harness);
    harness.desk.change_from_phone(LEAD, 7);
    harness.emit();

    let state = harness.console.state();
    assert_eq!(state.engine.phase, Phase::Running);
    assert_eq!(state.engine.error, None);
    assert_eq!(
        state.view_mix(LEAD).map(|mix| mix.volume_percent),
        Some(7),
        "the new run's events still count"
    );
}

#[test]
fn only_an_engine_that_is_not_running_is_started_any_other_failure_is_shown() {
    let mut desk = FakeDesk::stopped();
    desk.fail_next("engine_status", "engine state lock was poisoned", 1);
    let mut harness = Harness::connected(desk, Options::default());

    assert!(harness.starts().is_empty());
    assert_eq!(
        harness.console.state().connection,
        Connection {
            status: ConnectionStatus::Failed,
            error: Some("engine state lock was poisoned".to_owned()),
        }
    );

    reconnect(&mut harness);
    assert_eq!(harness.starts().len(), 1, "now it is just not running");
    assert_eq!(
        harness.console.state().connection.status,
        ConnectionStatus::Ready
    );
    assert_eq!(phase(&harness), Phase::Running);
}

#[test]
fn a_failed_event_registration_is_shown_and_connecting_again_listens_once() {
    let mut desk = FakeDesk::running();
    desk.listen_failure = Some("the event channel is closed".to_owned());
    let mut harness = Harness::connected(desk, Options::default());

    let connection = harness.console.state().connection.clone();
    assert_eq!(connection.status, ConnectionStatus::Failed);
    assert!(
        connection
            .error
            .unwrap_or_default()
            .contains("the event channel is closed")
    );
    harness.desk.change_from_phone(LEAD, 3);
    harness.emit();
    assert!(
        harness.console.state().musicians.is_empty(),
        "no half-connected listeners are left behind"
    );

    harness.desk.listen_failure = None;
    reconnect(&mut harness);
    let updates = Rc::new(RefCell::new(0));
    let counted = Rc::clone(&updates);
    harness
        .console
        .subscribe(move |_, _| *counted.borrow_mut() += 1);
    harness.desk.change_from_phone(LEAD, 4);
    harness.emit();

    assert_eq!(
        harness.console.state().connection.status,
        ConnectionStatus::Ready
    );
    assert_eq!(volume(&harness, LEAD), Some(4));
    assert_eq!(*updates.borrow(), 1, "each event is heard once");
    assert_eq!(harness.listens, 2, "one refused, one heard");
}

#[test]
fn a_start_whose_status_never_comes_says_so_and_offers_to_try_again() {
    let mut desk = FakeDesk::running();
    desk.status_on_start = false;
    let mut harness = Harness::connected(desk, Options::default());
    harness
        .desk
        .fail_next("engine_status", "the status could not be read", 4);

    start(&mut harness);
    harness.run_timers();

    let connection = harness.console.state().connection.clone();
    assert_eq!(
        connection.status,
        ConnectionStatus::Failed,
        "not left on starting with nothing to press"
    );
    assert!(
        connection
            .error
            .unwrap_or_default()
            .contains("could not be read")
    );

    reconnect(&mut harness);
    let state = harness.console.state();
    assert_eq!(state.connection.status, ConnectionStatus::Ready);
    assert_eq!(state.engine.phase, Phase::Running);
    assert_eq!(state.engine.run, Some(2), "the started run, never ended");
}

#[test]
fn two_quick_reconnects_listen_to_the_engine_only_once() {
    let mut desk = FakeDesk::running();
    desk.listen_failure = Some("the event channel is closed".to_owned());
    let mut harness = Harness::connected(desk, Options::default());
    harness.desk.listen_failure = None;

    let first = harness.console.connect();
    let second = harness.console.connect();
    harness.console.spawn(first);
    harness.run(second);

    assert_eq!(harness.listens, 2, "the refused one and one more");
    assert_eq!(
        harness.console.state().connection.status,
        ConnectionStatus::Ready
    );
}

#[test]
fn devices_are_listed_for_the_driver_asked_for_and_a_failure_to_list_says_so() {
    let mut harness = connected();

    let flow = harness.console.load_devices(Host::Wasapi);
    harness.run(flow);
    {
        let state = harness.console.state();
        assert_eq!(state.devices.host, Some(Host::Wasapi));
        assert_eq!(state.devices.list.len(), 3);
    }
    assert!(matches!(
        harness.calls_to("list_input_devices").last(),
        Some(Command::ListInputDevices { host: Some(host) }) if host == "wasapi"
    ));

    harness
        .desk
        .fail_next("list_input_devices", "no such backend", 1);
    let flow = harness.console.load_devices(Host::Asio);
    harness.run(flow);
    let state = harness.console.state();
    assert_eq!(state.devices.host, Some(Host::Asio));
    assert!(state.devices.list.is_empty());
    assert_eq!(state.devices.error.as_deref(), Some("no such backend"));
}

#[test]
fn a_driver_change_restarts_on_it_with_no_device_named_a_buffer_change_keeps_the_device() {
    let mut harness = connected();

    apply(&mut harness, host("wasapi"), None, false);
    apply(&mut harness, buffer(512.0), None, true);

    let requests: Vec<_> = harness
        .calls_to("start_engine")
        .into_iter()
        .map(|command| match command {
            Command::StartEngine(request) => (
                request.host.clone(),
                request.device_filter.clone(),
                request.buffer_frames,
            ),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        requests,
        [
            (Some("wasapi".to_owned()), None, Some(256)),
            (
                Some("wasapi".to_owned()),
                Some(INTERFACE.to_owned()),
                Some(512)
            ),
        ]
    );
}

#[test]
fn asio_starts_only_on_a_driver_the_engineer_picked() {
    let mut harness = connected();

    let refused = apply(&mut harness, host("asio"), None, false).expect("an answer");
    assert!(matches!(&refused, Err(message) if message.contains("Network and system")));
    assert!(harness.starts().is_empty(), "nothing loaded a driver");

    apply(&mut harness, host("asio"), Some("Volt ASIO"), false);
    assert_eq!(starts(&harness), [on("asio", Some("Volt ASIO"))]);
}

#[test]
fn after_asio_was_applied_a_plain_start_and_a_start_after_a_stop_use_the_picked_driver() {
    let mut harness = connected();
    apply(&mut harness, host("asio"), Some(ASIO_DRIVER), false);

    stop(&mut harness);
    start(&mut harness);

    assert_eq!(last_start(&harness), on("asio", Some(ASIO_DRIVER)));
}

#[test]
fn the_automatic_start_after_a_reconnect_uses_the_picked_asio_driver() {
    let mut harness = connected();
    apply(&mut harness, host("asio"), Some(ASIO_DRIVER), false);
    harness.desk.running = false;

    reconnect(&mut harness);

    assert_eq!(last_start(&harness), on("asio", Some(ASIO_DRIVER)));
}

#[test]
fn asio_without_a_picked_driver_never_reaches_the_engine_and_says_where_to_pick_one() {
    let mut harness = connected();
    stop(&mut harness);
    harness.console.set_audio(host("asio"));

    start(&mut harness);

    assert!(harness.starts().is_empty());
    let state = harness.console.state();
    assert_eq!(state.engine.phase, Phase::Error);
    assert!(
        state
            .engine
            .error
            .clone()
            .unwrap_or_default()
            .contains("Network and system")
    );
}

#[test]
fn a_failed_asio_start_keeps_the_settings_that_were_applied_a_successful_one_applies_it() {
    let mut harness = connected();
    harness.desk.fail_start = Some("the driver did not load".to_owned());

    let failed = apply(&mut harness, host("asio"), Some(ASIO_DRIVER), false);

    assert_eq!(failed, Some(Err("the driver did not load".to_owned())));
    assert_eq!(harness.console.state().audio, default_audio());
    harness.desk.fail_start = None;
    assert_eq!(
        apply(&mut harness, host("asio"), Some(ASIO_DRIVER), false),
        Some(Ok(()))
    );
    assert_eq!(
        harness.console.state().audio,
        audio(Host::Asio, None, Some(256), Some(ASIO_DRIVER))
    );
}

#[test]
fn a_device_switch_is_live_under_wasapi_and_a_restart_on_that_device_otherwise() {
    let mut harness = connected();

    switch(&mut harness, USB);
    assert_eq!(last_start(&harness).1.as_deref(), Some(USB));
    assert!(harness.calls_to("switch_device").is_empty());

    apply(&mut harness, host("wasapi"), None, false);
    switch(&mut harness, INTERFACE);
    assert!(matches!(
        harness.calls_to("switch_device").last(),
        Some(Command::SwitchDevice { device }) if device == INTERFACE
    ));
}

#[test]
fn a_refused_live_switch_is_said_and_the_engine_keeps_its_device() {
    let mut harness = connected();
    apply(&mut harness, host("wasapi"), None, false);
    harness
        .desk
        .fail_next("switch_device", "the device is busy", 1);

    switch(&mut harness, USB);

    assert_eq!(
        harness.console.state().command_error,
        Some(CommandError {
            action: CommandAction::Switch,
            message: "the device is busy".to_owned(),
        })
    );
    assert_eq!(phase(&harness), Phase::Running);
    assert_eq!(device_name(&harness), INTERFACE);
}

#[test]
fn a_switch_that_works_clears_the_refusal_of_an_earlier_one() {
    let mut harness = connected();
    apply(&mut harness, host("wasapi"), None, false);
    harness
        .desk
        .fail_next("switch_device", "the device is busy", 1);
    switch(&mut harness, USB);
    assert_eq!(
        harness
            .console
            .state()
            .command_error
            .as_ref()
            .map(|error| error.action),
        Some(CommandAction::Switch)
    );

    switch(&mut harness, USB);

    assert_eq!(harness.console.state().command_error, None);
    assert_eq!(device_name(&harness), USB);
}

#[test]
fn a_live_switch_is_remembered_start_a_start_after_a_stop_and_a_buffer_change_reopen_it() {
    let mut harness = connected();
    apply(&mut harness, host("wasapi"), None, false);

    switch(&mut harness, USB);

    assert_eq!(harness.console.state().audio.device.as_deref(), Some(USB));
    assert_eq!(
        harness.desk.saved_audio,
        Some(saved("wasapi", Some(USB), Some(256), None))
    );
    stop(&mut harness);
    start(&mut harness);
    assert_eq!(
        last_start(&harness),
        on("wasapi", Some(USB)),
        "not the system's default"
    );
    assert_eq!(device_name(&harness), USB);
    apply(&mut harness, buffer(512.0), None, true);
    assert_eq!(last_start(&harness), on("wasapi", Some(USB)));
    assert_eq!(
        harness.desk.saved_audio,
        Some(saved("wasapi", Some(USB), Some(512), None))
    );
}

#[test]
fn a_refused_live_switch_remembers_nothing() {
    let mut harness = connected();
    apply(&mut harness, host("wasapi"), None, false);
    let saves = audio_saves(&harness);
    harness
        .desk
        .fail_next("switch_device", "the device is busy", 1);

    switch(&mut harness, USB);

    assert_eq!(harness.console.state().audio.device, None);
    assert_eq!(audio_saves(&harness), saves, "nothing saved");
    stop(&mut harness);
    start(&mut harness);
    assert_eq!(last_start(&harness), on("wasapi", None));
}

#[test]
fn the_remembered_input_is_read_before_the_automatic_start_which_reopens_it() {
    let harness = opened(saved("wasapi", Some(USB), Some(512), None));

    let order: Vec<_> = harness.calls.iter().map(Command::name).collect();
    let read = order.iter().position(|name| *name == "audio_settings");
    let started = order.iter().position(|name| *name == "start_engine");
    assert!(read < started, "{order:?}");
    let requests: Vec<_> = harness
        .calls_to("start_engine")
        .into_iter()
        .map(|command| match command {
            Command::StartEngine(request) => (
                request.device_filter.clone(),
                request.host.clone(),
                request.buffer_frames,
            ),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        requests,
        [(Some(USB.to_owned()), Some("wasapi".to_owned()), Some(512))]
    );
    assert_eq!(device_name(&harness), USB);
    assert_eq!(
        harness.console.state().audio,
        audio(Host::Wasapi, Some(USB), Some(512), None),
        "the Network screen shows the remembered choice"
    );
    assert_eq!(
        audio_saves(&harness),
        0,
        "a choice already kept is not saved again"
    );
}

#[test]
fn a_remembered_input_that_is_gone_gives_way_to_the_default_on_its_driver_said_and_kept() {
    let kept = saved("wasapi", Some(VOLT), Some(256), None);
    let mut harness = opened(kept.clone());

    assert_eq!(
        starts(&harness),
        [on("wasapi", Some(VOLT)), on("wasapi", None)]
    );
    {
        let state = harness.console.state();
        assert_eq!(state.engine.phase, Phase::Running);
        assert_eq!(state.engine.status.as_ref().unwrap().device_name, INTERFACE);
        let notice = state.audio_notice.clone().expect("a word about the input");
        assert_eq!(notice.kind, AudioNoticeKind::Fallback);
        assert!(
            notice
                .message
                .contains("The saved input INPUT 1/2 (Volt 4) was not found")
        );
        assert_eq!(
            state.audio.device.as_deref(),
            Some(VOLT),
            "the choice stays"
        );
    }
    assert_eq!(
        harness.desk.saved_audio,
        Some(kept.clone()),
        "the default never replaces the saved input"
    );
    assert_eq!(audio_saves(&harness), 0);

    // Meanwhile a buffer change waits for the next start instead of restarting the default.
    apply(&mut harness, buffer(512.0), None, true);
    assert_eq!(harness.starts().len(), 2);
    assert_eq!(harness.console.state().audio.buffer_frames, Some(512));
    assert!(harness.console.state().audio_notice.is_some());

    stop(&mut harness);
    assert_eq!(
        harness.console.state().audio_notice,
        None,
        "said for its run only"
    );
    start(&mut harness);
    assert_eq!(
        starts(&harness)[2..],
        [on("wasapi", Some(VOLT)), on("wasapi", None)]
    );
    assert!(matches!(
        harness.calls_to("start_engine").last(),
        Some(Command::StartEngine(request)) if request.buffer_frames == Some(512)
    ));
    assert_eq!(
        harness.desk.saved_audio,
        Some(kept),
        "still the engineer's choice"
    );
}

#[test]
fn when_the_default_input_does_not_start_either_both_are_said() {
    let mut desk = FakeDesk::stopped();
    desk.saved_audio = Some(saved("wasapi", Some(VOLT), Some(256), None));
    desk.fail_next("start_engine", "the driver did not load", 2);
    let harness = Harness::connected(desk, Options::default());

    assert_eq!(
        starts(&harness),
        [on("wasapi", Some(VOLT)), on("wasapi", None)]
    );
    let state = harness.console.state();
    assert_eq!(state.engine.phase, Phase::Error);
    assert_eq!(
        state.engine.error.as_deref(),
        Some(
            "INPUT 1/2 (Volt 4) was not found, and the system's default input did not start either: \
             the driver did not load."
        )
    );
}

#[test]
fn a_remembered_asio_driver_that_is_gone_never_gives_way_to_another_asio_driver() {
    let kept = saved("asio", None, Some(128), Some("Gone ASIO"));
    let harness = opened(kept.clone());

    assert_eq!(
        starts(&harness),
        [on("asio", Some("Gone ASIO")), on("auto", None)]
    );
    let state = harness.console.state();
    assert_eq!(state.engine.phase, Phase::Running);
    assert!(state.audio_notice.as_ref().is_some_and(|notice| {
        notice
            .message
            .contains("The saved ASIO driver Gone ASIO was not found")
    }));
    assert_eq!(harness.desk.saved_audio, Some(kept));
}

#[test]
fn a_kept_asio_choice_starts_only_on_its_driver_and_one_without_a_driver_is_no_asio() {
    let picked = opened(saved("asio", None, Some(256), Some(ASIO_DRIVER)));
    assert_eq!(starts(&picked), [on("asio", Some(ASIO_DRIVER))]);

    let odd = opened(saved("asio", None, Some(256), None));
    assert_eq!(starts(&odd), [on("auto", None)], "no ASIO driver is loaded");
    assert_eq!(phase(&odd), Phase::Running);
    assert_eq!(odd.console.state().audio.host, Host::Auto);
}

#[test]
fn a_start_that_fails_while_the_remembered_input_is_there_shows_its_error_with_no_fallback() {
    let mut desk = FakeDesk::stopped();
    desk.saved_audio = Some(saved("wasapi", Some(USB), Some(256), None));
    desk.fail_start = Some("the device is busy".to_owned());
    let mut harness = Harness::connected(desk, Options::default());

    assert_eq!(starts(&harness), [on("wasapi", Some(USB))]);
    assert_eq!(phase(&harness), Phase::Error);
    assert_eq!(
        harness.console.state().engine.error.as_deref(),
        Some("the device is busy")
    );
    harness.desk.fail_start = None;
    start(&mut harness);
    assert_eq!(
        last_start(&harness),
        on("wasapi", Some(USB)),
        "Try again reopens it"
    );
}

#[test]
fn applied_audio_choices_are_saved_once_the_engine_runs_with_them_never_after_a_failed_start() {
    let mut harness = connected();
    harness.desk.fail_start = Some("the driver did not load".to_owned());

    apply(&mut harness, host("wasapi"), None, false);
    assert_eq!(harness.desk.saved_audio, None);

    harness.desk.fail_start = None;
    apply(&mut harness, host("wasapi"), None, false);
    assert_eq!(
        harness.desk.saved_audio,
        Some(saved("wasapi", None, Some(256), None))
    );
    apply(&mut harness, host("asio"), Some(ASIO_DRIVER), false);
    assert_eq!(
        harness.desk.saved_audio,
        Some(saved("asio", None, Some(256), Some(ASIO_DRIVER)))
    );
}

#[test]
fn a_choice_kept_while_nothing_ran_is_saved_once_a_start_runs_it() {
    let options = Options {
        auto_start: false,
        ..Options::default()
    };
    let mut harness = Harness::connected(FakeDesk::stopped(), options);

    harness.console.set_audio(buffer(512.0));
    assert_eq!(harness.desk.saved_audio, None);
    start(&mut harness);

    assert_eq!(
        harness
            .desk
            .saved_audio
            .as_ref()
            .and_then(|audio| audio.buffer_frames),
        Some(512)
    );
}

#[test]
fn a_kept_input_that_could_not_be_read_is_read_again_on_reconnect_and_the_start_opens_it() {
    let mut desk = FakeDesk::stopped();
    desk.saved_audio = Some(saved("wasapi", Some(USB), Some(256), None));
    desk.fail_next("audio_settings", "the session is busy", 1);
    desk.fail_next("engine_status", "the engine is busy", 1);
    let mut harness = Harness::connected(desk, Options::default());
    assert_eq!(
        harness.console.state().connection.status,
        ConnectionStatus::Failed
    );
    assert!(harness.starts().is_empty());

    reconnect(&mut harness);

    assert_eq!(
        starts(&harness),
        [on("wasapi", Some(USB))],
        "never the default in its place"
    );
    assert_eq!(device_name(&harness), USB);
}

#[test]
fn while_the_kept_input_cannot_be_read_nothing_starts_and_the_console_says_why() {
    let mut desk = FakeDesk::stopped();
    desk.saved_audio = Some(saved("wasapi", Some(USB), Some(256), None));
    desk.fail_next("audio_settings", "the session is busy", 1);
    let mut harness = Harness::connected(desk, Options::default());

    assert!(harness.starts().is_empty());
    assert_eq!(
        harness.console.state().connection,
        Connection {
            status: ConnectionStatus::Failed,
            error: Some(
                "The saved audio input could not be read, so nothing was started: the session is busy. \
                 Choose an input in Network and system to start without it."
                    .to_owned()
            ),
        }
    );

    harness
        .desk
        .fail_next("audio_settings", "the session is still busy", 1);
    start(&mut harness);
    assert!(harness.starts().is_empty(), "Start does not guess either");
    let error = harness
        .console
        .state()
        .engine
        .error
        .clone()
        .unwrap_or_default();
    assert!(
        error.contains("could not be read") && error.contains("still busy"),
        "{error}"
    );

    start(&mut harness);
    assert_eq!(starts(&harness), [on("wasapi", Some(USB))]);
}

#[test]
fn when_the_kept_input_can_never_be_read_choosing_an_input_in_network_and_system_starts_the_engine()
{
    let mut desk = FakeDesk::stopped();
    desk.saved_audio = Some(saved("wasapi", Some(USB), Some(256), None));
    desk.fail_next("audio_settings", "the session file is locked", 100);
    let mut harness = Harness::connected(desk, Options::default());
    reconnect(&mut harness);
    start(&mut harness);

    assert!(harness.starts().is_empty(), "nothing starts on a guess");
    let error = harness
        .console
        .state()
        .engine
        .error
        .clone()
        .unwrap_or_default();
    assert!(
        error.contains("Choose an input in Network and system"),
        "{error}"
    );

    switch(&mut harness, INTERFACE);

    let devices: Vec<_> = starts(&harness)
        .into_iter()
        .map(|(_, device)| device)
        .collect();
    assert_eq!(
        devices,
        [Some(INTERFACE.to_owned())],
        "the engineer's choice gets past it"
    );
    assert_eq!(phase(&harness), Phase::Running);
    assert_eq!(
        harness
            .desk
            .saved_audio
            .as_ref()
            .and_then(|audio| audio.device.clone())
            .as_deref(),
        Some(INTERFACE),
        "and is kept for the next start"
    );
}

#[test]
fn a_kept_input_that_cannot_be_read_leaves_a_running_engine_running_and_says_why_start_did_nothing()
{
    let mut desk = FakeDesk::running();
    desk.saved_audio = Some(saved("wasapi", Some(USB), Some(256), None));
    desk.fail_next("audio_settings", "the session file is locked", 100);
    let mut harness = Harness::connected(desk, Options::default());
    assert_eq!(
        phase(&harness),
        Phase::Running,
        "a running engine is shown as it is"
    );
    let running = harness.console.state().engine.run;

    let result = start(&mut harness).expect("an answer");

    assert!(result.is_err());
    assert!(harness.starts().is_empty());
    let state = harness.console.state();
    assert_eq!(state.engine.phase, Phase::Running, "the engine runs on");
    assert_eq!(state.engine.run, running);
    assert_eq!(
        state.command_error,
        Some(CommandError {
            action: CommandAction::Start,
            message: "The saved audio input could not be read, so nothing was started: the session file is \
                      locked. Choose an input in Network and system to start without it"
                .to_owned(),
        })
    );
}

#[test]
fn a_choice_the_engineer_makes_while_the_kept_one_is_unread_is_not_replaced_by_a_later_read() {
    let mut desk = FakeDesk::stopped();
    desk.saved_audio = Some(saved("wasapi", Some(USB), Some(256), None));
    desk.fail_next("audio_settings", "the session is busy", 1);
    let mut harness = Harness::connected(desk, Options::default());

    apply(&mut harness, host("auto"), None, false);
    let reads = harness.calls_to("audio_settings").len();
    reconnect(&mut harness);

    assert_eq!(
        harness.calls_to("audio_settings").len(),
        reads,
        "nothing to read any more"
    );
    assert_eq!(harness.console.state().audio.host, Host::Auto);
    assert_eq!(
        harness
            .desk
            .saved_audio
            .as_ref()
            .map(|audio| audio.host.as_str()),
        Some("auto")
    );
}

#[test]
fn a_choice_the_session_could_not_keep_is_said_and_the_next_start_keeps_it() {
    let mut harness = connected();
    harness
        .desk
        .fail_next("set_audio_settings", "could not save session.json", 1);

    apply(&mut harness, host("wasapi"), None, false);

    let notice = harness
        .console
        .state()
        .audio_notice
        .clone()
        .expect("a word about the input");
    assert_eq!(notice.kind, AudioNoticeKind::Unsaved);
    assert_eq!(
        notice.message,
        "The audio input runs but was not saved for the next start: could not save session.json."
    );
    stop(&mut harness);
    start(&mut harness);
    assert_eq!(
        harness
            .desk
            .saved_audio
            .as_ref()
            .map(|audio| audio.host.as_str()),
        Some("wasapi")
    );
    assert_eq!(harness.console.state().audio_notice, None);
}

#[test]
fn the_window_is_titled_after_the_input_device_and_plainly_when_no_engine_runs() {
    let mut harness = connected();
    stop(&mut harness);

    assert_eq!(
        harness.titles,
        ["BandEar Server \u{2014} Demo interface", "BandEar Server"]
    );
}

#[test]
fn picking_a_musician_opens_their_mix() {
    let harness = connected();

    harness.console.select(BASS);

    assert_eq!(harness.console.state().selected.as_deref(), Some(BASS));
}

#[test]
fn once_disposed_the_console_hears_no_more_events() {
    let mut harness = connected();
    harness.console.dispose();
    let before = volume(&harness, LEAD);

    harness.desk.change_from_phone(LEAD, 5);
    harness.emit();

    assert_eq!(volume(&harness, LEAD), before);
}

#[test]
fn the_console_starts_the_demo_by_its_driver_and_keeps_the_choice() {
    let mut harness = connected();

    let result = apply(&mut harness, host("demo"), None, false);

    assert_eq!(result, Some(Ok(())));
    let state = harness.console.state();
    assert_eq!(state.audio.host, Host::Demo);
    assert!(is_demo_running(state.engine.status.as_ref()));
    assert_eq!(window_title(&state), "BandEar Server \u{2014} Demo source");
}

#[test]
fn the_title_says_demo_source_only_while_the_demo_plays() {
    let mut state = ConsoleState::new();
    assert_eq!(window_title(&state), "BandEar Server");
    state.reduce(&Action::Status(Box::new(sample_status())));
    assert_eq!(
        window_title(&state),
        "BandEar Server \u{2014} Demo interface"
    );
    let mut demo = sample_status();
    demo.status.demo = true;
    demo.status.device_name = crate::demo_source::DEMO_DEVICE.to_owned();
    state.reduce(&Action::Status(Box::new(demo.clone())));
    assert_eq!(window_title(&state), "BandEar Server \u{2014} Demo source");
    demo.status.stopped = true;
    state.reduce(&Action::Status(Box::new(demo)));
    assert_eq!(
        window_title(&state),
        "BandEar Server \u{2014} Demo (no interface)",
        "a stopped engine plays nothing"
    );
}

#[test]
fn desk_turns_go_one_at_a_time_and_count_as_edits_until_they_are_over() {
    let mut harness = connected();
    harness.hold("channel_table");
    let order = Rc::new(RefCell::new(Vec::new()));
    for name in ["first", "second"] {
        let console = harness.console.clone();
        let seen = Rc::clone(&order);
        let turn = harness.console.in_turn(true, async move {
            seen.borrow_mut().push(format!("{name} begins"));
            drop(console.run(Command::ChannelTable).await);
            seen.borrow_mut().push(format!("{name} ends"));
        });
        harness.console.spawn(turn);
    }
    harness.settle();
    assert_eq!(
        *order.borrow(),
        ["first begins"],
        "the second waits for the first's answer"
    );
    assert_eq!(harness.console.unsaved_edits(), 2);

    harness.release();
    harness.release();
    assert_eq!(
        *order.borrow(),
        ["first begins", "first ends", "second begins", "second ends"]
    );
    assert_eq!(harness.console.unsaved_edits(), 0);
}

#[test]
fn a_hold_keeps_sends_back_until_it_is_released_and_says_so() {
    let harness = connected();
    let released = Rc::new(RefCell::new(0));
    let counted = Rc::clone(&released);
    harness.console.on_sends_released(move |console| {
        assert!(
            !console.holding(),
            "the hold is over when the waiting edits go out"
        );
        *counted.borrow_mut() += 1;
    });

    harness.console.hold_sends();
    assert!(harness.console.holding());
    harness.console.release_sends();

    assert!(!harness.console.holding());
    assert_eq!(*released.borrow(), 1);
}

#[test]
fn a_store_listener_hears_each_change_once_and_a_dispatch_from_it_waits_its_turn() {
    let harness = connected();
    let heard = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&heard);
    harness.console.subscribe(move |console, action| {
        if let Action::Select(address) = action {
            seen.borrow_mut().push(address.clone());
            if address == BASS {
                console.dispatch(Action::Select(LEAD.to_owned()));
                seen.borrow_mut()
                    .push("after the nested dispatch".to_owned());
            }
        }
    });

    harness.console.select(BASS);
    assert!(
        !harness.console.dispatch(Action::Select(LEAD.to_owned())),
        "unchanged: nobody is told"
    );

    assert_eq!(
        *heard.borrow(),
        [
            BASS.to_owned(),
            "after the nested dispatch".to_owned(),
            LEAD.to_owned()
        ]
    );
}
