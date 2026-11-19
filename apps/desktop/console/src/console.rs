//! The console: the one place that talks to the desk and keeps the state (`console-app.js`). It
//! lives as long as the window; screens come and go and use it. No window, no thread: a state
//! machine whose steps are an intent, a reply, a timer or an event, each returning its effects.
//!
//! - Connecting listens to the engine's events once, reads the audio choices the desk's session
//!   keeps (until a read succeeds), then the engine's state. When no engine runs (an answer, not an
//!   error) it starts one with the old window's request on those choices, unless `auto_start` is
//!   off. A failed listen or state read is the console's connection error, and so is an unread
//!   choice when it would start: nothing starts on the defaults in place of a kept input. A second
//!   connect while one runs joins it.
//! - The audio choices are applied only once an engine started or switched with them, and only
//!   then kept in the session. A plain start whose input is gone starts on the default input
//!   instead, never on ASIO, says so, and keeps the choices for the next start.
//! - Start and stop drive the engine and its phase; a start, a failed start and a confirmed stop
//!   end the current run. A refused stop or an unreadable status after a start ends nothing.
//! - Meters skip the state: the latest of the live run is [Console::meters], and listeners get
//!   each one.
//! - The window's title follows the device ([window_title]).
//!
//! The flows are futures on the console's own executor ([crate::exec]); they await the desk's
//! replies through tickets, so the web console's async code ports line for line and stays
//! deterministic. Screens write theirs the same way (see the crate's documentation).

use std::cell::{Ref, RefCell};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

use crate::command::{Command, FromAnswer, Owner, Reply, Ticket};
use crate::demo_source::is_demo_running;
use crate::engine_config::{
    ASIO_NEEDS_DRIVER, Audio, AudioChange, Host, audio_choice, fallback_audio, fallback_notice,
    remembered_device, restored_audio, settings_request, start_request,
};
use crate::exec::{Executor, Slot};
use crate::format::sentence_with_reason;
use crate::state::{Action, AudioNotice, AudioNoticeKind, CommandAction, ConsoleState, Phase};
use crate::wire::{EngineEvent, EngineState, Meters};

/// After a start whose status could not be read: how often to ask again.
pub const STATUS_RETRIES: u32 = 3;

/// How the console behaves.
#[derive(Clone, Debug)]
pub struct Options {
    /// Start an engine when none runs as the console connects.
    pub auto_start: bool,
    /// The wait between status reads after a start whose status could not be read.
    pub status_retry: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            auto_start: true,
            status_retry: Duration::from_millis(500),
        }
    }
}

/// What the console asks of whoever runs it (the app, or a test's harness).
#[derive(Debug)]
pub enum Effect {
    /// Run `command` on the desk and hand the reply to [Console::answer] with `ticket`.
    Run {
        ticket: Ticket,
        owner: Owner,
        command: Command,
    },
    /// Call [Console::fire] with `ticket` once `after` has passed.
    Timer { ticket: Ticket, after: Duration },
    /// Start delivering the engine's events to [Console::event], then say whether that worked
    /// with [Console::listened].
    Listen { ticket: Ticket },
    /// The window's title is now this.
    SetTitle(String),
}

/// The window's title: the old window's, after the input device in use, or "Demo source".
pub fn window_title(state: &ConsoleState) -> String {
    if is_demo_running(state.engine.status.as_ref()) {
        return "BandEar Server \u{2014} Demo source".to_owned();
    }
    match state
        .engine
        .status
        .as_ref()
        .map(|status| status.device_name.as_str())
    {
        Some(device) if !device.is_empty() => format!("BandEar Server \u{2014} {device}"),
        _ => "BandEar Server".to_owned(),
    }
}

/// A listener of the state: every action that changed it, with the console to read it from.
pub type Subscriber = Rc<dyn Fn(&Console, &Action)>;

/// A listener of the meters of the live run.
pub type MeterListener = Rc<dyn Fn(&Meters)>;

/// What a running flow gets back from the console's commands of its own.
pub type Outcome = Result<(), String>;

struct Inner {
    state: ConsoleState,
    options: Options,
    next_ticket: u64,
    effects: VecDeque<Effect>,
    replies: HashMap<Ticket, Slot<Reply>>,
    timers: HashMap<Ticket, Slot<()>>,
    listens: HashMap<Ticket, Slot<Outcome>>,
    meters: Option<Rc<Meters>>,
    meter_listeners: Vec<(u64, MeterListener)>,
    subscribers: Vec<(u64, Subscriber)>,
    next_listener: u64,
    queued: VecDeque<Action>,
    dispatching: bool,
    changed: bool,
    waker: Option<Rc<dyn Fn()>>,
    title: String,
    listening: bool,
    disposed: bool,
    /// The audio choices are known once read from the session, or once the engineer made one
    /// (which a later read must not replace).
    choice_known: bool,
    /// The applied choices differ from what the session keeps: the next start that runs them
    /// saves them.
    choice_unsaved: bool,
    /// The connection attempt running now, with whoever waits for it.
    connecting: Option<Vec<Slot<()>>>,
    /// The desk lane: whether a turn runs, and the turns waiting for it.
    desk_busy: bool,
    desk_waiting: VecDeque<Slot<()>>,
    /// The channel and group edits (and other turns counted as edits) asked for and not over.
    edits_on_their_way: usize,
    /// While a saved session loads, no edit goes out.
    holding: bool,
    released: Vec<(u64, Rc<dyn Fn(&Console)>)>,
}

