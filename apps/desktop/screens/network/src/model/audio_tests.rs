//! `network-view.test.js` (the audio setup), the audio-setup part of `demo-source.test.js` and the
//! Local mix out part of `local-mix.test.js`, ported.

use bandear_desktop_console::console::window_title;
use bandear_desktop_console::demo_source::demo_devices;
use bandear_desktop_console::engine_config::AudioChange;
use bandear_desktop_console::state::{Action, CommandError};
use bandear_desktop_console::testing::{FakeDesk, sample_status};
use bandear_desktop_console::types::SessionAudio;
use bandear_desktop_console::wire::StatusReport;

use super::*;

const DEMO: &str = "Demo interface";
const SPEAKERS: &str = "Speakers (Realtek Audio)";

fn running() -> ConsoleState {
    with(sample_status())
}

fn with(report: StatusReport) -> ConsoleState {
    let mut state = ConsoleState::new();
    state.reduce(&Action::Status(Box::new(report)));
    state
}

fn sample_devices() -> Vec<InputDeviceDto> {
    FakeDesk::devices("auto")
        .into_iter()
        .map(|(name, channels)| InputDeviceDto {
            name: name.to_owned(),
            channels,
        })
        .collect()
}

fn with_devices(mut state: ConsoleState, host: Host, list: Vec<InputDeviceDto>) -> ConsoleState {
    state.reduce(&Action::Devices {
        host,
        list,
        error: None,
    });
    state
}

fn no_draft() -> Draft {
    Draft::default()
}

#[test]
fn the_driver_choices_put_the_safe_one_first_and_say_what_asio_risks() {
    let hosts: Vec<_> = host_options().into_iter().map(|(host, _)| host).collect();
    assert_eq!(
        hosts,
        [Host::Auto, Host::Wasapi, Host::Asio, Host::Demo],
        "the demo source last, never in the way of an interface"
    );
    assert!(host_options()[0].1.contains("safe"));
    assert!(host_options()[2].1.contains("risky"));
    assert_eq!(host_options()[3].1, "Demo (no interface)");
    let buffers: Vec<_> = BUFFER_OPTIONS.iter().map(|(frames, _)| *frames).collect();
    assert_eq!(buffers, [128, 256, 512, 1024]);
}

#[test]
fn devices_are_listed_with_their_channel_count_or_as_unknown() {
    let device = |name: &str, channels| InputDeviceDto {
        name: name.to_owned(),
        channels,
    };
    let labels: Vec<_> = device_options(&[
        device("Volt 4", 4),
        device("Mic", 1),
        device("ASIO driver", 0),
    ])
    .into_iter()
    .map(|choice| choice.label)
    .collect();
    assert_eq!(
        labels,
        [
            "Volt 4 \u{2014} 4 channels",
            "Mic \u{2014} 1 channel",
            "ASIO driver \u{2014} channels unknown"
        ]
    );
}

#[test]
fn a_device_switch_that_would_leave_groups_without_inputs_asks_first() {
    let state = running();
    let devices = sample_devices();
    assert_eq!(device_switch_plan(&state, &devices, DEMO), SwitchPlan::None);
    assert_eq!(
        device_switch_plan(&state, &devices, "Nowhere"),
        SwitchPlan::None
    );
    assert_eq!(device_switch_plan(&state, &devices, ""), SwitchPlan::None);
    let mut digital = devices.clone();
    digital[2].channels = 6;
    assert_eq!(
        device_switch_plan(&state, &digital, "Digital input"),
        SwitchPlan::Switch {
            name: "Digital input".to_owned()
        }
    );
    assert_eq!(
        device_switch_plan(&state, &devices, "Microphone (USB audio)"),
        SwitchPlan::Confirm {
            name: "Microphone (USB audio)".to_owned(),
            message: "Switching to Microphone (USB audio) (2 ch) makes 2 groups unavailable: Drums, Vocals. They are kept, not deleted, and become available again if you switch back.".to_owned(),
        }
    );
    let mut unknown = devices;
    unknown[1].channels = 0;
    assert_eq!(
        device_switch_plan(&state, &unknown, "Microphone (USB audio)"),
        SwitchPlan::Switch {
            name: "Microphone (USB audio)".to_owned()
        },
        "an unknown channel count never asks"
    );
}

