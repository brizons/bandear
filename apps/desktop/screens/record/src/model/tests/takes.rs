//! Takes: start, markers, stop, refusals and the listing (`recording.test.js`).

use super::*;

// ---- Takes -----------------------------------------------------------------------------------

#[test]
fn a_take_records_the_armed_inputs_takes_markers_and_stops_into_the_takes_list() {
    let mut rig = rig();
    rig.change(SettingsChange {
        armed: Some(vec![1, 2, 9]),
        include_local: Some(true),
        ..SettingsChange::default()
    });

    let started = rig.run(rig.model.start_recording("  Soundcheck ".to_owned()));
    assert!(started.ok, "{}", started.message);
    assert_eq!(
        started.message,
        "Recording \"Soundcheck\" on 3 tracks in D:\\Recordings. CH 9 is armed but not on this \
         device, so not recorded."
    );
    let request = rig.harness.calls_to("start_recording")[0].clone();
    assert!(
        matches!(
            request,
            Command::StartRecording { ref take_name, utc_offset_minutes }
                if take_name == "Soundcheck" && utc_offset_minutes == Some(local_offset_minutes())
        ),
        "the name trimmed, the window's offset: {request:?}"
    );
    let recording = rig.harness.console.state().recording_status();
    assert_eq!(
        recording.state, "recording",
        "the status read after the start"
    );
    assert_eq!(recording.tracks, ["Kick", "Snare", "Local mix"]);
    assert!(
        rig.sent("list_takes") >= 1,
        "the takes read after the start"
    );

    let chorus = rig.run(rig.model.add_marker("Chorus".to_owned()));
    assert_eq!(chorus.message, "Marked \"Chorus\" at 0:00.");
    let unnamed = rig.run(rig.model.add_marker(String::new()));
    assert_eq!(unnamed.message, "Marked \"Marker 2\" at 0:00.");
    assert_eq!(
        rig.harness.console.state().recording_status().markers.len(),
        2
    );

    let stopped = rig.run(rig.model.stop_recording());
    assert!(stopped.ok, "{}", stopped.message);
    assert_eq!(stopped.message, "\"Soundcheck\" stopped: 0:02 on 3 tracks.");
    assert_eq!(rig.harness.console.state().recording_status().state, "idle");
    assert_eq!(
        rig.harness
            .console
            .state()
            .last_take()
            .map(|take| take.name.clone()),
        Some("Soundcheck".to_owned())
    );
    let listed = rig
        .harness
        .console
        .state()
        .takes
        .list
        .clone()
        .unwrap_or_default();
    assert_eq!(listed[0].name, "Soundcheck", "the takes are read again");
    assert_eq!(listed[0].markers.len(), 2);
    let late = rig.run(rig.model.add_marker("Late".to_owned()));
    assert!(!late.ok);
    assert_eq!(
        late.message,
        "The marker was not added: no take is recording: start one to mark it."
    );
    assert_eq!(
        rig.outcome().map(|(action, ok, _)| (action, ok)),
        Some((RecordingAction::Marker, false))
    );
}

#[test]
fn a_blank_take_name_is_the_desks_take() {
    let mut rig = rig();
    rig.arm_inputs(vec![1]);
    let started = rig.run(rig.model.start_recording("   ".to_owned()));
    assert_eq!(
        started.message,
        "Recording \"Take\" on 1 track in D:\\Recordings."
    );
    assert!(matches!(
        &rig.harness.calls_to("start_recording")[0],
        Command::StartRecording { take_name, .. } if take_name.is_empty()
    ));
}

#[test]
fn a_stop_whose_files_are_still_being_finished_is_asked_again_and_given_up_on_in_words() {
    let mut rig = rig();
    rig.arm_inputs(vec![1]);
    rig.run(rig.model.start_recording("One".to_owned()));
    rig.recorder.borrow_mut().finishing = 2;

    let stopping = rig.start(rig.model.stop_recording());
    rig.harness.run_timers();
    let stopped = stopping.borrow_mut().take().expect("the stop ended");
    assert!(stopped.ok, "{}", stopped.message);
    assert_eq!(
        rig.sent("stop_recording"),
        3,
        "asked until the files were finished"
    );

    rig.run(rig.model.start_recording("Two".to_owned()));
    rig.recorder.borrow_mut().finishing = 10;
    let before = rig.sent("stop_recording");
    let waiting = rig.start(rig.model.stop_recording());
    rig.harness.run_timers();
    let waiting = waiting.borrow_mut().take().expect("the stop ended");
    assert!(!waiting.ok);
    assert_eq!(waiting.message, STILL_FINISHING);
    assert_eq!(
        rig.sent("stop_recording") - before,
        1 + STOP_RETRIES as usize,
        "five more times"
    );
    assert_eq!(
        rig.harness.console.state().recording_status().state,
        "stopping"
    );
    assert!(rig.harness.console.state().is_recording_now());
    let transport = crate::record_view::transport_view(
        &rig.harness.console.state(),
        &rig.model.transport_local(),
    );
    assert!(transport.can_stop, "Stop can be pressed again");
}

