//! The screen's flows for the audio input, the phones and the local output (`network-screen.js`
//! and the parts of `console-app.js` only this screen sends): each is a future on the console's
//! executor, its outcome kept in the screen's own state to be said.

use std::future::Future;

use bandear_desktop_console::command::{Answer, Command};
use bandear_desktop_console::engine_config::{AudioChange, Host};
use bandear_desktop_console::exec::Slot;
use bandear_desktop_console::format::sentence_with_reason;
use bandear_desktop_console::state::Action;
use bandear_desktop_console::types::dto::{
    BufferSetDto, LocalOutputRequestDto, OutputDeviceDto, RedundancySetDto,
};

use super::Model;
use super::RowSaid;
use super::arming::{CONFIRM_DELAY, rest_of_arming};
use super::audio::{
    SwitchPlan, audio_setup_view, device_switch_plan, format_local_output_set, parse_output_value,
};
use super::musicians::{
    BufferAsked, RedundancyAsked, Said, block_ms_of, buffer_request, format_buffer_set,
    format_redundancy_set, name_for_key, validate_address,
};

/// What a local output choice came to: done or not in words, and whether a newer choice came after
/// it (then nothing of it is said).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputOutcome {
    pub ok: bool,
    pub message: Option<String>,
    pub superseded: bool,
}

impl Model {
    // ---- The audio input ----------------------------------------------------------------------

    /// A driver chosen. ASIO is never started on its own: its drivers are listed and the setup
    /// waits for one to be picked (N44). Another driver than the applied one restarts the engine
    /// on it (its default input), then its devices are listed.
    pub fn choose_host(&self, host: Host) {
        let console = self.console().clone();
        let applied = console.state().audio.host;
        if host == Host::Asio && applied != Host::Asio {
            self.update(|local| {
                local.draft.host = Some(Host::Asio);
                local.draft.device = None;
                local.draft.pending_switch = None;
            });
            console.spawn(console.load_devices(Host::Asio));
            return;
        }
        self.update(|local| {
            local.draft.host = None;
            local.draft.device = None;
            local.draft.pending_switch = None;
        });
        let model = self.clone();
        console.clone().spawn(async move {
            if host != applied {
                let change = AudioChange {
                    host: Some(host.name().to_owned()),
                    ..AudioChange::default()
                };
                drop(console.apply_audio(change, None, false).await);
            }
            console.load_devices(host).await;
            model.changed();
        });
    }

    /// A device chosen: while ASIO waits it is the driver picked; otherwise the switch plan
    /// decides (N46): ask first, switch, or nothing.
    pub fn choose_device(&self, name: &str) {
        if self.shared.local.borrow().draft.host == Some(Host::Asio) {
            let picked = (!name.is_empty()).then(|| name.to_owned());
            self.update(|local| local.draft.device = picked);
            return;
        }
        let plan = {
            let draft = self.shared.local.borrow().draft.clone();
            let state = self.console().state();
            let listing = audio_setup_view(&state, &draft).listing;
            let devices = if listing {
                Vec::new()
            } else {
                state.devices.list.clone()
            };
            device_switch_plan(&state, &devices, name)
        };
        match plan {
            SwitchPlan::Confirm { name, message } => {
                self.update(|local| local.draft.pending_switch = Some((name, message)));
            }
            SwitchPlan::Switch { name } => {
                self.update(|local| local.draft.pending_switch = None);
                self.switch(name);
            }
            SwitchPlan::None => self.changed(),
        }
    }

    fn switch(&self, name: String) {
        let console = self.console().clone();
        let model = self.clone();
        console.clone().spawn(async move {
            console.switch_device(name).await;
            model.changed();
        });
    }

    /// "Switch anyway": the switch the engineer was asked about goes ahead.
    pub fn confirm_switch(&self) {
        let pending = self.update(|local| local.draft.pending_switch.take());
        if let Some((name, _)) = pending {
            self.switch(name);
        }
    }

    /// "Cancel": the switch asked about is dropped.
    pub fn cancel_switch(&self) {
        self.update(|local| local.draft.pending_switch = None);
    }

