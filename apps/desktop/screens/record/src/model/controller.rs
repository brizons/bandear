//! What the screen does between the console and its controls (`record-controller.js`): the
//! transport's guard, the lanes' peak histories, arming, the folder field and the soundcheck's
//! Play, Stop, seek and loop.

use std::future::Future;

use bandear_desktop_console::state::ConsoleState;
use bandear_desktop_console::wire::Meters;

use super::{Busy, Flow, LANE_CAPACITY, Lane, Model, Said};
use crate::record_view::{
    PeakHistory, TrackRow, arm_change, local_track_row, settings_view, track_level, track_rows,
};
use crate::recording::SettingsChange;
use crate::soundcheck::frame_at;
use crate::soundcheck_view::{PlayAction, soundcheck_play};

fn nothing() -> Flow<Option<Said>> {
    Box::pin(async { None })
}

impl Model {
    /// Runs the transport command `busy`: `run` is called only when no other command is on its
    /// way, and its buttons wait until it is answered. `None` when another was on its way (the
    /// press is not run).
    pub fn command<T: 'static, F: Future<Output = T> + 'static>(
        &self,
        busy: Busy,
        run: impl FnOnce() -> F,
    ) -> Flow<Option<T>> {
        let free = {
            let mut inner = self.inner.borrow_mut();
            let free = inner.local.busy.is_none();
            if free {
                inner.local.busy = Some(busy);
            }
            free
        };
        let flow = free.then(run);
        let model = self.clone();
        Box::pin(async move {
            let outcome = flow?.await;
            model.inner.borrow_mut().local.busy = None;
            Some(outcome)
        })
    }

    /// A move of the take: it shares the transport's guard ("move"), so none goes out while
    /// another command is on its way (a move during Stop would be refused and its words would
    /// replace the stop's), but moves do not wait for each other: the console sends the latest.
    fn move_take(&self, run: impl FnOnce() -> Flow<Said>) -> Flow<Option<Said>> {
        let free = {
            let mut inner = self.inner.borrow_mut();
            let free = inner.local.busy.is_none_or(|busy| busy == Busy::Move);
            if free {
                inner.moving += 1;
                inner.local.busy = Some(Busy::Move);
            }
            free
        };
        let flow = free.then(run);
        let model = self.clone();
        Box::pin(async move {
            let said = flow?.await;
            let mut inner = model.inner.borrow_mut();
            inner.moving -= 1;
            if inner.moving == 0 {
                inner.local.busy = None;
            }
            Some(said)
        })
    }

    fn soundchecking(&self) -> bool {
        self.console.state().is_soundcheck_now()
    }

    /// Record: starts a take called `name` through the transport's guard.
    pub fn record(&self, name: String) -> Flow<Option<Said>> {
        let model = self.clone();
        self.command(Busy::Start, move || model.start_recording(name))
    }

    /// Add marker: marks the take being recorded with the desk's next name.
    pub fn mark(&self) -> Flow<Option<Said>> {
        let model = self.clone();
        self.command(Busy::Mark, move || model.add_marker(String::new()))
    }

    /// Play: starts the take chosen in the soundcheck card (else the last take) as a soundcheck,
    /// pauses the take playing, or plays a paused one on. `None` when Play cannot be pressed or
    /// another command is on its way.
    pub fn play(&self) -> Flow<Option<Said>> {
        let (view, looping) = {
            let local = self.local();
            let state = self.console.state();
            (
                soundcheck_play(&state, local.chosen_take.as_deref()),
                local.loop_next,
            )
        };
        if !view.can {
            return nothing();
        }
        let model = self.clone();
        match (view.action, view.folder) {
            (Some(PlayAction::Start), Some(folder)) => self.command(Busy::Play, move || {
                model.start_soundcheck(folder, 0, looping)
            }),
            (Some(PlayAction::Pause), _) => {
                self.command(Busy::Play, move || model.pause_soundcheck())
            }
            (Some(PlayAction::Resume), _) => {
                self.command(Busy::Play, move || model.resume_soundcheck())
            }
            _ => nothing(),
        }
    }