#[test]
fn a_start_with_no_engine_or_nothing_armed_is_refused_in_words() {
    let mut stopped = rig_on(
        FakeDesk::stopped(),
        Options {
            auto_start: false,
            ..Options::default()
        },
    );
    let no_engine = stopped.run(stopped.model.start_recording("Show".to_owned()));
    assert!(!no_engine.ok);
    assert_eq!(
        no_engine.message,
        "The recording did not start: the engine is not running: start it to record."
    );

    let mut rig = rig();
    let nothing = rig.run(rig.model.start_recording("Show".to_owned()));
    assert_eq!(
        nothing.message,
        "The recording did not start: nothing is armed: arm at least one input, or record the \
         local mix."
    );
    rig.arm_inputs(vec![7, 8]);
    let elsewhere = rig.run(rig.model.start_recording("Show".to_owned()));
    assert!(
        elsewhere
            .message
            .contains("CH 7, CH 8 are past its 6 inputs")
    );
    assert_eq!(rig.harness.console.state().recording_status().state, "idle");
}

#[test]
fn a_take_recording_blocks_a_device_switch_and_a_driver_change_saying_why() {
    let mut rig = rig();
    rig.arm_inputs(vec![1]);
    rig.run(rig.model.start_recording("Show".to_owned()));
    let starts = rig.sent("start_engine");

    let switch = rig
        .harness
        .console
        .switch_device("Microphone (USB audio)".to_owned());
    rig.run(switch);
    let change = rig
        .harness
        .console
        .apply_audio(Default::default(), None, true);
    let changed = rig.run(change);

    assert_eq!(rig.sent("switch_device"), 0, "the switch never went out");
    assert_eq!(rig.sent("start_engine"), starts, "nor a restart");
    let error = rig
        .harness
        .console
        .state()
        .command_error
        .clone()
        .expect("said");
    assert_eq!(
        (error.action, error.message.as_str()),
        (CommandAction::Switch, RECORDING_BLOCKS_SWITCH)
    );
    assert_eq!(changed, Err(RECORDING_BLOCKS_SWITCH.to_owned()));
    assert_eq!(
        rig.harness.console.state().recording_status().state,
        "recording"
    );
}

#[test]
fn the_takes_are_listed_say_what_was_left_out_and_only_a_listed_one_opens() {
    let mut rig = rig();
    let (first, _) = rig.with_takes();
    rig.recorder.borrow_mut().unreadable = 1;

    assert_eq!(rig.run(rig.model.refresh_takes()), None);
    let takes = rig.harness.console.state().takes.clone();
    assert_eq!(
        takes.list.as_ref().map(|list| list
            .iter()
            .map(|take| take.name.clone())
            .collect::<Vec<_>>()),
        Some(vec!["Second".to_owned(), "First".to_owned()]),
        "newest first"
    );
    assert_eq!(takes.unreadable, 1);
    assert_eq!(takes.folder.as_deref(), Some(RECORDINGS));

    let opened = rig.run(rig.model.open_take_folder(first.clone()));
    assert_eq!(
        opened,
        Said {
            ok: true,
            message: format!("Opened {first}."),
        }
    );
    assert_eq!(rig.recorder.borrow().opened, [first.clone()]);
    let refused = rig.run(rig.model.open_take_folder("C:\\Windows".to_owned()));
    assert_eq!(
        refused.message,
        "The take folder was not opened: C:\\Windows is not a take folder in the recordings \
         folder D:\\Recordings."
    );
    assert_eq!(
        rig.recorder.borrow().opened,
        [first],
        "nothing else was opened"
    );

    rig.harness
        .desk
        .fail_next("list_takes", "the folder could not be read", 1);
    assert_eq!(
        rig.run(rig.model.refresh_takes()).as_deref(),
        Some("the folder could not be read")
    );
    assert_eq!(
        rig.harness
            .console
            .state()
            .takes
            .list
            .as_ref()
            .map(Vec::len),
        Some(2),
        "the last list stays"
    );
    assert_eq!(
        rig.harness.console.state().takes.error.as_deref(),
        Some("the folder could not be read")
    );
}

#[test]
fn stop_reads_the_disk_estimate_again() {
    let mut rig = rig();
    rig.arm_inputs(vec![1]);
    rig.run(rig.model.record("Show".to_owned()));
    let reads = rig.sent("record_estimate");
    let said = rig.run(rig.model.stop()).expect("ran");
    assert!(said.ok, "{}", said.message);
    assert_eq!(rig.sent("record_estimate"), reads + 1);
}