    /// "Start on this driver": the engine starts on the ASIO driver picked, which becomes the
    /// applied one; its drivers are listed again once it runs.
    pub fn apply_asio(&self) {
        let picked = self.shared.local.borrow().draft.device.clone();
        let console = self.console().clone();
        let model = self.clone();
        console.clone().spawn(async move {
            let change = AudioChange {
                host: Some(Host::Asio.name().to_owned()),
                ..AudioChange::default()
            };
            let outcome = console.apply_audio(change, picked, false).await;
            if outcome.is_ok() {
                model.update(|local| {
                    local.draft.host = None;
                    local.draft.device = None;
                });
                console.load_devices(Host::Asio).await;
            }
            model.changed();
        });
    }

    /// A buffer chosen: with an engine running (and no ASIO draft) it restarts on the device in
    /// use; otherwise it is kept for the next start (N50, N51).
    pub fn choose_buffer(&self, frames: u32) {
        let console = self.console().clone();
        let change = AudioChange {
            buffer_frames: Some(Some(f64::from(frames))),
            ..AudioChange::default()
        };
        let drafting = self.shared.local.borrow().draft.host == Some(Host::Asio);
        if drafting || console.state().engine.status.is_none() {
            console.set_audio(change);
            self.changed();
            return;
        }
        let model = self.clone();
        console.clone().spawn(async move {
            drop(console.apply_audio(change, None, true).await);
            model.changed();
        });
    }

    // ---- The phones ------------------------------------------------------------------------------

