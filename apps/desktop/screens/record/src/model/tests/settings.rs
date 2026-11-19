//! Arming and the recording settings (`record-controller.test.js`, `recording.test.js`).

use super::*;

// ---- Arming and the settings -----------------------------------------------------------------

#[test]
fn the_console_reads_the_recording_settings_once_connected_and_two_quick_changes_both_land() {
    let mut rig = rig();
    assert_eq!(
        rig.harness.console.state().recording_settings,
        Some(rig.recorder.borrow().settings.clone())
    );

    // Both are asked for before the first is answered, as two quick clicks are.
    rig.harness.hold("set_recording_settings");
    let armed = rig.start(rig.model.set_recording_settings(SettingsChange {
        armed: Some(vec![2, 1]),
        ..SettingsChange::default()
    }));
    let local = rig.start(rig.model.set_recording_settings(SettingsChange {
        include_local: Some(true),
        ..SettingsChange::default()
    }));
    assert_eq!(rig.sent("set_recording_settings"), 1, "one at a time");
    rig.harness.release();
    rig.harness.release();
    assert!(armed.borrow().as_ref().is_some_and(|said| said.ok));
    assert!(local.borrow().as_ref().is_some_and(|said| said.ok));
    assert_eq!(
        rig.armed(),
        [1, 2],
        "the first change is not lost to the second"
    );
    assert_eq!(
        rig.requests().last(),
        Some(&RecordingSettingsRequest {
            folder: None,
            format: Some("int24".to_owned()),
            include_local: true,
            armed: vec![1, 2],
        })
    );
    assert_eq!(
        rig.outcome(),
        Some((
            RecordingAction::Settings,
            true,
            "The recording settings were saved.".to_owned()
        ))
    );
}

#[test]
fn a_refused_settings_change_is_said_and_never_stops_a_later_one() {
    let mut rig = rig();
    rig.arm_inputs(vec![1, 2]);
    let refused = rig.change(SettingsChange {
        folder: Some(Some("Recordings".to_owned())),
        ..SettingsChange::default()
    });
    assert!(!refused.ok);
    assert!(
        refused.message.starts_with(
            "The recording settings were not changed: the recordings folder must be a whole path"
        ),
        "{}",
        refused.message
    );
    assert_eq!(
        rig.outcome(),
        Some((RecordingAction::Settings, false, refused.message.clone()))
    );
    let next = rig.change(SettingsChange {
        format: Some("float32".to_owned()),
        ..SettingsChange::default()
    });
    assert!(next.ok, "a refusal never stops a later change");
    assert_eq!(
        rig.harness
            .console
            .state()
            .recording_settings
            .as_ref()
            .map(|settings| settings.format),
        Some("float32")
    );
    assert_eq!(rig.armed(), [1, 2]);
}

#[test]
fn a_change_queued_behind_a_refused_one_builds_on_the_accepted_settings_not_the_refused() {
    let mut rig = rig();
    rig.harness.hold("set_recording_settings");
    let armed = rig.start(rig.model.set_recording_settings(SettingsChange {
        armed: Some(vec![1]),
        ..SettingsChange::default()
    }));
    let refused = rig.start(rig.model.set_recording_settings(SettingsChange {
        folder: Some(Some("Recordings".to_owned())),
        ..SettingsChange::default()
    }));
    let local = rig.start(rig.model.set_recording_settings(SettingsChange {
        include_local: Some(true),
        ..SettingsChange::default()
    }));
    assert_eq!(rig.sent("set_recording_settings"), 1, "one at a time");
    for _ in 0..3 {
        rig.harness.release();
    }

    assert!(armed.borrow().as_ref().is_some_and(|said| said.ok));
    assert!(refused.borrow().as_ref().is_some_and(|said| !said.ok));
    assert!(local.borrow().as_ref().is_some_and(|said| said.ok));
    assert_eq!(
        rig.requests().last(),
        Some(&RecordingSettingsRequest {
            folder: None,
            format: Some("int24".to_owned()),
            include_local: true,
            armed: vec![1],
        })
    );

    // A failed save (a valid change the desk could not keep) is not carried either.
    rig.harness.desk.fail_next(
        "set_recording_settings",
        "the session could not be saved",
        1,
    );
    let unsaved = rig.start(rig.model.set_recording_settings(SettingsChange {
        armed: Some(vec![4]),
        ..SettingsChange::default()
    }));
    let format = rig.start(rig.model.set_recording_settings(SettingsChange {
        format: Some("float32".to_owned()),
        ..SettingsChange::default()
    }));
    for _ in 0..2 {
        rig.harness.release();
    }
    assert!(unsaved.borrow().as_ref().is_some_and(|said| !said.ok));
    assert!(format.borrow().as_ref().is_some_and(|said| said.ok));
    assert_eq!(
        rig.requests().last().map(|request| request.armed.clone()),
        Some(vec![1])
    );
    assert_eq!(rig.armed(), [1]);
}

