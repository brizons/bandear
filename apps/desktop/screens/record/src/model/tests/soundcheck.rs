//! The soundcheck, alone and through the transport (`soundcheck.test.js`,
//! `record-controller.test.js`).

use super::*;

// ---- The soundcheck --------------------------------------------------------------------------

#[test]
fn a_take_plays_in_place_of_the_inputs_moves_pauses_loops_and_goes_back_live() {
    let mut rig = rig();
    let (first, _) = rig.with_takes();

    let started = rig.run(rig.model.start_soundcheck(first.clone(), 0, false));
    assert!(started.ok, "{}", started.message);
    assert_eq!(
        started.message,
        "Soundcheck: \"First\" plays in place of the live inputs from 0:00."
    );
    assert!(matches!(
        &rig.harness.calls_to("start_soundcheck")[0],
        Command::StartSoundcheck { take_folder, start_frame: Some(0), looping: Some(false) }
            if *take_folder == first
    ));
    let soundcheck = rig.soundcheck();
    assert_eq!(soundcheck.state, "playing");
    assert_eq!(soundcheck.take_name.as_deref(), Some("First"));
    assert_eq!(soundcheck.length_frames, 96_000);
    assert_eq!(soundcheck.clock, "timer");

    let moved = rig.run(rig.model.seek_soundcheck(48_000));
    assert_eq!(moved.message, "Soundcheck moved to 0:01.");
    let paused = rig.run(rig.model.pause_soundcheck());
    assert_eq!(paused.message, "Soundcheck paused at 0:01.");
    assert_eq!(rig.soundcheck().state, "paused", "the answer shows at once");
    let to_end = rig.run(rig.model.seek_soundcheck(96_000));
    assert!(to_end.ok);
    assert_eq!(
        rig.soundcheck().state,
        "paused",
        "moved to its end while paused, it stays paused"
    );
    let again = rig.run(rig.model.resume_soundcheck());
    assert_eq!(
        again.message, "Soundcheck playing from 0:00.",
        "Play starts it again"
    );
    let looping = rig.run(rig.model.set_soundcheck_loop(true));
    assert_eq!(
        looping.message,
        "The soundcheck starts the take again at its end."
    );
    let once = rig.run(rig.model.set_soundcheck_loop(false));
    assert_eq!(once.message, "The soundcheck stops at the end of the take.");
    assert_eq!(
        rig.outcome().map(|(action, ok, _)| (action, ok)),
        Some((RecordingAction::Soundcheck, true))
    );

    let reads = rig.sent("engine_status");
    let live = rig.run(rig.model.stop_soundcheck());
    assert_eq!(live.message, "Back to the live inputs.");
    assert_eq!(
        rig.sent("engine_status"),
        reads + 1,
        "the status read after it"
    );
    assert_eq!(rig.soundcheck().state, "off");
    assert!(!rig.harness.console.state().is_soundcheck_now());
}

#[test]
fn a_seek_plays_a_take_stopped_at_its_end_as_the_engine_does() {
    let mut rig = rig();
    let (first, _) = rig.with_takes();
    rig.run(rig.model.start_soundcheck(first, 0, false));
    // The engine paused the take at its end.
    {
        let soundcheck = &mut rig.harness.desk.report.status.soundcheck;
        soundcheck.state = "paused".to_owned();
        soundcheck.position_frames = soundcheck.length_frames;
    }
    rig.status_now();
    assert_eq!(rig.soundcheck().state, "paused");

    let from_end = rig.run(rig.model.seek_soundcheck(48_000));
    assert!(from_end.ok);
    assert_eq!(
        rig.soundcheck().state,
        "playing",
        "a seek from the end plays at once"
    );
    assert_eq!(rig.soundcheck().position_frames, 48_000);
}

#[test]
fn a_soundcheck_can_start_from_a_frame_looping() {
    let mut rig = rig();
    let (first, _) = rig.with_takes();
    let started = rig.run(rig.model.start_soundcheck(first, 72_000, true));
    assert!(started.ok, "{}", started.message);
    assert!(
        started.message.ends_with("from 0:01, looping."),
        "{}",
        started.message
    );
    assert_eq!(rig.soundcheck().position_frames, 72_000);
    assert!(rig.soundcheck().looping);
}

