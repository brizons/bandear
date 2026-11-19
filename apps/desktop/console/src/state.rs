//! The console's state and its reducer (`console-state.js`), fed by the engine's `status` and
//! `mix-changed` events, by the desk's answers and by the engineer's edits. Meters are not kept
//! here: at 30 updates a second they go straight to the views (see [crate::console::Console::meters]).
//!
//! The mix a screen shows is the engine's mix with the pending edits on top ([ConsoleState::view_mix]),
//! so a fader never jumps back to an older value while its change is on the way. Each copy of a
//! musician's mix carries the engine's revision, and a status row, a `mix-changed` event and a
//! `set_mix` answer can arrive in any order: the state keeps the copy with the highest revision from
//! any of them and never goes back to a lower one. An answer also settles the edits it carried. A
//! status that moves the layout (the groups' order, an input's pairing or hiding) drops the edits
//! whose position no longer means the same thing.
//!
//! Every engine payload names its run. Starting, a failed start and a confirmed stop end the
//! current run, and a payload of an ended or older run is ignored, so a late event can never bring
//! back an engine that has stopped. A stop the engine refused ends nothing: the engine may still
//! run, so its run stays live and the refusal is shown ([Action::CommandFailed]).
//!
//! Loading a saved session replaces the mixes, the groups and the channel details at once, so the
//! edits still waiting are dropped once it loaded ([Action::SessionLoaded]): none of them was meant
//! for the loaded session. A refused load keeps them.
//!
//! [ConsoleState::reduce] answers whether the state changed, as the web store answered with the
//! same object when nothing did: whoever listens is told only of a change.

use std::collections::{BTreeMap, HashMap};

use bandear_desktop_core::backups::MixBackupSummary;
use bandear_desktop_core::commands::processing::{
    ChannelProcessingDto, EqPointDto, StripPresetDto,
};
use bandear_desktop_core::dto::{
    DiskEstimateDto, EngineerControlsSetDto, InputDeviceDto, OutputDeviceDto, RecordingSettingsDto,
    SessionLoadedDto, TakeListItemDto, TakesDto,
};
use bandear_desktop_core::session::{SessionChannel, SessionPreset};
use bandear_desktop_core::sessions::SnapshotSummary;
use personal_monitoring::{
    DelaySettings, Effect, EffectLevels, EffectSettings, LimiterSettings, ReverbSettings,
    StripSettings,
};

use crate::engine_config::{Audio, AudioChange, Host, audio_choice, default_audio, restored_audio};
use crate::format::{musician_label, needs_attention};
use crate::mix::{EditKey, Edits, Mix, MixField, settle_edits};
use crate::wire::{
    Channel, MixChanged, Musician, Recording, Soundcheck, Status, StatusReport, TakeSummary,
};

/// The id of the engineer's local mix: its tab, its pending edits and its `mix-changed` event. A
/// musician's address always has a port, so it is never this.
pub const LOCAL_MIX: &str = "local";

/// The engine's phase as the console knows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Idle,
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
}

impl Phase {
    /// Every phase.
    pub const ALL: [Phase; 6] = [
        Phase::Idle,
        Phase::Starting,
        Phase::Running,
        Phase::Stopping,
        Phase::Stopped,
        Phase::Error,
    ];

    /// The phase's name (`idle`, `starting`, ...).
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Stopped => "stopped",
            Self::Error => "error",
        }
    }

    /// The phase `name` names; any other is refused.
    pub fn from_name(name: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|phase| phase.name() == name)
            .ok_or_else(|| format!("unknown engine phase: {name}"))
    }

    /// True for a phase in which the engine of the current run is gone or being replaced.
    pub fn ends_run(self) -> bool {
        matches!(self, Self::Idle | Self::Starting | Self::Error)
    }
}

/// The engine as the console shows it.
#[derive(Clone, Debug)]
pub struct Engine {
    pub phase: Phase,
    /// Why the engine is not running, in the phase `error`.
    pub error: Option<String>,
    /// The last status, without its musicians; `None` while no engine runs.
    pub status: Option<Status>,
    /// The run that status came from; `None` while none runs.
    pub run: Option<u64>,
    /// The newest run known to have ended.
    pub ended_run: u64,
}

/// Whether the console hears the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionStatus {
    Connecting,
    /// The engine answered, or a status came.
    Ready,
    /// The console cannot hear the engine, for the connection's `error`.
    Failed,
}

/// The console's connection to the engine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    pub status: ConnectionStatus,
    pub error: Option<String>,
}

/// The open musician a status no longer listed, so the first one opened instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Departed {
    pub address: String,
    pub label: String,
}

/// The last refused change of a mix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixError {
    pub address: String,
    pub message: String,
}

/// The last `set_engineer_controls` answer for a musician, when it did not both apply and save.
#[derive(Clone, Debug)]
pub struct ControlsNotice {
    pub address: String,
    pub outcome: EngineerControlsSetDto,
}

/// What a word about the audio input says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioNoticeKind {
    /// The input the choices name was missing and the engine started on the default input.
    Fallback,
    /// The choices run but the session could not keep them.
    Unsaved,
}

/// A word about the audio input of the current run; a run's end clears it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioNotice {
    pub kind: AudioNoticeKind,
    pub message: String,
}

/// A command the engine refused while its run went on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandAction {
    Start,
    Stop,
    Switch,
}

impl CommandAction {
    /// The command's name (`start`, `stop`, `switch`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Switch => "switch",
        }
    }
}

/// The refusal of a stop, a start or a device switch; cleared by the next start or stop, or when
/// that command works.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandError {
    pub action: CommandAction,
    pub message: String,
}

