//! The model's flows for the audio input and the phones, over the console's simulated desk
//! (`network-screen.js` behaviour, and the parts of `console-app.test.js` and
//! `network-view.test.js` that run through the console).

use bandear_desktop_console::command::{Answer, Command, Owner};
use bandear_desktop_console::console::{Options, RECORDING_BLOCKS_SWITCH};
use bandear_desktop_console::engine_config::Host;
use bandear_desktop_console::router::Section;
use bandear_desktop_console::testing::{FakeDesk, Harness, USB};

use super::*;

const BASS: &str = "192.168.1.12:50000";
const KEYS: &str = "192.168.1.17:50000";
const DRUMS: &str = "192.168.1.20:50000";
const DEMO: &str = "Demo interface";

/// A console connected to `desk`, and the screen's model over it.
fn network(desk: FakeDesk) -> (Harness, Model) {
    let harness = Harness::connected(desk, Options::default());
    let model = Model::new(harness.console.for_owner(Owner::Screen(Section::Network)));
    model.set_zone(Zone::Fixed(0));
    (harness, model)
}

fn names(harness: &Harness, from: usize) -> Vec<&'static str> {
    harness.calls[from..].iter().map(Command::name).collect()
}

fn row<'a>(view: &'a ScreenView, address: &str) -> &'a ShownRow {
    view.table
        .rows
        .iter()
        .find(|row| row.row.address == address)
        .expect("the row")
}

#[test]
fn the_model_sends_its_commands_for_its_screen() {
    let console = Console::new(Options::default()).for_owner(Owner::Screen(Section::Network));
    let model = Model::new(console);
    assert_eq!(model.console().owner(), Owner::Screen(Section::Network));
}

// ---- The audio input --------------------------------------------------------------------------

#[test]
fn shown_the_screen_lists_the_devices_of_the_applied_driver_and_the_mix_backups() {
    let (mut harness, model) = network(FakeDesk::running());
    let from = harness.calls.len();
    model.mount();
    harness.settle();
    let sent = names(&harness, from);
    assert!(sent.contains(&"list_input_devices"));
    assert!(sent.contains(&"list_mix_backups"));
    assert!(matches!(
        harness.calls_to("list_input_devices").last(),
        Some(Command::ListInputDevices { host: Some(host) }) if host == "auto"
    ));
    let view = model.screen();
    assert!(!view.audio.listing, "the applied driver's devices are in");
    assert_eq!(view.audio.devices.len(), 3);
}

#[test]
fn a_driver_change_restarts_on_its_default_input_keeps_the_choice_and_lists_its_devices() {
    let (mut harness, model) = network(FakeDesk::running());
    let from = harness.calls.len();
    model.choose_host(Host::Wasapi);
    harness.settle();

    assert_eq!(harness.starts().last(), Some(&("wasapi".to_owned(), None)));
    let sent = names(&harness, from);
    assert_eq!(sent.first(), Some(&"start_engine"));
    assert!(
        sent.contains(&"set_audio_settings"),
        "kept once it runs: {sent:?}"
    );
    assert_eq!(sent.last(), Some(&"list_input_devices"));
    assert_eq!(harness.console.state().audio.host, Host::Wasapi);
    assert_eq!(model.screen().audio.host, Host::Wasapi);

    let from = harness.calls.len();
    model.choose_host(Host::Wasapi);
    harness.settle();
    assert_eq!(
        names(&harness, from),
        ["list_input_devices"],
        "the driver applied already restarts nothing"
    );
}

#[test]
fn choosing_asio_lists_its_drivers_and_starts_nothing_until_one_is_picked_and_started() {
    let (mut harness, model) = network(FakeDesk::running());
    let starts = harness.starts().len();
    model.choose_host(Host::Asio);
    harness.settle();

    assert_eq!(
        harness.starts().len(),
        starts,
        "ASIO is never started on its own"
    );
    let view = model.screen();
    assert_eq!(view.audio.host, Host::Asio);
    assert!(view.audio.asio_pending);
    assert_eq!(view.audio.device, "");
    assert!(!view.audio.can_apply_asio);
    assert_eq!(
        harness.console.state().audio.host,
        Host::Auto,
        "nothing applied"
    );

    model.choose_device("Demo ASIO driver");
    assert_eq!(harness.starts().len(), starts, "picking is not starting");
    assert!(model.screen().audio.can_apply_asio);
    model.apply_asio();
    harness.settle();

    assert_eq!(
        harness.starts().last(),
        Some(&("asio".to_owned(), Some("Demo ASIO driver".to_owned())))
    );
    let audio = harness.console.state().audio.clone();
    assert_eq!(audio.host, Host::Asio);
    assert_eq!(audio.asio_driver.as_deref(), Some("Demo ASIO driver"));
    let view = model.screen();
    assert!(!view.audio.asio_pending, "the draft is over");
    assert_eq!(view.audio.device, "Demo ASIO driver");
    assert!(matches!(
        harness.calls_to("list_input_devices").last(),
        Some(Command::ListInputDevices { host: Some(host) }) if host == "asio"
    ));

    model.choose_host(Host::Auto);
    harness.settle();
    assert_eq!(
        harness.starts().last(),
        Some(&("auto".to_owned(), None)),
        "back to the safe driver"
    );
}