#[test]
fn while_a_soundcheck_plays_nothing_records_and_the_input_device_stays_saying_why() {
    let mut rig = rig();
    let (first, _) = rig.with_takes();
    rig.run(rig.model.start_soundcheck(first, 0, false));
    let records = rig.sent("start_recording");

    let recorded = rig.run(rig.model.start_recording("Late".to_owned()));
    assert_eq!(
        recorded,
        Said {
            ok: false,
            message: SOUNDCHECK_BLOCKS_RECORDING.to_owned(),
        }
    );
    assert_eq!(rig.sent("start_recording"), records, "no start went out");
    let switch = rig
        .harness
        .console
        .switch_device("Microphone (USB audio)".to_owned());
    rig.run(switch);
    assert_eq!(rig.sent("switch_device"), 0, "nor a switch");
    assert_eq!(
        rig.harness
            .console
            .state()
            .command_error
            .as_ref()
            .map(|error| error.message.clone()),
        Some(SOUNDCHECK_BLOCKS_SWITCH.to_owned())
    );
}

#[test]
fn a_take_recording_leaves_no_soundcheck_to_start_saying_why() {
    let mut rig = rig();
    let (first, _) = rig.with_takes();
    rig.run(rig.model.start_recording("Third".to_owned()));

    let refused = rig.run(rig.model.start_soundcheck(first, 0, false));
    assert_eq!(
        refused,
        Said {
            ok: false,
            message: RECORDING_BLOCKS_SOUNDCHECK.to_owned(),
        }
    );
    assert_eq!(rig.sent("start_soundcheck"), 0, "nothing went out");
}

#[test]
fn the_desks_refusals_are_said_with_the_commands_lead() {
    let mut rig = rig();
    let (first, _) = rig.with_takes();

    let elsewhere = rig.run(rig.model.start_soundcheck(
        "D:\\Elsewhere\\2026-10-16_190000_Show".to_owned(),
        0,
        false,
    ));
    assert_eq!(
        elsewhere.message,
        "The soundcheck did not start: D:\\Elsewhere\\2026-10-16_190000_Show is not a take \
         folder in the recordings folder D:\\Recordings."
    );
    let past = rig.run(rig.model.start_soundcheck(first.clone(), 200_000, false));
    assert_eq!(
        past.message,
        "The soundcheck did not start: 4.2 s is past the end of the take (2.0 s)."
    );
    assert_eq!(rig.soundcheck().state, "off", "the inputs stay live");
    let nothing = rig.run(rig.model.pause_soundcheck());
    assert_eq!(
        nothing.message,
        "The soundcheck was not paused: no soundcheck is playing."
    );
    rig.run(rig.model.start_soundcheck(first.clone(), 0, false));
    let twice = rig.run(rig.model.start_soundcheck(first, 0, false));
    assert_eq!(
        twice.message,
        "The soundcheck did not start: a soundcheck is already playing a take: go back to the \
         live inputs first."
    );
    let beyond = rig.run(rig.model.seek_soundcheck(500_000));
    assert_eq!(
        beyond.message,
        "The soundcheck did not move: 10.4 s is past the end of the take (2.0 s)."
    );
    rig.harness
        .desk
        .fail_next("set_soundcheck_loop", "the engine is busy", 1);
    let looped = rig.run(rig.model.set_soundcheck_loop(true));
    assert_eq!(
        looped.message,
        "The soundcheck's loop was not changed: the engine is busy."
    );
    rig.harness
        .desk
        .fail_next("stop_soundcheck", "the engine is busy", 1);
    let stopped = rig.run(rig.model.stop_soundcheck());
    assert_eq!(
        stopped.message,
        "The soundcheck did not stop: the engine is busy."
    );
}

#[test]
fn a_take_whose_reading_fails_refuses_the_transport_and_still_goes_back_live() {
    let mut rig = rig();
    let (first, _) = rig.with_takes();
    rig.run(rig.model.start_soundcheck(first, 0, false));
    {
        let soundcheck = &mut rig.harness.desk.report.status.soundcheck;
        soundcheck.state = "failed".to_owned();
        soundcheck.reason = Some("reading \"01 Kick.wav\" failed".to_owned());
    }
    rig.status_now();
    assert_eq!(rig.soundcheck().state, "failed");

    let refused = rig.run(rig.model.resume_soundcheck());
    assert_eq!(
        refused.message,
        "The soundcheck did not play on: the soundcheck stopped playing: reading \"01 Kick.wav\" \
         failed; go back to the live inputs to stop it."
    );
    let live = rig.run(rig.model.stop_soundcheck());
    assert!(live.ok, "{}", live.message);
    assert_eq!(rig.soundcheck().state, "off");
}