/// The input devices listed for a driver.
#[derive(Clone, Debug)]
pub struct Devices {
    pub host: Option<Host>,
    pub list: Vec<InputDeviceDto>,
    pub error: Option<String>,
}

/// A list the desk keeps, `None` until listed, and why the last listing failed (the last list
/// known stays).
#[derive(Clone, Debug, PartialEq)]
pub struct Listing<T> {
    pub list: Option<Vec<T>>,
    pub error: Option<String>,
}

impl<T> Default for Listing<T> {
    fn default() -> Self {
        Self {
            list: None,
            error: None,
        }
    }
}

/// The takes in the recordings folder, the newest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Takes {
    pub list: Option<Vec<TakeListItemDto>>,
    pub folder: Option<String>,
    pub unreadable: usize,
    pub missing: bool,
    pub error: Option<String>,
}

/// A recording or soundcheck command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RecordingAction {
    Settings,
    Start,
    Stop,
    Marker,
    Open,
    Soundcheck,
}

/// The last recording or soundcheck command's outcome in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordingOutcome {
    pub action: RecordingAction,
    pub ok: bool,
    pub message: String,
}

/// A whole value on its way to the desk, stamped so its answer settles exactly it: a later edit
/// to the same thing has a newer stamp and stays.
#[derive(Clone, Debug, PartialEq)]
pub struct Stamped<T> {
    pub stamp: u64,
    pub value: T,
}

/// The last refused strip edit; `channel` is `None` when the strips could not be read after a load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessingError {
    pub channel: Option<usize>,
    pub message: String,
}

/// A level per effect, for the sends and returns on their way to the desk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Levels {
    pub reverb: Option<u8>,
    pub delay: Option<u8>,
}

impl Levels {
    /// The level of `effect`, if one waits.
    pub fn get(&self, effect: Effect) -> Option<u8> {
        match effect {
            Effect::Reverb => self.reverb,
            Effect::Delay => self.delay,
        }
    }

    fn slot(&mut self, effect: Effect) -> &mut Option<u8> {
        match effect {
            Effect::Reverb => &mut self.reverb,
            Effect::Delay => &mut self.delay,
        }
    }

    fn is_empty(&self) -> bool {
        self.reverb.is_none() && self.delay.is_none()
    }
}

/// An effect's settings, as an edit changes them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectChange {
    Reverb(ReverbSettings),
    Delay(DelaySettings),
}

impl EffectChange {
    /// The effect the settings are of.
    pub fn effect(self) -> Effect {
        match self {
            Self::Reverb(_) => Effect::Reverb,
            Self::Delay(_) => Effect::Delay,
        }
    }
}

/// Effect settings on their way to the desk.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EffectsPending {
    pub reverb: Option<Stamped<ReverbSettings>>,
    pub delay: Option<Stamped<DelaySettings>>,
}

impl EffectsPending {
    /// The stamp of the change of `effect` still waiting, if any.
    pub fn stamp(&self, effect: Effect) -> Option<u64> {
        match effect {
            Effect::Reverb => self.reverb.as_ref().map(|waiting| waiting.stamp),
            Effect::Delay => self.delay.as_ref().map(|waiting| waiting.stamp),
        }
    }
}

/// The strip a screen shows for a channel: the edit still waiting, else the desk's, else a
/// neutral one while the strips are not read yet.
#[derive(Clone, Debug, PartialEq)]
pub struct ShownStrip {
    pub channel: usize,
    pub settings: StripSettings,
    pub active: bool,
    pub pending: bool,
}

/// Everything the console knows. See the module's documentation, and each field's.
#[derive(Clone, Debug)]
pub struct ConsoleState {
    pub engine: Engine,
    /// In the status's order, each with its mix nested.
    pub musicians: Vec<Musician>,
    /// The address whose mix is open ([LOCAL_MIX] for the local mix). It survives a stop, so a
    /// restart reopens it.
    pub selected: Option<String>,
    pub departed: Option<Departed>,
    /// Local edits still waiting for the engine, by address (or [LOCAL_MIX]).
    pub pending: BTreeMap<String, Edits>,
    pub mix_error: Option<MixError>,
    pub controls_notice: Option<ControlsNotice>,
    /// The driver, input, buffer and picked ASIO driver the next start asks for.
    pub audio: Audio,
    pub audio_notice: Option<AudioNotice>,
    pub connection: Connection,
    pub command_error: Option<CommandError>,
    pub devices: Devices,
    /// The engineer's mix presets, `None` until read. They live in the desk's session, so a run's
    /// end keeps them.
    pub presets: Option<Vec<SessionPreset>>,
    pub sessions: Listing<SnapshotSummary>,
    /// What the desk said when it opened its session, or `None`.
    pub session_notice: Option<String>,
    pub mix_backups: Listing<MixBackupSummary>,
    /// The engineer's local mix, `None` while no engine runs.
    pub local_mix: Option<Mix>,
    /// The outputs the running engine offers for the local mix; a run's end forgets them.
    pub outputs: Listing<OutputDeviceDto>,
    pub recording_settings: Option<RecordingSettingsDto>,
    pub takes: Takes,
    /// What a take would take on the disk; a run's end forgets it.
    pub estimate: Option<DiskEstimateDto>,
    pub recording_outcome: Option<RecordingOutcome>,
    /// Every channel's strip, CH 1 first, `None` until read.
    pub processing: Option<Vec<ChannelProcessingDto>>,
    /// Strip edits still waiting for the desk, by channel (1-based).
    pub processing_pending: BTreeMap<usize, Stamped<StripSettings>>,
    pub processing_error: Option<ProcessingError>,
    pub strip_presets: Option<Vec<StripPresetDto>>,
    /// The equaliser response last worked out for a channel.
    pub eq_curves: BTreeMap<usize, Vec<EqPointDto>>,
    /// Master limiter changes on their way, by mix address (or [LOCAL_MIX]).
    pub limiter_pending: BTreeMap<String, Stamped<LimiterSettings>>,
    /// Sends on their way, by channel (1-based).
    pub sends_pending: BTreeMap<usize, Levels>,
    pub effects_pending: EffectsPending,
    /// Returns on their way, by mix address (or [LOCAL_MIX]).
    pub returns_pending: BTreeMap<String, Levels>,
    /// The last stamp given to a whole-value edit.
    pub stamps: u64,
}

