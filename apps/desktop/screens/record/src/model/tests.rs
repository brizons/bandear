//! The model over the simulated desk with its scripted recorder: `record-controller.test.js`, and
//! the console's recording and soundcheck flows of `recording.test.js` and `soundcheck.test.js`.

use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;

use bandear_desktop_console::chips::ChipKind;
use bandear_desktop_console::command::{Command, Owner};
use bandear_desktop_console::console::{
    Options, RECORDING_BLOCKS_SWITCH, SOUNDCHECK_BLOCKS_SWITCH,
};
use bandear_desktop_console::router::Section;
use bandear_desktop_console::state::{CommandAction, RecordingAction};
use bandear_desktop_console::testing::{FakeDesk, Harness, sample_status};
use bandear_desktop_console::types::dto::RecordingSettingsRequest;
use bandear_desktop_console::wire::{Meters, OutputLevels, Recording, Soundcheck, StatusReport};

use super::desk::{self, RECORDINGS, Recorder};
use super::*;
use crate::record_view::{TrackRow, track_rows};
use crate::recording::Arm;

/// A console connected to a running desk with the scripted recorder, and the screen's model.
struct Rig {
    harness: Harness,
    model: Model,
    recorder: Rc<RefCell<Recorder>>,
}

fn rig_on(mut desk: FakeDesk, options: Options) -> Rig {
    let recorder = desk::install(&mut desk);
    let harness = Harness::connected(desk, options);
    let model = Model::new(harness.console.for_owner(Owner::Screen(Section::Record)));
    Rig {
        harness,
        model,
        recorder,
    }
}

fn rig() -> Rig {
    rig_on(FakeDesk::running(), Options::default())
}

impl Rig {
    /// Runs `flow` until nothing more can happen without a timer or a held answer.
    fn run<T: 'static>(&mut self, flow: impl Future<Output = T> + 'static) -> T {
        self.harness.run(flow).expect("the flow ended")
    }

    /// Starts `flow`, which may wait on held answers; its outcome lands in the cell once it ends.
    fn start<T: 'static>(
        &mut self,
        flow: impl Future<Output = T> + 'static,
    ) -> Rc<RefCell<Option<T>>> {
        let kept = Rc::new(RefCell::new(None));
        let into = Rc::clone(&kept);
        self.harness.console.spawn(async move {
            *into.borrow_mut() = Some(flow.await);
        });
        self.harness.settle();
        kept
    }

    fn sent(&self, name: &str) -> usize {
        self.harness.calls_to(name).len()
    }

    fn requests(&self) -> Vec<RecordingSettingsRequest> {
        self.harness
            .calls_to("set_recording_settings")
            .into_iter()
            .filter_map(|command| match command {
                Command::SetRecordingSettings { settings } => Some(settings.clone()),
                _ => None,
            })
            .collect()
    }

    fn armed(&self) -> Vec<u16> {
        self.harness
            .console
            .state()
            .recording_settings
            .as_ref()
            .map(|settings| settings.armed.clone())
            .unwrap_or_default()
    }

    fn soundcheck(&self) -> Soundcheck {
        self.harness.console.state().soundcheck_status()
    }

    fn outcome(&self) -> Option<(RecordingAction, bool, String)> {
        self.harness
            .console
            .state()
            .recording_outcome
            .as_ref()
            .map(|outcome| (outcome.action, outcome.ok, outcome.message.clone()))
    }

    fn change(&mut self, change: SettingsChange) -> Said {
        let flow = self.model.set_recording_settings(change);
        self.run(flow)
    }

    fn arm_inputs(&mut self, armed: Vec<u32>) {
        let said = self.change(SettingsChange {
            armed: Some(armed),
            ..SettingsChange::default()
        });
        assert!(said.ok, "{}", said.message);
    }

    /// Two takes recorded, "First" then "Second", both two seconds of CH 1.
    fn with_takes(&mut self) -> (String, String) {
        self.arm_inputs(vec![1]);
        let mut folders = Vec::new();
        for name in ["First", "Second"] {
            let started = self.model.start_recording(name.to_owned());
            assert!(self.run(started).ok);
            let stopped = self.model.stop_recording();
            assert!(self.run(stopped).ok);
            folders.push(self.recorder.borrow().takes[0].folder.clone());
        }
        (folders[0].clone(), folders[1].clone())
    }

    /// Delivers the status the desk has now, as its next `status` event would bring it.
    fn status_now(&mut self) {
        self.harness.desk.emit_status();
        self.harness.emit();
    }
}