#[test]
fn only_wasapi_switches_a_device_live_every_other_driver_restarts() {
    let audio = |host| Audio {
        host,
        ..ConsoleState::new().audio
    };
    assert!(switches_live(&audio(Host::Wasapi)));
    for host in [Host::Auto, Host::Asio, Host::Demo] {
        assert!(!switches_live(&audio(host)), "{host:?}");
    }
}

#[test]
fn the_audio_setup_shows_the_applied_choices_the_device_and_the_engines_numbers() {
    let view = audio_setup_view(
        &with_devices(running(), Host::Auto, sample_devices()),
        &no_draft(),
    );
    assert_eq!(view.host, Host::Auto);
    assert!(!view.asio_pending);
    assert_eq!(view.device, DEMO);
    assert!(!view.device_disabled);
    assert_eq!(view.buffer, Some(256));
    assert!(!view.can_apply_asio);
    assert_eq!(
        view.readouts,
        Readouts {
            format: "6 ch \u{b7} 48 kHz \u{b7} 24-bit \u{b7} 256".to_owned(),
            sample_rate: "48,000 Hz".to_owned(),
            control_port: "50001".to_owned(),
            samples_received: "0".to_owned(),
            packets: "0 / 0".to_owned(),
        }
    );
    assert_eq!(
        view.device_choices, view.devices,
        "no extra line while devices are listed"
    );
}

#[test]
fn the_engines_load_shows_its_average_and_peak_and_warns_once_either_passes_70() {
    assert_eq!(
        audio_setup_view(&running(), &no_draft()).load,
        LoadView {
            text: "8% (peak 15%)".to_owned(),
            warning: false
        }
    );
    let load = |average: f32, peak: f32, late: u64| {
        let mut status = sample_status().status;
        status.load.average_percent = average;
        status.load.peak_percent = peak;
        status.load.late_blocks = late;
        let view = load_view(Some(&status));
        (view.text, view.warning)
    };
    assert_eq!(LOAD_WARNING_PERCENT, 70.0);
    assert_eq!(load(70.0, 70.0, 0), ("70% (peak 70%)".to_owned(), false));
    assert_eq!(load(40.0, 82.0, 0), ("40% (peak 82%)".to_owned(), true));
    assert_eq!(load(71.2, 75.0, 0), ("71% (peak 75%)".to_owned(), true));
    // The warning follows the numbers shown: 70.4% reads "70%", like 70%, and does not warn.
    assert_eq!(load(70.4, 70.4, 0), ("70% (peak 70%)".to_owned(), false));
    assert_eq!(load(40.0, 70.4, 0), ("40% (peak 70%)".to_owned(), false));
    assert_eq!(load(70.5, 40.0, 0), ("71% (peak 40%)".to_owned(), true));
    assert_eq!(load(40.0, 70.5, 0), ("40% (peak 71%)".to_owned(), true));
    assert_eq!(
        load(55.0, 130.0, 1),
        ("55% (peak 130%) \u{b7} 1 late block".to_owned(), true)
    );
    assert_eq!(
        load(55.0, 130.0, 1200).0,
        "55% (peak 130%) \u{b7} 1,200 late blocks"
    );
}

#[test]
fn with_no_engine_the_load_is_a_dash() {
    assert_eq!(
        load_view(None),
        LoadView {
            text: DASH.to_owned(),
            warning: false
        }
    );
    assert_eq!(
        audio_setup_view(&ConsoleState::new(), &no_draft())
            .load
            .text,
        DASH
    );
}

