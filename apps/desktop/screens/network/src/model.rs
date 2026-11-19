//! The Network and system screen's model (`network-screen.js` without the DOM): its own state and
//! flows, over the console. Pure: no window, tested with plain `#[test]`s over the console's
//! simulated desk.
//!
//! The screen's own state is what the web screen kept beside the store (`local`, :74-88) and what
//! its components kept that is behaviour rather than drawing: an ASIO driver being picked, a switch
//! waiting for the engineer's word, the last outcome of each kind, the choices on their way, the
//! armed removals and the Session panel's choice. Everything else is the console's state, read
//! fresh each time ([Model::screen]).
//!
//! The commands this screen sends are its own (lane ownership): the audio choices, devices and
//! outputs, adding, removing and naming phones, each musician's buffer and redundancy, the local
//! output, sessions and mix backups, and link history. The engine's lifecycle steps
//! ([Console::apply_audio], [Console::set_audio], [Console::switch_device],
//! [Console::load_devices]) are the console's, and called, never re-implemented.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use bandear_desktop_console::console::Console;
use bandear_desktop_console::state::Action;

pub mod arming;
pub mod audio;
pub mod clock;
mod flows;
pub mod link_chart;
pub mod link_follow;
pub mod link_view;
pub mod musicians;
mod session_flows;
pub mod sessions;

use arming::Arming;
use audio::{
    AudioSetupView, Draft, LocalOutputView, audio_message, audio_setup_view, command_message,
    local_output_view,
};
use clock::Zone;
use link_follow::LinkHealth;
use link_view::PanelView;
use musicians::{
    BufferAsked, BufferMode, MusicianRow, RedundancyAsked, Said, buffer_choice_settled,
    musician_rows, redundancy_choice_settled,
};
use sessions::{
    Asking, BackupRow, BackupsPanel, BackupsView, SessionPanel, SessionPanelView, SessionRow,
    mix_backup_rows, mix_backups_view, session_panel_view,
};

/// What a row's outcome line says, for the row at `address`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowSaid {
    pub address: String,
    pub said: Said,
}

/// Counters a field's view compares to empty the field when one moves: the address after a phone
/// was added, the session name after a save, the new name after a rename.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cleared {
    pub address: u64,
    pub save_name: u64,
    pub rename: u64,
}

#[derive(Default)]
pub(crate) struct Local {
    pub draft: Draft,
    pub add_error: Option<String>,
    pub table_error: Option<String>,
    /// The last change of the local output, said until the next one.
    pub output_said: Option<Said>,
    pub buffer_said: Option<RowSaid>,
    pub redundancy_said: Option<RowSaid>,
    /// The Buffer and Redundancy choices on their way, by address (N12, N16).
    pub buffer_asked: BTreeMap<String, BufferAsked>,
    pub redundancy_asked: BTreeMap<String, RedundancyAsked>,
    /// The Remove armed, by address (N7).
    pub remove: Arming<String>,
    pub panel: SessionPanel,
    pub backups: BackupsPanel,
    /// The Session panel's last outcome.
    pub session_said: Option<Said>,
    pub cleared: Cleared,
    /// True while the screen is shown: it follows the engine runs for their outputs then.
    pub mounted: bool,
    /// The engine run whose outputs were listed (`followOutputs`).
    pub outputs_listed: Option<u64>,
}

type Notify = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// The local output's lane (C32): one choice at a time, a choice overtaken while it waits is not
/// sent.
#[derive(Default)]
pub(crate) struct OutputLane {
    pub busy: bool,
    pub waiting: std::collections::VecDeque<bandear_desktop_console::exec::Slot<()>>,
    /// Counts the choices, so each knows whether a newer one came after it.
    pub choice: u64,
}

pub(crate) struct Shared {
    pub console: Console,
    pub local: RefCell<Local>,
    pub health: LinkHealth,
    pub output: RefCell<OutputLane>,
    zone: Cell<Zone>,
    notify: Notify,
}

/// The screen's model, built once as the window opens and kept for its life, so its flows and
/// lanes go on while another screen is shown. Clones share it.
#[derive(Clone)]
pub struct Model {
    pub(crate) shared: Rc<Shared>,
}

/// A row as the table draws it: the row, the choices still on their way shown over it, the last
/// outcomes and the Remove's state.
#[derive(Clone, Debug, PartialEq)]
pub struct ShownRow {
    pub row: MusicianRow,
    /// The Buffer mode shown: a choice on its way, else the reported one.
    pub buffer_mode: BufferMode,
    /// True while a Buffer choice is on its way (the length field is not overwritten then).
    pub buffer_pending: bool,
    pub redundancy_on: bool,
    pub redundancy_delay: u8,
    pub buffer_said: Option<Said>,
    pub redundancy_said: Option<Said>,
    pub selected: bool,
    /// `None` while idle; `Some(ready)` while the Remove asks.
    pub removing: Option<bool>,
}