    /// Runs a command about a musician, then reads the status so the table follows at once;
    /// `Err` with the engine's own words (`musicianCommand`).
    fn musician_command(
        &self,
        command: Command,
    ) -> impl Future<Output = Result<(), String>> + 'static {
        // Sent now; the future it gives borrows the model's console, the flow keeps its own.
        let answer = self.console().run(command);
        let console = self.console().clone();
        async move {
            answer.await?;
            console.read_status().await;
            Ok(())
        }
    }

    /// The address typed is checked first, then added; the field empties once it was (N31, N32).
    pub fn add_phone(&self, text: &str) {
        let address = match validate_address(text) {
            Ok(address) => address,
            Err(message) => {
                self.update(|local| local.add_error = Some(message));
                return;
            }
        };
        let added = self.musician_command(Command::AddTarget {
            address: address.clone(),
        });
        let model = self.clone();
        self.console().spawn(async move {
            match added.await {
                Ok(()) => model.update(|local| {
                    local.cleared.address += 1;
                    local.add_error = None;
                }),
                Err(message) => model.update(|local| {
                    local.add_error = Some(format!("Could not add {address}: {message}"));
                }),
            }
        });
    }

    /// Remove pressed: it asks first; the confirmation works after a pause, and it disarms itself
    /// after a while (N7).
    pub fn arm_remove(&self, address: &str) {
        let generation = self.update(|local| local.remove.arm(address.to_owned()));
        let model = self.clone();
        let console = self.console().clone();
        console.clone().spawn(async move {
            console.sleep(CONFIRM_DELAY).await;
            if !model.update(|local| local.remove.ready(generation)) {
                return;
            }
            console.sleep(rest_of_arming()).await;
            model.update(|local| local.remove.expire(generation));
        });
    }

    /// Keep, or Escape on the row: the musician stays.
    pub fn keep_remove(&self) {
        self.update(|local| local.remove.disarm());
    }

    /// The confirming Remove: works only once ready; a failure shows over the table (N33).
    pub fn confirm_remove(&self, address: &str) {
        if !self.update(|local| local.remove.confirm(&address.to_owned())) {
            return;
        }
        let removed = self.musician_command(Command::RemoveTarget {
            address: address.to_owned(),
        });
        let model = self.clone();
        let address = address.to_owned();
        self.console().spawn(async move {
            let outcome = removed.await;
            model.update(|local| {
                local.table_error = outcome
                    .err()
                    .map(|message| format!("Could not remove {address}: {message}"));
            });
        });
    }

    /// A name committed for the phone at `address` (the field trims it).
    pub fn rename(&self, address: &str, name: &str) {
        let renamed = self.musician_command(Command::SetMusicianName {
            address: address.to_owned(),
            name: name.trim().to_owned(),
        });
        let model = self.clone();
        self.console().spawn(async move {
            let outcome = renamed.await;
            model.update(|local| {
                local.table_error = outcome
                    .err()
                    .map(|message| format!("Could not save the name: {message}"));
            });
        });
    }

    /// History pressed, or the row clicked: the musician's link panel opens, or closes when it is
    /// theirs (N6, N18).
    pub fn select_musician(&self, address: &str) {
        let musicians = self.musicians();
        self.shared.health.select(&musicians, address);
        self.changed();
    }

    /// The link panel's Close.
    pub fn close_health(&self) {
        self.shared.health.close();
        self.changed();
    }

    /// A range chosen in the link panel: the history held is shown over it, nothing is read.
    pub fn set_range(&self, range_ms: u64) {
        self.shared.health.set_range(range_ms);
        self.changed();
    }

    /// Show as table / Hide the table.
    pub fn show_table(&self, shown: bool) {
        self.shared.health.show_table(shown);
        self.changed();
    }

    /// A Buffer chosen on a row (`mode` "auto" or "fixed", the length as typed): checked here,
    /// then sent in turn with the desk's edits (N11, N13); what came of it is said on the row.
    pub fn ask_buffer(&self, address: &str, mode: &str, ms_text: &str) {
        let fixed = mode == "fixed";
        let key = {
            let state = self.console().state();
            state
                .musician_by_address(address)
                .map(super::musicians::musician_key)
        };
        let Some(key) = key else {
            return;
        };
        self.update(|local| {
            local.buffer_asked.insert(
                address.to_owned(),
                BufferAsked {
                    mode: mode.to_owned(),
                    ms: fixed.then(|| ms_text.to_owned()),
                },
            );
        });
        let request = match buffer_request(mode, ms_text) {
            Ok(request) => request,
            Err(message) => {
                self.say_buffer(address, Said::new(false, message));
                return;
            }
        };
        let console = self.console().clone();
        let reader = console.clone();
        // Sent once its turn comes: a load or an edit asked for before it goes first.
        let turn = console.in_turn(false, async move {
            let sent = reader.call::<BufferSetDto>(Command::SetMusicianBuffer {
                musician_key: key.clone(),
                mode: request.mode.name().to_owned(),
                ms: request.ms,
            });
            match sent.await {
                Ok(answer) => {
                    reader.read_status().await;
                    let name = name_for_key(&reader.state().musicians, &key);
                    format_buffer_set(&answer, &name)
                }
                Err(error) => Said::new(
                    false,
                    sentence_with_reason("The buffer was not set", Some(&error)),
                ),
            }
        });
        let model = self.clone();
        let address = address.to_owned();
        console.spawn(async move {
            let said = turn.await;
            model.say_buffer(&address, said);
        });
    }

    fn say_buffer(&self, address: &str, said: Said) {
        self.update(|local| {
            // A new answer for the row settles its choice (N12).
            local.buffer_asked.remove(address);
            local.buffer_said = Some(RowSaid {
                address: address.to_owned(),
                said,
            });
        });
    }

    /// Redundancy switched on a row (with the default delay, an immediate copy) or its Copy
    /// chosen: sent in turn with the desk's edits (N14-N16); what came of it, with what it does to
    /// that musician's bandwidth and latency, is said on the row.
    pub fn ask_redundancy(&self, address: &str, on: bool, delay_blocks: Option<u8>) {
        let key = {
            let state = self.console().state();
            state
                .musician_by_address(address)
                .map(super::musicians::musician_key)
        };
        let Some(key) = key else {
            return;
        };
        self.update(|local| {
            local
                .redundancy_asked
                .insert(address.to_owned(), RedundancyAsked { on, delay_blocks });
        });
        let console = self.console().clone();
        let reader = console.clone();
        let turn = console.in_turn(false, async move {
            let sent = reader.call::<RedundancySetDto>(Command::SetMusicianRedundancy {
                musician_key: key.clone(),
                on,
                delay_blocks: delay_blocks.map(u32::from),
            });
            match sent.await {
                Ok(answer) => {
                    reader.read_status().await;
                    let state = reader.state();
                    let block = block_ms_of(
                        state
                            .engine
                            .status
                            .as_ref()
                            .map(|status| &status.capture_format),
                    );
                    format_redundancy_set(&answer, &name_for_key(&state.musicians, &key), block)
                }
                Err(error) => Said::new(
                    false,
                    sentence_with_reason("Redundancy was not set", Some(&error)),
                ),
            }
        });
        let model = self.clone();
        let address = address.to_owned();
        console.spawn(async move {
            let said = turn.await;
            model.update(|local| {
                local.redundancy_asked.remove(&address);
                local.redundancy_said = Some(RowSaid { address, said });
            });
        });
    }

    // ---- The local output ------------------------------------------------------------------------

    /// Lists the outputs the running engine offers; the answer names the run it was asked for, so
    /// one that comes back after another engine started is not taken (N53). `Some` with the error
    /// of a failed listing.
    pub fn load_outputs(&self) -> impl Future<Output = Option<String>> + 'static {
        let run = self.console().state().engine.run;
        let listed = self
            .console()
            .call::<Vec<OutputDeviceDto>>(Command::OutputDevices);
        let console = self.console().clone();
        async move {
            match listed.await {
                Ok(list) => {
                    console.dispatch(Action::Outputs { list, run });
                    None
                }
                Err(message) => {
                    console.dispatch(Action::OutputsFailed {
                        message: message.clone(),
                        run,
                    });
                    Some(message)
                }
            }
        }
    }

    /// Plays the local mix on `output`, or nowhere, one choice at a time: a choice waits for the
    /// one in flight, and one overtaken while it waits never goes out (`superseded`). The status
    /// is read after each, and nothing that happens to one choice stops the next (C32, N54).
    pub fn set_local_output(
        &self,
        output: Option<LocalOutputRequestDto>,
    ) -> impl Future<Output = OutputOutcome> + 'static {
        let (choice, turn) = {
            let mut lane = self.shared.output.borrow_mut();
            lane.choice += 1;
            let turn = if lane.busy {
                let slot = Slot::new();
                lane.waiting.push_back(slot.clone());
                Some(slot)
            } else {
                lane.busy = true;
                None
            };
            (lane.choice, turn)
        };
        let model = self.clone();
        async move {
            if let Some(turn) = turn {
                turn.wait().await;
            }
            let overtaken = |model: &Model| model.shared.output.borrow().choice != choice;
            let outcome = if overtaken(&model) {
                OutputOutcome {
                    ok: true,
                    message: None,
                    superseded: true,
                }
            } else {
                let console = model.console().clone();
                let (ok, words) = match console.run(Command::SetLocalOutput { output }).await {
                    Err(error) => (
                        false,
                        sentence_with_reason("The local mix output was not changed", Some(&error)),
                    ),
                    Ok(Answer::LocalOutputSet(answer)) => (true, format_local_output_set(&answer)),
                    Ok(_) => (
                        true,
                        sentence_with_reason(
                            "The local mix output was changed, but the desk's answer could not be read",
                            Some(
                                "the desk answered `set_local_output` with something it does not answer",
                            ),
                        ),
                    ),
                };
                let words = match console.read_status().await {
                    Some(unread) => format!(
                        "{words} {}",
                        sentence_with_reason("Where it plays now could not be read", Some(&unread))
                    ),
                    None => words,
                };
                OutputOutcome {
                    ok,
                    message: Some(words),
                    superseded: overtaken(&model),
                }
            };
            let next = {
                let mut lane = model.shared.output.borrow_mut();
                let next = lane.waiting.pop_front();
                if next.is_none() {
                    lane.busy = false;
                }
                next
            };
            // The lane stays busy and passes to the next choice.
            if let Some(next) = next {
                next.fill(());
            }
            outcome
        }
    }

    /// A choice in the Local mix out select (its value); only the last choice's outcome is said.
    pub fn choose_output(&self, value: &str) {
        let chosen = self.set_local_output(parse_output_value(value));
        let model = self.clone();
        self.console().spawn(async move {
            let outcome = chosen.await;
            if !outcome.superseded {
                model.update(|local| {
                    local.output_said =
                        Some(Said::new(outcome.ok, outcome.message.unwrap_or_default()));
                });
            }
        });
    }
}