#[test]
fn choosing_asio_waits_for_an_explicit_driver_before_anything_starts() {
    let drivers = with_devices(
        running(),
        Host::Asio,
        vec![InputDeviceDto {
            name: "Volt ASIO".to_owned(),
            channels: 0,
        }],
    );
    let drafting = audio_setup_view(
        &drivers,
        &Draft {
            host: Some(Host::Asio),
            ..Draft::default()
        },
    );
    assert_eq!(drafting.host, Host::Asio);
    assert!(drafting.asio_pending);
    assert_eq!(drafting.device, "", "no driver is picked for the engineer");
    assert!(!drafting.can_apply_asio);
    assert_eq!(
        drafting.device_choices[0],
        Choice::new("", "Choose an ASIO driver\u{2026}")
    );

    let picked = audio_setup_view(
        &drivers,
        &Draft {
            host: Some(Host::Asio),
            device: Some("Volt ASIO".to_owned()),
            pending_switch: None,
        },
    );
    assert_eq!(picked.device, "Volt ASIO");
    assert!(picked.can_apply_asio);
}

#[test]
fn devices_listed_for_another_driver_are_never_offered_under_the_one_shown() {
    let stale = with_devices(running(), Host::Auto, sample_devices());
    let view = audio_setup_view(
        &stale,
        &Draft {
            host: Some(Host::Asio),
            ..Draft::default()
        },
    );
    assert!(view.devices.is_empty());
    assert!(view.listing);
    assert!(view.device_disabled);
    assert_eq!(
        view.device_choices,
        [Choice::new("", "Listing the devices\u{2026}")]
    );
}

#[test]
fn with_no_engine_the_device_cannot_be_switched_and_the_numbers_are_blank() {
    let view = audio_setup_view(
        &with_devices(ConsoleState::new(), Host::Auto, sample_devices()),
        &no_draft(),
    );
    assert!(view.device_disabled);
    assert_eq!(view.readouts.format, DASH);
    let empty = audio_setup_view(
        &with_devices(running(), Host::Auto, Vec::new()),
        &no_draft(),
    );
    assert!(empty.device_disabled, "nothing to pick");
    assert_eq!(empty.device_choices, [Choice::new("", "no input devices")]);
}

#[test]
fn while_a_take_records_the_driver_device_and_buffer_are_locked_and_the_setup_says_why() {
    let free = audio_setup_view(
        &with_devices(running(), Host::Auto, sample_devices()),
        &no_draft(),
    );
    assert_eq!(free.recording_lock, None);
    assert!(!free.host_disabled);
    assert!(!free.buffer_disabled);

    let mut report = sample_status();
    report.status.recording.state = "recording".to_owned();
    let view = audio_setup_view(
        &with_devices(with(report.clone()), Host::Auto, sample_devices()),
        &no_draft(),
    );
    assert_eq!(view.recording_lock, Some(RECORDING_BLOCKS_SWITCH));
    assert!(view.host_disabled && view.device_disabled && view.buffer_disabled);

    report.status.recording.state = "stopping".to_owned();
    assert_eq!(
        audio_setup_view(&with(report), &no_draft()).recording_lock,
        Some(RECORDING_BLOCKS_SWITCH)
    );

    let mut soundcheck = sample_status();
    soundcheck.status.soundcheck.state = "playing".to_owned();
    let view = audio_setup_view(&with(soundcheck), &no_draft());
    assert_eq!(view.recording_lock, Some(SOUNDCHECK_BLOCKS_SWITCH));
    assert!(view.host_disabled && view.buffer_disabled);
    assert!(
        !audio_setup_view(
            &with(sample_status()),
            &Draft {
                host: Some(Host::Asio),
                device: Some("Volt ASIO".to_owned()),
                pending_switch: None
            }
        )
        .recording_lock
        .is_some()
    );
}

fn audio(state: &mut ConsoleState, saved: SessionAudio) {
    state.reduce(&Action::Audio(AudioChange {
        host: Some(saved.host),
        device: Some(saved.device),
        buffer_frames: Some(saved.buffer_frames.map(f64::from)),
        asio_driver: Some(saved.asio_driver),
    }));
}