struct Core {
    exec: Executor,
    inner: RefCell<Inner>,
}

/// The console, shared: every clone is the same console, tagged with the [Owner] its commands are
/// sent for.
#[derive(Clone)]
pub struct Console {
    core: Rc<Core>,
    owner: Owner,
}

impl Console {
    /// A console that has heard nothing yet; [Console::connect] starts it.
    pub fn new(options: Options) -> Self {
        let state = ConsoleState::new();
        let title = window_title(&state);
        Self {
            core: Rc::new(Core {
                exec: Executor::default(),
                inner: RefCell::new(Inner {
                    state,
                    options,
                    next_ticket: 0,
                    effects: VecDeque::new(),
                    replies: HashMap::new(),
                    timers: HashMap::new(),
                    listens: HashMap::new(),
                    meters: None,
                    meter_listeners: Vec::new(),
                    subscribers: Vec::new(),
                    next_listener: 0,
                    queued: VecDeque::new(),
                    dispatching: false,
                    changed: false,
                    waker: None,
                    title,
                    listening: false,
                    disposed: false,
                    choice_known: false,
                    choice_unsaved: false,
                    connecting: None,
                    desk_busy: false,
                    desk_waiting: VecDeque::new(),
                    edits_on_their_way: 0,
                    holding: false,
                    released: Vec::new(),
                }),
            }),
            owner: Owner::Console,
        }
    }

    /// The same console, sending its commands for `owner`.
    pub fn for_owner(&self, owner: Owner) -> Self {
        Self {
            core: Rc::clone(&self.core),
            owner,
        }
    }

    /// Whom this handle sends its commands for.
    pub fn owner(&self) -> Owner {
        self.owner
    }

    /// The console's options.
    pub fn options(&self) -> Options {
        self.inner().options.clone()
    }