#[test]
fn a_quick_run_of_seeks_sends_the_first_and_the_last() {
    let mut rig = rig();
    let (first, _) = rig.with_takes();
    rig.run(rig.model.start_soundcheck(first, 0, false));
    rig.harness.hold("seek_soundcheck");

    let seeks: Vec<_> = [12_000, 24_000, 36_000]
        .into_iter()
        .map(|frame| rig.start(rig.model.seek_soundcheck(frame)))
        .collect();
    assert_eq!(rig.sent("seek_soundcheck"), 1);
    rig.harness.release();
    assert_eq!(rig.sent("seek_soundcheck"), 2);
    rig.harness.release();

    let frames: Vec<_> = rig
        .harness
        .calls_to("seek_soundcheck")
        .into_iter()
        .filter_map(|command| match command {
            Command::SeekSoundcheck { frame } => Some(*frame),
            _ => None,
        })
        .collect();
    assert_eq!(
        frames,
        [12_000, 36_000],
        "a seek overtaken before it went out never goes"
    );
    assert!(
        seeks
            .iter()
            .all(|seek| seek.borrow().as_ref().is_some_and(|said| said.ok))
    );
    assert_eq!(rig.soundcheck().position_frames, 36_000);
}

#[test]
fn a_soundcheck_answer_shows_on_the_status_of_its_run_only() {
    let mut state = ConsoleState::new();
    assert_eq!(
        state.soundcheck_status().state,
        "off",
        "no engine: the inputs are live"
    );
    state.reduce(&Action::Status(Box::new(sample_status())));
    let playing = Soundcheck {
        state: "playing".to_owned(),
        take_name: Some("Show".to_owned()),
        ..Soundcheck::default()
    };
    state.reduce(&Action::Soundcheck {
        soundcheck: Box::new(playing.clone()),
        run: state.engine.run,
    });
    assert_eq!(
        state.soundcheck_status().state,
        "playing",
        "the answer shows at once"
    );
    assert!(state.is_soundcheck_now());
    let other = state.engine.run.map(|run| run + 5);
    state.reduce(&Action::Soundcheck {
        soundcheck: Box::new(Soundcheck::default()),
        run: other,
    });
    assert_eq!(
        state.soundcheck_status().state,
        "playing",
        "an answer of another run is not taken"
    );
}

// ---- The soundcheck through the transport ----------------------------------------------------

#[test]
fn play_starts_the_chosen_take_as_a_soundcheck_pauses_and_resumes_it_stop_goes_back_live() {
    let mut rig = rig();
    let (first, second) = rig.with_takes();

    rig.model.choose_take(Some(first.clone()));
    let started = rig.run(rig.model.play()).expect("Play ran");
    assert!(started.ok, "{}", started.message);
    assert_eq!(
        rig.soundcheck().take_folder,
        Some(first.clone()),
        "the chosen take, not the last"
    );
    assert_eq!(rig.soundcheck().state, "playing");

    assert!(rig.run(rig.model.play()).is_some_and(|said| said.ok));
    assert_eq!(
        rig.soundcheck().state,
        "paused",
        "Play pauses a take playing"
    );
    assert!(rig.run(rig.model.play()).is_some_and(|said| said.ok));
    assert_eq!(
        rig.soundcheck().state,
        "playing",
        "and plays a paused one on"
    );

    let stops = rig.sent("stop_recording");
    rig.run(rig.model.stop());
    assert_eq!(rig.soundcheck().state, "off", "back to the live inputs");
    assert_eq!(
        rig.sent("stop_recording"),
        stops,
        "no recording was stopped"
    );

    rig.model.choose_take(None);
    rig.run(rig.model.play());
    assert_eq!(
        rig.soundcheck().take_folder,
        Some(second),
        "with none chosen, the last take"
    );
    rig.run(rig.model.stop());
    let from_row = rig
        .run(rig.model.soundcheck_take(first.clone()))
        .expect("ran");
    assert!(from_row.ok, "{}", from_row.message);
    assert_eq!(
        rig.soundcheck().take_folder,
        Some(first),
        "a take row's Soundcheck plays that take"
    );
}