#[test]
fn the_audio_selects_show_the_remembered_choice_and_the_running_device_while_one_runs() {
    let volt = SessionAudio {
        host: "wasapi".to_owned(),
        device: Some("Microphone (USB audio)".to_owned()),
        buffer_frames: Some(512),
        asio_driver: None,
    };
    let mut idle = ConsoleState::new();
    audio(&mut idle, volt.clone());
    let idle = with_devices(idle, Host::Wasapi, sample_devices());
    let view = audio_setup_view(&idle, &no_draft());
    assert_eq!(view.host, Host::Wasapi);
    assert_eq!(
        view.device, "Microphone (USB audio)",
        "the input the next start opens"
    );
    assert_eq!(view.buffer, Some(512));

    let mut asio = ConsoleState::new();
    audio(
        &mut asio,
        SessionAudio {
            host: "asio".to_owned(),
            device: None,
            buffer_frames: Some(256),
            asio_driver: Some("Volt ASIO".to_owned()),
        },
    );
    assert_eq!(
        audio_setup_view(&asio, &no_draft()).device,
        "Volt ASIO",
        "under ASIO, its driver"
    );

    let mut on_default = ConsoleState::new();
    audio(&mut on_default, volt);
    on_default.reduce(&Action::Status(Box::new(sample_status())));
    assert_eq!(audio_setup_view(&on_default, &no_draft()).device, DEMO);
}

#[test]
fn the_audio_setup_lists_the_demos_one_device_under_its_driver() {
    let mut report = sample_status();
    report.status.device_name = "Demo (no interface)".to_owned();
    report.status.capture_format.channels = 8;
    report.status.demo = true;
    let mut state = with(report);
    state.reduce(&Action::Audio(AudioChange {
        host: Some("demo".to_owned()),
        device: Some(Some("Demo (no interface)".to_owned())),
        ..AudioChange::default()
    }));
    let state = with_devices(state, Host::Demo, demo_devices());
    let view = audio_setup_view(&state, &no_draft());
    assert_eq!(view.host, Host::Demo);
    assert_eq!(view.device, "Demo (no interface)");
    assert_eq!(
        view.devices,
        [Choice::new(
            "Demo (no interface)",
            "Demo (no interface) \u{2014} 8 channels"
        )]
    );
    assert!(
        view.readouts.format.starts_with("8 ch"),
        "{}",
        view.readouts.format
    );
    assert_eq!(window_title(&state), "BandEar Server \u{2014} Demo source");
}

#[test]
fn the_window_is_titled_after_the_input_device_as_before() {
    assert_eq!(
        window_title(&running()),
        "BandEar Server \u{2014} Demo interface"
    );
    assert_eq!(window_title(&ConsoleState::new()), "BandEar Server");
}

#[test]
fn a_refused_command_is_said_with_the_engines_words() {
    let mut state = running();
    assert_eq!(command_message(&state), None);
    for (action, said) in [
        (
            CommandAction::Stop,
            "The engine could not be stopped: busy.",
        ),
        (
            CommandAction::Start,
            "The engine could not be started: busy.",
        ),
        (
            CommandAction::Switch,
            "The input device could not be switched: busy.",
        ),
    ] {
        state.command_error = Some(CommandError {
            action,
            message: "busy".to_owned(),
        });
        assert_eq!(command_message(&state).as_deref(), Some(said));
    }
}

// ---- The local mix's output -----------------------------------------------------------------------

fn sample_outputs() -> Vec<OutputDeviceDto> {
    vec![
        OutputDeviceDto {
            name: DEMO.to_owned(),
            channels: 4,
        },
        OutputDeviceDto {
            name: SPEAKERS.to_owned(),
            channels: 2,
        },
    ]
}

fn playing_demo() -> LocalOutput {
    LocalOutput {
        device: Some(DEMO.to_owned()),
        first_channel: Some(2),
        state: "running".to_owned(),
        reason: None,
        underrun_frames: 0,
        dropped_frames: 0,
        notice: None,
    }
}

fn off() -> LocalOutput {
    LocalOutput {
        state: "off".to_owned(),
        ..LocalOutput::default()
    }
}

fn with_output(output: LocalOutput, listed: Option<Vec<OutputDeviceDto>>) -> ConsoleState {
    let mut report = sample_status();
    report.status.local_output = Some(output);
    let mut state = with(report);
    if let Some(list) = listed {
        state.reduce(&Action::Outputs { list, run: Some(1) });
    }
    state
}