    fn inner(&self) -> Ref<'_, Inner> {
        self.core.inner.borrow()
    }

    fn with<T>(&self, change: impl FnOnce(&mut Inner) -> T) -> T {
        change(&mut self.core.inner.borrow_mut())
    }

    /// The state now. Read it and let go: never hold it across an `await` or a dispatch.
    pub fn state(&self) -> Ref<'_, ConsoleState> {
        Ref::map(self.inner(), |inner| &inner.state)
    }

    /// The latest meters of the live run, if any came.
    pub fn meters(&self) -> Option<Rc<Meters>> {
        self.inner().meters.clone()
    }

    // ---- What runs the console calls -------------------------------------------------------

    /// Called whenever an effect is queued or the state or the meters changed, so the app can
    /// drain the effects and draw again. It must not call back into the console at once.
    pub fn set_waker(&self, waker: Rc<dyn Fn()>) {
        self.with(|inner| inner.waker = Some(waker));
    }

    fn wake(&self) {
        let waker = self.inner().waker.clone();
        if let Some(waker) = waker {
            waker();
        }
    }

    /// The effects queued since the last call, in order.
    pub fn take_effects(&self) -> Vec<Effect> {
        self.with(|inner| inner.effects.drain(..).collect())
    }

    /// True when the state or the meters changed since the last call: the views draw again.
    pub fn take_changed(&self) -> bool {
        self.with(|inner| std::mem::take(&mut inner.changed))
    }

    fn queue(&self, effect: Effect) {
        self.with(|inner| inner.effects.push_back(effect));
        self.wake();
    }

    fn ticket(&self) -> Ticket {
        self.with(|inner| {
            inner.next_ticket += 1;
            Ticket(inner.next_ticket)
        })
    }

    /// The desk's reply to the command sent with `ticket`.
    pub fn answer(&self, ticket: Ticket, reply: Reply) {
        let slot = self.with(|inner| inner.replies.remove(&ticket));
        if let Some(slot) = slot {
            slot.fill(reply);
        }
        self.drive();
    }

    /// The timer started with `ticket` ended.
    pub fn fire(&self, ticket: Ticket) {
        let slot = self.with(|inner| inner.timers.remove(&ticket));
        if let Some(slot) = slot {
            slot.fill(());
        }
        self.drive();
    }

    /// Whether listening to the engine's events, asked with `ticket`, worked.
    pub fn listened(&self, ticket: Ticket, outcome: Outcome) {
        let slot = self.with(|inner| inner.listens.remove(&ticket));
        if let Some(slot) = slot {
            slot.fill(outcome);
        }
        self.drive();
    }

    /// An event of the engine. Heard only while the console listens; meters of a run that is not
    /// live are dropped.
    pub fn event(&self, event: EngineEvent) {
        let heard = {
            let inner = self.inner();
            inner.listening && !inner.disposed
        };
        if !heard {
            return;
        }
        match event {
            EngineEvent::Status(report) => {
                self.dispatch(Action::Status(report));
            }
            EngineEvent::MixChanged(change) => {
                self.dispatch(Action::MixChanged(change));
            }
            EngineEvent::Meters(meters) => {
                if !self.state().is_live_run(meters.run) {
                    return;
                }
                let meters = Rc::new(*meters);
                let listeners: Vec<_> = self.with(|inner| {
                    inner.meters = Some(Rc::clone(&meters));
                    inner.changed = true;
                    inner
                        .meter_listeners
                        .iter()
                        .map(|(_, listener)| Rc::clone(listener))
                        .collect()
                });
                for listener in listeners {
                    listener(&meters);
                }
                self.wake();
            }
        }
        self.drive();
    }

    /// Stops listening to the engine; the engine itself keeps running. Events are dropped from
    /// now on, and so are the meter listeners.
    pub fn dispose(&self) {
        self.with(|inner| {
            inner.disposed = true;
            inner.listening = false;
            inner.meter_listeners.clear();
        });
    }

    /// True once disposed.
    pub fn disposed(&self) -> bool {
        self.inner().disposed
    }

    // ---- Flows ------------------------------------------------------------------------------

    /// Runs `flow` on the console's executor; it goes as far as it can at once.
    pub fn spawn<T: 'static>(&self, flow: impl Future<Output = T> + 'static) {
        self.core.exec.spawn(async move {
            flow.await;
        });
        self.drive();
    }

    /// Polls every flow that can go on.
    pub fn drive(&self) {
        self.core.exec.drive();
    }

    /// Sends `command` for this handle's owner; resolves to the desk's reply.
    pub fn run(&self, command: Command) -> impl Future<Output = Reply> + 'static {
        let ticket = self.ticket();
        let slot = Slot::new();
        self.with(|inner| inner.replies.insert(ticket, slot.clone()));
        self.queue(Effect::Run {
            ticket,
            owner: self.owner,
            command,
        });
        slot.wait()
    }

    /// Sends `command` and resolves to what its answer carries, or the reason it was refused.
    pub fn call<T: FromAnswer>(
        &self,
        command: Command,
    ) -> impl Future<Output = Result<T, String>> + 'static {
        let name = command.name();
        let reply = self.run(command);
        async move {
            let answer = reply.await?;
            T::from_answer(answer).map_err(|_| {
                format!("the desk answered `{name}` with something it does not answer")
            })
        }
    }

    /// Resolves once `after` has passed.
    pub fn sleep(&self, after: Duration) -> impl Future<Output = ()> + 'static {
        let ticket = self.ticket();
        let slot = Slot::new();
        self.with(|inner| inner.timers.insert(ticket, slot.clone()));
        self.queue(Effect::Timer { ticket, after });
        slot.wait()
    }

    fn listen(&self) -> impl Future<Output = Outcome> + 'static {
        let ticket = self.ticket();
        let slot = Slot::new();
        self.with(|inner| inner.listens.insert(ticket, slot.clone()));
        self.queue(Effect::Listen { ticket });
        slot.wait()
    }

    // ---- The store ----------------------------------------------------------------------------

    /// Applies `action` and tells every subscriber; true when the state changed. A dispatch from
    /// inside a subscriber waits until every subscriber saw the current action.
    pub fn dispatch(&self, action: Action) -> bool {
        let queued = self.with(|inner| {
            if inner.dispatching {
                inner.queued.push_back(action.clone());
                return true;
            }
            inner.dispatching = true;
            false
        });
        if queued {
            return false;
        }
        struct Done<'a>(&'a Console);
        impl Drop for Done<'_> {
            fn drop(&mut self) {
                self.0.with(|inner| {
                    inner.dispatching = false;
                    inner.queued.clear();
                });
            }
        }
        let _done = Done(self);
        let mut first = None;
        let mut next = Some(action);
        while let Some(action) = next {
            let changed = self.with(|inner| inner.state.reduce(&action));
            first.get_or_insert(changed);
            if changed {
                self.changed(&action);
            }
            next = self.with(|inner| inner.queued.pop_front());
        }
        first.unwrap_or(false)
    }

    fn changed(&self, action: &Action) {
        let (title, subscribers) = self.with(|inner| {
            inner.changed = true;
            // A loaded session keeps exactly what the next start opens.
            if let Action::SessionLoaded(outcome) = action
                && outcome.audio_loaded
            {
                inner.choice_known = true;
                inner.choice_unsaved = false;
            }
            let title = window_title(&inner.state);
            let title = (title != inner.title).then(|| {
                inner.title = title.clone();
                title
            });
            let subscribers: Vec<_> = inner
                .subscribers
                .iter()
                .map(|(_, subscriber)| Rc::clone(subscriber))
                .collect();
            (title, subscribers)
        });
        if let Some(title) = title {
            self.queue(Effect::SetTitle(title));
        }
        for subscriber in subscribers {
            subscriber(self, action);
        }
        self.wake();
    }

    /// Calls `subscriber` with every action that changes the state; returns its id for
    /// [Console::unsubscribe]. Listeners are called in the order they subscribed.
    pub fn subscribe(&self, subscriber: impl Fn(&Console, &Action) + 'static) -> u64 {
        self.with(|inner| {
            inner.next_listener += 1;
            inner
                .subscribers
                .push((inner.next_listener, Rc::new(subscriber)));
            inner.next_listener
        })
    }

    pub fn unsubscribe(&self, id: u64) {
        self.with(|inner| inner.subscribers.retain(|(listener, _)| *listener != id));
    }

    /// Calls `listener` with every meters event of the live run; returns its id.
    pub fn on_meters(&self, listener: impl Fn(&Meters) + 'static) -> u64 {
        self.with(|inner| {
            inner.next_listener += 1;
            inner
                .meter_listeners
                .push((inner.next_listener, Rc::new(listener)));
            inner.next_listener
        })
    }

    pub fn off_meters(&self, id: u64) {
        self.with(|inner| {
            inner
                .meter_listeners
                .retain(|(listener, _)| *listener != id)
        });
    }

    // ---- The desk lane and the session-load hold ----------------------------------------------

    /// Runs `job` once every desk turn asked for before it is over, and keeps the next one waiting
    /// for it: channel and group edits, session loads, mix-backup restores, buffers, redundancy and
    /// processing resets, copies and presets. `edit` counts the turn among [Console::unsaved_edits]
    /// until it is over.
    pub fn in_turn<T: 'static>(
        &self,
        edit: bool,
        job: impl Future<Output = T> + 'static,
    ) -> impl Future<Output = T> + 'static {
        let console = self.clone();
        if edit {
            self.with(|inner| inner.edits_on_their_way += 1);
        }
        let turn = self.with(|inner| {
            if inner.desk_busy {
                let slot = Slot::new();
                inner.desk_waiting.push_back(slot.clone());
                Some(slot)
            } else {
                inner.desk_busy = true;
                None
            }
        });
        async move {
            if let Some(turn) = turn {
                turn.wait().await;
            }
            let outcome = job.await;
            let next = console.with(|inner| {
                if edit {
                    inner.edits_on_their_way -= 1;
                }
                let next = inner.desk_waiting.pop_front();
                if next.is_none() {
                    inner.desk_busy = false;
                }
                next
            });
            // The lane stays busy and passes to the next turn.
            if let Some(next) = next {
                next.fill(());
            }
            outcome
        }
    }

    /// Holds every send back (a saved session loads): the lanes skip their sends while
    /// [Console::holding] is true.
    pub fn hold_sends(&self) {
        self.with(|inner| inner.holding = true);
    }

    /// Ends the hold and tells everyone who keeps edits waiting to send them
    /// ([Console::on_sends_released]): after a refused load they go out; after a load only the
    /// edits made since are left.
    pub fn release_sends(&self) {
        let released: Vec<_> = self.with(|inner| {
            inner.holding = false;
            inner
                .released
                .iter()
                .map(|(_, released)| Rc::clone(released))
                .collect()
        });
        for released in released {
            released(self);
        }
        self.drive();
    }

    /// True while a saved session loads.
    pub fn holding(&self) -> bool {
        self.inner().holding
    }

    /// Calls `released` each time a hold ends; returns its id.
    pub fn on_sends_released(&self, released: impl Fn(&Console) + 'static) -> u64 {
        self.with(|inner| {
            inner.next_listener += 1;
            inner
                .released
                .push((inner.next_listener, Rc::new(released)));
            inner.next_listener
        })
    }

    /// The edits a session load would drop now: every mix and strip edit still waiting and every
    /// desk turn counted as an edit and not over.
    pub fn unsaved_edits(&self) -> usize {
        let inner = self.inner();
        let mixes: usize = inner.state.pending.values().map(|edits| edits.len()).sum();
        mixes + inner.state.processing_pending.len() + inner.edits_on_their_way
    }

    // ---- The engine's lifecycle -----------------------------------------------------------------

    /// Connects: listens to the engine (once), reads the choices and the state, and starts an
    /// engine when none runs. Never fails: a failure is the state's connection error, and another
    /// connect tries again. A connect while one runs joins it.
    pub fn connect(&self) -> impl Future<Output = ()> + 'static {
        let done = Slot::new();
        let starts = self.with(|inner| match &mut inner.connecting {
            Some(waiting) => {
                waiting.push(done.clone());
                false
            }
            None => {
                inner.connecting = Some(vec![done.clone()]);
                true
            }
        });
        if starts {
            let console = self.clone();
            self.spawn(async move {
                console.run_connect().await;
                let waiting = console
                    .with(|inner| inner.connecting.take())
                    .unwrap_or_default();
                for done in waiting {
                    done.fill(());
                }
            });
        }
        done.wait()
    }

    async fn run_connect(&self) {
        self.dispatch(Action::Connecting);
        if !self.inner().listening {
            if let Err(error) = self.listen().await {
                self.dispatch(Action::ConnectionFailed(format!(
                    "Cannot hear the engine: {error}"
                )));
                return;
            }
            if self.disposed() {
                return;
            }
            self.with(|inner| inner.listening = true);
        }
        // Before any start, so the automatic start reopens the input the engineer chose last.
        let unread = self.read_choice().await;
        let answer = match self.call::<EngineState>(Command::EngineStatus).await {
            Ok(answer) => answer,
            Err(error) => {
                self.dispatch(Action::ConnectionFailed(error));
                return;
            }
        };
        self.dispatch(Action::Connected);
        // The presets, the strips, the strip presets, the saved sessions, the desk's word about
        // its session, the mix backups and the recording settings live in the desk's session,
        // engine or not; a failed read leaves them unknown until their screen reads them again.
        if self.state().presets.is_none() {
            self.load_presets().await;
        }
        if self.state().processing.is_none() {
            self.load_processing().await;
        }
        if self.state().strip_presets.is_none() {
            self.load_strip_presets().await;
        }
        if self.state().sessions.list.is_none() {
            self.load_sessions().await;
        }
        self.read_session_notice().await;
        if self.state().mix_backups.list.is_none() {
            drop(self.load_mix_backups().await);
        }
        if self.state().recording_settings.is_none() {
            self.load_recording_settings().await;
        }
        let auto_start = self.inner().options.auto_start;
        if matches!(answer, EngineState::NotRunning { .. }) && auto_start {
            if let Some(error) = unread {
                // Never the defaults in place of a kept input that could not be read: Try again
                // reads it again and starts.
                self.dispatch(Action::ConnectionFailed(choice_unread(&error)));
                return;
            }
            // A start that runs reads the disk estimate once its engine runs.
            drop(self.start_choice().await);
        } else {
            self.take_engine_state(answer);
            // What a take would take on the disk, or why that is not known, for the engine found.
            self.refresh_estimate().await;
        }
    }

    /// Starts the engine on the applied choices (Start, Try again, the automatic start): a missing
    /// input gives way to the default input, and choices not saved yet are saved once they run.
    /// Choices that could not be read yet are read first; while that fails nothing starts.
    pub fn start(&self) -> impl Future<Output = Outcome> + 'static {
        let console = self.clone();
        async move { console.start_choice().await }
    }

    async fn start_choice(&self) -> Outcome {
        if let Some(error) = self.read_choice().await {
            let reason = choice_unread(&error);
            // Nothing reaches the engine, so an engine that runs keeps running.
            let running = self.state().engine.status.is_some();
            if running {
                self.dispatch(Action::CommandFailed {
                    action: CommandAction::Start,
                    message: reason.strip_suffix('.').unwrap_or(&reason).to_owned(),
                });
            } else {
                self.dispatch(Action::Engine {
                    phase: Phase::Error,
                    error: Some(reason.clone()),
                });
            }
            return Err(reason);
        }
        let audio = self.state().audio.clone();
        let outcome = self.start_on(None, audio, true).await;
        let fell_back = self
            .state()
            .audio_notice
            .as_ref()
            .is_some_and(|notice| notice.kind == AudioNoticeKind::Fallback);
        if outcome.is_ok() && self.inner().choice_unsaved && !fell_back {
            self.keep_choice().await;
        }
        outcome
    }

    /// Stops the engine. A refusal is kept to be said and the status read again.
    pub fn stop(&self) -> impl Future<Output = ()> + 'static {
        let console = self.clone();
        async move { console.stop_engine().await }
    }

    async fn stop_engine(&self) {
        self.dispatch(Action::Engine {
            phase: Phase::Stopping,
            error: None,
        });
        if let Err(error) = self.call::<()>(Command::StopEngine).await {
            // Only a confirmed stop ends the run: the engine may still be running, so ask it.
            self.dispatch(Action::CommandFailed {
                action: CommandAction::Stop,
                message: error,
            });
            self.read_status().await;
            return;
        }
        self.dispatch(Action::Engine {
            phase: Phase::Idle,
            error: None,
        });
    }

    /// Applies a driver or buffer change by restarting the engine: on the device in use with
    /// `keep_device`, on `device_filter` when named, else on the input the choices name for the
    /// driver. The choices are applied and kept only once the engine started with them.
    pub fn apply_audio(
        &self,
        change: AudioChange,
        device_filter: Option<String>,
        keep_device: bool,
    ) -> impl Future<Output = Outcome> + 'static {
        let console = self.clone();
        async move {
            console
                .apply_audio_flow(change, device_filter, keep_device)
                .await
        }
    }

    /// Keeps an audio choice for the next start without restarting anything; the session keeps it
    /// once a start runs it.
    pub fn set_audio(&self, change: AudioChange) {
        self.dispatch(Action::Audio(change));
        self.with(|inner| {
            inner.choice_known = true;
            inner.choice_unsaved = true;
        });
    }

    /// Switches the input device: live under WASAPI, by a restart on it otherwise.
    pub fn switch_device(&self, name: String) -> impl Future<Output = ()> + 'static {
        let console = self.clone();
        async move { console.switch_device_flow(name).await }
    }

    /// Lists the input devices of `host` into the state.
    pub fn load_devices(&self, host: Host) -> impl Future<Output = ()> + 'static {
        let listed = self.call(Command::ListInputDevices {
            host: Some(host.name().to_owned()),
        });
        let console = self.clone();
        async move {
            let action = match listed.await {
                Ok(list) => Action::Devices {
                    host,
                    list,
                    error: None,
                },
                Err(error) => Action::Devices {
                    host,
                    list: Vec::new(),
                    error: Some(error),
                },
            };
            console.dispatch(action);
        }
    }

    /// Opens `address`'s mix.
    pub fn select(&self, address: &str) {
        self.dispatch(Action::Select(address.to_owned()));
        self.drive();
    }

    // ---- Loads ------------------------------------------------------------------------------------

    /// Reads the engine's state into the state: its status when it runs, idle when none does.
    /// Resolves to `None`, or to why it could not be read (which tells nothing about the engine).
    pub fn read_status(&self) -> impl Future<Output = Option<String>> + 'static {
        let answer = self.call::<EngineState>(Command::EngineStatus);
        let console = self.clone();
        async move {
            match answer.await {
                Ok(answer) => {
                    console.take_engine_state(answer);
                    None
                }
                Err(error) => Some(error),
            }
        }
    }

    /// Reads the desk's mix presets; `None`, or why they could not be read.
    pub fn load_presets(&self) -> impl Future<Output = Option<String>> + 'static {
        let read = self.call(Command::Presets);
        let console = self.clone();
        async move {
            match read.await {
                Ok(presets) => {
                    console.dispatch(Action::Presets(presets));
                    None
                }
                Err(error) => Some(error),
            }
        }
    }

    /// Reads every channel's strip; `None`, or why they could not be read.
    pub fn load_processing(&self) -> impl Future<Output = Option<String>> + 'static {
        let read = self.call(Command::Processing);
        let console = self.clone();
        async move {
            match read.await {
                Ok(strips) => {
                    console.dispatch(Action::Processing(Some(strips)));
                    None
                }
                Err(error) => Some(error),
            }
        }
    }

    /// Reads the strip presets; `None`, or why they could not be read.
    pub fn load_strip_presets(&self) -> impl Future<Output = Option<String>> + 'static {
        let read = self.call(Command::StripPresets);
        let console = self.clone();
        async move {
            match read.await {
                Ok(presets) => {
                    console.dispatch(Action::StripPresets(Some(presets)));
                    None
                }
                Err(error) => Some(error),
            }
        }
    }

    /// Lists the saved sessions; `None`, or why they could not be listed (kept to be said).
    pub fn load_sessions(&self) -> impl Future<Output = Option<String>> + 'static {
        let read = self.call(Command::Sessions);
        let console = self.clone();
        async move {
            match read.await {
                Ok(sessions) => {
                    console.dispatch(Action::Sessions(sessions));
                    None
                }
                Err(error) => {
                    console.dispatch(Action::SessionsFailed(error.clone()));
                    Some(error)
                }
            }
        }
    }

    /// Reads what the desk said when it opened its session; a failed read says nothing.
    pub fn read_session_notice(&self) -> impl Future<Output = ()> + 'static {
        let read = self.call(Command::SessionNotice);
        let console = self.clone();
        async move {
            // The notice is a courtesy: a failed read says nothing, and the next connect asks again.
            if let Ok(notice) = read.await {
                console.dispatch(Action::SessionNotice(notice));
            }
        }
    }

    /// Lists the mix backups: `Ok` with how many in words, or `Err` with why not (kept to be said).
    pub fn load_mix_backups(&self) -> impl Future<Output = Result<String, String>> + 'static {
        let read = self
            .call::<Vec<bandear_desktop_core::backups::MixBackupSummary>>(Command::ListMixBackups);
        let console = self.clone();
        async move {
            match read.await {
                Ok(list) => {
                    let count = list.len();
                    console.dispatch(Action::MixBackups(list));
                    Ok(if count == 1 {
                        "1 mix backup.".to_owned()
                    } else {
                        format!("{count} mix backups.")
                    })
                }
                Err(error) => {
                    console.dispatch(Action::MixBackupsFailed(error.clone()));
                    Err(sentence_with_reason(
                        "The mix backups could not be listed",
                        Some(&error),
                    ))
                }
            }
        }
    }

    /// Reads the recording settings; `None`, or why they could not be read.
    pub fn load_recording_settings(&self) -> impl Future<Output = Option<String>> + 'static {
        let read = self.call(Command::RecordingSettings);
        let console = self.clone();
        async move {
            match read.await {
                Ok(settings) => {
                    console.dispatch(Action::RecordingSettings(Some(settings)));
                    None
                }
                Err(error) => Some(error),
            }
        }
    }

    /// Reads what a take would take on the disk (forgotten when it cannot be read); `None`, or why.
    pub fn refresh_estimate(&self) -> impl Future<Output = Option<String>> + 'static {
        let read = self.call(Command::RecordEstimate);
        let console = self.clone();
        async move {
            match read.await {
                Ok(estimate) => {
                    console.dispatch(Action::Estimate(Some(estimate)));
                    None
                }
                Err(error) => {
                    console.dispatch(Action::Estimate(None));
                    Some(error)
                }
            }
        }
    }
}