/// What the musicians table shows.
#[derive(Clone, Debug, PartialEq)]
pub struct TableView {
    pub rows: Vec<ShownRow>,
    pub add_error: Option<String>,
    pub table_error: Option<String>,
}

/// What the Session panel shows, with its own state.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionView {
    pub panel: SessionPanelView,
    pub chosen: Option<SessionRow>,
    pub load_ready: bool,
    pub asking: Option<Asking>,
    /// Whether the delete is armed for the chosen session, and whether it can be confirmed.
    pub delete: (bool, bool),
    pub backups: BackupsView,
    pub restore_asking: Option<BackupRow>,
    pub said: Option<Said>,
}

/// Everything the screen draws, worked out once per frame.
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenView {
    /// False with no engine: the note above the body says no phone is listed.
    pub engine_running: bool,
    pub command: Option<String>,
    pub audio_message: Option<String>,
    pub table: TableView,
    pub health: Option<PanelView>,
    pub audio: AudioSetupView,
    pub local_output: LocalOutputView,
    pub output_said: Option<Said>,
    pub session: SessionView,
    pub cleared: Cleared,
}

impl Model {
    /// The model over `console` (a handle that sends its commands for this screen). It listens to
    /// the console: a new engine run lists its outputs while the screen is shown, and a musician
    /// leaving the list closes their link panel.
    pub fn new(console: Console) -> Self {
        let notify: Notify = Rc::new(RefCell::new(None));
        let woken = Rc::clone(&notify);
        let health = LinkHealth::new(
            &console,
            Rc::new(move || {
                let notify = woken.borrow().clone();
                if let Some(notify) = notify {
                    notify();
                }
            }),
        );
        let shared = Rc::new(Shared {
            console: console.clone(),
            local: RefCell::new(Local::default()),
            health,
            output: RefCell::new(OutputLane::default()),
            zone: Cell::new(Zone::Local),
            notify,
        });
        let weak: Weak<Shared> = Rc::downgrade(&shared);
        console.subscribe(move |_, action| {
            if let Some(shared) = weak.upgrade() {
                Model { shared }.heard(action);
            }
        });
        Self { shared }
    }

    /// The console this screen talks to the desk through.
    pub fn console(&self) -> &Console {
        &self.shared.console
    }

    /// Calls `notify` whenever the screen's own state changes, so the view draws again (the
    /// console's changes reach it through the hub).
    pub fn on_change(&self, notify: impl Fn() + 'static) {
        *self.shared.notify.borrow_mut() = Some(Rc::new(notify));
    }

    /// Shows the times in `zone` (the system's by default).
    pub fn set_zone(&self, zone: Zone) {
        self.shared.zone.set(zone);
    }

    pub(crate) fn changed(&self) {
        let notify = self.shared.notify.borrow().clone();
        if let Some(notify) = notify {
            notify();
        }
    }

    pub(crate) fn update<T>(&self, change: impl FnOnce(&mut Local) -> T) -> T {
        let value = change(&mut self.shared.local.borrow_mut());
        self.changed();
        value
    }

    /// The listed musicians now, out of the state (never held across a step of the console).
    pub(crate) fn musicians(&self) -> Vec<bandear_desktop_console::wire::Musician> {
        self.shared.console.state().musicians.clone()
    }

    fn heard(&self, action: &Action) {
        if matches!(action, Action::Status(_) | Action::Engine { .. }) {
            let musicians = self.musicians();
            self.shared.health.follow_list(&musicians);
        }
        if self.shared.local.borrow().mounted {
            self.follow_outputs();
        }
    }

    /// The screen is shown: the devices of the applied driver and the mix backups are listed
    /// afresh, and the outputs once for the engine run (0.3).
    pub fn mount(&self) {
        self.shared.local.borrow_mut().mounted = true;
        self.shared.local.borrow_mut().outputs_listed = None;
        let console = self.console().clone();
        let host = console.state().audio.host;
        console.spawn(console.load_devices(host));
        let backups = console.load_mix_backups();
        console.spawn(async move {
            drop(backups.await);
        });
        self.follow_outputs();
    }

    /// The screen is no longer shown: its link panel closes and stops reading (0.3).
    pub fn unmount(&self) {
        self.shared.local.borrow_mut().mounted = false;
        self.shared.health.close();
        self.changed();
    }