fn arm_change_of(input: u32, on: bool) -> SettingsChange {
    SettingsChange {
        arm: Some(Arm {
            input,
            on,
            // CH 2 linked to CH 3.
            links: vec![false, true, false, false, false, false],
        }),
        ..SettingsChange::default()
    }
}

// ---- The transport's guard -----------------------------------------------------------------

#[test]
fn a_transport_command_waits_for_its_answer_another_pressed_meanwhile_is_not_run() {
    let mut rig = rig();
    rig.arm_inputs(vec![1]);
    rig.harness.hold("start_recording");

    let started = rig.start(rig.model.record("Show".to_owned()));
    assert_eq!(rig.model.local().busy, Some(Busy::Start));
    let stopped = rig.start(rig.model.stop());
    assert_eq!(
        *stopped.borrow(),
        Some(None),
        "Stop pressed while the start is on its way is not run"
    );
    assert_eq!(rig.sent("stop_recording"), 0);

    rig.harness.release();
    let said = started
        .borrow_mut()
        .take()
        .flatten()
        .expect("the start ran");
    assert!(said.ok, "{}", said.message);
    assert_eq!(rig.model.local().busy, None, "the buttons are free");

    rig.harness
        .desk
        .fail_next("add_marker", "the take is gone", 1);
    let marked = rig.start(rig.model.mark());
    let marked = marked.borrow_mut().take().flatten().expect("the mark ran");
    assert!(!marked.ok);
    assert_eq!(
        rig.model.local().busy,
        None,
        "a refused command frees the buttons too"
    );
}

// ---- The lanes -------------------------------------------------------------------------------

fn recording_status(folder: &str, state: &str) -> StatusReport {
    let mut report = sample_status();
    report.status.recording = Recording {
        state: state.to_owned(),
        take_folder: Some(folder.to_owned()),
        ..Recording::default()
    };
    report
}

fn meters_of(channels: [u8; 6]) -> Meters {
    Meters {
        run: Some(1),
        channels: channels.to_vec(),
        ..Meters::default()
    }
}

#[test]
fn the_lanes_keep_the_armed_tracks_peaks_while_recording_and_a_new_take_starts_them_empty() {
    let mut rig = rig();
    rig.arm_inputs(vec![1]);
    let console = rig.harness.console.clone();
    let rows: Vec<TrackRow> = track_rows(&console.state());

    assert_eq!(
        rig.run(rig.model.rewind()),
        None,
        "nothing to send with the inputs live"
    );
    assert!(rig.model.local().at_start);
    console.dispatch(Action::Status(Box::new(recording_status(
        "D:\\Recordings\\take-1",
        "recording",
    ))));
    assert!(
        !rig.model.local().at_start,
        "a new take: the position counts from it"
    );
    let levels = rig
        .model
        .take_meters(&rows, &meters_of([50, 25, 0, 0, 0, 0]));
    assert_eq!(&levels[..2], [50.0, 25.0]);
    rig.model
        .take_meters(&rows, &meters_of([75, 0, 0, 0, 0, 0]));
    assert_eq!(
        rig.model.history("in-1").map(|history| history.len()),
        Some(2)
    );
    assert_eq!(
        rig.model.history("in-1").map(|history| history.at(1)),
        Some(75.0)
    );
    assert!(
        rig.model.history("in-2").is_none(),
        "an unarmed track keeps nothing"
    );
    assert_eq!(
        rig.model.lane_of(&rows[0], &console.state()),
        Lane::Recording
    );
    assert_eq!(rig.model.lane_of(&rows[1], &console.state()), Lane::Off);
    assert!(!rig.model.follow_take(&console.state()), "the same take");

    // After the take the lane stays drawn; nothing more is kept while idle.
    console.dispatch(Action::Status(Box::new(sample_status())));
    rig.model
        .take_meters(&rows, &meters_of([90, 0, 0, 0, 0, 0]));
    assert_eq!(
        rig.model.history("in-1").map(|history| history.len()),
        Some(2)
    );
    assert_eq!(
        rig.model.lane_of(&rows[0], &console.state()),
        Lane::Recording
    );

    console.dispatch(Action::Status(Box::new(recording_status(
        "D:\\Recordings\\take-2",
        "recording",
    ))));
    assert_eq!(
        rig.model.history("in-1").map(|history| history.len()),
        Some(0),
        "the next take starts its lanes empty"
    );
    console.dispatch(Action::Status(Box::new(sample_status())));
    assert_eq!(
        rig.model.lane_of(&rows[0], &console.state()),
        Lane::Armed,
        "armed, nothing recorded yet"
    );
}