#[test]
fn a_device_switch_is_live_under_wasapi_and_a_restart_on_that_device_otherwise() {
    // No group to lose: every switch goes ahead without asking.
    let mut desk = FakeDesk::running();
    desk.report.status.groups.clear();
    let (mut harness, model) = network(desk);
    model.mount();
    harness.settle();
    let starts = harness.starts().len();
    model.choose_device("Digital input");
    harness.settle();
    assert_eq!(
        harness.starts()[starts..],
        [("auto".to_owned(), Some("Digital input".to_owned()))],
        "auto restarts on the device"
    );

    model.choose_host(Host::Wasapi);
    harness.settle();
    let starts = harness.starts().len();
    model.choose_device("Digital input");
    harness.settle();
    assert_eq!(harness.starts().len(), starts, "WASAPI switches live");
    assert!(matches!(
        harness.calls_to("switch_device").last(),
        Some(Command::SwitchDevice { device }) if device == "Digital input"
    ));
}

#[test]
fn a_switch_that_leaves_groups_without_inputs_asks_first_and_switch_anyway_switches() {
    let (mut harness, model) = network(FakeDesk::running());
    model.mount();
    harness.settle();
    let starts = harness.starts().len();

    model.choose_device(USB);
    let view = model.screen();
    assert_eq!(
        view.audio.warning.as_deref(),
        Some(
            "Switching to Microphone (USB audio) (2 ch) makes 2 groups unavailable: Drums, Vocals. They are kept, not deleted, and become available again if you switch back."
        )
    );
    assert_eq!(
        harness.starts().len(),
        starts,
        "nothing switches while it asks"
    );
    model.cancel_switch();
    assert_eq!(model.screen().audio.warning, None);

    model.choose_device(USB);
    model.confirm_switch();
    harness.settle();
    assert_eq!(
        harness.starts().last(),
        Some(&("auto".to_owned(), Some(USB.to_owned())))
    );
    assert_eq!(model.screen().audio.warning, None);

    let starts = harness.starts().len();
    model.choose_device(USB);
    harness.settle();
    assert_eq!(
        harness.starts().len(),
        starts,
        "the device in use does nothing"
    );
}

#[test]
fn a_refused_switch_is_said_over_the_screen() {
    let mut desk = FakeDesk::running();
    desk.saved_audio = Some(bandear_desktop_console::types::SessionAudio {
        host: "wasapi".to_owned(),
        device: None,
        buffer_frames: Some(256),
        asio_driver: None,
    });
    desk.host = "wasapi".to_owned();
    desk.report.status.groups.clear();
    desk.fail_next("switch_device", "the device is busy", 1);
    let (mut harness, model) = network(desk);
    harness
        .console
        .spawn(harness.console.load_devices(Host::Wasapi));
    harness.settle();

    model.choose_device("Digital input");
    harness.settle();
    assert_eq!(
        model.screen().command.as_deref(),
        Some("The input device could not be switched: the device is busy.")
    );
}

#[test]
fn a_buffer_change_restarts_on_the_device_in_use_and_with_no_engine_is_kept_for_the_next_start() {
    let (mut harness, model) = network(FakeDesk::running());
    model.choose_buffer(512);
    harness.settle();
    assert_eq!(
        harness.starts().last(),
        Some(&("auto".to_owned(), Some(DEMO.to_owned()))),
        "on the device in use"
    );
    assert_eq!(harness.console.state().audio.buffer_frames, Some(512));

    let mut idle = Harness::connected(
        FakeDesk::stopped(),
        Options {
            auto_start: false,
            ..Options::default()
        },
    );
    let model = Model::new(idle.console.clone());
    let starts = idle.starts().len();
    let from = idle.calls.len();
    model.choose_buffer(1024);
    idle.settle();
    assert_eq!(idle.starts().len(), starts);
    assert!(
        names(&idle, from).is_empty(),
        "no command: kept for the next start"
    );
    assert_eq!(idle.console.state().audio.buffer_frames, Some(1024));
}