impl Default for ConsoleState {
    fn default() -> Self {
        Self::new()
    }
}

/// Everything that changes the state.
#[derive(Clone, Debug)]
pub enum Action {
    /// A status, from `engine_status` or the `status` event.
    Status(Box<StatusReport>),
    /// The engine's phase as the console learns it from its own start and stop calls; `starting`,
    /// `idle` and `error` end the current run.
    Engine {
        phase: Phase,
        error: Option<String>,
    },
    Select(String),
    /// A `mix-changed` event.
    MixChanged(Box<MixChanged>),
    /// Local edits to one mix.
    MixEdit {
        address: String,
        edits: Edits,
    },
    /// The engine answered a `set_mix` that carried `sent` with the mix it stored.
    MixApplied {
        address: String,
        sent: Edits,
        mix: Option<Mix>,
    },
    /// A `set_mix` that carried `sent` was refused.
    MixFailed {
        address: String,
        sent: Edits,
        error: String,
    },
    /// The engine answered a `set_engineer_controls` that carried `sent`.
    EngineerApplied {
        address: String,
        sent: Edits,
        outcome: Box<EngineerControlsSetDto>,
    },
    /// A change of the audio choices for the next start.
    Audio(AudioChange),
    AudioNotice(Option<AudioNotice>),
    /// The input devices listed for `host`, or why they could not be.
    Devices {
        host: Host,
        list: Vec<InputDeviceDto>,
        error: Option<String>,
    },
    /// The outputs the engine of `run` offers, taken only while that run is shown.
    Outputs {
        list: Vec<OutputDeviceDto>,
        run: Option<u64>,
    },
    OutputsFailed {
        message: String,
        run: Option<u64>,
    },
    Presets(Vec<SessionPreset>),
    Sessions(Vec<SnapshotSummary>),
    SessionsFailed(String),
    MixBackups(Vec<MixBackupSummary>),
    MixBackupsFailed(String),
    SessionNotice(Option<String>),
    /// A saved session was loaded (the `load_session` answer).
    SessionLoaded(Box<SessionLoadedDto>),
    /// The engine refused `action` but its run goes on.
    CommandFailed {
        action: CommandAction,
        message: String,
    },
    /// `action` worked: an earlier refusal of it no longer holds.
    CommandDone(CommandAction),
    RecordingSettings(Option<RecordingSettingsDto>),
    Takes(Box<TakesDto>),
    TakesFailed(String),
    Estimate(Option<DiskEstimateDto>),
    RecordingOutcome(Option<RecordingOutcome>),
    /// A soundcheck command's answer for engine run `run`: shown at once on that run's status.
    Soundcheck {
        soundcheck: Box<Soundcheck>,
        run: Option<u64>,
    },
    /// Every channel's strip as `processing` answered it.
    Processing(Option<Vec<ChannelProcessingDto>>),
    /// A strip edit of `channel` (1-based) on its way: the whole strip.
    ProcessingEdit {
        channel: usize,
        settings: StripSettings,
    },
    /// The desk answered a processing change (an edit of `channel` stamped `sent`, or a reset,
    /// copy or preset with neither) with the strips it changed.
    ProcessingApplied {
        channel: Option<usize>,
        sent: Option<u64>,
        channels: Vec<ChannelProcessingDto>,
    },
    ProcessingFailed {
        channel: usize,
        sent: Option<u64>,
        message: String,
    },
    /// The strips are no longer known (a loaded session's could not be read), for `message`.
    ProcessingUnknown(String),
    StripPresets(Option<Vec<StripPresetDto>>),
    EqCurve {
        channel: usize,
        points: Vec<EqPointDto>,
    },
    /// A master limiter change sent for the mix `address`, shown until answered.
    LimiterEdit {
        address: String,
        settings: LimiterSettings,
    },
    /// The answer (or failure) of the limiter change stamped `sent`; `kept`, when given, is what
    /// the desk set, kept as the mix's limiter until the next status.
    LimiterSettled {
        address: String,
        sent: Option<u64>,
        kept: Option<LimiterSettings>,
    },
    SendEdit {
        channel: usize,
        effect: Effect,
        level: u8,
    },
    SendSettled {
        channel: usize,
        effect: Effect,
        level: u8,
    },
    EffectEdit(EffectChange),
    /// The answer (or failure) of the effect change stamped `sent`; of `effects`, when given, only
    /// `effect`'s are kept until the next status.
    EffectSettled {
        effect: Effect,
        sent: Option<u64>,
        effects: Option<EffectSettings>,
    },
    ReturnEdit {
        address: String,
        effect: Effect,
        level: u8,
    },
    /// The answer (or failure) of a return; of `returns`, when given, only `effect`'s is kept
    /// until the next status.
    ReturnSettled {
        address: String,
        effect: Effect,
        level: u8,
        returns: Option<EffectLevels>,
    },
    Connecting,
    Connected,
    ConnectionFailed(String),
}

