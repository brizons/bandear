//! The Record screen's model: the recording and soundcheck flows of `console-app.js` and the
//! screen's own state and guards of `record-controller.js`, over the console. Pure: no window,
//! tested with plain `#[test]`s over the console's simulated desk.
//!
//! - The flows: the recording settings change one at a time, each request built when its turn
//!   comes on the settings the desk answered last ([Model::set_recording_settings]); a take starts
//!   named in the window's local time, stops asking again while its files are being finished, and
//!   takes markers; the takes are listed and a take's folder opened; the soundcheck starts, pauses,
//!   plays on, moves (one seek in flight, only the latest after it), loops and goes back to the live
//!   inputs, each answer shown at once on the status of the run it was asked for. Every outcome is
//!   said in words as the console's `recording_outcome`.
//! - The transport: one command at a time ([Model::command]); the buttons wait until it is
//!   answered. Moves of the take (Back to start, the seek slider) share the guard as "move": none
//!   goes out while another command is on its way, but moves do not wait for each other.
//! - The waveform lanes: while a take records, each armed track keeps a bounded history of its
//!   peaks, cleared when the next take folder appears.
//! - Arming: a press is a delta, so quick presses on several tracks all land, and a second press on
//!   a track before the first is answered toggles what the first asked.
//! - The folder field shows the desk's folder unless the engineer types in it; a refused folder
//!   stays to be corrected, a kept one shows as the desk has it and its takes are listed.
//!
//! The model's own state changes only with the console's (every flow ends on a dispatch) or on a
//! press the view handles, so the view draws again whenever it should.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Duration;

use bandear_desktop_console::chips::Chip;
use bandear_desktop_console::command::Command;
use bandear_desktop_console::console::Console;
use bandear_desktop_console::exec::Slot;
use bandear_desktop_console::format::sentence_with_reason;
use bandear_desktop_console::state::{Action, ConsoleState, RecordingAction, RecordingOutcome};
use bandear_desktop_console::types::dto::{
    RecordingSettingsDto, RecordingStartedDto, RecordingStoppedDto, SoundcheckDto, TakeMarkerDto,
    TakesDto,
};
use bandear_desktop_console::wire::Soundcheck;

use crate::record_view::{PeakHistory, TransportLocal, rec_chip, shown_rows};
use crate::recording::{SettingsChange, format_duration, format_started, format_stopped};
use crate::soundcheck::{
    RECORDING_BLOCKS_SOUNDCHECK, SOUNDCHECK_BLOCKS_RECORDING, SoundcheckAction,
    format_soundcheck_done,
};
use crate::soundcheck_view::soundcheck_chip;

/// The peaks a lane keeps: about 20 seconds at the 30 meters a second the engine sends.
pub const LANE_CAPACITY: usize = 600;

/// A stop whose take's files are still being finished: how often to ask again.
pub const STOP_RETRIES: u32 = 5;
/// ... and how far apart.
pub const STOP_RETRY: Duration = Duration::from_millis(300);

/// Why a stop has not stopped yet.
pub const STILL_FINISHING: &str = "The take's files are still being finished, so it has not \
                                   stopped yet. Press Stop again in a moment.";

/// A flow of the screen, run on the console's executor.
pub type Flow<T> = Pin<Box<dyn Future<Output = T>>>;

/// A command's outcome in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Said {
    pub ok: bool,
    pub message: String,
}

/// The transport command on its way: its buttons wait until it is answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Busy {
    Start,
    Stop,
    Mark,
    Play,
    Soundcheck,
    Loop,
    /// Back to start or the seek slider, which do not wait for each other.
    Move,
}

/// What the screen keeps of its own between its draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Local {
    pub busy: Option<Busy>,
    /// Back to start was pressed after a take.
    pub at_start: bool,
    /// The take the soundcheck card picks for Play.
    pub chosen_take: Option<String>,
    /// Where the seek slider is being dragged to, in seconds.
    pub drag: Option<f64>,
    /// The loop asked for the next soundcheck.
    pub loop_next: bool,
}

/// What a track's lane shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    /// Not armed.
    Off,
    /// Records with the next take.
    Armed,
    /// A take records, or the lane kept the peaks of the last one.
    Recording,
}

/// The seeks: the frame asked for last, and whoever waits for the seeks on their way.
#[derive(Default)]
struct Seeks {
    wanted: Option<u64>,
    waiting: Option<Vec<Slot<Said>>>,
}

#[derive(Default)]
struct Inner {
    local: Local,
    /// The moves on their way.
    moving: usize,
    /// The take folder the lanes follow.
    take: Option<String>,
    /// The folder last written into the folder field.
    shown_folder: Option<String>,
    /// Per row key, the arm press on its way: its id and what it asked.
    intents: HashMap<String, (u64, bool)>,
    next_intent: u64,
    histories: HashMap<String, PeakHistory>,
    /// The settings lane: whether a change is on its way, and the changes waiting for it.
    settings_busy: bool,
    settings_waiting: VecDeque<Slot<()>>,
    seeks: Seeks,
}