#[test]
fn back_to_start_and_the_slider_move_the_take_and_a_loop_chosen_before_the_start_loops() {
    let mut rig = rig();
    let (first, _) = rig.with_takes();

    assert_eq!(rig.run(rig.model.set_loop(true)), None);
    assert!(rig.model.local().loop_next, "kept for the start");
    rig.run(rig.model.soundcheck_take(first));
    assert!(matches!(
        &rig.harness.calls_to("start_soundcheck")[0],
        Command::StartSoundcheck {
            looping: Some(true),
            ..
        }
    ));

    rig.model.drag(1.5);
    assert_eq!(
        rig.model.local().drag,
        Some(1.5),
        "the slider shows where it is dragged to"
    );
    let moved = rig.run(rig.model.seek_to(1.5)).expect("moved");
    assert!(moved.ok, "{}", moved.message);
    assert_eq!(rig.model.local().drag, None);
    assert_eq!(rig.soundcheck().position_frames, 72_000);

    rig.run(rig.model.rewind());
    assert_eq!(
        rig.soundcheck().position_frames,
        0,
        "Back to start moves the take to its start"
    );
    rig.run(rig.model.set_loop(false));
    assert!(!rig.soundcheck().looping, "the take playing stops looping");
}

#[test]
fn a_move_or_loop_pressed_while_stop_goes_back_live_is_not_sent_so_the_stops_words_stay() {
    let mut rig = rig();
    let (first, _) = rig.with_takes();
    rig.run(rig.model.soundcheck_take(first));
    for name in ["stop_soundcheck", "seek_soundcheck", "set_soundcheck_loop"] {
        rig.harness.hold(name);
    }

    let stopping = rig.start(rig.model.stop());
    assert_eq!(rig.model.local().busy, Some(Busy::Stop));
    let pressed = [
        rig.start(rig.model.seek_to(1.0)),
        rig.start(rig.model.rewind()),
        rig.start(rig.model.set_loop(true)),
    ];
    rig.harness.release();

    assert!(
        pressed.iter().all(|answer| *answer.borrow() == Some(None)),
        "each was pressed while Stop was on its way"
    );
    assert_eq!(rig.sent("seek_soundcheck"), 0, "no move went out");
    assert_eq!(rig.sent("set_soundcheck_loop"), 0, "nor the loop");
    assert!(
        stopping
            .borrow()
            .clone()
            .flatten()
            .is_some_and(|said| said.ok)
    );
    assert_eq!(
        rig.outcome(),
        Some((
            RecordingAction::Soundcheck,
            true,
            "Back to the live inputs.".to_owned()
        ))
    );
    assert_eq!(rig.soundcheck().state, "off");
    assert_eq!(rig.model.local().busy, None);
}

#[test]
fn moves_on_their_way_hold_the_other_transport_commands_but_not_each_other() {
    let mut rig = rig();
    let (first, _) = rig.with_takes();
    rig.run(rig.model.soundcheck_take(first));
    rig.harness.hold("seek_soundcheck");

    let moves = [
        rig.start(rig.model.seek_to(0.5)),
        rig.start(rig.model.seek_to(1.0)),
    ];
    assert_eq!(rig.model.local().busy, Some(Busy::Move));
    assert_eq!(rig.run(rig.model.stop()), None, "Stop waits for the moves");
    assert_eq!(
        rig.run(rig.model.set_loop(true)),
        None,
        "and so does the loop"
    );
    rig.harness.release();
    rig.harness.release();

    assert!(
        moves.iter().all(|answer| answer
            .borrow()
            .clone()
            .flatten()
            .is_some_and(|said| said.ok)),
        "both moves were answered"
    );
    assert_eq!(
        rig.soundcheck().position_frames,
        48_000,
        "the latest move wins"
    );
    assert_eq!(rig.soundcheck().state, "playing", "nothing stopped it");
    assert_eq!(rig.sent("set_soundcheck_loop"), 0);
    assert_eq!(
        rig.model.local().busy,
        None,
        "the buttons are free once both are answered"
    );
}