/// An input of a device's channel list, as [pair_units] reads it.
pub trait PairInput {
    fn link(&self) -> bool;
    fn hidden(&self) -> bool;
}

impl PairInput for Channel {
    fn link(&self) -> bool {
        self.link
    }

    fn hidden(&self) -> bool {
        self.hidden
    }
}

impl PairInput for SessionChannel {
    fn link(&self) -> bool {
        self.link
    }

    fn hidden(&self) -> bool {
        self.hidden
    }
}

/// A device's inputs as the console treats them, left to right: `[first, second]` (0-based) for a
/// stereo pair (a link to a next input that is shown or hidden alike), else `[index]`.
pub fn pair_units<C: PairInput>(channels: &[C]) -> Vec<Vec<usize>> {
    let mut units = Vec::new();
    let mut index = 0;
    while index < channels.len() {
        let channel = &channels[index];
        match channels.get(index + 1) {
            Some(next) if channel.link() && next.hidden() == channel.hidden() => {
                units.push(vec![index, index + 1]);
                index += 2;
            }
            _ => {
                units.push(vec![index]);
                index += 1;
            }
        }
    }
    units
}

impl ConsoleState {
    /// The console before it heard anything: idle, connecting, the default audio choices.
    pub fn new() -> Self {
        Self {
            engine: Engine {
                phase: Phase::Idle,
                error: None,
                status: None,
                run: None,
                ended_run: 0,
            },
            musicians: Vec::new(),
            selected: None,
            departed: None,
            pending: BTreeMap::new(),
            mix_error: None,
            controls_notice: None,
            audio: default_audio(),
            audio_notice: None,
            connection: Connection {
                status: ConnectionStatus::Connecting,
                error: None,
            },
            command_error: None,
            devices: Devices {
                host: None,
                list: Vec::new(),
                error: None,
            },
            presets: None,
            sessions: Listing::default(),
            session_notice: None,
            mix_backups: Listing::default(),
            local_mix: None,
            outputs: Listing::default(),
            recording_settings: None,
            takes: Takes::default(),
            estimate: None,
            recording_outcome: None,
            processing: None,
            processing_pending: BTreeMap::new(),
            processing_error: None,
            strip_presets: None,
            eq_curves: BTreeMap::new(),
            limiter_pending: BTreeMap::new(),
            sends_pending: BTreeMap::new(),
            effects_pending: EffectsPending::default(),
            returns_pending: BTreeMap::new(),
            stamps: 0,
        }
    }