    /// A take row's Soundcheck (and the card's Start soundcheck): plays the take in `folder` in
    /// place of the live inputs.
    pub fn soundcheck_take(&self, folder: String) -> Flow<Option<Said>> {
        let looping = self.local().loop_next;
        let model = self.clone();
        self.command(Busy::Soundcheck, move || {
            model.start_soundcheck(folder, 0, looping)
        })
    }

    /// Stop: back to the live inputs while a take plays in place of them, else stops the take
    /// recording and reads the disk estimate again.
    pub fn stop(&self) -> Flow<Option<Said>> {
        let model = self.clone();
        if self.soundchecking() {
            return self.command(Busy::Stop, move || model.stop_soundcheck());
        }
        self.command(Busy::Stop, move || async move {
            let said = model.stop_recording().await;
            model.console.refresh_estimate().await;
            said
        })
    }

    /// Back to start: the take playing in place of the inputs moves to its start; otherwise the
    /// position shown goes back from the last take's length to 0:00.
    pub fn rewind(&self) -> Flow<Option<Said>> {
        if self.soundchecking() {
            let model = self.clone();
            return self.move_take(move || model.seek_soundcheck(0));
        }
        self.inner.borrow_mut().local.at_start = true;
        nothing()
    }

    /// The take the soundcheck card picks for Play (`None` or blank: the last take).
    pub fn choose_take(&self, folder: Option<String>) {
        self.inner.borrow_mut().local.chosen_take = folder.filter(|folder| !folder.is_empty());
    }

    /// The seek slider is being dragged to `seconds`: the position shows it until it is let go.
    pub fn drag(&self, seconds: f64) {
        self.inner.borrow_mut().local.drag = Some(seconds);
    }

    /// The seek slider let go at `seconds`: the take moves there. `None`, and nothing sent, while
    /// another transport command is on its way.
    pub fn seek_to(&self, seconds: f64) -> Flow<Option<Said>> {
        self.inner.borrow_mut().local.drag = None;
        let length = self.console.state().soundcheck_status().length_frames;
        let model = self.clone();
        self.move_take(move || model.seek_soundcheck(frame_at(seconds, length)))
    }

    /// The loop: the take playing starts again at its end (or not), through the transport's
    /// guard; with the inputs live, the choice is kept for the next start.
    pub fn set_loop(&self, on: bool) -> Flow<Option<Said>> {
        if self.soundchecking() {
            let model = self.clone();
            return self.command(Busy::Loop, move || model.set_soundcheck_loop(on));
        }
        self.inner.borrow_mut().local.loop_next = on;
        nothing()
    }

    /// Arms or disarms the row `key` (a track or the local mix); `None` for a row not shown. A
    /// press made while an earlier one on the same row is on its way toggles what that one
    /// asked, so a double press ends where it began.
    pub fn arm(&self, key: &str) -> Flow<Option<Said>> {
        let change = {
            let state = self.console.state();
            let row = track_rows(&state)
                .into_iter()
                .chain(local_track_row(&state))
                .find(|row| row.key == key);
            let Some(row) = row else {
                return nothing();
            };
            let mut inner = self.inner.borrow_mut();
            let armed_now = inner
                .intents
                .get(key)
                .map_or(row.armed, |&(_, asked)| asked);
            inner.next_intent += 1;
            let id = inner.next_intent;
            inner.intents.insert(key.to_owned(), (id, !armed_now));
            (id, arm_change(&state, &row, armed_now))
        };
        let (id, change) = change;
        let flow = self.set_recording_settings(change);
        let model = self.clone();
        let key = key.to_owned();
        Box::pin(async move {
            let said = flow.await;
            let mut inner = model.inner.borrow_mut();
            if inner
                .intents
                .get(&key)
                .is_some_and(|&(intent, _)| intent == id)
            {
                inner.intents.remove(&key);
            }
            Some(said)
        })
    }