/// The screen's model, built once as the window opens and kept for its life, so its flows and
/// lanes go on while another screen is shown. Clones share it.
#[derive(Clone)]
pub struct Model {
    console: Console,
    inner: Rc<RefCell<Inner>>,
}

/// The window's offset from UTC now, in minutes east: what a new take's folder is named in.
pub fn local_offset_minutes() -> i32 {
    chrono::Local::now().offset().local_minus_utc() / 60
}

/// The chips the shell shows above every screen for a take recording and a soundcheck playing,
/// in that order (the demo chip is the console's).
pub fn shell_chips(state: &ConsoleState) -> Vec<Chip> {
    rec_chip(state)
        .into_iter()
        .chain(soundcheck_chip(state))
        .collect()
}

impl Model {
    /// The model over `console` (a handle that sends its commands for this screen). It follows
    /// the takes as the status shows them and keeps the armed lanes' peaks from the meters.
    pub fn new(console: Console) -> Self {
        let model = Self {
            console: console.clone(),
            inner: Rc::new(RefCell::new(Inner::default())),
        };
        let follower = model.clone();
        console.subscribe(move |console, _| {
            let state = console.state();
            follower.follow_take(&state);
        });
        let keeper = model.clone();
        console.on_meters(move |meters| {
            let rows = shown_rows(&keeper.console.state());
            keeper.take_meters(&rows, meters);
        });
        model
    }

    /// The console this screen talks to the desk through.
    pub fn console(&self) -> &Console {
        &self.console
    }

    /// Runs `flow` on the console's executor.
    pub fn spawn<T: 'static>(&self, flow: impl Future<Output = T> + 'static) {
        self.console.spawn(flow);
    }

    /// The screen's own state now.
    pub fn local(&self) -> Local {
        self.inner.borrow().local.clone()
    }

    /// What the transport reads of the screen's own state.
    pub fn transport_local(&self) -> TransportLocal {
        let inner = self.inner.borrow();
        TransportLocal {
            at_start: inner.local.at_start,
            chosen_take: inner.local.chosen_take.clone(),
        }
    }

    /// Keeps `words` as the last recording outcome of `action` and answers it.
    fn said(&self, action: RecordingAction, ok: bool, words: impl Into<String>) -> Said {
        let message = words.into();
        self.console
            .dispatch(Action::RecordingOutcome(Some(RecordingOutcome {
                action,
                ok,
                message: message.clone(),
            })));
        Said { ok, message }
    }
}

// The flows (`console-app.js`).
impl Model {
    /// Changes the recording settings with `change` on top of the last change kept. Changes go
    /// out one at a time, each built when its turn comes on the settings the desk answered last,
    /// which hold every change before it that was kept: a refused change rides in no later
    /// request. Never fails: a refusal is said, and the disk estimate is read after a change.
    pub fn set_recording_settings(&self, change: SettingsChange) -> Flow<Said> {
        // The turn is claimed now, so changes go out in the order they were asked for.
        let turn = {
            let mut inner = self.inner.borrow_mut();
            if inner.settings_busy {
                let slot = Slot::new();
                inner.settings_waiting.push_back(slot.clone());
                Some(slot)
            } else {
                inner.settings_busy = true;
                None
            }
        };
        let model = self.clone();
        Box::pin(async move {
            if let Some(turn) = turn {
                turn.wait().await;
            }
            let said = model.send_settings(&change).await;
            let next = {
                let mut inner = model.inner.borrow_mut();
                let next = inner.settings_waiting.pop_front();
                if next.is_none() {
                    inner.settings_busy = false;
                }
                next
            };
            // The lane stays busy and passes to the next change.
            if let Some(next) = next {
                next.fill(());
            }
            said
        })
    }

    async fn send_settings(&self, change: &SettingsChange) -> Said {
        let request = {
            let state = self.console.state();
            crate::recording::settings_request(state.recording_settings.as_ref(), change)
        };
        let answer = self
            .console
            .call::<RecordingSettingsDto>(Command::SetRecordingSettings { settings: request })
            .await;
        match answer {
            Err(error) => self.said(
                RecordingAction::Settings,
                false,
                sentence_with_reason("The recording settings were not changed", Some(&error)),
            ),
            Ok(settings) => {
                self.console
                    .dispatch(Action::RecordingSettings(Some(settings)));
                self.console.refresh_estimate().await;
                self.said(
                    RecordingAction::Settings,
                    true,
                    "The recording settings were saved.",
                )
            }
        }
    }