    /// Applies `action`; true when the state changed.
    pub fn reduce(&mut self, action: &Action) -> bool {
        match action {
            Action::Status(report) => self.receive_status(report),
            Action::Engine { phase, error } => self.set_phase(*phase, error.clone()),
            Action::Select(address) => {
                if !self.is_listed(address) {
                    return false;
                }
                if self.selected.as_deref() == Some(address) && self.departed.is_none() {
                    return false;
                }
                self.selected = Some(address.clone());
                self.departed = None;
                true
            }
            Action::MixChanged(change) => {
                let address = if change.id.as_deref() == Some(LOCAL_MIX) {
                    Some(LOCAL_MIX)
                } else {
                    change.address.as_deref()
                };
                match address {
                    Some(address) if self.is_live_run(change.run) => {
                        self.offer_mix(address, &change.mix)
                    }
                    _ => false,
                }
            }
            Action::MixEdit { address, edits } => self.edit(address, edits),
            Action::MixApplied { address, sent, mix } => self.answered(address, sent, mix.as_ref()),
            Action::MixFailed {
                address,
                sent,
                error,
            } => {
                if !self.is_listed(address) {
                    return false;
                }
                let left = settle_edits(self.pending.get(address), sent);
                self.set_pending(address, left);
                self.mix_error = Some(MixError {
                    address: address.clone(),
                    message: error.clone(),
                });
                true
            }
            Action::EngineerApplied {
                address,
                sent,
                outcome,
            } => {
                if !self.is_listed(address) {
                    return false;
                }
                let mix = outcome.mix.as_ref().map(Mix::from_core);
                self.answered(address, sent, mix.as_ref());
                if !(outcome.applied && outcome.saved) {
                    self.controls_notice = Some(ControlsNotice {
                        address: address.clone(),
                        outcome: (**outcome).clone(),
                    });
                } else if self
                    .controls_notice
                    .as_ref()
                    .is_some_and(|notice| &notice.address == address)
                {
                    self.controls_notice = None;
                }
                true
            }
            Action::Audio(change) => {
                self.audio = audio_choice(&self.audio, change);
                true
            }
            Action::AudioNotice(notice) => {
                self.audio_notice = notice.clone();
                true
            }
            Action::CommandFailed { action, message } => {
                // The engine refused a command but its run goes on: back to the phase the last
                // status showed.
                self.engine.phase = match &self.engine.status {
                    Some(status) if status.stopped => Phase::Stopped,
                    Some(_) => Phase::Running,
                    None => Phase::Idle,
                };
                self.command_error = Some(CommandError {
                    action: *action,
                    message: message.clone(),
                });
                true
            }
            Action::CommandDone(action) => {
                if self
                    .command_error
                    .as_ref()
                    .is_some_and(|error| error.action == *action)
                {
                    self.command_error = None;
                    true
                } else {
                    false
                }
            }
            Action::Connecting => self.connect(ConnectionStatus::Connecting, None),
            Action::Connected => self.connect(ConnectionStatus::Ready, None),
            Action::ConnectionFailed(message) => {
                self.connect(ConnectionStatus::Failed, Some(message.clone()))
            }
            Action::Devices { host, list, error } => {
                self.devices = Devices {
                    host: Some(*host),
                    list: list.clone(),
                    error: error.clone(),
                };
                true
            }
            Action::Outputs { list, run } => {
                if !self.is_shown_run(*run) {
                    return false;
                }
                self.outputs = Listing {
                    list: Some(list.clone()),
                    error: None,
                };
                true
            }
            Action::OutputsFailed { message, run } => {
                if !self.is_shown_run(*run) {
                    return false;
                }
                self.outputs.error = Some(message.clone());
                true
            }
            Action::Presets(presets) => {
                self.presets = Some(presets.clone());
                true
            }
            Action::Processing(list) => {
                self.processing = list.clone();
                self.processing_error = None;
                true
            }
            Action::ProcessingUnknown(message) => {
                self.processing = None;
                self.processing_error = Some(ProcessingError {
                    channel: None,
                    message: message.clone(),
                });
                true
            }
            Action::ProcessingEdit { channel, settings } => {
                let stamp = self.next_stamp();
                self.processing_pending.insert(
                    *channel,
                    Stamped {
                        stamp,
                        value: *settings,
                    },
                );
                true
            }
            Action::ProcessingApplied {
                channel,
                sent,
                channels,
            } => {
                // An answer before the strips were ever read (or after they became unknown)
                // settles the edit but keeps them unknown: the read that follows brings them all.
                if let Some(processing) = self.processing.as_mut() {
                    for strip in processing.iter_mut() {
                        if let Some(changed) = channels
                            .iter()
                            .find(|changed| changed.channel == strip.channel)
                        {
                            *strip = changed.clone();
                        }
                    }
                }
                if let Some(channel) = channel {
                    settle_stamped(&mut self.processing_pending, channel, *sent);
                }
                self.processing_error = None;
                true
            }
            Action::ProcessingFailed {
                channel,
                sent,
                message,
            } => {
                settle_stamped(&mut self.processing_pending, channel, *sent);
                self.processing_error = Some(ProcessingError {
                    channel: Some(*channel),
                    message: message.clone(),
                });
                true
            }
            Action::StripPresets(list) => {
                self.strip_presets = list.clone();
                true
            }
            Action::EqCurve { channel, points } => {
                self.eq_curves.insert(*channel, points.clone());
                true
            }
            Action::LimiterEdit { address, settings } => {
                let stamp = self.next_stamp();
                self.limiter_pending.insert(
                    address.clone(),
                    Stamped {
                        stamp,
                        value: *settings,
                    },
                );
                true
            }
            Action::LimiterSettled {
                address,
                sent,
                kept,
            } => {
                if let Some(kept) = kept {
                    self.keep_limiter(address, *kept);
                }
                settle_stamped(&mut self.limiter_pending, address, *sent);
                true
            }
            Action::SendEdit {
                channel,
                effect,
                level,
            } => {
                *self
                    .sends_pending
                    .entry(*channel)
                    .or_default()
                    .slot(*effect) = Some(*level);
                true
            }
            Action::SendSettled {
                channel,
                effect,
                level,
            } => {
                settle_level(&mut self.sends_pending, channel, *effect, *level);
                true
            }
            Action::EffectEdit(change) => {
                let stamp = self.next_stamp();
                match *change {
                    EffectChange::Reverb(value) => {
                        self.effects_pending.reverb = Some(Stamped { stamp, value })
                    }
                    EffectChange::Delay(value) => {
                        self.effects_pending.delay = Some(Stamped { stamp, value })
                    }
                }
                true
            }
            Action::EffectSettled {
                effect,
                sent,
                effects,
            } => {
                // Only that effect's settings are kept: the other has a lane of its own, and an
                // answer about this one may be older than the other's latest change.
                if let (Some(effects), Some(status)) = (effects, self.engine.status.as_mut()) {
                    match effect {
                        Effect::Reverb => status.effects.reverb = effects.reverb,
                        Effect::Delay => status.effects.delay = effects.delay,
                    }
                }
                if *sent == self.effects_pending.stamp(*effect) {
                    match effect {
                        Effect::Reverb => self.effects_pending.reverb = None,
                        Effect::Delay => self.effects_pending.delay = None,
                    }
                }
                true
            }
            Action::ReturnEdit {
                address,
                effect,
                level,
            } => {
                *self
                    .returns_pending
                    .entry(address.clone())
                    .or_default()
                    .slot(*effect) = Some(*level);
                true
            }
            Action::ReturnSettled {
                address,
                effect,
                level,
                returns,
            } => {
                if let Some(returns) = returns {
                    self.keep_return(address, *effect, *returns);
                }
                settle_level(&mut self.returns_pending, address, *effect, *level);
                true
            }
            Action::Sessions(list) => {
                self.sessions = Listing {
                    list: Some(list.clone()),
                    error: None,
                };
                true
            }
            Action::SessionsFailed(message) => {
                self.sessions.error = Some(message.clone());
                true
            }
            Action::MixBackups(list) => {
                self.mix_backups = Listing {
                    list: Some(list.clone()),
                    error: None,
                };
                true
            }
            Action::MixBackupsFailed(message) => {
                self.mix_backups.error = Some(message.clone());
                true
            }
            Action::SessionNotice(notice) => {
                self.session_notice = notice.clone();
                true
            }
            Action::SessionLoaded(outcome) => {
                self.pending.clear();
                self.mix_error = None;
                self.controls_notice = None;
                self.processing_pending.clear();
                self.processing_error = None;
                self.limiter_pending.clear();
                self.sends_pending.clear();
                self.effects_pending = EffectsPending::default();
                self.returns_pending.clear();
                self.presets = Some(outcome.presets.clone());
                if outcome.audio_loaded {
                    self.audio = restored_audio(outcome.audio.as_ref());
                }
                true
            }
            Action::RecordingSettings(settings) => {
                self.recording_settings = settings.clone();
                true
            }
            Action::Takes(takes) => {
                self.takes = Takes {
                    list: Some(takes.takes.clone()),
                    folder: Some(takes.folder.clone()),
                    unreadable: takes.unreadable,
                    missing: takes.missing,
                    error: None,
                };
                true
            }
            Action::TakesFailed(message) => {
                self.takes.error = Some(message.clone());
                true
            }
            Action::Estimate(estimate) => {
                self.estimate = estimate.clone();
                true
            }
            Action::RecordingOutcome(outcome) => {
                self.recording_outcome = outcome.clone();
                true
            }
            Action::Soundcheck { soundcheck, run } => {
                if !self.is_shown_run(*run) {
                    return false;
                }
                match self.engine.status.as_mut() {
                    Some(status) => {
                        status.soundcheck = (**soundcheck).clone();
                        true
                    }
                    None => false,
                }
            }
        }
    }