    /// Asks for the settings `change` (the format, the local mix).
    pub fn change_settings(&self, change: SettingsChange) -> Flow<Option<Said>> {
        let flow = self.set_recording_settings(change);
        Box::pin(async move { Some(flow.await) })
    }

    /// Asks for the recordings folder `text`; a kept one is listed and shown in the field.
    pub fn use_folder(&self, text: String) -> Flow<Option<Said>> {
        let flow = self.set_recording_settings(SettingsChange {
            folder: Some(Some(text)),
            ..SettingsChange::default()
        });
        let model = self.clone();
        Box::pin(async move {
            let said = flow.await;
            if said.ok {
                model.inner.borrow_mut().shown_folder = None;
                model.refresh_takes().await;
            }
            Some(said)
        })
    }

    /// Goes back to the default recordings folder and lists its takes.
    pub fn use_default_folder(&self) -> Flow<Option<Said>> {
        self.inner.borrow_mut().shown_folder = None;
        let flow = self.set_recording_settings(SettingsChange {
            folder: Some(None),
            ..SettingsChange::default()
        });
        let model = self.clone();
        Box::pin(async move {
            let said = flow.await;
            model.refresh_takes().await;
            Some(said)
        })
    }

    /// The folder to write into the folder field for `state`, or `None` to leave the field as it
    /// is: already shown, or the engineer is `typing` in it.
    pub fn folder_to_show(&self, state: &ConsoleState, typing: bool) -> Option<String> {
        let folder = settings_view(state).folder;
        let mut inner = self.inner.borrow_mut();
        if typing || inner.shown_folder.as_deref() == Some(folder.as_str()) {
            return None;
        }
        inner.shown_folder = Some(folder.clone());
        Some(folder)
    }

    /// Follows the take of `state`: a new take folder starts the lanes empty and the position
    /// counts from it. True when a new take was seen.
    pub fn follow_take(&self, state: &ConsoleState) -> bool {
        let recording = state.recording_status();
        let Some(folder) = recording.take_folder else {
            return false;
        };
        let mut inner = self.inner.borrow_mut();
        if inner.take.as_deref() == Some(folder.as_str()) {
            return false;
        }
        inner.take = Some(folder);
        inner.local.at_start = false;
        for history in inner.histories.values_mut() {
            history.clear();
        }
        true
    }

    /// Whether a take records now: only then do the lanes keep peaks.
    pub fn taking(&self) -> bool {
        self.console.state().recording_status().state == "recording"
    }

    /// The level of each of `rows` from `meters`, in order; while a take records, the armed rows
    /// keep it in their history.
    pub fn take_meters(&self, rows: &[TrackRow], meters: &Meters) -> Vec<f64> {
        let keep = self.taking();
        let mut inner = self.inner.borrow_mut();
        rows.iter()
            .map(|row| {
                let level = track_level(row, meters);
                if keep && row.armed {
                    inner
                        .histories
                        .entry(row.key.clone())
                        .or_insert_with(|| PeakHistory::new(LANE_CAPACITY))
                        .push(level as f32);
                }
                level
            })
            .collect()
    }

    /// The peak history of the row `key`, or `None` when it has kept none.
    pub fn history(&self, key: &str) -> Option<PeakHistory> {
        self.inner.borrow().histories.get(key).cloned()
    }

    /// What the lane of `row` shows: off unarmed, recording while a take records or once it kept
    /// peaks, armed otherwise.
    pub fn lane_of(&self, row: &TrackRow, state: &ConsoleState) -> Lane {
        if !row.armed {
            return Lane::Off;
        }
        let live = state.recording_status().is_recording();
        let kept = self
            .inner
            .borrow()
            .histories
            .get(&row.key)
            .is_some_and(|history| !history.is_empty());
        if live || kept {
            Lane::Recording
        } else {
            Lane::Armed
        }
    }
}