    /// Lists the takes in the recordings folder; `None`, or why they could not be listed (the
    /// last list known stays, and the failure is kept to be said).
    pub fn refresh_takes(&self) -> Flow<Option<String>> {
        let console = self.console.clone();
        Box::pin(async move {
            match console.call::<TakesDto>(Command::ListTakes).await {
                Ok(takes) => {
                    console.dispatch(Action::Takes(Box::new(takes)));
                    None
                }
                Err(error) => {
                    console.dispatch(Action::TakesFailed(error.clone()));
                    Some(error)
                }
            }
        })
    }

    /// Starts a take called `name` (trimmed; blank lets the desk call it "Take") with the kept
    /// settings, its folder named in the window's local time, then reads the status and the takes.
    /// Refused in words while a soundcheck plays: a take records the live inputs.
    pub fn start_recording(&self, name: String) -> Flow<Said> {
        let model = self.clone();
        Box::pin(async move {
            if model.console.state().is_soundcheck_now() {
                return model.said(RecordingAction::Start, false, SOUNDCHECK_BLOCKS_RECORDING);
            }
            let take_name = name.trim().to_owned();
            let started = model
                .console
                .call::<RecordingStartedDto>(Command::StartRecording {
                    take_name: take_name.clone(),
                    utc_offset_minutes: Some(local_offset_minutes()),
                })
                .await;
            match started {
                Err(error) => model.said(
                    RecordingAction::Start,
                    false,
                    sentence_with_reason("The recording did not start", Some(&error)),
                ),
                Ok(started) => {
                    model.console.read_status().await;
                    model.refresh_takes().await;
                    let shown = if take_name.is_empty() {
                        "Take"
                    } else {
                        &take_name
                    };
                    model.said(
                        RecordingAction::Start,
                        true,
                        format_started(&started, shown),
                    )
                }
            }
        })
    }

    /// Stops the take. While its files are still being finished, asks again [STOP_RETRIES] times
    /// [STOP_RETRY] apart before saying so; the take keeps stopping and Stop can be pressed again.
    pub fn stop_recording(&self) -> Flow<Said> {
        let model = self.clone();
        Box::pin(async move {
            let refused = |model: &Model, error: String| {
                model.said(
                    RecordingAction::Stop,
                    false,
                    sentence_with_reason("The recording was not stopped", Some(&error)),
                )
            };
            let console = model.console.clone();
            let mut answer = match console
                .call::<RecordingStoppedDto>(Command::StopRecording)
                .await
            {
                Ok(answer) => answer,
                Err(error) => {
                    console.read_status().await;
                    return refused(&model, error);
                }
            };
            let mut retry = 0;
            while answer.finishing && retry < STOP_RETRIES && !console.disposed() {
                console.sleep(STOP_RETRY).await;
                answer = match console
                    .call::<RecordingStoppedDto>(Command::StopRecording)
                    .await
                {
                    Ok(answer) => answer,
                    Err(error) => {
                        console.read_status().await;
                        return refused(&model, error);
                    }
                };
                retry += 1;
            }
            console.read_status().await;
            if answer.finishing {
                return model.said(RecordingAction::Stop, false, STILL_FINISHING);
            }
            model.refresh_takes().await;
            // The desk answers the finished take with every stop that is not finishing.
            let words = answer
                .take
                .as_ref()
                .map_or_else(|| "The recording was stopped.".to_owned(), format_stopped);
            model.said(RecordingAction::Stop, true, words)
        })
    }

    /// Marks the take being recorded with `name` (blank: the desk's next "Marker n").
    pub fn add_marker(&self, name: String) -> Flow<Said> {
        let model = self.clone();
        Box::pin(async move {
            let marker = model
                .console
                .call::<TakeMarkerDto>(Command::AddMarker { name })
                .await;
            match marker {
                Err(error) => model.said(
                    RecordingAction::Marker,
                    false,
                    sentence_with_reason("The marker was not added", Some(&error)),
                ),
                Ok(marker) => {
                    model.console.read_status().await;
                    model.said(
                        RecordingAction::Marker,
                        true,
                        format!(
                            "Marked \"{}\" at {}.",
                            marker.name,
                            format_duration(marker.seconds)
                        ),
                    )
                }
            }
        })
    }

    /// Opens a take folder (as a take lists it) in the system's file manager.
    pub fn open_take_folder(&self, folder: String) -> Flow<Said> {
        let model = self.clone();
        Box::pin(async move {
            let opened = model
                .console
                .call::<()>(Command::OpenTakeFolder {
                    folder: folder.clone(),
                })
                .await;
            match opened {
                Err(error) => model.said(
                    RecordingAction::Open,
                    false,
                    sentence_with_reason("The take folder was not opened", Some(&error)),
                ),
                Ok(()) => model.said(RecordingAction::Open, true, format!("Opened {folder}.")),
            }
        })
    }