#[test]
fn quick_arm_presses_compose_and_a_refused_one_rides_in_no_later_request() {
    let mut rig = rig();
    rig.harness.hold("set_recording_settings");
    let first = rig.start(rig.model.set_recording_settings(arm_change_of(1, true)));
    let second = rig.start(rig.model.set_recording_settings(arm_change_of(4, true)));
    for _ in 0..2 {
        rig.harness.release();
    }
    assert!(first.borrow().as_ref().is_some_and(|said| said.ok));
    assert!(second.borrow().as_ref().is_some_and(|said| said.ok));
    assert_eq!(rig.armed(), [1, 4], "the first arm stays");

    let refused = rig.start(rig.model.set_recording_settings(arm_change_of(300, true)));
    let pair = rig.start(rig.model.set_recording_settings(arm_change_of(3, true)));
    let kick = rig.start(rig.model.set_recording_settings(arm_change_of(1, false)));
    for _ in 0..3 {
        rig.harness.release();
    }
    assert!(refused.borrow().as_ref().is_some_and(|said| !said.ok));
    assert!(pair.borrow().as_ref().is_some_and(|said| said.ok));
    assert!(kick.borrow().as_ref().is_some_and(|said| said.ok));
    assert_eq!(rig.armed(), [3, 4]);
    assert!(
        !rig.requests()
            .iter()
            .rev()
            .take(2)
            .any(|request| request.armed.contains(&300)),
        "the refused input is in no later request"
    );

    // Disarming the other side of a linked pair disarms it whole.
    rig.start(rig.model.set_recording_settings(arm_change_of(2, false)));
    rig.harness.release();
    assert_eq!(rig.armed(), [4]);
}

#[test]
fn two_quick_arm_presses_on_two_tracks_both_land() {
    let mut rig = rig();
    let kick = rig.start(rig.model.arm("in-1"));
    let snare = rig.start(rig.model.arm("in-2"));
    let unknown = rig.start(rig.model.arm("in-99"));
    assert!(kick.borrow().clone().flatten().is_some_and(|said| said.ok));
    assert!(snare.borrow().clone().flatten().is_some_and(|said| said.ok));
    assert_eq!(
        *unknown.borrow(),
        Some(None),
        "a row the screen does not show"
    );
    assert_eq!(rig.armed(), [1, 2]);
}

#[test]
fn a_second_press_on_the_same_row_before_the_first_is_answered_toggles_it_back() {
    let mut rig = rig();
    rig.harness.hold("set_recording_settings");
    let on = rig.start(rig.model.arm("in-1"));
    let off = rig.start(rig.model.arm("in-1"));
    for _ in 0..2 {
        rig.harness.release();
    }
    assert!(on.borrow().clone().flatten().is_some_and(|said| said.ok));
    assert!(off.borrow().clone().flatten().is_some_and(|said| said.ok));
    let asked: Vec<_> = rig
        .requests()
        .iter()
        .map(|request| request.armed.clone())
        .collect();
    assert_eq!(
        asked,
        [vec![1], vec![]],
        "the second press disarms what the first armed"
    );
    assert!(rig.armed().is_empty());

    // Once answered, a press toggles from what the desk keeps again.
    let again = rig.start(rig.model.arm("in-1"));
    rig.harness.release();
    assert!(again.borrow().clone().flatten().is_some_and(|said| said.ok));
    assert_eq!(rig.armed(), [1]);

    // The local mix toggles from its pending intent the same way.
    rig.start(rig.model.arm("local"));
    rig.start(rig.model.arm("local"));
    for _ in 0..2 {
        rig.harness.release();
    }
    let locals: Vec<_> = rig
        .requests()
        .iter()
        .rev()
        .take(2)
        .map(|request| request.include_local)
        .collect();
    assert_eq!(locals, [false, true], "on, then off again (newest first)");
    assert_eq!(
        rig.harness
            .console
            .state()
            .recording_settings
            .as_ref()
            .map(|settings| settings.include_local),
        Some(false)
    );
}