    /// Lists the outputs once for each engine run the console shows, never while none runs.
    fn follow_outputs(&self) {
        let run = {
            let state = self.shared.console.state();
            state.engine.status.as_ref().and(state.engine.run)
        };
        let fresh = {
            let mut local = self.shared.local.borrow_mut();
            if local.outputs_listed == run {
                false
            } else {
                local.outputs_listed = run;
                run.is_some()
            }
        };
        if fresh {
            let listing = self.load_outputs();
            self.shared.console.spawn(listing);
        }
    }

    /// Everything the screen draws now. It also settles what the web components settled as they
    /// drew: a choice the row reports now, a Remove whose musician left, a session or backup no
    /// longer listed.
    pub fn screen(&self) -> ScreenView {
        let zone = self.shared.zone.get();
        let musicians = self.musicians();
        let (selected, health) = self.shared.health.view(&musicians, zone);
        let console = &self.shared.console;
        let draft = self.shared.local.borrow().draft.clone();
        let (
            rows,
            audio,
            local_output,
            sessions,
            backups,
            backup_rows,
            engine_running,
            command,
            notice,
        ) = {
            let state = console.state();
            (
                musician_rows(&state),
                audio_setup_view(&state, &draft),
                local_output_view(&state),
                session_panel_view(&state, zone),
                mix_backups_view(&state, zone),
                mix_backup_rows(state.mix_backups.list.as_deref().unwrap_or(&[]), zone),
                state.engine.status.is_some(),
                command_message(&state),
                audio_message(&state),
            )
        };
        let mut local = self.shared.local.borrow_mut();
        let local = &mut *local;
        if let Some(armed) = local.remove.armed().cloned()
            && !rows.iter().any(|row| row.address == armed)
        {
            local.remove.disarm();
        }
        local.panel.set_rows(sessions.rows.clone());
        local.backups.set_rows(backup_rows);
        let shown = rows
            .into_iter()
            .map(|row| {
                let address = row.address.clone();
                if local
                    .buffer_asked
                    .get(&address)
                    .is_some_and(|asked| buffer_choice_settled(asked, &row.buffer))
                {
                    local.buffer_asked.remove(&address);
                }
                if local
                    .redundancy_asked
                    .get(&address)
                    .is_some_and(|asked| redundancy_choice_settled(asked, &row.redundancy))
                {
                    local.redundancy_asked.remove(&address);
                }
                let buffer_asked = local.buffer_asked.get(&address);
                let redundancy_asked = local.redundancy_asked.get(&address);
                let said_for = |said: &Option<RowSaid>| {
                    said.as_ref()
                        .filter(|said| said.address == address)
                        .map(|said| said.said.clone())
                };
                ShownRow {
                    buffer_mode: match buffer_asked.map(|asked| asked.mode.as_str()) {
                        Some("fixed") => BufferMode::Fixed,
                        Some(_) => BufferMode::Auto,
                        None => row.buffer.mode,
                    },
                    buffer_pending: buffer_asked.is_some(),
                    redundancy_on: redundancy_asked.map_or(row.redundancy.on, |asked| asked.on),
                    redundancy_delay: redundancy_asked
                        .and_then(|asked| asked.delay_blocks)
                        .or(row.redundancy.delay_blocks)
                        .unwrap_or(0),
                    buffer_said: said_for(&local.buffer_said),
                    redundancy_said: said_for(&local.redundancy_said),
                    selected: selected.as_deref() == Some(address.as_str()),
                    removing: local
                        .remove
                        .is_armed(&address)
                        .then(|| local.remove.is_ready(&address)),
                    row,
                }
            })
            .collect();
        ScreenView {
            engine_running,
            command,
            audio_message: notice,
            table: TableView {
                rows: shown,
                add_error: local.add_error.clone(),
                table_error: local.table_error.clone(),
            },
            health,
            audio,
            local_output,
            output_said: local.output_said.clone(),
            session: SessionView {
                chosen: local.panel.chosen().cloned(),
                load_ready: local.panel.load_ready(),
                asking: local.panel.asking(),
                delete: local.panel.delete_state(),
                panel: sessions,
                restore_asking: local.backups.asking().cloned(),
                backups,
                said: local.session_said.clone(),
            },
            cleared: local.cleared,
        }
    }
}

#[cfg(test)]
#[path = "model/model_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "model/link_tests.rs"]
mod link_tests;

#[cfg(test)]
#[path = "model/session_tests.rs"]
mod session_tests;