#[test]
fn while_a_take_records_the_driver_cannot_change_and_the_console_says_why() {
    let mut desk = FakeDesk::running();
    desk.report.status.recording.state = "recording".to_owned();
    let (mut harness, model) = network(desk);
    let starts = harness.starts().len();
    model.choose_host(Host::Wasapi);
    harness.settle();
    assert_eq!(harness.starts().len(), starts);
    assert_eq!(
        model.screen().command,
        Some(format!(
            "The input device could not be switched: {RECORDING_BLOCKS_SWITCH}."
        ))
    );
}

#[test]
fn the_screen_notes_a_missing_engine_and_shows_the_engines_numbers_while_one_runs() {
    let (_, model) = network(FakeDesk::running());
    let view = model.screen();
    assert!(view.engine_running);
    assert_eq!(view.table.rows.len(), 4);
    assert_eq!(view.audio.readouts.control_port, "50001");

    let harness = Harness::connected(
        FakeDesk::stopped(),
        Options {
            auto_start: false,
            ..Options::default()
        },
    );
    let model = Model::new(harness.console.clone());
    let view = model.screen();
    assert!(!view.engine_running, "the note shows");
    assert!(view.table.rows.is_empty());
}

// ---- The phones -------------------------------------------------------------------------------

/// A desk whose phones can be added, removed and named, as the desktop's commands do.
fn with_targets(mut desk: FakeDesk) -> FakeDesk {
    desk.script("add_target", |desk, command| {
        let Command::AddTarget { address } = command else {
            return Err("not an add".to_owned());
        };
        let address = if address.contains(':') {
            address.clone()
        } else {
            format!("{address}:50000")
        };
        let ip = address.split(':').next().unwrap_or_default().to_owned();
        if desk
            .report
            .musicians
            .iter()
            .any(|musician| musician.address.starts_with(&format!("{ip}:")))
        {
            return Err(format!("{ip} is already listed"));
        }
        let mut row = desk.report.musicians[0].clone();
        row.address = address.clone();
        row.client_id = None;
        row.client_name = None;
        row.name = None;
        desk.report.musicians.push(row);
        Ok(Answer::Address(address))
    });
    desk.script("remove_target", |desk, command| {
        let Command::RemoveTarget { address } = command else {
            return Err("not a removal".to_owned());
        };
        let before = desk.report.musicians.len();
        desk.report
            .musicians
            .retain(|musician| &musician.address != address);
        if desk.report.musicians.len() == before {
            return Err(format!("{address} is not a target"));
        }
        Ok(Answer::Address(address.clone()))
    });
    desk.script("set_musician_name", |desk, command| {
        let Command::SetMusicianName { address, name } = command else {
            return Err("not a name".to_owned());
        };
        let musician = desk
            .report
            .musicians
            .iter_mut()
            .find(|musician| &musician.address == address)
            .ok_or_else(|| format!("no musician at {address}"))?;
        musician.name = Some(name.clone()).filter(|name| !name.is_empty());
        musician.name_is_local = true;
        Ok(Answer::Done)
    });
    desk
}

#[test]
fn adding_removing_and_renaming_musicians_answer_with_the_result_and_the_list_follows() {
    let (mut harness, model) = network(with_targets(FakeDesk::running()));
    let count = harness.console.state().musicians.len();

    model.add_phone("192.168.1.40");
    harness.settle();
    assert_eq!(
        harness.console.state().musicians.len(),
        count + 1,
        "the status was read again"
    );
    let view = model.screen();
    assert_eq!(view.table.add_error, None);
    assert_eq!(view.cleared.address, 1, "the field empties");

    model.add_phone("192.168.1.40:50001");
    harness.settle();
    assert_eq!(
        model.screen().table.add_error.as_deref(),
        Some("Could not add 192.168.1.40:50001: 192.168.1.40 is already listed")
    );
    assert_eq!(model.screen().cleared.address, 1);

    model.rename("192.168.1.40:50000", "  Keys 2 ");
    harness.settle();
    assert!(matches!(
        harness.calls_to("set_musician_name").last(),
        Some(Command::SetMusicianName { name, .. }) if name == "Keys 2"
    ));
    let added = harness
        .console
        .state()
        .musicians
        .last()
        .cloned()
        .expect("a musician");
    assert_eq!(added.name.as_deref(), Some("Keys 2"));
    assert!(added.name_is_local);

    model.arm_remove("192.168.1.40:50000");
    harness.settle();
    harness.fire_timers();
    model.confirm_remove("192.168.1.40:50000");
    harness.settle();
    assert_eq!(harness.console.state().musicians.len(), count);
    assert_eq!(model.screen().table.table_error, None);
}