/// Why nothing started while the kept choices could not be read, and the way past it: an input the
/// engineer chooses in Network and system starts without them (and is kept from then on).
fn choice_unread(error: &str) -> String {
    let why = sentence_with_reason(
        "The saved audio input could not be read, so nothing was started",
        Some(error),
    );
    format!("{why} Choose an input in Network and system to start without it.")
}

// The steps of the lifecycle flows.
impl Console {
    /// Puts the engine's state in the state: its status when it runs, idle when none does.
    fn take_engine_state(&self, answer: EngineState) {
        match answer {
            EngineState::NotRunning { .. } => {
                self.dispatch(Action::Engine {
                    phase: Phase::Idle,
                    error: None,
                });
            }
            EngineState::Running(report) => {
                self.dispatch(Action::Status(report));
            }
        }
    }

    /// Reads the audio choices the desk's session keeps, until a read succeeds: every start builds
    /// on them. `None` once they are known, or why the read failed, which the next connect or start
    /// tries again. While they are unknown nothing starts on the defaults, so a passing failure
    /// never opens the default input in place of the kept one.
    async fn read_choice(&self) -> Option<String> {
        if self.inner().choice_known {
            return None;
        }
        match self
            .call::<Option<bandear_desktop_core::session::SessionAudio>>(Command::AudioSettings)
            .await
        {
            Ok(saved) => {
                // A choice the engineer made meanwhile is not replaced.
                if !self.inner().choice_known {
                    self.dispatch(Action::Audio(AudioChange::to(&restored_audio(
                        saved.as_ref(),
                    ))));
                    self.with(|inner| inner.choice_known = true);
                }
                None
            }
            Err(error) => Some(error),
        }
    }