    /// True when a payload of engine run `run` belongs to the engine shown, or to a newer one whose
    /// first status has not arrived yet; false for an ended or older run, or no run at all.
    pub fn is_live_run(&self, run: Option<u64>) -> bool {
        let Some(run) = run else {
            return false;
        };
        run > self.engine.ended_run && self.engine.run.is_none_or(|current| run >= current)
    }

    /// True when `run` is the run whose status is shown now, and live.
    pub fn is_shown_run(&self, run: Option<u64>) -> bool {
        run.is_some() && run == self.engine.run && self.is_live_run(run)
    }

    pub fn musician_by_address(&self, address: &str) -> Option<&Musician> {
        self.musicians
            .iter()
            .find(|musician| musician.address == address)
    }

    pub fn selected_musician(&self) -> Option<&Musician> {
        self.selected
            .as_deref()
            .and_then(|address| self.musician_by_address(address))
    }

    /// The edits to `address`'s mix still waiting for the engine.
    pub fn pending_edits(&self, address: &str) -> Edits {
        self.pending.get(address).cloned().unwrap_or_default()
    }

    /// The engine's mix `address` names: a musician's, or the local mix for [LOCAL_MIX].
    pub fn mix_of(&self, address: &str) -> Option<&Mix> {
        if address == LOCAL_MIX {
            return self.local_mix.as_ref();
        }
        self.musician_by_address(address)
            .map(|musician| &musician.mix)
    }

    /// The mix a screen shows for `address`: the engine's with the pending edits on top.
    pub fn view_mix(&self, address: &str) -> Option<Mix> {
        let mix = self.mix_of(address)?;
        Some(match self.pending.get(address) {
            Some(edits) => mix.with_edits(edits),
            None => mix.clone(),
        })
    }

    /// The recorder as the shown status reports it, or the idle one while no engine runs.
    pub fn recording_status(&self) -> Recording {
        self.engine
            .status
            .as_ref()
            .map(|status| status.recording.clone())
            .unwrap_or_default()
    }

    /// True while a take records or its files are being finished: the input cannot change.
    pub fn is_recording_now(&self) -> bool {
        self.engine
            .status
            .as_ref()
            .is_some_and(|status| status.recording.is_recording())
    }

    /// The soundcheck as the shown status reports it, or the live inputs while no engine runs.
    pub fn soundcheck_status(&self) -> Soundcheck {
        self.engine
            .status
            .as_ref()
            .map(|status| status.soundcheck.clone())
            .unwrap_or_default()
    }

    /// True while a take plays in place of the inputs: nothing records, the input cannot change.
    pub fn is_soundcheck_now(&self) -> bool {
        self.engine
            .status
            .as_ref()
            .is_some_and(|status| status.soundcheck.is_soundchecking())
    }

    /// The last take that ended while the shown engine ran.
    pub fn last_take(&self) -> Option<&TakeSummary> {
        self.engine
            .status
            .as_ref()
            .and_then(|status| status.last_take.as_ref())
    }

    /// The musicians whose link is a warning or bad, in the status's order.
    pub fn musicians_needing_attention(&self) -> Vec<&Musician> {
        self.musicians
            .iter()
            .filter(|musician| needs_attention(musician))
            .collect()
    }

    /// The strip a screen shows for `channel` (1-based).
    pub fn channel_strip(&self, channel: usize) -> ShownStrip {
        if let Some(waiting) = self.processing_pending.get(&channel) {
            return ShownStrip {
                channel,
                settings: waiting.value,
                active: !waiting.value.is_neutral(),
                pending: true,
            };
        }
        match self
            .processing
            .as_ref()
            .and_then(|strips| strips.iter().find(|strip| strip.channel == channel))
        {
            Some(kept) => ShownStrip {
                channel,
                settings: kept.settings,
                active: kept.active,
                pending: false,
            },
            None => ShownStrip {
                channel,
                settings: StripSettings::default(),
                active: false,
                pending: false,
            },
        }
    }

    /// Whether `channel`'s processing does something; `None` while the strips are not known, so
    /// a screen does not claim "Off" for a strip it has never read.
    pub fn processing_active(&self, channel: usize) -> Option<bool> {
        if self.processing.is_none() && !self.processing_pending.contains_key(&channel) {
            return None;
        }
        Some(self.channel_strip(channel).active)
    }
}