#[test]
fn the_local_mix_out_offers_none_first_then_each_stereo_pair_of_every_output_device() {
    assert_eq!(
        output_options(&sample_outputs()),
        [
            Choice::new("", "None (lowest load)"),
            Choice::new(output_value(DEMO, 0), "Demo interface \u{b7} outputs 1/2"),
            Choice::new(output_value(DEMO, 2), "Demo interface \u{b7} outputs 3/4"),
            Choice::new(
                output_value(SPEAKERS, 0),
                "Speakers (Realtek Audio) \u{b7} outputs 1/2"
            ),
        ]
    );
    let device = |name: &str, channels| OutputDeviceDto {
        name: name.to_owned(),
        channels,
    };
    let labels: Vec<_> = output_options(&[
        device("Mono out", 1),
        device("Odd out", 3),
        device("Not at 48 kHz", 0),
    ])
    .into_iter()
    .map(|choice| choice.label)
    .collect();
    assert_eq!(
        labels,
        ["None (lowest load)", "Odd out \u{b7} outputs 1/2"],
        "only whole pairs"
    );
    assert_eq!(output_label("Volt 4", 0), "Volt 4 \u{b7} outputs 1/2");
}

#[test]
fn a_choice_reads_back_from_the_selects_value_whatever_the_device_is_called() {
    for (device, first_channel) in [(DEMO, 2), ("Out | A (B)", 0)] {
        assert_eq!(
            parse_output_value(&output_value(device, first_channel)),
            Some(LocalOutputRequestDto {
                device: device.to_owned(),
                first_channel
            })
        );
    }
    assert_eq!(parse_output_value(""), None, "None");
    assert_eq!(parse_output_value("not a choice"), None);
    assert_eq!(parse_output_value("-2|Demo"), None);
    assert_eq!(parse_output_value("|Demo"), None);
    assert_eq!(parse_output_value("2|"), None);
}

#[test]
fn with_an_engine_the_select_shows_the_outputs_listed_and_the_one_in_use() {
    let view = local_output_view(&with_output(playing_demo(), Some(sample_outputs())));
    assert_eq!(view.value, output_value(DEMO, 2));
    assert!(!view.disabled);
    assert_eq!(view.options.len(), 4);
    assert_eq!(view.hint, LOCAL_OUTPUT_HINT);
    assert_eq!(view.error, None);
    assert_eq!(
        view.state,
        OutputState {
            kind: OutputKind::Running,
            text: "Playing on Demo interface \u{b7} outputs 3/4: no gaps, nothing dropped."
                .to_owned(),
            alert: None,
        }
    );
}

#[test]
fn the_state_of_the_output_is_said_in_words_with_its_counters_and_a_failure_as_an_alert() {
    let counted = local_output_view(&with_output(
        LocalOutput {
            underrun_frames: 1,
            dropped_frames: 12_480,
            ..playing_demo()
        },
        Some(sample_outputs()),
    ));
    assert_eq!(
        counted.state.text,
        "Playing on Demo interface \u{b7} outputs 3/4: 1 silent frame filled in, 12,480 frames dropped to hold the delay."
    );
    let failed = local_output_view(&with_output(
        LocalOutput {
            state: "failed".to_owned(),
            reason: Some("the device was unplugged".to_owned()),
            ..playing_demo()
        },
        Some(sample_outputs()),
    ));
    assert_eq!(failed.state.kind, OutputKind::Failed);
    assert_eq!(failed.state.text, "Stopped: the local mix plays nowhere.");
    assert_eq!(
        failed.value,
        output_value(DEMO, 2),
        "the output that failed stays chosen"
    );
    assert_eq!(
        failed.state.alert.as_deref(),
        Some(
            "Demo interface \u{b7} outputs 3/4 stopped: the device was unplugged. Choose it again, or another output."
        )
    );
    let quiet = local_output_view(&with_output(off(), Some(sample_outputs())));
    assert_eq!(quiet.value, "");
    assert_eq!(
        quiet.state,
        OutputState {
            kind: OutputKind::Off,
            text: "Off: the local mix plays nowhere.".to_owned(),
            alert: None,
        }
    );
}