    /// Keeps the applied choices, which an engine now runs with, in the session for every later
    /// start. A failure is said, and the next start that runs them tries again.
    async fn keep_choice(&self) {
        self.with(|inner| inner.choice_known = true);
        let request = settings_request(&self.state().audio);
        match self
            .call::<bandear_desktop_core::session::SessionAudio>(Command::SetAudioSettings {
                settings: request,
            })
            .await
        {
            Ok(_) => {
                self.with(|inner| inner.choice_unsaved = false);
                let unsaved = self
                    .state()
                    .audio_notice
                    .as_ref()
                    .is_some_and(|notice| notice.kind == AudioNoticeKind::Unsaved);
                if unsaved {
                    self.dispatch(Action::AudioNotice(None));
                }
            }
            Err(error) => {
                self.with(|inner| inner.choice_unsaved = true);
                self.dispatch(Action::AudioNotice(Some(AudioNotice {
                    kind: AudioNoticeKind::Unsaved,
                    message: sentence_with_reason(
                        "The audio input runs but was not saved for the next start",
                        Some(&error),
                    ),
                })));
            }
        }
    }

    /// The start on the default input (never ASIO) when the input `request` asked for is no longer
    /// listed for its driver, matched as the engine matches it; `None` when it is listed (the start
    /// failed for another reason), when none was asked for, or when the devices cannot be listed.
    async fn default_input_request(
        &self,
        audio: &Audio,
        request: &bandear_desktop_core::dto::StartRequest,
    ) -> Option<bandear_desktop_core::dto::StartRequest> {
        let sought = request.device_filter.as_ref()?.to_lowercase();
        let listed = self
            .call::<Vec<bandear_desktop_core::dto::InputDeviceDto>>(Command::ListInputDevices {
                host: request.host.clone(),
            })
            .await
            .ok()?;
        let present = listed
            .iter()
            .any(|device| device.name.to_lowercase().contains(&sought));
        if present {
            None
        } else {
            start_request(&fallback_audio(audio), None)
        }
    }