#[test]
fn the_meters_of_the_live_run_feed_the_lanes_on_their_own() {
    let mut rig = rig();
    rig.arm_inputs(vec![1]);
    let started = rig.model.start_recording("Show".to_owned());
    assert!(rig.run(started).ok);

    rig.harness.desk.emit_meters();
    rig.harness.emit();
    assert_eq!(
        rig.model.history("in-1").map(|history| history.len()),
        Some(1)
    );
}

#[test]
fn the_local_mix_lane_reads_the_louder_side_of_the_local_levels() {
    let mut rig = rig();
    rig.change(SettingsChange {
        include_local: Some(true),
        ..SettingsChange::default()
    });
    let console = rig.harness.console.clone();
    console.dispatch(Action::Status(Box::new(recording_status(
        "D:\\Recordings\\take-1",
        "recording",
    ))));
    let local = crate::record_view::local_track_row(&console.state()).expect("the local row");
    let meters = Meters {
        run: Some(1),
        local: OutputLevels {
            left: 25,
            right: 75,
            limiter_reduction_db: 0.0,
        },
        ..Meters::default()
    };
    assert_eq!(rig.model.take_meters(&[local], &meters), [75.0]);
    assert_eq!(
        rig.model.history("local").map(|history| history.at(0)),
        Some(75.0)
    );
}

// ---- The chips -------------------------------------------------------------------------------

#[test]
fn the_rec_chip_shows_while_a_take_records_is_finished_or_failed_hidden_otherwise() {
    let kinds = |report: StatusReport| -> Vec<ChipKind> {
        let mut state = ConsoleState::new();
        state.reduce(&Action::Status(Box::new(report)));
        shell_chips(&state)
            .into_iter()
            .map(|chip| chip.kind)
            .collect()
    };
    assert!(shell_chips(&ConsoleState::new()).is_empty(), "no take");
    assert_eq!(
        kinds(recording_status("D:\\Recordings\\take-1", "recording")),
        [ChipKind::Recording]
    );
    assert_eq!(
        kinds(recording_status("D:\\Recordings\\take-1", "stopping")),
        [ChipKind::Stopping]
    );
    assert_eq!(
        kinds(recording_status("D:\\Recordings\\take-1", "failed")),
        [ChipKind::Failed]
    );
    assert!(kinds(sample_status()).is_empty());
}

#[test]
fn the_strip_shows_the_soundcheck_chip_on_every_screen_while_a_take_plays_in_place_of_the_inputs() {
    let mut report = sample_status();
    report.status.soundcheck = Soundcheck {
        state: "playing".to_owned(),
        take_folder: Some("D:\\Recordings\\take-1".to_owned()),
        take_name: Some("Show".to_owned()),
        ..Soundcheck::default()
    };
    let mut state = ConsoleState::new();
    state.reduce(&Action::Status(Box::new(report)));
    let chips = shell_chips(&state);
    assert_eq!(chips.len(), 1, "no take records");
    assert_eq!(chips[0].kind, ChipKind::Soundcheck);
    assert_eq!(chips[0].word, "SOUNDCHECK");
    assert_eq!(chips[0].target, Section::Record);

    let mut both = state.clone();
    if let Some(status) = both.engine.status.as_mut() {
        status.recording = Recording {
            state: "failed".to_owned(),
            ..Recording::default()
        };
    }
    let kinds: Vec<_> = shell_chips(&both)
        .into_iter()
        .map(|chip| chip.kind)
        .collect();
    assert_eq!(
        kinds,
        [ChipKind::Failed, ChipKind::Soundcheck],
        "REC, then SOUNDCHECK"
    );
}

#[test]
fn the_model_sends_its_commands_for_its_screen() {
    let mut rig = rig();
    assert_eq!(rig.model.console().owner(), Owner::Screen(Section::Record));
    let _ = rig.run(rig.model.refresh_takes());
}

mod settings;
mod soundcheck;
mod takes;