// The reducer's steps, each answering whether it changed anything.
impl ConsoleState {
    fn next_stamp(&mut self) -> u64 {
        self.stamps += 1;
        self.stamps
    }

    fn connect(&mut self, status: ConnectionStatus, error: Option<String>) -> bool {
        self.connection = Connection { status, error };
        true
    }

    /// Whether the console has the mix `address` names: a listed musician's, or the local mix.
    fn is_listed(&self, address: &str) -> bool {
        self.mix_of(address).is_some()
    }

    fn set_pending(&mut self, address: &str, edits: Edits) {
        if edits.is_empty() {
            self.pending.remove(address);
        } else {
            self.pending.insert(address.to_owned(), edits);
        }
    }

    /// Takes `mix` for `address` when it is newer than the copy the console has; never goes back.
    fn offer_mix(&mut self, address: &str, mix: &Mix) -> bool {
        let Some(known) = self.mix_of(address) else {
            return false;
        };
        if mix.revision <= known.revision {
            return false;
        }
        if address == LOCAL_MIX {
            self.local_mix = Some(mix.clone());
        } else if let Some(musician) = self
            .musicians
            .iter_mut()
            .find(|musician| musician.address == address)
        {
            musician.mix = mix.clone();
        }
        true
    }

    fn edit(&mut self, address: &str, edits: &Edits) -> bool {
        // The local mix has no level cap, limit or locks: edits of those are never kept for it.
        let kept: Edits = edits
            .iter()
            .filter(|(key, _)| {
                address != LOCAL_MIX
                    || !(key.field == MixField::MaxLevelPercent || key.field.is_engineer())
            })
            .map(|(key, value)| (*key, *value))
            .collect();
        if kept.is_empty() || !self.is_listed(address) {
            return false;
        }
        let mut pending = self.pending_edits(address);
        pending.extend(kept);
        self.set_pending(address, pending);
        true
    }

    /// Only a listed mix can have pending edits (a status drops the others), so an answer for any
    /// other has nothing to settle. The answer's mix is offered like any other copy; either way
    /// the edits it carried never leave the fader on an older value.
    fn answered(&mut self, address: &str, sent: &Edits, mix: Option<&Mix>) -> bool {
        if !self.is_listed(address) {
            return false;
        }
        if let Some(mix) = mix {
            self.offer_mix(address, mix);
        }
        let left = settle_edits(self.pending.get(address), sent);
        self.set_pending(address, left);
        if self
            .mix_error
            .as_ref()
            .is_some_and(|error| error.address == address)
        {
            self.mix_error = None;
        }
        true
    }

    fn set_phase(&mut self, phase: Phase, error: Option<String>) -> bool {
        if phase.ends_run() {
            // The engine of the current run is gone, or a start is replacing it: forget its
            // snapshot and never take its late payloads again.
            let ended_run = self.engine.ended_run.max(self.engine.run.unwrap_or(0));
            self.engine = Engine {
                phase,
                error,
                status: None,
                run: None,
                ended_run,
            };
            self.musicians.clear();
            self.local_mix = None;
            self.outputs = Listing::default();
            self.estimate = None;
            self.departed = None;
            self.pending.clear();
            self.mix_error = None;
            self.controls_notice = None;
            self.command_error = None;
            self.audio_notice = None;
            return true;
        }
        self.engine.phase = phase;
        self.engine.error = error;
        self.command_error = None;
        true
    }

    fn receive_status(&mut self, report: &StatusReport) -> bool {
        let status = &report.status;
        if !self.is_live_run(status.run) {
            return false;
        }
        // A status may have been read before a change the console already has: keep the newer mix.
        let musicians: Vec<Musician> = report
            .musicians
            .iter()
            .map(|row| {
                let mut musician = row.clone();
                if let Some(known) = self.musician_by_address(&musician.address)
                    && musician.mix.revision <= known.mix.revision
                {
                    musician.mix = known.mix.clone();
                }
                musician
            })
            .collect();
        // The newer of the local mix the status carries and the copy the console has; none from a
        // desk before the local mix.
        let local_mix = status
            .local_mix
            .as_ref()
            .map(|incoming| match &self.local_mix {
                Some(known) if incoming.revision <= known.revision => known.clone(),
                _ => incoming.clone(),
            });
        // Every mix the console can open: the musicians' addresses, and the local mix's id.
        let listed = |address: &str| {
            (address == LOCAL_MIX && local_mix.is_some())
                || musicians.iter().any(|musician| musician.address == address)
        };
        let mut phase = if status.stopped {
            Phase::Stopped
        } else {
            Phase::Running
        };
        if self.engine.phase == Phase::Stopping {
            phase = Phase::Stopping;
        }
        let stays = self.selected.as_deref().is_some_and(listed);
        // The open musician was on screen and is gone now: the first one opens, and the console
        // says so.
        let leaving = if stays {
            None
        } else {
            self.selected_musician()
        };
        let mut departed = match leaving {
            Some(musician) => Some(Departed {
                address: musician.address.clone(),
                label: musician_label(musician.name.as_deref(), &musician.address),
            }),
            None => self.departed.clone(),
        };
        if departed
            .as_ref()
            .is_some_and(|departed| listed(&departed.address))
        {
            departed = None;
        }
        let mut pending = self.pending.clone();
        pending.retain(|address, _| listed(address));
        let pending = match &self.engine.status {
            Some(before) => settle_layout(pending, before, status),
            None => pending,
        };
        let selected = if stays {
            self.selected.clone()
        } else {
            musicians.first().map(|musician| musician.address.clone())
        };
        if self
            .mix_error
            .as_ref()
            .is_some_and(|error| !listed(&error.address))
        {
            self.mix_error = None;
        }
        if self
            .controls_notice
            .as_ref()
            .is_some_and(|notice| !listed(&notice.address))
        {
            self.controls_notice = None;
        }
        self.engine.phase = phase;
        if phase == Phase::Running {
            self.engine.error = None;
        }
        self.engine.status = Some(status.clone());
        self.engine.run = status.run;
        self.musicians = musicians;
        self.local_mix = local_mix;
        self.selected = selected;
        self.departed = departed;
        self.pending = pending;
        // A status proves the console hears the engine.
        self.connection = Connection {
            status: ConnectionStatus::Ready,
            error: None,
        };
        true
    }