    /// Starts (or restarts) the engine with `audio`, on `device_filter` when named, else on the
    /// input the choices name. Every start is built by [start_request], which never builds an ASIO
    /// start without a picked driver. With `or_default`, a start whose input is gone starts on the
    /// default input instead and says so; the choices stay as they are.
    async fn start_on(
        &self,
        device_filter: Option<String>,
        audio: Audio,
        or_default: bool,
    ) -> Outcome {
        let Some(request) = start_request(&audio, device_filter.as_deref()) else {
            // Nothing reaches the engine, so an engine that runs keeps running.
            let running = self.state().engine.status.is_some();
            if running {
                self.dispatch(Action::CommandFailed {
                    action: CommandAction::Start,
                    message: ASIO_NEEDS_DRIVER.to_owned(),
                });
            } else {
                self.dispatch(Action::Engine {
                    phase: Phase::Error,
                    error: Some(ASIO_NEEDS_DRIVER.to_owned()),
                });
            }
            return Err(ASIO_NEEDS_DRIVER.to_owned());
        };
        self.dispatch(Action::Engine {
            phase: Phase::Starting,
            error: None,
        });
        let mut fell_back = false;
        if let Err(error) = self
            .call::<bandear_desktop_core::dto::StartSummary>(Command::StartEngine(request.clone()))
            .await
        {
            // A failed start has stopped the engine it replaced and started none.
            let fallback = if or_default {
                self.default_input_request(&audio, &request).await
            } else {
                None
            };
            let Some(fallback) = fallback else {
                self.dispatch(Action::Engine {
                    phase: Phase::Error,
                    error: Some(error.clone()),
                });
                return Err(error);
            };
            if let Err(error) = self
                .call::<bandear_desktop_core::dto::StartSummary>(Command::StartEngine(fallback))
                .await
            {
                let missing = remembered_device(&audio).unwrap_or_default();
                let reason = sentence_with_reason(
                    &format!(
                        "{missing} was not found, and the system's default input did not start either"
                    ),
                    Some(&error),
                );
                self.dispatch(Action::Engine {
                    phase: Phase::Error,
                    error: Some(reason.clone()),
                });
                return Err(reason);
            }
            fell_back = true;
        }
        if fell_back {
            self.dispatch(Action::AudioNotice(Some(AudioNotice {
                kind: AudioNoticeKind::Fallback,
                message: fallback_notice(&audio),
            })));
        }
        // The engine runs now: failing to read its status must not end its run.
        if let Some(error) = self.read_status().await {
            self.fill_after_start(error);
        }
        // The run before it took its estimate with it; this engine's device decides the tracks.
        self.refresh_estimate().await;
        Ok(())
    }