#[test]
fn an_address_is_checked_before_anything_is_sent() {
    let (mut harness, model) = network(with_targets(FakeDesk::running()));
    let from = harness.calls.len();
    model.add_phone("192.168.1.300");
    harness.settle();
    assert!(names(&harness, from).is_empty());
    assert_eq!(
        model.screen().table.add_error.as_deref(),
        Some("192.168.1.300 is not an IP address: each part goes up to 255.")
    );
}

#[test]
fn a_refused_name_or_removal_is_said_over_the_table() {
    let mut desk = with_targets(FakeDesk::running());
    desk.fail_next("set_musician_name", "the name is taken", 1);
    desk.fail_next("remove_target", "the engine is stopping", 1);
    let (mut harness, model) = network(desk);
    model.rename(DRUMS, "Kit");
    harness.settle();
    assert_eq!(
        model.screen().table.table_error.as_deref(),
        Some("Could not save the name: the name is taken")
    );
    model.arm_remove(DRUMS);
    harness.settle();
    harness.fire_timers();
    model.confirm_remove(DRUMS);
    harness.settle();
    assert_eq!(
        model.screen().table.table_error.as_deref(),
        Some("Could not remove 192.168.1.20:50000: the engine is stopping")
    );
    model.rename(DRUMS, "Kit");
    harness.settle();
    assert_eq!(
        model.screen().table.table_error,
        None,
        "the next good answer clears it"
    );
}

#[test]
fn a_removal_asks_first_works_only_after_a_pause_and_disarms_itself() {
    let (mut harness, model) = network(with_targets(FakeDesk::running()));
    model.arm_remove(BASS);
    assert_eq!(row(&model.screen(), BASS).removing, Some(false));
    model.confirm_remove(BASS);
    harness.settle();
    assert!(
        harness.calls_to("remove_target").is_empty(),
        "a double click keeps the musician"
    );

    harness.fire_timers();
    assert_eq!(
        row(&model.screen(), BASS).removing,
        Some(true),
        "ready after the pause"
    );
    harness.fire_timers();
    assert_eq!(
        row(&model.screen(), BASS).removing,
        None,
        "it disarms itself"
    );

    model.arm_remove(BASS);
    model.arm_remove(KEYS);
    let view = model.screen();
    assert_eq!(
        row(&view, BASS).removing,
        None,
        "arming another disarms the first"
    );
    assert_eq!(row(&view, KEYS).removing, Some(false));
    model.keep_remove();
    assert_eq!(row(&model.screen(), KEYS).removing, None, "Keep");

    model.arm_remove(KEYS);
    harness
        .desk
        .report
        .musicians
        .retain(|musician| musician.address != KEYS);
    harness.desk.emit_status();
    harness.emit();
    model.screen();
    assert_eq!(
        model.shared.local.borrow().remove.armed(),
        None,
        "a row leaving the list disarms"
    );
}

#[test]
fn a_musicians_label_stays_on_screen_when_their_phone_comes_back_from_a_new_ip() {
    let (mut harness, model) = network(with_targets(FakeDesk::running()));
    model.rename(DRUMS, "Drum kit");
    harness.settle();
    let moved = "192.168.1.77:50000";
    harness.desk.report.musicians[3].address = moved.to_owned();
    harness.desk.emit_status();
    harness.emit();

    let view = model.screen();
    assert!(!view.table.rows.iter().any(|row| row.row.address == DRUMS));
    let row = &row(&view, moved).row;
    assert_eq!(row.name, "Drum kit");
    assert!(row.name_is_local);
    assert_eq!(row.label_follows, musicians::LabelFollows::Phone);
}

#[test]
fn the_screen_asks_to_be_drawn_again_when_its_own_state_changes() {
    let (mut harness, model) = network(with_targets(FakeDesk::running()));
    let draws = Rc::new(std::cell::Cell::new(0));
    let counted = Rc::clone(&draws);
    model.on_change(move || counted.set(counted.get() + 1));
    model.add_phone("");
    assert!(draws.get() > 0, "an error said at once");
    let before = draws.get();
    model.arm_remove(BASS);
    harness.settle();
    harness.fire_timers();
    assert!(
        draws.get() >= before + 2,
        "armed, then ready after the pause"
    );
}