    /// Runs the soundcheck command `action`: its answer, the soundcheck now, shows at once on the
    /// status of the run it was asked for (`read_after` reads the whole status after it too), and
    /// the outcome is said.
    async fn soundcheck_command(
        &self,
        action: SoundcheckAction,
        command: Command,
        read_after: bool,
    ) -> Said {
        let shown = self.console.state().engine.run;
        let answer = self.console.call::<SoundcheckDto>(command).await;
        let soundcheck = match answer {
            Ok(answer) => Soundcheck::from_core(&answer),
            Err(error) => {
                return self.said(
                    RecordingAction::Soundcheck,
                    false,
                    sentence_with_reason(action.refused_lead(), Some(&error)),
                );
            }
        };
        self.console.dispatch(Action::Soundcheck {
            soundcheck: Box::new(soundcheck.clone()),
            run: shown,
        });
        if read_after {
            self.console.read_status().await;
        }
        self.said(
            RecordingAction::Soundcheck,
            true,
            format_soundcheck_done(action, &soundcheck),
        )
    }

    /// Plays the take in `folder` in place of the live inputs from `start_frame`, looping or not:
    /// the virtual soundcheck. Refused in words while a take records.
    pub fn start_soundcheck(&self, folder: String, start_frame: u64, looping: bool) -> Flow<Said> {
        let model = self.clone();
        Box::pin(async move {
            if model.console.state().is_recording_now() {
                return model.said(
                    RecordingAction::Soundcheck,
                    false,
                    RECORDING_BLOCKS_SOUNDCHECK,
                );
            }
            let command = Command::StartSoundcheck {
                take_folder: folder,
                start_frame: Some(start_frame),
                looping: Some(looping),
            };
            model
                .soundcheck_command(SoundcheckAction::Start, command, true)
                .await
        })
    }

    /// Pauses the soundcheck's take.
    pub fn pause_soundcheck(&self) -> Flow<Said> {
        let model = self.clone();
        Box::pin(async move {
            model
                .soundcheck_command(SoundcheckAction::Pause, Command::PauseSoundcheck, false)
                .await
        })
    }

    /// Plays the take on (from its start once it stopped at its end).
    pub fn resume_soundcheck(&self) -> Flow<Said> {
        let model = self.clone();
        Box::pin(async move {
            model
                .soundcheck_command(SoundcheckAction::Resume, Command::ResumeSoundcheck, false)
                .await
        })
    }

    /// Whether the take starts again at its end.
    pub fn set_soundcheck_loop(&self, looping: bool) -> Flow<Said> {
        let model = self.clone();
        Box::pin(async move {
            model
                .soundcheck_command(
                    SoundcheckAction::Loop,
                    Command::SetSoundcheckLoop { looping },
                    false,
                )
                .await
        })
    }

    /// Stops the soundcheck: back to the live inputs at once.
    pub fn stop_soundcheck(&self) -> Flow<Said> {
        let model = self.clone();
        Box::pin(async move {
            model
                .soundcheck_command(SoundcheckAction::Stop, Command::StopSoundcheck, true)
                .await
        })
    }

    /// Moves the take to `frame`. Seeks go out one at a time and the last one asked wins: a
    /// slider dragged across the take sends the first move and then only the latest, never every
    /// step. Every caller is answered with the last seek's outcome.
    pub fn seek_soundcheck(&self, frame: u64) -> Flow<Said> {
        let done = Slot::new();
        let starts = {
            let mut inner = self.inner.borrow_mut();
            inner.seeks.wanted = Some(frame);
            match &mut inner.seeks.waiting {
                Some(waiting) => {
                    waiting.push(done.clone());
                    false
                }
                None => {
                    inner.seeks.waiting = Some(vec![done.clone()]);
                    true
                }
            }
        };
        if starts {
            let model = self.clone();
            self.console.spawn(async move {
                let mut said = None;
                loop {
                    let next = model.inner.borrow_mut().seeks.wanted.take();
                    let Some(next) = next else {
                        break;
                    };
                    let command = Command::SeekSoundcheck { frame: next };
                    said = Some(
                        model
                            .soundcheck_command(SoundcheckAction::Seek, command, false)
                            .await,
                    );
                }
                let waiting = model
                    .inner
                    .borrow_mut()
                    .seeks
                    .waiting
                    .take()
                    .unwrap_or_default();
                // At least one seek went out: the one this call asked for.
                let said = said.unwrap_or_else(|| Said {
                    ok: false,
                    message: SoundcheckAction::Seek.refused_lead().to_owned() + ".",
                });
                for slot in waiting {
                    slot.fill(said.clone());
                }
            });
        }
        Box::pin(done.wait())
    }
}

mod controller;

#[cfg(test)]
mod desk;

#[cfg(test)]
mod tests;