    /// After a start, the status the pump sends fills the view; when none has come yet, asks again
    /// a few times. When it never comes, says so, with Try again, rather than stay on starting.
    fn fill_after_start(&self, error: String) {
        let console = self.clone();
        let still_starting = |console: &Console| {
            !console.disposed() && console.state().engine.phase == Phase::Starting
        };
        self.spawn(async move {
            let mut error = error;
            for attempt in 1.. {
                if !still_starting(&console) {
                    return;
                }
                if attempt > STATUS_RETRIES {
                    console.dispatch(Action::ConnectionFailed(format!(
                        "The engine started, but its status could not be read: {error}"
                    )));
                    return;
                }
                let wait = console.inner().options.status_retry;
                console.sleep(wait).await;
                if !still_starting(&console) {
                    return;
                }
                match console.read_status().await {
                    None => return,
                    Some(again) => error = again,
                }
            }
        });
    }

    async fn apply_audio_flow(
        &self,
        change: AudioChange,
        device_filter: Option<String>,
        keep_device: bool,
    ) -> Outcome {
        if let Some(blocked) = self.switch_blocked() {
            return Err(blocked);
        }
        let fallback = self
            .state()
            .audio_notice
            .as_ref()
            .is_some_and(|notice| notice.kind == AudioNoticeKind::Fallback);
        if keep_device && fallback {
            // The engine runs on the default input because the saved one is missing: the change is
            // kept for the next start, which tries the saved input again.
            self.dispatch(Action::Audio(change));
            self.with(|inner| inner.choice_unsaved = true);
            return Ok(());
        }
        let (device, mut next) = {
            let state = self.state();
            let device = if keep_device {
                state
                    .engine
                    .status
                    .as_ref()
                    .map(|status| status.device_name.clone())
            } else {
                device_filter.clone()
            };
            (device, audio_choice(&state.audio, &change))
        };
        // Under ASIO the device named is the driver, which stays the applied one; under another
        // driver a device named becomes the applied input.
        if next.host == Host::Asio && device.is_some() {
            next = audio_choice(
                &next,
                &AudioChange {
                    asio_driver: Some(device.clone()),
                    ..AudioChange::default()
                },
            );
        } else if let Some(named) = &device_filter {
            next = audio_choice(
                &next,
                &AudioChange {
                    device: Some(Some(named.clone())),
                    ..AudioChange::default()
                },
            );
        }
        let outcome = self.start_on(device, next.clone(), false).await;
        if outcome.is_ok() {
            self.dispatch(Action::Audio(AudioChange::to(&next)));
            self.keep_choice().await;
        }
        outcome
    }