    /// The limiter `kept` as the mix `address`'s own (the local mix's in the status).
    fn keep_limiter(&mut self, address: &str, kept: LimiterSettings) {
        if address == LOCAL_MIX {
            if let Some(status) = self.engine.status.as_mut() {
                status.local_limiter = kept;
            }
        } else if let Some(musician) = self
            .musicians
            .iter_mut()
            .find(|musician| musician.address == address)
        {
            musician.limiter = kept;
        }
    }

    /// The return of `effect` in the mix `address` as the desk answered it, kept until the next
    /// status; only that effect's, for the same reason as the effects' settings.
    fn keep_return(&mut self, address: &str, effect: Effect, returns: EffectLevels) {
        let set = |levels: &mut EffectLevels| match effect {
            Effect::Reverb => levels.reverb = returns.reverb,
            Effect::Delay => levels.delay = returns.delay,
        };
        if address == LOCAL_MIX {
            if let Some(status) = self.engine.status.as_mut() {
                set(&mut status.local_fx_returns);
            }
        } else if let Some(musician) = self
            .musicians
            .iter_mut()
            .find(|musician| musician.address == address)
        {
            set(&mut musician.fx_returns);
        }
    }
}

/// Drops `key`'s value from `pending` when it is the very one stamped `sent` (a later one stays: it
/// goes out next).
fn settle_stamped<K: Ord + ?Sized, T>(
    pending: &mut BTreeMap<K::Owned, Stamped<T>>,
    key: &K,
    sent: Option<u64>,
) where
    K: ToOwned,
    K::Owned: Ord + std::borrow::Borrow<K>,
{
    if sent.is_some() && pending.get(key).map(|waiting| waiting.stamp) == sent {
        pending.remove(key);
    }
}

/// Drops `key`'s `effect` from `pending` once it is `level` (a later level stays).
fn settle_level<K: Ord + ?Sized>(
    pending: &mut BTreeMap<K::Owned, Levels>,
    key: &K,
    effect: Effect,
    level: u8,
) where
    K: ToOwned,
    K::Owned: Ord + std::borrow::Borrow<K>,
{
    let Some(waiting) = pending.get_mut(key) else {
        return;
    };
    if waiting.get(effect) != Some(level) {
        return;
    }
    *waiting.slot(effect) = None;
    if waiting.is_empty() {
        pending.remove(key);
    }
}

/// What each input of a status is on the Mix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputRole {
    Hidden,
    Single,
    First,
    Second,
}

fn input_roles(status: &Status) -> HashMap<usize, InputRole> {
    let channels = &status.channels[..status.channels.len().min(status.source_channels)];
    let mut roles = HashMap::new();
    for unit in pair_units(channels) {
        for (side, index) in unit.iter().enumerate() {
            let role = if channels[*index].hidden {
                InputRole::Hidden
            } else if unit.len() == 1 {
                InputRole::Single
            } else if side == 0 {
                InputRole::First
            } else {
                InputRole::Second
            };
            roles.insert(*index, role);
        }
    }
    roles
}

fn same_groups(before: &Status, after: &Status) -> bool {
    before
        .groups
        .iter()
        .map(|group| &group.id)
        .eq(after.groups.iter().map(|group| &group.id))
}

/// The pending edits that still mean what they meant once the layout went from status `before`
/// to `after`. Edits are keyed by position: a group level by the group's place, which moves when a
/// group is added, deleted or reordered, and a channel's values by input, whose strip changes when
/// it joins or leaves a pair or is hidden or shown. Such edits are dropped, so the next send never
/// writes a level meant for one group onto another, nor a mono strip's pan onto a new pair. A rename
/// moves nothing and keeps every edit.
fn settle_layout(
    pending: BTreeMap<String, Edits>,
    before: &Status,
    after: &Status,
) -> BTreeMap<String, Edits> {
    if pending.is_empty() {
        return pending;
    }
    let groups_moved = !same_groups(before, after);
    let (roles_before, roles_after) = (input_roles(before), input_roles(after));
    let stale = |key: &EditKey| match key.field {
        MixField::GroupLevels => groups_moved,
        MixField::ChannelGains
        | MixField::Pans
        | MixField::ChannelMuted
        | MixField::ChannelSolo
        | MixField::LockedChannels => {
            let index = key.index.unwrap_or(0);
            roles_before.get(&index) != roles_after.get(&index)
        }
        _ => false,
    };
    pending
        .into_iter()
        .filter_map(|(address, edits)| {
            let kept: Edits = edits.into_iter().filter(|(key, _)| !stale(key)).collect();
            (!kept.is_empty()).then_some((address, kept))
        })
        .collect()
}

#[cfg(test)]
mod tests;