#[test]
fn a_kept_output_the_engine_did_not_open_is_said_and_one_not_offered_now_still_shows() {
    let notice = "The saved local mix output, Volt 4 \u{b7} outputs 1/2, was not opened: the running audio driver does not offer it. The local mix plays nowhere until you choose an output.";
    let said = local_output_view(&with_output(
        LocalOutput {
            notice: Some(notice.to_owned()),
            ..off()
        },
        Some(sample_outputs()),
    ));
    assert_eq!(said.notice.as_deref(), Some(notice));
    let gone = local_output_view(&with_output(
        LocalOutput {
            state: "failed".to_owned(),
            reason: Some("unplugged".to_owned()),
            ..playing_demo()
        },
        Some(Vec::new()),
    ));
    assert_eq!(
        gone.options,
        [
            Choice::new("", "None (lowest load)"),
            Choice::new(
                output_value(DEMO, 2),
                "Demo interface \u{b7} outputs 3/4 (not offered now)"
            ),
        ]
    );
}

#[test]
fn with_no_engine_or_before_the_outputs_are_listed_the_select_waits_and_says_why() {
    let idle = local_output_view(&ConsoleState::new());
    assert!(idle.disabled);
    assert_eq!(idle.value, "");
    assert_eq!(idle.options, [Choice::new("", "None (lowest load)")]);
    assert_eq!(idle.state.kind, OutputKind::Idle);
    assert_eq!(
        idle.state.text,
        "Start the engine to choose where the local mix plays."
    );

    let listing = local_output_view(&with_output(off(), None));
    assert!(listing.disabled);
    assert_eq!(listing.state.text, "Listing the outputs\u{2026}");

    let mut failed = with_output(off(), None);
    failed.reduce(&Action::OutputsFailed {
        message: "no audio service".to_owned(),
        run: Some(1),
    });
    let failed = local_output_view(&failed);
    assert_eq!(
        failed.error.as_deref(),
        Some("The outputs could not be listed: no audio service.")
    );
    assert!(!failed.disabled, "None can still be chosen");

    let mut report = sample_status();
    report.status.stopped = true;
    report.status.local_output = Some(off());
    let mut stopped = with(report);
    stopped.reduce(&Action::Outputs {
        list: sample_outputs(),
        run: Some(1),
    });
    assert!(
        local_output_view(&stopped).disabled,
        "a stopped engine plays nothing"
    );
}

#[test]
fn what_a_change_of_the_output_did_is_said_in_words() {
    use bandear_desktop_console::types::dto::LocalOutputSetDto;
    let answer = |output: bool, applied, saved, save_error: Option<&str>| LocalOutputSetDto {
        // The session's output type is the core's; it is read from JSON so the screen names only
        // the console's types.
        output: output.then(|| {
            serde_json::from_value(serde_json::json!({ "device": DEMO, "firstChannel": 2 }))
                .expect("an output")
        }),
        local_output: None,
        applied,
        saved,
        save_error: save_error.map(str::to_owned),
    };
    assert_eq!(
        format_local_output_set(&answer(true, true, true, None)),
        "The local mix plays on Demo interface \u{b7} outputs 3/4."
    );
    assert_eq!(
        format_local_output_set(&answer(false, true, true, None)),
        "The local mix plays nowhere now."
    );
    assert_eq!(
        format_local_output_set(&answer(true, false, true, None)),
        "Demo interface \u{b7} outputs 3/4 is kept: the local mix plays there from the next start."
    );
    assert_eq!(
        format_local_output_set(&answer(false, false, true, None)),
        "No output is kept: the local mix plays nowhere from the next start."
    );
    assert_eq!(
        format_local_output_set(&answer(true, true, false, Some("disk full"))),
        "The local mix plays on Demo interface \u{b7} outputs 3/4. It was not saved for the next start: disk full."
    );
}