    /// Why the input device or its driver cannot change now (a take records, or a soundcheck plays
    /// in place of the inputs), said as a refused switch; `None` when it can.
    fn switch_blocked(&self) -> Option<String> {
        let blocked = {
            let state = self.state();
            if state.is_recording_now() {
                Some(RECORDING_BLOCKS_SWITCH)
            } else if state.is_soundcheck_now() {
                Some(SOUNDCHECK_BLOCKS_SWITCH)
            } else {
                None
            }
        }?;
        self.dispatch(Action::CommandFailed {
            action: CommandAction::Switch,
            message: blocked.to_owned(),
        });
        Some(blocked.to_owned())
    }

    async fn switch_device_flow(&self, name: String) {
        if self.switch_blocked().is_some() {
            return;
        }
        if self.state().audio.host != Host::Wasapi {
            drop(
                self.apply_audio_flow(AudioChange::default(), Some(name), false)
                    .await,
            );
            return;
        }
        if let Err(error) = self
            .call::<bandear_desktop_core::dto::StartSummary>(Command::SwitchDevice {
                device: name.clone(),
            })
            .await
        {
            // The engine keeps the device it had, and the choices stay as they were.
            self.dispatch(Action::CommandFailed {
                action: CommandAction::Switch,
                message: error,
            });
            self.read_status().await;
            return;
        }
        self.dispatch(Action::CommandDone(CommandAction::Switch));
        self.dispatch(Action::Audio(AudioChange {
            device: Some(Some(name)),
            ..AudioChange::default()
        }));
        let fallback = self
            .state()
            .audio_notice
            .as_ref()
            .is_some_and(|notice| notice.kind == AudioNoticeKind::Fallback);
        if fallback {
            // The engine runs on an input the engineer chose now.
            self.dispatch(Action::AudioNotice(None));
        }
        self.keep_choice().await;
        self.read_status().await;
    }
}

/// Why the input cannot change while a take records (`recording.js`).
pub const RECORDING_BLOCKS_SWITCH: &str = "Stop the recording before switching the input device or its driver: a take never mixes two devices.";

/// Why the input cannot change while a soundcheck plays (`soundcheck.js`).
pub const SOUNDCHECK_BLOCKS_SWITCH: &str = "Go back to the live inputs before switching the input device or its driver: a soundcheck plays a take in place of the inputs.";

#[cfg(test)]
mod tests;