#[test]
fn a_settings_request_or_listing_the_desk_refuses_says_why() {
    let mut rig = rig();
    rig.harness
        .desk
        .fail_next("set_recording_settings", "the bridge went away", 4);
    let armed = rig.run(rig.model.arm("in-2")).expect("a row");
    assert!(!armed.ok);
    assert_eq!(
        armed.message,
        "The recording settings were not changed: the bridge went away."
    );
    assert_eq!(
        rig.outcome(),
        Some((RecordingAction::Settings, false, armed.message.clone()))
    );
    for flow in [
        rig.model.use_folder("D:\\Takes".to_owned()),
        rig.model.use_default_folder(),
        rig.model.change_settings(SettingsChange {
            format: Some("float32".to_owned()),
            ..SettingsChange::default()
        }),
    ] {
        let said = rig.run(flow).expect("asked");
        assert!(!said.ok);
    }

    // A kept folder whose listing fails is still said kept, and the listing's failure is kept.
    rig.harness.desk.fail_next("list_takes", "no listing", 1);
    let kept = rig
        .run(rig.model.use_folder("D:\\Takes".to_owned()))
        .expect("asked");
    assert!(kept.ok, "{}", kept.message);
    assert_eq!(
        rig.harness.console.state().takes.error.as_deref(),
        Some("no listing")
    );
}

#[test]
fn a_refused_folder_stays_in_the_field_to_be_corrected_a_kept_one_shows_as_the_desk_has_it() {
    let mut rig = rig();
    let console = rig.harness.console.clone();
    assert_eq!(
        rig.model.folder_to_show(&console.state(), false).as_deref(),
        Some(RECORDINGS)
    );
    assert_eq!(
        rig.model.folder_to_show(&console.state(), false),
        None,
        "already shown"
    );

    let refused = rig
        .run(rig.model.use_folder("Recordings".to_owned()))
        .expect("asked");
    assert!(!refused.ok);
    assert_eq!(
        rig.model.folder_to_show(&console.state(), false),
        None,
        "the typed text stays"
    );

    let listed = rig.sent("list_takes");
    let kept = rig
        .run(rig.model.use_folder("D:\\Takes".to_owned()))
        .expect("asked");
    assert!(kept.ok, "{}", kept.message);
    assert_eq!(
        rig.sent("list_takes"),
        listed + 1,
        "the new folder's takes are listed"
    );
    assert_eq!(
        rig.model.folder_to_show(&console.state(), true),
        None,
        "never while the engineer types"
    );
    assert_eq!(
        rig.model.folder_to_show(&console.state(), false).as_deref(),
        Some("D:\\Takes")
    );

    rig.run(rig.model.use_default_folder());
    assert_eq!(
        rig.model.folder_to_show(&console.state(), false).as_deref(),
        Some(RECORDINGS)
    );
}

#[test]
fn a_settings_change_reads_the_estimate_again() {
    let mut rig = rig();
    let reads = rig.sent("record_estimate");
    rig.arm_inputs(vec![1, 2]);
    assert_eq!(rig.sent("record_estimate"), reads + 1);
    let estimate = rig
        .harness
        .console
        .state()
        .estimate
        .clone()
        .expect("an estimate");
    assert_eq!(estimate.bytes_per_second, Some(2 * 3 * 48_000));
    rig.change(SettingsChange {
        format: Some("float32".to_owned()),
        include_local: Some(true),
        ..SettingsChange::default()
    });
    assert_eq!(
        rig.harness
            .console
            .state()
            .estimate
            .as_ref()
            .and_then(|estimate| estimate.bytes_per_second),
        Some(4 * 4 * 48_000),
        "two mono files and the stereo local mix in float"
    );
}
